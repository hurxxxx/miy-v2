//! Read-only planning and revision checks shared by submission and execution.
use crate::policy::{self, AppAction};
use crate::{
    State,
    api_error::{ApiError, ApiResult},
    auth::Principal,
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
    let resource = sqlx::query(
        "SELECT d.id,d.status,d.schema_generation,d.schema_fingerprint,
                CASE WHEN o.app_id IS NOT NULL THEN COALESCE(c.disabled,false) END AS official_app_disabled
         FROM (SELECT $1::text AS id) a
         LEFT JOIN db_resources d ON d.app_id=a.id AND d.environment=$2
         LEFT JOIN official_apps o ON o.app_id=a.id LEFT JOIN app_controls c ON c.app_id=o.app_id",
    )
    .bind(app)
    .bind(environment)
    .fetch_one(pool)
    .await?;
    let database = resource.get::<Option<uuid::Uuid>, _>("id").map(|id| {
        json!({
            "databaseId": id, "status": resource.get::<String, _>("status"),
            "schemaGeneration": resource.get::<i64, _>("schema_generation"),
            "schemaFingerprint": resource.get::<Option<String>, _>("schema_fingerprint")
        })
    });
    serde_json::from_value(
        json!({"target":{"appId":app,"environment":environment},"installation":installation,"database":database,
               "officialAppDisabled":resource.get::<Option<bool>, _>("official_app_disabled")}),
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
    policy::require(
        state,
        p,
        app,
        environment,
        AppAction::Request(input.action.as_str().parse()?),
    )
    .await?;
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
    let image = if input.action == runtime_contract::Operation::Promote {
        let accepted = crate::runtime::checked_image(&state.pool, app, input.release_id.unwrap())
            .await
            .ok();
        checks.push(if accepted.is_some() {
            PreflightCheck::passed(CheckName::ReleaseAcceptance)
        } else {
            PreflightCheck::failed(
                CheckName::ReleaseAcceptance,
                runtime_contract::FailureCode::ReleaseNotAccepted,
            )
        });
        accepted
    } else if let Some(release) = input.release_id {
        let published = crate::official_catalog::release_image(&state.pool, app, release).await?;
        checks.push(if published.is_some() {
            PreflightCheck::passed(CheckName::OfficialRelease)
        } else {
            PreflightCheck::failed(
                CheckName::OfficialRelease,
                runtime_contract::FailureCode::OfficialReleaseRequired,
            )
        });
        published
    } else {
        input.image_id.clone()
    };
    // Submission refuses the same image; the worker still verifies the image itself.
    if let Some(image) = &image
        && !crate::runtime::server_built(&state.pool, app, image).await?
    {
        checks.push(PreflightCheck::failed(
            CheckName::Image,
            runtime_contract::FailureCode::ServerBuildRequired,
        ));
    }
    let mut plan = runtime_contract::preflight(&input, &current, checks)
        .map_err(|_| ApiError::bad("invalid_runtime_request"))?;
    if input.action == runtime_contract::Operation::Promote && plan.can_submit {
        let row = sqlx::query(
            r#"SELECT v.version,v.source_commit,v.archive_sha256,c.checks @> '[{"result":"passed"}]'::jsonb AS structured
             FROM releases r JOIN official_app_versions v ON (v.app_id,v.version)=(r.app_id,r.official_version)
             JOIN release_checks c ON c.release_id=r.id WHERE r.id=$1 AND r.app_id=$2"#,
        )
        .bind(input.release_id)
        .bind(app)
        .fetch_optional(&state.pool)
        .await?;
        plan.official_release = row.map(|r| runtime_contract::OfficialPromotion {
            version: r.get("version"),
            source_commit: r.get("source_commit"),
            archive_sha256: r.get("archive_sha256"),
            structured_acceptance: r.get("structured"),
        });
    }
    Ok(plan)
}
