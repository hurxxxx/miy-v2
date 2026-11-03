//! Integration harness with real runtimes and test-only lost-acknowledgement hooks.
use crate::{State, auth, domain, runtime, runtime_health};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sqlx::{Connection, Executor, Row};
use std::{path::PathBuf, sync::Arc, time::Duration};
use uuid::Uuid;

// The test-only hook returns a transport error after a real PostgreSQL write has
// committed. It models a lost acknowledgement without changing the persisted
// state or exposing failure injection in a production binary.
static DEPLOY_ACK_LOSS: std::sync::Mutex<Option<(Uuid, &'static str)>> =
    std::sync::Mutex::new(None);

pub(crate) fn deployment_write_ack(job: Uuid, stage: &'static str) -> Result<()> {
    let mut loss = DEPLOY_ACK_LOSS.lock().unwrap();
    if *loss == Some((job, stage)) {
        *loss = None;
        return Err(sqlx::Error::Io(std::io::ErrorKind::ConnectionReset.into()).into());
    }
    Ok(())
}

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
        cli_session_id: None,
        delegation_id: None,
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
    serde_json::from_value(json!({"action":action,"environment":env,"imageId":image,"releaseId":release,"idempotencyKey":key,"confirmed":true,"expectedState":{"target":{"appId":"audit-runtime","environment":env},"installation":null}})).unwrap()
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
    let mut request = req(action, env, image, release, key);
    request.expected_state = Some(
        crate::runtime_preflight::snapshot(&s.pool, "audit-runtime", env)
            .await
            .map_err(|e| anyhow::anyhow!(e.code))?,
    );
    let v = runtime::request(s, p, "audit-runtime", request)
        .await
        .map_err(|e| anyhow::anyhow!(e.code))?;
    Ok(serde_json::from_value(v["id"].clone())?)
}
async fn job(pool: &sqlx::PgPool, id: Uuid) -> Result<Value> {
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('id',id,'status',status,'step',step,'errorCode',error_code,'releaseId',release_id,'failureClass',failure_class,'retryable',retryable) FROM runtime_jobs WHERE id=$1").bind(id).fetch_one(pool).await?)
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

fn worker_application_name(process: u32) -> String {
    format!("miy-audit-worker-{process}")
}

async fn worker_guard_pid(pool: &sqlx::PgPool, process: u32) -> Result<i32> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let pid = sqlx::query_scalar("SELECT a.pid FROM pg_locks l JOIN pg_stat_activity a ON a.pid=l.pid WHERE l.locktype='advisory' AND l.classid=0 AND l.objid=73490123 AND l.objsubid=1 AND l.granted AND l.database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND a.application_name=$1")
                .bind(worker_application_name(process)).fetch_optional(pool).await?;
            if let Some(pid) = pid {
                return Ok(pid);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .context("worker did not acquire the isolated ownership lock")?
}

async fn blocked_build_recovery(pool: &sqlx::PgPool, process: u32) -> Result<i32> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let pid = sqlx::query_scalar("SELECT pid FROM pg_stat_activity WHERE datname=current_database() AND application_name=$1 AND state='active' AND wait_event_type='Lock' AND query LIKE 'SELECT * FROM app_builds WHERE status=''running''%' LIMIT 1")
                .bind(worker_application_name(process)).fetch_optional(pool).await?;
            if let Some(pid) = pid {
                return Ok(pid);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .context("worker recovery did not reach the held app_builds table lock")?
}

async fn start_during_recovery(s: &State, admin: &auth::Principal, key: &str) -> Result<Value> {
    tokio::time::timeout(Duration::from_secs(12), async {
        let id = submit(s, admin, "start", "prod", None, None, key).await?;
        let result = terminal(&s.pool, id).await?;
        ensure!(result["status"] == "succeeded", "{key}: {result}");
        Ok(result)
    })
    .await
    .context("runtime control was blocked by build recovery")?
}

// Real worker subprocesses, PostgreSQL locks and the existing owned production
// fixture. No substitute build implementation or worker scheduling hook is used.
async fn worker_recovery_boundaries(s: &State, admin: &auth::Principal) -> Result<()> {
    // Stop before measuring startup so Docker's normal stop grace is excluded.
    let stop = submit(
        s,
        admin,
        "stop",
        "prod",
        None,
        None,
        "recovery-prepare-prod",
    )
    .await?;
    ensure!(finish(&s.pool, stop).await?["status"] == "succeeded");
    let mut build_lock = s.pool.acquire().await?;
    build_lock.close_on_drop();
    ensure!(
        sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_lock(73490125)")
            .fetch_one(&mut *build_lock)
            .await?,
        "unexpected live build owner in the isolated fixture"
    );
    let build = Uuid::new_v4();
    ensure!(!crate::server_build::archive_path(build)?.exists());
    sqlx::query("INSERT INTO app_builds(id,app_id,actor,request_id,source_commit,archive_hash,size_bytes,platform,status,step,started_at) VALUES($1,'audit-runtime',$2,$3,$4,$5,1,$6,'running','validating_source',now())")
        .bind(build).bind(&admin.id).bind(Uuid::new_v4()).bind("a".repeat(40))
        .bind(crate::contract::hash(build.as_bytes()))
        .bind(crate::runtime_platform::server().context("unsupported fixture platform")?)
        .execute(&s.pool).await?;
    let mut first = worker(None, None)?;
    let first_process = first.id().context("worker process was not started")?;
    worker_guard_pid(&s.pool, first_process).await?;
    let control = start_during_recovery(s, admin, "start-with-held-build-lock").await?;
    let pending: String = sqlx::query_scalar("SELECT status FROM app_builds WHERE id=$1")
        .bind(build)
        .fetch_one(&s.pool)
        .await?;
    ensure!(
        pending == "running",
        "build recovery bypassed its ownership lock"
    );
    ensure!(
        sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock(73490125)")
            .fetch_one(&mut *build_lock)
            .await?
    );
    drop(build_lock);
    let recovered: String = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let row = sqlx::query("SELECT status,error_code FROM app_builds WHERE id=$1")
                .bind(build)
                .fetch_one(&s.pool)
                .await?;
            if row.get::<String, _>("status") == "failed" {
                return Ok::<String, anyhow::Error>(row.get("error_code"));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .context("build recovery was skipped after its lock became available")??;
    ensure!(
        recovered == "source_validation_failed",
        "unexpected recovery failure code: {recovered}"
    );
    ensure!(
        tokio::process::Command::new("/bin/kill")
            .args(["-TERM", "--", &first_process.to_string()])
            .status()
            .await?
            .success()
    );
    ensure!(
        tokio::time::timeout(Duration::from_secs(8), first.wait())
            .await??
            .success()
    );
    report(
        "held build ownership does not block control; recovery resumes when released",
        true,
        json!({"controlJob":control,"buildStatusWhileLocked":pending,"recoveredErrorCode":recovered,"workerPoolConnections":10}),
    );

    let stop = submit(
        s,
        admin,
        "stop",
        "prod",
        None,
        None,
        "table-lock-prepare-prod",
    )
    .await?;
    ensure!(finish(&s.pool, stop).await?["status"] == "succeeded");
    let mut table_lock = s.pool.begin().await?;
    sqlx::query("LOCK TABLE app_builds IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *table_lock)
        .await?;
    let mut blocked = worker(None, None)?;
    let blocked_process = blocked.id().context("worker process was not started")?;
    let guard_pid = worker_guard_pid(&s.pool, blocked_process).await?;
    let recovery_pid = blocked_build_recovery(&s.pool, blocked_process).await?;
    ensure!(
        guard_pid != recovery_pid,
        "recovery reused the ownership connection"
    );
    let control = start_during_recovery(s, admin, "start-with-blocked-build-recovery").await?;
    // Recheck every ownership condition in the terminating statement. Only this
    // subprocess's global guard in this disposable database may be terminated.
    let terminated: bool = sqlx::query_scalar("SELECT pg_terminate_backend(a.pid) FROM pg_locks l JOIN pg_stat_activity a ON a.pid=l.pid WHERE a.pid=$1 AND a.application_name=$2 AND a.datname=current_database() AND l.locktype='advisory' AND l.classid=0 AND l.objid=73490123 AND l.objsubid=1 AND l.granted AND l.database=(SELECT oid FROM pg_database WHERE datname=current_database())")
        .bind(guard_pid).bind(worker_application_name(blocked_process)).fetch_one(&s.pool).await?;
    ensure!(
        terminated,
        "isolated worker ownership connection was not terminated"
    );
    let exit = tokio::time::timeout(Duration::from_secs(8), blocked.wait())
        .await
        .context("worker did not exit after losing ownership during recovery")??;
    ensure!(
        !exit.success(),
        "worker ownership loss was reported as success"
    );
    let mut successor = worker(None, None)?;
    let successor_process = successor.id().context("successor worker was not started")?;
    worker_guard_pid(&s.pool, successor_process).await?;
    blocked_build_recovery(&s.pool, successor_process).await?;
    let successor_control =
        start_during_recovery(s, admin, "successor-start-with-blocked-recovery").await?;
    report(
        "blocked recovery does not delay ownership loss or successor control",
        true,
        json!({"controlJob":control,"successorControlJob":successor_control,"recoverySqlObservedWaiting":true,"ownershipLossExit":exit.code(),"tableLockStillHeld":true}),
    );

    // Exercise shutdown while the successor still holds BUILD_LOCK and its
    // recovery query is blocked. Dropping those futures must release both locks.
    let signal = tokio::process::Command::new("/bin/kill")
        .args(["-TERM", "--", &successor_process.to_string()])
        .status()
        .await?;
    ensure!(
        signal.success(),
        "could not signal the owned worker subprocess"
    );
    let exit = tokio::time::timeout(Duration::from_secs(8), successor.wait())
        .await
        .context("SIGTERM did not stop the worker during blocked recovery")??;
    ensure!(exit.success(), "worker SIGTERM was not a clean exit");
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_locks WHERE locktype='advisory' AND classid=0 AND objid IN (73490123,73490125) AND objsubid=1 AND granted AND database=(SELECT oid FROM pg_database WHERE datname=current_database())")
        .fetch_one(&s.pool).await?;
    ensure!(
        remaining == 0,
        "worker retained ownership or build lock after shutdown"
    );
    table_lock.rollback().await?;
    report(
        "SIGTERM cancels blocked recovery and releases worker and build ownership",
        true,
        json!({"workerExit":exit.code(),"remainingAdvisoryLocks":remaining,"tableLockReleased":true}),
    );
    Ok(())
}

async fn deployed(s: &State, p: &auth::Principal, image: &str, key: &str) -> Result<Value> {
    let id = submit(s, p, "deploy", "dev", Some(image), None, key).await?;
    let result = finish(&s.pool, id).await?;
    ensure!(result["status"] == "succeeded", "{key}: {result}");
    inst(&s.pool, "dev").await
}

async fn lost_deployment_acknowledgements(
    s: &State,
    p: &auth::Principal,
    image: &str,
) -> Result<()> {
    for stage in ["before_stop", "before_start", "after_start"] {
        let before = inst(&s.pool, "dev").await?;
        let name = before["runtimeContainer"].as_str().unwrap();
        let original = runtime::inspect(name).await?;
        ensure!(original["State"]["Running"] == true);
        let id = submit(
            s,
            p,
            "deploy",
            "dev",
            Some(image),
            None,
            &format!("lost-ack-{stage}"),
        )
        .await?;
        *DEPLOY_ACK_LOSS.lock().unwrap() = Some((id, stage));
        let failed = finish(&s.pool, id).await?;
        ensure!(
            DEPLOY_ACK_LOSS.lock().unwrap().take().is_none(),
            "ack loss was not reached"
        );
        ensure!(
            failed["status"] == "failed",
            "ack-loss job should not claim success"
        );
        let at_failure = runtime::inspect(name).await?;
        // Exercise both subsequent reconciliation and worker recovery. Neither
        // may remove/stop a container merely because the write result was unknown.
        runtime::reconcile(&s.pool).await?;
        runtime::recover(&s.pool).await?;
        runtime::reconcile(&s.pool).await?;
        let after = runtime::inspect(name).await?;
        let current = inst(&s.pool, "dev").await?;
        let expected_running = stage != "before_start";
        let preserved = after["Id"] == at_failure["Id"]
            && after["State"]["Running"] == expected_running
            && after["State"]["StartedAt"] == at_failure["State"]["StartedAt"]
            && after["State"]["FinishedAt"] == at_failure["State"]["FinishedAt"]
            && current["desiredState"] == "running"
            && current["runtimeStatus"]
                == if expected_running {
                    "running"
                } else {
                    "stopped"
                }
            && (stage != "before_stop"
                || (after["Id"] == original["Id"] && current["releaseId"] == before["releaseId"]));
        report(
            &format!("lost deployment acknowledgement {stage} preserves the observed container"),
            preserved,
            json!({"job":failed,"installation":current,"sameContainerAfterReconcile":after["Id"]==at_failure["Id"],"running":after["State"]["Running"]}),
        );
        ensure!(
            preserved,
            "reconciliation changed an uncertain deployment's container"
        );
        if !expected_running {
            let start = submit(s, p, "start", "dev", None, None, "start-after-lost-ack").await?;
            ensure!(finish(&s.pool, start).await?["status"] == "succeeded");
        }
        runtime_health::scan(&s.pool, &runtime_health::client()?).await?;
        ensure!(inst(&s.pool, "dev").await?["healthStatus"] == "healthy");
    }
    Ok(())
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
    let options = std::env::var("DATABASE_URL")?
        .parse::<sqlx::postgres::PgConnectOptions>()?
        .application_name(&worker_application_name(std::process::id()));
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect_with(options)
        .await?;
    crate::worker::run(pool).await
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
    let api_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let api_origin = format!("http://{}", api_listener.local_addr()?);
    unsafe {
        std::env::set_var("MIY_DEV_LOGIN", "1");
        std::env::set_var("MIY_DEV_ORIGIN", &api_origin);
    }
    let s = State {
        pool: pool.clone(),
        auth: Arc::new(auth::Auth::new().await?),
        artifacts: root().join("artifacts"),
    };
    let api_router = crate::http::router(s.clone());
    let api_server = tokio::spawn(async move { axum::serve(api_listener, api_router).await });
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
    let (first, first_local, import_evidence) = crate::image_import_regression::run(
        &s,
        &dev,
        &image("v1"),
        &format!("{}:legacy-v1", cfg["prefix"].as_str().unwrap()),
    )
    .await?;
    report(
        "classic Docker archive resolves a verified immutable server image",
        true,
        import_evidence,
    );
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
        docker["Image"] == first_local
            && initial["runtimeImage"] == first
            && docker["HostConfig"]["Memory"] == 268435456_i64
            && docker["HostConfig"]["NanoCpus"] == 1000000000_i64
            && docker["HostConfig"]["ReadonlyRootfs"] == true,
        json!({"installation":initial,"serverLocalImage":docker["Image"],"memory":docker["HostConfig"]["Memory"],"nanoCpus":docker["HostConfig"]["NanoCpus"],"pidsLimit":docker["HostConfig"]["PidsLimit"],"user":docker["Config"]["User"]}),
    );
    // Installations prepared before per-role caps receive them when a runner starts.
    let (runtime_role, migration_role): (String, String) = sqlx::query_as(
        "SELECT runtime_role, migration_role FROM db_resources WHERE installation_id=$1",
    )
    .bind(dev_id)
    .fetch_one(&pool)
    .await?;
    let mut provision =
        sqlx::PgConnection::connect(&std::env::var("PROVISION_DATABASE_URL")?).await?;
    for role in [&runtime_role, &migration_role] {
        provision
            .execute(format!("ALTER ROLE {role} CONNECTION LIMIT -1").as_str())
            .await?;
    }
    let changed = crate::worker::apply_connection_limits(&pool).await?;
    let limits: Vec<i32> = sqlx::query_scalar(
        "SELECT rolconnlimit FROM pg_roles WHERE rolname = ANY($1) ORDER BY rolname = $2 DESC",
    )
    .bind(vec![runtime_role.clone(), migration_role.clone()])
    .bind(&runtime_role)
    .fetch_all(&pool)
    .await?;
    let repeated = crate::worker::apply_connection_limits(&pool).await?;
    provision.close().await?;
    report(
        "existing installations receive role connection limits when a runner starts",
        changed == 2 && limits == [6, 2] && repeated == 0,
        json!({"changed":changed,"limits":limits,"repeated":repeated}),
    );
    let restart = submit(
        &s,
        &dev,
        "restart",
        "dev",
        None,
        None,
        "restart-imported-image",
    )
    .await?;
    let restarted = finish(&pool, restart).await?;
    runtime::reconcile(&pool).await?;
    runtime_health::scan(&pool, &runtime_health::client()?).await?;
    let after_restart = inst(&pool, "dev").await?;
    report(
        "imported original identity survives restart, reconciliation and health checks",
        restarted["status"] == "succeeded"
            && after_restart["runtimeImage"] == first
            && after_restart["runtimeStatus"] == "running"
            && after_restart["healthStatus"] == "healthy",
        json!({"job":restarted,"installation":after_restart,"serverLocalImageId":first_local}),
    );
    // Planning is a read: no runtime/job/installation mutation, and observations do not change a control revision.
    let before_plan = crate::runtime_preflight::snapshot(&pool, "audit-runtime", "dev")
        .await
        .map_err(|e| anyhow::anyhow!(e.code))?;
    let before_jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM runtime_jobs")
        .fetch_one(&pool)
        .await?;
    let input = serde_json::from_value(
        json!({"target":{"appId":"audit-runtime","environment":"dev"},"action":"stop"}),
    )?;
    let plan = crate::runtime_preflight::inspect(&s, &dev, "audit-runtime", input)
        .await
        .map_err(|e| anyhow::anyhow!(e.code))?;
    let after_jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM runtime_jobs")
        .fetch_one(&pool)
        .await?;
    let after_plan = crate::runtime_preflight::snapshot(&pool, "audit-runtime", "dev")
        .await
        .map_err(|e| anyhow::anyhow!(e.code))?;
    report(
        "preflight creates no job and changes no installation",
        before_jobs == after_jobs
            && serde_json::to_value(&before_plan)? == serde_json::to_value(&after_plan)?,
        json!({"plan":plan,"jobCount":after_jobs}),
    );
    runtime_health::record(&pool, dev_id, runtime_health::Observation::Healthy).await?;
    let observed = crate::runtime_preflight::snapshot(&pool, "audit-runtime", "dev")
        .await
        .map_err(|e| anyhow::anyhow!(e.code))?;
    report(
        "health observations do not invalidate a control revision",
        crate::runtime_contract::compare_expected(&before_plan, &observed).is_ok(),
        json!({"expected":before_plan,"current":observed}),
    );
    sqlx::query("UPDATE installations SET db_status='provisioning' WHERE id=$1")
        .bind(dev_id)
        .execute(&pool)
        .await?;
    sqlx::query("UPDATE installations SET db_status='ready' WHERE id=$1")
        .bind(dev_id)
        .execute(&pool)
        .await?;
    let mut stale = req("stop", "dev", None, None, "stale-preflight");
    stale.expected_state = Some(before_plan);
    let refused = runtime::request(&s, &dev, "audit-runtime", stale).await;
    report(
        "stale preflight is rejected before job creation",
        refused
            .as_ref()
            .err()
            .is_some_and(|e| e.code == "state_changed")
            && sqlx::query_scalar::<_, i64>("SELECT count(*) FROM runtime_jobs")
                .fetch_one(&pool)
                .await?
                == after_jobs,
        json!({"error":refused.err().map(|e|e.code)}),
    );
    let mut checked = req("stop", "dev", None, None, "worker-rechecks-state");
    checked.expected_state = Some(
        crate::runtime_preflight::snapshot(&pool, "audit-runtime", "dev")
            .await
            .map_err(|e| anyhow::anyhow!(e.code))?,
    );
    let accepted = runtime::request(&s, &dev, "audit-runtime", checked)
        .await
        .map_err(|e| anyhow::anyhow!(e.code))?;
    sqlx::query("UPDATE installations SET db_status='provisioning' WHERE id=$1")
        .bind(dev_id)
        .execute(&pool)
        .await?;
    sqlx::query("UPDATE installations SET db_status='ready' WHERE id=$1")
        .bind(dev_id)
        .execute(&pool)
        .await?;
    let rejected_job = finish(&pool, uuid(&accepted["id"])).await?;
    let still_running = runtime::inspect(initial["runtimeContainer"].as_str().unwrap()).await?;
    report(
        "worker rechecks expected state before container changes",
        rejected_job["status"] == "failed"
            && rejected_job["errorCode"] == "state_changed"
            && still_running["State"]["Running"] == true,
        json!({"job":rejected_job,"originalContainerRunning":true}),
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
    let prod_container = runtime::inspect(prod["runtimeContainer"].as_str().unwrap()).await?;
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
            && prod_container["Image"] == first_local
            && prod["releaseId"] == initial["releaseId"]
            && prod["id"] != initial["id"]
            && prodcount == 0,
        json!({"dev":initial,"prod":prod,"serverLocalImage":prod_container["Image"],"initialProdRows":prodcount}),
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
    lost_deployment_acknowledgements(&s, &dev, &first).await?;
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
        j["status"] == "failed"
            && j["errorCode"] == "migration_failed"
            && j["failureClass"] == "never"
            && j["retryable"] == false
            && bad_columns == 0
            && value == "dev-retained",
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
        rollback["status"] == "failed"
            && rollback["errorCode"] == "migration_history_mismatch"
            && rollback["failureClass"] == "never"
            && rollback["retryable"] == false
            && value == "dev-retained",
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
    let blocked = submit(&s, &dev, "stop", "dev", None, None, "stop-b").await;
    ensure!(blocked.is_err());
    runtime::tick(&pool).await?;
    let b = submit(&s, &dev, "stop", "dev", None, None, "stop-b").await?;
    runtime::tick(&pool).await?;
    let aj = job(&pool, a).await?;
    let bj = job(&pool, b).await?;
    report(
        "competing changes require fresh preflight after the pending operation",
        aj["status"] == "succeeded"
            && bj["status"] == "succeeded"
            && blocked.unwrap_err().to_string() == "operation_in_progress"
            && inst(&pool, "dev").await?["desiredState"] == "stopped",
        json!({"start":aj,"stop":bj,"firstStopAdmission":"operation_in_progress"}),
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
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()?;
    for name in crate::operations_regression::run(&s, &client, &api_origin).await? {
        report(
            &name,
            true,
            json!({"suite":"operations","transport":"HTTP","storage":"isolated PostgreSQL","runtime":"owned Docker fixtures"}),
        );
    }
    for name in crate::cli_regression::run(&s, &api_origin).await? {
        report(
            &name,
            true,
            json!({"suite":"cli","transport":"actual CLI binary and HTTP","storage":"isolated config and PostgreSQL"}),
        );
    }
    for name in crate::auth_sessions_regression::run(&s, &client, &api_origin).await? {
        report(
            &name,
            true,
            json!({"suite":"auth","transport":"HTTP","storage":"isolated PostgreSQL","identity":"synthetic development account"}),
        );
    }
    for name in crate::permissions_regression::run(&s, &client, &api_origin).await? {
        report(
            &name,
            true,
            json!({"suite":"permissions","transport":"HTTP and MCP","storage":"isolated PostgreSQL","runtime":"signed loopback app fixtures"}),
        );
    }
    for name in crate::delegation_regression::run(&s, &client, &api_origin).await? {
        report(
            &name,
            true,
            json!({"suite":"delegation","transport":"HTTP","storage":"isolated PostgreSQL","identity":"synthetic CLI sessions and delegated credentials"}),
        );
    }
    worker_recovery_boundaries(&s, &admin).await?;
    api_server.abort();
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
