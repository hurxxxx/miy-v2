//! Browser-approved, expiring CLI credentials. Never shares browser cookies with agents.
use crate::{
    State,
    auth::{self, Principal},
    contract::hash,
    domain,
    http::{ApiError, ApiResult},
};
use axum::{
    Json,
    extract::{Path, State as S},
    http::HeaderMap,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Start {
    challenge: String,
}
pub async fn start(
    S(s): S<State>,
    headers: HeaderMap,
    Json(v): Json<Start>,
) -> ApiResult<Json<Value>> {
    // Browser JS must not be able to create invisible CLI login requests cross-origin.
    if headers.contains_key("origin") {
        return Err(ApiError::forbidden());
    }
    if v.challenge.len() != 64 || !v.challenge.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::bad("invalid_challenge"));
    }
    let mut tx = s.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(73490125)")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM cli_authorizations WHERE expires_at<now()")
        .execute(&mut *tx)
        .await?;
    let count: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM cli_authorizations)+(SELECT count(*) FROM cli_device_authorizations WHERE expires_at>now())")
        .fetch_one(&mut *tx)
        .await?;
    if count >= 100 {
        return Err(ApiError::unavailable("too_many_pending_logins"));
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO cli_authorizations(id,challenge) VALUES($1,$2)")
        .bind(id)
        .bind(v.challenge)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"id":id,"verificationPath":format!("/cli/authorize/{id}"),"expiresIn":300}),
    ))
}
pub async fn approve(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    if p.csrf.is_none() {
        return Err(ApiError::forbidden());
    }
    crate::auth_sessions::ensure_active(&s.pool, &p.id).await?;
    domain::me(&s, &p).await?;
    let mut tx = s.pool.begin().await?;
    crate::auth_sessions::active_locked(&mut tx, &p.id).await?;
    sqlx::query("SELECT id FROM cli_authorizations WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let changed = sqlx::query("UPDATE cli_authorizations SET user_id=$2 WHERE id=$1 AND expires_at>clock_timestamp() AND user_id IS NULL")
        .bind(id).bind(&p.id).execute(&mut *tx).await?.rows_affected();
    if changed != 1 {
        return Err(ApiError::conflict("login_request_expired_or_approved"));
    }
    tx.commit().await?;
    Ok(Json(json!({"approved":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exchange {
    id: Uuid,
    verifier: String,
}
pub async fn exchange(
    S(s): S<State>,
    headers: HeaderMap,
    Json(v): Json<Exchange>,
) -> ApiResult<Json<Value>> {
    if headers.contains_key("origin") {
        return Err(ApiError::forbidden());
    }
    if v.verifier.len() < 32 || v.verifier.len() > 128 {
        return Err(ApiError::unauthorized());
    }
    let challenge = hash(&v.verifier);
    let approved_user: Option<String> = sqlx::query_scalar(
        "SELECT user_id FROM cli_authorizations WHERE id=$1 AND challenge=$2 AND expires_at>now()",
    )
    .bind(v.id)
    .bind(&challenge)
    .fetch_optional(&s.pool)
    .await?
    .flatten();
    let mut tx = s.pool.begin().await?;
    if let Some(user) = &approved_user {
        crate::auth_sessions::active_locked(&mut tx, user).await?;
    }
    let row = sqlx::query("SELECT user_id,expires_at FROM cli_authorizations WHERE id=$1 AND challenge=$2 AND expires_at>now() FOR UPDATE")
        .bind(v.id).bind(&challenge).fetch_optional(&mut *tx).await?.ok_or_else(ApiError::unauthorized)?;
    let expired: bool = sqlx::query_scalar("SELECT $1::timestamptz<=clock_timestamp()")
        .bind(row.get::<chrono::DateTime<chrono::Utc>, _>("expires_at"))
        .fetch_one(&mut *tx)
        .await?;
    if expired {
        return Err(ApiError::unauthorized());
    }
    let Some(user) = row.get::<Option<String>, _>("user_id") else {
        return Ok(Json(json!({"status":"pending"})));
    };
    if Some(&user) != approved_user.as_ref() {
        return Ok(Json(json!({"status":"pending"})));
    }
    let token = format!("miy_cli_{}", auth::random());
    sqlx::query("INSERT INTO cli_tokens(token_hash,user_id) VALUES($1,$2)")
        .bind(hash(&token))
        .bind(user)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM cli_authorizations WHERE id=$1")
        .bind(v.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"status":"approved","token":token,"expiresIn":28800}),
    ))
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
        cli_session_id: None,
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
