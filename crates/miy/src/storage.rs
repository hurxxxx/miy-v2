//! Local small-attachment bytes. Only an authenticated owning app may use this
//! API after checking its own current ACL. No browser or ordinary CLI bypass.
//!
//! Connection discipline: a handler never takes a second pooled connection while
//! it holds the reservation lock or a file row lock. The one exception is the
//! authorization recheck after an upload transfer, bounded by the transfer slots.
//! Disk usage is measured before a transaction, never under a lock.
use crate::{
    State,
    api_error::{ApiError, ApiResult},
    auth::Principal,
    policy::{AppAction, Write},
    storage_contract::{
        AttachFile, DEFAULT_INSTALLATION_BYTES, DEFAULT_INSTALLATION_FILES, DetachFile, FileStatus,
        FileTransfer, MAX_LARGE_FILE_BYTES, MAX_PART_BYTES, ReserveFile, StoredFile,
    },
};
use axum::{
    Extension, Json,
    body::{Body, Bytes},
    extract::{Path, Request, State as S},
    http::{HeaderMap, Method, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row, postgres::PgRow};
use std::{
    path::{Path as FsPath, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    sync::{Semaphore, SemaphorePermit},
};
use uuid::Uuid;

pub use crate::storage_contract::{MAX_FILE_BYTES, TRANSFER_SECONDS, allowed_route};
/// Free space that stays after every promised byte: the larger of this and 5%
/// of the file system (the store has no fixed total, docs/CAPACITY.md).
const MIN_FREE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MIN_FREE_INODES: u64 = 1024;
static TRANSFERS: Semaphore = Semaphore::const_new(4);
const SWEEP_SECONDS: u64 = 60;
const SWEEP_BATCH: i64 = 100;

async fn access(
    s: &State,
    p: &Principal,
    id: Uuid,
    extensions: &axum::http::Extensions,
    write: bool,
) -> ApiResult<PgRow> {
    let permission = if write {
        "platform:files:write"
    } else {
        "platform:files:read"
    };
    let installation = crate::domain::installation(s, p, id).await?;
    if let Some(preview) = extensions.get::<PreviewStorage>() {
        if p.browser_purpose.is_some()
            || preview.installation != id
            || installation.get::<&str, _>("environment") != "dev"
            || installation.get::<Uuid, _>("release_id") != preview.release
        {
            return Err(ApiError::forbidden());
        }
        crate::connections::require_preview(s, p, &preview.app).await?;
    } else {
        let context = extensions
            .get::<crate::app_calls::Context>()
            .ok_or_else(ApiError::forbidden)?;
        crate::app_calls::require(s, p, context, id, permission).await?;
    }
    crate::permissions::require(&s.pool, id, installation.get("release_id"), permission).await?;
    crate::delegations::check(
        s,
        p,
        installation.get("app_id"),
        installation.get("environment"),
        if write {
            crate::delegations::Action::AppWrite
        } else {
            crate::delegations::Action::AppRead
        },
    )
    .await?;
    Ok(installation)
}

fn file_row(row: &PgRow) -> ApiResult<StoredFile> {
    let expires_at = row.get::<Option<DateTime<Utc>>, _>("expires_at");
    let status = match row.get::<&str, _>("status") {
        // An elapsed reservation is unusable and uncounted at once; the sweeper
        // removes its bytes and records `expired` later.
        "pending" | "uploading" if expires_at.is_some_and(|at| at <= Utc::now()) => {
            FileStatus::Expired
        }
        "pending" | "uploading" => FileStatus::Pending,
        "ready" => FileStatus::Ready,
        "attached" => FileStatus::Attached,
        "cancelling" | "cancelled" => FileStatus::Cancelled,
        "expired" => FileStatus::Expired,
        _ => return Err(ApiError::unavailable("file_metadata_invalid")),
    };
    let size = row.get::<i64, _>("size") as u64;
    let transfer = match row.get::<&str, _>("transfer") {
        "parts" => FileTransfer::Parts,
        _ => FileTransfer::Single,
    };
    let received = match status {
        FileStatus::Ready | FileStatus::Attached => size,
        _ => row.get::<i64, _>("received") as u64,
    };
    Ok(StoredFile {
        id: row.get("id"),
        version: 1,
        installation_id: row.get("installation_id"),
        request_id: row.get("request_id"),
        name: row.get("name"),
        mime: row.get("mime"),
        size,
        sha256: row.get("sha256"),
        status,
        reference: row.get("reference"),
        transfer,
        received,
        expires_at,
    })
}

fn validate_reservation(value: &mut ReserveFile) -> ApiResult<()> {
    value.mime.make_ascii_lowercase();
    let limit = match value.transfer {
        FileTransfer::Single => MAX_FILE_BYTES,
        FileTransfer::Parts => MAX_LARGE_FILE_BYTES,
    };
    if value.request_id.is_nil()
        || value.size > limit
        || (value.transfer == FileTransfer::Parts && value.size == 0)
        || value.name.trim().is_empty()
        || value.name.chars().count() > 255
        || value
            .name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\'))
        || value.mime.len() > 128
        || value.mime.matches('/').count() != 1
        || value.mime.split('/').any(str::is_empty)
        || !value
            .mime
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/!#$&^_.+-".contains(&b))
    {
        return Err(ApiError::bad("invalid_file_reservation"));
    }
    Ok(())
}

fn valid_reference(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= 512 && !value.chars().any(char::is_control)
}

async fn root(artifacts: &FsPath, installation: Uuid) -> ApiResult<PathBuf> {
    let base = artifacts.join(".files");
    let directory = base.join(installation.to_string());
    for path in [artifacts, base.as_path(), directory.as_path()] {
        match tokio::fs::symlink_metadata(path).await {
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
            Ok(_) => return Err(ApiError::unavailable("file_storage_path_invalid")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match tokio::fs::create_dir(path).await {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        let meta = tokio::fs::symlink_metadata(path)
                            .await
                            .map_err(|_| ApiError::unavailable("file_storage_unavailable"))?;
                        if !meta.is_dir() || meta.file_type().is_symlink() {
                            return Err(ApiError::unavailable("file_storage_path_invalid"));
                        }
                    }
                    Err(_) => return Err(ApiError::unavailable("file_storage_unavailable")),
                }
            }
            Err(_) => return Err(ApiError::unavailable("file_storage_unavailable")),
        }
    }
    Ok(directory)
}

async fn remove(path: &FsPath) -> ApiResult<()> {
    match tokio::fs::symlink_metadata(path).await {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => {
            tokio::fs::remove_file(path)
                .await
                .map_err(|_| ApiError::unavailable("file_cleanup_failed"))?;
        }
        Ok(_) => return Err(ApiError::unavailable("file_storage_path_invalid")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(ApiError::unavailable("file_cleanup_failed")),
    }
    Ok(())
}

async fn remove_bytes(artifacts: &FsPath, installation: Uuid, file: Uuid) -> ApiResult<()> {
    let directory = root(artifacts, installation).await?;
    remove(&directory.join(format!("{file}.part"))).await?;
    remove(&directory.join(format!("{file}.blob"))).await
}

fn df_available(output: &[u8]) -> Option<u64> {
    let text = std::str::from_utf8(output).ok()?;
    let mut fields = text.lines().nth(1)?.split_whitespace();
    let value = fields.next()?.parse().ok()?;
    fields.next().is_none().then_some(value)
}

async fn df(path: &FsPath, args: &[&str]) -> ApiResult<u64> {
    let unknown = || ApiError::unavailable("file_storage_usage_unknown");
    let output = tokio::time::timeout(
        Duration::from_secs(3),
        tokio::process::Command::new("df")
            .args(args)
            .arg(path)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| unknown())?
    .map_err(|_| unknown())?;
    if !output.status.success() {
        return Err(unknown());
    }
    df_available(&output.stdout).ok_or_else(unknown)
}

/// Free space of the storage file system. Measured before a transaction: a
/// reservation changes what is promised, not what the disk holds, so a value
/// read just before the lock is as current as one read under it.
struct DiskSpace {
    bytes: u64,
    total: u64,
    inodes: u64,
}

impl DiskSpace {
    async fn measure(path: &FsPath) -> ApiResult<Self> {
        Ok(Self {
            bytes: df(path, &["--output=avail", "-B1"]).await?,
            total: df(path, &["--output=size", "-B1"]).await?,
            inodes: df(path, &["--output=iavail"]).await?,
        })
    }

    /// The floor must remain after `bytes` in `files` promised files arrive.
    fn require(&self, bytes: u64, files: u64) -> ApiResult<()> {
        let floor = MIN_FREE_BYTES.max(self.total / 20);
        if self.bytes < floor.saturating_add(bytes)
            || self.inodes < MIN_FREE_INODES.saturating_add(files)
        {
            return Err(ApiError::unavailable("file_storage_low"));
        }
        Ok(())
    }
}

async fn audit(tx: &mut Write, p: &Principal, action: &str, file: Uuid) -> ApiResult<()> {
    crate::audit::record(tx, &p.id, action, &format!("file:{file}")).await
}

async fn commit(tx: Write, p: &Principal, installation: &PgRow) -> ApiResult<()> {
    tx.decide(
        p,
        installation.get("app_id"),
        installation.get("environment"),
        AppAction::Use,
    )
    .await?
    .commit()
    .await
}

/// Lock one file row for a state change. A live upload holds this lock through
/// its bounded transfer. Wait briefly, so an aborted upload can release it, then
/// report the upload instead of pinning a pooled connection behind it.
async fn lock_file(tx: &mut Write, installation: Uuid, file: Uuid) -> ApiResult<PgRow> {
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut **tx)
        .await?;
    let row =
        match sqlx::query("SELECT * FROM app_files WHERE installation_id=$1 AND id=$2 FOR UPDATE")
            .bind(installation)
            .bind(file)
            .fetch_optional(&mut **tx)
            .await
        {
            Ok(row) => row,
            Err(error)
                if error.as_database_error().and_then(|e| e.code()).as_deref() == Some("55P03") =>
            {
                return Err(ApiError::conflict("file_upload_in_progress"));
            }
            Err(error) => return Err(error.into()),
        };
    sqlx::query("SET LOCAL lock_timeout=0")
        .execute(&mut **tx)
        .await?;
    row.ok_or_else(ApiError::not_found)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Sweep {
    pub released: u64,
    pub failed: u64,
}

/// Housekeeping without a request principal: reclaim elapsed incomplete uploads
/// and finish cancellations whose byte removal failed. Ready bytes may be
/// referenced by an app DB, so they never expire implicitly.
///
/// A file whose bytes cannot be removed keeps its state and is counted in
/// `failed`; the scan continues past it, so one bad entry delays neither the
/// other files nor any reservation. Rows locked by a live upload are skipped.
pub(crate) async fn sweep(pool: &PgPool, artifacts: &FsPath) -> ApiResult<Sweep> {
    let mut report = Sweep::default();
    let mut after: Option<Uuid> = None;
    loop {
        let mut tx = pool.begin().await?;
        let rows = sqlx::query("SELECT id,installation_id,status FROM app_files WHERE ($1::uuid IS NULL OR id>$1) AND ((status IN ('pending','uploading') AND expires_at<=clock_timestamp()) OR status='cancelling') ORDER BY id LIMIT $2 FOR UPDATE SKIP LOCKED")
            .bind(after).bind(SWEEP_BATCH).fetch_all(&mut *tx).await?;
        let Some(last) = rows.last() else {
            return Ok(report);
        };
        after = Some(last.get("id"));
        for row in &rows {
            let file: Uuid = row.get("id");
            if remove_bytes(artifacts, row.get("installation_id"), file)
                .await
                .is_err()
            {
                report.failed += 1;
                tracing::warn!(event = "file_cleanup_failed", file = %file);
                continue;
            }
            let status = if row.get::<&str, _>("status") == "cancelling" {
                "cancelled"
            } else {
                "expired"
            };
            sqlx::query("UPDATE app_files SET status=$2,expires_at=NULL,updated_at=clock_timestamp() WHERE id=$1")
                .bind(file).bind(status).execute(&mut *tx).await?;
            // No actor: the platform did this, not whoever made the next request.
            sqlx::query("INSERT INTO audit(action,target,request_id) VALUES($1,$2,$3)")
                .bind(format!("file.{status}"))
                .bind(format!("file:{file}"))
                .bind(Uuid::new_v4().to_string())
                .execute(&mut *tx)
                .await?;
            report.released += 1;
        }
        tx.commit().await?;
    }
}

pub(crate) struct Sweeper(tokio::task::JoinHandle<()>);
impl Drop for Sweeper {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The worker runs the sweep, so a reservation never waits for housekeeping.
pub(crate) fn start(pool: PgPool) -> anyhow::Result<Sweeper> {
    let artifacts = PathBuf::from(std::env::var("MIY_ARTIFACTS")?);
    Ok(Sweeper(tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(SWEEP_SECONDS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            match sweep(&pool, &artifacts).await {
                Ok(report) if report.failed > 0 => {
                    tracing::warn!(event = "file_sweep_incomplete", failed = report.failed);
                }
                Ok(_) => {}
                Err(_) => tracing::error!(event = "file_sweep_failed"),
            }
        }
    })))
}

pub async fn reserve(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
    request: Request,
) -> ApiResult<Json<StoredFile>> {
    let installation = access(&s, &p, id, request.extensions(), true).await?;
    let (parts, body) = request.into_parts();
    let bytes = tokio::time::timeout(
        Duration::from_secs(5),
        axum::body::to_bytes(body, 16 * 1024),
    )
    .await
    .map_err(|_| ApiError::bad("file_metadata_timeout"))?
    .map_err(|_| ApiError::bad("file_metadata_too_large"))?;
    let mut value: ReserveFile = serde_json::from_slice(&bytes)?;
    validate_reservation(&mut value)?;
    let request = Request::from_parts(parts, Body::empty());
    let directory = root(&s.artifacts, id).await?;
    // Checked only for a new reservation; a repeated request never needs it.
    let space = DiskSpace::measure(&directory).await;
    // A single-request reservation keeps the hash it always had.
    let input_hash = crate::contract::hash(&match value.transfer {
        FileTransfer::Single => serde_json::to_vec(&(&p.id, &value.name, &value.mime, value.size))?,
        FileTransfer::Parts => serde_json::to_vec(&(
            &p.id,
            &value.name,
            &value.mime,
            value.size,
            value.transfer.as_str(),
        ))?,
    });
    // Recheck after the body read and the measurement. The transaction below
    // only runs short statements on its own connection.
    access(&s, &p, id, request.extensions(), true).await?;
    let mut tx = Write::begin(&s.pool).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('miy-app-file-reservations'))")
        .execute(&mut *tx)
        .await?;
    if let Some(row) =
        sqlx::query("SELECT * FROM app_files WHERE installation_id=$1 AND request_id=$2")
            .bind(id)
            .bind(value.request_id)
            .fetch_optional(&mut *tx)
            .await?
    {
        if row.get::<String, _>("input_hash") != input_hash {
            return Err(ApiError::conflict("file_request_conflict"));
        }
        let result = file_row(&row)?;
        commit(tx, &p, &installation).await?;
        return Ok(Json(result));
    }
    // Elapsed reservations no longer count, whether or not the sweeper has
    // removed them yet: quota never depends on housekeeping timing.
    // The installation's limit (administrator-set or default) bounds what it
    // owns; free disk space, not a fixed total, bounds the whole store.
    // Promised bytes are what reservations have not written yet.
    let quota = sqlx::query("SELECT COALESCE(sum(size) FILTER(WHERE installation_id=$1),0)::bigint AS owned, COALESCE(sum(size-received) FILTER(WHERE status IN ('pending','uploading')),0)::bigint AS pending, count(*) FILTER(WHERE installation_id=$1) AS owned_files, count(*) FILTER(WHERE status IN ('pending','uploading')) AS pending_files, (SELECT max_bytes FROM app_file_quotas WHERE installation_id=$1) AS max_bytes, (SELECT max_files FROM app_file_quotas WHERE installation_id=$1) AS max_files FROM app_files WHERE status IN ('ready','attached','cancelling') OR (status IN ('pending','uploading') AND expires_at>clock_timestamp())")
        .bind(id).fetch_one(&mut *tx).await?;
    let max_bytes = quota
        .get::<Option<i64>, _>("max_bytes")
        .map_or(DEFAULT_INSTALLATION_BYTES, |v| v as u64);
    let max_files = quota
        .get::<Option<i32>, _>("max_files")
        .map_or(DEFAULT_INSTALLATION_FILES, |v| v as u64);
    if quota.get::<i64, _>("owned") as u64 + value.size > max_bytes
        || quota.get::<i64, _>("owned_files") as u64 >= max_files
    {
        return Err(ApiError::conflict("file_quota_exceeded"));
    }
    space?.require(
        quota.get::<i64, _>("pending") as u64 + value.size,
        quota.get::<i64, _>("pending_files") as u64 + 1,
    )?;
    let file = Uuid::new_v4();
    let row = sqlx::query("INSERT INTO app_files(id,installation_id,request_id,created_by,input_hash,name,mime,size,status,expires_at,transfer) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'pending',clock_timestamp()+interval '24 hours',$9) RETURNING *")
        .bind(file).bind(id).bind(value.request_id).bind(&p.id).bind(input_hash).bind(value.name).bind(value.mime).bind(value.size as i64).bind(value.transfer.as_str()).fetch_one(&mut *tx).await?;
    audit(&mut tx, &p, "file.reserved", file).await?;
    commit(tx, &p, &installation).await?;
    Ok(Json(file_row(&row)?))
}

