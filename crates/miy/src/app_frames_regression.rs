//! Business app frames against actual PostgreSQL and the Core HTTP listeners, only
//! in the isolated regression stack (candidate design docs/APP-FRAMES.md, FV-03,
//! FV-04 server side, FV-05 server side and FV-10). Browser accounts are synthetic
//! development sessions and the frame origin is a port-pool name sent as Host; this
//! is not a real browser, TLS or DNS test. Credentials stay in memory.
use crate::{State, auth, contract::hash};
use anyhow::{Context, Result, ensure};
use axum::{Json, Router, extract::State as S, http::HeaderMap, routing::any};
use reqwest::{Client, Method, RequestBuilder, Response};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;
use uuid::Uuid;

/// A browser portal session (development login) and where it lives.
pub(crate) struct PortalSession {
    pub cookie: String,
    pub csrf: String,
    pub origin: String,
}

/// An opened app frame, addressed the way a browser on its origin would be.
pub(crate) struct FrameClient<'a> {
    client: &'a Client,
    /// Where the frame listener really listens (the harness socket).
    address: String,
    pub origin: String,
    host: String,
    pub token: String,
    pub id: Uuid,
    pub release: Uuid,
    pub asset_base: String,
    pub bootstrap: Value,
}
impl FrameClient<'_> {
    /// A browser-shaped frame request: same-origin Fetch Metadata for reads, the
    /// frame Origin for writes, the frame token and the current release.
    pub fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let read = matches!(method, Method::GET | Method::HEAD);
        let builder = self
            .raw(method, path)
            .bearer_auth(&self.token)
            .header("x-miy-release", self.release.to_string());
        if read {
            builder.header("sec-fetch-site", "same-origin")
        } else {
            builder.header("origin", &self.origin)
        }
    }
    /// Only the Host of this frame origin; the caller adds everything else.
    pub fn raw(&self, method: Method, path: &str) -> RequestBuilder {
        self.client
            .request(method, format!("{}{path}", self.address))
            .header("host", &self.host)
    }
}

pub(crate) fn host_of(origin: &str) -> Result<String> {
    let url = url::Url::parse(origin)?;
    Ok(match url.port() {
        Some(port) => format!("{}:{port}", url.host_str().context("frame host")?),
        None => url.host_str().context("frame host")?.to_owned(),
    })
}

/// `POST /api/v1/app-frames` exactly as the portal page sends it.
pub(crate) async fn launch_code(
    client: &Client,
    state: &State,
    portal: &PortalSession,
    purpose: auth::BrowserPurpose,
    installation: Uuid,
) -> Result<Response> {
    Ok(client
        .post(format!("{}/api/v1/app-frames", portal.origin))
        .header(
            "cookie",
            format!(
                "{}={}",
                state.auth.cookie_name(purpose, "development"),
                portal.cookie
            ),
        )
        .header("origin", &portal.origin)
        .header("x-miy-csrf", &portal.csrf)
        .json(&json!({"installationId":installation}))
        .send()
        .await?)
}

/// The launch form POST into the frame (`code` plus the app location).
pub(crate) async fn submit_launch(
    client: &Client,
    address: &str,
    frame_origin: &str,
    origin: Option<&str>,
    code: &str,
) -> Result<Response> {
    let mut request = client
        .post(format!("{address}/_miy/launch"))
        .header("host", host_of(frame_origin)?)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(format!(
            "code={code}&pathname=%2Fitems&search=%3Fview%3Dfull&basePath=%2Fapps%2Ffixture"
        ));
    if let Some(origin) = origin {
        request = request.header("origin", origin);
    }
    Ok(request.send().await?)
}

pub(crate) fn bootstrap(html: &str) -> Result<Value> {
    let start = html
        .split_once("<script id=\"miy-frame-bootstrap\" type=\"application/json\">")
        .context("frame document without bootstrap")?
        .1;
    let json = start
        .split_once("</script>")
        .context("unterminated bootstrap")?
        .0;
    Ok(serde_json::from_str(json)?)
}

