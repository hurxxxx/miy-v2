//! Browser-approved CLI session families with rotating, hash-only credentials.
//!
//! Core `users.enabled` is authoritative. This module does not infer that an
//! upstream identity-provider disable has propagated to Core.
use crate::{
    State,
    api_error::{ApiError, ApiResult},
    auth::{self, Principal},
    contract::hash,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State as S},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool, Row};
use uuid::Uuid;

const DEVICE_PREFIX: &str = "miy_device_";
const ACCESS_PREFIX: &str = "miy_access_";
const REFRESH_PREFIX: &str = "miy_refresh_";
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const POLL_SECONDS: i32 = 5;
const ACCESS_SECONDS: i64 = 900;

fn error(status: StatusCode, code: &str) -> ApiError {
    ApiError {
        status,
        code: code.into(),
    }
}

fn private_json(value: Value) -> Response {
    (
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::PRAGMA, "no-cache"),
        ],
        Json(value),
    )
        .into_response()
}

fn public_client(headers: &HeaderMap) -> ApiResult<()> {
    if headers.contains_key(header::ORIGIN) {
        return Err(ApiError::forbidden());
    }
    Ok(())
}

fn valid_secret(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|secret| {
        (32..=128).contains(&secret.len())
            && secret
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    })
}

fn client_name(name: Option<&str>) -> ApiResult<String> {
    let name = name.unwrap_or("miy CLI").trim();
    if name.is_empty()
        || name.chars().count() > 80
        || name.chars().any(|character| {
            character.is_control()
                || matches!(character, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
    {
        return Err(ApiError::bad("invalid_client_name"));
    }
    Ok(name.to_owned())
}

fn user_code() -> String {
    let mut rng = rand::thread_rng();
    let letters: String = (0..10)
        .map(|_| CODE_ALPHABET[rng.gen_range(0..CODE_ALPHABET.len())] as char)
        .collect();
    format!("{}-{}", &letters[..5], &letters[5..])
}

fn normalized_code(value: &str) -> Option<String> {
    if value.len() > 16 {
        return None;
    }
    let code: String = value
        .bytes()
        .filter(|byte| *byte != b'-' && *byte != b' ')
        .map(|byte| byte.to_ascii_uppercase() as char)
        .collect();
    (code.len() == 10 && code.bytes().all(|byte| CODE_ALPHABET.contains(&byte))).then_some(code)
}

fn code_hash(id: Uuid, code: &str) -> String {
    hash(format!("{id}:{code}"))
}

/// Called on issuance, refresh and every authenticated request. Absence and
/// database failure fail closed; the Core switch is checked without a cache.
pub async fn ensure_active(pool: &PgPool, user: &str) -> ApiResult<()> {
    let enabled: Option<bool> = sqlx::query_scalar("SELECT enabled FROM users WHERE id=$1")
        .bind(user)
        .fetch_optional(pool)
        .await?;
    active_result(enabled)
}

fn active_result(enabled: Option<bool>) -> ApiResult<()> {
    match enabled {
        Some(true) => Ok(()),
        Some(false) => Err(error(StatusCode::FORBIDDEN, "user_disabled")),
        None => Err(ApiError::unauthorized()),
    }
}

pub(crate) async fn active_locked(connection: &mut PgConnection, user: &str) -> ApiResult<()> {
    let enabled: Option<bool> =
        sqlx::query_scalar("SELECT enabled FROM users WHERE id=$1 FOR SHARE")
            .bind(user)
            .fetch_optional(connection)
            .await?;
    active_result(enabled)
}

/// The first entry is always a global limit. Commit rejected attempts as well,
/// and stop before inserting attacker-controlled keys after that limit is hit.
async fn limits(pool: &PgPool, entries: &[(&str, &str, i32)]) -> ApiResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(73490126)")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM cli_auth_rate_limits WHERE window_start < date_trunc('minute',now()) - interval '1 minute'")
        .execute(&mut *tx).await?;
    for (scope, subject, limit) in entries {
        let attempts: i32 = sqlx::query_scalar("INSERT INTO cli_auth_rate_limits(scope,subject_hash,window_start,attempts) VALUES($1,$2,date_trunc('minute',now()),1) ON CONFLICT(scope,subject_hash,window_start) DO UPDATE SET attempts=LEAST(cli_auth_rate_limits.attempts+1,1000000) RETURNING attempts")
            .bind(scope).bind(hash(subject)).fetch_one(&mut *tx).await?;
        if attempts > *limit {
            tx.commit().await?;
            return Err(error(StatusCode::TOO_MANY_REQUESTS, "rate_limited"));
        }
    }
    tx.commit().await?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceStart {
    client_id: Uuid,
    client_name: Option<String>,
}

pub async fn start(
    S(state): S<State>,
    headers: HeaderMap,
    Json(request): Json<DeviceStart>,
) -> ApiResult<Response> {
    public_client(&headers)?;
    if request.client_id.is_nil() {
        return Err(ApiError::bad("invalid_client_id"));
    }
    let name = client_name(request.client_name.as_deref())?;
    let client = request.client_id.to_string();
    limits(
        &state.pool,
        &[
            ("start_global", "global", 120),
            ("start_client", &client, 5),
        ],
    )
    .await?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(73490125)")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM cli_device_authorizations WHERE expires_at<=now()")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM cli_authorizations WHERE expires_at<=now()")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM cli_access_tokens WHERE expires_at<=now()")
        .execute(&mut *tx)
        .await?;
    // A used hash remains available for reuse detection for the family's whole
    // lifetime. Only an expired family may lose its refresh history.
    sqlx::query("DELETE FROM cli_refresh_tokens r USING cli_sessions s WHERE r.session_id=s.id AND s.expires_at<=now()")
        .execute(&mut *tx).await?;
    let count: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM cli_device_authorizations)+(SELECT count(*) FROM cli_authorizations)")
        .fetch_one(&mut *tx).await?;
    if count >= 100 {
        return Err(ApiError::unavailable("too_many_pending_logins"));
    }
    let id = Uuid::new_v4();
    let device = format!("{DEVICE_PREFIX}{}", auth::random());
    let code = user_code();
    sqlx::query("INSERT INTO cli_device_authorizations(id,client_id,client_name,device_secret_hash,user_code_hash) VALUES($1,$2,$3,$4,$5)")
        .bind(id).bind(request.client_id).bind(name).bind(hash(&device))
        .bind(code_hash(id,&normalized_code(&code).unwrap())).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(private_json(
        json!({"id":id,"deviceCode":device,"userCode":code,
        "verificationPath":format!("/cli/device/{id}"),"expiresIn":300,"interval":POLL_SECONDS}),
    ))
}

