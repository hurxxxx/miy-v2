//! HTTP tests through the real SDK gateway-identity check, PostgreSQL and a
//! stand-in for the platform backend (directory, Storage, notifications) on
//! the SDK's local preview transport.
//!
//! Ports of AI-DO apps/api/tests/test_dm.py, test_dm_conversation_membership.py,
//! test_dm_message_flow.py, test_dm_read_state.py, test_dm_safety_db.py and
//! test_dm_attachment_records.py where the behaviour exists in miy, plus the
//! miy additions (edit/delete, notifications, mute, manager role).
//!
//! They need a disposable PostgreSQL server, so they are ignored by default:
//!   MESSENGER_TEST_DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:PORT/postgres \
//!     cargo test -- --include-ignored
//! Each run creates a fresh database on that server and applies migrations/.
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode as AxStatus},
    response::IntoResponse,
    routing::{get, post, put},
};
use jsonwebtoken::{EncodingKey, Header, encode};
use reqwest::{Client, Method, Response, StatusCode};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, Executor, PgConnection, postgres::PgPoolOptions};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

const SECRET: &str = "messenger-api-test-gateway-secret-0123456789";
const CAPABILITY: &str = "messenger-preview-capability-0123456789abcdef";
const INSTALLATION: &str = "00000000-0000-4000-8000-0000000000b1";
const RELEASE: &str = "00000000-0000-4000-8000-0000000000b2";

static SERVER: OnceLock<String> = OnceLock::new();
static POOL: OnceLock<String> = OnceLock::new();

// --- platform stand-in ----------------------------------------------------------

#[derive(Default)]
struct Platform {
    users: HashMap<String, String>,
    files: HashMap<String, FileState>,
    events: Vec<(String, String, Value)>,
}

#[derive(Clone)]
struct FileState {
    request_id: String,
    name: String,
    mime: String,
    size: u64,
    bytes: Option<Vec<u8>>,
    status: &'static str,
    reference: Option<String>,
}

fn platform() -> &'static Mutex<Platform> {
    static PLATFORM: OnceLock<Mutex<Platform>> = OnceLock::new();
    PLATFORM.get_or_init(|| Mutex::new(Platform::default()))
}

fn record(kind: &str, actor: &HeaderMap, body: Value) {
    let actor = actor
        .get("x-miy-preview-user")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    platform()
        .lock()
        .unwrap()
        .events
        .push((kind.to_string(), actor, body));
}

fn authorized(headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == format!("Bearer {CAPABILITY}"))
}

fn file_json(id: &str, file: &FileState) -> Value {
    json!({
        "id": id, "version": 1, "name": file.name, "mime": file.mime, "size": file.size,
        "sha256": file.bytes.as_ref().map(|b| hex::encode(Sha256::digest(b))),
        "status": file.status, "reference": file.reference,
    })
}

async fn directory(
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    if !authorized(&headers) {
        return (AxStatus::UNAUTHORIZED, Json(json!({}))).into_response();
    }
    let subject = headers["x-miy-preview-user"].to_str().unwrap().to_string();
    let state = platform().lock().unwrap();
    let ids: Option<Vec<&str>> = query.get("ids").map(|ids| ids.split(',').collect());
    let q = query.get("q").map(|q| q.to_lowercase()).unwrap_or_default();
    let limit: usize = query
        .get("limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(50);
    let mut users: Vec<(&String, &String)> = state
        .users
        .iter()
        .filter(|(id, name)| {
            ids.as_ref().is_none_or(|ids| ids.contains(&id.as_str()))
                && name.to_lowercase().contains(&q)
        })
        .collect();
    users.sort_by(|a, b| a.1.cmp(b.1).then(a.0.cmp(b.0)));
    let users: Vec<Value> = users
        .into_iter()
        .take(limit)
        .map(|(id, name)| json!({"id": id, "name": name, "primaryGroup": {"id": "g1", "name": "개발팀"}}))
        .collect();
    Json(json!({"kind": query.get("kind").cloned().unwrap_or("users".into()), "subjectId": subject,
        "installationId": INSTALLATION, "environment": "dev", "users": users, "groups": [], "nextCursor": null}))
    .into_response()
}

async fn notifications(headers: HeaderMap, Json(body): Json<Value>) -> Json<Value> {
    let accepted = body["recipients"].as_array().map_or(0, |r| r.len());
    let request_id = body["requestId"].clone();
    record("notify", &headers, body);
    Json(json!({"requestId": request_id, "accepted": accepted}))
}

async fn retract(headers: HeaderMap, Json(body): Json<Value>) -> Json<Value> {
    record("retract", &headers, body);
    Json(json!({"retracted": 1}))
}

async fn read(headers: HeaderMap, Json(body): Json<Value>) -> Json<Value> {
    record("read", &headers, body);
    Json(json!({}))
}

async fn files_create(Json(body): Json<Value>) -> Json<Value> {
    let mut state = platform().lock().unwrap();
    let request_id = body["requestId"].as_str().unwrap().to_string();
    if let Some((id, file)) = state.files.iter().find(|(_, f)| f.request_id == request_id) {
        return Json(file_json(id, file));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let file = FileState {
        request_id,
        name: body["name"].as_str().unwrap().to_string(),
        mime: body["mime"].as_str().unwrap().to_string(),
        size: body["size"].as_u64().unwrap(),
        bytes: None,
        status: "pending",
        reference: None,
    };
    let value = file_json(&id, &file);
    state.files.insert(id, file);
    Json(value)
}

async fn files_get(Path(id): Path<String>) -> impl IntoResponse {
    let state = platform().lock().unwrap();
    match state.files.get(&id) {
        Some(file) => Json(file_json(&id, file)).into_response(),
        None => AxStatus::NOT_FOUND.into_response(),
    }
}

async fn files_cancel(headers: HeaderMap, Path(id): Path<String>) -> Json<Value> {
    record("cancel", &headers, json!({"id": id}));
    let mut state = platform().lock().unwrap();
    let file = state.files.get_mut(&id).unwrap();
    file.status = "cancelled";
    Json(file_json(&id, file))
}

async fn files_put(Path(id): Path<String>, body: Bytes) -> impl IntoResponse {
    let mut state = platform().lock().unwrap();
    let file = state.files.get_mut(&id).unwrap();
    if body.len() as u64 != file.size {
        return AxStatus::UNPROCESSABLE_ENTITY.into_response();
    }
    file.bytes = Some(body.to_vec());
    file.status = "ready";
    Json(file_json(&id, file)).into_response()
}

async fn files_content(Path(id): Path<String>) -> impl IntoResponse {
    let state = platform().lock().unwrap();
    let file = state.files.get(&id).unwrap();
    let bytes = file.bytes.clone().unwrap_or_default();
    (
        [
            ("content-type", file.mime.clone()),
            ("content-length", bytes.len().to_string()),
        ],
        bytes,
    )
        .into_response()
}

async fn files_attach(
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    record(
        "attach",
        &headers,
        json!({"id": id, "reference": body["reference"]}),
    );
    let mut state = platform().lock().unwrap();
    let file = state.files.get_mut(&id).unwrap();
    file.status = "attached";
    file.reference = body["reference"].as_str().map(str::to_string);
    Json(file_json(&id, file))
}

async fn files_detach(
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    record(
        "detach",
        &headers,
        json!({"id": id, "reference": body["reference"]}),
    );
    let mut state = platform().lock().unwrap();
    let file = state.files.get_mut(&id).unwrap();
    file.status = "cancelled";
    file.bytes = None;
    Json(file_json(&id, file))
}

fn platform_router() -> Router {
    Router::new()
        .route("/platform/directory", get(directory))
        .route("/platform/notifications", post(notifications))
        .route("/platform/notifications/retract", post(retract))
        .route("/platform/notifications/read", post(read))
        .route("/platform/files", post(files_create))
        .route("/platform/files/{id}", get(files_get).delete(files_cancel))
        .route(
            "/platform/files/{id}/content",
            put(files_put).get(files_content),
        )
        .route("/platform/files/{id}/attach", post(files_attach))
        .route("/platform/files/{id}/detach", post(files_detach))
}

// --- app server -------------------------------------------------------------------

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

async fn prepare_database(admin_url: &str) -> (sqlx::PgPool, String) {
    let name = format!("messenger_test_{}", uuid::Uuid::new_v4().simple());
    let mut admin = PgConnection::connect(admin_url)
        .await
        .expect("connect to MESSENGER_TEST_DATABASE_URL");
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
    (pool, url)
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn server() -> &'static str {
    SERVER.get_or_init(|| {
        let admin_url = std::env::var("MESSENGER_TEST_DATABASE_URL")
            .expect("set MESSENGER_TEST_DATABASE_URL to a disposable PostgreSQL server");
        let port = free_port();
        let platform_port = free_port();
        // SAFETY: set once, before the server threads start reading them.
        unsafe {
            std::env::set_var("MIY_GATEWAY_SECRET", SECRET);
            std::env::set_var("MIY_INSTALLATION_ID", INSTALLATION);
            std::env::set_var("MIY_RELEASE_ID", RELEASE);
            std::env::set_var("MIY_BIND_ADDR", format!("127.0.0.1:{port}"));
            std::env::set_var(
                "MIY_PREVIEW_PLATFORM_ORIGIN",
                format!("http://127.0.0.1:{platform_port}"),
            );
            std::env::set_var("MIY_PREVIEW_PLATFORM_CAPABILITY", CAPABILITY);
        }
        let (ready, wait) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::bind(("127.0.0.1", platform_port))
                    .await
                    .unwrap();
                tokio::spawn(
                    async move { axum::serve(listener, platform_router()).await.unwrap() },
                );
                let (pool, url) = prepare_database(&admin_url).await;
                ready.send(url).unwrap();
                miy_app_sdk::serve(crate::router(pool)).await.unwrap();
            });
        });
        let url = wait.recv().expect("test server failed to start");
        POOL.set(url).unwrap();
        for _ in 0..200 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return format!("http://127.0.0.1:{port}");
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        panic!("test server did not listen");
    })
}