/// Launch and open one frame of `installation` for `portal`.
pub(crate) async fn open<'a>(
    client: &'a Client,
    state: &State,
    address: &str,
    portal: &PortalSession,
    purpose: auth::BrowserPurpose,
    installation: Uuid,
) -> Result<FrameClient<'a>> {
    let launch: Value = status(
        launch_code(client, state, portal, purpose, installation).await?,
        200,
    )
    .await?
    .json()
    .await?;
    let origin = launch["frameOrigin"]
        .as_str()
        .context("launch without frame origin")?
        .to_owned();
    let code = launch["code"].as_str().context("launch without code")?;
    let page = status(
        submit_launch(client, address, &origin, Some(&portal.origin), code).await?,
        200,
    )
    .await?;
    let value = bootstrap(&page.text().await?)?;
    ensure!(
        value["frameId"] == launch["frameId"],
        "the frame opened with another id than its launch"
    );
    Ok(FrameClient {
        client,
        address: address.to_owned(),
        host: host_of(&origin)?,
        token: value["token"]
            .as_str()
            .context("bootstrap token")?
            .to_owned(),
        id: value["frameId"].as_str().context("frame id")?.parse()?,
        release: value["releaseId"].as_str().context("release")?.parse()?,
        asset_base: value["assetBase"]
            .as_str()
            .context("asset base")?
            .to_owned(),
        origin,
        bootstrap: value,
    })
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
    match claims {
        Some(claims) => {
            backend.capture.lock().unwrap().push(claims);
            Json(json!({"verified":true}))
        }
        None => Json(json!({"verified":false})),
    }
}

struct App {
    app: String,
    installation: Uuid,
    release: Uuid,
    artifact: String,
    capture: Arc<Mutex<Vec<Value>>>,
}
impl App {
    fn last(&self) -> Result<Value> {
        self.capture
            .lock()
            .map_err(|_| anyhow::anyhow!("capture poisoned"))?
            .last()
            .cloned()
            .context("fixture app was not called")
    }
    fn calls(&self) -> usize {
        self.capture.lock().unwrap().len()
    }
    fn proof(&self) -> Result<String> {
        self.last()?["delegation_token"]
            .as_str()
            .map(str::to_owned)
            .context("identity without continuation proof")
    }
}

struct Harness<'a> {
    state: &'a State,
    client: &'a Client,
    portal: &'a str,
    dev_portal: &'a str,
    address: &'a str,
    backends: Vec<JoinHandle<()>>,
}
impl Drop for Harness<'_> {
    fn drop(&mut self) {
        for task in &self.backends {
            task.abort();
        }
    }
}
async fn status(response: Response, expected: u16) -> Result<Response> {
    // Never format successful bodies: frame documents and launches carry
    // credentials. An error body carries only a stable code.
    let received = response.status().as_u16();
    if received != expected {
        let code = if received >= 400 {
            response
                .json::<Value>()
                .await
                .ok()
                .and_then(|v| v["error"]["code"].as_str().map(str::to_owned))
                .unwrap_or_default()
        } else {
            String::new()
        };
        anyhow::bail!("frame regression expected HTTP {expected}, received HTTP {received} {code}");
    }
    Ok(response)
}
async fn json(response: Response, expected: u16) -> Result<Value> {
    Ok(status(response, expected).await?.json().await?)
}
async fn relaunched(response: Response) -> Result<()> {
    let response = status(response, 410).await?;
    ensure!(
        response
            .headers()
            .get("content-security-policy")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("frame-ancestors")),
        "relaunch document lost its ancestor restriction"
    );
    let text = response.text().await?;
    ensure!(
        text.contains("miy-parent-origin") && !text.contains("miy-frame-bootstrap"),
        "relaunch document must ask the portal to relaunch and carry no credential"
    );
    Ok(())
}