/// Browser-only public request metadata. Never expose either approval secret.
pub async fn device_status(
    S(state): S<State>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> ApiResult<Response> {
    if principal.csrf.is_none() {
        return Err(ApiError::forbidden());
    }
    ensure_active(&state.pool, &principal.id).await?;
    let row = sqlx::query("SELECT client_name,status,user_id,expires_at,GREATEST(0,EXTRACT(EPOCH FROM (expires_at-clock_timestamp()))::bigint) AS remaining FROM cli_device_authorizations WHERE id=$1")
        .bind(id).fetch_optional(&state.pool).await?.ok_or_else(ApiError::not_found)?;
    let owner: Option<String> = row.get("user_id");
    if owner.as_deref().is_some_and(|owner| owner != principal.id) {
        return Err(ApiError::not_found());
    }
    let remaining: i64 = row.get("remaining");
    let status: String = row.get("status");
    Ok(private_json(
        json!({"id":id,"clientName":row.get::<String,_>("client_name"),
        "status":if remaining == 0 { "expired" } else { &status },
        "expiresAt":row.get::<DateTime<Utc>,_>("expires_at"),"expiresIn":remaining}),
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Approve,
    Deny,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Approval {
    user_code: String,
    decision: Decision,
}

pub async fn approve(
    S(state): S<State>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
    Json(request): Json<Approval>,
) -> ApiResult<Response> {
    if principal.csrf.is_none() {
        return Err(ApiError::forbidden());
    }
    ensure_active(&state.pool, &principal.id).await?;
    limits(
        &state.pool,
        &[
            ("approval_global", "global", 120),
            ("approval_actor", &principal.id, 10),
        ],
    )
    .await?;
    let code =
        normalized_code(&request.user_code).ok_or_else(|| ApiError::bad("invalid_user_code"))?;
    let mut tx = state.pool.begin().await?;
    active_locked(&mut tx, &principal.id).await?;
    let status = match request.decision {
        Decision::Approve => "approved",
        Decision::Deny => "denied",
    };
    sqlx::query("SELECT id FROM cli_device_authorizations WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    let changed = sqlx::query("UPDATE cli_device_authorizations SET status=$3,user_id=$4 WHERE id=$1 AND user_code_hash=$2 AND status='pending' AND expires_at>clock_timestamp()")
        .bind(id).bind(code_hash(id,&code)).bind(status).bind(&principal.id).execute(&mut *tx).await?.rows_affected();
    if changed != 1 {
        return Err(ApiError::conflict("login_request_expired_or_decided"));
    }
    tx.commit().await?;
    Ok(private_json(json!({"id":id,"status":status})))
}

#[derive(Deserialize)]
#[serde(tag = "grantType", deny_unknown_fields)]
pub enum TokenRequest {
    #[serde(rename = "device_code")]
    Device {
        id: Uuid,
        #[serde(rename = "deviceCode")]
        device_code: String,
    },
    #[serde(rename = "refresh_token")]
    Refresh {
        #[serde(rename = "refreshToken")]
        refresh_token: String,
    },
}

pub async fn token(
    S(state): S<State>,
    headers: HeaderMap,
    Json(request): Json<TokenRequest>,
) -> ApiResult<Response> {
    public_client(&headers)?;
    limits(&state.pool, &[("token_global", "global", 12000)]).await?;
    match request {
        TokenRequest::Device { id, device_code } => {
            exchange_device(&state.pool, id, &device_code).await
        }
        TokenRequest::Refresh { refresh_token } => rotate(&state.pool, &refresh_token).await,
    }
}

async fn exchange_device(pool: &PgPool, id: Uuid, secret: &str) -> ApiResult<Response> {
    if !valid_secret(secret, DEVICE_PREFIX) {
        return Err(error(StatusCode::BAD_REQUEST, "invalid_grant"));
    }
    let digest = hash(secret);
    let approved_user: Option<String> = sqlx::query_scalar("SELECT user_id FROM cli_device_authorizations WHERE id=$1 AND device_secret_hash=$2 AND status='approved'")
        .bind(id).bind(&digest).fetch_optional(pool).await?.flatten();
    let mut tx = pool.begin().await?;
    // Match the disable trigger's user -> credential lock order. If approval
    // races this read, return one pending poll instead of locking in reverse.
    if let Some(user) = &approved_user {
        active_locked(&mut tx, user).await?;
    }
    let row = sqlx::query(
        "SELECT * FROM cli_device_authorizations WHERE id=$1 AND device_secret_hash=$2 FOR UPDATE",
    )
    .bind(id)
    .bind(&digest)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| error(StatusCode::BAD_REQUEST, "invalid_grant"))?;
    let checked_at: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut *tx)
        .await?;
    if row.get::<DateTime<Utc>, _>("expires_at") <= checked_at {
        return Err(error(StatusCode::BAD_REQUEST, "expired_token"));
    }
    match row.get::<String, _>("status").as_str() {
        "denied" => return Err(error(StatusCode::BAD_REQUEST, "access_denied")),
        "consumed" => return Err(error(StatusCode::BAD_REQUEST, "invalid_grant")),
        _ => {}
    }
    let interval: i32 = row.get("poll_interval");
    if row
        .get::<Option<DateTime<Utc>>, _>("last_poll_at")
        .is_some_and(|last| last + Duration::seconds(i64::from(interval)) > checked_at)
    {
        let interval = (interval + POLL_SECONDS).min(60);
        sqlx::query(
            "UPDATE cli_device_authorizations SET last_poll_at=clock_timestamp(),poll_interval=$2 WHERE id=$1",
        )
        .bind(id)
        .bind(interval)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(private_json(
            json!({"status":"slow_down","interval":interval}),
        ));
    }
    sqlx::query("UPDATE cli_device_authorizations SET last_poll_at=clock_timestamp() WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let user = row.get::<Option<String>, _>("user_id");
    if user.is_none() || user != approved_user {
        tx.commit().await?;
        return Ok(private_json(
            json!({"status":"pending","interval":interval}),
        ));
    }
    let user = user.unwrap();
    let session = Uuid::new_v4();
    let expires: DateTime<Utc> = sqlx::query_scalar("INSERT INTO cli_sessions(id,user_id,client_id,client_name) VALUES($1,$2,$3,$4) RETURNING expires_at")
        .bind(session).bind(&user).bind(row.get::<Uuid,_>("client_id")).bind(row.get::<String,_>("client_name"))
        .fetch_one(&mut *tx).await?;
    let tokens = issue(&mut tx, session, &user, expires).await?;
    sqlx::query("UPDATE cli_device_authorizations SET status='consumed' WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(private_json(tokens))
}

async fn issue(
    connection: &mut PgConnection,
    session: Uuid,
    user: &str,
    expires: DateTime<Utc>,
) -> ApiResult<Value> {
    let access = format!("{ACCESS_PREFIX}{}", auth::random());
    let refresh = format!("{REFRESH_PREFIX}{}", auth::random());
    let access_expires: DateTime<Utc> = sqlx::query_scalar("INSERT INTO cli_access_tokens(token_hash,session_id,expires_at) VALUES($1,$2,LEAST(now()+interval '15 minutes',$3)) RETURNING expires_at")
        .bind(hash(&access)).bind(session).bind(expires).fetch_one(&mut *connection).await?;
    sqlx::query("INSERT INTO cli_refresh_tokens(token_hash,session_id) VALUES($1,$2)")
        .bind(hash(&refresh))
        .bind(session)
        .execute(&mut *connection)
        .await?;
    Ok(
        json!({"status":"approved","sessionId":session,"userId":user,"accessToken":access,
        "refreshToken":refresh,"tokenType":"Bearer","expiresIn":((access_expires-Utc::now()).num_milliseconds()+999).div_euclid(1000).clamp(0,ACCESS_SECONDS),
        "refreshExpiresAt":expires}),
    )
}

async fn rotate(pool: &PgPool, secret: &str) -> ApiResult<Response> {
    if !valid_secret(secret, REFRESH_PREFIX) {
        return Err(error(StatusCode::BAD_REQUEST, "invalid_grant"));
    }
    let digest = hash(secret);
    let identity =
        sqlx::query("SELECT r.session_id,s.user_id FROM cli_refresh_tokens r JOIN cli_sessions s ON s.id=r.session_id WHERE r.token_hash=$1")
            .bind(&digest)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| error(StatusCode::BAD_REQUEST, "invalid_grant"))?;
    let session: Uuid = identity.get("session_id");
    let user: String = identity.get("user_id");
    let family = session.to_string();
    limits(
        pool,
        &[
            ("refresh_global", "global", 1200),
            ("refresh_family", &family, 10),
        ],
    )
    .await?;
    let mut tx = pool.begin().await?;
    active_locked(&mut tx, &user).await?;
    let row = sqlx::query("SELECT * FROM cli_sessions WHERE id=$1 FOR UPDATE")
        .bind(session)
        .fetch_one(&mut *tx)
        .await?;
    let checked_at: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut *tx)
        .await?;
    if row.get::<DateTime<Utc>, _>("expires_at") <= checked_at {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "session_expired_or_revoked",
        ));
    }
    // This separate statement sees a preceding rotation after waiting for the
    // family lock; never rely on a joined token row's earlier MVCC snapshot.
    let used: bool = sqlx::query_scalar(
        "SELECT used_at IS NOT NULL FROM cli_refresh_tokens WHERE token_hash=$1",
    )
    .bind(&digest)
    .fetch_one(&mut *tx)
    .await?;
    if used {
        sqlx::query("UPDATE cli_sessions SET revoked_at=COALESCE(revoked_at,now()),revocation_reason='refresh_reuse' WHERE id=$1")
            .bind(session).execute(&mut *tx).await?;
        tx.commit().await?; // Returning Err before commit would undo revocation.
        return Err(error(StatusCode::UNAUTHORIZED, "refresh_reuse_detected"));
    }
    if row.get::<Option<DateTime<Utc>>, _>("revoked_at").is_some() {
        return Err(error(
            StatusCode::UNAUTHORIZED,
            "session_expired_or_revoked",
        ));
    }
    sqlx::query("UPDATE cli_refresh_tokens SET used_at=now() WHERE token_hash=$1")
        .bind(&digest)
        .execute(&mut *tx)
        .await?;
    let tokens = issue(&mut tx, session, &user, row.get("expires_at")).await?;
    sqlx::query("UPDATE cli_sessions SET last_used_at=now() WHERE id=$1")
        .bind(session)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(private_json(tokens))
}

pub async fn principal(state: &State, token: &str) -> ApiResult<Principal> {
    if !valid_secret(token, ACCESS_PREFIX) {
        return Err(ApiError::unauthorized());
    }
    let row = sqlx::query("SELECT s.id AS cli_session_id,s.user_id,u.enabled FROM cli_access_tokens a JOIN cli_sessions s ON s.id=a.session_id JOIN users u ON u.id=s.user_id WHERE a.token_hash=$1 AND a.expires_at>now() AND s.expires_at>now() AND s.revoked_at IS NULL")
        .bind(hash(token)).fetch_optional(&state.pool).await?.ok_or_else(ApiError::unauthorized)?;
    active_result(Some(row.get("enabled")))?;
    Ok(Principal {
        cli_session_id: Some(row.get("cli_session_id")),
        id: row.get("user_id"),
        scopes: vec!["miy.read".into(), "miy.dev".into()],
        csrf: None,
        development: false,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeRequest {
    refresh_token: String,
}

pub async fn revoke(
    S(state): S<State>,
    headers: HeaderMap,
    Json(request): Json<RevokeRequest>,
) -> ApiResult<Response> {
    public_client(&headers)?;
    limits(&state.pool, &[("revoke_global", "global", 1200)]).await?;
    if !valid_secret(&request.refresh_token, REFRESH_PREFIX) {
        return Err(ApiError::bad("invalid_refresh_token"));
    }
    sqlx::query("UPDATE cli_sessions SET revoked_at=COALESCE(revoked_at,now()),revocation_reason=COALESCE(revocation_reason,'logout') WHERE id=(SELECT session_id FROM cli_refresh_tokens WHERE token_hash=$1)")
        .bind(hash(&request.refresh_token)).execute(&state.pool).await?;
    Ok(private_json(json!({"revoked":true})))
}

/// Preserve the legacy logout endpoint for callers carrying a new access token.
pub async fn revoke_access(pool: &PgPool, token: &str) -> ApiResult<()> {
    sqlx::query("UPDATE cli_sessions SET revoked_at=COALESCE(revoked_at,now()),revocation_reason=COALESCE(revocation_reason,'logout') WHERE id=(SELECT session_id FROM cli_access_tokens WHERE token_hash=$1)")
        .bind(hash(token)).execute(pool).await?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionPage {
    limit: Option<u16>,
    cursor: Option<String>,
}

impl SessionPage {
    fn limit(&self) -> ApiResult<i64> {
        let limit = self.limit.unwrap_or(50);
        if !(1..=200).contains(&limit) {
            return Err(ApiError::bad("invalid_limit"));
        }
        Ok(i64::from(limit))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionCursor {
    version: u8,
    user_id: String,
    created_at: DateTime<Utc>,
    id: Uuid,
}

impl SessionCursor {
    fn encode(&self) -> ApiResult<String> {
        Ok(URL_SAFE_NO_PAD.encode(serde_json::to_vec(self)?))
    }

    fn decode(raw: &str, user: &str) -> ApiResult<Self> {
        if raw.len() > 2048 {
            return Err(ApiError::bad("invalid_cursor"));
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(raw)
            .map_err(|_| ApiError::bad("invalid_cursor"))?;
        let cursor: Self =
            serde_json::from_slice(&bytes).map_err(|_| ApiError::bad("invalid_cursor"))?;
        if cursor.version != 1 || cursor.user_id != user || cursor.id.is_nil() {
            return Err(ApiError::bad("invalid_cursor"));
        }
        Ok(cursor)
    }
}

pub async fn list_sessions(
    S(state): S<State>,
    Extension(principal): Extension<Principal>,
    Query(page): Query<SessionPage>,
) -> ApiResult<Response> {
    ensure_active(&state.pool, &principal.id).await?;
    let limit = page.limit()?;
    let cursor = page
        .cursor
        .as_deref()
        .map(|raw| SessionCursor::decode(raw, &principal.id))
        .transpose()?;
    let mut rows = sqlx::query("SELECT id,client_name,created_at,expires_at,last_used_at,revoked_at FROM cli_sessions WHERE user_id=$1 AND ($2::timestamptz IS NULL OR (created_at,id)<($2,$3::uuid)) ORDER BY created_at DESC,id DESC LIMIT $4")
        .bind(&principal.id).bind(cursor.as_ref().map(|cursor|cursor.created_at))
        .bind(cursor.as_ref().map(|cursor|cursor.id)).bind(limit+1).fetch_all(&state.pool).await?;
    let has_more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let next = if has_more {
        let last = rows.last().unwrap();
        Some(
            SessionCursor {
                version: 1,
                user_id: principal.id.clone(),
                created_at: last.get("created_at"),
                id: last.get("id"),
            }
            .encode()?,
        )
    } else {
        None
    };
    let sessions: Vec<Value> = rows.into_iter().map(|row| json!({"id":row.get::<Uuid,_>("id"),
        "clientName":row.get::<String,_>("client_name"),"createdAt":row.get::<DateTime<Utc>,_>("created_at"),
        "expiresAt":row.get::<DateTime<Utc>,_>("expires_at"),"lastUsedAt":row.get::<DateTime<Utc>,_>("last_used_at"),
        "revokedAt":row.get::<Option<DateTime<Utc>>,_>("revoked_at")})).collect();
    Ok(private_json(json!({"sessions":sessions,"nextCursor":next})))
}

pub async fn revoke_session(
    S(state): S<State>,
    Extension(principal): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> ApiResult<Response> {
    ensure_active(&state.pool, &principal.id).await?;
    let changed = sqlx::query("UPDATE cli_sessions SET revoked_at=COALESCE(revoked_at,now()),revocation_reason=COALESCE(revocation_reason,'user_revoked') WHERE id=$1 AND user_id=$2")
        .bind(id).bind(&principal.id).execute(&state.pool).await?.rows_affected();
    if changed != 1 {
        return Err(ApiError::not_found());
    }
    Ok(private_json(json!({"id":id,"revoked":true})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_code_is_high_entropy_formatted_and_bound_to_login_id() {
        let code = user_code();
        assert_eq!(code.len(), 11);
        assert_eq!(code.as_bytes()[5], b'-');
        let normalized = normalized_code(&code).unwrap();
        assert_eq!(
            normalized_code(&code.to_ascii_lowercase()).unwrap(),
            normalized
        );
        assert!(normalized_code("AAAAA-AAAA0").is_none());
        assert!(normalized_code("AAAAAAAAAA secret").is_none());
        assert_ne!(
            code_hash(Uuid::from_u128(1), &normalized),
            code_hash(Uuid::from_u128(2), &normalized)
        );
    }

    #[test]
    fn credentials_cannot_be_confused_with_other_token_kinds() {
        let raw = auth::random();
        assert!(valid_secret(
            &format!("{ACCESS_PREFIX}{raw}"),
            ACCESS_PREFIX
        ));
        assert!(!valid_secret(
            &format!("{REFRESH_PREFIX}{raw}"),
            ACCESS_PREFIX
        ));
        assert!(!valid_secret("miy_access_short", ACCESS_PREFIX));
        assert!(!valid_secret(
            &format!("{ACCESS_PREFIX}{}\n", "a".repeat(40)),
            ACCESS_PREFIX
        ));
    }

    #[test]
    fn client_labels_are_bounded_and_reject_terminal_controls() {
        assert_eq!(client_name(None).unwrap(), "miy CLI");
        assert_eq!(client_name(Some("  작업 PC  ")).unwrap(), "작업 PC");
        assert!(client_name(Some("pc\u{1b}[31m")).is_err());
        assert!(client_name(Some("pc\u{202e}exe")).is_err());
        assert!(client_name(Some(&"x".repeat(81))).is_err());
        assert!(client_name(Some(" ")).is_err());
    }

    #[test]
    fn wire_requests_reject_unknown_fields_and_invalid_grants() {
        assert!(
            serde_json::from_value::<TokenRequest>(
                json!({"grantType":"device_code","id":Uuid::new_v4(),"deviceCode":"secret"})
            )
            .is_ok()
        );
        assert!(
            serde_json::from_value::<TokenRequest>(
                json!({"grantType":"refresh_token","refreshToken":"secret","userId":"other"})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<TokenRequest>(
                json!({"grantType":"password","password":"secret"})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<Approval>(
                json!({"userCode":"code","decision":"approve","userId":"other"})
            )
            .is_err()
        );
    }

    #[test]
    fn active_source_fails_closed_without_treating_absence_as_enabled() {
        assert!(active_result(Some(true)).is_ok());
        let disabled = active_result(Some(false)).unwrap_err();
        assert_eq!(disabled.code, "user_disabled");
        assert_eq!(disabled.status, StatusCode::FORBIDDEN);
        assert_eq!(
            active_result(None).unwrap_err().status,
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn session_cursor_is_versioned_bounded_and_scoped_to_the_authenticated_user() {
        let mut cursor = SessionCursor {
            version: 1,
            user_id: "alice".into(),
            created_at: Utc::now(),
            id: Uuid::new_v4(),
        };
        let raw = cursor.encode().unwrap();
        let decoded = SessionCursor::decode(&raw, "alice").unwrap();
        assert_eq!(decoded.id, cursor.id);
        assert_eq!(decoded.created_at, cursor.created_at);
        assert!(SessionCursor::decode(&raw, "bob").is_err());
        assert!(SessionCursor::decode(&"a".repeat(2049), "alice").is_err());
        assert!(SessionCursor::decode("not@base64", "alice").is_err());
        cursor.version = 2;
        assert!(SessionCursor::decode(&cursor.encode().unwrap(), "alice").is_err());
        cursor.version = 1;
        cursor.id = Uuid::nil();
        assert!(SessionCursor::decode(&cursor.encode().unwrap(), "alice").is_err());
        for (limit, valid) in [
            (None, true),
            (Some(0), false),
            (Some(1), true),
            (Some(200), true),
            (Some(201), false),
        ] {
            assert_eq!(
                SessionPage {
                    limit,
                    cursor: None
                }
                .limit()
                .is_ok(),
                valid
            );
        }
    }
}
