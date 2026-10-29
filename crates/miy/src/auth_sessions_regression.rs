//! Real PostgreSQL + HTTP tests called only by the isolated audit fixture.
//! Synthetic device/credential values never appear in returned evidence or errors.
//! Browser authorization uses the local development-cookie path; this is not an
//! OIDC-provider integration test. No runtime jobs or containers are created.
use crate::{State, auth, contract::hash};
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use reqwest::{Client, Method, RequestBuilder, Response};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

struct Browser {
    user: String,
    cookie: String,
    csrf: String,
}
struct Device {
    id: Uuid,
    client: Uuid,
    secret: String,
    code: String,
}
struct Tokens {
    session: Uuid,
    user: String,
    access: String,
    refresh: String,
    expires: DateTime<Utc>,
}
struct Harness<'a> {
    state: &'a State,
    client: &'a Client,
    base: &'a str,
}

fn string(value: &Value, field: &str) -> Result<String> {
    value[field]
        .as_str()
        .map(str::to_owned)
        .with_context(|| format!("expected response field {field}"))
}

fn tokens(value: Value) -> Result<Tokens> {
    ensure!(
        value["status"] == "approved",
        "token response is not approved"
    );
    let access = string(&value, "accessToken")?;
    let refresh = string(&value, "refreshToken")?;
    ensure!(
        access.starts_with("miy_access_") && refresh.starts_with("miy_refresh_"),
        "unexpected credential kinds"
    );
    ensure!(value["tokenType"] == "Bearer", "unexpected token type");
    ensure!(
        value["expiresIn"]
            .as_i64()
            .is_some_and(|seconds| (1..=900).contains(&seconds)),
        "access lifetime exceeds its bound"
    );
    Ok(Tokens {
        session: string(&value, "sessionId")?.parse()?,
        user: string(&value, "userId")?,
        access,
        refresh,
        expires: string(&value, "refreshExpiresAt")?.parse()?,
    })
}

async fn response(response: Response, expected: u16) -> Result<Value> {
    // Never include the body: successful token responses contain credentials.
    ensure!(
        response.status().as_u16() == expected,
        "expected HTTP {expected}, received HTTP {}",
        response.status().as_u16()
    );
    Ok(response.json().await?)
}

async fn private_response(value: Response, expected: u16) -> Result<Value> {
    ensure!(
        value
            .headers()
            .get("cache-control")
            .is_some_and(|value| value == "no-store"),
        "credential response missing no-store"
    );
    response(value, expected).await
}

async fn error(response_value: Response, status: u16, code: &str) -> Result<()> {
    let value = response(response_value, status).await?;
    ensure!(
        value["error"]["code"] == code,
        "expected stable error code {code}"
    );
    Ok(())
}

