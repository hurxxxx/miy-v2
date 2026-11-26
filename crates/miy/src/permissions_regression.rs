//! Isolated PostgreSQL + HTTP checks for explicit installation permissions.
//! App backends are loopback fixtures. Credentials and signed identities remain
//! in memory and are never included in returned evidence or assertion messages.
use crate::{State, auth, contract::hash};
use anyhow::{Context, Result, ensure};
use axum::{Json, Router, extract::State as S, http::HeaderMap, routing::any};
use reqwest::{Client, Method, RequestBuilder, Response};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::task::JoinHandle;
use uuid::Uuid;

struct Actor {
    id: String,
    token: String,
    session: Uuid,
}
struct App {
    app: String,
    installation: Uuid,
    release: Uuid,
    capture: Arc<Mutex<Vec<Value>>>,
}
impl App {
    fn proof(&self) -> Result<String> {
        self.capture
            .lock()
            .map_err(|_| anyhow::anyhow!("fixture capture poisoned"))?
            .last()
            .and_then(|value| value["delegation_token"].as_str())
            .map(str::to_owned)
            .context("app identity missing opaque delegation proof")
    }
    fn last_identity(&self) -> Result<Value> {
        self.capture
            .lock()
            .map_err(|_| anyhow::anyhow!("fixture capture poisoned"))?
            .last()
            .cloned()
            .context("fixture app was not called")
    }
    fn calls(&self) -> usize {
        self.capture.lock().unwrap().len()
    }
    fn connection(&self, target: &Self) -> String {
        format!(
            "/api/v1/installations/{}/connections/{}/api/probe",
            self.installation, target.app
        )
    }
    fn directory(&self) -> String {
        format!(
            "/api/v1/installations/{}/platform/directory",
            self.installation
        )
    }
    fn permissions(&self) -> String {
        format!("/api/v1/installations/{}/permissions", self.installation)
    }
}

#[derive(Clone)]
struct Backend {
    secret: String,
    capture: Arc<Mutex<Vec<Value>>>,
}
async fn probe(S(backend): S<Backend>, headers: HeaderMap) -> Json<Value> {
    let claims = headers
        .get("x-miy-identity")
        .and_then(|value| value.to_str().ok())
        .and_then(|token| {
            let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
            validation.set_audience(&["miy-app"]);
            validation.set_issuer(&["miy"]);
            jsonwebtoken::decode::<Value>(
                token,
                &jsonwebtoken::DecodingKey::from_secret(backend.secret.as_bytes()),
                &validation,
            )
            .ok()
            .map(|decoded| decoded.claims)
        });
    if let Some(claims) = claims {
        backend.capture.lock().unwrap().push(claims);
        Json(json!({"verified":true}))
    } else {
        Json(json!({"verified":false}))
    }
}

