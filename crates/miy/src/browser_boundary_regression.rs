//! Actual PostgreSQL and two Core HTTP listeners, only in the isolated regression
//! stack. Browser accounts use synthetic development sessions; this is not an OIDC
//! provider, TLS, DNS or real browser execution test. Credentials stay in memory.
use crate::{
    State,
    auth::{self, BrowserPurpose},
    contract::hash,
};
use anyhow::{Context, Result, ensure};
use axum::{Json, Router, routing::get};
use chrono::{DateTime, Utc};
use reqwest::{Client, Method, RequestBuilder, Response};
use serde_json::{Value, json};
use tokio::task::JoinHandle;
use uuid::Uuid;

struct Browser {
    user: String,
    cookie: String,
    csrf: String,
    purpose: BrowserPurpose,
}
struct Device {
    id: Uuid,
    code: String,
    secret: String,
}
struct Harness<'a> {
    state: &'a State,
    client: &'a Client,
    portal: &'a str,
    console: &'a str,
}
struct AppFixture {
    installation: Uuid,
    asset: String,
    task: JoinHandle<()>,
}
impl Drop for AppFixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn field(value: &Value, name: &str) -> Result<String> {
    value[name]
        .as_str()
        .map(str::to_owned)
        .with_context(|| format!("missing boundary response field {name}"))
}
async fn status(response: Response, expected: u16) -> Result<Response> {
    // Never format response bodies: successful authentication carries credentials.
    ensure!(
        response.status().as_u16() == expected,
        "boundary expected HTTP {expected}, received HTTP {}",
        response.status().as_u16()
    );
    Ok(response)
}
async fn json_response(response: Response, expected: u16) -> Result<Value> {
    Ok(status(response, expected).await?.json().await?)
}
async fn denied(response: Response, expected: u16) -> Result<()> {
    let value = json_response(response, expected).await?;
    ensure!(
        value["error"]["code"].is_string(),
        "boundary denial lost its stable error code"
    );
    Ok(())
}

