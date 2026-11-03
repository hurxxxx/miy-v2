use crate::{
    auth, contract,
    runtime_contract::{FailureCode, RuntimeFailure},
};
use anyhow::{Context, Result};
use sqlx::{Connection, Executor, Row};
use std::{future::Future, time::Duration};
use uuid::Uuid;

pub async fn run(pool: sqlx::PgPool) -> Result<()> {
    // A dedicated connection owns this session lock. A second runner cannot
    // reclaim another live runner's jobs. On process death PostgreSQL releases it.
    let mut guard = pool.acquire().await?;
    guard.close_on_drop();
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(73490123)")
        .fetch_one(&mut *guard)
        .await?;
    anyhow::ensure!(acquired, "another_db_runner_is_active");
    tracing::info!(event = "db_runner_ready");
    // HTTP observations never occupy the deployment/control queue. The task is
    // aborted before this runner releases its exclusive ownership on shutdown.
    let _log_monitor = crate::runtime_logs::start(pool.clone());
    let _health_monitor = crate::runtime_health::start(pool.clone())?;
    let _development_monitor = crate::development::start(pool.clone());
    // Direct futures share ownership of this scope: a failed lane or lost
    // session lock drops every other lane before the ownership connection.
    // Build recovery can itself take minutes and must not delay control startup.
    supervise(
        control_loop(&pool),
        build_loop(&pool),
        watch_ownership(&mut guard),
        shutdown(),
    )
    .await
}

async fn supervise(
    control: impl Future<Output = Result<()>>,
    builds: impl Future<Output = Result<()>>,
    ownership: impl Future<Output = Result<()>>,
    shutdown: impl Future<Output = Result<()>>,
) -> Result<()> {
    tokio::select! {
        // Ownership and shutdown take precedence when multiple branches wake.
        biased;
        result = ownership => { result?; anyhow::bail!("worker_ownership_monitor_stopped") },
        result = shutdown => result,
        result = control => { result?; anyhow::bail!("worker_control_loop_stopped") },
        result = builds => { result?; anyhow::bail!("worker_build_loop_stopped") },
    }
}

async fn watch_ownership(guard: &mut sqlx::pool::PoolConnection<sqlx::Postgres>) -> Result<()> {
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        tokio::time::timeout(
            Duration::from_secs(3),
            sqlx::query("SELECT 1").execute(&mut **guard),
        )
        .await
        .context("worker_ownership_check_timeout")?
        .context("worker_ownership_connection_lost")?;
    }
}

async fn shutdown() -> Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result?,
            _ = terminate.recv() => {},
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await?;
    tracing::info!(event = "worker_shutdown");
    Ok(())
}