struct Harness<'a> {
    state: &'a State,
    client: &'a Client,
    base: &'a str,
    backends: Vec<JoinHandle<()>>,
}
impl Drop for Harness<'_> {
    fn drop(&mut self) {
        for task in &self.backends {
            task.abort();
        }
    }
}
async fn response(value: Response, expected: u16) -> Result<Value> {
    let status = value.status().as_u16();
    if status != expected {
        // The path and the stable error code locate a failure; bodies of
        // successful calls may carry credentials and are never printed.
        let path = value.url().path().to_owned();
        let code = value
            .json::<Value>()
            .await
            .ok()
            .and_then(|body| body["error"]["code"].as_str().map(str::to_owned))
            .unwrap_or_default();
        anyhow::bail!(
            "permission fixture expected HTTP {expected}, received HTTP {status} from {path} ({code})"
        );
    }
    Ok(value.json().await?)
}
async fn denied(value: Response) -> Result<()> {
    let status = value.status().as_u16();
    ensure!(
        matches!(status, 401 | 403),
        "permission fixture expected authorization denial, received HTTP {status}"
    );
    let value: Value = value.json().await?;
    ensure!(
        value["error"]["code"].is_string(),
        "missing stable denial code"
    );
    Ok(())
}
impl Harness<'_> {
    fn request(&self, token: &str, method: Method, path: &str) -> RequestBuilder {
        self.client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(token)
    }
    async fn actor(&self, admin: bool) -> Result<Actor> {
        let id = format!("permission-audit-{}", Uuid::new_v4());
        let token = format!("miy_access_{}", auth::random());
        let session = Uuid::new_v4();
        sqlx::query("INSERT INTO users(id,name,admin) VALUES($1,$1,$2)")
            .bind(&id)
            .bind(admin)
            .execute(&self.state.pool)
            .await?;
        sqlx::query("INSERT INTO cli_sessions(id,user_id,client_id,client_name) VALUES($1,$2,$3,'Permission audit fixture')")
            .bind(session).bind(&id).bind(Uuid::new_v4()).execute(&self.state.pool).await?;
        sqlx::query("INSERT INTO cli_access_tokens(token_hash,session_id,expires_at) VALUES($1,$2,now()+interval '10 minutes')")
            .bind(hash(&token)).bind(session).execute(&self.state.pool).await?;
        Ok(Actor { id, token, session })
    }
    async fn json(
        &self,
        token: &str,
        method: Method,
        path: &str,
        body: Value,
        expected: u16,
    ) -> Result<Value> {
        response(
            self.request(token, method, path).json(&body).send().await?,
            expected,
        )
        .await
    }
    /// The installation as the user's platform home lists it, if the user may use it.
    async fn visible(&self, actor: &Actor, app: &App) -> Result<Option<Value>> {
        let rows = response(
            self.request(&actor.token, Method::GET, "/api/v1/installations")
                .send()
                .await?,
            200,
        )
        .await?;
        Ok(rows
            .as_array()
            .context("installations list")?
            .iter()
            .find(|row| row["id"] == json!(app.installation))
            .cloned())
    }
    /// A filter or detail must not bypass admission. CLI app details may include
    /// managed, closed installations, but their canOpen remains false.
    async fn visibility_consistent(&self, actor: &Actor, app: &App, visible: bool) -> Result<()> {
        ensure!(
            self.visible(actor, app).await?.is_some() == visible,
            "default installation visibility differs from expected admission"
        );
        for path in [
            format!("/api/v1/installations?limit=200&appId={}", app.app),
            format!("/api/v1/apps/{}", app.app),
        ] {
            let value = response(
                self.request(&actor.token, Method::GET, &path)
                    .send()
                    .await?,
                200,
            )
            .await?;
            let found = value["installations"]
                .as_array()
                .context("filtered installations list")?
                .iter()
                .any(|row| row["id"] == json!(app.installation) && row["canOpen"] == true);
            ensure!(
                found == visible,
                "filtered installation visibility bypassed admission"
            );
        }
        Ok(())
    }
    async fn grant(&self, actor: &Actor, app: &App, environment: &str) -> Result<()> {
        sqlx::query("INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,$2,$3,'use') ON CONFLICT DO NOTHING")
            .bind(&actor.id).bind(&app.app).bind(environment).execute(&self.state.pool).await?;
        Ok(())
    }
    async fn app(&mut self, app: String, environment: &str, permissions: &[String]) -> Result<App> {
        let release = Uuid::new_v4();
        let installation = Uuid::new_v4();
        let secret = auth::random();
        let capture = Arc::new(Mutex::new(Vec::new()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = i32::from(listener.local_addr()?.port());
        let router = Router::new()
            .route("/api/{*path}", any(probe))
            .with_state(Backend {
                secret: secret.clone(),
                capture: capture.clone(),
            });
        self.backends.push(tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        }));
        sqlx::query(
            "INSERT INTO apps(id,name) VALUES($1,'Permission audit app') ON CONFLICT DO NOTHING",
        )
        .bind(&app)
        .execute(&self.state.pool)
        .await?;
        let manifest = json!({
            "contractVersion":"miy.app/v1alpha1", "id":app, "name":"Permission audit app", "version":"1.0.0",
            "runtime":{"profile":"rust-web-v1"},
            "ui":{"delivery":"module","moduleEntry":"mf-manifest.json","moduleExport":"./AppRoot","contractVersion":"miy.ui/v1alpha1"},
            "resources":{"database":"postgresql"},"migrations":{"directory":"migrations"},"requestedPermissions":permissions,
        });
        sqlx::query(
            "INSERT INTO releases(id,app_id,version,artifact_hash,manifest) VALUES($1,$2,$3,$4,$5)",
        )
        .bind(release)
        .bind(&app)
        .bind(format!("1.0.{}", if environment == "dev" { 0 } else { 1 }))
        .bind(hash(release.to_string()))
        .bind(manifest)
        .execute(&self.state.pool)
        .await?;
        sqlx::query("INSERT INTO installations(id,app_id,environment,release_id,db_status,runtime_status,desired_state,runtime_port,runtime_secret) VALUES($1,$2,$3,$4,'ready','running','running',$5,$6)")
            .bind(installation).bind(&app).bind(environment).bind(release).bind(port)
            .bind(auth::encrypt(&secret)?).execute(&self.state.pool).await?;
        Ok(App {
            app,
            installation,
            release,
            capture,
        })
    }
    async fn read(&self, actor: &Actor, app: &App, release: Uuid) -> Result<Value> {
        response(
            self.request(&actor.token, Method::GET, &app.permissions())
                .query(&[("releaseId", release.to_string())])
                .send()
                .await?,
            200,
        )
        .await
    }
    async fn update(
        &self,
        actor: &Actor,
        app: &App,
        release: Uuid,
        revision: u64,
        permissions: &[String],
    ) -> Result<Response> {
        Ok(self
            .request(&actor.token, Method::PUT, &app.permissions())
            .json(
                &json!({"releaseId":release,"expectedRevision":revision,"permissions":permissions}),
            )
            .send()
            .await?)
    }
    async fn replace(
        &self,
        actor: &Actor,
        app: &App,
        release: Uuid,
        permissions: &[String],
    ) -> Result<Value> {
        let current = self.read(actor, app, release).await?;
        let revision = current["revision"]
            .as_u64()
            .context("missing approval revision")?;
        response(
            self.update(actor, app, release, revision, permissions)
                .await?,
            200,
        )
        .await
    }
    async fn call(&self, token: &str, source: &App, target: &App) -> Result<()> {
        let value = response(
            self.request(token, Method::GET, &source.connection(target))
                .send()
                .await?,
            200,
        )
        .await?;
        ensure!(
            value["verified"] == true,
            "backend did not verify the core-signed identity"
        );
        Ok(())
    }
    async fn pending_write(
        &self,
        actor: &Actor,
        source: &App,
        target: &App,
    ) -> Result<tokio::net::TcpStream> {
        self.pending_body(
            &actor.token,
            source.release,
            &source.connection(target),
            None,
        )
        .await
    }
    async fn pending_body(
        &self,
        token: &str,
        release: Uuid,
        path: &str,
        instance: Option<Uuid>,
    ) -> Result<tokio::net::TcpStream> {
        let base = url::Url::parse(self.base)?;
        ensure!(
            base.scheme() == "http" && base.host_str() == Some("127.0.0.1"),
            "streaming probe requires the isolated loopback HTTP fixture"
        );
        let port = base.port().context("fixture API port is missing")?;
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
        // Hyper emits 100 Continue when the handler first polls the body. This
        // barrier establishes an actual in-flight body read without a sleep or
        // exposing the credential in subprocess arguments, logs, or evidence.
        let extra = instance
            .map(|id| format!("X-Miy-Preview-Instance: {id}\r\n"))
            .unwrap_or_default();
        let request = format!(
            "POST {} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {}\r\nX-Miy-Release: {}\r\n{extra}Content-Type: application/json\r\nContent-Length: 2\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n",
            path, token, release
        );
        stream.write_all(request.as_bytes()).await?;
        let mut interim = Vec::new();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !interim.ends_with(b"\r\n\r\n") {
                ensure!(interim.len() < 4096, "oversized interim fixture response");
                interim.push(stream.read_u8().await?);
            }
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("core did not begin reading the held request body")??;
        ensure!(
            interim.starts_with(b"HTTP/1.1 100 "),
            "fixture did not receive the HTTP body-read barrier"
        );
        Ok(stream)
    }
    async fn mcp(&self, actor: &Actor, tool: &str, arguments: Value) -> Result<Value> {
        let init=self.request(&actor.token,Method::POST,"/mcp")
            .header("Host","127.0.0.1:18080")
            .header("Accept","application/json, text/event-stream")
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"permission-audit","version":"1"}}}))
            .send().await?;
        let session = init.headers().get("mcp-session-id").cloned();
        let initialized = response(init, 200).await?;
        ensure!(
            initialized["result"]["protocolVersion"].is_string(),
            "MCP initialize failed"
        );
        let request = |body: Value| {
            let mut request = self
                .request(&actor.token, Method::POST, "/mcp")
                .header("Host", "127.0.0.1:18080")
                .header("Accept", "application/json, text/event-stream")
                .header("MCP-Protocol-Version", "2025-11-25")
                .json(&body);
            if let Some(session) = &session {
                request = request.header("Mcp-Session-Id", session);
            }
            request
        };
        let notification = request(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .send()
            .await?;
        ensure!(
            notification.status().is_success(),
            "MCP initialized notification failed"
        );
        let value=response(request(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":tool,"arguments":arguments}})).send().await?,200).await?;
        ensure!(
            value["result"]["content"].is_array(),
            "MCP tool response missing content"
        );
        let text = value["result"]["content"][0]["text"]
            .as_str()
            .context("MCP tool missing text result")?;
        Ok(
            json!({"isError":value["result"]["isError"].as_bool().unwrap_or(false),"data":serde_json::from_str::<Value>(text)?}),
        )
    }
}

pub async fn run(
    state: &State,
    client: &Client,
    base: &str,
    dev_portal: &str,
    frames: &str,
) -> Result<Vec<String>> {
    let mut h = Harness {
        state,
        client,
        base,
        backends: Vec::new(),
    };
    let admin = h.actor(true).await?;
    let user = h.actor(false).await?;
    let suffix = &Uuid::new_v4().simple().to_string()[..10];
    let b_name = format!("permission-b-{suffix}");
    let c_name = format!("permission-c-{suffix}");
    let a_permissions = vec![
        "platform:directory:read".into(),
        format!("app:{b_name}:read"),
        format!("app:{b_name}:write"),
        format!("app:{c_name}:read"),
    ];
    let b_permissions = vec![
        "platform:directory:read".into(),
        format!("app:{c_name}:read"),
    ];
    let directory_permission = vec!["platform:directory:read".into()];
    let a = h
        .app(format!("permission-a-{suffix}"), "dev", &a_permissions)
        .await?;
    let b = h.app(b_name, "dev", &b_permissions).await?;
    let c = h.app(c_name, "dev", &directory_permission).await?;
    for app in [&a, &b, &c] {
        h.grant(&user, app, "dev").await?;
    }
    let mut passed = Vec::new();
    // The following groups deliberately use public HTTP entrypoints, including
    // synthetic access-token authentication; no product API is replaced.
    denied(
        h.request(&user.token, Method::GET, &a.permissions())
            .send()
            .await?,
    )
    .await?;
    let initial = h.read(&admin, &a, a.release).await?;
    ensure!(
        initial["revision"] == 0,
        "new installation permission revision is not zero"
    );
    ensure!(
        initial["approvedPermissions"] == json!([]) && initial["history"] == json!([]),
        "new external approvals were inherited"
    );
    denied(
        h.request(&user.token, Method::GET, &a.directory())
            .send()
            .await?,
    )
    .await?;
    denied(
        h.request(&admin.token, Method::GET, &a.directory())
            .send()
            .await?,
    )
    .await?;
    denied(
        h.request(&user.token, Method::GET, &a.connection(&b))
            .send()
            .await?,
    )
    .await?;
    denied(h.update(&user, &a, a.release, 0, &a_permissions).await?).await?;
    ensure!(b.calls() == 0, "unapproved app call reached backend");
    passed.push(
        "installation permissions start empty and user/admin calls cannot bypass approval".into(),
    );

    let (first, second) = tokio::join!(
        h.update(&admin, &a, a.release, 0, &a_permissions),
        h.update(&admin, &a, a.release, 0, &a_permissions)
    );
    let first = first?;
    let second = second?;
    let statuses = [first.status().as_u16(), second.status().as_u16()];
    ensure!(
        statuses.contains(&200) && statuses.contains(&409),
        "approval CAS allowed two writers or lost both"
    );
    response(first, if statuses[0] == 200 { 200 } else { 409 }).await?;
    response(second, if statuses[1] == 200 { 200 } else { 409 }).await?;
    let approved = h.read(&admin, &a, a.release).await?;
    ensure!(
        approved["revision"] == 1,
        "approval revision did not advance exactly once"
    );
    ensure!(
        approved["history"].as_array().is_some_and(|v| v.len() == 1)
            && approved["approvedBy"] == admin.id
            && approved["history"][0]["actor"] == admin.id,
        "approval audit does not match committed writer"
    );
    response(
        h.request(&user.token, Method::GET, &a.directory())
            .send()
            .await?,
        200,
    )
    .await?;
    passed.push(
        "concurrent approval compare-and-swap accepts one writer and exposes current revision"
            .into(),
    );

    directory_contract(&h, &user, &a).await?;
    passed.push("app directory pages expose only bounded active user/group DTOs; literal search, scoped cursors, original-user membership and immediate group/account revocation are enforced".into());
    let full_size_transfer = std::env::var("MIY_RUNTIME_MAX_FILE").is_ok_and(|value| value == "1");
    storage_contract(&mut h, &admin, &user, full_size_transfer).await?;
    let storage_observation = "file storage enforces backend-only current approvals, original user authority, bounded immutable uploads that can be sent again after a failure, idempotent request lookup, attachment retention, explicit cancellation, reference-checked detach, administrator-set reservation quotas, resumable parts uploads (committed offset, interrupted tails truncated, gaps refused, hashed on completion, repeatable completion, cancellation), sharded storage with files of the old layout readable and moved by the worker, integrity checks that stop downloads of changed bytes, and housekeeping without a user actor";
    passed.push(if full_size_transfer {
        format!("{storage_observation}; a 2 GiB upload sent all 256 8 MiB ranges, matched the expected SHA-256, and was deleted")
    } else {
        storage_observation.into()
    });
    notification_contract(&mut h, &admin, &user)
        .await
        .context("notification contract")?;
    passed.push("notifications: only the app backend's call context with the approved permission publishes; recipients are filtered to active users who may use the app in that environment and who did not mute the kind; request IDs are idempotent; a group replaces, retracts and marks read only within its installation".into());

    for invalid in [
        vec!["database".into()],
        vec![format!("app:{}:write", c.app)],
        vec![a_permissions[0].clone(), a_permissions[0].clone()],
    ] {
        response(h.update(&admin, &a, a.release, 1, &invalid).await?, 400).await?;
    }
    response(h.update(&admin, &a, b.release, 0, &[]).await?, 400).await?;
    let outsider = h.actor(false).await?;
    denied(
        h.request(&outsider.token, Method::GET, &a.permissions())
            .send()
            .await?,
    )
    .await?;
    ensure!(
        h.read(&admin, &a, a.release).await?["revision"] == 1,
        "rejected approval changed revision"
    );
    let next_release = Uuid::new_v4();
    let mut next_permissions = a_permissions[..3].to_vec();
    next_permissions.push(format!("app:{}:write", c.app));
    sqlx::query("INSERT INTO releases(id,app_id,version,artifact_hash,manifest) SELECT $1,app_id,'2.0.0',artifact_hash,jsonb_set(jsonb_set(manifest,'{version}','\"2.0.0\"'::jsonb),'{requestedPermissions}',$2) FROM releases WHERE id=$3")
        .bind(next_release).bind(json!(next_permissions)).bind(a.release).execute(&state.pool).await?;
    ensure!(
        h.read(&admin, &a, next_release).await?["approvedPermissions"] == json!([]),
        "inactive unreleased manifest has an approval snapshot"
    );
    sqlx::query("UPDATE installations SET release_id=$2 WHERE id=$1")
        .bind(a.installation)
        .bind(next_release)
        .execute(&state.pool)
        .await?;
    let advanced = h.read(&admin, &a, next_release).await?;
    ensure!(
        advanced["unreviewedPermissions"] == json!([format!("app:{}:write", c.app)])
            && advanced["history"][0]["inheritedFromReleaseId"] == json!(a.release)
            && advanced["history"][0]["actor"].is_null()
            && advanced["approvedPermissions"]
                .as_array()
                .is_some_and(|p| p.len() == 3),
        "release activation did not carry the exact continuing approval"
    );
    response(
        h.request(&user.token, Method::GET, &a.directory())
            .send()
            .await?,
        200,
    )
    .await?;
    response(h.update(&admin, &a, a.release, 1, &[]).await?, 409).await?;
    h.replace(&admin, &a, next_release, &[a_permissions[0].clone()])
        .await?;
    response(
        h.request(&user.token, Method::GET, &a.directory())
            .send()
            .await?,
        200,
    )
    .await?;
    denied(
        h.request(&user.token, Method::GET, &a.connection(&b))
            .send()
            .await?,
    )
    .await?;
    sqlx::query("UPDATE installations SET release_id=$2 WHERE id=$1")
        .bind(a.installation)
        .bind(a.release)
        .execute(&state.pool)
        .await?;
    let rolled_back = h.read(&admin, &a, a.release).await?;
    ensure!(
        rolled_back["approvedPermissions"] == json!([&a_permissions[0]])
            && rolled_back["unreviewedPermissions"] == json!([&a_permissions[3]]),
        "rollback restored a revoked or removed permission"
    );
    denied(
        h.request(&user.token, Method::GET, &a.connection(&b))
            .send()
            .await?,
    )
    .await?;
    h.replace(&admin, &a, a.release, &a_permissions).await?;
    let prod = h.app(a.app.clone(), "prod", &a_permissions).await?;
    sqlx::query("UPDATE installations SET release_id=$2 WHERE id=$1")
        .bind(prod.installation)
        .bind(a.release)
        .execute(&state.pool)
        .await?;
    h.grant(&user, &prod, "prod").await?;
    h.grant(&user, &b, "prod").await?;
    denied(
        h.request(&user.token, Method::GET, &prod.directory())
            .send()
            .await?,
    )
    .await?;
    h.replace(&admin, &prod, a.release, &a_permissions).await?;
    response(
        h.request(&user.token, Method::GET, &prod.connection(&b))
            .send()
            .await?,
        404,
    )
    .await?;
    passed.push("approvals reject foreign releases and undeclared grants; release activation carries only continuing permissions; new requests, revocation, rollback and independent prod approval".into());

    h.call(&user.token, &a, &b).await?;
    let identity = b.last_identity()?;
    ensure!(
        identity["sub"] == user.id
            && identity["admin"] == false
            && identity["environment"] == "dev"
            && identity["installation_id"] == b.installation.to_string()
            && identity["release_id"] == b.release.to_string(),
        "signed destination identity changed original user or environment"
    );
    ensure!(
        identity["callers"].as_array().is_some_and(|v| v.len() == 1)
            && identity["callers"][0]["installation_id"] == a.installation.to_string()
            && identity["callers"][0]["release_id"] == a.release.to_string()
            && identity["callers"][0]["app_id"] == a.app,
        "source caller was not signed into destination context"
    );
    let before = b.calls();
    response(
        h.request(&user.token, Method::POST, &a.connection(&b))
            .header("x-miy-release", Uuid::new_v4().to_string())
            .json(&json!({}))
            .send()
            .await?,
        409,
    )
    .await?;
    ensure!(b.calls() == before, "stale source write reached target");
    let forged = response(
        h.request(&user.token, Method::POST, &a.connection(&b))
            .header("x-miy-release", a.release.to_string())
            .header("x-miy-identity", "forged")
            .header("x-miy-user", admin.id.as_str())
            .header("x-miy-environment", "prod")
            .json(&json!({}))
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        forged["verified"] == true
            && b.last_identity()?["sub"] == user.id
            && b.last_identity()?["environment"] == "dev",
        "untrusted caller headers changed signed authority"
    );
    sqlx::query("DELETE FROM grants WHERE user_id=$1 AND app_id=$2 AND environment='dev'")
        .bind(&user.id)
        .bind(&b.app)
        .execute(&state.pool)
        .await?;
    denied(
        h.request(&user.token, Method::GET, &a.connection(&b))
            .send()
            .await?,
    )
    .await?;
    h.grant(&user, &b, "dev").await?;
    h.replace(&admin, &a, a.release, &[]).await?;
    denied(
        h.request(&user.token, Method::GET, &a.connection(&b))
            .send()
            .await?,
    )
    .await?;
    denied(
        h.request(&admin.token, Method::GET, &a.connection(&b))
            .send()
            .await?,
    )
    .await?;
    h.replace(&admin, &a, a.release, &a_permissions).await?;
    let before = b.calls();
    let mut pending = h.pending_write(&user, &a, &b).await?;
    h.replace(&admin, &a, a.release, &[]).await?;
    pending.write_all(b"{}").await?;
    let mut denied_response = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(5),
        pending.read_to_end(&mut denied_response),
    )
    .await
    .context("held request did not terminate after its body completed")??;
    ensure!(
        denied_response.starts_with(b"HTTP/1.1 403 ") && b.calls() == before,
        "approval revoked during body read reached the target backend"
    );
    h.replace(&admin, &a, a.release, &a_permissions).await?;
    let preview_path = format!(
        "/api/v1/preview/apps/{}/connections/{}/api/probe",
        a.app, b.app
    );
    sqlx::query("INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,$2,'dev','develop') ON CONFLICT DO NOTHING")
        .bind(&user.id).bind(&a.app).execute(&state.pool).await?;
    denied(
        h.request(&user.token, Method::POST, &preview_path)
            .json(&json!({}))
            .send()
            .await?,
    )
    .await?;
    let instance = Uuid::new_v4();
    let proof = format!(
        "{}{}",
        crate::preview_auth::PREFIX,
        crate::identity::random()
    );
    let parent = hash(crate::identity::random());
    sqlx::query("INSERT INTO development_sessions(id_hash,user_id,csrf,expires_at,browser_purpose) VALUES($1,$2,$3,now()+interval '5 minutes','dev_portal')")
        .bind(&parent).bind(&user.id).bind(crate::identity::random()).execute(&state.pool).await?;
    sqlx::query("INSERT INTO preview_browser_logins(app_id,instance_id,host_user,cli_session,port,state,challenge,grant_hash,session_hash,development) VALUES($1,$2,$3,$4,23456,'fixture','fixture',$5,$6,true)")
        .bind(&a.app).bind(instance).bind(&user.id).bind(user.session).bind(hash(&proof)).bind(&parent).execute(&state.pool).await?;
    let preview = response(
        h.request(&proof, Method::POST, &preview_path)
            .header("x-miy-preview-instance", instance.to_string())
            .json(&json!({}))
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        preview["verified"] == true && b.last_identity()?.get("delegation_token").is_none(),
        "preview backend did not receive a signed identity without a continuation proof"
    );
    for boundary in ["development grant", "installation approval", "CLI session"] {
        let before = b.calls();
        let mut pending = h
            .pending_body(&proof, a.release, &preview_path, Some(instance))
            .await?;
        if boundary == "development grant" {
            sqlx::query("DELETE FROM grants WHERE user_id=$1 AND app_id=$2 AND environment='dev' AND permission='develop'")
                .bind(&user.id).bind(&a.app).execute(&state.pool).await?;
        } else if boundary == "CLI session" {
            sqlx::query("UPDATE cli_sessions SET revoked_at=clock_timestamp() WHERE id=$1")
                .bind(user.session)
                .execute(&state.pool)
                .await?;
        } else {
            h.replace(&admin, &a, a.release, &[]).await?;
        }
        pending.write_all(b"{}").await?;
        let mut result = Vec::new();
        tokio::time::timeout(Duration::from_secs(5), pending.read_to_end(&mut result))
            .await
            .context("held preview request did not finish")??;
        ensure!(
            result.starts_with(if boundary == "CLI session" {
                b"HTTP/1.1 401 "
            } else {
                b"HTTP/1.1 403 "
            }) && b.calls() == before,
            "preview {boundary} revoked during body read reached the target backend"
        );
        sqlx::query("INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,$2,'dev','develop') ON CONFLICT DO NOTHING")
            .bind(&user.id).bind(&a.app).execute(&state.pool).await?;
        if boundary == "CLI session" {
            sqlx::query("UPDATE cli_sessions SET revoked_at=NULL WHERE id=$1")
                .bind(user.session)
                .execute(&state.pool)
                .await?;
        }
        h.replace(&admin, &a, a.release, &a_permissions).await?;
    }
    sqlx::query("DELETE FROM grants WHERE user_id=$1 AND app_id=$2 AND environment='dev' AND permission='develop'")
        .bind(&user.id).bind(&a.app).execute(&state.pool).await?;
    passed.push("signed caller identity and stale-release writes fail closed; current user and approval revocations apply".into());

    h.replace(&admin, &b, b.release, &b_permissions).await?;
    h.replace(&admin, &c, c.release, &directory_permission)
        .await?;
    h.call(&user.token, &a, &b).await?;
    let b_proof = b.proof()?;
    h.call(&b_proof, &b, &c).await?;
    let c_proof = c.proof()?;
    let identity = c.last_identity()?;
    ensure!(
        identity["sub"] == user.id
            && identity["environment"] == "dev"
            && identity["admin"] == false
            && identity["callers"].as_array().is_some_and(|v| v.len() == 2)
            && identity["callers"][0]["installation_id"] == a.installation.to_string()
            && identity["callers"][1]["installation_id"] == b.installation.to_string(),
        "multi-hop identity lost original subject or ordered source chain"
    );
    response(
        h.request(&c_proof, Method::GET, &c.directory())
            .send()
            .await?,
        200,
    )
    .await?;
    denied(
        h.request(&b_proof, Method::GET, &a.connection(&b))
            .send()
            .await?,
    )
    .await?;
    denied(
        h.request(&b_proof, Method::GET, "/api/v1/me")
            .send()
            .await?,
    )
    .await?;
    denied(
        h.request(&b_proof, Method::PUT, &b.permissions())
            .json(&json!({"releaseId":b.release,"expectedRevision":1,"permissions":b_permissions}))
            .send()
            .await?,
    )
    .await?;
    let bogus = format!("miy_call_{}", auth::random());
    denied(
        h.request(&bogus, Method::GET, &b.connection(&c))
            .send()
            .await?,
    )
    .await?;
    let a_without_edge: Vec<_> = a_permissions
        .iter()
        .filter(|permission| **permission != format!("app:{}:read", b.app))
        .cloned()
        .collect();
    h.replace(&admin, &a, a.release, &a_without_edge).await?;
    denied(
        h.request(&b_proof, Method::GET, &b.connection(&c))
            .send()
            .await?,
    )
    .await?;
    denied(
        h.request(&c_proof, Method::GET, &c.directory())
            .send()
            .await?,
    )
    .await?;
    h.replace(&admin, &a, a.release, &a_permissions).await?;
    h.replace(&admin, &b, b.release, &directory_permission)
        .await?;
    denied(
        h.request(&c_proof, Method::GET, &c.directory())
            .send()
            .await?,
    )
    .await?;
    h.replace(&admin, &b, b.release, &b_permissions).await?;
    sqlx::query("DELETE FROM grants WHERE user_id=$1 AND app_id=$2 AND environment='dev'")
        .bind(&user.id)
        .bind(&a.app)
        .execute(&state.pool)
        .await?;
    denied(
        h.request(&c_proof, Method::GET, &c.directory())
            .send()
            .await?,
    )
    .await?;
    h.grant(&user, &a, "dev").await?;
    sqlx::query("UPDATE installations SET release_id=$2 WHERE id=$1")
        .bind(a.installation)
        .bind(next_release)
        .execute(&state.pool)
        .await?;
    denied(
        h.request(&c_proof, Method::GET, &c.directory())
            .send()
            .await?,
    )
    .await?;
    sqlx::query("UPDATE installations SET release_id=$2 WHERE id=$1")
        .bind(a.installation)
        .bind(a.release)
        .execute(&state.pool)
        .await?;
    let b_next_release = Uuid::new_v4();
    sqlx::query("INSERT INTO releases(id,app_id,version,artifact_hash,manifest) SELECT $1,app_id,'2.0.0',artifact_hash,jsonb_set(manifest,'{version}','\"2.0.0\"'::jsonb) FROM releases WHERE id=$2")
        .bind(b_next_release).bind(b.release).execute(&state.pool).await?;
    sqlx::query("UPDATE installations SET release_id=$2 WHERE id=$1")
        .bind(b.installation)
        .bind(b_next_release)
        .execute(&state.pool)
        .await?;
    denied(
        h.request(&b_proof, Method::GET, &b.directory())
            .send()
            .await?,
    )
    .await?;
    denied(
        h.request(&c_proof, Method::GET, &c.directory())
            .send()
            .await?,
    )
    .await?;
    sqlx::query("UPDATE installations SET release_id=$2 WHERE id=$1")
        .bind(b.installation)
        .bind(b.release)
        .execute(&state.pool)
        .await?;
    passed.push("opaque multi-hop proofs retain ordered identity and recheck every source grant, approval and release".into());

    // The target's wider authority must never widen its caller's capability.
    let narrow: Vec<_> = a_permissions
        .iter()
        .filter(|permission| **permission != format!("app:{}:read", c.app))
        .cloned()
        .collect();
    h.replace(&admin, &a, a.release, &narrow).await?;
    h.call(&user.token, &a, &b).await?;
    let narrow_proof = b.proof()?;
    denied(
        h.request(&narrow_proof, Method::GET, &b.connection(&c))
            .send()
            .await?,
    )
    .await?;
    response(
        h.request(&narrow_proof, Method::GET, &b.directory())
            .send()
            .await?,
        200,
    )
    .await?;
    h.replace(&admin, &a, a.release, &a_permissions).await?;
    // An issued narrow proof cannot gain newly approved authority later.
    denied(
        h.request(&narrow_proof, Method::GET, &b.connection(&c))
            .send()
            .await?,
    )
    .await?;
    sqlx::query("UPDATE app_call_tokens SET expires_at=clock_timestamp()-interval '1 second' WHERE token_hash=$1")
        .bind(hash(&narrow_proof)).execute(&state.pool).await?;
    denied(
        h.request(&narrow_proof, Method::GET, &b.directory())
            .send()
            .await?,
    )
    .await?;

    let names: Vec<_> = (0..9)
        .map(|index| format!("permission-hop-{index}-{suffix}"))
        .collect();
    let all_permissions: Vec<_> = std::iter::once("platform:directory:read".into())
        .chain(names.iter().map(|name| format!("app:{name}:read")))
        .collect();
    let mut hops = Vec::new();
    for name in names {
        let hop = h.app(name, "dev", &all_permissions).await?;
        h.grant(&user, &hop, "dev").await?;
        h.replace(&admin, &hop, hop.release, &all_permissions)
            .await?;
        hops.push(hop);
    }
    h.call(&user.token, &hops[0], &hops[1]).await?;
    denied(
        h.request(
            &hops[1].proof()?,
            Method::GET,
            &hops[1].connection(&hops[0]),
        )
        .send()
        .await?,
    )
    .await?;
    for index in 1..8 {
        h.call(&hops[index].proof()?, &hops[index], &hops[index + 1])
            .await?;
    }
    let final_identity = hops[8].last_identity()?;
    ensure!(
        final_identity["callers"]
            .as_array()
            .is_some_and(|v| v.len() == 8)
            && final_identity.get("delegation_token").is_none(),
        "maximum-hop identity issued further authority"
    );
    h.call(&user.token, &a, &b).await?;
    let template_hash = hash(b.proof()?);
    let fixture = Uuid::new_v4();
    let expired_hashes: Vec<_> = (0..3001)
        .map(|index| hash(format!("{fixture}:{index}")))
        .collect();
    let inserted=sqlx::query("INSERT INTO app_call_tokens(token_hash,user_id,cli_session_id,delegation_id,origin,installation_id,release_id,environment,callers,permissions_used,effective_permissions,created_at,expires_at) SELECT seed.token_hash,t.user_id,t.cli_session_id,t.delegation_id,t.origin,t.installation_id,t.release_id,t.environment,t.callers,t.permissions_used,t.effective_permissions,clock_timestamp()-interval '60 seconds',clock_timestamp()-interval '31 seconds' FROM app_call_tokens t CROSS JOIN unnest($2::text[]) AS seed(token_hash) WHERE t.token_hash=$1")
        .bind(template_hash).bind(&expired_hashes).execute(&state.pool).await?.rows_affected();
    ensure!(
        inserted == 3001,
        "expired-proof quota fixture was not completely seeded"
    );
    h.call(&user.token, &a, &b).await?;
    ensure!(
        b.proof()?.starts_with("miy_call_"),
        "expired proof backlog blocked a fresh continuation"
    );
    let remaining: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app_call_tokens WHERE user_id=$1 AND expires_at<=clock_timestamp()",
    )
    .bind(&user.id)
    .fetch_one(&state.pool)
    .await?;
    ensure!(
        remaining >= 1001,
        "quota probe did not retain an expired backlog beyond the cleanup batch"
    );
    passed.push("continuation authority narrows; expiry, cycles and eight-hop bounds hold while expired backlogs do not consume live quota".into());

    let use_only_mcp = h
        .mcp(
            &user,
            "get_installation_permissions",
            json!({"installationId":a.installation,"releaseId":a.release}),
        )
        .await?;
    ensure!(
        use_only_mcp["isError"] == true && use_only_mcp["data"]["error"]["code"] == "forbidden",
        "MCP exposed approval records to a use-only user"
    );
    let http = h.read(&admin, &a, a.release).await?;
    let mcp = h
        .mcp(
            &admin,
            "get_installation_permissions",
            json!({"installationId":a.installation,"releaseId":a.release}),
        )
        .await?;
    ensure!(
        mcp["isError"] == false && mcp["data"] == http,
        "MCP permission reader differs from HTTP policy"
    );
    let revision = http["revision"]
        .as_u64()
        .context("missing MCP comparison revision")?;
    let arguments = json!({"installationId":a.installation,"releaseId":a.release,"expectedRevision":revision,"permissions":a_permissions});
    let denied_mcp = h
        .mcp(&user, "set_installation_permissions", arguments.clone())
        .await?;
    ensure!(
        denied_mcp["isError"] == true && denied_mcp["data"]["error"]["code"] == "forbidden",
        "MCP allowed non-admin approval"
    );
    let approved_mcp = h
        .mcp(&admin, "set_installation_permissions", arguments)
        .await?;
    ensure!(
        approved_mcp["isError"] == false && approved_mcp["data"]["revision"] == revision + 1,
        "MCP administrator approval failed"
    );
    let stale=h.mcp(&admin,"set_installation_permissions",json!({"installationId":a.installation,"releaseId":a.release,"expectedRevision":revision,"permissions":[]})).await?;
    ensure!(
        stale["isError"] == true && stale["data"]["error"]["code"] == "permission_revision_changed",
        "MCP did not enforce approval CAS"
    );
    ensure!(
        h.read(&admin, &a, a.release).await? == approved_mcp["data"],
        "MCP write was not visible through HTTP"
    );
    passed.push("HTTP and MCP share permission reads, administrator checks, committed updates and CAS conflicts".into());

    h.call(&user.token, &a, &b).await?;
    let proof = b.proof()?;
    sqlx::query("UPDATE users SET enabled=false WHERE id=$1")
        .bind(&user.id)
        .execute(&state.pool)
        .await?;
    denied(
        h.request(&proof, Method::GET, &b.directory())
            .send()
            .await?,
    )
    .await?;
    sqlx::query("UPDATE users SET enabled=true WHERE id=$1")
        .bind(&user.id)
        .execute(&state.pool)
        .await?;
    // Disabling the owner revokes sessions permanently; re-enabling must not
    // reactivate the previously issued proof or the original access token.
    denied(
        h.request(&proof, Method::GET, &b.directory())
            .send()
            .await?,
    )
    .await?;
    denied(
        h.request(&user.token, Method::GET, &a.directory())
            .send()
            .await?,
    )
    .await?;
    let revoked: bool =
        sqlx::query_scalar("SELECT revoked_at IS NOT NULL FROM cli_sessions WHERE id=$1")
            .bind(user.session)
            .fetch_one(&state.pool)
            .await?;
    ensure!(
        revoked,
        "user disable did not revoke the proof's parent session"
    );
    // Explicit parent revocation is independent of account disable.
    h.call(&admin.token, &a, &b).await?;
    let admin_proof = b.proof()?;
    let before = b.calls();
    let mut pending = h.pending_write(&admin, &a, &b).await?;
    sqlx::query("UPDATE cli_sessions SET revoked_at=clock_timestamp(),revocation_reason='audit' WHERE id=$1")
        .bind(admin.session).execute(&state.pool).await?;
    pending.write_all(b"{}").await?;
    let mut held_response = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(5),
        pending.read_to_end(&mut held_response),
    )
    .await
    .context("held request did not terminate after parent revocation")??;
    ensure!(
        (held_response.starts_with(b"HTTP/1.1 401 ")
            || held_response.starts_with(b"HTTP/1.1 403 "))
            && b.calls() == before,
        "session revoked during body read reached the target backend"
    );
    denied(
        h.request(&admin_proof, Method::GET, &b.directory())
            .send()
            .await?,
    )
    .await?;
    // An administrator's portal session must not hand administrator status to
    // the destination or to any continuation hop of the same browser chain.
    let browser = h.actor(true).await?;
    for app in [&a, &b, &c] {
        h.grant(&browser, app, "dev").await?;
    }
    // These are dev installations: only a dev-portal session may use them. The
    // prod portal refuses the same browser chain before any app is called.
    let prod_session = auth::random();
    let prod_cookie = format!(
        "{}={prod_session}",
        state
            .auth
            .cookie_name(auth::BrowserPurpose::Portal, "development")
    );
    let prod_csrf = auth::random();
    sqlx::query("INSERT INTO development_sessions(id_hash,user_id,csrf,expires_at,browser_purpose) VALUES($1,$2,$3,now()+interval '5 minutes','portal')")
        .bind(hash(&prod_session)).bind(&browser.id).bind(&prod_csrf).execute(&state.pool).await?;
    let before = b.calls();
    denied(
        client
            .get(format!("{}{}", base, a.connection(&b)))
            .header("Cookie", &prod_cookie)
            .send()
            .await?,
    )
    .await?;
    denied(
        crate::app_frames_regression::launch_code(
            client,
            state,
            &crate::app_frames_regression::PortalSession {
                cookie: prod_session.clone(),
                csrf: prod_csrf,
                origin: base.to_owned(),
            },
            auth::BrowserPurpose::Portal,
            a.installation,
        )
        .await?,
    )
    .await?;
    ensure!(
        b.calls() == before,
        "the prod portal reached a dev installation"
    );
    let session = auth::random();
    let csrf = auth::random();
    let cookie = format!(
        "{}={session}",
        state
            .auth
            .cookie_name(auth::BrowserPurpose::DevPortal, "development")
    );
    sqlx::query("INSERT INTO development_sessions(id_hash,user_id,csrf,expires_at,browser_purpose) VALUES($1,$2,$3,now()+interval '5 minutes','dev_portal')")
        .bind(hash(&session)).bind(&browser.id).bind(&csrf).execute(&state.pool).await?;
    // Business apps run in frames: the dev portal session opens A's frame, and A's
    // frame calls B through its approved connection.
    let frame = crate::app_frames_regression::open(
        client,
        state,
        frames,
        &crate::app_frames_regression::PortalSession {
            cookie: session.clone(),
            csrf: csrf.clone(),
            origin: dev_portal.to_owned(),
        },
        auth::BrowserPurpose::DevPortal,
        a.installation,
    )
    .await?;
    let called = response(
        frame
            .request(
                Method::GET,
                &format!("/_miy/connections/{}/api/probe", b.app),
            )
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        called["verified"] == true,
        "browser app call lacked signed identity"
    );
    ensure!(
        b.last_identity()?["sub"] == browser.id && b.last_identity()?["admin"] == false,
        "portal browser call carried administrator status"
    );
    let browser_proof = b.proof()?;
    h.call(&browser_proof, &b, &c).await?;
    ensure!(
        c.last_identity()?["sub"] == browser.id && c.last_identity()?["admin"] == false,
        "browser-originated continuation carried administrator status"
    );
    let browser_child = c.proof()?;
    response(
        client
            .post(format!("{dev_portal}/auth/logout"))
            .header("Cookie", &cookie)
            .header("x-miy-csrf", &csrf)
            .header("Origin", dev_portal)
            .send()
            .await?,
        200,
    )
    .await?;
    denied(
        h.request(&browser_proof, Method::GET, &b.directory())
            .send()
            .await?,
    )
    .await?;
    denied(
        h.request(&browser_child, Method::GET, &c.directory())
            .send()
            .await?,
    )
    .await?;
    let remains: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM development_sessions WHERE id_hash=$1)")
            .bind(hash(&session))
            .fetch_one(&state.pool)
            .await?;
    ensure!(!remains, "browser logout retained its source session");

    let legacy = format!("miy_cli_{}", auth::random());
    sqlx::query("INSERT INTO cli_tokens(token_hash,user_id) VALUES($1,$2)")
        .bind(hash(&legacy))
        .bind(&browser.id)
        .execute(&state.pool)
        .await?;
    h.call(&legacy, &a, &b).await?;
    let legacy_proof = b.proof()?;
    h.call(&legacy_proof, &b, &c).await?;
    let legacy_child = c.proof()?;
    response(
        h.request(&legacy, Method::POST, "/api/v1/cli/logout")
            .send()
            .await?,
        200,
    )
    .await?;
    denied(
        h.request(&legacy_proof, Method::GET, &b.directory())
            .send()
            .await?,
    )
    .await?;
    denied(
        h.request(&legacy_child, Method::GET, &c.directory())
            .send()
            .await?,
    )
    .await?;
    passed.push("user disable and CLI/browser/legacy logout revoke issued continuations, including held requests and child proofs".into());
    groups(&mut h, suffix, &mut passed).await?;
    // Independent fixtures: a failing check here does not hide the others.
    let mut stopped = Vec::new();
    if let Err(error) = admin_user_directory(&h, suffix).await {
        stopped.push(format!("administrator user directory: {error:#}"));
    } else {
        passed.push("administrator user directory scopes literal search, organization and role filters, and cursor continuation".into());
    }
    if let Err(error) = policy_snapshot(&mut h, suffix, &mut passed).await {
        stopped.push(format!("policy snapshot: {error:#}"));
    }
    if let Err(error) = policy_matrix(&mut h, suffix, &mut passed).await {
        stopped.push(format!("policy matrix: {error:#}"));
    }
    if let Err(error) = disabled_app_approvals(&mut h, suffix, &mut passed).await {
        stopped.push(format!("approvals of a disabled app: {error:#}"));
    }
    if let Err(error) = administrator_write_rechecks(&h, suffix, &mut passed).await {
        stopped.push(format!("administrator write rechecks: {error:#}"));
    }
    if let Err(error) = app_write_revocations(&mut h, suffix, &mut passed).await {
        stopped.push(format!("app write revocations: {error:#}"));
    }
    if let Err(error) = admin_overview(&mut h, suffix).await {
        stopped.push(format!("administrator overview: {error:#}"));
    } else {
        passed.push("administrator overview pages outstanding approval decisions and runtime issues, distinguishes partial and empty decisions, and denies ordinary users".into());
    }
    ensure!(stopped.is_empty(), "{}", stopped.join("; "));
    Ok(passed)
}

