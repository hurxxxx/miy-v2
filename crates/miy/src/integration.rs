//! Destructive scenarios are confined to uniquely named synthetic installations.
//! Run explicitly with the local services up and the ordinary DB runner stopped.
use crate::{State, auth, contract, domain};
use anyhow::Result;
use serde_json::{Value, json};
use sqlx::{Connection, Row};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use uuid::Uuid;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned()
}
fn copy(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        if e.file_type()?.is_dir() {
            copy(&e.path(), &to.join(e.file_name()))?;
        } else {
            std::fs::copy(e.path(), to.join(e.file_name()))?;
        }
    }
    Ok(())
}
async fn release(
    state: &State,
    p: &auth::Principal,
    app: &str,
    version: &str,
    sql1: &str,
    sql2: Option<&str>,
    sql3: Option<&str>,
) -> Result<Uuid> {
    let temp = tempfile::tempdir()?;
    copy(
        &root().join("web/fixtures/dist/reference"),
        &temp.path().join("ui"),
    )?;
    std::fs::create_dir(temp.path().join("migrations"))?;
    std::fs::write(temp.path().join("migrations/0001_assets.sql"), sql1)?;
    if let Some(sql) = sql2 {
        std::fs::write(temp.path().join("migrations/0002_inspection.sql"), sql)?;
    }
    if let Some(sql) = sql3 {
        std::fs::write(temp.path().join("migrations/0003_failure.sql"), sql)?;
    }
    let m = json!({"contractVersion":contract::VERSION,"id":app,"name":"DB check","version":version,"runtime":{"profile":"rust-web-v1"},"ui":{"delivery":"module","moduleEntry":"mf-manifest.json","moduleExport":"./AppRoot","contractVersion":"miy.ui/v1alpha1"},"resources":{"database":"postgresql"},"migrations":{"directory":"migrations"},"requestedPermissions":["database"]});
    std::fs::write(temp.path().join("manifest.json"), serde_json::to_vec(&m)?)?;
    let hash = contract::artifact_hash(temp.path())?;
    copy(temp.path(), &state.artifacts.join(&hash))?;
    let v = domain::create_release(
        state,
        p,
        app,
        domain::NewRelease {
            artifact_hash: hash,
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.code))?;
    Ok(serde_json::from_value(v["id"].clone())?)
}
fn runner(failpoint: Option<&str>) -> Result<tokio::process::Child> {
    let mut c = tokio::process::Command::new(root().join("target/debug/miy"));
    c.arg("worker")
        .kill_on_drop(true)
        .env_remove("MIY_FAILPOINT");
    if let Some(p) = failpoint {
        c.env("MIY_FAILPOINT", p);
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root().join(".local/evidence/worker-integration.log"))?;
    c.stdout(file.try_clone()?).stderr(file);
    Ok(c.spawn()?)
}
async fn enqueue(
    state: &State,
    p: &auth::Principal,
    installation: Uuid,
    release: Uuid,
    key: &str,
) -> Result<Uuid> {
    let v = domain::prepare(
        state,
        p,
        domain::Prepare {
            installation_id: installation,
            release_id: release,
            idempotency_key: key.into(),
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.code))?;
    Ok(serde_json::from_value(v["id"].clone())?)
}
async fn wait(pool: &sqlx::PgPool, id: Uuid, status: &str) -> Result<()> {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let row = sqlx::query("SELECT status,error_code FROM jobs WHERE id=$1")
                .bind(id)
                .fetch_one(pool)
                .await?;
            let current: String = row.get("status");
            if current == status {
                return Ok(());
            }
            anyhow::ensure!(
                current != "failed" || status == "failed",
                "job failed: {:?}",
                row.get::<Option<String>, _>("error_code")
            );
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
    })
    .await?
}
#[tokio::test]
#[ignore = "requires real local services and no other DB runner; see scripts/check.sh"]
async fn real_database_isolation_migrations_and_crash_recovery() -> Result<()> {
    let pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL")?).await?;
    let state = State {
        pool: pool.clone(),
        auth: Arc::new(auth::Auth::new().await?),
        artifacts: std::env::var("MIY_ARTIFACTS")?.into(),
    };
    let actor = auth::Principal {
        id: "00000000-0000-4000-8000-000000000001".into(),
        scopes: vec!["miy.dev".into()],
        csrf: None,
    };
    let app = format!("dbcheck-{}", &Uuid::new_v4().simple().to_string()[..12]);
    domain::create_app(
        &state,
        &actor,
        domain::NewApp {
            id: app.clone(),
            name: "DB check".into(),
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.code))?;
    let sql1 = "CREATE TABLE assets(id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,name text NOT NULL);";
    let v1 = release(&state, &actor, &app, "1.0.0", sql1, None, None).await?;
    let i = domain::create_installation(
        &state,
        &actor,
        domain::NewInstallation {
            app_id: app.clone(),
            environment: "dev".into(),
            release_id: v1,
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.code))?;
    let id: Uuid = serde_json::from_value(i["id"].clone())?;
    let job = enqueue(&state, &actor, id, v1, &format!("initial-{id}")).await?;
    let mut crashing = runner(Some("after_database"))?;
    let status =
        tokio::time::timeout(std::time::Duration::from_secs(30), crashing.wait()).await??;
    assert_eq!(status.code(), Some(86));
    let before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM db_resources WHERE installation_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(before, 1);
    let mut process = runner(None)?;
    wait(&pool, job, "succeeded").await?;
    let resource = sqlx::query("SELECT * FROM db_resources WHERE installation_id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await?;
    let mut url = url::Url::parse(&std::env::var("PROVISION_DATABASE_URL")?)?;
    url.set_path(resource.get("database_name"));
    url.set_username(resource.get("runtime_role")).unwrap();
    url.set_password(Some(&auth::decrypt(resource.get("runtime_secret"))?))
        .unwrap();
    let mut runtime = sqlx::PgConnection::connect(url.as_str()).await?;
    sqlx::query("INSERT INTO assets(name) VALUES('retained')")
        .execute(&mut runtime)
        .await?;
    assert!(
        sqlx::query("CREATE TABLE forbidden(id int)")
            .execute(&mut runtime)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("CREATE TEMP TABLE forbidden(id int)")
            .execute(&mut runtime)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM app._sqlx_migrations")
            .execute(&mut runtime)
            .await
            .is_err()
    );
    for db in ["miy_core", "miy_identity", "postgres"] {
        let mut foreign = url.clone();
        foreign.set_path(db);
        assert!(
            sqlx::PgConnection::connect(foreign.as_str()).await.is_err(),
            "unexpected access to {db}"
        );
    }
    let i2 = domain::create_installation(
        &state,
        &actor,
        domain::NewInstallation {
            app_id: app.clone(),
            environment: "prod".into(),
            release_id: v1,
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.code))?;
    let id2: Uuid = serde_json::from_value(i2["id"].clone())?;
    let job2 = enqueue(&state, &actor, id2, v1, &format!("prod-{id}")).await?;
    wait(&pool, job2, "succeeded").await?;
    let db2: String =
        sqlx::query_scalar("SELECT database_name FROM db_resources WHERE installation_id=$1")
            .bind(id2)
            .fetch_one(&pool)
            .await?;
    let mut foreign = url.clone();
    foreign.set_path(&db2);
    assert!(sqlx::PgConnection::connect(foreign.as_str()).await.is_err());
    process.kill().await?;
    let _ = process.wait().await;
    let sql2 = "ALTER TABLE assets ADD COLUMN inspection_due date; CREATE TABLE notes(id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,body text NOT NULL);";
    let v2 = release(&state, &actor, &app, "1.1.0", sql1, Some(sql2), None).await?;
    let job3 = enqueue(&state, &actor, id, v2, &format!("upgrade-{id}")).await?;
    let mut crashing = runner(Some("after_migration"))?;
    let status =
        tokio::time::timeout(std::time::Duration::from_secs(30), crashing.wait()).await??;
    assert_eq!(status.code(), Some(86));
    let mut process = runner(None)?;
    wait(&pool, job3, "succeeded").await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM assets WHERE name='retained'")
            .fetch_one(&mut runtime)
            .await?,
        1
    );
    sqlx::query("INSERT INTO notes(body) VALUES('new table permissions')")
        .execute(&mut runtime)
        .await?;
    assert!(
        sqlx::query("ALTER TABLE notes ADD COLUMN denied int")
            .execute(&mut runtime)
            .await
            .is_err()
    );
    let bad = release(
        &state,
        &actor,
        &app,
        "2.0.0",
        sql1,
        Some(sql2),
        Some("ALTER TABLE assets ADD COLUMN never_committed int; SELECT 1/0;"),
    )
    .await?;
    let fail = enqueue(&state, &actor, id, bad, &format!("sql-fail-{id}")).await?;
    wait(&pool, fail, "failed").await?;
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM information_schema.columns WHERE table_schema='app' AND table_name='assets' AND column_name='never_committed'").fetch_one(&mut runtime).await?,0);
    // This release changed an applied migration: it must fail before executing SQL.
    let modified = release(
        &state,
        &actor,
        &app,
        "3.0.0",
        "CREATE TABLE changed(id int);",
        None,
        None,
    )
    .await?;
    let fail2 = enqueue(&state, &actor, id, modified, &format!("checksum-{id}")).await?;
    wait(&pool, fail2, "failed").await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM assets WHERE name='retained'")
            .fetch_one(&mut runtime)
            .await?,
        1
    );
    let code: Option<String> = sqlx::query_scalar("SELECT error_code FROM jobs WHERE id=$1")
        .bind(fail2)
        .fetch_one(&pool)
        .await?;
    assert_eq!(code.as_deref(), Some("migration_history_mismatch"));
    let mut admin_url = url::Url::parse(&std::env::var("PROVISION_DATABASE_URL")?)?;
    admin_url.set_path(resource.get("database_name"));
    let mut history_admin = sqlx::PgConnection::connect(admin_url.as_str()).await?;
    sqlx::query("INSERT INTO app._sqlx_migrations(version,description,success,checksum,execution_time) VALUES(9999,'unknown',true,decode('00','hex'),0)")
        .execute(&mut history_admin).await?;
    let unknown = enqueue(&state, &actor, id, v2, &format!("unknown-history-{id}")).await?;
    wait(&pool, unknown, "failed").await?;
    let duplicate = enqueue(&state, &actor, id, v2, &format!("upgrade-{id}")).await?;
    assert_eq!(duplicate, job3);
    let mismatch = domain::prepare(
        &state,
        &actor,
        domain::Prepare {
            installation_id: id,
            release_id: v1,
            idempotency_key: format!("upgrade-{id}"),
        },
    )
    .await;
    assert_eq!(mismatch.unwrap_err().code, "idempotency_conflict");
    process.kill().await?;
    let _ = process.wait().await;
    let report: Value = json!({"installationId":id,"otherEnvironmentId":id2,"passed":["runtime-crud","runtime-ddl-denied","runtime-temp-denied","migration-history-protected","core-identity-db-denied","other-environment-denied","crash-after-create-recovered","crash-after-migration-recovered","data-preserved","new-table-default-grants","transactional-failure-rolled-back","checksum-rejected","unknown-history-rejected","idempotency-conflict"]});
    std::fs::write(
        root().join(".local/evidence/database.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}