async fn build_loop(pool: &sqlx::PgPool) -> Result<()> {
    // A previous worker may still be unwinding its build lock after losing
    // ownership. Retry recovery instead of silently abandoning running jobs.
    while !crate::server_build_worker::recover(pool).await? {
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    loop {
        if !crate::server_build_worker::tick(pool).await? {
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }
}

async fn control_loop(pool: &sqlx::PgPool) -> Result<()> {
    sqlx::query("UPDATE jobs SET status='queued',updated_at=now() WHERE status='running'")
        .execute(pool)
        .await?;
    crate::runtime::recover(pool).await?;
    crate::development::recover(pool).await?;
    sqlx::query("UPDATE image_uploads SET status='pending' WHERE status='loading'")
        .execute(pool)
        .await?;
    let mut last_reconcile = std::time::Instant::now();
    loop {
        if last_reconcile.elapsed().as_secs() >= 5 {
            crate::runtime::reconcile(pool).await?;
            last_reconcile = std::time::Instant::now();
        }
        let row =
            sqlx::query("SELECT * FROM jobs WHERE status='queued' ORDER BY created_at,id LIMIT 1")
                .fetch_optional(pool)
                .await?;
        if let Some(row) = row {
            let id: Uuid = row.get("id");
            let installation: Uuid = row.get("installation_id");
            let release: Uuid = row.get("release_id");
            let target = sqlx::query("SELECT app_id,environment FROM installations WHERE id=$1")
                .bind(installation)
                .fetch_one(pool)
                .await?;
            let mut app_lock = pool.acquire().await?;
            app_lock.close_on_drop();
            sqlx::query("SELECT pg_advisory_lock(hashtextextended($1,0))")
                .bind(format!(
                    "runtime:{}:{}",
                    target.get::<String, _>("app_id"),
                    target.get::<String, _>("environment")
                ))
                .execute(&mut *app_lock)
                .await?;
            let mut lock = pool.acquire().await?;
            lock.close_on_drop();
            sqlx::query("SELECT pg_advisory_lock(hashtextextended($1,0))")
                .bind(installation.to_string())
                .execute(&mut *lock)
                .await?;
            sqlx::query("UPDATE jobs SET status='running',updated_at=now() WHERE id=$1")
                .bind(id)
                .execute(pool)
                .await?;
            let result = async {
                crate::auth_sessions::ensure_active(pool,row.get("actor")).await.map_err(|_| RuntimeFailure::new(FailureCode::UserDisabled))?;
                let authorized:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id=$1 AND admin) OR EXISTS(SELECT 1 FROM installations i JOIN effective_grants g ON g.app_id=i.app_id AND g.environment=i.environment WHERE i.id=$2 AND i.environment='dev' AND g.user_id=$1 AND g.permission='develop')")
                    .bind(row.get::<String,_>("actor")).bind(installation).fetch_one(pool).await?;
                anyhow::ensure!(authorized, RuntimeFailure::new(FailureCode::PermissionRevoked));
                if target.get::<String,_>("environment")=="dev" && crate::development::active_for_app(pool,target.get("app_id")).await? {anyhow::bail!(RuntimeFailure::new(FailureCode::ActiveDevelopmentSession));}
                crate::delegations::check_queued(pool,row.get("actor"),row.get("delegation_id"),target.get("app_id"),target.get("environment"),row.get::<String,_>("delegation_action").parse().map_err(|_|RuntimeFailure::new(FailureCode::InvalidRequest))?).await?;
                prepare(pool, id, installation, release).await
            }.await;
            match result {
                Ok(()) => {
                    sqlx::query("UPDATE jobs SET status='succeeded',step='ready',error_code=NULL,updated_at=now() WHERE id=$1").bind(id).execute(pool).await?;
                }
                Err(e) => {
                    let failure = crate::runtime::classify_database_prepare_failure(e);
                    let code = failure.code();
                    sqlx::query("UPDATE jobs SET status='failed',error_code=$2,updated_at=now() WHERE id=$1").bind(id).bind(code).execute(pool).await?;
                    sqlx::query("UPDATE installations SET db_status='failed' WHERE id=$1")
                        .bind(installation)
                        .execute(pool)
                        .await?;
                    tracing::error!(event="database_job_failed",job_id=%id,code);
                }
            }
            sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1,0))")
                .bind(installation.to_string())
                .execute(&mut *lock)
                .await?;
        } else if !crate::development::tick(pool).await?
            && !crate::delivery::tick(pool).await?
            && !crate::runtime::tick(pool).await?
        {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }
    }
}
async fn step(pool: &sqlx::PgPool, id: Uuid, name: &str) -> Result<()> {
    sqlx::query("UPDATE jobs SET step=$2,updated_at=now() WHERE id=$1")
        .bind(id)
        .bind(name)
        .execute(pool)
        .await?;
    Ok(())
}
/// The SDK pool opens at most 5 runtime connections; one more covers a manual check.
pub(crate) const RUNTIME_CONNECTION_LIMIT: u32 = 6;
/// Migrations run on one connection; one more covers an overlapping retry.
pub(crate) const MIGRATION_CONNECTION_LIMIT: u32 = 2;

