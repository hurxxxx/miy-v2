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
    ensure!(
        value.status().as_u16() == expected,
        "permission fixture expected HTTP {expected}, received HTTP {}",
        value.status().as_u16()
    );
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
        self.pending_body(actor, source.release, &source.connection(target))
            .await
    }
    async fn pending_body(
        &self,
        actor: &Actor,
        release: Uuid,
        path: &str,
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
        let request = format!(
            "POST {} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {}\r\nX-Miy-Release: {}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n",
            path, actor.token, release
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

pub async fn run(state: &State, client: &Client, base: &str) -> Result<Vec<String>> {
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
    let initial = h.read(&user, &a, a.release).await?;
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
    let approved = h.read(&user, &a, a.release).await?;
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
        h.read(&user, &a, a.release).await?["revision"] == 1,
        "rejected approval changed revision"
    );
    let next_release = Uuid::new_v4();
    let mut next_permissions = a_permissions.clone();
    next_permissions.push(format!("app:{}:write", c.app));
    sqlx::query("INSERT INTO releases(id,app_id,version,artifact_hash,manifest) SELECT $1,app_id,'2.0.0',artifact_hash,jsonb_set(jsonb_set(manifest,'{version}','\"2.0.0\"'::jsonb),'{requestedPermissions}',$2) FROM releases WHERE id=$3")
        .bind(next_release).bind(json!(next_permissions)).bind(a.release).execute(&state.pool).await?;
    ensure!(
        h.read(&user, &a, next_release).await?["approvedPermissions"] == json!([]),
        "new release inherited old approvals"
    );
    sqlx::query("UPDATE installations SET release_id=$2 WHERE id=$1")
        .bind(a.installation)
        .bind(next_release)
        .execute(&state.pool)
        .await?;
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
    response(
        h.update(&admin, &a, next_release, 0, &[a_permissions[0].clone()])
            .await?,
        200,
    )
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
    passed.push("approvals reject foreign releases and undeclared grants; new releases and prod never inherit dev approval".into());

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
    let preview = response(
        h.request(&user.token, Method::POST, &preview_path)
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
    for boundary in ["development grant", "installation approval"] {
        let before = b.calls();
        let mut pending = h.pending_body(&user, a.release, &preview_path).await?;
        if boundary == "development grant" {
            sqlx::query("DELETE FROM grants WHERE user_id=$1 AND app_id=$2 AND environment='dev' AND permission='develop'")
                .bind(&user.id).bind(&a.app).execute(&state.pool).await?;
        } else {
            h.replace(&admin, &a, a.release, &[]).await?;
        }
        pending.write_all(b"{}").await?;
        let mut result = Vec::new();
        tokio::time::timeout(Duration::from_secs(5), pending.read_to_end(&mut result))
            .await
            .context("held preview request did not finish")??;
        ensure!(
            result.starts_with(b"HTTP/1.1 403 ") && b.calls() == before,
            "preview {boundary} revoked during body read reached the target backend"
        );
        sqlx::query("INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,$2,'dev','develop') ON CONFLICT DO NOTHING")
            .bind(&user.id).bind(&a.app).execute(&state.pool).await?;
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

    let http = h.read(&user, &a, a.release).await?;
    let mcp = h
        .mcp(
            &user,
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
        h.read(&user, &a, a.release).await? == approved_mcp["data"],
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
    let browser = h.actor(false).await?;
    for app in [&a, &b, &c] {
        h.grant(&browser, app, "dev").await?;
    }
    let session = auth::random();
    let csrf = auth::random();
    let cookie = format!("miy_development={session}");
    sqlx::query("INSERT INTO development_sessions(id_hash,user_id,csrf,expires_at) VALUES($1,$2,$3,now()+interval '5 minutes')")
        .bind(hash(&session)).bind(&browser.id).bind(&csrf).execute(&state.pool).await?;
    let called = response(
        client
            .get(format!("{}{}", base, a.connection(&b)))
            .header("Cookie", &cookie)
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        called["verified"] == true,
        "browser app call lacked signed identity"
    );
    let browser_proof = b.proof()?;
    h.call(&browser_proof, &b, &c).await?;
    let browser_child = c.proof()?;
    response(
        client
            .post(format!("{base}/auth/logout"))
            .header("Cookie", &cookie)
            .header("x-miy-csrf", &csrf)
            .header("Origin", base)
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
    Ok(passed)
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
        format!("/api/v1/groups/{local}/members"),
        json!({"userId":carol.id,"member":false}),
        200,
    )
    .await?;
    ensure!(
        h.visible(&carol, &dev).await?.is_none(),
        "removed member kept use"
    );
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
    ensure!(
        h.visible(&operator, &prod).await?.is_some(),
        "administrator could not see prod"
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
    passed.push("the company switch stops use for administrators and users while developers keep dev, and re-enabling restores it".into());

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
    Ok(())
}
