//! Real PostgreSQL + HTTP checks of account links, called only by the isolated audit
//! fixture. Identity administration is a recording stand-in on loopback: this proves
//! the platform's own decisions and its recovery, not the identity provider's
//! behaviour, which the disposable full stack exercises with a real Keycloak.
//! Synthetic link secrets and passwords never appear in returned evidence or errors.
use crate::{State, auth, contract::hash};
use anyhow::{Context, Result, ensure};
use axum::{
    Json, Router,
    extract::{Path, State as S},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, post, put},
};
use reqwest::{Client, Method, RequestBuilder};
use serde_json::{Value, json};
use sqlx::Row;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

pub const SECRET: &str = "synthetic-accounts-secret";
const BEARER: &str = "Bearer synthetic-accounts-token";
/// A password the stand-in refuses the way a stricter realm policy would.
const REFUSED_BY_PROVIDER: &str = "refused-by-provider-policy";

struct Account {
    username: String,
    email: String,
    password: String,
}

#[derive(Default)]
struct Recorded {
    accounts: HashMap<String, Account>,
    removed: Vec<String>,
    ended: Vec<String>,
    /// The next created account receives this subject instead of a new one.
    next_subject: Option<String>,
    unavailable: bool,
}

#[derive(Clone, Default)]
pub struct Identity(Arc<Mutex<Recorded>>);

impl Identity {
    fn read<T>(&self, read: impl FnOnce(&mut Recorded) -> T) -> T {
        read(&mut self.0.lock().unwrap())
    }
}

/// The identity administration requests the core sends, on the fixture issuer.
pub fn fixture() -> (Router, Identity) {
    let identity = Identity::default();
    let router = Router::new()
        .route("/realms/miy/protocol/openid-connect/token", post(token))
        .route("/admin/realms/miy/users", post(create))
        .route(
            "/admin/realms/miy/users/{id}/reset-password",
            put(set_password),
        )
        .route("/admin/realms/miy/users/{id}/logout", post(end_sessions))
        .route("/admin/realms/miy/users/{id}", delete(remove))
        .with_state(identity.clone());
    (router, identity)
}

async fn token(S(identity): S<Identity>, body: String) -> Response {
    let form: HashMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect();
    let field = |name: &str| form.get(name).map(String::as_str);
    if identity.read(|state| state.unavailable) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if field("grant_type") != Some("client_credentials")
        || field("client_id") != Some("miy-accounts")
        || field("client_secret") != Some(SECRET)
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({"access_token": "synthetic-accounts-token", "expires_in": 300})).into_response()
}

fn authorized(headers: &HeaderMap) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .is_some_and(|value| value == BEARER)
}

fn policy(password: &str) -> Option<Response> {
    (password == REFUSED_BY_PROVIDER).then(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "invalidPasswordHistoryMessage"})),
        )
            .into_response()
    })
}

async fn create(S(identity): S<Identity>, headers: HeaderMap, Json(body): Json<Value>) -> Response {
    if !authorized(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let text = |value: &Value| value.as_str().unwrap_or_default().to_owned();
    let (username, email) = (text(&body["username"]), text(&body["email"]));
    let password = text(&body["credentials"][0]["value"]);
    if body["enabled"] != true
        || body["credentials"][0]["temporary"] != false
        || body.get("firstName").is_some()
        || body.get("lastName").is_some()
    {
        return StatusCode::UNPROCESSABLE_ENTITY.into_response();
    }
    if let Some(refused) = policy(&password) {
        return refused;
    }
    identity.read(|state| {
        if state
            .accounts
            .values()
            .any(|account| account.username == username || account.email == email)
        {
            return (
                StatusCode::CONFLICT,
                Json(json!({"errorMessage": "User exists with same username"})),
            )
                .into_response();
        }
        let id = state
            .next_subject
            .take()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        state.accounts.insert(
            id.clone(),
            Account {
                username,
                email,
                password,
            },
        );
        (
            StatusCode::CREATED,
            [(
                header::LOCATION,
                format!("http://identity.invalid/admin/realms/miy/users/{id}"),
            )],
        )
            .into_response()
    })
}

async fn set_password(
    S(identity): S<Identity>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if !authorized(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let password = body["value"].as_str().unwrap_or_default().to_owned();
    if let Some(refused) = policy(&password) {
        return refused;
    }
    identity.read(|state| match state.accounts.get_mut(&id) {
        Some(account) => {
            account.password = password;
            StatusCode::NO_CONTENT.into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "User not found"})),
        )
            .into_response(),
    })
}

