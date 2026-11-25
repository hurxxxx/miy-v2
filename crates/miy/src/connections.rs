//! Same-environment, user-authorized data connections. Never opens another app's DB.
use crate::policy::{self, AppAction};
use crate::{
    State,
    auth::Principal,
    domain,
    http::{ApiError, ApiResult},
};
use axum::{
    Json,
    extract::{Path, Request, State as S},
    response::Response,
};
use sqlx::Row;
use uuid::Uuid;

async fn source(
    s: &State,
    p: &Principal,
    id: Uuid,
    permission: &str,
) -> ApiResult<sqlx::postgres::PgRow> {
    let inst = domain::installation(s, p, id).await?;
    crate::permissions::require(&s.pool, id, inst.get("release_id"), permission).await?;
    let action = if permission == "platform:directory:read" {
        crate::delegations::Action::DirectoryRead
    } else if permission.ends_with(":write") {
        crate::delegations::Action::AppWrite
    } else {
        crate::delegations::Action::AppRead
    };
    crate::delegations::check(s, p, inst.get("app_id"), inst.get("environment"), action).await?;
    Ok(inst)
}
pub async fn directory(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path(id): Path<Uuid>,
    req: Request,
) -> ApiResult<Json<crate::api_contract::DirectoryPage>> {
    let inst = source(&s, &p, id, "platform:directory:read").await?;
    if let Some(c) = req.extensions().get::<crate::app_calls::Context>() {
        crate::app_calls::require(&s, &p, c, id, "platform:directory:read").await?;
    }
    let query = crate::directory::DirectoryQuery::from_uri(req.uri())?;
    let environment = inst
        .get::<&str, _>("environment")
        .parse()
        .map_err(|_| ApiError::unavailable("invalid_installation_environment"))?;
    let page =
        crate::directory::read(&s, &p.id, id, inst.get("release_id"), environment, query).await?;
    let current = source(&s, &p, id, "platform:directory:read").await?;
    if current.get::<Uuid, _>("release_id") != inst.get::<Uuid, _>("release_id") {
        return Err(ApiError::conflict("release_changed_reload_required"));
    }
    if let Some(c) = req.extensions().get::<crate::app_calls::Context>() {
        crate::app_calls::require(&s, &p, c, id, "platform:directory:read").await?;
    }
    Ok(Json(page))
}
pub async fn connect(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path((id, target, path)): Path<(Uuid, String, String)>,
    mut req: Request,
) -> ApiResult<Response> {
    let access = if matches!(
        *req.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    ) {
        "read"
    } else {
        "write"
    };
    let permission = format!("app:{target}:{access}");
    let inst = source(&s, &p, id, &permission).await?;
    if let Some(c) = req.extensions().get::<crate::app_calls::Context>() {
        crate::app_calls::require(&s, &p, c, id, &permission).await?;
    }
    let current = inst.get::<Uuid, _>("release_id");
    if access == "write"
        && req
            .headers()
            .get("x-miy-release")
            .and_then(|h| h.to_str().ok())
            != Some(current.to_string().as_str())
    {
        return Err(ApiError::conflict("release_changed_reload_required"));
    }
    forward(&s, &p, &target, &inst, &permission, &path, &mut req).await
}
// Preview remains developer-only, additionally using explicit dev installation
// approvals. A local manifest cannot elevate the installed app's permissions.
pub async fn preview_connect(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path((app, target, path)): Path<(String, String, String)>,
    mut req: Request,
) -> ApiResult<Response> {
    require_preview(&s, &p, &app).await?;
    let id = dev_installation(&s, &app).await?;
    let access = if matches!(
        *req.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    ) {
        "read"
    } else {
        "write"
    };
    let permission = format!("app:{target}:{access}");
    let inst = source(&s, &p, id, &permission).await?;
    // Local-preview code is not the installed release. Preserve its developer
    // context but do not issue a backend continuation proof for that assertion.
    req.extensions_mut().insert(PreviewCall {
        app_id: app,
        installation_id: id,
        release_id: inst.get("release_id"),
        permission: permission.clone(),
    });
    forward(&s, &p, &target, &inst, &permission, &path, &mut req).await
}
pub async fn preview_context(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path(app): Path<String>,
) -> ApiResult<Json<crate::api_contract::PreviewContext>> {
    require_preview(&s, &p, &app).await?;
    let installation_id = sqlx::query_scalar(
        "SELECT COALESCE(i.id,a.initial_dev_installation_id) FROM apps a
         LEFT JOIN installations i ON i.app_id=a.id AND i.environment='dev' WHERE a.id=$1",
    )
    .bind(&app)
    .fetch_optional(&s.pool)
    .await?
    .ok_or_else(ApiError::not_found)?;
    Ok(Json(crate::api_contract::PreviewContext {
        app_id: app,
        subject_id: p.id,
        environment: crate::runtime_contract::Environment::Dev,
        installation_id,
    }))
}