pub async fn by_request(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path((id, request_id)): Path<(Uuid, Uuid)>,
    request: Request,
) -> ApiResult<Json<StoredFile>> {
    access(&s, &p, id, request.extensions(), false).await?;
    let row = sqlx::query(
        "SELECT * FROM app_files WHERE installation_id=$1 AND request_id=$2 AND created_by=$3",
    )
    .bind(id)
    .bind(request_id)
    .bind(&p.id)
    .fetch_optional(&s.pool)
    .await?
    .ok_or_else(ApiError::not_found)?;
    Ok(Json(file_row(&row)?))
}

pub async fn metadata(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path((id, file)): Path<(Uuid, Uuid)>,
    request: Request,
) -> ApiResult<Json<StoredFile>> {
    access(&s, &p, id, request.extensions(), false).await?;
    let row = sqlx::query("SELECT * FROM app_files WHERE installation_id=$1 AND id=$2")
        .bind(id)
        .bind(file)
        .fetch_optional(&s.pool)
        .await?
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(file_row(&row)?))
}

/// Read a small JSON body, then recheck authorization: the write that follows
/// runs on one connection and does not check again.
async fn checked_body<T: serde::de::DeserializeOwned>(
    s: &State,
    p: &Principal,
    id: Uuid,
    request: Request,
) -> ApiResult<(PgRow, T)> {
    access(s, p, id, request.extensions(), true).await?;
    let (parts, body) = request.into_parts();
    let bytes = tokio::time::timeout(Duration::from_secs(5), axum::body::to_bytes(body, 4096))
        .await
        .map_err(|_| ApiError::bad("file_metadata_timeout"))?
        .map_err(|_| ApiError::bad("file_metadata_too_large"))?;
    let value = serde_json::from_slice(&bytes)?;
    let installation = access(s, p, id, &parts.extensions, true).await?;
    Ok((installation, value))
}