async fn admin_overview(h: &mut Harness<'_>, suffix: &str) -> Result<()> {
    const PATH: &str = "/api/v1/admin/overview";
    let admin = h.actor(true).await?;
    let user = h.actor(false).await?;
    response(h.request(&user.token, Method::GET, PATH).send().await?, 403).await?;
    let before = response(
        h.request(&admin.token, Method::GET, PATH).send().await?,
        200,
    )
    .await?;
    let external = "platform:directory:read".to_owned();
    let app = h
        .app(
            format!("overview-app-{suffix}"),
            "dev",
            &[
                "database".into(),
                external.clone(),
                "platform:files:read".into(),
            ],
        )
        .await?;
    sqlx::query("UPDATE releases SET image_id=$2 WHERE id=$1")
        .bind(app.release)
        .bind(format!("sha256:{}", hash(suffix)))
        .execute(&h.state.pool)
        .await?;
    sqlx::query("UPDATE installations SET runtime_status='failed' WHERE id=$1")
        .bind(app.installation)
        .execute(&h.state.pool)
        .await?;
    sqlx::query("INSERT INTO app_registrations(id,app_id,name,actor) VALUES($1,$2,'Overview pending request',$3)")
        .bind(Uuid::new_v4()).bind(format!("overview-pending-{suffix}")).bind(&user.id).execute(&h.state.pool).await?;
    let after = response(
        h.request(&admin.token, Method::GET, PATH).send().await?,
        200,
    )
    .await?;
    for key in ["pendingRegistrations", "pendingApprovals", "runtimeIssues"] {
        ensure!(
            after[key].as_u64() == before[key].as_u64().map(|count| count + 1),
            "overview {key} did not count the new record"
        );
    }
    for runtime_status in ["unreachable", "stopped"] {
        sqlx::query("UPDATE installations SET runtime_status=$2 WHERE id=$1")
            .bind(app.installation)
            .bind(runtime_status)
            .execute(&h.state.pool)
            .await?;
        let summary = response(
            h.request(&admin.token, Method::GET, PATH).send().await?,
            200,
        )
        .await?;
        ensure!(
            summary["runtimeIssues"] == after["runtimeIssues"],
            "unexpected {runtime_status} was hidden from the overview"
        );
    }
    sqlx::query("UPDATE installations SET desired_state='stopped' WHERE id=$1")
        .bind(app.installation)
        .execute(&h.state.pool)
        .await?;
    h.replace(&admin, &app, app.release, std::slice::from_ref(&external))
        .await?;
    let approved = response(
        h.request(&admin.token, Method::GET, PATH).send().await?,
        200,
    )
    .await?;
    ensure!(
        approved["pendingApprovals"] == before["pendingApprovals"],
        "installation stayed pending after a partial approval decision"
    );
    ensure!(
        approved["runtimeIssues"] == before["runtimeIssues"],
        "an intentional stop was reported as an issue"
    );

    let mut fixtures = Vec::new();
    for index in 0..6 {
        let key = format!("overview-page-{suffix}-{index}");
        let app = h
            .app(key.clone(), "dev", std::slice::from_ref(&external))
            .await?;
        sqlx::query("UPDATE releases SET image_id=$1 WHERE id=$2")
            .bind(format!("sha256:{}", hash(&key)))
            .bind(app.release)
            .execute(&h.state.pool)
            .await?;
        sqlx::query("UPDATE installations SET runtime_status='failed' WHERE id=$1")
            .bind(app.installation)
            .execute(&h.state.pool)
            .await?;
        fixtures.push((key, app));
    }
    let mut page = response(
        h.request(&admin.token, Method::GET, PATH).send().await?,
        200,
    )
    .await?;
    ensure!(
        page["pendingApprovals"].as_u64()
            == before["pendingApprovals"].as_u64().map(|count| count + 6)
    );
    ensure!(
        page["runtimeIssues"].as_u64() == before["runtimeIssues"].as_u64().map(|count| count + 6)
    );
    let approval_cursor = page["approvalNextCursor"]
        .as_str()
        .context("missing approval cursor")?;
    response(
        h.request(&admin.token, Method::GET, PATH)
            .query(&[("runtimeCursor", approval_cursor)])
            .send()
            .await?,
        400,
    )
    .await?;
    let mut approvals = std::collections::BTreeSet::new();
    let mut issues = std::collections::BTreeSet::new();
    let mut scan_approvals = true;
    let mut scan_issues = true;
    let mut params = std::collections::BTreeMap::new();
    for _ in 0..100 {
        for (active, items, cursor, param, seen) in [
            (
                &mut scan_approvals,
                "approvalExamples",
                "approvalNextCursor",
                "approvalCursor",
                &mut approvals,
            ),
            (
                &mut scan_issues,
                "runtimeExamples",
                "runtimeNextCursor",
                "runtimeCursor",
                &mut issues,
            ),
        ] {
            if !*active {
                continue;
            }
            let rows = page[items].as_array().context("missing overview page")?;
            ensure!(rows.len() <= 5);
            for row in rows {
                let app = row["appId"].as_str().context("missing overview app")?;
                let environment = row["environment"]
                    .as_str()
                    .context("missing overview environment")?;
                ensure!(
                    seen.insert(format!("{app}:{environment}")),
                    "overview repeated an installation"
                );
            }
            if let Some(next) = page[cursor].as_str() {
                params.insert(param, next.to_owned());
            } else {
                *active = false;
            }
        }
        if !scan_approvals && !scan_issues {
            break;
        }
        page = response(
            h.request(&admin.token, Method::GET, PATH)
                .query(&params)
                .send()
                .await?,
            200,
        )
        .await?;
    }
    ensure!(
        !scan_approvals && !scan_issues,
        "overview page traversal did not finish"
    );
    for (key, _) in &fixtures {
        let key = format!("{key}:dev");
        ensure!(
            approvals.contains(&key) && issues.contains(&key),
            "overview omitted a fixture"
        );
    }
    let (_, declined) = fixtures.first().unwrap();
    h.replace(&admin, declined, declined.release, &[]).await?;
    let reviewed = response(
        h.request(&admin.token, Method::GET, PATH).send().await?,
        200,
    )
    .await?;
    ensure!(
        reviewed["pendingApprovals"].as_u64()
            == before["pendingApprovals"].as_u64().map(|count| count + 5)
    );
    Ok(())
}

