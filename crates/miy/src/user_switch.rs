//! User switch (M6, docs/CORE-PERMISSIONS.md "사용자 전환"), ported from
//! AI-DO's impersonation: a console administrator signs in as a person. The
//! result is that person's ordinary portal session, with the same lifetime,
//! CSRF and sign-out. The session records who started it, so it ends when that
//! administrator is disabled or loses administration, and the start is audited.
//!
//! The console receives a one-time code and opens the target portal at
//! `/switch-user#<code>`; the portal page posts the code to its own
//! `/auth/switch`. A fragment never reaches a server log or a Referer, and the
//! exchange is a same-origin request on the target portal.
use crate::{
    State,
    auth::{BrowserPurpose, Principal, cookie, encrypt, random},
    contract::hash,
    http::{ApiError, ApiResult},
    policy::{self, PlatformAction},
};
use axum::{
    Json,
    extract::{Path, State as S},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};

const CODE_SECONDS: f64 = 60.0;
/// Same as an ordinary browser session (`sessions.created_at` + 8 hours).
const SESSION_SECONDS: u32 = 28_800;

// Session subqueries use the alias `s`. An existing parent session alone is
// insufficient when an app frame, continuation or live channel rechecks it.
macro_rules! issuer_current_sql {
    () => {
        "(s.impersonator_user_id IS NULL OR EXISTS(SELECT 1 FROM users issuer \
          WHERE issuer.id=s.impersonator_user_id AND issuer.enabled AND issuer.admin))"
    };
}
pub(crate) use issuer_current_sql;

/// Use the same user-row lock order as the account state and role writers.
pub(crate) async fn lock_users(
    conn: &mut sqlx::PgConnection,
    target: &str,
    issuer: &str,
) -> ApiResult<(Option<bool>, bool)> {
    let users: Vec<(String, bool, bool)> =
        sqlx::query_as("SELECT id,enabled,admin FROM users WHERE id=ANY($1) ORDER BY id FOR SHARE")
            .bind([target, issuer].as_slice())
            .fetch_all(conn)
            .await?;
    let target_enabled = users
        .iter()
        .find(|(id, _, _)| id == target)
        .map(|(_, enabled, _)| *enabled);
    let issuer_allowed = users
        .iter()
        .any(|(id, enabled, admin)| id == issuer && *enabled && *admin);
    Ok((target_enabled, issuer_allowed))
}

/// An account state or role writer already holds the user's row lock.
pub(crate) async fn revoke_for_user(
    tx: &mut policy::Decided,
    user: &str,
    disabled: bool,
) -> ApiResult<()> {
    sqlx::query(
        "DELETE FROM user_switch_codes WHERE impersonator_user_id=$1 OR ($2 AND target_user_id=$1)",
    )
    .bind(user)
    .bind(disabled)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "DELETE FROM sessions WHERE impersonator_user_id=$1 \
         OR ($2 AND user_id=$1 AND impersonator_user_id IS NOT NULL)",
    )
    .bind(user)
    .bind(disabled)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Start {
    /// `prod` opens the business portal, `dev` the dev portal.
    pub environment: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exchange {
    pub code: String,
}

/// `POST /api/v1/users/{id}/sign-in-as` (console administrator).
pub async fn start(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path(id): Path<String>,
    Json(input): Json<Start>,
) -> ApiResult<Json<Value>> {
    if p.browser_purpose != Some(BrowserPurpose::Console) {
        return Err(ApiError::forbidden());
    }
    crate::domain::admin(&s, &p).await?;
    let purpose = match input.environment.as_str() {
        "prod" => BrowserPurpose::Portal,
        "dev" => BrowserPurpose::DevPortal,
        _ => return Err(ApiError::bad("invalid_environment")),
    };
    if id == p.id {
        return Err(ApiError::bad("cannot_switch_to_self"));
    }
    let mut tx = policy::Write::begin(&s.pool).await?;
    let (enabled, _) = lock_users(&mut tx, &id, &p.id).await?;
    let mut tx = tx.decide_platform(&p, PlatformAction::Administer).await?;
    let enabled = enabled.ok_or_else(ApiError::not_found)?;
    if !enabled {
        return Err(ApiError::conflict("user_disabled"));
    }
    if crate::accounts::reset_pending(&mut tx, &id).await? {
        return Err(ApiError::conflict("password_reset_in_progress"));
    }
    sqlx::query("DELETE FROM user_switch_codes WHERE expires_at<clock_timestamp()")
        .execute(&mut *tx)
        .await?;
    let code = random();
    let expires_at: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO user_switch_codes(code_hash,target_user_id,impersonator_user_id,browser_purpose,expires_at) \
         VALUES($1,$2,$3,$4,clock_timestamp()+make_interval(secs=>$5)) RETURNING expires_at",
    )
    .bind(hash(&code))
    .bind(&id)
    .bind(&p.id)
    .bind(purpose.as_str())
    .bind(CODE_SECONDS)
    .fetch_one(&mut *tx)
    .await?;
    crate::audit::record(
        &mut tx,
        &p.id,
        "user.sign_in_as",
        &format!("user:{id}:environment:{}", input.environment),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({
        "url": format!("{}/switch-user#{code}", s.auth.browser_origin(purpose)),
        "expiresAt": expires_at,
    })))
}

