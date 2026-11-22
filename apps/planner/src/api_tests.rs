//! HTTP tests through the real SDK gateway-identity check and PostgreSQL.
//!
//! Ports of AI-DO apps/api/tests/test_planner_events.py (planner parts),
//! test_calendar_events.py (contract parts that apply to the planner source)
//! and test_personal_widgets.py. Meeting/PMS/availability cases are not ported
//! with their features.
//!
//! They need a disposable PostgreSQL server, so they are ignored by default:
//!   PLANNER_TEST_DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:PORT/postgres \
//!     cargo test -- --include-ignored
//! Each run creates a fresh database on that server and applies migrations/.
use jsonwebtoken::{EncodingKey, Header, encode};
use reqwest::{Client, Method, Response, StatusCode};
use serde_json::{Value, json};
use sqlx::{Connection, Executor, PgConnection, postgres::PgPoolOptions};
use std::sync::OnceLock;

const SECRET: &str = "planner-api-test-gateway-secret-0123456789";
const INSTALLATION: &str = "00000000-0000-4000-8000-0000000000a1";
const RELEASE: &str = "00000000-0000-4000-8000-0000000000a2";

static SERVER: OnceLock<String> = OnceLock::new();

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

async fn prepare_database(admin_url: &str) -> sqlx::PgPool {
    let name = format!("planner_test_{}", uuid::Uuid::new_v4().simple());
    let mut admin = PgConnection::connect(admin_url)
        .await
        .expect("connect to PLANNER_TEST_DATABASE_URL");
    admin
        .execute(format!("CREATE DATABASE {name}").as_str())
        .await
        .expect("create test database");
    let url = database_url_with_name(admin_url, &name);
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("connect to test database");
    // Apply migrations in order, one transaction each, as the platform runner does.
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

fn server() -> &'static str {
    SERVER.get_or_init(|| {
        let admin_url = std::env::var("PLANNER_TEST_DATABASE_URL")
            .expect("set PLANNER_TEST_DATABASE_URL to a disposable PostgreSQL server");
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
        }
        let (ready, wait) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let pool = prepare_database(&admin_url).await;
                ready.send(()).unwrap();
                miy_app_sdk::serve(crate::router(pool)).await.unwrap();
            });
        });
        wait.recv().expect("test server failed to start");
        for _ in 0..200 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return format!("http://127.0.0.1:{port}");
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        panic!("test server did not listen");
    })
}

fn new_user() -> String {
    format!("user-{}", uuid::Uuid::new_v4())
}

fn identity(user: &str) -> String {
    identity_with_platform(user, false)
}

fn identity_with_platform(user: &str, platform: bool) -> String {
    let exp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 300;
    encode(
        &Header::default(),
        &json!({"sub": user, "admin": false, "platform": platform, "installation_id": INSTALLATION,
            "release_id": RELEASE, "iss": "miy", "aud": "miy-app", "exp": exp}),
        &EncodingKey::from_secret(SECRET.as_bytes()),
    )
    .unwrap()
}

#[tokio::test]
#[ignore = "needs PLANNER_TEST_DATABASE_URL"]
async fn platform_callbacks_preserve_their_http_contract() {
    let user = new_user();
    let title = format!("Todo {}", uuid::Uuid::new_v4());
    create_todo(&user, &title).await;
    create_todo(&new_user(), &title).await;

    let body = json!({"query": title, "locale": "en-US"});
    let forbidden = call(Some(&user), Method::POST, "/v1/search", Some(body.clone())).await;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_of(forbidden).await["error"]["code"],
        "planner_platform_only"
    );

    let token = identity_with_platform(&user, true);
    let client = Client::new();
    let response = client
        .post(format!("{}/api/v1/search", server()))
        .header("x-miy-identity", &token)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_of(response).await,
        json!({"items": [{"title": title, "snippet": null, "path": "/widgets/todos", "kind": "To-do"}]})
    );

    let card = client
        .post(format!("{}/api/v1/home-card", server()))
        .header("x-miy-identity", &token)
        .send()
        .await
        .unwrap();
    assert_eq!(card.status(), StatusCode::OK);
    assert_eq!(
        json_of(card).await,
        json!({"title": "오늘 일정", "count": 0, "items": []})
    );
}

