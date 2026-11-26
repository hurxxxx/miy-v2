//! One-time account links. An administrator's invitation lets one person create
//! their own sign-in account, which is registered here at the same moment; a reset
//! link lets a registered person set a new password. No mail server is involved:
//! the administrator hands the link over, and only its hash is stored.
//!
//! The identity provider still only authenticates. A link grants nothing unless
//! its issuer is an enabled administrator when it is used, and an invited person
//! holds no app permission until one is granted.
use crate::{
    State,
    api_error::{ApiError, ApiResult},
    auth::{BrowserPurpose, Principal},
    contract::hash,
    policy::{PlatformAction, Write},
};
use axum::{
    Extension, Json, Router,
    extract::{DefaultBodyLimit, Path, State as S},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use std::time::Duration;
use url::Url;
use uuid::Uuid;

/// The realm's managed password policy asks for at least this many characters.
/// A stricter realm policy is still enforced by the identity provider.
pub const PASSWORD_MIN: usize = 10;
const PASSWORD_MAX: usize = 256;
/// Completion attempts one link accepts before it stops working.
const MAX_ATTEMPTS: i32 = 20;
/// Unused links kept at once; an administrator revokes old ones to issue more.
const MAX_PENDING: i64 = 500;
const REALM: &str = "miy";
const CLIENT_ID: &str = "miy-accounts";

enum PasswordChangeError {
    Refused(ApiError),
    Uncertain,
}

/// Loopback administration of the identity provider's sign-in accounts with a
/// service client limited to this realm's users.
pub struct IdentityAdmin {
    base: String,
    secret: String,
    client: reqwest::Client,
}

impl std::fmt::Debug for IdentityAdmin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentityAdmin")
            .field("base", &self.base)
            .field("secret", &"[redacted]")
            .finish()
    }
}