pub async fn attach(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path((id, file)): Path<(Uuid, Uuid)>,
    request: Request,
) -> ApiResult<Json<StoredFile>> {
    let (installation, value): (_, AttachFile) = checked_body(&s, &p, id, request).await?;
    if value.request_id.is_nil() || !valid_reference(&value.reference) {
        return Err(ApiError::bad("invalid_file_reference"));
    }
    let mut tx = Write::begin(&s.pool).await?;
    let row = lock_file(&mut tx, id, file).await?;
    let current = file_row(&row)?;
    if !matches!(current.status, FileStatus::Ready | FileStatus::Attached) {
        return Err(ApiError::conflict("file_not_ready"));
    }
    let bytes = root(&s.artifacts, id).await?.join(format!("{file}.blob"));
    let info = tokio::fs::symlink_metadata(&bytes)
        .await
        .map_err(|_| ApiError::unavailable("file_bytes_unavailable"))?;
    if !info.is_file() || info.file_type().is_symlink() || info.len() != current.size {
        return Err(ApiError::unavailable("file_bytes_invalid"));
    }
    if current.status == FileStatus::Attached {
        if current.reference.as_ref() != Some(&value.reference)
            || row.get::<Option<Uuid>, _>("attach_request_id") != Some(value.request_id)
        {
            return Err(ApiError::conflict("file_reference_conflict"));
        }
        commit(tx, &p, &installation).await?;
        return Ok(Json(current));
    }
    let row = sqlx::query("UPDATE app_files SET status='attached',reference=$2,attach_request_id=$3,updated_at=clock_timestamp() WHERE id=$1 RETURNING *")
        .bind(file).bind(value.reference).bind(value.request_id).fetch_one(&mut *tx).await?;
    audit(&mut tx, &p, "file.attached", file).await?;
    commit(tx, &p, &installation).await?;
    Ok(Json(file_row(&row)?))
}

pub async fn cancel(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path((id, file)): Path<(Uuid, Uuid)>,
    request: Request,
) -> ApiResult<Json<StoredFile>> {
    let installation = access(&s, &p, id, request.extensions(), true).await?;
    let mut tx = Write::begin(&s.pool).await?;
    let row = lock_file(&mut tx, id, file).await?;
    if row.get::<&str, _>("status") == "attached" {
        return Err(ApiError::conflict("file_attached"));
    }
    if matches!(row.get::<&str, _>("status"), "cancelled" | "expired") {
        let result = file_row(&row)?;
        commit(tx, &p, &installation).await?;
        return Ok(Json(result));
    }
    // Commit the logical tombstone before deleting bytes. If deletion, audit or
    // the final DB commit fails, the file never rolls back to attachable `ready`.
    // This internal state still occupies quota until deletion is confirmed.
    sqlx::query("UPDATE app_files SET status='cancelling',expires_at=NULL,updated_at=clock_timestamp() WHERE id=$1").bind(file).execute(&mut *tx).await?;
    audit(&mut tx, &p, "file.cancel_requested", file).await?;
    commit(tx, &p, &installation).await?;
    discard(&s, &p, &installation, id, file, "file.cancelled").await
}

/// Release an attached file after the owning app removed its reference. The
/// matching reference is the app's statement of which attachment it means;
/// `cancel` stays unable to delete attached bytes.
pub async fn detach(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path((id, file)): Path<(Uuid, Uuid)>,
    request: Request,
) -> ApiResult<Json<StoredFile>> {
    let (installation, value): (_, DetachFile) = checked_body(&s, &p, id, request).await?;
    if !valid_reference(&value.reference) {
        return Err(ApiError::bad("invalid_file_reference"));
    }
    let mut tx = Write::begin(&s.pool).await?;
    let row = lock_file(&mut tx, id, file).await?;
    match row.get::<&str, _>("status") {
        // A repeated request after the tombstone committed finishes the removal.
        "cancelling" | "cancelled" => {}
        "attached" => {
            if row.get::<Option<&str>, _>("reference") != Some(value.reference.as_str()) {
                return Err(ApiError::conflict("file_reference_conflict"));
            }
            sqlx::query("UPDATE app_files SET status='cancelling',reference=NULL,attach_request_id=NULL,updated_at=clock_timestamp() WHERE id=$1").bind(file).execute(&mut *tx).await?;
            audit(&mut tx, &p, "file.detach_requested", file).await?;
        }
        _ => return Err(ApiError::conflict("file_not_attached")),
    }
    commit(tx, &p, &installation).await?;
    discard(&s, &p, &installation, id, file, "file.detached").await
}

/// Second phase of cancel and detach. The tombstone is already durable, so a
/// failure here leaves `cancelling` for a repeated request or the sweeper.
/// Reserved bytes are released only after the bytes are gone, never before.
async fn discard(
    s: &State,
    p: &Principal,
    installation: &PgRow,
    id: Uuid,
    file: Uuid,
    action: &str,
) -> ApiResult<Json<StoredFile>> {
    let mut tx = Write::begin(&s.pool).await?;
    let row = lock_file(&mut tx, id, file).await?;
    let row = match row.get::<&str, _>("status") {
        "cancelled" => row,
        "cancelling" => {
            remove_bytes(&s.artifacts, id, file).await?;
            let row = sqlx::query("UPDATE app_files SET status='cancelled',expires_at=NULL,updated_at=clock_timestamp() WHERE id=$1 RETURNING *")
                .bind(file).fetch_one(&mut *tx).await?;
            audit(&mut tx, p, action, file).await?;
            row
        }
        _ => return Err(ApiError::conflict("file_state_changed")),
    };
    commit(tx, p, installation).await?;
    Ok(Json(file_row(&row)?))
}

