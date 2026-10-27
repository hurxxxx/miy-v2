use crate::{
    auth, contract,
    runtime_contract::{FailureCode, RuntimeFailure},
};
use anyhow::{Context, Result};
use sqlx::{Connection, Executor, Row};
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
    sqlx::query("UPDATE jobs SET status='queued',updated_at=now() WHERE status='running'")
        .execute(&pool)
        .await?;
    tracing::info!(event = "db_runner_ready");
    crate::runtime::recover(&pool).await?;
    crate::development::recover(&pool).await?;
    sqlx::query("UPDATE image_uploads SET status='pending' WHERE status='loading'")
        .execute(&pool)
        .await?;
    // HTTP observations never occupy the deployment/control queue. The task is
    // aborted before this runner releases its exclusive ownership on shutdown.
    let _log_monitor = crate::runtime_logs::start(pool.clone());
    let _health_monitor = crate::runtime_health::start(pool.clone())?;
    let _development_monitor = crate::development::start(pool.clone());
    let mut last_reconcile = std::time::Instant::now();
    loop {
        if last_reconcile.elapsed().as_secs() >= 5 {
            crate::runtime::reconcile(&pool).await?;
            last_reconcile = std::time::Instant::now();
        }
        let row =
            sqlx::query("SELECT * FROM jobs WHERE status='queued' ORDER BY created_at,id LIMIT 1")
                .fetch_optional(&pool)
                .await?;
        if let Some(row) = row {
            let id: Uuid = row.get("id");
            let installation: Uuid = row.get("installation_id");
            let release: Uuid = row.get("release_id");
            let target = sqlx::query("SELECT app_id,environment FROM installations WHERE id=$1")
                .bind(installation)
                .fetch_one(&pool)
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
                .execute(&pool)
                .await?;
            let result = async {
                crate::auth_sessions::ensure_active(&pool,row.get("actor")).await.map_err(|_| RuntimeFailure::new(FailureCode::UserDisabled))?;
                let authorized:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id=$1 AND admin) OR EXISTS(SELECT 1 FROM installations i JOIN grants g ON g.app_id=i.app_id AND g.environment=i.environment WHERE i.id=$2 AND i.environment='dev' AND g.user_id=$1 AND g.permission='develop')")
                    .bind(row.get::<String,_>("actor")).bind(installation).fetch_one(&pool).await?;
                anyhow::ensure!(authorized, RuntimeFailure::new(FailureCode::PermissionRevoked));
                if target.get::<String,_>("environment")=="dev" && crate::development::active_for_app(&pool,target.get("app_id")).await? {anyhow::bail!(RuntimeFailure::new(FailureCode::ActiveDevelopmentSession));}
                prepare(&pool, id, installation, release).await
            }.await;
            match result {
                Ok(()) => {
                    sqlx::query("UPDATE jobs SET status='succeeded',step='ready',error_code=NULL,updated_at=now() WHERE id=$1").bind(id).execute(&pool).await?;
                }
                Err(e) => {
                    let code = if let Some(failure) = e.downcast_ref::<RuntimeFailure>() {
                        failure.code()
                    } else if matches!(
                        e.downcast_ref::<sqlx::migrate::MigrateError>(),
                        Some(
                            sqlx::migrate::MigrateError::VersionMissing(_)
                                | sqlx::migrate::MigrateError::VersionMismatch(_)
                        )
                    ) {
                        "migration_history_mismatch"
                    } else {
                        "database_prepare_failed"
                    };
                    sqlx::query("UPDATE jobs SET status='failed',error_code=$2,updated_at=now() WHERE id=$1").bind(id).bind(code).execute(&pool).await?;
                    sqlx::query("UPDATE installations SET db_status='failed' WHERE id=$1")
                        .bind(installation)
                        .execute(&pool)
                        .await?;
                    tracing::error!(event="database_job_failed",job_id=%id,code);
                }
            }
            sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1,0))")
                .bind(installation.to_string())
                .execute(&mut *lock)
                .await?;
        } else if !crate::development::tick(&pool).await?
            && !crate::delivery::tick(&pool).await?
            && !crate::runtime::tick(&pool).await?
        {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }
        // The connection must remain live for the entire runner lifetime.
        sqlx::query("SELECT 1").execute(&mut *guard).await?;
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