async fn end_sessions(
    S(identity): S<Identity>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> StatusCode {
    if !authorized(&headers) {
        return StatusCode::FORBIDDEN;
    }
    identity.read(|state| state.ended.push(id));
    StatusCode::NO_CONTENT
}

async fn remove(
    S(identity): S<Identity>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> StatusCode {
    if !authorized(&headers) {
        return StatusCode::FORBIDDEN;
    }
    identity.read(|state| {
        state.accounts.remove(&id);
        state.removed.push(id);
    });
    StatusCode::NO_CONTENT
}

struct Harness<'a> {
    state: &'a State,
    client: &'a Client,
    console: &'a str,
    portal: &'a str,
}

struct Admin {
    cookie: String,
    csrf: String,
}

struct Link {
    id: Uuid,
    token: String,
}

impl Harness<'_> {
    /// A console development session, as the other suites use for browser calls.
    async fn session(&self, user: &str) -> Result<Admin> {
        let (cookie, csrf) = (auth::random(), auth::random());
        sqlx::query("INSERT INTO development_sessions(id_hash,user_id,csrf,expires_at,browser_purpose) VALUES($1,$2,$3,now()+interval '8 hours','console')")
            .bind(hash(&cookie)).bind(user).bind(&csrf).execute(&self.state.pool).await?;
        Ok(Admin { cookie, csrf })
    }

    fn signed(&self, method: Method, path: &str, admin: &Admin) -> RequestBuilder {
        self.client
            .request(method, format!("{}{path}", self.console))
            .header(
                "cookie",
                format!(
                    "{}={}",
                    self.state
                        .auth
                        .cookie_name(auth::BrowserPurpose::Console, "development"),
                    admin.cookie
                ),
            )
            .header("origin", self.console)
            .header("x-miy-csrf", &admin.csrf)
    }

    /// The console's own page: its Origin and no credential.
    fn page(&self, path: &str, body: Value) -> RequestBuilder {
        self.client
            .post(format!("{}{path}", self.console))
            .header("origin", self.console)
            .json(&body)
    }

    async fn issue(&self, admin: &Admin, body: Value) -> Result<Link> {
        let reply = self
            .signed(Method::POST, "/api/v1/account-links", admin)
            .json(&body)
            .send()
            .await?;
        ensure!(reply.status() == 201, "link was not issued");
        let value: Value = reply.json().await?;
        let url = value["url"]
            .as_str()
            .context("issued link has no address")?;
        let page = if body["kind"] == "invitation" {
            "join"
        } else {
            "reset-password"
        };
        let token = url
            .strip_prefix(&format!("{}/{page}#", self.console))
            .context("link is not a console page with the secret in its fragment")?;
        ensure!(token.len() == 43, "unexpected link secret length");
        let id: Uuid = value["id"].as_str().context("link id")?.parse()?;
        // Only the hash is stored, and the listing never returns the secret.
        let stored: String = sqlx::query_scalar("SELECT token_hash FROM account_links WHERE id=$1")
            .bind(id)
            .fetch_one(&self.state.pool)
            .await?;
        ensure!(
            stored == hash(token),
            "stored value is not the secret's hash"
        );
        Ok(Link {
            id,
            token: token.to_owned(),
        })
    }

    async fn inspect(&self, link: &Link) -> Result<(u16, Value)> {
        let reply = self
            .page(
                "/api/v1/account-links/inspect",
                json!({"token": link.token}),
            )
            .send()
            .await?;
        Ok((reply.status().as_u16(), reply.json().await?))
    }

    async fn complete(&self, link: &Link, mut body: Value) -> Result<(u16, Value)> {
        body["token"] = json!(link.token);
        let reply = self
            .page("/api/v1/account-links/complete", body)
            .send()
            .await?;
        Ok((reply.status().as_u16(), reply.json().await?))
    }

    async fn refused(&self, link: &Link) -> Result<bool> {
        let (status, value) = self.inspect(link).await?;
        let (used, used_value) = self
            .complete(link, json!({"password": "another-valid-password-1"}))
            .await?;
        Ok(status == 404
            && value["error"]["code"] == "account_link_invalid"
            && used == 404
            && used_value["error"]["code"] == "account_link_invalid")
    }

    async fn scalar(&self, query: &str, link: Uuid) -> Result<String> {
        Ok(sqlx::query_scalar(query)
            .bind(link)
            .fetch_one(&self.state.pool)
            .await?)
    }

    async fn status(&self, link: &Link) -> Result<String> {
        self.scalar("SELECT status FROM account_links WHERE id=$1", link.id)
            .await
    }

    async fn listed(&self, admin: &Admin, link: &Link) -> Result<String> {
        let value: Value = self
            .signed(Method::GET, "/api/v1/account-links", admin)
            .send()
            .await?
            .json()
            .await?;
        ensure!(
            !value.to_string().contains(&link.token),
            "the listing exposed a link secret"
        );
        value["links"]
            .as_array()
            .and_then(|links| links.iter().find(|row| row["id"] == link.id.to_string()))
            .and_then(|row| row["state"].as_str())
            .map(str::to_owned)
            .context("link is not listed")
    }

    async fn users(&self) -> Result<i64> {
        Ok(sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(&self.state.pool)
            .await?)
    }
}

