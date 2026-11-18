//! A runtime job owns the attempted release. The installation publishes the
//! active release and image together only after that exact container is healthy.
use super::*;
use crate::runtime_contract::{DbStatus, DesiredState, RuntimeStatus};

pub(super) async fn activate(
    pool: &sqlx::PgPool,
    installation: Uuid,
    release: Uuid,
    image: &str,
    local_image: &str,
    port: i32,
    guard_enabled: bool,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        "UPDATE installations i SET release_id=$2,runtime_image=$3,runtime_local_image=$4,
             runtime_port=$5,runtime_status='running',desired_state='running'
         WHERE id=$1 AND (NOT $6 OR NOT EXISTS(
             SELECT 1 FROM app_controls c WHERE c.app_id=i.app_id AND c.disabled))",
    )
    .bind(installation)
    .bind(release)
    .bind(image)
    .bind(local_image)
    .bind(port)
    .bind(guard_enabled)
    .execute(&mut *tx)
    .await?;
    anyhow::ensure!(
        result.rows_affected() == 1,
        RuntimeFailure::new(FailureCode::StateChanged)
    );
    runtime_health::record_and_commit(tx, installation, Observation::Healthy).await
}

pub(super) async fn finish_failed_deployment(
    pool: &sqlx::PgPool,
    inst: &sqlx::postgres::PgRow,
    attempted: Option<Uuid>,
    expected: Option<&ExpectedState>,
    allow_restore: bool,
) -> Result<()> {
    let installation: Uuid = inst.get("id");
    let active_release: Uuid = inst.get("release_id");
    let name = format!("miy-app-{}", installation.simple());
    let previous = format!("{name}-previous");
    let replacing =
        attempted != Some(active_release) && inst.get::<Option<&str>, _>("runtime_image").is_some();
    let state = if replacing || inst.get::<&str, _>("runtime_status") == "recovery_required" {
        "recovery_required"
    } else {
        "failed"
    };
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE installations SET runtime_status=$2,desired_state='stopped',runtime_port=NULL WHERE id=$1")
        .bind(installation).bind(state).execute(&mut *tx).await?;
    runtime_health::record_and_commit(tx, installation, Observation::Inactive).await?;
    // Re-read the DB fingerprint written by successful preparation. Its identity,
    // generation and complete history must match the reviewed healthy predecessor.
    if replacing && let Some(expected) = expected {
        match restore_previous(pool, inst, expected, &name, &previous, allow_restore).await {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(error) if crate::db_outcome::uncertain(&error) => return Err(error),
            Err(_) => {
                tracing::warn!(event="previous_release_restore_failed",installation_id=%installation)
            }
        }
    }
    if let Some(meta) = removable_container(&name, installation).await? {
        let id = meta["Id"].as_str().context("missing_container_id")?;
        docker(&["stop", "--time", "5", id]).await?;
    }
    remove_owned_container(&previous, installation).await?;
    Ok(())
}

async fn restore_previous(
    pool: &sqlx::PgPool,
    inst: &sqlx::postgres::PgRow,
    expected: &ExpectedState,
    name: &str,
    previous: &str,
    may_start: bool,
) -> Result<bool> {
    let (Some(before), Some(database)) = (&expected.installation, &expected.database) else {
        return Ok(false);
    };
    let id = inst.get::<Uuid, _>("id");
    let active_release = inst.get::<Uuid, _>("release_id");
    if before.installation_id != id
        || before.release_id != active_release
        || before.desired_state != DesiredState::Running
        || before.runtime_status != RuntimeStatus::Running
        || before.db_status != DbStatus::Ready
        || database.status != DbStatus::Ready
        || database.schema_fingerprint.is_none()
        || before.image_id.as_deref() != inst.get("runtime_image")
    {
        return Ok(false);
    }
    let current =
        crate::runtime_preflight::snapshot(pool, inst.get("app_id"), inst.get("environment"))
            .await
            .map_err(|_| anyhow::anyhow!("previous_schema_unverified"))?;
    let guard_enabled = expected.official_app_disabled == Some(false);
    if current.database.as_ref() != Some(database)
        || current
            .installation
            .as_ref()
            .is_none_or(|i| i.db_status != DbStatus::Ready)
    {
        return Ok(false);
    }
    // Leave an untouched, already-running predecessor serving if the company
    // switched the app off before Docker stopped it. Never start one for that job.
    let may_start = may_start && !(guard_enabled && current.official_app_disabled == Some(true));
    let saved = removable_container(previous, id).await?;
    if let Some(saved) = saved {
        if !may_start {
            return Ok(false);
        }
        anyhow::ensure!(
            container_matches(&saved, inst),
            "previous_container_mismatch"
        );
        remove_owned_container(name, id).await?;
        docker(&[
            "rename",
            saved["Id"].as_str().context("missing_container_id")?,
            name,
        ])
        .await?;
    }
    let meta = owned(name, id).await?;
    anyhow::ensure!(
        container_matches(&meta, inst),
        "previous_container_mismatch"
    );
    let container = meta["Id"].as_str().context("missing_container_id")?;
    if meta["State"]["Running"] != true {
        if !may_start {
            return Ok(false);
        }
        check_company_switch(pool, inst.get("app_id"), guard_enabled).await?;
        docker(&["start", container]).await?;
    }
    let meta = owned(container, id).await?;
    let port = published_port(&meta)?;
    healthy(port, active_release).await?;
    let image: &str = inst.get("runtime_image");
    let local_image: &str = inst
        .get::<Option<&str>, _>("runtime_local_image")
        .unwrap_or(image);
    activate(
        pool,
        id,
        active_release,
        image,
        local_image,
        port,
        may_start && guard_enabled,
    )
    .await?;
    Ok(true)
}