impl IdentityAdmin {
    /// Both settings or neither: a half-configured server does not start.
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let setting = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
        match (
            setting("MIY_IDENTITY_ADMIN_URL"),
            setting("MIY_ACCOUNTS_SECRET"),
        ) {
            (None, None) => Ok(None),
            (Some(url), Some(secret)) => {
                let mut builder = reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .timeout(std::time::Duration::from_secs(10))
                    .retry(reqwest::retry::never())
                    .no_proxy();
                if let Some(path) =
                    std::env::var_os("MIY_CA_FILE").filter(|value| !value.is_empty())
                {
                    let cert = reqwest::Certificate::from_pem(&std::fs::read(path)?)?;
                    builder = builder.add_root_certificate(cert);
                }
                Ok(Some(Self {
                    base: admin_base(&url)?,
                    secret,
                    client: builder.build()?,
                }))
            }
            _ => {
                anyhow::bail!("MIY_IDENTITY_ADMIN_URL and MIY_ACCOUNTS_SECRET must be set together")
            }
        }
    }

    async fn token(&self) -> ApiResult<String> {
        #[derive(Deserialize)]
        struct Reply {
            access_token: String,
        }
        let reply = self
            .client
            .post(format!(
                "{}/realms/{REALM}/protocol/openid-connect/token",
                self.base
            ))
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", CLIENT_ID),
                ("client_secret", self.secret.as_str()),
            ])
            .send()
            .await
            .map_err(|_| unavailable())?;
        if !reply.status().is_success() {
            tracing::error!(
                event = "identity_admin_token_rejected",
                status = reply.status().as_u16()
            );
            return Err(unavailable());
        }
        Ok(reply
            .json::<Reply>()
            .await
            .map_err(|_| unavailable())?
            .access_token)
    }

    fn users(&self, tail: &str) -> String {
        format!("{}/admin/realms/{REALM}/users{tail}", self.base)
    }

    /// Return registered subject candidates for an exact directory search. No provider fields leave Core.
    pub(crate) async fn search_alias(&self, query: &str) -> ApiResult<Vec<String>> {
        #[derive(Deserialize)]
        struct Hit {
            id: String,
            username: String,
            email: Option<String>,
            enabled: bool,
        }
        let token = self.token().await?;
        let mut ids = Vec::new();
        let fields: &[&str] = if query.contains('@') {
            &["username", "email"]
        } else {
            &["username"]
        };
        for field in fields {
            let reply = self
                .client
                .get(self.users(""))
                .bearer_auth(&token)
                .query(&[
                    (*field, query),
                    ("exact", "true"),
                    ("max", "100"),
                    ("briefRepresentation", "true"),
                ])
                .send()
                .await
                .map_err(|_| unavailable())?
                .error_for_status()
                .map_err(|_| unavailable())?;
            let hits: Vec<Hit> = reply.json().await.map_err(|_| unavailable())?;
            for hit in hits {
                if hit.enabled
                    && (hit.username.eq_ignore_ascii_case(query)
                        || hit
                            .email
                            .as_ref()
                            .is_some_and(|e| e.eq_ignore_ascii_case(query)))
                {
                    ids.push(hit.id);
                }
            }
        }
        Ok(ids)
    }

    /// Create an enabled sign-in account and return its subject ID. The identity
    /// provider applies its password policy to the same request, so a refused
    /// password leaves no account behind.
    async fn create(&self, username: &str, email: &str, password: &str) -> ApiResult<String> {
        let token = self.token().await?;
        let reply = self
            .client
            .post(self.users(""))
            .bearer_auth(token)
            .json(&json!({
                "username": username, "email": email, "enabled": true, "emailVerified": false,
                "credentials": [{"type": "password", "value": password, "temporary": false}],
            }))
            .send()
            .await
            .map_err(|_| unavailable())?;
        match reply.status().as_u16() {
            201 => {
                let id = reply
                    .headers()
                    .get("location")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.rsplit('/').next())
                    .unwrap_or_default();
                crate::user_admin::subject(id).map_err(|_| {
                    tracing::error!(event = "identity_account_without_subject");
                    unavailable()
                })
            }
            409 => Err(ApiError::conflict("account_exists")),
            400 => Err(rejected(reply).await),
            status => {
                tracing::error!(event = "identity_account_create_failed", status);
                Err(unavailable())
            }
        }
    }

    async fn set_password(
        &self,
        token: &str,
        subject: &str,
        password: &str,
    ) -> Result<(), PasswordChangeError> {
        let reply = self
            .client
            .put(self.users(&format!("/{subject}/reset-password")))
            .bearer_auth(token)
            .json(&json!({"type": "password", "value": password, "temporary": false}))
            .send()
            .await
            .map_err(|_| PasswordChangeError::Uncertain)?;
        match reply.status().as_u16() {
            204 => Ok(()),
            400 => Err(PasswordChangeError::Refused(rejected(reply).await)),
            401 | 403 => Err(PasswordChangeError::Refused(unavailable())),
            404 => Err(PasswordChangeError::Refused(ApiError::conflict(
                "identity_account_missing",
            ))),
            status => {
                tracing::error!(event = "identity_password_change_failed", status);
                Err(PasswordChangeError::Uncertain)
            }
        }
    }

    async fn end_sessions(&self, subject: &str) -> bool {
        let Ok(token) = self.token().await else {
            return false;
        };
        self.client
            .post(self.users(&format!("/{subject}/logout")))
            .bearer_auth(token)
            .send()
            .await
            .is_ok_and(|reply| reply.status().is_success())
    }

    /// Undo a sign-in account whose registration did not commit.
    async fn remove(&self, subject: &str) -> bool {
        match self.token().await {
            Ok(token) => self
                .client
                .delete(self.users(&format!("/{subject}")))
                .bearer_auth(token)
                .send()
                .await
                .is_ok_and(|reply| reply.status().is_success()),
            Err(_) => false,
        }
    }
}

/// Administrative credentials only cross a loopback connection.
fn admin_base(value: &str) -> anyhow::Result<String> {
    let url = Url::parse(value)?;
    let loopback = match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(name)) => name == "localhost",
        None => false,
    };
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https")
            && loopback
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "identity_administration_requires_a_loopback_endpoint"
    );
    Ok(value.trim_end_matches('/').to_owned())
}

fn unavailable() -> ApiError {
    ApiError::unavailable("identity_unavailable")
}

/// The identity provider names the violated rule; the person's input is never echoed.
async fn rejected(reply: reqwest::Response) -> ApiError {
    let body: Value = reply.json().await.unwrap_or_default();
    if body["error"]
        .as_str()
        .is_some_and(|code| code.starts_with("invalidPassword"))
    {
        ApiError::bad("password_policy")
    } else {
        ApiError::bad("account_rejected")
    }
}

fn configured(state: &State) -> ApiResult<&IdentityAdmin> {
    state
        .auth
        .accounts
        .as_ref()
        .ok_or_else(|| ApiError::unavailable("account_links_unavailable"))
}

fn valid_token(value: &str) -> bool {
    value.len() == 43
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn valid_username(value: &str) -> bool {
    let bytes = value.as_bytes();
    (3..=32).contains(&bytes.len())
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(b))
}

fn valid_email(value: &str) -> bool {
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    value.len() <= 254
        && !value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "<>\"'\\,;".contains(c))
        && !local.is_empty()
        && !domain.contains('@')
        && domain.contains('.')
        && domain
            .split('.')
            .all(|label| !label.is_empty() && label.len() <= 63)
}

