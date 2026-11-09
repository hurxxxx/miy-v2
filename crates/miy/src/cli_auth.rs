//! Legacy (pre-0.5) CLI credentials: the approval flow is retired; issued tokens
//! remain valid until expiry and can be listed/revoked. Never shares browser cookies.
use crate::{
    State,
    auth::Principal,
    contract::hash,
    http::{ApiError, ApiResult},
};
use axum::{
    Json,
    extract::{Path, State as S},
    http::HeaderMap,
};
use serde_json::{Value, json};
use uuid::Uuid;

/// The pre-0.5 browser-approval login (`start` -> click approve -> `exchange`)
/// is retired: a single click on a console page could hand a CLI credential to
/// whoever started the request. Supported CLIs use the device flow, which needs
/// the code shown in the user's own terminal. Already issued legacy credentials
/// keep working until they expire and stay listable/revocable below.
const LEGACY_LOGIN_RETIRED: &str = "cli_upgrade_required";

pub async fn start() -> ApiResult<Json<Value>> {
    Err(ApiError::gone(LEGACY_LOGIN_RETIRED))
}
pub async fn approve() -> ApiResult<Json<Value>> {
    Err(ApiError::gone(LEGACY_LOGIN_RETIRED))
}

/// Older CLI credentials remain independent of a browser logout. List only
/// non-secret identifiers and current lifetime, without changing published
/// device-session response shapes.
pub async fn list_legacy(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
) -> ApiResult<Json<Value>> {
    crate::delegations::unrestricted(&p)?;
    crate::auth_sessions::ensure_active(&s.pool, &p.id).await?;
    let sessions: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id',id,'createdAt',created_at,'expiresAt',expires_at) FROM cli_tokens WHERE user_id=$1 AND expires_at>now() ORDER BY created_at DESC,id"
    ).bind(&p.id).fetch_all(&s.pool).await?;
    Ok(Json(json!({"sessions":sessions})))
}

pub async fn revoke_legacy(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    crate::delegations::unrestricted(&p)?;
    let mut tx = s.pool.begin().await?;
    crate::auth_sessions::active_locked(&mut tx, &p.id).await?;
    let count = sqlx::query("DELETE FROM cli_tokens WHERE id=$1 AND user_id=$2")
        .bind(id)
        .bind(&p.id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if count == 0 {
        return Err(ApiError::not_found());
    }
    sqlx::query("INSERT INTO audit(actor,action,target,request_id) VALUES($1,'auth.legacy_session.revoked',$2,$3)")
        .bind(&p.id).bind(id.to_string()).bind(Uuid::new_v4().to_string())
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id,"revoked":true})))
}
pub async fn exchange() -> ApiResult<Json<Value>> {
    Err(ApiError::gone(LEGACY_LOGIN_RETIRED))
}
pub async fn principal(s: &State, token: &str) -> ApiResult<Principal> {
    if token.starts_with("miy_access_") {
        return crate::auth_sessions::principal(s, token).await;
    }
    let user: String = sqlx::query_scalar(
        "SELECT user_id FROM cli_tokens WHERE token_hash=$1 AND expires_at>now()",
    )
    .bind(hash(token))
    .fetch_optional(&s.pool)
    .await?
    .ok_or_else(ApiError::unauthorized)?;
    crate::auth_sessions::ensure_active(&s.pool, &user).await?;
    Ok(Principal {
        browser_purpose: None,
        cli_session_id: None,
        delegation_id: None,
        id: user,
        scopes: vec!["miy.read".into(), "miy.dev".into()],
        csrf: None,
        development: false,
    })
}
pub async fn revoke(S(s): S<State>, headers: HeaderMap) -> ApiResult<Json<Value>> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(ApiError::unauthorized)?;
    if token.starts_with("miy_access_") {
        crate::auth_sessions::revoke_access(&s.pool, token).await?;
        return Ok(Json(json!({"signedOut":true})));
    }
    sqlx::query("DELETE FROM cli_tokens WHERE token_hash=$1")
        .bind(hash(token))
        .execute(&s.pool)
        .await?;
    Ok(Json(json!({"signedOut":true})))
}