impl Harness<'_> {
    async fn user(&self) -> Result<String> {
        let id = format!("frame-user-{}", Uuid::new_v4());
        sqlx::query("INSERT INTO users(id,name,admin) VALUES($1,$1,false)")
            .bind(&id)
            .execute(&self.state.pool)
            .await?;
        Ok(id)
    }
    async fn session(&self, user: &str, purpose: auth::BrowserPurpose) -> Result<PortalSession> {
        let cookie = auth::random();
        let csrf = auth::random();
        sqlx::query("INSERT INTO development_sessions(id_hash,user_id,csrf,expires_at,browser_purpose) VALUES($1,$2,$3,now()+interval '8 hours',$4)")
            .bind(hash(&cookie)).bind(user).bind(&csrf).bind(purpose.as_str())
            .execute(&self.state.pool).await?;
        Ok(PortalSession {
            cookie,
            csrf,
            origin: match purpose {
                auth::BrowserPurpose::DevPortal => self.dev_portal.to_owned(),
                _ => self.portal.to_owned(),
            },
        })
    }
    async fn grant(&self, user: &str, app: &App) -> Result<()> {
        sqlx::query("INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,$2,'dev','use') ON CONFLICT DO NOTHING")
            .bind(user).bind(&app.app).execute(&self.state.pool).await?;
        Ok(())
    }
    async fn approve(&self, app: &App, permissions: &[String]) -> Result<()> {
        let admin = format!("frame-admin-{}", Uuid::new_v4());
        sqlx::query("INSERT INTO users(id,name,admin) VALUES($1,$1,true)")
            .bind(&admin)
            .execute(&self.state.pool)
            .await?;
        sqlx::query("INSERT INTO installation_permissions(installation_id,release_id,revision,permissions,approved_by) VALUES($1,$2,1,$3,$4) ON CONFLICT (installation_id,release_id) DO UPDATE SET revision=installation_permissions.revision+1,permissions=EXCLUDED.permissions")
            .bind(app.installation).bind(app.release).bind(json!(permissions)).bind(&admin)
            .execute(&self.state.pool).await?;
        Ok(())
    }
    async fn app(&mut self, name: &str, permissions: &[String]) -> Result<App> {
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
        let artifact = hash(format!("frame-fixture:{installation}"));
        let directory = self.state.artifacts.join(&artifact).join("ui");
        tokio::fs::create_dir_all(&directory).await?;
        tokio::fs::write(
            directory.join("remoteEntry.js"),
            "export const frameProbe = true;\n",
        )
        .await?;
        sqlx::query("INSERT INTO apps(id,name) VALUES($1,'Frame fixture') ON CONFLICT DO NOTHING")
            .bind(name)
            .execute(&self.state.pool)
            .await?;
        let manifest = json!({
            "contractVersion":"miy.app/v1alpha1","id":name,"name":"Frame fixture","version":"1.0.0",
            "runtime":{"profile":"rust-web-v1"},
            "ui":{"delivery":"module","moduleEntry":"remoteEntry.js","moduleExport":"./AppRoot","contractVersion":"miy.ui/v1alpha1"},
            "resources":{"database":"postgresql"},"migrations":{"directory":"migrations"},"requestedPermissions":permissions,
        });
        sqlx::query("INSERT INTO releases(id,app_id,version,artifact_hash,manifest) VALUES($1,$2,'1.0.0',$3,$4)")
            .bind(release).bind(name).bind(&artifact).bind(manifest).execute(&self.state.pool).await?;
        sqlx::query("INSERT INTO installations(id,app_id,environment,release_id,db_status,runtime_status,desired_state,runtime_port,runtime_secret) VALUES($1,$2,'dev',$3,'ready','running','running',$4,$5)")
            .bind(installation).bind(name).bind(release).bind(port).bind(auth::encrypt(&secret)?)
            .execute(&self.state.pool).await?;
        Ok(App {
            app: name.to_owned(),
            installation,
            release,
            artifact,
            capture,
        })
    }
    async fn open(&self, portal: &PortalSession, app: &App) -> Result<FrameClient<'_>> {
        open(
            self.client,
            self.state,
            self.address,
            portal,
            auth::BrowserPurpose::DevPortal,
            app.installation,
        )
        .await
    }
    /// A backend continuation call with an issued `miy_call_` proof.
    async fn call(&self, proof: &str, from: &App, to: &App) -> Result<Response> {
        Ok(self
            .client
            .get(format!(
                "{}/api/v1/installations/{}/connections/{}/api/probe",
                self.portal, from.installation, to.app
            ))
            .bearer_auth(proof)
            .header("x-miy-release", from.release.to_string())
            .send()
            .await?)
    }
}

