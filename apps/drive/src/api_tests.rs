//! HTTP tests through the real SDK gateway-identity check, PostgreSQL and a
//! fake core (directory, notifications, Storage with `single` and `parts`
//! transfers and download links) that the SDK calls with the signed
//! identity's delegation proof.
//!
//! Ports of AI-DO apps/api/tests/test_file_manager.py (the cases that apply
//! without RAG), plus miy additions (large transfer, versions, trash, move,
//! spaces, sharing notifications, platform endpoints).
//!
//! They need a disposable PostgreSQL server, so they are ignored by default:
//!   DRIVE_TEST_DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:PORT/postgres \
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
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

const SECRET: &str = "drive-api-test-gateway-secret-0123456789";
const INSTALLATION: &str = "00000000-0000-4000-8000-0000000000d1";
const RELEASE: &str = "00000000-0000-4000-8000-0000000000d2";
const MIB: usize = 1024 * 1024;

// ---------------------------------------------------------------- fake core

#[derive(Default)]
struct Core {
    users: Vec<(String, String)>,
    groups: Vec<String>,
    memberships: HashMap<String, Vec<String>>,
    events: Vec<Value>,
    files: HashMap<String, (Value, Vec<u8>)>,
}

static CORE: OnceLock<Mutex<Core>> = OnceLock::new();
static FAIL_DETACH: AtomicBool = AtomicBool::new(false);

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
            reqwest::Url::parse(&format!("http://x/?{query}"))
                .unwrap()
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect()
        })
        .unwrap_or_default()
}

fn json_response(status: AxumStatus, value: Value) -> AxumResponse {
    (status, axum::Json(value)).into_response()
}

fn sha(bytes: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(bytes))
}