async fn storage_contract(
    h: &mut Harness<'_>,
    admin: &Actor,
    user: &Actor,
    full_size_transfer: bool,
) -> Result<()> {
    async fn fresh_proof(h: &Harness<'_>, user: &Actor, app: &App) -> Result<String> {
        let gateway = format!("/api/v1/installations/{}/files/probe", app.installation);
        response(
            h.request(&user.token, Method::GET, &gateway).send().await?,
            200,
        )
        .await?;
        app.proof()
    }

    async fn upload_maximum_file(
        h: &Harness<'_>,
        user: &Actor,
        app: &App,
        base: &str,
    ) -> Result<()> {
        const PART_SIZE: usize = 8 * 1024 * 1024;
        const FILE_SIZE: usize = 2 * 1024 * 1024 * 1024;
        // SHA-256 of 2 GiB of byte 0xa5, computed independently of Core.
        const EXPECTED_HASH: &str =
            "c5f2d2b3f3972213c61d8756a63742fa61e3685b9c8b459430b571365c12d94b";
        let mut proof = fresh_proof(h, user, app).await?;
        let reservation = h.json(
            &proof, Method::POST, base,
            json!({"requestId":Uuid::new_v4(),"name":"max-size.bin","mime":"application/octet-stream","size":FILE_SIZE,"transfer":"parts"}),
            200,
        ).await?;
        let path = format!(
            "{base}/{}",
            reservation["id"].as_str().context("full-size file id")?
        );
        let part = axum::body::Bytes::from(vec![0xa5; PART_SIZE]);
        for (index, start) in (0..FILE_SIZE).step_by(PART_SIZE).enumerate() {
            if index != 0 && index.is_multiple_of(8) {
                proof = fresh_proof(h, user, app).await?;
            }
            let end = start + PART_SIZE - 1;
            let result = response(
                h.request(&proof, Method::PUT, &format!("{path}/parts"))
                    .header("content-range", format!("bytes {start}-{end}/{FILE_SIZE}"))
                    .body(part.clone())
                    .send()
                    .await?,
                200,
            )
            .await
            .with_context(|| format!("full-size file part {}", index + 1))?;
            ensure!(
                result["received"] == end + 1,
                "full-size transfer committed the wrong offset"
            );
        }
        let ready = tokio::time::timeout(Duration::from_secs(135), async {
            loop {
                let proof = fresh_proof(h, user, app).await?;
                let reply = h
                    .request(&proof, Method::POST, &format!("{path}/complete"))
                    .send()
                    .await?;
                if reply.status().as_u16() != 202 {
                    break response(reply, 200).await;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        })
        .await
        .context("full-size file was not hashed before the deadline")??;
        ensure!(
            ready["status"] == "ready"
                && ready["size"] == FILE_SIZE
                && ready["sha256"] == EXPECTED_HASH,
            "full-size file did not complete with the expected hash"
        );
        let proof = fresh_proof(h, user, app).await?;
        let cancelled =
            response(h.request(&proof, Method::DELETE, &path).send().await?, 200).await?;
        ensure!(
            cancelled["status"] == "cancelled",
            "full-size file was not cleaned up"
        );
        Ok(())
    }

    let permissions = vec!["platform:files:read".into(), "platform:files:write".into()];
    let app = h
        .app(
            format!("files-{}", &Uuid::new_v4().simple().to_string()[..12]),
            "dev",
            &permissions,
        )
        .await?;
    h.grant(user, &app, "dev").await?;
    let base = format!("/api/v1/installations/{}/platform/files", app.installation);
    let gateway = format!("/api/v1/installations/{}/files/probe", app.installation);
    let reservation = |id: Uuid, name: &str, size: usize| json!({"requestId":id,"name":name,"mime":"text/plain","size":size});
    // Even a valid ordinary CLI credential must enter its owning app ACL first.
    for token in [&user.token, &admin.token] {
        denied(
            h.request(token, Method::POST, &base)
                .json(&reservation(Uuid::new_v4(), "blocked.txt", 1))
                .send()
                .await?,
        )
        .await?;
    }
    response(
        h.request(&user.token, Method::GET, &gateway).send().await?,
        200,
    )
    .await?;
    ensure!(
        app.last_identity()?.get("delegation_token").is_none(),
        "an app without approved permissions received a continuation proof"
    );
    // Empty effective permissions intentionally issue no proof. Grant read only
    // to obtain a real limited proof, then prove it cannot reserve/write bytes.
    h.replace(admin, &app, app.release, &["platform:files:read".into()])
        .await?;
    response(
        h.request(&user.token, Method::GET, &gateway).send().await?,
        200,
    )
    .await?;
    denied(
        h.request(&app.proof()?, Method::POST, &base)
            .json(&reservation(Uuid::new_v4(), "unapproved.txt", 1))
            .send()
            .await?,
    )
    .await?;
    h.replace(admin, &app, app.release, &permissions).await?;
    response(
        h.request(&user.token, Method::GET, &gateway).send().await?,
        200,
    )
    .await?;
    let proof = app.proof()?;
    let request_id = Uuid::new_v4();
    let input = reservation(request_id, "검토.txt", 5);
    let first = h
        .json(&proof, Method::POST, &base, input.clone(), 200)
        .await?;
    let duplicate = h.json(&proof, Method::POST, &base, input, 200).await?;
    ensure!(
        first["id"] == duplicate["id"] && first["status"] == "pending" && first["sha256"].is_null(),
        "file reservation was not idempotent"
    );
    h.json(
        &proof,
        Method::POST,
        &base,
        reservation(request_id, "different.txt", 5),
        409,
    )
    .await?;
    let lookup = response(
        h.request(
            &proof,
            Method::GET,
            &format!("{base}/by-request/{request_id}"),
        )
        .send()
        .await?,
        200,
    )
    .await?;
    ensure!(
        lookup["id"] == first["id"],
        "lost reservation cannot be looked up by request id"
    );
    let file = first["id"].as_str().context("stored file id")?;
    let path = format!("{base}/{file}");
    let uploaded = response(
        h.request(&proof, Method::PUT, &format!("{path}/content"))
            .body("hello")
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        uploaded["status"] == "ready"
            && uploaded["sha256"] == hash("hello")
            && uploaded["expiresAt"].is_null(),
        "ready file metadata or retention is invalid"
    );
    let download = h
        .request(&proof, Method::GET, &format!("{path}/content"))
        .send()
        .await?;
    ensure!(
        download.status().as_u16() == 200
            && download
                .headers()
                .get("content-security-policy")
                .is_some_and(|v| v.to_str().is_ok_and(|s| s.starts_with("sandbox")))
            && download
                .headers()
                .get("x-content-type-options")
                .is_some_and(|v| v == "nosniff")
            && download
                .headers()
                .get("content-disposition")
                .is_some_and(|v| v.to_str().is_ok_and(|s| s.starts_with("attachment;")))
            && !download.headers().contains_key("set-cookie")
            && !download.headers().contains_key("clear-site-data"),
        "file response headers are unsafe"
    );
    ensure!(
        download.bytes().await?.as_ref() == b"hello",
        "stored file bytes changed"
    );
    response(
        h.request(&proof, Method::PUT, &format!("{path}/content"))
            .body("hello")
            .send()
            .await?,
        409,
    )
    .await?;
    // Use a real 100-Continue barrier to revoke approval after upload started,
    // before any bytes are completed. No timing sleep or credential output.
    let held = h
        .json(
            &proof,
            Method::POST,
            &base,
            reservation(Uuid::new_v4(), "held.txt", 2),
            200,
        )
        .await?;
    let held_id = held["id"].as_str().context("held file id")?;
    let parsed_base = url::Url::parse(h.base)?;
    ensure!(
        parsed_base.scheme() == "http" && parsed_base.host_str() == Some("127.0.0.1"),
        "storage fixture requires isolated loopback server"
    );
    let port = parsed_base.port().context("storage fixture port")?;
    let mut held_stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
    let request = format!(
        "PUT {base}/{held_id}/content HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {proof}\r\nContent-Length: 2\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n"
    );
    held_stream.write_all(request.as_bytes()).await?;
    let mut interim = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !interim.ends_with(b"\r\n\r\n") {
            ensure!(interim.len() < 4096, "oversized file interim response");
            interim.push(held_stream.read_u8().await?);
        }
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("file upload did not begin body read")??;
    ensure!(
        interim.starts_with(b"HTTP/1.1 100 "),
        "file upload body barrier missing"
    );
    h.replace(admin, &app, app.release, &[]).await?;
    held_stream.write_all(b"xy").await?;
    let mut rejected = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(5),
        held_stream.read_to_end(&mut rejected),
    )
    .await??;
    ensure!(
        rejected.starts_with(b"HTTP/1.1 403 "),
        "file upload committed after approval revocation"
    );
    h.replace(admin, &app, app.release, &permissions).await?;
    let not_ready = response(
        h.request(&proof, Method::GET, &format!("{base}/{held_id}"))
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        not_ready["status"] == "pending" && not_ready["sha256"].is_null(),
        "revoked upload became ready"
    );
    // The failed attempt left the internal `uploading` state behind. The same
    // reservation must accept the bytes again instead of answering 409 forever.
    let retried = response(
        h.request(&proof, Method::PUT, &format!("{base}/{held_id}/content"))
            .body("xy")
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        retried["status"] == "ready" && retried["sha256"] == hash("xy"),
        "a failed upload could not be sent again"
    );
    response(
        h.request(&proof, Method::DELETE, &format!("{base}/{held_id}"))
            .send()
            .await?,
        200,
    )
    .await?;
    let attach_id = Uuid::new_v4();
    let attach = json!({"requestId":attach_id,"reference":"document:fixture"});
    let attached = h
        .json(
            &proof,
            Method::POST,
            &format!("{path}/attach"),
            attach.clone(),
            200,
        )
        .await?;
    ensure!(
        attached["status"] == "attached",
        "app reference did not attach"
    );
    h.json(&proof, Method::POST, &format!("{path}/attach"), attach, 200)
        .await?;
    h.json(
        &proof,
        Method::POST,
        &format!("{path}/attach"),
        json!({"requestId":Uuid::new_v4(),"reference":"different"}),
        409,
    )
    .await?;
    response(h.request(&proof, Method::DELETE, &path).send().await?, 409).await?;
    let old = h
        .json(
            &proof,
            Method::POST,
            &base,
            reservation(Uuid::new_v4(), "unfinished.txt", 3),
            200,
        )
        .await?;
    let old_id = Uuid::parse_str(old["id"].as_str().context("unfinished file")?)?;
    sqlx::query(
        "UPDATE app_files SET expires_at=clock_timestamp()-interval '1 second' WHERE id=$1",
    )
    .bind(old_id)
    .execute(&h.state.pool)
    .await?;
    let next = h
        .json(
            &proof,
            Method::POST,
            &base,
            reservation(Uuid::new_v4(), "cancel.txt", 3),
            200,
        )
        .await?;
    let expired = response(
        h.request(&proof, Method::GET, &format!("{base}/{old_id}"))
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        expired["status"] == "expired",
        "unfinished reservation was not reclaimed"
    );
    response(
        h.request(&proof, Method::PUT, &format!("{base}/{old_id}/content"))
            .body("old")
            .send()
            .await?,
        409,
    )
    .await?;
    let still_attached = response(h.request(&proof, Method::GET, &path).send().await?, 200).await?;
    ensure!(
        still_attached["status"] == "attached",
        "expiry removed an attached file"
    );
    let cancel_path = format!("{base}/{}", next["id"].as_str().context("cancel file")?);
    response(
        h.request(&proof, Method::PUT, &format!("{cancel_path}/content"))
            .body("bad!")
            .send()
            .await?,
        400,
    )
    .await?;
    response(
        h.request(&proof, Method::PUT, &format!("{cancel_path}/content"))
            .body("abc")
            .send()
            .await?,
        200,
    )
    .await?;
    let cancel_id = Uuid::parse_str(next["id"].as_str().context("cancel file id")?)?;
    // The file's shard: the first two hex digits of its ID.
    let store = h
        .state
        .artifacts
        .join(".files")
        .join(app.installation.to_string())
        .join(&cancel_id.simple().to_string()[..2]);
    let blob = store.join(format!("{cancel_id}.blob"));
    let saved = store.join(format!("{cancel_id}.saved"));
    tokio::fs::rename(&blob, &saved).await?;
    h.json(
        &proof,
        Method::POST,
        &format!("{cancel_path}/attach"),
        json!({"requestId":Uuid::new_v4(),"reference":"missing-bytes"}),
        503,
    )
    .await?;
    let unchanged: String = sqlx::query_scalar("SELECT status FROM app_files WHERE id=$1")
        .bind(cancel_id)
        .fetch_one(&h.state.pool)
        .await?;
    ensure!(unchanged == "ready", "missing bytes were attached");
    // Force cleanup to fail after the durable cancellation tombstone commits.
    tokio::fs::create_dir(&blob).await?;
    response(
        h.request(&proof, Method::DELETE, &cancel_path)
            .send()
            .await?,
        503,
    )
    .await?;
    let tombstone: String = sqlx::query_scalar("SELECT status FROM app_files WHERE id=$1")
        .bind(cancel_id)
        .fetch_one(&h.state.pool)
        .await?;
    ensure!(
        tombstone == "cancelling",
        "failed cleanup rolled cancellation back to ready"
    );
    h.json(
        &proof,
        Method::POST,
        &format!("{cancel_path}/attach"),
        json!({"requestId":Uuid::new_v4(),"reference":"cancelled-bytes"}),
        409,
    )
    .await?;
    tokio::fs::remove_dir(&blob).await?;
    tokio::fs::rename(&saved, &blob).await?;
    let cancelled = response(
        h.request(&proof, Method::DELETE, &cancel_path)
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        cancelled["status"] == "cancelled",
        "explicit app cancellation did not release reservation"
    );
    response(
        h.request(&proof, Method::GET, &format!("{cancel_path}/content"))
            .send()
            .await?,
        404,
    )
    .await?;
    // Installation scope is fixed by the opaque continuation, not by a chosen id.
    denied(
        h.request(
            &proof,
            Method::GET,
            &format!(
                "/api/v1/installations/{}/platform/files/{file}",
                Uuid::new_v4()
            ),
        )
        .send()
        .await?,
    )
    .await?;
    h.replace(admin, &app, app.release, &[]).await?;
    denied(h.request(&proof, Method::GET, &path).send().await?).await?;
    h.replace(admin, &app, app.release, &permissions).await?;
    response(
        h.request(&user.token, Method::GET, &gateway).send().await?,
        200,
    )
    .await?;
    let proof = app.proof()?;
    sqlx::query("DELETE FROM grants WHERE user_id=$1 AND app_id=$2 AND permission='use'")
        .bind(&user.id)
        .bind(&app.app)
        .execute(&h.state.pool)
        .await?;
    denied(h.request(&proof, Method::GET, &path).send().await?).await?;
    h.grant(user, &app, "dev").await?;
    // An attached file is released only by detach with the attached reference.
    // Cancel never deletes it, and detach never applies to an unattached file.
    let unattached = h
        .json(
            &fresh_proof(h, user, &app).await?,
            Method::POST,
            &base,
            reservation(Uuid::new_v4(), "unattached.txt", 2),
            200,
        )
        .await?;
    let unattached_path = format!(
        "{base}/{}",
        unattached["id"].as_str().context("unattached file id")?
    );
    response(
        h.request(
            &fresh_proof(h, user, &app).await?,
            Method::PUT,
            &format!("{unattached_path}/content"),
        )
        .body("ok")
        .send()
        .await?,
        200,
    )
    .await?;
    let not_attached = h
        .json(
            &fresh_proof(h, user, &app).await?,
            Method::POST,
            &format!("{unattached_path}/detach"),
            json!({"reference":"document:fixture"}),
            409,
        )
        .await?;
    ensure!(
        not_attached["error"]["code"] == "file_not_attached",
        "detach accepted a file that was never attached"
    );
    response(
        h.request(
            &fresh_proof(h, user, &app).await?,
            Method::DELETE,
            &unattached_path,
        )
        .send()
        .await?,
        200,
    )
    .await?;
    let wrong = h
        .json(
            &fresh_proof(h, user, &app).await?,
            Method::POST,
            &format!("{path}/detach"),
            json!({"reference":"document:other"}),
            409,
        )
        .await?;
    ensure!(
        wrong["error"]["code"] == "file_reference_conflict",
        "detach accepted a reference the file was not attached with"
    );
    let before_detach: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(size),0)::bigint FROM app_files WHERE installation_id=$1 AND status IN ('ready','attached','cancelling')",
    )
    .bind(app.installation)
    .fetch_one(&h.state.pool)
    .await?;
    for _ in 0..2 {
        let detached = h
            .json(
                &fresh_proof(h, user, &app).await?,
                Method::POST,
                &format!("{path}/detach"),
                json!({"reference":"document:fixture"}),
                200,
            )
            .await?;
        ensure!(
            detached["status"] == "cancelled" && detached["reference"].is_null(),
            "detach did not release the attached file"
        );
    }
    let after_detach: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(size),0)::bigint FROM app_files WHERE installation_id=$1 AND status IN ('ready','attached','cancelling')",
    )
    .bind(app.installation)
    .fetch_one(&h.state.pool)
    .await?;
    ensure!(
        before_detach - after_detach == 5
            && !tokio::fs::try_exists(
                h.state
                    .artifacts
                    .join(".files")
                    .join(app.installation.to_string())
                    .join(&file[..2])
                    .join(format!("{file}.blob"))
            )
            .await?,
        "detach kept the bytes or the quota of a released file"
    );
    response(
        h.request(
            &fresh_proof(h, user, &app).await?,
            Method::GET,
            &format!("{path}/content"),
        )
        .send()
        .await?,
        404,
    )
    .await?;
    // Exercise bytes above the unchanged JSON gateway's 4 MiB ceiling. Each
    // storage request receives a newly minted short-lived backend proof.
    let large_payload = vec![0xabu8; 5 * 1024 * 1024];
    let large_digest = hash(&large_payload);
    let large = h
        .json(
            &fresh_proof(h, user, &app).await?,
            Method::POST,
            &base,
            reservation(Uuid::new_v4(), "large-stream.bin", large_payload.len()),
            200,
        )
        .await?;
    let large_id = large["id"].as_str().context("large file id")?;
    let large_path = format!("{base}/{large_id}");
    let uploaded = response(
        h.request(
            &fresh_proof(h, user, &app).await?,
            Method::PUT,
            &format!("{large_path}/content"),
        )
        .body(large_payload)
        .send()
        .await?,
        200,
    )
    .await?;
    ensure!(
        uploaded["size"] == 5 * 1024 * 1024 && uploaded["sha256"] == large_digest,
        "5 MiB storage upload was truncated or changed"
    );
    let downloaded = h
        .request(
            &fresh_proof(h, user, &app).await?,
            Method::GET,
            &format!("{large_path}/content"),
        )
        .send()
        .await?;
    ensure!(
        downloaded.status().as_u16() == 200,
        "5 MiB storage download was rejected"
    );
    let bytes = downloaded.bytes().await?;
    ensure!(
        bytes.len() == 5 * 1024 * 1024 && hash(&bytes) == large_digest,
        "5 MiB storage download hash differs"
    );
    let cancelled = response(
        h.request(
            &fresh_proof(h, user, &app).await?,
            Method::DELETE,
            &large_path,
        )
        .send()
        .await?,
        200,
    )
    .await?;
    ensure!(
        cancelled["status"] == "cancelled",
        "5 MiB temporary file was not cancelled before quota tests"
    );
    if full_size_transfer {
        upload_maximum_file(h, user, &app, &base)
            .await
            .context("2 GiB storage acceptance")?;
    }
    let proof = fresh_proof(h, user, &app).await?;
    // Fixed-size reservations count before bytes arrive, so parallel upload starts
    // cannot exceed the installation quota (here an administrator-set 256 MiB).
    // Fixture rows never create disk bytes.
    sqlx::query("INSERT INTO app_file_quotas(installation_id,max_bytes,max_files,updated_by) VALUES($1,268435456,1000,$2)")
        .bind(app.installation)
        .bind(&admin.id)
        .execute(&h.state.pool)
        .await?;
    for _ in 0..25 {
        sqlx::query("INSERT INTO app_files(id,installation_id,request_id,created_by,input_hash,name,mime,size,status,expires_at) VALUES($1,$2,$3,$4,'quota-fixture','quota.bin','application/octet-stream',10485760,'pending',clock_timestamp()+interval '1 day')")
            .bind(Uuid::new_v4()).bind(app.installation).bind(Uuid::new_v4()).bind(&user.id).execute(&h.state.pool).await?;
    }
    let too_large = reservation(Uuid::new_v4(), "quota.bin", 7 * 1024 * 1024);
    let quota = h.json(&proof, Method::POST, &base, too_large, 409).await?;
    ensure!(
        quota["error"]["code"] == "file_quota_exceeded",
        "installation reserved-byte quota was not enforced"
    );
    sqlx::query("DELETE FROM app_files WHERE installation_id=$1 AND input_hash='quota-fixture'")
        .bind(app.installation)
        .execute(&h.state.pool)
        .await?;
    // Large transfer (M10): a `parts` reservation takes contiguous ranges from
    // its committed offset, truncates an interrupted tail, refuses gaps, and
    // publishes the bytes only on `complete`, after hashing them.
    let proof = fresh_proof(h, user, &app).await?;
    let parts = |id: Uuid, name: &str, size: usize| json!({"requestId":id,"name":name,"mime":"application/octet-stream","size":size,"transfer":"parts"});
    let put_part = |proof: &str, path: &str, range: &str, body: &'static [u8]| {
        h.request(proof, Method::PUT, &format!("{path}/parts"))
            .header("content-range", range)
            .body(body)
    };
    let large = h
        .json(
            &proof,
            Method::POST,
            &base,
            parts(Uuid::new_v4(), "large.bin", 10),
            200,
        )
        .await?;
    ensure!(
        large["transfer"] == "parts" && large["received"] == 0 && large["status"] == "pending",
        "a parts reservation did not record its transfer and offset"
    );
    let large_path = format!("{base}/{}", large["id"].as_str().context("large file id")?);
    let mismatch = response(
        h.request(&proof, Method::PUT, &format!("{large_path}/content"))
            .body("0123456789")
            .send()
            .await?,
        409,
    )
    .await?;
    let first_part = response(
        put_part(&proof, &large_path, "bytes 0-3/10", b"0123")
            .send()
            .await?,
        200,
    )
    .await?;
    let stale = response(
        put_part(&proof, &large_path, "bytes 0-3/10", b"0123")
            .send()
            .await?,
        409,
    )
    .await?;
    response(
        put_part(&proof, &large_path, "bytes 4-7/11", b"4567")
            .send()
            .await?,
        400,
    )
    .await?;
    let early = response(
        h.request(&proof, Method::POST, &format!("{large_path}/complete"))
            .send()
            .await?,
        409,
    )
    .await?;
    ensure!(
        mismatch["error"]["code"] == "file_transfer_mismatch"
            && first_part["received"] == 4
            && stale["error"]["code"] == "file_part_offset"
            && early["error"]["code"] == "file_parts_incomplete",
        "parts did not keep their offset, refuse a repeat or wait for every range"
    );
    // An interrupted attempt left bytes past the committed offset.
    let large_id = large["id"].as_str().context("large file id")?.to_owned();
    // Bytes sit in the shard named by the file ID's first two hex digits.
    let shard_of = |id: &str| {
        h.state
            .artifacts
            .join(".files")
            .join(app.installation.to_string())
            .join(&id[..2])
    };
    let large_part = shard_of(&large_id).join(format!("{large_id}.part"));
    {
        use tokio::io::AsyncWriteExt;
        let mut tail = tokio::fs::OpenOptions::new()
            .append(true)
            .open(&large_part)
            .await?;
        tail.write_all(b"garbage").await?;
    }
    response(
        put_part(&proof, &large_path, "bytes 4-7/10", b"4567")
            .send()
            .await?,
        200,
    )
    .await?;
    let last = response(
        put_part(&proof, &large_path, "bytes 8-9/10", b"89")
            .send()
            .await?,
        200,
    )
    .await?;
    let proof = fresh_proof(h, user, &app).await?;
    let pending = response(
        h.request(&proof, Method::POST, &format!("{large_path}/complete"))
            .send()
            .await?,
        202,
    )
    .await?;
    ensure!(pending["status"] == "pending" && pending["sha256"].is_null());
    let done = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let reply = h
                .request(&proof, Method::POST, &format!("{large_path}/complete"))
                .send()
                .await?;
            if reply.status().as_u16() != 202 {
                return response(reply, 200).await;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await??;
    let again = response(
        h.request(&proof, Method::POST, &format!("{large_path}/complete"))
            .send()
            .await?,
        200,
    )
    .await?;
    let expected = hash(b"0123456789");
    let body = h
        .request(&proof, Method::GET, &format!("{large_path}/content"))
        .send()
        .await?
        .bytes()
        .await?;
    ensure!(
        last["received"] == 10
            && done["status"] == "ready"
            && done["sha256"] == expected.as_str()
            && again["sha256"] == done["sha256"]
            && body.as_ref() == b"0123456789"
            && !tokio::fs::try_exists(&large_part).await?,
        "completed parts were not the uploaded bytes or completion was not repeatable"
    );
    // Missing committed bytes stop the upload; cancellation removes the part.
    let gap = h
        .json(
            &proof,
            Method::POST,
            &base,
            parts(Uuid::new_v4(), "gap.bin", 8),
            200,
        )
        .await?;
    let gap_id = gap["id"].as_str().context("gap file id")?.to_owned();
    let gap_path = format!("{base}/{gap_id}");
    response(
        put_part(&proof, &gap_path, "bytes 0-3/8", b"abcd")
            .send()
            .await?,
        200,
    )
    .await?;
    let gap_part = shard_of(&gap_id).join(format!("{gap_id}.part"));
    tokio::fs::remove_file(&gap_part).await?;
    let missing = response(
        put_part(&proof, &gap_path, "bytes 4-7/8", b"efgh")
            .send()
            .await?,
        503,
    )
    .await?;
    tokio::fs::write(&gap_part, b"abcd").await?;
    let cancelled_parts = response(
        h.request(&proof, Method::DELETE, &gap_path).send().await?,
        200,
    )
    .await?;
    ensure!(
        missing["error"]["code"] == "file_part_bytes_missing"
            && cancelled_parts["status"] == "cancelled"
            && !tokio::fs::try_exists(&gap_part).await?,
        "a gap in committed bytes was not refused or a cancelled part kept its bytes"
    );
    // Layout and integrity (M10): bytes of the layout before shards stay
    // readable and the worker moves them into their shard; the periodic check
    // records verified files and stops downloads of a file whose bytes changed.
    let proof = fresh_proof(h, user, &app).await?;
    let legacy = h
        .json(
            &proof,
            Method::POST,
            &base,
            reservation(Uuid::new_v4(), "legacy.txt", 6),
            200,
        )
        .await?;
    let legacy_id = legacy["id"].as_str().context("legacy file id")?.to_owned();
    let legacy_path = format!("{base}/{legacy_id}");
    response(
        h.request(&proof, Method::PUT, &format!("{legacy_path}/content"))
            .body("legacy")
            .send()
            .await?,
        200,
    )
    .await?;
    let installation_dir = h
        .state
        .artifacts
        .join(".files")
        .join(app.installation.to_string());
    let sharded = installation_dir
        .join(&legacy_id[..2])
        .join(format!("{legacy_id}.blob"));
    let old_place = installation_dir.join(format!("{legacy_id}.blob"));
    ensure!(
        tokio::fs::try_exists(&sharded).await?,
        "a new file was not written into its shard"
    );
    // Put it back where the layout before shards kept it.
    tokio::fs::rename(&sharded, &old_place).await?;
    let read_old = h
        .request(&proof, Method::GET, &format!("{legacy_path}/content"))
        .send()
        .await?
        .bytes()
        .await?;
    let moved = crate::storage::relocate(&h.state.pool, &h.state.artifacts)
        .await
        .map_err(|error| anyhow::anyhow!("relocation failed: {}", error.code))?;
    let read_new = h
        .request(&proof, Method::GET, &format!("{legacy_path}/content"))
        .send()
        .await?
        .bytes()
        .await?;
    ensure!(
        read_old.as_ref() == b"legacy"
            && moved >= 1
            && tokio::fs::try_exists(&sharded).await?
            && !tokio::fs::try_exists(&old_place).await?
            && read_new.as_ref() == b"legacy",
        "a file of the old layout was not readable or not moved into its shard"
    );
    // Only this file is due (others in the shared database count as checked),
    // and the byte budget covers exactly it.
    sqlx::query("UPDATE app_files SET verified_at=CASE WHEN id=$1 THEN NULL ELSE COALESCE(verified_at,clock_timestamp()) END")
        .bind(Uuid::parse_str(&legacy_id)?)
        .execute(&h.state.pool)
        .await?;
    let first = crate::storage::verify(&h.state.pool, &h.state.artifacts, 6)
        .await
        .map_err(|error| anyhow::anyhow!("verification failed: {}", error.code))?;
    let verified_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT verified_at FROM app_files WHERE id=$1")
            .bind(Uuid::parse_str(&legacy_id)?)
            .fetch_one(&h.state.pool)
            .await?;
    // Same length, other bytes: only the sum can tell.
    tokio::fs::write(&sharded, b"LEGACY").await?;
    // Only this file is due (others in the shared database count as checked),
    // and the byte budget covers exactly it.
    sqlx::query("UPDATE app_files SET verified_at=CASE WHEN id=$1 THEN NULL ELSE COALESCE(verified_at,clock_timestamp()) END")
        .bind(Uuid::parse_str(&legacy_id)?)
        .execute(&h.state.pool)
        .await?;
    let second = crate::storage::verify(&h.state.pool, &h.state.artifacts, 6)
        .await
        .map_err(|error| anyhow::anyhow!("verification failed: {}", error.code))?;
    let refused = response(
        h.request(&proof, Method::GET, &format!("{legacy_path}/content"))
            .send()
            .await?,
        409,
    )
    .await?;
    let corrupt_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit WHERE action='file.corrupt' AND target=$1 AND actor IS NULL",
    )
    .bind(format!("file:{legacy_id}"))
    .fetch_one(&h.state.pool)
    .await?;
    ensure!(
        first.verified >= 1
            && verified_at.is_some()
            && second.corrupt >= 1
            && refused["error"]["code"] == "file_corrupt"
            && corrupt_audits == 1,
        "the integrity check did not verify sums or did not stop a changed file"
    );
    // Elapsed reservations in five abandoned installations stop counting as
    // promised bytes when they elapse, before any housekeeping removes them.
    let mut abandoned = Vec::new();
    let mut abandoned_part = None;
    let mut stuck = None;
    for index in 0..5 {
        let other = h
            .app(
                format!(
                    "abandoned-{index}-{}",
                    &Uuid::new_v4().simple().to_string()[..8]
                ),
                "dev",
                &[],
            )
            .await?;
        let count = if index == 4 { 2 } else { 25 };
        for _ in 0..count {
            let file = Uuid::new_v4();
            sqlx::query("INSERT INTO app_files(id,installation_id,request_id,created_by,input_hash,name,mime,size,status,expires_at) VALUES($1,$2,$3,$4,'global-expiry-fixture','quota.bin','application/octet-stream',10485760,'pending',clock_timestamp()-interval '1 second')")
                .bind(file).bind(other.installation).bind(Uuid::new_v4()).bind(&user.id).execute(&h.state.pool).await?;
            let directory = h
                .state
                .artifacts
                .join(".files")
                .join(other.installation.to_string());
            if abandoned.is_empty() {
                tokio::fs::create_dir_all(&directory).await?;
                let path = directory.join(format!("{file}.part"));
                tokio::fs::write(&path, b"incomplete fixture bytes").await?;
                abandoned_part = Some(path);
            } else if abandoned.len() == 1 {
                // A directory where bytes belong makes this one removal fail.
                let path = directory.join(format!("{file}.blob"));
                tokio::fs::create_dir_all(&path).await?;
                stuck = Some((file, path));
            }
            abandoned.push(file);
        }
    }
    let (stuck_file, stuck_path) = stuck.context("stuck fixture file")?;
    let reclaimed = h
        .json(
            &proof,
            Method::POST,
            &base,
            reservation(Uuid::new_v4(), "after-expiry.bin", 8 * 1024 * 1024),
            200,
        )
        .await?;
    ensure!(
        reclaimed["status"] == "pending",
        "elapsed reservations of abandoned installations retained global quota"
    );
    let expired_count = |pool: sqlx::PgPool, files: Vec<Uuid>| async move {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app_files WHERE id=ANY($1) AND status='expired'",
        )
        .bind(files)
        .fetch_one(&pool)
        .await
    };
    // One removal that cannot succeed is reported and skipped. It neither stops
    // the other 101 files nor makes a reservation fail.
    let report = crate::storage::sweep(&h.state.pool, &h.state.artifacts)
        .await
        .map_err(|error| anyhow::anyhow!("file sweep failed: {}", error.code))?;
    let stuck_status: String = sqlx::query_scalar("SELECT status FROM app_files WHERE id=$1")
        .bind(stuck_file)
        .fetch_one(&h.state.pool)
        .await?;
    ensure!(
        report.failed == 1
            && stuck_status == "pending"
            && expired_count(h.state.pool.clone(), abandoned.clone()).await? == 101,
        "one failed removal stopped the sweep of the other files"
    );
    h.json(
        &proof,
        Method::POST,
        &base,
        reservation(Uuid::new_v4(), "during-failed-sweep.bin", 0),
        200,
    )
    .await?;
    tokio::fs::remove_dir(&stuck_path).await?;
    let report = crate::storage::sweep(&h.state.pool, &h.state.artifacts)
        .await
        .map_err(|error| anyhow::anyhow!("file sweep failed: {}", error.code))?;
    ensure!(
        report.failed == 0 && expired_count(h.state.pool.clone(), abandoned.clone()).await? == 102,
        "a recovered file was not swept on the next pass"
    );
    ensure!(
        !tokio::fs::try_exists(abandoned_part.context("abandoned fixture bytes")?).await?,
        "global expiry left incomplete bytes behind"
    );
    // Housekeeping is the platform's act, never the next caller's.
    let targets: Vec<String> = abandoned
        .iter()
        .map(|file| format!("file:{file}"))
        .collect();
    let (system, attributed): (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER(WHERE actor IS NULL), count(*) FILTER(WHERE actor IS NOT NULL) FROM audit WHERE action='file.expired' AND target=ANY($1)",
    )
    .bind(&targets)
    .fetch_one(&h.state.pool)
    .await?;
    ensure!(
        system == 102 && attributed == 0,
        "file expiry was attributed to a user"
    );
    Ok(())
}

