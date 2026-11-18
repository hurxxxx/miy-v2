//! Opt-in integration tests against a disposable, dedicated PostgreSQL cluster.
use super::*;
use futures_util::{SinkExt, StreamExt};
use std::{path::Path, sync::Arc};

fn passed(name: &str) {
    println!("PASS shared-development: {name}");
}
fn api<T>(result: ApiResult<T>) -> Result<T> {
    result.map_err(|e| anyhow::anyhow!(e.code))
}
fn principal(parent: Uuid) -> Principal {
    Principal {
        browser_purpose: None,
        id: "developer".into(),
        cli_session_id: Some(parent),
        delegation_id: None,
        scopes: vec!["miy.dev".into()],
        csrf: None,
        development: false,
    }
}
fn request(key: &str, sqls: &[&str]) -> NewSession {
    NewSession{idempotency_key:key.into(),manifest:serde_json::from_value(json!({"contractVersion":"miy.app/v1alpha1","id":"shared-dev-test","name":"Shared DB test","version":"1.0.0","runtime":{"profile":"rust-web-v1"},"ui":{"delivery":"module","moduleEntry":"mf-manifest.json","moduleExport":"./AppRoot","contractVersion":"miy.ui/v1alpha1"},"resources":{"database":"postgresql"},"migrations":{"directory":"migrations"},"requestedPermissions":["database"]})).unwrap(),migrations:sqls.iter().enumerate().map(|(i,sql)|MigrationFile{name:format!("{:04}_fixture.sql",i+1),sql:(*sql).into()}).collect()}
}
async fn new(state: &State, parent: Uuid, key: &str, sqls: &[&str]) -> Result<Value> {
    new_for(state, parent, "shared-dev-test", key, sqls).await
}
async fn new_for(
    state: &State,
    parent: Uuid,
    app: &str,
    key: &str,
    sqls: &[&str],
) -> Result<Value> {
    let mut input = request(key, sqls);
    input.manifest.id = app.into();
    Ok(api(create(
        S(state.clone()),
        Extension(principal(parent)),
        axum::extract::Path(app.into()),
        Json(input),
    )
    .await)?
    .0)
}

async fn official_schema(s: &State, parent: Uuid) -> Result<()> {
    const APP: &str = "catalog-schema-test";
    const FIRST: &str = "CREATE TABLE app.records(id integer PRIMARY KEY);";
    const SECOND: &str = "CREATE TABLE app.additional(id integer PRIMARY KEY);";
    let pool = &s.pool;
    sqlx::raw_sql(
        "INSERT INTO apps VALUES('catalog-schema-test','Schema app');
         INSERT INTO official_apps VALUES('catalog-schema-test',true);
         INSERT INTO official_app_versions VALUES('catalog-schema-test','1.0.0',repeat('a',40),repeat('b',64));
         INSERT INTO grants VALUES('developer','catalog-schema-test','dev','develop');",
    ).execute(pool).await?;
    let session = new_for(s, parent, APP, "before-deploy", &[FIRST]).await?;
    let id: Uuid = serde_json::from_value(session["id"].clone())?;
    tick(pool).await?;
    anyhow::ensure!(state(s, parent, id).await?["errorCode"] == OFFICIAL_SCHEMA_REQUIRES_RELEASE);
    stop(s, parent, id).await?;

    // Prepare the physical dev database with the same schema used by the
    // fixture release. Docker builds/deployments are covered separately.
    sqlx::raw_sql(
        "INSERT INTO app_builds(id,app_id,actor,request_id,source_commit,archive_hash,size_bytes,platform,status,image_id,version)
             VALUES(gen_random_uuid(),'catalog-schema-test','developer',gen_random_uuid(),repeat('a',40),repeat('b',64),1,'linux/amd64','succeeded','sha256:'||repeat('c',64),'1.0.0');
         INSERT INTO releases(id,app_id,version,artifact_hash,manifest,image_id,source_commit,official_version)
             VALUES(gen_random_uuid(),'catalog-schema-test','1.0.0','fixture','{}','sha256:'||repeat('c',64),repeat('a',40),'1.0.0');",
    ).execute(pool).await?;
    let install: Uuid = sqlx::query_scalar(
        "INSERT INTO installations(id,app_id,environment,release_id,db_status,runtime_status,desired_state)
         SELECT gen_random_uuid(),app_id,'dev',id,'ready','running','running' FROM releases WHERE app_id=$1 RETURNING id",
    ).bind(APP).fetch_one(pool).await?;
    let db = worker::ensure_resource(pool, APP, "dev", Some(install)).await?;
    let dir = tempfile::tempdir()?;
    std::fs::write(dir.path().join("0001_fixture.sql"), FIRST)?;
    worker::prepare_resource(pool, &db, dir.path(), None).await?;
    let before = api(crate::runtime_preflight::snapshot(pool, APP, "dev").await)?;
    for (name, migrations, expected_status) in [
        ("same", &[FIRST][..], "ready"),
        ("different", &[FIRST, SECOND][..], "failed"),
    ] {
        let session = new_for(s, parent, APP, name, migrations).await?;
        let session_id: Uuid = serde_json::from_value(session["id"].clone())?;
        tick(pool).await?;
        let result = state(s, parent, session_id).await?;
        anyhow::ensure!(result["status"] == expected_status);
        if expected_status == "ready" {
            let mut conn = runtime_connection(&session["connection"]).await?;
            conn.execute("INSERT INTO app.records VALUES(1)").await?;
        } else {
            anyhow::ensure!(result["errorCode"] == OFFICIAL_SCHEMA_REQUIRES_RELEASE);
        }
        stop(s, parent, session_id).await?;
    }
    let after = api(crate::runtime_preflight::snapshot(pool, APP, "dev").await)?;
    let mut conn = app_admin(&db.database).await?;
    let actual: (bool, i64) =
        sqlx::query_as("SELECT to_regclass('app.additional') IS NULL, count(*) FROM app.records")
            .fetch_one(&mut conn)
            .await?;
    anyhow::ensure!(before == after && actual == (true, 1));
    passed(
        "official dev schema accepts matching local development, rejects schema changes before stopping the installation or migrating",
    );
    Ok(())
}
async fn state(state: &State, parent: Uuid, id: Uuid) -> Result<Value> {
    Ok(api(get(
        S(state.clone()),
        Extension(principal(parent)),
        axum::extract::Path(id),
    )
    .await)?
    .0)
}
async fn stop(state: &State, parent: Uuid, id: Uuid) -> Result<()> {
    let _ = api(close(
        S(state.clone()),
        Extension(principal(parent)),
        axum::extract::Path(id),
    )
    .await)?;
    reconcile(&state.pool).await?;
    Ok(())
}
async fn app_admin(database: &str) -> Result<sqlx::PgConnection> {
    let mut url = url::Url::parse(&std::env::var("PROVISION_DATABASE_URL")?)?;
    url.set_path(database);
    Ok(sqlx::PgConnection::connect(url.as_str()).await?)
}
async fn runtime_connection(secret: &Value) -> Result<sqlx::PgConnection> {
    let mut url = url::Url::parse(&std::env::var("PROVISION_DATABASE_URL")?)?;
    url.set_path(secret["database"].as_str().unwrap());
    url.set_username(secret["user"].as_str().unwrap()).unwrap();
    url.set_password(secret["password"].as_str()).unwrap();
    Ok(sqlx::PgConnection::connect(url.as_str()).await?)
}