impl Harness<'_> {
    fn origin(&self, purpose: BrowserPurpose) -> &str {
        match purpose {
            BrowserPurpose::Portal => self.portal,
            BrowserPurpose::Console => self.console,
        }
    }
    fn cookie(&self, browser: &Browser) -> String {
        format!(
            "{}={}",
            self.state.auth.cookie_name(browser.purpose, "development"),
            browser.cookie
        )
    }
    fn request(&self, purpose: BrowserPurpose, method: Method, path: &str) -> RequestBuilder {
        self.client
            .request(method, format!("{}{path}", self.origin(purpose)))
    }
    fn cookie_request(&self, browser: &Browser, method: Method, path: &str) -> RequestBuilder {
        self.request(browser.purpose, method, path)
            .header("cookie", self.cookie(browser))
    }
    fn browser_request(&self, browser: &Browser, method: Method, path: &str) -> RequestBuilder {
        self.cookie_request(browser, method, path)
            .header("origin", self.origin(browser.purpose))
            .header("x-miy-csrf", &browser.csrf)
    }
    async fn user(&self, admin: bool) -> Result<String> {
        let id = format!("boundary-user-{}", Uuid::new_v4());
        sqlx::query("INSERT INTO users(id,name,admin) VALUES($1,$1,$2)")
            .bind(&id)
            .bind(admin)
            .execute(&self.state.pool)
            .await?;
        Ok(id)
    }
    async fn browser(&self, user: &str, purpose: BrowserPurpose) -> Result<Browser> {
        let cookie = auth::random();
        let csrf = auth::random();
        sqlx::query("INSERT INTO development_sessions(id_hash,user_id,csrf,expires_at,browser_purpose) VALUES($1,$2,$3,now()+interval '8 hours',$4)")
            .bind(hash(&cookie)).bind(user).bind(&csrf).bind(purpose.as_str())
            .execute(&self.state.pool).await?;
        Ok(Browser {
            user: user.to_owned(),
            cookie,
            csrf,
            purpose,
        })
    }
    /// The pre-0.5 browser-approval login is retired. Credentials it issued before
    /// retirement remain valid until expiry, so seed one directly.
    async fn legacy_retired(&self, user: &str) -> Result<String> {
        for (method, path) in [
            (Method::POST, "/auth/cli/start"),
            (Method::POST, "/auth/cli/exchange"),
        ] {
            let value = json_response(
                self.request(BrowserPurpose::Portal, method, path)
                    .json(&json!({"challenge":hash(auth::random())}))
                    .send()
                    .await?,
                410,
            )
            .await?;
            ensure!(
                value["error"]["code"] == "cli_upgrade_required",
                "retired legacy login lost its upgrade error"
            );
        }
        let token = format!("miy_cli_{}", auth::random());
        sqlx::query("INSERT INTO cli_tokens(token_hash,user_id) VALUES($1,$2)")
            .bind(hash(&token))
            .bind(user)
            .execute(&self.state.pool)
            .await?;
        Ok(token)
    }
    async fn device(&self) -> Result<Device> {
        let value = json_response(
            self.request(BrowserPurpose::Portal, Method::POST, "/auth/cli/device")
                .json(&json!({"clientId":Uuid::new_v4(),"clientName":"Boundary regression"}))
                .send()
                .await?,
            200,
        )
        .await?;
        let id: Uuid = field(&value, "id")?.parse()?;
        ensure!(
            value["verificationPath"] == format!("/cli/device/{id}")
                && value["expiresIn"] == 300
                && value["interval"] == 5,
            "device start changed its relative path, lifetime or polling interval"
        );
        Ok(Device {
            id,
            code: field(&value, "userCode")?,
            secret: field(&value, "deviceCode")?,
        })
    }
    async fn bearer_me(&self, token: &str, expected: u16) -> Result<Value> {
        json_response(
            self.request(BrowserPurpose::Portal, Method::GET, "/api/v1/me")
                .bearer_auth(token)
                .send()
                .await?,
            expected,
        )
        .await
    }
    async fn app(&self) -> Result<AppFixture> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = i32::from(listener.local_addr()?.port());
        let router = Router::new()
            .route(
                "/api/probe",
                get(|| async { Json(json!({"boundaryProbe":true})) }),
            )
            .route(
                "/api/document",
                get(|| async {
                    (
                        [
                            ("content-type", "text/html"),
                            ("set-cookie", "boundary_untrusted=blocked; Path=/"),
                            ("clear-site-data", "\"cookies\""),
                            ("location", "https://untrusted.invalid/"),
                            ("refresh", "0;url=https://untrusted.invalid/"),
                            ("content-security-policy", "script-src 'unsafe-inline'"),
                            ("access-control-allow-origin", "*"),
                            ("service-worker-allowed", "/"),
                        ],
                        "<!doctype html><title>Boundary fixture</title>",
                    )
                }),
            );
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        let id = Uuid::new_v4();
        let app = format!("boundary-{}", id.simple());
        let release = Uuid::new_v4();
        let artifact = hash(format!("boundary-fixture:{id}"));
        let directory = self.state.artifacts.join(&artifact).join("ui");
        tokio::fs::create_dir_all(&directory).await?;
        tokio::fs::write(
            directory.join("boundary.js"),
            "export const boundaryProbe = true;\n",
        )
        .await?;
        sqlx::query("INSERT INTO apps(id,name) VALUES($1,'Boundary fixture')")
            .bind(&app)
            .execute(&self.state.pool)
            .await?;
        let manifest = json!({
            "contractVersion":"miy.app/v1alpha1","id":app,"name":"Boundary fixture","version":"1.0.0",
            "runtime":{"profile":"rust-web-v1"},
            "ui":{"delivery":"module","moduleEntry":"boundary.js","moduleExport":"./AppRoot","contractVersion":"miy.ui/v1alpha1"},
            "resources":{"database":"postgresql"},"migrations":{"directory":"migrations"},"requestedPermissions":[],
        });
        sqlx::query("INSERT INTO releases(id,app_id,version,artifact_hash,manifest) VALUES($1,$2,'1.0.0',$3,$4)")
            .bind(release).bind(&app).bind(&artifact).bind(manifest).execute(&self.state.pool).await?;
        sqlx::query("INSERT INTO installations(id,app_id,environment,release_id,db_status,runtime_status,desired_state,runtime_port,runtime_secret) VALUES($1,$2,'dev',$3,'ready','running','running',$4,$5)")
            .bind(id).bind(&app).bind(release).bind(port).bind(auth::encrypt(&auth::random())?)
            .execute(&self.state.pool).await?;
        Ok(AppFixture {
            installation: id,
            asset: format!("/artifacts/{artifact}/ui/boundary.js"),
            task,
        })
    }
}

