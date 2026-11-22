//! A person's own browser sessions on the three surfaces, listed and ended from
//! the portal's security settings (AI-DO's session list). Ending a session
//! deletes it, which also ends the app frames launched from it.
use crate::{
    State,
    api_contract::{BrowserSession, BrowserSessionPage},
    auth::Principal,
    http::{ApiError, ApiResult},
    policy::{PlatformAction, Write},
};
use axum::http::HeaderMap;
use sqlx::Row;
use uuid::Uuid;

/// Live sessions only; real sessions last eight hours from sign-in, matching
/// the check every request makes.
const LIVE: &str = "SELECT id,browser_purpose,created_at,created_at+interval '8 hours' AS expires_at,\
     last_seen_at,user_agent,id_hash FROM sessions \
     WHERE user_id=$1 AND created_at>now()-interval '8 hours' \
     UNION ALL SELECT id,browser_purpose,created_at,expires_at,last_seen_at,user_agent,id_hash \
     FROM development_sessions WHERE user_id=$1 AND expires_at>now()";

fn current(state: &State, p: &Principal, headers: &HeaderMap) -> Option<String> {
    p.browser_purpose
        .and_then(|purpose| state.auth.session_hash(headers, purpose))
}

pub async fn list(
    state: &State,
    p: &Principal,
    headers: &HeaderMap,
) -> ApiResult<BrowserSessionPage> {
    crate::delegations::unrestricted(p)?;
    let current = current(state, p, headers);
    let rows = sqlx::query(&format!(
        "{LIVE} ORDER BY last_seen_at DESC NULLS LAST, created_at DESC LIMIT 200"
    ))
    .bind(&p.id)
    .fetch_all(&state.pool)
    .await?;
    let mut items: Vec<BrowserSession> = rows
        .iter()
        .map(|row| BrowserSession {
            id: row.get("id"),
            surface: row.get("browser_purpose"),
            created_at: row.get("created_at"),
            expires_at: row.get("expires_at"),
            last_seen_at: row.get("last_seen_at"),
            user_agent: row.get("user_agent"),
            current: current.as_deref() == Some(row.get::<&str, _>("id_hash")),
        })
        .collect();
    // This browser's own session first, as AI-DO lists it.
    items.sort_by_key(|item| !item.current);
    Ok(BrowserSessionPage { items })
}

pub async fn revoke(state: &State, p: &Principal, headers: &HeaderMap, id: Uuid) -> ApiResult<()> {
    crate::delegations::unrestricted(p)?;
    let mut tx = Write::begin(&state.pool).await?;
    let hash: Option<String> =
        sqlx::query_scalar(&format!("SELECT id_hash FROM ({LIVE}) live WHERE id=$2"))
            .bind(&p.id)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let hash = hash.ok_or_else(ApiError::not_found)?;
    if current(state, p, headers).as_deref() == Some(hash.as_str()) {
        // Signing out also ends the identity provider session; use that instead.
        return Err(ApiError::conflict("use_sign_out_for_current_session"));
    }
    sqlx::query("DELETE FROM sessions WHERE id=$1 AND user_id=$2")
        .bind(id)
        .bind(&p.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM development_sessions WHERE id=$1 AND user_id=$2")
        .bind(id)
        .bind(&p.id)
        .execute(&mut *tx)
        .await?;
    tx.decide_platform(p, PlatformAction::Personal)
        .await?
        .commit()
        .await
}