/// `POST /auth/switch` on a portal: exchange the code for the person's session.
/// Same-origin only; the code is the credential and is used once.
pub async fn exchange(
    S(s): S<State>,
    axum::Extension(purpose): axum::Extension<BrowserPurpose>,
    headers: HeaderMap,
    Json(input): Json<Exchange>,
) -> ApiResult<Response> {
    let own = s.auth.browser_origin(purpose);
    // Exactly one Origin, equal to this portal; at most one Fetch Metadata
    // site, and then same-origin. Repeated headers are refused, not merged.
    let single = |name: &str| {
        let mut values = headers.get_all(name).iter();
        match (values.next(), values.next()) {
            (Some(value), None) => value.to_str().ok().map(Some),
            (None, None) => Some(None),
            _ => None,
        }
    };
    let (origin, site) = (single(header::ORIGIN.as_str()), single("sec-fetch-site"));
    if purpose == BrowserPurpose::Console
        || origin != Some(Some(own))
        || !matches!(site, Some(None | Some("same-origin")))
    {
        return Err(ApiError::forbidden());
    }
    if input.code.is_empty() || input.code.len() > 128 {
        return Err(ApiError::unauthorized());
    }
    let code_hash = hash(&input.code);
    let mut tx = s.pool.begin().await?;
    let users: Option<(String, String)> = sqlx::query_as(
        "SELECT target_user_id,impersonator_user_id FROM user_switch_codes \
         WHERE code_hash=$1 AND browser_purpose=$2",
    )
    .bind(&code_hash)
    .bind(purpose.as_str())
    .fetch_optional(&mut *tx)
    .await?;
    let (target, issuer) = users.ok_or_else(ApiError::unauthorized)?;
    // Lock the users before touching their code or sessions. The account
    // writers revoke switched credentials in exactly this order.
    let (target_enabled, issuer_allowed) = lock_users(&mut tx, &target, &issuer).await?;
    let live: Option<bool> = sqlx::query_scalar(
        "DELETE FROM user_switch_codes c USING users target \
         WHERE c.code_hash=$1 AND c.browser_purpose=$2 AND target.id=c.target_user_id \
         RETURNING c.expires_at>clock_timestamp() \
         AND (target.browser_login_after IS NULL OR c.created_at>=target.browser_login_after)",
    )
    .bind(&code_hash)
    .bind(purpose.as_str())
    .fetch_optional(&mut *tx)
    .await?;
    if live != Some(true) {
        tx.commit().await?;
        return Err(ApiError::unauthorized());
    }
    if target_enabled != Some(true)
        || !issuer_allowed
        || crate::accounts::reset_pending(&mut tx, &target).await?
    {
        tx.commit().await?;
        return Err(ApiError::forbidden());
    }
    // A session this browser already had on this portal (often the
    // administrator's own) ends: one person per portal.
    for kind in ["session", "development"] {
        if let Some(value) = cookie(&headers, &s.auth.cookie_name(purpose, kind)) {
            let table = if kind == "session" {
                "sessions"
            } else {
                "development_sessions"
            };
            sqlx::query(&format!(
                "DELETE FROM {table} WHERE id_hash=$1 AND browser_purpose=$2"
            ))
            .bind(hash(&value))
            .bind(purpose.as_str())
            .execute(&mut *tx)
            .await?;
        }
    }
    let session = random();
    // No identity provider tokens: the session is checked against the
    // platform's own user records instead (auth::session).
    let empty = encrypt("").map_err(|_| ApiError::unavailable("identity_unavailable"))?;
    sqlx::query(
        "INSERT INTO sessions(id_hash,user_id,access_token,refresh_token,csrf,expires_at,browser_purpose,user_agent,last_seen_at,impersonator_user_id) \
         VALUES($1,$2,$3,$3,$4,clock_timestamp()+interval '8 hours',$5,$6,clock_timestamp(),$7)",
    )
    .bind(hash(&session))
    .bind(&target)
    .bind(&empty)
    .bind(random())
    .bind(purpose.as_str())
    .bind(crate::auth::user_agent(&headers))
    .bind(&issuer)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    headers.append(
        header::SET_COOKIE,
        s.auth
            .cookie(purpose, "session", &session, SESSION_SECONDS)
            .parse()
            .unwrap(),
    );
    headers.append(
        header::SET_COOKIE,
        s.auth
            .cookie(purpose, "development", "", 0)
            .parse()
            .unwrap(),
    );
    Ok(response)
}