fn valid_password(password: &str, username: Option<&str>) -> bool {
    (PASSWORD_MIN..=PASSWORD_MAX).contains(&password.chars().count())
        && !password.chars().any(char::is_control)
        && username.is_none_or(|name| !password.eq_ignore_ascii_case(name))
}

fn invalid() -> ApiError {
    // One answer for unknown, used, revoked, expired and locked links.
    ApiError {
        status: StatusCode::NOT_FOUND,
        code: "account_link_invalid".into(),
    }
}

/// A link works only while it is unused and unexpired, its issuer is still an
/// enabled administrator and, for a reset, its account is still enabled.
const USABLE: &str = "SELECT l.id,l.kind,l.name,l.can_develop,l.user_id,l.created_by,l.expires_at,u.name AS user_name,(SELECT email FROM signup_requests WHERE link_id=l.id) AS signup_email \
    FROM account_links l \
    JOIN users c ON c.id=l.created_by AND c.admin AND c.enabled \
    LEFT JOIN users u ON u.id=l.user_id \
    WHERE l.token_hash=$1 AND l.status='pending' AND l.expires_at>clock_timestamp() AND l.attempts<$2 \
    AND (l.kind<>'password_reset' OR u.enabled)";

/// Served only to the console's own page: no bearer or other site needs these paths.
fn console_page(state: &State, purpose: BrowserPurpose, headers: &HeaderMap) -> ApiResult<()> {
    if purpose != BrowserPurpose::Console
        || headers.contains_key("authorization")
        || !state.auth.allowed_origin(headers, false, purpose)
    {
        return Err(ApiError::forbidden());
    }
    Ok(())
}

pub fn public_router() -> Router<State> {
    Router::new()
        .route("/api/v1/signup-requests", post(request_signup))
        .route("/api/v1/account-links/inspect", post(inspect))
        .route("/api/v1/account-links/complete", post(complete))
        .layer(DefaultBodyLimit::max(8 * 1024))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Signup {
    name: String,
    email: String,
}

async fn request_signup(
    S(s): S<State>,
    Extension(purpose): Extension<BrowserPurpose>,
    headers: HeaderMap,
    Json(input): Json<Signup>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    console_page(&s, purpose, &headers)?;
    configured(&s)?;
    let name = crate::user_admin::display_name(&input.name)?;
    let email = input.email.trim();
    if !valid_email(email) {
        return Err(ApiError::bad("invalid_email"));
    }
    crate::auth_sessions::limits(&s.pool, &[("signup", "global", 30)]).await?;
    let mut tx = s.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(76432935)")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM signup_requests WHERE created_at < now()-interval '30 days' AND status <> 'approved'").execute(&mut *tx).await?;
    let pending: i64 =
        sqlx::query_scalar("SELECT count(*) FROM signup_requests WHERE status='pending'")
            .fetch_one(&mut *tx)
            .await?;
    if pending >= 1000 {
        return Err(ApiError::unavailable("signup_queue_full"));
    }
    sqlx::query(
        "INSERT INTO signup_requests(id,name,email) SELECT $1,$2,$3 WHERE NOT EXISTS (
        SELECT 1 FROM signup_requests s JOIN account_links l ON l.id=s.link_id
        WHERE lower(s.email)=lower($3) AND l.status='pending' AND l.expires_at>now()
    ) ON CONFLICT DO NOTHING",
    )
    .bind(Uuid::new_v4())
    .bind(name)
    .bind(email)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::ACCEPTED, Json(json!({"status":"received"}))))
}