pub async fn run(
    state: &State,
    client: &Client,
    portal: &str,
    dev_portal: &str,
    address: &str,
) -> Result<Vec<String>> {
    let mut passed = Vec::new();
    let mut h = Harness {
        state,
        client,
        portal,
        dev_portal,
        address,
        backends: Vec::new(),
    };
    let suffix = Uuid::new_v4().simple().to_string();
    let (a_id, b_id, c_id) = (
        format!("frame-a-{suffix}"),
        format!("frame-b-{suffix}"),
        format!("frame-c-{suffix}"),
    );
    // A continuation keeps only what every earlier hop was approved for, so A and
    // B also request what C is called with.
    let directory = "platform:directory:read".to_owned();
    let a_permissions = [
        format!("app:{b_id}:read"),
        format!("app:{c_id}:read"),
        directory.clone(),
    ];
    let b_permissions = [format!("app:{c_id}:read"), directory.clone()];
    let a = h.app(&a_id, &a_permissions).await?;
    let b = h.app(&b_id, &b_permissions).await?;
    let c = h.app(&c_id, std::slice::from_ref(&directory)).await?;
    // Later installations for origin reuse and pool exhaustion (frames borrow h).
    let d = h.app(&format!("frame-d-{suffix}"), &[]).await?;
    let e = h.app(&format!("frame-e-{suffix}"), &[]).await?;
    let user = h.user().await?;
    for app in [&a, &b, &c] {
        h.grant(&user, app).await?;
    }
    let dev = h.session(&user, auth::BrowserPurpose::DevPortal).await?;
    let prod = h.session(&user, auth::BrowserPurpose::Portal).await?;

    // 1. Launch codes: one-time, short-lived, bound to their portal and host.
    let frame = h.open(&dev, &a).await?;
    ensure!(
        frame.bootstrap["protocol"] == "miy.frame/v1"
            && frame.bootstrap["parentOrigin"] == dev.origin
            && frame.bootstrap["installationId"] == json!(a.installation)
            && frame.bootstrap["location"]["pathname"] == "/items"
            && frame.bootstrap["location"]["search"] == "?view=full"
            && frame.bootstrap["assetBase"]
                .as_str()
                .is_some_and(|v| v.starts_with("/_miy/a/") && v.ends_with("/ui/")),
        "frame bootstrap lost its binding"
    );
    let b_frame = h.open(&dev, &b).await?;
    ensure!(
        b_frame.origin != frame.origin,
        "two installations shared a frame origin"
    );
    let again = h.open(&dev, &a).await?;
    ensure!(
        again.origin == frame.origin && again.id != frame.id,
        "an installation's frame origin changed between launches"
    );
    let code = || async {
        let value = json(
            launch_code(
                client,
                state,
                &dev,
                auth::BrowserPurpose::DevPortal,
                a.installation,
            )
            .await?,
            200,
        )
        .await?;
        Ok::<String, anyhow::Error>(value["code"].as_str().context("code")?.to_owned())
    };
    let used = code().await?;
    status(
        submit_launch(client, address, &frame.origin, Some(&dev.origin), &used).await?,
        200,
    )
    .await?;
    relaunched(submit_launch(client, address, &frame.origin, Some(&dev.origin), &used).await?)
        .await?;
    // The reload of a launched frame resends the consumed code with Origin null.
    relaunched(submit_launch(client, address, &frame.origin, Some("null"), &used).await?).await?;
    let wrong_origin = code().await?;
    relaunched(submit_launch(client, address, &frame.origin, Some(portal), &wrong_origin).await?)
        .await?;
    relaunched(
        submit_launch(
            client,
            address,
            &frame.origin,
            Some(&dev.origin),
            &wrong_origin,
        )
        .await?,
    )
    .await?;
    let wrong_host = code().await?;
    relaunched(
        submit_launch(
            client,
            address,
            &b_frame.origin,
            Some(&dev.origin),
            &wrong_host,
        )
        .await?,
    )
    .await?;
    let expired = code().await?;
    sqlx::query("UPDATE app_frame_codes SET expires_at=created_at WHERE code_hash=$1")
        .bind(hash(&expired))
        .execute(&state.pool)
        .await?;
    relaunched(submit_launch(client, address, &frame.origin, Some(&dev.origin), &expired).await?)
        .await?;
    relaunched(frame.raw(Method::GET, "/_miy/launch").send().await?).await?;
    // A dev installation launches only from the dev portal, and only for a browser.
    status(
        launch_code(
            client,
            state,
            &prod,
            auth::BrowserPurpose::Portal,
            a.installation,
        )
        .await?,
        403,
    )
    .await?;
    let cli = format!("miy_cli_{}", auth::random());
    sqlx::query("INSERT INTO cli_tokens(token_hash,user_id) VALUES($1,$2)")
        .bind(hash(&cli))
        .bind(&user)
        .execute(&state.pool)
        .await?;
    status(
        client
            .post(format!("{portal}/api/v1/app-frames"))
            .bearer_auth(&cli)
            .json(&json!({"installationId":a.installation}))
            .send()
            .await?,
        403,
    )
    .await?;
    passed.push("launch codes are one-time, expire, and open only their installation's own origin from the issuing portal; reloads, replays, other origins and other hosts get a credential-free relaunch document; prod portal sessions and CLI bearers cannot launch a dev frame".into());

    // 2. Browser request rules on the frame listener.
    let probe = json(
        frame.request(Method::GET, "/_miy/api/probe").send().await?,
        200,
    )
    .await?;
    ensure!(
        probe["verified"] == true,
        "frame API lacked signed identity"
    );
    let identity = a.last()?;
    ensure!(
        identity["sub"] == user.as_str()
            && identity["admin"] == false
            && identity["callers"] == json!([])
            && identity["environment"] == "dev",
        "frame API identity changed"
    );
    status(
        frame
            .raw(Method::GET, "/_miy/api/probe")
            .bearer_auth(&frame.token)
            .send()
            .await?,
        403,
    )
    .await?;
    for site in ["same-site", "cross-site", "none"] {
        status(
            frame
                .raw(Method::GET, "/_miy/api/probe")
                .bearer_auth(&frame.token)
                .header("sec-fetch-site", site)
                .send()
                .await?,
            403,
        )
        .await?;
    }
    for origin in ["null", dev.origin.as_str(), b_frame.origin.as_str()] {
        status(
            frame
                .request(Method::GET, "/_miy/api/probe")
                .header("origin", origin)
                .send()
                .await?,
            403,
        )
        .await?;
    }
    status(
        frame
            .raw(Method::POST, "/_miy/api/probe")
            .bearer_auth(&frame.token)
            .header("x-miy-release", a.release.to_string())
            .send()
            .await?,
        403,
    )
    .await?;
    json(
        frame
            .request(Method::POST, "/_miy/api/probe")
            .send()
            .await?,
        200,
    )
    .await?;
    status(
        frame
            .raw(Method::POST, "/_miy/api/probe")
            .bearer_auth(&frame.token)
            .header("origin", &frame.origin)
            .header("x-miy-release", Uuid::new_v4().to_string())
            .send()
            .await?,
        409,
    )
    .await?;
    // A's token at B's origin, and an unregistered origin of the pool.
    status(
        b_frame
            .raw(Method::GET, "/_miy/api/probe")
            .bearer_auth(&frame.token)
            .header("sec-fetch-site", "same-origin")
            .send()
            .await?,
        403,
    )
    .await?;
    let unregistered = format!("{}:1", frame.origin.rsplit_once(':').context("port")?.0);
    status(
        client
            .get(format!("{address}/_miy/api/probe"))
            .header("host", host_of(&unregistered)?)
            .bearer_auth(&frame.token)
            .header("sec-fetch-site", "same-origin")
            .send()
            .await?,
        404,
    )
    .await?;
    // A frame token is no CLI, portal or console credential.
    for (base, with_origin) in [(portal, false), (portal, true), (dev_portal, false)] {
        let mut request = client
            .get(format!("{base}/api/v1/me"))
            .bearer_auth(&frame.token);
        if with_origin {
            request = request.header("origin", &frame.origin);
        }
        let code = request.send().await?.status().as_u16();
        ensure!(
            matches!(code, 401 | 403),
            "frame token used outside its frame: HTTP {code}"
        );
    }
    passed.push("frame requests need their registered installation host, same-origin Fetch Metadata for reads and the exact frame Origin for writes; null, portal and sibling Origins, another installation's host, unregistered pool origins and other Core APIs refuse a frame token".into());

    // 3. Another app only through the approved connection; callers start at A.
    let before = b.calls();
    status(
        frame
            .request(Method::GET, &format!("/_miy/connections/{b_id}/api/probe"))
            .send()
            .await?,
        403,
    )
    .await?;
    ensure!(
        b.calls() == before,
        "an unapproved frame reached another app"
    );
    status(
        frame
            .request(Method::GET, "/_miy/platform/directory")
            .send()
            .await?,
        403,
    )
    .await?;
    h.approve(&a, &a_permissions).await?;
    h.approve(&b, &b_permissions).await?;
    h.approve(&c, std::slice::from_ref(&directory)).await?;
    json(
        frame
            .request(Method::GET, &format!("/_miy/connections/{b_id}/api/probe"))
            .send()
            .await?,
        200,
    )
    .await?;
    let identity = b.last()?;
    ensure!(
        identity["sub"] == user.as_str()
            && identity["admin"] == false
            && identity["callers"][0]["installation_id"] == json!(a.installation),
        "frame connection did not start its chain at the frame installation"
    );
    let b_proof = b.proof()?;
    json(h.call(&b_proof, &b, &c).await?, 200).await?;
    let identity = c.last()?;
    ensure!(
        identity["admin"] == false
            && identity["callers"]
                .as_array()
                .is_some_and(|callers| callers.len() == 2),
        "frame-originated continuation lost its chain or carried administrator status"
    );
    let c_proof = c.proof()?;
    json(
        client
            .get(format!(
                "{portal}/api/v1/installations/{}/platform/directory",
                c.installation
            ))
            .bearer_auth(&c_proof)
            .send()
            .await?,
        200,
    )
    .await?;
    let users = json(
        frame
            .request(Method::GET, "/_miy/platform/directory")
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        users["users"].is_array(),
        "approved frame directory changed shape"
    );
    passed.push("an app frame reaches another app and the directory only through its own approvals; the chain starts at the frame installation and every hop is non-admin".into());

    // 4. Revocation: parent logout, frame revocation, use revocation, release change.
    let logout = status(
        client
            .post(format!("{dev_portal}/auth/logout"))
            .header(
                "cookie",
                format!(
                    "{}={}",
                    state
                        .auth
                        .cookie_name(auth::BrowserPurpose::DevPortal, "development"),
                    dev.cookie
                ),
            )
            .header("x-miy-csrf", &dev.csrf)
            .header("origin", dev_portal)
            .send()
            .await?,
        200,
    )
    .await;
    logout?;
    status(
        frame.request(Method::GET, "/_miy/api/probe").send().await?,
        401,
    )
    .await?;
    for (proof, target) in [(&b_proof, &c), (&c_proof, &c)] {
        let code = client
            .get(format!(
                "{portal}/api/v1/installations/{}/platform/directory",
                target.installation
            ))
            .bearer_auth(proof)
            .send()
            .await?
            .status()
            .as_u16();
        ensure!(
            matches!(code, 401 | 403),
            "continuation survived its parent logout: HTTP {code}"
        );
    }
    let remains: i64 = sqlx::query_scalar("SELECT count(*) FROM app_frame_sessions WHERE id=$1")
        .bind(frame.id)
        .fetch_one(&state.pool)
        .await?;
    ensure!(remains == 0, "logout kept the frame session");
    let dev = h.session(&user, auth::BrowserPurpose::DevPortal).await?;
    let frame = h.open(&dev, &a).await?;
    sqlx::query("UPDATE app_frame_sessions SET revoked_at=now() WHERE id=$1")
        .bind(frame.id)
        .execute(&state.pool)
        .await?;
    status(
        frame.request(Method::GET, "/_miy/api/probe").send().await?,
        401,
    )
    .await?;
    let frame = h.open(&dev, &a).await?;
    sqlx::query("DELETE FROM grants WHERE user_id=$1 AND app_id=$2")
        .bind(&user)
        .bind(&a.app)
        .execute(&state.pool)
        .await?;
    status(
        frame.request(Method::GET, "/_miy/api/probe").send().await?,
        403,
    )
    .await?;
    h.grant(&user, &a).await?;
    let frame = h.open(&dev, &a).await?;
    let next = Uuid::new_v4();
    sqlx::query("INSERT INTO releases(id,app_id,version,artifact_hash,manifest) SELECT $1,app_id,'1.0.1',artifact_hash,manifest FROM releases WHERE id=$2")
        .bind(next).bind(a.release).execute(&state.pool).await?;
    sqlx::query("UPDATE installations SET release_id=$2 WHERE id=$1")
        .bind(a.installation)
        .bind(next)
        .execute(&state.pool)
        .await?;
    let changed = json(
        frame.request(Method::GET, "/_miy/api/probe").send().await?,
        409,
    )
    .await?;
    ensure!(
        changed["error"]["code"] == "release_changed_reload_required",
        "release change lost its reload code"
    );
    sqlx::query("UPDATE installations SET release_id=$2 WHERE id=$1")
        .bind(a.installation)
        .bind(a.release)
        .execute(&state.pool)
        .await?;
    passed.push("parent portal logout removes its frames and refuses their issued continuations; frame revocation, use revocation and a release change refuse the next frame request".into());

    // 5. Renewal: one-time, same frame, never extends it, only for its parent.
    let frame = h.open(&dev, &a).await?;
    let renew_code = |portal: &PortalSession, id: Uuid| {
        let cookie = format!(
            "{}={}",
            state
                .auth
                .cookie_name(auth::BrowserPurpose::DevPortal, "development"),
            portal.cookie
        );
        client
            .post(format!("{}/api/v1/app-frames/{id}/renew", portal.origin))
            .header("cookie", cookie)
            .header("origin", &portal.origin)
            .header("x-miy-csrf", &portal.csrf)
            .send()
    };
    let other = h.session(&user, auth::BrowserPurpose::DevPortal).await?;
    status(renew_code(&other, frame.id).await?, 404).await?;
    let renewal = json(renew_code(&dev, frame.id).await?, 200).await?;
    let renewal = renewal["code"].as_str().context("renew code")?.to_owned();
    let exchange = |code: &str, origin: &str| {
        frame
            .raw(Method::POST, "/_miy/renew")
            .header("origin", origin)
            .json(&json!({"code":code}))
            .send()
    };
    status(exchange(&renewal, &dev.origin).await?, 403).await?;
    let token = json(exchange(&renewal, &frame.origin).await?, 200).await?;
    status(exchange(&renewal, &frame.origin).await?, 401).await?;
    let renewed = token["token"].as_str().context("renewed token")?;
    ensure!(renewed != frame.token, "renew kept the old token");
    status(
        frame.request(Method::GET, "/_miy/api/probe").send().await?,
        401,
    )
    .await?;
    json(
        frame
            .raw(Method::GET, "/_miy/api/probe")
            .bearer_auth(renewed)
            .header("sec-fetch-site", "same-origin")
            .send()
            .await?,
        200,
    )
    .await?;
    let (token_end, frame_end): (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as("SELECT token_expires_at,expires_at FROM app_frame_sessions WHERE id=$1")
            .bind(frame.id)
            .fetch_one(&state.pool)
            .await?;
    ensure!(
        token_end <= frame_end
            && frame.bootstrap["frameExpiresAt"]
                .as_str()
                .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
                .is_some_and(|v| v.timestamp() == frame_end.timestamp()),
        "renewal extended the frame"
    );
    passed.push("renew codes come only from the frame's own parent session, are exchanged once on the frame origin, rotate the token and never extend the frame".into());

    // 6. Assets: the frame's capability, current use and release, authorized 304.
    let frame = h.open(&dev, &a).await?;
    let asset = format!("{}remoteEntry.js", frame.asset_base);
    let response = status(
        frame
            .raw(Method::GET, &asset)
            .header("sec-fetch-site", "same-origin")
            .send()
            .await?,
        200,
    )
    .await?;
    let etag = response
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .context("asset without etag")?
        .to_owned();
    ensure!(
        response
            .headers()
            .get("cache-control")
            .is_some_and(|v| v == "private, no-cache")
            && !response
                .headers()
                .get("content-security-policy")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .contains("sandbox"),
        "frame asset cache or execution policy changed"
    );
    ensure!(
        response.text().await? == "export const frameProbe = true;\n",
        "frame asset changed"
    );
    status(
        frame
            .raw(Method::GET, &asset)
            .header("sec-fetch-site", "same-origin")
            .header("if-none-match", &etag)
            .send()
            .await?,
        304,
    )
    .await?;
    for (target, path, site) in [
        (
            &frame,
            format!("/_miy/a/{}/ui/remoteEntry.js", auth::random()),
            "same-origin",
        ),
        (&b_frame, asset.clone(), "same-origin"),
        (&frame, asset.clone(), "cross-site"),
        (
            &frame,
            format!("{}../{}/ui/remoteEntry.js", frame.asset_base, a.artifact),
            "same-origin",
        ),
    ] {
        status(
            target
                .raw(Method::GET, &path)
                .header("sec-fetch-site", site)
                .send()
                .await?,
            404,
        )
        .await?;
    }
    sqlx::query("DELETE FROM grants WHERE user_id=$1 AND app_id=$2")
        .bind(&user)
        .bind(&a.app)
        .execute(&state.pool)
        .await?;
    let refused = frame
        .raw(Method::GET, &asset)
        .header("sec-fetch-site", "same-origin")
        .header("if-none-match", &etag)
        .send()
        .await?
        .status()
        .as_u16();
    ensure!(
        refused == 403,
        "revoked use still received the asset or 304: HTTP {refused}"
    );
    h.grant(&user, &a).await?;
    passed.push("frame assets need the frame's capability on its own origin with same-origin Fetch Metadata; conditional requests are authorized before 304 and use revocation refuses cached revalidation".into());

    // 7. Frame document and listener headers.
    let launch = json(
        launch_code(
            client,
            state,
            &dev,
            auth::BrowserPurpose::DevPortal,
            a.installation,
        )
        .await?,
        200,
    )
    .await?;
    let page = status(
        submit_launch(
            client,
            address,
            &frame.origin,
            Some(&dev.origin),
            launch["code"].as_str().context("code")?,
        )
        .await?,
        200,
    )
    .await?;
    let header = |name: &str| {
        page.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    let csp = header("content-security-policy").unwrap_or_default();
    ensure!(
        csp.contains(&format!("frame-ancestors {}", dev.origin))
            && csp.contains("script-src 'self'")
            && !csp.contains("unsafe-eval")
            && header("x-frame-options").is_none()
            && header("cache-control").as_deref() == Some("no-store")
            && header("referrer-policy").as_deref() == Some("no-referrer"),
        "frame document headers changed"
    );
    status(
        frame
            .raw(Method::GET, "/_miy/relaunch.js")
            .header("sec-fetch-site", "same-origin")
            .header("service-worker", "script")
            .send()
            .await?,
        403,
    )
    .await?;
    for path in ["/", "/api/v1/me", "/auth/login", "/_miy/unknown"] {
        status(frame.request(Method::GET, path).send().await?, 404).await?;
    }
    passed.push("frame documents allow only their portal as ancestor without X-Frame-Options, are not stored and send no referrer; frames refuse service workers and serve no other Core route".into());

    // 8. Portal cookies no longer reach app code or data; CLI bearers still do.
    let cookie = format!(
        "{}={}",
        state
            .auth
            .cookie_name(auth::BrowserPurpose::DevPortal, "development"),
        dev.cookie
    );
    for path in [
        format!("/api/v1/installations/{}/api/probe", a.installation),
        format!(
            "/api/v1/installations/{}/connections/{b_id}/api/probe",
            a.installation
        ),
        format!(
            "/api/v1/installations/{}/platform/directory",
            a.installation
        ),
        format!("/artifacts/{}/ui/remoteEntry.js", a.artifact),
    ] {
        status(
            client
                .get(format!("{dev_portal}{path}"))
                .header("cookie", &cookie)
                .send()
                .await?,
            403,
        )
        .await?;
    }
    let session = Uuid::new_v4();
    let access = format!("miy_access_{}", auth::random());
    sqlx::query("INSERT INTO cli_sessions(id,user_id,client_id,client_name) VALUES($1,$2,$3,'Frame regression')")
        .bind(session).bind(&user).bind(Uuid::new_v4()).execute(&state.pool).await?;
    sqlx::query("INSERT INTO cli_access_tokens(token_hash,session_id,expires_at) VALUES($1,$2,now()+interval '10 minutes')")
        .bind(hash(&access)).bind(session).execute(&state.pool).await?;
    json(
        client
            .get(format!(
                "{portal}/api/v1/installations/{}/api/probe",
                a.installation
            ))
            .bearer_auth(&access)
            .send()
            .await?,
        200,
    )
    .await?;
    passed.push("portal cookie sessions no longer reach app APIs, connections, directory or UI artifacts; CLI bearers keep the published Core address".into());

    // 9. A removed installation's origin stops resolving and is never reassigned.
    let c_frame = h.open(&dev, &c).await?;
    sqlx::query("UPDATE app_frame_origins SET installation_id=NULL WHERE origin=$1")
        .bind(&c_frame.origin)
        .execute(&state.pool)
        .await?;
    status(
        c_frame
            .request(Method::GET, "/_miy/platform/directory")
            .send()
            .await?,
        404,
    )
    .await?;
    h.grant(&user, &d).await?;
    let d_frame = h.open(&dev, &d).await?;
    let taken: Vec<String> = sqlx::query_scalar("SELECT origin FROM app_frame_origins")
        .fetch_all(&state.pool)
        .await?;
    ensure!(
        d_frame.origin != c_frame.origin
            && taken.iter().filter(|o| **o == d_frame.origin).count() == 1,
        "a removed installation's frame origin was handed to another installation"
    );
    passed.push("a removed installation's frame origin stops resolving and is never handed to another installation".into());

    // 10. An exhausted port pool refuses launches explicitly, never sharing origins.
    let prefix = d_frame
        .origin
        .rsplit_once(':')
        .context("pool origin port")?
        .0
        .to_owned();
    let filled = sqlx::query("INSERT INTO app_frame_origins(origin,port) SELECT $1||':'||p,p FROM generate_series(1,65535) p WHERE p BETWEEN 47100 AND 47399 ON CONFLICT DO NOTHING")
        .bind(&prefix)
        .execute(&state.pool)
        .await?
        .rows_affected();
    h.grant(&user, &e).await?;
    let exhausted = json(
        launch_code(
            client,
            state,
            &dev,
            auth::BrowserPurpose::DevPortal,
            e.installation,
        )
        .await?,
        503,
    )
    .await?;
    ensure!(
        exhausted["error"]["code"] == "app_frame_origin_pool_exhausted",
        "pool exhaustion lost its stable code"
    );
    sqlx::query("DELETE FROM app_frame_origins WHERE installation_id IS NULL AND port BETWEEN 47100 AND 47399 AND origin<>$1")
        .bind(&c_frame.origin)
        .execute(&state.pool)
        .await?;
    let e_frame = h.open(&dev, &e).await?;
    ensure!(
        filled > 0 && e_frame.origin != c_frame.origin,
        "the pool did not recover or reused a removed origin"
    );
    passed.push("an exhausted frame port pool refuses launches with app_frame_origin_pool_exhausted instead of sharing an origin, and recovers when ports are free".into());
    Ok(passed)
}