impl Harness<'_> {
    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.client.request(method, format!("{}{path}", self.base))
    }

    fn browser(&self, method: Method, path: &str, browser: &Browser, csrf: bool) -> RequestBuilder {
        let request = self
            .request(method, path)
            .header("cookie", format!("miy_development={}", browser.cookie))
            .header("origin", &self.state.auth.origin);
        if csrf {
            request.header("x-miy-csrf", &browser.csrf)
        } else {
            request
        }
    }

    async fn user(&self) -> Result<Browser> {
        let user = format!("auth-audit-{}", Uuid::new_v4());
        sqlx::query("INSERT INTO users(id,name,can_develop) VALUES($1,$1,true)")
            .bind(&user)
            .execute(&self.state.pool)
            .await?;
        self.new_browser(&user).await
    }

    async fn new_browser(&self, user: &str) -> Result<Browser> {
        let cookie = auth::random();
        let csrf = auth::random();
        sqlx::query("INSERT INTO development_sessions(id_hash,user_id,csrf,expires_at) VALUES($1,$2,$3,now()+interval '8 hours')")
            .bind(hash(&cookie)).bind(user).bind(&csrf).execute(&self.state.pool).await?;
        Ok(Browser {
            user: user.into(),
            cookie,
            csrf,
        })
    }

    async fn device_with_client(&self, client: Uuid) -> Result<Device> {
        let value = private_response(
            self.request(Method::POST, "/auth/cli/device")
                .json(&json!({"clientId":client,"clientName":"Isolated auth regression"}))
                .send()
                .await?,
            200,
        )
        .await?;
        ensure!(
            value["expiresIn"] == 300 && value["interval"] == 5,
            "unexpected device lifetime or polling interval"
        );
        let id: Uuid = string(&value, "id")?.parse()?;
        ensure!(
            value["verificationPath"] == format!("/cli/device/{id}"),
            "unexpected verification path"
        );
        Ok(Device {
            id,
            client,
            secret: string(&value, "deviceCode")?,
            code: string(&value, "userCode")?,
        })
    }

    async fn device(&self) -> Result<Device> {
        self.device_with_client(Uuid::new_v4()).await
    }

    async fn approve(&self, browser: &Browser, device: &Device, decision: &str) -> Result<()> {
        let value = private_response(
            self.browser(
                Method::POST,
                &format!("/api/v1/cli/devices/{}/decision", device.id),
                browser,
                true,
            )
            .json(&json!({"userCode":device.code,"decision":decision}))
            .send()
            .await?,
            200,
        )
        .await?;
        ensure!(
            value["status"]
                == if decision == "approve" {
                    "approved"
                } else {
                    "denied"
                },
            "unexpected decision result"
        );
        Ok(())
    }

    async fn poll(&self, device: &Device) -> Result<Response> {
        Ok(self
            .request(Method::POST, "/auth/cli/token")
            .json(&json!({"grantType":"device_code","id":device.id,"deviceCode":device.secret}))
            .send()
            .await?)
    }

    async fn ready_to_poll(&self, device: &Device) -> Result<()> {
        // Advance only this synthetic request's timer; do not sleep five seconds.
        sqlx::query("UPDATE cli_device_authorizations SET last_poll_at=clock_timestamp()-interval '61 seconds' WHERE id=$1")
            .bind(device.id).execute(&self.state.pool).await?;
        Ok(())
    }

    async fn exchange(&self, device: &Device) -> Result<Tokens> {
        tokens(private_response(self.poll(device).await?, 200).await?)
    }

    async fn issue(&self, browser: &Browser) -> Result<Tokens> {
        let device = self.device().await?;
        self.approve(browser, &device, "approve").await?;
        let tokens = self.exchange(&device).await?;
        ensure!(
            tokens.user == browser.user,
            "approved account changed during exchange"
        );
        Ok(tokens)
    }

    async fn refresh(&self, refresh: &str) -> Result<Response> {
        Ok(self
            .request(Method::POST, "/auth/cli/token")
            .json(&json!({"grantType":"refresh_token","refreshToken":refresh}))
            .send()
            .await?)
    }

    async fn me(&self, access: &str) -> Result<Response> {
        Ok(self
            .request(Method::GET, "/api/v1/me")
            .bearer_auth(access)
            .send()
            .await?)
    }

    async fn active(&self, tokens: &Tokens) -> Result<()> {
        let me = response(self.me(&tokens.access).await?, 200).await?;
        ensure!(me["id"] == tokens.user, "access token changed account");
        Ok(())
    }
}