async fn db() -> PgConnection {
    server();
    PgConnection::connect(POOL.get().unwrap()).await.unwrap()
}

/// A person the platform directory lists.
fn person(name: &str) -> String {
    let id = format!("user-{}", uuid::Uuid::new_v4());
    platform()
        .lock()
        .unwrap()
        .users
        .insert(id.clone(), format!("{name} {}", &id[5..9]));
    id
}

fn name_of(id: &str) -> String {
    platform().lock().unwrap().users[id].clone()
}

fn identity_token(user: &str, platform_call: bool) -> String {
    let exp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 300;
    encode(
        &Header::default(),
        &json!({"sub": user, "admin": false, "installation_id": INSTALLATION,
            "release_id": RELEASE, "iss": "miy", "aud": "miy-app", "exp": exp,
            "environment": "dev", "platform": platform_call}),
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

async fn request(
    user: Option<&str>,
    method: Method,
    path: &str,
    body: Option<Value>,
    platform_call: bool,
) -> Response {
    let mut request = Client::new().request(method, format!("{}/api{path}", server()));
    if let Some(user) = user {
        request = request.header("x-miy-identity", identity_token(user, platform_call));
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    request.send().await.unwrap()
}

async fn call(user: &str, method: Method, path: &str, body: Option<Value>) -> Response {
    request(Some(user), method, path, body, false).await
}

async fn json_of(response: Response) -> Value {
    let status = response.status();
    let text = response.text().await.unwrap();
    serde_json::from_str(&text).unwrap_or_else(|_| panic!("{status}: {text}"))
}

async fn ok(response: Response) -> Value {
    let status = response.status();
    let body = json_of(response).await;
    assert!(status.is_success(), "{status}: {body}");
    body
}

async fn code(response: Response) -> (StatusCode, String) {
    let status = response.status();
    let body = json_of(response).await;
    (
        status,
        body["error"]["code"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    )
}

async fn direct(a: &str, b: &str) -> String {
    let body = ok(call(
        a,
        Method::POST,
        "/v1/dm/conversations",
        Some(json!({"recipient_user_id": b})),
    )
    .await)
    .await;
    body["id"].as_str().unwrap().to_string()
}

async fn send(user: &str, conversation: &str, body: &str) -> Response {
    call(
        user,
        Method::POST,
        &format!("/v1/dm/conversations/{conversation}/messages"),
        Some(json!({"body": body})),
    )
    .await
}

async fn messages(user: &str, conversation: &str) -> Vec<Value> {
    ok(call(
        user,
        Method::GET,
        &format!("/v1/dm/conversations/{conversation}/messages"),
        None,
    )
    .await)
    .await["items"]
        .as_array()
        .unwrap()
        .clone()
}

async fn listed(user: &str, conversation: &str) -> (Value, Value) {
    let body = ok(call(user, Method::GET, "/v1/dm/conversations", None).await).await;
    let item = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == conversation)
        .cloned()
        .unwrap_or(Value::Null);
    (body, item)
}

fn events(kind: &str, group: &str) -> Vec<(String, Value)> {
    platform()
        .lock()
        .unwrap()
        .events
        .iter()
        .filter(|(k, _, body)| k == kind && (body["groupKey"] == group || body["id"] == group))
        .map(|(_, actor, body)| (actor.clone(), body.clone()))
        .collect()
}

fn file_events(kind: &str, file_id: &str) -> usize {
    platform()
        .lock()
        .unwrap()
        .events
        .iter()
        .filter(|(k, _, body)| k == kind && body["id"] == file_id)
        .count()
}

async fn claim_manager() -> String {
    // The role table is shared by all tests of one run: claim once.
    static MANAGER: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
    MANAGER
        .get_or_init(|| async {
            let manager = person("관리자");
            let response = call(&manager, Method::POST, "/v1/dm-safety/managers/claim", None).await;
            assert_eq!(response.status(), StatusCode::OK);
            manager
        })
        .await
        .clone()
}

/// The terms settings row is shared: the test that switches terms on runs
/// alone, the others share this lock.
static SETTINGS: tokio::sync::RwLock<()> = tokio::sync::RwLock::const_new(());

// --- tests --------------------------------------------------------------------------

// test_dm.py::test_dm_conversations_messages_are_global_without_notification_rows
// (miy: the message is published to the platform inbox instead).
#[tokio::test]
#[ignore]
async fn direct_conversation_send_read_and_notifications() {
    let _settings = SETTINGS.read().await;
    let sender = person("보낸이");
    let recipient = person("받는이");
    let outsider = person("외부인");
    let me = ok(call(&sender, Method::GET, "/v1/dm/me", None).await).await;
    assert_eq!(
        (me["user"]["id"].as_str(), me["manager"].as_bool()),
        (Some(sender.as_str()), Some(false))
    );
    assert_eq!(me["user"]["full_name"], name_of(&sender));
    let created = call(
        &sender,
        Method::POST,
        "/v1/dm/conversations",
        Some(json!({"recipient_user_id": recipient})),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let conversation = json_of(created).await;
    let id = conversation["id"].as_str().unwrap().to_string();
    assert_eq!(conversation["conversation_type"], "direct");
    assert_eq!(conversation["other_user"]["id"], recipient);
    assert_eq!(conversation["other_user"]["full_name"], name_of(&recipient));
    assert_eq!(
        conversation["other_user"]["primary_org_unit_name"],
        "개발팀"
    );
    assert_eq!(conversation["unread_count"], 0);
    assert_eq!(direct(&sender, &recipient).await, id);
    assert_eq!(direct(&recipient, &sender).await, id);

    let (status, error) = code(
        call(
            &outsider,
            Method::GET,
            &format!("/v1/dm/conversations/{id}/messages"),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::NOT_FOUND, "dm_thread_not_found")
    );

    let message = ok(send(&sender, &id, "  Hello from HQ  ").await).await;
    assert_eq!(message["body"], "Hello from HQ");
    assert_eq!(message["sequence"], 1);
    assert_eq!(message["sender_id"], sender);
    assert_eq!(message["read_state"]["unread_count"], 1);
    assert_eq!(message["edited_at"], Value::Null);

    let received = messages(&recipient, &id).await;
    assert_eq!(received.len(), 1);
    assert_eq!(received[0]["id"], message["id"]);
    let (_, item) = listed(&recipient, &id).await;
    assert_eq!(item["last_message"]["body"], "Hello from HQ");
    assert_eq!(item["unread_count"], 1);

    let published = events("notify", &id);
    assert_eq!(published.len(), 1);
    let notification = &published[0].1;
    assert_eq!(notification["recipients"], json!([recipient]));
    assert_eq!(notification["kind"], "message");
    assert_eq!(
        notification["title"],
        format!("{} · 새 메시지 1개", name_of(&sender))
    );
    assert_eq!(notification["path"], format!("/conversations/{id}"));
    assert_eq!(notification["resourceId"], message["id"]);
    assert!(!notification.to_string().contains("Hello from HQ"));

    ok(send(&sender, &id, "second").await).await;
    let published = events("notify", &id);
    assert_eq!(
        published[1].1["title"],
        format!("{} · 새 메시지 2개", name_of(&sender))
    );

    let read = ok(call(
        &recipient,
        Method::PATCH,
        &format!("/v1/dm/conversations/{id}/read"),
        None,
    )
    .await)
    .await;
    assert_eq!(read["unread_count"], 0);
    let read_events = events("read", &id);
    assert_eq!(read_events.len(), 1);
    assert_eq!(read_events[0].0, recipient);
    // Polling marks read again while visible: nothing moved, nothing sent.
    ok(call(
        &recipient,
        Method::PATCH,
        &format!("/v1/dm/conversations/{id}/read"),
        None,
    )
    .await)
    .await;
    assert_eq!(events("read", &id).len(), 1);
    // The sender's own messages read as read by everyone now.
    let mine = messages(&sender, &id).await;
    assert_eq!(mine[1]["read_state"]["read_by_all"], true);
}

// test_dm.py::test_dm_request_schema_rejects_invalid_route_ids_and_shapes (subset)
#[tokio::test]
#[ignore]
async fn request_shapes_are_validated() {
    let _settings = SETTINGS.read().await;
    let user = person("검증");
    let other = person("상대");
    for body in [
        json!({}),
        json!({"recipient_user_id": other, "participant_user_ids": [other]}),
        json!({"recipient_user_id": "bad/id"}),
        json!({"participant_user_ids": ["a", "a"]}),
        json!({"participant_user_ids": ["a", "b"], "title": "가".repeat(141)}),
    ] {
        let (status, error) =
            code(call(&user, Method::POST, "/v1/dm/conversations", Some(body)).await).await;
        assert_eq!(
            (status, error.as_str()),
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_request_invalid"
            )
        );
    }
    let (status, error) = code(
        call(
            &user,
            Method::POST,
            "/v1/dm/conversations",
            Some(json!({"recipient_user_id": user})),
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::NOT_FOUND, "dm_recipient_not_found")
    );
    let (status, error) = code(
        call(
            &user,
            Method::POST,
            "/v1/dm/conversations",
            Some(json!({"recipient_user_id": "user-unknown"})),
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::NOT_FOUND, "dm_recipient_not_found")
    );
    let id = direct(&user, &other).await;
    let (status, _) = code(send(&user, &id, "   ").await).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    // test_dm.py::test_dm_message_accepts_maximum_body_length
    assert_eq!(
        send(&user, &id, &"a".repeat(120_000)).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        send(&user, &id, &"a".repeat(120_001)).await.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (status, error) = code(
        call(
            &user,
            Method::POST,
            "/v1/dm/conversations",
            Some(json!({"participant_user_ids": [other]})),
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "dm_group_participants_required"
        )
    );
    // The gateway identity is required.
    assert_eq!(
        request(None, Method::GET, "/v1/dm/conversations", None, false)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

// test_dm.py::test_group_dm_management_tracks_membership_lifecycle
#[tokio::test]
#[ignore]
async fn group_membership_lifecycle() {
    let _settings = SETTINGS.read().await;
    let owner = person("방장");
    let member = person("멤버");
    let removable = person("내보낼");
    let added = person("추가");
    let member_added = person("멤버추가");
    let created = ok(call(
        &owner,
        Method::POST,
        "/v1/dm/conversations",
        Some(json!({"title": "Original Room", "participant_user_ids": [member, removable]})),
    )
    .await)
    .await;
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["display_name"], "Original Room");
    let updated = ok(call(
        &owner,
        Method::PATCH,
        &format!("/v1/dm/conversations/{id}"),
        Some(json!({"title": "Updated Room"})),
    )
    .await)
    .await;
    assert_eq!(updated["title"], "Updated Room");
    let (status, error) = code(
        call(
            &member,
            Method::PATCH,
            &format!("/v1/dm/conversations/{id}"),
            Some(json!({"title": "x"})),
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::FORBIDDEN, "dm_manage_forbidden")
    );

    let first = ok(send(&owner, &id, "Before add").await).await;
    let snapshot = ok(call(
        &owner,
        Method::POST,
        &format!("/v1/dm/conversations/{id}/participants"),
        Some(json!({"user_ids": [added]})),
    )
    .await)
    .await;
    assert_eq!(snapshot["participant_count"], 4);
    let (_, added_view) = listed(&added, &id).await;
    assert_eq!(added_view["unread_count"], 0);
    assert_eq!(added_view["last_read_message_id"], first["id"]);

    let reply = ok(call(
        &owner,
        Method::POST,
        &format!("/v1/dm/conversations/{id}/messages"),
        Some(json!({"body": "Reply after add", "reply_to_message_id": first["id"]})),
    )
    .await)
    .await;
    assert_eq!(reply["reply_to"]["id"], first["id"]);
    // Late joiners see only messages after they joined, without old quotes.
    let late = messages(&added, &id).await;
    assert_eq!(late.len(), 1);
    assert_eq!(late[0]["id"], reply["id"]);
    assert_eq!(late[0]["reply_to"], Value::Null);
    let (status, _) = code(
        call(
            &added,
            Method::POST,
            &format!("/v1/dm/conversations/{id}/messages"),
            Some(json!({"body": "x", "reply_to_message_id": first["id"]})),
        )
        .await,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let member_add = ok(call(
        &member,
        Method::POST,
        &format!("/v1/dm/conversations/{id}/participants"),
        Some(json!({"user_ids": [member_added]})),
    )
    .await)
    .await;
    assert_eq!(member_add["participant_count"], 5);
    let removed = ok(call(
        &owner,
        Method::DELETE,
        &format!("/v1/dm/conversations/{id}/participants/{removable}"),
        None,
    )
    .await)
    .await;
    assert_eq!(removed["participant_count"], 4);
    let (_, gone) = listed(&removable, &id).await;
    assert_eq!(gone, Value::Null);
    let retracted = events("retract", &id);
    assert!(
        retracted
            .iter()
            .any(|(_, body)| body["recipients"] == json!([removable]))
    );

    let leave = call(
        &owner,
        Method::DELETE,
        &format!("/v1/dm/conversations/{id}/participants/me"),
        None,
    )
    .await;
    assert_eq!(leave.status(), StatusCode::NO_CONTENT);
    let (_, member_view) = listed(&member, &id).await;
    assert_eq!(member_view["participant_count"], 3);
    assert!(
        member_view["participants"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["user"]["id"] == member && p["role"] == "owner")
    );
    assert!(
        events("retract", &id)
            .iter()
            .any(|(_, body)| body["recipients"] == json!([owner]))
    );
    // Direct conversations cannot be left.
    let pair = direct(&member, &added).await;
    let (status, error) = code(
        call(
            &member,
            Method::DELETE,
            &format!("/v1/dm/conversations/{pair}/participants/me"),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "dm_group_only")
    );
}

// test_dm_safety_db.py::test_report_with_block_is_idempotent_and_blocks_one_to_one_both_ways
#[tokio::test]
#[ignore]
async fn report_with_block_and_one_to_one_blocks() {
    let _settings = SETTINGS.read().await;
    let manager = claim_manager().await;
    let a = person("신고자");
    let b = person("대상");
    let id = direct(&a, &b).await;
    let sent = ok(send(&b, &id, "욕설 본문").await).await;
    let message_id = sent["id"].as_str().unwrap().to_string();
    let payload = json!({
        "client_report_id": uuid::Uuid::new_v4().to_string(),
        "conversation_id": id, "message_id": message_id,
        "category": "harassment", "note": "메모 내용", "block": true,
    });
    let first = ok(call(&a, Method::POST, "/v1/dm/reports", Some(payload.clone())).await).await;
    let report_id = first["report_id"].as_str().unwrap().to_string();
    assert_eq!(
        first,
        json!({"report_id": report_id, "status": "received", "duplicate": false, "blocked": true})
    );
    let replay = ok(call(&a, Method::POST, "/v1/dm/reports", Some(payload.clone())).await).await;
    assert_eq!(replay["duplicate"], true);
    assert_eq!(replay["report_id"], report_id);
    let mut changed = payload.clone();
    changed["category"] = json!("spam");
    let (status, error) = code(call(&a, Method::POST, "/v1/dm/reports", Some(changed)).await).await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::CONFLICT, "dm_safety_report_conflict")
    );
    let mut same_target = payload.clone();
    same_target["client_report_id"] = json!(uuid::Uuid::new_v4().to_string());
    same_target["block"] = json!(false);
    let duplicate = ok(call(&a, Method::POST, "/v1/dm/reports", Some(same_target)).await).await;
    assert_eq!(
        (
            duplicate["report_id"].as_str(),
            duplicate["duplicate"].as_bool()
        ),
        (Some(report_id.as_str()), Some(true))
    );

    for user in [&a, &b] {
        let (status, error) = code(send(user, &id, "또 보냄").await).await;
        assert_eq!(
            (status, error.as_str()),
            (StatusCode::FORBIDDEN, "dm_conversation_blocked")
        );
    }
    let (status, error) = code(call(
        &b,
        Method::POST,
        &format!("/v1/dm/conversations/{id}/attachments"),
        Some(json!({"request_id": uuid::Uuid::new_v4().to_string(), "filename": "x.png", "content_type": "image/png", "size_bytes": 3})),
    )
    .await)
    .await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::FORBIDDEN, "dm_conversation_blocked")
    );
    let (body_a, item_a) = listed(&a, &id).await;
    let (_, item_b) = listed(&b, &id).await;
    assert_eq!(
        (
            item_a["peer_blocked"].as_bool(),
            item_b["peer_blocked"].as_bool()
        ),
        (Some(true), Some(false))
    );
    assert_eq!(body_a["safety"]["version"], 1);
    assert_eq!(body_a["safety"]["manager"], false);
    let blocks_a = ok(call(&a, Method::GET, "/v1/dm/blocks", None).await).await;
    assert_eq!(blocks_a["items"][0]["user"]["id"], b);
    let blocks_b = ok(call(&b, Method::GET, "/v1/dm/blocks", None).await).await;
    assert_eq!(blocks_b["items"], json!([]));

    // Managers other than the parties hear of it, category only.
    let report_events = events("notify", &format!("report:{report_id}"));
    assert_eq!(report_events.len(), 1);
    assert_eq!(report_events[0].1["recipients"], json!([manager]));
    assert_eq!(report_events[0].1["title"], "괴롭힘 신고가 접수됐습니다.");
    let mut conn = db().await;
    let payloads: Vec<Value> = sqlx::query_scalar(
        "SELECT payload FROM dm_audit_logs WHERE entity_id = $1 OR entity_id = $2",
    )
    .bind(&report_id)
    .bind(&b)
    .fetch_all(&mut conn)
    .await
    .unwrap();
    let dumped = serde_json::to_string(&payloads).unwrap();
    assert!(!dumped.contains("메모 내용") && !dumped.contains("욕설 본문"));

    let unblock = ok(call(&a, Method::DELETE, &format!("/v1/dm/blocks/{b}"), None).await).await;
    assert_eq!(
        unblock,
        json!({"user_id": b, "blocked": false, "changed": true})
    );
    let again = ok(call(&a, Method::DELETE, &format!("/v1/dm/blocks/{b}"), None).await).await;
    assert_eq!(again["changed"], false);
    assert_eq!(send(&b, &id, "다시 보냄").await.status(), StatusCode::OK);
    // A blocked pair cannot open a new 1:1 or be invited (actor ↔ invitee only).
    let c = person("제3자");
    ok(call(&c, Method::PUT, &format!("/v1/dm/blocks/{a}"), None).await).await;
    let (status, error) = code(
        call(
            &a,
            Method::POST,
            "/v1/dm/conversations",
            Some(json!({"recipient_user_id": c})),
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::FORBIDDEN, "dm_conversation_blocked")
    );
    let (status, _) = code(
        call(
            &a,
            Method::POST,
            "/v1/dm/conversations",
            Some(json!({"participant_user_ids": [b, c]})),
        )
        .await,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        call(
            &b,
            Method::POST,
            "/v1/dm/conversations",
            Some(json!({"participant_user_ids": [a, c]}))
        )
        .await
        .status(),
        StatusCode::CREATED
    );
}

// test_dm_safety_db.py::test_invalid_targets_and_missing_conversation
#[tokio::test]
#[ignore]
async fn invalid_report_targets() {
    let _settings = SETTINGS.read().await;
    let a = person("가");
    let b = person("나");
    let outsider = person("다");
    let id = direct(&a, &b).await;
    let own = ok(send(&a, &id, "내 메시지").await).await;
    let report = |conversation: &str, message: Option<&str>, user: Option<&str>| {
        json!({"client_report_id": uuid::Uuid::new_v4().to_string(), "conversation_id": conversation,
            "message_id": message, "reported_user_id": user, "category": "spam"})
    };
    let (status, error) = code(
        call(
            &a,
            Method::POST,
            "/v1/dm/reports",
            Some(report(&id, own["id"].as_str(), None)),
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "dm_safety_report_target_invalid"
        )
    );
    let (status, error) = code(
        call(
            &outsider,
            Method::POST,
            "/v1/dm/reports",
            Some(report(&id, None, Some(&a))),
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::NOT_FOUND, "dm_thread_not_found")
    );
    let (status, error) = code(
        call(
            &a,
            Method::POST,
            "/v1/dm/reports",
            Some(report(&id, None, Some(&outsider))),
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "dm_safety_report_target_invalid"
        )
    );
    let person_report = ok(call(
        &a,
        Method::POST,
        "/v1/dm/reports",
        Some(report(&id, None, Some(&b))),
    )
    .await)
    .await;
    assert_eq!(person_report["duplicate"], false);
    let mut extra = report(&id, None, Some(&b));
    extra["unexpected"] = json!(true);
    assert_eq!(
        call(&a, Method::POST, "/v1/dm/reports", Some(extra))
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

// test_dm_safety_db.py::test_report_rate_limit_is_ten_per_hour_with_retry_after
#[tokio::test]
#[ignore]
async fn report_rate_limit() {
    let _settings = SETTINGS.read().await;
    let reporter = person("많이");
    for index in 0..11 {
        let target = person("상대");
        let id = direct(&reporter, &target).await;
        let message = ok(send(&target, &id, "x").await).await;
        let response = call(
            &reporter,
            Method::POST,
            "/v1/dm/reports",
            Some(
                json!({"client_report_id": uuid::Uuid::new_v4().to_string(), "conversation_id": id,
                "message_id": message["id"], "category": "spam"}),
            ),
        )
        .await;
        if index < 10 {
            assert_eq!(response.status(), StatusCode::OK);
        } else {
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
            let retry: u64 = response.headers()["retry-after"]
                .to_str()
                .unwrap()
                .parse()
                .unwrap();
            assert!(retry > 0 && retry <= 3600);
        }
    }
}

// test_dm_safety_db.py::test_admin_reviews_only_the_reported_message_and_hides_it_everywhere
// and test_reporter_or_subject_cannot_review_their_own_report (miy manager role).
#[tokio::test]
#[ignore]
async fn manager_reviews_and_hides_the_reported_message() {
    let _settings = SETTINGS.read().await;
    let manager = claim_manager().await;
    let intruder = person("탈취");
    let (status, error) = code(
        call(
            &intruder,
            Method::POST,
            "/v1/dm-safety/managers/claim",
            None,
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::CONFLICT, "dm_safety_manager_already_assigned")
    );
    let (status, error) =
        code(call(&intruder, Method::GET, "/v1/dm-safety/reports", None).await).await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::FORBIDDEN, "dm_safety_manager_required")
    );

    let a = person("신고");
    let b = person("피신고");
    let id = direct(&a, &b).await;
    let before = ok(send(&b, &id, "앞 메시지").await).await;
    let reported = ok(send(&b, &id, "나쁜 메시지").await).await;
    let reply = ok(call(
        &a,
        Method::POST,
        &format!("/v1/dm/conversations/{id}/messages"),
        Some(json!({"body": "답장", "reply_to_message_id": reported["id"]})),
    )
    .await)
    .await;
    let report = ok(call(
        &a,
        Method::POST,
        "/v1/dm/reports",
        Some(json!({
        "client_report_id": uuid::Uuid::new_v4().to_string(), "conversation_id": id,
        "message_id": reported["id"], "category": "hate", "note": "검토 바람"})),
    )
    .await)
    .await;
    let report_id = report["report_id"].as_str().unwrap().to_string();

    let list = ok(call(
        &manager,
        Method::GET,
        "/v1/dm-safety/reports?status=open",
        None,
    )
    .await)
    .await;
    let summary = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == report_id)
        .unwrap()
        .clone();
    assert_eq!(summary["has_message"], true);
    assert_eq!(summary["conversation_type"], "direct");
    assert!(summary.get("note").is_none());
    let detail = ok(call(
        &manager,
        Method::GET,
        &format!("/v1/dm-safety/reports/{report_id}"),
        None,
    )
    .await)
    .await;
    assert_eq!(detail["note"], "검토 바람");
    assert_eq!(detail["reported_user"]["id"], b);
    let evidence = ok(call(
        &manager,
        Method::GET,
        &format!("/v1/dm-safety/reports/{report_id}/evidence"),
        None,
    )
    .await)
    .await;
    assert_eq!(evidence["message"]["id"], reported["id"]);
    assert_eq!(evidence["message"]["body"], "나쁜 메시지");
    assert!(!evidence.to_string().contains("앞 메시지"));

    // Parties cannot review: grant the reporter the role to check.
    ok(call(
        &manager,
        Method::POST,
        "/v1/dm-safety/managers",
        Some(json!({"user_id": a})),
    )
    .await)
    .await;
    let (status, error) = code(
        call(
            &a,
            Method::GET,
            &format!("/v1/dm-safety/reports/{report_id}"),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::FORBIDDEN, "dm_safety_review_conflict")
    );
    ok(call(
        &manager,
        Method::DELETE,
        &format!("/v1/dm-safety/managers/{a}"),
        None,
    )
    .await)
    .await;

    let resolved = ok(call(&manager, Method::POST, &format!("/v1/dm-safety/reports/{report_id}/resolve"),
        Some(json!({"expected_status": "open", "outcome": "actioned", "action": "hide_message", "resolution_note": "가림"})))
    .await)
    .await;
    assert_eq!(resolved["status"], "actioned");
    assert_eq!(resolved["message_hidden"], true);
    let (status, error) = code(
        call(
            &manager,
            Method::POST,
            &format!("/v1/dm-safety/reports/{report_id}/resolve"),
            Some(json!({"expected_status": "open", "outcome": "dismissed", "action": "none"})),
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::CONFLICT, "dm_safety_report_state_conflict")
    );
    assert!(
        events("retract", &format!("report:{report_id}"))
            .iter()
            .any(|(_, body)| body.get("recipients").is_none())
    );

    for viewer in [&a, &b] {
        let items = messages(viewer, &id).await;
        let hidden = items.iter().find(|m| m["id"] == reported["id"]).unwrap();
        assert_eq!(hidden["hidden"], true);
        assert_eq!(hidden["body"], "관리자가 가린 메시지입니다.");
        let quoting = items.iter().find(|m| m["id"] == reply["id"]).unwrap();
        assert_eq!(quoting["reply_to"]["hidden"], true);
        assert_eq!(
            quoting["reply_to"]["body_preview"],
            "관리자가 가린 메시지입니다."
        );
        assert_eq!(
            items.iter().find(|m| m["id"] == before["id"]).unwrap()["body"],
            "앞 메시지"
        );
    }
    let restored = ok(call(
        &manager,
        Method::POST,
        &format!("/v1/dm-safety/reports/{report_id}/message-visibility"),
        Some(json!({"hidden": false})),
    )
    .await)
    .await;
    assert_eq!(restored["changed"], true);
    assert_eq!(
        messages(&a, &id)
            .await
            .iter()
            .find(|m| m["id"] == reported["id"])
            .unwrap()["body"],
        "나쁜 메시지"
    );
    // The last manager stays.
    let (status, error) = code(
        call(
            &manager,
            Method::DELETE,
            &format!("/v1/dm-safety/managers/{manager}"),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "dm_safety_last_manager")
    );
}

// miy: author edit and delete (COMMON-FEATURES-PLAN §2.2).
#[tokio::test]
#[ignore]
async fn author_edits_and_deletes_with_history_and_notifications() {
    let _settings = SETTINGS.read().await;
    let manager = claim_manager().await;
    let a = person("작성자");
    let b = person("독자");
    let id = direct(&a, &b).await;
    let message = ok(send(&a, &id, "처음 글").await).await;
    let message_id = message["id"].as_str().unwrap().to_string();
    let path = format!("/v1/dm/conversations/{id}/messages/{message_id}");
    let (status, error) =
        code(call(&b, Method::PATCH, &path, Some(json!({"body": "남의 글"}))).await).await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::FORBIDDEN, "dm_message_edit_forbidden")
    );
    let edited = ok(call(
        &a,
        Method::PATCH,
        &path,
        Some(json!({"body": "  고친 글 "})),
    )
    .await)
    .await;
    assert_eq!(edited["body"], "고친 글");
    assert!(edited["edited_at"].is_string());
    // b had not read it: the line is replaced (same count, no body).
    let lines = events("notify", &id);
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[1].1["title"],
        format!("{} · 새 메시지 1개", name_of(&a))
    );
    let mut conn = db().await;
    let revisions: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT action, body_before FROM dm_message_revisions WHERE message_id = $1 ORDER BY id",
    )
    .bind(&message_id)
    .fetch_all(&mut conn)
    .await
    .unwrap();
    assert_eq!(
        revisions,
        vec![("edit".to_string(), Some("처음 글".to_string()))]
    );

    let (status, _) = code(call(&b, Method::DELETE, &path, None).await).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let deleted = ok(call(&a, Method::DELETE, &path, None).await).await;
    assert_eq!(
        (deleted["deleted"].as_bool(), deleted["body"].as_str()),
        (Some(true), Some(""))
    );
    let seen = messages(&b, &id).await;
    assert_eq!(seen[0]["deleted"], true);
    assert_eq!(seen[0]["body"], "");
    // Nothing unread is left for b: the line is retracted.
    assert!(
        events("retract", &id)
            .iter()
            .any(|(_, body)| body["recipients"] == json!([b]))
    );
    let (_, item) = listed(&b, &id).await;
    assert_eq!(item["unread_count"], 0);
    let revisions: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT action, body_before FROM dm_message_revisions WHERE message_id = $1 ORDER BY id",
    )
    .bind(&message_id)
    .fetch_all(&mut conn)
    .await
    .unwrap();
    assert_eq!(
        revisions,
        vec![("edit".to_string(), None), ("delete".to_string(), None)]
    );
    let (status, error) =
        code(call(&a, Method::PATCH, &path, Some(json!({"body": "되살리기"}))).await).await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "dm_message_not_editable")
    );

    // Reported then deleted: the copy stays for the manager until resolved.
    let bad = ok(send(&a, &id, "신고될 글").await).await;
    let bad_id = bad["id"].as_str().unwrap().to_string();
    let report = ok(call(
        &b,
        Method::POST,
        "/v1/dm/reports",
        Some(json!({
        "client_report_id": uuid::Uuid::new_v4().to_string(), "conversation_id": id,
        "message_id": bad_id, "category": "spam"})),
    )
    .await)
    .await;
    let report_id = report["report_id"].as_str().unwrap().to_string();
    ok(call(
        &a,
        Method::DELETE,
        &format!("/v1/dm/conversations/{id}/messages/{bad_id}"),
        None,
    )
    .await)
    .await;
    assert_eq!(messages(&b, &id).await.last().unwrap()["body"], "");
    let evidence = ok(call(
        &manager,
        Method::GET,
        &format!("/v1/dm-safety/reports/{report_id}/evidence"),
        None,
    )
    .await)
    .await;
    assert_eq!(evidence["message"]["body"], "신고될 글");
    assert!(evidence["message"]["deleted_at"].is_string());
    let detail = ok(call(
        &manager,
        Method::GET,
        &format!("/v1/dm-safety/reports/{report_id}"),
        None,
    )
    .await)
    .await;
    assert_eq!(detail["message_deleted"], true);
    ok(call(
        &manager,
        Method::POST,
        &format!("/v1/dm-safety/reports/{report_id}/resolve"),
        Some(json!({"expected_status": "open", "outcome": "dismissed", "action": "none"})),
    )
    .await)
    .await;
    let copies: Vec<Option<String>> =
        sqlx::query_scalar("SELECT body_before FROM dm_message_revisions WHERE message_id = $1")
            .bind(&bad_id)
            .fetch_all(&mut conn)
            .await
            .unwrap();
    assert!(copies.iter().all(Option::is_none));
    // A deleted message cannot be reported again.
    let (status, _) = code(
        call(
            &b,
            Method::POST,
            "/v1/dm/reports",
            Some(json!({
        "client_report_id": uuid::Uuid::new_v4().to_string(), "conversation_id": id,
        "message_id": bad_id, "category": "spam"})),
        )
        .await,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

async fn upload(user: &str, conversation: &str, name: &str, declared: &str, bytes: &[u8]) -> Value {
    let reserved = ok(call(
        user,
        Method::POST,
        &format!("/v1/dm/conversations/{conversation}/attachments"),
        Some(
            json!({"request_id": uuid::Uuid::new_v4().to_string(), "filename": name,
            "content_type": declared, "size_bytes": bytes.len()}),
        ),
    )
    .await)
    .await;
    assert_eq!(reserved["is_image"], false);
    let id = reserved["id"].as_str().unwrap();
    let response = Client::new()
        .put(format!(
            "{}/api/files/dm/attachments/{id}/content",
            server()
        ))
        .header("x-miy-identity", identity_token(user, false))
        .header("content-type", "application/octet-stream")
        .body(bytes.to_vec())
        .send()
        .await
        .unwrap();
    ok(response).await
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

// test_dm.py::test_dm_message_attachments_are_private_to_participants,
// test_dm_spoofed_image_attachment_is_not_previewable and the miy Storage flow.
#[tokio::test]
#[ignore]
async fn attachments_use_platform_storage_and_stay_private() {
    let _settings = SETTINGS.read().await;
    let a = person("첨부");
    let b = person("수신");
    let outsider = person("외부");
    let id = direct(&a, &b).await;
    let image = upload(&a, &id, "사진.png", "image/png", PNG).await;
    assert_eq!(
        (image["is_image"].as_bool(), image["content_type"].as_str()),
        (Some(true), Some("image/png"))
    );
    let spoofed = upload(&a, &id, "가짜.png", "image/png", b"not an image").await;
    assert_eq!(
        (
            spoofed["is_image"].as_bool(),
            spoofed["content_type"].as_str()
        ),
        (Some(false), Some("application/octet-stream"))
    );
    let image_id = image["id"].as_str().unwrap().to_string();
    // An unsent attachment is only the uploader's.
    let path = format!("/files/dm/attachments/{image_id}/content");
    assert_eq!(
        call(&b, Method::GET, &path, None).await.status(),
        StatusCode::NOT_FOUND
    );
    let sent = ok(call(
        &a,
        Method::POST,
        &format!("/v1/dm/conversations/{id}/messages"),
        Some(json!({"body": "", "attachment_ids": [image_id, spoofed["id"]]})),
    )
    .await)
    .await;
    assert_eq!(sent["attachments"].as_array().unwrap().len(), 2);
    let mut conn = db().await;
    let file_id: uuid::Uuid =
        sqlx::query_scalar("SELECT storage_file_id FROM dm_message_attachments WHERE id = $1")
            .bind(&image_id)
            .fetch_one(&mut conn)
            .await
            .unwrap();
    let file_id = file_id.to_string();
    assert_eq!(file_events("attach", &file_id), 1);

    let download = call(&b, Method::GET, &path, None).await;
    assert_eq!(download.status(), StatusCode::OK);
    assert_eq!(download.headers()["content-type"], "image/png");
    assert!(
        download.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("attachment;")
    );
    assert_eq!(download.bytes().await.unwrap().as_ref(), PNG);
    assert_eq!(
        call(&outsider, Method::GET, &path, None).await.status(),
        StatusCode::NOT_FOUND
    );
    let preview = ok(call(
        &b,
        Method::GET,
        &format!("/v1/dm/attachments/{image_id}/preview"),
        None,
    )
    .await)
    .await;
    assert_eq!(
        preview["url"],
        format!("/dm/attachments/{image_id}/content?disposition=inline")
    );
    let (status, error) = code(
        call(
            &b,
            Method::GET,
            &format!(
                "/v1/dm/attachments/{}/preview",
                spoofed["id"].as_str().unwrap()
            ),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "dm_attachment_preview_unsupported"
        )
    );
    // Sent once only; an attachment of another person cannot be linked.
    let (status, error) = code(
        call(
            &a,
            Method::POST,
            &format!("/v1/dm/conversations/{id}/messages"),
            Some(json!({"attachment_ids": [image_id]})),
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "dm_attachment_already_sent"
        )
    );
    let foreign = upload(&b, &id, "b.txt", "text/plain", b"hello").await;
    let (status, _) = code(
        call(
            &a,
            Method::POST,
            &format!("/v1/dm/conversations/{id}/messages"),
            Some(json!({"attachment_ids": [foreign["id"]]})),
        )
        .await,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    // Limits: empty and above 10 MiB.
    let reserve = |size: i64| json!({"request_id": uuid::Uuid::new_v4().to_string(), "filename": "f", "size_bytes": size});
    let (status, error) = code(
        call(
            &a,
            Method::POST,
            &format!("/v1/dm/conversations/{id}/attachments"),
            Some(reserve(0)),
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "dm_attachment_empty")
    );
    let (status, error) = code(
        call(
            &a,
            Method::POST,
            &format!("/v1/dm/conversations/{id}/attachments"),
            Some(reserve(10 * 1024 * 1024 + 1)),
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            "dm_attachment_size_limit_exceeded"
        )
    );
    // An unsent file can be cancelled by its uploader.
    let cancel = call(
        &b,
        Method::DELETE,
        &format!("/v1/dm/attachments/{}", foreign["id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(cancel.status(), StatusCode::NO_CONTENT);

    // Deleting the message releases its files and hides them.
    let message_id = sent["id"].as_str().unwrap();
    ok(call(
        &a,
        Method::DELETE,
        &format!("/v1/dm/conversations/{id}/messages/{message_id}"),
        None,
    )
    .await)
    .await;
    assert_eq!(file_events("detach", &file_id), 1);
    assert_eq!(
        call(&b, Method::GET, &path, None).await.status(),
        StatusCode::NOT_FOUND
    );
    let status: String =
        sqlx::query_scalar("SELECT storage_status FROM dm_message_attachments WHERE id = $1")
            .bind(&image_id)
            .fetch_one(&mut conn)
            .await
            .unwrap();
    assert_eq!(status, "released");
}

// test_dm_safety.py terms/filter cases through the web routes.
#[tokio::test]
#[ignore]
async fn terms_and_banned_words_guard_user_content() {
    let _settings = SETTINGS.write().await;
    let manager = claim_manager().await;
    let a = person("규칙");
    let b = person("상대");
    let id = direct(&a, &b).await;
    let settings =
        ok(call(&manager, Method::GET, "/v1/dm-safety/terms-settings", None).await).await;
    assert_eq!(settings["version"], "2026-11-ugc-v1");
    assert_eq!(settings["required"], false);
    let term = ok(call(
        &manager,
        Method::POST,
        "/v1/dm-safety/filter-terms",
        Some(json!({"term": "금지어"})),
    )
    .await)
    .await;
    let (status, error) = code(
        call(
            &manager,
            Method::POST,
            "/v1/dm-safety/filter-terms",
            Some(json!({"term": " 금 지 어 "})),
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::CONFLICT, "ugc_filter_term_duplicate")
    );
    let (status, error) = code(send(&a, &id, "이건 금.지.어 입니다").await).await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::UNPROCESSABLE_ENTITY, "ugc_content_rejected")
    );
    let c = person("셋째");
    let (status, _) = code(
        call(
            &a,
            Method::POST,
            "/v1/dm/conversations",
            Some(json!({"participant_user_ids": [b, c], "title": "금지어 방"})),
        )
        .await,
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let remove = call(
        &manager,
        Method::DELETE,
        &format!(
            "/v1/dm-safety/filter-terms/{}",
            term["id"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(remove.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        send(&a, &id, "이건 금지어 입니다").await.status(),
        StatusCode::OK
    );

    // Terms switched on (shared settings: restored at the end).
    ok(call(&manager, Method::PUT, "/v1/dm-safety/terms-settings",
        Some(json!({"version": "2026-12-test", "required": true, "document_url": "https://example.com/terms"}))).await).await;
    let fresh = person("신규");
    let pair = direct(&fresh, &b).await;
    let (status, error) = code(send(&fresh, &pair, "안녕").await).await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::FORBIDDEN, "ugc_terms_acceptance_required")
    );
    let terms = ok(call(&fresh, Method::GET, "/v1/dm/terms", None).await).await;
    assert_eq!(
        (terms["required"].as_bool(), terms["accepted"].as_bool()),
        (Some(true), Some(false))
    );
    assert_eq!(terms["document_url"], "https://example.com/terms");
    let (status, error) = code(
        call(
            &fresh,
            Method::POST,
            "/v1/dm/terms/accept",
            Some(json!({"version": "old"})),
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::CONFLICT, "ugc_terms_version_mismatch")
    );
    let accepted = ok(call(
        &fresh,
        Method::POST,
        "/v1/dm/terms/accept",
        Some(json!({"version": "2026-12-test"})),
    )
    .await)
    .await;
    assert_eq!(accepted["changed"], true);
    let again = ok(call(
        &fresh,
        Method::POST,
        "/v1/dm/terms/accept",
        Some(json!({"version": "2026-12-test"})),
    )
    .await)
    .await;
    assert_eq!(again["changed"], false);
    assert_eq!(send(&fresh, &pair, "안녕").await.status(), StatusCode::OK);
    ok(call(
        &manager,
        Method::PUT,
        "/v1/dm-safety/terms-settings",
        Some(json!({"version": "2026-11-ugc-v1", "required": false})),
    )
    .await)
    .await;
}

// miy: platform → app recheck, mute, user search and older pages.
#[tokio::test]
#[ignore]
async fn notification_check_mute_search_and_older_messages() {
    let _settings = SETTINGS.read().await;
    let a = person("확인");
    let b = person("검색대상");
    let outsider = person("남");
    let id = direct(&a, &b).await;
    let check = |user: &str, platform_call: bool| {
        let user = user.to_string();
        let id = id.clone();
        async move {
            request(Some(&user), Method::POST, "/platform/notification-check",
                Some(json!({"items": [{"groupKey": id, "resourceId": "m"}, {"groupKey": "unknown", "resourceId": null}]})),
                platform_call)
            .await
        }
    };
    assert_eq!(ok(check(&a, true).await).await, json!({"allowed": [id]}));
    assert_eq!(
        ok(check(&outsider, true).await).await,
        json!({"allowed": []})
    );
    let (status, error) = code(check(&a, false).await).await;
    assert_eq!(
        (status, error.as_str()),
        (StatusCode::FORBIDDEN, "platform_call_required")
    );

    // Home card: unread conversations for the platform, no message body.
    ok(send(&b, &id, "비밀 본문").await).await;
    let card = ok(request(
        Some(&a),
        Method::POST,
        "/platform/home-card",
        Some(json!({})),
        true,
    )
    .await)
    .await;
    assert_eq!(card["title"], "안 읽은 대화");
    assert_eq!(card["count"], 1);
    assert_eq!(card["items"][0]["title"], name_of(&b));
    assert_eq!(
        card["items"][0]["subtitle"],
        format!("{} · 새 메시지 1개", name_of(&b))
    );
    assert_eq!(card["items"][0]["path"], format!("/conversations/{id}"));
    assert!(card["items"][0]["at"].as_str().unwrap().ends_with('Z'));
    assert!(!card.to_string().contains("비밀 본문"));
    let (status, _) = code(
        request(
            Some(&a),
            Method::POST,
            "/platform/home-card",
            Some(json!({})),
            false,
        )
        .await,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    ok(call(
        &a,
        Method::PATCH,
        &format!("/v1/dm/conversations/{id}/read"),
        None,
    )
    .await)
    .await;
    let card = ok(request(
        Some(&a),
        Method::POST,
        "/platform/home-card",
        Some(json!({})),
        true,
    )
    .await)
    .await;
    assert_eq!(
        (
            card["count"].as_u64(),
            card["items"].as_array().map(Vec::len)
        ),
        (Some(0), Some(0))
    );

    let muted = ok(call(
        &b,
        Method::PUT,
        &format!("/v1/dm/conversations/{id}/mute"),
        Some(json!({"muted": true})),
    )
    .await)
    .await;
    assert!(muted["muted_at"].is_string());
    let before = events("notify", &id).len();
    ok(send(&a, &id, "조용히").await).await;
    assert_eq!(events("notify", &id).len(), before);
    ok(call(
        &b,
        Method::PUT,
        &format!("/v1/dm/conversations/{id}/mute"),
        Some(json!({"muted": false})),
    )
    .await)
    .await;
    ok(send(&a, &id, "다시").await).await;
    assert_eq!(events("notify", &id).len(), before + 1);

    let found = ok(call(&a, Method::GET, "/v1/dm/users?q=검색대상", None).await).await;
    assert!(found.as_array().unwrap().iter().any(|u| u["id"] == b));
    let me = ok(call(
        &a,
        Method::GET,
        &format!("/v1/dm/users?q={}", &name_of(&a)),
        None,
    )
    .await)
    .await;
    assert!(me.as_array().unwrap().iter().all(|u| u["id"] != a));
    let me = ok(call(
        &a,
        Method::GET,
        &format!("/v1/dm/users?q={}&include_current=true", &name_of(&a)),
        None,
    )
    .await)
    .await;
    assert!(me.as_array().unwrap().iter().any(|u| u["id"] == a));

    for index in 0..5 {
        ok(send(&a, &id, &format!("page {index}")).await).await;
    }
    let newest = ok(call(
        &a,
        Method::GET,
        &format!("/v1/dm/conversations/{id}/messages?limit=3"),
        None,
    )
    .await)
    .await;
    let items = newest["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[2]["body"], "page 4");
    let oldest_loaded = items[0]["created_at"].as_str().unwrap();
    let older = ok(call(
        &a,
        Method::GET,
        &format!("/v1/dm/conversations/{id}/messages?limit=3&before={oldest_loaded}"),
        None,
    )
    .await)
    .await;
    let older = older["items"].as_array().unwrap();
    assert_eq!(older.len(), 3);
    assert_eq!(older[2]["body"], "page 1");
}
