use crate::{auth, contract};
use anyhow::{Context, Result};
use sqlx::{Connection, Executor, Row};
use uuid::Uuid;

pub async fn run(pool: sqlx::PgPool) -> Result<()> {
    // A dedicated connection owns this session lock. A second runner cannot
    // reclaim another live runner's jobs. On process death PostgreSQL releases it.
    let mut guard = pool.acquire().await?;
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(73490123)")
        .fetch_one(&mut *guard)
        .await?;
    anyhow::ensure!(acquired, "another_db_runner_is_active");
    sqlx::query("UPDATE jobs SET status='queued',updated_at=now() WHERE status='running'")
        .execute(&pool)
        .await?;
    tracing::info!(event = "db_runner_ready");
    crate::runtime::recover(&pool).await?;
    loop {
        let row =
            sqlx::query("SELECT * FROM jobs WHERE status='queued' ORDER BY created_at,id LIMIT 1")
                .fetch_optional(&pool)
                .await?;
        if let Some(row) = row {
            let id: Uuid = row.get("id");
            let installation: Uuid = row.get("installation_id");
            let release: Uuid = row.get("release_id");
            let mut lock = pool.acquire().await?;
            sqlx::query("SELECT pg_advisory_lock(hashtextextended($1,0))")
                .bind(installation.to_string())
                .execute(&mut *lock)
                .await?;
            sqlx::query("UPDATE jobs SET status='running',updated_at=now() WHERE id=$1")
                .bind(id)
                .execute(&pool)
                .await?;
            let result = prepare(&pool, id, installation, release).await;
            match result {
                Ok(()) => {
                    sqlx::query("UPDATE jobs SET status='succeeded',step='ready',error_code=NULL,updated_at=now() WHERE id=$1").bind(id).execute(&pool).await?;
                }
                Err(e) => {
                    let text = format!("{e:#}");
                    let code = if text.contains("checksum") || text.contains("previously applied") {
                        "migration_history_mismatch"
                    } else if text.contains("artifact") {
                        "invalid_artifact"
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
        } else if !crate::runtime::tick(&pool).await? {
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
    let artifact = contract::verified_artifact(&root, row.get("artifact_hash"))?;
    let m: contract::Manifest = serde_json::from_value(row.get("manifest"))?;
    contract::validate(&m).map_err(anyhow::Error::msg)?;
    let migrations = artifact.join(&m.migrations.directory);
    crate::bundle::migrations(&migrations)?;
    let suffix = installation.simple().to_string();
    let db = format!("miy_app_{suffix}");
    let owner = format!("miy_o_{suffix}");
    let migration = format!("miy_m_{suffix}");
    let runtime = format!("miy_r_{suffix}");
    sqlx::query("INSERT INTO db_resources VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING")
        .bind(installation)
        .bind(&db)
        .bind(&owner)
        .bind(&migration)
        .bind(&runtime)
        .bind(auth::encrypt(&auth::random())?)
        .bind(auth::encrypt(&auth::random())?)
        .execute(pool)
        .await?;
    let resource = sqlx::query("SELECT * FROM db_resources WHERE installation_id=$1")
        .bind(installation)
        .fetch_one(pool)
        .await?;
    let mp = auth::decrypt(resource.get("migration_secret"))?;
    let rp = auth::decrypt(resource.get("runtime_secret"))?;
    let provision = std::env::var("PROVISION_DATABASE_URL")?;
    let mut admin = sqlx::PgConnection::connect(&provision).await?;
    step(pool, job, "provisioning").await?;
    sqlx::query("UPDATE installations SET db_status='provisioning' WHERE id=$1")
        .bind(installation)
        .execute(pool)
        .await?;
    for (name, password) in [
        (&owner, None),
        (&migration, Some(&mp)),
        (&runtime, Some(&rp)),
    ] {
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
            .bind(&db)
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
    step(pool, job, "database_created").await?;
    if std::env::var("MIY_FAILPOINT").as_deref() == Ok("after_database") {
        std::process::exit(86);
    }
    let mut admin_url = url::Url::parse(&provision)?;
    admin_url.set_path(&db);
    let mut conn = sqlx::PgConnection::connect(admin_url.as_str()).await?;
    conn.execute("REVOKE ALL ON SCHEMA public FROM PUBLIC")
        .await?;
    conn.execute(format!("CREATE SCHEMA IF NOT EXISTS app AUTHORIZATION {owner}").as_str())
        .await?;
    conn.execute(format!("GRANT USAGE ON SCHEMA app TO {runtime}").as_str())
        .await?;
    conn.execute(format!("ALTER DEFAULT PRIVILEGES FOR ROLE {owner} IN SCHEMA app GRANT SELECT,INSERT,UPDATE,DELETE ON TABLES TO {runtime}").as_str()).await?;
    conn.execute(format!("ALTER DEFAULT PRIVILEGES FOR ROLE {owner} IN SCHEMA app GRANT USAGE,SELECT ON SEQUENCES TO {runtime}").as_str()).await?;
    for role in [&migration, &runtime] {
        conn.execute(format!("ALTER ROLE {role} IN DATABASE {db} SET search_path=app").as_str())
            .await?;
    }
    step(pool, job, "migrating").await?;
    let mut target = admin_url;
    target
        .set_username(&migration)
        .map_err(|_| anyhow::anyhow!("invalid_role"))?;
    target
        .set_password(Some(&mp))
        .map_err(|_| anyhow::anyhow!("invalid_secret"))?;
    let mut conn = sqlx::PgConnection::connect(target.as_str()).await?;
    conn.execute(format!("SET ROLE {owner}").as_str()).await?;
    let migrator = sqlx::migrate::Migrator::new(migrations.as_path()).await?;
    migrator.run(&mut conn).await.context("migration_failed")?;
    conn.execute(format!("REVOKE ALL ON TABLE app._sqlx_migrations FROM {runtime}").as_str())
        .await?;
    step(pool, job, "migrations_applied").await?;
    if std::env::var("MIY_FAILPOINT").as_deref() == Ok("after_migration") {
        std::process::exit(86);
    }
    sqlx::query("UPDATE installations SET db_status='ready',release_id=$2 WHERE id=$1")
        .bind(installation)
        .bind(release)
        .execute(pool)
        .await?;
    tracing::info!(event="database_ready",job_id=%job,installation_id=%installation);
    Ok(())
}