async fn approval_and_rotation(h: &Harness<'_>, passed: &mut Vec<String>) -> Result<()> {
    let browser = h.user().await?;
    error(
        h.request(Method::POST, "/auth/cli/device")
            .header("origin", &h.state.auth.origin)
            .json(&json!({"clientId":Uuid::new_v4()}))
            .send()
            .await?,
        403,
        "forbidden",
    )
    .await?;
    let device = h.device().await?;
    let metadata_path = format!("/api/v1/cli/devices/{}", device.id);
    error(
        h.request(Method::GET, &metadata_path).send().await?,
        401,
        "unauthorized",
    )
    .await?;
    let metadata = private_response(
        h.browser(Method::GET, &metadata_path, &browser, false)
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        metadata["status"] == "pending"
            && metadata["expiresIn"]
                .as_i64()
                .is_some_and(|value| (1..=300).contains(&value)),
        "invalid public device lifetime"
    );
    ensure!(
        metadata.as_object().is_some_and(|value| value.len() == 5)
            && !metadata.to_string().contains(&device.secret)
            && !metadata.to_string().contains(&device.code),
        "device status exposed approval credentials"
    );
    let stored=sqlx::query("SELECT device_secret_hash,user_code_hash,EXTRACT(epoch FROM expires_at-created_at)::bigint AS seconds FROM cli_device_authorizations WHERE id=$1")
        .bind(device.id).fetch_one(&h.state.pool).await?;
    ensure!(
        stored.get::<String, _>("device_secret_hash") == hash(&device.secret),
        "device secret was not hashed"
    );
    let normalized = device.code.replace('-', "");
    ensure!(
        stored.get::<String, _>("user_code_hash") == hash(format!("{}:{normalized}", device.id)),
        "user code is not bound to login ID"
    );
    ensure!(
        stored.get::<i64, _>("seconds") == 300,
        "device expiry is not five minutes"
    );
    error(h.request(Method::POST,"/auth/cli/token").json(&json!({"grantType":"device_code","id":device.id,"deviceCode":format!("miy_device_{}",auth::random())})).send().await?,400,"invalid_grant").await?;
    let pending = private_response(h.poll(&device).await?, 200).await?;
    ensure!(
        pending["status"] == "pending" && pending["interval"] == 5,
        "first poll is not pending at five seconds"
    );
    let slowdown = private_response(h.poll(&device).await?, 200).await?;
    ensure!(
        slowdown["status"] == "slow_down" && slowdown["interval"] == 10,
        "early polling did not slow down"
    );
    let path = format!("/api/v1/cli/devices/{}/decision", device.id);
    let decision = json!({"userCode":device.code,"decision":"approve"});
    error(
        h.browser(Method::POST, &path, &browser, false)
            .json(&decision)
            .send()
            .await?,
        403,
        "forbidden",
    )
    .await?;
    error(
        h.browser(Method::POST, &path, &browser, false)
            .header("x-miy-csrf", "incorrect")
            .json(&decision)
            .send()
            .await?,
        403,
        "forbidden",
    )
    .await?;
    error(
        h.request(Method::POST, &path)
            .header("cookie", format!("miy_development={}", browser.cookie))
            .header("x-miy-csrf", &browser.csrf)
            .header("origin", "https://foreign.invalid")
            .json(&decision)
            .send()
            .await?,
        403,
        "forbidden",
    )
    .await?;
    let wrong = if device.code == "AAAAA-AAAAA" {
        "BBBBB-BBBBB"
    } else {
        "AAAAA-AAAAA"
    };
    error(
        h.browser(Method::POST, &path, &browser, true)
            .json(&json!({"userCode":wrong,"decision":"approve"}))
            .send()
            .await?,
        409,
        "login_request_expired_or_decided",
    )
    .await?;
    ensure!(
        sqlx::query_scalar::<_, String>("SELECT status FROM cli_device_authorizations WHERE id=$1")
            .bind(device.id)
            .fetch_one(&h.state.pool)
            .await?
            == "pending",
        "invalid approval mutated the request"
    );
    h.approve(&browser, &device, "approve").await?;
    h.ready_to_poll(&device).await?;
    let original = h.exchange(&device).await?;
    error(
        h.request(Method::GET, &metadata_path)
            .bearer_auth(&original.access)
            .send()
            .await?,
        403,
        "forbidden",
    )
    .await?;
    let other_browser = h.user().await?;
    error(
        h.browser(Method::GET, &metadata_path, &other_browser, false)
            .send()
            .await?,
        404,
        "not_found",
    )
    .await?;
    h.active(&original).await?;
    error(h.poll(&device).await?, 400, "invalid_grant").await?;
    ensure!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM cli_sessions WHERE client_id=$1")
            .bind(device.client)
            .fetch_one(&h.state.pool)
            .await?
            == 1,
        "device exchange issued more than once"
    );
    error(
        h.request(Method::GET, "/api/v1/me")
            .bearer_auth(&original.access)
            .header("origin", &h.state.auth.origin)
            .send()
            .await?,
        403,
        "forbidden",
    )
    .await?;
    error(
        h.request(Method::POST, &path)
            .bearer_auth(&original.access)
            .json(&decision)
            .send()
            .await?,
        403,
        "forbidden",
    )
    .await?;
    passed.push(
        "device hashes, pending/slow_down, browser CSRF/origin, exact account and one-use exchange"
            .into(),
    );

    let lifetime=sqlx::query("SELECT EXTRACT(epoch FROM s.expires_at-s.created_at)::bigint AS session_seconds,EXTRACT(epoch FROM a.expires_at-a.created_at)::bigint AS access_seconds FROM cli_sessions s JOIN cli_access_tokens a ON a.session_id=s.id WHERE a.token_hash=$1")
        .bind(hash(&original.access)).fetch_one(&h.state.pool).await?;
    ensure!(
        lifetime.get::<i64, _>("session_seconds") == 720 * 60 * 60
            && lifetime.get::<i64, _>("access_seconds") == 900,
        "credential lifetime exceeds contract"
    );
    let refreshed = tokens(private_response(h.refresh(&original.refresh).await?, 200).await?)?;
    ensure!(
        refreshed.session == original.session
            && refreshed.user == original.user
            && refreshed.expires == original.expires,
        "refresh changed identity or absolute expiry"
    );
    ensure!(
        refreshed.access != original.access && refreshed.refresh != original.refresh,
        "refresh did not rotate both secrets"
    );
    let row=sqlx::query("SELECT count(*) AS total,count(*) FILTER (WHERE used_at IS NOT NULL) AS used,count(*) FILTER (WHERE used_at IS NULL) AS current FROM cli_refresh_tokens WHERE session_id=$1")
        .bind(original.session).fetch_one(&h.state.pool).await?;
    ensure!(
        row.get::<i64, _>("total") == 2
            && row.get::<i64, _>("used") == 1
            && row.get::<i64, _>("current") == 1,
        "refresh history or current-token cardinality is invalid"
    );
    h.active(&refreshed).await?;
    error(
        h.refresh(&original.refresh).await?,
        401,
        "refresh_reuse_detected",
    )
    .await?;
    let revoked:bool=sqlx::query_scalar("SELECT revoked_at IS NOT NULL AND revocation_reason='refresh_reuse' FROM cli_sessions WHERE id=$1")
        .bind(original.session).fetch_one(&h.state.pool).await?;
    ensure!(
        revoked,
        "refresh reuse returned an error without committing family revocation"
    );
    error(h.me(&original.access).await?, 401, "unauthorized").await?;
    error(h.me(&refreshed.access).await?, 401, "unauthorized").await?;
    error(
        h.refresh(&refreshed.refresh).await?,
        401,
        "session_expired_or_revoked",
    )
    .await?;
    passed.push(
        "15-minute access, fixed 30-day family, rotation history and committed reuse revocation"
            .into(),
    );
    Ok(())
}

