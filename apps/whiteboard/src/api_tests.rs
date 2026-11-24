//! HTTP tests through the real SDK gateway-identity check, PostgreSQL and a
//! fake core (directory, notifications, files) that the SDK calls with the
//! signed identity's delegation proof.
//!
//! Ports of AI-DO apps/api/tests/test_whiteboard_hub.py,
//! test_whiteboard_access.py, test_whiteboard_item_mutations.py and
//! test_whiteboard_target.py (without the collaboration, link-share and
//! PMS/meeting cases), plus miy additions (revisions, stored images, spaces,
//! notifications, platform endpoints).
//!
//! They need a disposable PostgreSQL server, so they are ignored by default:
//!   WHITEWHITEBOARD_TEST_DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:PORT/postgres \
//!     cargo test -- --include-ignored
//! Each server start creates a fresh database on that server and applies migrations/.
use axum::{
    body::{Body, Bytes},
    extract::Request,
    http::{HeaderMap, StatusCode as AxumStatus},
    response::{IntoResponse, Response as AxumResponse},
};
use jsonwebtoken::{EncodingKey, Header, encode};
use reqwest::{Client, Method, Response, StatusCode};
use serde_json::{Value, json};
use sha2::Digest;
use sqlx::{Connection, Executor, PgConnection, PgPool, postgres::PgPoolOptions};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

const SECRET: &str = "whiteboard-api-test-gateway-secret-012345";
const INSTALLATION: &str = "00000000-0000-4000-8000-0000000000c1";
const RELEASE: &str = "00000000-0000-4000-8000-0000000000c2";

// ---------------------------------------------------------------- fake core

#[derive(Default)]
struct Core {
    users: Vec<(String, String)>,
    groups: Vec<(String, String)>,
    memberships: HashMap<String, Vec<String>>,
    events: Vec<Value>,
    files: HashMap<String, (Value, Vec<u8>)>,
}

static CORE: OnceLock<Mutex<Core>> = OnceLock::new();
static FAIL_DETACH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static FAIL_ATTACH: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn core() -> std::sync::MutexGuard<'static, Core> {
    CORE.get_or_init(|| Mutex::new(Core::default()))
        .lock()
        .unwrap()
}

fn caller(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer proof:")
        .map(str::to_string)
}

fn query_map(query: Option<&str>) -> HashMap<String, String> {
    query
        .map(|query| {
            url_pairs(query)
                .into_iter()
                .collect::<HashMap<String, String>>()
        })
        .unwrap_or_default()
}

fn url_pairs(query: &str) -> Vec<(String, String)> {
    reqwest::Url::parse(&format!("http://x/?{query}"))
        .unwrap()
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

fn json_response(status: AxumStatus, value: Value) -> AxumResponse {
    (status, axum::Json(value)).into_response()
}

async fn fake_core(request: Request) -> AxumResponse {
    let (parts, body) = request.into_parts();
    let Some(user) = caller(&parts.headers) else {
        return json_response(
            AxumStatus::UNAUTHORIZED,
            json!({"error": {"code": "no_proof"}}),
        );
    };
    let prefix = format!("/api/v1/installations/{INSTALLATION}/");
    let Some(path) = parts.uri.path().strip_prefix(&prefix).map(str::to_string) else {
        return json_response(AxumStatus::NOT_FOUND, json!({}));
    };
    let query = query_map(parts.uri.query());
    let bytes: Bytes = axum::body::to_bytes(body, 32 * 1024 * 1024).await.unwrap();
    let body_json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let method = parts.method.clone();
    let segments: Vec<&str> = path.split('/').collect();
    let mut core = core();
    match (method.as_str(), segments.as_slice()) {
        ("GET", ["platform", "directory"]) => {
            let kind = query.get("kind").cloned().unwrap_or_else(|| "users".into());
            let ids: Option<Vec<String>> = query
                .get("ids")
                .map(|ids| ids.split(',').map(str::to_string).collect());
            let limit: usize = query
                .get("limit")
                .and_then(|l| l.parse().ok())
                .unwrap_or(50);
            let offset: usize = query
                .get("cursor")
                .and_then(|c| c.parse().ok())
                .unwrap_or(0);
            let (users, groups, next) = match kind.as_str() {
                "users" => {
                    let q = query.get("q").map(|q| q.to_lowercase());
                    let all: Vec<&(String, String)> = core
                        .users
                        .iter()
                        .filter(|(id, name)| {
                            ids.as_ref().is_none_or(|ids| ids.contains(id))
                                && q.as_ref().is_none_or(|q| name.to_lowercase().contains(q))
                        })
                        .collect();
                    let page: Vec<Value> = all
                        .iter()
                        .skip(offset)
                        .take(limit)
                        .map(|(id, name)| json!({"id": id, "name": name, "primaryGroup": null}))
                        .collect();
                    let next = (offset + limit < all.len()).then(|| (offset + limit).to_string());
                    (page, vec![], next)
                }
                "groups" => (
                    vec![],
                    core.groups
                        .iter()
                        .filter(|(id, _)| ids.as_ref().is_none_or(|ids| ids.contains(id)))
                        .map(|(id, name)| json!({"id": id, "name": name, "source": "local"}))
                        .collect(),
                    None,
                ),
                _ => (
                    vec![],
                    core.memberships
                        .get(&user)
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|id| ids.as_ref().is_none_or(|ids| ids.contains(id)))
                        .map(|id| json!({"id": id, "name": id, "source": "local"}))
                        .collect(),
                    None,
                ),
            };
            json_response(
                AxumStatus::OK,
                json!({"kind": kind, "subjectId": user, "installationId": INSTALLATION,
                    "environment": "dev", "users": users, "groups": groups, "nextCursor": next}),
            )
        }
        ("POST", ["platform", "notifications"]) => {
            let accepted = body_json["recipients"].as_array().map_or(0, Vec::len);
            core.events
                .push(json!({"op": "notify", "by": user, "body": body_json.clone()}));
            json_response(
                AxumStatus::OK,
                json!({"requestId": body_json["requestId"], "accepted": accepted}),
            )
        }
        ("GET", ["platform", "notifications", "by-request", id]) => {
            json_response(AxumStatus::OK, json!({"requestId": id, "accepted": 1}))
        }
        ("POST", ["platform", "notifications", "retract"]) => {
            core.events
                .push(json!({"op": "retract", "by": user, "body": body_json}));
            json_response(AxumStatus::OK, json!({"retracted": 1}))
        }
        ("POST", ["platform", "notifications", "read"]) => {
            core.events
                .push(json!({"op": "read", "by": user, "body": body_json}));
            json_response(AxumStatus::OK, json!({}))
        }
        ("POST", ["platform", "files"]) => {
            let id = uuid::Uuid::new_v4().to_string();
            let metadata = json!({"id": id, "version": 1, "name": body_json["name"], "mime": body_json["mime"],
                "size": body_json["size"], "sha256": null, "status": "pending", "reference": null,
                "requestId": body_json["requestId"]});
            core.files.insert(id, (metadata.clone(), vec![]));
            json_response(AxumStatus::OK, metadata)
        }
        ("GET", ["platform", "files", "by-request", request_id]) => {
            match core
                .files
                .values()
                .find(|(meta, _)| meta["requestId"] == *request_id)
            {
                Some((meta, _)) => json_response(AxumStatus::OK, meta.clone()),
                None => json_response(AxumStatus::NOT_FOUND, json!({})),
            }
        }
        (method, ["platform", "files", id, rest @ ..]) => {
            let Some((meta, data)) = core.files.get_mut(*id) else {
                return json_response(AxumStatus::NOT_FOUND, json!({}));
            };
            match (method, rest) {
                ("GET", []) => json_response(AxumStatus::OK, meta.clone()),
                ("PUT", ["content"]) => {
                    *data = bytes.to_vec();
                    meta["sha256"] = json!(hex::encode(sha2::Sha256::digest(&bytes)));
                    meta["status"] = json!("ready");
                    json_response(AxumStatus::OK, meta.clone())
                }
                ("POST", ["attach"])
                    if meta["name"] == "fail-attach.png"
                        && FAIL_ATTACH.load(std::sync::atomic::Ordering::SeqCst) =>
                {
                    json_response(AxumStatus::SERVICE_UNAVAILABLE, json!({}))
                }
                ("POST", ["attach"]) => {
                    meta["status"] = json!("attached");
                    meta["reference"] = body_json["reference"].clone();
                    json_response(AxumStatus::OK, meta.clone())
                }
                ("POST", ["detach"])
                    if meta["name"] == "fail-detach.png"
                        && FAIL_DETACH.load(std::sync::atomic::Ordering::SeqCst) =>
                {
                    json_response(AxumStatus::SERVICE_UNAVAILABLE, json!({}))
                }
                ("POST", ["detach"]) | ("DELETE", []) => {
                    meta["status"] = json!("cancelled");
                    json_response(AxumStatus::OK, meta.clone())
                }
                ("GET", ["content"]) => AxumResponse::builder()
                    .status(200)
                    .header(
                        "content-type",
                        meta["mime"].as_str().unwrap_or("application/octet-stream"),
                    )
                    .header("content-length", data.len())
                    .body(Body::from(data.clone()))
                    .unwrap(),
                _ => json_response(AxumStatus::NOT_FOUND, json!({})),
            }
        }
        _ => json_response(AxumStatus::NOT_FOUND, json!({})),
    }
}