pub(crate) async fn prepare(
    pool: &sqlx::PgPool,
    job: Uuid,
    installation: Uuid,
    release: Uuid,
) -> Result<()> {
    let row = sqlx::query("SELECT artifact_hash,manifest FROM releases WHERE id=$1")
        .bind(release)
        .fetch_one(pool)
        .await?;
    let root = std::path::PathBuf::from(std::env::var("MIY_ARTIFACTS")?);
    let artifact = contract::verified_artifact(&root, row.get("artifact_hash"))
        .map_err(|_| RuntimeFailure::new(FailureCode::InvalidImageContract))?;
    let m: contract::Manifest = serde_json::from_value(row.get("manifest"))?;
    contract::validate(&m).map_err(anyhow::Error::msg)?;
    let migrations = artifact.join(&m.migrations.directory);
    crate::bundle::migrations(&migrations)?;
    let target = sqlx::query("SELECT app_id,environment FROM installations WHERE id=$1")
        .bind(installation)
        .fetch_one(pool)
        .await?;
    let resource = ensure_resource(
        pool,
        target.get("app_id"),
        target.get("environment"),
        Some(installation),
    )
    .await?;
    sqlx::query("UPDATE installations SET db_status='provisioning' WHERE id=$1")
        .bind(installation)
        .execute(pool)
        .await?;
    prepare_resource(pool, &resource, &migrations, Some(job)).await?;
    sqlx::query("UPDATE installations SET db_status='ready',release_id=$2 WHERE id=$1")
        .bind(installation)
        .bind(release)
        .execute(pool)
        .await?;
    tracing::info!(event="database_ready",job_id=%job,installation_id=%installation);
    Ok(())
}

pub(crate) struct DatabaseResource {
    pub id: Uuid,
    pub database: String,
    pub owner: String,
    pub migration: String,
    pub runtime: String,
    migration_secret: String,
    runtime_secret: String,
}

fn resource(row: sqlx::postgres::PgRow) -> DatabaseResource {
    DatabaseResource {
        id: row.get("id"),
        database: row.get("database_name"),
        owner: row.get("owner_role"),
        migration: row.get("migration_role"),
        runtime: row.get("runtime_role"),
        migration_secret: row.get("migration_secret"),
        runtime_secret: row.get("runtime_secret"),
    }
}

pub(crate) async fn resource_by_id(pool: &sqlx::PgPool, id: Uuid) -> Result<DatabaseResource> {
    Ok(resource(
        sqlx::query("SELECT * FROM db_resources WHERE id=$1")
            .bind(id)
            .fetch_one(pool)
            .await?,
    ))
}

pub(crate) async fn ensure_resource(
    pool: &sqlx::PgPool,
    app: &str,
    environment: &str,
    installation: Option<Uuid>,
) -> Result<DatabaseResource> {
    anyhow::ensure!(matches!(environment, "dev" | "prod"), "invalid_environment");
    let id = installation.unwrap_or_else(Uuid::new_v4);
    let suffix = id.simple().to_string();
    sqlx::query("INSERT INTO db_resources(id,installation_id,app_id,environment,database_name,owner_role,migration_role,runtime_role,migration_secret,runtime_secret,status) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'not_prepared') ON CONFLICT(app_id,environment) DO UPDATE SET installation_id=COALESCE(db_resources.installation_id,excluded.installation_id)")
        .bind(id).bind(installation).bind(app).bind(environment)
        .bind(format!("miy_app_{suffix}")).bind(format!("miy_o_{suffix}"))
        .bind(format!("miy_m_{suffix}")).bind(format!("miy_r_{suffix}"))
        .bind(auth::encrypt(&auth::random())?).bind(auth::encrypt(&auth::random())?)
        .execute(pool).await?;
    let row = sqlx::query("SELECT * FROM db_resources WHERE app_id=$1 AND environment=$2")
        .bind(app)
        .bind(environment)
        .fetch_one(pool)
        .await?;
    if let Some(installation) = installation {
        anyhow::ensure!(
            row.get::<Option<Uuid>, _>("installation_id") == Some(installation),
            "database_installation_mismatch"
        );
    }
    Ok(resource(row))
}

pub(crate) fn desired_fingerprint(migrator: &sqlx::migrate::Migrator) -> Result<String> {
    let history: Vec<_> = migrator
        .iter()
        .map(|m| (m.version, hex::encode(&m.checksum)))
        .collect();
    Ok(contract::hash(serde_json::to_vec(&history)?))
}