pub async fn upload(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path((id, file)): Path<(Uuid, Uuid)>,
    request: Request,
) -> ApiResult<Json<StoredFile>> {
    let installation = access(&s, &p, id, request.extensions(), true).await?;
    let _slot = TRANSFERS
        .try_acquire()
        .map_err(|_| ApiError::unavailable("file_transfer_busy"))?;
    let claimed = request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or_else(|| ApiError::bad("file_content_length_required"))?;
    let directory = root(&s.artifacts, id).await?;
    let part = directory.join(format!("{file}.part"));
    let blob = directory.join(format!("{file}.blob"));
    let space = DiskSpace::measure(&directory).await;
    let mut initial = Write::begin(&s.pool).await?;
    let row = lock_file(&mut initial, id, file).await?;
    // `uploading` only records that bytes may exist. A failed or interrupted
    // attempt leaves it behind, and the reservation stays usable until it
    // expires: the row lock below, not this status, excludes a second upload.
    if !matches!(row.get::<&str, _>("status"), "pending" | "uploading") {
        return Err(ApiError::conflict("file_upload_state_changed"));
    }
    if row.get::<&str, _>("transfer") != "single" {
        return Err(ApiError::conflict("file_transfer_mismatch"));
    }
    if claimed != row.get::<i64, _>("size") as u64 || claimed > MAX_FILE_BYTES {
        return Err(ApiError::bad("file_size_mismatch"));
    }
    if row.get::<DateTime<Utc>, _>("expires_at") <= Utc::now() {
        return Err(ApiError::conflict("file_upload_expired"));
    }
    space?.require(claimed, 1)?;
    sqlx::query("UPDATE app_files SET status='uploading',updated_at=clock_timestamp() WHERE id=$1")
        .bind(file)
        .execute(&mut *initial)
        .await?;
    commit(initial, &p, &installation).await?;

    // Holding this row prevents explicit cancellation/expiry from releasing quota
    // while a live stream still owns an inode. An interrupted handler rolls back;
    // its incomplete bytes retain a reservation and are eligible for expiry.
    let mut tx = Write::begin(&s.pool).await?;
    let row = lock_file(&mut tx, id, file).await?;
    if row.get::<&str, _>("status") != "uploading" {
        return Err(ApiError::conflict("file_upload_state_changed"));
    }
    // No committed state refers to bytes of an earlier attempt, including a
    // blob whose `ready` commit did not happen.
    remove(&part).await?;
    remove(&blob).await?;
    let (parts, body) = request.into_parts();
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut output = options
        .open(&part)
        .await
        .map_err(|_| ApiError::unavailable("file_storage_unavailable"))?;
    let transferred = tokio::time::timeout(Duration::from_secs(TRANSFER_SECONDS), async {
        let mut stream = body.into_data_stream();
        let mut count = 0u64;
        let mut hash = Sha256::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| ApiError::bad("file_transfer_failed"))?;
            count = count
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| ApiError::bad("file_size_mismatch"))?;
            if count > claimed {
                return Err(ApiError::bad("file_size_mismatch"));
            }
            hash.update(&chunk);
            output
                .write_all(&chunk)
                .await
                .map_err(|_| ApiError::unavailable("file_storage_write_failed"))?;
        }
        if count != claimed {
            return Err(ApiError::bad("file_size_mismatch"));
        }
        output
            .sync_all()
            .await
            .map_err(|_| ApiError::unavailable("file_storage_write_failed"))?;
        Ok::<String, ApiError>(hex::encode(hash.finalize()))
    })
    .await
    .map_err(|_| ApiError::bad("file_transfer_timeout"))
    .and_then(|v| v);
    drop(output);
    let stored = async {
        let digest = transferred?;
        // Do not extend a 30-second app-call proof for a file transfer. If it
        // expired, no ready state is committed and the caller uploads again
        // with a new app request. This is the one pooled connection taken while
        // a row lock is held; the transfer slots bound it below the pool size.
        access(&s, &p, id, &parts.extensions, true).await?;
        if tokio::fs::try_exists(&blob).await? {
            return Err(ApiError::conflict("file_bytes_already_exist"));
        }
        tokio::fs::rename(&part, &blob)
            .await
            .map_err(|_| ApiError::unavailable("file_storage_write_failed"))?;
        tokio::fs::File::open(&directory).await?.sync_all().await?;
        let row = sqlx::query("UPDATE app_files SET status='ready',sha256=$2,received=size,expires_at=NULL,updated_at=clock_timestamp() WHERE id=$1 RETURNING *")
            .bind(file).bind(digest).fetch_one(&mut *tx).await?;
        audit(&mut tx, &p, "file.ready", file).await?;
        commit(tx, &p, &installation).await?;
        file_row(&row)
    }
    .await;
    if stored.is_err() {
        // Only the incomplete part: after a rename the commit result may be
        // unknown, so a blob is left for the next attempt, cancel or expiry.
        let _ = remove(&part).await;
    }
    stored.map(Json)
}

/// `Content-Range: bytes START-END/TOTAL` of one `parts` request.
fn content_range(headers: &HeaderMap) -> ApiResult<(u64, u64, u64)> {
    let invalid = || ApiError::bad("file_part_range_invalid");
    let mut values = headers.get_all(header::CONTENT_RANGE).iter();
    let (Some(value), None) = (values.next(), values.next()) else {
        return Err(invalid());
    };
    let text = value.to_str().map_err(|_| invalid())?;
    let (range, total) = text
        .strip_prefix("bytes ")
        .and_then(|rest| rest.split_once('/'))
        .ok_or_else(invalid)?;
    let (start, end) = range.split_once('-').ok_or_else(invalid)?;
    let number = |value: &str| -> ApiResult<u64> {
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) || value.len() > 19 {
            return Err(invalid());
        }
        value.parse().map_err(|_| invalid())
    };
    let (start, end, total) = (number(start)?, number(end)?, number(total)?);
    if end < start || end >= total || end - start + 1 > MAX_PART_BYTES {
        return Err(invalid());
    }
    Ok((start, end, total))
}

/// A `parts` row may take the range `start..=end` of `total` bytes now.
fn check_part(row: &PgRow, start: u64, total: u64) -> ApiResult<()> {
    if row.get::<&str, _>("transfer") != "parts" {
        return Err(ApiError::conflict("file_transfer_mismatch"));
    }
    if !matches!(row.get::<&str, _>("status"), "pending" | "uploading") {
        return Err(ApiError::conflict("file_upload_state_changed"));
    }
    if row
        .get::<Option<DateTime<Utc>>, _>("expires_at")
        .is_none_or(|at| at <= Utc::now())
    {
        return Err(ApiError::conflict("file_upload_expired"));
    }
    if total != row.get::<i64, _>("size") as u64 {
        return Err(ApiError::bad("file_size_mismatch"));
    }
    if start != row.get::<i64, _>("received") as u64 {
        // The caller resumes from `received` in the file metadata.
        return Err(ApiError::conflict("file_part_offset"));
    }
    Ok(())
}

/// A regular file at `path` (not a link), or none.
async fn regular_file(path: &FsPath) -> ApiResult<Option<u64>> {
    match tokio::fs::symlink_metadata(path).await {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Ok(Some(meta.len())),
        Ok(_) => Err(ApiError::unavailable("file_storage_path_invalid")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(ApiError::unavailable("file_storage_unavailable")),
    }
}

/// `PUT platform/files/{id}/parts` (M10): one contiguous range of a `parts`
/// reservation, starting exactly at the committed offset. The bytes are
/// synced before the new offset commits, so a lost answer resumes from the
/// metadata's `received` and never from guessed bytes.
pub async fn upload_part(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path((id, file)): Path<(Uuid, Uuid)>,
    request: Request,
) -> ApiResult<Json<StoredFile>> {
    let installation = access(&s, &p, id, request.extensions(), true).await?;
    let _slot = TRANSFERS
        .try_acquire()
        .map_err(|_| ApiError::unavailable("file_transfer_busy"))?;
    let (start, end, total) = content_range(request.headers())?;
    let length = end - start + 1;
    let claimed = request
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .ok_or_else(|| ApiError::bad("file_content_length_required"))?;
    if claimed != length {
        return Err(ApiError::bad("file_size_mismatch"));
    }
    let directory = root(&s.artifacts, id).await?;
    let part = directory.join(format!("{file}.part"));
    let space = DiskSpace::measure(&directory).await;
    let mut initial = Write::begin(&s.pool).await?;
    let row = lock_file(&mut initial, id, file).await?;
    check_part(&row, start, total)?;
    space?.require(length, 1)?;
    sqlx::query("UPDATE app_files SET status='uploading',updated_at=clock_timestamp() WHERE id=$1")
        .bind(file)
        .execute(&mut *initial)
        .await?;
    commit(initial, &p, &installation).await?;

    // As for a single upload, the row lock is held through the bounded transfer.
    let mut tx = Write::begin(&s.pool).await?;
    let row = lock_file(&mut tx, id, file).await?;
    check_part(&row, start, total)?;
    let existing = regular_file(&part).await?;
    match existing {
        // The database records more than the disk holds: stop, never invent bytes.
        Some(len) if len < start => return Err(ApiError::unavailable("file_part_bytes_missing")),
        None if start > 0 => return Err(ApiError::unavailable("file_part_bytes_missing")),
        _ => {}
    }
    let mut options = tokio::fs::OpenOptions::new();
    options.write(true).create(existing.is_none());
    #[cfg(unix)]
    options.mode(0o600);
    let mut output = options
        .open(&part)
        .await
        .map_err(|_| ApiError::unavailable("file_storage_unavailable"))?;
    let (parts, body) = request.into_parts();
    let written = tokio::time::timeout(Duration::from_secs(TRANSFER_SECONDS), async {
        // A tail longer than the committed offset is an interrupted attempt.
        output
            .set_len(start)
            .await
            .map_err(|_| ApiError::unavailable("file_storage_write_failed"))?;
        output
            .seek(std::io::SeekFrom::Start(start))
            .await
            .map_err(|_| ApiError::unavailable("file_storage_write_failed"))?;
        let mut stream = body.into_data_stream();
        let mut count = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| ApiError::bad("file_transfer_failed"))?;
            count = count
                .checked_add(chunk.len() as u64)
                .filter(|count| *count <= length)
                .ok_or_else(|| ApiError::bad("file_size_mismatch"))?;
            output
                .write_all(&chunk)
                .await
                .map_err(|_| ApiError::unavailable("file_storage_write_failed"))?;
        }
        if count != length {
            return Err(ApiError::bad("file_size_mismatch"));
        }
        output
            .sync_all()
            .await
            .map_err(|_| ApiError::unavailable("file_storage_write_failed"))?;
        if existing.is_none() {
            tokio::fs::File::open(&directory)
                .await?
                .sync_all()
                .await
                .map_err(|_| ApiError::unavailable("file_storage_write_failed"))?;
        }
        Ok::<(), ApiError>(())
    })
    .await
    .map_err(|_| ApiError::bad("file_transfer_timeout"))
    .and_then(|v| v);
    drop(output);
    written?;
    access(&s, &p, id, &parts.extensions, true).await?;
    let row = sqlx::query(
        "UPDATE app_files SET received=$2,updated_at=clock_timestamp() WHERE id=$1 RETURNING *",
    )
    .bind(file)
    .bind((end + 1) as i64)
    .fetch_one(&mut *tx)
    .await?;
    commit(tx, &p, &installation).await?;
    Ok(Json(file_row(&row)?))
}

