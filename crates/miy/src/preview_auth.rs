//! The CLI starts a preview. Its browser authenticates separately on the dev portal.
use crate::{
    State,
    auth::{BrowserPurpose, Principal, random},
    contract::hash,
    http::{ApiError, ApiResult},
    policy::{self, AppAction},
};
use axum::{
    Extension, Json,
    extract::{Path, State as S},
    http::{HeaderMap, Method},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Row, postgres::PgRow};
use uuid::Uuid;

pub const PREFIX: &str = "miy_preview_";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Start {
    instance: Uuid,
    port: std::num::NonZeroU16,
    state: String,
    challenge: String,
}

pub async fn start(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(app): Path<String>,
    Json(v): Json<Start>,
) -> ApiResult<Json<Value>> {
    crate::connections::require_host(&s, &p, &app).await?;
    if !crate::preview::valid_nonce(&v.state) || !crate::preview::valid_nonce(&v.challenge) {
        return Err(ApiError::bad("invalid_preview_challenge"));
    }
    sqlx::query("DELETE FROM preview_browser_logins WHERE expires_at<=now()")
        .execute(&s.pool)
        .await?;
    let id: Uuid = sqlx::query_scalar("INSERT INTO preview_browser_logins(app_id,instance_id,host_user,cli_session,delegation,port,state,challenge) VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id")
        .bind(app).bind(v.instance).bind(p.id).bind(p.cli_session_id).bind(p.delegation_id)
        .bind(i32::from(v.port.get())).bind(v.state).bind(v.challenge).fetch_one(&s.pool).await?;
    Ok(Json(
        json!({"authorizationUrl":format!("{}/preview/login/{id}",s.auth.dev_portal_origin)}),
    ))
}

fn host(row: &PgRow) -> Principal {
    Principal {
        id: row.get("host_user"),
        cli_session_id: row.get("cli_session"),
        delegation_id: row.get("delegation"),
        browser_purpose: None,
        preview_login: None,
        scopes: vec!["miy.read".into(), "miy.dev".into()],
        development: false,
        csrf: None,
    }
}

async fn check_access(s: &State, row: &PgRow, viewer: &Principal) -> ApiResult<()> {
    let app: &str = row.get("app_id");
    crate::connections::require_host(s, &host(row), app).await?;
    policy::require(s, viewer, app, "dev", AppAction::Use).await?;
    // Before deployment only developers (including platform admins) may open this scope.
    let installed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM installations WHERE app_id=$1 AND environment='dev')",
    )
    .bind(app)
    .fetch_one(&s.pool)
    .await?;
    if !installed {
        let mut developer = viewer.clone();
        developer.browser_purpose = None;
        policy::require(s, &developer, app, "dev", AppAction::Develop).await?;
    }
    Ok(())
}

