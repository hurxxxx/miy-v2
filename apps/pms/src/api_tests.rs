//! HTTP tests through the real SDK gateway-identity check, PostgreSQL and a
//! fake core (directory, notifications, files) on loopback.
//!
//! Ports of the AI-DO PMS server tests that apply (apps/api/tests/
//! test_pms_issues.py, test_pms_space_membership.py, test_pms_app_workspaces.py,
//! test_pms_list_archive.py, test_pms_bulk_task_dates.py, test_pms_task_copy.py,
//! test_pms_task_work_items.py, test_pms_space_preferences.py,
//! test_pms_view_preferences.py, test_pms_task_list_customization.py,
//! test_pms_task_export.py) plus the miy contracts (notifications, panel/home
//! card, notification check, search, attachments, single-task grants).
//!
//! They need a disposable PostgreSQL server, so they are ignored by default:
//!   PMS_TEST_DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:PORT/postgres \
//!     cargo test -- --include-ignored
//! Each run creates a fresh database on that server and applies migrations/.
//! The reinforcement (templates, custom fields, milestones, saved filters) is
//! tested in api_tests/reinforcement.rs.
mod reinforcement;

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::Duration,
};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode as AxumStatus},
    response::IntoResponse,
    routing::{get, post, put},
};
use jsonwebtoken::{EncodingKey, Header, encode};
use reqwest::{Client, Method, Response, StatusCode};
use serde_json::{Value, json};
use sqlx::{Connection, Executor, PgConnection, PgPool, postgres::PgPoolOptions};

const SECRET: &str = "pms-api-test-gateway-secret-0123456789abcdef";
const INSTALLATION: &str = "00000000-0000-4000-8000-0000000000b1";
const RELEASE: &str = "00000000-0000-4000-8000-0000000000b2";

static SERVER: OnceLock<(String, PgPool)> = OnceLock::new();
static CORE: OnceLock<String> = OnceLock::new();

#[derive(Default)]
struct FakeCore {
    users: HashMap<String, (String, Option<(String, String)>)>,
    notifications: Vec<Value>,
    retracted: Vec<Value>,
    read: Vec<(String, String)>,
    files: HashMap<String, (Value, Vec<u8>)>,
}

fn core() -> &'static Mutex<FakeCore> {
    static STATE: OnceLock<Mutex<FakeCore>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(FakeCore::default()))
}

fn caller(headers: &HeaderMap) -> String {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer tok-"))
        .unwrap_or_default()
        .to_string()
}

async fn fake_directory(
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Json<Value> {
    let state = core().lock().unwrap();
    let ids: Option<Vec<String>> = query
        .get("ids")
        .map(|ids| ids.split(',').map(str::to_string).collect());
    let mut users: Vec<Value> = state
        .users
        .iter()
        .filter(|(id, _)| ids.as_ref().is_none_or(|ids| ids.contains(id)))
        .map(|(id, (name, group))| {
            json!({"id": id, "name": name, "primaryGroup": group.as_ref().map(|(gid, gname)| json!({"id": gid, "name": gname}))})
        })
        .collect();
    users.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Json(json!({
        "kind": query.get("kind").cloned().unwrap_or_else(|| "users".into()),
        "subjectId": caller(&headers),
        "installationId": INSTALLATION,
        "environment": "dev",
        "users": users,
        "groups": [],
        "nextCursor": null,
    }))
}

async fn fake_notify(Json(body): Json<Value>) -> Json<Value> {
    let accepted = body["recipients"].as_array().map_or(0, Vec::len);
    let request_id = body["requestId"].clone();
    core().lock().unwrap().notifications.push(body);
    Json(json!({"requestId": request_id, "accepted": accepted}))
}

async fn fake_retract(Json(body): Json<Value>) -> Json<Value> {
    core().lock().unwrap().retracted.push(body);
    Json(json!({"retracted": 1}))
}

async fn fake_read(headers: HeaderMap, Json(body): Json<Value>) -> Json<Value> {
    core().lock().unwrap().read.push((
        caller(&headers),
        body["groupKey"].as_str().unwrap_or_default().to_string(),
    ));
    Json(json!({}))
}

async fn fake_file_create(Json(body): Json<Value>) -> Json<Value> {
    let mut state = core().lock().unwrap();
    if let Some((meta, _)) = state
        .files
        .values()
        .find(|(meta, _)| meta["requestId"] == body["requestId"])
    {
        return Json(meta.clone());
    }
    let id = uuid::Uuid::new_v4().to_string();
    let meta = json!({"id": id, "version": 1, "name": body["name"], "mime": body["mime"],
        "size": body["size"], "sha256": null, "status": "pending", "reference": null,
        "requestId": body["requestId"]});
    state.files.insert(id, (meta.clone(), Vec::new()));
    Json(meta)
}

async fn fake_file_get(Path((_, id)): Path<(String, String)>) -> impl IntoResponse {
    match core().lock().unwrap().files.get(&id) {
        Some((meta, _)) => Json(meta.clone()).into_response(),
        None => AxumStatus::NOT_FOUND.into_response(),
    }
}

async fn fake_file_upload(Path((_, id)): Path<(String, String)>, body: Bytes) -> impl IntoResponse {
    let mut state = core().lock().unwrap();
    let Some((meta, bytes)) = state.files.get_mut(&id) else {
        return AxumStatus::NOT_FOUND.into_response();
    };
    *bytes = body.to_vec();
    meta["status"] = "ready".into();
    meta["sha256"] = "a".repeat(64).into();
    Json(meta.clone()).into_response()
}

async fn fake_file_attach(
    Path((_, id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let mut state = core().lock().unwrap();
    let Some((meta, _)) = state.files.get_mut(&id) else {
        return AxumStatus::NOT_FOUND.into_response();
    };
    meta["status"] = "attached".into();
    meta["reference"] = body["reference"].clone();
    Json(meta.clone()).into_response()
}

async fn fake_file_release(Path((_, id)): Path<(String, String)>) -> impl IntoResponse {
    let mut state = core().lock().unwrap();
    let Some((meta, _)) = state.files.get_mut(&id) else {
        return AxumStatus::NOT_FOUND.into_response();
    };
    meta["status"] = "cancelled".into();
    Json(meta.clone()).into_response()
}

async fn fake_file_content(Path((_, id)): Path<(String, String)>) -> impl IntoResponse {
    match core().lock().unwrap().files.get(&id) {
        Some((_, bytes)) => (
            [("content-type", "application/octet-stream")],
            bytes.clone(),
        )
            .into_response(),
        None => AxumStatus::NOT_FOUND.into_response(),
    }
}

fn start_core() -> String {
    CORE.get_or_init(|| {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let base = "/api/v1/installations/{installation}/platform";
                let app = Router::new()
                    .route(&format!("{base}/directory"), get(fake_directory))
                    .route(&format!("{base}/notifications"), post(fake_notify))
                    .route(&format!("{base}/notifications/retract"), post(fake_retract))
                    .route(&format!("{base}/notifications/read"), post(fake_read))
                    .route(&format!("{base}/files"), post(fake_file_create))
                    .route(
                        &format!("{base}/files/{{id}}"),
                        get(fake_file_get).delete(fake_file_release),
                    )
                    .route(
                        &format!("{base}/files/{{id}}/content"),
                        put(fake_file_upload).get(fake_file_content),
                    )
                    .route(
                        &format!("{base}/files/{{id}}/attach"),
                        post(fake_file_attach),
                    )
                    .route(
                        &format!("{base}/files/{{id}}/detach"),
                        post(fake_file_release),
                    )
                    .with_state(());
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                axum::serve(listener, app).await.unwrap();
            });
        });
        format!("http://127.0.0.1:{port}")
    })
    .clone()
}

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
    let name = format!("pms_test_{}", uuid::Uuid::new_v4().simple());
    let mut admin = PgConnection::connect(admin_url)
        .await
        .expect("connect to PMS_TEST_DATABASE_URL");
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
    pool
}

async fn wait_for_query_blocked_by(blocker: i32, query_pattern: &str) {
    let pool = &server().1;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity \
                 WHERE wait_event_type = 'Lock' AND query LIKE $2 AND $1 = ANY(pg_blocking_pids(pid)))",
            )
            .bind(blocker)
            .bind(query_pattern)
            .fetch_one(pool)
            .await
            .unwrap();
            if waiting {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("request should be waiting on the test transaction");
}

fn server() -> &'static (String, PgPool) {
    SERVER.get_or_init(|| {
        let admin_url = std::env::var("PMS_TEST_DATABASE_URL")
            .expect("set PMS_TEST_DATABASE_URL to a disposable PostgreSQL server");
        let core = start_core();
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        // SAFETY: set once, before the server thread starts reading them.
        unsafe {
            std::env::set_var("MIY_GATEWAY_SECRET", SECRET);
            std::env::set_var("MIY_INSTALLATION_ID", INSTALLATION);
            std::env::set_var("MIY_RELEASE_ID", RELEASE);
            std::env::set_var("MIY_BIND_ADDR", format!("127.0.0.1:{port}"));
            std::env::set_var("MIY_CORE_ORIGIN", format!("{core}/"));
            std::env::set_var("MIY_CORE_ALLOW_HTTP", "1");
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
        for _ in 0..200 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return (format!("http://127.0.0.1:{port}"), pool);
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        panic!("test server did not listen");
    })
}