pub async fn preview_directory(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path(app): Path<String>,
    req: Request,
) -> ApiResult<Json<crate::api_contract::DirectoryPage>> {
    require_preview(&s, &p, &app).await?;
    let id = dev_installation(&s, &app).await?;
    let inst = source(&s, &p, id, "platform:directory:read").await?;
    let query = crate::directory::DirectoryQuery::from_uri(req.uri())?;
    let page = crate::directory::read(
        &s,
        &p.id,
        id,
        inst.get("release_id"),
        crate::runtime_contract::Environment::Dev,
        query,
    )
    .await?;
    require_preview(&s, &p, &app).await?;
    let current = source(&s, &p, id, "platform:directory:read").await?;
    if current.get::<Uuid, _>("release_id") != inst.get::<Uuid, _>("release_id") {
        return Err(ApiError::conflict("release_changed_reload_required"));
    }
    Ok(Json(page))
}
async fn forward(
    s: &State,
    p: &Principal,
    target: &str,
    source: &sqlx::postgres::PgRow,
    permission: &str,
    path: &str,
    req: &mut Request,
) -> ApiResult<Response> {
    let env: &str = source.get("environment");
    policy::require(s, p, target, env, AppAction::Use).await?;
    let row = sqlx::query("SELECT * FROM installations WHERE app_id=$1 AND environment=$2")
        .bind(target)
        .bind(env)
        .fetch_optional(&s.pool)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if req.extensions().get::<PreviewCall>().is_none() {
        let context = crate::app_calls::forward(
            s,
            p,
            source,
            &row,
            req.extensions().get::<crate::app_calls::Context>(),
            req.extensions().get::<crate::app_calls::Origin>(),
            permission,
        )
        .await?;
        req.extensions_mut().insert(context);
    }
    // The browser signs its source release. The gateway resolves the target's current release.
    req.headers_mut().insert(
        "x-miy-release",
        row.get::<Uuid, _>("release_id")
            .to_string()
            .parse()
            .unwrap(),
    );
    let request = std::mem::replace(req, Request::new(axum::body::Body::empty()));
    crate::runtime::proxy(
        S(s.clone()),
        axum::Extension(p.clone()),
        Path((row.get("id"), path.into())),
        request,
    )
    .await
}

#[derive(Clone)]
pub struct PreviewCall {
    app_id: String,
    installation_id: Uuid,
    release_id: Uuid,
    permission: String,
}
/// The local preview carries a CLI or scoped delegation credential. Recheck the
/// actual credential after body reads, not just the user's still-existing grants.
pub(crate) async fn require_preview(s: &State, p: &Principal, app: &str) -> ApiResult<()> {
    if p.browser_purpose.is_some() {
        return Err(ApiError::forbidden());
    }
    if let Some(session) = p.cli_session_id {
        let live: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cli_sessions WHERE id=$1 AND user_id=$2 AND revoked_at IS NULL AND expires_at>clock_timestamp())")
            .bind(session).bind(&p.id).fetch_one(&s.pool).await?;
        if !live {
            return Err(ApiError::unauthorized());
        }
    } else if p.delegation_id.is_none() {
        return Err(ApiError::forbidden());
    }
    policy::require(
        s,
        p,
        app,
        "dev",
        AppAction::Manage(crate::delegations::Action::DevSession),
    )
    .await
}

pub async fn recheck_preview(s: &State, p: &Principal, call: &PreviewCall) -> ApiResult<()> {
    require_preview(s, p, &call.app_id).await?;
    let inst = source(s, p, call.installation_id, &call.permission).await?;
    if inst.get::<Uuid, _>("release_id") != call.release_id
        || inst.get::<String, _>("environment") != "dev"
        || inst.get::<String, _>("app_id") != call.app_id
    {
        return Err(ApiError::forbidden());
    }
    Ok(())
}
async fn dev_installation(s: &State, app: &str) -> ApiResult<Uuid> {
    sqlx::query_scalar("SELECT id FROM installations WHERE app_id=$1 AND environment='dev'")
        .bind(app)
        .fetch_optional(&s.pool)
        .await?
        .ok_or_else(ApiError::forbidden)
}