/// SHA-256 of a whole file, read in bounded chunks.
async fn file_digest(path: &FsPath) -> ApiResult<String> {
    let mut input = tokio::fs::File::open(path)
        .await
        .map_err(|_| ApiError::unavailable("file_bytes_unavailable"))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = input
            .read(&mut buffer)
            .await
            .map_err(|_| ApiError::unavailable("file_bytes_unavailable"))?;
        if read == 0 {
            return Ok(hex::encode(hash.finalize()));
        }
        hash.update(&buffer[..read]);
    }
}

/// `POST platform/files/{id}/complete` (M10): after every range arrived, hash
/// the whole file, recheck the caller's write access, publish the bytes and
/// mark the file ready. Repeating it after success returns the ready file; an
/// interruption after the rename verifies the published bytes again.
pub async fn complete(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path((id, file)): Path<(Uuid, Uuid)>,
    request: Request,
) -> ApiResult<Json<StoredFile>> {
    let installation = access(&s, &p, id, request.extensions(), true).await?;
    let _slot = TRANSFERS
        .try_acquire()
        .map_err(|_| ApiError::unavailable("file_transfer_busy"))?;
    let directory = root(&s.artifacts, id).await?;
    let part = directory.join(format!("{file}.part"));
    let blob = directory.join(format!("{file}.blob"));
    let mut tx = Write::begin(&s.pool).await?;
    let row = lock_file(&mut tx, id, file).await?;
    if row.get::<&str, _>("transfer") != "parts" {
        return Err(ApiError::conflict("file_transfer_mismatch"));
    }
    if matches!(row.get::<&str, _>("status"), "ready" | "attached") {
        let result = file_row(&row)?;
        commit(tx, &p, &installation).await?;
        return Ok(Json(result));
    }
    if !matches!(row.get::<&str, _>("status"), "pending" | "uploading") {
        return Err(ApiError::conflict("file_upload_state_changed"));
    }
    if row
        .get::<Option<DateTime<Utc>>, _>("expires_at")
        .is_none_or(|at| at <= Utc::now())
    {
        return Err(ApiError::conflict("file_upload_expired"));
    }
    let size = row.get::<i64, _>("size") as u64;
    if row.get::<i64, _>("received") as u64 != size {
        return Err(ApiError::conflict("file_parts_incomplete"));
    }
    let (source, published) = match (regular_file(&part).await?, regular_file(&blob).await?) {
        (Some(len), None) if len == size => (part.clone(), false),
        (None, Some(len)) if len == size => (blob.clone(), true),
        _ => return Err(ApiError::unavailable("file_bytes_invalid")),
    };
    let digest = tokio::time::timeout(Duration::from_secs(120), file_digest(&source))
        .await
        .map_err(|_| ApiError::unavailable("file_digest_timeout"))??;
    // The caller must still be allowed to write when the file becomes usable.
    access(&s, &p, id, request.extensions(), true).await?;
    if !published {
        tokio::fs::rename(&part, &blob)
            .await
            .map_err(|_| ApiError::unavailable("file_storage_write_failed"))?;
        tokio::fs::File::open(&directory).await?.sync_all().await?;
    }
    let row = sqlx::query("UPDATE app_files SET status='ready',sha256=$2,received=size,expires_at=NULL,updated_at=clock_timestamp() WHERE id=$1 RETURNING *")
        .bind(file).bind(digest).fetch_one(&mut *tx).await?;
    audit(&mut tx, &p, "file.ready", file).await?;
    commit(tx, &p, &installation).await?;
    Ok(Json(file_row(&row)?))
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuotaInput {
    /// Bytes this installation may keep (0 to 1 TiB).
    pub max_bytes: u64,
    /// Files this installation may keep (0 to 10,000,000).
    pub max_files: u64,
}

async fn quota_view(pool: &PgPool, id: Uuid) -> ApiResult<serde_json::Value> {
    let row = sqlx::query("SELECT (SELECT max_bytes FROM app_file_quotas WHERE installation_id=$1) AS max_bytes, (SELECT max_files FROM app_file_quotas WHERE installation_id=$1) AS max_files, COALESCE((SELECT sum(size) FROM app_files WHERE installation_id=$1 AND (status IN ('ready','attached','cancelling') OR (status IN ('pending','uploading') AND expires_at>clock_timestamp()))),0)::bigint AS used_bytes, (SELECT count(*) FROM app_files WHERE installation_id=$1 AND (status IN ('ready','attached','cancelling') OR (status IN ('pending','uploading') AND expires_at>clock_timestamp()))) AS used_files")
        .bind(id).fetch_one(pool).await?;
    let max_bytes: Option<i64> = row.get("max_bytes");
    Ok(serde_json::json!({
        "installationId": id,
        "custom": max_bytes.is_some(),
        "maxBytes": max_bytes.map_or(DEFAULT_INSTALLATION_BYTES, |v| v as u64),
        "maxFiles": row.get::<Option<i32>, _>("max_files").map_or(DEFAULT_INSTALLATION_FILES, |v| v as u64),
        "usedBytes": row.get::<i64, _>("used_bytes"),
        "usedFiles": row.get::<i64, _>("used_files"),
    }))
}

/// `GET /api/v1/installations/{id}/file-quota` (console administrator).
pub async fn quota(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    crate::domain::admin(&s, &p).await?;
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM installations WHERE id=$1")
        .bind(id)
        .fetch_optional(&s.pool)
        .await?
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(quota_view(&s.pool, id).await?))
}

/// `PUT /api/v1/installations/{id}/file-quota` sets the installation's limits;
/// `DELETE` returns it to the defaults. Existing files stay; only new
/// reservations are refused above the limit. Audited.
pub async fn set_quota(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
    method: Method,
    body: Option<Json<QuotaInput>>,
) -> ApiResult<Json<serde_json::Value>> {
    crate::domain::admin(&s, &p).await?;
    let input = match (&method, body) {
        (&Method::PUT, Some(Json(input))) => Some(input),
        (&Method::DELETE, _) => None,
        _ => return Err(ApiError::bad("invalid_file_quota")),
    };
    if input
        .as_ref()
        .is_some_and(|q| q.max_bytes > 1024 * 1024 * 1024 * 1024 || q.max_files > 10_000_000)
    {
        return Err(ApiError::bad("invalid_file_quota"));
    }
    let mut tx = Write::begin(&s.pool).await?;
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM installations WHERE id=$1 FOR SHARE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let mut tx = tx
        .decide_platform(&p, crate::policy::PlatformAction::Administer)
        .await?;
    let target = match &input {
        Some(q) => {
            sqlx::query("INSERT INTO app_file_quotas(installation_id,max_bytes,max_files,updated_by) VALUES($1,$2,$3,$4) ON CONFLICT(installation_id) DO UPDATE SET max_bytes=$2,max_files=$3,updated_by=$4,updated_at=clock_timestamp()")
                .bind(id).bind(q.max_bytes as i64).bind(q.max_files as i32).bind(&p.id).execute(&mut *tx).await?;
            format!(
                "installation:{id}:bytes:{}:files:{}",
                q.max_bytes, q.max_files
            )
        }
        None => {
            sqlx::query("DELETE FROM app_file_quotas WHERE installation_id=$1")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            format!("installation:{id}:default")
        }
    };
    crate::audit::record(&mut tx, &p.id, "file.quota_set", &target).await?;
    tx.commit().await?;
    Ok(Json(quota_view(&s.pool, id).await?))
}

/// Download links live this long (each GET and Range is rechecked anyway).
const DOWNLOAD_LINK_SECONDS: f64 = 300.0;
/// Large downloads have their own budget, apart from upload/backend transfers.
static DOWNLOADS: Semaphore = Semaphore::const_new(8);
/// A stalled reader is cut; a slow but steady download is not.
const DOWNLOAD_IDLE: Duration = Duration::from_secs(60);

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LinkInput {
    #[serde(default)]
    reference: Option<String>,
}