async fn concurrent_exchange_and_refresh(h: &Harness<'_>, passed: &mut Vec<String>) -> Result<()> {
    let browser = h.user().await?;
    let device = h.device().await?;
    h.approve(&browser, &device, "approve").await?;
    let (left, right) = tokio::join!(h.poll(&device), h.poll(&device));
    let left = left?;
    let right = right?;
    let (winner, loser) = if left.status().is_success() {
        (left, right)
    } else {
        (right, left)
    };
    let original = tokens(private_response(winner, 200).await?)?;
    error(loser, 400, "invalid_grant").await?;
    ensure!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM cli_sessions WHERE client_id=$1")
            .bind(device.client)
            .fetch_one(&h.state.pool)
            .await?
            == 1,
        "concurrent device exchange created two families"
    );

    let (left, right) = tokio::join!(h.refresh(&original.refresh), h.refresh(&original.refresh));
    let left = left?;
    let right = right?;
    let (winner, loser) = if left.status().is_success() {
        (left, right)
    } else {
        (right, left)
    };
    let rotated = tokens(private_response(winner, 200).await?)?;
    error(loser, 401, "refresh_reuse_detected").await?;
    let row=sqlx::query("SELECT (SELECT count(*) FROM cli_refresh_tokens WHERE session_id=$1) AS refresh_count,(SELECT count(*) FROM cli_access_tokens WHERE session_id=$1) AS access_count,revoked_at IS NOT NULL AS revoked FROM cli_sessions WHERE id=$1")
        .bind(original.session).fetch_one(&h.state.pool).await?;
    ensure!(
        row.get::<i64, _>("refresh_count") == 2
            && row.get::<i64, _>("access_count") == 2
            && row.get::<bool, _>("revoked"),
        "concurrent refresh did not serialize into one rotation and a revoked family"
    );
    error(h.me(&rotated.access).await?, 401, "unauthorized").await?;
    passed.push("concurrent device exchange issues once; concurrent refresh commits one rotation then family revocation".into());
    Ok(())
}

