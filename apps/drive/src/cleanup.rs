//! Releasing Storage after the app's own commit (AI-DO `remove_storage_objects`
//! with `file_manager_storage_cleanup_jobs` for failures).
//!
//! A permanent delete removes the app rows and records one job per Core file
//! in the same transaction; after the commit the request releases them:
//! `files_detach` for an attached file (Core deletes the bytes and returns the
//! quota), `files_cancel` for one that never got attached. A failed release
//! keeps its job, and later trash or space requests retry due jobs. Bytes are
//! never released before the commit that removed the app's reference.
use miy_app_sdk::{FileStatus, Identity};
use sqlx::{PgPool, Postgres, Transaction};
use std::time::Duration;

use crate::{error::ApiResult, time::now_utc};

const CALL_TIMEOUT: Duration = Duration::from_secs(10);
const DUE_PER_REQUEST: i64 = 20;

/// Record the Core files of `file_ids` (all their versions) for release.
/// Returns the storage file IDs to release after the commit.
pub async fn enqueue_for_files(
    tx: &mut Transaction<'_, Postgres>,
    file_ids: &[String],
) -> ApiResult<Vec<String>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT storage_file_id::text, id FROM file_manager_file_versions \
         WHERE file_id = ANY($1) AND storage_file_id IS NOT NULL",
    )
    .bind(file_ids)
    .fetch_all(&mut **tx)
    .await?;
    enqueue(tx, &rows).await
}

/// Record every Core file of a space for release (space deletion).
pub async fn enqueue_for_space(
    tx: &mut Transaction<'_, Postgres>,
    space_id: &str,
) -> ApiResult<Vec<String>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT v.storage_file_id::text, v.id FROM file_manager_file_versions v \
         JOIN file_manager_files f ON f.id = v.file_id \
         WHERE f.workspace_id = $1 AND v.storage_file_id IS NOT NULL",
    )
    .bind(space_id)
    .fetch_all(&mut **tx)
    .await?;
    enqueue(tx, &rows).await
}

async fn enqueue(
    tx: &mut Transaction<'_, Postgres>,
    rows: &[(String, String)],
) -> ApiResult<Vec<String>> {
    let now = now_utc();
    let mut ids = Vec::new();
    for (storage, version) in rows {
        let Ok(storage_uuid) = uuid::Uuid::parse_str(storage) else {
            continue;
        };
        sqlx::query(
            "INSERT INTO file_manager_storage_cleanup_jobs \
             (storage_file_id, reference, attempts, created_at, next_retry_at) \
             VALUES ($1, $2, 0, $3, $3) ON CONFLICT (storage_file_id) DO NOTHING",
        )
        .bind(storage_uuid)
        .bind(crate::versions::reference(version))
        .bind(now)
        .execute(&mut **tx)
        .await?;
        ids.push(storage.clone());
    }
    Ok(ids)
}

/// Release one Core file. `Ok(())` when it is gone (or was never ready).
async fn release(identity: &Identity, storage: &str, reference: &str) -> Result<(), &'static str> {
    let file = match tokio::time::timeout(CALL_TIMEOUT, identity.files_get(storage)).await {
        Ok(Ok(file)) => file,
        Ok(Err(error)) if error.status == Some(404) => return Ok(()),
        Ok(Err(error)) => return Err(error.code),
        Err(_) => return Err("timeout"),
    };
    let result = match file.status {
        FileStatus::Cancelled | FileStatus::Expired => return Ok(()),
        FileStatus::Attached => {
            let reference = file
                .reference
                .clone()
                .unwrap_or_else(|| reference.to_string());
            tokio::time::timeout(CALL_TIMEOUT, identity.files_detach(storage, &reference)).await
        }
        FileStatus::Pending | FileStatus::Ready => {
            tokio::time::timeout(CALL_TIMEOUT, identity.files_cancel(storage)).await
        }
    };
    match result {
        Ok(Ok(file)) if matches!(file.status, FileStatus::Cancelled | FileStatus::Expired) => {
            Ok(())
        }
        Ok(Ok(_)) => Err("file_not_released"),
        Ok(Err(error)) => Err(error.code),
        Err(_) => Err("timeout"),
    }
}

/// Release the given jobs (just committed) and up to a few older due ones.
pub async fn run(identity: &Identity, pool: &PgPool, just_committed: &[String]) {
    if identity.context.platform {
        return;
    }
    let mut ids: Vec<String> = just_committed.to_vec();
    if let Ok(due) = sqlx::query_scalar::<_, String>(
        "SELECT storage_file_id::text FROM file_manager_storage_cleanup_jobs \
         WHERE next_retry_at <= $1 AND NOT (storage_file_id::text = ANY($2)) \
         ORDER BY next_retry_at LIMIT $3",
    )
    .bind(now_utc())
    .bind(&ids)
    .bind(DUE_PER_REQUEST)
    .fetch_all(pool)
    .await
    {
        ids.extend(due);
    }
    for storage in ids {
        let Ok(storage_uuid) = uuid::Uuid::parse_str(&storage) else {
            continue;
        };
        let Ok(Some(reference)) = sqlx::query_scalar::<_, String>(
            "SELECT reference FROM file_manager_storage_cleanup_jobs WHERE storage_file_id = $1",
        )
        .bind(storage_uuid)
        .fetch_optional(pool)
        .await
        else {
            continue;
        };
        match release(identity, &storage, &reference).await {
            Ok(()) => {
                let _ = sqlx::query(
                    "DELETE FROM file_manager_storage_cleanup_jobs WHERE storage_file_id = $1",
                )
                .bind(storage_uuid)
                .execute(pool)
                .await;
            }
            Err(code) => {
                let _ = sqlx::query(
                    "UPDATE file_manager_storage_cleanup_jobs SET attempts = attempts + 1, \
                     last_error = $2, next_retry_at = $3 + make_interval(mins => LEAST(60, attempts + 1)) \
                     WHERE storage_file_id = $1",
                )
                .bind(storage_uuid)
                .bind(code)
                .bind(now_utc())
                .execute(pool)
                .await;
            }
        }
    }
}