/// A build may add migrations, but must preserve every already applied SQL file.
/// This connection is read only: no provisioning, schema changes, or migrations.
pub(crate) async fn verify_development_migrations(
    pool: &sqlx::PgPool,
    app: &str,
    migrations: &std::path::Path,
) -> Result<()> {
    crate::bundle::migrations(migrations)?;
    let desired = sqlx::migrate::Migrator::new(migrations).await?;
    let Some(database) = sqlx::query_scalar::<_, String>(
        "SELECT database_name FROM db_resources WHERE app_id=$1 AND environment='dev'",
    )
    .bind(app)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(());
    };
    let mut address = url::Url::parse(&std::env::var("PROVISION_DATABASE_URL")?)?;
    address.set_path(&database);
    let mut conn = sqlx::PgConnection::connect(address.as_str())
        .await
        .map_err(|_| anyhow::anyhow!("build_database_history_unavailable"))?;
    let mut tx = conn.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let exists: bool = sqlx::query_scalar("SELECT to_regclass('app._sqlx_migrations') IS NOT NULL")
        .fetch_one(&mut *tx)
        .await?;
    if exists {
        let applied = sqlx::query(
            "SELECT version,checksum,success FROM app._sqlx_migrations ORDER BY version",
        )
        .fetch_all(&mut *tx)
        .await?;
        for row in applied {
            let version: i64 = row.get("version");
            let checksum: Vec<u8> = row.get("checksum");
            anyhow::ensure!(row.get::<bool,_>("success") && desired.iter().any(|m| m.version == version && m.checksum.as_ref() == checksum.as_slice()), "migration_history_mismatch");
        }
    }
    tx.commit().await?;
    Ok(())
}

/// A self-consistent lock alone is insufficient: it must name published bytes.
pub(crate) async fn verify_build_components(
    pool: &sqlx::PgPool,
    root: &std::path::Path,
) -> Result<()> {
    let verified = crate::component_cli::verify(root)?;
    for component in verified["components"]
        .as_array()
        .context("missing_component_lock")?
    {
        let expected: Option<String> = sqlx::query_scalar(
            "SELECT digest FROM component_releases WHERE name=$1 AND version=$2",
        )
        .bind(
            component["name"]
                .as_str()
                .context("invalid_component_name")?,
        )
        .bind(
            component["version"]
                .as_str()
                .context("invalid_component_version")?,
        )
        .fetch_optional(pool)
        .await?;
        anyhow::ensure!(
            expected
                .as_deref()
                .is_some_and(|digest| Some(digest) == component["sha256"].as_str()),
            "unpublished_source_component"
        );
    }
    Ok(())
}

async fn record_schema(
    pool: &sqlx::PgPool,
    resource: &DatabaseResource,
    conn: &mut sqlx::PgConnection,
    succeeded: bool,
) -> Result<()> {
    let exists: bool = sqlx::query_scalar("SELECT to_regclass('app._sqlx_migrations') IS NOT NULL")
        .fetch_one(&mut *conn)
        .await?;
    let mut history = Vec::new();
    if exists {
        for row in sqlx::query(
            "SELECT version,checksum FROM app._sqlx_migrations WHERE success ORDER BY version",
        )
        .fetch_all(&mut *conn)
        .await?
        {
            history.push((
                row.get::<i64, _>("version"),
                hex::encode(row.get::<Vec<u8>, _>("checksum")),
            ));
        }
        conn.execute(
            format!(
                "REVOKE ALL ON TABLE app._sqlx_migrations FROM {}",
                resource.runtime
            )
            .as_str(),
        )
        .await?;
    }
    let fingerprint = contract::hash(serde_json::to_vec(&history)?);
    sqlx::query("UPDATE db_resources SET status=$2,schema_generation=schema_generation+CASE WHEN schema_fingerprint IS DISTINCT FROM $3 THEN 1 ELSE 0 END,schema_fingerprint=$3 WHERE id=$1")
        .bind(resource.id).bind(if succeeded {"ready"} else {"failed"}).bind(fingerprint).execute(pool).await?;
    Ok(())
}