async fn directory_contract(h: &Harness<'_>, caller: &Actor, app: &App) -> Result<()> {
    async fn get(
        h: &Harness<'_>,
        actor: &Actor,
        app: &App,
        query: &[(&str, &str)],
        expected: u16,
    ) -> Result<Value> {
        response(
            h.request(&actor.token, Method::GET, &app.directory())
                .query(query)
                .send()
                .await?,
            expected,
        )
        .await
    }

    let subject = h.actor(false).await?;
    h.grant(&subject, app, "dev").await?;
    let second = h.actor(false).await?;
    let third = h.actor(false).await?;
    let disabled = h.actor(false).await?;
    let literal = h.actor(false).await?;
    let marker = format!("directory-{}", Uuid::new_v4().simple());
    for actor in [&subject, &second, &third, &disabled] {
        sqlx::query("UPDATE users SET name=$2 WHERE id=$1")
            .bind(&actor.id)
            .bind(&marker)
            .execute(&h.state.pool)
            .await?;
    }
    sqlx::query("UPDATE users SET enabled=false WHERE id=$1")
        .bind(&disabled.id)
        .execute(&h.state.pool)
        .await?;
    sqlx::query("UPDATE users SET name=$2 WHERE id=$1")
        .bind(&literal.id)
        .bind(format!("literal-only-{} %_", Uuid::new_v4()))
        .execute(&h.state.pool)
        .await?;

    let first = get(h, &subject, app, &[("q", &marker), ("limit", "1")], 200).await?;
    let keys: Vec<_> = first
        .as_object()
        .context("directory page is not an object")?
        .keys()
        .map(String::as_str)
        .collect();
    ensure!(
        keys == [
            "environment",
            "groups",
            "installationId",
            "kind",
            "nextCursor",
            "subjectId",
            "users"
        ] && first["subjectId"] == subject.id
            && first["installationId"] == json!(app.installation)
            && first["environment"] == "dev"
            && first["kind"] == "users"
            && first["groups"] == json!([]),
        "directory response exposed fields outside its public DTO"
    );
    let mut seen = Vec::new();
    let mut page = first.clone();
    loop {
        let users = page["users"]
            .as_array()
            .context("directory users missing")?;
        ensure!(users.len() == 1, "directory page limit was ignored");
        let row = &users[0];
        ensure!(
            row.as_object().is_some_and(|row| row.len() == 3)
                && row["id"].is_string()
                && row["name"] == marker
                && row["primaryGroup"].is_null(),
            "directory user DTO leaked administrator, email, or grant information"
        );
        seen.push(
            row["id"]
                .as_str()
                .context("directory id missing")?
                .to_owned(),
        );
        let Some(cursor) = page["nextCursor"].as_str() else {
            break;
        };
        ensure!(seen.len() <= 3, "directory pagination did not terminate");
        page = get(
            h,
            &subject,
            app,
            &[("q", &marker), ("limit", "1"), ("cursor", cursor)],
            200,
        )
        .await?;
    }
    let mut expected = vec![subject.id.clone(), second.id.clone(), third.id.clone()];
    expected.sort();
    ensure!(
        seen == expected,
        "same-name directory cursor skipped or duplicated users"
    );
    let cursor = first["nextCursor"]
        .as_str()
        .context("directory cursor missing")?;
    get(h, caller, app, &[("q", &marker), ("cursor", cursor)], 400).await?;
    get(
        h,
        &subject,
        app,
        &[("q", "another"), ("cursor", cursor)],
        400,
    )
    .await?;
    for query in [
        vec![("userId", caller.id.as_str())],
        vec![("subjectId", caller.id.as_str())],
        vec![("limit", "0")],
        vec![("limit", "101")],
        vec![("kind", "memberships"), ("q", "query")],
        vec![("kind", "groups"), ("ids", "not-a-uuid")],
    ] {
        get(h, &subject, app, &query, 400).await?;
    }
    let filtered = get(h, &subject, app, &[("q", "%_")], 200).await?;
    ensure!(
        filtered["users"]
            .as_array()
            .is_some_and(|users| users.len() == 1)
            && filtered["users"][0]["id"] == literal.id,
        "directory literal percent/underscore search became a wildcard"
    );
    let ids = format!("{},{}", subject.id, disabled.id);
    let filtered = get(h, &subject, app, &[("ids", &ids)], 200).await?;
    ensure!(
        filtered["users"]
            .as_array()
            .is_some_and(|users| users.len() == 1)
            && filtered["users"][0]["id"] == subject.id,
        "candidate recheck exposed a disabled user"
    );

    let parent = Uuid::new_v4();
    let child = Uuid::new_v4();
    let local = Uuid::new_v4();
    let inactive = Uuid::new_v4();
    for (id, name, source, active, ancestor) in [
        (parent, "parent", "hr", true, None),
        (child, "child", "hr", true, Some(parent)),
        (local, "local", "local", true, None),
        (inactive, "inactive", "local", false, None),
    ] {
        sqlx::query("INSERT INTO groups(id,name,source,active,parent_id) VALUES($1,$2,$3,$4,$5)")
            .bind(id)
            .bind(format!("{marker}-{name}"))
            .bind(source)
            .bind(active)
            .bind(ancestor)
            .execute(&h.state.pool)
            .await?;
    }
    sqlx::query("UPDATE users SET primary_group_id=$2 WHERE id=$1")
        .bind(&subject.id)
        .bind(child)
        .execute(&h.state.pool)
        .await?;
    for group in [local, inactive] {
        sqlx::query("INSERT INTO group_members(group_id,user_id) VALUES($1,$2)")
            .bind(group)
            .bind(&subject.id)
            .execute(&h.state.pool)
            .await?;
    }
    let groups = get(h, &subject, app, &[("kind", "groups"), ("q", &marker)], 200).await?;
    ensure!(
        groups["users"] == json!([])
            && groups["groups"]
                .as_array()
                .is_some_and(|groups| groups.len() == 3
                    && groups
                        .iter()
                        .all(|group| group.as_object().is_some_and(|group| group.len() == 3))),
        "directory groups leaked inactive groups or administrative metadata"
    );
    let with_department = get(h, &subject, app, &[("ids", &subject.id)], 200).await?;
    ensure!(
        with_department["users"][0]["primaryGroup"]
            == json!({"id":child,"name":format!("{marker}-child")}),
        "directory primary HR group is missing or exposes extra fields"
    );
    let ids = format!("{parent},{child},{local},{inactive}");
    let query = [("kind", "memberships"), ("ids", ids.as_str())];
    let members = get(h, &subject, app, &query, 200).await?;
    let mut group_ids: Vec<_> = members["groups"]
        .as_array()
        .context("memberships groups missing")?
        .iter()
        .map(|group| group["id"].as_str().unwrap_or_default())
        .collect();
    group_ids.sort();
    let mut expected = vec![child.to_string(), local.to_string()];
    expected.sort();
    ensure!(
        group_ids == expected,
        "membership inherited an ancestor or inactive group"
    );
    let caller_members = get(h, caller, app, &query, 200).await?;
    ensure!(
        caller_members["groups"] == json!([]),
        "membership leaked a different user's groups"
    );

    sqlx::query("DELETE FROM group_members WHERE group_id=$1 AND user_id=$2")
        .bind(local)
        .bind(&subject.id)
        .execute(&h.state.pool)
        .await?;
    let after_local_revoke = get(h, &subject, app, &query, 200).await?;
    ensure!(
        after_local_revoke["groups"]
            .as_array()
            .is_some_and(|groups| groups.len() == 1)
            && after_local_revoke["groups"][0]["id"] == child.to_string(),
        "membership kept a removed local member"
    );
    sqlx::query("UPDATE groups SET active=false WHERE id=$1")
        .bind(child)
        .execute(&h.state.pool)
        .await?;
    ensure!(
        get(h, &subject, app, &query, 200).await?["groups"] == json!([]),
        "membership kept a disabled HR group"
    );
    ensure!(
        get(h, &subject, app, &[("ids", &subject.id)], 200).await?["users"][0]["primaryGroup"]
            .is_null(),
        "inactive primary group was exposed"
    );
    sqlx::query("UPDATE users SET enabled=false WHERE id=$1")
        .bind(&subject.id)
        .execute(&h.state.pool)
        .await?;
    denied(
        h.request(&subject.token, Method::GET, &app.directory())
            .query(&query)
            .send()
            .await?,
    )
    .await?;
    Ok(())
}

/// Neither the initial nor the final state permits this user to read/manage the app:
/// enabled without a grant -> disabled with a direct grant. Hold the grants relation
/// while a real HTTP list and the real worker recheck reach their policy reads, then
/// commit both changes together. Separate user/grant queries combine these states and
/// incorrectly allow both; one statement snapshot must deny both.
async fn policy_snapshot(
    h: &mut Harness<'_>,
    suffix: &str,
    passed: &mut Vec<String>,
) -> Result<()> {
    use crate::{delegations::Action, policy, runtime_contract::FailureCode};
    let actor = h.actor(false).await?;
    let app = h.app(format!("snapshot-{suffix}"), "dev", &[]).await?;
    ensure!(
        h.visible(&actor, &app).await?.is_none(),
        "initial app visibility"
    );
    ensure!(
        policy::recheck_queued(
            &h.state.pool,
            &actor.id,
            None,
            &app.app,
            "dev",
            Action::Start
        )
        .await
        .is_err(),
        "initial worker authority"
    );
    let mut gate = h.state.pool.begin().await?;
    let blocker: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *gate)
        .await?;
    sqlx::query("LOCK TABLE grants IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *gate)
        .await?;
    let request = h.request(
        &actor.token,
        Method::GET,
        &format!("/api/v1/installations?limit=200&appId={}", app.app),
    );
    let change = async {
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                let waiting: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM pg_stat_activity WHERE pid<>$1 AND $1=ANY(pg_blocking_pids(pid)) AND query LIKE '%effective_grants%'",
                )
                .bind(blocker)
                .fetch_one(&h.state.pool)
                .await?;
                if waiting >= 2 {
                    return Ok::<_, anyhow::Error>(());
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .context("HTTP list and worker recheck did not wait on policy grants")??;
        sqlx::query("UPDATE users SET enabled=false WHERE id=$1")
            .bind(&actor.id)
            .execute(&mut *gate)
            .await?;
        sqlx::query("INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,$2,'dev','develop')")
            .bind(&actor.id).bind(&app.app).execute(&mut *gate).await?;
        gate.commit().await?;
        Ok::<_, anyhow::Error>(())
    };
    let (http, queued, changed) = tokio::time::timeout(Duration::from_secs(15), async {
        tokio::join!(
            request.send(),
            policy::recheck_queued(
                &h.state.pool,
                &actor.id,
                None,
                &app.app,
                "dev",
                Action::Start
            ),
            change,
        )
    })
    .await
    .context("policy snapshot requests did not finish")?;
    changed?;
    let listing = response(http?, 200).await?;
    ensure!(
        listing["installations"]
            .as_array()
            .context("snapshot installation list")?
            .is_empty(),
        "HTTP list combined an old enabled flag with a new grant"
    );
    let failure = queued
        .err()
        .context("worker combined an old enabled flag with a new grant")?;
    ensure!(
        matches!(
            failure.code,
            FailureCode::UserDisabled | FailureCode::PermissionRevoked
        ),
        "snapshot denial was not an authority denial"
    );
    passed.push("HTTP visibility and worker rechecks never combine an old active user with a newly granted permission after disable".into());
    Ok(())
}

async fn admin_user_directory(h: &Harness<'_>, suffix: &str) -> Result<()> {
    let lead = h.actor(true).await?;
    let child_user = h.actor(false).await?;
    let unassigned = h.actor(false).await?;
    let wildcard_match = h.actor(false).await?;
    let root = Uuid::new_v4();
    let child = Uuid::new_v4();
    let name = format!("directory-{suffix} %_");
    sqlx::query("INSERT INTO groups (id, name, source, parent_id) VALUES ($1, 'Division', 'hr', NULL), ($2, 'Team', 'hr', $1)")
        .bind(root)
        .bind(child)
        .execute(&h.state.pool)
        .await?;
    for (actor, primary, enabled, can_develop) in [
        (&lead, Some(root), true, false),
        (&child_user, Some(child), true, true),
        (&unassigned, None, false, false),
    ] {
        sqlx::query(
            "UPDATE users SET name=$2, primary_group_id=$3, enabled=$4, can_develop=$5 WHERE id=$1",
        )
        .bind(&actor.id)
        .bind(&name)
        .bind(primary)
        .bind(enabled)
        .bind(can_develop)
        .execute(&h.state.pool)
        .await?;
    }
    sqlx::query("UPDATE users SET name=$2, primary_group_id=$3 WHERE id=$1")
        .bind(&wildcard_match.id)
        .bind(format!("directory-{suffix} XX"))
        .bind(root)
        .execute(&h.state.pool)
        .await?;
    const PATH: &str = "/api/v1/users/directory";
    let denied = h
        .request(&child_user.token, Method::GET, PATH)
        .send()
        .await?;
    response(denied, 403).await?;
    let read = |params: &[(&str, &str)]| {
        h.request(&lead.token, Method::GET, PATH)
            .query(&[("q", name.as_str())])
            .query(params)
            .send()
    };
    let root_id = root.to_string();
    let direct = response(read(&[("organization", &root_id)]).await?, 200).await?;
    ensure!(direct["items"].as_array().unwrap().len() == 1 && direct["items"][0]["id"] == lead.id);
    let first = response(
        read(&[
            ("organization", &root_id),
            ("descendants", "true"),
            ("limit", "1"),
        ])
        .await?,
        200,
    )
    .await?;
    let cursor = first["nextCursor"]
        .as_str()
        .context("missing user directory cursor")?;
    let second = response(
        read(&[
            ("organization", &root_id),
            ("descendants", "true"),
            ("cursor", cursor),
        ])
        .await?,
        200,
    )
    .await?;
    let mut ids = [lead.id.as_str(), child_user.id.as_str()];
    ids.sort_unstable();
    ensure!(
        first["items"][0]["id"] == ids[0]
            && second["items"][0]["id"] == ids[1]
            && second["nextCursor"].is_null()
    );
    response(
        read(&[("organization", &root_id), ("cursor", cursor)]).await?,
        400,
    )
    .await?;
    let developers = response(
        read(&[("role", "developer"), ("status", "enabled")]).await?,
        200,
    )
    .await?;
    ensure!(
        developers["items"].as_array().unwrap().len() == 1
            && developers["items"][0]["id"] == child_user.id
            && developers["items"][0]["primaryGroupName"] == "Team"
    );
    let inactive = response(
        read(&[
            ("organization", "unassigned"),
            ("role", "member"),
            ("status", "disabled"),
        ])
        .await?,
        200,
    )
    .await?;
    ensure!(
        inactive["items"].as_array().unwrap().len() == 1
            && inactive["items"][0]["id"] == unassigned.id
            && inactive["items"][0]["primaryGroupId"].is_null()
    );
    Ok(())
}