async fn fake_core(request: Request) -> AxumResponse {
    let (parts, body) = request.into_parts();
    let Some(user) = caller(&parts.headers) else {
        return json_response(AxumStatus::UNAUTHORIZED, json!({}));
    };
    let prefix = format!("/api/v1/installations/{INSTALLATION}/");
    let Some(path) = parts.uri.path().strip_prefix(&prefix).map(str::to_string) else {
        return json_response(AxumStatus::NOT_FOUND, json!({}));
    };
    let query = query_map(parts.uri.query());
    let bytes: Bytes = axum::body::to_bytes(body, 64 * MIB).await.unwrap();
    let body_json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let segments: Vec<&str> = path.split('/').collect();
    let mut core = core();
    match (parts.method.as_str(), segments.as_slice()) {
        ("GET", ["platform", "directory"]) => {
            let kind = query.get("kind").cloned().unwrap_or_else(|| "users".into());
            let ids: Option<Vec<String>> = query
                .get("ids")
                .map(|ids| ids.split(',').map(str::to_string).collect());
            let (users, groups) = if kind == "groups" {
                (
                    vec![],
                    core.groups
                        .iter()
                        .filter(|id| ids.as_ref().is_none_or(|ids| ids.contains(id)))
                        .map(|id| json!({"id": id, "name": format!("group {id}"), "source": "local"}))
                        .collect::<Vec<_>>(),
                )
            } else if kind == "users" {
                let q = query.get("q").map(|q| q.to_lowercase());
                (
                    core.users
                        .iter()
                        .filter(|(id, name)| {
                            ids.as_ref().is_none_or(|ids| ids.contains(id))
                                && q.as_ref().is_none_or(|q| name.to_lowercase().contains(q))
                        })
                        .map(|(id, name)| json!({"id": id, "name": name, "primaryGroup": null}))
                        .collect::<Vec<_>>(),
                    vec![],
                )
            } else {
                (
                    vec![],
                    core.memberships
                        .get(&user)
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|id| ids.as_ref().is_none_or(|ids| ids.contains(id)))
                        .map(|id| json!({"id": id, "name": id, "source": "local"}))
                        .collect(),
                )
            };
            json_response(
                AxumStatus::OK,
                json!({"kind": kind, "subjectId": user, "installationId": INSTALLATION,
                    "environment": "dev", "users": users, "groups": groups, "nextCursor": null}),
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
        ("POST", ["platform", "notifications", op]) => {
            core.events
                .push(json!({"op": op, "by": user, "body": body_json}));
            json_response(AxumStatus::OK, json!({"retracted": 1}))
        }
        ("POST", ["platform", "files"]) => {
            if let Some((meta, _)) = core
                .files
                .values()
                .find(|(meta, _)| meta["requestId"] == body_json["requestId"])
            {
                return json_response(AxumStatus::OK, meta.clone());
            }
            let id = uuid::Uuid::new_v4().to_string();
            let transfer = body_json["transfer"].as_str().unwrap_or("single");
            let metadata = json!({"id": id, "version": 1, "name": body_json["name"],
                "mime": body_json["mime"], "size": body_json["size"], "sha256": null,
                "status": "pending", "reference": null, "transfer": transfer, "received": 0,
                "requestId": body_json["requestId"]});
            core.files.insert(id, (metadata.clone(), vec![]));
            json_response(AxumStatus::OK, metadata)
        }
        ("GET", ["platform", "files", "by-request", request_id]) => match core
            .files
            .values()
            .find(|(meta, _)| meta["requestId"] == *request_id)
        {
            Some((meta, _)) => json_response(AxumStatus::OK, meta.clone()),
            None => json_response(AxumStatus::NOT_FOUND, json!({})),
        },
        (method, ["platform", "files", id, rest @ ..]) => {
            let id = id.to_string();
            let Some((meta, data)) = core.files.get_mut(&id) else {
                return json_response(AxumStatus::NOT_FOUND, json!({}));
            };
            match (method, rest) {
                ("GET", []) => json_response(AxumStatus::OK, meta.clone()),
                ("PUT", ["content"]) => {
                    *data = bytes.to_vec();
                    meta["sha256"] = json!(sha(&bytes));
                    meta["received"] = json!(bytes.len());
                    meta["status"] = json!("ready");
                    json_response(AxumStatus::OK, meta.clone())
                }
                ("PUT", ["parts"]) => {
                    let range = parts.headers["content-range"].to_str().unwrap().to_string();
                    let start: usize = range
                        .strip_prefix("bytes ")
                        .unwrap()
                        .split('-')
                        .next()
                        .unwrap()
                        .parse()
                        .unwrap();
                    if start != data.len() || bytes.len() > 8 * MIB {
                        return json_response(
                            AxumStatus::CONFLICT,
                            json!({"error": {"code": "file_part_offset"}}),
                        );
                    }
                    data.extend_from_slice(&bytes);
                    meta["received"] = json!(data.len());
                    json_response(AxumStatus::OK, meta.clone())
                }
                ("POST", ["complete"]) => {
                    if meta["received"] != meta["size"] {
                        return json_response(AxumStatus::CONFLICT, json!({}));
                    }
                    if meta["status"] == "pending" {
                        meta["sha256"] = json!(sha(data));
                        meta["status"] = json!("ready");
                    }
                    json_response(AxumStatus::OK, meta.clone())
                }
                ("POST", ["attach"]) => {
                    meta["status"] = json!("attached");
                    meta["reference"] = body_json["reference"].clone();
                    json_response(AxumStatus::OK, meta.clone())
                }
                ("POST", ["detach"]) if FAIL_DETACH.load(Ordering::SeqCst) => {
                    json_response(AxumStatus::SERVICE_UNAVAILABLE, json!({}))
                }
                ("POST", ["detach"]) => {
                    if meta["reference"] != body_json["reference"] {
                        return json_response(AxumStatus::CONFLICT, json!({}));
                    }
                    meta["status"] = json!("cancelled");
                    json_response(AxumStatus::OK, meta.clone())
                }
                ("DELETE", []) => {
                    if meta["status"] == "attached" {
                        return json_response(AxumStatus::CONFLICT, json!({}));
                    }
                    meta["status"] = json!("cancelled");
                    json_response(AxumStatus::OK, meta.clone())
                }
                ("POST", ["download"]) => {
                    let reference = body_json["reference"].clone();
                    let meta = meta.clone();
                    core.events.push(
                        json!({"op": "download_link", "by": user, "file": id, "reference": reference}),
                    );
                    if meta["status"] == "attached" && meta["reference"] != reference {
                        return json_response(AxumStatus::CONFLICT, json!({}));
                    }
                    json_response(
                        AxumStatus::OK,
                        json!({"url": format!("/_miy/downloads/{}", "a".repeat(43)),
                            "expiresAt": "2026-11-22T00:05:00Z"}),
                    )
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

fn storage(storage_id: &str) -> (Value, Vec<u8>) {
    core().files[storage_id].clone()
}

/// A person in the fake directory.
fn user(name: &str) -> String {
    let id = format!("u-{}", uuid::Uuid::new_v4());
    core().users.push((id.clone(), name.to_string()));
    id
}

fn group(members: &[&str]) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let mut core = core();
    core.groups.push(id.clone());
    for member in members {
        core.memberships
            .entry(member.to_string())
            .or_default()
            .push(id.clone());
    }
    id
}

fn leave_group(member: &str, group: &str) {
    if let Some(groups) = core().memberships.get_mut(member) {
        groups.retain(|g| g != group);
    }
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
    let name = format!("drive_test_{}", uuid::Uuid::new_v4().simple());
    let mut admin = PgConnection::connect(admin_url)
        .await
        .expect("connect to DRIVE_TEST_DATABASE_URL");
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
        .max_connections(20)
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

fn start_server() -> (String, PgPool) {
    let admin_url = std::env::var("DRIVE_TEST_DATABASE_URL")
        .expect("set DRIVE_TEST_DATABASE_URL to a disposable PostgreSQL server");
    let core_origin = start_fake_core();
    let port = free_port();
    // SAFETY: the single server starts once (OnceLock) before any request.
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

fn base() -> &'static str {
    &SERVER.get_or_init(start_server).0
}

async fn db() -> PgPool {
    let options = SERVER.get_or_init(start_server).1.connect_options();
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

fn identity_with(user: &str, admin: bool) -> String {
    token(
        json!({"sub": user, "admin": admin, "installation_id": INSTALLATION,
        "release_id": RELEASE, "iss": "miy", "aud": "miy-app", "exp": exp(),
        "environment": "dev", "delegation_token": format!("proof:{user}")}),
    )
}

/// A platform call: like the core, it signs the person's current groups.
fn platform_identity(user: &str) -> String {
    let groups = core().memberships.get(user).cloned().unwrap_or_default();
    token(
        json!({"sub": user, "admin": false, "installation_id": INSTALLATION,
        "release_id": RELEASE, "iss": "miy", "aud": "miy-app", "exp": exp(),
        "environment": "dev", "platform": true, "groups": groups}),
    )
}

async fn send(header: Option<String>, method: Method, path: &str, body: Option<Value>) -> Response {
    let mut request = Client::new().request(method, format!("{}/api{path}", base()));
    if let Some(header) = header {
        request = request.header("x-miy-identity", header);
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    request.send().await.unwrap()
}

async fn send_bytes(user: &str, path: &str, bytes: Vec<u8>) -> Response {
    Client::new()
        .put(format!("{}/api{path}", base()))
        .header("x-miy-identity", identity_with(user, false))
        .header("content-type", "application/octet-stream")
        .body(bytes)
        .send()
        .await
        .unwrap()
}

async fn get(user: &str, path: &str) -> Response {
    send(Some(identity_with(user, false)), Method::GET, path, None).await
}

async fn post(user: &str, path: &str, body: Value) -> Response {
    send(
        Some(identity_with(user, false)),
        Method::POST,
        path,
        Some(body),
    )
    .await
}

async fn patch(user: &str, path: &str, body: Value) -> Response {
    send(
        Some(identity_with(user, false)),
        Method::PATCH,
        path,
        Some(body),
    )
    .await
}

async fn delete(user: &str, path: &str) -> Response {
    send(Some(identity_with(user, false)), Method::DELETE, path, None).await
}

async fn platform_post(user: &str, path: &str, body: Value) -> Response {
    send(
        Some(platform_identity(user)),
        Method::POST,
        path,
        Some(body),
    )
    .await
}

async fn expect(response: Response, status: StatusCode) -> Value {
    let actual = response.status();
    let text = response.text().await.unwrap();
    assert_eq!(actual, status, "{text}");
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

async fn error_code(response: Response, status: StatusCode) -> String {
    expect(response, status).await["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

// ---------------------------------------------------------------- helpers

async fn new_space(owner: &str, name: &str) -> String {
    let space = expect(
        post(owner, "/v1/spaces", json!({"name": name})).await,
        StatusCode::CREATED,
    )
    .await;
    space["id"].as_str().unwrap().to_string()
}

/// Replace a space's grants with the owner plus `members` (kind, id, role).
async fn share(owner: &str, space: &str, members: &[(&str, &str, &str)]) {
    let snapshot = expect(
        get(owner, &format!("/v1/spaces/{space}/sharing")).await,
        StatusCode::OK,
    )
    .await;
    let mut entries = vec![json!({"subject": {"kind": "user", "id": owner}, "role": "admin"})];
    for (kind, id, role) in members {
        entries.push(json!({"subject": {"kind": kind, "id": id}, "role": role}));
    }
    expect(
        send(
            Some(identity_with(owner, false)),
            Method::PUT,
            &format!("/v1/spaces/{space}/sharing"),
            Some(json!({"expectedRevision": snapshot["revision"],
                "requestId": uuid::Uuid::new_v4().to_string(), "entries": entries})),
        )
        .await,
        StatusCode::OK,
    )
    .await;
}

async fn new_folder(
    user: &str,
    space: &str,
    name: &str,
    parent: Option<&str>,
    visibility: &str,
) -> Value {
    expect(
        post(
            user,
            &format!("/v1/spaces/{space}/files/folders"),
            json!({"name": name, "parent_id": parent, "visibility": visibility}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await
}

async fn reserve(
    user: &str,
    space: &str,
    name: &str,
    mime: &str,
    size: usize,
    folder: Option<&str>,
    visibility: &str,
) -> Response {
    post(
        user,
        &format!("/v1/spaces/{space}/files/uploads"),
        json!({"request_id": uuid::Uuid::new_v4().to_string(), "filename": name,
            "content_type": mime, "size_bytes": size, "folder_id": folder,
            "visibility": visibility}),
    )
    .await
}

/// AI-DO `POST /files/upload` in three miy steps.
async fn upload(
    user: &str,
    space: &str,
    name: &str,
    mime: &str,
    bytes: &[u8],
    folder: Option<&str>,
    visibility: &str,
) -> Value {
    let upload = expect(
        reserve(user, space, name, mime, bytes.len(), folder, visibility).await,
        StatusCode::CREATED,
    )
    .await;
    let id = upload["id"].as_str().unwrap();
    expect(
        send_bytes(
            user,
            &format!("/files/uploads/{id}/content"),
            bytes.to_vec(),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    expect(
        post(
            user,
            &format!("/v1/spaces/{space}/files/uploads/{id}/complete"),
            json!({}),
        )
        .await,
        StatusCode::OK,
    )
    .await
}

async fn browse(user: &str, space: &str, folder: Option<&str>) -> Value {
    let path = match folder {
        Some(folder) => format!("/v1/spaces/{space}/files?folder_id={folder}"),
        None => format!("/v1/spaces/{space}/files"),
    };
    expect(get(user, &path).await, StatusCode::OK).await
}

fn ids(items: &Value) -> Vec<String> {
    items
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect()
}

async fn storage_of(file_id: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT storage_file_id::text FROM file_manager_file_versions WHERE file_id = $1 \
         AND storage_file_id IS NOT NULL ORDER BY version_number",
    )
    .bind(file_id)
    .fetch_all(&db().await)
    .await
    .unwrap()
}

// ------------------------------------------------- AI-DO test_file_manager.py

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn upload_download_and_delete() {
    let admin = user("Administrator");
    let space = new_space(&admin, "administrator").await;
    let folder = new_folder(&admin, &space, "Policies", None, "workspace").await;
    assert_eq!(folder["name"], "Policies");
    assert_eq!(folder["visibility"], "workspace");
    let folder_id = folder["id"].as_str().unwrap();

    let file = upload(
        &admin,
        &space,
        "policy.txt",
        "text/plain",
        b"plain file storage",
        Some(folder_id),
        "workspace",
    )
    .await;
    assert_eq!(file["filename"], "policy.txt");
    assert_eq!(file["folder_id"], folder_id);
    assert_eq!(file["visibility"], "workspace");
    assert_eq!(file["owner_name"], "Administrator");
    let file_id = file["id"].as_str().unwrap();

    let listing = browse(&admin, &space, Some(folder_id)).await;
    assert_eq!(listing["current_folder"]["id"], folder_id);
    assert_eq!(listing["files"][0]["id"], file_id);

    let link = expect(
        get(
            &admin,
            &format!("/v1/spaces/{space}/files/{file_id}/download"),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert!(
        link["url"]
            .as_str()
            .unwrap()
            .starts_with("/_miy/downloads/")
    );
    let storage_id = &storage_of(file_id).await[0];
    let (meta, bytes) = storage(storage_id);
    assert_eq!(bytes, b"plain file storage");
    assert_eq!(meta["status"], "attached");
    let version: String =
        sqlx::query_scalar("SELECT id FROM file_manager_file_versions WHERE file_id = $1")
            .bind(file_id)
            .fetch_one(&db().await)
            .await
            .unwrap();
    assert_eq!(meta["reference"], format!("drive:version:{version}"));

    expect(
        delete(&admin, &format!("/v1/spaces/{space}/files/{file_id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let deleted: Option<chrono::NaiveDateTime> =
        sqlx::query_scalar("SELECT deleted_at FROM file_manager_files WHERE id = $1")
            .bind(file_id)
            .fetch_one(&db().await)
            .await
            .unwrap();
    assert!(deleted.is_some());
    // In the trash, the bytes stay until a permanent delete.
    assert_eq!(storage(storage_id).0["status"], "attached");
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn image_preview_returns_inline_raster_bytes_only() {
    let admin = user("Admin");
    let space = new_space(&admin, "preview").await;
    let png = b"\x89PNG\r\n\x1a\npng-bytes";
    let file = upload(
        &admin,
        &space,
        "diagram.png",
        "image/png",
        png,
        None,
        "private",
    )
    .await;
    let response = get(
        &admin,
        &format!("/files/items/{}/preview", file["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("image/png")
    );
    assert!(
        response.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("inline")
    );
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(response.bytes().await.unwrap().as_ref(), png);

    let svg = upload(
        &admin,
        &space,
        "x.svg",
        "image/svg+xml",
        b"<svg/>",
        None,
        "private",
    )
    .await;
    let code = error_code(
        get(
            &admin,
            &format!("/files/items/{}/preview", svg["id"].as_str().unwrap()),
        )
        .await,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
    )
    .await;
    assert_eq!(code, "files_preview_unsupported_type");
    // A PNG name over non-PNG bytes is refused by the signature check.
    let fake = upload(
        &admin,
        &space,
        "fake.png",
        "image/png",
        b"<html>",
        None,
        "private",
    )
    .await;
    expect(
        get(
            &admin,
            &format!("/files/items/{}/preview", fake["id"].as_str().unwrap()),
        )
        .await,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
    )
    .await;
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn workspace_shared_file_is_readable_but_not_deletable_by_member() {
    let admin = user("Admin");
    let member = user("Member");
    let space = new_space(&admin, "shared").await;
    share(&admin, &space, &[("user", &member, "member")]).await;
    let file = upload(
        &admin,
        &space,
        "shared.txt",
        "text/plain",
        b"shared",
        None,
        "workspace",
    )
    .await;
    let listing = browse(&member, &space, None).await;
    assert!(ids(&listing["files"]).contains(&file["id"].as_str().unwrap().to_string()));
    assert_eq!(listing["files"][0]["can_delete"], false);
    let code = error_code(
        delete(
            &member,
            &format!("/v1/spaces/{space}/files/{}", file["id"].as_str().unwrap()),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_eq!(code, "files_delete_access_required");
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn child_folder_and_upload_inherit_parent_visibility() {
    let admin = user("Admin");
    let space = new_space(&admin, "inherit").await;
    let parent = new_folder(&admin, &space, "Shared root", None, "workspace").await;
    assert_eq!(parent["visibility"], "workspace");
    let child = new_folder(&admin, &space, "Child", parent["id"].as_str(), "private").await;
    assert_eq!(child["visibility"], "workspace");
    let file = upload(
        &admin,
        &space,
        "inherited.txt",
        "text/plain",
        b"inherited",
        child["id"].as_str(),
        "private",
    )
    .await;
    assert_eq!(file["visibility"], "workspace");
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn folder_visibility_changes_are_rejected() {
    let admin = user("Admin");
    let space = new_space(&admin, "stable").await;
    let folder = new_folder(&admin, &space, "Stable visibility", None, "private").await;
    let code = error_code(
        patch(
            &admin,
            &format!(
                "/v1/spaces/{space}/files/folders/{}",
                folder["id"].as_str().unwrap()
            ),
            json!({"visibility": "workspace"}),
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    assert_eq!(code, "files_visibility_change_not_allowed");
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn private_file_is_hidden_from_workspace_member() {
    let admin = user("Admin");
    let member = user("Viewer");
    let space = new_space(&admin, "private").await;
    share(&admin, &space, &[("user", &member, "member")]).await;
    let file = upload(
        &admin,
        &space,
        "private.txt",
        "text/plain",
        b"private",
        None,
        "private",
    )
    .await;
    let file_id = file["id"].as_str().unwrap();
    assert!(!ids(&browse(&member, &space, None).await["files"]).contains(&file_id.to_string()));
    let code = error_code(
        get(
            &member,
            &format!("/v1/spaces/{space}/files/{file_id}/download"),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_eq!(code, "files_file_access_required");
    // A file in someone's private folder is hidden with it.
    let folder = new_folder(&admin, &space, "Mine", None, "private").await;
    let inside = upload(
        &admin,
        &space,
        "inside.txt",
        "text/plain",
        b"x",
        folder["id"].as_str(),
        "workspace",
    )
    .await;
    expect(
        get(
            &member,
            &format!(
                "/v1/spaces/{space}/files/{}/download",
                inside["id"].as_str().unwrap()
            ),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    // The space admin sees private items (AI-DO workspace admin).
    assert!(ids(&browse(&admin, &space, None).await["files"]).contains(&file_id.to_string()));
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn download_access_is_rechecked_when_the_link_is_used() {
    let admin = user("Admin");
    let member = user("Member");
    let space = new_space(&admin, "revocable").await;
    share(&admin, &space, &[("user", &member, "member")]).await;
    let file = upload(
        &admin,
        &space,
        "revocable.txt",
        "text/plain",
        b"revocable",
        None,
        "workspace",
    )
    .await;
    let file_id = file["id"].as_str().unwrap();
    expect(
        get(
            &member,
            &format!("/v1/spaces/{space}/files/{file_id}/download"),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let link = events()
        .into_iter()
        .rev()
        .find(|e| e["op"] == "download_link" && e["by"] == member)
        .unwrap();
    let request = json!({"fileId": link["file"], "reference": link["reference"]});
    let allowed = expect(
        platform_post(&member, "/v1/file-access", request.clone()).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(allowed["allowed"], true);
    // fileAccess is a platform call only.
    expect(
        post(&member, "/v1/file-access", request.clone()).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    // Another reference or another person is refused.
    let wrong = json!({"fileId": link["file"], "reference": "drive:version:other"});
    assert_eq!(
        expect(
            platform_post(&member, "/v1/file-access", wrong).await,
            StatusCode::OK
        )
        .await["allowed"],
        false
    );
    let stranger = user("Stranger");
    assert_eq!(
        expect(
            platform_post(&stranger, "/v1/file-access", request.clone()).await,
            StatusCode::OK
        )
        .await["allowed"],
        false
    );

    // AI-DO deleted the workspace binding; miy takes the member off the space.
    share(&admin, &space, &[]).await;
    let allowed = expect(
        platform_post(&member, "/v1/file-access", request.clone()).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(allowed["allowed"], false);
    // A trashed file is no longer downloadable even for its owner.
    let owner_request = request.clone();
    assert_eq!(
        expect(
            platform_post(&admin, "/v1/file-access", owner_request.clone()).await,
            StatusCode::OK
        )
        .await["allowed"],
        true
    );
    expect(
        delete(&admin, &format!("/v1/spaces/{space}/files/{file_id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(
        expect(
            platform_post(&admin, "/v1/file-access", owner_request).await,
            StatusCode::OK
        )
        .await["allowed"],
        false
    );
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn archive_plan_and_bulk_delete_selected_items() {
    let admin = user("Admin");
    let space = new_space(&admin, "archive").await;
    let folder = new_folder(&admin, &space, "Project", None, "private").await;
    let folder_id = folder["id"].as_str().unwrap();
    let root = upload(
        &admin,
        &space,
        "root.txt",
        "text/plain",
        b"root",
        None,
        "private",
    )
    .await;
    let nested = upload(
        &admin,
        &space,
        "nested.txt",
        "text/plain",
        b"nested",
        Some(folder_id),
        "private",
    )
    .await;
    let plan = expect(
        post(
            &admin,
            &format!("/v1/spaces/{space}/files/archive"),
            json!({"file_ids": [root["id"]], "folder_ids": [folder_id]}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let paths: Vec<&str> = plan["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, vec!["Project/", "Project/nested.txt", "root.txt"]);
    assert_eq!(plan["file_count"], 2);
    assert_eq!(plan["total_size"], 10);

    expect(
        post(
            &admin,
            &format!("/v1/spaces/{space}/files/bulk-delete"),
            json!({"file_ids": [root["id"]], "folder_ids": [folder_id]}),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    for id in [root["id"].as_str().unwrap(), nested["id"].as_str().unwrap()] {
        let deleted: Option<chrono::NaiveDateTime> =
            sqlx::query_scalar("SELECT deleted_at FROM file_manager_files WHERE id = $1")
                .bind(id)
                .fetch_one(&db().await)
                .await
                .unwrap();
        assert!(deleted.is_some());
    }
    let code = error_code(
        post(
            &admin,
            &format!("/v1/spaces/{space}/files/bulk-delete"),
            json!({}),
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    assert_eq!(code, "files_selection_required");
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn upload_rejects_files_over_the_size_limit() {
    let admin = user("Admin");
    let space = new_space(&admin, "limit").await;
    let code = error_code(
        reserve(
            &admin,
            &space,
            "too-big.bin",
            "application/octet-stream",
            2 * 1024 * MIB + 1,
            None,
            "private",
        )
        .await,
        StatusCode::PAYLOAD_TOO_LARGE,
    )
    .await;
    assert_eq!(code, "files_file_size_limit_exceeded");
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM file_manager_files WHERE filename = 'too-big.bin'",
    )
    .fetch_one(&db().await)
    .await
    .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn archive_sanitizes_windows_paths() {
    let admin = user("Admin");
    let space = new_space(&admin, "sanitize").await;
    let file = upload(
        &admin,
        &space,
        "..\\evil.txt",
        "text/plain",
        b"safe",
        None,
        "private",
    )
    .await;
    assert_eq!(file["filename"], "evil.txt");
    let plan = expect(
        post(
            &admin,
            &format!("/v1/spaces/{space}/files/archive"),
            json!({"file_ids": [file["id"]]}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(plan["entries"][0]["path"], "evil.txt");
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn workspace_member_cannot_write_inside_owner_shared_folder() {
    let admin = user("Admin");
    let member = user("Member");
    let viewer = user("Viewer");
    let space = new_space(&admin, "writes").await;
    share(
        &admin,
        &space,
        &[("user", &member, "member"), ("user", &viewer, "viewer")],
    )
    .await;
    let folder = new_folder(&admin, &space, "Shared owner folder", None, "workspace").await;
    let folder_id = folder["id"].as_str().unwrap();
    let code = error_code(
        post(
            &member,
            &format!("/v1/spaces/{space}/files/folders"),
            json!({"name": "Member child", "parent_id": folder_id, "visibility": "private"}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_eq!(code, "files_parent_manage_access_required");
    let code = error_code(
        reserve(
            &member,
            &space,
            "member.txt",
            "text/plain",
            6,
            Some(folder_id),
            "private",
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_eq!(code, "files_parent_manage_access_required");
    let listing = browse(&member, &space, Some(folder_id)).await;
    assert_eq!(listing["can_ingest"], false);
    assert_eq!(listing["ingest_block_reason"], "parent_manage_required");
    // A viewer (AI-DO workspace viewer) adds nothing, even at the root.
    let code = error_code(
        reserve(
            &viewer,
            &space,
            "viewer.txt",
            "text/plain",
            6,
            None,
            "private",
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_eq!(code, "files_workspace_access_required");
    assert_eq!(
        browse(&viewer, &space, None).await["ingest_block_reason"],
        "workspace_access_required"
    );
    // Members add at the root.
    upload(
        &member,
        &space,
        "root.txt",
        "text/plain",
        b"ok",
        None,
        "private",
    )
    .await;
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn permanent_delete_releases_storage_after_commit_and_retries_failures() {
    let admin = user("Admin");
    let space = new_space(&admin, "cleanup").await;
    let file = upload(
        &admin,
        &space,
        "cleanup.txt",
        "text/plain",
        b"cleanup",
        None,
        "private",
    )
    .await;
    let file_id = file["id"].as_str().unwrap();
    let storage_id = storage_of(file_id).await[0].clone();
    expect(
        delete(&admin, &format!("/v1/spaces/{space}/files/{file_id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let trash = expect(
        get(&admin, &format!("/v1/spaces/{space}/trash")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(trash["items"][0]["id"], file_id);

    FAIL_DETACH.store(true, Ordering::SeqCst);
    expect(
        post(
            &admin,
            &format!("/v1/spaces/{space}/trash/delete"),
            json!({"file_ids": [file_id]}),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    FAIL_DETACH.store(false, Ordering::SeqCst);
    // The app rows are gone; the Core file waits in a cleanup job.
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM file_manager_files WHERE id = $1")
        .bind(file_id)
        .fetch_one(&db().await)
        .await
        .unwrap();
    assert_eq!(rows, 0);
    assert_eq!(storage(&storage_id).0["status"], "attached");
    let attempts: i32 = sqlx::query_scalar(
        "SELECT attempts FROM file_manager_storage_cleanup_jobs WHERE storage_file_id = $1::uuid",
    )
    .bind(&storage_id)
    .fetch_one(&db().await)
    .await
    .unwrap();
    assert_eq!(attempts, 1);
    // A later trash request retries due jobs.
    sqlx::query(
        "UPDATE file_manager_storage_cleanup_jobs SET next_retry_at = now() - interval '1 hour'",
    )
    .execute(&db().await)
    .await
    .unwrap();
    expect(
        post(
            &admin,
            &format!("/v1/spaces/{space}/trash/empty"),
            json!({}),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    assert_eq!(storage(&storage_id).0["status"], "cancelled");
    let jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM file_manager_storage_cleanup_jobs WHERE storage_file_id = $1::uuid",
    )
    .bind(&storage_id)
    .fetch_one(&db().await)
    .await
    .unwrap();
    assert_eq!(jobs, 0);
}

// ------------------------------------------------------------ miy additions

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn large_upload_sends_parts_from_the_committed_offset() {
    let owner = user("Owner");
    let space = new_space(&owner, "large").await;
    let size = 8 * MIB * 2 + 1234;
    let bytes: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
    let upload = expect(
        reserve(
            &owner,
            &space,
            "big.bin",
            "application/octet-stream",
            size,
            None,
            "private",
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(upload["transfer"], "parts");
    assert_eq!(upload["part_size"], 8 * MIB);
    let id = upload["id"].as_str().unwrap();
    let part = |offset: usize| bytes[offset..(offset + 8 * MIB).min(size)].to_vec();

    let first = expect(
        send_bytes(
            &owner,
            &format!("/files/uploads/{id}/parts?offset=0"),
            part(0),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(first["received"], 8 * MIB);
    // A repeat at the old offset (a lost answer) is refused; the state tells where to resume.
    let code = error_code(
        send_bytes(
            &owner,
            &format!("/files/uploads/{id}/parts?offset=0"),
            part(0),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(code, "files_part_offset");
    let state = expect(
        get(&owner, &format!("/v1/spaces/{space}/files/uploads/{id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(state["received"], 8 * MIB);
    // Only the uploader sends bytes.
    let other = user("Other");
    expect(
        send_bytes(
            &other,
            &format!("/files/uploads/{id}/parts?offset={}", 8 * MIB),
            vec![0; 16],
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    // Completing early is refused.
    expect(
        post(
            &owner,
            &format!("/v1/spaces/{space}/files/uploads/{id}/complete"),
            json!({}),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
    // A part over 8 MiB is refused (the server may close before reading it all).
    let oversized = Client::new()
        .put(format!(
            "{}/api/files/uploads/{id}/parts?offset={}",
            base(),
            8 * MIB
        ))
        .header("x-miy-identity", identity_with(&owner, false))
        .body(vec![0; 8 * MIB + 1])
        .send()
        .await;
    if let Ok(response) = oversized {
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
    let state = expect(
        get(&owner, &format!("/v1/spaces/{space}/files/uploads/{id}")).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(state["received"], 8 * MIB);
    for offset in [8 * MIB, 16 * MIB] {
        expect(
            send_bytes(
                &owner,
                &format!("/files/uploads/{id}/parts?offset={offset}"),
                part(offset),
            )
            .await,
            StatusCode::OK,
        )
        .await;
    }
    let file = expect(
        post(
            &owner,
            &format!("/v1/spaces/{space}/files/uploads/{id}/complete"),
            json!({}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(file["size_bytes"], size);
    // Completing again is safe.
    let again = expect(
        post(
            &owner,
            &format!("/v1/spaces/{space}/files/uploads/{id}/complete"),
            json!({}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(again["id"], file["id"]);
    let storage_id = &storage_of(file["id"].as_str().unwrap()).await[0];
    let (meta, stored) = storage(storage_id);
    assert_eq!(meta["status"], "attached");
    assert_eq!(meta["sha256"], sha(&bytes));
    assert!(stored == bytes);
    let sha_row: String =
        sqlx::query_scalar("SELECT sha256 FROM file_manager_file_versions WHERE file_id = $1")
            .bind(file["id"].as_str().unwrap())
            .fetch_one(&db().await)
            .await
            .unwrap();
    assert_eq!(sha_row, sha(&bytes));
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn upload_is_recovered_by_its_request_id_and_cancel_releases_it() {
    let owner = user("Owner");
    let space = new_space(&owner, "recover").await;
    let request_id = uuid::Uuid::new_v4().to_string();
    let body = json!({"request_id": request_id, "filename": "a.txt", "content_type": "text/plain",
        "size_bytes": 3, "visibility": "private"});
    let first = expect(
        post(
            &owner,
            &format!("/v1/spaces/{space}/files/uploads"),
            body.clone(),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    let again = expect(
        post(
            &owner,
            &format!("/v1/spaces/{space}/files/uploads"),
            body.clone(),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(first["id"], again["id"]);
    // Another person cannot take over the request ID.
    let other = user("Other");
    expect(
        post(&other, &format!("/v1/spaces/{space}/files/uploads"), body).await,
        StatusCode::NOT_FOUND,
    )
    .await;
    // An unfinished upload is not a file yet.
    assert!(
        browse(&owner, &space, None).await["files"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let id = first["id"].as_str().unwrap();
    expect(
        delete(&owner, &format!("/v1/spaces/{space}/files/uploads/{id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let storage_id: String = sqlx::query_scalar(
        "SELECT storage_file_id::text FROM file_manager_file_versions WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&db().await)
    .await
    .unwrap();
    assert_eq!(storage(&storage_id).0["status"], "cancelled");
    expect(
        send_bytes(
            &owner,
            &format!("/files/uploads/{id}/content"),
            b"abc".to_vec(),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
    // A completed upload cannot be cancelled.
    let file = upload(
        &owner,
        &space,
        "done.txt",
        "text/plain",
        b"done",
        None,
        "private",
    )
    .await;
    let version: String =
        sqlx::query_scalar("SELECT id FROM file_manager_file_versions WHERE file_id = $1")
            .bind(file["id"].as_str().unwrap())
            .fetch_one(&db().await)
            .await
            .unwrap();
    let code = error_code(
        delete(
            &owner,
            &format!("/v1/spaces/{space}/files/uploads/{version}"),
        )
        .await,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(code, "files_upload_completed");
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn new_versions_keep_history_and_download_by_version() {
    let owner = user("Owner");
    let member = user("Member");
    let space = new_space(&owner, "versions").await;
    share(&owner, &space, &[("user", &member, "member")]).await;
    let file = upload(
        &owner,
        &space,
        "plan.txt",
        "text/plain",
        b"v1",
        None,
        "workspace",
    )
    .await;
    let file_id = file["id"].as_str().unwrap();
    // Only the owner (or a space admin) uploads a new version.
    let body = json!({"request_id": uuid::Uuid::new_v4().to_string(), "filename": "plan-v2.txt",
        "content_type": "text/plain", "size_bytes": 4, "file_id": file_id});
    expect(
        post(
            &member,
            &format!("/v1/spaces/{space}/files/uploads"),
            body.clone(),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    let upload_v2 = expect(
        post(&owner, &format!("/v1/spaces/{space}/files/uploads"), body).await,
        StatusCode::CREATED,
    )
    .await;
    let id = upload_v2["id"].as_str().unwrap();
    expect(
        send_bytes(
            &owner,
            &format!("/files/uploads/{id}/content"),
            b"v2v2".to_vec(),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let updated = expect(
        post(
            &owner,
            &format!("/v1/spaces/{space}/files/uploads/{id}/complete"),
            json!({}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(updated["id"], file_id);
    assert_eq!(updated["filename"], "plan.txt");
    assert_eq!(updated["size_bytes"], 4);
    assert_eq!(updated["version_number"], 2);
    assert_eq!(updated["version_count"], 2);
    let versions = expect(
        get(
            &member,
            &format!("/v1/spaces/{space}/files/{file_id}/versions"),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let list = versions["versions"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["version_number"], 2);
    assert_eq!(list[0]["is_current"], true);
    assert_eq!(list[1]["filename"], "plan.txt");
    let old = list[1]["id"].as_str().unwrap();
    expect(
        get(
            &member,
            &format!("/v1/spaces/{space}/files/{file_id}/versions/{old}/download"),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let storages = storage_of(file_id).await;
    assert_eq!(storage(&storages[0]).1, b"v1");
    assert_eq!(storage(&storages[1]).1, b"v2v2");
    // Permanently deleting the file releases every version.
    expect(
        delete(&owner, &format!("/v1/spaces/{space}/files/{file_id}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    expect(
        post(
            &owner,
            &format!("/v1/spaces/{space}/trash/delete"),
            json!({"file_ids": [file_id]}),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    for storage_id in storages {
        assert_eq!(storage(&storage_id).0["status"], "cancelled");
    }
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn trash_restores_items_to_their_folder_or_the_root() {
    let owner = user("Owner");
    let space = new_space(&owner, "trash").await;
    let folder = new_folder(&owner, &space, "Docs", None, "private").await;
    let folder_id = folder["id"].as_str().unwrap();
    let inner = upload(
        &owner,
        &space,
        "inner.txt",
        "text/plain",
        b"in",
        Some(folder_id),
        "private",
    )
    .await;
    let loose = upload(
        &owner,
        &space,
        "loose.txt",
        "text/plain",
        b"lo",
        Some(folder_id),
        "private",
    )
    .await;
    // Trash one file first, then its folder.
    expect(
        delete(
            &owner,
            &format!("/v1/spaces/{space}/files/{}", loose["id"].as_str().unwrap()),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    expect(
        delete(
            &owner,
            &format!("/v1/spaces/{space}/files/folders/{folder_id}"),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let trash = expect(
        get(&owner, &format!("/v1/spaces/{space}/trash")).await,
        StatusCode::OK,
    )
    .await;
    let items = trash["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let folder_item = items.iter().find(|i| i["kind"] == "folder").unwrap();
    assert_eq!(folder_item["file_count"], 1);
    let loose_item = items.iter().find(|i| i["kind"] == "file").unwrap();
    assert_eq!(loose_item["original_path"], "Docs");
    assert!(
        browse(&owner, &space, None).await["folders"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // The loose file's folder is in the trash: it comes back at the root.
    expect(
        post(
            &owner,
            &format!("/v1/spaces/{space}/trash/restore"),
            json!({"file_ids": [loose["id"]]}),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let root = browse(&owner, &space, None).await;
    assert!(ids(&root["files"]).contains(&loose["id"].as_str().unwrap().to_string()));
    // The folder comes back with its file.
    expect(
        post(
            &owner,
            &format!("/v1/spaces/{space}/trash/restore"),
            json!({"folder_ids": [folder_id]}),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let docs = browse(&owner, &space, Some(folder_id)).await;
    assert_eq!(
        ids(&docs["files"]),
        vec![inner["id"].as_str().unwrap().to_string()]
    );
    // Restoring something not in the trash is refused.
    let code = error_code(
        post(
            &owner,
            &format!("/v1/spaces/{space}/trash/restore"),
            json!({"folder_ids": [folder_id]}),
        )
        .await,
        StatusCode::NOT_FOUND,
    )
    .await;
    assert_eq!(code, "files_trash_item_not_found");
    // A member sees only their own trashed items.
    let member = user("Member");
    share(&owner, &space, &[("user", &member, "member")]).await;
    expect(
        delete(
            &owner,
            &format!("/v1/spaces/{space}/files/{}", inner["id"].as_str().unwrap()),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let theirs = expect(
        get(&member, &format!("/v1/spaces/{space}/trash")).await,
        StatusCode::OK,
    )
    .await;
    assert!(theirs["items"].as_array().unwrap().is_empty());
    expect(
        post(
            &member,
            &format!("/v1/spaces/{space}/trash/restore"),
            json!({"file_ids": [inner["id"]]}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn rename_and_move_follow_the_manage_and_upload_rules() {
    let owner = user("Owner");
    let member = user("Member");
    let space = new_space(&owner, "move").await;
    share(&owner, &space, &[("user", &member, "member")]).await;
    let a = new_folder(&owner, &space, "A", None, "workspace").await;
    let b = new_folder(&owner, &space, "B", a["id"].as_str(), "workspace").await;
    let file = upload(
        &owner,
        &space,
        "report.txt",
        "text/plain",
        b"r",
        None,
        "workspace",
    )
    .await;
    let file_id = file["id"].as_str().unwrap();
    let renamed = expect(
        patch(
            &owner,
            &format!("/v1/spaces/{space}/files/{file_id}"),
            json!({"filename": "a/b\\final.txt"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(renamed["filename"], "final.txt");
    let moved = expect(
        patch(
            &owner,
            &format!("/v1/spaces/{space}/files/{file_id}"),
            json!({"folder_id": b["id"]}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(moved["folder_id"], b["id"]);
    // Back to the root with an explicit null.
    let back = expect(
        patch(
            &owner,
            &format!("/v1/spaces/{space}/files/{file_id}"),
            json!({"folder_id": null}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(back["folder_id"], Value::Null);
    // A folder cannot move under itself.
    let code = error_code(
        patch(
            &owner,
            &format!(
                "/v1/spaces/{space}/files/folders/{}",
                a["id"].as_str().unwrap()
            ),
            json!({"parent_id": b["id"]}),
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    assert_eq!(code, "files_move_into_itself");
    // A member moves neither someone else's file nor into someone else's folder.
    expect(
        patch(
            &member,
            &format!("/v1/spaces/{space}/files/{file_id}"),
            json!({"filename": "x.txt"}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    let mine = upload(
        &member,
        &space,
        "mine.txt",
        "text/plain",
        b"m",
        None,
        "workspace",
    )
    .await;
    let code = error_code(
        post(
            &member,
            &format!("/v1/spaces/{space}/files/move"),
            json!({"file_ids": [mine["id"]], "target_folder_id": a["id"]}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    assert_eq!(code, "files_parent_manage_access_required");
    // The owner moves a selection.
    expect(
        post(&owner, &format!("/v1/spaces/{space}/files/move"),
            json!({"file_ids": [file_id, mine["id"]], "folder_ids": [b["id"]], "target_folder_id": null})).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let root = browse(&owner, &space, None).await;
    assert_eq!(ids(&root["folders"]).len(), 2);
    assert_eq!(ids(&root["files"]).len(), 2);
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn name_and_metadata_search_returns_only_readable_files() {
    let owner = user("Kim Minji");
    let member = user("Lee");
    let space = new_space(&owner, "search").await;
    share(&owner, &space, &[("user", &member, "member")]).await;
    let folder = new_folder(&owner, &space, "Budget", None, "workspace").await;
    upload(
        &owner,
        &space,
        "Release plan 2026.pdf",
        "application/pdf",
        b"p",
        folder["id"].as_str(),
        "workspace",
    )
    .await;
    upload(
        &owner,
        &space,
        "release-private.txt",
        "text/plain",
        b"p",
        None,
        "private",
    )
    .await;
    upload(
        &owner,
        &space,
        "notes.txt",
        "text/plain",
        b"n",
        None,
        "workspace",
    )
    .await;
    let search = |user: String, body: Value| {
        let space = space.clone();
        async move { post(&user, &format!("/v1/spaces/{space}/files/search"), body).await }
    };
    let result = expect(
        search(
            member.clone(),
            json!({"query": "release", "strategy": "keyword", "page": 1, "page_size": 20}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let hits = result["hits"].as_array().unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["filename"], "Release plan 2026.pdf");
    assert_eq!(
        hits[0]["snippet"]["highlights"][0],
        json!({"start": 0, "end": 7})
    );
    assert_eq!(result["strategy"], "keyword");
    // The owner also finds their private file; metadata (folder, owner) matches too.
    let own = expect(
        search(
            owner.clone(),
            json!({"query": "release", "strategy": "keyword"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(own["hits"].as_array().unwrap().len(), 2);
    let by_folder = expect(
        search(
            member.clone(),
            json!({"query": "budget", "strategy": "keyword"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        by_folder["hits"][0]["methods"],
        json!(["filename", "metadata"])
    );
    let by_owner = expect(
        search(
            member.clone(),
            json!({"query": "minji notes", "strategy": "keyword"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(by_owner["hits"][0]["filename"], "notes.txt");
    // Content/semantic search is not available; unknown fields are refused like AI-DO.
    let code = error_code(
        search(
            member.clone(),
            json!({"query": "x", "strategy": "semantic"}),
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    assert_eq!(code, "files_search_strategy_unsupported");
    expect(
        search(
            member.clone(),
            json!({"query": "x", "strategy": "keyword", "bogus": 1}),
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;
    expect(
        search(
            member.clone(),
            json!({"query": "x", "strategy": "keyword", "page": 6, "page_size": 20}),
        )
        .await,
        StatusCode::UNPROCESSABLE_ENTITY,
    )
    .await;

    // The portal's integrated search: platform calls only, readable files only.
    expect(
        post(
            &member,
            "/v1/search",
            json!({"query": "release", "limit": 5}),
        )
        .await,
        StatusCode::FORBIDDEN,
    )
    .await;
    let portal = expect(
        platform_post(
            &member,
            "/v1/search",
            json!({"query": "release", "limit": 5, "locale": "ko-KR"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let items = portal["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Release plan 2026.pdf");
    assert_eq!(items[0]["kind"], "파일");
    assert!(
        items[0]["path"]
            .as_str()
            .unwrap()
            .starts_with(&format!("/spaces/{space}?folder="))
    );
    let stranger = user("Stranger");
    let none = expect(
        platform_post(
            &stranger,
            "/v1/search",
            json!({"query": "release", "limit": 5}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert!(none["items"].as_array().unwrap().is_empty());
    let escaped = expect(
        platform_post(&member, "/v1/search", json!({"query": "%", "limit": 5})).await,
        StatusCode::OK,
    )
    .await;
    assert!(escaped["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn sharing_a_space_notifies_and_retracts_and_is_rechecked() {
    let owner = user("Owner Kim");
    let member = user("Member");
    let space = new_space(&owner, "Design").await;
    share(&owner, &space, &[("user", &member, "viewer")]).await;
    let notified = events()
        .into_iter()
        .find(|e| e["op"] == "notify" && e["body"]["groupKey"] == format!("space:{space}"))
        .expect("share notification");
    assert_eq!(notified["body"]["kind"], "share");
    assert_eq!(notified["body"]["recipients"], json!([member]));
    assert_eq!(
        notified["body"]["title"],
        "Owner Kim님이 \"Design\" 공간을 공유했습니다"
    );
    assert_eq!(notified["body"]["path"], format!("/spaces/{space}"));
    let check = json!({"items": [{"groupKey": format!("space:{space}"), "resourceId": space},
        {"groupKey": "space:other", "resourceId": space}]});
    let allowed = expect(
        platform_post(&member, "/platform/notification-check", check.clone()).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(allowed["allowed"], json!([format!("space:{space}")]));
    expect(
        post(&member, "/platform/notification-check", check.clone()).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    // Opening the space marks it read (after the answer).
    browse(&member, &space, None).await;
    let mut read = false;
    for _ in 0..100 {
        read = events()
            .iter()
            .any(|e| e["op"] == "read" && e["by"] == member);
        if read {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(read);
    share(&owner, &space, &[]).await;
    let retracted = events()
        .into_iter()
        .find(|e| e["op"] == "retract" && e["body"]["groupKey"] == format!("space:{space}"))
        .expect("retraction");
    assert_eq!(retracted["body"]["recipients"], json!([member]));
    let allowed = expect(
        platform_post(&member, "/platform/notification-check", check).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(allowed["allowed"], json!([]));
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn group_grants_apply_and_the_admin_flag_grants_nothing() {
    let owner = user("Owner");
    let teammate = user("Teammate");
    let team = group(&[&teammate]);
    let space = new_space(&owner, "team").await;
    share(&owner, &space, &[("group", &team, "member")]).await;
    let file = upload(
        &owner,
        &space,
        "team.txt",
        "text/plain",
        b"t",
        None,
        "workspace",
    )
    .await;
    let file_id = file["id"].as_str().unwrap();
    let spaces = expect(get(&teammate, "/v1/spaces").await, StatusCode::OK).await;
    assert_eq!(spaces["spaces"][0]["role"], "member");
    assert!(ids(&browse(&teammate, &space, None).await["files"]).contains(&file_id.to_string()));
    expect(
        get(
            &teammate,
            &format!("/v1/spaces/{space}/files/{file_id}/download"),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let link = events()
        .into_iter()
        .rev()
        .find(|e| e["op"] == "download_link" && e["by"] == teammate)
        .unwrap();
    let request = json!({"fileId": link["file"], "reference": link["reference"]});
    // The download check uses the current groups the platform call carries.
    assert_eq!(
        expect(
            platform_post(&teammate, "/v1/file-access", request.clone()).await,
            StatusCode::OK
        )
        .await["allowed"],
        true
    );
    // The portal search and notification check see group shares too.
    let portal = expect(
        platform_post(
            &teammate,
            "/v1/search",
            json!({"query": "team", "limit": 5}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(portal["items"][0]["title"], "team.txt");
    let notified = events()
        .into_iter()
        .find(|e| e["op"] == "notify" && e["body"]["groupKey"] == format!("space:{space}"))
        .expect("group share notification");
    assert_eq!(notified["body"]["recipientGroups"], json!([team]));
    assert_eq!(notified["body"]["recipients"], json!([]));
    let check = json!({"items": [{"groupKey": format!("space:{space}"), "resourceId": space}]});
    let allowed = expect(
        platform_post(&teammate, "/platform/notification-check", check.clone()).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(allowed["allowed"], json!([format!("space:{space}")]));
    leave_group(&teammate, &team);
    expect(
        get(&teammate, &format!("/v1/spaces/{space}/files")).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    let allowed = expect(
        platform_post(&teammate, "/platform/notification-check", check).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(allowed["allowed"], json!([]));
    assert_eq!(
        expect(
            platform_post(&teammate, "/v1/file-access", request).await,
            StatusCode::OK
        )
        .await["allowed"],
        false
    );

    // A platform administrator flag opens nothing.
    let admin = user("Platform admin");
    let response = send(
        Some(identity_with(&admin, true)),
        Method::GET,
        &format!("/v1/spaces/{space}/files"),
        None,
    )
    .await;
    assert_eq!(
        error_code(response, StatusCode::FORBIDDEN).await,
        "acl_denied"
    );
    let listed = expect(
        send(
            Some(identity_with(&admin, true)),
            Method::GET,
            "/v1/spaces",
            None,
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert!(listed["spaces"].as_array().unwrap().is_empty());
    // No identity, no access.
    assert_eq!(
        send(None, Method::GET, "/v1/spaces", None).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
#[ignore = "needs DRIVE_TEST_DATABASE_URL"]
async fn deleting_a_space_releases_its_files_after_commit() {
    let owner = user("Owner");
    let member = user("Member");
    let space = new_space(&owner, "doomed").await;
    share(&owner, &space, &[("user", &member, "admin")]).await;
    let file = upload(&owner, &space, "a.txt", "text/plain", b"a", None, "private").await;
    let trashed = upload(&owner, &space, "b.txt", "text/plain", b"b", None, "private").await;
    expect(
        delete(
            &owner,
            &format!(
                "/v1/spaces/{space}/files/{}",
                trashed["id"].as_str().unwrap()
            ),
        )
        .await,
        StatusCode::NO_CONTENT,
    )
    .await;
    let storages = [
        storage_of(file["id"].as_str().unwrap()).await[0].clone(),
        storage_of(trashed["id"].as_str().unwrap()).await[0].clone(),
    ];
    let viewer = user("Viewer");
    share(
        &owner,
        &space,
        &[("user", &member, "admin"), ("user", &viewer, "viewer")],
    )
    .await;
    expect(
        delete(&viewer, &format!("/v1/spaces/{space}")).await,
        StatusCode::FORBIDDEN,
    )
    .await;
    expect(
        delete(&member, &format!("/v1/spaces/{space}")).await,
        StatusCode::NO_CONTENT,
    )
    .await;
    for storage_id in &storages {
        assert_eq!(storage(storage_id).0["status"], "cancelled");
    }
    expect(
        get(&owner, &format!("/v1/spaces/{space}/files")).await,
        StatusCode::NOT_FOUND,
    )
    .await;
    let grants: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.miy_acl_grants WHERE resource_id = $1")
            .bind(&space)
            .fetch_one(&db().await)
            .await
            .unwrap();
    assert_eq!(grants, 0);
    assert!(events().iter().any(|e| e["op"] == "retract"
        && e["body"]["groupKey"] == format!("space:{space}")
        && e["body"].get("recipients").is_none()));
}
