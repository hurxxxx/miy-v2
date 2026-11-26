//! Run only through scripts/test-sdk-acl.py against its disposable database.
use super::*;
use axum::{
    Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::get,
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
const INSTALLATION: &str = "00000000-0000-4000-8000-000000000001";
const OTHER_INSTALLATION: &str = "00000000-0000-4000-8000-000000000002";
const RELEASE: &str = "00000000-0000-4000-8000-000000000003";
const GROUP: &str = "00000000-0000-4000-8000-000000000004";
#[derive(Clone)]
struct Directory {
    users: BTreeSet<String>,
    groups: BTreeSet<String>,
    membership: BTreeMap<String, BTreeSet<String>>,
    unavailable: bool,
    wrong_scope: bool,
    requests: usize,
}
async fn directory(
    State(state): State<Arc<Mutex<Directory>>>,
    Path(installation): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let mut state = state.lock().unwrap();
    state.requests += 1;
    if state.unavailable {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let Some(token) = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer proof:"))
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some((user, environment)) = token.split_once(':') else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if !state.users.contains(user)
        || headers.get("x-miy-release").and_then(|h| h.to_str().ok()) != Some(RELEASE)
    {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let ids = query
        .get("ids")
        .map(|v| v.split(',').collect::<BTreeSet<_>>());
    let kind = query.get("kind").map(String::as_str).unwrap_or("users");
    let mut values = match kind {
        "users" => state
            .users
            .iter()
            .filter(|id| ids.as_ref().is_none_or(|ids| ids.contains(id.as_str())))
            .map(|id| json!({"id":id,"name":format!("Name {id}"),"primaryGroup":null}))
            .collect::<Vec<_>>(),
        "groups" => state
            .groups
            .iter()
            .filter(|id| ids.as_ref().is_none_or(|ids| ids.contains(id.as_str())))
            .map(|id| json!({"id":id,"name":format!("Group {id}"),"source":"local"}))
            .collect(),
        "memberships" => state
            .membership
            .get(user)
            .into_iter()
            .flatten()
            .filter(|id| {
                state.groups.contains(*id)
                    && ids.as_ref().is_none_or(|ids| ids.contains(id.as_str()))
            })
            .map(|id| json!({"id":id,"name":format!("Group {id}"),"source":"local"}))
            .collect(),
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    let offset = query
        .get("cursor")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let limit = query
        .get("limit")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(50);
    let next = (values.len() > offset + limit).then(|| (offset + limit).to_string());
    values = values.into_iter().skip(offset).take(limit).collect();
    let users = if kind == "users" {
        values.clone()
    } else {
        vec![]
    };
    let groups = if kind == "users" { vec![] } else { values };
    Json(json!({"kind":kind,"subjectId":user,"installationId":if state.wrong_scope{OTHER_INSTALLATION}else{&installation},"environment":environment,"users":users,"groups":groups,"nextCursor":next})).into_response()
}
fn identity(user: &str, environment: &str, installation: &str, admin: bool) -> Identity {
    Identity {
        user_id: user.into(),
        admin,
        context: crate::InvocationContext {
            installation_id: installation.into(),
            release_id: RELEASE.into(),
            environment: Some(environment.into()),
            callers: vec![],
            effective_permissions: vec![
                "platform:directory:read".into(),
                "platform:stewards:claim".into(),
            ],
            platform: false,
            groups: None,
        },
        delegation_token: Some(format!("proof:{user}:{environment}")),
    }
}
fn grant(kind: SubjectKind, id: &str, role: &str) -> Grant {
    Grant {
        subject: SubjectRef {
            kind,
            id: id.into(),
        },
        role: role.into(),
    }
}
fn owner_grant() -> Grant {
    grant(SubjectKind::User, "owner", "owner")
}
fn key(id: &str) -> ResourceKey {
    ResourceKey {
        kind: "projects".into(),
        id: id.into(),
    }
}
fn request(n: u64) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
async fn create(acl: &Acl, owner: &Identity, id: &str) -> ShareSnapshot {
    let mut tx = acl.pool.begin().await.unwrap();
    let snapshot = acl
        .create_resource_in(&mut tx, owner, &key(id), "owner")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    snapshot
}
async fn replace(
    acl: &Acl,
    identity: &Identity,
    id: &str,
    revision: i64,
    n: u64,
    grants: &[Grant],
) -> Result<ShareSnapshot> {
    let mut tx = acl.pool.begin().await.map_err(database)?;
    let snapshot = acl
        .replace_in(&mut tx, identity, &key(id), revision, &request(n), grants)
        .await?;
    tx.commit().await.map_err(database)?;
    Ok(snapshot)
}
async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

#[tokio::test]
#[ignore = "requires the disposable database and mock Core from scripts/test-sdk-acl.py"]
async fn postgres_acl_end_to_end() {
    let database_url = std::env::var("MIY_ACL_TEST_DATABASE_URL")
        .expect("isolated ACL test database must be supplied");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("isolated ACL test database connection");
    // Refuse an existing application schema: this is not a migration runner.
    let existing: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.schemata WHERE schema_name='app')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!existing, "ACL test requires an empty disposable database");
    sqlx::query("CREATE SCHEMA app")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql(MIGRATION).execute(&pool).await.unwrap();
    sqlx::raw_sql("CREATE TABLE app.acl_test_business(id text PRIMARY KEY,value text NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    let state = Arc::new(Mutex::new(Directory {
        users: ["owner", "reader", "admin", "recipient"]
            .map(String::from)
            .into(),
        groups: [GROUP.to_owned()].into(),
        membership: BTreeMap::from([("reader".into(), [GROUP.to_owned()].into())]),
        unavailable: false,
        wrong_scope: false,
        requests: 0,
    }));
    let origin = reqwest::Url::parse(&std::env::var("MIY_CORE_ORIGIN").unwrap()).unwrap();
    assert_eq!(origin.host_str(), Some("127.0.0.1"));
    let listener =
        tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, origin.port().unwrap()))
            .await
            .unwrap();
    // An acknowledgement lost after the local commit must never re-grant a removed role.
    let acknowledgements = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let calls = acknowledgements.clone();
    let router = Router::new()
        .route(
            "/api/v1/installations/{installation}/platform/steward-assignment",
            get(|| async { Json(json!({"assignmentId":GROUP})) }).post(
                |Json(input): Json<Value>| async move {
                    assert_eq!(input, json!({"assignmentId":GROUP,"stewards":[]}));
                    Json(json!({"assignmentId":GROUP}))
                },
            ),
        )
        .route(
            "/api/v1/installations/{installation}/platform/steward-assignment/applied",
            axum::routing::post(move || {
                let failed = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0;
                async move {
                    if failed {
                        StatusCode::SERVICE_UNAVAILABLE.into_response()
                    } else {
                        Json(json!({"assignmentId":GROUP})).into_response()
                    }
                }
            }),
        )
        .route(
            "/api/v1/installations/{installation}/platform/directory",
            get(directory),
        )
        .with_state(state.clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let acl = Acl::new(
        pool.clone(),
        vec![
            RoleDefinition {
                id: "owner".into(),
                label: "Owner".into(),
                actions: vec![MANAGE.into(), "read".into(), "write".into()],
            },
            RoleDefinition {
                id: "reader".into(),
                label: "Reader".into(),
                actions: vec!["read".into()],
            },
            RoleDefinition {
                id: "editor".into(),
                label: "Editor".into(),
                actions: vec!["read".into(), "write".into()],
            },
        ],
    )
    .unwrap();
    let owner = identity("owner", "dev", INSTALLATION, false);
    let reader = identity("reader", "dev", INSTALLATION, false);
    let admin = identity("admin", "dev", INSTALLATION, true);
    sqlx::raw_sql(crate::stewards::MIGRATION)
        .execute(&pool)
        .await
        .unwrap();
    let failed = owner
        .apply_steward_assignment_with(&pool, async |_, _| Err(sqlx::Error::RowNotFound))
        .await;
    assert!(failed.is_err());
    assert_eq!(
        count(&pool, "SELECT count(*) FROM app.miy_steward_receipts").await,
        0
    );
    sqlx::query("CREATE TABLE app.fixture_accept_audit (assignment_id uuid PRIMARY KEY)")
        .execute(&pool)
        .await
        .unwrap();
    let acknowledged = owner
        .apply_steward_assignment_with(&pool, async |conn, id| {
            sqlx::query("INSERT INTO app.fixture_accept_audit VALUES($1::uuid)")
                .bind(id)
                .execute(conn)
                .await?;
            Ok(())
        })
        .await;
    assert!(acknowledged.is_err()); // local commit succeeded, response to acknowledgement was lost
    assert_eq!(
        count(&pool, "SELECT count(*) FROM app.fixture_accept_audit").await,
        1
    );
    let mut tx = pool.begin().await.unwrap();
    assert!(owner.is_installation_steward_in(&mut tx).await.unwrap());
    assert!(!admin.is_installation_steward_in(&mut tx).await.unwrap());
    sqlx::query("DELETE FROM app.miy_installation_stewards")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(owner.apply_steward_assignment(&pool).await.unwrap());
    assert_eq!(
        count(&pool, "SELECT count(*) FROM app.miy_installation_stewards").await,
        0
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM app.miy_steward_receipts").await,
        1
    );
    for id in ["a-hidden", "b-shared", "c-hidden", "d-shared"] {
        create(&acl, &owner, id).await;
    }
    let group_reader = grant(SubjectKind::Group, GROUP, "reader");
    let direct_editor = grant(SubjectKind::User, "reader", "editor");
    replace(
        &acl,
        &owner,
        "b-shared",
        1,
        1,
        &[owner_grant(), group_reader.clone()],
    )
    .await
    .unwrap();
    replace(
        &acl,
        &owner,
        "d-shared",
        1,
        2,
        &[owner_grant(), direct_editor.clone()],
    )
    .await
    .unwrap();
    let first = acl
        .list_resources(&reader, "projects", "read", 1, None)
        .await
        .unwrap();
    assert_eq!(first.resources, [key("b-shared")]);
    assert_eq!(first.next_cursor.as_deref(), Some("b-shared"));
    let second = acl
        .list_resources(&reader, "projects", "read", 1, first.next_cursor.as_deref())
        .await
        .unwrap();
    assert_eq!(second.resources, [key("d-shared")]);
    assert_eq!(second.next_cursor, None);
    let search = acl
        .list_resources(&reader, "projects", "read", 1, Some("a-hidden"))
        .await
        .unwrap();
    assert_eq!(search.resources, [key("b-shared")]);
    assert_eq!(
        acl.list_resources(&reader, "projects", "write", 10, None)
            .await
            .unwrap()
            .resources,
        [key("d-shared")]
    );
    assert!(
        acl.list_resources(&reader, "tasks", "read", 10, None)
            .await
            .unwrap()
            .resources
            .is_empty()
    );
    acl.require(&reader, &key("b-shared"), "read")
        .await
        .unwrap();
    assert_eq!(
        acl.require(&reader, &key("b-shared"), "write")
            .await
            .unwrap_err()
            .code,
        "acl_denied"
    );
    acl.require(&reader, &key("d-shared"), "write")
        .await
        .unwrap();
    assert_eq!(
        acl.require(&reader, &key("a-hidden"), "read")
            .await
            .unwrap_err()
            .code,
        "acl_denied"
    );
    assert_eq!(
        acl.require(&admin, &key("b-shared"), "read")
            .await
            .unwrap_err()
            .code,
        "acl_denied"
    );
    assert!(
        acl.list_resources(&admin, "projects", "read", 10, None)
            .await
            .unwrap()
            .resources
            .is_empty()
    );
    // Fresh Core groups, not token claims or cached selections, control access.
    let before = state.lock().unwrap().requests;
    state.lock().unwrap().membership.remove("reader");
    assert_eq!(
        acl.require(&reader, &key("b-shared"), "read")
            .await
            .unwrap_err()
            .code,
        "acl_denied"
    );
    assert!(state.lock().unwrap().requests > before);
    assert_eq!(
        acl.list_resources(&reader, "projects", "read", 10, None)
            .await
            .unwrap()
            .resources,
        [key("d-shared")]
    );
    replace(&acl, &owner, "d-shared", 2, 3, &[owner_grant()])
        .await
        .unwrap();
    for action in ["read", "write"] {
        assert_eq!(
            acl.require(&reader, &key("d-shared"), action)
                .await
                .unwrap_err()
                .code,
            "acl_denied"
        );
    }
    // Account deactivation is a denial, even for a direct owner grant.
    state.lock().unwrap().users.remove("owner");
    assert_eq!(
        acl.require(&owner, &key("a-hidden"), "read")
            .await
            .unwrap_err()
            .code,
        "acl_denied"
    );
    state.lock().unwrap().users.insert("owner".into());
    // Current revocation also applies inside a business mutation transaction.
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(
        acl.require_in(&mut tx, &reader, &key("d-shared"), "write")
            .await
            .unwrap_err()
            .code,
        "acl_denied"
    );
    tx.rollback().await.unwrap();
    let prod = identity("owner", "prod", INSTALLATION, false);
    let other = identity("owner", "dev", OTHER_INSTALLATION, false);
    for caller in [&prod, &other] {
        assert_eq!(
            acl.require(caller, &key("b-shared"), "read")
                .await
                .unwrap_err()
                .code,
            "acl_denied"
        );
        assert!(
            acl.list_resources(caller, "projects", "read", 10, None)
                .await
                .unwrap()
                .resources
                .is_empty()
        );
        create(&acl, caller, "b-shared").await;
        assert_eq!(
            acl.sharing(caller, &key("b-shared"))
                .await
                .unwrap()
                .revision,
            1
        );
    }
    let audit_before = count(&pool, "SELECT count(*) FROM app.miy_acl_audit").await;
    let before = acl.sharing(&owner, &key("b-shared")).await.unwrap();
    assert_eq!(before.revision, 2);
    assert_eq!(
        replace(&acl, &owner, "b-shared", 1, 10, &[owner_grant()])
            .await
            .unwrap_err()
            .code,
        "acl_revision_conflict"
    );
    assert_eq!(acl.sharing(&owner, &key("b-shared")).await.unwrap(), before);
    let entries = vec![
        owner_grant(),
        grant(SubjectKind::User, "recipient", "reader"),
    ];
    let changed = replace(&acl, &owner, "b-shared", 2, 11, &entries)
        .await
        .unwrap();
    assert_eq!(changed.revision, 3);
    let mut reordered = entries.clone();
    reordered.reverse();
    let replay = replace(&acl, &owner, "b-shared", 2, 11, &reordered)
        .await
        .unwrap();
    assert_eq!(replay, changed);
    assert_eq!(
        count(&pool, "SELECT count(*) FROM app.miy_acl_audit").await,
        audit_before + 1
    );
    assert_eq!(
        replace(&acl, &owner, "b-shared", 2, 11, &[owner_grant()])
            .await
            .unwrap_err()
            .code,
        "acl_request_conflict"
    );
    assert_eq!(
        acl.replacement_result(&owner, &key("b-shared"), &request(11))
            .await
            .unwrap(),
        Some(changed.clone())
    );
    assert_eq!(
        acl.replacement_result(&owner, &key("b-shared"), &request(999))
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        acl.replacement_result(&reader, &key("b-shared"), &request(11))
            .await
            .unwrap_err()
            .code,
        "acl_denied"
    );
    assert_eq!(
        replace(
            &acl,
            &owner,
            "b-shared",
            3,
            12,
            &[grant(SubjectKind::User, "recipient", "owner")]
        )
        .await
        .unwrap_err()
        .code,
        "acl_owner_protected"
    );
    assert_eq!(
        replace(
            &acl,
            &owner,
            "b-shared",
            3,
            13,
            &[
                owner_grant(),
                grant(SubjectKind::User, "inactive", "reader")
            ]
        )
        .await
        .unwrap_err()
        .code,
        "acl_invalid_subject"
    );
    // An existing inactive recipient can remain or be removed, but not elevated.
    state.lock().unwrap().users.remove("recipient");
    let retained = replace(&acl, &owner, "b-shared", 3, 14, &entries)
        .await
        .unwrap();
    assert_eq!(retained.revision, 4);
    assert_eq!(
        replace(
            &acl,
            &owner,
            "b-shared",
            4,
            15,
            &[
                owner_grant(),
                grant(SubjectKind::User, "recipient", "owner")
            ]
        )
        .await
        .unwrap_err()
        .code,
        "acl_invalid_subject"
    );
    state
        .lock()
        .unwrap()
        .membership
        .insert("reader".into(), [GROUP.to_owned()].into());
    let manager_entries = vec![owner_grant(), grant(SubjectKind::Group, GROUP, "owner")];
    replace(&acl, &owner, "b-shared", 4, 16, &manager_entries)
        .await
        .unwrap();
    state.lock().unwrap().users.remove("owner");
    assert_eq!(
        replace(&acl, &reader, "b-shared", 5, 17, &manager_entries)
            .await
            .unwrap_err()
            .code,
        "acl_last_manager"
    );
    let mut recovered = manager_entries.clone();
    recovered.push(grant(SubjectKind::User, "reader", "owner"));
    replace(&acl, &reader, "b-shared", 5, 18, &recovered)
        .await
        .unwrap();
    state.lock().unwrap().users.insert("owner".into());
    // The original actor and current management privilege both gate result reads.
    assert_eq!(
        acl.replacement_result(&owner, &key("b-shared"), &request(18))
            .await
            .unwrap_err()
            .code,
        "acl_denied"
    );
    state.lock().unwrap().unavailable = true;
    assert_eq!(
        acl.require(&owner, &key("b-shared"), "read")
            .await
            .unwrap_err()
            .code,
        "acl_directory_unavailable"
    );
    assert_eq!(
        acl.list_resources(&reader, "projects", "read", 10, None)
            .await
            .unwrap_err()
            .code,
        "acl_directory_unavailable"
    );
    state.lock().unwrap().unavailable = false;
    state.lock().unwrap().wrong_scope = true;
    assert_eq!(
        acl.require(&owner, &key("b-shared"), "read")
            .await
            .unwrap_err()
            .code,
        "acl_directory_unavailable"
    );
    state.lock().unwrap().wrong_scope = false;
    // Explicit business + ACL creation, rollback, and audit atomicity.
    let count_before = count(&pool, "SELECT count(*) FROM app.miy_acl_resources").await;
    let audit_before = count(&pool, "SELECT count(*) FROM app.miy_acl_audit").await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.acl_test_business VALUES('rolled-back','private')")
        .execute(&mut *tx)
        .await
        .unwrap();
    acl.create_resource_in(&mut tx, &owner, &key("rolled-back"), "owner")
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(
        count(&pool, "SELECT count(*) FROM app.acl_test_business").await,
        0
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM app.miy_acl_resources").await,
        count_before
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM app.miy_acl_audit").await,
        audit_before
    );
    // Rollback restores old grants, revision, result record and audit together.
    let before = acl.sharing(&owner, &key("b-shared")).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    acl.replace_in(
        &mut tx,
        &owner,
        &key("b-shared"),
        before.revision,
        &request(30),
        &[owner_grant()],
    )
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(acl.sharing(&owner, &key("b-shared")).await.unwrap(), before);
    assert_eq!(
        acl.replacement_result(&owner, &key("b-shared"), &request(30))
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        count(&pool, "SELECT count(*) FROM app.miy_acl_audit").await,
        audit_before
    );
    // A business write holds a shared resource lock against concurrent grants.
    let mut business = pool.begin().await.unwrap();
    acl.require_in(&mut business, &owner, &key("b-shared"), "write")
        .await
        .unwrap();
    let mut changing = pool.begin().await.unwrap();
    sqlx::query("SET LOCAL lock_timeout='100ms'")
        .execute(&mut *changing)
        .await
        .unwrap();
    assert_eq!(
        acl.replace_in(
            &mut changing,
            &owner,
            &key("b-shared"),
            before.revision,
            &request(31),
            &[owner_grant()]
        )
        .await
        .unwrap_err()
        .code,
        "acl_database_unavailable"
    );
    changing.rollback().await.unwrap();
    business.rollback().await.unwrap();
    // Enumeration has a hard fail-closed ceiling rather than partial visibility.
    {
        let mut directory = state.lock().unwrap();
        let many: BTreeSet<String> = (100..4197).map(request).collect();
        directory.groups.extend(many.clone());
        directory.membership.insert("reader".into(), many);
    }
    assert_eq!(
        acl.list_resources(&reader, "projects", "read", 10, None)
            .await
            .unwrap_err()
            .code,
        "acl_membership_limit"
    );
    // A resource with candidate groups needs only those bounded current groups.
    state
        .lock()
        .unwrap()
        .membership
        .get_mut("reader")
        .unwrap()
        .insert(GROUP.into());
    acl.require(&reader, &key("b-shared"), "read")
        .await
        .unwrap();
    server.abort();
    pool.close().await;
    println!(
        "ACL_POSTGRES_PASS: SQL-filtered pagination; fresh groups; direct revocation; default deny/admin; installation/environment isolation; revision conflict; canonical idempotence; result ownership; creator/active manager protection; inactive recipients; directory failure/scope mismatch; create/replace rollback; business row lock; membership ceiling; steward audit rollback and commit; steward receipt retry after handover"
    );
}
