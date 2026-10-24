//! Test-only integration harness. Product implementations are imported unchanged.
use crate::{State, auth, domain, runtime, runtime_health};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use sqlx::{Connection, Executor, Row};
use std::{path::PathBuf, sync::Arc, time::Duration};
use uuid::Uuid;

fn root() -> PathBuf {
    std::env::var("MIY_AUDIT_ROOT").unwrap().into()
}
fn report(name: &str, passed: bool, evidence: Value) {
    let line = json!({"test":name,"status":if passed{"PASS"}else{"FAIL"},"evidence":evidence});
    println!("{line}");
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root().join("evidence/results.jsonl"))
        .unwrap();
    writeln!(f, "{line}").unwrap();
}
fn actor(id: &str) -> auth::Principal {
    auth::Principal {
        id: id.into(),
        scopes: vec!["miy.dev".into()],
        csrf: None,
        development: false,
    }
}
fn req(
    action: &str,
    env: &str,
    image: Option<&str>,
    release: Option<Uuid>,
    key: &str,
) -> runtime::RuntimeRequest {
    serde_json::from_value(json!({"action":action,"environment":env,"imageId":image,"releaseId":release,"idempotencyKey":key})).unwrap()
}
async fn submit(
    s: &State,
    p: &auth::Principal,
    action: &str,
    env: &str,
    image: Option<&str>,
    release: Option<Uuid>,
    key: &str,
) -> Result<Uuid> {
    let v = runtime::request(s, p, "audit-runtime", req(action, env, image, release, key))
        .await
        .map_err(|e| anyhow::anyhow!(e.code))?;
    Ok(serde_json::from_value(v["id"].clone())?)
}
async fn job(pool: &sqlx::PgPool, id: Uuid) -> Result<Value> {
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('id',id,'status',status,'step',step,'errorCode',error_code,'releaseId',release_id) FROM runtime_jobs WHERE id=$1").bind(id).fetch_one(pool).await?)
}
async fn finish(pool: &sqlx::PgPool, id: Uuid) -> Result<Value> {
    runtime::tick(pool).await?;
    job(pool, id).await
}
async fn inst(pool: &sqlx::PgPool, env: &str) -> Result<Value> {
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('id',id,'releaseId',release_id,'runtimeImage',runtime_image,'runtimeContainer',runtime_container,'runtimePort',runtime_port,'runtimeStatus',runtime_status,'desiredState',desired_state,'dbStatus',db_status,'healthStatus',health_status,'healthFailures',health_failures,'healthCheckedAt',health_checked_at,'healthError',health_error) FROM installations WHERE app_id='audit-runtime' AND environment=$1").bind(env).fetch_one(pool).await?)
}
async fn db(pool: &sqlx::PgPool, id: Uuid, runtime_role: bool) -> Result<sqlx::PgConnection> {
    let r = sqlx::query("SELECT * FROM db_resources WHERE installation_id=$1")
        .bind(id)
        .fetch_one(pool)
        .await?;
    let mut u = url::Url::parse(&std::env::var("PROVISION_DATABASE_URL")?)?;
    u.set_path(r.get("database_name"));
    if runtime_role {
        u.set_username(r.get("runtime_role")).unwrap();
        u.set_password(Some(&auth::decrypt(r.get("runtime_secret"))?))
            .unwrap();
    }
    Ok(sqlx::PgConnection::connect(u.as_str()).await?)
}
fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}
// Real worker/control job with twelve slow HTTP services and isolated metadata.
// These rows exercise observation scheduling, not image admission or OAuth.
async fn monitoring_boundaries(s: &State, admin: &auth::Principal) -> Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    // Container shutdown grace is deliberately allowed by the platform. Stop
    // before this scheduling test so it measures queue dispatch/start, not grace.
    let stop = submit(s, admin, "stop", "prod", None, None, "prepare-monitor-test").await?;
    ensure!(finish(&s.pool, stop).await?["status"] == "succeeded");
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let started = Arc::new(AtomicUsize::new(0));
    let (sender, mut entered) = tokio::sync::mpsc::unbounded_channel();
    let mut servers = tokio::task::JoinSet::new();
    let mut ids = Vec::new();
    let mut releases = Vec::new();
    let mut apps = Vec::new();
    for index in 0..12 {
        let id = Uuid::new_v4();
        let release = Uuid::new_v4();
        let app_id = format!("health-scheduling-{index}");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = i32::from(listener.local_addr()?.port());
        let gate = gate.clone();
        let started = started.clone();
        let sender = sender.clone();
        let app = axum::Router::new().route(
            "/health",
            axum::routing::get(move || {
                let gate = gate.clone();
                let started = started.clone();
                let sender = sender.clone();
                async move {
                    started.fetch_add(1, Ordering::SeqCst);
                    let _ = sender.send(id);
                    gate.acquire().await.unwrap().forget();
                    axum::http::StatusCode::SERVICE_UNAVAILABLE
                }
            }),
        );
        servers.spawn(async move { axum::serve(listener, app).await });
        sqlx::query("INSERT INTO apps(id,name) VALUES($1,'Slow health fixture')")
            .bind(&app_id)
            .execute(&s.pool)
            .await?;
        sqlx::query("INSERT INTO releases(id,app_id,version,artifact_hash,manifest) SELECT $1,$2,'1.0.0',r.artifact_hash,jsonb_set(r.manifest,'{id}',to_jsonb($2::text)) FROM installations i JOIN releases r ON r.id=i.release_id WHERE i.app_id='audit-runtime' AND i.environment='dev'").bind(release).bind(&app_id).execute(&s.pool).await?;
        sqlx::query("INSERT INTO installations(id,app_id,environment,release_id,db_status,runtime_status,desired_state,runtime_port) VALUES($1,$2,'dev',$3,'ready','running','running',$4)").bind(id).bind(&app_id).bind(release).bind(port).execute(&s.pool).await?;
        ids.push(id);
        releases.push(release);
        apps.push(app_id);
    }
    let mut runner = worker(None, None)?;
    let first = tokio::time::timeout(Duration::from_secs(10), entered.recv())
        .await?
        .unwrap();
    for _ in 0..3 {
        tokio::time::timeout(Duration::from_secs(2), entered.recv())
            .await?
            .unwrap();
    }
    // Simulate a completed same-release restart while its earlier probe is pending.
    runtime_health::record(&s.pool, first, runtime_health::Observation::Inactive).await?;
    runtime_health::record(&s.pool, first, runtime_health::Observation::Healthy).await?;
    let before: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT health_checked_at FROM installations WHERE id=$1")
            .bind(first)
            .fetch_one(&s.pool)
            .await?;
    let stop = submit(
        s,
        admin,
        "start",
        "prod",
        None,
        None,
        "start-during-slow-monitor",
    )
    .await?;
    let result = terminal(&s.pool, stop).await?;
    report(
        "slow health services do not occupy the control queue",
        result["status"] == "succeeded" && started.load(Ordering::SeqCst) == 4,
        json!({"job":result,"startedBeforeResponsesReleased":started.load(Ordering::SeqCst),"maximumConcurrentProbes":4}),
    );
    gate.add_permits(100);
    for _ in 0..100 {
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM installations WHERE id=ANY($1) AND health_checked_at IS NOT NULL",
        )
        .bind(&ids)
        .fetch_one(&s.pool)
        .await?;
        if count == 12 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    runner.kill().await?;
    let _ = runner.wait().await;
    let row = sqlx::query(
        "SELECT health_status,health_failures,health_checked_at FROM installations WHERE id=$1",
    )
    .bind(first)
    .fetch_one(&s.pool)
    .await?;
    let degraded: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM installations WHERE id=ANY($1) AND health_status='degraded'",
    )
    .bind(&ids)
    .fetch_one(&s.pool)
    .await?;
    report(
        "late health result cannot overwrite a newer lifecycle generation",
        row.get::<String, _>("health_status") == "healthy"
            && row.get::<i32, _>("health_failures") == 0
            && row.get::<chrono::DateTime<chrono::Utc>, _>("health_checked_at") == before
            && degraded == 11,
        json!({"preservedStatus":row.get::<String,_>("health_status"),"otherFailuresRecorded":degraded,"checksStarted":started.load(Ordering::SeqCst)}),
    );
    sqlx::query("DELETE FROM installations WHERE id=ANY($1)")
        .bind(&ids)
        .execute(&s.pool)
        .await?;
    sqlx::query("DELETE FROM releases WHERE id=ANY($1)")
        .bind(&releases)
        .execute(&s.pool)
        .await?;
    sqlx::query("DELETE FROM apps WHERE id=ANY($1)")
        .bind(&apps)
        .execute(&s.pool)
        .await?;
    servers.abort_all();
    Ok(())
}
async fn deployed(s: &State, p: &auth::Principal, image: &str, key: &str) -> Result<Value> {
    let id = submit(s, p, "deploy", "dev", Some(image), None, key).await?;
    let result = finish(&s.pool, id).await?;
    ensure!(result["status"] == "succeeded", "{key}: {result}");
    inst(&s.pool, "dev").await
}
fn worker(failpoint: Option<&str>, crash_remove: Option<&str>) -> Result<tokio::process::Child> {
    let mut c = tokio::process::Command::new(std::env::current_exe()?);
    c.args([
        "--exact",
        "runtime_regression::worker_child",
        "--ignored",
        "--nocapture",
    ])
    .kill_on_drop(true)
    .env_remove("MIY_FAILPOINT")
    .env_remove("AUDIT_CRASH_REMOVE");
    if let Some(f) = failpoint {
        c.env("MIY_FAILPOINT", f);
    }
    if let Some(n) = crash_remove {
        c.env("AUDIT_CRASH_REMOVE", n);
    }
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root().join("evidence/worker.log"))?;
    c.stdout(log.try_clone()?).stderr(log);
    Ok(c.spawn()?)
}
async fn terminal(pool: &sqlx::PgPool, id: Uuid) -> Result<Value> {
    for _ in 0..300 {
        let j = job(pool, id).await?;
        if j["status"] == "succeeded" || j["status"] == "failed" {
            return Ok(j);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    anyhow::bail!("job timed out")
}
#[tokio::test]
#[ignore = "owned audit worker subprocess only"]
async fn worker_child() -> Result<()> {
    ensure!(
        root().join("private-env.json").exists(),
        "audit env required"
    );
    unsafe {
        std::env::set_var("AUDIT_WORKER_PID", std::process::id().to_string());
    }
    crate::worker::run(sqlx::PgPool::connect(&std::env::var("DATABASE_URL")?).await?).await
}

#[tokio::test]
#[ignore = "isolated real PostgreSQL and Docker; run only through audit runner"]
async fn isolated_runtime_regression() -> Result<()> {
    let cfg: Value = serde_json::from_slice(&std::fs::read(root().join("config.json"))?)?;
    ensure!(std::env::var("MIY_DATABASE_CONTAINER")? == cfg["databaseContainer"].as_str().unwrap());
    ensure!(std::env::var("DATABASE_URL")?.ends_with("/miy_audit_core"));
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(24)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    sqlx::migrate!("../../migrations/core").run(&pool).await?;
    sqlx::query("INSERT INTO users(id,name,admin) VALUES('audit-admin','Synthetic admin',true),('audit-dev','Synthetic developer',false),('audit-user','Synthetic user',false) ON CONFLICT (id) DO NOTHING").execute(&pool).await?;
    // Only OIDC discovery metadata is a fixture. This run does not claim OAuth verification.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let origin = format!("http://{}", listener.local_addr()?);
    let issuer = origin.clone();
    let meta = json!({"issuer":issuer,"authorization_endpoint":format!("{issuer}/authorize"),"token_endpoint":format!("{issuer}/token"),"introspection_endpoint":format!("{issuer}/introspect"),"jwks_uri":format!("{issuer}/jwks"),"end_session_endpoint":format!("{issuer}/logout")});
    let app = axum::Router::new()
        .route(
            "/.well-known/openid-configuration",
            axum::routing::get(move || {
                let m = meta.clone();
                async move { axum::Json(m) }
            }),
        )
        .route(
            "/jwks",
            axum::routing::get(|| async { axum::Json(json!({"keys":[]})) }),
        );
    let oidc = tokio::spawn(async move { axum::serve(listener, app).await });
    unsafe {
        std::env::set_var("MIY_ISSUER", &origin);
        std::env::set_var("MIY_ORIGIN", &origin);
        std::env::set_var("MIY_WEB_SECRET", "synthetic");
        std::env::set_var("MIY_RESOURCE_SECRET", "synthetic");
    }
    let s = State {
        pool: pool.clone(),
        auth: Arc::new(auth::Auth::new().await?),
        artifacts: root().join("artifacts"),
    };
    let admin = actor("audit-admin");
    let dev = actor("audit-dev");
    let ordinary = actor("audit-user");
    domain::create_app(
        &s,
        &admin,
        domain::NewApp {
            id: "audit-runtime".into(),
            name: "Isolated runtime probe".into(),
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.code))?;
    sqlx::query("INSERT INTO grants VALUES('audit-dev','audit-runtime','dev','develop')")
        .execute(&pool)
        .await?;
    let image = |name: &str| cfg["images"][name]["id"].as_str().unwrap().to_owned();
    let first = image("v1");
    let denied = runtime::request(
        &s,
        &ordinary,
        "audit-runtime",
        req("deploy", "dev", Some(&first), None, "denied"),
    )
    .await;
    report(
        "unauthorized runtime request creates no job",
        denied.is_err()
            && sqlx::query_scalar::<_, i64>("SELECT count(*) FROM runtime_jobs")
                .fetch_one(&pool)
                .await?
                == 0,
        json!({"error":denied.err().map(|e|e.code)}),
    );
    let futures = (0..16).map(|_| {
        runtime::request(
            &s,
            &dev,
            "audit-runtime",
            req("deploy", "dev", Some(&first), None, "same-request"),
        )
    });
    let results = futures_util::future::join_all(futures).await;
    let ids: std::collections::BTreeSet<_> = results
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .map(|v| v["id"].to_string())
        .collect();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM runtime_jobs")
        .fetch_one(&pool)
        .await?;
    report(
        "16 concurrent duplicate submissions coalesce",
        results.iter().all(|r| r.is_ok()) && ids.len() == 1 && count == 1,
        json!({"uniqueIds":ids.len(),"rows":count}),
    );
    let conflict = runtime::request(
        &s,
        &dev,
        "audit-runtime",
        req("stop", "dev", None, None, "same-request"),
    )
    .await;
    report(
        "same key different input conflicts",
        conflict
            .as_ref()
            .err()
            .is_some_and(|e| e.code == "idempotency_conflict"),
        json!({"error":conflict.err().map(|e|e.code)}),
    );
    let id = uuid(&results[0].as_ref().unwrap()["id"]);
    let j = finish(&pool, id).await?;
    ensure!(j["status"] == "succeeded", "initial deploy {j}");
    let initial = inst(&pool, "dev").await?;
    let dev_id = uuid(&initial["id"]);
    let release = uuid(&initial["releaseId"]);
    let docker = runtime::inspect(initial["runtimeContainer"].as_str().unwrap()).await?;
    let mut devdb = db(&pool, dev_id, true).await?;
    devdb
        .execute("INSERT INTO app.records VALUES(1,'dev-retained')")
        .await?;
    report(
        "real Docker deployment and resource configuration",
        docker["Image"] == first
            && docker["HostConfig"]["Memory"] == 268435456_i64
            && docker["HostConfig"]["NanoCpus"] == 1000000000_i64
            && docker["HostConfig"]["ReadonlyRootfs"] == true,
        json!({"installation":initial,"memory":docker["HostConfig"]["Memory"],"nanoCpus":docker["HostConfig"]["NanoCpus"],"pidsLimit":docker["HostConfig"]["PidsLimit"],"user":docker["Config"]["User"]}),
    );
    let unaccepted = runtime::request(
        &s,
        &admin,
        "audit-runtime",
        req("promote", "prod", None, Some(release), "unaccepted"),
    )
    .await;
    report(
        "unaccepted production promotion rejected",
        unaccepted
            .as_ref()
            .err()
            .is_some_and(|e| e.code == "release_not_tested_in_dev"),
        json!({"error":unaccepted.err().map(|e|e.code)}),
    );
    runtime::check_release(
        &s,
        &dev,
        "audit-runtime",
        runtime::ReleaseCheck {
            release_id: release,
            note: "Synthetic lifecycle probe; not business acceptance".into(),
            checks: vec![],
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.code))?;
    sqlx::query("INSERT INTO grants VALUES('audit-dev','audit-runtime','prod','develop')")
        .execute(&pool)
        .await?;
    let wrong_perm = runtime::request(
        &s,
        &dev,
        "audit-runtime",
        req("promote", "prod", None, Some(release), "dev-only"),
    )
    .await;
    report(
        "prod develop permission cannot promote",
        wrong_perm.is_err(),
        json!({"error":wrong_perm.err().map(|e|e.code)}),
    );
    let pid = submit(
        &s,
        &admin,
        "promote",
        "prod",
        None,
        Some(release),
        "prod-first",
    )
    .await?;
    let pj = finish(&pool, pid).await?;
    ensure!(pj["status"] == "succeeded", "prod {pj}");
    let prod = inst(&pool, "prod").await?;
    let mut proddb = db(&pool, uuid(&prod["id"]), true).await?;
    let prodcount: i64 = sqlx::query_scalar("SELECT count(*) FROM app.records")
        .fetch_one(&mut proddb)
        .await?;
    proddb
        .execute("INSERT INTO app.records VALUES(1,'prod-retained')")
        .await?;
    report(
        "exact image promotion and independent prod data",
        prod["runtimeImage"] == initial["runtimeImage"]
            && prod["releaseId"] == initial["releaseId"]
            && prod["id"] != initial["id"]
            && prodcount == 0,
        json!({"dev":initial,"prod":prod,"initialProdRows":prodcount}),
    );
    let ddl = devdb.execute("CREATE TABLE app.forbidden(id int)").await;
    let history = devdb.execute("DELETE FROM app._sqlx_migrations").await;
    report(
        "runtime DB role cannot DDL or edit migration history",
        ddl.is_err() && history.is_err(),
        json!({"ddlDenied":ddl.is_err(),"historyDenied":history.is_err()}),
    );
    // Accepted queued job must honor later revocation before it touches the container.
    let revoke = submit(
        &s,
        &dev,
        "stop",
        "dev",
        None,
        None,
        "revoke-before-execution",
    )
    .await?;
    sqlx::query("DELETE FROM grants WHERE user_id='audit-dev' AND environment='dev'")
        .execute(&pool)
        .await?;
    let rj = finish(&pool, revoke).await?;
    let after = runtime::inspect(initial["runtimeContainer"].as_str().unwrap()).await?;
    report(
        "queued authority revoked before execution",
        rj["errorCode"] == "permission_revoked" && after["State"]["Running"] == true,
        rj,
    );
    sqlx::query("INSERT INTO grants VALUES('audit-dev','audit-runtime','dev','develop')")
        .execute(&pool)
        .await?;
    for (variant, key) in [
        ("wrong-app", "wrong-app"),
        ("v1-conflict", "immutable-version"),
    ] {
        let id = submit(&s, &dev, "deploy", "dev", Some(&image(variant)), None, key).await?;
        let j = finish(&pool, id).await?;
        let after = runtime::inspect(initial["runtimeContainer"].as_str().unwrap()).await?;
        report(
            key,
            j["status"] == "failed"
                && after["Id"] == docker["Id"]
                && after["State"]["Running"] == true,
            json!({"job":j,"oldContainerPreserved":after["Id"]==docker["Id"]}),
        );
    }
    let hash: String = sqlx::query_scalar("SELECT artifact_hash FROM releases WHERE id=$1")
        .bind(release)
        .fetch_one(&pool)
        .await?;
    let asset = s.artifacts.join(hash).join("ui/entry.js");
    let original = std::fs::read(&asset)?;
    std::fs::write(&asset, b"tampered")?;
    let id = submit(
        &s,
        &dev,
        "deploy",
        "dev",
        Some(&first),
        None,
        "tampered-artifact",
    )
    .await?;
    let j = finish(&pool, id).await?;
    std::fs::write(&asset, original)?;
    let after = runtime::inspect(initial["runtimeContainer"].as_str().unwrap()).await?;
    report(
        "stored artifact tampering rejected before old app stop",
        j["status"] == "failed" && after["Id"] == docker["Id"] && after["State"]["Running"] == true,
        j,
    );
    let updated = deployed(&s, &dev, &image("v2"), "update-v2").await?;
    let devvalue: String = sqlx::query_scalar("SELECT value FROM app.records WHERE id=1")
        .fetch_one(&mut devdb)
        .await?;
    let prodafter = inst(&pool, "prod").await?;
    report(
        "schema update preserves data and prod release",
        devvalue == "dev-retained"
            && prodafter["releaseId"] == prod["releaseId"]
            && updated["id"] == initial["id"],
        json!({"dev":updated,"prod":prodafter,"devValue":devvalue}),
    );
    let id = submit(
        &s,
        &dev,
        "deploy",
        "dev",
        Some(&image("bad")),
        None,
        "bad-migration",
    )
    .await?;
    let j = finish(&pool, id).await?;
    let mut admin_db = db(&pool, dev_id, false).await?;
    let bad_columns:i64=sqlx::query_scalar("SELECT count(*) FROM information_schema.columns WHERE table_schema='app' AND table_name='records' AND column_name='failed_column'").fetch_one(&mut admin_db).await?;
    let value: String = sqlx::query_scalar("SELECT value FROM app.records WHERE id=1")
        .fetch_one(&mut devdb)
        .await?;
    report(
        "failed migration rolls back DDL and preserves data",
        j["status"] == "failed" && bad_columns == 0 && value == "dev-retained",
        json!({"job":j,"partialColumnCount":bad_columns,"state":inst(&pool,"dev").await?,"retainedValue":value}),
    );
    let recovered = deployed(&s, &dev, &image("recovery"), "forward-recovery").await?;
    report(
        "new release recovers failed update",
        recovered["runtimeStatus"] == "running" && recovered["dbStatus"] == "ready",
        recovered.clone(),
    );
    let id = submit(
        &s,
        &dev,
        "deploy",
        "dev",
        Some(&first),
        None,
        "old-version-rollback",
    )
    .await?;
    let rollback = finish(&pool, id).await?;
    let value: String = sqlx::query_scalar("SELECT value FROM app.records WHERE id=1")
        .fetch_one(&mut devdb)
        .await?;
    report(
        "old image cannot silently roll back advanced schema",
        rollback["status"] == "failed" && value == "dev-retained",
        json!({"job":rollback,"state":inst(&pool,"dev").await?,"retainedValue":value,"automaticRollbackSupported":false}),
    );
    deployed(&s, &dev, &image("recovery"), "restore-current").await?;
    // Crash occurs in the unchanged worker after actual DB creation.
    let id = submit(
        &s,
        &dev,
        "deploy",
        "dev",
        Some(&image("recovery")),
        None,
        "crash-deployment",
    )
    .await?;
    let mut child = worker(Some("after_database"), None)?;
    let exit = tokio::time::timeout(Duration::from_secs(45), child.wait()).await??;
    let interrupted = job(&pool, id).await?;
    let mut resume = worker(None, None)?;
    let finalj = terminal(&pool, id).await?;
    resume.kill().await?;
    let _ = resume.wait().await;
    let retained: String = sqlx::query_scalar("SELECT value FROM app.records WHERE id=1")
        .fetch_one(&mut devdb)
        .await?;
    report(
        "worker crash after DB provisioning resumes deployment",
        exit.code() == Some(86)
            && interrupted["status"] == "running"
            && finalj["status"] == "succeeded"
            && retained == "dev-retained",
        json!({"exitCode":exit.code(),"interrupted":interrupted,"recovered":finalj,"retainedValue":retained}),
    );
    let stop = submit(&s, &dev, "stop", "dev", None, None, "manual-stop").await?;
    ensure!(finish(&pool, stop).await?["status"] == "succeeded");
    let mut idle = worker(None, None)?;
    tokio::time::sleep(Duration::from_secs(6)).await;
    idle.kill().await?;
    let _ = idle.wait().await;
    let stopped = inst(&pool, "dev").await?;
    let stopped_meta = runtime::inspect(stopped["runtimeContainer"].as_str().unwrap()).await?;
    report(
        "manual stop persists across worker restart",
        stopped["desiredState"] == "stopped"
            && stopped_meta["State"]["Running"] == false
            && stopped["healthStatus"] == "not_applicable"
            && stopped["healthCheckedAt"].is_null(),
        stopped,
    );
    let a = submit(&s, &dev, "start", "dev", None, None, "start-a").await?;
    tokio::time::sleep(Duration::from_millis(2)).await;
    let b = submit(&s, &dev, "stop", "dev", None, None, "stop-b").await?;
    runtime::tick(&pool).await?;
    runtime::tick(&pool).await?;
    let aj = job(&pool, a).await?;
    let bj = job(&pool, b).await?;
    report(
        "conflicting queued start-stop requests execute in order",
        aj["status"] == "succeeded"
            && bj["status"] == "succeeded"
            && inst(&pool, "dev").await?["desiredState"] == "stopped",
        json!({"start":aj,"stop":bj}),
    );
    let a = submit(&s, &dev, "start", "dev", None, None, "start-for-health").await?;
    ensure!(finish(&pool, a).await?["status"] == "succeeded");
    let live = inst(&pool, "dev").await?;
    let name = live["runtimeContainer"].as_str().unwrap();
    runtime::docker(&["exec", name, "touch", "/tmp/unhealthy"]).await?;
    let response = reqwest::get(format!("http://127.0.0.1:{}/health", live["runtimePort"])).await?;
    runtime_health::scan(&pool, &runtime_health::client()?).await?;
    let health_state = inst(&pool, "dev").await?;
    report(
        "first service failure is degraded while process remains running",
        response.status().as_u16() == 503
            && health_state["runtimeStatus"] == "running"
            && health_state["healthStatus"] == "degraded"
            && health_state["healthFailures"] == 1,
        health_state,
    );
    runtime_health::scan(&pool, &runtime_health::client()?).await?;
    runtime_health::scan(&pool, &runtime_health::client()?).await?;
    let unhealthy = inst(&pool, "dev").await?;
    report(
        "repeated HTTP failure is unhealthy without stopping container",
        unhealthy["runtimeStatus"] == "running"
            && unhealthy["healthStatus"] == "unhealthy"
            && unhealthy["healthFailures"] == 3,
        unhealthy,
    );
    runtime::docker(&["exec", name, "rm", "/tmp/unhealthy"]).await?;
    runtime_health::scan(&pool, &runtime_health::client()?).await?;
    let healthy = inst(&pool, "dev").await?;
    let transitions: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit WHERE target=$1 AND action LIKE 'runtime_health.%' ORDER BY id",
    )
    .bind(dev_id.to_string())
    .fetch_all(&pool)
    .await?;
    report(
        "HTTP recovery clears failures and records health transitions",
        healthy["healthStatus"] == "healthy"
            && healthy["healthFailures"] == 0
            && transitions.ends_with(&[
                "runtime_health.degraded".into(),
                "runtime_health.unhealthy".into(),
                "runtime_health.healthy".into(),
            ]),
        json!({"state":healthy,"transitions":transitions}),
    );
    let metadata = runtime::inspect(name).await?;
    // A daemon/list failure must never be mistaken for confirmed absence.
    unsafe {
        std::env::set_var("AUDIT_FAIL_LIST", "1");
    }
    let uncertain = submit(&s, &dev, "remove", "dev", None, None, "uncertain-docker").await?;
    let uncertain_job = finish(&pool, uncertain).await?;
    unsafe {
        std::env::remove_var("AUDIT_FAIL_LIST");
    }
    let preserved = runtime::inspect(name).await?;
    report(
        "Docker lookup failure preserves container and does not complete remove",
        uncertain_job["status"] == "failed"
            && preserved["Id"] == metadata["Id"]
            && preserved["State"]["Running"] == true,
        json!({"job":uncertain_job,"state":inst(&pool,"dev").await?}),
    );
    // A same-name replacement owned by somebody else must not be stopped/deleted.
    let backup = format!("{name}-audit-backup");
    runtime::docker(&["rename", name, &backup]).await?;
    let foreign = runtime::docker(&[
        "run",
        "-d",
        "--name",
        name,
        "--network",
        "none",
        "--memory",
        "64m",
        "--label",
        &format!("org.miy.installation={}", Uuid::new_v4()),
        &image("v1"),
    ])
    .await?;
    let forbidden = submit(&s, &dev, "remove", "dev", None, None, "foreign-container").await?;
    let foreign_job = finish(&pool, forbidden).await?;
    let kept = runtime::inspect(&foreign).await?;
    report(
        "foreign same-name container is preserved",
        foreign_job["status"] == "failed" && kept["State"]["Running"] == true,
        foreign_job,
    );
    runtime::docker(&["rm", "-f", &foreign]).await?;
    runtime::docker(&["rename", &backup, name]).await?;
    // Kill only the audit worker immediately after it successfully removes its own probe.
    let removal = submit(&s, &dev, "remove", "dev", None, None, "remove-crash").await?;
    let mut child = worker(None, Some(metadata["Id"].as_str().unwrap()))?;
    let exit = tokio::time::timeout(Duration::from_secs(45), child.wait()).await??;
    let before = job(&pool, removal).await?;
    let mut resume = worker(None, None)?;
    let after = terminal(&pool, removal).await?;
    tokio::time::sleep(Duration::from_secs(6)).await;
    resume.kill().await?;
    let _ = resume.wait().await;
    let removal_state = inst(&pool, "dev").await?;
    let retained: String = sqlx::query_scalar("SELECT value FROM app.records WHERE id=1")
        .fetch_one(&mut devdb)
        .await?;
    report(
        "remove resumes after container deletion before DB status commit",
        after["status"] == "succeeded" && removal_state["runtimeStatus"] == "removed",
        json!({"workerExit":exit.to_string(),"before":before,"after":after,"state":removal_state,"databaseRetained":retained=="dev-retained","crashMarker":root().join("evidence/remove-crash-triggered.json").exists()}),
    );
    let again = submit(&s, &dev, "remove", "dev", None, None, "remove-again").await?;
    let again_job = finish(&pool, again).await?;
    report(
        "confirmed absent container removal is idempotent",
        again_job["status"] == "succeeded"
            && inst(&pool, "dev").await?["runtimeStatus"] == "removed",
        again_job,
    );
    // Redeploy after removal must retain the same database.
    let restored = deployed(&s, &dev, &image("recovery"), "redeploy-after-remove").await?;
    report(
        "redeploy after interrupted removal restores same data",
        restored["runtimeStatus"] == "running"
            && sqlx::query_scalar::<_, String>("SELECT value FROM app.records WHERE id=1")
                .fetch_one(&mut devdb)
                .await?
                == "dev-retained",
        restored,
    );
    monitoring_boundaries(&s, &admin).await?;
    let installations: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id',id,'container',runtime_container) FROM installations",
    )
    .fetch_all(&pool)
    .await?;
    std::fs::write(
        root().join("evidence/owned-installations.json"),
        serde_json::to_vec_pretty(&installations)?,
    )?;
    oidc.abort();
    let failures = std::fs::read_to_string(root().join("evidence/results.jsonl"))?
        .lines()
        .filter(|line| serde_json::from_str::<Value>(line).is_ok_and(|v| v["status"] == "FAIL"))
        .count();
    ensure!(
        failures == 0,
        "{failures} regression observations failed; see results.jsonl"
    );
    Ok(())
}