async fn denial_and_expiry(h: &Harness<'_>, passed: &mut Vec<String>) -> Result<()> {
    let browser = h.user().await?;
    let denied = h.device().await?;
    h.approve(&browser, &denied, "deny").await?;
    error(h.poll(&denied).await?, 400, "access_denied").await?;
    let expired = h.device().await?;
    sqlx::query("UPDATE cli_device_authorizations SET created_at=created_at-interval '6 minutes',expires_at=expires_at-interval '6 minutes' WHERE id=$1")
        .bind(expired.id).execute(&h.state.pool).await?;
    error(h.poll(&expired).await?, 400, "expired_token").await?;
    error(
        h.browser(
            Method::POST,
            &format!("/api/v1/cli/devices/{}/decision", expired.id),
            &browser,
            true,
        )
        .json(&json!({"userCode":expired.code,"decision":"approve"}))
        .send()
        .await?,
        409,
        "login_request_expired_or_decided",
    )
    .await?;

    let original = h.issue(&browser).await?;
    sqlx::query("UPDATE cli_access_tokens SET created_at=created_at-interval '16 minutes',expires_at=expires_at-interval '16 minutes' WHERE token_hash=$1")
        .bind(hash(&original.access)).execute(&h.state.pool).await?;
    error(h.me(&original.access).await?, 401, "unauthorized").await?;
    let refreshed = tokens(private_response(h.refresh(&original.refresh).await?, 200).await?)?;
    h.active(&refreshed).await?;
    ensure!(
        refreshed.expires == original.expires,
        "refresh extended the original family lifetime"
    );
    sqlx::query("UPDATE cli_sessions SET created_at=created_at-interval '721 hours',expires_at=expires_at-interval '721 hours' WHERE id=$1")
        .bind(original.session).execute(&h.state.pool).await?;
    error(h.me(&refreshed.access).await?, 401, "unauthorized").await?;
    error(
        h.refresh(&refreshed.refresh).await?,
        401,
        "session_expired_or_revoked",
    )
    .await?;
    passed.push("denial and five-minute device expiry; expired access refreshes; absolute family expiry denies both tokens".into());
    Ok(())
}

async fn session_ownership_and_pagination(h: &Harness<'_>, passed: &mut Vec<String>) -> Result<()> {
    let alice = h.user().await?;
    let bob = h.user().await?;
    let first = h.issue(&alice).await?;
    let second = h.issue(&alice).await?;
    let foreign = h.issue(&bob).await?;
    let one = private_response(
        h.request(Method::GET, "/api/v1/cli/sessions")
            .bearer_auth(&second.access)
            .query(&[("limit", "1")])
            .send()
            .await?,
        200,
    )
    .await?;
    let cursor = string(&one, "nextCursor")?;
    let one_sessions = one["sessions"]
        .as_array()
        .context("session page missing rows")?;
    ensure!(
        one_sessions.len() == 1 && one_sessions[0]["id"] == second.session.to_string(),
        "first page is not latest own session"
    );
    let two = private_response(
        h.request(Method::GET, "/api/v1/cli/sessions")
            .bearer_auth(&second.access)
            .query(&[("limit", "1"), ("cursor", cursor.as_str())])
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        two["sessions"]
            .as_array()
            .is_some_and(|rows| rows.len() == 1)
            && two["sessions"][0]["id"] == first.session.to_string()
            && two["nextCursor"].is_null(),
        "keyset page skipped, repeated or leaked a session"
    );
    error(
        h.request(Method::GET, "/api/v1/cli/sessions")
            .bearer_auth(&foreign.access)
            .query(&[("cursor", cursor.as_str())])
            .send()
            .await?,
        400,
        "invalid_cursor",
    )
    .await?;
    error(
        h.request(Method::GET, "/api/v1/cli/sessions")
            .bearer_auth(&second.access)
            .query(&[("limit", "201")])
            .send()
            .await?,
        400,
        "invalid_limit",
    )
    .await?;
    error(
        h.request(
            Method::DELETE,
            &format!("/api/v1/cli/sessions/{}", foreign.session),
        )
        .bearer_auth(&second.access)
        .send()
        .await?,
        404,
        "not_found",
    )
    .await?;
    private_response(
        h.request(
            Method::DELETE,
            &format!("/api/v1/cli/sessions/{}", first.session),
        )
        .bearer_auth(&second.access)
        .send()
        .await?,
        200,
    )
    .await?;
    error(h.me(&first.access).await?, 401, "unauthorized").await?;
    h.active(&second).await?;
    h.active(&foreign).await?;
    for _ in 0..2 {
        private_response(
            h.request(Method::POST, "/auth/cli/revoke")
                .json(&json!({"refreshToken":second.refresh}))
                .send()
                .await?,
            200,
        )
        .await?;
    }
    error(h.me(&second.access).await?, 401, "unauthorized").await?;
    passed.push("self-only session keyset pagination/revocation and idempotent logout without affecting another family".into());
    Ok(())
}

