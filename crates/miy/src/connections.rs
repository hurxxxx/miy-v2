//! Same-environment, user-authorized data connections. Never opens another app's DB.
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
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

async fn source(
    s: &State,
    p: &Principal,
    id: Uuid,
    permission: &str,
) -> ApiResult<sqlx::postgres::PgRow> {
    let inst = domain::installation(s, p, id, false).await?;
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
) -> ApiResult<Json<Value>> {
    source(&s, &p, id, "platform:directory:read").await?;
    if let Some(c) = req.extensions().get::<crate::app_calls::Context>() {
        crate::app_calls::require(&s, &p, c, id, "platform:directory:read").await?;
    }
    directory_data(&s).await
}
pub async fn directory_data(s: &State) -> ApiResult<Json<Value>> {
    let users: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id',id,'name',name) FROM users WHERE enabled ORDER BY name",
    )
    .fetch_all(&s.pool)
    .await?;
    Ok(Json(json!({"users":users})))
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
    domain::manage_action(&s, &p, &app, "dev", crate::delegations::Action::DevSession).await?;
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
pub async fn preview_directory(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path(app): Path<String>,
) -> ApiResult<Json<Value>> {
    domain::manage_action(&s, &p, &app, "dev", crate::delegations::Action::DevSession).await?;
    source(
        &s,
        &p,
        dev_installation(&s, &app).await?,
        "platform:directory:read",
    )
    .await?;
    directory_data(&s).await
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
    domain::authorize(s, p, target, env, false).await?;
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
pub async fn recheck_preview(s: &State, p: &Principal, call: &PreviewCall) -> ApiResult<()> {
    domain::manage_action(
        s,
        p,
        &call.app_id,
        "dev",
        crate::delegations::Action::DevSession,
    )
    .await?;
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
