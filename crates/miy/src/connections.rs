//! Same-environment, user-authorized data connections. Never opens another app's DB.
use crate::{
    State,
    auth::Principal,
    contract::Manifest,
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
    let value: Value = sqlx::query_scalar("SELECT manifest FROM releases WHERE id=$1")
        .bind(inst.get::<Uuid, _>("release_id"))
        .fetch_one(&s.pool)
        .await?;
    let manifest: Manifest = serde_json::from_value(value)?;
    if !manifest
        .requested_permissions
        .iter()
        .any(|v| v == permission)
    {
        return Err(ApiError::forbidden());
    }
    Ok(inst)
}
pub async fn directory(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    source(&s, &p, id, "platform:directory:read").await?;
    directory_data(&s).await
}
pub async fn directory_data(s: &State) -> ApiResult<Json<Value>> {
    let users: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id',id,'name',name) FROM users ORDER BY name",
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
    let inst = source(&s, &p, id, &format!("app:{target}:{access}")).await?;
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
    forward(&s, &p, &target, inst.get("environment"), &path, &mut req).await
}
// Preview requests are developer-only and always target dev. The local app's manifest
// is checked by the preview gateway; target app authorization is always enforced here.
pub async fn preview_connect(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path((app, target, path)): Path<(String, String, String)>,
    mut req: Request,
) -> ApiResult<Response> {
    domain::manage(&s, &p, &app, "dev").await?;
    forward(&s, &p, &target, "dev", &path, &mut req).await
}
pub async fn preview_directory(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path(app): Path<String>,
) -> ApiResult<Json<Value>> {
    domain::manage(&s, &p, &app, "dev").await?;
    directory_data(&s).await
}
async fn forward(
    s: &State,
    p: &Principal,
    target: &str,
    env: &str,
    path: &str,
    req: &mut Request,
) -> ApiResult<Response> {
    domain::authorize(s, p, target, env, false).await?;
    let row =
        sqlx::query("SELECT id,release_id FROM installations WHERE app_id=$1 AND environment=$2")
            .bind(target)
            .bind(env)
            .fetch_optional(&s.pool)
            .await?
            .ok_or_else(ApiError::not_found)?;
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