async fn permission_and_disable(h: &Harness<'_>, passed: &mut Vec<String>) -> Result<()> {
    let browser = h.user().await?;
    let issued = h.issue(&browser).await?;
    let app = format!("auth-probe-{}", &Uuid::new_v4().simple().to_string()[..12]);
    sqlx::query("INSERT INTO apps(id,name) VALUES($1,'Auth regression app')")
        .bind(&app)
        .execute(&h.state.pool)
        .await?;
    sqlx::query(
        "INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,$2,'dev','develop')",
    )
    .bind(&browser.user)
    .bind(&app)
    .execute(&h.state.pool)
    .await?;
    let preflight = json!({"target":{"appId":app,"environment":"dev"},"action":"deploy","imageId":format!("sha256:{}","a".repeat(64))});
    response(
        h.request(
            Method::POST,
            &format!("/api/v1/apps/{app}/runtime/preflight"),
        )
        .bearer_auth(&issued.access)
        .json(&preflight)
        .send()
        .await?,
        200,
    )
    .await?;
    sqlx::query("DELETE FROM grants WHERE user_id=$1 AND app_id=$2")
        .bind(&browser.user)
        .bind(&app)
        .execute(&h.state.pool)
        .await?;
    error(
        h.request(
            Method::POST,
            &format!("/api/v1/apps/{app}/runtime/preflight"),
        )
        .bearer_auth(&issued.access)
        .json(&preflight)
        .send()
        .await?,
        403,
        "forbidden",
    )
    .await?;
    sqlx::query(
        "INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,$2,'dev','develop')",
    )
    .bind(&browser.user)
    .bind(&app)
    .execute(&h.state.pool)
    .await?;
    let legacy = format!("miy_cli_{}", auth::random());
    sqlx::query("INSERT INTO cli_tokens(token_hash,user_id) VALUES($1,$2)")
        .bind(hash(&legacy))
        .bind(&browser.user)
        .execute(&h.state.pool)
        .await?;
    response(h.me(&legacy).await?, 200).await?;
    let legacy_id = Uuid::new_v4();
    sqlx::query("INSERT INTO cli_authorizations(id,challenge,user_id) VALUES($1,$2,$3)")
        .bind(legacy_id)
        .bind(hash(auth::random()))
        .bind(&browser.user)
        .execute(&h.state.pool)
        .await?;
    sqlx::query("INSERT INTO sessions(id_hash,user_id,access_token,refresh_token,csrf,expires_at) VALUES($1,$2,$3,$4,$5,now()+interval '1 hour')")
        .bind(hash(auth::random())).bind(&browser.user).bind(auth::encrypt("synthetic-browser-access")?)
        .bind(auth::encrypt("synthetic-browser-refresh")?).bind(auth::random()).execute(&h.state.pool).await?;
    let approved = h.device().await?;
    h.approve(&browser, &approved, "approve").await?;
    response(
        h.browser(Method::GET, "/api/v1/me", &browser, false)
            .send()
            .await?,
        200,
    )
    .await?;
    sqlx::query("UPDATE users SET enabled=false WHERE id=$1")
        .bind(&browser.user)
        .execute(&h.state.pool)
        .await?;
    error(h.me(&issued.access).await?, 401, "unauthorized").await?;
    error(h.me(&legacy).await?, 401, "unauthorized").await?;
    error(
        h.browser(Method::GET, "/api/v1/me", &browser, false)
            .send()
            .await?,
        401,
        "unauthorized",
    )
    .await?;
    error(h.refresh(&issued.refresh).await?, 403, "user_disabled").await?;
    ensure!(
        !crate::domain::can_manage(&h.state.pool, &browser.user, &app, "dev").await?,
        "disabled user retained queued runtime authority"
    );
    let removed:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM cli_tokens WHERE user_id=$1)+(SELECT count(*) FROM sessions WHERE user_id=$1)+(SELECT count(*) FROM development_sessions WHERE user_id=$1)+(SELECT count(*) FROM cli_authorizations WHERE user_id=$1)")
        .bind(&browser.user).fetch_one(&h.state.pool).await?;
    ensure!(
        removed == 0,
        "disable trigger left an old browser, development or legacy credential"
    );
    let revoked:bool=sqlx::query_scalar("SELECT revoked_at IS NOT NULL AND revocation_reason='user_disabled' FROM cli_sessions WHERE id=$1")
        .bind(issued.session).fetch_one(&h.state.pool).await?;
    ensure!(revoked, "disable did not revoke the CLI family");
    ensure!(
        sqlx::query_scalar::<_, String>("SELECT status FROM cli_device_authorizations WHERE id=$1")
            .bind(approved.id)
            .fetch_one(&h.state.pool)
            .await?
            == "denied",
        "disable left an approved device exchange available"
    );
    sqlx::query("UPDATE users SET enabled=true WHERE id=$1")
        .bind(&browser.user)
        .execute(&h.state.pool)
        .await?;
    error(h.me(&issued.access).await?, 401, "unauthorized").await?;
    error(h.me(&legacy).await?, 401, "unauthorized").await?;
    error(
        h.browser(Method::GET, "/api/v1/me", &browser, false)
            .send()
            .await?,
        401,
        "unauthorized",
    )
    .await?;
    error(
        h.refresh(&issued.refresh).await?,
        401,
        "session_expired_or_revoked",
    )
    .await?;
    error(h.poll(&approved).await?, 400, "access_denied").await?;
    let fresh_browser = h.new_browser(&browser.user).await?;
    let fresh = h.issue(&fresh_browser).await?;
    h.active(&fresh).await?;
    passed.push("grant revocation and Core disable immediately block API/worker authority; re-enable never resurrects old sessions".into());
    Ok(())
}