/// A new directory user with a display name and primary group.
fn new_user(name: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    core().lock().unwrap().users.insert(
        id.clone(),
        (name.to_string(), Some(("g-dev".into(), "개발팀".into()))),
    );
    id
}

fn identity_with(user: &str, admin: bool, platform: bool) -> String {
    let exp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 300;
    let mut claims = json!({"sub": user, "admin": admin, "installation_id": INSTALLATION,
        "release_id": RELEASE, "iss": "miy", "aud": "miy-app", "exp": exp, "environment": "dev",
        "platform": platform});
    if !platform {
        claims["delegation_token"] = json!(format!("tok-{user}"));
    }
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

async fn send(
    user: &str,
    admin: bool,
    platform: bool,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Response {
    let mut request = Client::new()
        .request(method, format!("{}/api{path}", server().0))
        .header("x-miy-identity", identity_with(user, admin, platform));
    if let Some(body) = body {
        request = request.json(&body);
    }
    request.send().await.unwrap()
}

async fn call(user: &str, method: Method, path: &str, body: Option<Value>) -> Response {
    send(user, false, false, method, path, body).await
}

async fn ok(user: &str, method: Method, path: &str, body: Option<Value>) -> Value {
    let response = call(user, method.clone(), path, body).await;
    let status = response.status();
    let text = response.text().await.unwrap();
    assert!(status.is_success(), "{method} {path}: {status} {text}");
    if text.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text).unwrap()
    }
}