pub async fn reject_signup(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    crate::domain::admin(&s, &p).await?;
    let mut tx = Write::begin(&s.pool).await?;
    let result = sqlx::query("UPDATE signup_requests SET status='rejected',decided_at=now(),decided_by=$2 WHERE id=$1 AND status='pending'")
        .bind(id).bind(&p.id).execute(&mut *tx).await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::conflict("signup_decided"));
    }
    let mut tx = tx.decide_platform(&p, PlatformAction::Administer).await?;
    crate::audit::record(&mut tx, &p.id, "signup.rejected", &id.to_string()).await?;
    tx.commit().await?;
    Ok(Json(json!({"status":"rejected"})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inspection {
    token: String,
}

async fn inspect(
    S(s): S<State>,
    Extension(purpose): Extension<BrowserPurpose>,
    headers: HeaderMap,
    Json(input): Json<Inspection>,
) -> ApiResult<Json<Value>> {
    console_page(&s, purpose, &headers)?;
    configured(&s)?;
    if !valid_token(&input.token) {
        return Err(invalid());
    }
    let row = sqlx::query(USABLE)
        .bind(hash(&input.token))
        .bind(MAX_ATTEMPTS)
        .fetch_optional(&s.pool)
        .await?
        .ok_or_else(invalid)?;
    let kind: String = row.get("kind");
    let name: Option<String> = if kind == "invitation" {
        row.get("name")
    } else {
        row.get("user_name")
    };
    Ok(Json(json!({
        "kind": kind, "name": name, "email": row.get::<Option<String>,_>("signup_email"),
        "expiresAt": row.get::<DateTime<Utc>, _>("expires_at"),
        "passwordMinLength": PASSWORD_MIN,
        "portalOrigin": s.auth.origin,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Completion {
    token: String,
    password: String,
    /// Invitation only: the new sign-in name.
    username: Option<String>,
    /// Invitation only: the display name shown in the platform.
    name: Option<String>,
    /// Invitation only: kept by the identity provider.
    email: Option<String>,
}

async fn complete(
    S(s): S<State>,
    Extension(purpose): Extension<BrowserPurpose>,
    headers: HeaderMap,
    Json(input): Json<Completion>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    console_page(&s, purpose, &headers)?;
    let identity = configured(&s)?;
    if !valid_token(&input.token) {
        return Err(invalid());
    }
    let token = hash(&input.token);
    let link = sqlx::query(USABLE)
        .bind(&token)
        .bind(MAX_ATTEMPTS)
        .fetch_optional(&s.pool)
        .await?
        .ok_or_else(invalid)?;
    let kind: &str = link.get("kind");
    // Input the form can check costs no attempt.
    let account = if kind == "invitation" {
        let username = input.username.as_deref().unwrap_or_default();
        let email = input.email.as_deref().unwrap_or_default().trim();
        if !valid_username(username) {
            return Err(ApiError::bad("invalid_username"));
        }
        if !valid_email(email)
            || link
                .get::<Option<String>, _>("signup_email")
                .is_some_and(|bound| bound != email)
        {
            return Err(ApiError::bad("invalid_email"));
        }
        let name = crate::user_admin::display_name(input.name.as_deref().unwrap_or_default())?;
        Some((username.to_owned(), email.to_owned(), name))
    } else {
        if input.username.is_some() || input.name.is_some() || input.email.is_some() {
            return Err(ApiError::bad("invalid_account_link_input"));
        }
        None
    };
    if !valid_password(&input.password, account.as_ref().map(|a| a.0.as_str())) {
        return Err(ApiError::bad("password_policy"));
    }
    // Counted outside the transaction below, so a failed attempt is not forgotten.
    sqlx::query("UPDATE account_links SET attempts=attempts+1 WHERE token_hash=$1 AND status='pending' AND expires_at>clock_timestamp() AND attempts<$2 RETURNING id")
        .bind(&token)
        .bind(MAX_ATTEMPTS)
        .fetch_optional(&s.pool)
        .await?
        .ok_or_else(invalid)?;
    let Some((username, email, name)) = account else {
        let subject: &str = link.get("user_id");
        let issuer: &str = link.get("created_by");
        let access = identity.token().await?;
        let id = claim_reset(&s.pool, &token, subject, issuer).await?;
        change_password(&s.pool, identity, id, subject, &access, &input.password).await?;
        let done = finish_reset(&s.pool, identity, id)
            .await
            .unwrap_or_else(|_| {
                tracing::warn!(event = "password_reset_finalize_failed", link = %id);
                false
            });
        let (status, result) = if done {
            (StatusCode::OK, "completed")
        } else {
            (StatusCode::ACCEPTED, "ending_sessions")
        };
        return Ok((
            status,
            Json(json!({"status":result,"kind":kind,"loginUrl":format!("{}/", s.auth.origin)})),
        ));
    };

    // The row lock makes one invitation create at most one account.
    let mut tx = s.pool.begin().await?;
    let row = sqlx::query(&format!("{USABLE} FOR UPDATE OF l FOR SHARE OF c"))
        .bind(&token)
        .bind(MAX_ATTEMPTS + 1)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(invalid)?;
    let id: Uuid = row.get("id");
    let can_develop: bool = row.get("can_develop");
    let subject = identity.create(&username, &email, &input.password).await?;
    if let Err(error) = register(tx, id, &subject, &name, can_develop).await {
        // Free the login name if the platform registration failed.
        if !identity.remove(&subject).await {
            tracing::error!(event = "unregistered_identity_account_left", subject = %subject);
        }
        return Err(error);
    }
    tracing::info!(event = "invitation_completed", link = %id);
    Ok((
        StatusCode::OK,
        Json(json!({
            "status": "completed", "kind": kind,
            "loginUrl": format!("{}/", s.auth.origin),
        })),
    ))
}

async fn claim_reset(pool: &PgPool, token: &str, user: &str, issuer: &str) -> ApiResult<Uuid> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(crate::user_admin::ACCOUNT_LOCK)
        .execute(&mut *tx)
        .await?;
    // Order account locks as for state changes. The target lock also serializes
    // the claim with creation of a browser session.
    sqlx::query("SELECT id FROM users WHERE id=ANY($1) ORDER BY id FOR NO KEY UPDATE")
        .bind(vec![user, issuer])
        .execute(&mut *tx)
        .await?;
    let row = sqlx::query(&format!("{USABLE} FOR UPDATE OF l"))
        .bind(token)
        .bind(MAX_ATTEMPTS + 1)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(invalid)?;
    let id: Uuid = row.get("id");
    if reset_pending(&mut tx, user).await? {
        return Err(ApiError::conflict("password_reset_in_progress"));
    }
    sqlx::query("INSERT INTO password_resets(user_id,link_id) VALUES($1,$2)")
        .bind(user)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE account_links SET status='revoked' WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(id)
}

async fn change_password(
    pool: &PgPool,
    identity: &IdentityAdmin,
    id: Uuid,
    user: &str,
    access: &str,
    password: &str,
) -> ApiResult<()> {
    let unknown = || ApiError::unavailable("password_reset_unknown");
    let mut tx = pool.begin().await.map_err(|_| unknown())?;
    // Hold this row during the only password request. If the caller disappears
    // before acquiring it, recovery can finish the reservation without a write.
    let claimed: Option<Uuid> = sqlx::query_scalar(
        "SELECT link_id FROM password_resets WHERE link_id=$1 AND retry_at>clock_timestamp() FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| unknown())?;
    if claimed.is_none() {
        return Err(unknown());
    }
    match identity.set_password(access, user, password).await {
        Ok(()) => {
            sqlx::query("UPDATE password_resets SET password_changed=true,retry_at=clock_timestamp() WHERE link_id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|_| unknown())?;
            tx.commit().await.map_err(|_| unknown())?;
            Ok(())
        }
        Err(PasswordChangeError::Uncertain) => {
            tracing::warn!(event = "password_reset_outcome_unknown", link = %id);
            Err(unknown())
        }
        Err(PasswordChangeError::Refused(error)) => {
            // A definitive provider rejection is safe to retry with corrected input.
            sqlx::query("UPDATE account_links SET status='pending' WHERE id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|_| unknown())?;
            sqlx::query("DELETE FROM password_resets WHERE link_id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|_| unknown())?;
            tx.commit().await.map_err(|_| unknown())?;
            Err(error)
        }
    }
}

async fn finish_reset(pool: &PgPool, identity: &IdentityAdmin, id: Uuid) -> ApiResult<bool> {
    let mut tx = pool.begin().await?;
    let Some((user, changed)) = sqlx::query_as::<_, (String, bool)>(
        "SELECT user_id,password_changed FROM password_resets WHERE link_id=$1 AND retry_at<=clock_timestamp() FOR UPDATE SKIP LOCKED",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(false);
    };
    if !identity.end_sessions(&user).await {
        sqlx::query("UPDATE password_resets SET retry_at=clock_timestamp()+interval '15 seconds' WHERE link_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        tracing::warn!(event = "password_reset_waiting_for_logout", link = %id);
        return Ok(false);
    }
    sqlx::query("UPDATE users SET browser_login_after=clock_timestamp() WHERE id=$1")
        .bind(&user)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id=$1")
        .bind(&user)
        .execute(&mut *tx)
        .await?;
    let action = if changed {
        sqlx::query("UPDATE account_links SET status='completed',completed_at=now() WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        "user.password_reset"
    } else {
        "user.password_reset_uncertain"
    };
    crate::audit::record(&mut tx, &user, action, &format!("user:{user}:link:{id}")).await?;
    sqlx::query("DELETE FROM password_resets WHERE link_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

pub(crate) async fn reconcile_resets(state: &State) -> ApiResult<()> {
    let Some(identity) = &state.auth.accounts else {
        return Ok(());
    };
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT link_id FROM password_resets WHERE retry_at<=clock_timestamp() ORDER BY retry_at,link_id LIMIT 16",
    )
    .fetch_all(&state.pool)
    .await?;
    for id in ids {
        if finish_reset(&state.pool, identity, id).await.is_err() {
            tracing::warn!(event = "password_reset_finalize_failed", link = %id);
        }
    }
    Ok(())
}

pub(crate) async fn reset_pending(conn: &mut sqlx::PgConnection, user: &str) -> ApiResult<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM password_resets WHERE user_id=$1)")
            .bind(user)
            .fetch_one(conn)
            .await?,
    )
}

pub(crate) struct ResetMonitor(tokio::task::JoinHandle<()>);

impl Drop for ResetMonitor {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(crate) fn start_reset_monitor(state: State) -> Option<ResetMonitor> {
    state.auth.accounts.as_ref()?;
    Some(ResetMonitor(tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if reconcile_resets(&state).await.is_err() {
                tracing::warn!(event = "password_reset_reconcile_failed");
            }
        }
    })))
}

async fn register(
    mut tx: sqlx::Transaction<'static, sqlx::Postgres>,
    link: Uuid,
    subject: &str,
    name: &str,
    can_develop: bool,
) -> ApiResult<()> {
    sqlx::query("INSERT INTO users(id,name,admin,can_develop,enabled) VALUES($1,$2,false,$3,true)")
        .bind(subject)
        .bind(name)
        .bind(can_develop)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE account_links SET status='completed',user_id=$2,completed_at=now() WHERE id=$1",
    )
    .bind(link)
    .bind(subject)
    .execute(&mut *tx)
    .await?;
    crate::audit::record(
        &mut tx,
        subject,
        "user.registered",
        &format!("user:{subject}:developer:{can_develop}:invitation:{link}"),
    )
    .await?;
    Ok(tx.commit().await?)
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewLink {
    /// `invitation` or `password_reset`.
    pub kind: String,
    /// Invitation: the invited person's display name, 1-120 characters.
    pub name: Option<String>,
    /// Invitation: the new account may request app registrations.
    pub can_develop: Option<bool>,
    /// Password reset: the registered account's ID.
    pub user_id: Option<String>,
    /// Invitation 1-720 (default 168), password reset 1-72 (default 24).
    pub expires_in_hours: Option<i32>,
    /// Issue the invitation for this pending signup. Reissuing requires its current link ID.
    pub signup_request_id: Option<Uuid>,
    pub previous_link_id: Option<Uuid>,
}

pub async fn create(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Json(input): Json<NewLink>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    crate::domain::admin(&s, &p).await?;
    configured(&s)?;
    let bad = || ApiError::bad("invalid_account_link");
    if (input.signup_request_id.is_some()
        && (input.kind != "invitation" || input.can_develop == Some(true)))
        || (input.previous_link_id.is_some() && input.signup_request_id.is_none())
    {
        return Err(bad());
    }
    let (mut name, can_develop, user, hours, page) = match input.kind.as_str() {
        "invitation" if input.user_id.is_none() => (
            Some(crate::user_admin::display_name(
                input.name.as_deref().unwrap_or_default(),
            )?),
            input.can_develop.unwrap_or(false),
            None,
            input.expires_in_hours.unwrap_or(168),
            "join",
        ),
        "password_reset" if input.name.is_none() && input.can_develop.is_none() => (
            None,
            false,
            Some(crate::user_admin::subject(
                input.user_id.as_deref().unwrap_or_default(),
            )?),
            input.expires_in_hours.unwrap_or(24),
            "reset-password",
        ),
        _ => return Err(bad()),
    };
    if !(1..=if user.is_some() { 72 } else { 720 }).contains(&hours) {
        return Err(bad());
    }
    let token = crate::identity::random();
    let id = Uuid::new_v4();
    let mut tx = Write::begin(&s.pool).await?;
    // One account change is decided at a time, as registration and state changes are.
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(crate::user_admin::ACCOUNT_LOCK)
        .execute(&mut *tx)
        .await?;
    if let Some(signup) = input.signup_request_id {
        let entry = sqlx::query("SELECT s.name,s.status,s.link_id,l.status AS link_status FROM signup_requests s LEFT JOIN account_links l ON l.id=s.link_id WHERE s.id=$1 FOR UPDATE OF s")
            .bind(signup).fetch_optional(&mut *tx).await?.ok_or_else(ApiError::not_found)?;
        let status: String = entry.get("status");
        let old: Option<Uuid> = entry.get("link_id");
        if old != input.previous_link_id
            || !(status == "pending"
                || (status == "approved"
                    && entry.get::<Option<String>, _>("link_status").as_deref()
                        != Some("completed")))
        {
            return Err(ApiError::conflict("signup_decided"));
        }
        if let Some(old) = old {
            let updated = sqlx::query(
                "UPDATE account_links SET status='revoked' WHERE id=$1 AND status<>'completed'",
            )
            .bind(old)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() == 0 {
                return Err(ApiError::conflict("signup_decided"));
            }
        }
        name = Some(entry.get("name"));
    }
    if let Some(user) = &user {
        let enabled: Option<bool> =
            sqlx::query_scalar("SELECT enabled FROM users WHERE id=$1 FOR SHARE")
                .bind(user)
                .fetch_optional(&mut *tx)
                .await?;
        match enabled {
            None => return Err(ApiError::not_found()),
            Some(false) => return Err(ApiError::conflict("user_disabled")),
            Some(true) => {}
        }
        if reset_pending(&mut tx, user).await? {
            return Err(ApiError::conflict("password_reset_in_progress"));
        }
        // One working reset link per account: a new one replaces the previous.
        sqlx::query("UPDATE account_links SET status='revoked' WHERE user_id=$1 AND kind='password_reset' AND status='pending'")
            .bind(user)
            .execute(&mut *tx)
            .await?;
    }
    let pending: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM account_links WHERE status='pending' AND expires_at>now()) + \
                (SELECT count(*) FROM password_resets r JOIN account_links l ON l.id=r.link_id WHERE l.expires_at>now())",
    )
    .fetch_one(&mut *tx)
    .await?;
    if pending >= MAX_PENDING {
        return Err(ApiError::conflict("account_link_limit"));
    }
    let expires: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO account_links(id,token_hash,kind,name,can_develop,user_id,created_by,expires_at) \
         VALUES($1,$2,$3,$4,$5,$6,$7,now()+make_interval(hours => $8)) RETURNING expires_at",
    )
    .bind(id)
    .bind(hash(&token))
    .bind(&input.kind)
    .bind(&name)
    .bind(can_develop)
    .bind(&user)
    .bind(&p.id)
    .bind(hours)
    .fetch_one(&mut *tx)
    .await?;
    if let Some(signup) = input.signup_request_id {
        sqlx::query("UPDATE signup_requests SET status='approved',decided_at=now(),decided_by=$2,link_id=$3 WHERE id=$1")
            .bind(signup).bind(&p.id).bind(id).execute(&mut *tx).await?;
    }
    // Decided after the insert: a link whose administrator was revoked meanwhile
    // rolls back together with its audit row.
    let mut tx = tx.decide_platform(&p, PlatformAction::Administer).await?;
    if let Some(signup) = input.signup_request_id {
        crate::audit::record(&mut tx, &p.id, "signup.approved", &signup.to_string()).await?;
    }
    let target = match &user {
        Some(user) => format!("password_reset:{id}:user:{user}"),
        None => format!("invitation:{id}:developer:{can_develop}"),
    };
    crate::audit::record(&mut tx, &p.id, "account_link.created", &target).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": id, "kind": input.kind, "name": name, "canDevelop": can_develop,
            "userId": user, "expiresAt": expires,
            // Shown once: only the hash of this secret is stored.
            "url": format!("{}/{page}#{token}", s.auth.console_origin),
        })),
    ))
}