async fn rate_and_capacity(h: &Harness<'_>, passed: &mut Vec<String>) -> Result<()> {
    let client_id = Uuid::new_v4();
    let mut limited = false;
    // Permit a minute-boundary rollover without making the test sleep or
    // depend on the audit runner's position within the current minute.
    for _ in 0..12 {
        let result = h
            .request(Method::POST, "/auth/cli/device")
            .json(&json!({"clientId":client_id}))
            .send()
            .await?;
        if result.status().as_u16() == 429 {
            error(result, 429, "rate_limited").await?;
            limited = true;
            break;
        }
        private_response(result, 200).await?;
    }
    ensure!(
        limited,
        "per-client creation did not enforce its minute limit"
    );
    let attempts:i32=sqlx::query_scalar("SELECT COALESCE(max(attempts),0) FROM cli_auth_rate_limits WHERE scope='start_client' AND subject_hash=$1")
        .bind(hash(client_id.to_string())).fetch_one(&h.state.pool).await?;
    ensure!(attempts > 5, "rejected client attempts were not committed");
    let browser = h.user().await?;
    let device = h.device().await?;
    let path = format!("/api/v1/cli/devices/{}/decision", device.id);
    let wrong = if device.code == "AAAAA-AAAAA" {
        "BBBBB-BBBBB"
    } else {
        "AAAAA-AAAAA"
    };
    let mut limited = false;
    for _ in 0..22 {
        let result = h
            .browser(Method::POST, &path, &browser, true)
            .json(&json!({"userCode":wrong,"decision":"approve"}))
            .send()
            .await?;
        if result.status().as_u16() == 429 {
            error(result, 429, "rate_limited").await?;
            limited = true;
            break;
        }
        error(result, 409, "login_request_expired_or_decided").await?;
    }
    ensure!(
        limited,
        "per-actor approval did not enforce its minute limit"
    );
    let attempts:i32=sqlx::query_scalar("SELECT COALESCE(max(attempts),0) FROM cli_auth_rate_limits WHERE scope='approval_actor' AND subject_hash=$1")
        .bind(hash(&browser.user)).fetch_one(&h.state.pool).await?;
    ensure!(
        attempts > 10,
        "rejected approval attempts were not committed"
    );
    ensure!(
        sqlx::query_scalar::<_, String>("SELECT status FROM cli_device_authorizations WHERE id=$1")
            .bind(device.id)
            .fetch_one(&h.state.pool)
            .await?
            == "pending",
        "rate-limited actor approved a request"
    );
    let live:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM cli_device_authorizations WHERE expires_at>now())+(SELECT count(*) FROM cli_authorizations WHERE expires_at>now())")
        .fetch_one(&h.state.pool).await?;
    ensure!(
        (0..=100).contains(&live),
        "login capacity was already exceeded"
    );
    let capacity_client = Uuid::new_v4();
    let ids: Vec<Uuid> = (live..100).map(|_| Uuid::new_v4()).collect();
    sqlx::query("INSERT INTO cli_device_authorizations(id,client_id,client_name,device_secret_hash,user_code_hash) SELECT id,$2,'Isolated capacity fixture','capacity-secret-'||id::text,'capacity-code-'||id::text FROM unnest($1::uuid[]) AS fixture(id)")
        .bind(&ids).bind(capacity_client).execute(&h.state.pool).await?;
    let limited = h
        .request(Method::POST, "/auth/cli/device")
        .json(&json!({"clientId":Uuid::new_v4()}))
        .send()
        .await;
    // Remove only capacity rows, even when the HTTP assertion fails.
    sqlx::query("DELETE FROM cli_device_authorizations WHERE client_id=$1")
        .bind(capacity_client)
        .execute(&h.state.pool)
        .await?;
    error(limited?, 503, "too_many_pending_logins").await?;
    passed.push("per-client creation, per-actor code guessing and shared legacy/device global capacity are bounded".into());
    Ok(())
}