async fn fixture_auth() -> Result<(
    Arc<auth::Auth>,
    tokio::task::JoinHandle<std::io::Result<()>>,
)> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let origin = format!("http://{}", listener.local_addr()?);
    let issuer = format!("{origin}/identity/realms/miy");
    unsafe {
        std::env::set_var("MIY_ORIGIN", "http://127.0.0.1:1");
        std::env::set_var("MIY_CONSOLE_ORIGIN", &origin);
        std::env::set_var("MIY_ISSUER", &issuer);
        // Core requires the dev portal and a frame origin pool to start. This
        // regression never opens them; the names are loopback and never bound.
        std::env::set_var("MIY_DEV_PORTAL_ORIGIN", "http://127.0.0.1:2");
        std::env::set_var("MIY_APP_FRAME_ORIGIN", "http://127.0.0.1:{port}");
        std::env::set_var("MIY_APP_FRAME_PORTS", "3-9");
    }
    let metadata = json!({"issuer":issuer,"authorization_endpoint":format!("{origin}/authorize"),"token_endpoint":format!("{origin}/token"),"jwks_uri":format!("{origin}/jwks"),"introspection_endpoint":format!("{origin}/introspect"),"end_session_endpoint":format!("{origin}/logout")});
    let router = axum::Router::new()
        .route(
            "/identity/realms/miy/.well-known/openid-configuration",
            axum::routing::get(move || {
                let value = metadata.clone();
                async move { Json(value) }
            }),
        )
        .route(
            "/jwks",
            axum::routing::get(|| async { Json(json!({"keys":[]})) }),
        );
    let task = tokio::spawn(async move { axum::serve(listener, router).await });
    Ok((Arc::new(auth::Auth::new().await?), task))
}