pub async fn list(S(s): S<State>, Extension(p): Extension<Principal>) -> ApiResult<Json<Value>> {
    crate::domain::admin(&s, &p).await?;
    let rows = sqlx::query(
        "WITH actionable AS ( \
           SELECT id FROM account_links WHERE status='pending' AND expires_at>now() \
           UNION ALL SELECT link_id FROM password_resets \
         ), recent AS ( \
           SELECT l.id FROM account_links l WHERE NOT EXISTS (SELECT 1 FROM actionable a WHERE a.id=l.id) \
           ORDER BY l.created_at DESC,l.id LIMIT 10 \
         ) \
         SELECT l.id,l.kind,l.can_develop,l.user_id,l.created_by,l.created_at,l.expires_at,l.completed_at, \
         CASE WHEN l.kind='invitation' AND l.user_id IS NULL THEN l.name ELSE u.name END AS name, \
         c.name AS created_by_name, \
         CASE WHEN r.link_id IS NOT NULL THEN 'resetting' \
              WHEN l.status<>'pending' THEN l.status \
              WHEN l.expires_at<=now() THEN 'expired' \
              WHEN l.attempts>=$1 OR NOT (c.admin AND c.enabled) \
                   OR (l.kind='password_reset' AND NOT u.enabled) THEN 'inactive' \
              ELSE 'pending' END AS state \
         FROM (SELECT id FROM actionable UNION ALL SELECT id FROM recent) shown \
         JOIN account_links l ON l.id=shown.id JOIN users c ON c.id=l.created_by \
         LEFT JOIN users u ON u.id=l.user_id LEFT JOIN password_resets r ON r.link_id=l.id \
         ORDER BY l.created_at DESC,l.id",
    )
    .bind(MAX_ATTEMPTS)
    .fetch_all(&s.pool)
    .await?;
    let links: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<Uuid, _>("id"), "kind": row.get::<String, _>("kind"),
                "name": row.get::<Option<String>, _>("name"),
                "canDevelop": row.get::<bool, _>("can_develop"),
                "userId": row.get::<Option<String>, _>("user_id"),
                "state": row.get::<String, _>("state"),
                "createdBy": row.get::<String, _>("created_by"),
                "createdByName": row.get::<String, _>("created_by_name"),
                "createdAt": row.get::<DateTime<Utc>, _>("created_at"),
                "expiresAt": row.get::<DateTime<Utc>, _>("expires_at"),
                "completedAt": row.get::<Option<DateTime<Utc>>, _>("completed_at"),
            })
        })
        .collect();
    let requests = sqlx::query("SELECT s.id,s.name,s.email,s.created_at,s.link_id FROM signup_requests s LEFT JOIN account_links l ON l.id=s.link_id
        WHERE s.status='pending' OR (s.status='approved' AND l.status<>'completed' AND s.decided_at > now()-interval '30 days')
        ORDER BY (s.status='pending') DESC,s.created_at")
        .fetch_all(&s.pool).await?.into_iter().map(|r| json!({
            "id":r.get::<Uuid,_>("id"), "name":r.get::<String,_>("name"), "email":r.get::<String,_>("email"),
            "createdAt":r.get::<DateTime<Utc>,_>("created_at"), "linkId":r.get::<Option<Uuid>,_>("link_id")
        })).collect::<Vec<_>>();
    Ok(Json(json!({
        "signupRequests": requests,
        "available": s.auth.accounts.is_some(),
        "links": links,
    })))
}