pub(crate) async fn prepare_resource(
    pool: &sqlx::PgPool,
    resource: &DatabaseResource,
    migrations: &std::path::Path,
    job: Option<Uuid>,
) -> Result<()> {
    crate::bundle::migrations(migrations)?;
    let db = &resource.database;
    let owner = &resource.owner;
    let migration = &resource.migration;
    let runtime = &resource.runtime;
    let mp = auth::decrypt(&resource.migration_secret)?;
    let rp = auth::decrypt(&resource.runtime_secret)?;
    let provision = std::env::var("PROVISION_DATABASE_URL")?;
    let address = url::Url::parse(&provision)?;
    let host = address
        .host_str()
        .context("database_host_required")?
        .trim_matches(['[', ']']);
    sqlx::query("UPDATE db_resources SET endpoint_host=$2,endpoint_port=$3 WHERE id=$1")
        .bind(resource.id)
        .bind(host)
        .bind(i32::from(address.port().unwrap_or(5432)))
        .execute(pool)
        .await?;
    let mut admin = sqlx::PgConnection::connect(&provision)
        .await
        .map_err(|_| RuntimeFailure::new(FailureCode::DatabaseUnavailable))?;
    sqlx::query("UPDATE db_resources SET status='provisioning' WHERE id=$1")
        .bind(resource.id)
        .execute(pool)
        .await?;
    if let Some(job) = job {
        step(pool, job, "provisioning").await?;
    }
    for (name, password) in [(owner, None), (migration, Some(&mp)), (runtime, Some(&rp))] {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_roles WHERE rolname=$1)")
                .bind(name)
                .fetch_one(&mut admin)
                .await?;
        if !exists {
            let cmd = match password {
                Some(p) => format!(
                    "CREATE ROLE {name} LOGIN PASSWORD '{p}' NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION"
                ),
                None => format!(
                    "CREATE ROLE {name} NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION"
                ),
            };
            admin.execute(cmd.as_str()).await?;
        }
    }
    // Per-role caps keep one app from exhausting the shared cluster (V1-O2). Applied on
    // every preparation so existing installations receive them at their next deployment.
    admin
        .execute(
            format!("ALTER ROLE {migration} CONNECTION LIMIT {MIGRATION_CONNECTION_LIMIT}")
                .as_str(),
        )
        .await?;
    admin
        .execute(
            format!("ALTER ROLE {runtime} CONNECTION LIMIT {RUNTIME_CONNECTION_LIMIT}").as_str(),
        )
        .await?;
    admin
        .execute(format!("GRANT {owner} TO {migration}").as_str())
        .await?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname=$1)")
            .bind(db)
            .fetch_one(&mut admin)
            .await?;
    if !exists {
        admin
            .execute(format!("CREATE DATABASE {db} ALLOW_CONNECTIONS false").as_str())
            .await?;
    }
    admin
        .execute(format!("REVOKE ALL ON DATABASE {db} FROM PUBLIC").as_str())
        .await?;
    admin
        .execute(format!("GRANT CONNECT ON DATABASE {db} TO {migration},{runtime}").as_str())
        .await?;
    admin
        .execute(format!("ALTER DATABASE {db} ALLOW_CONNECTIONS true").as_str())
        .await?;
    if let Some(job) = job {
        step(pool, job, "database_created").await?;
        if std::env::var("MIY_FAILPOINT").as_deref() == Ok("after_database") {
            std::process::exit(86);
        }
    }
    let mut target = url::Url::parse(&provision)?;
    target.set_path(db);
    let mut conn = sqlx::PgConnection::connect(target.as_str()).await?;
    conn.execute("REVOKE ALL ON SCHEMA public FROM PUBLIC")
        .await?;
    conn.execute(format!("CREATE SCHEMA IF NOT EXISTS app AUTHORIZATION {owner}").as_str())
        .await?;
    conn.execute(format!("GRANT USAGE ON SCHEMA app TO {runtime}").as_str())
        .await?;
    conn.execute(format!("ALTER DEFAULT PRIVILEGES FOR ROLE {owner} IN SCHEMA app GRANT SELECT,INSERT,UPDATE,DELETE ON TABLES TO {runtime}").as_str()).await?;
    conn.execute(format!("ALTER DEFAULT PRIVILEGES FOR ROLE {owner} IN SCHEMA app GRANT USAGE,SELECT ON SEQUENCES TO {runtime}").as_str()).await?;
    for role in [migration, runtime] {
        conn.execute(format!("ALTER ROLE {role} IN DATABASE {db} SET search_path=app").as_str())
            .await?;
    }
    if let Some(job) = job {
        step(pool, job, "migrating").await?;
    }
    target
        .set_username(migration)
        .map_err(|_| anyhow::anyhow!("invalid_role"))?;
    target
        .set_password(Some(&mp))
        .map_err(|_| anyhow::anyhow!("invalid_secret"))?;
    let mut conn = sqlx::PgConnection::connect(target.as_str()).await?;
    conn.execute(format!("SET ROLE {owner}").as_str()).await?;
    let migrator = sqlx::migrate::Migrator::new(migrations).await?;
    let applied = migrator.run(&mut conn).await;
    record_schema(pool, resource, &mut conn, applied.is_ok()).await?;
    applied.context("migration_failed")?;
    if let Some(job) = job {
        step(pool, job, "migrations_applied").await?;
        if std::env::var("MIY_FAILPOINT").as_deref() == Ok("after_migration") {
            std::process::exit(86);
        }
    }
    Ok(())
}

