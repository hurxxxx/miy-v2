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
    storage_contract::{AttachFile, DetachFile, FileStatus, ReserveFile, StoredFile},
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
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Semaphore, SemaphorePermit},
};
use uuid::Uuid;

pub use crate::storage_contract::{MAX_FILE_BYTES, TRANSFER_SECONDS, allowed_route};
const MIN_FREE_BYTES: u64 = 512 * 1024 * 1024;
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
    Ok(StoredFile {
        id: row.get("id"),
        version: 1,
        installation_id: row.get("installation_id"),
        request_id: row.get("request_id"),
        name: row.get("name"),
        mime: row.get("mime"),
        size: row.get::<i64, _>("size") as u64,
        sha256: row.get("sha256"),
        status,
        reference: row.get("reference"),
        expires_at,
    })
}

fn validate_reservation(value: &mut ReserveFile) -> ApiResult<()> {
    value.mime.make_ascii_lowercase();
    if value.request_id.is_nil()
        || value.size > MAX_FILE_BYTES
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
    inodes: u64,
}

impl DiskSpace {
    async fn measure(path: &FsPath) -> ApiResult<Self> {
        Ok(Self {
            bytes: df(path, &["--output=avail", "-B1"]).await?,
            inodes: df(path, &["--output=iavail"]).await?,
        })
    }

    /// The floor must remain after `bytes` in `files` promised files arrive.
    fn require(&self, bytes: u64, files: u64) -> ApiResult<()> {
        if self.bytes < MIN_FREE_BYTES.saturating_add(bytes)
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
    let input_hash = crate::contract::hash(&serde_json::to_vec(&(
        &p.id,
        &value.name,
        &value.mime,
        value.size,
    ))?);
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
    let quota = sqlx::query("SELECT COALESCE(sum(size),0)::bigint AS total, COALESCE(sum(size) FILTER(WHERE installation_id=$1),0)::bigint AS owned, COALESCE(sum(size) FILTER(WHERE status IN ('pending','uploading')),0)::bigint AS pending, count(*) AS files, count(*) FILTER(WHERE installation_id=$1) AS owned_files, count(*) FILTER(WHERE status IN ('pending','uploading')) AS pending_files FROM app_files WHERE status IN ('ready','attached','cancelling') OR (status IN ('pending','uploading') AND expires_at>clock_timestamp())")
        .bind(id).fetch_one(&mut *tx).await?;
    if quota.get::<i64, _>("total") as u64 + value.size > crate::storage_contract::TOTAL_BYTES
        || quota.get::<i64, _>("owned") as u64 + value.size
            > crate::storage_contract::INSTALLATION_BYTES
        || quota.get::<i64, _>("files") >= 10_000
        || quota.get::<i64, _>("owned_files") >= 1_000
    {
        return Err(ApiError::conflict("file_quota_exceeded"));
    }
    space?.require(
        quota.get::<i64, _>("pending") as u64 + value.size,
        quota.get::<i64, _>("pending_files") as u64 + 1,
    )?;
    let file = Uuid::new_v4();
    let row = sqlx::query("INSERT INTO app_files(id,installation_id,request_id,created_by,input_hash,name,mime,size,status,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'pending',clock_timestamp()+interval '24 hours') RETURNING *")
        .bind(file).bind(id).bind(value.request_id).bind(&p.id).bind(input_hash).bind(value.name).bind(value.mime).bind(value.size as i64).fetch_one(&mut *tx).await?;
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
        let row = sqlx::query("UPDATE app_files SET status='ready',sha256=$2,expires_at=NULL,updated_at=clock_timestamp() WHERE id=$1 RETURNING *")
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
                ..valid
            })
            .is_err()
        );
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
            inodes: MIN_FREE_INODES + 1,
        };
        assert!(space.require(10, 1).is_ok());
        assert!(space.require(11, 1).is_err());
        assert!(space.require(10, 2).is_err());
        assert!(space.require(u64::MAX, 1).is_err());
    }

    #[test]
    fn references_are_bounded_single_line_values() {
        assert!(valid_reference("document:1"));
        for value in ["", "  ", "line\nbreak", &"x".repeat(513)] {
            assert!(!valid_reference(value));
        }
    }
}