pub async fn run(
    state: &State,
    client: &Client,
    portal: &str,
    console: &str,
) -> Result<Vec<String>> {
    for origin in [portal, console] {
        let url = url::Url::parse(origin)?;
        ensure!(
            url.host_str()
                .is_some_and(|host| matches!(host, "127.0.0.1" | "localhost" | "[::1]")),
            "boundary fixtures require loopback listeners, never a production host"
        );
    }
    ensure!(
        state.auth.development_login
            && portal == state.auth.origin
            && console == state.auth.console_origin
            && portal != console,
        "boundary regression needs separate configured listeners and isolated development login"
    );
    let h = Harness {
        state,
        client,
        portal,
        console,
    };
    let admin_id = h.user(true).await?;
    let user_id = h.user(false).await?;
    let other_id = h.user(false).await?;
    let portal_admin = h.browser(&admin_id, BrowserPurpose::Portal).await?;
    let console_admin = h.browser(&admin_id, BrowserPurpose::Console).await?;
    let portal_user = h.browser(&user_id, BrowserPurpose::Portal).await?;
    let mut console_user = h.browser(&user_id, BrowserPurpose::Console).await?;
    let console_other = h.browser(&other_id, BrowserPurpose::Console).await?;
    let mut passed = Vec::new();

    for browser in [&portal_admin, &console_admin] {
        let me = json_response(
            h.cookie_request(browser, Method::GET, "/api/v1/me")
                .send()
                .await?,
            200,
        )
        .await?;
        ensure!(
            me["id"] == browser.user
                && me["browserPurpose"] == browser.purpose.as_str()
                && me["portalOrigin"] == portal
                && me["consoleOrigin"] == console,
            "browser identity lost its purpose or configured origins"
        );
        ensure!(
            me["admin"] == (browser.purpose == BrowserPurpose::Console),
            "portal exposed administrator authority"
        );
        let other_purpose = if browser.purpose == BrowserPurpose::Portal {
            BrowserPurpose::Console
        } else {
            BrowserPurpose::Portal
        };
        let renamed = format!(
            "{}={}",
            state.auth.cookie_name(other_purpose, "development"),
            browser.cookie
        );
        denied(
            h.request(other_purpose, Method::GET, "/api/v1/me")
                .header("cookie", renamed)
                .send()
                .await?,
            401,
        )
        .await?;
        denied(
            h.request(other_purpose, Method::GET, "/api/v1/me")
                .header("cookie", h.cookie(browser))
                .send()
                .await?,
            401,
        )
        .await?;
        for duplicate in [&browser.cookie, "invalid-duplicate"] {
            let name = state.auth.cookie_name(browser.purpose, "development");
            denied(
                h.request(browser.purpose, Method::GET, "/api/v1/me")
                    .header(
                        "cookie",
                        format!("{name}={}; {name}={duplicate}", browser.cookie),
                    )
                    .send()
                    .await?,
                401,
            )
            .await?;
        }
        // Real same-host browsers send both port-specific cookie names together.
        let me = json_response(
            h.request(browser.purpose, Method::GET, "/api/v1/me")
                .header(
                    "cookie",
                    format!("{}; {}", h.cookie(&portal_admin), h.cookie(&console_admin)),
                )
                .send()
                .await?,
            200,
        )
        .await?;
        ensure!(
            me["browserPurpose"] == browser.purpose.as_str(),
            "coexisting cookies confused the listener purpose"
        );
    }
    passed.push("separate HTTP listeners bind browser credentials to stored purpose, reject copied/renamed and duplicate cookies, and accept both legitimate same-host cookie names without authority mixing".into());

    // Same-host ports share cookies; code on one origin must not use the other's
    // cookies even for reads. Reject before any session lookup.
    for browser in [&portal_admin, &console_admin] {
        let foreign = h.origin(if browser.purpose == BrowserPurpose::Portal {
            BrowserPurpose::Console
        } else {
            BrowserPurpose::Portal
        });
        for headers in [
            vec![("origin", foreign)],
            vec![("sec-fetch-site", "same-site")],
            vec![
                ("sec-fetch-site", "same-site"),
                ("sec-fetch-mode", "navigate"),
            ],
            vec![("sec-fetch-site", "cross-site"), ("origin", "null")],
        ] {
            let mut request = h.cookie_request(browser, Method::GET, "/api/v1/me");
            for (name, value) in headers {
                request = request.header(name, value);
            }
            denied(request.send().await?, 403).await?;
        }
        for site in ["same-origin", "none"] {
            status(
                h.cookie_request(browser, Method::GET, "/api/v1/me")
                    .header("sec-fetch-site", site)
                    .send()
                    .await?,
                200,
            )
            .await?;
        }
        status(
            h.cookie_request(browser, Method::GET, "/api/v1/me")
                .header("sec-fetch-site", "same-origin")
                .header("origin", h.origin(browser.purpose))
                .send()
                .await?,
            200,
        )
        .await?;
    }
    for mode in ["no-cors", "cors"] {
        denied(
            h.request(BrowserPurpose::Portal, Method::GET, "/auth/login")
                .header("sec-fetch-mode", mode)
                .send()
                .await?,
            403,
        )
        .await?;
    }
    passed.push("cookie-authenticated requests from another same-host origin, including reads and navigations, are rejected before session lookup; sign-in starts only from a navigation".into());

    let legacy_token = h.legacy_retired(&user_id).await?;
    let device = h.device().await?;
    let legacy_approval = format!("/api/v1/cli-authorizations/{}/approve", Uuid::new_v4());
    let device_approval = format!("/api/v1/cli/devices/{}/decision", device.id);
    for browser in [&portal_admin, &portal_user] {
        for path in [
            "/api/v1/users",
            "/api/v1/groups",
            "/api/v1/runtime/installations",
            "/api/v1/cli/sessions",
            "/api/v1/cli/legacy-sessions",
        ] {
            denied(
                h.browser_request(browser, Method::GET, path).send().await?,
                403,
            )
            .await?;
        }
        denied(
            h.browser_request(browser, Method::POST, &legacy_approval)
                .send()
                .await?,
            403,
        )
        .await?;
        denied(
            h.browser_request(browser, Method::POST, &device_approval)
                .json(&json!({"userCode":device.code,"decision":"approve"}))
                .send()
                .await?,
            403,
        )
        .await?;
    }
    let device_pending: bool = sqlx::query_scalar(
        "SELECT status='pending' AND user_id IS NULL FROM cli_device_authorizations WHERE id=$1",
    )
    .bind(device.id)
    .fetch_one(&state.pool)
    .await?;
    ensure!(
        device_pending,
        "portal rejection still changed a CLI approval"
    );
    let console_url = url::Url::parse(console)?;
    let console_host = console_url[url::Position::BeforeHost..url::Position::AfterPort].to_owned();
    denied(
        h.browser_request(&portal_admin, Method::GET, "/api/v1/users")
            .header("host", &console_host)
            .header("x-forwarded-host", &console_host)
            .header("x-miy-browser-purpose", "console")
            .send()
            .await?,
        403,
    )
    .await?;
    passed.push("portal administrators and ordinary users cannot read management credentials or approve either legacy/device CLI login; Host and forwarded-purpose headers cannot select console authority".into());

    let group_name = format!("Boundary group {}", Uuid::new_v4());
    let group = json!({"name":group_name,"source":"local"});
    for (origin, csrf) in [
        (None, Some(console_admin.csrf.as_str())),
        (Some(portal), Some(console_admin.csrf.as_str())),
        (Some(console), None),
        (Some(console), Some("wrong-csrf")),
    ] {
        let mut request = h
            .cookie_request(&console_admin, Method::POST, "/api/v1/groups")
            .json(&group);
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        if let Some(csrf) = csrf {
            request = request.header("x-miy-csrf", csrf);
        }
        denied(request.send().await?, 403).await?;
    }
    denied(
        h.browser_request(&console_user, Method::POST, "/api/v1/groups")
            .json(&group)
            .send()
            .await?,
        403,
    )
    .await?;
    let created = json_response(
        h.browser_request(&console_admin, Method::POST, "/api/v1/groups")
            .json(&group)
            .send()
            .await?,
        200,
    )
    .await?;
    let audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit WHERE actor=$1 AND action='group.created' AND target=$2",
    )
    .bind(&admin_id)
    .bind(format!("group:{}", field(&created, "id")?))
    .fetch_one(&state.pool)
    .await?;
    ensure!(
        audit == 1,
        "console management change did not record exactly its authorized actor"
    );
    passed.push("console mutations need exact Origin and synchronizer CSRF as well as current administrator authority; rejected attempts do not produce a management change and the authorized actor is audited".into());

    for (kind, id) in [("authorize", Uuid::new_v4()), ("device", device.id)] {
        let redirect = status(
            h.request(
                BrowserPurpose::Portal,
                Method::GET,
                &format!("/cli/{kind}/{id}?next=https%3A%2F%2Funtrusted.invalid&code=discarded"),
            )
            .send()
            .await?,
            303,
        )
        .await?;
        ensure!(
            redirect
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                == Some(format!("{console}/cli/{kind}/{id}").as_str()),
            "CLI compatibility redirect forwarded query values or an untrusted target"
        );
    }
    for browser in [&portal_admin, &console_admin] {
        denied(
            h.browser_request(browser, Method::GET, "/api/v1/boundary-unclassified-route")
                .send()
                .await?,
            403,
        )
        .await?;
    }
    let challenge = status(
        h.request(BrowserPurpose::Portal, Method::GET, "/mcp")
            .send()
            .await?,
        401,
    )
    .await?;
    let expected_challenge = format!(
        "Bearer resource_metadata=\"{portal}/.well-known/oauth-protected-resource/mcp\", scope=\"miy.read\""
    );
    ensure!(
        challenge
            .headers()
            .get("www-authenticate")
            .and_then(|value| value.to_str().ok())
            == Some(expected_challenge.as_str()),
        "anonymous MCP discovery lost the published authentication challenge"
    );
    passed.push("published relative legacy/device verification paths redirect to the fixed console origin with only the flow ID; unclassified HTTP routes fail closed and anonymous MCP preserves its 401 resource metadata challenge".into());

    let app = h.app().await?;
    sqlx::query("INSERT INTO grants(user_id,app_id,environment,permission) SELECT $1,app_id,'dev','use' FROM installations WHERE id=$2")
        .bind(&user_id).bind(app.installation).execute(&state.pool).await?;
    let probe_path = format!("/api/v1/installations/{}/api/probe", app.installation);
    let probe = json_response(
        h.browser_request(&portal_admin, Method::GET, &probe_path)
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        probe["boundaryProbe"] == true,
        "portal app API fixture did not execute"
    );
    let asset = status(
        h.browser_request(&portal_admin, Method::GET, &app.asset)
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        !asset
            .headers()
            .get("content-security-policy")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .contains("sandbox"),
        "app API sandbox broke the existing MF JavaScript asset path"
    );
    ensure!(
        asset.text().await? == "export const boundaryProbe = true;\n",
        "portal app JavaScript changed"
    );
    for path in [&probe_path, &app.asset] {
        denied(
            h.browser_request(&console_admin, Method::GET, path)
                .send()
                .await?,
            403,
        )
        .await?;
        denied(
            h.request(BrowserPurpose::Console, Method::GET, path)
                .send()
                .await?,
            403,
        )
        .await?;
        denied(
            h.browser_request(&portal_admin, Method::GET, path)
                .header("service-worker", "script")
                .send()
                .await?,
            403,
        )
        .await?;
    }
    let html_path = format!("/api/v1/installations/{}/api/document", app.installation);
    let html = status(
        h.browser_request(&portal_admin, Method::GET, &html_path)
            .send()
            .await?,
        200,
    )
    .await?;
    for header in [
        "set-cookie",
        "clear-site-data",
        "location",
        "refresh",
        "access-control-allow-origin",
        "service-worker-allowed",
    ] {
        ensure!(
            !html.headers().contains_key(header),
            "app response escaped the header allowlist: {header}"
        );
    }
    ensure!(
        html.headers()
            .get("x-content-type-options")
            .is_some_and(|v| v == "nosniff")
            && html
                .headers()
                .get("content-security-policy")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.contains("sandbox")),
        "active app API response was not sandboxed and protected from MIME sniffing"
    );
    passed.push("existing app API and MF JavaScript work on the portal but are denied on the console; app response headers cannot set cookies, clear site data, redirect, relax CSP/CORS or register workers, and HTML API responses are sandboxed".into());

    // Even a console user cannot approve the retired legacy flow any more.
    let retired = json_response(
        h.browser_request(&console_user, Method::POST, &legacy_approval)
            .send()
            .await?,
        410,
    )
    .await?;
    ensure!(
        retired["error"]["code"] == "cli_upgrade_required",
        "retired legacy approval lost its upgrade error"
    );
    ensure!(
        h.bearer_me(&legacy_token, 200).await?["id"] == user_id,
        "legacy token changed its approving user"
    );
    json_response(
        h.request(BrowserPurpose::Portal, Method::GET, &probe_path)
            .bearer_auth(&legacy_token)
            .send()
            .await?,
        200,
    )
    .await?;
    denied(
        h.request(BrowserPurpose::Portal, Method::GET, "/api/v1/users")
            .bearer_auth(&legacy_token)
            .header("origin", portal)
            .send()
            .await?,
        403,
    )
    .await?;
    for path in [&probe_path, &app.asset] {
        denied(
            h.request(BrowserPurpose::Console, Method::GET, path)
                .bearer_auth(&legacy_token)
                .send()
                .await?,
            403,
        )
        .await?;
    }
    passed.push("the retired legacy start/approve/exchange flow answers 410 cli_upgrade_required even on the console, while a previously issued eight-hour legacy credential keeps its identity; browser Origin cannot turn CLI bearer access into management authority".into());

    // A browser logout is deliberately independent of existing CLI credentials.
    json_response(
        h.browser_request(&console_user, Method::POST, "/auth/logout")
            .send()
            .await?,
        200,
    )
    .await?;
    denied(
        h.cookie_request(&console_user, Method::GET, "/api/v1/me")
            .send()
            .await?,
        401,
    )
    .await?;
    ensure!(
        h.bearer_me(&legacy_token, 200).await?["id"] == user_id,
        "browser logout silently revoked a CLI credential"
    );
    console_user = h.browser(&user_id, BrowserPurpose::Console).await?;
    json_response(
        h.cookie_request(&console_user, Method::GET, "/api/v1/me")
            .send()
            .await?,
        200,
    )
    .await?;
    let sessions = json_response(
        h.browser_request(&console_user, Method::GET, "/api/v1/cli/legacy-sessions")
            .send()
            .await?,
        200,
    )
    .await?;
    let listed = sessions["sessions"]
        .as_array()
        .context("legacy session list")?;
    ensure!(
        listed.len() == 1,
        "legacy list was not restricted to the current user's live credentials"
    );
    let entry = listed[0].as_object().context("legacy session entry")?;
    ensure!(
        entry.len() == 3
            && ["id", "createdAt", "expiresAt"]
                .iter()
                .all(|key| entry.contains_key(*key)),
        "legacy management response exposed extra credential material"
    );
    let legacy_id: Uuid = field(&listed[0], "id")?.parse()?;
    let expires: DateTime<Utc> = field(&listed[0], "expiresAt")?.parse()?;
    let created: DateTime<Utc> = field(&listed[0], "createdAt")?.parse()?;
    ensure!(
        (expires - created).num_seconds() == 28800,
        "stored legacy lifetime changed"
    );
    let path = format!("/api/v1/cli/legacy-sessions/{legacy_id}");
    denied(
        h.browser_request(&console_other, Method::DELETE, &path)
            .send()
            .await?,
        404,
    )
    .await?;
    denied(
        h.browser_request(&portal_user, Method::DELETE, &path)
            .send()
            .await?,
        403,
    )
    .await?;
    json_response(
        h.browser_request(&console_user, Method::DELETE, &path)
            .send()
            .await?,
        200,
    )
    .await?;
    h.bearer_me(&legacy_token, 401).await?;
    let revoked: i64 = sqlx::query_scalar("SELECT count(*) FROM audit WHERE actor=$1 AND action='auth.legacy_session.revoked' AND target=$2")
        .bind(&user_id).bind(legacy_id.to_string()).fetch_one(&state.pool).await?;
    ensure!(
        revoked == 1,
        "legacy revocation did not audit the owning user"
    );
    passed.push("browser logout and reauthentication preserve independent CLI lifetime; legacy sessions expose only public IDs/timestamps, deny another user and portal revocation, and owner revocation immediately invalidates the token with audit".into());

    let waiting = json_response(
        h.request(BrowserPurpose::Portal, Method::POST, "/auth/cli/token")
            .json(&json!({"grantType":"device_code","id":device.id,"deviceCode":device.secret}))
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        waiting["status"] == "pending",
        "device flow was approved without console consent"
    );
    json_response(
        h.browser_request(&console_user, Method::POST, &device_approval)
            .json(&json!({"userCode":device.code,"decision":"approve"}))
            .send()
            .await?,
        200,
    )
    .await?;
    // Advance only this synthetic flow's poll timer instead of sleeping.
    sqlx::query("UPDATE cli_device_authorizations SET last_poll_at=clock_timestamp()-interval '61 seconds' WHERE id=$1")
        .bind(device.id).execute(&state.pool).await?;
    let device_value = json_response(
        h.request(BrowserPurpose::Portal, Method::POST, "/auth/cli/token")
            .json(&json!({"grantType":"device_code","id":device.id,"deviceCode":device.secret}))
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        device_value["status"] == "approved"
            && device_value["tokenType"] == "Bearer"
            && device_value["expiresIn"]
                .as_i64()
                .is_some_and(|v| (1..=900).contains(&v)),
        "device access shape or 15-minute lifetime changed"
    );
    let session: Uuid = field(&device_value, "sessionId")?.parse()?;
    let first_access = field(&device_value, "accessToken")?;
    let first_refresh = field(&device_value, "refreshToken")?;
    let lifetime: i64 = sqlx::query_scalar(
        "SELECT EXTRACT(EPOCH FROM expires_at-created_at)::bigint FROM cli_sessions WHERE id=$1",
    )
    .bind(session)
    .fetch_one(&state.pool)
    .await?;
    ensure!(
        lifetime == 30 * 24 * 60 * 60,
        "device family no longer has its 30-day absolute lifetime"
    );
    let rotated = json_response(
        h.request(BrowserPurpose::Portal, Method::POST, "/auth/cli/token")
            .json(&json!({"grantType":"refresh_token","refreshToken":first_refresh}))
            .send()
            .await?,
        200,
    )
    .await?;
    let access = field(&rotated, "accessToken")?;
    let refresh = field(&rotated, "refreshToken")?;
    ensure!(
        rotated["sessionId"] == device_value["sessionId"]
            && rotated["refreshExpiresAt"] == device_value["refreshExpiresAt"]
            && access != first_access
            && refresh != first_refresh,
        "device refresh changed identity/absolute lifetime or failed to rotate"
    );
    ensure!(
        h.bearer_me(&access, 200).await?["id"] == user_id,
        "device refresh changed its user"
    );
    passed.push("ordinary console approval keeps published device start/poll/refresh payloads and relative URL, 15-minute access, rotating tokens and the unchanged 30-day absolute family lifetime".into());

    let path = format!("/api/v1/cli/sessions/{session}");
    denied(
        h.browser_request(&console_other, Method::DELETE, &path)
            .send()
            .await?,
        404,
    )
    .await?;
    denied(
        h.browser_request(&portal_user, Method::DELETE, &path)
            .send()
            .await?,
        403,
    )
    .await?;
    json_response(
        h.browser_request(&console_user, Method::DELETE, &path)
            .send()
            .await?,
        200,
    )
    .await?;
    h.bearer_me(&access, 401).await?;
    denied(
        h.request(BrowserPurpose::Portal, Method::POST, "/auth/cli/token")
            .json(&json!({"grantType":"refresh_token","refreshToken":refresh}))
            .send()
            .await?,
        401,
    )
    .await?;
    passed.push("device family revocation requires the console owner, denies portal and other-user requests, and blocks both access and refresh without an auth rewrite".into());

    sqlx::query(
        "UPDATE development_sessions SET expires_at=now()-interval '1 second' WHERE id_hash=$1",
    )
    .bind(hash(&console_user.cookie))
    .execute(&state.pool)
    .await?;
    denied(
        h.cookie_request(&console_user, Method::GET, "/api/v1/me")
            .send()
            .await?,
        401,
    )
    .await?;
    let fresh = h.browser(&user_id, BrowserPurpose::Console).await?;
    json_response(
        h.cookie_request(&fresh, Method::GET, "/api/v1/me")
            .send()
            .await?,
        200,
    )
    .await?;
    sqlx::query("UPDATE users SET enabled=false WHERE id=$1")
        .bind(&user_id)
        .execute(&state.pool)
        .await?;
    // Core's disable trigger removes all browser sessions. Re-enabling must not
    // resurrect either the old console session or the portal session.
    denied(
        h.cookie_request(&fresh, Method::GET, "/api/v1/me")
            .send()
            .await?,
        401,
    )
    .await?;
    sqlx::query("UPDATE users SET enabled=true WHERE id=$1")
        .bind(&user_id)
        .execute(&state.pool)
        .await?;
    denied(
        h.cookie_request(&fresh, Method::GET, "/api/v1/me")
            .send()
            .await?,
        401,
    )
    .await?;
    denied(
        h.cookie_request(&portal_user, Method::GET, "/api/v1/me")
            .send()
            .await?,
        401,
    )
    .await?;
    let renewed = h.browser(&user_id, BrowserPurpose::Console).await?;
    json_response(
        h.cookie_request(&renewed, Method::GET, "/api/v1/me")
            .send()
            .await?,
        200,
    )
    .await?;
    passed.push("expired or disabled browser sessions require new authentication; restoring an account does not revive old portal/console cookies and a fresh purpose-bound session works".into());
    ensure!(passed.len() == 11, "boundary observation count changed");
    Ok(passed)
}
