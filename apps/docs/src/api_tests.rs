//! HTTP tests through the real SDK gateway-identity check, PostgreSQL and a
//! fake core (directory, notifications, files) that the SDK calls with the
//! signed identity's delegation proof.
//!
//! Ports of AI-DO apps/api/tests/test_docs_hub.py, test_docs_page_mutations.py,
//! test_docs_page_attachments.py, test_docs_content_formats.py and
//! test_docs_cross_space_prefs.py (the cases that apply without link shares,
//! HTML pages, PMS and meetings), plus miy additions (revisions, spaces,
//! docs managers, notifications, platform endpoints, media).
//!
//! They need a disposable PostgreSQL server, so they are ignored by default:
//!   DOCS_TEST_DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:PORT/postgres \
//!     cargo test -- --include-ignored --test-threads=1
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

const SECRET: &str = "docs-api-test-gateway-secret-01234567890";
const INSTALLATION: &str = "00000000-0000-4000-8000-0000000000d1";
const RELEASE: &str = "00000000-0000-4000-8000-0000000000d2";

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
    let name = format!("docs_test_{}", uuid::Uuid::new_v4().simple());
    let mut admin = PgConnection::connect(admin_url)
        .await
        .expect("connect to DOCS_TEST_DATABASE_URL");
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
    let admin_url = std::env::var("DOCS_TEST_DATABASE_URL")
        .expect("set DOCS_TEST_DATABASE_URL to a disposable PostgreSQL server");
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

