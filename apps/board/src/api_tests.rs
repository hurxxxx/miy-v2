//! HTTP tests through the real SDK gateway-identity check, PostgreSQL and a
//! fake core (directory, notifications, files) that the SDK calls with the
//! signed identity's delegation proof.
//!
//! Ports of AI-DO apps/api/tests/test_community.py, test_announcements.py and
//! the banned-word cases of test_ugc_policy*.py, plus miy additions
//! (notifications, platform endpoints, spaces, board managers, media).
//!
//! They need a disposable PostgreSQL server, so they are ignored by default:
//!   BOARD_TEST_DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:PORT/postgres \
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

const SECRET: &str = "board-api-test-gateway-secret-0123456789";
const INSTALLATION: &str = "00000000-0000-4000-8000-0000000000b1";
const RELEASE: &str = "00000000-0000-4000-8000-0000000000b2";
const DEFAULT: &str = "suggestions";

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
    let name = format!("board_test_{}", uuid::Uuid::new_v4().simple());
    let mut admin = PgConnection::connect(admin_url)
        .await
        .expect("connect to BOARD_TEST_DATABASE_URL");
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
    let admin_url = std::env::var("BOARD_TEST_DATABASE_URL")
        .expect("set BOARD_TEST_DATABASE_URL to a disposable PostgreSQL server");
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

async fn json_of(response: Response) -> Value {
    let status = response.status();
    let text = response.text().await.unwrap();
    serde_json::from_str(&text).unwrap_or_else(|_| panic!("{status}: {text}"))
}