fn events() -> Vec<Value> {
    core().events.clone()
}

fn file_status(file_id: &str) -> String {
    core().files[file_id].0["status"]
        .as_str()
        .unwrap()
        .to_string()
}

/// A person in the fake directory.
fn user(name: &str) -> String {
    let id = format!("u-{}", uuid::Uuid::new_v4());
    core().users.push((id.clone(), name.to_string()));
    id
}

fn group(name: &str, members: &[&str]) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let mut core = core();
    core.groups.push((id.clone(), name.to_string()));
    for member in members {
        core.memberships
            .entry(member.to_string())
            .or_default()
            .push(id.clone());
    }
    id
}

// ---------------------------------------------------------------- app server

fn database_url_with_name(url: &str, name: &str) -> String {
    let (base, query) = match url.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (url, None),
    };
    let prefix = base.rsplit_once('/').map_or(base, |(prefix, _)| prefix);
    match query {
        Some(query) => format!("{prefix}/{name}?{query}"),
        None => format!("{prefix}/{name}"),
    }
}

async fn prepare_database(admin_url: &str) -> PgPool {
    let name = format!("whiteboard_test_{}", uuid::Uuid::new_v4().simple());
    let mut admin = PgConnection::connect(admin_url)
        .await
        .expect("connect to WHITEBOARD_TEST_DATABASE_URL");
    admin
        .execute(format!("CREATE DATABASE {name}").as_str())
        .await
        .expect("create test database");
    let url = database_url_with_name(admin_url, &name);
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&url)
        .await
        .expect("connect to test database");
    // The platform provisions schema app with search_path app; migrations run there in order.
    sqlx::raw_sql("CREATE SCHEMA app")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql(&format!("ALTER DATABASE {name} SET search_path = app"))
        .execute(&pool)
        .await
        .unwrap();
    drop(pool);
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&url)
        .await
        .expect("reconnect to test database");
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let mut files: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "sql"))
        .collect();
    files.sort();
    for file in files {
        let sql = std::fs::read_to_string(&file).unwrap();
        let mut tx = pool.begin().await.unwrap();
        sqlx::raw_sql(&sql)
            .execute(&mut *tx)
            .await
            .unwrap_or_else(|error| panic!("{}: {error}", file.display()));
        tx.commit().await.unwrap();
    }
    pool
}

static START: Mutex<()> = Mutex::new(());
static FAKE_CORE: OnceLock<String> = OnceLock::new();

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn wait_listening(port: u16) {
    for _ in 0..400 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("server on {port} did not listen");
}

fn start_fake_core() -> String {
    FAKE_CORE
        .get_or_init(|| {
            let port = free_port();
            std::thread::spawn(move || {
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(async move {
                        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
                            .await
                            .unwrap();
                        axum::serve(listener, axum::Router::new().fallback(fake_core))
                            .await
                            .unwrap();
                    });
            });
            wait_listening(port);
            format!("http://127.0.0.1:{port}")
        })
        .clone()
}

/// Start an app server on a fresh database; returns its URL and a pool.
fn start_server() -> (String, PgPool) {
    let _guard = START.lock().unwrap();
    let admin_url = std::env::var("WHITEBOARD_TEST_DATABASE_URL")
        .expect("set WHITEBOARD_TEST_DATABASE_URL to a disposable PostgreSQL server");
    let core_origin = start_fake_core();
    let port = free_port();
    // SAFETY: servers start one at a time under START; each reads these at start.
    unsafe {
        std::env::set_var("MIY_GATEWAY_SECRET", SECRET);
        std::env::set_var("MIY_INSTALLATION_ID", INSTALLATION);
        std::env::set_var("MIY_RELEASE_ID", RELEASE);
        std::env::set_var("MIY_CORE_ORIGIN", core_origin);
        std::env::set_var("MIY_CORE_ALLOW_HTTP", "1");
        std::env::set_var("MIY_BIND_ADDR", format!("127.0.0.1:{port}"));
    }
    let (ready, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let pool = prepare_database(&admin_url).await;
            ready.send(pool.clone()).unwrap();
            miy_app_sdk::serve(crate::router(pool)).await.unwrap();
        });
    });
    let pool = wait.recv().expect("test server failed to start");
    wait_listening(port);
    (format!("http://127.0.0.1:{port}"), pool)
}

static SERVER: OnceLock<(String, PgPool)> = OnceLock::new();

fn server() -> &'static (String, PgPool) {
    SERVER.get_or_init(start_server)
}

fn base() -> &'static str {
    &server().0
}

/// A pool on the shared test database, for setup and assertions (each test
/// runtime gets its own connections).
async fn db() -> PgPool {
    let options = server().1.connect_options();
    PgPoolOptions::new()
        .max_connections(3)
        .connect_with((*options).clone())
        .await
        .unwrap()
}

fn token(claims: Value) -> String {
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

fn exp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 300
}

fn identity(user: &str) -> String {
    token(
        json!({"sub": user, "admin": false, "installation_id": INSTALLATION,
        "release_id": RELEASE, "iss": "miy", "aud": "miy-app", "exp": exp(),
        "environment": "dev", "delegation_token": format!("proof:{user}")}),
    )
}

fn platform_identity(user: &str) -> String {
    token(
        json!({"sub": user, "admin": false, "installation_id": INSTALLATION,
        "release_id": RELEASE, "iss": "miy", "aud": "miy-app", "exp": exp(),
        "environment": "dev", "platform": true}),
    )
}

/// A platform call with the person's current groups signed in (Core does this
/// for an app approved to read the directory).
fn platform_identity_with_groups(user: &str, groups: Value) -> String {
    token(
        json!({"sub": user, "admin": false, "installation_id": INSTALLATION,
        "release_id": RELEASE, "iss": "miy", "aud": "miy-app", "exp": exp(),
        "environment": "dev", "platform": true, "groups": groups}),
    )
}