/// A platform call into an app approved to read the directory: the core
/// signs the person's current groups into it.
fn platform_identity_with_groups(user: &str) -> String {
    let groups = core().memberships.get(user).cloned().unwrap_or_default();
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

async fn platform(user: &str, path: &str, body: Value) -> Response {
    send(
        base(),
        Some(platform_identity(user)),
        Method::POST,
        path,
        Some(body),
    )
    .await
}

async fn file_call(user: &str, method: Method, path: &str, bytes: Option<Vec<u8>>) -> Response {
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

async fn make_manager(name: &str) -> String {
    let id = user(name);
    sqlx::query(
        "INSERT INTO docs_managers (user_id, granted_by, created_at) VALUES ($1, $1, now())",
    )
    .bind(&id)
    .execute(&db().await)
    .await
    .unwrap();
    id
}

async fn create_doc(owner: &str, title: &str, space: Option<&str>, in_space: bool) -> Value {
    let space_id = space.unwrap_or("global");
    let mut body = json!({"title": title});
    if in_space {
        body["primary_target"] =
            json!({"app": "docs", "type": "workspace_sidebar", "id": space_id});
    }
    let path = match space {
        Some(space) => format!("/v1/docs/items?space={space}"),
        None => "/v1/docs/items".to_string(),
    };
    expect(post(owner, &path, body).await, StatusCode::CREATED).await
}

async fn first_page(user: &str, doc_id: &str) -> Value {
    let pages = expect(
        get(user, &format!("/v1/docs/items/{doc_id}/pages")).await,
        StatusCode::OK,
    )
    .await;
    pages["items"][0].clone()
}

fn hub_ids(hub: &Value) -> Vec<String> {
    hub["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect()
}

async fn hub(user: &str, query: &str) -> Value {
    expect(
        get(user, &format!("/v1/docs/hub{query}")).await,
        StatusCode::OK,
    )
    .await
}

async fn create_space(manager: &str, name: &str) -> String {
    let space = expect(
        post(manager, "/v1/spaces", json!({"name": name})).await,
        StatusCode::CREATED,
    )
    .await;
    space["id"].as_str().unwrap().to_string()
}

/// Replace a space's grants (the creator stays its admin).
async fn set_space_members(admin: &str, space: &str, members: &[(&str, &str, &str)]) {
    let snapshot = expect(
        get(admin, &format!("/v1/spaces/{space}/sharing")).await,
        StatusCode::OK,
    )
    .await;
    let mut entries = vec![json!({"subject": {"kind": "user", "id": admin}, "role": "admin"})];
    for (kind, id, role) in members {
        entries.push(json!({"subject": {"kind": kind, "id": id}, "role": role}));
    }
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

fn events_for(group_key: &str) -> Vec<Value> {
    events()
        .into_iter()
        .filter(|event| event["body"]["groupKey"] == group_key)
        .collect()
}

// ---------------------------------------------------------------- identity

#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn identity_is_required_and_forged_identities_are_refused() {
    let none = send(base(), None, Method::GET, "/v1/docs/hub", None).await;
    assert_eq!(none.status(), StatusCode::UNAUTHORIZED);
    let forged = encode(
        &Header::default(),
        &json!({"sub": "x", "admin": false, "installation_id": INSTALLATION, "release_id": RELEASE,
            "iss": "miy", "aud": "miy-app", "exp": exp(), "environment": "dev"}),
        &EncodingKey::from_secret(b"another-secret-another-secret-12345"),
    )
    .unwrap();
    let response = send(base(), Some(forged), Method::GET, "/v1/docs/hub", None).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn the_platform_admin_flag_grants_nothing() {
    let owner = user("Owner");
    let doc = create_doc(&owner, "Private plan", None, false).await;
    let admin = user("Platform admin");
    let token = token(
        json!({"sub": admin, "admin": true, "installation_id": INSTALLATION,
        "release_id": RELEASE, "iss": "miy", "aud": "miy-app", "exp": exp(),
        "environment": "dev", "delegation_token": format!("proof:{admin}")}),
    );
    let response = send(
        base(),
        Some(token),
        Method::GET,
        &format!("/v1/docs/items/{}", doc["id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------- hub and sharing

/// AI-DO test_docs_native_docs_and_direct_user_share_grant_docs_access.
#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn native_docs_and_direct_user_share_grant_docs_access() {
    let owner = user("Docs Owner");
    let doc = create_doc(&owner, "Private Plan", None, false).await;
    assert_eq!(doc["source_type"], "native_doc");
    assert_eq!(doc["can_edit"], true);
    assert_eq!(doc["can_share"], true);
    assert_eq!(doc["is_private"], true);
    assert_eq!(doc["space_id"], "global");
    let doc_id = doc["id"].as_str().unwrap();
    let page = first_page(&owner, doc_id).await;
    assert_eq!(page["title"], "Private Plan");
    assert_eq!(page["revision"], 1);

    let shared = user("Docs Shared User");
    assert!(!hub_ids(&hub(&shared, "").await).contains(&doc_id.to_string()));
    let share = expect(
        put(
            &owner,
            &format!("/v1/docs/items/{doc_id}/sharing/users/{shared}"),
            json!({"access_level": "read"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(share["users"][0]["access_level"], "read");
    assert_eq!(share["users"][0]["full_name"], "Docs Shared User");

    let shared_hub = hub(&shared, "").await;
    let item = shared_hub["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == doc_id)
        .expect("shared doc listed")
        .clone();
    assert_eq!(item["can_edit"], false);
    assert_eq!(item["can_share"], false);
    assert_eq!(item["is_private"], false);
    assert_eq!(item["sharing_summary"]["user_share_count"], 1);

    let blocked = patch(
        &shared,
        &format!("/v1/docs/pages/{}", page["id"].as_str().unwrap()),
        json!({"content_blocks": [{"type": "paragraph", "content": "blocked"}]}),
    )
    .await;
    assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
    // Sharing needs share access; the owner cannot be added; unknown people are refused.
    let other = user("Other");
    let refused = put(
        &shared,
        &format!("/v1/docs/items/{doc_id}/sharing/users/{other}"),
        json!({"access_level": "read"}),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    let own = expect(
        put(
            &owner,
            &format!("/v1/docs/items/{doc_id}/sharing/users/{owner}"),
            json!({"access_level": "read"}),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(own["error"]["code"], "docs_owner_already_has_full_access");
    let unknown = expect(
        put(
            &owner,
            &format!("/v1/docs/items/{doc_id}/sharing/users/u-missing"),
            json!({"access_level": "edit"}),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    assert_eq!(unknown["error"]["code"], "auth_user_not_found");
    // An edit share lets the person edit but not manage.
    expect(
        put(
            &owner,
            &format!("/v1/docs/items/{doc_id}/sharing/users/{shared}"),
            json!({"access_level": "edit"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let item = expect(
        get(&shared, &format!("/v1/docs/items/{doc_id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(item["can_edit"], true);
    assert_eq!(item["can_manage"], false);
    let rename = patch(
        &shared,
        &format!("/v1/docs/items/{doc_id}"),
        json!({"title": "x"}),
    )
    .await;
    assert_eq!(rename.status(), StatusCode::FORBIDDEN);
    // Removing the share removes access.
    expect(
        delete(
            &owner,
            &format!("/v1/docs/items/{doc_id}/sharing/users/{shared}"),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let gone = get(&shared, &format!("/v1/docs/items/{doc_id}")).await;
    assert_eq!(gone.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn share_notifications_are_published_retracted_and_read() {
    let owner = user("Notifier");
    let reader = user("Reader");
    let doc = create_doc(&owner, "Quarterly report", None, false).await;
    let doc_id = doc["id"].as_str().unwrap();
    let group = format!("doc:{doc_id}");
    expect(
        put(
            &owner,
            &format!("/v1/docs/items/{doc_id}/sharing/users/{reader}"),
            json!({"access_level": "read"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let published = events_for(&group);
    assert_eq!(published.len(), 1);
    let body = &published[0]["body"];
    assert_eq!(published[0]["op"], "notify");
    assert_eq!(body["kind"], "share");
    assert_eq!(body["recipients"], json!([reader]));
    assert_eq!(body["path"], format!("/{doc_id}"));
    assert_eq!(body["resourceId"], doc_id);
    assert_eq!(
        body["title"],
        "Notifier님이 \"Quarterly report\" 문서를 공유했습니다"
    );
    // Changing the level does not notify again.
    expect(
        put(
            &owner,
            &format!("/v1/docs/items/{doc_id}/sharing/users/{reader}"),
            json!({"access_level": "edit"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(events_for(&group).len(), 1);
    // Opening the doc reads it.
    expect(
        post(
            &reader,
            &format!("/v1/docs/items/{doc_id}/view"),
            json!({"page_id": null}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert!(
        events_for(&group)
            .iter()
            .any(|event| event["op"] == "read" && event["by"] == reader)
    );
    // Removing the share retracts it for that person.
    expect(
        delete(
            &owner,
            &format!("/v1/docs/items/{doc_id}/sharing/users/{reader}"),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert!(events_for(&group).iter().any(|event| event["op"] == "retract" && event["body"]["recipients"] == json!([reader])));
    // Trashing the doc retracts it for everyone.
    expect(
        put(
            &owner,
            &format!("/v1/docs/items/{doc_id}/sharing/users/{reader}"),
            json!({"access_level": "read"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    expect(
        delete(&owner, &format!("/v1/docs/items/{doc_id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert!(
        events_for(&group)
            .iter()
            .any(|event| event["op"] == "retract"
                && event["body"].get("recipients").is_none_or(Value::is_null))
    );
}

#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn global_space_docs_follow_member_and_manager_roles() {
    let owner = user("Space Owner");
    let doc = create_doc(&owner, "Handbook", None, true).await;
    let doc_id = doc["id"].as_str().unwrap();
    assert_eq!(doc["is_private"], false);
    assert_eq!(doc["location_label"], "Workspace Docs");
    assert_eq!(doc["primary_target"]["type"], "workspace_sidebar");
    // Everyone who may use the app is a member of the global space: edit, not manage.
    let member = user("Member");
    let item = expect(
        get(&member, &format!("/v1/docs/items/{doc_id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(item["can_edit"], true);
    assert_eq!(item["can_manage"], false);
    assert!(hub_ids(&hub(&member, "?view=shared").await).contains(&doc_id.to_string()));
    let delete_refused = delete(&member, &format!("/v1/docs/items/{doc_id}")).await;
    assert_eq!(delete_refused.status(), StatusCode::FORBIDDEN);
    // A docs manager is the global space's admin.
    let manager = make_manager("Manager").await;
    let item = expect(
        get(&manager, &format!("/v1/docs/items/{doc_id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(item["can_manage"], true);
    assert_eq!(item["can_share"], true);
    // Removing the manager takes the rights away on the next request.
    let other_manager = make_manager("Second manager").await;
    expect(
        delete(&other_manager, &format!("/v1/docs/managers/{manager}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let item = expect(
        get(&manager, &format!("/v1/docs/items/{doc_id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(item["can_manage"], false);
    let refused = patch(
        &manager,
        &format!("/v1/docs/items/{doc_id}"),
        json!({"title": "x"}),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    // Moving the doc to private hides it from members (AI-DO allowed editors to move).
    expect(
        delete(&owner, &format!("/v1/docs/items/{doc_id}/target")).await,
        StatusCode::OK,
    )
    .await;
    let hidden = get(&member, &format!("/v1/docs/items/{doc_id}")).await;
    assert_eq!(hidden.status(), StatusCode::NOT_FOUND);
    // Only the doc's own space sidebar is a valid target.
    let wrong = expect(
        put(
            &owner,
            &format!("/v1/docs/items/{doc_id}/target"),
            json!({"app": "pms", "type": "space", "id": "x"}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_eq!(wrong["error"]["code"], "docs_target_edit_access_required");
}

#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn named_spaces_use_viewer_member_and_admin_roles() {
    let manager = make_manager("Space creator").await;
    let not_manager = user("Not a manager");
    let refused = post(&not_manager, "/v1/spaces", json!({"name": "Nope"})).await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    let space = create_space(&manager, "Engineering").await;
    let viewer = user("Viewer");
    let member = user("Member");
    let in_group = user("Group member");
    let group = group("Writers", &[&in_group]);
    let outsider = user("Outsider");
    set_space_members(
        &manager,
        &space,
        &[
            ("user", &viewer, "viewer"),
            ("user", &member, "member"),
            ("group", &group, "member"),
        ],
    )
    .await;

    let doc = create_doc(&member, "Design", Some(&space), true).await;
    let doc_id = doc["id"].as_str().unwrap();
    assert_eq!(doc["space_id"], space);
    let as_viewer = expect(
        get(&viewer, &format!("/v1/docs/items/{doc_id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(as_viewer["can_edit"], false);
    let as_group = expect(
        get(&in_group, &format!("/v1/docs/items/{doc_id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(as_group["can_edit"], true);
    let as_admin = expect(
        get(&manager, &format!("/v1/docs/items/{doc_id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(as_admin["can_manage"], true);
    assert_eq!(
        get(&outsider, &format!("/v1/docs/items/{doc_id}"))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let outsider_hub = expect(
        get(&outsider, &format!("/v1/docs/hub?space={space}")).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_eq!(
        outsider_hub["error"]["code"],
        "docs_requests_workspace_context_required"
    );
    assert!(hub_ids(&hub(&viewer, &format!("?space={space}")).await).contains(&doc_id.to_string()));
    // The target must name the space the doc was created in.
    let mismatch = post(&member, &format!("/v1/docs/items?space={space}"),
        json!({"title": "x", "primary_target": {"app": "docs", "type": "workspace_sidebar", "id": "global"}})).await;
    assert_eq!(mismatch.status(), StatusCode::FORBIDDEN);
    // Spaces list shows roles.
    let spaces = expect(get(&viewer, "/v1/spaces").await, StatusCode::OK).await;
    let listed: Vec<(String, String)> = spaces["spaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["id"].as_str().unwrap().to_string(),
                s["role"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(listed[0], ("global".to_string(), "member".to_string()));
    assert!(listed.contains(&(space.clone(), "viewer".to_string())));
    assert!(
        !expect(get(&outsider, "/v1/spaces").await, StatusCode::OK).await["spaces"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == space)
    );
}

#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn deleting_a_space_needs_it_empty_and_reclaims_its_attachments() {
    let manager = make_manager("Space admin").await;
    let space = create_space(&manager, "Temporary").await;
    let doc = create_doc(&manager, "Old notes", Some(&space), true).await;
    let doc_id = doc["id"].as_str().unwrap().to_string();
    let page = first_page(&manager, &doc_id).await;
    let page_id = page["id"].as_str().unwrap().to_string();
    let attachment = upload_attachment(&manager, &page_id, "notes.txt", b"hello").await;
    let busy = expect(
        delete(&manager, &format!("/v1/spaces/{space}")).await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(busy["error"]["code"], "docs_space_not_empty");
    // Trashing keeps the attachment (the trash can restore it); deleting the
    // space empties its trash and releases the file.
    expect(
        delete(&manager, &format!("/v1/docs/items/{doc_id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let file_id: String =
        sqlx::query_scalar("SELECT file_id FROM docs_page_attachments WHERE id = $1")
            .bind(attachment["id"].as_str().unwrap())
            .fetch_one(&db().await)
            .await
            .unwrap();
    assert_eq!(file_status(&file_id), "attached");
    expect(
        delete(&manager, &format!("/v1/spaces/{space}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(file_status(&file_id), "cancelled");
    let status: String =
        sqlx::query_scalar("SELECT status FROM docs_page_attachments WHERE id = $1")
            .bind(attachment["id"].as_str().unwrap())
            .fetch_one(&db().await)
            .await
            .unwrap();
    assert_eq!(status, "detached", "the release record outlives the page");
    let left: i64 =
        sqlx::query_scalar("SELECT count(*) FROM docs_native_docs WHERE workspace_id = $1")
            .bind(&space)
            .fetch_one(&db().await)
            .await
            .unwrap();
    assert_eq!(left, 0);
    let protected = expect(
        delete(&manager, "/v1/spaces/global").await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    assert_eq!(protected["error"]["code"], "docs_global_space_protected");
}

#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn space_deletion_releases_files_of_trashed_docs_left_unreleased() {
    // A file whose release the core never confirmed is reclaimed with the space.
    let manager = make_manager("Reclaimer").await;
    let space = create_space(&manager, "Leftovers").await;
    let doc = create_doc(&manager, "Draft", Some(&space), true).await;
    let doc_id = doc["id"].as_str().unwrap().to_string();
    let page_id = first_page(&manager, &doc_id).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let attachment = upload_attachment(&manager, &page_id, "keep.bin", b"bytes").await;
    // Simulate an old trash that left the attachment attached.
    let pool = db().await;
    sqlx::query("UPDATE docs_native_docs SET trashed_at = now() WHERE id = $1")
        .bind(&doc_id)
        .execute(&pool)
        .await
        .unwrap();
    let file_id = attachment_file(&attachment).await;
    assert_eq!(file_status(&file_id), "attached");
    expect(
        delete(&manager, &format!("/v1/spaces/{space}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(file_status(&file_id), "cancelled");
}

async fn attachment_file(attachment: &Value) -> String {
    sqlx::query_scalar("SELECT file_id FROM docs_page_attachments WHERE id = $1")
        .bind(attachment["id"].as_str().unwrap())
        .fetch_one(&db().await)
        .await
        .unwrap()
}

/// AI-DO test_native_doc_page_patch_rejects_cycle.
#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn native_doc_page_patch_rejects_cycle() {
    let owner = user("Cycle Owner");
    let doc = create_doc(&owner, "Cycle Plan", None, false).await;
    let doc_id = doc["id"].as_str().unwrap();
    let parent = expect(
        post(
            &owner,
            &format!("/v1/docs/items/{doc_id}/pages"),
            json!({"title": "Parent"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let child = expect(
        post(
            &owner,
            &format!("/v1/docs/items/{doc_id}/pages"),
            json!({"title": "Child", "parent_id": parent["id"]}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let reject = expect(
        patch(
            &owner,
            &format!("/v1/docs/pages/{}", parent["id"].as_str().unwrap()),
            json!({"parent_id": child["id"]}),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(reject["error"]["code"], "docs_page_parent_cycle");
    let missing = expect(
        post(
            &owner,
            &format!("/v1/docs/items/{doc_id}/pages"),
            json!({"title": "Invalid child", "parent_id": "missing-parent"}),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    assert_eq!(missing["error"]["code"], "docs_parent_page_not_found");
    assert_eq!(missing["error"]["message"], "Parent page not found.");
    // AI-DO test_create_native_page_module_rejects_doc_prefixed_parent_id
    let prefixed = post(&owner, &format!("/v1/docs/items/{doc_id}/pages"),
        json!({"title": "x", "parent_id": format!("native_doc__{}", parent["id"].as_str().unwrap())})).await;
    assert_eq!(prefixed.status(), StatusCode::NOT_FOUND);
}

/// AI-DO test_update_native_page_module_updates_metadata and
/// test_delete_native_page_module_trashes_subtree, test_docs_content_formats.
#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn page_mutations_update_metadata_and_trash_subtrees() {
    let owner = user("Pages Owner");
    let doc = create_doc(&owner, "Tree", None, false).await;
    let doc_id = doc["id"].as_str().unwrap();
    let root = first_page(&owner, doc_id).await;
    let second = expect(
        post(
            &owner,
            &format!("/v1/docs/items/{doc_id}/pages"),
            json!({"title": "Second"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(second["sort_order"], 1);
    let nested = expect(
        post(
            &owner,
            &format!("/v1/docs/items/{doc_id}/pages"),
            json!({"title": "Nested", "parent_id": second["id"]}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let updated = expect(
        patch(
            &owner,
            &format!("/v1/docs/pages/{}", second["id"].as_str().unwrap()),
            json!({"title": "  Renamed  ", "sort_order": 5000}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(updated["title"], "Renamed");
    assert_eq!(updated["sort_order"], 5000);
    assert_eq!(
        updated["revision"], 1,
        "metadata does not bump the content revision"
    );
    // HTML pages and content_text are not available.
    let html = post(
        &owner,
        &format!("/v1/docs/items/{doc_id}/pages"),
        json!({"title": "x", "content_format": "html"}),
    )
    .await;
    assert_eq!(html.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let text = expect(
        post(
            &owner,
            &format!("/v1/docs/items/{doc_id}/pages"),
            json!({"title": "x", "content_text": "<p>"}),
        )
        .await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    assert_eq!(text["error"]["code"], "docs_content_format_mismatch");
    let block_doc_text = post(
        &owner,
        "/v1/docs/items",
        json!({"title": "x", "first_page_content_text": "x"}),
    )
    .await;
    assert_eq!(block_doc_text.status(), StatusCode::BAD_REQUEST);
    // Deleting a page trashes its subtree.
    expect(
        delete(
            &owner,
            &format!("/v1/docs/pages/{}", second["id"].as_str().unwrap()),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let pages = expect(
        get(&owner, &format!("/v1/docs/items/{doc_id}/pages")).await,
        StatusCode::OK,
    )
    .await;
    let ids: Vec<&str> = pages["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec![root["id"].as_str().unwrap()]);
    let gone = get(
        &owner,
        &format!("/v1/docs/pages/{}", nested["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(gone.status(), StatusCode::FORBIDDEN);
    let item = expect(
        get(&owner, &format!("/v1/docs/items/{doc_id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(item["page_count"], 1);
}

#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn content_saves_check_the_revision() {
    let owner = user("Editor A");
    let doc = create_doc(&owner, "Shared draft", None, true).await;
    let doc_id = doc["id"].as_str().unwrap();
    let page = first_page(&owner, doc_id).await;
    let page_path = format!("/v1/docs/pages/{}", page["id"].as_str().unwrap());
    let blocks = |text: &str| json!([{"type": "paragraph", "content": [{"type": "text", "text": text, "styles": {}}]}]);
    let saved = expect(
        patch(
            &owner,
            &page_path,
            json!({"content_blocks": blocks("first"), "expected_revision": 1}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(saved["revision"], 2);
    // Another person (global space member) saves against the old revision.
    let other = user("Editor B");
    let conflict = expect(
        patch(
            &other,
            &page_path,
            json!({"content_blocks": blocks("theirs"), "expected_revision": 1}),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(conflict["error"]["code"], "docs_page_revision_conflict");
    let current = expect(get(&other, &page_path).await, StatusCode::OK).await;
    assert_eq!(current["content_blocks"], blocks("first"));
    assert_eq!(current["revision"], 2);
    // Saving the same content again is not a change and not a conflict.
    let same = expect(
        patch(
            &other,
            &page_path,
            json!({"content_blocks": blocks("first"), "expected_revision": 1}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(same["revision"], 2);
    // "Keep mine": save against the latest revision.
    let kept = expect(
        patch(
            &other,
            &page_path,
            json!({"content_blocks": blocks("theirs"), "expected_revision": 2}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(kept["revision"], 3);
    // Page text feeds the integrated search.
    let search_text: String =
        sqlx::query_scalar("SELECT search_text FROM docs_native_doc_pages WHERE id = $1")
            .bind(page["id"].as_str().unwrap())
            .fetch_one(&db().await)
            .await
            .unwrap();
    assert_eq!(search_text, "theirs");
    // A viewer without edit access gets 403, not a conflict.
    let private = create_doc(&owner, "Private", None, false).await;
    let private_page = first_page(&owner, private["id"].as_str().unwrap()).await;
    let refused = patch(
        &other,
        &format!("/v1/docs/pages/{}", private_page["id"].as_str().unwrap()),
        json!({"content_blocks": blocks("x"), "expected_revision": 1}),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
}

/// AI-DO test_duplicate_doc_via_read_share_creates_private_copy_for_recipient.
#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn duplicate_doc_via_read_share_creates_private_copy_for_recipient() {
    let owner = user("Owner");
    let recipient = user("Recipient");
    let original = create_doc(&owner, "Shared Plan", None, false).await;
    let original_id = original["id"].as_str().unwrap();
    expect(
        post(
            &owner,
            &format!("/v1/docs/items/{original_id}/pages"),
            json!({"title": "Appendix"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    expect(
        put(
            &owner,
            &format!("/v1/docs/items/{original_id}/sharing/users/{recipient}"),
            json!({"access_level": "read"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let duplicate = expect(
        post(
            &recipient,
            &format!("/v1/docs/items/{original_id}/duplicate"),
            json!({}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(duplicate["title"], "Shared Plan Copy");
    assert_eq!(duplicate["can_manage"], true);
    assert_eq!(duplicate["is_private"], true);
    assert_eq!(duplicate["page_count"], 2);
    assert_eq!(duplicate["created_by_id"], recipient);
}

#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn hub_views_favorites_and_recent_pages() {
    let me = user("Viewer of views");
    let mine = create_doc(&me, "Mine private", None, false).await;
    let space_doc = create_doc(&me, "Mine in space", None, true).await;
    let other = user("Other author");
    let theirs = create_doc(&other, "Theirs in space", None, true).await;
    let id = |v: &Value| v["id"].as_str().unwrap().to_string();
    assert!(hub_ids(&hub(&me, "?view=private").await).contains(&id(&mine)));
    assert!(!hub_ids(&hub(&me, "?view=private").await).contains(&id(&space_doc)));
    assert!(hub_ids(&hub(&me, "?view=mine").await).contains(&id(&space_doc)));
    assert!(hub_ids(&hub(&me, "?view=shared").await).contains(&id(&theirs)));
    assert!(!hub_ids(&hub(&me, "?view=shared").await).contains(&id(&mine)));
    assert!(hub_ids(&hub(&me, "?q=MINE%20PRIV").await).contains(&id(&mine)));
    // Favorites and recent pages.
    let favorite = expect(
        patch(
            &me,
            &format!("/v1/docs/items/{}/favorite", id(&theirs)),
            json!({}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(favorite["is_favorite"], true);
    let favorites = expect(
        get(&me, "/v1/docs/favorites?scope=docs_spaces").await,
        StatusCode::OK,
    )
    .await;
    let fav = favorites
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == id(&theirs))
        .unwrap();
    assert_eq!(fav["space"]["key"], "global");
    let page = first_page(&me, &id(&theirs)).await;
    expect(
        post(
            &me,
            &format!("/v1/docs/items/{}/view", id(&theirs)),
            json!({"page_id": page["id"]}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let recent = expect(
        get(&me, "/v1/docs/recent-pages?limit=5&scope=docs_spaces").await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(recent[0]["doc_id"], id(&theirs));
    assert_eq!(recent[0]["page_id"], page["id"]);
    assert!(hub_ids(&hub(&me, "?view=recent").await).contains(&id(&theirs)));
    // A doc that becomes unreadable leaves favorites and recent pages.
    expect(
        delete(&other, &format!("/v1/docs/items/{}/target", id(&theirs))).await,
        StatusCode::OK,
    )
    .await;
    let favorites = expect(
        get(&me, "/v1/docs/favorites?scope=docs_spaces").await,
        StatusCode::OK,
    )
    .await;
    assert!(
        !favorites
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["id"] == id(&theirs))
    );
    let recent = expect(
        get(&me, "/v1/docs/recent-pages?scope=docs_spaces").await,
        StatusCode::OK,
    )
    .await;
    assert!(
        !recent
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["doc_id"] == id(&theirs))
    );
    // Archived shows trashed docs to those who manage them.
    expect(
        delete(&me, &format!("/v1/docs/items/{}", id(&mine))).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert!(hub_ids(&hub(&me, "?view=archived").await).contains(&id(&mine)));
    assert!(!hub_ids(&hub(&me, "").await).contains(&id(&mine)));
    let bad = get(&me, "/v1/docs/hub?page_size=500").await;
    assert_eq!(bad.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ---------------------------------------------------------------- files

async fn upload_attachment(user: &str, page_id: &str, name: &str, bytes: &[u8]) -> Value {
    let reserved = expect(
        post(
            user,
            &format!("/v1/docs/pages/{page_id}/attachments"),
            json!({"request_id": uuid::Uuid::new_v4().to_string(), "filename": name,
                   "content_type": "text/plain", "size_bytes": bytes.len()}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(reserved["status"], "pending");
    let stored = file_call(
        user,
        Method::PUT,
        &format!(
            "/files/docs/attachments/{}/content",
            reserved["id"].as_str().unwrap()
        ),
        Some(bytes.to_vec()),
    )
    .await;
    expect(stored, StatusCode::OK).await
}

/// AI-DO test_attachment_round_trip_lists_uploads_in_order,
/// test_attachment_delete_removes_row_and_object,
/// test_attachment_over_the_size_limit_is_refused,
/// test_trashing_a_page_takes_its_attachments_with_it and
/// test_html_attachment_is_never_served_as_renderable_html.
#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn attachments_round_trip_with_page_access() {
    let owner = user("Attacher");
    let doc = create_doc(&owner, "With files", None, false).await;
    let doc_id = doc["id"].as_str().unwrap().to_string();
    let page = first_page(&owner, &doc_id).await;
    let page_id = page["id"].as_str().unwrap().to_string();
    let first = upload_attachment(&owner, &page_id, "a/b/first.html", b"<script>x</script>").await;
    assert_eq!(first["status"], "attached");
    assert_eq!(first["filename"], "first.html");
    assert_eq!(first["uploaded_by_name"], "Attacher");
    let second = upload_attachment(&owner, &page_id, "second.txt", b"two").await;
    let listed = expect(
        get(&owner, &format!("/v1/docs/pages/{page_id}/attachments")).await,
        StatusCode::OK,
    )
    .await;
    let names: Vec<&str> = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["filename"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["first.html", "second.txt"]);
    // Readers download; never as renderable HTML.
    let reader = user("File reader");
    expect(
        put(
            &owner,
            &format!("/v1/docs/items/{doc_id}/sharing/users/{reader}"),
            json!({"access_level": "read"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let download = file_call(
        &reader,
        Method::GET,
        &format!(
            "/files/docs/attachments/{}/content",
            first["id"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(download.status(), StatusCode::OK);
    assert_eq!(
        download.headers()["content-type"],
        "application/octet-stream"
    );
    assert_eq!(
        download.bytes().await.unwrap().as_ref(),
        b"<script>x</script>"
    );
    // Readers cannot add or remove.
    let refused = post(
        &reader,
        &format!("/v1/docs/pages/{page_id}/attachments"),
        json!({"request_id": uuid::Uuid::new_v4().to_string(), "filename": "x", "size_bytes": 1}),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        delete(
            &reader,
            &format!("/v1/docs/attachments/{}", second["id"].as_str().unwrap())
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    // Strangers cannot list or download.
    let stranger = user("Stranger");
    assert_eq!(
        get(&stranger, &format!("/v1/docs/pages/{page_id}/attachments"))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        file_call(
            &stranger,
            Method::GET,
            &format!(
                "/files/docs/attachments/{}/content",
                first["id"].as_str().unwrap()
            ),
            None
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    // Over the size limit.
    let too_big = expect(
        post(&owner, &format!("/v1/docs/pages/{page_id}/attachments"),
            json!({"request_id": uuid::Uuid::new_v4().to_string(), "filename": "big.bin", "size_bytes": 10 * 1024 * 1024 + 1})).await,
        StatusCode::PAYLOAD_TOO_LARGE,
    )
    .await;
    assert_eq!(
        too_big["error"]["code"],
        "docs_attachment_size_limit_exceeded"
    );
    // Delete detaches the core file.
    expect(
        delete(
            &owner,
            &format!("/v1/docs/attachments/{}", second["id"].as_str().unwrap()),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(file_status(&attachment_file(&second).await), "cancelled");
    // Trashing the page keeps the attachment; emptying it from the trash releases it.
    let extra = expect(
        post(
            &owner,
            &format!("/v1/docs/items/{doc_id}/pages"),
            json!({"title": "Extra"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let extra_file =
        upload_attachment(&owner, extra["id"].as_str().unwrap(), "extra.txt", b"x").await;
    expect(
        delete(
            &owner,
            &format!("/v1/docs/pages/{}", extra["id"].as_str().unwrap()),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let extra_file_id = attachment_file(&extra_file).await;
    assert_eq!(file_status(&extra_file_id), "attached");
    let trashed_download = file_call(
        &reader,
        Method::GET,
        &format!(
            "/files/docs/attachments/{}/content",
            extra_file["id"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(trashed_download.status(), StatusCode::FORBIDDEN);
    expect(
        delete(
            &owner,
            &format!("/v1/docs/trash/pages/{}", extra["id"].as_str().unwrap()),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(file_status(&extra_file_id), "cancelled");
    assert_eq!(file_status(&attachment_file(&first).await), "attached");
}

#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn attachment_reservations_are_recovered_by_request_id() {
    let owner = user("Retrier");
    let doc = create_doc(&owner, "Retry", None, false).await;
    let page_id = first_page(&owner, doc["id"].as_str().unwrap()).await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let request_id = uuid::Uuid::new_v4().to_string();
    let body = json!({"request_id": request_id, "filename": "r.txt", "content_type": "text/plain", "size_bytes": 3});
    let first = expect(
        post(
            &owner,
            &format!("/v1/docs/pages/{page_id}/attachments"),
            body.clone(),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let again = expect(
        post(
            &owner,
            &format!("/v1/docs/pages/{page_id}/attachments"),
            body.clone(),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(first["id"], again["id"]);
    // The same request ID for another file is a conflict, never a second file.
    let mut other = body.clone();
    other["size_bytes"] = json!(4);
    let conflict = expect(
        post(
            &owner,
            &format!("/v1/docs/pages/{page_id}/attachments"),
            other,
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(
        conflict["error"]["code"],
        "docs_attachment_request_conflict"
    );
    let path = format!(
        "/files/docs/attachments/{}/content",
        first["id"].as_str().unwrap()
    );
    expect(
        file_call(&owner, Method::PUT, &path, Some(b"abc".to_vec())).await,
        StatusCode::OK,
    )
    .await;
    // Repeating the transfer after a lost answer reports the stored state.
    let repeated = expect(
        file_call(&owner, Method::PUT, &path, Some(b"abc".to_vec())).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(repeated["status"], "attached");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM docs_page_attachments WHERE request_id = $1::uuid",
    )
    .bind(&request_id)
    .fetch_one(&db().await)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn body_images_follow_page_access_and_are_released_with_the_page() {
    let owner = user("Image owner");
    let doc = create_doc(&owner, "Pictures", None, false).await;
    let doc_id = doc["id"].as_str().unwrap().to_string();
    let page = first_page(&owner, &doc_id).await;
    let page_id = page["id"].as_str().unwrap().to_string();
    let reserved = expect(post(&owner, "/v1/media/upload",
        json!({"requestId": uuid::Uuid::new_v4().to_string(), "name": "p.png", "mime": "image/png", "size": 4})).await, StatusCode::CREATED).await;
    let media_id = reserved["id"].as_str().unwrap().to_string();
    expect(
        file_call(
            &owner,
            Method::PUT,
            &format!("/files/media/{media_id}/content"),
            Some(b"\x89PNG".to_vec()),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let reader = user("Image reader");
    // Not linked yet: only the uploader sees it.
    assert_eq!(
        file_call(
            &reader,
            Method::GET,
            &format!("/files/media/{media_id}/content"),
            None
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    let blocks = json!([{"type": "image", "props": {"url": format!("media:{media_id}")}}]);
    expect(
        patch(
            &owner,
            &format!("/v1/docs/pages/{page_id}"),
            json!({"content_blocks": blocks, "expected_revision": 1}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let (resource, status): (Option<String>, String) =
        sqlx::query_as("SELECT resource_id, status FROM media_files WHERE id = $1")
            .bind(&media_id)
            .fetch_one(&db().await)
            .await
            .unwrap();
    assert_eq!(resource.as_deref(), Some(page_id.as_str()));
    assert_eq!(status, "attached");
    // Still hidden from people who cannot read the page.
    assert_eq!(
        file_call(
            &reader,
            Method::GET,
            &format!("/files/media/{media_id}/content"),
            None
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    expect(
        put(
            &owner,
            &format!("/v1/docs/items/{doc_id}/sharing/users/{reader}"),
            json!({"access_level": "read"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let shown = file_call(
        &reader,
        Method::GET,
        &format!("/files/media/{media_id}/content"),
        None,
    )
    .await;
    assert_eq!(shown.status(), StatusCode::OK);
    // SVG is refused.
    let svg = post(&owner, "/v1/media/upload", json!({"requestId": uuid::Uuid::new_v4().to_string(), "name": "x.svg", "mime": "image/svg+xml", "size": 4})).await;
    assert_eq!(svg.status(), StatusCode::BAD_REQUEST);
    // Removing the image from the body keeps it linked (older versions show it).
    expect(
        patch(
            &owner,
            &format!("/v1/docs/pages/{page_id}"),
            json!({"content_blocks": [], "expected_revision": 2}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        file_call(
            &reader,
            Method::GET,
            &format!("/files/media/{media_id}/content"),
            None
        )
        .await
        .status(),
        StatusCode::OK
    );
    // Trashing keeps the image; emptying the trash releases it.
    expect(
        delete(&owner, &format!("/v1/docs/items/{doc_id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let file_id: String = sqlx::query_scalar("SELECT file_id FROM media_files WHERE id = $1")
        .bind(&media_id)
        .fetch_one(&db().await)
        .await
        .unwrap();
    assert_eq!(file_status(&file_id), "attached");
    assert_eq!(
        file_call(
            &reader,
            Method::GET,
            &format!("/files/media/{media_id}/content"),
            None
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    expect(
        delete(&owner, &format!("/v1/docs/trash/items/{doc_id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(file_status(&file_id), "cancelled");
}

// ---------------------------------------------------------------- platform endpoints

#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn platform_calls_only_answer_what_the_person_may_read_now() {
    let owner = user("Searcher owner");
    let token = uuid::Uuid::new_v4().simple().to_string();
    let private = create_doc(&owner, &format!("Secret {token}"), None, false).await;
    let public = create_doc(&owner, &format!("Open {token}"), None, true).await;
    let public_page = first_page(&owner, public["id"].as_str().unwrap()).await;
    expect(patch(&owner, &format!("/v1/docs/pages/{}", public_page["id"].as_str().unwrap()),
        json!({"content_blocks": [{"type": "paragraph", "content": [{"type": "text", "text": format!("body word{token}"), "styles": {}}]}], "expected_revision": 1})).await, StatusCode::OK).await;
    let reader = user("Searcher");
    // Non-platform calls are refused.
    assert_eq!(
        post(&reader, "/v1/search", json!({"query": token, "limit": 5}))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let found = expect(
        platform(&reader, "/v1/search", json!({"query": token, "limit": 10})).await,
        StatusCode::OK,
    )
    .await;
    let titles: Vec<&str> = found["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, vec![format!("Open {token}").as_str()]);
    assert_eq!(found["items"][0]["kind"], "문서");
    let by_text = expect(
        platform(
            &reader,
            "/v1/search",
            json!({"query": format!("word{token}"), "limit": 10, "locale": "en-US"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(by_text["items"][0]["kind"], "Doc");
    assert!(
        by_text["items"][0]["path"]
            .as_str()
            .unwrap()
            .contains("?page=")
    );
    assert!(
        by_text["items"][0]["snippet"]
            .as_str()
            .unwrap()
            .contains(&format!("word{token}"))
    );
    // LIKE characters match literally.
    let wildcard = expect(
        platform(&reader, "/v1/search", json!({"query": "%", "limit": 10})).await,
        StatusCode::OK,
    )
    .await;
    assert!(
        wildcard["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["title"].as_str().unwrap().contains('%'))
    );
    // The owner sees both.
    let owner_found = expect(
        platform(&owner, "/v1/search", json!({"query": token, "limit": 10})).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(owner_found["items"].as_array().unwrap().len(), 2);
    // Notification check: only readable docs, matching resource IDs.
    let private_id = private["id"].as_str().unwrap();
    let public_id = public["id"].as_str().unwrap();
    let check = expect(
        platform(
            &reader,
            "/platform/notification-check",
            json!({"items": [
                {"groupKey": format!("doc:{private_id}"), "resourceId": private_id},
                {"groupKey": format!("doc:{public_id}"), "resourceId": public_id},
                {"groupKey": format!("doc:{public_id}"), "resourceId": "other"},
                {"groupKey": "post:1", "resourceId": "1"}
            ]}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(check["allowed"], json!([format!("doc:{public_id}")]));
    assert_eq!(
        post(
            &reader,
            "/platform/notification-check",
            json!({"items": []})
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    // Home card: recent docs still readable now.
    expect(
        post(
            &reader,
            &format!("/v1/docs/items/{public_id}/view"),
            json!({"page_id": public_page["id"]}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let card = expect(
        platform(
            &reader,
            "/v1/home-card",
            json!({"timeZone": "Asia/Seoul", "locale": "ko-KR"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(card["title"], "최근 문서");
    assert_eq!(card["items"][0]["title"], format!("Open {token}"));
    expect(
        delete(&owner, &format!("/v1/docs/items/{public_id}/target")).await,
        StatusCode::OK,
    )
    .await;
    let card = expect(
        platform(&reader, "/v1/home-card", json!({"locale": "en-US"})).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(card["title"], "Recent docs");
    assert!(
        !card["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["title"] == format!("Open {token}"))
    );
    let check = expect(
        platform(
            &reader,
            "/platform/notification-check",
            json!({"items": [
        {"groupKey": format!("doc:{public_id}"), "resourceId": public_id}]}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(check["allowed"], json!([]));
}

#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn platform_calls_count_group_members_through_signed_groups() {
    let manager = make_manager("Platform space admin").await;
    let space = create_space(&manager, "Platform space").await;
    let direct = user("Direct member");
    let via_group = user("Group member");
    let group = group("Group", &[&via_group]);
    set_space_members(
        &manager,
        &space,
        &[("user", &direct, "viewer"), ("group", &group, "member")],
    )
    .await;
    let token = uuid::Uuid::new_v4().simple().to_string();
    let doc = create_doc(&manager, &format!("Space doc {token}"), Some(&space), true).await;
    let doc_id = doc["id"].as_str().unwrap().to_string();
    let search = json!({"query": token, "limit": 5});
    let direct_found = expect(
        platform(&direct, "/v1/search", search.clone()).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(direct_found["items"].as_array().unwrap().len(), 1);
    // The core signed the group member's current groups into the call.
    let with_groups = |path: &'static str, body: Value| {
        let header = platform_identity_with_groups(&via_group);
        async move { send(base(), Some(header), Method::POST, path, Some(body)).await }
    };
    let group_found = expect(
        with_groups("/v1/search", search.clone()).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(group_found["items"].as_array().unwrap().len(), 1);
    let check = expect(
        with_groups(
            "/platform/notification-check",
            json!({"items": [{"groupKey": format!("doc:{doc_id}"), "resourceId": doc_id}]}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(check["allowed"], json!([format!("doc:{doc_id}")]));
    // Not approved to read the directory: no signed groups, user grants only.
    let unsigned = expect(
        platform(&via_group, "/v1/search", search.clone()).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(unsigned["items"].as_array().unwrap().len(), 0);
    // Leaving the group: the next call carries the current groups only.
    core().memberships.remove(&via_group);
    let left = expect(with_groups("/v1/search", search).await, StatusCode::OK).await;
    assert_eq!(left["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn managers_bootstrap_once_and_keep_one() {
    // Isolated from parallel tests: only checks relative behavior.
    let me_view = expect(get(&user("Curious"), "/v1/docs/me").await, StatusCode::OK).await;
    if me_view["canBootstrap"] == true {
        // A parallel test may add a manager first: then this is the late call.
        let first = user("First manager");
        let status = post(&first, "/v1/docs/managers/bootstrap", json!({}))
            .await
            .status();
        assert!(
            status == StatusCode::CREATED || status == StatusCode::CONFLICT,
            "{status}"
        );
    }
    let late = expect(
        post(&user("Late"), "/v1/docs/managers/bootstrap", json!({})).await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(late["error"]["code"], "docs_manager_exists");
    let manager = make_manager("Solo").await;
    let not_manager = user("Plain");
    assert_eq!(
        get(&not_manager, "/v1/docs/managers").await.status(),
        StatusCode::FORBIDDEN
    );
    let listed = expect(get(&manager, "/v1/docs/managers").await, StatusCode::OK).await;
    assert!(
        listed["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["userId"] == manager)
    );
}

// ---------------------------------------------------------------- 보강: trash

#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn trash_lists_restores_and_purges_docs_and_pages() {
    let owner = user("Trash owner");
    let doc = create_doc(&owner, "Trash doc", None, true).await;
    let doc_id = doc["id"].as_str().unwrap().to_string();
    let root = first_page(&owner, &doc_id).await;
    let parent = expect(
        post(
            &owner,
            &format!("/v1/docs/items/{doc_id}/pages"),
            json!({"title": "Parent"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let child = expect(
        post(
            &owner,
            &format!("/v1/docs/items/{doc_id}/pages"),
            json!({"title": "Child", "parent_id": parent["id"]}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let grandchild = expect(
        post(
            &owner,
            &format!("/v1/docs/items/{doc_id}/pages"),
            json!({"title": "Grandchild", "parent_id": child["id"]}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let id = |v: &Value| v["id"].as_str().unwrap().to_string();
    // Trash the child (with its grandchild), then the parent separately.
    expect(
        delete(&owner, &format!("/v1/docs/pages/{}", id(&child))).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    expect(
        delete(&owner, &format!("/v1/docs/pages/{}", id(&parent))).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let trash = expect(get(&owner, "/v1/docs/trash").await, StatusCode::OK).await;
    let mine: Vec<&Value> = trash["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["doc_id"] == doc_id)
        .collect();
    assert_eq!(mine.len(), 2, "one entry per deletion: {mine:?}");
    let child_entry = mine.iter().find(|e| e["id"] == id(&child)).unwrap();
    assert_eq!(child_entry["kind"], "page");
    assert_eq!(child_entry["subpage_count"], 1);
    assert_eq!(child_entry["parent_in_trash"], true);
    assert_eq!(child_entry["trashed_by_name"], "Trash owner");
    // A space member (edit) sees page entries but may not purge them.
    let member = user("Trash member");
    let as_member = expect(get(&member, "/v1/docs/trash").await, StatusCode::OK).await;
    let member_entry = as_member["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id(&child))
        .unwrap()
        .clone();
    assert_eq!(member_entry["can_delete"], false);
    assert_eq!(
        delete(&member, &format!("/v1/docs/trash/pages/{}", id(&child)))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    // Restoring the child while its parent is trashed puts it at the top of the tree.
    let restored = expect(
        post(
            &member,
            &format!("/v1/docs/trash/pages/{}/restore", id(&child)),
            json!({}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(restored["moved_to_root"], true);
    assert_eq!(restored["restored_count"], 2);
    assert_eq!(restored["page"]["parent_id"], Value::Null);
    let pages = expect(
        get(&owner, &format!("/v1/docs/items/{doc_id}/pages")).await,
        StatusCode::OK,
    )
    .await;
    let live: Vec<String> = pages["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect();
    assert!(
        live.contains(&id(&child)) && live.contains(&id(&grandchild)) && live.contains(&id(&root))
    );
    assert!(!live.contains(&id(&parent)));
    // Not in the trash any more.
    let again = expect(
        post(
            &owner,
            &format!("/v1/docs/trash/pages/{}/restore", id(&child)),
            json!({}),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(again["error"]["code"], "docs_not_in_trash");
    // Purge the parent for good.
    expect(
        delete(&owner, &format!("/v1/docs/trash/pages/{}", id(&parent))).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(
        get(&owner, &format!("/v1/docs/pages/{}", id(&parent)))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    // Trash the doc: its pages go with it and come back with it; a page
    // trashed before stays in the trash.
    let extra = expect(
        post(
            &owner,
            &format!("/v1/docs/items/{doc_id}/pages"),
            json!({"title": "Earlier"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    expect(
        delete(&owner, &format!("/v1/docs/pages/{}", id(&extra))).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    expect(
        delete(&owner, &format!("/v1/docs/items/{doc_id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let trash = expect(get(&owner, "/v1/docs/trash").await, StatusCode::OK).await;
    let entries: Vec<&Value> = trash["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["doc_id"] == doc_id)
        .collect();
    assert_eq!(
        entries.len(),
        1,
        "page entries of a trashed doc are inside the doc entry"
    );
    assert_eq!(entries[0]["kind"], "doc");
    // A member cannot see or restore a doc they cannot manage.
    let as_member = expect(get(&member, "/v1/docs/trash").await, StatusCode::OK).await;
    assert!(
        !as_member["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["doc_id"] == doc_id)
    );
    assert_eq!(
        post(
            &member,
            &format!("/v1/docs/trash/items/{doc_id}/restore"),
            json!({})
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    let back = expect(
        post(
            &owner,
            &format!("/v1/docs/trash/items/{doc_id}/restore"),
            json!({}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(back["trashed_at"], Value::Null);
    let pages = expect(
        get(&owner, &format!("/v1/docs/items/{doc_id}/pages")).await,
        StatusCode::OK,
    )
    .await;
    let live: Vec<String> = pages["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect();
    assert!(live.contains(&id(&child)) && live.contains(&id(&root)));
    assert!(!live.contains(&id(&extra)));
    let trash = expect(get(&owner, "/v1/docs/trash").await, StatusCode::OK).await;
    assert!(
        trash["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == id(&extra))
    );
    // Purge needs the doc in the trash; then it is gone.
    assert_eq!(
        delete(&owner, &format!("/v1/docs/trash/items/{doc_id}"))
            .await
            .status(),
        StatusCode::CONFLICT
    );
    expect(
        delete(&owner, &format!("/v1/docs/items/{doc_id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    expect(
        delete(&owner, &format!("/v1/docs/trash/items/{doc_id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(
        get(&owner, &format!("/v1/docs/items/{doc_id}"))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

// ---------------------------------------------------------------- 보강: versions

#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn versions_record_each_saved_revision_and_restore_as_new_ones() {
    let owner = user("Versioner");
    let doc = create_doc(&owner, "History", None, false).await;
    let page = first_page(&owner, doc["id"].as_str().unwrap()).await;
    let page_id = page["id"].as_str().unwrap().to_string();
    let blocks = |text: &str| json!([{"type": "paragraph", "content": [{"type": "text", "text": text, "styles": {}}]}]);
    for (revision, text) in [(1, "one"), (2, "two"), (3, "three")] {
        expect(
            patch(
                &owner,
                &format!("/v1/docs/pages/{page_id}"),
                json!({"content_blocks": blocks(text), "expected_revision": revision}),
            )
            .await,
            StatusCode::OK,
        )
        .await;
    }
    let versions = expect(
        get(&owner, &format!("/v1/docs/pages/{page_id}/versions")).await,
        StatusCode::OK,
    )
    .await;
    let revisions: Vec<i64> = versions["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["revision"].as_i64().unwrap())
        .collect();
    assert_eq!(revisions, vec![4, 3, 2, 1]);
    assert_eq!(versions["items"][0]["is_current"], true);
    assert_eq!(versions["items"][0]["created_by_name"], "Versioner");
    assert!(versions["items"][0].get("content_blocks").is_none());
    let two_id = versions["items"][2]["id"].as_str().unwrap().to_string();
    let preview = expect(
        get(
            &owner,
            &format!("/v1/docs/pages/{page_id}/versions/{two_id}"),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(preview["content_blocks"], blocks("one"));
    // A reader previews but cannot restore.
    let reader = user("Version reader");
    expect(
        put(
            &owner,
            &format!(
                "/v1/docs/items/{}/sharing/users/{reader}",
                doc["id"].as_str().unwrap()
            ),
            json!({"access_level": "read"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let as_reader = expect(
        get(&reader, &format!("/v1/docs/pages/{page_id}/versions")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(as_reader["can_restore"], false);
    assert_eq!(
        post(
            &reader,
            &format!("/v1/docs/pages/{page_id}/versions/{two_id}/restore"),
            json!({"expected_revision": 4})
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    // Restoring against an old revision conflicts.
    let stale = expect(
        post(
            &owner,
            &format!("/v1/docs/pages/{page_id}/versions/{two_id}/restore"),
            json!({"expected_revision": 3}),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(stale["error"]["code"], "docs_page_revision_conflict");
    let restored = expect(
        post(
            &owner,
            &format!("/v1/docs/pages/{page_id}/versions/{two_id}/restore"),
            json!({"expected_revision": 4}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(restored["revision"], 5);
    assert_eq!(restored["content_blocks"], blocks("one"));
    let versions = expect(
        get(&owner, &format!("/v1/docs/pages/{page_id}/versions")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        versions["items"].as_array().unwrap().len(),
        5,
        "history is never rewritten"
    );
    assert_eq!(versions["items"][0]["restored_from_revision"], 2);
    // Pruning keeps the last 50 and one per older day.
    let pool = db().await;
    for revision in 6..=60 {
        sqlx::query(
            "INSERT INTO docs_page_versions (id, page_id, revision, title, content_blocks, created_by_id, created_at) \
             VALUES ($1, $2, $3, 'x', '[]', 'u', now() - make_interval(days => $4))",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&page_id)
        .bind(revision as i64)
        .bind(match revision {
            6..=8 => 5,
            9..=11 => 4,
            _ => 0,
        })
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query("UPDATE docs_native_doc_pages SET revision = 60 WHERE id = $1")
        .bind(&page_id)
        .execute(&pool)
        .await
        .unwrap();
    expect(
        patch(
            &owner,
            &format!("/v1/docs/pages/{page_id}"),
            json!({"content_blocks": blocks("latest"), "expected_revision": 60}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let kept: Vec<i64> = sqlx::query_scalar(
        "SELECT revision FROM docs_page_versions WHERE page_id = $1 ORDER BY revision DESC",
    )
    .bind(&page_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(kept.len(), 52, "{kept:?}");
    assert_eq!(kept[0], 61);
    assert_eq!(kept[49], 12);
    // Older than the latest 50 (revisions 1..=11): 1..=5 share today with
    // revision 61, so today is already kept; 6..=8 and 9..=11 keep their last.
    assert!(kept.contains(&11) && kept.contains(&8));
    assert!(!kept.contains(&10) && !kept.contains(&7) && !kept.contains(&5));
}

// ---------------------------------------------------------------- 보강: comments

#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn comments_threads_mentions_and_notifications() {
    let owner = user("Comment owner");
    let doc = create_doc(&owner, "Discussed", None, false).await;
    let doc_id = doc["id"].as_str().unwrap().to_string();
    let page = first_page(&owner, &doc_id).await;
    let page_id = page["id"].as_str().unwrap().to_string();
    let reader = user("Comment reader");
    let outsider = user("Outsider");
    expect(
        put(
            &owner,
            &format!("/v1/docs/items/{doc_id}/sharing/users/{reader}"),
            json!({"access_level": "read"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    // Readers comment; strangers cannot.
    let request_id = uuid::Uuid::new_v4().to_string();
    let body = json!({"body": "  Looks good @Comment owner  ", "mention_ids": [owner, outsider], "request_id": request_id});
    let created = expect(
        post(
            &reader,
            &format!("/v1/docs/pages/{page_id}/comments"),
            body.clone(),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let comment_id = created["id"].as_str().unwrap().to_string();
    // Same request ID: the same comment, no second notification.
    let repeated = expect(
        post(&reader, &format!("/v1/docs/pages/{page_id}/comments"), body).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(repeated["id"], comment_id);
    assert_eq!(
        post(
            &outsider,
            &format!("/v1/docs/pages/{page_id}/comments"),
            json!({"body": "x", "request_id": uuid::Uuid::new_v4().to_string()})
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    // The owner is mentioned (mention, not comment); the outsider cannot read the page and gets nothing.
    let mention_events = events_for(&format!("mention:{comment_id}"));
    assert_eq!(mention_events.len(), 1);
    assert_eq!(mention_events[0]["body"]["kind"], "mention");
    assert_eq!(mention_events[0]["body"]["recipients"], json!([owner]));
    assert_eq!(
        mention_events[0]["body"]["path"],
        format!("/{doc_id}?page={page_id}&comment={comment_id}")
    );
    assert_eq!(
        mention_events[0]["body"]["title"],
        "Comment reader님이 \"Discussed\"에서 회원님을 언급했습니다"
    );
    assert!(events_for(&format!("comment:{comment_id}")).is_empty());
    // A reply notifies the thread author and the owner (comment kind).
    let reply = expect(post(&owner, &format!("/v1/docs/pages/{page_id}/comments"),
        json!({"body": "Thanks", "parent_id": comment_id, "request_id": uuid::Uuid::new_v4().to_string()})).await, StatusCode::CREATED).await;
    let reply_id = reply["id"].as_str().unwrap().to_string();
    let reply_events = events_for(&format!("comment:{reply_id}"));
    assert_eq!(reply_events.len(), 1);
    assert_eq!(reply_events[0]["body"]["recipients"], json!([reader]));
    // Replies are one level deep.
    assert_eq!(post(&owner, &format!("/v1/docs/pages/{page_id}/comments"),
        json!({"body": "deeper", "parent_id": reply_id, "request_id": uuid::Uuid::new_v4().to_string()})).await.status(), StatusCode::NOT_FOUND);
    // Listing shows the thread, names and flags, and reads the notifications.
    let listed = expect(
        get(&owner, &format!("/v1/docs/pages/{page_id}/comments")).await,
        StatusCode::OK,
    )
    .await;
    let thread = &listed["items"][0];
    assert_eq!(thread["body"], "Looks good @Comment owner");
    assert_eq!(thread["author_name"], "Comment reader");
    assert_eq!(thread["mentions"].as_array().unwrap().len(), 2);
    assert_eq!(thread["replies"][0]["id"], reply_id);
    assert_eq!(thread["can_edit"], false);
    assert_eq!(thread["can_delete"], true, "the owner manages the doc");
    assert_eq!(thread["can_resolve"], true);
    assert!(
        events_for(&format!("mention:{comment_id}"))
            .iter()
            .any(|e| e["op"] == "read" && e["by"] == owner)
    );
    // Resolve and reopen (root only).
    expect(
        post(
            &reader,
            &format!("/v1/docs/comments/{comment_id}/resolve"),
            json!({}),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let listed = expect(
        get(&reader, &format!("/v1/docs/pages/{page_id}/comments")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(listed["items"][0]["resolved_by_name"], "Comment reader");
    assert_eq!(
        post(
            &owner,
            &format!("/v1/docs/comments/{reply_id}/resolve"),
            json!({})
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    expect(
        post(
            &owner,
            &format!("/v1/docs/comments/{comment_id}/reopen"),
            json!({}),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    // Editing: only the author; removing a mention retracts it for that person.
    assert_eq!(
        patch(
            &owner,
            &format!("/v1/docs/comments/{comment_id}"),
            json!({"body": "x"})
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    expect(
        patch(
            &reader,
            &format!("/v1/docs/comments/{comment_id}"),
            json!({"body": "Edited", "mention_ids": []}),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert!(
        events_for(&format!("mention:{comment_id}"))
            .iter()
            .any(|e| e["op"] == "retract" && e["body"]["recipients"] == json!([owner]))
    );
    // Losing access retracts: removing the reader's share retracts their reply notification.
    expect(
        delete(
            &owner,
            &format!("/v1/docs/items/{doc_id}/sharing/users/{reader}"),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert!(
        events_for(&format!("comment:{reply_id}"))
            .iter()
            .any(|e| e["op"] == "retract" && e["body"]["recipients"] == json!([reader]))
    );
    // Deleting a comment retracts its groups and leaves a placeholder for a thread with replies.
    expect(
        put(
            &owner,
            &format!("/v1/docs/items/{doc_id}/sharing/users/{reader}"),
            json!({"access_level": "read"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    expect(
        delete(&reader, &format!("/v1/docs/comments/{comment_id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert!(
        events_for(&format!("comment:{comment_id}"))
            .iter()
            .any(|e| e["op"] == "retract")
    );
    let listed = expect(
        get(&owner, &format!("/v1/docs/pages/{page_id}/comments")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(listed["items"][0]["is_deleted"], true);
    assert_eq!(listed["items"][0]["body"], "");
    assert_eq!(listed["items"][0]["replies"].as_array().unwrap().len(), 1);
    // Platform recheck: live comments on readable pages only.
    let check = expect(
        platform(
            &reader,
            "/platform/notification-check",
            json!({"items": [
                {"groupKey": format!("comment:{reply_id}"), "resourceId": reply_id},
                {"groupKey": format!("mention:{comment_id}"), "resourceId": comment_id}
            ]}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(check["allowed"], json!([format!("comment:{reply_id}")]));
    // Trashing the page retracts the remaining comment notifications.
    let second = expect(post(&owner, &format!("/v1/docs/pages/{page_id}/comments"),
        json!({"body": "ping", "mention_ids": [reader], "request_id": uuid::Uuid::new_v4().to_string()})).await, StatusCode::CREATED).await;
    let second_id = second["id"].as_str().unwrap().to_string();
    assert_eq!(events_for(&format!("mention:{second_id}")).len(), 1);
    let extra = expect(
        post(
            &owner,
            &format!("/v1/docs/items/{doc_id}/pages"),
            json!({"title": "Keep"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    assert!(extra["id"].is_string());
    expect(
        delete(&owner, &format!("/v1/docs/pages/{page_id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert!(
        events_for(&format!("mention:{second_id}"))
            .iter()
            .any(|e| e["op"] == "retract" && e["body"]["recipients"] == json!([reader]))
    );
    let check = expect(
        platform(
            &reader,
            "/platform/notification-check",
            json!({"items": [
        {"groupKey": format!("comment:{reply_id}"), "resourceId": reply_id}]}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(check["allowed"], json!([]));
    // Unknown mentions are refused.
    let unknown = expect(post(&owner, &format!("/v1/docs/pages/{}/comments", extra["id"].as_str().unwrap()),
        json!({"body": "x", "mention_ids": ["u-nobody"], "request_id": uuid::Uuid::new_v4().to_string()})).await, StatusCode::BAD_REQUEST).await;
    assert_eq!(unknown["error"]["code"], "docs_mention_unknown_user");
}

// ---------------------------------------------------------------- 보강: templates

#[tokio::test]
#[ignore = "needs DOCS_TEST_DATABASE_URL"]
async fn templates_are_saved_from_pages_and_start_new_pages_and_docs() {
    let manager = make_manager("Template admin").await;
    let space = create_space(&manager, "Templates space").await;
    let member = user("Template member");
    let viewer = user("Template viewer");
    let outsider = user("Template outsider");
    set_space_members(
        &manager,
        &space,
        &[("user", &member, "member"), ("user", &viewer, "viewer")],
    )
    .await;
    let doc = create_doc(&member, "Source", Some(&space), true).await;
    let doc_id = doc["id"].as_str().unwrap().to_string();
    let page = first_page(&member, &doc_id).await;
    let page_id = page["id"].as_str().unwrap().to_string();
    let blocks = json!([{"type": "heading", "props": {"level": 2}, "content": [{"type": "text", "text": "Agenda", "styles": {}}]}]);
    expect(
        patch(
            &member,
            &format!("/v1/docs/pages/{page_id}"),
            json!({"content_blocks": blocks, "expected_revision": 1}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    // Viewers may not save templates; members may.
    assert_eq!(
        post(&viewer, "/v1/docs/templates", json!({"page_id": page_id}))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let template = expect(
        post(
            &member,
            "/v1/docs/templates",
            json!({"page_id": page_id, "title": "Meeting", "description": "Weekly"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let template_id = template["id"].as_str().unwrap().to_string();
    assert_eq!(template["space_id"], space);
    let listed = expect(
        get(&viewer, &format!("/v1/docs/templates?space={space}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(listed["items"][0]["title"], "Meeting");
    assert_eq!(listed["items"][0]["can_delete"], false);
    assert_eq!(listed["can_create"], false);
    assert_eq!(
        get(&outsider, &format!("/v1/docs/templates?space={space}"))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    // New page from the template.
    let from_template = expect(
        post(
            &member,
            &format!("/v1/docs/items/{doc_id}/pages"),
            json!({"title": "This week", "template_id": template_id}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(from_template["content_blocks"], blocks);
    // New doc from the template in the same space.
    let new_doc = expect(
        post(
            &member,
            &format!("/v1/docs/items?space={space}"),
            json!({"title": "From template", "template_id": template_id,
        "primary_target": {"app": "docs", "type": "workspace_sidebar", "id": space}}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let new_page = first_page(&member, new_doc["id"].as_str().unwrap()).await;
    assert_eq!(new_page["content_blocks"], blocks);
    // A template of another space cannot be used here.
    let elsewhere = post(
        &member,
        "/v1/docs/items",
        json!({"title": "x", "template_id": template_id}),
    )
    .await;
    assert_eq!(elsewhere.status(), StatusCode::NOT_FOUND);
    // The creator or a space admin deletes it.
    assert_eq!(
        delete(&viewer, &format!("/v1/docs/templates/{template_id}"))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    expect(
        delete(&manager, &format!("/v1/docs/templates/{template_id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(
        get(&member, &format!("/v1/docs/templates/{template_id}"))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}
