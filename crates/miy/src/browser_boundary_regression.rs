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
    dev_portal: &'a str,
}
const PURPOSES: [BrowserPurpose; 3] = [
    BrowserPurpose::Portal,
    BrowserPurpose::DevPortal,
    BrowserPurpose::Console,
];
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
            BrowserPurpose::DevPortal => self.dev_portal,
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
    fn portal_session(&self, browser: &Browser) -> crate::app_frames_regression::PortalSession {
        crate::app_frames_regression::PortalSession {
            cookie: browser.cookie.clone(),
            csrf: browser.csrf.clone(),
            origin: self.origin(browser.purpose).to_owned(),
        }
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
    async fn app(&self, environment: &str) -> Result<AppFixture> {
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
        sqlx::query("INSERT INTO installations(id,app_id,environment,release_id,db_status,runtime_status,desired_state,runtime_port,runtime_secret) VALUES($1,$2,$6,$3,'ready','running','running',$4,$5)")
            .bind(id).bind(&app).bind(release).bind(port).bind(auth::encrypt(&auth::random())?).bind(environment)
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
    dev_portal: &str,
    frames: &str,
) -> Result<Vec<String>> {
    for origin in [portal, console, dev_portal, frames] {
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
            && dev_portal == state.auth.dev_portal_origin
            && portal != console
            && portal != dev_portal
            && console != dev_portal,
        "boundary regression needs separate configured listeners and isolated development login"
    );
    let h = Harness {
        state,
        client,
        portal,
        console,
        dev_portal,
    };
    let admin_id = h.user(true).await?;
    let user_id = h.user(false).await?;
    let other_id = h.user(false).await?;
    let portal_admin = h.browser(&admin_id, BrowserPurpose::Portal).await?;
    let console_admin = h.browser(&admin_id, BrowserPurpose::Console).await?;
    let dev_admin = h.browser(&admin_id, BrowserPurpose::DevPortal).await?;
    let portal_user = h.browser(&user_id, BrowserPurpose::Portal).await?;
    let mut console_user = h.browser(&user_id, BrowserPurpose::Console).await?;
    let console_other = h.browser(&other_id, BrowserPurpose::Console).await?;
    let mut passed = Vec::new();

    for browser in [&portal_admin, &dev_admin, &console_admin] {
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
                && me["consoleOrigin"] == console
                && me["devPortalOrigin"] == dev_portal,
            "browser identity lost its purpose or configured origins"
        );
        ensure!(
            me["admin"] == (browser.purpose == BrowserPurpose::Console),
            "portal exposed administrator authority"
        );
        for other_purpose in PURPOSES.into_iter().filter(|p| *p != browser.purpose) {
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
        }
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
                    format!(
                        "{}; {}; {}",
                        h.cookie(&portal_admin),
                        h.cookie(&dev_admin),
                        h.cookie(&console_admin)
                    ),
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

    // Personal settings: each surface changes only its own person's record, with
    // the CSRF header; omitted fields stay and the defaults read before any row.
    let fresh = json_response(
        h.cookie_request(&console_other, Method::GET, "/api/v1/me")
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        fresh["preferences"]
            == json!({"theme":"system","locale":"ko-KR","timeZone":"Asia/Seoul",
                "dateFormat":"korean","jobTitle":null,"pinnedApps":[],"pinnedPanels":[]}),
        "personal settings lost their defaults"
    );
    denied(
        h.cookie_request(&portal_user, Method::PATCH, "/api/v1/me/preferences")
            .json(&json!({"theme":"dark"}))
            .send()
            .await?,
        403,
    )
    .await?;
    for (browser, body) in [
        (
            &portal_user,
            json!({"theme":"dark","pinnedApps":["planner"],"pinnedPanels":["planner/todos"]}),
        ),
        (
            &console_user,
            json!({"locale":"en-US","timeZone":"Europe/Berlin"}),
        ),
        (&dev_admin, json!({"dateFormat":"iso","jobTitle":"책임"})),
    ] {
        json_response(
            h.browser_request(browser, Method::PATCH, "/api/v1/me/preferences")
                .json(&body)
                .send()
                .await?,
            200,
        )
        .await?;
    }
    for body in [
        json!({"theme":"sepia"}),
        json!({"timeZone":"../etc"}),
        json!({"pinnedApps":["planner","planner"]}),
        json!({"pinnedPanels":["planner"]}),
        json!({"pinnedPanels":["planner/Todos"]}),
        json!({"pinnedPanels":["planner/todos","planner/todos"]}),
        json!({"pinnedPanels":(0..17).map(|i| format!("app{i}/panel")).collect::<Vec<_>>()}),
        json!({"name":""}),
        json!({"userId":"someone-else"}),
    ] {
        let response = h
            .browser_request(&portal_user, Method::PATCH, "/api/v1/me/preferences")
            .json(&body)
            .send()
            .await?;
        ensure!(
            matches!(response.status().as_u16(), 400 | 422),
            "invalid personal settings were accepted"
        );
    }
    let user_me = json_response(
        h.cookie_request(&portal_user, Method::GET, "/api/v1/me")
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        user_me["preferences"]
            == json!({"theme":"dark","locale":"en-US","timeZone":"Europe/Berlin",
                "dateFormat":"korean","jobTitle":null,"pinnedApps":["planner"],
                "pinnedPanels":["planner/todos"]}),
        "personal settings did not merge across surfaces"
    );
    let admin_me = json_response(
        h.cookie_request(&console_admin, Method::GET, "/api/v1/me")
            .send()
            .await?,
        200,
    )
    .await?;
    let other_me = json_response(
        h.cookie_request(&console_other, Method::GET, "/api/v1/me")
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        admin_me["preferences"]["jobTitle"] == "책임"
            && admin_me["preferences"]["theme"] == "system"
            && other_me["preferences"] == fresh["preferences"],
        "personal settings leaked to another person"
    );
    let renamed = json_response(
        h.browser_request(&portal_user, Method::PATCH, "/api/v1/me/preferences")
            .json(&json!({"name":"  새 이름  "}))
            .send()
            .await?,
        200,
    )
    .await?;
    let audited: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit WHERE actor=$1 AND action='user.profile.rename' AND target=$1",
    )
    .bind(&user_id)
    .fetch_one(&state.pool)
    .await?;
    ensure!(
        renamed["name"] == "새 이름" && audited == 1,
        "a display name change was not applied or not audited"
    );
    passed.push("personal settings: defaults before any row, CSRF required, partial updates merge across portal, dev portal and console, invalid values (including malformed, repeated or too many pinned panels) and foreign fields rejected, other people unaffected, renames audited".into());

    // Own browser sessions: listed across surfaces with this one marked and
    // first; another of one's sessions ends (with CSRF), the current one and
    // other people's do not.
    let spare = h.browser(&user_id, BrowserPurpose::DevPortal).await?;
    let sessions = json_response(
        h.cookie_request(&portal_user, Method::GET, "/api/v1/me/sessions")
            .send()
            .await?,
        200,
    )
    .await?;
    let items = sessions["items"].as_array().context("session list")?;
    ensure!(
        items.len() >= 3
            && items[0]["current"] == true
            && items[0]["surface"] == "portal"
            && items.iter().filter(|item| item["current"] == true).count() == 1,
        "the session list lost the current session or other surfaces"
    );
    let id_of = |surface: &str, current: bool| {
        items
            .iter()
            .find(|item| item["surface"] == surface && item["current"] == current)
            .and_then(|item| item["id"].as_str())
            .map(str::to_owned)
    };
    let own = id_of("portal", true).context("current session id")?;
    let other_list = json_response(
        h.cookie_request(&console_other, Method::GET, "/api/v1/me/sessions")
            .send()
            .await?,
        200,
    )
    .await?;
    let foreign = other_list["items"][0]["id"]
        .as_str()
        .context("other person's session")?
        .to_owned();
    ensure!(
        !items.iter().any(|item| item["id"] == foreign.as_str()),
        "another person's session was listed"
    );
    denied(
        h.cookie_request(
            &portal_user,
            Method::DELETE,
            &format!(
                "/api/v1/me/sessions/{}",
                id_of("dev_portal", false).context("spare session")?
            ),
        )
        .send()
        .await?,
        403,
    )
    .await?;
    for (id, expected) in [(&own, 409), (&foreign, 404)] {
        denied(
            h.browser_request(
                &portal_user,
                Method::DELETE,
                &format!("/api/v1/me/sessions/{id}"),
            )
            .send()
            .await?,
            expected,
        )
        .await?;
    }
    let spare_id =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM development_sessions WHERE id_hash=$1")
            .bind(hash(&spare.cookie))
            .fetch_one(&state.pool)
            .await?;
    status(
        h.browser_request(
            &portal_user,
            Method::DELETE,
            &format!("/api/v1/me/sessions/{spare_id}"),
        )
        .send()
        .await?,
        204,
    )
    .await?;
    denied(
        h.cookie_request(&spare, Method::GET, "/api/v1/me")
            .send()
            .await?,
        401,
    )
    .await?;
    status(
        h.cookie_request(&console_other, Method::GET, "/api/v1/me")
            .send()
            .await?,
        200,
    )
    .await?;
    passed.push("browser sessions: own sessions across surfaces with the current one first, another person's hidden, revocation needs CSRF, refuses the current and foreign sessions, and ends the chosen one".into());

    // Launcher categories: portals read, only a console administrator arranges,
    // with known apps in at most one category; every change is audited.
    for app in ["boundary-ledger", "boundary-notes"] {
        sqlx::query("INSERT INTO apps(id,name) VALUES($1,$1) ON CONFLICT DO NOTHING")
            .bind(app)
            .execute(&state.pool)
            .await?;
    }
    let before = json_response(
        h.cookie_request(&portal_user, Method::GET, "/api/v1/launcher/categories")
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        before["categories"].is_array(),
        "the launcher arrangement was not readable"
    );
    let category = json!({"title":"업무 지원","iconKey":"layout-grid"});
    denied(
        h.browser_request(&portal_admin, Method::POST, "/api/v1/launcher/categories")
            .json(&category)
            .send()
            .await?,
        403,
    )
    .await?;
    denied(
        h.browser_request(&console_user, Method::POST, "/api/v1/launcher/categories")
            .json(&category)
            .send()
            .await?,
        403,
    )
    .await?;
    let created = json_response(
        h.browser_request(&console_admin, Method::POST, "/api/v1/launcher/categories")
            .json(&category)
            .send()
            .await?,
        200,
    )
    .await?;
    let id = created["categories"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["title"] == "업무 지원"))
        .and_then(|item| item["id"].as_str())
        .context("created category")?
        .to_owned();
    for layout in [
        json!({"expectedRevision":created["revision"],"categories":[{"id":id,"title":"업무 지원","iconKey":"layout-grid","appIds":["boundary-ledger","boundary-ledger"]}]}),
        json!({"expectedRevision":created["revision"],"categories":[{"id":id,"title":"업무 지원","iconKey":"layout-grid","appIds":["no-such-app"]}]}),
        json!({"expectedRevision":created["revision"],"categories":[{"id":id,"title":"","iconKey":"layout-grid","appIds":[]}]}),
    ] {
        denied(
            h.browser_request(
                &console_admin,
                Method::PUT,
                "/api/v1/launcher/categories/layout",
            )
            .json(&layout)
            .send()
            .await?,
            400,
        )
        .await?;
    }
    let arranged = json_response(
        h.browser_request(&console_admin, Method::PUT, "/api/v1/launcher/categories/layout")
            .json(&json!({"expectedRevision":created["revision"],"categories":[{"id":id,"title":"업무 지원","iconKey":"briefcase","appIds":["boundary-notes","boundary-ledger"]}]}))
            .send()
            .await?,
        200,
    )
    .await?;
    denied(
        h.browser_request(
            &console_admin,
            Method::PUT,
            "/api/v1/launcher/categories/layout",
        )
        .json(&json!({"expectedRevision":created["revision"],"categories":[]}))
        .send()
        .await?,
        409,
    )
    .await?;
    let seen = json_response(
        h.cookie_request(&portal_user, Method::GET, "/api/v1/launcher/categories")
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        arranged == seen
            && seen["categories"].as_array().map(Vec::len) == Some(1)
            && seen["categories"][0]["appIds"] == json!(["boundary-notes", "boundary-ledger"])
            && seen["categories"][0]["iconKey"] == "briefcase",
        "the launcher arrangement did not persist as laid out"
    );
    let removed = json_response(
        h.browser_request(
            &console_admin,
            Method::DELETE,
            &format!("/api/v1/launcher/categories/{id}"),
        )
        .send()
        .await?,
        200,
    )
    .await?;
    let audited: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit WHERE actor=$1 AND action LIKE 'launcher.%'",
    )
    .bind(&admin_id)
    .fetch_one(&state.pool)
    .await?;
    ensure!(
        removed["categories"].as_array().map(Vec::len) == Some(0) && audited == 3,
        "a launcher change was not applied or not audited"
    );
    passed.push("launcher categories: every surface reads the arrangement, only a console administrator changes it, unknown or repeated apps and empty titles are refused, the layout persists in order and each change is audited".into());

    // Notification inbox: each portal reads its own environment's notifications
    // of apps the person may use now; the console has no inbox.
    let notify_app = format!("boundary-notify-{}", Uuid::new_v4().simple());
    sqlx::query("INSERT INTO apps(id,name) VALUES($1,'Boundary notify')")
        .bind(&notify_app)
        .execute(&state.pool)
        .await?;
    let notify_release = Uuid::new_v4();
    // A complete manifest: later suites list every installation of this database.
    let notify_manifest = json!({
        "contractVersion":"miy.app/v1alpha1","id":notify_app,"name":"Boundary notify","version":"1.0.0",
        "runtime":{"profile":"rust-web-v1"},
        "ui":{"delivery":"module","moduleEntry":"mf-manifest.json","moduleExport":"./AppRoot","contractVersion":"miy.ui/v1alpha1"},
        "resources":{"database":"postgresql"},"migrations":{"directory":"migrations"},
        "requestedPermissions":[],
    });
    sqlx::query("INSERT INTO releases(id,app_id,version,artifact_hash,manifest) VALUES($1,$2,'1.0.0',$3,$4)")
        .bind(notify_release)
        .bind(&notify_app)
        .bind(hash(format!("boundary-notify:{notify_release}")))
        .bind(&notify_manifest)
        .execute(&state.pool)
        .await?;
    let mut notify_installations = std::collections::HashMap::new();
    for environment in ["dev", "prod"] {
        let installation = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO installations(id,app_id,environment,release_id) VALUES($1,$2,$3,$4)",
        )
        .bind(installation)
        .bind(&notify_app)
        .bind(environment)
        .bind(notify_release)
        .execute(&state.pool)
        .await?;
        sqlx::query(
            "INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,$2,$3,'use')",
        )
        .bind(&user_id)
        .bind(&notify_app)
        .bind(environment)
        .execute(&state.pool)
        .await?;
        for (group, title) in [("a", "첫 알림"), ("b", "둘째 알림")] {
            sqlx::query("INSERT INTO notifications(id,installation_id,app_id,environment,user_id,kind,group_key,title,path,actor) VALUES($1,$2,$3,$4,$5,'message',$6,$7,'/inbox',$5)")
                .bind(Uuid::new_v4())
                .bind(installation)
                .bind(&notify_app)
                .bind(environment)
                .bind(&user_id)
                .bind(format!("{environment}-{group}"))
                .bind(format!("{environment} {title}"))
                .execute(&state.pool)
                .await?;
        }
        notify_installations.insert(environment, installation);
    }
    let dev_user = h.browser(&user_id, BrowserPurpose::DevPortal).await?;
    let prod_inbox = json_response(
        h.cookie_request(&portal_user, Method::GET, "/api/v1/notifications")
            .send()
            .await?,
        200,
    )
    .await?;
    let dev_inbox = json_response(
        h.cookie_request(&dev_user, Method::GET, "/api/v1/notifications")
            .send()
            .await?,
        200,
    )
    .await?;
    let titles = |inbox: &Value| -> Vec<String> {
        inbox["items"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter(|item| item["appId"] == notify_app.as_str())
                    .filter_map(|item| item["title"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    ensure!(
        titles(&prod_inbox)
            .iter()
            .all(|title| title.starts_with("prod "))
            && titles(&prod_inbox).len() == 2
            && titles(&dev_inbox)
                .iter()
                .all(|title| title.starts_with("dev "))
            && titles(&dev_inbox).len() == 2
            && prod_inbox["unread"].as_u64() >= Some(2),
        "a portal saw another environment's notifications"
    );
    denied(
        h.cookie_request(&console_user, Method::GET, "/api/v1/notifications")
            .send()
            .await?,
        403,
    )
    .await?;
    let first = prod_inbox["items"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["appId"] == notify_app.as_str())
        })
        .and_then(|item| item["id"].as_str())
        .context("prod notification")?
        .to_owned();
    denied(
        h.cookie_request(
            &portal_user,
            Method::POST,
            &format!("/api/v1/notifications/{first}/read"),
        )
        .send()
        .await?,
        403,
    )
    .await?;
    status(
        h.browser_request(
            &portal_user,
            Method::POST,
            &format!("/api/v1/notifications/{first}/read"),
        )
        .send()
        .await?,
        204,
    )
    .await?;
    // The dev portal cannot mark a prod notification.
    let dev_first = dev_inbox["items"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["appId"] == notify_app.as_str())
        })
        .and_then(|item| item["id"].as_str())
        .context("dev notification")?
        .to_owned();
    denied(
        h.browser_request(
            &dev_user,
            Method::POST,
            &format!("/api/v1/notifications/{first}/read"),
        )
        .send()
        .await?,
        404,
    )
    .await?;
    status(
        h.browser_request(&dev_user, Method::POST, "/api/v1/notifications/read-all")
            .send()
            .await?,
        204,
    )
    .await?;
    let unread_dev: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications WHERE user_id=$1 AND app_id=$2 AND environment='dev' AND read_at IS NULL")
        .bind(&user_id)
        .bind(&notify_app)
        .fetch_one(&state.pool)
        .await?;
    let unread_prod: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications WHERE user_id=$1 AND app_id=$2 AND environment='prod' AND read_at IS NULL")
        .bind(&user_id)
        .bind(&notify_app)
        .fetch_one(&state.pool)
        .await?;
    ensure!(
        unread_dev == 0 && unread_prod == 1 && !dev_first.is_empty(),
        "reading notifications crossed environments"
    );
    // Opening the panel rechecks with the app's declared endpoint: the app sees
    // the person as a platform call (no admin, no call token) and a definite
    // denial removes that notification; an app without an answer only hides.
    let check_secret = auth::random();
    let seen_claims = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let check_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let check_port = i32::from(check_listener.local_addr()?.port());
    let check_router = {
        let secret = check_secret.clone();
        let seen = seen_claims.clone();
        Router::new().route(
            "/api/notification-check",
            axum::routing::post(
                move |headers: axum::http::HeaderMap, Json(body): Json<Value>| {
                    let secret = secret.clone();
                    let seen = seen.clone();
                    async move {
                        let mut validation =
                            jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
                        validation.set_audience(&["miy-app"]);
                        validation.set_issuer(&["miy"]);
                        let claims = headers
                            .get("x-miy-identity")
                            .and_then(|value| value.to_str().ok())
                            .and_then(|token| {
                                jsonwebtoken::decode::<Value>(
                                    token,
                                    &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
                                    &validation,
                                )
                                .ok()
                            })
                            .map(|decoded| decoded.claims);
                        let Some(claims) = claims else {
                            return Json(json!({"allowed": []}));
                        };
                        seen.lock()
                            .unwrap()
                            .push(json!({"claims": claims, "body": body}));
                        Json(json!({"allowed": ["prod-a"]}))
                    }
                },
            ),
        )
    };
    let check_server = tokio::spawn(async move {
        let _ = axum::serve(check_listener, check_router).await;
    });
    sqlx::query("UPDATE releases SET manifest=manifest||$2 WHERE id=$1")
        .bind(notify_release)
        .bind(json!({"platformEndpoints": {"notificationCheck": "/notification-check"}}))
        .execute(&state.pool)
        .await?;
    sqlx::query("UPDATE installations SET runtime_status='running',desired_state='running',runtime_port=$2,runtime_secret=$3 WHERE id=$1")
        .bind(notify_installations["prod"])
        .bind(check_port)
        .bind(auth::encrypt(&check_secret)?)
        .execute(&state.pool)
        .await?;
    let checked = json_response(
        h.cookie_request(
            &portal_user,
            Method::GET,
            "/api/v1/notifications?check=true",
        )
        .send()
        .await?,
        200,
    )
    .await?;
    let remaining: Vec<String> = sqlx::query_scalar(
        "SELECT group_key FROM notifications WHERE user_id=$1 AND app_id=$2 AND environment='prod' ORDER BY group_key",
    )
    .bind(&user_id)
    .bind(&notify_app)
    .fetch_all(&state.pool)
    .await?;
    let claims = seen_claims
        .lock()
        .unwrap()
        .first()
        .cloned()
        .unwrap_or(Value::Null);
    ensure!(
        titles(&checked) == vec!["prod 첫 알림".to_owned()]
            && remaining == vec!["prod-a".to_owned()]
            && claims["claims"]["platform"] == true
            && claims["claims"]["admin"] == false
            && claims["claims"]["sub"] == user_id.as_str()
            && claims["claims"].get("delegation_token").is_none()
            && claims["body"]["items"].as_array().map(Vec::len) == Some(2),
        "the panel recheck did not ask the app as the person or did not apply its answer"
    );
    // The dev installation is not running: its notifications are hidden, kept.
    let dev_checked = json_response(
        h.cookie_request(&dev_user, Method::GET, "/api/v1/notifications?check=true")
            .send()
            .await?,
        200,
    )
    .await?;
    let dev_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notifications WHERE user_id=$1 AND app_id=$2 AND environment='dev'",
    )
    .bind(&user_id)
    .bind(&notify_app)
    .fetch_one(&state.pool)
    .await?;
    ensure!(
        titles(&dev_checked).is_empty() && dev_rows == 2,
        "an app without an answer lost or showed its notifications"
    );
    check_server.abort();
    // Losing use hides the app's notifications at once; the rows stay for a restore.
    sqlx::query("DELETE FROM grants WHERE user_id=$1 AND app_id=$2 AND environment='prod'")
        .bind(&user_id)
        .bind(&notify_app)
        .execute(&state.pool)
        .await?;
    let hidden = json_response(
        h.cookie_request(&portal_user, Method::GET, "/api/v1/notifications")
            .send()
            .await?,
        200,
    )
    .await?;
    let count = json_response(
        h.cookie_request(
            &portal_user,
            Method::GET,
            "/api/v1/notifications/unread-count",
        )
        .send()
        .await?,
        200,
    )
    .await?;
    ensure!(
        titles(&hidden).is_empty() && count["unread"].as_u64() == hidden["unread"].as_u64(),
        "notifications of an app the person lost stayed visible or counted"
    );
    let settings =
        json!({"mutes":[{"appId":notify_app,"kind":"message"},{"appId":notify_app,"kind":"*"}]});
    let saved = json_response(
        h.browser_request(&dev_user, Method::PUT, "/api/v1/notifications/settings")
            .json(&settings)
            .send()
            .await?,
        200,
    )
    .await?;
    denied(
        h.browser_request(&dev_user, Method::PUT, "/api/v1/notifications/settings")
            .json(&json!({"mutes":[{"appId":"Bad","kind":"message"}]}))
            .send()
            .await?,
        400,
    )
    .await?;
    ensure!(saved == settings, "notification settings were not saved");
    // The portal's live channel pushes the unread count on every inbox change
    // and closes when the browser session ends; the console has none.
    {
        use futures_util::StreamExt;
        use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
        let channel = |browser: &Browser| -> Result<_> {
            let origin = h.origin(browser.purpose).to_owned();
            let mut request = format!("{}/api/v1/live", origin.replacen("http", "ws", 1))
                .into_client_request()?;
            request
                .headers_mut()
                .insert("cookie", h.cookie(browser).parse()?);
            request.headers_mut().insert("origin", origin.parse()?);
            request
                .headers_mut()
                .insert("sec-fetch-site", "same-origin".parse()?);
            Ok(request)
        };
        ensure!(
            tokio_tungstenite::connect_async(channel(&console_user)?)
                .await
                .is_err(),
            "the console opened a notification channel"
        );
        let (mut socket, _) = tokio_tungstenite::connect_async(channel(&dev_user)?).await?;
        let mut next_count = async || -> Result<Value> {
            loop {
                match tokio::time::timeout(std::time::Duration::from_secs(15), socket.next())
                    .await?
                {
                    Some(Ok(Message::Text(text))) => {
                        return Ok(serde_json::from_str(text.as_str())?);
                    }
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
                    other => anyhow::bail!(
                        "notification channel ended: {:?}",
                        other.map(|m| m.map(|m| m.is_close()))
                    ),
                }
            }
        };
        let first = next_count().await?;
        ensure!(
            first == json!({"type":"notifications.unread","unread":0}),
            "the channel did not start with the current count"
        );
        sqlx::query("UPDATE notifications SET read_at=NULL WHERE user_id=$1 AND app_id=$2 AND environment='dev' AND group_key='dev-a'")
            .bind(&user_id)
            .bind(&notify_app)
            .execute(&state.pool)
            .await?;
        sqlx::query("SELECT pg_notify('miy_notifications', $1 || ' dev')")
            .bind(&user_id)
            .execute(&state.pool)
            .await?;
        let pushed = next_count().await?;
        drop(next_count);
        ensure!(
            pushed["unread"].as_i64().is_some_and(|count| count >= 1),
            "an inbox change did not reach the live channel"
        );
        sqlx::query("DELETE FROM development_sessions WHERE id_hash=$1")
            .bind(hash(&dev_user.cookie))
            .execute(&state.pool)
            .await?;
        let code = loop {
            match tokio::time::timeout(
                crate::live::RECHECK + std::time::Duration::from_secs(15),
                socket.next(),
            )
            .await?
            {
                Some(Ok(Message::Close(frame))) => break frame.map(|f| u16::from(f.code)),
                Some(Ok(_)) => continue,
                _ => break None,
            }
        };
        ensure!(
            code == Some(crate::live::close::SESSION_ENDED),
            "the notification channel outlived its browser session"
        );
    }
    let _ = notify_installations;
    passed.push("notification inbox: each portal reads and marks only its environment's notifications, the console has none, CSRF is required, losing use hides them from the list and the count, mute settings are validated and saved, and the live channel pushes the count and closes with the session".into());

    // Home summary cards and side panels (M5): the prod portal asks each app the
    // person may use that declares a card, as a platform call; the answer is
    // cut to the card contract, an app that cannot answer is marked
    // unavailable, and an app without use has no card. Installations list the
    // panels their release declares.
    let home_secret = auth::random();
    let home_seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let home_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let home_port = i32::from(home_listener.local_addr()?.port());
    let home_router = {
        let secret = home_secret.clone();
        let seen = home_seen.clone();
        Router::new().route(
            "/api/home-card",
            axum::routing::post(
                move |headers: axum::http::HeaderMap, Json(body): Json<Value>| {
                    let secret = secret.clone();
                    let seen = seen.clone();
                    async move {
                        let mut validation =
                            jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
                        validation.set_audience(&["miy-app"]);
                        validation.set_issuer(&["miy"]);
                        let claims = headers
                            .get("x-miy-identity")
                            .and_then(|value| value.to_str().ok())
                            .and_then(|token| {
                                jsonwebtoken::decode::<Value>(
                                    token,
                                    &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
                                    &validation,
                                )
                                .ok()
                            })
                            .map(|decoded| decoded.claims)
                            .unwrap_or(Value::Null);
                        seen.lock()
                            .unwrap()
                            .push(json!({"claims": claims, "body": body}));
                        Json(json!({
                            "title": "가".repeat(100),
                            "count": 2,
                            "extra": "dropped",
                            "items": [
                                {"title": "<b>주간 회의</b>", "subtitle": "회의실 A", "at": "2026-11-22T10:00:00+09:00", "path": "/?event=1", "html": "<i>x</i>"},
                                {"title": "외부 링크", "path": "https://untrusted.invalid/"},
                                {"title": ""}
                            ]
                        }))
                    }
                },
            ),
        )
    };
    let home_router = home_router.route(
        "/api/search",
        axum::routing::post({
            let seen = home_seen.clone();
            move |Json(body): Json<Value>| {
                let seen = seen.clone();
                async move {
                    seen.lock().unwrap().push(json!({"search": body}));
                    Json(json!({"items": [
                        {"title": "<i>검색 결과</i>", "snippet": "가".repeat(300), "path": "/found/1", "kind": "일정", "secret": "dropped"},
                        {"title": "둘째", "path": "https://untrusted.invalid/"},
                        {"title": "셋째"}
                    ]}))
                }
            }
        }),
    );
    let home_server = tokio::spawn(async move {
        let _ = axum::serve(home_listener, home_router).await;
    });
    let mut home_apps = Vec::new();
    for (label, running, granted, panels) in [
        (
            "card",
            true,
            true,
            json!([{"id":"todos","title":"할 일","path":"/widgets/todos"}]),
        ),
        ("down", false, true, json!([])),
        ("hidden", true, false, json!([])),
    ] {
        let app = format!("boundary-home-{label}-{}", Uuid::new_v4().simple());
        sqlx::query("INSERT INTO apps(id,name) VALUES($1,$2)")
            .bind(&app)
            .bind(format!("Home {label}"))
            .execute(&state.pool)
            .await?;
        let release = Uuid::new_v4();
        let manifest = json!({
            "contractVersion":"miy.app/v1alpha1","id":app,"name":"Home fixture","version":"1.0.0",
            "runtime":{"profile":"rust-web-v1"},
            "ui":{"delivery":"module","moduleEntry":"mf-manifest.json","moduleExport":"./AppRoot","contractVersion":"miy.ui/v1alpha1"},
            "resources":{"database":"postgresql"},"migrations":{"directory":"migrations"},
            "requestedPermissions":[],"panels":panels,
            "platformEndpoints":{"homeCard":"/home-card","search":"/search"},
        });
        sqlx::query("INSERT INTO releases(id,app_id,version,artifact_hash,manifest) VALUES($1,$2,'1.0.0',$3,$4)")
            .bind(release)
            .bind(&app)
            .bind(hash(format!("boundary-home:{release}")))
            .bind(&manifest)
            .execute(&state.pool)
            .await?;
        let installation = Uuid::new_v4();
        sqlx::query("INSERT INTO installations(id,app_id,environment,release_id,db_status,runtime_status,desired_state,runtime_port,runtime_secret) VALUES($1,$2,'prod',$3,'ready',$4,$4,$5,$6)")
            .bind(installation)
            .bind(&app)
            .bind(release)
            .bind(if running { "running" } else { "stopped" })
            .bind(home_port)
            .bind(auth::encrypt(&home_secret)?)
            .execute(&state.pool)
            .await?;
        if granted {
            sqlx::query("INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,$2,'prod','use')")
                .bind(&user_id)
                .bind(&app)
                .execute(&state.pool)
                .await?;
        }
        home_apps.push((app, installation));
    }
    let home = json_response(
        h.cookie_request(&portal_user, Method::GET, "/api/v1/home/cards")
            .send()
            .await?,
        200,
    )
    .await?;
    let card_of = |app: &str| -> Value {
        home["cards"]
            .as_array()
            .and_then(|cards| cards.iter().find(|card| card["appId"] == app))
            .cloned()
            .unwrap_or(Value::Null)
    };
    let (card, down, hidden) = (
        card_of(&home_apps[0].0),
        card_of(&home_apps[1].0),
        card_of(&home_apps[2].0),
    );
    let home_claims = home_seen.lock().unwrap().clone();
    ensure!(
        card["status"] == "ok"
            && card["title"].as_str().map(|t| t.chars().count()) == Some(80)
            && card["count"] == 2
            && card["items"]
                == json!([
                    {"title":"<b>주간 회의</b>","subtitle":"회의실 A","path":"/?event=1","at":"2026-11-22T01:00:00Z"},
                    {"title":"외부 링크","subtitle":null,"path":null,"at":null}
                ])
            && card.get("extra").is_none(),
        "the home card was not cut to the card contract: {card}"
    );
    ensure!(
        down["status"] == "unavailable" && down["items"] == json!([]) && hidden.is_null(),
        "an app that cannot answer was not marked, or an app without use had a card"
    );
    ensure!(
        home_claims.len() == 1
            && home_claims[0]["claims"]["platform"] == true
            && home_claims[0]["claims"]["admin"] == false
            && home_claims[0]["claims"]["sub"] == user_id.as_str()
            && home_claims[0]["claims"].get("delegation_token").is_none()
            && home_claims[0]["body"] == json!({"timeZone":"Europe/Berlin","locale":"en-US"}),
        "the home card call did not carry the person's platform identity and display settings"
    );
    // Integrated search asks the same apps the same way, with the query.
    let found = json_response(
        h.cookie_request(
            &portal_user,
            Method::GET,
            "/api/v1/search?q=%20%ED%9A%8C%EC%9D%98%20&limit=2",
        )
        .send()
        .await?,
        200,
    )
    .await?;
    let group_of = |app: &str| -> Value {
        found["groups"]
            .as_array()
            .and_then(|groups| groups.iter().find(|group| group["appId"] == app))
            .cloned()
            .unwrap_or(Value::Null)
    };
    let (hits, slow, unseen) = (
        group_of(&home_apps[0].0),
        group_of(&home_apps[1].0),
        group_of(&home_apps[2].0),
    );
    let search_body = home_seen
        .lock()
        .unwrap()
        .iter()
        .find_map(|seen| seen.get("search").cloned())
        .unwrap_or(Value::Null);
    ensure!(
        found["query"] == "회의"
            && hits["status"] == "ok"
            && hits["items"]
                == json!([
                    {"title":"<i>검색 결과</i>","snippet":"가".repeat(240),"path":"/found/1","at":null,"kind":"일정"},
                    {"title":"둘째","snippet":null,"path":null,"at":null,"kind":null}
                ])
            && slow["status"] == "unavailable"
            && unseen.is_null()
            && search_body
                == json!({"query":"회의","limit":2,"timeZone":"Europe/Berlin","locale":"en-US"}),
        "integrated search did not ask usable apps as the person or did not cut answers to the contract: {found}"
    );
    for bad in ["", "%20%20", "a%00b"] {
        denied(
            h.cookie_request(
                &portal_user,
                Method::GET,
                &format!("/api/v1/search?q={bad}"),
            )
            .send()
            .await?,
            400,
        )
        .await?;
    }
    // Without directory approval the platform call carries no groups; with it,
    // the person's current groups, so the app's ACL can see group grants.
    ensure!(
        home_claims[0]["claims"].get("groups").is_none(),
        "a platform call carried groups without directory approval"
    );
    let home_group = Uuid::new_v4();
    sqlx::query("INSERT INTO groups(id,name,source) VALUES($1,'Home fixture group','local')")
        .bind(home_group)
        .execute(&state.pool)
        .await?;
    sqlx::query("INSERT INTO group_members(group_id,user_id) VALUES($1,$2)")
        .bind(home_group)
        .bind(&user_id)
        .execute(&state.pool)
        .await?;
    let card_release: Uuid = sqlx::query_scalar("SELECT release_id FROM installations WHERE id=$1")
        .bind(home_apps[0].1)
        .fetch_one(&state.pool)
        .await?;
    sqlx::query("UPDATE releases SET manifest=jsonb_set(manifest,'{requestedPermissions}','[\"platform:directory:read\"]') WHERE id=$1")
        .bind(card_release)
        .execute(&state.pool)
        .await?;
    sqlx::query("INSERT INTO installation_permissions(installation_id,release_id,revision,permissions,approved_by) VALUES($1,$2,1,$3,$4)")
        .bind(home_apps[0].1)
        .bind(card_release)
        .bind(json!(["platform:directory:read"]))
        .bind(&admin_id)
        .execute(&state.pool)
        .await?;
    home_seen.lock().unwrap().clear();
    json_response(
        h.cookie_request(&portal_user, Method::GET, "/api/v1/home/cards")
            .send()
            .await?,
        200,
    )
    .await?;
    let with_groups = home_seen.lock().unwrap().clone();
    ensure!(
        with_groups.iter().any(|seen| seen["claims"]["groups"]
            .as_array()
            .is_some_and(|groups| groups.iter().any(|g| g == &json!(home_group.to_string())))),
        "a platform call to a directory-approved app did not carry the person's groups"
    );
    denied(
        h.cookie_request(&console_user, Method::GET, "/api/v1/search?q=x")
            .send()
            .await?,
        403,
    )
    .await?;
    denied(
        h.cookie_request(&console_user, Method::GET, "/api/v1/home/cards")
            .send()
            .await?,
        403,
    )
    .await?;
    let dev_home_user = h.browser(&user_id, BrowserPurpose::DevPortal).await?;
    let dev_home = json_response(
        h.cookie_request(&dev_home_user, Method::GET, "/api/v1/home/cards")
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        !dev_home["cards"]
            .as_array()
            .is_some_and(|cards| cards.iter().any(|card| home_apps
                .iter()
                .any(|(app, _)| card["appId"] == app.as_str()))),
        "the dev portal showed prod home cards"
    );
    let listed = json_response(
        h.cookie_request(&portal_user, Method::GET, "/api/v1/installations")
            .send()
            .await?,
        200,
    )
    .await?;
    let panels_of = |id: Uuid| -> Value {
        listed
            .as_array()
            .and_then(|items| items.iter().find(|item| item["id"] == id.to_string()))
            .map(|item| item["panels"].clone())
            .unwrap_or(Value::Null)
    };
    ensure!(
        panels_of(home_apps[0].1)
            == json!([{"id":"todos","title":"할 일","path":"/widgets/todos"}])
            && panels_of(home_apps[1].1) == json!([]),
        "installations did not list their release's panels"
    );
    home_server.abort();
    passed.push("home cards and integrated search: the prod portal asks only apps the person may use that declare a card or search, as a platform call with the person's display settings (and the trimmed query and limit); answers are cut to the contract, the person's current groups go only to a directory-approved app, an app that cannot answer is marked unavailable, invalid queries and the console are refused, the dev portal sees no prod cards, and installations list their release's panels".into());

    // User switch (M6): a console administrator gets a one-time code for one
    // portal; that portal exchanges it, same-origin only, for the person's
    // ordinary session, ending the browser's previous session there. The
    // session ends with the administrator and signs out without the identity
    // provider.
    let switch_admin = h.user(true).await?;
    let switch_console = h.browser(&switch_admin, BrowserPurpose::Console).await?;
    let switch_target = h.user(false).await?;
    let start = |browser: &Browser, target: &str, environment: &str| {
        h.browser_request(
            browser,
            Method::POST,
            &format!("/api/v1/users/{target}/sign-in-as"),
        )
        .json(&json!({ "environment": environment }))
    };
    denied(
        h.cookie_request(
            &switch_console,
            Method::POST,
            &format!("/api/v1/users/{switch_target}/sign-in-as"),
        )
        .json(&json!({"environment":"prod"}))
        .send()
        .await?,
        403,
    )
    .await?;
    denied(
        start(&console_user, &switch_target, "prod").send().await?,
        403,
    )
    .await?;
    denied(
        start(&portal_admin, &switch_target, "prod").send().await?,
        403,
    )
    .await?;
    let switch_bearer = h.legacy_retired(&switch_admin).await?;
    denied(
        h.request(
            BrowserPurpose::Portal,
            Method::POST,
            &format!("/api/v1/users/{switch_target}/sign-in-as"),
        )
        .bearer_auth(switch_bearer)
        .json(&json!({"environment":"prod"}))
        .send()
        .await?,
        403,
    )
    .await?;
    denied(
        start(&switch_console, &switch_admin, "prod").send().await?,
        400,
    )
    .await?;
    denied(
        start(&switch_console, &switch_target, "staging")
            .send()
            .await?,
        400,
    )
    .await?;
    let issue = |environment: &'static str| {
        let request = start(&switch_console, &switch_target, environment);
        async move {
            let value = json_response(request.send().await?, 200).await?;
            let url = value["url"].as_str().unwrap_or_default().to_owned();
            let (base, code) = url
                .split_once('#')
                .ok_or_else(|| anyhow::anyhow!("switch URL without a code"))?;
            Ok::<_, anyhow::Error>((base.to_owned(), code.to_owned()))
        }
    };
    let (switch_url, code) = issue("prod").await?;
    ensure!(
        switch_url == format!("{}/switch-user", h.origin(BrowserPurpose::Portal)),
        "the switch link does not open the prod portal"
    );
    let exchange_from = |purpose: BrowserPurpose, origin: &str, site: &str, code: &str| {
        h.request(purpose, Method::POST, "/auth/switch")
            .header("origin", origin)
            .header("sec-fetch-site", site)
            .json(&json!({ "code": code }))
    };
    let exchange = |purpose: BrowserPurpose, origin: &str, code: &str| {
        exchange_from(purpose, origin, "same-origin", code)
    };
    // Another origin, or the dev portal for a prod code, leaves the code unused.
    denied(
        exchange(
            BrowserPurpose::Portal,
            h.origin(BrowserPurpose::Console),
            &code,
        )
        .send()
        .await?,
        403,
    )
    .await?;
    denied(
        exchange_from(
            BrowserPurpose::Portal,
            h.origin(BrowserPurpose::Portal),
            "same-site",
            &code,
        )
        .send()
        .await?,
        403,
    )
    .await?;
    // Repeated Fetch Metadata or Origin is refused, not read by its first value.
    denied(
        exchange(
            BrowserPurpose::Portal,
            h.origin(BrowserPurpose::Portal),
            &code,
        )
        .header("sec-fetch-site", "same-site")
        .send()
        .await?,
        403,
    )
    .await?;
    denied(
        exchange(
            BrowserPurpose::Portal,
            h.origin(BrowserPurpose::Portal),
            &code,
        )
        .header("origin", h.origin(BrowserPurpose::Console))
        .send()
        .await?,
        403,
    )
    .await?;
    denied(
        exchange(
            BrowserPurpose::DevPortal,
            h.origin(BrowserPurpose::DevPortal),
            &code,
        )
        .send()
        .await?,
        401,
    )
    .await?;
    let previous = h.browser(&switch_admin, BrowserPurpose::Portal).await?;
    let switched = status(
        exchange(
            BrowserPurpose::Portal,
            h.origin(BrowserPurpose::Portal),
            &code,
        )
        .header("cookie", h.cookie(&previous))
        .send()
        .await?,
        204,
    )
    .await?;
    let session_cookie = |response: &Response| -> Option<String> {
        let name = state.auth.cookie_name(BrowserPurpose::Portal, "session");
        response
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|value| value.to_str().ok())
            .filter_map(|value| value.split(';').next())
            .find(|pair| pair.starts_with(&format!("{name}=")) && pair.len() > name.len() + 1)
            .map(str::to_owned)
    };
    let cookie = session_cookie(&switched)
        .ok_or_else(|| anyhow::anyhow!("the switch did not set a portal session"))?;
    let me = json_response(
        h.request(BrowserPurpose::Portal, Method::GET, "/api/v1/me")
            .header("cookie", &cookie)
            .send()
            .await?,
        200,
    )
    .await?;
    denied(
        h.cookie_request(&previous, Method::GET, "/api/v1/me")
            .send()
            .await?,
        401,
    )
    .await?;
    denied(
        exchange(
            BrowserPurpose::Portal,
            h.origin(BrowserPurpose::Portal),
            &code,
        )
        .send()
        .await?,
        401,
    )
    .await?;
    let audited: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit WHERE actor=$1 AND action='user.sign_in_as' AND target=$2",
    )
    .bind(&switch_admin)
    .bind(format!("user:{switch_target}:environment:prod"))
    .fetch_one(&state.pool)
    .await?;
    ensure!(
        me["id"] == switch_target.as_str() && me["admin"] == false && audited == 1,
        "the switched session is not the person's own or the start was not audited"
    );
    let logout = json_response(
        h.request(BrowserPurpose::Portal, Method::POST, "/auth/logout")
            .header("cookie", &cookie)
            .header("origin", h.origin(BrowserPurpose::Portal))
            .header("x-miy-csrf", me["csrf"].as_str().unwrap_or_default())
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        logout["redirect"] == format!("{}/", h.origin(BrowserPurpose::Portal)),
        "a switched session signed out through the identity provider"
    );
    // An expired code is refused; a live session ends with its administrator.
    let (_, stale) = issue("prod").await?;
    sqlx::query(
        "UPDATE user_switch_codes SET expires_at=now()-interval '1 second' WHERE code_hash=$1",
    )
    .bind(hash(&stale))
    .execute(&state.pool)
    .await?;
    denied(
        exchange(
            BrowserPurpose::Portal,
            h.origin(BrowserPurpose::Portal),
            &stale,
        )
        .send()
        .await?,
        401,
    )
    .await?;
    let (dev_url, dev_code) = issue("dev").await?;
    let dev_switched = status(
        exchange(
            BrowserPurpose::DevPortal,
            h.origin(BrowserPurpose::DevPortal),
            &dev_code,
        )
        .send()
        .await?,
        204,
    )
    .await?;
    let dev_name = state.auth.cookie_name(BrowserPurpose::DevPortal, "session");
    let dev_cookie = dev_switched
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next())
        .find(|pair| pair.starts_with(&format!("{dev_name}=")) && pair.len() > dev_name.len() + 1)
        .map(str::to_owned)
        .ok_or_else(|| anyhow::anyhow!("the dev switch did not set a session"))?;
    let dev_me = json_response(
        h.request(BrowserPurpose::DevPortal, Method::GET, "/api/v1/me")
            .header("cookie", &dev_cookie)
            .send()
            .await?,
        200,
    )
    .await?;
    let switched_app = h.app("dev").await?;
    sqlx::query("INSERT INTO grants(user_id,app_id,environment,permission) SELECT $1,app_id,'dev','use' FROM installations WHERE id=$2")
        .bind(&switch_target)
        .bind(switched_app.installation)
        .execute(&state.pool)
        .await?;
    let launch = json_response(
        h.request(
            BrowserPurpose::DevPortal,
            Method::POST,
            "/api/v1/app-frames",
        )
        .header("cookie", &dev_cookie)
        .header("origin", dev_portal)
        .header("x-miy-csrf", field(&dev_me, "csrf")?)
        .json(&json!({"installationId":switched_app.installation}))
        .send()
        .await?,
        200,
    )
    .await?;
    let frame_origin = field(&launch, "frameOrigin")?;
    let frame = status(
        crate::app_frames_regression::submit_launch(
            h.client,
            frames,
            &frame_origin,
            Some(dev_portal),
            &field(&launch, "code")?,
        )
        .await?,
        200,
    )
    .await?;
    let frame = crate::app_frames_regression::bootstrap(&frame.text().await?)?;
    let frame_host = crate::app_frames_regression::host_of(&frame_origin)?;
    let frame_token = field(&frame, "token")?;
    let frame_release = field(&frame, "releaseId")?;
    let frame_probe = || {
        h.client
            .get(format!("{frames}/_miy/api/probe"))
            .header("host", &frame_host)
            .header("sec-fetch-site", "same-origin")
            .header("x-miy-release", &frame_release)
            .bearer_auth(&frame_token)
    };
    json_response(frame_probe().send().await?, 200).await?;
    // A direct account-state change leaves the parent row in place, so the
    // derived frame must fail its own issuer check even without a cascade.
    sqlx::query("UPDATE users SET admin=false WHERE id=$1")
        .bind(&switch_admin)
        .execute(&state.pool)
        .await?;
    denied(
        h.request(BrowserPurpose::DevPortal, Method::GET, "/api/v1/me")
            .header("cookie", &dev_cookie)
            .send()
            .await?,
        401,
    )
    .await?;
    denied(frame_probe().send().await?, 401).await?;
    let set_admin = |admin: bool| {
        h.browser_request(
            &console_admin,
            Method::PATCH,
            &format!("/api/v1/users/{switch_admin}"),
        )
        .json(&json!({"admin":admin}))
    };
    json_response(set_admin(true).send().await?, 200).await?;
    let (_, unexchanged) = issue("dev").await?;
    json_response(set_admin(false).send().await?, 200).await?;
    json_response(set_admin(true).send().await?, 200).await?;
    denied(
        h.request(BrowserPurpose::DevPortal, Method::GET, "/api/v1/me")
            .header("cookie", &dev_cookie)
            .send()
            .await?,
        401,
    )
    .await?;
    denied(frame_probe().send().await?, 401).await?;
    denied(
        exchange(BrowserPurpose::DevPortal, dev_portal, &unexchanged)
            .send()
            .await?,
        401,
    )
    .await?;
    ensure!(
        dev_url == format!("{}/switch-user", h.origin(BrowserPurpose::DevPortal))
            && dev_me["id"] == switch_target.as_str(),
        "the dev portal switch did not sign in as the person"
    );
    passed.push("user switch: only a console administrator with CSRF starts it (not for themself), the start is audited, the one-time code works once, unexpired, on its own portal from that portal's origin, the result is the person's own session replacing the browser's previous one, it signs out without the identity provider, and issuer demotion ends the session and frames and invalidates pending codes even after re-promotion".into());

    // File storage limits (M10): only a console administrator reads or sets an
    // installation's limits; each change is audited.
    let quota_admin = h.user(true).await?;
    let quota_console = h.browser(&quota_admin, BrowserPurpose::Console).await?;
    let quota_path = format!("/api/v1/installations/{}/file-quota", home_apps[0].1);
    let defaults = json_response(
        h.cookie_request(&quota_console, Method::GET, &quota_path)
            .send()
            .await?,
        200,
    )
    .await?;
    let set = json_response(
        h.browser_request(&quota_console, Method::PUT, &quota_path)
            .json(&json!({"maxBytes": 1048576, "maxFiles": 10}))
            .send()
            .await?,
        200,
    )
    .await?;
    denied(
        h.browser_request(&quota_console, Method::PUT, &quota_path)
            .json(&json!({"maxBytes": 2_000_000_000_000u64, "maxFiles": 10}))
            .send()
            .await?,
        400,
    )
    .await?;
    denied(
        h.cookie_request(&quota_console, Method::PUT, &quota_path)
            .json(&json!({"maxBytes": 1, "maxFiles": 1}))
            .send()
            .await?,
        403,
    )
    .await?;
    denied(
        h.cookie_request(&portal_admin, Method::GET, &quota_path)
            .send()
            .await?,
        403,
    )
    .await?;
    denied(
        h.browser_request(&console_user, Method::PUT, &quota_path)
            .json(&json!({"maxBytes": 1, "maxFiles": 1}))
            .send()
            .await?,
        403,
    )
    .await?;
    let reset = json_response(
        h.browser_request(&quota_console, Method::DELETE, &quota_path)
            .send()
            .await?,
        200,
    )
    .await?;
    let quota_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit WHERE actor=$1 AND action='file.quota_set' AND target LIKE $2",
    )
    .bind(&quota_admin)
    .bind(format!("installation:{}:%", home_apps[0].1))
    .fetch_one(&state.pool)
    .await?;
    ensure!(
        defaults["custom"] == false
            && defaults["maxBytes"] == 21474836480u64
            && set["custom"] == true
            && set["maxBytes"] == 1048576
            && set["maxFiles"] == 10
            && reset["custom"] == false
            && quota_audits == 2,
        "file storage limits were not set, reset and audited by the console administrator only"
    );
    passed.push("file storage limits: only a console administrator with CSRF reads, sets and resets an installation's limits, out-of-range values are refused, portals and ordinary users are refused, and every change is audited".into());

    // Same-host ports share cookies; code on one origin must not use the other's
    // cookies even for reads. Reject before any session lookup.
    for browser in [&portal_admin, &dev_admin, &console_admin] {
        let foreign = h.origin(if browser.purpose == BrowserPurpose::Portal {
            BrowserPurpose::DevPortal
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

    // The fixture is a dev installation: only a frame launched from the dev portal
    // may run it. Portal cookies never reach app code or data on any listener.
    let app = h.app("dev").await?;
    sqlx::query("INSERT INTO grants(user_id,app_id,environment,permission) SELECT $1,app_id,'dev','use' FROM installations WHERE id=$2")
        .bind(&user_id).bind(app.installation).execute(&state.pool).await?;
    let probe_path = format!("/api/v1/installations/{}/api/probe", app.installation);
    for path in [&probe_path, &app.asset] {
        for browser in [&dev_admin, &portal_admin, &console_admin] {
            denied(
                h.browser_request(browser, Method::GET, path).send().await?,
                403,
            )
            .await?;
        }
        denied(
            h.request(BrowserPurpose::Console, Method::GET, path)
                .send()
                .await?,
            403,
        )
        .await?;
    }
    let frame = crate::app_frames_regression::open(
        h.client,
        state,
        frames,
        &h.portal_session(&dev_admin),
        BrowserPurpose::DevPortal,
        app.installation,
    )
    .await?;
    let probe = json_response(
        frame.request(Method::GET, "/_miy/api/probe").send().await?,
        200,
    )
    .await?;
    ensure!(
        probe["boundaryProbe"] == true,
        "dev frame app API fixture did not execute"
    );
    let asset = status(
        frame
            .raw(Method::GET, &format!("{}boundary.js", frame.asset_base))
            .header("sec-fetch-site", "same-origin")
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
        "app API sandbox broke the frame JavaScript asset path"
    );
    ensure!(
        asset.text().await? == "export const boundaryProbe = true;\n",
        "frame app JavaScript changed"
    );
    denied(
        frame
            .request(Method::GET, "/_miy/api/probe")
            .header("service-worker", "script")
            .send()
            .await?,
        403,
    )
    .await?;
    // The prod portal and the console never launch dev code, even for an administrator.
    for browser in [&portal_admin, &console_admin] {
        status(
            crate::app_frames_regression::launch_code(
                h.client,
                state,
                &h.portal_session(browser),
                browser.purpose,
                app.installation,
            )
            .await?,
            403,
        )
        .await?;
    }
    let html = status(
        frame
            .request(Method::GET, "/_miy/api/document")
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
    passed.push("a dev app's API and JavaScript run only in a frame launched from the dev portal; portal, dev portal and console cookies reach neither, the prod portal and console cannot launch it, and app response headers cannot set cookies, clear site data, redirect, relax CSP/CORS or register workers while HTML API responses stay sandboxed".into());

    // A prod installation is the mirror image: prod portal frames only, with explicit
    // use even for an administrator; the dev portal never launches prod code.
    let prod = h.app("prod").await?;
    sqlx::query("INSERT INTO grants(user_id,app_id,environment,permission) SELECT $1,app_id,'prod','use' FROM installations WHERE id=$2")
        .bind(&admin_id).bind(prod.installation).execute(&state.pool).await?;
    let prod_frame = crate::app_frames_regression::open(
        h.client,
        state,
        frames,
        &h.portal_session(&portal_admin),
        BrowserPurpose::Portal,
        prod.installation,
    )
    .await?;
    ensure!(
        prod_frame.origin != frame.origin,
        "dev and prod installations shared a frame origin"
    );
    json_response(
        prod_frame
            .request(Method::GET, "/_miy/api/probe")
            .send()
            .await?,
        200,
    )
    .await?;
    status(
        crate::app_frames_regression::launch_code(
            h.client,
            state,
            &h.portal_session(&dev_admin),
            BrowserPurpose::DevPortal,
            prod.installation,
        )
        .await?,
        403,
    )
    .await?;
    // The dev portal accepts no bearer credential and serves no management route.
    for path in [
        "/api/v1/users",
        "/api/v1/runtime/installations",
        "/api/v1/cli/sessions",
    ] {
        denied(
            h.browser_request(&dev_admin, Method::GET, path)
                .send()
                .await?,
            403,
        )
        .await?;
    }
    denied(
        h.request(BrowserPurpose::DevPortal, Method::GET, &probe_path)
            .bearer_auth(&legacy_token)
            .send()
            .await?,
        403,
    )
    .await?;
    passed.push("prod apps run only in prod portal frames (explicit use, administrators included) and dev apps only in dev portal frames, each on its own origin; the dev portal refuses bearer credentials and management routes".into());

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
    ensure!(passed.len() == 19, "boundary observation count changed");
    Ok(passed)
}
