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
    /// A filter or cursor must not bypass the default installation admission policy.
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
                .any(|row| row["id"] == json!(app.installation));
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
    // Independent fixtures: a failing check here does not hide the others.
    let mut stopped = Vec::new();
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
    ensure!(stopped.is_empty(), "{}", stopped.join("; "));
    Ok(passed)
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
    // remove it, but it is not openable in prod; others cannot read this list.
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
        denied(
            h.request(&actor.token, Method::GET, "/api/v1/runtime/installations")
                .send()
                .await?,
        )
        .await?;
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
    passed.push("the company switch consistently filters default, paged and app-detail installations; developers keep dev and re-enabling restores use; administrators keep the disabled app in runtime management, unopenable in prod, and non-administrators cannot read that list".into());

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
    passed.push("personal grant changes (including registration approval), user state changes and primary-group assignments retain their exact before/after targets in the same transaction audit".into());
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
    for (environment, installation) in [("dev", &dev), ("prod", &prod)] {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO runtime_jobs(id,app_id,environment,actor,action,installation_id,release_id,idempotency_key,input_hash,status,step) VALUES($1,$2,$3,$4,'restart',$5,$6,$7,'policy-matrix','succeeded','completed')")
            .bind(id).bind(&app).bind(environment).bind(&admin.id)
            .bind(installation.installation).bind(installation.release)
            .bind(format!("policy-matrix-{environment}-{suffix}"))
            .execute(&h.state.pool).await?;
        jobs.push((environment, id));
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
            open: [both, &["dev"]],
            observe: [both, both],
            manage: both,
            promote: true,
        },
        PolicyCase {
            name: "deployer",
            token: deployer.token.clone(),
            open: [both, &["dev"]],
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
            observe: [both, &[]],
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
            open: [&["prod"], &[]],
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
            for row in default.iter().chain(&paged) {
                let environment = row["environment"].as_str().unwrap_or_default();
                ensure!(
                    row["canManage"] == json!(case.manage.contains(&environment))
                        && row["canPromote"] == json!(case.promote),
                    "{context}: display values differ from the management and promotion decisions"
                );
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
    passed.push("default and paged installation lists return the same records, and with installation approval records, job details, job lists and display values agree for administrators, deployers, developers, users, strangers and delegated credentials with and without read, with the company switch off and on".into());
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
    for after in [Some(unit), Some(team), None] {
        h.json(
            &operator.token,
            Method::PUT,
            &format!("/api/v1/users/{}/primary-group", user.id),
            json!({"groupId":after}),
            200,
        )
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