#[cfg(test)]
mod scheduling_tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::sync::oneshot;

    struct Dropped(Arc<AtomicUsize>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    async fn pending() -> Result<()> {
        std::future::pending().await
    }

    #[tokio::test]
    async fn blocked_build_does_not_delay_control() {
        let (build_entered, build_started) = oneshot::channel();
        let (control_completed, control_done) = oneshot::channel();
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            supervise(
                async {
                    build_started.await?;
                    control_completed.send(()).unwrap();
                    pending().await
                },
                async {
                    build_entered.send(()).unwrap();
                    pending().await
                },
                pending(),
                async {
                    control_done.await?;
                    Ok(())
                },
            ),
        )
        .await;
        assert!(
            result.is_ok_and(|r| r.is_ok()),
            "control must complete while build remains blocked"
        );
    }

    #[tokio::test]
    async fn lost_ownership_cancels_both_active_lanes() {
        let dropped = Arc::new(AtomicUsize::new(0));
        let control_dropped = Dropped(dropped.clone());
        let build_dropped = Dropped(dropped.clone());
        let (control_entered, control_started) = oneshot::channel();
        let (build_entered, build_started) = oneshot::channel();
        let result = supervise(
            async {
                let _guard = control_dropped;
                control_entered.send(()).unwrap();
                pending().await
            },
            async {
                let _guard = build_dropped;
                build_entered.send(()).unwrap();
                pending().await
            },
            async {
                control_started.await?;
                build_started.await?;
                anyhow::bail!("synthetic_ownership_loss")
            },
            pending(),
        )
        .await;
        assert_eq!(result.unwrap_err().to_string(), "synthetic_ownership_loss");
        assert_eq!(dropped.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn shutdown_cancels_active_lanes_before_returning() {
        let dropped = Arc::new(AtomicUsize::new(0));
        let control_dropped = Dropped(dropped.clone());
        let build_dropped = Dropped(dropped.clone());
        let (control_entered, control_started) = oneshot::channel();
        let (build_entered, build_started) = oneshot::channel();
        supervise(
            async {
                let _guard = control_dropped;
                control_entered.send(()).unwrap();
                pending().await
            },
            async {
                let _guard = build_dropped;
                build_entered.send(()).unwrap();
                pending().await
            },
            pending(),
            async {
                control_started.await?;
                build_started.await?;
                Ok(())
            },
        )
        .await
        .unwrap();
        assert_eq!(dropped.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn failed_lane_stops_worker_and_successful_loop_exit_is_an_error() {
        let result = supervise(
            async { anyhow::bail!("control_failure") },
            pending(),
            pending(),
            pending(),
        )
        .await;
        assert_eq!(result.unwrap_err().to_string(), "control_failure");
        let result = supervise(pending(), async { Ok(()) }, pending(), pending()).await;
        assert_eq!(result.unwrap_err().to_string(), "worker_build_loop_stopped");
    }
}