/// Groups and app access (V1-G1, V1-G2): permissions are computed from current
/// memberships on every request and never copied into user grants.
async fn groups(h: &mut Harness<'_>, suffix: &str, passed: &mut Vec<String>) -> Result<()> {
    let operator = h.actor(true).await?;
    let carol = h.actor(false).await?;
    let dave = h.actor(false).await?;
    let erin = h.actor(false).await?;
    let app = format!("permission-g-{suffix}");
    let dev = h.app(app.clone(), "dev", &[]).await?;
    let prod = h.app(app.clone(), "prod", &[]).await?;
    let harness: &Harness<'_> = h;
    let token = operator.token.as_str();
    let admin = move |method: Method, path: String, body: Value, expected: u16| async move {
        harness.json(token, method, &path, body, expected).await
    };

    let local = admin(
        Method::POST,
        "/api/v1/groups".into(),
        json!({"name":format!("로컬 {suffix}"),"source":"local"}),
        200,
    )
    .await?;
    let local = local["id"].as_str().context("local group id")?.to_owned();
    admin(
        Method::PUT,
        "/api/v1/group-grants".into(),
        json!({"groupId":local,"appId":app,"environment":"dev","permission":"use","enabled":true}),
        200,
    )
    .await?;
    ensure!(
        h.visible(&carol, &dev).await?.is_none(),
        "group grant reached a non-member"
    );
    admin(
        Method::PUT,
        format!("/api/v1/groups/{local}/members"),
        json!({"userId":carol.id,"member":true}),
        200,
    )
    .await?;
    ensure!(
        h.visible(&carol, &dev).await?.is_some(),
        "local member did not receive use"
    );
    ensure!(
        h.visible(&carol, &prod).await?.is_none(),
        "dev group grant reached prod"
    );
    admin(
        Method::PUT,
        "/api/v1/group-grants".into(),
        json!({"groupId":local,"appId":app,"environment":"prod","permission":"use","enabled":true}),
        200,
    )
    .await?;
    ensure!(
        h.visible(&carol, &prod).await?.is_some(),
        "explicit prod group use did not admit its member"
    );
    admin(
        Method::PUT,
        format!("/api/v1/groups/{local}/members"),
        json!({"userId":carol.id,"member":false}),
        200,
    )
    .await?;
    ensure!(
        h.visible(&carol, &dev).await?.is_none() && h.visible(&carol, &prod).await?.is_none(),
        "removed member kept dev or prod use"
    );
    admin(
        Method::PUT,
        "/api/v1/group-grants".into(),
        json!({"groupId":local,"appId":app,"environment":"prod","permission":"use","enabled":false}),
        200,
    )
    .await?;
    let copied: i64 = sqlx::query_scalar("SELECT count(*) FROM grants WHERE user_id=$1")
        .bind(&carol.id)
        .fetch_one(&h.state.pool)
        .await?;
    ensure!(copied == 0, "group permission was copied into user grants");
    passed.push("a local group grant gives use to current members only, and removal applies to the next request without copied grants".into());

    let unit = admin(
        Method::POST,
        "/api/v1/groups".into(),
        json!({"name":format!("본부 {suffix}"),"source":"hr"}),
        200,
    )
    .await?;
    let unit = unit["id"].as_str().context("hr group id")?.to_owned();
    let team = admin(
        Method::POST,
        "/api/v1/groups".into(),
        json!({"name":format!("팀 {suffix}"),"source":"hr","parentId":unit}),
        200,
    )
    .await?;
    let team = team["id"].as_str().context("hr child id")?.to_owned();
    admin(
        Method::PUT,
        "/api/v1/group-grants".into(),
        json!({"groupId":unit,"appId":app,"environment":"dev","permission":"use","enabled":true}),
        200,
    )
    .await?;
    admin(
        Method::PUT,
        format!("/api/v1/users/{}/primary-group", dave.id),
        json!({"groupId":team}),
        200,
    )
    .await?;
    ensure!(
        h.visible(&dave, &dev).await?.is_none(),
        "a child unit inherited the parent grant"
    );
    admin(
        Method::PUT,
        format!("/api/v1/users/{}/primary-group", dave.id),
        json!({"groupId":unit}),
        200,
    )
    .await?;
    ensure!(
        h.visible(&dave, &dev).await?.is_some(),
        "an exact primary member did not receive use"
    );
    admin(
        Method::PUT,
        format!("/api/v1/groups/{unit}/members"),
        json!({"userId":dave.id,"member":true}),
        400,
    )
    .await?;
    admin(
        Method::PUT,
        format!("/api/v1/groups/{unit}/parent"),
        json!({"parentId":team}),
        409,
    )
    .await?;
    admin(
        Method::PUT,
        format!("/api/v1/groups/{team}/parent"),
        json!({"parentId":null}),
        200,
    )
    .await?;
    passed.push("HR groups admit exact primary members only, neither parents nor children inherit, and hierarchy cycles are rejected".into());

    admin(Method::PUT, "/api/v1/group-grants".into(), json!({"groupId":local,"appId":app,"environment":"prod","permission":"deploy","enabled":true}), 400).await?;
    admin(Method::PUT, "/api/v1/group-grants".into(), json!({"groupId":local,"appId":app,"environment":"dev","permission":"develop","enabled":true}), 200).await?;
    admin(
        Method::PUT,
        format!("/api/v1/groups/{local}/members"),
        json!({"userId":carol.id,"member":true}),
        200,
    )
    .await?;
    let row = h
        .visible(&carol, &dev)
        .await?
        .context("group developer cannot see dev")?;
    ensure!(
        row["canManage"] == true && row["canPromote"] == false,
        "group develop must manage dev only: {row}"
    );
    ensure!(
        h.visible(&carol, &prod).await?.is_none(),
        "group develop reached prod"
    );
    passed.push("groups cannot hold deploy, and a group develop grant manages only the development environment".into());

    ensure!(
        h.visible(&erin, &prod).await?.is_none(),
        "unadmitted user saw prod"
    );
    admin(
        Method::PUT,
        format!("/api/v1/apps/{app}/admission"),
        json!({"environment":"prod","mode":"all"}),
        200,
    )
    .await?;
    ensure!(
        h.visible(&erin, &prod).await?.is_some(),
        "all admission did not admit an enabled user"
    );
    ensure!(
        h.visible(&operator, &prod).await?.is_some(),
        "all admission did not admit an administrator"
    );
    ensure!(
        h.visible(&erin, &dev).await?.is_none(),
        "prod admission reached dev"
    );
    admin(
        Method::PUT,
        format!("/api/v1/apps/{app}/admission"),
        json!({"environment":"prod","mode":"selected"}),
        200,
    )
    .await?;
    ensure!(
        h.visible(&erin, &prod).await?.is_none(),
        "selected mode kept an implicit user"
    );
    ensure!(
        h.visible(&operator, &prod).await?.is_none(),
        "selected mode kept administrator implicit prod use"
    );
    admin(
        Method::PUT,
        format!("/api/v1/apps/{app}/admission"),
        json!({"environment":"prod","mode":"everyone"}),
        400,
    )
    .await?;
    passed.push("admitting all users is an explicit per-environment choice, and returning to selected removes it".into());

    admin(
        Method::PUT,
        "/api/v1/grants".into(),
        json!({"userId":erin.id,"appId":app,"environment":"dev","permission":"use","enabled":true}),
        200,
    )
    .await?;
    admin(
        Method::PUT,
        "/api/v1/grants".into(),
        json!({"userId":operator.id,"appId":app,"environment":"prod","permission":"use","enabled":true}),
        200,
    )
    .await?;
    ensure!(
        h.visible(&operator, &prod).await?.is_some(),
        "explicit use did not admit administrator"
    );
    admin(
        Method::PUT,
        format!("/api/v1/apps/{app}/control"),
        json!({"disabled":true}),
        200,
    )
    .await?;
    ensure!(
        h.visible(&operator, &prod).await?.is_none(),
        "an administrator used a disabled app"
    );
    ensure!(
        h.visible(&erin, &dev).await?.is_none(),
        "a user used a disabled app"
    );
    ensure!(
        h.visible(&carol, &dev).await?.is_some(),
        "a developer lost dev to repair the app"
    );
    h.visibility_consistent(&operator, &prod, false).await?;
    h.visibility_consistent(&operator, &dev, true).await?;
    h.visibility_consistent(&erin, &dev, false).await?;
    h.visibility_consistent(&carol, &dev, true).await?;
    h.visibility_consistent(&carol, &prod, false).await?;
    let blocked = response(
        h.request(
            &operator.token,
            Method::GET,
            &format!("/api/v1/installations/{}/api/probe", prod.installation),
        )
        .send()
        .await?,
        403,
    )
    .await?;
    ensure!(
        blocked["error"]["code"] == "app_disabled",
        "disabled app call lacks a stable code: {blocked}"
    );
    // Runtime management is not use: administrators keep the installation to stop or
    // remove it, but it is not openable in prod. Other managers see only their own
    // app/environment; use-only users receive an empty management list.
    let entry = |rows: &Value, app: &App| {
        rows.as_array().and_then(|rows| {
            rows.iter()
                .find(|row| row["id"] == json!(app.installation))
                .cloned()
        })
    };
    let managed = response(
        h.request(
            &operator.token,
            Method::GET,
            "/api/v1/runtime/installations",
        )
        .send()
        .await?,
        200,
    )
    .await?;
    let managed_prod = entry(&managed, &prod).context("disabled prod left runtime management")?;
    let managed_dev = entry(&managed, &dev).context("disabled dev left runtime management")?;
    ensure!(
        managed_prod["appDisabled"] == true
            && managed_prod["canOpen"] == false
            && managed_prod["canManage"] == true,
        "runtime management view of a disabled prod app: {managed_prod}"
    );
    ensure!(
        managed_dev["appDisabled"] == true && managed_dev["canOpen"] == true,
        "administrator lost dev repair of a disabled app: {managed_dev}"
    );
    for actor in [&carol, &erin] {
        let managed = response(
            h.request(&actor.token, Method::GET, "/api/v1/runtime/installations")
                .send()
                .await?,
            200,
        )
        .await?;
        ensure!(
            entry(&managed, &prod).is_none(),
            "a non-manager listed prod"
        );
        ensure!(
            entry(&managed, &dev).is_some() == (actor.id == carol.id),
            "per-app developer management visibility is inconsistent"
        );
    }
    admin(
        Method::PUT,
        format!("/api/v1/apps/{app}/control"),
        json!({"disabled":false}),
        200,
    )
    .await?;
    ensure!(
        h.visible(&operator, &prod).await?.is_some() && h.visible(&erin, &dev).await?.is_some(),
        "re-enabling did not restore use"
    );
    h.visibility_consistent(&operator, &prod, true).await?;
    h.visibility_consistent(&erin, &dev, true).await?;
    let managed = response(
        h.request(
            &operator.token,
            Method::GET,
            "/api/v1/runtime/installations",
        )
        .send()
        .await?,
        200,
    )
    .await?;
    let managed_prod =
        entry(&managed, &prod).context("enabled prod missing from runtime management")?;
    ensure!(
        managed_prod["appDisabled"] == false && managed_prod["canOpen"] == true,
        "re-enabling did not reach runtime management: {managed_prod}"
    );
    passed.push("the company switch consistently filters default, paged and app-detail installations; developers keep dev and re-enabling restores explicit use; administrators require explicit prod admission, non-administrator developers see only their managed installations, and disabled apps stay manageable".into());

    denied(
        h.request(&carol.token, Method::GET, "/api/v1/groups")
            .send()
            .await?,
    )
    .await?;
    denied(
        h.request(
            &carol.token,
            Method::PUT,
            &format!("/api/v1/apps/{app}/control"),
        )
        .json(&json!({"disabled":true}))
        .send()
        .await?,
    )
    .await?;
    denied(
        h.request(&carol.token, Method::GET, "/api/v1/audit")
            .send()
            .await?,
    )
    .await?;
    let audit = |query: String| {
        let request = harness.request(token, Method::GET, &format!("/api/v1/audit?{query}"));
        async move { response(request.send().await?, 200).await }
    };
    let first = audit(format!("actor={}&action=group.&limit=2", operator.id)).await?;
    let cursor = first["nextCursor"]
        .as_i64()
        .context("audit paging lacks nextCursor")?;
    ensure!(
        first["entries"].as_array().map(Vec::len) == Some(2),
        "audit page size: {first}"
    );
    let rest = audit(format!(
        "actor={}&action=group.&limit=200&cursor={cursor}",
        operator.id
    ))
    .await?;
    ensure!(
        rest["entries"]
            .as_array()
            .is_some_and(|e| e.iter().all(|x| x["id"].as_i64() < Some(cursor))),
        "audit cursor did not page backwards"
    );
    let all = audit(format!("actor={}&limit=200", operator.id)).await?;
    let mut actions: Vec<String> = Vec::new();
    for page in [&first, &rest, &all] {
        for entry in page["entries"].as_array().context("audit entries")? {
            ensure!(
                entry["actor"] == json!(operator.id),
                "audit actor filter leaked: {entry}"
            );
            actions.push(entry["action"].as_str().unwrap_or_default().to_owned());
        }
    }
    for action in [
        "group.created",
        "group_grant.enabled",
        "group.member_added",
        "group.member_removed",
        "user.primary_group_set",
        "group.parent_set",
        "app.admission_set",
        "app.disabled",
        "app.enabled",
    ] {
        ensure!(
            actions.iter().any(|a| a == action),
            "missing audit action {action}"
        );
    }
    passed.push("only administrators change groups and access, and every change is audited and queryable by actor, action and cursor".into());
    mutation_revocation(h, &app, &local, &unit, &team, &dave, &erin, suffix).await?;
    passed.push("all group/access mutations, personal grants, user state changes and registration decisions roll back without domain audit when an administrator is revoked or disabled during a PostgreSQL lock wait".into());
    grant_and_primary_audit(h, &operator, &app, &unit, &team).await?;
    passed.push("personal grant changes (including registration approval), user state changes and primary-group assignments retain their exact before/after targets in the same transaction audit; stale organization removals preserve the new primary group".into());
    primary_group_lock_order(h).await?;
    passed.push("opposing administrators update primary groups with ordered user locks, preserving the unlocked target and completing without deadlock".into());
    Ok(())
}

/// One expected row of the policy matrix: which environments a principal may open and
/// whose operating records (jobs, approvals) it may observe (company switch off, on),
/// and its display values.
struct PolicyCase {
    name: &'static str,
    token: String,
    open: [&'static [&'static str]; 2],
    observe: [&'static [&'static str]; 2],
    manage: &'static [&'static str],
    promote: bool,
}

/// Environments listed for `app` by a list endpoint; a 403 page means nothing is listed.
async fn listed_environments(
    h: &Harness<'_>,
    token: &str,
    path: &str,
    field: Option<&str>,
    app: &str,
) -> Result<Vec<Value>> {
    let answer = h.request(token, Method::GET, path).send().await?;
    if answer.status().as_u16() == 403 {
        return Ok(Vec::new());
    }
    let value = response(answer, 200).await?;
    let rows = match field {
        Some(field) => value[field].as_array(),
        None => value.as_array(),
    }
    .context("policy matrix list")?;
    Ok(rows
        .iter()
        .filter(|row| row["appId"] == json!(app))
        .cloned()
        .collect())
}

fn environments(rows: &[Value]) -> Vec<String> {
    let mut found: Vec<String> = rows
        .iter()
        .filter_map(|row| row["environment"].as_str().map(str::to_owned))
        .collect();
    found.sort();
    found
}

/// Lists, details and display values come from one policy decision (`policy.rs`). For
/// each kind of principal, with the company switch off and on, the default and paged
/// installation lists must equal the use decision, the installation's approval record,
/// the job list and the job details must equal the observe decision, and both
/// installation lists report the same display values that the matching requests would get.
async fn policy_matrix(h: &mut Harness<'_>, suffix: &str, passed: &mut Vec<String>) -> Result<()> {
    let app = format!("policy-{suffix}");
    let dev = h.app(app.clone(), "dev", &[]).await?;
    let prod = h.app(app.clone(), "prod", &[]).await?;
    let admin = h.actor(true).await?;
    let deployer = h.actor(false).await?;
    let developer = h.actor(false).await?;
    let user = h.actor(false).await?;
    let stranger = h.actor(false).await?;
    for (actor, environment, permission) in [
        (&deployer, "dev", "develop"),
        (&deployer, "prod", "deploy"),
        (&developer, "dev", "develop"),
        (&user, "dev", "use"),
        (&user, "prod", "use"),
    ] {
        sqlx::query(
            "INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,$2,$3,$4)",
        )
        .bind(&actor.id)
        .bind(&app)
        .bind(environment)
        .bind(permission)
        .execute(&h.state.pool)
        .await?;
    }
    let mut jobs = Vec::new();
    let mut database_jobs = Vec::new();
    for (environment, installation) in [("dev", &dev), ("prod", &prod)] {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO runtime_jobs(id,app_id,environment,actor,action,installation_id,release_id,idempotency_key,input_hash,status,step) VALUES($1,$2,$3,$4,'restart',$5,$6,$7,'policy-matrix','succeeded','completed')")
            .bind(id).bind(&app).bind(environment).bind(&admin.id)
            .bind(installation.installation).bind(installation.release)
            .bind(format!("policy-matrix-{environment}-{suffix}"))
            .execute(&h.state.pool).await?;
        jobs.push((environment, id));
        let database_id = Uuid::new_v4();
        sqlx::query("INSERT INTO jobs(id,installation_id,release_id,actor,idempotency_key,input_hash,status,step) VALUES($1,$2,$3,$4,$5,'policy-matrix','succeeded','completed')")
            .bind(database_id).bind(installation.installation).bind(installation.release)
            .bind(&admin.id).bind(format!("policy-db-{environment}-{suffix}"))
            .execute(&h.state.pool).await?;
        database_jobs.push((environment, database_id));
        sqlx::query("UPDATE releases SET source_commit=$2 WHERE id=$1")
            .bind(installation.release)
            .bind("a".repeat(40))
            .execute(&h.state.pool)
            .await?;
    }
    let delegate = |actor: &Actor, environment: &'static str, actions: &'static [&'static str]| {
        let body = json!({"name":"Policy matrix","appId":app,"environment":environment,"actions":actions,"expiresIn":3600});
        let token = actor.token.clone();
        let h = &*h;
        async move {
            let value = h
                .json(&token, Method::POST, "/api/v1/delegations", body, 200)
                .await?;
            Ok::<_, anyhow::Error>(
                value["credential"]["accessToken"]
                    .as_str()
                    .context("delegated access token")?
                    .to_owned(),
            )
        }
    };
    let both: &[&str] = &["dev", "prod"];
    let cases = vec![
        PolicyCase {
            name: "administrator",
            token: admin.token.clone(),
            open: [&["dev"], &["dev"]],
            observe: [both, both],
            manage: both,
            promote: true,
        },
        PolicyCase {
            name: "deployer",
            token: deployer.token.clone(),
            open: [&["dev"], &["dev"]],
            observe: [both, both],
            manage: both,
            promote: true,
        },
        PolicyCase {
            name: "developer",
            token: developer.token.clone(),
            open: [&["dev"], &["dev"]],
            observe: [&["dev"], &["dev"]],
            manage: &["dev"],
            promote: false,
        },
        PolicyCase {
            name: "user",
            token: user.token.clone(),
            open: [both, &[]],
            observe: [&[], &[]],
            manage: &[],
            promote: false,
        },
        PolicyCase {
            name: "stranger",
            token: stranger.token.clone(),
            open: [&[], &[]],
            observe: [&[], &[]],
            manage: &[],
            promote: false,
        },
        PolicyCase {
            name: "delegated developer with read",
            token: delegate(&developer, "dev", &["read", "start"]).await?,
            open: [&["dev"], &["dev"]],
            observe: [&["dev"], &["dev"]],
            manage: &["dev"],
            promote: false,
        },
        PolicyCase {
            name: "delegated developer without read",
            token: delegate(&developer, "dev", &["start"]).await?,
            open: [&[], &[]],
            observe: [&[], &[]],
            manage: &[],
            promote: false,
        },
        PolicyCase {
            name: "delegated deployer",
            token: delegate(&deployer, "prod", &["read", "promote"]).await?,
            open: [&[], &[]],
            observe: [&["prod"], &["prod"]],
            manage: &["prod"],
            promote: true,
        },
    ];
    for (state, disabled) in [(0, false), (1, true)] {
        h.json(
            &admin.token,
            Method::PUT,
            &format!("/api/v1/apps/{app}/control"),
            json!({"disabled":disabled}),
            200,
        )
        .await?;
        for case in &cases {
            let context = format!(
                "{} with the company switch {}",
                case.name,
                if disabled { "on" } else { "off" }
            );
            let default =
                listed_environments(h, &case.token, "/api/v1/installations", None, &app).await?;
            let paged = listed_environments(
                h,
                &case.token,
                &format!("/api/v1/installations?limit=200&appId={app}"),
                Some("installations"),
                &app,
            )
            .await?;
            let mut detail = Vec::new();
            for (environment, installation) in [("dev", &dev), ("prod", &prod)] {
                let status = h
                    .request(
                        &case.token,
                        Method::GET,
                        &format!(
                            "/api/v1/installations/{}/permissions",
                            installation.installation
                        ),
                    )
                    .send()
                    .await?
                    .status()
                    .as_u16();
                ensure!(
                    matches!(status, 200 | 401 | 403),
                    "{context}: installation detail answered HTTP {status}"
                );
                if status == 200 {
                    detail.push(environment.to_owned());
                }
            }
            let expected: Vec<String> = case.open[state].iter().map(|e| (*e).to_owned()).collect();
            ensure!(
                environments(&default) == expected,
                "{context}: default installation list differs from policy"
            );
            ensure!(
                environments(&paged) == expected,
                "{context}: paged installation list differs from policy"
            );
            // One installation record (api_contract::Installation) on both paths.
            ensure!(
                default == paged,
                "{context}: default and paged installation lists return different records"
            );
            let observed: Vec<String> = case.observe[state]
                .iter()
                .map(|e| (*e).to_owned())
                .collect();
            ensure!(
                detail == observed,
                "{context}: installation approval record differs from the observe decision"
            );
            let managed =
                listed_environments(h, &case.token, "/api/v1/runtime/installations", None, &app)
                    .await?;
            let expected_managed: Vec<String> =
                case.manage.iter().map(|env| (*env).to_owned()).collect();
            ensure!(
                environments(&managed) == expected_managed,
                "{context}: managed installations differ from per-environment management read"
            );
            let app_detail = h
                .request(&case.token, Method::GET, &format!("/api/v1/apps/{app}"))
                .send()
                .await?;
            if case.name != "stranger" && case.name != "delegated developer without read" {
                let app_detail = response(app_detail, 200).await?;
                let installations = app_detail["installations"]
                    .as_array()
                    .context("CLI app detail installations")?;
                let mut expected_detail = expected.clone();
                expected_detail.extend(expected_managed.iter().cloned());
                expected_detail.sort();
                expected_detail.dedup();
                ensure!(
                    environments(installations) == expected_detail,
                    "{context}: CLI app show/status lost a managed installation"
                );
                for row in installations {
                    ensure!(
                        row["canOpen"]
                            == json!(
                                case.open[state]
                                    .contains(&row["environment"].as_str().unwrap_or(""))
                            ),
                        "{context}: CLI management metadata bypassed Use"
                    );
                }
            } else {
                ensure!(
                    matches!(app_detail.status().as_u16(), 403 | 404),
                    "{context}: app detail admitted an unauthorized principal"
                );
            }
            for row in default.iter().chain(&paged) {
                let environment = row["environment"].as_str().unwrap_or_default();
                ensure!(
                    row["canManage"] == json!(case.manage.contains(&environment))
                        && row["canPromote"] == json!(case.promote),
                    "{context}: display values differ from the management and promotion decisions"
                );
                if !case.manage.contains(&environment) {
                    ensure!(
                        row["lastOperation"].is_null()
                            && row["acceptance"].is_null()
                            && row["sourceCommit"].is_null()
                            && row["imageId"].is_null()
                            && row["health"]["errorCode"].is_null(),
                        "{context}: a use-only installation exposed operational details"
                    );
                }
            }
            let listed_jobs = listed_environments(
                h,
                &case.token,
                &format!("/api/v1/runtime/jobs?limit=200&appId={app}"),
                Some("jobs"),
                &app,
            )
            .await?;
            let mut followed = Vec::new();
            for (environment, id) in &jobs {
                let status = h
                    .request(
                        &case.token,
                        Method::GET,
                        &format!("/api/v1/runtime/jobs/{id}"),
                    )
                    .send()
                    .await?
                    .status()
                    .as_u16();
                ensure!(
                    matches!(status, 200 | 401 | 403),
                    "{context}: job detail answered HTTP {status}"
                );
                if status == 200 {
                    followed.push((*environment).to_owned());
                }
            }
            ensure!(
                environments(&listed_jobs) == observed,
                "{context}: job list differs from policy"
            );
            ensure!(
                followed == observed,
                "{context}: job detail differs from the job list"
            );
            let mut database_observed = Vec::new();
            for (environment, id) in &database_jobs {
                let status = h
                    .request(&case.token, Method::GET, &format!("/api/v1/jobs/{id}"))
                    .send()
                    .await?
                    .status()
                    .as_u16();
                ensure!(
                    matches!(status, 200 | 401 | 403),
                    "{context}: database job answered HTTP {status}"
                );
                if status == 200 {
                    database_observed.push((*environment).to_owned());
                }
            }
            ensure!(
                database_observed == observed,
                "{context}: database job bypassed management read"
            );
        }
    }
    h.json(
        &admin.token,
        Method::PUT,
        &format!("/api/v1/apps/{app}/control"),
        json!({"disabled":false}),
        200,
    )
    .await?;
    passed.push("default and paged installation lists, per-environment managed lists, approval records, runtime/database jobs and redacted display values agree for administrators, deployers, developers, users, strangers and delegated credentials; prod management never grants use and use never grants operating records, with company switch off and on".into());
    // A database preparation receipt is an observable job like a runtime receipt: a
    // delegation needs prepare_database and read before anything is stored.
    let jobs_by = async |actor: &Actor| -> Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT count(*) FROM jobs WHERE actor=$1")
                .bind(&actor.id)
                .fetch_one(&h.state.pool)
                .await?,
        )
    };
    let prepare = |token: String, key: &'static str| {
        let body = json!({"installationId":dev.installation,"releaseId":dev.release,"idempotencyKey":format!("{key}-{suffix}")});
        h.request(&token, Method::POST, "/api/v1/database/prepare")
            .json(&body)
            .send()
    };
    let without_read = delegate(&developer, "dev", &["prepare_database"]).await?;
    let denied = prepare(without_read, "policy-prepare-without-read").await?;
    ensure!(
        denied.status().as_u16() == 403,
        "database preparation without read answered HTTP {}",
        denied.status().as_u16()
    );
    let code: Value = denied.json().await?;
    ensure!(
        code["error"]["code"] == "delegation_scope_denied",
        "database preparation without read used another denial"
    );
    ensure!(
        jobs_by(&developer).await? == 0,
        "a denied delegated database preparation stored a job"
    );
    let with_read = delegate(&developer, "dev", &["prepare_database", "read"]).await?;
    let receipt = response(prepare(with_read, "policy-prepare-with-read").await?, 200).await?;
    ensure!(
        receipt["installationId"] == json!(dev.installation) && jobs_by(&developer).await? == 1,
        "delegated database preparation with read did not return its stored job"
    );
    passed.push("delegated database preparation needs prepare_database and read before a job is stored; with both it returns the stored job".into());
    // A delegation never carries administrator authority: the grants page shows a
    // delegated administrator only its own grants, like any delegated user.
    let grants_page = format!("/api/v1/grants?limit=200&appId={app}&environment=dev");
    let delegated_admin = delegate(&admin, "dev", &["read"]).await?;
    let own = response(
        h.request(&delegated_admin, Method::GET, &grants_page)
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        own["grants"]
            .as_array()
            .context("delegated grants page")?
            .iter()
            .all(|row| row["userId"] == json!(admin.id)),
        "a delegated administrator listed other users' grants"
    );
    let full = response(
        h.request(&admin.token, Method::GET, &grants_page)
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        full["grants"]
            .as_array()
            .context("grants page")?
            .iter()
            .any(|row| row["userId"] == json!(developer.id)),
        "an administrator could not list other users' grants"
    );
    passed.push("a delegated administrator lists only its own grants; administrator authority stays with undelegated credentials".into());
    Ok(())
}