/// `POST platform/files/{id}/download` (M10): after its own ACL check, the
/// owning app's backend gets a short-lived link for the frame that started
/// the request. The link names the file and, for an attached file, the app
/// reference it was authorized for.
pub async fn download_link(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path((id, file)): Path<(Uuid, Uuid)>,
    request: Request,
) -> ApiResult<Json<serde_json::Value>> {
    let installation = access(&s, &p, id, request.extensions(), false).await?;
    // Only a chain that a frame of this very installation started: the link is
    // served on that frame's origin and bound to that frame session.
    let frame = request
        .extensions()
        .get::<crate::app_calls::Context>()
        .filter(|context| context.installation_id == id && context.callers.is_empty())
        .and_then(|context| context.origin.as_ref())
        .filter(|origin| origin.kind == "frame")
        .and_then(|origin| Uuid::parse_str(&origin.digest).ok())
        .ok_or_else(|| ApiError::forbidden_code("file_download_frame_required"))?;
    let (parts, body) = request.into_parts();
    let bytes = tokio::time::timeout(Duration::from_secs(5), axum::body::to_bytes(body, 4096))
        .await
        .map_err(|_| ApiError::bad("file_metadata_timeout"))?
        .map_err(|_| ApiError::bad("file_metadata_too_large"))?;
    let input: LinkInput = if bytes.is_empty() {
        LinkInput { reference: None }
    } else {
        serde_json::from_slice(&bytes)?
    };
    if input
        .reference
        .as_deref()
        .is_some_and(|r| !valid_reference(r))
    {
        return Err(ApiError::bad("invalid_file_reference"));
    }
    let installation_now = access(&s, &p, id, &parts.extensions, false).await?;
    let declared: bool = sqlx::query_scalar(
        "SELECT COALESCE(jsonb_typeof(r.manifest->'platformEndpoints'->'fileAccess')='string',false) FROM releases r WHERE r.id=$1",
    )
    .bind(installation_now.get::<Uuid, _>("release_id"))
    .fetch_one(&s.pool)
    .await?;
    if !declared {
        return Err(ApiError::conflict("file_access_endpoint_missing"));
    }
    let mut tx = Write::begin(&s.pool).await?;
    let row = sqlx::query(
        "SELECT status,reference FROM app_files WHERE installation_id=$1 AND id=$2 FOR SHARE",
    )
    .bind(id)
    .bind(file)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(ApiError::not_found)?;
    match row.get::<&str, _>("status") {
        "ready" | "attached" => {}
        _ => return Err(ApiError::conflict("file_not_ready")),
    }
    if row.get::<Option<String>, _>("reference") != input.reference {
        return Err(ApiError::conflict("file_reference_conflict"));
    }
    sqlx::query("DELETE FROM app_file_downloads WHERE expires_at<clock_timestamp()")
        .execute(&mut *tx)
        .await?;
    let token = crate::auth::random();
    let expires_at: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO app_file_downloads(token_hash,installation_id,release_id,file_id,frame_id,user_id,reference,expires_at) \
         SELECT $1,$2,$3,$4,f.id,$6,$7,clock_timestamp()+make_interval(secs=>$8) FROM app_frame_sessions f \
         WHERE f.id=$5 AND f.user_id=$6 AND f.installation_id=$2 AND f.revoked_at IS NULL AND f.expires_at>clock_timestamp() \
         RETURNING expires_at",
    )
    .bind(crate::contract::hash(&token))
    .bind(id)
    .bind(installation_now.get::<Uuid, _>("release_id"))
    .bind(file)
    .bind(frame)
    .bind(&p.id)
    .bind(&input.reference)
    .bind(DOWNLOAD_LINK_SECONDS)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(ApiError::unauthorized)?;
    commit(tx, &p, &installation).await?;
    Ok(Json(serde_json::json!({
        "url": format!("/_miy/downloads/{token}"),
        "expiresAt": expires_at,
    })))
}

/// One `Range: bytes=...` request against `size`, if it asks for one range.
/// `Ok(None)` serves the whole file; `Err` is 416.
fn requested_range(headers: &HeaderMap, size: u64, etag: &str) -> Result<Option<(u64, u64)>, ()> {
    let mut values = headers.get_all(header::RANGE).iter();
    let (Some(value), None) = (values.next(), values.next()) else {
        return Ok(None);
    };
    // A changed file answers whole, as HTTP says for a failed If-Range.
    if let Some(condition) = headers.get(header::IF_RANGE)
        && condition.to_str().ok() != Some(etag)
    {
        return Ok(None);
    }
    let Some(spec) = value.to_str().ok().and_then(|v| v.strip_prefix("bytes=")) else {
        return Ok(None);
    };
    if spec.contains(',') {
        return Ok(None);
    }
    let (start, end) = spec.split_once('-').ok_or(())?;
    let number = |v: &str| v.trim().parse::<u64>().map_err(|_| ());
    let (start, end) = match (start.trim().is_empty(), end.trim().is_empty()) {
        (false, true) => (number(start)?, size.saturating_sub(1)),
        (false, false) => (number(start)?, number(end)?.min(size.saturating_sub(1))),
        (true, false) => {
            let suffix = number(end)?.min(size);
            (size - suffix, size.saturating_sub(1))
        }
        (true, true) => return Err(()),
    };
    if size == 0 || start > end || start >= size {
        return Err(());
    }
    Ok(Some((start, end)))
}

/// Serve a download link for the frame listener (app_frames.rs has checked the
/// origin, the frame session, the person's use and the release). Here: the
/// release's read approval, the file and its reference, and the owning app's
/// own answer for this file, then the bytes (one range at most).
pub(crate) async fn serve_download(
    s: &State,
    p: &Principal,
    installation: Uuid,
    release: Uuid,
    file: Uuid,
    reference: Option<String>,
    headers: &HeaderMap,
) -> ApiResult<Response> {
    crate::permissions::require(&s.pool, installation, release, "platform:files:read").await?;
    let slot = DOWNLOADS
        .try_acquire()
        .map_err(|_| ApiError::unavailable("file_download_busy"))?;
    let row = sqlx::query("SELECT * FROM app_files WHERE installation_id=$1 AND id=$2 AND status IN ('ready','attached')")
        .bind(installation).bind(file).fetch_optional(&s.pool).await?.ok_or_else(ApiError::not_found)?;
    let metadata = file_row(&row)?;
    if metadata.reference != reference {
        return Err(ApiError::not_found());
    }
    let answer = crate::platform_calls::call(
        s,
        p,
        installation,
        crate::platform_calls::Endpoint::FileAccess,
        &serde_json::json!({"fileId": file, "reference": reference}),
    )
    .await;
    match answer {
        Ok(value) if value["allowed"] == true => {}
        Err(crate::platform_calls::Failure::Unavailable) => {
            return Err(ApiError::unavailable("file_access_unavailable"));
        }
        _ => return Err(ApiError::forbidden_code("file_access_denied")),
    }
    let path = root(&s.artifacts, installation)
        .await?
        .join(format!("{file}.blob"));
    let info = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|_| ApiError::unavailable("file_bytes_unavailable"))?;
    if !info.is_file() || info.file_type().is_symlink() || info.len() != metadata.size {
        return Err(ApiError::unavailable("file_bytes_invalid"));
    }
    let etag = format!("\"{}\"", metadata.sha256.as_deref().unwrap_or_default());
    let range = match requested_range(headers, metadata.size, &etag) {
        Ok(range) => range,
        Err(()) => {
            let mut response = axum::http::StatusCode::RANGE_NOT_SATISFIABLE.into_response();
            response.headers_mut().insert(
                header::CONTENT_RANGE,
                format!("bytes */{}", metadata.size).parse().unwrap(),
            );
            return Ok(response);
        }
    };
    let (start, length) = match range {
        Some((start, end)) => (start, end - start + 1),
        None => (0, metadata.size),
    };
    let mut handle = tokio::fs::File::open(&path)
        .await
        .map_err(|_| ApiError::unavailable("file_bytes_unavailable"))?;
    handle
        .seek(std::io::SeekFrom::Start(start))
        .await
        .map_err(|_| ApiError::unavailable("file_bytes_unavailable"))?;
    struct Stream {
        file: tokio::fs::File,
        remaining: u64,
        _slot: SemaphorePermit<'static>,
    }
    let stream = futures_util::stream::try_unfold(
        Stream {
            file: handle,
            remaining: length,
            _slot: slot,
        },
        |mut state| async move {
            if state.remaining == 0 {
                return Ok::<_, std::io::Error>(None);
            }
            let mut bytes = vec![0u8; state.remaining.min(256 * 1024) as usize];
            let n = tokio::time::timeout(DOWNLOAD_IDLE, state.file.read(&mut bytes))
                .await
                .map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::TimedOut, "file_download_idle")
                })??;
            if n == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "file_bytes_invalid",
                ));
            }
            state.remaining -= n as u64;
            bytes.truncate(n);
            Ok(Some((Bytes::from(bytes), state)))
        },
    );
    let mut response = Response::new(Body::from_stream(stream));
    if let Some((start, end)) = range {
        *response.status_mut() = axum::http::StatusCode::PARTIAL_CONTENT;
        response.headers_mut().insert(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{}", metadata.size)
                .parse()
                .unwrap(),
        );
    }
    let out = response.headers_mut();
    secure_headers(out);
    out.insert(
        header::CONTENT_TYPE,
        metadata
            .mime
            .parse()
            .map_err(|_| ApiError::unavailable("file_metadata_invalid"))?,
    );
    out.insert(header::CONTENT_LENGTH, length.to_string().parse().unwrap());
    out.insert(
        header::CONTENT_DISPOSITION,
        attachment(&metadata.name)
            .parse()
            .map_err(|_| ApiError::unavailable("file_metadata_invalid"))?,
    );
    out.insert(header::ETAG, etag.parse().unwrap());
    out.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
    out.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    Ok(response)
}