async fn error_code(response: Response) -> (StatusCode, String) {
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    (
        status,
        body["error"]["code"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    )
}

async fn create_space(owner: &str, name: &str) -> Value {
    ok(
        owner,
        Method::POST,
        "/v1/pms/spaces",
        Some(json!({"name": name})),
    )
    .await
}

async fn add_member(owner: &str, space: &str, user: &str, role: &str) {
    ok(
        owner,
        Method::POST,
        &format!("/v1/pms/spaces/{space}/members"),
        Some(json!({"user_id": user, "role": role})),
    )
    .await;
}

async fn create_list(owner: &str, space: &str, key: &str, name: &str) -> Value {
    ok(
        owner,
        Method::POST,
        "/v1/pms/lists",
        Some(json!({"team_id": space, "key": key, "name": name})),
    )
    .await
}

async fn create_task(user: &str, list: &str, body: Value) -> Value {
    ok(
        user,
        Method::POST,
        &format!("/v1/pms/lists/{list}/tasks"),
        Some(body),
    )
    .await
}

fn unique_key(prefix: &str) -> String {
    format!(
        "{prefix}{}",
        &uuid::Uuid::new_v4().simple().to_string()[..6].to_uppercase()
    )
}

struct Fixture {
    owner: String,
    space: String,
    list: String,
}

async fn fixture() -> Fixture {
    let owner = new_user("홍길동");
    let space = create_space(&owner, "제품 개발").await["id"]
        .as_str()
        .unwrap()
        .to_string();
    let list = create_list(&owner, &space, &unique_key("W"), "웹 개선").await["id"]
        .as_str()
        .unwrap()
        .to_string();
    Fixture { owner, space, list }
}

fn notifications_for(task_id: &str) -> Vec<Value> {
    core()
        .lock()
        .unwrap()
        .notifications
        .iter()
        .filter(|n| n["resourceId"] == task_id)
        .cloned()
        .collect()
}

// ── Spaces and membership ──────────────────────────────────────────────

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn any_user_creates_a_space_and_owns_it_others_need_membership() {
    let owner = new_user("Owner");
    let stranger = new_user("Stranger");
    let space = create_space(&owner, "  Mobile App  ").await;
    assert_eq!(space["name"], "Mobile App");
    assert_eq!(space["current_user_role"], "owner");
    assert_eq!(space["member_count"], 1);
    assert!(space["key"].as_str().unwrap().starts_with("s-"));
    let id = space["id"].as_str().unwrap();
    let spaces = ok(&owner, Method::GET, "/v1/pms/spaces", None).await;
    assert!(spaces.as_array().unwrap().iter().any(|s| s["id"] == id));
    let spaces = ok(&stranger, Method::GET, "/v1/pms/spaces", None).await;
    assert!(!spaces.as_array().unwrap().iter().any(|s| s["id"] == id));
    let (status, code) = error_code(
        call(
            &stranger,
            Method::GET,
            &format!("/v1/pms/spaces/{id}/members"),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, code.as_str()),
        (StatusCode::FORBIDDEN, "pms_space_access_required")
    );
    // Neither the platform admin flag bypasses membership (test_pms_issues
    // test_workspace_admin_needs_direct_pms_space_membership).
    let response = send(
        &stranger,
        true,
        false,
        Method::GET,
        &format!("/v1/pms/spaces/{id}/lists"),
        None,
    )
    .await;
    assert_eq!(error_code(response).await.1, "pms_space_access_required");
    // Default statuses were seeded with the space.
    let statuses = ok(
        &owner,
        Method::GET,
        &format!("/v1/pms/spaces/{id}/statuses"),
        None,
    )
    .await;
    let slugs: Vec<&str> = statuses["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["slug"].as_str().unwrap())
        .collect();
    assert_eq!(
        slugs,
        vec!["todo", "in_progress", "review", "done", "complete"]
    );
    let (status, code) = error_code(
        call(
            &owner,
            Method::POST,
            "/v1/pms/spaces",
            Some(json!({"name": " x "})),
        )
        .await,
    )
    .await;
    assert_eq!(
        (status, code.as_str()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_request_invalid"
        )
    );
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn member_roles_follow_the_app_space_rules() {
    let f = fixture().await;
    let admin = new_user("Admin");
    let member = new_user("Member");
    let other = new_user("Other");
    add_member(&f.owner, &f.space, &admin, "admin").await;
    // An admin may add members but not grant admin/owner.
    add_member(&admin, &f.space, &member, "member").await;
    let response = call(
        &admin,
        Method::POST,
        &format!("/v1/pms/spaces/{}/members", f.space),
        Some(json!({"user_id": other, "role": "admin"})),
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (
            StatusCode::FORBIDDEN,
            "pms_space_owner_admin_manage_required".into()
        )
    );
    // A member cannot manage.
    let response = call(
        &member,
        Method::POST,
        &format!("/v1/pms/spaces/{}/members", f.space),
        Some(json!({"user_id": other})),
    )
    .await;
    assert_eq!(
        error_code(response).await.1,
        "pms_space_owner_admin_required"
    );
    // Already a member; unknown user.
    let response = call(
        &f.owner,
        Method::POST,
        &format!("/v1/pms/spaces/{}/members", f.space),
        Some(json!({"user_id": member})),
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (StatusCode::CONFLICT, "pms_user_already_space_member".into())
    );
    let response = call(
        &f.owner,
        Method::POST,
        &format!("/v1/pms/spaces/{}/members", f.space),
        Some(json!({"user_id": "missing-user"})),
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (StatusCode::NOT_FOUND, "auth_user_not_found".into())
    );
    // Bulk add skips existing members.
    let result = ok(
        &f.owner,
        Method::POST,
        &format!("/v1/pms/spaces/{}/members/bulk", f.space),
        Some(json!({"user_ids": [member, other], "role": "viewer"})),
    )
    .await;
    assert_eq!(result["skipped_count"], 1);
    assert_eq!(result["items"][0]["user_id"], other.as_str());
    assert_eq!(result["items"][0]["full_name"], "Other");
    assert_eq!(result["items"][0]["primary_org_unit_name"], "개발팀");
    // Listing: owners/admins first, then by name.
    let members = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/spaces/{}/members", f.space),
        None,
    )
    .await;
    let names: Vec<&str> = members["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["full_name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["Admin", "홍길동", "Member", "Other"]);
    // The last owner must remain; anyone may leave.
    let response = call(
        &f.owner,
        Method::DELETE,
        &format!("/v1/pms/spaces/{}/members/{}", f.space, f.owner),
        None,
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (StatusCode::CONFLICT, "pms_space_owner_must_remain".into())
    );
    let response = call(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/spaces/{}/members/{}", f.space, f.owner),
        Some(json!({"role": "member"})),
    )
    .await;
    assert_eq!(error_code(response).await.1, "pms_space_owner_must_remain");
    let response = call(
        &member,
        Method::DELETE,
        &format!("/v1/pms/spaces/{}/members/{member}", f.space),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    // An owner hands over ownership, then may step down.
    ok(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/spaces/{}/members/{admin}", f.space),
        Some(json!({"role": "owner"})),
    )
    .await;
    ok(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/spaces/{}/members/{}", f.space, f.owner),
        Some(json!({"role": "member"})),
    )
    .await;
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn space_preferences_pin_and_hide_exclude_each_other() {
    let f = fixture().await;
    let path = format!("/v1/pms/spaces/{}/preference", f.space);
    let pinned = ok(
        &f.owner,
        Method::PATCH,
        &path,
        Some(json!({"pinned": true})),
    )
    .await;
    assert_eq!(
        pinned,
        json!({"space_id": f.space, "pinned": true, "hidden": false})
    );
    let hidden = ok(
        &f.owner,
        Method::PATCH,
        &path,
        Some(json!({"hidden": true})),
    )
    .await;
    assert_eq!(hidden["pinned"], false);
    assert_eq!(hidden["hidden"], true);
    let listed = ok(&f.owner, Method::GET, "/v1/pms/space-preferences", None).await;
    assert_eq!(listed["items"].as_array().unwrap().len(), 1);
    let cleared = ok(
        &f.owner,
        Method::PATCH,
        &path,
        Some(json!({"hidden": false})),
    )
    .await;
    assert_eq!(cleared["hidden"], false);
    assert!(
        ok(&f.owner, Method::GET, "/v1/pms/space-preferences", None).await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let response = call(
        &f.owner,
        Method::PATCH,
        &path,
        Some(json!({"pinned": true, "hidden": true})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let response = call(
        &f.owner,
        Method::PATCH,
        &path,
        Some(json!({"color": "red"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let stranger = new_user("Stranger");
    assert_eq!(
        error_code(
            call(
                &stranger,
                Method::PATCH,
                &path,
                Some(json!({"pinned": true}))
            )
            .await
        )
        .await
        .1,
        "pms_space_access_required"
    );
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn view_preferences_default_to_status_grouping() {
    let user = new_user("Viewer");
    assert_eq!(
        ok(&user, Method::GET, "/v1/pms/view-preferences", None).await["task_list_group_by"],
        "status"
    );
    ok(
        &user,
        Method::PATCH,
        "/v1/pms/view-preferences",
        Some(json!({"task_list_group_by": "assignee"})),
    )
    .await;
    assert_eq!(
        ok(&user, Method::GET, "/v1/pms/view-preferences", None).await["task_list_group_by"],
        "assignee"
    );
    let response = call(
        &user,
        Method::PATCH,
        "/v1/pms/view-preferences",
        Some(json!({"task_list_group_by": "milestone"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ── Lists and statuses ─────────────────────────────────────────────────

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn task_lists_inherit_the_space_workflow_and_get_default_labels() {
    let f = fixture().await;
    let list = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/lists/{}", f.list),
        None,
    )
    .await;
    assert_eq!(list["status_mode"], "inherit");
    assert_eq!(list["role"], "owner");
    assert_eq!(list["team_name"], "제품 개발");
    assert_eq!(list["member_count"], 1);
    let labels = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/lists/{}/labels", f.list),
        None,
    )
    .await;
    let names: Vec<&str> = labels["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["blocked", "customer", "qa"]);
    let statuses = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/lists/{}/statuses", f.list),
        None,
    )
    .await;
    assert_eq!(statuses["mode"], "inherit");
    assert_eq!(statuses["source"], "space");
    // A list status switches the list to its own copy of the workflow.
    let created = ok(
        &f.owner,
        Method::POST,
        &format!("/v1/pms/lists/{}/statuses", f.list),
        Some(json!({"name": "QA Check", "category": "active", "sort_order": 2})),
    )
    .await;
    assert_eq!(created["slug"], "qa_check");
    let statuses = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/lists/{}/statuses", f.list),
        None,
    )
    .await;
    assert_eq!(statuses["mode"], "custom");
    assert_eq!(statuses["items"].as_array().unwrap().len(), 6);
    // Only one closed status; the closed one cannot be deleted.
    let response = call(
        &f.owner,
        Method::POST,
        &format!("/v1/pms/lists/{}/statuses", f.list),
        Some(json!({"name": "Dropped", "category": "closed"})),
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (StatusCode::CONFLICT, "pms_closed_status_single".into())
    );
    let duplicate = call(
        &f.owner,
        Method::POST,
        &format!("/v1/pms/lists/{}/statuses", f.list),
        Some(json!({"name": "qa check"})),
    )
    .await;
    assert_eq!(error_code(duplicate).await.1, "pms_status_name_exists");
    // A status in use cannot be deleted; switching back to inherit refuses
    // statuses the space does not have.
    let task = create_task(
        &f.owner,
        &f.list,
        json!({"title": "QA task", "status": "qa_check"}),
    )
    .await;
    assert_eq!(task["status_label"], "QA Check");
    let repeated = ok(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/lists/{}/status-mode", f.list),
        Some(json!({"mode": "custom"})),
    )
    .await;
    assert_eq!(repeated, statuses);
    let id = created["id"].as_str().unwrap();
    let response = call(
        &f.owner,
        Method::DELETE,
        &format!("/v1/pms/task-list-statuses/{id}"),
        None,
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (StatusCode::CONFLICT, "pms_status_in_use".into())
    );
    let response = call(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/lists/{}/status-mode", f.list),
        Some(json!({"mode": "inherit"})),
    )
    .await;
    assert_eq!(error_code(response).await.1, "pms_status_mode_incompatible");
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn tasks_accept_full_length_workflow_statuses() {
    let f = fixture().await;
    let created = ok(
        &f.owner,
        Method::POST,
        &format!("/v1/pms/lists/{}/statuses", f.list),
        Some(json!({"name": "Awaiting Compliance Review", "category": "active"})),
    )
    .await;
    assert_eq!(created["slug"], "awaiting_compliance_review");
    let task = create_task(
        &f.owner,
        &f.list,
        json!({"title": "Compliance task", "status": created["slug"]}),
    )
    .await;
    assert_eq!(task["status"], created["slug"]);
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn concurrent_list_edits_preserve_unrelated_fields() {
    let f = fixture().await;
    let path = format!("/v1/pms/lists/{}", f.list);
    let mut blocker = server().1.begin().await.unwrap();
    let blocker_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM pms_task_lists WHERE id = $1 FOR UPDATE")
        .bind(&f.list)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let rename_owner = f.owner.clone();
    let rename_path = path.clone();
    let renamed = tokio::spawn(async move {
        ok(
            &rename_owner,
            Method::PATCH,
            &rename_path,
            Some(json!({"name": "Renamed list"})),
        )
        .await
    });
    wait_for_query_blocked_by(blocker_pid, "%pms_task_lists WHERE id = $1 FOR UPDATE").await;
    let update_owner = f.owner.clone();
    let update_path = path.clone();
    let updated = tokio::spawn(async move {
        ok(
            &update_owner,
            Method::PATCH,
            &update_path,
            Some(json!({"description": "Updated description"})),
        )
        .await
    });
    blocker.commit().await.unwrap();
    renamed.await.unwrap();
    updated.await.unwrap();
    let final_list = ok(&f.owner, Method::GET, &path, None).await;
    assert_eq!(final_list["name"], "Renamed list");
    assert_eq!(final_list["description"], "Updated description");
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn deleting_former_parent_preserves_concurrently_moved_child() {
    let f = fixture().await;
    let former = create_task(&f.owner, &f.list, json!({"title": "Former parent"})).await;
    let destination = create_task(&f.owner, &f.list, json!({"title": "New parent"})).await;
    let child = create_task(
        &f.owner,
        &f.list,
        json!({"title": "Moving child", "parent_id": former["id"]}),
    )
    .await;
    let child_id = child["id"].as_str().unwrap();
    let child_path = format!("/v1/pms/tasks/{child_id}");
    let mut blocker = server().1.begin().await.unwrap();
    let blocker_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM pms_tasks WHERE id = $1 FOR UPDATE")
        .bind(child_id)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let move_owner = f.owner.clone();
    let move_path = child_path.clone();
    let destination_id = destination["id"].clone();
    let moved = tokio::spawn(async move {
        ok(
            &move_owner,
            Method::PATCH,
            &move_path,
            Some(json!({"parent_id": destination_id})),
        )
        .await
    });
    wait_for_query_blocked_by(blocker_pid, "%pms_tasks WHERE id = $1 FOR UPDATE").await;
    let delete_owner = f.owner.clone();
    let delete_path = format!("/v1/pms/tasks/{}", former["id"].as_str().unwrap());
    let deleted = tokio::spawn(async move {
        call(&delete_owner, Method::DELETE, &delete_path, None)
            .await
            .status()
    });
    blocker.commit().await.unwrap();
    assert_eq!(moved.await.unwrap()["parent_id"], destination["id"]);
    assert_eq!(deleted.await.unwrap(), StatusCode::NO_CONTENT);
    let child = ok(&f.owner, Method::GET, &child_path, None).await;
    assert_eq!(child["task"]["parent_id"], destination["id"]);
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn archived_lists_are_read_only_and_must_be_archived_before_delete() {
    let f = fixture().await;
    let member = new_user("Member");
    add_member(&f.owner, &f.space, &member, "member").await;
    let task = create_task(&f.owner, &f.list, json!({"title": "Keep me"})).await;
    let task_id = task["id"].as_str().unwrap();
    let response = call(
        &f.owner,
        Method::DELETE,
        &format!("/v1/pms/lists/{}", f.list),
        None,
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (
            StatusCode::CONFLICT,
            "pms_task_list_archive_before_delete".into()
        )
    );
    let response = call(
        &member,
        Method::PATCH,
        &format!("/v1/pms/lists/{}", f.list),
        Some(json!({"archived": true})),
    )
    .await;
    assert_eq!(
        error_code(response).await.1,
        "pms_task_list_owner_admin_required"
    );
    let archived = ok(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/lists/{}", f.list),
        Some(json!({"archived": true})),
    )
    .await;
    assert_eq!(archived["archived"], true);
    let response = call(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/tasks/{task_id}"),
        Some(json!({"title": "Changed"})),
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (
            StatusCode::CONFLICT,
            "pms_task_list_archived_read_only".into()
        )
    );
    let response = call(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/lists/{}", f.list),
        Some(json!({"name": "Renamed"})),
    )
    .await;
    assert_eq!(
        error_code(response).await.1,
        "pms_task_list_archived_read_only"
    );
    // Detail stays readable; the active list query hides it.
    ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/tasks/{task_id}"),
        None,
    )
    .await;
    let lists = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/spaces/{}/lists?archived=false", f.space),
        None,
    )
    .await;
    assert_eq!(lists["total"], 0);
    let response = call(
        &f.owner,
        Method::DELETE,
        &format!("/v1/pms/lists/{}", f.list),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = call(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/tasks/{task_id}"),
        None,
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (StatusCode::NOT_FOUND, "pms_task_not_found".into())
    );
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn folders_create_a_default_list_and_keep_lists_on_delete() {
    let f = fixture().await;
    let folder = ok(
        &f.owner,
        Method::POST,
        "/v1/pms/folders",
        Some(json!({"name": "Q4", "team_id": f.space})),
    )
    .await;
    assert_eq!(folder["list_count"], 1);
    let folder_id = folder["id"].as_str().unwrap();
    let lists = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/spaces/{}/lists?sort_by=sort_order", f.space),
        None,
    )
    .await;
    let in_folder: Vec<&Value> = lists["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["folder_id"] == folder_id)
        .collect();
    assert_eq!(in_folder.len(), 1);
    assert_eq!(in_folder[0]["name"], "List");
    assert_eq!(in_folder[0]["folder_name"], "Q4");
    ok(
        &f.owner,
        Method::DELETE,
        &format!("/v1/pms/folders/{folder_id}"),
        None,
    )
    .await;
    let lists = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/spaces/{}/lists", f.space),
        None,
    )
    .await;
    assert_eq!(lists["total"], 2);
    let viewer = new_user("Viewer");
    add_member(&f.owner, &f.space, &viewer, "viewer").await;
    let response = call(
        &viewer,
        Method::POST,
        "/v1/pms/folders",
        Some(json!({"name": "No", "team_id": f.space})),
    )
    .await;
    assert_eq!(
        error_code(response).await.1,
        "pms_space_viewer_modify_denied"
    );
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn deleting_a_space_hides_its_data() {
    let f = fixture().await;
    let task = create_task(&f.owner, &f.list, json!({"title": "Hidden"})).await;
    let response = call(
        &f.owner,
        Method::DELETE,
        &format!("/v1/pms/spaces/{}", f.space),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(
        !ok(&f.owner, Method::GET, "/v1/pms/spaces", None)
            .await
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == f.space.as_str())
    );
    let response = call(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/lists/{}", f.list),
        None,
    )
    .await;
    assert_eq!(error_code(response).await.1, "pms_task_list_not_found");
    let response = call(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/tasks/{}", task["id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(error_code(response).await.1, "pms_task_access_required");
}

// ── Tasks ──────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn task_crud_status_flow_and_activity() {
    let f = fixture().await;
    let task = create_task(
        &f.owner,
        &f.list,
        json!({"title": "  로그인 화면 개선 ", "priority": "high",
        "start_date": "2026-11-01", "due_date": "2026-11-30"}),
    )
    .await;
    assert_eq!(task["title"], "로그인 화면 개선");
    assert_eq!(task["status"], "todo");
    assert_eq!(task["status_label"], "To Do");
    assert_eq!(task["priority_label"], "High");
    assert_eq!(task["reporter_name"], "홍길동");
    assert_eq!(task["progress"], 0.0);
    assert!(task["reference"].as_str().unwrap().ends_with("-1"));
    let id = task["id"].as_str().unwrap();
    let updated = ok(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/tasks/{id}"),
        Some(json!({"status": "done"})),
    )
    .await;
    assert_eq!(updated["status"], "done");
    assert!(
        updated["completed_date"].is_string(),
        "completion status sets the missing completed date"
    );
    // Explicit null clears nullable fields; omitted fields stay.
    let cleared = ok(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/tasks/{id}"),
        Some(json!({"due_date": null, "completed_date": null})),
    )
    .await;
    assert!(cleared["due_date"].is_null());
    assert!(cleared["completed_date"].is_null());
    assert_eq!(cleared["start_date"], "2026-11-01");
    // The effective range is checked without side effects.
    let response = call(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/tasks/{id}"),
        Some(json!({"due_date": "2026-10-01", "title": "Should not save"})),
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "pms_task_date_range_invalid".into()
        )
    );
    let response = call(
        &f.owner,
        Method::POST,
        &format!("/v1/pms/lists/{}/tasks", f.list),
        Some(json!({"title": "Bad range", "start_date": "2026-12-02", "due_date": "2026-12-01"})),
    )
    .await;
    assert_eq!(error_code(response).await.1, "pms_task_date_range_invalid");
    let response = call(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/tasks/{id}"),
        Some(json!({"unknown": 1})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let detail = ok(&f.owner, Method::GET, &format!("/v1/pms/tasks/{id}"), None).await;
    assert_eq!(detail["task"]["title"], "로그인 화면 개선");
    let logs = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/tasks/{id}/activity-logs"),
        None,
    )
    .await;
    let fields: Vec<&str> = logs["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l["field_name"].as_str())
        .collect();
    assert!(fields.contains(&"status"));
    assert!(fields.contains(&"completed_date"));
    assert!(fields.contains(&"due_date"));
    let messages: Vec<&str> = logs["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["message"].as_str().unwrap())
        .collect();
    assert!(messages.iter().any(|m| m.starts_with("홍길동 created ")));
    assert_eq!(logs["items"][0]["actor_name"], "홍길동");
    let response = call(
        &f.owner,
        Method::DELETE,
        &format!("/v1/pms/tasks/{id}"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn viewers_read_but_cannot_modify_and_assignees_must_be_members() {
    let f = fixture().await;
    let viewer = new_user("Viewer");
    let outsider = new_user("Outsider");
    add_member(&f.owner, &f.space, &viewer, "viewer").await;
    let task = create_task(&f.owner, &f.list, json!({"title": "Read only"})).await;
    let id = task["id"].as_str().unwrap();
    ok(&viewer, Method::GET, &format!("/v1/pms/tasks/{id}"), None).await;
    let response = call(
        &viewer,
        Method::PATCH,
        &format!("/v1/pms/tasks/{id}"),
        Some(json!({"title": "Nope"})),
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (
            StatusCode::FORBIDDEN,
            "pms_task_list_viewer_modify_denied".into()
        )
    );
    let response = call(
        &viewer,
        Method::POST,
        &format!("/v1/pms/tasks/{id}/comments"),
        Some(json!({"body": "hi"})),
    )
    .await;
    assert_eq!(
        error_code(response).await.1,
        "pms_task_list_viewer_modify_denied"
    );
    let response = call(&outsider, Method::GET, &format!("/v1/pms/tasks/{id}"), None).await;
    assert_eq!(
        error_code(response).await,
        (StatusCode::FORBIDDEN, "pms_task_access_required".into())
    );
    let response = call(
        &f.owner,
        Method::PUT,
        &format!("/v1/pms/tasks/{id}/assignees"),
        Some(json!({"user_ids": [outsider]})),
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (
            StatusCode::BAD_REQUEST,
            "pms_assignees_task_list_members_required".into()
        )
    );
    let response = call(
        &f.owner,
        Method::PUT,
        &format!("/v1/pms/tasks/{id}/followers"),
        Some(json!({"user_ids": [outsider]})),
    )
    .await;
    assert_eq!(
        error_code(response).await.1,
        "pms_followers_task_list_members_required"
    );
    let assignees = ok(
        &f.owner,
        Method::PUT,
        &format!("/v1/pms/tasks/{id}/assignees"),
        Some(json!({"user_ids": [viewer, f.owner, viewer]})),
    )
    .await;
    assert_eq!(
        assignees,
        json!([{"user_id": viewer, "full_name": "Viewer"}, {"user_id": f.owner, "full_name": "홍길동"}])
    );
    let detail = ok(&f.owner, Method::GET, &format!("/v1/pms/tasks/{id}"), None).await;
    assert_eq!(detail["task"]["assignee_id"], viewer.as_str());
    assert_eq!(detail["task"]["assignee_name"], "Viewer");
    let followers = ok(
        &f.owner,
        Method::PUT,
        &format!("/v1/pms/tasks/{id}/followers"),
        Some(json!({"user_ids": [viewer]})),
    )
    .await;
    assert_eq!(followers[0]["full_name"], "Viewer");
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn task_list_sorts_by_dates_titles_and_paginates() {
    let f = fixture().await;
    for (title, due) in [
        ("10. 시공", Some("2026-12-03")),
        ("2. 설계", None),
        ("1. 기획", Some("2026-12-01")),
    ] {
        let mut body = json!({"title": title});
        if let Some(due) = due {
            body["due_date"] = json!(due);
        }
        create_task(&f.owner, &f.list, body).await;
    }
    let titles = |value: &Value| -> Vec<String> {
        value["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["title"].as_str().unwrap().to_string())
            .collect()
    };
    let by_title = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/lists/{}/tasks?sort_by=title", f.list),
        None,
    )
    .await;
    assert_eq!(titles(&by_title), vec!["1. 기획", "2. 설계", "10. 시공"]);
    let by_due = ok(
        &f.owner,
        Method::GET,
        &format!(
            "/v1/pms/lists/{}/tasks?sort_by=due_date&sort_dir=desc",
            f.list
        ),
        None,
    )
    .await;
    assert_eq!(titles(&by_due), vec!["10. 시공", "1. 기획", "2. 설계"]);
    let searched = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/lists/{}/tasks?q=%EC%84%A4%EA%B3%84", f.list),
        None,
    )
    .await;
    assert_eq!(titles(&searched), vec!["2. 설계"]);
    let filtered = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/lists/{}/tasks?due_date_from=2026-12-02", f.list),
        None,
    )
    .await;
    assert_eq!(titles(&filtered), vec!["10. 시공"]);
    for index in 0..102 {
        create_task(
            &f.owner,
            &f.list,
            json!({"title": format!("bulk {index:03}")}),
        )
        .await;
    }
    let page = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/lists/{}/tasks?page=2&page_size=100", f.list),
        None,
    )
    .await;
    assert_eq!(page["total"], 105);
    assert_eq!(page["items"].as_array().unwrap().len(), 5);
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn reorder_moves_tasks_and_parents_without_cycles() {
    let f = fixture().await;
    let a = create_task(&f.owner, &f.list, json!({"title": "AA"})).await;
    let b = create_task(&f.owner, &f.list, json!({"title": "BB"})).await;
    let (a, b) = (a["id"].as_str().unwrap(), b["id"].as_str().unwrap());
    let path = format!("/v1/pms/lists/{}/tasks/reorder", f.list);
    let result = ok(
        &f.owner,
        Method::PATCH,
        &path,
        Some(json!({"items": [
            {"task_id": b, "board_position": 1000, "parent_id": a},
            {"task_id": a, "board_position": 2000},
        ]})),
    )
    .await;
    assert_eq!(result["updated_count"], 2);
    assert_eq!(result["items"][0]["parent_id"], a);
    let response = call(
        &f.owner,
        Method::PATCH,
        &path,
        Some(json!({"items": [{"task_id": a, "board_position": 1, "parent_id": b}]})),
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (StatusCode::CONFLICT, "pms_task_parent_cycle".into())
    );
    let response = call(
        &f.owner,
        Method::PATCH,
        &path,
        Some(json!({"items": [
        {"task_id": a, "board_position": 1}, {"task_id": a, "board_position": 2}]})),
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (StatusCode::BAD_REQUEST, "pms_duplicate_task_ids".into())
    );
    let detail = ok(&f.owner, Method::GET, &format!("/v1/pms/tasks/{a}"), None).await;
    assert_eq!(detail["task"]["board_position"], 2000);
    assert_eq!(detail["task"]["subtask_count"], 1);
    assert_eq!(detail["subtasks"][0]["id"], b);
    // Unlinking the subtask places it right after its former parent.
    let c = create_task(&f.owner, &f.list, json!({"title": "CC"})).await;
    let unlinked = ok(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/tasks/{b}"),
        Some(json!({"parent_id": null})),
    )
    .await;
    assert!(unlinked["parent_id"].is_null());
    let roots = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/lists/{}/tasks", f.list),
        None,
    )
    .await;
    let order: Vec<&str> = roots["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap())
        .collect();
    assert_eq!(order, vec!["AA", "BB", "CC"], "{c}");
    // Name order is saved as the custom order.
    let saved = ok(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/lists/{}/tasks/reorder-by-title", f.list),
        Some(json!({"sort_dir": "desc"})),
    )
    .await;
    assert!(saved["updated_count"].as_i64().unwrap() > 0);
    let roots = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/lists/{}/tasks", f.list),
        None,
    )
    .await;
    let order: Vec<&str> = roots["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["title"].as_str().unwrap())
        .collect();
    assert_eq!(order, vec!["CC", "BB", "AA"]);
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn bulk_changes_status_labels_dates_and_deletes() {
    let f = fixture().await;
    let labels = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/lists/{}/labels", f.list),
        None,
    )
    .await;
    let qa = labels["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["name"] == "qa")
        .unwrap()["id"]
        .clone();
    let one = create_task(
        &f.owner,
        &f.list,
        json!({"title": "One", "start_date": "2026-11-10"}),
    )
    .await;
    let two = create_task(&f.owner, &f.list, json!({"title": "Two"})).await;
    let ids = json!([one["id"], two["id"]]);
    let path = format!("/v1/pms/lists/{}/tasks/bulk", f.list);
    let result = ok(
        &f.owner,
        Method::PATCH,
        &path,
        Some(json!({"task_ids": ids, "status": "done",
        "add_label_ids": [qa], "due_date": "2026-11-20", "assignee_id": f.owner})),
    )
    .await;
    assert_eq!(result, json!({"updated_count": 2, "deleted_count": 0}));
    let tasks = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/lists/{}/tasks", f.list),
        None,
    )
    .await;
    for task in tasks["items"].as_array().unwrap() {
        assert_eq!(task["status"], "done");
        assert!(task["completed_date"].is_string());
        assert_eq!(task["due_date"], "2026-11-20");
        assert_eq!(task["labels"][0]["name"], "qa");
        assert_eq!(task["assignee_ids"], json!([f.owner]));
    }
    // One inverted range rejects the whole request.
    let response = call(
        &f.owner,
        Method::PATCH,
        &path,
        Some(json!({"task_ids": ids, "due_date": "2026-11-01"})),
    )
    .await;
    assert_eq!(error_code(response).await.1, "pms_task_date_range_invalid");
    // Sent as null clears.
    ok(
        &f.owner,
        Method::PATCH,
        &path,
        Some(json!({"task_ids": ids, "due_date": null})),
    )
    .await;
    let response = call(
        &f.owner,
        Method::PATCH,
        &path,
        Some(json!({"task_ids": ["missing"]})),
    )
    .await;
    assert_eq!(
        error_code(response).await,
        (StatusCode::NOT_FOUND, "pms_no_matching_tasks".into())
    );
    let too_many: Vec<String> = (0..201).map(|i| format!("t{i}")).collect();
    let response = call(
        &f.owner,
        Method::PATCH,
        &path,
        Some(json!({"task_ids": too_many})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let deleted = ok(
        &f.owner,
        Method::PATCH,
        &path,
        Some(json!({"task_ids": ids, "delete": true})),
    )
    .await;
    assert_eq!(deleted, json!({"updated_count": 0, "deleted_count": 2}));
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn checklist_comments_templates_and_custom_fields() {
    let f = fixture().await;
    let task = create_task(&f.owner, &f.list, json!({"title": "Work items"})).await;
    let id = task["id"].as_str().unwrap();
    let item = ok(
        &f.owner,
        Method::POST,
        &format!("/v1/pms/tasks/{id}/checklist"),
        Some(json!({"text": "Confirm"})),
    )
    .await;
    let item_id = item["id"].as_str().unwrap();
    assert_eq!(
        ok(
            &f.owner,
            Method::PATCH,
            &format!("/v1/pms/checklist/{item_id}"),
            Some(json!({"completed": true}))
        )
        .await["completed"],
        true
    );
    ok(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/checklist/{item_id}"),
        Some(json!({"completed": false})),
    )
    .await;
    let logs = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/tasks/{id}/activity-logs"),
        None,
    )
    .await;
    let actions: Vec<&str> = logs["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["action"].as_str().unwrap())
        .collect();
    assert!(actions.contains(&"checklist_checked") && actions.contains(&"checklist_unchecked"));
    let second = ok(
        &f.owner,
        Method::POST,
        &format!("/v1/pms/tasks/{id}/checklist"),
        Some(json!({"text": "Second", "sort_order": 5})),
    )
    .await;
    ok(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/tasks/{id}/checklist/reorder"),
        Some(json!({"item_ids": [second["id"], "unknown"]})),
    )
    .await;
    // Unknown ids are ignored; an item left out keeps its order value.
    let detail = ok(&f.owner, Method::GET, &format!("/v1/pms/tasks/{id}"), None).await;
    let order: Vec<(&str, i64)> = detail["checklist_items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            (
                item["text"].as_str().unwrap(),
                item["sort_order"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(order, vec![("Confirm", 0), ("Second", 0)]);
    assert_eq!(detail["task"]["checklist_total"], 2);
    let comment = ok(
        &f.owner,
        Method::POST,
        &format!("/v1/pms/tasks/{id}/comments"),
        Some(json!({"body": " 확인 부탁드립니다 "})),
    )
    .await;
    assert_eq!(comment["body"], "확인 부탁드립니다");
    assert_eq!(comment["author_name"], "홍길동");
    let template = ok(&f.owner, Method::POST, &format!("/v1/pms/spaces/{}/templates", f.space),
        Some(json!({"name": "Bug", "default_priority": "high", "checklist_items": [{"text": "Reproduce"}]}))).await;
    assert_eq!(template["default_priority"], "high");
    let updated = ok(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/templates/{}", template["id"].as_str().unwrap()),
        Some(json!({"name": "Defect"})),
    )
    .await;
    assert_eq!(updated["name"], "Defect");
    let field = ok(
        &f.owner,
        Method::POST,
        &format!("/v1/pms/lists/{}/custom-fields", f.list),
        Some(json!({"name": "Customer", "field_type": "select", "options": ["A", "B"]})),
    )
    .await;
    let field_id = field["id"].as_str().unwrap();
    ok(
        &f.owner,
        Method::PUT,
        &format!("/v1/pms/tasks/{id}/custom-field-values"),
        Some(json!({"field_id": field_id, "value": "A"})),
    )
    .await;
    ok(
        &f.owner,
        Method::PUT,
        &format!("/v1/pms/tasks/{id}/custom-field-values"),
        Some(json!({"field_id": field_id, "value": "B"})),
    )
    .await;
    let values = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/tasks/{id}/custom-field-values"),
        None,
    )
    .await;
    assert_eq!(values, json!([{"field_id": field_id, "value": "B"}]));
    let response = call(
        &f.owner,
        Method::PUT,
        &format!("/v1/pms/tasks/{id}/custom-field-values"),
        Some(json!({"field_id": "missing", "value": "x"})),
    )
    .await;
    assert_eq!(error_code(response).await.1, "pms_custom_field_wrong_list");
    let milestone = ok(
        &f.owner,
        Method::POST,
        &format!("/v1/pms/lists/{}/milestones", f.list),
        Some(json!({"title": "Beta", "due_date": "2026-12-31"})),
    )
    .await;
    ok(
        &f.owner,
        Method::PATCH,
        &format!("/v1/pms/tasks/{id}"),
        Some(json!({"milestone_id": milestone["id"]})),
    )
    .await;
    let milestones = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/lists/{}/milestones", f.list),
        None,
    )
    .await;
    assert_eq!(milestones["items"][0]["task_count"], 1);
    let summary = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/dashboard/summary?list_id={}", f.list),
        None,
    )
    .await;
    assert_eq!(summary["list_count"], 1);
    assert_eq!(
        summary["milestone_due_soon_count"].as_i64().unwrap() >= 0,
        true
    );
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn assigned_and_today_overdue_show_only_the_callers_open_tasks() {
    let f = fixture().await;
    let member = new_user("Member");
    add_member(&f.owner, &f.space, &member, "member").await;
    create_task(
        &f.owner,
        &f.list,
        json!({"title": "Overdue", "assignee_ids": [member], "due_date": "2026-11-01"}),
    )
    .await;
    create_task(
        &f.owner,
        &f.list,
        json!({"title": "Today", "assignee_ids": [member], "due_date": "2026-11-22"}),
    )
    .await;
    create_task(
        &f.owner,
        &f.list,
        json!({"title": "Later", "assignee_ids": [member], "due_date": "2026-12-22"}),
    )
    .await;
    create_task(&f.owner, &f.list, json!({"title": "Done", "assignee_ids": [member], "status": "done", "due_date": "2026-11-01"})).await;
    create_task(
        &f.owner,
        &f.list,
        json!({"title": "Someone else", "due_date": "2026-11-01"}),
    )
    .await;
    let titles = |value: &Value| -> Vec<String> {
        value["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["title"].as_str().unwrap().to_string())
            .collect()
    };
    let today = ok(
        &member,
        Method::GET,
        "/v1/pms/tasks/today-overdue?today=2026-11-22",
        None,
    )
    .await;
    assert_eq!(titles(&today), vec!["Overdue", "Today"]);
    let assigned = ok(&member, Method::GET, "/v1/pms/tasks/assigned", None).await;
    assert_eq!(titles(&assigned), vec!["Overdue", "Today", "Later"]);
    assert_eq!(assigned["page_size"], 10);
    let paged = ok(
        &member,
        Method::GET,
        "/v1/pms/tasks/assigned?page=2&page_size=2",
        None,
    )
    .await;
    assert_eq!(titles(&paged), vec!["Later"]);
    let widget = ok(
        &member,
        Method::GET,
        "/v1/personal-widgets/pms/tasks/assigned",
        None,
    )
    .await;
    assert_eq!(widget["total"], 3);
    assert_eq!(widget["items"][0]["workspace"]["id"], f.space.as_str());
    assert_eq!(widget["workspaces"][0]["role"], "member");
    let response = call(&member, Method::GET, "/v1/pms/tasks/today-overdue", None).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn copy_keeps_matching_values_and_counts_the_rest() {
    let f = fixture().await;
    let member = new_user("Member");
    add_member(&f.owner, &f.space, &member, "member").await;
    let labels = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/lists/{}/labels", f.list),
        None,
    )
    .await;
    let qa = labels["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["name"] == "qa")
        .unwrap()["id"]
        .clone();
    let parent = create_task(&f.owner, &f.list, json!({"title": "Parent", "assignee_ids": [member], "label_ids": [qa], "status": "in_progress"})).await;
    create_task(
        &f.owner,
        &f.list,
        json!({"title": "Child", "parent_id": parent["id"]}),
    )
    .await;
    let other = fixture().await;
    let target = create_list(&f.owner, &f.space, &unique_key("T"), "대상").await;
    let response = call(
        &other.owner,
        Method::POST,
        &format!("/v1/pms/lists/{}/tasks/copy", f.list),
        Some(json!({"task_ids": [parent["id"]], "target_list_id": other.list})),
    )
    .await;
    assert_eq!(
        error_code(response).await.1,
        "pms_task_list_access_required"
    );
    let result = ok(
        &f.owner,
        Method::POST,
        &format!("/v1/pms/lists/{}/tasks/copy", f.list),
        Some(json!({"task_ids": [parent["id"]], "target_list_id": target["id"]})),
    )
    .await;
    assert_eq!(result["copied_count"], 2);
    assert_eq!(
        result["unmatched"],
        json!({"assignees": 0, "labels": 0, "milestones": 0, "custom_fields": 0})
    );
    let copied = ok(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/lists/{}/tasks", target["id"].as_str().unwrap()),
        None,
    )
    .await;
    let parent_copy = copied["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["title"] == "Parent")
        .unwrap();
    assert_eq!(parent_copy["status"], "in_progress");
    assert_eq!(parent_copy["assignee_ids"], json!([member]));
    assert_eq!(parent_copy["labels"][0]["name"], "qa");
    assert_eq!(parent_copy["reporter_id"], f.owner.as_str());
    let reset = ok(&f.owner, Method::POST, &format!("/v1/pms/lists/{}/tasks/copy", f.list),
        Some(json!({"task_ids": [parent["id"]], "target_list_id": target["id"], "reset_progress": true}))).await;
    let reset_copy = ok(
        &f.owner,
        Method::GET,
        &format!(
            "/v1/pms/tasks/{}",
            reset["copied_task_ids"][0].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(reset_copy["task"]["status"], "todo");
    assert_eq!(reset_copy["task"]["assignee_ids"], json!([]));
    let response = call(
        &f.owner,
        Method::POST,
        &format!("/v1/pms/lists/{}/tasks/copy", f.list),
        Some(json!({"task_ids": [parent["id"], parent["id"]], "target_list_id": target["id"]})),
    )
    .await;
    assert_eq!(error_code(response).await.1, "pms_duplicate_task_ids");
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn export_returns_a_workbook_named_after_the_list() {
    let f = fixture().await;
    create_task(&f.owner, &f.list, json!({"title": "=HYPERLINK(\"x\")"})).await;
    let response = call(
        &f.owner,
        Method::GET,
        &format!("/v1/pms/lists/{}/export?q=HYPER", f.list),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let disposition = response.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .to_string();
    assert!(disposition.contains("filename*=UTF-8''"), "{disposition}");
    assert!(
        disposition.contains("%ED%95%84%ED%84%B0"),
        "filtered file says so: {disposition}"
    );
    let bytes = response.bytes().await.unwrap();
    assert!(bytes.starts_with(b"PK"));
}

// ── miy: notifications, grants, platform calls, attachments ────────────

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn notifications_reach_readers_and_are_retracted_and_read() {
    let f = fixture().await;
    let member = new_user("Member");
    let follower = new_user("Follower");
    let outsider = new_user("Outsider");
    add_member(&f.owner, &f.space, &member, "member").await;
    add_member(&f.owner, &f.space, &follower, "member").await;
    let task = create_task(&f.owner, &f.list, json!({"title": "알림 확인"})).await;
    let id = task["id"].as_str().unwrap();
    ok(
        &f.owner,
        Method::PUT,
        &format!("/v1/pms/tasks/{id}/assignees"),
        Some(json!({"user_ids": [member]})),
    )
    .await;
    ok(
        &f.owner,
        Method::PUT,
        &format!("/v1/pms/tasks/{id}/followers"),
        Some(json!({"user_ids": [follower]})),
    )
    .await;
    let assigned: Vec<Value> = notifications_for(id)
        .into_iter()
        .filter(|n| n["kind"] == "assigned")
        .collect();
    assert_eq!(assigned.len(), 1);
    assert_eq!(assigned[0]["recipients"], json!([member]));
    assert_eq!(assigned[0]["groupKey"], format!("task:{id}"));
    assert_eq!(assigned[0]["path"], format!("/tasks/{id}"));
    assert_eq!(assigned[0]["title"], "홍길동 · 업무 배정 · 알림 확인");
    ok(
        &member,
        Method::PATCH,
        &format!("/v1/pms/tasks/{id}"),
        Some(json!({"status": "in_progress"})),
    )
    .await;
    let status: Vec<Value> = notifications_for(id)
        .into_iter()
        .filter(|n| n["kind"] == "status")
        .collect();
    let mut recipients: Vec<String> = status[0]["recipients"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    recipients.sort();
    let mut expected = vec![f.owner.clone(), follower.clone()];
    expected.sort();
    assert_eq!(recipients, expected, "the actor is left out");
    assert!(status[0]["title"].as_str().unwrap().ends_with("· 진행 중"));
    // A comment notifies the task's people; a mention outside the space is
    // not sent (they could not open it).
    ok(
        &follower,
        Method::POST,
        &format!("/v1/pms/tasks/{id}/comments"),
        Some(json!({"body": format!("@{outsider} 확인해 주세요")})),
    )
    .await;
    let comments: Vec<Value> = notifications_for(id)
        .into_iter()
        .filter(|n| n["kind"] == "comment")
        .collect();
    assert_eq!(comments.len(), 1);
    assert!(!notifications_for(id).iter().any(|n| n["kind"] == "mention"));
    add_member(&f.owner, &f.space, &outsider, "viewer").await;
    ok(
        &follower,
        Method::POST,
        &format!("/v1/pms/tasks/{id}/comments"),
        Some(json!({"body": format!("@{outsider} 다시 확인")})),
    )
    .await;
    let mention: Vec<Value> = notifications_for(id)
        .into_iter()
        .filter(|n| n["kind"] == "mention")
        .collect();
    assert_eq!(mention[0]["recipients"], json!([outsider]));
    // Opening the task marks the caller's line read.
    ok(&member, Method::GET, &format!("/v1/pms/tasks/{id}"), None).await;
    assert!(
        core()
            .lock()
            .unwrap()
            .read
            .iter()
            .any(|(user, key)| *user == member && *key == format!("task:{id}"))
    );
    // Removing a member retracts their lines; deleting the task retracts all.
    ok(
        &f.owner,
        Method::DELETE,
        &format!("/v1/pms/spaces/{}/members/{member}", f.space),
        None,
    )
    .await;
    assert!(
        core()
            .lock()
            .unwrap()
            .retracted
            .iter()
            .any(|r| r["groupKey"] == format!("task:{id}") && r["recipients"] == json!([member]))
    );
    ok(
        &f.owner,
        Method::DELETE,
        &format!("/v1/pms/tasks/{id}"),
        None,
    )
    .await;
    assert!(
        core()
            .lock()
            .unwrap()
            .retracted
            .iter()
            .any(|r| r["groupKey"] == format!("task:{id}") && r.get("recipients").is_none())
    );
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn space_membership_writes_check_role_after_the_lock_wait() {
    let f = fixture().await;
    let manager = new_user("Manager");
    add_member(&f.owner, &f.space, &manager, "admin").await;
    let invitee = new_user("Invitee");

    let mut tx = server().1.begin().await.unwrap();
    let blocker = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM pms_spaces WHERE id = $1 FOR UPDATE")
        .bind(&f.space)
        .execute(&mut *tx)
        .await
        .unwrap();
    let request = tokio::spawn({
        let manager = manager.clone();
        let invitee = invitee.clone();
        let path = format!("/v1/pms/spaces/{}/members", f.space);
        async move {
            call(
                &manager,
                Method::POST,
                &path,
                Some(json!({"user_id": invitee})),
            )
            .await
        }
    });
    wait_for_query_blocked_by(blocker, "%FROM pms_spaces%FOR UPDATE%").await;
    sqlx::query(
        "UPDATE pms_space_members SET role = 'viewer' WHERE space_id = $1 AND user_id = $2",
    )
    .bind(&f.space)
    .bind(&manager)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(
        error_code(request.await.unwrap()).await,
        (
            StatusCode::FORBIDDEN,
            "pms_space_owner_admin_required".into()
        )
    );
    let invited: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pms_space_members WHERE space_id = $1 AND user_id = $2)",
    )
    .bind(&f.space)
    .bind(&invitee)
    .fetch_one(&server().1)
    .await
    .unwrap();
    assert!(!invited);
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn a_single_task_grant_reads_that_task_only() {
    let f = fixture().await;
    let guest = new_user("Guest");
    let parent = create_task(&f.owner, &f.list, json!({"title": "Hidden parent"})).await;
    let shared = create_task(
        &f.owner,
        &f.list,
        json!({"title": "Shared", "parent_id": parent["id"]}),
    )
    .await;
    let shared_id = shared["id"].as_str().unwrap();
    let other = create_task(
        &f.owner,
        &f.list,
        json!({"title": "Hidden child", "parent_id": shared_id}),
    )
    .await;
    let shared_child = create_task(
        &f.owner,
        &f.list,
        json!({"title": "Shared child", "parent_id": shared_id}),
    )
    .await;
    let shared_child_id = shared_child["id"].as_str().unwrap();
    create_task(
        &f.owner,
        &f.list,
        json!({"title": "Hidden grandchild", "parent_id": shared_child_id}),
    )
    .await;
    for task_id in [shared_id, shared_child_id] {
        sqlx::query(
            "INSERT INTO pms_task_user_access (id, task_id, user_id, granted_by_user_id, reason, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, 'explicit', now(), now())",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(task_id)
        .bind(&guest)
        .bind(&f.owner)
        .execute(&server().1)
        .await
        .unwrap();
    }
    let detail = ok(
        &guest,
        Method::GET,
        &format!("/v1/pms/tasks/{shared_id}"),
        None,
    )
    .await;
    assert!(detail["task"]["parent_id"].is_null());
    assert_eq!(detail["task"]["subtask_count"], 1);
    let children = detail["subtasks"].as_array().unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0]["id"], shared_child_id);
    assert_eq!(children[0]["subtask_count"], 0);
    ok(
        &guest,
        Method::GET,
        &format!("/v1/pms/tasks/{shared_id}/activity-logs"),
        None,
    )
    .await;
    let response = call(
        &guest,
        Method::GET,
        &format!("/v1/pms/tasks/{}", other["id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(error_code(response).await.1, "pms_task_access_required");
    let response = call(
        &guest,
        Method::GET,
        &format!("/v1/pms/lists/{}", f.list),
        None,
    )
    .await;
    assert_eq!(
        error_code(response).await.1,
        "pms_task_list_access_required"
    );
    let response = call(
        &guest,
        Method::PATCH,
        &format!("/v1/pms/tasks/{shared_id}"),
        Some(json!({"title": "No"})),
    )
    .await;
    assert_eq!(
        error_code(response).await.1,
        "pms_task_list_access_required"
    );
    // The notification check allows the shared task and nothing else.
    let check = send(
        &guest,
        false,
        true,
        Method::POST,
        "/platform/notification-check",
        Some(json!({"items": [
            {"groupKey": format!("task:{shared_id}"), "resourceId": shared_id},
            {"groupKey": format!("task:{}", other["id"].as_str().unwrap())},
            {"groupKey": "other:1"},
        ]})),
    )
    .await;
    assert_eq!(check.status(), StatusCode::OK);
    assert_eq!(
        check.json::<Value>().await.unwrap(),
        json!({"allowed": [format!("task:{shared_id}")]})
    );
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn platform_endpoints_answer_platform_calls_only() {
    let f = fixture().await;
    let today = crate::home_card::local_today(Some("Asia/Seoul"));
    let yesterday = today - chrono::Duration::days(1);
    create_task(&f.owner, &f.list, json!({"title": "검색 대상 오늘", "assignee_ids": [f.owner], "due_date": today.to_string()})).await;
    create_task(
        &f.owner,
        &f.list,
        json!({"title": "어제 마감", "assignee_ids": [f.owner], "due_date": yesterday.to_string()}),
    )
    .await;
    for path in [
        "/v1/home-card",
        "/platform/notification-check",
        "/v1/search",
    ] {
        let response = call(&f.owner, Method::POST, path, Some(json!({}))).await;
        assert_eq!(
            error_code(response).await,
            (StatusCode::FORBIDDEN, "pms_platform_only".into()),
            "{path}"
        );
    }
    let card = send(
        &f.owner,
        false,
        true,
        Method::POST,
        "/v1/home-card",
        Some(json!({"timeZone": "Asia/Seoul", "locale": "ko-KR"})),
    )
    .await;
    let card: Value = card.json().await.unwrap();
    assert_eq!(card["title"], "내 업무 · 오늘 1 · 지연 1");
    assert_eq!(card["count"], 2);
    assert_eq!(card["items"][0]["title"], "어제 마감");
    assert!(
        card["items"][0]["path"]
            .as_str()
            .unwrap()
            .starts_with("/tasks/")
    );
    let search = send(
        &f.owner,
        false,
        true,
        Method::POST,
        "/v1/search",
        Some(json!({"query": "검색", "limit": 5, "timeZone": "Asia/Seoul", "locale": "ko-KR"})),
    )
    .await;
    let search: Value = search.json().await.unwrap();
    assert_eq!(search["items"][0]["title"], "검색 대상 오늘");
    assert_eq!(search["items"][0]["kind"], "업무");
    let stranger = new_user("Stranger");
    let search = send(
        &stranger,
        false,
        true,
        Method::POST,
        "/v1/search",
        Some(json!({"query": "검색"})),
    )
    .await;
    assert_eq!(search.json::<Value>().await.unwrap()["items"], json!([]));
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn assigned_views_hide_a_task_when_its_space_membership_is_removed() {
    let f = fixture().await;
    let member = new_user("Member");
    add_member(&f.owner, &f.space, &member, "member").await;
    let today = crate::home_card::local_today(Some("Asia/Seoul"));
    let task = create_task(
        &f.owner,
        &f.list,
        json!({"title": "Assigned", "assignee_ids": [member], "due_date": today.to_string()}),
    )
    .await;
    let task_id = task["id"].as_str().unwrap();
    let card = send(
        &member,
        false,
        true,
        Method::POST,
        "/v1/home-card",
        Some(json!({"timeZone": "Asia/Seoul"})),
    )
    .await;
    assert_eq!(card.status(), StatusCode::OK);
    assert_eq!(card.json::<Value>().await.unwrap()["count"], 1);

    ok(
        &f.owner,
        Method::DELETE,
        &format!("/v1/pms/spaces/{}/members/{member}", f.space),
        None,
    )
    .await;
    let assignment_remains: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pms_task_assignees WHERE task_id = $1 AND user_id = $2)",
    )
    .bind(task_id)
    .bind(&member)
    .fetch_one(&server().1)
    .await
    .unwrap();
    assert!(assignment_remains);
    for path in [
        "/v1/pms/tasks/assigned",
        &format!("/v1/pms/tasks/today-overdue?today={today}"),
        "/v1/personal-widgets/pms/tasks/assigned",
    ] {
        let response = ok(&member, Method::GET, path, None).await;
        assert_eq!(response["total"], 0, "{path}");
        assert_eq!(response["items"], json!([]), "{path}");
    }
    let card = send(&member, false, true, Method::POST, "/v1/home-card", None).await;
    assert_eq!(card.status(), StatusCode::OK);
    let card: Value = card.json().await.unwrap();
    assert_eq!(card["count"], 0);
    assert_eq!(card["items"], json!([]));
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn attachments_upload_download_and_release_through_the_platform() {
    let f = fixture().await;
    let task = create_task(&f.owner, &f.list, json!({"title": "With file"})).await;
    let id = task["id"].as_str().unwrap();
    let request_id = uuid::Uuid::new_v4().to_string();
    let pending = ok(&f.owner, Method::POST, &format!("/v1/pms/tasks/{id}/attachments"),
        Some(json!({"request_id": request_id, "filename": "보고서.txt", "content_type": "text/plain", "size_bytes": 5}))).await;
    assert_eq!(pending["status"], "pending");
    let attachment_id = pending["id"].as_str().unwrap();
    let response = call(&f.owner, Method::POST, &format!("/v1/pms/tasks/{id}/attachments"),
        Some(json!({"request_id": uuid::Uuid::new_v4().to_string(), "filename": "big.bin", "size_bytes": 10 * 1024 * 1024 + 1}))).await;
    assert_eq!(
        error_code(response).await,
        (
            StatusCode::PAYLOAD_TOO_LARGE,
            "pms_file_size_limit_exceeded".into()
        )
    );
    let uploaded = Client::new()
        .put(format!(
            "{}/api/files/pms/attachments/{attachment_id}/content",
            server().0
        ))
        .header("x-miy-identity", identity_with(&f.owner, false, false))
        .header("content-type", "application/octet-stream")
        .body("hello")
        .send()
        .await
        .unwrap();
    assert_eq!(uploaded.status(), StatusCode::OK);
    let item: Value = uploaded.json().await.unwrap();
    assert_eq!(
        item["download_url"],
        format!("/pms/attachments/{attachment_id}/content")
    );
    assert_eq!(item["uploaded_by_name"], "홍길동");
    let detail = ok(&f.owner, Method::GET, &format!("/v1/pms/tasks/{id}"), None).await;
    assert_eq!(detail["attachments"][0]["filename"], "보고서.txt");
    let downloaded = Client::new()
        .get(format!(
            "{}/api/files/pms/attachments/{attachment_id}/content",
            server().0
        ))
        .header("x-miy-identity", identity_with(&f.owner, false, false))
        .send()
        .await
        .unwrap();
    assert_eq!(downloaded.status(), StatusCode::OK);
    assert_eq!(downloaded.bytes().await.unwrap().as_ref(), b"hello");
    let stranger = new_user("Stranger");
    let response = Client::new()
        .get(format!(
            "{}/api/files/pms/attachments/{attachment_id}/content",
            server().0
        ))
        .header("x-miy-identity", identity_with(&stranger, false, false))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    ok(
        &f.owner,
        Method::DELETE,
        &format!("/v1/pms/attachments/{attachment_id}"),
        None,
    )
    .await;
    let state = core().lock().unwrap();
    let file = state
        .files
        .values()
        .find(|(meta, _)| meta["requestId"] == request_id.as_str())
        .unwrap();
    assert_eq!(
        file.0["status"], "cancelled",
        "the platform file is released"
    );
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn attachment_finalization_checks_editor_role_after_the_lock_wait() {
    let f = fixture().await;
    let editor = new_user("Editor");
    add_member(&f.owner, &f.space, &editor, "member").await;
    let task = create_task(&f.owner, &f.list, json!({"title": "Upload in progress"})).await;
    let id = task["id"].as_str().unwrap();
    let request_id = uuid::Uuid::new_v4().to_string();
    let pending = ok(
        &editor,
        Method::POST,
        &format!("/v1/pms/tasks/{id}/attachments"),
        Some(json!({"request_id": request_id, "filename": "draft.txt", "size_bytes": 5})),
    )
    .await;
    let attachment_id = pending["id"].as_str().unwrap();

    let mut tx = server().1.begin().await.unwrap();
    let blocker = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM pms_attachments WHERE id = $1 FOR UPDATE")
        .bind(attachment_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    let upload = tokio::spawn({
        let url = format!(
            "{}/api/files/pms/attachments/{attachment_id}/content",
            server().0
        );
        let identity = identity_with(&editor, false, false);
        async move {
            Client::new()
                .put(url)
                .header("x-miy-identity", identity)
                .header("content-type", "application/octet-stream")
                .body("hello")
                .send()
                .await
                .unwrap()
        }
    });
    wait_for_query_blocked_by(blocker, "%FROM pms_attachments WHERE id = $1 FOR UPDATE%").await;
    sqlx::query(
        "UPDATE pms_space_members SET role = 'viewer' WHERE space_id = $1 AND user_id = $2",
    )
    .bind(&f.space)
    .bind(&editor)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(
        error_code(upload.await.unwrap()).await,
        (
            StatusCode::FORBIDDEN,
            "pms_task_list_viewer_modify_denied".into()
        )
    );
    let status: String = sqlx::query_scalar("SELECT status FROM pms_attachments WHERE id = $1")
        .bind(attachment_id)
        .fetch_one(&server().1)
        .await
        .unwrap();
    assert_eq!(status, "pending");
    let state = core().lock().unwrap();
    let (file, _) = state
        .files
        .values()
        .find(|(meta, _)| meta["requestId"] == request_id.as_str())
        .unwrap();
    assert_eq!(file["status"], "ready");
    assert!(file["reference"].is_null());
}

#[tokio::test]
#[ignore = "needs PMS_TEST_DATABASE_URL"]
async fn the_frame_learns_the_current_user_and_the_directory() {
    let user = new_user("본인");
    let me = ok(&user, Method::GET, "/v1/pms/me", None).await;
    assert_eq!(
        me,
        json!({"id": user, "name": "본인", "primary_org_unit_id": "g-dev", "primary_org_unit_name": "개발팀"})
    );
    let users = ok(&user, Method::GET, "/v1/pms/users", None).await;
    assert!(
        users
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["id"] == user.as_str() && u["email"] == "")
    );
    let response = Client::new()
        .get(format!("{}/api/v1/pms/me", server().0))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