fn person(username: &str, password: &str) -> Value {
    json!({
        "username": username, "name": "초대 사용자",
        "email": format!("{username}@accounts.invalid"), "password": password,
    })
}

pub async fn run(
    state: &State,
    client: &Client,
    console: &str,
    portal: &str,
    identity: &Identity,
) -> Result<Vec<String>> {
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&state.pool)
        .await?;
    ensure!(
        database == "miy_audit_core",
        "isolated accounts regression database required"
    );
    ensure!(
        state.auth.accounts.is_some() && state.auth.development_login,
        "accounts regression requires configured identity administration and the local profile"
    );
    let h = Harness {
        state,
        client,
        console: console.trim_end_matches('/'),
        portal: portal.trim_end_matches('/'),
    };
    let mut passed = vec![];
    let admin = h.session("audit-admin").await?;
    let ordinary = h.session("audit-user").await?;
    let invitation = json!({"kind": "invitation", "name": "초대 사용자", "canDevelop": true, "expiresInHours": 24});

    // 1. Who may issue, list and use links.
    let anonymous = client
        .get(format!("{}/api/v1/account-links", h.console))
        .send()
        .await?;
    ensure!(anonymous.status() == 401, "anonymous listing was served");
    for (method, path) in [
        (Method::GET, "/api/v1/account-links".to_owned()),
        (Method::POST, "/api/v1/account-links".to_owned()),
        (
            Method::POST,
            format!("/api/v1/account-links/{}/revoke", Uuid::new_v4()),
        ),
    ] {
        let reply = h
            .signed(method, &path, &ordinary)
            .json(&invitation)
            .send()
            .await?;
        ensure!(reply.status() == 403, "an ordinary user managed links");
    }
    let probe = Link {
        id: Uuid::nil(),
        token: auth::random(),
    };
    let body = json!({"token": probe.token});
    for request in [
        // The portals never serve these paths, with or without a bearer.
        client
            .post(format!("{}/api/v1/account-links/inspect", h.portal))
            .header("origin", h.portal)
            .json(&body),
        client
            .post(format!("{}/api/v1/account-links/complete", h.portal))
            .json(&body),
        // On the console only its own page may call: no missing or foreign Origin, no bearer.
        client
            .post(format!("{}/api/v1/account-links/inspect", h.console))
            .json(&body),
        client
            .post(format!("{}/api/v1/account-links/inspect", h.console))
            .header("origin", h.portal)
            .json(&body),
        h.page("/api/v1/account-links/inspect", body.clone())
            .header("authorization", "Bearer miy_access_synthetic"),
    ] {
        ensure!(
            request.send().await?.status() == 403,
            "an account-link page call was accepted outside the console page"
        );
    }
    ensure!(
        h.refused(&probe).await?,
        "an unknown link was not refused uniformly"
    );
    passed.push("account links are issued by console administrators only and used only from the console page".into());

    // 2. Invitation: input the form can check costs no attempt; the provider's own
    // policy refusal leaves nothing behind; one success registers exactly one person.
    let users_before = h.users().await?;
    let first = h.issue(&admin, invitation.clone()).await?;
    let (status, value) = h.inspect(&first).await?;
    ensure!(
        status == 200
            && value["kind"] == "invitation"
            && value["name"] == "초대 사용자"
            && value["passwordMinLength"] == 10
            && value["portalOrigin"] == h.portal,
        "invitation was not described"
    );
    for (body, code) in [
        (
            person("Invited.User", "correct-horse-battery-1"),
            "invalid_username",
        ),
        (person("invited.user", "short"), "password_policy"),
        (person("invited.user", "invited.user"), "password_policy"),
        (
            json!({"username": "invited.user", "name": "초대 사용자", "email": "not-an-address", "password": "correct-horse-battery-1"}),
            "invalid_email",
        ),
        (
            json!({"username": "invited.user", "name": " ", "email": "invited.user@accounts.invalid", "password": "correct-horse-battery-1"}),
            "invalid_user_name",
        ),
    ] {
        let (status, value) = h.complete(&first, body).await?;
        ensure!(
            status == 400 && value["error"]["code"] == code,
            "expected {code}"
        );
    }
    let attempts: i32 = sqlx::query_scalar("SELECT attempts FROM account_links WHERE id=$1")
        .bind(first.id)
        .fetch_one(&state.pool)
        .await?;
    ensure!(attempts == 0, "checkable input consumed attempts");
    let (status, value) = h
        .complete(&first, person("invited.user", REFUSED_BY_PROVIDER))
        .await?;
    ensure!(
        status == 400 && value["error"]["code"] == "password_policy",
        "the provider's policy refusal was not reported"
    );
    ensure!(
        identity.read(|state| state.accounts.is_empty())
            && h.users().await? == users_before
            && h.status(&first).await? == "pending",
        "a refused password left an account, a registration or a used link"
    );
    let (status, value) = h
        .complete(&first, person("invited.user", "correct-horse-battery-1"))
        .await?;
    ensure!(
        status == 200
            && value["status"] == "completed"
            && value["kind"] == "invitation"
            && value["loginUrl"] == format!("{}/", h.portal),
        "invitation was not completed"
    );
    let subject = identity.read(|state| {
        state
            .accounts
            .iter()
            .find(|(_, account)| {
                account.username == "invited.user"
                    && account.email == "invited.user@accounts.invalid"
                    && account.password == "correct-horse-battery-1"
            })
            .map(|(id, _)| id.clone())
    });
    let subject = subject.context("the sign-in account was not created as entered")?;
    let row = sqlx::query("SELECT name,admin,can_develop,enabled FROM users WHERE id=$1")
        .bind(&subject)
        .fetch_one(&state.pool)
        .await?;
    ensure!(
        row.get::<String, _>("name") == "초대 사용자"
            && !row.get::<bool, _>("admin")
            && row.get::<bool, _>("can_develop")
            && row.get::<bool, _>("enabled"),
        "the invited person was not registered as the invitation said"
    );
    let granted: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM grants WHERE user_id=$1)+(SELECT count(*) FROM group_members WHERE user_id=$1)")
        .bind(&subject).fetch_one(&state.pool).await?;
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM audit WHERE (actor='audit-admin' AND action='account_link.created' AND target=$1) OR (actor=$2 AND action='user.registered' AND target=$3)")
        .bind(format!("invitation:{}:developer:true", first.id))
        .bind(&subject)
        .bind(format!("user:{subject}:developer:true:invitation:{}", first.id))
        .fetch_one(&state.pool).await?;
    ensure!(
        granted == 0
            && audited == 2
            && h.users().await? == users_before + 1
            && h.scalar("SELECT user_id FROM account_links WHERE id=$1", first.id)
                .await?
                == subject
            && h.listed(&admin, &first).await? == "completed",
        "the invitation granted a permission or left no exact record"
    );
    ensure!(
        h.refused(&first).await? && identity.read(|state| state.accounts.len()) == 1,
        "a used invitation worked again"
    );
    passed.push("an invitation creates one sign-in account, registers it without any permission and works once".into());

    // 3. A taken sign-in name or address: nothing registered, the link stays usable.
    let second = h.issue(&admin, invitation.clone()).await?;
    let (status, value) = h
        .complete(&second, person("invited.user", "correct-horse-battery-2"))
        .await?;
    ensure!(
        status == 409 && value["error"]["code"] == "account_exists",
        "a taken sign-in name was not reported"
    );
    ensure!(
        h.users().await? == users_before + 1 && h.status(&second).await? == "pending",
        "a taken sign-in name registered a person or consumed the link"
    );
    let (status, _) = h
        .complete(&second, person("second.user", "correct-horse-battery-2"))
        .await?;
    ensure!(
        status == 200 && h.users().await? == users_before + 2,
        "the link did not work after choosing a free name"
    );
    passed.push("a taken sign-in name registers nobody and leaves the invitation usable".into());

    // 4. Links stop working when revoked, expired, exhausted or issued by someone who
    // is no longer an enabled administrator; the last is decided at use, not at issue.
    let revoked = h.issue(&admin, invitation.clone()).await?;
    let path = format!("/api/v1/account-links/{}/revoke", revoked.id);
    ensure!(
        h.signed(Method::POST, &path, &admin).send().await?.status() == 200
            && h.refused(&revoked).await?
            && h.listed(&admin, &revoked).await? == "revoked",
        "a revoked link still worked"
    );
    let again: Value = h
        .signed(Method::POST, &path, &admin)
        .send()
        .await?
        .json()
        .await?;
    let unknown = h
        .signed(
            Method::POST,
            &format!("/api/v1/account-links/{}/revoke", Uuid::new_v4()),
            &admin,
        )
        .send()
        .await?;
    ensure!(
        again["error"]["code"] == "account_link_not_pending" && unknown.status() == 404,
        "revoking twice or an unknown link was not refused"
    );
    let expired = h.issue(&admin, invitation.clone()).await?;
    sqlx::query("UPDATE account_links SET expires_at=now()-interval '1 second' WHERE id=$1")
        .bind(expired.id)
        .execute(&state.pool)
        .await?;
    ensure!(
        h.refused(&expired).await? && h.listed(&admin, &expired).await? == "expired",
        "an expired link still worked"
    );
    let exhausted = h.issue(&admin, invitation.clone()).await?;
    sqlx::query("UPDATE account_links SET attempts=19 WHERE id=$1")
        .bind(exhausted.id)
        .execute(&state.pool)
        .await?;
    // The twentieth attempt is still made, fails at the provider and is the last.
    let (status, _) = h
        .complete(&exhausted, person("exhausted.user", REFUSED_BY_PROVIDER))
        .await?;
    ensure!(
        status == 400
            && h.refused(&exhausted).await?
            && h.listed(&admin, &exhausted).await? == "inactive",
        "a link outlived its attempts"
    );
    let former = format!("accounts-audit-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO users(id,name,admin) VALUES($1,'Former administrator',true)")
        .bind(&former)
        .execute(&state.pool)
        .await?;
    let former_session = h.session(&former).await?;
    let orphaned = h.issue(&former_session, invitation.clone()).await?;
    for (change, works) in [
        ("UPDATE users SET admin=false WHERE id=$1", false),
        ("UPDATE users SET admin=true WHERE id=$1", true),
        ("UPDATE users SET enabled=false WHERE id=$1", false),
    ] {
        sqlx::query(change)
            .bind(&former)
            .execute(&state.pool)
            .await?;
        let (status, _) = h.inspect(&orphaned).await?;
        ensure!(
            (status == 200) == works && (h.listed(&admin, &orphaned).await? == "pending") == works,
            "a link did not follow its issuer's current authority"
        );
    }
    ensure!(
        h.refused(&orphaned).await? && h.status(&orphaned).await? == "pending",
        "a former administrator's link registered a person"
    );
    passed.push(
        "revoked, expired and exhausted links and links of a former administrator stop working"
            .into(),
    );

    // 5. A sign-in account whose registration cannot commit is removed again.
    // (The previous check added the former administrator's own row.)
    let registered = h.users().await?;
    let colliding = h.issue(&admin, invitation.clone()).await?;
    identity.read(|state| state.next_subject = Some(subject.clone()));
    let (status, value) = h
        .complete(
            &colliding,
            person("colliding.user", "correct-horse-battery-3"),
        )
        .await?;
    ensure!(
        status == 409 && value["error"]["code"] == "already_exists",
        "a registration that cannot commit was reported as done"
    );
    ensure!(
        identity.read(|state| state.removed == vec![subject.clone()])
            && h.status(&colliding).await? == "pending"
            && h.users().await? == registered,
        "an unregistered sign-in account was left behind or the link was consumed"
    );
    // The stand-in keyed both accounts by one subject; restore the first for the reset checks.
    identity.read(|state| {
        state.accounts.insert(
            subject.clone(),
            Account {
                username: "invited.user".into(),
                email: "invited.user@accounts.invalid".into(),
                password: "correct-horse-battery-1".into(),
            },
        );
    });
    passed.push("a registration that cannot commit removes the sign-in account it created".into());

    // 6. Identity administration outage: nothing changes and the link survives.
    let waiting = h.issue(&admin, invitation.clone()).await?;
    identity.read(|state| state.unavailable = true);
    let (status, value) = h
        .complete(&waiting, person("waiting.user", "correct-horse-battery-4"))
        .await?;
    identity.read(|state| state.unavailable = false);
    ensure!(
        status == 503
            && value["error"]["code"] == "identity_unavailable"
            && h.status(&waiting).await? == "pending"
            && h.users().await? == registered,
        "an identity outage was not reported as unavailable without changes"
    );
    let (status, _) = h
        .complete(&waiting, person("waiting.user", "correct-horse-battery-4"))
        .await?;
    ensure!(status == 200, "the link did not work after the outage");
    passed.push("an identity administration outage changes nothing and keeps the link".into());

    // 7. Password reset: one working link per account, one use, browser sessions end.
    let reset = json!({"kind": "password_reset", "userId": subject});
    let replaced = h.issue(&admin, reset.clone()).await?;
    let current = h.issue(&admin, reset.clone()).await?;
    ensure!(
        h.refused(&replaced).await? && h.listed(&admin, &replaced).await? == "revoked",
        "an earlier reset link survived a newer one"
    );
    let (status, value) = h.inspect(&current).await?;
    ensure!(
        status == 200 && value["kind"] == "password_reset" && value["name"] == "초대 사용자",
        "reset link was not described"
    );
    let (status, value) = h
        .complete(&current, person("invited.user", "a-brand-new-password-5"))
        .await?;
    ensure!(
        status == 400 && value["error"]["code"] == "invalid_account_link_input",
        "a reset link accepted account fields"
    );
    sqlx::query("INSERT INTO sessions(id_hash,user_id,access_token,refresh_token,csrf,expires_at,browser_purpose) VALUES($1,$2,'synthetic','synthetic','synthetic',now()+interval '1 hour','portal')")
        .bind(hash(auth::random())).bind(&subject).execute(&state.pool).await?;
    let (status, value) = h
        .complete(&current, json!({"password": "a-brand-new-password-5"}))
        .await?;
    ensure!(
        status == 200 && value["kind"] == "password_reset",
        "reset was not completed"
    );
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions WHERE user_id=$1")
        .bind(&subject)
        .fetch_one(&state.pool)
        .await?;
    let audited: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit WHERE actor=$1 AND action='user.password_reset' AND target=$2",
    )
    .bind(&subject)
    .bind(format!("user:{subject}:link:{}", current.id))
    .fetch_one(&state.pool)
    .await?;
    ensure!(
        identity.read(|state| {
            state.accounts[&subject].password == "a-brand-new-password-5"
                && state.ended == vec![subject.clone()]
        }) && sessions == 0
            && audited == 1
            && h.refused(&current).await?,
        "the reset did not set the password once, end browser sessions and leave a record"
    );
    // A disabled account gets no link, and one issued earlier stops working.
    let held = h.issue(&admin, reset.clone()).await?;
    sqlx::query("UPDATE users SET enabled=false WHERE id=$1")
        .bind(&subject)
        .execute(&state.pool)
        .await?;
    let denied: Value = h
        .signed(Method::POST, "/api/v1/account-links", &admin)
        .json(&reset)
        .send()
        .await?
        .json()
        .await?;
    ensure!(
        denied["error"]["code"] == "user_disabled"
            && h.refused(&held).await?
            && h.listed(&admin, &held).await? == "inactive",
        "a disabled account could be recovered through a reset link"
    );
    sqlx::query("UPDATE users SET enabled=true WHERE id=$1")
        .bind(&subject)
        .execute(&state.pool)
        .await?;
    // An account the identity provider does not know: reported, link kept.
    let missing = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO users(id,name) VALUES($1,'Registered elsewhere')")
        .bind(&missing)
        .execute(&state.pool)
        .await?;
    let stray = h
        .issue(&admin, json!({"kind": "password_reset", "userId": missing}))
        .await?;
    let (status, value) = h
        .complete(&stray, json!({"password": "a-brand-new-password-6"}))
        .await?;
    ensure!(
        status == 409
            && value["error"]["code"] == "identity_account_missing"
            && h.status(&stray).await? == "pending",
        "a missing sign-in account was not reported"
    );
    for (body, code) in [
        (
            json!({"kind": "password_reset", "userId": "not-a-subject"}),
            400,
        ),
        (
            json!({"kind": "password_reset", "userId": Uuid::new_v4().to_string()}),
            404,
        ),
        (
            json!({"kind": "password_reset", "userId": subject, "expiresInHours": 73}),
            400,
        ),
        (
            json!({"kind": "invitation", "name": "초대", "expiresInHours": 0}),
            400,
        ),
        (
            json!({"kind": "invitation", "name": "초대", "userId": subject}),
            400,
        ),
        (json!({"kind": "other", "name": "초대"}), 400),
    ] {
        let reply = h
            .signed(Method::POST, "/api/v1/account-links", &admin)
            .json(&body)
            .send()
            .await?;
        ensure!(reply.status() == code, "an invalid link request was issued");
    }
    passed.push("a password reset link replaces earlier ones, works once for an enabled account and ends its browser sessions".into());

    // 8. Clients learn that the feature is configured from the server.
    let options: Value = client
        .get(format!("{}/auth/options", h.console))
        .send()
        .await?
        .json()
        .await?;
    let capabilities: Value = client
        .get(format!("{}/api/v1/capabilities", h.console))
        .send()
        .await?
        .json()
        .await?;
    ensure!(
        options["accountLinks"] == true
            && options["passwordUrl"] == format!("{}/auth/password", h.console)
            && capabilities["features"]
                .as_array()
                .is_some_and(|features| features.contains(&json!("accounts.links.v1"))),
        "the configured feature was not advertised"
    );
    passed.push("sign-in options and capabilities advertise configured account links".into());
    Ok(passed)
}