async fn expect(response: Response, status: StatusCode) -> Value {
    let actual = response.status();
    let text = response.text().await.unwrap();
    assert_eq!(actual, status, "{text}");
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

async fn make_manager(name: &str) -> String {
    let id = user(name);
    sqlx::query(
        "INSERT INTO board_managers (user_id, granted_by, created_at) VALUES ($1, $1, now())",
    )
    .bind(&id)
    .execute(&db().await)
    .await
    .unwrap();
    id
}

async fn create_channel(admin: &str, key_prefix: &str, flags: Value) -> String {
    let key = format!(
        "{key_prefix}-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    );
    let mut body = json!({"key": key, "name": key_prefix, "description": "", "position": 1, "active": true,
        "readOnly": false, "forceAnonymous": false, "adminOnlyContent": false,
        "templateTitle": "", "templateBody": ""});
    for (name, value) in flags.as_object().unwrap() {
        body[name] = value.clone();
    }
    let created = expect(
        post(admin, "/v1/community/admin/channels", body).await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(created["key"], key);
    key
}

/// Posts in one channel take numbers one at a time; parallel tests sharing the
/// default channel resubmit a refused post as a writer would (AI-DO 409
/// `community.post_number_busy`).
async fn create_post(user: &str, channel: &str, body: Value) -> Value {
    for _ in 0..50 {
        let response = post(
            user,
            &format!("/v1/community/channels/{channel}/posts"),
            body.clone(),
        )
        .await;
        if response.status() == StatusCode::CONFLICT {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            continue;
        }
        return expect(response, StatusCode::CREATED).await;
    }
    panic!("channel {channel} stayed busy");
}

async fn simple_post(user: &str, channel: &str, title: &str) -> Value {
    create_post(
        user,
        channel,
        json!({"title": title, "body": "본문", "isAnonymous": false, "isSecret": false}),
    )
    .await
}

async fn list(user: &str, channel: &str, query: &str) -> Value {
    expect(
        get(
            user,
            &format!("/v1/community/channels/{channel}/posts{query}"),
        )
        .await,
        StatusCode::OK,
    )
    .await
}

// ---------------------------------------------------------------- community

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn default_channel_and_posts_are_company_scoped() {
    let author = user("author");
    let other = user("other");
    let channels = expect(get(&author, "/v1/community/channels").await, StatusCode::OK).await;
    assert!(
        channels["channels"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["key"] == DEFAULT)
    );
    let created = simple_post(&author, DEFAULT, "건의합니다").await;
    assert_eq!(created["channelKey"], DEFAULT);
    let detail = expect(
        get(
            &other,
            &format!("/v1/community/posts/{}", created["id"].as_str().unwrap()),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(detail["title"], "건의합니다");
    assert_eq!(detail["authorName"], "author");
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn post_read_state_is_per_user_idempotent_and_resets_for_other_users_on_edit() {
    let admin = make_manager("admin").await;
    let channel = create_channel(&admin, "reads", json!({})).await;
    let author = user("author");
    let reader = user("reader");
    let created = simple_post(&author, &channel, "읽음 상태").await;
    assert_eq!(created["isRead"], true);
    let id = created["id"].as_str().unwrap().to_string();
    let detail = expect(
        get(&reader, &format!("/v1/community/posts/{id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(detail["isRead"], false);
    let pool = db().await;
    let count = || async {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM community_post_reads WHERE post_id = $1 AND user_id = $2",
        )
        .bind(&id)
        .bind(&reader)
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    assert_eq!(count().await, 0, "detail has no side effect");
    for _ in 0..2 {
        expect(
            post(
                &reader,
                &format!("/v1/community/posts/{id}/read"),
                json!({}),
            )
            .await,
            StatusCode::NO_CONTENT,
        )
        .await;
    }
    assert_eq!(count().await, 1);
    assert_eq!(
        list(&reader, &channel, "").await["posts"][0]["isRead"],
        true
    );
    let updated = expect(
        patch(
            &author,
            &format!("/v1/community/posts/{id}"),
            json!({"title": "읽음 상태 수정", "body": "수정 내용"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(updated["isRead"], true);
    assert_eq!(
        list(&reader, &channel, "").await["posts"][0]["isRead"],
        false
    );
    assert_eq!(
        list(&author, &channel, "").await["posts"][0]["isRead"],
        true
    );
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn deleting_post_removes_read_receipts_and_retracts_its_notifications() {
    let author = user("author");
    let reader = user("reader");
    let created = simple_post(&author, DEFAULT, "삭제할 글").await;
    let id = created["id"].as_str().unwrap().to_string();
    expect(
        post(
            &reader,
            &format!("/v1/community/posts/{id}/read"),
            json!({}),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    expect(
        delete(&author, &format!("/v1/community/posts/{id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let reads: i64 =
        sqlx::query_scalar("SELECT count(*) FROM community_post_reads WHERE post_id = $1")
            .bind(&id)
            .fetch_one(&db().await)
            .await
            .unwrap();
    assert_eq!(reads, 0);
    let key = format!("post:{id}");
    assert!(
        events()
            .iter()
            .any(|e| e["op"] == "retract" && e["body"]["groupKey"] == key)
    );
    expect(
        get(&author, &format!("/v1/community/posts/{id}")).await,
        StatusCode::NOT_FOUND,
    )
    .await;
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn comment_on_my_post_notifies_me_with_a_link_and_reading_marks_it_read() {
    let author = user("author");
    let commenter = user("commenter-named");
    let admin = make_manager("admin").await;
    let channel = create_channel(&admin, "team", json!({})).await;
    let created = simple_post(&author, &channel, "알림 받을 글").await;
    let id = created["id"].as_str().unwrap().to_string();
    expect(
        post(
            &commenter,
            &format!("/v1/community/posts/{id}/comments"),
            json!({"body": "좋은 의견입니다", "isAnonymous": false}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let key = format!("post:{id}");
    let sent: Vec<Value> = events()
        .into_iter()
        .filter(|e| e["op"] == "notify" && e["body"]["groupKey"] == key)
        .collect();
    assert_eq!(sent.len(), 1);
    let body = &sent[0]["body"];
    assert_eq!(body["kind"], "comment");
    assert_eq!(body["recipients"], json!([author]));
    assert_eq!(
        body["path"],
        format!("/community/posts/{id}?channel={channel}")
    );
    assert_eq!(body["resourceId"], id);
    let title = body["title"].as_str().unwrap();
    assert!(title.contains("commenter-named님이"), "{title}");
    assert!(title.contains("알림 받을 글"));
    assert!(
        !title.contains("좋은 의견입니다"),
        "no comment body in the line"
    );
    expect(
        get(&author, &format!("/v1/community/posts/{id}")).await,
        StatusCode::OK,
    )
    .await;
    assert!(
        events().iter().any(|e| e["op"] == "read"
            && e["by"] == author.as_str()
            && e["body"]["groupKey"] == key)
    );
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn anonymous_comment_notification_hides_commenter_identity() {
    let author = user("author");
    let commenter = user("secret-commenter");
    let created = simple_post(&author, DEFAULT, "익명 댓글 받을 글").await;
    let id = created["id"].as_str().unwrap().to_string();
    expect(
        post(
            &commenter,
            &format!("/v1/community/posts/{id}/comments"),
            json!({"body": "익명 의견", "isAnonymous": true}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let key = format!("post:{id}");
    let sent = events()
        .into_iter()
        .find(|e| e["op"] == "notify" && e["body"]["groupKey"] == key)
        .unwrap();
    let title = sent["body"]["title"].as_str().unwrap();
    assert!(title.starts_with("익명 사용자가"), "{title}");
    assert!(!title.contains("secret-commenter"));
    assert_eq!(sent["body"]["path"], format!("/community/posts/{id}"));
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn my_own_comment_does_not_notify_me() {
    let author = user("author");
    let created = simple_post(&author, DEFAULT, "내 글").await;
    let id = created["id"].as_str().unwrap().to_string();
    expect(
        post(
            &author,
            &format!("/v1/community/posts/{id}/comments"),
            json!({"body": "내 댓글"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let key = format!("post:{id}");
    assert!(
        !events()
            .iter()
            .any(|e| e["op"] == "notify" && e["body"]["groupKey"] == key)
    );
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn admin_only_channel_comment_notifies_only_a_manager_author() {
    let admin = make_manager("admin").await;
    let channel = create_channel(&admin, "box", json!({"adminOnlyContent": true})).await;
    let author = user("author");
    let created = simple_post(&author, &channel, "건의").await;
    let id = created["id"].as_str().unwrap().to_string();
    expect(
        post(
            &admin,
            &format!("/v1/community/posts/{id}/comments"),
            json!({"body": "확인했습니다"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let key = format!("post:{id}");
    assert!(
        !events()
            .iter()
            .any(|e| e["op"] == "notify" && e["body"]["groupKey"] == key)
    );
}

async fn upload_image(user_id: &str, bytes: &[u8]) -> (String, String) {
    upload_named_image(user_id, "screenshot.png", bytes).await
}

async fn upload_named_image(user_id: &str, name: &str, bytes: &[u8]) -> (String, String) {
    let reserved = expect(
        post(
            user_id,
            "/v1/media/upload",
            json!({"requestId": uuid::Uuid::new_v4().to_string(),
            "name": name, "mime": "image/png", "size": bytes.len()}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let media_id = reserved["id"].as_str().unwrap().to_string();
    assert_eq!(reserved["url"], format!("media:{media_id}"));
    let uploaded = Client::new()
        .put(format!("{}/api/files/media/{media_id}/content", base()))
        .header("x-miy-identity", identity(user_id))
        .header("content-type", "application/octet-stream")
        .body(bytes.to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(
        uploaded.status(),
        StatusCode::OK,
        "{}",
        uploaded.text().await.unwrap()
    );
    let file_id: String = sqlx::query_scalar("SELECT file_id FROM media_files WHERE id = $1")
        .bind(&media_id)
        .fetch_one(&db().await)
        .await
        .unwrap();
    (media_id, file_id)
}

async fn media_link(media_id: &str) -> (Option<String>, Option<String>, String) {
    sqlx::query_as("SELECT resource_type, resource_id, status FROM media_files WHERE id = $1")
        .bind(media_id)
        .fetch_one(&db().await)
        .await
        .unwrap()
}

async fn download(user_id: &str, media_id: &str) -> StatusCode {
    Client::new()
        .get(format!("{}/api/files/media/{media_id}/content", base()))
        .header("x-miy-identity", identity(user_id))
        .send()
        .await
        .unwrap()
        .status()
}

async fn download_post_media(
    user_id: &str,
    post_id: &str,
    media_id: &str,
    password: Option<&str>,
) -> Response {
    Client::new()
        .post(format!(
            "{}/api/files/community/posts/{post_id}/media/{media_id}",
            base()
        ))
        .header("x-miy-identity", identity(user_id))
        .json(&json!({"password": password}))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn create_post_and_comment_link_embedded_media_and_delete_releases_it() {
    let author = user("author");
    let reader = user("reader");
    let stranger = user("stranger");
    let (media_id, file_id) = upload_image(&author, b"\x89PNG fake").await;
    assert_eq!(
        download(&author, &media_id).await,
        StatusCode::OK,
        "uploader previews"
    );
    assert_eq!(
        download(&stranger, &media_id).await,
        StatusCode::NOT_FOUND,
        "unlinked media is the uploader's"
    );
    let created = create_post(
        &author,
        DEFAULT,
        json!({"title": "사진", "body": format!("![첨부](media:{media_id})")}),
    )
    .await;
    let post_id = created["id"].as_str().unwrap().to_string();
    assert_eq!(
        media_link(&media_id).await,
        (
            Some("community_post".into()),
            Some(post_id.clone()),
            "attached".into()
        )
    );
    assert_eq!(file_status(&file_id), "attached");
    assert_eq!(download(&reader, &media_id).await, StatusCode::OK);
    let bytes = download_post_media(&reader, &post_id, &media_id, None).await;
    assert_eq!(bytes.status(), StatusCode::OK);
    assert_eq!(bytes.bytes().await.unwrap().as_ref(), b"\x89PNG fake");

    // Someone else cannot link another person's upload.
    let (other_media, _) = upload_image(&author, b"second").await;
    let comment = expect(
        post(
            &reader,
            &format!("/v1/community/posts/{post_id}/comments"),
            json!({"body": format!("![x](media:{other_media})")}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(media_link(&other_media).await.0, None);
    let (comment_media, comment_file) = upload_image(&reader, b"third").await;
    expect(
        patch(
            &reader,
            &format!(
                "/v1/community/posts/{post_id}/comments/{}",
                comment["id"].as_str().unwrap()
            ),
            json!({"body": format!("![y](media:{comment_media})")}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        media_link(&comment_media).await.0.as_deref(),
        Some("community_comment")
    );
    assert_eq!(
        download_post_media(&stranger, &post_id, &comment_media, None)
            .await
            .status(),
        StatusCode::OK
    );

    expect(
        delete(&author, &format!("/v1/community/posts/{post_id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(file_status(&file_id), "cancelled");
    assert_eq!(file_status(&comment_file), "cancelled");
    assert_eq!(media_link(&media_id).await.2, "detached");
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn resolve_secret_post_media_accepts_post_password() {
    let author = user("author");
    let reader = user("reader");
    let (media_id, _) = upload_image(&author, b"secret image").await;
    let created = create_post(
        &author,
        DEFAULT,
        json!({"title": "비밀", "body": format!("![s](media:{media_id})"),
        "isSecret": true, "password": "secret-pw"}),
    )
    .await;
    let post_id = created["id"].as_str().unwrap().to_string();
    assert_eq!(download(&reader, &media_id).await, StatusCode::NOT_FOUND);
    assert_eq!(
        download_post_media(&reader, &post_id, &media_id, None)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        download_post_media(&reader, &post_id, &media_id, Some("wrong"))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        download_post_media(&reader, &post_id, &media_id, Some("secret-pw"))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        download_post_media(&author, &post_id, &media_id, None)
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn media_upload_validates_type_and_size() {
    let author = user("author");
    let svg = post(
        &author,
        "/v1/media/upload",
        json!({"requestId": uuid::Uuid::new_v4().to_string(),
        "name": "x.svg", "mime": "image/svg+xml", "size": 10}),
    )
    .await;
    assert_eq!(
        json_of(svg).await["error"]["code"],
        "media_unsupported_file_type"
    );
    let big = post(
        &author,
        "/v1/media/upload",
        json!({"requestId": uuid::Uuid::new_v4().to_string(),
        "name": "x.png", "mime": "image/png", "size": 10 * 1024 * 1024 + 1}),
    )
    .await;
    assert_eq!(big.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn inactive_channel_is_hidden_from_public_channel_list() {
    let admin = make_manager("admin").await;
    let key = create_channel(&admin, "hidden", json!({"active": false})).await;
    let reader = user("reader");
    let public = expect(get(&reader, "/v1/community/channels").await, StatusCode::OK).await;
    assert!(
        !public["channels"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["key"] == key.as_str())
    );
    let admin_list = expect(
        get(&admin, "/v1/community/admin/channels").await,
        StatusCode::OK,
    )
    .await;
    assert!(
        admin_list["channels"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["key"] == key.as_str())
    );
    expect(
        get(&reader, "/v1/community/admin/channels").await,
        StatusCode::FORBIDDEN,
    )
    .await;
    expect(
        get(&reader, &format!("/v1/community/channels/{key}/posts")).await,
        StatusCode::NOT_FOUND,
    )
    .await;
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn list_posts_searches_visible_title_and_author_across_pages() {
    let admin = make_manager("admin").await;
    let channel = create_channel(&admin, "search", json!({})).await;
    let author = user("author");
    let writer_name = format!("writer-{}", uuid::Uuid::new_v4().simple());
    let writer = user(&writer_name);
    simple_post(&author, &channel, "Alpha release").await;
    simple_post(&writer, &channel, "Beta handbook").await;
    let by_title = list(&author, &channel, "?page=1&page_size=1&q=beta").await;
    assert_eq!(by_title["total"], 1);
    assert_eq!(by_title["posts"][0]["title"], "Beta handbook");
    let by_author = list(&author, &channel, &format!("?q={writer_name}")).await;
    assert_eq!(by_author["total"], 1);
    assert_eq!(by_author["posts"][0]["authorName"], writer_name);
    let paged = list(&author, &channel, "?page=2&page_size=1").await;
    assert_eq!(paged["total"], 2);
    assert_eq!(paged["page"], 2);
    assert_eq!(paged["pageSize"], 1);
    assert_eq!(paged["posts"][0]["title"], "Alpha release");
    expect(
        get(
            &author,
            &format!("/v1/community/channels/{channel}/posts?page_size=101"),
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn list_posts_search_does_not_match_hidden_admin_only_content() {
    let admin = make_manager("admin").await;
    let channel = create_channel(&admin, "private-search", json!({"adminOnlyContent": true})).await;
    let author_name = format!("author-{}", uuid::Uuid::new_v4().simple());
    let author = user(&author_name);
    let reader = user("reader");
    simple_post(&author, &channel, "Needle private title").await;
    assert_eq!(list(&reader, &channel, "").await["total"], 1);
    assert_eq!(list(&reader, &channel, "?q=needle").await["total"], 0);
    let by_author = list(&reader, &channel, &format!("?q={author_name}")).await;
    assert_eq!(by_author["total"], 1);
    assert_eq!(by_author["posts"][0]["authorName"], Value::Null);
    let admin_search = list(&admin, &channel, "?q=needle").await;
    assert_eq!(admin_search["total"], 1);
    assert_eq!(admin_search["posts"][0]["title"], "Needle private title");
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn list_posts_search_respects_anonymous_channel_visibility() {
    let admin = make_manager("admin").await;
    let channel = create_channel(&admin, "anonymous-search", json!({"forceAnonymous": true})).await;
    let author_name = format!("hidden-author-{}", uuid::Uuid::new_v4().simple());
    let author = user(&author_name);
    let reader = user("reader");
    simple_post(&author, &channel, "Visible anonymous title").await;
    let by_title = list(&reader, &channel, "?q=visible").await;
    assert_eq!(by_title["total"], 1);
    assert_eq!(by_title["posts"][0]["authorName"], Value::Null);
    assert_eq!(
        list(&reader, &channel, &format!("?q={author_name}")).await["total"],
        0
    );
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn list_posts_search_disables_private_anonymous_channel_for_regular_viewer() {
    let admin = make_manager("admin").await;
    let channel = create_channel(
        &admin,
        "private-anonymous",
        json!({"adminOnlyContent": true, "forceAnonymous": true}),
    )
    .await;
    let author_name = format!("private-author-{}", uuid::Uuid::new_v4().simple());
    let author = user(&author_name);
    let reader = user("reader");
    simple_post(&author, &channel, "Hidden combined title").await;
    assert_eq!(list(&reader, &channel, "?q=combined").await["total"], 0);
    assert_eq!(
        list(&reader, &channel, &format!("?q={author_name}")).await["total"],
        0
    );
    let admin_search = list(&admin, &channel, "?q=combined").await;
    assert_eq!(admin_search["total"], 1);
    assert_eq!(admin_search["posts"][0]["title"], "Hidden combined title");
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn channel_admin_contract_key_uniqueness_and_deletion_rules() {
    let admin = make_manager("admin").await;
    let key = create_channel(&admin, "dup", json!({})).await;
    let duplicate = post(
        &admin,
        "/v1/community/admin/channels",
        json!({"key": key, "name": "Dup"}),
    )
    .await;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_of(duplicate).await["error"]["code"],
        "community_channel_key_exists"
    );
    let blank = post(
        &admin,
        "/v1/community/admin/channels",
        json!({"key": "blank-name", "name": "   "}),
    )
    .await;
    assert_eq!(blank.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let other = create_channel(&admin, "other", json!({})).await;
    let renamed = patch(
        &admin,
        &format!("/v1/community/admin/channels/{other}"),
        json!({"key": key, "name": "x"}),
    )
    .await;
    assert_eq!(renamed.status(), StatusCode::CONFLICT);
    // An empty non-default channel can be deleted.
    expect(
        delete(&admin, &format!("/v1/community/admin/channels/{other}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    // A channel with posts and the default channel cannot.
    simple_post(&admin, &key, "글").await;
    let blocked = delete(&admin, &format!("/v1/community/admin/channels/{key}")).await;
    assert_eq!(blocked.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_of(blocked).await["error"]["code"],
        "community_channel_delete_blocked"
    );
    expect(
        delete(&admin, &format!("/v1/community/admin/channels/{DEFAULT}")).await,
        StatusCode::CONFLICT,
    )
    .await;
    let member = user("member");
    expect(
        post(
            &member,
            "/v1/community/admin/channels",
            json!({"key": "nope", "name": "Nope"}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn read_only_channel_rejects_regular_posts_and_comments_but_allows_admin() {
    let admin = make_manager("admin").await;
    let member = user("member");
    let channel = create_channel(&admin, "announcements", json!({"readOnly": true})).await;
    let denied = post(&member, &format!("/v1/community/channels/{channel}/posts"),
        json!({"title": "Member post", "body": "Should be blocked", "isAnonymous": false, "isSecret": false})).await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_of(denied).await["error"]["code"],
        "community_channel_read_only"
    );
    let admin_post = simple_post(&admin, &channel, "공지").await;
    let post_id = admin_post["id"].as_str().unwrap();
    expect(
        post(
            &member,
            &format!("/v1/community/posts/{post_id}/comments"),
            json!({"body": "댓글도 차단"}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    expect(
        post(
            &admin,
            &format!("/v1/community/posts/{post_id}/comments"),
            json!({"body": "관리자 댓글"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn read_only_channel_disables_regular_author_modifications() {
    let admin = make_manager("admin").await;
    let member = user("member");
    let channel = create_channel(&admin, "read-only-edits", json!({})).await;
    let created = simple_post(&member, &channel, "기존 글").await;
    let post_id = created["id"].as_str().unwrap().to_string();
    let comment = expect(
        post(
            &member,
            &format!("/v1/community/posts/{post_id}/comments"),
            json!({"body": "기존 댓글"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let comment_id = comment["id"].as_str().unwrap();
    expect(
        patch(
            &admin,
            &format!("/v1/community/admin/channels/{channel}"),
            json!({"key": channel, "name": "ro", "readOnly": true}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let detail = expect(
        get(&member, &format!("/v1/community/posts/{post_id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(detail["canModify"], false);
    assert_eq!(detail["comments"][0]["canModify"], false);
    expect(
        patch(
            &member,
            &format!("/v1/community/posts/{post_id}"),
            json!({"title": "수정", "body": "수정"}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    expect(
        patch(
            &member,
            &format!("/v1/community/posts/{post_id}/comments/{comment_id}"),
            json!({"body": "수정 댓글"}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    expect(
        delete(
            &member,
            &format!("/v1/community/posts/{post_id}/comments/{comment_id}"),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    expect(
        delete(&member, &format!("/v1/community/posts/{post_id}")).await,
        StatusCode::FORBIDDEN,
    )
    .await;
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn post_read_endpoint_is_authenticated_idempotent_and_detail_has_no_side_effect() {
    let admin = make_manager("admin").await;
    let channel = create_channel(&admin, "read-api", json!({})).await;
    let author = user("author");
    let reader = user("reader");
    let created = simple_post(&author, &channel, "읽음 API").await;
    assert_eq!(created["isRead"], true);
    let id = created["id"].as_str().unwrap();
    let anonymous = send(
        base(),
        None,
        Method::POST,
        &format!("/v1/community/posts/{id}/read"),
        None,
    )
    .await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    expect(
        post(
            &reader,
            "/v1/community/posts/does-not-exist/read",
            json!({}),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    let detail = expect(
        get(&reader, &format!("/v1/community/posts/{id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(detail["isRead"], false);
    assert_eq!(
        list(&reader, &channel, "").await["posts"][0]["isRead"],
        false
    );
    for _ in 0..2 {
        expect(
            post(
                &reader,
                &format!("/v1/community/posts/{id}/read"),
                json!({}),
            )
            .await,
            StatusCode::NO_CONTENT,
        )
        .await;
    }
    assert_eq!(
        list(&reader, &channel, "").await["posts"][0]["isRead"],
        true
    );
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn unlock_marks_secret_post_read_but_direct_read_is_denied_while_locked() {
    let admin = make_manager("admin").await;
    let channel = create_channel(&admin, "secret", json!({})).await;
    let author = user("author");
    let reader = user("reader");
    let created = create_post(
        &author,
        &channel,
        json!({"title": "비밀 글", "body": "비밀 본문", "isAnonymous": false,
        "isSecret": true, "password": "secret-pw"}),
    )
    .await;
    let id = created["id"].as_str().unwrap();
    expect(
        post(
            &author,
            &format!("/v1/community/posts/{id}/read"),
            json!({}),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    expect(
        post(
            &reader,
            &format!("/v1/community/posts/{id}/read"),
            json!({}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    let wrong = post(
        &reader,
        &format!("/v1/community/posts/{id}/unlock"),
        json!({"password": "wrong-pw"}),
    )
    .await;
    assert_eq!(wrong.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_of(wrong).await["error"]["code"],
        "community_invalid_password"
    );
    let unlocked = expect(
        post(
            &reader,
            &format!("/v1/community/posts/{id}/unlock"),
            json!({"password": "secret-pw"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(unlocked["locked"], false);
    assert_eq!(unlocked["isRead"], true);
    assert_eq!(unlocked["body"], "비밀 본문");
    let listed = list(&reader, &channel, "").await;
    assert_eq!(listed["posts"][0]["locked"], true);
    assert_eq!(listed["posts"][0]["isRead"], true);
    // A secret post without a password is refused (AI-DO community.password_required).
    let missing = post(
        &author,
        &format!("/v1/community/channels/{channel}/posts"),
        json!({"title": "t", "body": "b", "isSecret": true, "password": " "}),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_of(missing).await["error"]["code"],
        "community_password_required"
    );
    // Commenting on a secret post needs the password unless author or manager.
    expect(
        post(
            &reader,
            &format!("/v1/community/posts/{id}/comments"),
            json!({"body": "c"}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    expect(
        post(
            &reader,
            &format!("/v1/community/posts/{id}/comments"),
            json!({"body": "c", "password": "secret-pw"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    expect(
        post(
            &admin,
            &format!("/v1/community/posts/{id}/comments"),
            json!({"body": "c"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn admin_only_locked_post_cannot_be_marked_or_unlocked_by_regular_user() {
    let admin = make_manager("admin").await;
    let reader = user("reader");
    let channel =
        create_channel(&admin, "private-reports", json!({"adminOnlyContent": true})).await;
    let created = simple_post(&admin, &channel, "관리자 전용").await;
    // AI-DO answers the create with viewer_is_admin=False: locked even for a manager.
    assert_eq!(created["locked"], true);
    assert_eq!(created["lockedReason"], "admin_only");
    let id = created["id"].as_str().unwrap();
    let read = post(
        &reader,
        &format!("/v1/community/posts/{id}/read"),
        json!({}),
    )
    .await;
    assert_eq!(read.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_of(read).await["error"]["code"],
        "community_admin_only_content"
    );
    expect(
        post(
            &reader,
            &format!("/v1/community/posts/{id}/unlock"),
            json!({"password": "anything"}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    let reader_list = list(&reader, &channel, "").await;
    assert_eq!(reader_list["posts"][0]["locked"], true);
    assert_eq!(reader_list["posts"][0]["isRead"], false);
    let admin_list = list(&admin, &channel, "").await;
    assert_eq!(admin_list["posts"][0]["locked"], false);
    expect(
        post(
            &reader,
            &format!("/v1/community/posts/{id}/comments"),
            json!({"body": "c"}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn secret_post_is_masked_until_author_admin_or_password_unlock() {
    let admin = make_manager("admin").await;
    let channel = create_channel(&admin, "masked", json!({})).await;
    let author = user("author");
    let other = user("other");
    let created = create_post(&author, &channel, json!({"title": "비밀 제안", "body": "민감한 내용", "isSecret": true, "password": "secret-pw"})).await;
    assert_eq!(created["locked"], false);
    let id = created["id"].as_str().unwrap();
    let masked = &list(&other, &channel, "").await["posts"][0];
    assert_eq!(masked["locked"], true);
    assert_eq!(masked["title"], "");
    assert_eq!(masked["body"], "");
    let locked = expect(
        get(&other, &format!("/v1/community/posts/{id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(locked["locked"], true);
    assert_eq!(locked["comments"], json!([]));
    let author_detail = expect(
        get(&author, &format!("/v1/community/posts/{id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(author_detail["body"], "민감한 내용");
    let admin_detail = expect(
        get(&admin, &format!("/v1/community/posts/{id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(admin_detail["body"], "민감한 내용");
    let hash: String =
        sqlx::query_scalar("SELECT password_hash FROM community_posts WHERE id = $1")
            .bind(id)
            .fetch_one(&db().await)
            .await
            .unwrap();
    assert!(hash.starts_with("pbkdf2_sha256$600000$"));
    assert!(!hash.contains("secret-pw"));
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn force_anonymous_channel_masks_names_except_for_admin() {
    let admin = make_manager("admin").await;
    let channel = create_channel(&admin, "whistleblowing", json!({"forceAnonymous": true})).await;
    let author = user("author");
    let commenter = user("commenter");
    let created = simple_post(&author, &channel, "익명 제보").await;
    let id = created["id"].as_str().unwrap();
    let stored_anonymous: bool =
        sqlx::query_scalar("SELECT is_anonymous FROM community_posts WHERE id = $1")
            .bind(id)
            .fetch_one(&db().await)
            .await
            .unwrap();
    assert!(
        stored_anonymous,
        "the author is stored for moderation, marked anonymous"
    );
    expect(
        post(
            &commenter,
            &format!("/v1/community/posts/{id}/comments"),
            json!({"body": "댓글", "isAnonymous": false}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let regular = &list(&commenter, &channel, "").await["posts"][0];
    assert_eq!(regular["isAnonymous"], true);
    assert_eq!(regular["authorName"], Value::Null);
    let admin_page = &list(&admin, &channel, "").await["posts"][0];
    assert_eq!(admin_page["isAnonymous"], true);
    assert_eq!(admin_page["authorName"], "author");
    let regular_detail = expect(
        get(&commenter, &format!("/v1/community/posts/{id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(regular_detail["authorName"], Value::Null);
    assert_eq!(regular_detail["comments"][0]["isAnonymous"], true);
    assert_eq!(regular_detail["comments"][0]["authorName"], Value::Null);
    assert_eq!(regular_detail["comments"][0]["anonSeq"], 1);
    let admin_detail = expect(
        get(&admin, &format!("/v1/community/posts/{id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(admin_detail["authorName"], "author");
    assert_eq!(admin_detail["comments"][0]["authorName"], "commenter");
    let raw = serde_json::to_string(&regular_detail).unwrap();
    assert!(
        !raw.contains(&author) && !raw.contains(&commenter),
        "no user IDs leak to other users"
    );
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn admin_only_channel_masks_content_and_media_for_regular_viewers() {
    let admin = make_manager("admin").await;
    let channel = create_channel(&admin, "private-report", json!({"adminOnlyContent": true})).await;
    let author = user("author");
    let reader = user("reader");
    let (media_id, _) = upload_image(&author, b"report image").await;
    let created = create_post(&author, &channel, json!({"title": "비공개 제보", "body": format!("관리자 확인 필요\n\n![첨부](media:{media_id})"),
        "isAnonymous": false, "isSecret": true})).await;
    assert_eq!(created["locked"], true);
    assert_eq!(created["lockedReason"], "admin_only");
    assert_eq!(created["authorName"], Value::Null);
    let id = created["id"].as_str().unwrap();
    let secret: bool = sqlx::query_scalar("SELECT is_secret FROM community_posts WHERE id = $1")
        .bind(id)
        .fetch_one(&db().await)
        .await
        .unwrap();
    assert!(!secret);
    let masked = &list(&reader, &channel, "").await["posts"][0];
    assert_eq!(masked["locked"], true);
    assert_eq!(masked["title"], "");
    assert_eq!(masked["authorName"], Value::Null);
    let author_detail = expect(
        get(&author, &format!("/v1/community/posts/{id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(author_detail["locked"], true);
    assert_eq!(author_detail["comments"], json!([]));
    assert_eq!(
        download_post_media(&reader, id, &media_id, None)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(download(&reader, &media_id).await, StatusCode::NOT_FOUND);
    let admin_detail = expect(
        get(&admin, &format!("/v1/community/posts/{id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(admin_detail["locked"], false);
    assert_eq!(admin_detail["title"], "비공개 제보");
    assert_eq!(admin_detail["authorName"], "author");
    assert_eq!(
        download_post_media(&admin, id, &media_id, None)
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn anonymous_comment_numbering_is_stable_per_author() {
    let author = user("author");
    let first = user("first");
    let second = user("second");
    let created = simple_post(&author, DEFAULT, "제목").await;
    let id = created["id"].as_str().unwrap();
    for commenter in [&first, &second, &first] {
        expect(
            post(
                commenter,
                &format!("/v1/community/posts/{id}/comments"),
                json!({"body": "익명 의견", "isAnonymous": true}),
            )
            .await,
            StatusCode::CREATED,
        )
        .await;
    }
    let detail = expect(
        get(&author, &format!("/v1/community/posts/{id}")).await,
        StatusCode::OK,
    )
    .await;
    let anon: Vec<&Value> = detail["comments"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["isAnonymous"] == true)
        .collect();
    assert_eq!(
        anon.iter()
            .map(|c| c["anonSeq"].clone())
            .collect::<Vec<_>>(),
        vec![json!(1), json!(2), json!(1)]
    );
    assert!(anon.iter().all(|c| c["authorName"].is_null()));
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn deleting_comment_soft_deletes_and_keeps_replies() {
    let author = user("author");
    let replier = user("replier");
    let created = simple_post(&author, DEFAULT, "제목").await;
    let id = created["id"].as_str().unwrap();
    let parent = expect(
        post(
            &author,
            &format!("/v1/community/posts/{id}/comments"),
            json!({"body": "부모 댓글"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let parent_id = parent["id"].as_str().unwrap();
    let reply = expect(
        post(
            &replier,
            &format!("/v1/community/posts/{id}/comments"),
            json!({"body": "답글", "parentCommentId": parent_id}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(reply["parentCommentId"], parent_id);
    // A reply to a reply attaches to the top-level comment.
    let nested = expect(
        post(
            &author,
            &format!("/v1/community/posts/{id}/comments"),
            json!({"body": "답글의 답글", "parent_comment_id": reply["id"]}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(nested["parentCommentId"], parent_id);
    expect(
        delete(
            &replier,
            &format!("/v1/community/posts/{id}/comments/{parent_id}"),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    expect(
        delete(
            &author,
            &format!("/v1/community/posts/{id}/comments/{parent_id}"),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let detail = expect(
        get(&author, &format!("/v1/community/posts/{id}")).await,
        StatusCode::OK,
    )
    .await;
    let deleted = detail["comments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == parent_id)
        .unwrap();
    assert_eq!(deleted["isDeleted"], true);
    assert_eq!(deleted["body"], "");
    assert!(
        detail["comments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["parentCommentId"] == parent_id)
    );
    assert_eq!(detail["commentCount"], 2);
    expect(
        patch(
            &author,
            &format!("/v1/community/posts/{id}/comments/{parent_id}"),
            json!({"body": "x"}),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    expect(
        post(
            &author,
            &format!("/v1/community/posts/{id}/comments"),
            json!({"body": "x", "parentCommentId": parent_id}),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn posts_get_fixed_numbers_per_channel_that_are_never_reused() {
    let admin = make_manager("admin").await;
    let channel = create_channel(&admin, "numbers", json!({})).await;
    let other_channel = create_channel(&admin, "team-news", json!({})).await;
    let author = user("author");
    let updated_before: chrono::NaiveDateTime =
        sqlx::query_scalar("SELECT updated_at FROM community_channels WHERE key = $1")
            .bind(&channel)
            .fetch_one(&db().await)
            .await
            .unwrap();
    let first = simple_post(&author, &channel, "첫 글").await;
    let second = simple_post(&author, &channel, "둘째 글").await;
    let third = simple_post(&author, &channel, "셋째 글").await;
    assert_eq!(
        [&first["number"], &second["number"], &third["number"]],
        [&json!(1), &json!(2), &json!(3)]
    );
    expect(
        delete(
            &author,
            &format!("/v1/community/posts/{}", third["id"].as_str().unwrap()),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(simple_post(&author, &channel, "넷째 글").await["number"], 4);
    assert_eq!(
        simple_post(&author, &other_channel, "다른 게시판 첫 글").await["number"],
        1
    );
    let listed = list(&author, &channel, "").await;
    let numbers: Vec<Value> = listed["posts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["number"].clone())
        .collect();
    assert_eq!(numbers, vec![json!(4), json!(2), json!(1)]);
    let updated_after: chrono::NaiveDateTime =
        sqlx::query_scalar("SELECT updated_at FROM community_channels WHERE key = $1")
            .bind(&channel)
            .fetch_one(&db().await)
            .await
            .unwrap();
    assert_eq!(updated_before, updated_after);
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn list_posts_finds_a_post_by_its_number() {
    let admin = make_manager("admin").await;
    let channel = create_channel(&admin, "find-number", json!({})).await;
    let author = user("author");
    for title in ["Alpha", "Beta", "Gamma"] {
        simple_post(&author, &channel, title).await;
    }
    for query in ["%232", "2", "%20%232%20"] {
        let found = list(&author, &channel, &format!("?q={query}")).await;
        let posts: Vec<(Value, Value)> = found["posts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p["number"].clone(), p["title"].clone()))
            .collect();
        assert_eq!(posts, vec![(json!(2), json!("Beta"))], "{query}");
    }
    assert_eq!(list(&author, &channel, "?q=%2399").await["total"], 0);
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn post_number_lock_never_waits() {
    let admin = make_manager("admin").await;
    let channel = create_channel(&admin, "lock", json!({})).await;
    let author = user("author");
    let first = simple_post(&author, &channel, "첫 글").await;
    let pool = db().await;
    let mut holder = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM community_channels WHERE key = $1 FOR UPDATE")
        .bind(&channel)
        .execute(&mut *holder)
        .await
        .unwrap();
    let started = std::time::Instant::now();
    let busy = post(
        &author,
        &format!("/v1/community/channels/{channel}/posts"),
        json!({"title": "잠금 중 글", "body": "본문"}),
    )
    .await;
    assert!(started.elapsed() < std::time::Duration::from_millis(1500));
    assert_eq!(busy.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_of(busy).await["error"]["code"],
        "community_post_number_busy"
    );
    holder.rollback().await.unwrap();
    let mut created = Vec::new();
    let writers: Vec<_> = (0..4)
        .map(|index| {
            let author = author.clone();
            let channel = channel.clone();
            tokio::spawn(async move {
                post(
                    &author,
                    &format!("/v1/community/channels/{channel}/posts"),
                    json!({"title": format!("동시 글 {index}"), "body": "본문"}),
                )
                .await
            })
        })
        .collect();
    for writer in writers {
        let response = writer.await.unwrap();
        let status = response.status();
        assert!(status == StatusCode::CREATED || status == StatusCode::CONFLICT);
        if status == StatusCode::CREATED {
            created.push(json_of(response).await["number"].as_i64().unwrap());
        }
    }
    created.sort();
    let base_number = first["number"].as_i64().unwrap();
    assert!(!created.is_empty());
    assert_eq!(
        created,
        (1..=created.len() as i64)
            .map(|offset| base_number + offset)
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn validation_follows_the_ai_do_schemas() {
    let author = user("author");
    let path = format!("/v1/community/channels/{DEFAULT}/posts");
    for body in [
        json!({"title": "", "body": "b"}),
        json!({"title": "   ", "body": "b"}),
        json!({"title": "t".repeat(241), "body": "b"}),
        json!({"title": "t", "body": "b".repeat(20001)}),
        json!({"body": "b"}),
    ] {
        let response = post(&author, &path, body).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            json_of(response).await["error"]["code"],
            "validation_request_invalid"
        );
    }
    let created = create_post(
        &author,
        DEFAULT,
        json!({"title": "  공백  ", "body": "  본문  ", "is_anonymous": true}),
    )
    .await;
    assert_eq!(created["title"], "공백");
    assert_eq!(created["body"], "본문");
    assert_eq!(
        created["isAnonymous"], true,
        "snake_case keys are accepted (populate_by_name)"
    );
    let id = created["id"].as_str().unwrap();
    let long_comment = post(
        &author,
        &format!("/v1/community/posts/{id}/comments"),
        json!({"body": "c".repeat(10001)}),
    )
    .await;
    assert_eq!(long_comment.status(), StatusCode::UNPROCESSABLE_ENTITY);
    expect(
        get(&author, "/v1/community/channels/unknown-channel/posts").await,
        StatusCode::NOT_FOUND,
    )
    .await;
}

// ---------------------------------------------------------------- banned words

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn banned_words_reject_posts_and_comments_without_naming_the_term() {
    let admin = make_manager("admin").await;
    let member = user("member");
    let term = format!("금칙{}", &uuid::Uuid::new_v4().simple().to_string()[..6]);
    expect(
        post(
            &member,
            "/v1/community/admin/filter-terms",
            json!({"term": term}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    let added = expect(
        post(
            &admin,
            "/v1/community/admin/filter-terms",
            json!({"term": format!(" {term} ")}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(added["term"], term);
    assert!(added["created_at"].is_string());
    let duplicate = post(
        &admin,
        "/v1/community/admin/filter-terms",
        json!({"term": term.to_uppercase()}),
    )
    .await;
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_of(duplicate).await["error"]["code"],
        "ugc_filter_term_duplicate"
    );
    let invalid = post(
        &admin,
        "/v1/community/admin/filter-terms",
        json!({"term": "㍿".repeat(13)}),
    )
    .await;
    assert_eq!(
        json_of(invalid).await["error"]["code"],
        "ugc_filter_term_invalid"
    );
    let extra = post(
        &admin,
        "/v1/community/admin/filter-terms",
        json!({"term": "x", "other": 1}),
    )
    .await;
    assert_eq!(extra.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let listed = expect(
        get(&admin, "/v1/community/admin/filter-terms").await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(listed["limit"], 1000);
    assert!(
        listed["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["term"] == term.as_str())
    );

    let spaced: String = term.chars().map(|c| format!("{c} ")).collect();
    let rejected = post(
        &member,
        &format!("/v1/community/channels/{DEFAULT}/posts"),
        json!({"title": "제목", "body": format!("이건 {spaced} 입니다")}),
    )
    .await;
    assert_eq!(rejected.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let error = json_of(rejected).await;
    assert_eq!(error["error"]["code"], "ugc_content_rejected");
    assert!(!error.to_string().contains(&term));
    expect(
        post(
            &member,
            &format!("/v1/community/channels/{DEFAULT}/posts"),
            json!({"title": format!("{term}!"), "body": "b"}),
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    let created = simple_post(&member, DEFAULT, "정상 글").await;
    let id = created["id"].as_str().unwrap();
    expect(
        patch(
            &member,
            &format!("/v1/community/posts/{id}"),
            json!({"title": "정상", "body": term}),
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    expect(
        post(
            &member,
            &format!("/v1/community/posts/{id}/comments"),
            json!({"body": term}),
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    let comment = expect(
        post(
            &member,
            &format!("/v1/community/posts/{id}/comments"),
            json!({"body": "괜찮은 댓글"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    expect(
        patch(
            &member,
            &format!(
                "/v1/community/posts/{id}/comments/{}",
                comment["id"].as_str().unwrap()
            ),
            json!({"body": term}),
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;

    let term_id = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["term"] == term.as_str())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    expect(
        delete(
            &admin,
            &format!("/v1/community/admin/filter-terms/{term_id}"),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    expect(
        delete(
            &admin,
            &format!("/v1/community/admin/filter-terms/{term_id}"),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    expect(
        post(
            &member,
            &format!("/v1/community/posts/{id}/comments"),
            json!({"body": term}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
}

// ---------------------------------------------------------------- spaces and notices

async fn create_space(manager: &str, name: &str) -> String {
    let created = expect(
        post(manager, "/v1/spaces", json!({"name": name})).await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(created["canManage"], true);
    created["id"].as_str().unwrap().to_string()
}

async fn share(owner: &str, space: &str, entries: Value) -> Value {
    let sharing = expect(
        get(owner, &format!("/v1/spaces/{space}/sharing")).await,
        StatusCode::OK,
    )
    .await;
    expect(
        call(owner, Method::PUT, &format!("/v1/spaces/{space}/sharing"), Some(json!({
            "expectedRevision": sharing["revision"], "requestId": uuid::Uuid::new_v4().to_string(), "entries": entries,
        }))).await,
        StatusCode::OK,
    )
    .await
}

fn grant(kind: &str, id: &str, role: &str) -> Value {
    json!({"subject": {"kind": kind, "id": id}, "role": role})
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn announcement_crud_and_pin_ordering() {
    let manager = make_manager("manager").await;
    let space = create_space(&manager, "개발팀").await;
    let base_path = format!("/v1/spaces/{space}/announcements");
    let first = expect(
        post(
            &manager,
            &base_path,
            json!({"title": "1차 공지", "body": "본문 내용"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(first["title"], "1차 공지");
    assert_eq!(first["isPinned"], false);
    assert_eq!(first["authorName"], "manager");
    assert_eq!(first["workspaceId"], space);
    let second = expect(
        post(
            &manager,
            &base_path,
            json!({"title": "고정 공지", "isPinned": true}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let second_id = second["id"].as_str().unwrap();
    let listing = expect(get(&manager, &base_path).await, StatusCode::OK).await;
    let items = listing["items"].as_array().unwrap();
    assert_eq!(items[0]["id"], second_id, "pinned first");
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|item| item["canManage"] == true));
    expect(
        get(&manager, &format!("{base_path}/{second_id}")).await,
        StatusCode::OK,
    )
    .await;
    let patched = expect(
        patch(
            &manager,
            &format!("{base_path}/{second_id}"),
            json!({"isPinned": false, "title": "고정 해제 공지"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(patched["isPinned"], false);
    assert_eq!(patched["title"], "고정 해제 공지");
    let first_id = first["id"].as_str().unwrap();
    expect(
        delete(&manager, &format!("{base_path}/{first_id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    expect(
        get(&manager, &format!("{base_path}/{first_id}")).await,
        StatusCode::NOT_FOUND,
    )
    .await;
    let key = format!("notice:{first_id}");
    assert!(
        events()
            .iter()
            .any(|e| e["op"] == "retract" && e["body"]["groupKey"] == key)
    );
    let limited = expect(
        get(&manager, &format!("{base_path}?limit=0")).await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    assert_eq!(limited["error"]["code"], "validation_request_invalid");
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn announcement_requires_space_admin_to_write_and_membership_to_read() {
    let manager = make_manager("manager").await;
    let member = user("member");
    let outsider = user("outsider");
    let space = create_space(&manager, "영업팀").await;
    share(
        &manager,
        &space,
        json!([
            grant("user", &manager, "admin"),
            grant("user", &member, "member")
        ]),
    )
    .await;
    let base_path = format!("/v1/spaces/{space}/announcements");
    let forbidden = post(&member, &base_path, json!({"title": "권한 없는 공지"})).await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_of(forbidden).await["error"]["code"],
        "announcements_admin_required"
    );
    let listing = expect(get(&member, &base_path).await, StatusCode::OK).await;
    assert_eq!(listing["items"], json!([]));
    expect(get(&outsider, &base_path).await, StatusCode::FORBIDDEN).await;
    let spaces = expect(get(&member, "/v1/spaces").await, StatusCode::OK).await;
    let mine = spaces["spaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == space.as_str())
        .unwrap();
    assert_eq!(mine["canManage"], false);
    let outsider_spaces = expect(get(&outsider, "/v1/spaces").await, StatusCode::OK).await;
    assert!(
        !outsider_spaces["spaces"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == space.as_str())
    );
    expect(
        post(&member, "/v1/spaces", json!({"name": "새 공간"})).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    // Membership through a group works in the app (live ACL check).
    let grouped = user("grouped");
    let team = group("team", &[&grouped]);
    share(
        &manager,
        &space,
        json!([
            grant("user", &manager, "admin"),
            grant("group", &team, "admin")
        ]),
    )
    .await;
    expect(
        post(&grouped, &base_path, json!({"title": "그룹 관리자 공지"})).await,
        StatusCode::CREATED,
    )
    .await;
    expect(get(&member, &base_path).await, StatusCode::FORBIDDEN).await;
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn company_announcements_are_visible_everywhere_and_need_a_board_manager() {
    let manager = make_manager("manager").await;
    let space_a = create_space(&manager, "A").await;
    let space_b = create_space(&manager, "B").await;
    let other = user("other");
    share(
        &manager,
        &space_b,
        json!([
            grant("user", &manager, "admin"),
            grant("user", &other, "member")
        ]),
    )
    .await;
    let company = expect(
        post(
            &manager,
            &format!("/v1/spaces/{space_a}/announcements"),
            json!({"title": "전사 공지", "scope": "company", "isPinned": true}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(company["scope"], "company");
    assert_eq!(company["workspaceId"], Value::Null);
    let company_id = company["id"].as_str().unwrap();
    let workspace_only = expect(
        post(
            &manager,
            &format!("/v1/spaces/{space_a}/announcements"),
            json!({"title": "팀 공지"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let company_list = expect(
        get(
            &manager,
            &format!("/v1/spaces/{space_a}/announcements?scope=company"),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let ids: Vec<&str> = company_list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&company_id));
    assert!(!ids.contains(&workspace_only["id"].as_str().unwrap()));
    let workspace_list = expect(
        get(
            &manager,
            &format!("/v1/spaces/{space_a}/announcements?scope=workspace"),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert!(
        !workspace_list["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == company_id)
    );
    let from_b = expect(
        get(
            &other,
            &format!("/v1/spaces/{space_b}/announcements?scope=company"),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert!(
        from_b["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == company_id)
    );
    assert!(
        from_b["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["canManage"] == false)
    );
    expect(
        get(
            &other,
            &format!("/v1/spaces/{space_b}/announcements/{company_id}"),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    expect(
        get(
            &other,
            &format!(
                "/v1/spaces/{space_b}/announcements/{}",
                workspace_only["id"].as_str().unwrap()
            ),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    let denied = post(
        &other,
        &format!("/v1/spaces/{space_b}/announcements"),
        json!({"title": "전사 공지 시도", "scope": "company"}),
    )
    .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_of(denied).await["error"]["code"],
        "announcements_company_admin_required"
    );
    // Without a space path: company notices only.
    let reader = user("reader");
    let listed = expect(
        get(&reader, "/v1/announcements?scope=company").await,
        StatusCode::OK,
    )
    .await;
    assert!(
        listed["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == company_id)
    );
    expect(
        get(&reader, "/v1/announcements").await,
        StatusCode::BAD_REQUEST,
    )
    .await;
    expect(
        get(&reader, &format!("/v1/announcements/{company_id}")).await,
        StatusCode::OK,
    )
    .await;
    let denied_edit = patch(
        &reader,
        &format!("/v1/announcements/{company_id}"),
        json!({"title": "Denied edit"}),
    )
    .await;
    assert_eq!(denied_edit.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_of(denied_edit).await["error"]["code"],
        "announcements_company_admin_required"
    );
    expect(
        patch(
            &manager,
            &format!("/v1/announcements/{company_id}"),
            json!({"title": "Updated"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    // Deleting a space never removes a company notice.
    expect(
        delete(&manager, &format!("/v1/spaces/{space_a}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    expect(
        get(&reader, &format!("/v1/announcements/{company_id}")).await,
        StatusCode::OK,
    )
    .await;
    expect(
        delete(&manager, &format!("/v1/announcements/{company_id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn notices_notify_their_readers_and_platform_rechecks_follow_access() {
    let manager = make_manager("manager").await;
    let member = user("member");
    let grouped = user("grouped");
    let outsider = user("outsider");
    let team = group("team", &[&grouped]);
    let space = create_space(&manager, "마케팅").await;
    share(
        &manager,
        &space,
        json!([
            grant("user", &manager, "admin"),
            grant("user", &member, "member"),
            grant("group", &team, "member")
        ]),
    )
    .await;
    let notice = expect(
        post(
            &manager,
            &format!("/v1/spaces/{space}/announcements"),
            json!({"title": "주간 회의 안내"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let notice_id = notice["id"].as_str().unwrap().to_string();
    let key = format!("notice:{notice_id}");
    let sent = events()
        .into_iter()
        .find(|e| e["op"] == "notify" && e["body"]["groupKey"] == key)
        .unwrap();
    assert_eq!(sent["body"]["kind"], "notice");
    assert_eq!(
        sent["body"]["recipients"],
        json!([member]),
        "direct members except the author"
    );
    assert_eq!(sent["body"]["title"], "[마케팅 공지] 주간 회의 안내");
    assert_eq!(
        sent["body"]["path"],
        format!("/notices/{notice_id}?space={space}")
    );

    let company = expect(
        post(
            &manager,
            "/v1/announcements",
            json!({"title": "전사 안내", "scope": "company"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let company_id = company["id"].as_str().unwrap().to_string();
    let company_key = format!("notice:{company_id}");
    let sent: Vec<Value> = events()
        .into_iter()
        .filter(|e| e["op"] == "notify" && e["body"]["groupKey"] == company_key)
        .collect();
    let recipients: Vec<Value> = sent
        .iter()
        .flat_map(|e| e["body"]["recipients"].as_array().unwrap().clone())
        .collect();
    assert!(recipients.contains(&json!(outsider)) && recipients.contains(&json!(member)));
    assert!(!recipients.contains(&json!(manager)));
    assert!(
        sent.iter()
            .all(|e| e["body"]["recipients"].as_array().unwrap().len() <= 500)
    );
    assert_eq!(sent[0]["body"]["title"], "[전사 공지] 전사 안내");

    // A comment group for a post the person can read.
    let author = user("author");
    let created = simple_post(&author, DEFAULT, "알림 확인 글").await;
    let post_key = format!("post:{}", created["id"].as_str().unwrap());
    let items = json!({"items": [
        {"groupKey": key, "resourceId": notice_id},
        {"groupKey": company_key, "resourceId": company_id},
        {"groupKey": post_key, "resourceId": "p"},
        {"groupKey": "notice:missing", "resourceId": "m"},
        {"groupKey": "other:thing", "resourceId": "o"},
    ]});
    let allowed = |who: &str| {
        let who = who.to_string();
        let items = items.clone();
        async move {
            let response = send(
                base(),
                Some(platform_identity(&who)),
                Method::POST,
                "/platform/notification-check",
                Some(items),
            )
            .await;
            expect(response, StatusCode::OK).await["allowed"].clone()
        }
    };
    assert_eq!(allowed(&member).await, json!([key, company_key, post_key]));
    assert_eq!(allowed(&outsider).await, json!([company_key, post_key]));
    assert_eq!(
        allowed(&grouped).await,
        json!([company_key, post_key]),
        "group-only members fail closed"
    );
    let not_platform = post(&member, "/platform/notification-check", items.clone()).await;
    assert_eq!(not_platform.status(), StatusCode::FORBIDDEN);

    // Removing a member retracts the space's notices for them.
    share(&manager, &space, json!([grant("user", &manager, "admin")])).await;
    assert!(events().iter().any(|e| e["op"] == "retract"
        && e["body"]["groupKey"] == key
        && e["body"]["recipients"] == json!([member])));
    assert_eq!(allowed(&member).await, json!([company_key, post_key]));

    // Reading a notice marks its group read.
    expect(
        get(&outsider, &format!("/v1/announcements/{company_id}")).await,
        StatusCode::OK,
    )
    .await;
    assert!(events().iter().any(|e| e["op"] == "read"
        && e["by"] == outsider.as_str()
        && e["body"]["groupKey"] == company_key));
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn home_card_and_recent_notices_list_what_the_caller_may_read() {
    let manager = make_manager("manager").await;
    let member = user("member");
    let space_name = format!("홈카드-{}", &uuid::Uuid::new_v4().simple().to_string()[..6]);
    let space = create_space(&manager, &space_name).await;
    share(
        &manager,
        &space,
        json!([
            grant("user", &manager, "admin"),
            grant("user", &member, "member")
        ]),
    )
    .await;
    let pinned = expect(
        post(
            &manager,
            &format!("/v1/spaces/{space}/announcements"),
            json!({"title": "고정된 공간 공지", "isPinned": true}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let latest = expect(
        post(
            &manager,
            "/v1/announcements",
            json!({"title": "최신 전사 공지", "scope": "company"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;

    let card = expect(
        send(
            base(),
            Some(platform_identity(&member)),
            Method::POST,
            "/platform/home-card",
            Some(json!({"timeZone": "Asia/Seoul", "locale": "ko-KR"})),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(card["title"], "공지");
    let items = card["items"].as_array().unwrap();
    assert!(items.len() <= 5);
    let first_pinned = items
        .iter()
        .position(|i| i["title"] == "고정된 공간 공지")
        .unwrap();
    let company = items.iter().position(|i| i["title"] == "최신 전사 공지");
    assert!(
        company.is_none_or(|index| first_pinned < index),
        "pinned first"
    );
    assert_eq!(items[first_pinned]["subtitle"], space_name);
    assert_eq!(
        items[first_pinned]["path"],
        format!("/notices/{}?space={space}", pinned["id"].as_str().unwrap())
    );
    assert!(items[first_pinned]["at"].as_str().unwrap().ends_with('Z'));
    assert!(card.get("count").is_none());
    let outsider = user("outsider");
    let outsider_card = expect(
        send(
            base(),
            Some(platform_identity(&outsider)),
            Method::POST,
            "/platform/home-card",
            Some(json!({})),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert!(
        !outsider_card["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["title"] == "고정된 공간 공지")
    );
    expect(
        post(&member, "/platform/home-card", json!({})).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    let english = expect(
        send(
            base(),
            Some(platform_identity(&member)),
            Method::POST,
            "/platform/home-card",
            Some(json!({"locale": "en-US"})),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(english["title"], "Announcements");
    let empty_body = send(
        base(),
        Some(platform_identity(&member)),
        Method::POST,
        "/platform/home-card",
        None,
    )
    .await;
    assert_eq!(empty_body.status(), StatusCode::OK);

    let recent = expect(
        get(&member, "/v1/board/notices/recent?limit=20").await,
        StatusCode::OK,
    )
    .await;
    let items = recent["items"].as_array().unwrap();
    let company_index = items.iter().position(|i| i["id"] == latest["id"]).unwrap();
    let space_index = items.iter().position(|i| i["id"] == pinned["id"]).unwrap();
    assert!(company_index < space_index, "newest first");
    assert_eq!(items[space_index]["spaceName"], space_name);
    assert_eq!(items[company_index]["scope"], "company");
    let outsider_recent = expect(
        get(&outsider, "/v1/board/notices/recent").await,
        StatusCode::OK,
    )
    .await;
    assert!(outsider_recent["items"].as_array().unwrap().len() <= 5);
    assert!(
        !outsider_recent["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == pinned["id"])
    );
    expect(
        get(&member, "/v1/board/notices/recent?limit=21").await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
}

// ---------------------------------------------------------------- managers and identity

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn first_board_manager_is_claimed_once_and_the_last_cannot_leave() {
    // A server of its own: bootstrap needs a database without managers.
    let (url, _pool) = start_server();
    let first = user("first");
    let second = user("second");
    let call_at = |who: &str, method: Method, path: &str, body: Option<Value>| {
        let url = url.clone();
        let header = identity(who);
        let path = path.to_string();
        async move { send(&url, Some(header), method, &path, body).await }
    };
    let me = expect(
        call_at(&second, Method::GET, "/v1/board/me", None).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(me["canBootstrap"], true);
    assert_eq!(me["isManager"], false);
    expect(
        call_at(
            &second,
            Method::POST,
            "/v1/community/admin/channels",
            Some(json!({"key": "x", "name": "x"})),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    let claimed = expect(
        call_at(&first, Method::POST, "/v1/board/managers/bootstrap", None).await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(claimed["isManager"], true);
    let again = call_at(&second, Method::POST, "/v1/board/managers/bootstrap", None).await;
    assert_eq!(again.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_of(again).await["error"]["code"],
        "board_manager_exists"
    );
    let me = expect(
        call_at(&second, Method::GET, "/v1/board/me", None).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(me["canBootstrap"], false);
    expect(
        call_at(
            &second,
            Method::POST,
            "/v1/board/managers",
            Some(json!({"userId": second})),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    let last = call_at(
        &first,
        Method::DELETE,
        &format!("/v1/board/managers/{first}"),
        None,
    )
    .await;
    assert_eq!(last.status(), StatusCode::CONFLICT);
    assert_eq!(json_of(last).await["error"]["code"], "board_last_manager");
    let unknown = call_at(
        &first,
        Method::POST,
        "/v1/board/managers",
        Some(json!({"userId": "nobody"})),
    )
    .await;
    assert_eq!(unknown.status(), StatusCode::BAD_REQUEST);
    expect(
        call_at(
            &first,
            Method::POST,
            "/v1/board/managers",
            Some(json!({"userId": second})),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let managers = expect(
        call_at(&second, Method::GET, "/v1/board/managers", None).await,
        StatusCode::OK,
    )
    .await;
    let names: Vec<Value> = managers["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].clone())
        .collect();
    assert_eq!(names, vec![json!("first"), json!("second")]);
    expect(
        call_at(
            &second,
            Method::DELETE,
            &format!("/v1/board/managers/{first}"),
            None,
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    expect(
        call_at(
            &first,
            Method::POST,
            "/v1/community/admin/channels",
            Some(json!({"key": "x", "name": "x"})),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn requests_without_a_valid_gateway_identity_are_refused() {
    let missing = send(base(), None, Method::GET, "/v1/community/channels", None).await;
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        json_of(missing).await["error"]["code"],
        "invalid_gateway_identity"
    );
    let forged = encode(
        &Header::default(),
        &json!({"sub": "x", "admin": true, "installation_id": INSTALLATION, "release_id": RELEASE,
            "iss": "miy", "aud": "miy-app", "exp": exp()}),
        &EncodingKey::from_secret(b"not-the-gateway-secret-0123456789"),
    )
    .unwrap();
    let forged = send(
        base(),
        Some(forged),
        Method::GET,
        "/v1/community/channels",
        None,
    )
    .await;
    assert_eq!(forged.status(), StatusCode::UNAUTHORIZED);
    // The platform administrator flag grants nothing in the board.
    let admin_flag = token(
        json!({"sub": user("platform-admin"), "admin": true, "installation_id": INSTALLATION,
        "release_id": RELEASE, "iss": "miy", "aud": "miy-app", "exp": exp(), "environment": "dev"}),
    );
    let response = send(
        base(),
        Some(admin_flag),
        Method::GET,
        "/v1/community/admin/channels",
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_of(response).await["error"]["code"],
        "board_manager_required"
    );
}

#[test]
fn error_codes_are_safe_for_the_browser_transport() {
    for code in [
        "community_channel_not_found",
        "community_post_number_busy",
        "announcements_company_admin_required",
        "ugc_content_rejected",
        "board_manager_required",
    ] {
        assert!(code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'));
    }
}

#[tokio::test]
#[ignore = "needs BOARD_TEST_DATABASE_URL"]
async fn unconfirmed_detach_work_is_kept_and_finished_by_a_later_request() {
    let author = user("author");
    let (media_id, file_id) = upload_named_image(&author, "fail-detach.png", b"img").await;
    let created = create_post(
        &author,
        DEFAULT,
        json!({"title": "첨부", "body": format!("![a](media:{media_id})")}),
    )
    .await;
    FAIL_DETACH.store(true, std::sync::atomic::Ordering::SeqCst);
    expect(
        delete(
            &author,
            &format!("/v1/community/posts/{}", created["id"].as_str().unwrap()),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(media_link(&media_id).await.2, "detaching");
    assert_eq!(file_status(&file_id), "attached");
    FAIL_DETACH.store(false, std::sync::atomic::Ordering::SeqCst);
    let other = simple_post(&author, DEFAULT, "다른 글").await;
    expect(
        delete(
            &author,
            &format!("/v1/community/posts/{}", other["id"].as_str().unwrap()),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(media_link(&media_id).await.2, "detached");
    assert_eq!(file_status(&file_id), "cancelled");
}