async fn call(user: Option<&str>, method: Method, path: &str, body: Option<Value>) -> Response {
    let mut request = Client::new().request(method, format!("{}/api{path}", server()));
    if let Some(user) = user {
        request = request.header("x-miy-identity", identity(user));
    }
    if let Some(body) = body {
        request = request.json(&body);
    }
    request.send().await.unwrap()
}

async fn json_of(response: Response) -> Value {
    response.json().await.unwrap()
}

async fn create_planner_event(user: &str, body: Value) -> Value {
    let response = call(Some(user), Method::POST, "/v1/planner/events", Some(body)).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    json_of(response).await
}

fn event_body(title: &str, start: &str, end: &str, all_day: bool, location: &str) -> Value {
    json!({"title": title, "description": "", "location": location, "allDay": all_day,
        "start": start, "end": end})
}

#[tokio::test]
#[ignore = "needs PLANNER_TEST_DATABASE_URL"]
async fn planner_event_crud_happy_path() {
    let user = new_user();
    let created = create_planner_event(
        &user,
        json!({"title": "고객사 방문", "description": "오프라인 미팅", "location": "성수동",
            "allDay": false, "start": "2026-05-04T01:00:00+00:00", "end": "2026-05-04T03:30:00+00:00"}),
    )
    .await;
    assert_eq!(created["title"], "고객사 방문");
    assert_eq!(created["location"], "성수동");
    assert_eq!(created["timeZone"], "Asia/Seoul");
    assert_eq!(created["allDay"], false);
    assert_eq!(created["startHasTime"], true);
    assert_eq!(created["endHasTime"], true);
    assert_eq!(created["start"], "2026-05-04T01:00:00+00:00");
    assert_eq!(created["end"], "2026-05-04T03:30:00+00:00");
    assert_eq!(created["ownerId"], user.as_str());
    let id = created["id"].as_str().unwrap().to_string();

    let detail = call(
        Some(&user),
        Method::GET,
        &format!("/v1/planner/events/{id}"),
        None,
    )
    .await;
    assert_eq!(detail.status(), StatusCode::OK);
    assert_eq!(json_of(detail).await["id"], id.as_str());

    let updated = call(
        Some(&user),
        Method::PATCH,
        &format!("/v1/planner/events/{id}"),
        Some(
            json!({"allDay": true, "start": "2026-05-06", "end": "2026-05-08", "location": "부산"}),
        ),
    )
    .await;
    assert_eq!(updated.status(), StatusCode::OK);
    let updated = json_of(updated).await;
    assert_eq!(updated["allDay"], true);
    assert_eq!(updated["startHasTime"], false);
    assert_eq!(updated["endHasTime"], false);
    assert_eq!(updated["start"], "2026-05-06");
    assert_eq!(updated["end"], "2026-05-08");
    assert_eq!(updated["location"], "부산");

    let listing = call(
        Some(&user),
        Method::GET,
        "/v1/planner/events?from=2026-05-01&to=2026-05-31",
        None,
    )
    .await;
    assert_eq!(listing.status(), StatusCode::OK);
    let ids: Vec<String> = json_of(listing).await["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect();
    assert!(ids.contains(&id));

    let deleted = call(
        Some(&user),
        Method::DELETE,
        &format!("/v1/planner/events/{id}"),
        None,
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let after = call(
        Some(&user),
        Method::GET,
        &format!("/v1/planner/events/{id}"),
        None,
    )
    .await;
    assert_eq!(after.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        json_of(after).await["error"]["code"],
        "planner_event_not_found"
    );
}

#[tokio::test]
#[ignore = "needs PLANNER_TEST_DATABASE_URL"]
async fn planner_event_owner_only_access() {
    let owner = new_user();
    let member = new_user();
    let created = create_planner_event(
        &owner,
        event_body(
            "출장",
            "2026-05-04T01:00:00+00:00",
            "2026-05-04T02:00:00+00:00",
            false,
            "",
        ),
    )
    .await;
    let path = format!("/v1/planner/events/{}", created["id"].as_str().unwrap());
    assert_eq!(
        call(Some(&member), Method::GET, &path, None).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(
            Some(&member),
            Method::PATCH,
            &path,
            Some(json!({"title": "가로채기"}))
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(Some(&member), Method::DELETE, &path, None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    // The owner still has it, unchanged.
    let detail = json_of(call(Some(&owner), Method::GET, &path, None).await).await;
    assert_eq!(detail["title"], "출장");
}

#[tokio::test]
#[ignore = "needs PLANNER_TEST_DATABASE_URL"]
async fn calendar_events_include_only_current_user_planner_events() {
    let owner = new_user();
    let member = new_user();
    let own = create_planner_event(
        &owner,
        event_body(
            "나의 일정",
            "2026-05-11T00:00:00+00:00",
            "2026-05-11T02:00:00+00:00",
            false,
            "판교",
        ),
    )
    .await;
    create_planner_event(
        &member,
        event_body(
            "다른 사람 공개 일정",
            "2026-05-11T03:00:00+00:00",
            "2026-05-11T04:00:00+00:00",
            false,
            "강남",
        ),
    )
    .await;
    let response = call(
        Some(&owner),
        Method::GET,
        "/v1/calendar/events?from=2026-05-01&to=2026-06-01&sources=planner_event",
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let items = json_of(response).await["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item["sourceType"], "planner_event");
    assert_eq!(item["sourceId"], own["id"]);
    assert_eq!(item["metadata"]["plannerEventId"], own["id"]);
    assert!(item["workspace"].is_null());
    assert_eq!(item["metadata"]["location"], "판교");
    assert_eq!(item["metadata"]["plannerAllDay"], false);
    assert_eq!(item["metadata"]["plannerStartHasTime"], true);
    assert_eq!(item["metadata"]["plannerEndHasTime"], true);
}

#[tokio::test]
#[ignore = "needs PLANNER_TEST_DATABASE_URL"]
async fn planner_event_accepts_partial_or_unspecified_times() {
    let user = new_user();
    let start_only = create_planner_event(
        &user,
        event_body(
            "시작만 있는 일정",
            "2026-05-12T09:00:00+09:00",
            "2026-05-12",
            false,
            "",
        ),
    )
    .await;
    assert_eq!(start_only["allDay"], false);
    assert_eq!(start_only["startHasTime"], true);
    assert_eq!(start_only["endHasTime"], false);
    assert_eq!(start_only["start"], "2026-05-12T00:00:00+00:00");
    assert_eq!(start_only["end"], "2026-05-12");

    let end_only = create_planner_event(
        &user,
        event_body(
            "종료만 있는 일정",
            "2026-05-13",
            "2026-05-13T18:00:00+09:00",
            false,
            "",
        ),
    )
    .await;
    assert_eq!(end_only["startHasTime"], false);
    assert_eq!(end_only["endHasTime"], true);
    assert_eq!(end_only["start"], "2026-05-13");
    assert_eq!(end_only["end"], "2026-05-13T09:00:00+00:00");

    let no_time = create_planner_event(
        &user,
        event_body("시간 미정 일정", "2026-05-14", "2026-05-14", false, ""),
    )
    .await;
    assert_eq!(no_time["startHasTime"], false);
    assert_eq!(no_time["endHasTime"], false);
    assert_eq!(no_time["start"], "2026-05-14");
    assert_eq!(no_time["end"], "2026-05-14");

    let calendar = call(
        Some(&user),
        Method::GET,
        "/v1/calendar/events?from=2026-05-12&to=2026-05-15&sources=planner_event",
        None,
    )
    .await;
    assert_eq!(calendar.status(), StatusCode::OK);
    let items = json_of(calendar).await["items"].as_array().unwrap().clone();
    let by_title = |title: &str| {
        items
            .iter()
            .find(|item| item["title"] == title)
            .cloned()
            .unwrap()
    };
    assert_eq!(by_title("시작만 있는 일정")["allDay"], false);
    assert_eq!(
        by_title("시작만 있는 일정")["metadata"]["plannerEndHasTime"],
        false
    );
    assert_eq!(by_title("종료만 있는 일정")["allDay"], false);
    assert_eq!(
        by_title("종료만 있는 일정")["metadata"]["plannerStartHasTime"],
        false
    );
    assert_eq!(by_title("시간 미정 일정")["allDay"], true);
    assert_eq!(
        by_title("시간 미정 일정")["metadata"]["plannerAllDay"],
        false
    );
}

#[tokio::test]
#[ignore = "needs PLANNER_TEST_DATABASE_URL"]
async fn planner_event_stores_the_callers_time_zone() {
    let user = new_user();
    let response = call(
        Some(&user),
        Method::POST,
        "/v1/planner/events?timeZone=Europe/Berlin",
        Some(event_body("Offsite", "2026-07-01", "2026-07-03", true, "")),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let created = json_of(response).await;
    assert_eq!(created["timeZone"], "Europe/Berlin");
    assert_eq!(created["start"], "2026-07-01");
    assert_eq!(created["end"], "2026-07-03");
    // Berlin midnight (UTC+2) is 22:00 UTC the day before: a UTC range
    // starting on 2026-07-01 still overlaps it.
    let listing = json_of(
        call(
            Some(&user),
            Method::GET,
            "/v1/planner/events?from=2026-06-30T21:00:00Z&to=2026-06-30T23:00:00Z",
            None,
        )
        .await,
    )
    .await;
    assert_eq!(listing["items"].as_array().unwrap().len(), 1);

    let unknown = json_of(
        call(
            Some(&user),
            Method::POST,
            "/v1/planner/events?timeZone=Not/AZone",
            Some(event_body("x", "2026-07-01", "2026-07-02", true, "")),
        )
        .await,
    )
    .await;
    assert_eq!(unknown["timeZone"], "Asia/Seoul");
}

#[tokio::test]
#[ignore = "needs PLANNER_TEST_DATABASE_URL"]
async fn planner_event_validation_and_business_errors() {
    let user = new_user();
    let too_long = call(
        Some(&user),
        Method::POST,
        "/v1/planner/events",
        Some(event_body(
            &"x".repeat(201),
            "2026-05-04",
            "2026-05-05",
            true,
            "",
        )),
    )
    .await;
    assert_eq!(too_long.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        json_of(too_long).await["error"]["code"],
        "validation_request_invalid"
    );

    let missing = call(
        Some(&user),
        Method::POST,
        "/v1/planner/events",
        Some(json!({"title": "x"})),
    )
    .await;
    assert_eq!(missing.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let reversed = call(
        Some(&user),
        Method::POST,
        "/v1/planner/events",
        Some(event_body("x", "2026-05-05", "2026-05-04", true, "")),
    )
    .await;
    assert_eq!(reversed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_of(reversed).await["error"]["code"],
        "planner_event_end_after_start"
    );

    let one_sided = call(
        Some(&user),
        Method::GET,
        "/v1/planner/events?from=2026-05-01",
        None,
    )
    .await;
    assert_eq!(one_sided.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_of(one_sided).await["error"]["code"],
        "planner_list_range_required"
    );

    let created = create_planner_event(
        &user,
        event_body(
            "x",
            "2026-05-04T01:00:00Z",
            "2026-05-04T02:00:00Z",
            false,
            "",
        ),
    )
    .await;
    let partial = call(
        Some(&user),
        Method::PATCH,
        &format!("/v1/planner/events/{}", created["id"].as_str().unwrap()),
        Some(json!({"allDay": true, "start": "2026-05-04"})),
    )
    .await;
    assert_eq!(partial.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_of(partial).await["error"]["code"],
        "planner_update_start_end_required"
    );
}

#[tokio::test]
#[ignore = "needs PLANNER_TEST_DATABASE_URL"]
async fn calendar_events_requires_auth() {
    let response = call(
        None,
        Method::GET,
        "/v1/calendar/events?from=2026-04-01&to=2026-05-01",
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "needs PLANNER_TEST_DATABASE_URL"]
async fn calendar_events_query_policy() {
    let user = new_user();
    let get = |query: &'static str| {
        let user = user.clone();
        async move {
            call(
                Some(&user),
                Method::GET,
                &format!("/v1/calendar/events?{query}"),
                None,
            )
            .await
        }
    };

    let cap = get("from=2020-01-01&to=2099-12-31").await;
    assert_eq!(cap.status(), StatusCode::BAD_REQUEST);
    assert!(
        json_of(cap).await["error"]["message"]
            .as_str()
            .unwrap()
            .contains("366")
    );

    let bogus = get("from=2026-04-01&to=2026-05-01&sources=planner_event,bogus").await;
    assert_eq!(bogus.status(), StatusCode::BAD_REQUEST);
    assert!(
        json_of(bogus).await["error"]["message"]
            .as_str()
            .unwrap()
            .contains("bogus")
    );

    let empty = get("from=2026-04-01&to=2026-05-01&sources=").await;
    assert_eq!(empty.status(), StatusCode::OK);
    assert_eq!(json_of(empty).await, json!({"items": []}));

    let invalid = get("from=not%20a%20date&to=also%20not").await;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

    let reversed = get("from=2026-04-30&to=2026-04-01").await;
    assert_eq!(reversed.status(), StatusCode::BAD_REQUEST);

    let missing = get("from=2026-04-01").await;
    assert_eq!(missing.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
#[ignore = "needs PLANNER_TEST_DATABASE_URL"]
async fn calendar_events_date_only_and_offset_ranges_match() {
    let user = new_user();
    create_planner_event(
        &user,
        event_body(
            "Offset range event",
            "2026-04-15T01:00:00+00:00",
            "2026-04-15T02:00:00+00:00",
            false,
            "",
        ),
    )
    .await;
    let date_only = call(
        Some(&user),
        Method::GET,
        "/v1/calendar/events?from=2026-04-15&to=2026-04-16&sources=planner_event",
        None,
    )
    .await;
    let offset = call(
        Some(&user),
        Method::GET,
        "/v1/calendar/events?from=2026-04-15T00:00:00%2B09:00&to=2026-04-16T00:00:00%2B09:00&sources=planner_event",
        None,
    )
    .await;
    assert_eq!(date_only.status(), StatusCode::OK);
    assert_eq!(offset.status(), StatusCode::OK);
    let date_only = json_of(date_only).await;
    assert_eq!(date_only["items"].as_array().unwrap().len(), 1);
    assert_eq!(date_only, json_of(offset).await);
}

#[tokio::test]
#[ignore = "needs PLANNER_TEST_DATABASE_URL"]
async fn calendar_events_assignee_id_query_param_is_ignored() {
    let owner = new_user();
    let other = new_user();
    create_planner_event(
        &other,
        event_body(
            "other",
            "2026-04-15T01:00:00Z",
            "2026-04-15T02:00:00Z",
            false,
            "",
        ),
    )
    .await;
    let response = call(
        Some(&owner),
        Method::GET,
        &format!("/v1/calendar/events?from=2026-04-01&to=2026-05-01&assignee_id={other}"),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_of(response).await, json!({"items": []}));
}

async fn create_todo(user: &str, title: &str) -> Value {
    let response = call(
        Some(user),
        Method::POST,
        "/v1/personal-widgets/todos",
        Some(json!({"title": title})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    json_of(response).await
}

fn ids(list: &Value) -> Vec<String> {
    list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
#[ignore = "needs PLANNER_TEST_DATABASE_URL"]
async fn personal_todo_crud_and_completed_filter() {
    let user = new_user();
    let first = create_todo(&user, "  First task  ").await;
    let second = create_todo(&user, "Second task").await;
    assert_eq!(first["title"], "First task");
    assert_eq!(first["completed"], false);
    assert!(first["completedAt"].is_null());
    assert_eq!(first["sortOrder"], 1000);
    assert_eq!(second["sortOrder"], 2000);

    let updated = call(
        Some(&user),
        Method::PATCH,
        &format!(
            "/v1/personal-widgets/todos/{}",
            first["id"].as_str().unwrap()
        ),
        Some(json!({"completed": true, "title": "Done task"})),
    )
    .await;
    assert_eq!(updated.status(), StatusCode::OK);
    let updated = json_of(updated).await;
    assert_eq!(updated["title"], "Done task");
    assert_eq!(updated["completed"], true);
    assert!(updated["completedAt"].is_string());

    let list =
        json_of(call(Some(&user), Method::GET, "/v1/personal-widgets/todos", None).await).await;
    assert_eq!(
        ids(&list),
        vec![
            second["id"].as_str().unwrap().to_string(),
            first["id"].as_str().unwrap().to_string()
        ]
    );

    let active = json_of(
        call(
            Some(&user),
            Method::GET,
            "/v1/personal-widgets/todos?include_completed=false",
            None,
        )
        .await,
    )
    .await;
    assert_eq!(
        ids(&active),
        vec![second["id"].as_str().unwrap().to_string()]
    );

    let deleted = call(
        Some(&user),
        Method::DELETE,
        &format!(
            "/v1/personal-widgets/todos/{}",
            second["id"].as_str().unwrap()
        ),
        None,
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let remaining =
        json_of(call(Some(&user), Method::GET, "/v1/personal-widgets/todos", None).await).await;
    assert_eq!(
        ids(&remaining),
        vec![first["id"].as_str().unwrap().to_string()]
    );

    let reopened = json_of(
        call(
            Some(&user),
            Method::PATCH,
            &format!(
                "/v1/personal-widgets/todos/{}",
                first["id"].as_str().unwrap()
            ),
            Some(json!({"completed": false})),
        )
        .await,
    )
    .await;
    assert!(reopened["completedAt"].is_null());
}

#[tokio::test]
#[ignore = "needs PLANNER_TEST_DATABASE_URL"]
async fn personal_todos_are_isolated_by_user() {
    let owner = new_user();
    let other = new_user();
    let todo = create_todo(&owner, "Private task").await;
    let other_list = json_of(
        call(
            Some(&other),
            Method::GET,
            "/v1/personal-widgets/todos",
            None,
        )
        .await,
    )
    .await;
    assert_eq!(other_list, json!({"items": []}));
    let path = format!(
        "/v1/personal-widgets/todos/{}",
        todo["id"].as_str().unwrap()
    );
    assert_eq!(
        call(
            Some(&other),
            Method::PATCH,
            &path,
            Some(json!({"completed": true}))
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        call(Some(&other), Method::DELETE, &path, None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    let owner_list = json_of(
        call(
            Some(&owner),
            Method::GET,
            "/v1/personal-widgets/todos",
            None,
        )
        .await,
    )
    .await;
    assert_eq!(
        ids(&owner_list),
        vec![todo["id"].as_str().unwrap().to_string()]
    );
}

#[tokio::test]
#[ignore = "needs PLANNER_TEST_DATABASE_URL"]
async fn personal_todo_title_validation() {
    let user = new_user();
    let blank = call(
        Some(&user),
        Method::POST,
        "/v1/personal-widgets/todos",
        Some(json!({"title": "   "})),
    )
    .await;
    assert_eq!(blank.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let long = call(
        Some(&user),
        Method::POST,
        "/v1/personal-widgets/todos",
        Some(json!({"title": "x".repeat(241)})),
    )
    .await;
    assert_eq!(long.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bad_filter = call(
        Some(&user),
        Method::GET,
        "/v1/personal-widgets/todos?include_completed=maybe",
        None,
    )
    .await;
    assert_eq!(bad_filter.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
#[ignore = "needs PLANNER_TEST_DATABASE_URL"]
async fn personal_memo_get_and_save() {
    let user = new_user();
    let empty = call(Some(&user), Method::GET, "/v1/personal-widgets/memo", None).await;
    assert_eq!(empty.status(), StatusCode::OK);
    assert_eq!(
        json_of(empty).await,
        json!({"id": null, "body": "", "createdAt": null, "updatedAt": null})
    );

    let saved = call(
        Some(&user),
        Method::PUT,
        "/v1/personal-widgets/memo",
        Some(json!({"body": "Line one\nLine two", "expectedUpdatedAt": null})),
    )
    .await;
    assert_eq!(saved.status(), StatusCode::OK);
    let saved = json_of(saved).await;
    assert!(saved["id"].is_string());
    assert_eq!(saved["body"], "Line one\nLine two");
    assert!(saved["createdAt"].is_string());
    assert!(saved["updatedAt"].is_string());

    let updated = json_of(
        call(
            Some(&user),
            Method::PUT,
            "/v1/personal-widgets/memo",
            Some(json!({"body": "Updated memo", "expectedUpdatedAt": saved["updatedAt"]})),
        )
        .await,
    )
    .await;
    assert_eq!(updated["id"], saved["id"]);
    assert_eq!(updated["body"], "Updated memo");

    let fetched =
        json_of(call(Some(&user), Method::GET, "/v1/personal-widgets/memo", None).await).await;
    assert_eq!(fetched["body"], "Updated memo");

    let too_long = call(
        Some(&user),
        Method::PUT,
        "/v1/personal-widgets/memo",
        Some(json!({"body": "x".repeat(20001), "expectedUpdatedAt": fetched["updatedAt"]})),
    )
    .await;
    assert_eq!(too_long.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
#[ignore = "needs PLANNER_TEST_DATABASE_URL"]
async fn personal_memo_is_isolated_by_user() {
    let owner = new_user();
    let other = new_user();
    let owner_save = call(
        Some(&owner),
        Method::PUT,
        "/v1/personal-widgets/memo",
        Some(json!({"body": "Owner private memo", "expectedUpdatedAt": null})),
    )
    .await;
    assert_eq!(owner_save.status(), StatusCode::OK);
    let other_get =
        json_of(call(Some(&other), Method::GET, "/v1/personal-widgets/memo", None).await).await;
    assert_eq!(other_get["body"], "");
    let other_save = call(
        Some(&other),
        Method::PUT,
        "/v1/personal-widgets/memo",
        Some(json!({"body": "Other private memo", "expectedUpdatedAt": null})),
    )
    .await;
    assert_eq!(other_save.status(), StatusCode::OK);
    let owner_get =
        json_of(call(Some(&owner), Method::GET, "/v1/personal-widgets/memo", None).await).await;
    assert_eq!(owner_get["body"], "Owner private memo");
}

#[tokio::test]
#[ignore = "needs PLANNER_TEST_DATABASE_URL"]
async fn competing_memo_saves_use_the_same_revision_once() {
    let user = new_user();
    let path = "/v1/personal-widgets/memo";
    let (first, second) = tokio::join!(
        call(
            Some(&user),
            Method::PUT,
            path,
            Some(json!({"body": "First", "expectedUpdatedAt": null}))
        ),
        call(
            Some(&user),
            Method::PUT,
            path,
            Some(json!({"body": "Second", "expectedUpdatedAt": null}))
        ),
    );
    let (saved, conflict) = if first.status() == StatusCode::OK {
        (first, second)
    } else {
        (second, first)
    };
    assert_eq!(saved.status(), StatusCode::OK);
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    assert_eq!(
        json_of(conflict).await["error"]["code"],
        "personal_widgets_memo_conflict"
    );
    let saved = json_of(saved).await;

    let (first, second) = tokio::join!(
        call(
            Some(&user),
            Method::PUT,
            path,
            Some(json!({"body": "Edit A", "expectedUpdatedAt": saved["updatedAt"]}))
        ),
        call(
            Some(&user),
            Method::PUT,
            path,
            Some(json!({"body": "Edit B", "expectedUpdatedAt": saved["updatedAt"]}))
        ),
    );
    let (updated, conflict) = if first.status() == StatusCode::OK {
        (first, second)
    } else {
        (second, first)
    };
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
    let updated = json_of(updated).await;
    assert_ne!(saved["updatedAt"], updated["updatedAt"]);
    assert_eq!(
        json_of(call(Some(&user), Method::GET, path, None).await).await,
        updated
    );
}

#[tokio::test]
#[ignore = "needs PLANNER_TEST_DATABASE_URL"]
async fn requests_without_a_gateway_identity_are_refused() {
    for path in [
        "/v1/planner/events",
        "/v1/personal-widgets/todos",
        "/v1/personal-widgets/memo",
    ] {
        let response = call(None, Method::GET, path, None).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
    // A forged identity (wrong secret) is refused too.
    let forged = encode(
        &Header::default(),
        &json!({"sub": "intruder", "admin": true, "installation_id": INSTALLATION,
            "release_id": RELEASE, "iss": "miy", "aud": "miy-app", "exp": 4_102_444_800u64}),
        &EncodingKey::from_secret(b"not-the-gateway-secret-not-the-gateway"),
    )
    .unwrap();
    let response = Client::new()
        .get(format!("{}/api/v1/personal-widgets/memo", server()))
        .header("x-miy-identity", forged)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn database_urls_keep_credentials_and_options() {
    assert_eq!(
        database_url_with_name("postgres://u:p@127.0.0.1:5432/postgres", "t1"),
        "postgres://u:p@127.0.0.1:5432/t1"
    );
    assert_eq!(
        database_url_with_name("postgres://u:p@h:1/postgres?sslmode=disable", "t1"),
        "postgres://u:p@h:1/t1?sslmode=disable"
    );
}
