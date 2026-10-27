//! Read-only planning and revision checks shared by submission and execution.
use crate::{
    State,
    api_error::{ApiError, ApiResult},
    auth::Principal,
    domain,
    runtime_contract::{self, ExpectedState, PreflightInput, PreflightResult},
};
use serde_json::json;
use sqlx::Row;

pub async fn snapshot(
    pool: &sqlx::PgPool,
    app: &str,
    environment: &str,
) -> ApiResult<ExpectedState> {
    let row = sqlx::query("SELECT * FROM installations WHERE app_id=$1 AND environment=$2")
        .bind(app)
        .bind(environment)
        .fetch_optional(pool)
        .await?;
    let installation = row.map(|row| {
        json!({
            "installationId":row.get::<uuid::Uuid,_>("id"),
            "releaseId":row.get::<uuid::Uuid,_>("release_id"),
            "generation":row.get::<i64,_>("control_generation"),
            "desiredState":row.get::<String,_>("desired_state"),
            "runtimeStatus":row.get::<String,_>("runtime_status"),
            "dbStatus":row.get::<String,_>("db_status"),
            "imageId":row.get::<Option<String>,_>("runtime_image")
        })
    });
    let resource=sqlx::query("SELECT id,status,schema_generation,schema_fingerprint FROM db_resources WHERE app_id=$1 AND environment=$2").bind(app).bind(environment).fetch_optional(pool).await?;
    let database=resource.map(|row|json!({"databaseId":row.get::<uuid::Uuid,_>("id"),"status":row.get::<String,_>("status"),"schemaGeneration":row.get::<i64,_>("schema_generation"),"schemaFingerprint":row.get::<Option<String>,_>("schema_fingerprint")}));
    serde_json::from_value(
        json!({"target":{"appId":app,"environment":environment},"installation":installation,"database":database}),
    )
    .map_err(|_| ApiError::unavailable("unsupported_installation_state"))
}

pub async fn inspect(
    state: &State,
    p: &Principal,
    app: &str,
    input: PreflightInput,
) -> ApiResult<PreflightResult> {
    if input.target.app_id != app {
        return Err(ApiError::bad("target_mismatch"));
    }
    runtime_contract::validate_input(&input)
        .map_err(|_| ApiError::bad("invalid_runtime_request"))?;
    let environment = input.target.environment.as_str();
    domain::manage(state, p, app, environment).await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM apps WHERE id=$1)")
        .bind(app)
        .fetch_one(&state.pool)
        .await?;
    if !exists {
        return Err(ApiError::not_found());
    }
    let current = snapshot(&state.pool, app, environment).await?;
    let busy:bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runtime_jobs WHERE app_id=$1 AND environment=$2 AND status IN ('queued','running'))")
        .bind(app).bind(environment).fetch_one(&state.pool).await?;
    use runtime_contract::{CheckName, PreflightCheck};
    let mut checks = vec![
        PreflightCheck::passed(CheckName::Target),
        PreflightCheck::passed(CheckName::Authorization),
        if environment == "dev" && crate::development::active_for_app(&state.pool, app).await? {
            PreflightCheck::failed(
                CheckName::PendingOperation,
                runtime_contract::FailureCode::ActiveDevelopmentSession,
            )
        } else if busy {
            PreflightCheck::failed(
                CheckName::PendingOperation,
                runtime_contract::FailureCode::OperationInProgress,
            )
        } else {
            PreflightCheck::passed(CheckName::PendingOperation)
        },
    ];
    if input.action == runtime_contract::Operation::Promote {
        let accepted = crate::runtime::checked_image(&state.pool, app, input.release_id.unwrap())
            .await
            .is_ok();
        checks.push(if accepted {
            PreflightCheck::passed(CheckName::ReleaseAcceptance)
        } else {
            PreflightCheck::failed(
                CheckName::ReleaseAcceptance,
                runtime_contract::FailureCode::ReleaseNotAccepted,
            )
        });
    }
    runtime_contract::preflight(&input, &current, checks)
        .map_err(|_| ApiError::bad("invalid_runtime_request"))
}