async fn backfill(pool: &sqlx::PgPool) -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/core");
    let previous = tempfile::tempdir()?;
    for file in std::fs::read_dir(&root)? {
        let file = file?;
        if file.file_name().to_str().unwrap() < "0013" {
            std::fs::copy(file.path(), previous.path().join(file.file_name()))?;
        }
    }
    sqlx::migrate::Migrator::new(previous.path())
        .await?
        .run(pool)
        .await?;
    let release = Uuid::new_v4();
    let installation = Uuid::new_v4();
    sqlx::query("INSERT INTO apps VALUES('legacy-db','Legacy')")
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO releases(id,app_id,version,artifact_hash,manifest) VALUES($1,'legacy-db','1.0.0','legacy','{}')").bind(release).execute(pool).await?;
    sqlx::query("INSERT INTO installations(id,app_id,environment,release_id) VALUES($1,'legacy-db','dev',$2)").bind(installation).bind(release).execute(pool).await?;
    let migration = auth::encrypt("retained-migration-secret")?;
    let runtime = auth::encrypt("retained-runtime-secret")?;
    sqlx::query("INSERT INTO db_resources VALUES($1,'miy_test_legacy','miy_test_legacy_owner','miy_test_legacy_migration','miy_test_legacy_runtime',$2,$3)").bind(installation).bind(&migration).bind(&runtime).execute(pool).await?;
    let mut admin = sqlx::PgConnection::connect(&std::env::var("PROVISION_DATABASE_URL")?).await?;
    admin.execute("CREATE DATABASE miy_test_legacy").await?;
    let mut legacy = app_admin("miy_test_legacy").await?;
    legacy.execute("CREATE TABLE retained(value text); INSERT INTO retained VALUES('existing business data')").await?;
    sqlx::migrate::Migrator::new(root.as_path())
        .await?
        .run(pool)
        .await?;
    let row = sqlx::query("SELECT * FROM db_resources WHERE id=$1")
        .bind(installation)
        .fetch_one(pool)
        .await?;
    anyhow::ensure!(
        row.get::<String, _>("app_id") == "legacy-db"
            && row.get::<String, _>("database_name") == "miy_test_legacy"
            && row.get::<String, _>("migration_secret") == migration
            && row.get::<String, _>("runtime_secret") == runtime
    );
    let retained: String = sqlx::query_scalar("SELECT value FROM retained")
        .fetch_one(&mut legacy)
        .await?;
    anyhow::ensure!(retained == "existing business data");
    passed("migration backfills existing DB identities and retains physical data/secrets");
    Ok(())
}

async fn tcp_bridge(
    ws_origin: String,
    id: Uuid,
    token: String,
) -> Result<(u16, tokio::task::JoinHandle<()>)> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut client, _)) = listener.accept().await else {
                break;
            };
            let address = format!("{ws_origin}/dev/{id}/database");
            let token = token.clone();
            tokio::spawn(async move {
                let result:Result<()>=async{
                    use tokio_tungstenite::tungstenite::{client::IntoClientRequest,Message};
                    let mut request=address.into_client_request()?;request.headers_mut().insert("authorization",format!("Bearer {token}").parse()?);
                    let (mut socket,_)=tokio_tungstenite::connect_async(request).await?;let mut buffer=[0u8;32768];
                    loop {tokio::select!{
                        input=client.read(&mut buffer)=>{let size=input?;if size==0{break;}socket.send(Message::Binary(buffer[..size].to_vec().into())).await?;},
                        output=socket.next()=>match output{Some(Ok(Message::Binary(bytes)))=>client.write_all(&bytes).await?,Some(Ok(Message::Ping(value)))=>socket.send(Message::Pong(value)).await?,Some(Ok(Message::Pong(_)))=>{},_=>break},
                    }}
                    let _=client.shutdown().await;Ok(())
                }.await;
                let _ = result;
            });
        }
    });
    Ok((port, task))
}
async fn tunnel_connection(port: u16, secret: &Value) -> Result<sqlx::PgConnection> {
    let mut url = url::Url::parse(&format!("postgres://127.0.0.1:{port}/?sslmode=disable"))?;
    url.set_path(secret["database"].as_str().unwrap());
    url.set_username(secret["user"].as_str().unwrap()).unwrap();
    url.set_password(secret["password"].as_str()).unwrap();
    Ok(sqlx::PgConnection::connect(url.as_str()).await?)
}