pub fn attachment(name: &str) -> String {
    let mut encoded = String::new();
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    format!("attachment; filename=\"download\"; filename*=UTF-8''{encoded}")
}

pub fn secure_headers(headers: &mut HeaderMap) {
    crate::file_transfer::secure_headers(headers);
}

/// Trusted extension constructed only by the developer-only preview endpoint.
#[derive(Clone)]
struct PreviewStorage {
    app: String,
    installation: Uuid,
    release: Uuid,
}

async fn prepare_preview(
    s: &State,
    p: &Principal,
    app: &str,
    request: &mut Request,
) -> ApiResult<Uuid> {
    if p.browser_purpose.is_some() {
        return Err(ApiError::forbidden());
    }
    crate::connections::require_preview(s, p, app).await?;
    let row = sqlx::query(
        "SELECT id,release_id FROM installations WHERE app_id=$1 AND environment='dev'",
    )
    .bind(app)
    .fetch_optional(&s.pool)
    .await?
    .ok_or_else(ApiError::not_found)?;
    let id: Uuid = row.get("id");
    request.extensions_mut().insert(PreviewStorage {
        app: app.into(),
        installation: id,
        release: row.get("release_id"),
    });
    Ok(id)
}

pub async fn preview_reserve(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(app): Path<String>,
    mut request: Request,
) -> ApiResult<Response> {
    let id = prepare_preview(&s, &p, &app, &mut request).await?;
    Ok(reserve(S(s), Extension(p), Path(id), request)
        .await?
        .into_response())
}

pub async fn preview_file(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path((app, path)): Path<(String, String)>,
    mut request: Request,
) -> ApiResult<Response> {
    if !allowed_route(&format!("platform/files/{path}"), request.method()) {
        return Err(ApiError::not_found());
    }
    let id = prepare_preview(&s, &p, &app, &mut request).await?;
    if let Some(request_id) = path.strip_prefix("by-request/") {
        let request_id =
            Uuid::parse_str(request_id).map_err(|_| ApiError::bad("invalid_file_request"))?;
        return Ok(
            by_request(S(s), Extension(p), Path((id, request_id)), request)
                .await?
                .into_response(),
        );
    }
    let (file, action) = path.split_once('/').unwrap_or((path.as_str(), ""));
    let file = Uuid::parse_str(file).map_err(|_| ApiError::bad("invalid_file_id"))?;
    match (request.method().clone(), action) {
        (Method::GET, "") => Ok(metadata(S(s), Extension(p), Path((id, file)), request)
            .await?
            .into_response()),
        (Method::DELETE, "") => Ok(cancel(S(s), Extension(p), Path((id, file)), request)
            .await?
            .into_response()),
        (Method::POST, "attach") => Ok(attach(S(s), Extension(p), Path((id, file)), request)
            .await?
            .into_response()),
        (Method::POST, "detach") => Ok(detach(S(s), Extension(p), Path((id, file)), request)
            .await?
            .into_response()),
        (Method::PUT, "parts") => Ok(upload_part(S(s), Extension(p), Path((id, file)), request)
            .await?
            .into_response()),
        (Method::POST, "complete") => Ok(complete(S(s), Extension(p), Path((id, file)), request)
            .await?
            .into_response()),
        (Method::POST, "download") => {
            Ok(download_link(S(s), Extension(p), Path((id, file)), request)
                .await?
                .into_response())
        }
        (Method::PUT, "content") => Ok(upload(S(s), Extension(p), Path((id, file)), request)
            .await?
            .into_response()),
        (Method::GET, "content") => download(S(s), Extension(p), Path((id, file)), request).await,
        _ => Err(ApiError::not_found()),
    }
}

struct Download {
    file: tokio::fs::File,
    remaining: u64,
    deadline: tokio::time::Instant,
    _slot: SemaphorePermit<'static>,
}

pub async fn download(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path((id, file)): Path<(Uuid, Uuid)>,
    request: Request,
) -> ApiResult<Response> {
    access(&s, &p, id, request.extensions(), false).await?;
    let slot = TRANSFERS
        .try_acquire()
        .map_err(|_| ApiError::unavailable("file_transfer_busy"))?;
    let row = sqlx::query("SELECT * FROM app_files WHERE installation_id=$1 AND id=$2 AND status IN ('ready','attached')")
        .bind(id).bind(file).fetch_optional(&s.pool).await?.ok_or_else(ApiError::not_found)?;
    let metadata = file_row(&row)?;
    let path = root(&s.artifacts, id).await?.join(format!("{file}.blob"));
    let info = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|_| ApiError::unavailable("file_bytes_unavailable"))?;
    if !info.is_file() || info.file_type().is_symlink() || info.len() != metadata.size {
        return Err(ApiError::unavailable("file_bytes_invalid"));
    }
    let file_handle = tokio::fs::File::open(&path)
        .await
        .map_err(|_| ApiError::unavailable("file_bytes_unavailable"))?;
    access(&s, &p, id, request.extensions(), false).await?;
    let stream = futures_util::stream::try_unfold(
        Download {
            file: file_handle,
            remaining: metadata.size,
            deadline: tokio::time::Instant::now() + Duration::from_secs(TRANSFER_SECONDS),
            _slot: slot,
        },
        |mut state| async move {
            if state.remaining == 0 {
                return Ok::<_, std::io::Error>(None);
            }
            let mut bytes = vec![0u8; state.remaining.min(64 * 1024) as usize];
            let n = tokio::time::timeout_at(state.deadline, state.file.read(&mut bytes))
                .await
                .map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::TimedOut, "file_transfer_timeout")
                })??;
            if n == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "file_bytes_invalid",
                ));
            }
            state.remaining -= n as u64;
            bytes.truncate(n);
            Ok(Some((Bytes::from(bytes), state)))
        },
    );
    let mut response = Response::new(Body::from_stream(stream));
    let headers = response.headers_mut();
    secure_headers(headers);
    headers.insert(
        header::CONTENT_TYPE,
        metadata
            .mime
            .parse()
            .map_err(|_| ApiError::unavailable("file_metadata_invalid"))?,
    );
    headers.insert(
        header::CONTENT_LENGTH,
        metadata.size.to_string().parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        attachment(&metadata.name)
            .parse()
            .map_err(|_| ApiError::unavailable("file_metadata_invalid"))?,
    );
    headers.insert(
        header::ETAG,
        format!("\"{}\"", metadata.sha256.as_deref().unwrap_or_default())
            .parse()
            .unwrap(),
    );
    headers.insert("x-miy-file-id", metadata.id.to_string().parse().unwrap());
    headers.insert("x-miy-file-version", "1".parse().unwrap());
    headers.insert(
        "x-miy-file-sha256",
        metadata
            .sha256
            .as_deref()
            .unwrap_or_default()
            .parse()
            .unwrap(),
    );
    Ok(response)
}