async fn send(
    url: &str,
    header: Option<String>,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Response {
    let mut request = Client::new().request(method, format!("{url}/api{path}"));
    if let Some(header) = header {
        request = request.header("x-miy-identity", header);
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    request.send().await.unwrap()
}

async fn call(user: &str, method: Method, path: &str, body: Option<Value>) -> Response {
    send(base(), Some(identity(user)), method, path, body).await
}

async fn get(user: &str, path: &str) -> Response {
    call(user, Method::GET, path, None).await
}

async fn post(user: &str, path: &str, body: Value) -> Response {
    call(user, Method::POST, path, Some(body)).await
}

async fn patch(user: &str, path: &str, body: Value) -> Response {
    call(user, Method::PATCH, path, Some(body)).await
}

async fn delete(user: &str, path: &str) -> Response {
    call(user, Method::DELETE, path, None).await
}

async fn expect(response: Response, status: StatusCode) -> Value {
    let actual = response.status();
    let text = response.text().await.unwrap();
    assert_eq!(actual, status, "{text}");
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

async fn put(user: &str, path: &str, body: Value) -> Response {
    call(user, Method::PUT, path, Some(body)).await
}

fn admin_identity(user: &str) -> String {
    token(
        json!({"sub": user, "admin": true, "installation_id": INSTALLATION,
        "release_id": RELEASE, "iss": "miy", "aud": "miy-app", "exp": exp(),
        "environment": "dev", "delegation_token": format!("proof:{user}")}),
    )
}

/// The whiteboard routes of a space (AI-DO `/api/v1/workspaces/{slug}/whiteboard`);
/// `None` is the app-global space (`/v1/whiteboard`).
fn wb(space: Option<&str>) -> String {
    match space {
        None => "/v1/whiteboard".to_string(),
        Some(space) => format!("/v1/workspaces/{space}/whiteboard"),
    }
}

async fn create_board(user: &str, space: Option<&str>, title: &str, shared: bool) -> Value {
    let mut body = json!({"title": title});
    if shared {
        body["primary_target"] = json!({"app": "whiteboard", "type": "workspace_sidebar",
            "id": space.unwrap_or("global")});
    }
    expect(
        post(user, &format!("{}/items", wb(space)), body).await,
        StatusCode::CREATED,
    )
    .await
}

async fn create_space(user: &str, name: &str) -> String {
    let created = expect(
        post(user, "/v1/spaces", json!({"name": name})).await,
        StatusCode::CREATED,
    )
    .await;
    created["id"].as_str().unwrap().to_string()
}

/// Replace a space's members (SDK ACL sharing contract).
async fn set_members(admin: &str, space: &str, entries: Value) {
    let snapshot = expect(
        get(admin, &format!("/v1/spaces/{space}/sharing")).await,
        StatusCode::OK,
    )
    .await;
    expect(
        put(
            admin,
            &format!("/v1/spaces/{space}/sharing"),
            json!({"expectedRevision": snapshot["revision"], "requestId": uuid::Uuid::new_v4().to_string(),
                "entries": entries}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
}

fn member(id: &str, role: &str) -> Value {
    json!({"subject": {"kind": "user", "id": id}, "role": role})
}

fn ids(response: &Value) -> Vec<String> {
    response["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect()
}

fn id_of(value: &Value) -> String {
    value["id"].as_str().unwrap().to_string()
}

async fn send_bytes(user: &str, method: Method, path: &str, bytes: Option<Vec<u8>>) -> Response {
    let mut request = Client::new()
        .request(method, format!("{}/api{path}", base()))
        .header("x-miy-identity", identity(user));
    if let Some(bytes) = bytes {
        request = request
            .header("content-type", "application/octet-stream")
            .body(bytes);
    }
    request.send().await.unwrap()
}

fn events_for(group_key: &str) -> Vec<Value> {
    events()
        .into_iter()
        .filter(|event| event["body"]["groupKey"] == group_key)
        .collect()
}

// ---------------------------------------------------------------- AI-DO cases

/// AI-DO test_whiteboard_create_update_reload_and_archive (in a fresh space,
/// so the hub views answer exactly), with miy's base_revision.
#[tokio::test]
#[ignore = "needs WHITEBOARD_TEST_DATABASE_URL"]
async fn create_update_reload_and_archive() {
    let owner = user("owner");
    let space = create_space(&owner, "Delivery hub").await;
    let base = wb(Some(&space));
    let created = create_board(&owner, Some(&space), "Launch Map", true).await;
    assert_eq!(created["title"], "Launch Map");
    assert_eq!(
        created["scene"],
        json!({"elements": [], "appState": {}, "files": {}})
    );
    assert_eq!(created["primary_target"]["app"], "whiteboard");
    assert_eq!(created["can_edit"], true);
    assert_eq!(created["revision"], 1);
    assert_eq!(created["is_private"], false);
    assert_eq!(created["created_by_name"], "owner");
    let id = id_of(&created);

    let scene = json!({
        "elements": [{"id": "text-1", "type": "text", "text": "hello"}],
        "appState": {"viewBackgroundColor": "#ffffff"},
        "files": {},
    });
    let updated = expect(
        patch(
            &owner,
            &format!("{base}/items/{id}"),
            json!({"title": "Launch Map v2", "scene": scene, "base_revision": 1}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(updated["title"], "Launch Map v2");
    assert_eq!(updated["scene"], scene);
    assert_eq!(updated["revision"], 2);

    let reloaded = expect(
        get(&owner, &format!("{base}/items/{id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(reloaded["scene"], scene);
    let updated_at = reloaded["updated_at"].clone();

    expect(
        post(&owner, &format!("{base}/items/{id}/view"), json!({})).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let after_view = expect(
        get(&owner, &format!("{base}/items/{id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(after_view["updated_at"], updated_at);

    // Per-viewer editor state is not a change, even from an older revision.
    let noop = expect(
        patch(
            &owner,
            &format!("{base}/items/{id}"),
            json!({"scene": {
                "elements": scene["elements"], "files": {}, "type": "excalidraw", "version": 2,
                "appState": {"viewBackgroundColor": "#ffffff", "name": "Launch Map v2",
                    "selectedElementIds": {"text-1": true}, "scrollX": 12, "zoom": {"value": 1}},
            }, "base_revision": 1}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(noop["updated_at"], updated_at);
    assert_eq!(noop["revision"], 2);

    let recent = expect(
        get(&owner, &format!("{base}/hub?view=recent")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(ids(&recent), vec![id.clone()]);

    let favorite = expect(
        patch(&owner, &format!("{base}/items/{id}/favorite"), json!({})).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(favorite["is_favorite"], true);
    let favorites = expect(
        get(&owner, &format!("{base}/hub?view=favorites")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(ids(&favorites), vec![id.clone()]);

    expect(
        delete(&owner, &format!("{base}/items/{id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let normal = expect(get(&owner, &format!("{base}/hub")).await, StatusCode::OK).await;
    assert!(!ids(&normal).contains(&id));
    let archived = expect(
        get(&owner, &format!("{base}/hub?view=archived")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(ids(&archived), vec![id.clone()]);

    let restored = expect(
        post(&owner, &format!("{base}/items/{id}/restore"), json!({})).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(restored["trashed_at"], Value::Null);
    let normal = expect(get(&owner, &format!("{base}/hub")).await, StatusCode::OK).await;
    assert!(ids(&normal).contains(&id));

    // Permanent deletion only from the trash.
    let refused = expect(
        delete(&owner, &format!("{base}/items/{id}/permanent")).await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(
        refused["error"]["code"],
        "whiteboard_archive_before_permanent_delete"
    );
    expect(
        delete(&owner, &format!("{base}/items/{id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    expect(
        delete(&owner, &format!("{base}/items/{id}/permanent")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let gone = expect(
        get(&owner, &format!("{base}/items/{id}")).await,
        StatusCode::NOT_FOUND,
    )
    .await;
    assert_eq!(gone["error"]["code"], "whiteboard_not_found");
    let archived = expect(
        get(&owner, &format!("{base}/hub?view=archived")).await,
        StatusCode::OK,
    )
    .await;
    assert!(!ids(&archived).contains(&id));

    // A board answers only under its own space (AI-DO workspace filter).
    let other = create_board(&owner, None, "Global board", false).await;
    expect(
        get(&owner, &format!("{base}/items/{}", id_of(&other))).await,
        StatusCode::NOT_FOUND,
    )
    .await;
}

/// AI-DO test_private_whiteboard_is_owner_only.
#[tokio::test]
#[ignore = "needs WHITEBOARD_TEST_DATABASE_URL"]
async fn private_whiteboard_is_owner_only() {
    let owner = user("owner");
    let recipient = user("recipient");
    let created = create_board(&owner, None, "Private Board", false).await;
    assert_eq!(created["is_private"], true);
    let id = id_of(&created);
    let blocked = expect(
        get(&recipient, &format!("/v1/whiteboard/items/{id}")).await,
        StatusCode::NOT_FOUND,
    )
    .await;
    assert_eq!(blocked["error"]["code"], "whiteboard_not_found");
    let hub = expect(get(&recipient, "/v1/whiteboard/hub").await, StatusCode::OK).await;
    assert!(!ids(&hub).contains(&id));

    // The platform administrator flag grants nothing.
    let admin = user("platform admin");
    let response = send(
        base(),
        Some(admin_identity(&admin)),
        Method::GET,
        &format!("/v1/whiteboard/items/{id}"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    // No or forged identity never reaches the board.
    let anonymous = send(
        base(),
        None,
        Method::GET,
        &format!("/v1/whiteboard/items/{id}"),
        None,
    )
    .await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    let forged = encode(
        &Header::default(),
        &json!({"sub": owner, "admin": false, "installation_id": INSTALLATION, "release_id": RELEASE,
            "iss": "miy", "aud": "miy-app", "exp": exp(), "environment": "dev"}),
        &EncodingKey::from_secret(b"not-the-gateway-secret-at-all-0123456789"),
    )
    .unwrap();
    let forged = send(
        base(),
        Some(forged),
        Method::GET,
        &format!("/v1/whiteboard/items/{id}"),
        None,
    )
    .await;
    assert_eq!(forged.status(), StatusCode::UNAUTHORIZED);
}

/// AI-DO test_whiteboard_user_and_link_shares_grant_access (user shares; link
/// shares are not offered in miy), plus the share notification.
#[tokio::test]
#[ignore = "needs WHITEBOARD_TEST_DATABASE_URL"]
async fn user_shares_grant_access_and_notify() {
    let owner = user("owner");
    let recipient = user("Recipient");
    let created = create_board(&owner, None, "Shared Board", false).await;
    let id = id_of(&created);
    let items = format!("/v1/whiteboard/items/{id}");
    let group_key = format!("whiteboard:{id}");

    let shared = expect(
        put(
            &owner,
            &format!("{items}/sharing/users/{recipient}"),
            json!({"access_level": "read"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(shared["users"][0]["user_id"], recipient.as_str());
    assert_eq!(shared["users"][0]["full_name"], "Recipient");
    assert_eq!(shared["users"][0]["access_level"], "read");
    assert_eq!(shared["link_share"], Value::Null);
    let notified = events_for(&group_key);
    assert_eq!(notified.len(), 1, "{notified:?}");
    assert_eq!(notified[0]["op"], "notify");
    assert_eq!(notified[0]["body"]["kind"], "share");
    assert_eq!(notified[0]["body"]["recipients"], json!([recipient]));
    assert_eq!(notified[0]["body"]["path"], format!("/{id}"));
    assert_eq!(notified[0]["body"]["resourceId"], id.as_str());
    assert!(
        notified[0]["body"]["title"]
            .as_str()
            .unwrap()
            .contains("Shared Board")
    );

    let seen = expect(get(&recipient, &items).await, StatusCode::OK).await;
    assert_eq!(seen["can_edit"], false);
    let blocked = expect(
        patch(
            &recipient,
            &items,
            json!({"scene": {"elements": [{"id": "user-read-blocked"}], "appState": {}, "files": {}},
                "base_revision": 1}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_eq!(blocked["error"]["code"], "whiteboard_edit_access_required");
    let no_share = expect(
        get(&recipient, &format!("{items}/sharing")).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_eq!(
        no_share["error"]["code"],
        "whiteboard_share_access_required"
    );

    // A level change does not notify again.
    expect(
        put(
            &owner,
            &format!("{items}/sharing/users/{recipient}"),
            json!({"access_level": "edit"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(events_for(&group_key).len(), 1);
    let saved = expect(
        patch(
            &recipient,
            &items,
            json!({"scene": {"elements": [{"id": "via-share"}], "appState": {}, "files": {}},
                "base_revision": 1}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(saved["scene"]["elements"][0]["id"], "via-share");
    // Editing is not managing: no renaming or trashing.
    let rename = expect(
        patch(&recipient, &items, json!({"title": "Mine"})).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_eq!(rename["error"]["code"], "whiteboard_manage_access_required");
    expect(delete(&recipient, &items).await, StatusCode::FORBIDDEN).await;

    // Opening the board reads its notification.
    expect(
        post(&recipient, &format!("{items}/view"), json!({})).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert!(
        events_for(&group_key)
            .iter()
            .any(|event| event["op"] == "read" && event["by"] == recipient.as_str())
    );

    let own = expect(
        put(
            &owner,
            &format!("{items}/sharing/users/{owner}"),
            json!({"access_level": "read"}),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(
        own["error"]["code"],
        "whiteboard_owner_already_has_full_access"
    );
    let unknown = expect(
        put(
            &owner,
            &format!("{items}/sharing/users/u-nobody"),
            json!({"access_level": "read"}),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    assert_eq!(unknown["error"]["code"], "auth_user_not_found");
    expect(
        put(
            &owner,
            &format!("{items}/sharing/users/{recipient}"),
            json!({"access_level": "admin"}),
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;

    // Unsharing retracts the person's notification and closes the board.
    let after = expect(
        delete(&owner, &format!("{items}/sharing/users/{recipient}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(after["users"], json!([]));
    assert!(
        events_for(&group_key)
            .iter()
            .any(|event| event["op"] == "retract"
                && event["body"]["recipients"] == json!([recipient]))
    );
    expect(get(&recipient, &items).await, StatusCode::NOT_FOUND).await;

    // Shareable people: the directory without the caller.
    let people = expect(
        get(&owner, "/v1/whiteboard/shareable-users?q=Recipient").await,
        StatusCode::OK,
    )
    .await;
    assert!(
        people
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == recipient.as_str())
    );
    assert!(
        !people
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == owner.as_str())
    );
}

/// AI-DO test_whiteboard_reuses_pms_space_acl, on the app's own space: the
/// space target gives members edit and admins manage; others see nothing.
#[tokio::test]
#[ignore = "needs WHITEBOARD_TEST_DATABASE_URL"]
async fn space_members_edit_and_admins_manage() {
    let owner = user("space owner");
    let member_user = user("member");
    let viewer = user("viewer");
    let space_admin = user("space admin");
    let outsider = user("outsider");
    let grouped = user("group member");
    let team = group("team", &[&grouped]);
    let space = create_space(&owner, "Design").await;
    set_members(
        &owner,
        &space,
        json!([member(&owner, "admin"), member(&member_user, "member"), member(&viewer, "viewer"), member(&space_admin, "admin"),
            {"subject": {"kind": "group", "id": team}, "role": "member"}]),
    )
    .await;
    let base = wb(Some(&space));
    let created = create_board(&owner, Some(&space), "ACL Board", true).await;
    let id = id_of(&created);
    let items = format!("{base}/items/{id}");
    let reference = format!("{items}/reference");
    let unscoped_reference = format!("/v1/whiteboard/items/{id}/reference");

    let seen = expect(get(&member_user, &items).await, StatusCode::OK).await;
    assert_eq!(seen["can_edit"], true);
    assert_eq!(seen["can_manage"], false);
    assert_eq!(
        expect(get(&member_user, &reference).await, StatusCode::OK).await,
        json!({"id": id, "title": "ACL Board", "space_id": space})
    );
    assert_eq!(
        expect(get(&member_user, &unscoped_reference).await, StatusCode::OK).await,
        json!({"id": id, "title": "ACL Board", "space_id": space})
    );
    expect(get(&outsider, &reference).await, StatusCode::NOT_FOUND).await;
    expect(
        get(&outsider, &unscoped_reference).await,
        StatusCode::NOT_FOUND,
    )
    .await;
    expect(
        patch(
            &member_user,
            &items,
            json!({"scene": {"elements": [{"id": "allowed"}], "appState": {}, "files": {}}, "base_revision": 1}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    expect(
        patch(&member_user, &items, json!({"title": "x"})).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    let by_group = expect(get(&grouped, &items).await, StatusCode::OK).await;
    assert_eq!(by_group["can_edit"], true);
    let read_only = expect(get(&viewer, &items).await, StatusCode::OK).await;
    assert_eq!(read_only["can_view"], true);
    assert_eq!(read_only["can_edit"], false);
    assert_eq!(read_only["can_manage"], false);
    let viewer_hub = expect(get(&viewer, &format!("{base}/hub")).await, StatusCode::OK).await;
    assert_eq!(ids(&viewer_hub), vec![id.clone()]);
    assert_eq!(viewer_hub["can_publish"], false);
    expect(
        patch(&viewer, &items, json!({"scene": null, "base_revision": 2})).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    expect(
        post(
            &viewer,
            &format!("{base}/items"),
            json!({"title":"Viewer cannot publish","primary_target":{"app":"whiteboard","type":"workspace_sidebar","id":space}}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;

    expect(get(&outsider, &items).await, StatusCode::NOT_FOUND).await;
    let hub = expect(
        get(&outsider, &format!("{base}/hub")).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_eq!(
        hub["error"]["code"],
        "whiteboard_workspace_context_required"
    );
    let member_hub = expect(
        get(&member_user, &format!("{base}/hub")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(ids(&member_hub), vec![id.clone()]);

    let managed = expect(get(&space_admin, &items).await, StatusCode::OK).await;
    assert_eq!(managed["can_manage"], true);
    assert_eq!(managed["can_share"], true);
    expect(
        patch(&space_admin, &items, json!({"title": "Renamed by admin"})).await,
        StatusCode::OK,
    )
    .await;

    // A share target in an app space must be a direct member.
    let refused = expect(
        put(
            &owner,
            &format!("{items}/sharing/users/{outsider}"),
            json!({"access_level": "read"}),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(
        refused["error"]["code"],
        "whiteboard_shared_users_workspace_required"
    );
    let people = expect(
        get(&owner, &format!("{base}/shareable-users")).await,
        StatusCode::OK,
    )
    .await;
    let people: Vec<&str> = people
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert!(people.contains(&member_user.as_str()));
    assert!(!people.contains(&outsider.as_str()));

    let private = create_board(&owner, Some(&space), "Restricted board", false).await;
    let private_items = format!("{base}/items/{}", id_of(&private));
    expect(
        put(
            &owner,
            &format!("{private_items}/sharing/users/{member_user}"),
            json!({"access_level": "edit"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    expect(get(&member_user, &private_items).await, StatusCode::OK).await;
    expect(
        put(
            &member_user,
            &format!("{private_items}/target"),
            json!({"app": "whiteboard", "type": "workspace_sidebar", "id": space}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    expect(
        delete(&member_user, &format!("{items}/target")).await,
        StatusCode::FORBIDDEN,
    )
    .await;

    // Losing membership closes a board even when an explicit edit grant remains.
    set_members(
        &owner,
        &space,
        json!([member(&owner, "admin"), member(&space_admin, "admin")]),
    )
    .await;
    expect(get(&member_user, &items).await, StatusCode::NOT_FOUND).await;
    expect(
        get(&member_user, &private_items).await,
        StatusCode::NOT_FOUND,
    )
    .await;
    expect(get(&member_user, &reference).await, StatusCode::NOT_FOUND).await;
    expect(
        patch(
            &member_user,
            &format!("{private_items}/favorite"),
            json!({}),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;

    // A space with boards is not deleted.
    let busy = expect(
        delete(&owner, &format!("/v1/spaces/{space}")).await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(busy["error"]["code"], "whiteboard_space_not_empty");
    expect(delete(&owner, &items).await, StatusCode::NO_CONTENT).await;
    expect(get(&owner, &reference).await, StatusCode::NOT_FOUND).await;
    set_members(
        &owner,
        &space,
        json!([member(&owner, "admin"), member(&member_user, "member")]),
    )
    .await;
    let archive = expect(
        get(&member_user, &format!("{base}/hub?view=archived")).await,
        StatusCode::OK,
    )
    .await;
    assert!(ids(&archive).is_empty());
    expect(
        delete(&owner, &format!("{items}/permanent")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    expect(delete(&owner, &private_items).await, StatusCode::NO_CONTENT).await;
    expect(
        delete(&owner, &format!("{private_items}/permanent")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    expect(
        delete(&owner, &format!("/v1/spaces/{space}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let gone = expect(
        get(&owner, &format!("{base}/hub")).await,
        StatusCode::NOT_FOUND,
    )
    .await;
    assert_eq!(gone["error"]["code"], "workspace_not_found");
}

/// AI-DO test_whiteboard_pms_space_target_link_filters_and_sort_order, with
/// the space target (PMS targets are refused in miy).
#[tokio::test]
#[ignore = "needs WHITEBOARD_TEST_DATABASE_URL"]
async fn target_filters_sort_order_and_unlink() {
    let owner = user("owner");
    let space = create_space(&owner, "Targets").await;
    let base = wb(Some(&space));
    let target = |sort_order: i32| json!({"app": "whiteboard", "type": "workspace_sidebar", "id": space.clone(), "sort_order": sort_order});
    let first = expect(
        post(
            &owner,
            &format!("{base}/items"),
            json!({"title": "Second", "primary_target": target(20)}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let second = expect(
        post(
            &owner,
            &format!("{base}/items"),
            json!({"title": "First", "primary_target": target(10)}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(
        first["source_deeplink"],
        format!("/{}?space={space}", id_of(&first))
    );
    let listed = expect(
        get(
            &owner,
            &format!(
                "{base}/hub?target_app=whiteboard&target_type=workspace_sidebar&target_id={space}&sort_by=target_sort_order&sort_dir=asc"
            ),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(ids(&listed), vec![id_of(&second), id_of(&first)]);
    let by_title = expect(
        get(&owner, &format!("{base}/hub?sort_by=title&sort_dir=asc")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(ids(&by_title), vec![id_of(&second), id_of(&first)]);
    let searched = expect(
        get(&owner, &format!("{base}/hub?q=seco")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(ids(&searched), vec![id_of(&first)]);

    let updated = expect(
        put(
            &owner,
            &format!("{base}/items/{}/target", id_of(&first)),
            target(0),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(updated["primary_target"]["sort_order"], 0);
    let unlinked = expect(
        delete(&owner, &format!("{base}/items/{}/target", id_of(&first))).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(unlinked["primary_target"], Value::Null);
    assert_eq!(unlinked["is_private"], true);
    assert_eq!(unlinked["location_label"], "Unfiled");

    let pms = json!({"app": "pms", "type": "space", "id": "pms-space"});
    let refused = expect(
        put(
            &owner,
            &format!("{base}/items/{}/target", id_of(&first)),
            pms.clone(),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_eq!(
        refused["error"]["code"],
        "whiteboard_target_edit_access_required"
    );
    expect(
        post(
            &owner,
            &format!("{base}/items"),
            json!({"title": "PMS", "primary_target": pms}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    let invalid = expect(
        get(&owner, &format!("{base}/hub?page_size=500")).await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    assert_eq!(invalid["error"]["code"], "validation_request_invalid");
}

// ---------------------------------------------------------------- miy

/// Single-editor saving: an older revision may not overwrite a newer scene.
#[tokio::test]
#[ignore = "needs WHITEBOARD_TEST_DATABASE_URL"]
async fn older_revision_conflicts_and_keep_resends_on_the_current_one() {
    let owner = user("owner");
    let editor = user("editor");
    let created = create_board(&owner, None, "Concurrent", false).await;
    let id = id_of(&created);
    let items = format!("/v1/whiteboard/items/{id}");
    expect(
        put(
            &owner,
            &format!("{items}/sharing/users/{editor}"),
            json!({"access_level": "edit"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let scene_a = json!({"elements": [{"id": "a", "version": 1}], "appState": {}, "files": {}});
    let scene_b = json!({"elements": [{"id": "b", "version": 1}], "appState": {}, "files": {}});
    let saved = expect(
        patch(
            &editor,
            &items,
            json!({"scene": scene_a, "base_revision": 1}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(saved["revision"], 2);
    let conflict = expect(
        patch(
            &owner,
            &items,
            json!({"scene": scene_b, "base_revision": 1}),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(conflict["error"]["code"], "whiteboard_revision_conflict");
    let current = expect(get(&owner, &items).await, StatusCode::OK).await;
    assert_eq!(current["scene"], scene_a);
    assert_eq!(current["revision"], 2);
    // "Keep mine": the same scene sent on the current revision.
    let kept = expect(
        patch(
            &owner,
            &items,
            json!({"scene": scene_b, "base_revision": 2}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(kept["scene"], scene_b);
    assert_eq!(kept["revision"], 3);
    let missing = expect(
        patch(&owner, &items, json!({"scene": scene_a})).await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    assert_eq!(missing["error"]["code"], "validation_request_invalid");
    // A title change needs no revision and leaves it.
    let renamed = expect(
        patch(&owner, &items, json!({"title": "Renamed"})).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(renamed["revision"], 3);
    assert_eq!(renamed["scene"], scene_b);

    let cleared = expect(
        patch(&owner, &items, json!({"scene": null, "base_revision": 3})).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(cleared["scene"], crate::scene::empty_scene());
    assert_eq!(cleared["revision"], 4);
}

async fn reserve(
    user: &str,
    board: &str,
    request_id: &str,
    name: &str,
    mime: &str,
    size: usize,
) -> Response {
    post(
        user,
        &format!("/v1/whiteboard/items/{board}/files"),
        json!({"request_id": request_id, "file_id": format!("x-{name}"), "name": name, "mime": mime, "size": size}),
    )
    .await
}

fn file_entry(name: &str, row: &str) -> Value {
    json!({"id": format!("x-{name}"), "mimeType": "image/png", "created": 1, "storage": row})
}

/// Images live in the core storage; every call checks the board's access.
#[tokio::test]
#[ignore = "needs WHITEBOARD_TEST_DATABASE_URL"]
async fn images_are_stored_attached_read_and_released() {
    let owner = user("owner");
    let reader = user("reader");
    let stranger = user("stranger");
    let created = create_board(&owner, None, "Pictures", false).await;
    let id = id_of(&created);
    let items = format!("/v1/whiteboard/items/{id}");
    expect(
        put(
            &owner,
            &format!("{items}/sharing/users/{reader}"),
            json!({"access_level": "read"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let bytes = b"\x89PNG-test".to_vec();
    let request = uuid::Uuid::new_v4().to_string();
    let reserved = expect(
        reserve(&owner, &id, &request, "a.png", "image/png", bytes.len()).await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(reserved["status"], "pending");
    assert_eq!(reserved["file_id"], "x-a.png");
    let row = id_of(&reserved);
    // A lost answer is repeated with the same request ID: the same row.
    let again = expect(
        reserve(&owner, &id, &request, "a.png", "image/png", bytes.len()).await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(id_of(&again), row);
    let conflict = expect(
        reserve(&owner, &id, &request, "a.png", "image/png", bytes.len() + 1).await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(conflict["error"]["code"], "media_request_conflict");
    let svg = expect(
        reserve(
            &owner,
            &id,
            &uuid::Uuid::new_v4().to_string(),
            "a.svg",
            "image/svg+xml",
            10,
        )
        .await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    assert_eq!(svg["error"]["code"], "media_unsupported_file_type");
    expect(
        reserve(
            &owner,
            &id,
            &uuid::Uuid::new_v4().to_string(),
            "big.png",
            "image/png",
            11 * 1024 * 1024,
        )
        .await,
        StatusCode::PAYLOAD_TOO_LARGE,
    )
    .await;
    expect(
        reserve(
            &reader,
            &id,
            &uuid::Uuid::new_v4().to_string(),
            "r.png",
            "image/png",
            4,
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;

    let content = format!("/files/whiteboard/items/{id}/files/{row}");
    let uploaded = expect(
        send_bytes(&owner, Method::PUT, &content, Some(bytes.clone())).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(uploaded["status"], "ready");
    // Repeating the transfer reports the stored state.
    let repeated = expect(
        send_bytes(&owner, Method::PUT, &content, Some(bytes.clone())).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(repeated["status"], "ready");
    let replay = expect(
        reserve(&owner, &id, &request, "a.png", "image/png", bytes.len()).await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(replay["status"], "ready");
    expect(
        send_bytes(&reader, Method::PUT, &content, Some(bytes.clone())).await,
        StatusCode::FORBIDDEN,
    )
    .await;

    // Inline bytes and foreign rows are refused in a scene.
    let inline = expect(
        patch(&owner, &items, json!({"scene": {"elements": [], "appState": {},
            "files": {"x-a.png": {"id": "x-a.png", "mimeType": "image/png", "dataURL": "data:image/png;base64,AA"}}},
            "base_revision": 1}))
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    assert_eq!(inline["error"]["code"], "whiteboard_scene_file_invalid");
    let other = create_board(&owner, None, "Other", false).await;
    expect(
        patch(
            &owner,
            &format!("/v1/whiteboard/items/{}", id_of(&other)),
            json!({"scene": {"elements": [],
            "appState": {}, "files": {"x-a.png": file_entry("a.png", &row)}}, "base_revision": 1}),
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;

    // Saving the scene that names the image attaches it once.
    let scene = json!({"elements": [{"id": "img", "type": "image", "fileId": "x-a.png"}], "appState": {},
        "files": {"x-a.png": file_entry("a.png", &row)}});
    expect(
        patch(&owner, &items, json!({"scene": scene, "base_revision": 1})).await,
        StatusCode::OK,
    )
    .await;
    let core_id: String = sqlx::query_scalar("SELECT file_id FROM whiteboard_files WHERE id = $1")
        .bind(&row)
        .fetch_one(&db().await)
        .await
        .unwrap();
    assert_eq!(file_status(&core_id), "attached");

    let read = send_bytes(&reader, Method::GET, &content, None).await;
    assert_eq!(read.status(), StatusCode::OK);
    assert_eq!(read.bytes().await.unwrap().to_vec(), bytes);
    expect(
        send_bytes(&stranger, Method::GET, &content, None).await,
        StatusCode::NOT_FOUND,
    )
    .await;

    // Trash keeps the image; permanent deletion releases it in the core.
    expect(delete(&owner, &items).await, StatusCode::NO_CONTENT).await;
    expect(
        send_bytes(&reader, Method::GET, &content, None).await,
        StatusCode::NOT_FOUND,
    )
    .await;
    assert_eq!(file_status(&core_id), "attached");
    expect(
        delete(&owner, &format!("{items}/permanent")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(file_status(&core_id), "cancelled");
    let status: String = sqlx::query_scalar("SELECT status FROM whiteboard_files WHERE id = $1")
        .bind(&row)
        .fetch_one(&db().await)
        .await
        .unwrap();
    assert_eq!(status, "detached");
}

/// A failed attach leaves the row ready; the next save attaches it with the
/// same request (no deletion, no new reservation).
#[tokio::test]
#[ignore = "needs WHITEBOARD_TEST_DATABASE_URL"]
async fn failed_attach_is_recovered_by_the_next_save() {
    let owner = user("owner");
    let created = create_board(&owner, None, "Retry", false).await;
    let id = id_of(&created);
    let items = format!("/v1/whiteboard/items/{id}");
    let reserved = expect(
        reserve(
            &owner,
            &id,
            &uuid::Uuid::new_v4().to_string(),
            "fail-attach.png",
            "image/png",
            3,
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let row = id_of(&reserved);
    expect(
        send_bytes(
            &owner,
            Method::PUT,
            &format!("/files/whiteboard/items/{id}/files/{row}"),
            Some(vec![1, 2, 3]),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let entry =
        json!({"id": "x-fail-attach.png", "mimeType": "image/png", "created": 1, "storage": row});
    FAIL_ATTACH.store(true, std::sync::atomic::Ordering::SeqCst);
    expect(
        patch(
            &owner,
            &items,
            json!({"scene": {"elements": [{"id": "1"}], "appState": {},
            "files": {"x-fail-attach.png": entry}}, "base_revision": 1}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    FAIL_ATTACH.store(false, std::sync::atomic::Ordering::SeqCst);
    let pool = db().await;
    let (status, core_id): (String, String) =
        sqlx::query_as("SELECT status, file_id FROM whiteboard_files WHERE id = $1")
            .bind(&row)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "ready");
    assert_eq!(file_status(&core_id), "ready");
    expect(
        patch(
            &owner,
            &items,
            json!({"scene": {"elements": [{"id": "2"}], "appState": {},
            "files": {"x-fail-attach.png": entry}}, "base_revision": 2}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(file_status(&core_id), "attached");
}

/// The notification recheck and integrated search answer only boards the
/// person may open now, from the app DB, for platform calls only.
#[tokio::test]
#[ignore = "needs WHITEBOARD_TEST_DATABASE_URL"]
async fn platform_check_and_search_follow_current_access() {
    let owner = user("owner");
    let viewer = user("viewer");
    let grouped = user("grouped");
    let team = group("search team", &[&grouped]);
    let token = format!("zq{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let shared = create_board(&owner, None, &format!("{token} shared"), false).await;
    let private = create_board(&owner, None, &format!("{token} private"), false).await;
    let everyone = create_board(&owner, None, &format!("{token} everyone"), true).await;
    let space = create_space(&owner, "Search space").await;
    set_members(
        &owner,
        &space,
        json!([member(&owner, "admin"), member(&viewer, "member"),
            {"subject": {"kind": "group", "id": team}, "role": "member"}]),
    )
    .await;
    let in_space = create_board(&owner, Some(&space), &format!("{token} space"), true).await;
    let owned_by_viewer =
        create_board(&viewer, Some(&space), &format!("{token} owned"), false).await;
    let pct = create_board(&owner, None, &format!("{token} 100% done"), true).await;
    expect(
        put(
            &owner,
            &format!(
                "/v1/whiteboard/items/{}/sharing/users/{viewer}",
                id_of(&shared)
            ),
            json!({"access_level": "read"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;

    let check = |user: &str, ids: &[&Value]| {
        let items: Vec<Value> = ids
            .iter()
            .map(|board| json!({"groupKey": format!("whiteboard:{}", id_of(board)), "resourceId": id_of(board)}))
            .collect();
        let header = platform_identity(user);
        async move {
            expect(
                send(
                    base(),
                    Some(header),
                    Method::POST,
                    "/platform/notification-check",
                    Some(json!({"items": items})),
                )
                .await,
                StatusCode::OK,
            )
            .await
        }
    };
    let allowed = check(&viewer, &[&shared, &private, &everyone, &in_space]).await;
    let mut expected = vec![
        format!("whiteboard:{}", id_of(&shared)),
        format!("whiteboard:{}", id_of(&everyone)),
        format!("whiteboard:{}", id_of(&in_space)),
    ];
    expected.sort();
    let mut got: Vec<String> = allowed["allowed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    got.sort();
    assert_eq!(got, expected);
    // A resource ID that does not match its group is not answered.
    let mismatched = expect(
        send(base(), Some(platform_identity(&viewer)), Method::POST, "/platform/notification-check",
            Some(json!({"items": [{"groupKey": format!("whiteboard:{}", id_of(&shared)), "resourceId": "other"}]}))).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(mismatched["allowed"], json!([]));
    // A member through a group counts with the groups the platform signed in;
    // without them (directory not approved) only direct grants count.
    let grouped_allowed = check(&grouped, &[&in_space]).await;
    assert_eq!(grouped_allowed["allowed"], json!([]));
    let check_as = |header: String, board: &Value| {
        let items = json!({"items": [{"groupKey": format!("whiteboard:{}", id_of(board)), "resourceId": id_of(board)}]});
        async move {
            send(
                base(),
                Some(header),
                Method::POST,
                "/platform/notification-check",
                Some(items),
            )
            .await
        }
    };
    let signed = expect(
        check_as(
            platform_identity_with_groups(&grouped, json!([team.to_uppercase()])),
            &in_space,
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        signed["allowed"],
        json!([format!("whiteboard:{}", id_of(&in_space))])
    );
    let other_group = uuid::Uuid::new_v4().to_string();
    let unrelated = expect(
        check_as(
            platform_identity_with_groups(&grouped, json!([other_group])),
            &in_space,
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(unrelated["allowed"], json!([]));
    // Private boards stay private to group members.
    let private_for_group = expect(
        check_as(
            platform_identity_with_groups(&grouped, json!([team])),
            &private,
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(private_for_group["allowed"], json!([]));
    expect(
        check_as(
            platform_identity_with_groups(&grouped, json!(["../not-a-group"])),
            &in_space,
        )
        .await,
        StatusCode::SERVICE_UNAVAILABLE,
    )
    .await;
    // The SDK ACL reads the same signed groups on a platform call's board read.
    let board_read = send(
        base(),
        Some(platform_identity_with_groups(&grouped, json!([team]))),
        Method::GET,
        &format!(
            "/v1/workspaces/{space}/whiteboard/items/{}",
            id_of(&in_space)
        ),
        None,
    )
    .await;
    assert_eq!(board_read.status(), StatusCode::OK);
    let board_hidden = send(
        base(),
        Some(platform_identity(&grouped)),
        Method::GET,
        &format!(
            "/v1/workspaces/{space}/whiteboard/items/{}",
            id_of(&in_space)
        ),
        None,
    )
    .await;
    assert_eq!(board_hidden.status(), StatusCode::NOT_FOUND);
    // Not a platform call.
    expect(
        call(
            &viewer,
            Method::POST,
            "/platform/notification-check",
            Some(json!({"items": []})),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;

    let search = |user: &str, query: &str| {
        let header = platform_identity(user);
        let query = query.to_string();
        async move {
            expect(
                send(base(), Some(header), Method::POST, "/v1/search",
                    Some(json!({"query": query, "limit": 10, "timeZone": "Asia/Seoul", "locale": "ko-KR"}))).await,
                StatusCode::OK,
            )
            .await
        }
    };
    let found = search(&viewer, &token).await;
    let titles: Vec<&str> = found["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap())
        .collect();
    assert!(titles.contains(&format!("{token} shared").as_str()));
    assert!(titles.contains(&format!("{token} everyone").as_str()));
    assert!(titles.contains(&format!("{token} space").as_str()));
    assert!(!titles.iter().any(|t| t.contains("private")));
    let space_hit = found["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["title"] == format!("{token} space"))
        .unwrap();
    assert_eq!(
        space_hit["path"],
        format!("/{}?space={space}", id_of(&in_space))
    );
    assert_eq!(space_hit["snippet"], "Search space");
    assert_eq!(space_hit["kind"], "화이트보드");
    let percent = search(&viewer, &format!("{token} 100%")).await;
    assert_eq!(percent["items"].as_array().unwrap().len(), 1);
    assert_eq!(percent["items"][0]["path"], format!("/{}", id_of(&pct)));
    // Search finds a group member's space board only with the signed groups.
    let grouped_search = |header: String, query: String| async move {
        expect(
            send(
                base(),
                Some(header),
                Method::POST,
                "/v1/search",
                Some(json!({"query": query, "limit": 10, "timeZone": "Asia/Seoul", "locale": "ko-KR"})),
            )
            .await,
            StatusCode::OK,
        )
        .await
    };
    let with_groups = grouped_search(
        platform_identity_with_groups(&grouped, json!([team])),
        token.clone(),
    )
    .await;
    let group_titles: Vec<&str> = with_groups["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap())
        .collect();
    assert!(group_titles.contains(&format!("{token} space").as_str()));
    assert!(group_titles.contains(&format!("{token} everyone").as_str()));
    assert!(
        !group_titles
            .iter()
            .any(|t| t.contains("private") || t.contains("shared"))
    );
    let without_groups = grouped_search(platform_identity(&grouped), token.clone()).await;
    assert!(
        !without_groups["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["title"] == format!("{token} space"))
    );
    let wildcard = search(&viewer, "%").await;
    assert!(
        wildcard["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["title"].as_str().unwrap().contains('%'))
    );
    expect(
        call(
            &viewer,
            Method::POST,
            "/v1/search",
            Some(json!({"query": token})),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;

    expect(
        put(
            &owner,
            &format!(
                "{}/items/{}/sharing/users/{viewer}",
                wb(Some(&space)),
                id_of(&in_space)
            ),
            json!({"access_level": "read"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    set_members(&owner, &space, json!([member(&owner, "admin")])).await;
    assert_eq!(
        check(&viewer, &[&in_space, &owned_by_viewer]).await["allowed"],
        json!([])
    );
    assert_eq!(
        search(&viewer, &format!("{token} space")).await["items"],
        json!([])
    );
    assert_eq!(
        search(&viewer, &format!("{token} owned")).await["items"],
        json!([])
    );
    expect(
        get(
            &viewer,
            &format!("/v1/whiteboard/items/{}/reference", id_of(&owned_by_viewer)),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;

    // Trash closes it for the check and the search; the group is retracted.
    expect(
        delete(&owner, &format!("/v1/whiteboard/items/{}", id_of(&shared))).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert!(
        events_for(&format!("whiteboard:{}", id_of(&shared)))
            .iter()
            .any(|e| e["op"] == "retract" && e["body"]["recipients"].is_null())
    );
    let after = check(&viewer, &[&shared]).await;
    assert_eq!(after["allowed"], json!([]));
    let found = search(&viewer, &format!("{token} shared")).await;
    assert_eq!(found["items"], json!([]));
}

/// AI-DO hub views: mine, recent and favorites per person.
#[tokio::test]
#[ignore = "needs WHITEBOARD_TEST_DATABASE_URL"]
async fn hub_views_are_per_person() {
    let owner = user("owner");
    let other = user("other");
    let space = create_space(&owner, "Views").await;
    set_members(
        &owner,
        &space,
        json!([member(&owner, "admin"), member(&other, "member")]),
    )
    .await;
    let base = wb(Some(&space));
    let mine = create_board(&owner, Some(&space), "Mine", true).await;
    let theirs = create_board(&other, Some(&space), "Theirs", true).await;
    let private = create_board(&other, Some(&space), "Their private", false).await;
    let all = expect(get(&owner, &format!("{base}/hub")).await, StatusCode::OK).await;
    let mut all_ids = ids(&all);
    all_ids.sort();
    let mut expected = vec![id_of(&mine), id_of(&theirs)];
    expected.sort();
    assert_eq!(all_ids, expected);
    assert_eq!(all["total"], 2);
    let my = expect(
        get(&owner, &format!("{base}/hub?view=mine")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(ids(&my), vec![id_of(&mine)]);
    let category = expect(
        get(&owner, &format!("{base}/hub?category=my")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(ids(&category), vec![id_of(&mine)]);
    expect(
        patch(
            &owner,
            &format!("{base}/items/{}/favorite", id_of(&theirs)),
            json!({}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let favorites = expect(
        get(&owner, &format!("{base}/hub?view=favorites")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(ids(&favorites), vec![id_of(&theirs)]);
    let other_favorites = expect(
        get(&other, &format!("{base}/hub?view=favorites")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(ids(&other_favorites), Vec::<String>::new());
    let theirs_all = expect(
        get(
            &other,
            &format!("{base}/hub?page_size=1&sort_by=title&sort_dir=asc"),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(theirs_all["total"], 3);
    assert_eq!(ids(&theirs_all), vec![id_of(&mine)]);
    let _ = private;
}

/// A detach the core did not confirm stays recorded (`detaching`) and the next
/// permanent deletion finishes it; the core keeps the quota until then.
#[tokio::test]
#[ignore = "needs WHITEBOARD_TEST_DATABASE_URL"]
async fn unconfirmed_detach_is_finished_later() {
    let owner = user("owner");
    let first = create_board(&owner, None, "Detach later", false).await;
    let id = id_of(&first);
    let items = format!("/v1/whiteboard/items/{id}");
    let reserved = expect(
        reserve(
            &owner,
            &id,
            &uuid::Uuid::new_v4().to_string(),
            "fail-detach.png",
            "image/png",
            3,
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let row = id_of(&reserved);
    expect(
        send_bytes(
            &owner,
            Method::PUT,
            &format!("/files/whiteboard/items/{id}/files/{row}"),
            Some(vec![7, 8, 9]),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let entry =
        json!({"id": "x-fail-detach.png", "mimeType": "image/png", "created": 1, "storage": row});
    expect(
        patch(
            &owner,
            &items,
            json!({"scene": {"elements": [], "appState": {}, "files": {"x-fail-detach.png": entry}},
                "base_revision": 1}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let pool = db().await;
    let core_id: String = sqlx::query_scalar("SELECT file_id FROM whiteboard_files WHERE id = $1")
        .bind(&row)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(file_status(&core_id), "attached");
    expect(delete(&owner, &items).await, StatusCode::NO_CONTENT).await;
    FAIL_DETACH.store(true, std::sync::atomic::Ordering::SeqCst);
    expect(
        delete(&owner, &format!("{items}/permanent")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let status: String = sqlx::query_scalar("SELECT status FROM whiteboard_files WHERE id = $1")
        .bind(&row)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "detaching");
    assert_eq!(file_status(&core_id), "attached");
    FAIL_DETACH.store(false, std::sync::atomic::Ordering::SeqCst);

    let second = create_board(&owner, None, "Trigger", false).await;
    let second_items = format!("/v1/whiteboard/items/{}", id_of(&second));
    expect(delete(&owner, &second_items).await, StatusCode::NO_CONTENT).await;
    expect(
        delete(&owner, &format!("{second_items}/permanent")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let status: String = sqlx::query_scalar("SELECT status FROM whiteboard_files WHERE id = $1")
        .bind(&row)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "detached");
    assert_eq!(file_status(&core_id), "cancelled");
}

async fn shared_editors() -> (String, String, String, String, String) {
    let owner = user("Room owner");
    let editor = user("Room editor");
    let space = create_space(&owner, "Shared drawing").await;
    set_members(
        &owner,
        &space,
        json!([member(&owner, "admin"), member(&editor, "member")]),
    )
    .await;
    let board = create_board(&owner, Some(&space), "Live board", true).await;
    let key = id_of(&board);
    let item = key.clone();
    (owner, editor, space, item, key)
}

#[tokio::test]
#[ignore = "requires a disposable PostgreSQL database"]
async fn editing_sessions_share_initial_state_and_recheck_resource_access() {
    let (owner, editor, space, _, key) = shared_editors().await;
    let path = format!("/v1/collaboration/{key}/session");
    let (a, b) = tokio::join!(
        post(&owner, &path, json!({})),
        post(&editor, &path, json!({}))
    );
    let a = expect(a, StatusCode::OK).await;
    let b = expect(b, StatusCode::OK).await;
    assert_eq!(a["roomId"], key);
    assert_eq!(a["generation"], b["generation"]);
    assert_ne!(a["sessionId"], b["sessionId"]);
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM collab_rooms WHERE resource_id=$1 AND octet_length(state)>0",
    )
    .bind(&key)
    .fetch_one(&db().await)
    .await
    .unwrap();
    assert_eq!(rows, 1);
    set_members(
        &owner,
        &space,
        json!([member(&owner, "admin"), member(&editor, "viewer")]),
    )
    .await;
    assert_eq!(
        post(&editor, &path, json!({"session":b["sessionId"]}))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let pool = db().await;
    let (lease, generation): (uuid::Uuid, uuid::Uuid) =
        sqlx::query_as("SELECT owner,generation FROM collab_rooms WHERE resource_id=$1")
            .bind(&key)
            .fetch_one(&pool)
            .await
            .unwrap();
    expect(patch(&owner,&format!("{}/items/{key}",wb(Some(&space))),json!({
         "scene":{"elements":[],"files":{},"appState":{"viewBackgroundColor":"#eeeeee"}},"base_revision":a["revision"]})).await,StatusCode::OK).await;
    let mut late = pool.begin().await.unwrap();
    let error =
        miy_app_collab::db::claim_save(&mut late, &key, lease, generation, Some(&[0, 0]), false)
            .await
            .unwrap_err();
    assert_eq!(error.to_string(), "generation_changed");
}