#[tokio::test]
#[ignore = "run scripts/check-development-database.py; requires an owned disposable PostgreSQL"]
async fn shared_development_database() -> Result<()> {
    anyhow::ensure!(
        std::env::var_os("MIY_DEVELOPMENT_AUDIT").is_some(),
        "isolated_fixture_required"
    );
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(12)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    backfill(&pool).await?;
    let (auth, identity_task) = fixture_auth().await?;
    let s = State {
        pool: pool.clone(),
        auth,
        artifacts: std::env::var("MIY_ARTIFACTS")?.into(),
    };
    sqlx::query("INSERT INTO users(id,name) VALUES('developer','Developer'),('other','Other')")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO apps VALUES('shared-dev-test','Shared development')")
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO grants VALUES('developer','shared-dev-test','dev','develop')")
        .execute(&pool)
        .await?;
    let parent = Uuid::new_v4();
    let new_parent = Uuid::new_v4();
    sqlx::query("INSERT INTO cli_sessions(id,user_id,client_id,client_name) VALUES($1,'developer',$2,'integration fixture')").bind(parent).bind(Uuid::new_v4()).execute(&pool).await?;
    sqlx::query("INSERT INTO cli_sessions(id,user_id,client_id,client_name) VALUES($1,'developer',$2,'fresh login fixture')").bind(new_parent).bind(Uuid::new_v4()).execute(&pool).await?;
    const FIRST: &str = "CREATE TABLE app.records(id integer PRIMARY KEY,value text NOT NULL);";
    const SECOND: &str = "ALTER TABLE app.records ADD COLUMN category text;";
    const THIRD: &str = "ALTER TABLE app.records ADD COLUMN retained text;";
    const BAD: &str = "ALTER TABLE app.records ADD COLUMN rolled_back text; SELECT intentionally_missing_function();";
    const FIXED: &str = "ALTER TABLE app.records ADD COLUMN recovered text;";
    let mut unauthorized = principal(parent);
    unauthorized.id = "other".into();
    let denied = create(
        S(s.clone()),
        Extension(unauthorized),
        axum::extract::Path("shared-dev-test".into()),
        Json(request("denied", &[FIRST])),
    )
    .await
    .unwrap_err();
    anyhow::ensure!(denied.status == axum::http::StatusCode::FORBIDDEN);
    let mut invalid = request("invalid", &[FIRST]);
    invalid.migrations[0].name = "../escape.sql".into();
    anyhow::ensure!(validate_request("shared-dev-test", &invalid).is_err());
    passed("development permission and migration paths are enforced");
    official_schema(&s, parent).await?;
    let first = new(&s, parent, "initial", &[FIRST]).await?;
    let id = Uuid::parse_str(first["id"].as_str().unwrap())?;
    anyhow::ensure!(first["status"] == "queued" && first["connection"].is_object());
    let repeat = new(&s, parent, "initial", &[FIRST]).await?;
    anyhow::ensure!(repeat["id"] == first["id"] && repeat["connection"] == first["connection"]);
    let recovered_request = api(by_request(
        S(s.clone()),
        Extension(principal(parent)),
        axum::extract::Path(("shared-dev-test".into(), "initial".into())),
    )
    .await)?
    .0;
    anyhow::ensure!(
        recovered_request["id"] == first["id"] && recovered_request.get("connection").is_none()
    );
    anyhow::ensure!(
        by_request(
            S(s.clone()),
            Extension(principal(parent)),
            axum::extract::Path(("shared-dev-test".into(), "missing".into()))
        )
        .await
        .unwrap_err()
        .status
            == axum::http::StatusCode::NOT_FOUND
    );
    anyhow::ensure!(
        get(
            S(s.clone()),
            Extension(principal(new_parent)),
            axum::extract::Path(id)
        )
        .await
        .unwrap_err()
        .status
            == axum::http::StatusCode::FORBIDDEN
    );
    passed("lost acknowledgement lookup is read-only and parent-scoped without secrets");
    let collision = create(
        S(s.clone()),
        Extension(principal(parent)),
        axum::extract::Path("shared-dev-test".into()),
        Json(request("other", &[FIRST])),
    )
    .await
    .unwrap_err();
    anyhow::ensure!(collision.code == "active_dev_session");
    let changed = create(
        S(s.clone()),
        Extension(principal(parent)),
        axum::extract::Path("shared-dev-test".into()),
        Json(request("initial", &[FIRST, SECOND])),
    )
    .await
    .unwrap_err();
    anyhow::ensure!(changed.code == "idempotency_conflict");
    anyhow::ensure!(tick(&pool).await?);
    let ready = state(&s, parent, id).await?;
    anyhow::ensure!(ready["status"] == "ready" && ready.get("connection").is_none());
    let db = first["connection"]["database"].as_str().unwrap();
    let uninstalled: bool =
        sqlx::query_scalar("SELECT installation_id IS NULL FROM db_resources WHERE id=$1")
            .bind(Uuid::parse_str(first["databaseId"].as_str().unwrap())?)
            .fetch_one(&pool)
            .await?;
    anyhow::ensure!(uninstalled);
    passed(
        "first local session provisions a dev DB before any release; retries preserve its identity",
    );
    sqlx::query("UPDATE app_dev_sessions SET status='preparing' WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await?;
    recover(&pool).await?;
    tick(&pool).await?;
    anyhow::ensure!(
        state(&s, parent, id).await?["schemaFingerprint"] == ready["schemaFingerprint"]
    );
    passed("worker recovery replays preparation with the same database and migration history");
    let mut local = runtime_connection(&first["connection"]).await?;
    local
        .execute("INSERT INTO app.records VALUES(42,'created locally')")
        .await?;
    anyhow::ensure!(
        local
            .execute("CREATE TABLE app.forbidden(id int)")
            .await
            .is_err()
    );
    anyhow::ensure!(
        local
            .execute("SELECT * FROM app._sqlx_migrations")
            .await
            .is_err()
    );
    let permanent_role: String =
        sqlx::query_scalar("SELECT runtime_role FROM db_resources WHERE id=$1")
            .bind(Uuid::parse_str(first["databaseId"].as_str().unwrap())?)
            .fetch_one(&pool)
            .await?;
    anyhow::ensure!(
        local
            .execute(format!("SET ROLE {permanent_role}").as_str())
            .await
            .is_err()
    );
    anyhow::ensure!(
        local
            .execute(format!("ALTER ROLE {permanent_role} PASSWORD 'test-must-be-denied'").as_str())
            .await
            .is_err()
    );
    passed(
        "temporary role permits CRUD and denies DDL, migration history, SET ROLE and permanent password changes",
    );
    let mut admin = sqlx::PgConnection::connect(&std::env::var("PROVISION_DATABASE_URL")?).await?;
    admin
        .execute(
            format!(
                "ALTER ROLE {} VALID UNTIL '2000-01-01'",
                first["connection"]["user"].as_str().unwrap()
            )
            .as_str(),
        )
        .await?;
    let mut unrelated = pool.acquire().await?;
    unrelated.close_on_drop();
    sqlx::query("SELECT pg_advisory_lock(hashtextextended('runtime:unrelated-app:dev',0))")
        .execute(&mut *unrelated)
        .await?;
    let monitor = start(pool.clone());
    tokio::time::sleep(Duration::from_secs(6)).await;
    let role_live: bool =
        sqlx::query_scalar("SELECT rolvaliduntil>now() FROM pg_roles WHERE rolname=$1")
            .bind(first["connection"]["user"].as_str().unwrap())
            .fetch_one(&mut admin)
            .await?;
    anyhow::ensure!(role_live);
    drop(monitor);
    drop(unrelated);
    passed(
        "independent lease monitor renews credentials while another app operation holds its lock",
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let origin = format!("ws://{}", listener.local_addr()?);
    let router = axum::Router::new()
        .route("/dev/{id}/database", axum::routing::get(database))
        .with_state(s.clone());
    let tunnel_task = tokio::spawn(async move { axum::serve(listener, router).await });
    let (port, bridge) = tcp_bridge(
        origin.clone(),
        id,
        first["connection"]["tunnelToken"].as_str().unwrap().into(),
    )
    .await?;
    let mut tunneled = tunnel_connection(port, &first["connection"]).await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.records")
        .fetch_one(&mut tunneled)
        .await?;
    anyhow::ensure!(count == 1);
    let mut wrong = first["connection"].clone();
    wrong["database"] = json!("miy_test_core");
    anyhow::ensure!(tunnel_connection(port, &wrong).await.is_err());
    let _ = api(heartbeat(
        S(s.clone()),
        Extension(principal(parent)),
        axum::extract::Path(id),
    )
    .await)?;
    passed("authenticated WS carries PostgreSQL and rejects cross-database startup");
    let snapshot = crate::runtime_preflight::snapshot(&pool, "shared-dev-test", "dev")
        .await
        .map_err(|e| anyhow::anyhow!(e.code))?;
    let preflight=crate::runtime_preflight::inspect(&s,&principal(parent),"shared-dev-test",serde_json::from_value(json!({"target":{"appId":"shared-dev-test","environment":"dev"},"action":"deploy","imageId":format!("sha256:{}","a".repeat(64))}))?).await.map_err(|e|anyhow::anyhow!(e.code))?;
    anyhow::ensure!(
        !preflight.can_submit
            && preflight
                .checks
                .iter()
                .any(|c| c.code
                    == Some(crate::runtime_contract::FailureCode::ActiveDevelopmentSession))
    );
    stop(&s, new_parent, id).await?;
    passed("fresh login can release an older owned lease without receiving its secrets");
    anyhow::ensure!(local.execute("SELECT 1").await.is_err());
    bridge.abort();
    drop(tunneled);
    let release = Uuid::new_v4();
    let installation = Uuid::new_v4();
    sqlx::query("INSERT INTO releases(id,app_id,version,artifact_hash,manifest,image_id) VALUES($1,'shared-dev-test','1.0.0','fixture','{}',$2)").bind(release).bind(format!("sha256:{}","a".repeat(64))).execute(&pool).await?;
    sqlx::query("INSERT INTO installations(id,app_id,environment,release_id) VALUES($1,'shared-dev-test','dev',$2)").bind(installation).bind(release).execute(&pool).await?;
    let deployed =
        worker::ensure_resource(&pool, "shared-dev-test", "dev", Some(installation)).await?;
    anyhow::ensure!(
        deployed.database == db
            && deployed.id == Uuid::parse_str(first["databaseId"].as_str().unwrap())?
    );
    let mut deployed_db = app_admin(&deployed.database).await?;
    let record: String = sqlx::query_scalar("SELECT value FROM app.records WHERE id=42")
        .fetch_one(&mut deployed_db)
        .await?;
    anyhow::ensure!(record == "created locally");
    let prod = worker::ensure_resource(&pool, "shared-dev-test", "prod", None).await?;
    // A dbname query parameter must not send schema provisioning back to the
    // maintenance DB. Its namespace and existing public permissions stay put.
    let provision = std::env::var("PROVISION_DATABASE_URL")?;
    let mut maintenance = sqlx::PgConnection::connect(&provision).await?;
    let (maintenance_database, app_exists, public_acl): (String, bool, Option<String>) =
        sqlx::query_as("SELECT current_database(), to_regnamespace('app') IS NOT NULL, nspacl::text FROM pg_namespace WHERE nspname='public'")
            .fetch_one(&mut maintenance)
            .await?;
    anyhow::ensure!(
        !app_exists,
        "fixture maintenance DB already has an app schema"
    );
    let mut address = url::Url::parse(&provision)?;
    address
        .query_pairs_mut()
        .append_pair("dbname", &maintenance_database);
    let prod_migrations = migration_directory(id)?;
    unsafe { std::env::set_var("PROVISION_DATABASE_URL", address.as_str()) };
    let prepared = worker::prepare_resource(&pool, &prod, &prod_migrations, None).await;
    unsafe { std::env::set_var("PROVISION_DATABASE_URL", &provision) };
    let (app_exists, after_acl): (bool, Option<String>) =
        sqlx::query_as("SELECT to_regnamespace('app') IS NOT NULL, nspacl::text FROM pg_namespace WHERE nspname='public'")
            .fetch_one(&mut maintenance)
            .await?;
    anyhow::ensure!(
        !app_exists && public_acl == after_acl,
        "provisioning modified the maintenance database"
    );
    drop(maintenance);
    prepared?;
    let mut prod_db = app_admin(&prod.database).await?;
    let (database, owner): (String, String) = sqlx::query_as(
        "SELECT current_database(), pg_get_userbyid(nspowner) FROM pg_namespace WHERE nspname='app'",
    )
    .fetch_one(&mut prod_db)
    .await?;
    anyhow::ensure!(database == prod.database && owner == prod.owner);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.records")
        .fetch_one(&mut prod_db)
        .await?;
    anyhow::ensure!(
        count == 0 && prod.database != deployed.database && prod.runtime != deployed.runtime
    );
    passed(
        "dev deployment reuses local records; query DSN provisioning stays in the separate production DB",
    );
    let health_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let health_port = i32::from(health_listener.local_addr()?.port());
    let health = axum::Router::new().route(
        "/health",
        axum::routing::get(move || async move { Json(json!({"releaseId":release})) }),
    );
    let health_task = tokio::spawn(async move { axum::serve(health_listener, health).await });
    sqlx::query("UPDATE installations SET db_status='ready',runtime_status='running',desired_state='running',runtime_port=$2 WHERE id=$1").bind(installation).bind(health_port).execute(&pool).await?;
    sqlx::query("INSERT INTO runtime_jobs(id,app_id,environment,actor,action,installation_id,release_id,idempotency_key,input_hash,status) VALUES($1,'shared-dev-test','dev','developer','deploy',$2,$3,'accepted-fixture','fixture','succeeded')").bind(Uuid::new_v4()).bind(installation).bind(release).execute(&pool).await?;
    api(crate::runtime::check_release(&s,&principal(parent),"shared-dev-test",serde_json::from_value(json!({"releaseId":release,"note":"real DB test with fixture runtime health","checks":[]}))?).await)?;
    anyhow::ensure!(
        crate::runtime::checked_image(&pool, "shared-dev-test", release)
            .await
            .is_ok()
    );
    let second = new(&s, parent, "second", &[FIRST, SECOND]).await?;
    let second_id = Uuid::parse_str(second["id"].as_str().unwrap())?;
    tick(&pool).await?;
    anyhow::ensure!(state(&s, parent, second_id).await?["status"] == "ready");
    let after = crate::runtime_preflight::snapshot(&pool, "shared-dev-test", "dev")
        .await
        .map_err(|e| anyhow::anyhow!(e.code))?;
    anyhow::ensure!(crate::runtime_contract::compare_expected(&snapshot, &after).is_err());
    let inst = sqlx::query("SELECT desired_state,db_status FROM installations WHERE id=$1")
        .bind(installation)
        .fetch_one(&pool)
        .await?;
    anyhow::ensure!(
        inst.get::<String, _>("desired_state") == "stopped"
            && inst.get::<String, _>("db_status") == "not_prepared"
    );
    stop(&s, parent, second_id).await?;
    anyhow::ensure!(
        crate::runtime::checked_image(&pool, "shared-dev-test", release)
            .await
            .is_err()
    );
    health_task.abort();
    passed("acceptance is bound to the exact tested schema and expires after a local migration");
    let failure = new(&s, parent, "partial", &[FIRST, SECOND, THIRD, BAD]).await?;
    let failed_id = Uuid::parse_str(failure["id"].as_str().unwrap())?;
    tick(&pool).await?;
    anyhow::ensure!(state(&s, parent, failed_id).await?["status"] == "failed");
    let retained:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema='app' AND table_name='records' AND column_name='retained')").fetch_one(&mut deployed_db).await?;
    let rolled_back:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema='app' AND table_name='records' AND column_name='rolled_back')").fetch_one(&mut deployed_db).await?;
    anyhow::ensure!(retained && !rolled_back);
    let recovered = new(&s, parent, "recover", &[FIRST, SECOND, THIRD, FIXED]).await?;
    let recovered_id = Uuid::parse_str(recovered["id"].as_str().unwrap())?;
    tick(&pool).await?;
    anyhow::ensure!(state(&s, parent, recovered_id).await?["status"] == "ready");
    stop(&s, parent, recovered_id).await?;
    let changed_history = new(
        &s,
        parent,
        "changed-history",
        &[
            "CREATE TABLE app.records(id integer PRIMARY KEY,value varchar NOT NULL);",
            SECOND,
            THIRD,
            FIXED,
        ],
    )
    .await?;
    let changed_id = Uuid::parse_str(changed_history["id"].as_str().unwrap())?;
    tick(&pool).await?;
    anyhow::ensure!(
        state(&s, parent, changed_id).await?["errorCode"] == "migration_history_mismatch"
    );
    passed(
        "schema changes invalidate snapshots, keep old deployment stopped, and recover partial SQL failures without resetting data",
    );
    // The history relation belongs to the app: a migration can replace it with a
    // view over its own function. Check the authenticated identity as well as
    // the effective role: SET ROLE alone cannot contain a superuser session.
    let history = migration_directory(recovered_id)?;
    let history_resource = worker::resource_by_id(
        &pool,
        sqlx::query_scalar(
            "SELECT id FROM db_resources WHERE app_id='shared-dev-test' AND environment='dev'",
        )
        .fetch_one(&pool)
        .await?,
    )
    .await?;
    worker::verify_development_migrations(&pool, "shared-dev-test", &history).await?;
    let mut owner = worker::app_owner_connection(&history_resource).await?;
    for sql in [
        "ALTER TABLE app._sqlx_migrations RENAME TO history_fixture",
        "CREATE FUNCTION app.reader_is_plain() RETURNS boolean LANGUAGE plpgsql AS $$ BEGIN EXECUTE 'RESET ROLE'; RETURN current_setting('is_superuser')='off' AND current_user=session_user AND NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname IN (current_user,session_user) AND rolsuper); END $$",
        "CREATE VIEW app._sqlx_migrations AS SELECT version,description,installed_on,success AND app.reader_is_plain() AS success,checksum,execution_time FROM app.history_fixture",
    ] {
        owner.execute(sql).await?;
    }
    drop(owner);
    // Negative control: the old superuser login plus SET ROLE must fail this
    // fixture even in a read-only transaction, after the function resets role.
    let mut privileged = app_admin(&history_resource.database).await?;
    let mut tx = privileged.begin().await?;
    tx.execute("SET TRANSACTION READ ONLY").await?;
    tx.execute(format!("SET ROLE {}", history_resource.owner).as_str())
        .await?;
    let plain: bool = sqlx::query_scalar("SELECT app.reader_is_plain()")
        .fetch_one(&mut *tx)
        .await?;
    anyhow::ensure!(
        !plain,
        "history fixture failed to detect the superuser login"
    );
    tx.rollback().await?;
    drop(privileged);

    // The disposable harness supplies a generated hexadecimal password. Keep
    // credentials in memory and exercise both SQLx query identity overrides and
    // its separate database override through the real connection helper.
    let provision = std::env::var("PROVISION_DATABASE_URL")?;
    let mut addresses = vec![provision.clone()];
    for include_database in [false, true] {
        let mut address = url::Url::parse(&provision)?;
        let user = address.username().to_owned();
        let password = address
            .password()
            .context("fixture_password_required")?
            .to_owned();
        let database = address.path().trim_start_matches('/').to_owned();
        address
            .query_pairs_mut()
            .append_pair("user", &user)
            .append_pair("password", &password);
        if include_database {
            address.query_pairs_mut().append_pair("dbname", &database);
        }
        addresses.push(address.to_string());
    }
    for address in addresses {
        // This opt-in regression runs alone against its owned cluster.
        unsafe { std::env::set_var("PROVISION_DATABASE_URL", address) };
        let checked: Result<()> = async {
            let mut reader = worker::app_owner_connection(&history_resource).await?;
            let (database, session, role): (String, String, String) =
                sqlx::query_as("SELECT current_database(), session_user, current_user")
                    .fetch_one(&mut reader)
                    .await?;
            anyhow::ensure!(
                database == history_resource.database
                    && session == history_resource.migration
                    && role == history_resource.owner,
                "history reader escaped app database or login"
            );
            let plain: bool = sqlx::query_scalar("SELECT app.reader_is_plain()")
                .fetch_one(&mut reader)
                .await?;
            let role: String = sqlx::query_scalar("SELECT current_user")
                .fetch_one(&mut reader)
                .await?;
            anyhow::ensure!(
                plain && role == history_resource.migration,
                "RESET ROLE escaped the migration login"
            );
            reader.close().await?;
            worker::verify_development_migrations(&pool, "shared-dev-test", &history).await?;
            Ok(())
        }
        .await;
        unsafe { std::env::set_var("PROVISION_DATABASE_URL", &provision) };
        checked?;
    }
    // A read the app stalls is abandoned and its session ended, not waited for.
    let mut owner = worker::app_owner_connection(&history_resource).await?;
    for sql in [
        "CREATE FUNCTION app.reader_stalls() RETURNS boolean LANGUAGE sql AS $$ SELECT pg_sleep(600) IS NOT NULL $$",
        "CREATE OR REPLACE VIEW app._sqlx_migrations AS SELECT version,description,installed_on,success AND app.reader_stalls() AS success,checksum,execution_time FROM app.history_fixture",
    ] {
        owner.execute(sql).await?;
    }
    let stalled = std::time::Instant::now();
    let refused = worker::verify_development_migrations(&pool, "shared-dev-test", &history)
        .await
        .unwrap_err();
    anyhow::ensure!(
        refused.to_string() == "build_database_history_unavailable"
            && stalled.elapsed() < std::time::Duration::from_secs(40)
    );
    let mut admin = sqlx::PgConnection::connect(&std::env::var("PROVISION_DATABASE_URL")?).await?;
    let left: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_stat_activity WHERE usename=$1 AND query LIKE '%_sqlx_migrations%' AND state='active'",
    )
    .bind(&history_resource.migration)
    .fetch_one(&mut admin)
    .await?;
    anyhow::ensure!(left == 0, "stalled history read kept its session");
    for sql in [
        "DROP VIEW app._sqlx_migrations",
        "DROP FUNCTION app.reader_stalls()",
        "DROP FUNCTION app.reader_is_plain()",
        "ALTER TABLE app.history_fixture RENAME TO _sqlx_migrations",
    ] {
        owner.execute(sql).await?;
    }
    drop(owner);
    passed(
        "build history uses the app's migration login even with query DSNs, cannot regain superuser through RESET ROLE, and is bounded in time",
    );
    let lease = new(&s, parent, "lease", &[FIRST, SECOND, THIRD, FIXED]).await?;
    let lease_id = Uuid::parse_str(lease["id"].as_str().unwrap())?;
    tick(&pool).await?;
    let (port, lease_bridge) = tcp_bridge(
        origin.clone(),
        lease_id,
        lease["connection"]["tunnelToken"].as_str().unwrap().into(),
    )
    .await?;
    let mut lease_connection = tunnel_connection(port, &lease["connection"]).await?;
    sqlx::query("UPDATE app_dev_sessions SET expires_at=now()-interval '1 second' WHERE id=$1")
        .bind(lease_id)
        .execute(&pool)
        .await?;
    tokio::time::sleep(Duration::from_secs(6)).await;
    anyhow::ensure!(lease_connection.execute("SELECT 1").await.is_err());
    reconcile(&pool).await?;
    lease_bridge.abort();
    anyhow::ensure!(!session_valid(&pool, lease_id).await?);
    passed("expired lease closes existing WS and DB connections and removes the role");
    let ungranted = new(&s, parent, "grant-revoke", &[FIRST, SECOND, THIRD, FIXED]).await?;
    let ungranted_id = Uuid::parse_str(ungranted["id"].as_str().unwrap())?;
    tick(&pool).await?;
    let (port, ungranted_bridge) = tcp_bridge(
        origin.clone(),
        ungranted_id,
        ungranted["connection"]["tunnelToken"]
            .as_str()
            .unwrap()
            .into(),
    )
    .await?;
    let mut ungranted_connection = tunnel_connection(port, &ungranted["connection"]).await?;
    sqlx::query("DELETE FROM grants WHERE user_id='developer' AND app_id='shared-dev-test'")
        .execute(&pool)
        .await?;
    tokio::time::sleep(Duration::from_secs(6)).await;
    anyhow::ensure!(ungranted_connection.execute("SELECT 1").await.is_err());
    stop(&s, new_parent, ungranted_id).await?;
    ungranted_bridge.abort();
    sqlx::query("INSERT INTO grants VALUES('developer','shared-dev-test','dev','develop')")
        .execute(&pool)
        .await?;
    passed("grant revocation closes existing DB connections and still permits own-lease cleanup");
    let revoked = new(&s, parent, "revoke", &[FIRST, SECOND, THIRD, FIXED]).await?;
    let revoked_id = Uuid::parse_str(revoked["id"].as_str().unwrap())?;
    tick(&pool).await?;
    let (port, revoke_bridge) = tcp_bridge(
        origin.clone(),
        revoked_id,
        revoked["connection"]["tunnelToken"]
            .as_str()
            .unwrap()
            .into(),
    )
    .await?;
    let mut revoked_connection = tunnel_connection(port, &revoked["connection"]).await?;
    sqlx::query("UPDATE cli_sessions SET revoked_at=now(),revocation_reason='test' WHERE id=$1")
        .bind(parent)
        .execute(&pool)
        .await?;
    tokio::time::sleep(Duration::from_secs(6)).await;
    anyhow::ensure!(revoked_connection.execute("SELECT 1").await.is_err());
    reconcile(&pool).await?;
    revoke_bridge.abort();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.records WHERE id=42")
        .fetch_one(&mut deployed_db)
        .await?;
    anyhow::ensure!(count == 1);
    passed("parent CLI revocation terminates existing connections while preserving data");
    let disabled = new(&s, new_parent, "disable", &[FIRST, SECOND, THIRD, FIXED]).await?;
    let disabled_id = Uuid::parse_str(disabled["id"].as_str().unwrap())?;
    tick(&pool).await?;
    let (port, disabled_bridge) = tcp_bridge(
        origin,
        disabled_id,
        disabled["connection"]["tunnelToken"]
            .as_str()
            .unwrap()
            .into(),
    )
    .await?;
    let mut disabled_connection = tunnel_connection(port, &disabled["connection"]).await?;
    sqlx::query("UPDATE users SET enabled=false WHERE id='developer'")
        .execute(&pool)
        .await?;
    tokio::time::sleep(Duration::from_secs(6)).await;
    anyhow::ensure!(disabled_connection.execute("SELECT 1").await.is_err());
    reconcile(&pool).await?;
    disabled_bridge.abort();
    passed("disabled user cannot retain an existing development DB connection");
    tunnel_task.abort();
    identity_task.abort();
    pool.close().await;
    Ok(())
}