/// Call near the end of runtime_regression's owned fixture, after migrations and
/// after serving http::router(state.clone()) on `base`. Requires MIY_DEV_LOGIN=1
/// before Auth::new. Run without other auth clients during the capacity check.
/// The caller owns server/container teardown and evidence I/O.
pub async fn run(state: &State, client: &Client, base: &str) -> Result<Vec<String>> {
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&state.pool)
        .await?;
    ensure!(
        database == "miy_audit_core",
        "isolated auth regression database required"
    );
    let url = url::Url::parse(base)?;
    ensure!(
        matches!(url.host(),Some(url::Host::Ipv4(ip)) if ip.is_loopback())
            || matches!(url.host(),Some(url::Host::Ipv6(ip)) if ip.is_loopback())
            || url.host_str() == Some("localhost"),
        "loopback HTTP fixture required"
    );
    ensure!(
        state.auth.development_login,
        "auth regression requires the explicit local development profile"
    );
    let h = Harness {
        state,
        client,
        base: base.trim_end_matches('/'),
    };
    let mut passed = vec![];
    let login_client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    for path in [
        "/auth/callback",
        "/auth/callback?error=expired_code&state=untrusted",
        "/auth/callback?code=consumed&state=unknown",
    ] {
        let response = login_client.get(format!("{}{path}", h.base)).send().await?;
        ensure!(
            response.status().as_u16() == 303
                && response
                    .headers()
                    .get("location")
                    .is_some_and(|value| value == "/?login=expired"),
            "invalid OAuth callback did not return safe login recovery"
        );
        ensure!(
            response.headers().get("set-cookie").is_some_and(|value| {
                value.to_str().is_ok_and(|value| {
                    value.starts_with("miy_login=;") && value.contains("Max-Age=0")
                })
            }),
            "invalid callback retained its login cookie"
        );
    }
    passed.push("expired/missing/replayed OAuth callbacks clear login state and redirect to fixed recovery without authenticating".into());
    approval_and_rotation(&h, &mut passed)
        .await
        .context("device approval and rotation regression")?;
    concurrent_exchange_and_refresh(&h, &mut passed)
        .await
        .context("concurrent credential regression")?;
    denial_and_expiry(&h, &mut passed)
        .await
        .context("credential expiry regression")?;
    session_ownership_and_pagination(&h, &mut passed)
        .await
        .context("session ownership regression")?;
    permission_and_disable(&h, &mut passed)
        .await
        .context("Core enabled-state regression")?;
    rate_and_capacity(&h, &mut passed)
        .await
        .context("auth rate and capacity regression")?;
    Ok(passed)
}