/// An administrator manages a company-disabled prod installation (it stays in the
/// runtime management list), so its approvals stay readable and changeable there, and
/// a change is answered with the record it committed instead of a later `app_disabled`.
async fn disabled_app_approvals(
    h: &mut Harness<'_>,
    suffix: &str,
    passed: &mut Vec<String>,
) -> Result<()> {
    let app = h.app(format!("approvals-{suffix}"), "prod", &[]).await?;
    let admin = h.actor(true).await?;
    h.json(
        &admin.token,
        Method::PUT,
        &format!("/api/v1/apps/{}/control", app.app),
        json!({"disabled":true}),
        200,
    )
    .await?;
    let status = h
        .update(&admin, &app, app.release, 0, &[])
        .await?
        .status()
        .as_u16();
    let committed: Option<i64> = sqlx::query_scalar(
        "SELECT revision FROM installation_permissions WHERE installation_id=$1 AND release_id=$2",
    )
    .bind(app.installation)
    .bind(app.release)
    .fetch_optional(&h.state.pool)
    .await?;
    ensure!(
        status == 200 && committed == Some(1),
        "an approval change on a disabled prod app answered HTTP {status} with committed revision {committed:?}"
    );
    let current = h.read(&admin, &app, app.release).await?;
    let latest = response(
        h.request(&admin.token, Method::GET, &app.permissions())
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        current["revision"] == 1 && latest["revision"] == 1,
        "the approval record of a disabled prod app is not the committed one"
    );
    passed.push("an administrator reads and changes the approvals of a company-disabled prod installation it manages, and a change answers with the record it committed".into());
    Ok(())
}

/// Administrator writes outside groups and grants: app creation, app display name and
/// shared component publication. Each waits on a PostgreSQL lock while its
/// administrator is revoked, and must then roll back.
async fn administrator_write_rechecks(
    h: &Harness<'_>,
    suffix: &str,
    passed: &mut Vec<String>,
) -> Result<()> {
    // Interpolated values are exclusively IDs generated by this isolated fixture.
    let created = format!("admin-write-app-{suffix}");
    let renamed = format!("admin-write-name-{suffix}");
    let component = format!("admin-write-component-{suffix}");
    sqlx::query("INSERT INTO apps(id,name) VALUES($1,'Rename fixture')")
        .bind(&renamed)
        .execute(&h.state.pool)
        .await?;
    let cases = [
        PendingMutation {
            name: "create app",
            method: Method::POST,
            path: "/api/v1/apps".into(),
            body: json!({"id":created,"name":"Pending app"}),
            lock: format!("INSERT INTO apps(id,name) VALUES('{created}','Lock holder')"),
            snapshot: format!("SELECT count(*)::text FROM apps WHERE id='{created}'"),
            action: None,
        },
        PendingMutation {
            name: "rename app",
            method: Method::POST,
            path: format!("/api/v1/apps/{renamed}/name"),
            body: json!({"name":format!("Renamed {suffix}")}),
            lock: format!("SELECT id FROM apps WHERE id='{renamed}' FOR UPDATE"),
            snapshot: format!("SELECT name FROM apps WHERE id='{renamed}'"),
            action: None,
        },
        PendingMutation {
            name: "publish component",
            method: Method::POST,
            path: "/api/v1/components".into(),
            body: json!({
                "protocol":"miy.component/v1","name":component,"version":"1.0.0",
                "description":"Pending component","uiContract":"miy.ui/v1alpha1",
                "bundleVersions":["1.0.0"],
                "files":{"README.md":"Pending component\n","index.ts":"export {};\n"},
            }),
            lock: format!(
                "INSERT INTO component_releases(name,version,digest,package,publisher) VALUES('{component}','1.0.0','lock','{{}}','lock')"
            ),
            snapshot: format!(
                "SELECT count(*)::text FROM component_releases WHERE name='{component}'"
            ),
            action: None,
        },
    ];
    let mut failed = Vec::new();
    for case in &cases {
        if let Err(error) = pending_revocation(h, case).await {
            failed.push(format!("{}: {error:#}", case.name));
        }
    }
    ensure!(failed.is_empty(), "{}", failed.join("; "));
    passed.push("app creation, app display name changes and shared component publication roll back when their administrator is revoked or disabled during a PostgreSQL lock wait".into());
    Ok(())
}

/// App-level writes decide inside their transaction after their locks. Each case
/// holds a lock its write must wait for, revokes the developer's grant during the
/// wait, then releases the lock: the write must answer 403 and leave nothing behind.
async fn app_write_revocations(
    h: &mut Harness<'_>,
    suffix: &str,
    passed: &mut Vec<String>,
) -> Result<()> {
    // Interpolated values are exclusively IDs generated by this isolated fixture.
    let installed = format!("app-write-install-{suffix}");
    let release = Uuid::new_v4();
    sqlx::query("INSERT INTO apps(id,name) VALUES($1,'App write fixture')")
        .bind(&installed)
        .execute(&h.state.pool)
        .await?;
    sqlx::query("INSERT INTO releases(id,app_id,version,artifact_hash,manifest) VALUES($1,$2,'1.0.0',$3,'{}')")
        .bind(release)
        .bind(&installed)
        .bind(hash(release.to_string()))
        .execute(&h.state.pool)
        .await?;
    let prepared = h
        .app(format!("app-write-prepare-{suffix}"), "dev", &[])
        .await?;
    let mut failed = Vec::new();
    for name in [
        "create installation",
        "prepare database",
        "issue delegation",
    ] {
        let actor = h.actor(false).await?;
        let key = format!("app-write-{}", Uuid::new_v4());
        let (app, path, body, lock, snapshot) = match name {
            "create installation" => (
                installed.clone(),
                "/api/v1/installations".to_owned(),
                json!({"appId":installed,"environment":"dev","releaseId":release}),
                format!(
                    "INSERT INTO installations(id,app_id,environment,release_id) VALUES('{}','{installed}','dev','{release}')",
                    Uuid::new_v4()
                ),
                format!("SELECT count(*)::text FROM installations WHERE app_id='{installed}'"),
            ),
            "prepare database" => (
                prepared.app.clone(),
                "/api/v1/database/prepare".to_owned(),
                json!({"installationId":prepared.installation,"releaseId":prepared.release,"idempotencyKey":key}),
                format!(
                    "INSERT INTO jobs(id,installation_id,release_id,actor,idempotency_key,input_hash) VALUES('{}','{}','{}','{}','{key}','lock')",
                    Uuid::new_v4(),
                    prepared.installation,
                    prepared.release,
                    actor.id
                ),
                format!("SELECT count(*)::text FROM jobs WHERE actor='{}'", actor.id),
            ),
            _ => (
                installed.clone(),
                "/api/v1/delegations".to_owned(),
                json!({"name":"Pending delegation","appId":installed,"environment":"dev","actions":["start","read"],"expiresIn":3600}),
                format!(
                    "SELECT pg_advisory_xact_lock(hashtextextended('delegations:{}',0))",
                    actor.id
                ),
                format!(
                    "SELECT count(*)::text FROM delegations WHERE user_id='{}'",
                    actor.id
                ),
            ),
        };
        let outcome = async {
            sqlx::query("INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,$2,'dev','develop')")
                .bind(&actor.id)
                .bind(&app)
                .execute(&h.state.pool)
                .await?;
            let before: String = sqlx::query_scalar(&snapshot).fetch_one(&h.state.pool).await?;
            let mut blocker = h.state.pool.begin().await?;
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                .fetch_one(&mut *blocker)
                .await?;
            sqlx::query(&lock).fetch_all(&mut *blocker).await?;
            let pending = h
                .request(&actor.token, Method::POST, &path)
                .json(&body)
                .send();
            tokio::pin!(pending);
            tokio::select! {
                observed = blocked_by(&h.state.pool, pid, "") => { observed?; }
                early = &mut pending => {
                    anyhow::bail!("completed before its lock wait (HTTP {})", early?.status().as_u16());
                }
            }
            sqlx::query("DELETE FROM grants WHERE user_id=$1")
                .bind(&actor.id)
                .execute(&h.state.pool)
                .await?;
            blocker.rollback().await?;
            let status = tokio::time::timeout(Duration::from_secs(10), pending)
                .await??
                .status()
                .as_u16();
            let after: String = sqlx::query_scalar(&snapshot).fetch_one(&h.state.pool).await?;
            ensure!(
                status == 403 && before == after,
                "answered HTTP {status} and {} after its grant was revoked during the lock wait",
                if before == after { "wrote nothing" } else { "kept its write" }
            );
            Ok::<_, anyhow::Error>(())
        }
        .await;
        if let Err(error) = outcome {
            failed.push(format!("{name}: {error:#}"));
        }
    }
    ensure!(failed.is_empty(), "{}", failed.join("; "));
    passed.push("installation creation, database preparation and delegation issue decide after their PostgreSQL lock wait: a developer revoked during the wait gets 403 and nothing is written".into());
    Ok(())
}

/// Observe an actual PostgreSQL wait, rather than assuming a request started after a sleep.
async fn blocked_by(pool: &sqlx::PgPool, blocker: i32, prefix: &str) -> Result<i32> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let pid: Option<i32> = sqlx::query_scalar(
                "SELECT pid FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' \
                 AND $1=ANY(pg_blocking_pids(pid)) AND starts_with(query,$2) LIMIT 1",
            )
            .bind(blocker)
            .bind(prefix)
            .fetch_optional(pool)
            .await?;
            if let Some(pid) = pid {
                return Ok(pid);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .context("permission regression did not observe the expected PostgreSQL lock wait")?
}

struct PendingMutation {
    name: &'static str,
    method: Method,
    path: String,
    body: Value,
    lock: String,
    snapshot: String,
    /// The audit action a committed change would record, when the change is audited.
    action: Option<&'static str>,
}

#[allow(clippy::too_many_arguments)]
async fn mutation_revocation(
    h: &Harness<'_>,
    app: &str,
    local: &str,
    unit: &str,
    team: &str,
    primary_user: &Actor,
    personal_user: &Actor,
    suffix: &str,
) -> Result<()> {
    // Interpolated values are exclusively IDs generated by this isolated fixture.
    let group_lock = |id: &str| format!("SELECT id FROM groups WHERE id='{id}' FOR UPDATE");
    let primary_snapshot = format!(
        "SELECT COALESCE(primary_group_id::text,'none') FROM users WHERE id='{}'",
        primary_user.id
    );
    // A pending registration whose approval would create the app and a develop grant.
    let registration = Uuid::new_v4();
    let registered = format!("permission-r-{suffix}");
    sqlx::query("INSERT INTO app_registrations(id,app_id,name,actor) VALUES($1,$2,$2,$3)")
        .bind(registration)
        .bind(&registered)
        .bind(&personal_user.id)
        .execute(&h.state.pool)
        .await?;
    let cases = [
        PendingMutation {
            name: "create group",
            method: Method::POST,
            path: "/api/v1/groups".into(),
            body: json!({"name":format!("pending {suffix}"),"source":"hr","parentId":unit}),
            lock: group_lock(unit),
            snapshot: format!("SELECT count(*)::text FROM groups WHERE name='pending {suffix}'"),
            action: Some("group.created"),
        },
        PendingMutation {
            name: "update group",
            method: Method::PATCH,
            path: format!("/api/v1/groups/{local}"),
            body: json!({"name":format!("changed {suffix}"),"active":false}),
            lock: group_lock(local),
            snapshot: format!("SELECT name||':'||active::text FROM groups WHERE id='{local}'"),
            action: Some("group.deactivated"),
        },
        PendingMutation {
            name: "set parent",
            method: Method::PUT,
            path: format!("/api/v1/groups/{unit}/parent"),
            body: json!({"parentId":team}),
            lock: group_lock(unit),
            snapshot: format!(
                "SELECT COALESCE(parent_id::text,'none') FROM groups WHERE id='{unit}'"
            ),
            action: Some("group.parent_set"),
        },
        PendingMutation {
            name: "set member",
            method: Method::PUT,
            path: format!("/api/v1/groups/{local}/members"),
            body: json!({"userId":personal_user.id,"member":true}),
            lock: group_lock(local),
            snapshot: format!(
                "SELECT count(*)::text FROM group_members WHERE group_id='{local}' AND user_id='{}'",
                personal_user.id
            ),
            action: Some("group.member_added"),
        },
        PendingMutation {
            name: "set primary group",
            method: Method::PUT,
            path: format!("/api/v1/users/{}/primary-group", primary_user.id),
            body: json!({"groupId":team}),
            lock: group_lock(team),
            snapshot: primary_snapshot,
            action: Some("user.primary_group_set"),
        },
        PendingMutation {
            name: "set group grant",
            method: Method::PUT,
            path: "/api/v1/group-grants".into(),
            body: json!({"groupId":local,"appId":app,"environment":"prod","permission":"use","enabled":true}),
            lock: group_lock(local),
            snapshot: format!(
                "SELECT count(*)::text FROM group_grants WHERE group_id='{local}' AND app_id='{app}' AND environment='prod'"
            ),
            action: Some("group_grant.enabled"),
        },
        PendingMutation {
            name: "set admission",
            method: Method::PUT,
            path: format!("/api/v1/apps/{app}/admission"),
            body: json!({"environment":"prod","mode":"all"}),
            lock: format!(
                "SELECT app_id FROM app_admission WHERE app_id='{app}' AND environment='prod' FOR UPDATE"
            ),
            snapshot: format!(
                "SELECT mode FROM app_admission WHERE app_id='{app}' AND environment='prod'"
            ),
            action: Some("app.admission_set"),
        },
        PendingMutation {
            name: "set company control",
            method: Method::PUT,
            path: format!("/api/v1/apps/{app}/control"),
            body: json!({"disabled":true}),
            lock: format!("SELECT app_id FROM app_controls WHERE app_id='{app}' FOR UPDATE"),
            snapshot: format!("SELECT disabled::text FROM app_controls WHERE app_id='{app}'"),
            action: Some("app.disabled"),
        },
        PendingMutation {
            name: "revoke personal grant",
            method: Method::PUT,
            path: "/api/v1/grants".into(),
            body: json!({"userId":personal_user.id,"appId":app,"environment":"dev","permission":"use","enabled":false}),
            lock: format!(
                "SELECT user_id FROM grants WHERE user_id='{}' AND app_id='{app}' AND environment='dev' AND permission='use' FOR UPDATE",
                personal_user.id
            ),
            snapshot: format!(
                "SELECT count(*)::text FROM grants WHERE user_id='{}' AND app_id='{app}' AND environment='dev' AND permission='use'",
                personal_user.id
            ),
            action: Some("grant.disabled"),
        },
        PendingMutation {
            name: "disable user",
            method: Method::POST,
            path: format!("/api/v1/users/{}/state", personal_user.id),
            body: json!({"enabled":false}),
            // The request waits on the user-state lock before it locks any user row,
            // so the actor can be revoked while the change is pending.
            lock: "SELECT pg_advisory_xact_lock(73490130)".into(),
            snapshot: format!(
                "SELECT enabled::text FROM users WHERE id='{}'",
                personal_user.id
            ),
            action: Some("user.disabled"),
        },
        PendingMutation {
            name: "approve registration",
            method: Method::POST,
            path: format!("/api/v1/app-registrations/{registration}/decision"),
            body: json!({"approve":true}),
            lock: format!("SELECT id FROM app_registrations WHERE id='{registration}' FOR UPDATE"),
            snapshot: format!(
                "SELECT status||':'||(SELECT count(*) FROM apps WHERE id='{registered}')||':'||(SELECT count(*) FROM grants WHERE app_id='{registered}') FROM app_registrations WHERE id='{registration}'"
            ),
            action: Some("grant.enabled"),
        },
    ];
    for case in &cases {
        pending_revocation(h, case).await?;
    }
    Ok(())
}

/// Hold `case.lock`, start the change, revoke the administrator (admin, then enabled
/// on a fresh actor) while the change waits on the lock, then release it: the change
/// must answer 403 and leave the snapshot and the audit trail unchanged.
async fn pending_revocation(h: &Harness<'_>, case: &PendingMutation) -> Result<()> {
    for revoke in ["admin", "enabled"] {
        let actor = h.actor(true).await?;
        let before: String = sqlx::query_scalar(&case.snapshot)
            .fetch_one(&h.state.pool)
            .await?;
        let mut blocker = h.state.pool.begin().await?;
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *blocker)
            .await?;
        sqlx::query(&case.lock).fetch_all(&mut *blocker).await?;
        let pending = h
            .request(&actor.token, case.method.clone(), &case.path)
            .json(&case.body)
            .send();
        tokio::pin!(pending);
        tokio::select! {
            observed = blocked_by(&h.state.pool, pid, "") => { observed?; }
            early = &mut pending => {
                anyhow::bail!("{} completed before its lock wait (HTTP {})", case.name, early?.status().as_u16());
            }
        }
        // Both are Core-authoritative user state. No test credential is printed.
        sqlx::query(&format!("UPDATE users SET {revoke}=false WHERE id=$1"))
            .bind(&actor.id)
            .execute(&h.state.pool)
            .await?;
        blocker.rollback().await?;
        response(
            tokio::time::timeout(Duration::from_secs(10), pending).await??,
            403,
        )
        .await?;
        let after: String = sqlx::query_scalar(&case.snapshot)
            .fetch_one(&h.state.pool)
            .await?;
        ensure!(
            before == after,
            "{} survived {revoke} revocation",
            case.name
        );
        if let Some(action) = case.action {
            let audits: i64 =
                sqlx::query_scalar("SELECT count(*) FROM audit WHERE actor=$1 AND action=$2")
                    .bind(&actor.id)
                    .bind(action)
                    .fetch_one(&h.state.pool)
                    .await?;
            ensure!(
                audits == 0,
                "{} recorded a rolled-back domain change",
                case.name
            );
        }
    }
    Ok(())
}