pub async fn authorize(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> ApiResult<Json<Value>> {
    if p.browser_purpose != Some(BrowserPurpose::DevPortal) || p.csrf.is_none() {
        return Err(ApiError::forbidden());
    }
    let row = sqlx::query("SELECT * FROM preview_browser_logins WHERE id=$1 AND expires_at>now() AND session_hash IS NULL")
        .bind(id).fetch_optional(&s.pool).await?.ok_or_else(ApiError::unauthorized)?;
    check_access(&s, &row, &p).await?;
    let digest = s
        .auth
        .session_hash(&headers, BrowserPurpose::DevPortal)
        .ok_or_else(ApiError::unauthorized)?;
    let code = random();
    let changed=sqlx::query("UPDATE preview_browser_logins SET session_hash=$2,development=$3,code_hash=$4,expires_at=now()+interval '60 seconds' WHERE id=$1 AND session_hash IS NULL AND expires_at>now()")
        .bind(id).bind(digest).bind(p.development).bind(hash(&code)).execute(&s.pool).await?;
    if changed.rows_affected() != 1 {
        return Err(ApiError::unauthorized());
    }
    Ok(Json(
        json!({"port":row.get::<i32,_>("port"),"state":row.get::<String,_>("state"),"code":code}),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exchange {
    instance: Uuid,
    code: String,
    verifier: String,
}
pub async fn exchange(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(app): Path<String>,
    Json(v): Json<Exchange>,
) -> ApiResult<Json<Value>> {
    crate::connections::require_host(&s, &p, &app).await?;
    if !crate::preview::valid_nonce(&v.code) || !crate::preview::valid_nonce(&v.verifier) {
        return Err(ApiError::unauthorized());
    }
    let grant = format!("{PREFIX}{}", random());
    let changed=sqlx::query("UPDATE preview_browser_logins SET code_hash=NULL,grant_hash=$1,expires_at=now()+interval '8 hours' WHERE app_id=$2 AND instance_id=$3 AND host_user=$4 AND cli_session IS NOT DISTINCT FROM $5 AND delegation IS NOT DISTINCT FROM $6 AND code_hash=$7 AND challenge=$8 AND expires_at>clock_timestamp()")
        .bind(hash(&grant)).bind(app).bind(v.instance).bind(p.id).bind(p.cli_session_id).bind(p.delegation_id)
        .bind(hash(&v.code)).bind(crate::preview::challenge(&v.verifier)).execute(&s.pool).await?;
    if changed.rows_affected() != 1 {
        return Err(ApiError::unauthorized());
    }
    Ok(Json(json!({"grant":grant})))
}

/// This bearer is accepted exclusively on matched preview data routes. Each handler
/// checks its path app against the grant (including after streaming/body waits).
pub fn data_route(route: &str, method: &Method) -> bool {
    match route.strip_prefix("/api/v1").unwrap_or(route) {
        "/preview/apps/{app}/browser/viewer" | "/preview/apps/{app}/platform/directory" => {
            method == Method::GET
        }
        "/preview/apps/{app}/connections/{target}/api/{*path}"
        | "/preview/apps/{app}/platform/files"
        | "/preview/apps/{app}/platform/files/{*path}" => true,
        _ => false,
    }
}

async fn subject(s: &State, row: &PgRow) -> ApiResult<Principal> {
    let mut p = s
        .auth
        .session_by_hash(
            s,
            BrowserPurpose::DevPortal,
            row.get("session_hash"),
            row.get("development"),
        )
        .await?;
    check_access(s, row, &p).await?;
    p.csrf = None;
    p.preview_login = Some(row.get("id"));
    Ok(p)
}

pub async fn authenticate(
    s: &State,
    token: &str,
    instance: Uuid,
    app: &str,
) -> ApiResult<Principal> {
    let row=sqlx::query("SELECT * FROM preview_browser_logins WHERE grant_hash=$1 AND instance_id=$2 AND app_id=$3 AND expires_at>now()")
        .bind(hash(token)).bind(instance).bind(app).fetch_optional(&s.pool).await?.ok_or_else(ApiError::unauthorized)?;
    subject(s, &row).await
}

pub async fn recheck(s: &State, p: &Principal, app: &str) -> ApiResult<()> {
    let id = p.preview_login.ok_or_else(ApiError::forbidden)?;
    let row=sqlx::query("SELECT * FROM preview_browser_logins WHERE id=$1 AND app_id=$2 AND grant_hash IS NOT NULL AND expires_at>now()")
        .bind(id).bind(app).fetch_optional(&s.pool).await?.ok_or_else(ApiError::unauthorized)?;
    if subject(s, &row).await?.id != p.id {
        return Err(ApiError::forbidden());
    }
    Ok(())
}

pub async fn viewer(S(s): S<State>, Extension(p): Extension<Principal>) -> ApiResult<Json<Value>> {
    if p.preview_login.is_none() {
        return Err(ApiError::forbidden());
    }
    let name: String = sqlx::query_scalar("SELECT name FROM users WHERE id=$1 AND enabled")
        .bind(&p.id)
        .fetch_optional(&s.pool)
        .await?
        .ok_or_else(ApiError::forbidden)?;
    // App frames never receive platform administrator authority.
    Ok(Json(json!({"id":p.id,"name":name,"admin":false})))
}