pub async fn revoke(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    crate::domain::admin(&s, &p).await?;
    let mut tx = Write::begin(&s.pool).await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(crate::user_admin::ACCOUNT_LOCK)
        .execute(&mut *tx)
        .await?;
    let kind: Option<String> = sqlx::query_scalar(
        "UPDATE account_links SET status='revoked' WHERE id=$1 AND status='pending' RETURNING kind",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(kind) = kind else {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM account_links WHERE id=$1)")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        return Err(if exists {
            ApiError::conflict("account_link_not_pending")
        } else {
            ApiError::not_found()
        });
    };
    let mut tx = tx.decide_platform(&p, PlatformAction::Administer).await?;
    crate::audit::record(
        &mut tx,
        &p.id,
        "account_link.revoked",
        &format!("{kind}:{id}"),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"id": id, "state": "revoked"})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_in_names_emails_and_passwords_are_bounded_before_any_identity_request() {
        for name in ["hong", "kim.cs", "a-b_c", "9lives", &"a".repeat(32)] {
            assert!(valid_username(name), "{name}");
        }
        for name in [
            "",
            "ab",
            "Hong",
            "홍길동",
            ".hong",
            "-hong",
            "hong gil",
            "hong@corp",
            &"a".repeat(33),
        ] {
            assert!(!valid_username(name), "{name}");
        }
        for email in ["hong@example.com", "a.b+c@sub.example.co.kr"] {
            assert!(valid_email(email), "{email}");
        }
        for email in [
            "",
            "hong",
            "hong@",
            "@example.com",
            "hong@example",
            "hong@@example.com",
            "hong@example..com",
            "hong @example.com",
            "hong@example.com\n",
            "<hong@example.com>",
            &format!("{}@example.com", "a".repeat(250)),
        ] {
            assert!(!valid_email(email), "{email}");
        }
        assert!(valid_password("correct-horse-9", Some("hong")));
        assert!(valid_password(&"가".repeat(PASSWORD_MIN), None));
        assert!(!valid_password(&"a".repeat(PASSWORD_MIN - 1), None));
        assert!(!valid_password(&"a".repeat(PASSWORD_MAX + 1), None));
        assert!(!valid_password("line\nbreak-pass", None));
        assert!(!valid_password("HongGilDong", Some("honggildong")));
    }

    #[test]
    fn link_secrets_are_fixed_length_url_safe_values() {
        let token = crate::identity::random();
        assert!(valid_token(&token));
        assert_eq!(hash(&token).len(), 64);
        for value in [
            "",
            "short",
            &"a".repeat(42),
            &"a".repeat(44),
            &format!("{}/", "a".repeat(42)),
        ] {
            assert!(!valid_token(value), "{value}");
        }
    }

    #[test]
    fn identity_administration_is_loopback_only() {
        assert_eq!(
            admin_base("http://127.0.0.1:18181/identity/").unwrap(),
            "http://127.0.0.1:18181/identity"
        );
        assert!(admin_base("http://[::1]:18181/identity").is_ok());
        assert!(admin_base("https://localhost:18181/identity").is_ok());
        for value in [
            "https://console.example.com/identity",
            "http://10.0.0.5:18181/identity",
            "http://user:secret@127.0.0.1:18181/identity",
            "http://127.0.0.1:18181/identity?next=1",
            "http://127.0.0.1:18181/identity#top",
            "ftp://127.0.0.1/identity",
            "identity",
        ] {
            assert!(admin_base(value).is_err(), "{value}");
        }
    }
}