async fn grant_and_primary_audit(
    h: &Harness<'_>,
    operator: &Actor,
    app: &str,
    unit: &str,
    team: &str,
) -> Result<()> {
    let user = h.actor(false).await?;
    for (before, after) in [(false, true), (true, true), (true, false)] {
        h.json(
            &operator.token, Method::PUT, "/api/v1/grants",
            json!({"userId":user.id,"appId":app,"environment":"prod","permission":"deploy","enabled":after}),
            200,
        ).await?;
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM grants WHERE user_id=$1 AND app_id=$2 AND environment='prod' AND permission='deploy')")
            .bind(&user.id).bind(app).fetch_one(&h.state.pool).await?;
        ensure!(exists == after, "personal grant state did not commit");
        let action = if after {
            "grant.enabled"
        } else {
            "grant.disabled"
        };
        let target = format!(
            "user:{}:app:{app}:prod:deploy:from:{before}:to:{after}",
            user.id
        );
        let recorded: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM audit WHERE actor=$1 AND action=$2 AND target=$3)",
        )
        .bind(&operator.id)
        .bind(action)
        .bind(target)
        .fetch_one(&h.state.pool)
        .await?;
        ensure!(
            recorded,
            "personal grant audit lost its user/app/environment/permission or before/after state"
        );
    }
    let mut before = "none";
    let primary_path = format!("/api/v1/users/{}/primary-group", user.id);
    for after in [Some(unit), Some(team), None] {
        let body = if after.is_none() {
            let stale = h
                .json(
                    &operator.token,
                    Method::PUT,
                    &primary_path,
                    json!({"groupId":null,"expectedGroupId":unit}),
                    409,
                )
                .await?;
            ensure!(
                stale["error"]["code"] == "primary_group_changed",
                "stale organization removal returned an unexpected error"
            );
            let removal_audited: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM audit WHERE actor=$1 AND action='user.primary_group_set' AND target=$2)")
                .bind(&operator.id)
                .bind(format!("user:{}:from:{team}:to:none", user.id))
                .fetch_one(&h.state.pool)
                .await?;
            ensure!(
                !removal_audited,
                "stale organization removal committed an audit"
            );
            for expected in [json!(null), json!(unit)] {
                let stale = h
                    .json(
                        &operator.token,
                        Method::PUT,
                        &primary_path,
                        json!({"groupId":unit,"expectedGroupId":expected}),
                        409,
                    )
                    .await?;
                ensure!(stale["error"]["code"] == "primary_group_changed");
            }
            // This can succeed only if the stale operations left the team as primary.
            json!({"groupId":null,"expectedGroupId":team})
        } else if before == "none" {
            json!({"groupId":after,"expectedGroupId":null})
        } else {
            json!({"groupId":after,"expectedGroupId":before})
        };
        h.json(&operator.token, Method::PUT, &primary_path, body, 200)
            .await?;
        let target = format!(
            "user:{}:from:{before}:to:{}",
            user.id,
            after.unwrap_or("none")
        );
        let recorded: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM audit WHERE actor=$1 AND action='user.primary_group_set' AND target=$2)")
            .bind(&operator.id).bind(target).fetch_one(&h.state.pool).await?;
        ensure!(
            recorded,
            "primary-group audit lost its old/new group or explicit removal"
        );
        before = after.unwrap_or("none");
    }
    let audited = move |action: &'static str, target: String| async move {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM audit WHERE actor=$1 AND action=$2 AND target=$3)",
        )
        .bind(&operator.id)
        .bind(action)
        .bind(target)
        .fetch_one(&h.state.pool)
        .await
    };
    for (before, after) in [(true, false), (false, true)] {
        h.json(
            &operator.token,
            Method::POST,
            &format!("/api/v1/users/{}/state", user.id),
            json!({"enabled":after}),
            200,
        )
        .await?;
        let action = if after {
            "user.enabled"
        } else {
            "user.disabled"
        };
        ensure!(
            audited(action, format!("user:{}:from:{before}:to:{after}", user.id)).await?,
            "user state audit lost its user or before/after state"
        );
    }
    // A person the identity provider authenticates is admitted only by an
    // administrator's registration; nothing is created by a sign-in attempt.
    let subject = Uuid::new_v4().to_string();
    // The user above lost its sessions when it was disabled.
    let bystander = h.actor(false).await?;
    for (body, code) in [
        (
            json!({"id":"not-a-subject","name":"New person"}),
            "invalid_user_id",
        ),
        (
            json!({"id":subject.to_uppercase(),"name":"New person"}),
            "invalid_user_id",
        ),
        (json!({"id":subject,"name":"  "}), "invalid_user_name"),
    ] {
        let refused = h
            .json(&operator.token, Method::POST, "/api/v1/users", body, 400)
            .await?;
        ensure!(
            refused["error"]["code"] == code,
            "registration accepted {code}"
        );
    }
    h.json(
        &bystander.token,
        Method::POST,
        "/api/v1/users",
        json!({"id":subject,"name":"New person"}),
        403,
    )
    .await?;
    let created = h
        .json(
            &operator.token,
            Method::POST,
            "/api/v1/users",
            json!({"id":subject,"name":" New person ","canDevelop":true}),
            200,
        )
        .await?;
    ensure!(
        created
            == json!({"id":subject,"name":"New person","admin":false,"canDevelop":true,"enabled":true}),
        "registration did not create an enabled non-administrator"
    );
    ensure!(
        audited("user.registered", format!("user:{subject}:developer:true")).await?,
        "registration left no audit row"
    );
    let again = h
        .json(
            &operator.token,
            Method::POST,
            "/api/v1/users",
            json!({"id":subject,"name":"Other"}),
            409,
        )
        .await?;
    ensure!(again["error"]["code"] == "user_exists");
    let grants: i64 = sqlx::query_scalar("SELECT count(*) FROM effective_grants WHERE user_id=$1")
        .bind(&subject)
        .fetch_one(&h.state.pool)
        .await?;
    ensure!(grants == 0, "registration granted an app permission");
    let path = format!("/api/v1/users/{subject}");
    let empty = h
        .json(&operator.token, Method::PATCH, &path, json!({}), 400)
        .await?;
    ensure!(empty["error"]["code"] == "no_user_change");
    h.json(
        &bystander.token,
        Method::PATCH,
        &path,
        json!({"admin":true}),
        403,
    )
    .await?;
    h.json(
        &operator.token,
        Method::PATCH,
        "/api/v1/users/00000000-0000-4000-8000-00000000ffff",
        json!({"admin":true}),
        404,
    )
    .await?;
    let changed = h
        .json(
            &operator.token,
            Method::PATCH,
            &path,
            json!({"name":"Renamed","admin":true,"canDevelop":false}),
            200,
        )
        .await?;
    ensure!(
        changed
            == json!({"id":subject,"name":"Renamed","admin":true,"canDevelop":false,"enabled":true})
    );
    for action in [
        "user.admin.enabled",
        "user.developer.disabled",
        "user.renamed",
    ] {
        ensure!(
            audited(action, format!("user:{subject}")).await?,
            "{action} left no audit row"
        );
    }
    let unchanged = h
        .json(
            &operator.token,
            Method::PATCH,
            &path,
            json!({"admin":true}),
            200,
        )
        .await?;
    ensure!(unchanged["admin"] == true);
    let repeats: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit WHERE action='user.admin.enabled' AND target=$1",
    )
    .bind(format!("user:{subject}"))
    .fetch_one(&h.state.pool)
    .await?;
    ensure!(repeats == 1, "an unchanged role was audited as a change");
    h.json(
        &operator.token,
        Method::PATCH,
        &path,
        json!({"admin":false}),
        200,
    )
    .await?;
    ensure!(
        audited("user.admin.disabled", format!("user:{subject}")).await?,
        "administrator removal left no audit row"
    );
    let registration = Uuid::new_v4();
    let registered = format!("permission-a-{}", Uuid::new_v4().simple());
    sqlx::query("INSERT INTO app_registrations(id,app_id,name,actor) VALUES($1,$2,$2,$3)")
        .bind(registration)
        .bind(&registered)
        .bind(&user.id)
        .execute(&h.state.pool)
        .await?;
    h.json(
        &operator.token,
        Method::POST,
        &format!("/api/v1/app-registrations/{registration}/decision"),
        json!({"approve":true}),
        200,
    )
    .await?;
    ensure!(
        audited(
            "grant.enabled",
            format!(
                "user:{}:app:{registered}:dev:develop:from:false:to:true",
                user.id
            )
        )
        .await?,
        "registration approval created a develop grant without its personal grant audit"
    );
    Ok(())
}

async fn primary_group_lock_order(h: &Harness<'_>) -> Result<()> {
    let mut actors = [h.actor(true).await?, h.actor(true).await?];
    actors.sort_by(|a, b| a.id.cmp(&b.id));
    let [low, high] = &actors;
    let mut blocker = h.state.pool.begin().await?;
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *blocker)
        .await?;
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
        .bind(&low.id)
        .fetch_one(&mut *blocker)
        .await?;
    let first = h
        .request(
            &low.token,
            Method::PUT,
            &format!("/api/v1/users/{}/primary-group", high.id),
        )
        .json(&json!({"groupId":null}))
        .send();
    tokio::pin!(first);
    tokio::select! {
        observed = blocked_by(&h.state.pool, pid, "SELECT id,primary_group_id FROM users") => { observed?; }
        early = &mut first => { anyhow::bail!("first primary-group change skipped ordered locks (HTTP {})", early?.status().as_u16()); }
    }
    // The high target must still be free: target-first locking would have acquired
    // it before waiting on the low actor, creating an A->B/B->A deadlock.
    let mut probe = h.state.pool.begin().await?;
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR NO KEY UPDATE NOWAIT")
        .bind(&high.id)
        .fetch_one(&mut *probe)
        .await?;
    probe.rollback().await?;
    let second = h
        .request(
            &high.token,
            Method::PUT,
            &format!("/api/v1/users/{}/primary-group", low.id),
        )
        .json(&json!({"groupId":null}))
        .send();
    tokio::pin!(second);
    tokio::select! {
        early = &mut first => { anyhow::bail!("first primary-group change escaped its user lock (HTTP {})", early?.status().as_u16()); }
        early = &mut second => { anyhow::bail!("second primary-group change escaped its user lock (HTTP {})", early?.status().as_u16()); }
        // The second request may wait on the first waiter's tuple lock rather
        // than the blocker PID, but both must still be acquiring ordered users.
        waits = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let n: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND starts_with(query,'SELECT id,primary_group_id FROM users')")
                    .fetch_one(&h.state.pool).await?;
                if n >= 2 { return Ok::<_, anyhow::Error>(()); }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }) => { waits??; }
    }
    blocker.rollback().await?;
    let (a, b) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::try_join!(first, second)
    })
    .await??;
    response(a, 200).await?;
    response(b, 200).await?;
    Ok(())
}

async fn notification_contract(h: &mut Harness<'_>, admin: &Actor, user: &Actor) -> Result<()> {
    async fn fresh_proof(h: &Harness<'_>, user: &Actor, app: &App) -> Result<String> {
        let gateway = format!("/api/v1/installations/{}/api/probe", app.installation);
        response(
            h.request(&user.token, Method::GET, &gateway).send().await?,
            200,
        )
        .await
        .context("notification step 1")?;
        app.proof()
    }
    async fn inbox(h: &Harness<'_>, user: &str, app: &App) -> Result<Vec<(String, String, bool)>> {
        Ok(sqlx::query_as::<_, (String, String, bool)>(
            "SELECT group_key,title,read_at IS NOT NULL FROM notifications WHERE user_id=$1 AND installation_id=$2 ORDER BY group_key",
        )
        .bind(user)
        .bind(app.installation)
        .fetch_all(&h.state.pool)
        .await?)
    }
    let permission = vec![crate::notifications::PERMISSION.to_string()];
    // A call token exists once any platform permission is approved; approve the
    // directory first so the unapproved notification permission is what refuses.
    let both = vec![
        crate::notifications::PERMISSION.to_string(),
        "platform:directory:read".to_string(),
    ];
    let app = h
        .app(
            format!("notify-{}", &Uuid::new_v4().simple().to_string()[..12]),
            "dev",
            &both,
        )
        .await
        .context("notification step 2")?;
    let other = h
        .app(
            format!("notify-other-{}", &Uuid::new_v4().simple().to_string()[..8]),
            "dev",
            &permission,
        )
        .await
        .context("notification step 3")?;
    let recipient = h.actor(false).await.context("notification step 4")?;
    let outsider = h.actor(false).await.context("notification step 5")?;
    let disabled = h.actor(false).await.context("notification step 6")?;
    let muted = h.actor(false).await.context("notification step 7")?;
    for actor in [user, &recipient, &disabled, &muted] {
        h.grant(actor, &app, "dev")
            .await
            .context("notification step 8")?;
    }
    sqlx::query("UPDATE users SET enabled=false WHERE id=$1")
        .bind(&disabled.id)
        .execute(&h.state.pool)
        .await
        .context("notification step 9")?;
    sqlx::query("INSERT INTO notification_mutes(user_id,app_id,kind) VALUES($1,$2,'message')")
        .bind(&muted.id)
        .bind(&app.app)
        .execute(&h.state.pool)
        .await
        .context("notification step 10")?;
    let base = format!(
        "/api/v1/installations/{}/platform/notifications",
        app.installation
    );
    let request = Uuid::new_v4();
    let body = json!({"requestId":request,
        "recipients":[recipient.id, outsider.id, disabled.id, muted.id, "no-such-user"],
        "kind":"message","groupKey":"conversation:1","title":"앨리스 · 새 메시지 2개",
        "path":"/conversations/1","resourceId":"1"});
    // Not approved yet, and never from a credential that is not an app call.
    h.replace(admin, &app, app.release, &both[1..])
        .await
        .context("notification step 11")?;
    let proof = fresh_proof(h, user, &app)
        .await
        .context("notification step 12")?;
    denied(
        h.request(&proof, Method::POST, &base)
            .json(&body)
            .send()
            .await?,
    )
    .await
    .context("notification step 13")?;
    h.replace(admin, &app, app.release, &both)
        .await
        .context("notification step 14")?;
    h.replace(admin, &other, other.release, &permission)
        .await
        .context("notification step 15")?;
    for token in [&user.token, &admin.token] {
        denied(
            h.request(token, Method::POST, &base)
                .json(&body)
                .send()
                .await?,
        )
        .await
        .context("notification step 16")?;
    }
    let proof = fresh_proof(h, user, &app)
        .await
        .context("notification step 17")?;
    let published = h
        .json(&proof, Method::POST, &base, body.clone(), 200)
        .await
        .context("notification step 18")?;
    ensure!(
        published == json!({"requestId":request,"accepted":1}),
        "notification recipients were not filtered to permitted, active, unmuted people"
    );
    ensure!(
        inbox(h, &recipient.id, &app).await?
            == vec![(
                "conversation:1".into(),
                "앨리스 · 새 메시지 2개".into(),
                false
            )]
            && inbox(h, &outsider.id, &app).await?.is_empty()
            && inbox(h, &disabled.id, &app).await?.is_empty()
            && inbox(h, &muted.id, &app).await?.is_empty(),
        "a notification reached someone outside the app's users"
    );
    // The same request ID answers the first result; a different body is a conflict.
    let again = h
        .json(&proof, Method::POST, &base, body.clone(), 200)
        .await
        .context("notification step 19")?;
    ensure!(again == published, "a repeated request ID published twice");
    let mut changed = body.clone();
    changed["title"] = json!("다른 내용");
    response(
        h.request(&proof, Method::POST, &base)
            .json(&changed)
            .send()
            .await?,
        409,
    )
    .await
    .context("notification step 20")?;
    let looked_up = response(
        h.request(&proof, Method::GET, &format!("{base}/by-request/{request}"))
            .send()
            .await?,
        200,
    )
    .await
    .context("notification step 21")?;
    ensure!(
        looked_up == published,
        "the request lookup lost the first result"
    );
    // A newer notification of the same group replaces the line; invalid input is refused.
    let replace = json!({"requestId":Uuid::new_v4(),"recipients":[recipient.id],"kind":"message",
        "groupKey":"conversation:1","title":"앨리스 · 새 메시지 3개","path":"/conversations/1"});
    h.json(&proof, Method::POST, &base, replace, 200)
        .await
        .context("notification step 22")?;
    ensure!(
        inbox(h, &recipient.id, &app).await?
            == vec![(
                "conversation:1".into(),
                "앨리스 · 새 메시지 3개".into(),
                false
            )],
        "a newer notification did not replace its group"
    );
    for invalid in [
        json!({"requestId":Uuid::new_v4(),"recipients":[],"kind":"message","groupKey":"g","title":"t","path":"/"}),
        json!({"requestId":Uuid::new_v4(),"recipients":[recipient.id],"kind":"Message","groupKey":"g","title":"t","path":"/"}),
        json!({"requestId":Uuid::new_v4(),"recipients":[recipient.id],"kind":"message","groupKey":"g","title":"t","path":"https://x"}),
    ] {
        response(
            h.request(&proof, Method::POST, &base)
                .json(&invalid)
                .send()
                .await?,
            400,
        )
        .await
        .context("notification step 23")?;
    }
    // Another installation's call context cannot touch this installation's groups.
    h.grant(user, &other, "dev")
        .await
        .context("notification step 24")?;
    let other_proof = fresh_proof(h, user, &other)
        .await
        .context("notification step 25")?;
    denied(
        h.request(&other_proof, Method::POST, &format!("{base}/retract"))
            .json(&json!({"groupKey":"conversation:1"}))
            .send()
            .await?,
    )
    .await
    .context("notification step 26")?;
    // The recipient reads the conversation in the app: their own line turns read.
    let recipient_proof = fresh_proof(h, &recipient, &app)
        .await
        .context("notification step 27")?;
    status_of(
        h.request(&recipient_proof, Method::POST, &format!("{base}/read"))
            .json(&json!({"groupKey":"conversation:1"}))
            .send()
            .await?,
        204,
    )?;
    ensure!(
        inbox(h, &recipient.id, &app)
            .await?
            .first()
            .is_some_and(|row| row.2),
        "reading in the app did not mark the notification read"
    );
    let proof = fresh_proof(h, user, &app)
        .await
        .context("notification step 28")?;
    let retracted = h
        .json(
            &proof,
            Method::POST,
            &format!("{base}/retract"),
            json!({"groupKey":"conversation:1"}),
            200,
        )
        .await
        .context("notification step 29")?;
    ensure!(
        retracted == json!({"retracted":1}) && inbox(h, &recipient.id, &app).await?.is_empty(),
        "retracting a group left notifications behind"
    );
    // Group recipients: current members of a platform group, with the same
    // filters, only for an app approved to read the directory.
    let member = h.actor(false).await.context("notification step 30")?;
    h.grant(&member, &app, "dev")
        .await
        .context("notification step 31")?;
    let group = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO groups(id,name,source) VALUES($1,'Notification fixture group','local')",
    )
    .bind(group)
    .execute(&h.state.pool)
    .await?;
    // The caller is a member too: group expansion never adds them.
    h.grant(user, &app, "dev").await.ok();
    for person in [&member.id, &outsider.id, &muted.id, &user.id] {
        sqlx::query("INSERT INTO group_members(group_id,user_id) VALUES($1,$2)")
            .bind(group)
            .bind(person)
            .execute(&h.state.pool)
            .await?;
    }
    let grouped = json!({"requestId":Uuid::new_v4(),"recipients":[],"recipientGroups":[group],
        "kind":"message","groupKey":"conversation:2","title":"그룹 알림","path":"/conversations/2"});
    let proof = fresh_proof(h, user, &app)
        .await
        .context("notification step 32")?;
    let reached = h
        .json(&proof, Method::POST, &base, grouped.clone(), 200)
        .await
        .context("notification step 33")?;
    let other_base = format!(
        "/api/v1/installations/{}/platform/notifications",
        other.installation
    );
    let other_proof = fresh_proof(h, user, &other)
        .await
        .context("notification step 34")?;
    let denied_group = h
        .json(
            &other_proof,
            Method::POST,
            &other_base,
            json!({"requestId":Uuid::new_v4(),"recipients":[],"recipientGroups":[group],
                "kind":"message","groupKey":"g","title":"그룹","path":"/g"}),
            403,
        )
        .await
        .context("notification step 35")?;
    ensure!(denied_group["error"]["code"] == "installation_permission_denied");
    ensure!(
        reached["accepted"] == 1
            && !inbox(h, &user.id, &app)
                .await?
                .iter()
                .any(|row| row.0 == "conversation:2")
            && inbox(h, &member.id, &app)
                .await?
                .iter()
                .any(|row| row.0 == "conversation:2"),
        "group recipients did not reach exactly the permitted, unmuted members"
    );
    Ok(())
}

fn status_of(value: Response, expected: u16) -> Result<()> {
    ensure!(
        value.status().as_u16() == expected,
        "expected HTTP {expected}, received HTTP {}",
        value.status().as_u16()
    );
    Ok(())
}