pub fn catalog() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({"id":"reserve_file","method":"POST","path":"/api/v1/installations/{id}/platform/files","permission":"owning app continuation, current use and platform:files:write approval","description":"Reserve one immutable file up to 10 MiB. Stable requestId retrieves the same result; app database owns references and ACL. Browser and ordinary CLI credentials are rejected.","input":schemars::schema_for!(crate::storage_contract::ReserveFile)}),
        serde_json::json!({"id":"file_request","method":"GET","path":"/api/v1/installations/{id}/platform/files/by-request/{request}","permission":"owning app continuation, original requester and platform:files:read approval","description":"Recover a reservation result after an uncertain response; never compensate by deleting a possibly attached file"}),
        serde_json::json!({"id":"file_metadata","method":"GET","path":"/api/v1/installations/{id}/platform/files/{file}","permission":"owning app continuation, current use and platform:files:read approval","description":"Read app-scoped immutable file metadata; the backend must check current app ACL before calling"}),
        serde_json::json!({"id":"file_upload","method":"PUT","path":"/api/v1/installations/{id}/platform/files/{file}/content","permission":"owning app continuation, current use and platform:files:write approval","description":"Stream exactly the reserved Content-Length within 20 seconds; current authorization is rechecked before ready commit. A failed or interrupted transfer may be sent again until the reservation expires; a concurrent state change answers 409 file_upload_in_progress","binary":true}),
        serde_json::json!({"id":"file_download","method":"GET","path":"/api/v1/installations/{id}/platform/files/{file}/content","permission":"owning app continuation, current use and platform:files:read approval","description":"Stream a ready or attached file with a fixed response header allowlist","binary":true}),
        serde_json::json!({"id":"file_attach","method":"POST","path":"/api/v1/installations/{id}/platform/files/{file}/attach","permission":"owning app continuation, current use and platform:files:write approval","description":"Idempotently attach an opaque app-owned reference; ready and attached files do not expire","input":schemars::schema_for!(crate::storage_contract::AttachFile)}),
        serde_json::json!({"id":"file_detach","method":"POST","path":"/api/v1/installations/{id}/platform/files/{file}/detach","permission":"owning app continuation, current use and platform:files:write approval","description":"Release an attached file after the app's transaction removed its reference: the matching reference is required, the bytes are deleted and the quota is returned. Repeating the request is safe","input":schemars::schema_for!(crate::storage_contract::DetachFile)}),
        serde_json::json!({"id":"file_cancel","method":"DELETE","path":"/api/v1/installations/{id}/platform/files/{file}","permission":"owning app continuation, current use and platform:files:write approval","description":"Explicitly cancel an unattached file only after the app confirms no reference; attached files are preserved and are released with detach"}),
        serde_json::json!({"id":"app_file_read","method":"GET","path":"/api/v1/installations/{id}/files/{path}","permission":"current use and AppRead delegation; owning app ACL","description":"Bounded streaming gateway to the owning app /api/files/{path}; browser clients use the app frame /_miy/files path","binary":true}),
        serde_json::json!({"id":"app_file_write","method":"POST","path":"/api/v1/installations/{id}/files/{path}","permission":"current use and AppWrite delegation, current release; owning app ACL","description":"File mutation through the owning app, separate from the 4 MiB JSON gateway","binary":true}),
        serde_json::json!({"id":"preview_file_reserve","method":"POST","path":"/api/v1/preview/apps/{app}/platform/files","permission":"CLI DevSession on app/dev, current use and platform:files:write approval","description":"Native local backend capability proxy only; never browser credentials or production installations","input":schemars::schema_for!(crate::storage_contract::ReserveFile)}),
        serde_json::json!({"id":"preview_file_read","method":"GET","path":"/api/v1/preview/apps/{app}/platform/files/{path}","permission":"CLI DevSession on app/dev, current use and platform:files:read approval","description":"Same Storage metadata/stream contract scoped to the current dev installation"}),
        serde_json::json!({"id":"preview_file_upload","method":"PUT","path":"/api/v1/preview/apps/{app}/platform/files/{path}","permission":"CLI DevSession on app/dev, current use and platform:files:write approval","description":"Same bounded Storage upload contract through the local backend capability proxy","binary":true}),
        serde_json::json!({"id":"preview_file_attach","method":"POST","path":"/api/v1/preview/apps/{app}/platform/files/{path}","permission":"CLI DevSession on app/dev, current use and platform:files:write approval","description":"Same Storage attach contract for the actual dev installation; the detach path takes the detach input","input":schemars::schema_for!(crate::storage_contract::AttachFile)}),
        serde_json::json!({"id":"preview_file_cancel","method":"DELETE","path":"/api/v1/preview/apps/{app}/platform/files/{path}","permission":"CLI DevSession on app/dev, current use and platform:files:write approval","description":"Same explicit unattached cancellation contract for the actual dev installation"}),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_metadata_cannot_supply_paths_headers_or_oversized_reservations() {
        let valid = ReserveFile {
            request_id: Uuid::new_v4(),
            name: "설계 검토.txt".into(),
            mime: "Text/Plain".into(),
            size: MAX_FILE_BYTES,
            transfer: FileTransfer::Single,
        };
        let mut candidate = valid.clone();
        validate_reservation(&mut candidate).unwrap();
        assert_eq!(candidate.mime, "text/plain");
        for name in ["", "../private", "C:\\file", "header\r\nvalue"] {
            let mut candidate = ReserveFile {
                name: name.into(),
                ..valid.clone()
            };
            assert!(validate_reservation(&mut candidate).is_err());
        }
        for mime in [
            "text/html; charset=utf-8",
            "text/plain\r\nSet-Cookie: x",
            "/plain",
            "text/",
        ] {
            assert!(
                validate_reservation(&mut ReserveFile {
                    mime: mime.into(),
                    ..valid.clone()
                })
                .is_err()
            );
        }
        assert!(
            validate_reservation(&mut ReserveFile {
                size: MAX_FILE_BYTES + 1,
                ..valid.clone()
            })
            .is_err()
        );
        // A parts reservation goes up to 2 GiB, never empty.
        let large = ReserveFile {
            size: MAX_LARGE_FILE_BYTES,
            transfer: FileTransfer::Parts,
            ..valid.clone()
        };
        assert!(validate_reservation(&mut large.clone()).is_ok());
        for size in [0, MAX_LARGE_FILE_BYTES + 1] {
            assert!(
                validate_reservation(&mut ReserveFile {
                    size,
                    ..large.clone()
                })
                .is_err()
            );
        }
        assert!(attachment("한글\".txt").contains("%22"));
        assert!(!attachment("한글\".txt").contains('한'));
    }

    #[test]
    fn usage_measurements_fail_closed_and_keep_the_floor() {
        assert_eq!(df_available(b"Avail\n 1234\n"), Some(1234));
        assert_eq!(df_available(b"Avail\n -\n"), None);
        assert_eq!(df_available(b"Avail\n 1 2\n"), None);
        let space = DiskSpace {
            bytes: MIN_FREE_BYTES + 10,
            total: MIN_FREE_BYTES * 4,
            inodes: MIN_FREE_INODES + 1,
        };
        assert!(space.require(10, 1).is_ok());
        assert!(space.require(11, 1).is_err());
        assert!(space.require(10, 2).is_err());
        assert!(space.require(u64::MAX, 1).is_err());
        // On a large file system the floor is 5% of it.
        let large = DiskSpace {
            bytes: 100 * MIN_FREE_BYTES,
            total: 1000 * MIN_FREE_BYTES,
            inodes: MIN_FREE_INODES + 1,
        };
        assert!(large.require(50 * MIN_FREE_BYTES, 1).is_ok());
        assert!(large.require(51 * MIN_FREE_BYTES, 1).is_err());
    }

    #[test]
    fn download_ranges_are_one_satisfiable_range_or_the_whole_file() {
        let range = |value: Option<&str>, condition: Option<&str>| {
            let mut headers = HeaderMap::new();
            if let Some(value) = value {
                headers.insert(header::RANGE, value.parse().unwrap());
            }
            if let Some(condition) = condition {
                headers.insert(header::IF_RANGE, condition.parse().unwrap());
            }
            requested_range(&headers, 10, "\"etag\"")
        };
        assert_eq!(range(None, None), Ok(None));
        assert_eq!(range(Some("bytes=2-4"), None), Ok(Some((2, 4))));
        assert_eq!(range(Some("bytes=7-"), None), Ok(Some((7, 9))));
        assert_eq!(range(Some("bytes=-3"), None), Ok(Some((7, 9))));
        assert_eq!(range(Some("bytes=5-100"), None), Ok(Some((5, 9))));
        assert_eq!(range(Some("bytes=0-1,4-5"), None), Ok(None));
        assert_eq!(range(Some("items=0-1"), None), Ok(None));
        assert_eq!(range(Some("bytes=2-4"), Some("\"other\"")), Ok(None));
        assert_eq!(range(Some("bytes=2-4"), Some("\"etag\"")), Ok(Some((2, 4))));
        for bad in ["bytes=10-12", "bytes=4-2", "bytes=-", "bytes=a-b"] {
            assert_eq!(range(Some(bad), None), Err(()), "{bad}");
        }
    }

    #[test]
    fn part_ranges_are_single_contiguous_and_bounded() {
        let range = |value: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_RANGE, value.parse().unwrap());
            content_range(&headers)
        };
        assert_eq!(range("bytes 0-99/1000").unwrap(), (0, 99, 1000));
        assert_eq!(
            range(&format!(
                "bytes 0-{}/{}",
                MAX_PART_BYTES - 1,
                MAX_PART_BYTES * 2
            ))
            .unwrap(),
            (0, MAX_PART_BYTES - 1, MAX_PART_BYTES * 2)
        );
        for bad in [
            "bytes 0-99/99",
            "bytes 10-5/100",
            "bytes */100",
            "bytes 0-99",
            "items 0-99/100",
            "bytes -1-5/100",
            "bytes +1-5/100",
            &format!("bytes 0-{}/{}", MAX_PART_BYTES, MAX_PART_BYTES * 2),
        ] {
            assert!(range(bad).is_err(), "{bad}");
        }
        let mut repeated = HeaderMap::new();
        repeated.append(header::CONTENT_RANGE, "bytes 0-1/10".parse().unwrap());
        repeated.append(header::CONTENT_RANGE, "bytes 0-1/10".parse().unwrap());
        assert!(content_range(&repeated).is_err());
    }

    #[test]
    fn references_are_bounded_single_line_values() {
        assert!(valid_reference("document:1"));
        for value in ["", "  ", "line\nbreak", &"x".repeat(513)] {
            assert!(!valid_reference(value));
        }
    }
}
