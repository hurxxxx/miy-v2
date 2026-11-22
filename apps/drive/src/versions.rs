//! File versions: each uploaded content of a file, pointing at its Core file
//! (COMMON-FEATURES-PLAN §2.3: the drive owns versions as rows that point at
//! new file UUIDs). AI-DO kept one storage object per file; a new version is
//! a miy addition (보강).
use chrono::NaiveDateTime;
use miy_app_sdk::{FileStatus, Identity};
use sqlx::{FromRow, PgExecutor, PgPool};

use crate::error::ApiResult;

#[derive(Clone, Debug, FromRow)]
pub struct Version {
    pub id: String,
    pub file_id: String,
    pub version_number: i32,
    pub request_id: String,
    pub attach_request_id: String,
    pub storage_file_id: Option<String>,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub transfer: String,
    pub status: String,
    pub attached: bool,
    pub sha256: Option<String>,
    pub uploaded_by: String,
    pub created_at: NaiveDateTime,
    pub completed_at: Option<NaiveDateTime>,
}

pub const VERSION_COLUMNS: &str = "id, file_id, version_number, request_id::text AS request_id, \
     attach_request_id::text AS attach_request_id, storage_file_id::text AS storage_file_id, \
     filename, content_type, size_bytes, transfer, status, attached, sha256, uploaded_by, \
     created_at, completed_at";

/// The Core attach reference of a version: `drive:version:<id>`.
pub fn reference(version_id: &str) -> String {
    format!("drive:version:{version_id}")
}

pub async fn load<'e, E: PgExecutor<'e>>(
    executor: E,
    version_id: &str,
) -> ApiResult<Option<Version>> {
    let sql = format!("SELECT {VERSION_COLUMNS} FROM file_manager_file_versions WHERE id = $1");
    Ok(sqlx::query_as::<_, Version>(&sql)
        .bind(version_id)
        .fetch_optional(executor)
        .await?)
}

pub async fn by_storage_file<'e, E: PgExecutor<'e>>(
    executor: E,
    storage_file_id: &str,
) -> ApiResult<Option<Version>> {
    let Ok(id) = uuid::Uuid::parse_str(storage_file_id) else {
        return Ok(None);
    };
    let sql = format!(
        "SELECT {VERSION_COLUMNS} FROM file_manager_file_versions WHERE storage_file_id = $1"
    );
    Ok(sqlx::query_as::<_, Version>(&sql)
        .bind(id)
        .fetch_optional(executor)
        .await?)
}

pub async fn ready_versions(pool: &PgPool, file_id: &str) -> ApiResult<Vec<Version>> {
    let sql = format!(
        "SELECT {VERSION_COLUMNS} FROM file_manager_file_versions WHERE file_id = $1 \
         AND status = 'ready' ORDER BY version_number DESC"
    );
    Ok(sqlx::query_as::<_, Version>(&sql)
        .bind(file_id)
        .fetch_all(pool)
        .await?)
}

/// Attach a ready version after the app commit that made it current (the
/// contract's order). The same attach request ID makes a repeat safe; an
/// uncertain answer is looked up. Returns the reference to use for a download
/// link, or `None` while the file is still unattached.
pub async fn ensure_attached(
    identity: &Identity,
    pool: &PgPool,
    version: &Version,
) -> Option<String> {
    let reference = reference(&version.id);
    if version.attached {
        return Some(reference);
    }
    let storage = version.storage_file_id.as_deref()?;
    if version.status != "ready" {
        return None;
    }
    let attached = match identity
        .files_attach(storage, &reference, &version.attach_request_id)
        .await
    {
        Ok(file) => file.status == FileStatus::Attached,
        Err(_) => identity.files_get(storage).await.is_ok_and(|file| {
            file.status == FileStatus::Attached && file.reference.as_deref() == Some(&reference)
        }),
    };
    if !attached {
        return None;
    }
    let _ = sqlx::query("UPDATE file_manager_file_versions SET attached = true WHERE id = $1")
        .bind(&version.id)
        .execute(pool)
        .await;
    Some(reference)
}
