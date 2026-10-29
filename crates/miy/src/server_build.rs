//! Durable, bounded source upload and observations for server-side builds.
//! Only the worker executes build commands; reconnects never create duplicate work.
use crate::{
    State,
    auth::Principal,
    domain,
    http::{ApiError, ApiResult},
    source_archive::{MAX_SOURCE_BYTES, valid_commit},
};
use anyhow::{Context, Result, ensure};
use axum::{
    Extension, Json,
    extract::{Path, Query, Request, State as S},
    http::HeaderMap,
};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row, postgres::PgRow};
use std::{os::unix::fs::PermissionsExt, path::PathBuf, time::Duration};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

pub const MAX_LOG_BYTES: usize = 2 * 1024 * 1024;
const MAX_ACTOR_PENDING: i64 = 2;
const MAX_GLOBAL_PENDING: i64 = 8;

pub fn root() -> Result<PathBuf> {
    Ok(PathBuf::from(std::env::var("MIY_ARTIFACTS")?).join(".builds"))
}
pub fn archive_path(id: Uuid) -> Result<PathBuf> {
    Ok(root()?.join(format!("{id}.tar")))
}
fn partial_path(id: Uuid) -> Result<PathBuf> {
    Ok(root()?.join(format!("{id}.part")))
}
async fn prepare_root() -> Result<()> {
    let path = root()?;
    tokio::fs::create_dir_all(&path).await?;
    ensure!(
        !tokio::fs::symlink_metadata(&path)
            .await?
            .file_type()
            .is_symlink(),
        "build_root_is_symlink"
    );
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).await?;
    Ok(())
}

#[derive(Debug, PartialEq)]
struct UploadInput {
    request_id: Uuid,
    source_commit: String,
    archive_hash: String,
    size_bytes: i64,
    platform: String,
}
fn lower_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn header<'a>(headers: &'a HeaderMap, name: &str) -> ApiResult<&'a str> {
    let mut values = headers.get_all(name).iter();
    let value = values
        .next()
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::bad("invalid_build_headers"))?;
    if values.next().is_some() {
        return Err(ApiError::bad("invalid_build_headers"));
    }
    Ok(value)
}
impl UploadInput {
    fn parse(headers: &HeaderMap) -> ApiResult<Self> {
        let source_commit = header(headers, "x-miy-source-commit")?;
        if !valid_commit(source_commit) {
            return Err(ApiError::bad("invalid_source_commit"));
        }
        let archive_hash = header(headers, "x-miy-archive-sha256")?;
        if !lower_hash(archive_hash) {
            return Err(ApiError::bad("invalid_archive_hash"));
        }
        let platform = header(headers, "x-miy-platform")?;
        if !matches!(platform, "linux/amd64" | "linux/arm64") {
            return Err(ApiError::bad("invalid_build_platform"));
        }
        let size = header(headers, "content-length")?
            .parse::<u64>()
            .ok()
            .filter(|n| *n > 0 && *n <= MAX_SOURCE_BYTES)
            .ok_or_else(|| ApiError::bad("source_size_limit_64MiB"))?;
        let request_id = Uuid::parse_str(header(headers, "x-miy-request-id")?)
            .ok()
            .filter(|id| !id.is_nil())
            .ok_or_else(|| ApiError::bad("invalid_build_request_id"))?;
        Ok(Self {
            request_id,
            source_commit: source_commit.into(),
            archive_hash: archive_hash.into(),
            size_bytes: size as i64,
            platform: platform.into(),
        })
    }
    fn matches(&self, row: &PgRow, app: &str) -> bool {
        row.get::<String, _>("app_id") == app
            && row.get::<String, _>("source_commit") == self.source_commit
            && row.get::<String, _>("archive_hash") == self.archive_hash
            && row.get::<i64, _>("size_bytes") == self.size_bytes
            && row.get::<String, _>("platform") == self.platform
    }
}

pub async fn authorize(s: &State, p: &Principal, app: &str) -> ApiResult<()> {
    crate::auth_sessions::ensure_active(&s.pool, &p.id).await?;
    domain::manage(s, p, app, "dev").await?;
    if !sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM apps WHERE id=$1)")
        .bind(app)
        .fetch_one(&s.pool)
        .await?
    {
        return Err(ApiError::not_found());
    }
    Ok(())
}
async fn owned(s: &State, p: &Principal, id: Uuid) -> ApiResult<PgRow> {
    let row = sqlx::query("SELECT * FROM app_builds WHERE id=$1")
        .bind(id)
        .fetch_optional(&s.pool)
        .await?
        .ok_or_else(ApiError::not_found)?;
    authorize(s, p, row.get("app_id")).await?;
    if row.get::<String, _>("actor") != p.id {
        return Err(ApiError::forbidden());
    }
    Ok(row)
}
pub fn view(row: &PgRow) -> Value {
    json!({"id":row.get::<Uuid,_>("id"),"requestId":row.get::<Uuid,_>("request_id"),"canonicalRequestId":row.get::<Uuid,_>("request_id"),
        "actor":row.get::<String,_>("actor"),"userId":row.get::<String,_>("actor"),"appId":row.get::<String,_>("app_id"),
        "action":"build","environment":"dev","status":row.get::<String,_>("status"),"step":row.get::<String,_>("step"),
        "errorCode":row.get::<Option<String>,_>("error_code"),"retryable":false,
        "sourceCommit":row.get::<String,_>("source_commit"),"archiveHash":row.get::<String,_>("archive_hash"),"sizeBytes":row.get::<i64,_>("size_bytes"),
        "platform":row.get::<String,_>("platform"),"imageId":row.get::<Option<String>,_>("image_id"),
        "version":row.get::<Option<String>,_>("version"),"bundleVersion":row.get::<Option<String>,_>("bundle_version"),
        "createdAt":row.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"updatedAt":row.get::<chrono::DateTime<chrono::Utc>,_>("updated_at"),
        "startedAt":row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("started_at"),"finishedAt":row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("finished_at"),
        "logTruncated":row.get::<bool,_>("log_truncated")})
}
fn receipt(row: &PgRow, request: Uuid) -> Value {
    let mut result = view(row);
    result["requestId"] = json!(request);
    result
}

pub async fn upload(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(app): Path<String>,
    req: Request,
) -> ApiResult<Json<Value>> {
    authorize(&s, &p, &app).await?;
    let input = UploadInput::parse(req.headers())?;
    prepare_root().await?;
    let mut tx = s.pool.begin().await?;
    // Short admission lock protects aliases, content deduplication and both caps.
    // The slow stream never holds this transaction or the global queue lock.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('server-build-admission',0))")
        .execute(&mut *tx)
        .await?;
    let previous = sqlx::query("SELECT b.* FROM app_build_requests r JOIN app_builds b ON b.id=r.build_id WHERE r.actor=$1 AND r.request_id=$2")
        .bind(&p.id).bind(input.request_id).fetch_optional(&mut *tx).await?;
    let previous = match previous {
        Some(row) => Some(row),
        None => sqlx::query("SELECT * FROM app_builds WHERE actor=$1 AND app_id=$2 AND source_commit=$3 AND archive_hash=$4 AND platform=$5 AND status<>'failed'")
            .bind(&p.id).bind(&app).bind(&input.source_commit).bind(&input.archive_hash).bind(&input.platform).fetch_optional(&mut *tx).await?,
    };
    if previous
        .as_ref()
        .is_some_and(|row| !input.matches(row, &app))
    {
        return Err(ApiError::conflict("idempotency_conflict"));
    }
    let id = previous
        .as_ref()
        .map(|row| row.get::<Uuid, _>("id"))
        .unwrap_or_else(Uuid::new_v4);
    if previous
        .as_ref()
        .is_none_or(|row| row.get::<String, _>("status") == "receiving")
    {
        let counts = sqlx::query("SELECT count(*) AS total,count(*) FILTER(WHERE actor=$1) AS actor_count FROM app_builds WHERE id<>$2 AND (status IN ('queued','running') OR (status='receiving' AND upload_expires_at>clock_timestamp()))")
            .bind(&p.id).bind(id).fetch_one(&mut *tx).await?;
        if counts.get::<i64, _>("total") >= MAX_GLOBAL_PENDING
            || counts.get::<i64, _>("actor_count") >= MAX_ACTOR_PENDING
        {
            return Err(ApiError::conflict("build_queue_full"));
        }
    }
    if previous.is_none() {
        sqlx::query("INSERT INTO app_builds(id,app_id,actor,request_id,source_commit,archive_hash,size_bytes,platform) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(id).bind(&app).bind(&p.id).bind(input.request_id).bind(&input.source_commit).bind(&input.archive_hash).bind(input.size_bytes).bind(&input.platform).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO app_build_requests(actor,request_id,build_id) VALUES($1,$2,$3) ON CONFLICT(actor,request_id) DO NOTHING")
        .bind(&p.id).bind(input.request_id).bind(id).execute(&mut *tx).await?;
    sqlx::query("UPDATE app_builds SET upload_expires_at=clock_timestamp()+interval '5 minutes' WHERE id=$1 AND status='receiving'")
        .bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    if previous
        .as_ref()
        .is_some_and(|row| row.get::<String, _>("status") != "receiving")
    {
        return Ok(Json(receipt(&owned(&s, &p, id).await?, input.request_id)));
    }
    // A connection-owned lock is released on cancellation/crash. A reconnect can
    // replace a stale partial file, but cannot disturb another active transfer.
    let mut guard = s.pool.acquire().await?;
    guard.close_on_drop();
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtextextended($1,0))")
        .bind(format!("build-upload:{id}"))
        .fetch_one(&mut *guard)
        .await?;
    let row = owned(&s, &p, id).await?;
    if !acquired || row.get::<String, _>("status") != "receiving" {
        return Ok(Json(receipt(&row, input.request_id)));
    }
    let archive = archive_path(id)?;
    // A crash after atomic publication but before queuing reuses verified bytes.
    let ready = verified_archive(&archive, input.size_bytes, &input.archive_hash).await?;
    if !ready {
        let part = partial_path(id)?;
        sqlx::query("SELECT 1").execute(&mut *guard).await?;
        let _ = tokio::fs::remove_file(&part).await;
        let _ = tokio::fs::remove_file(&archive).await;
        sqlx::query(
            "UPDATE app_builds SET step='receiving',error_code=NULL,updated_at=now() WHERE id=$1",
        )
        .bind(id)
        .execute(&mut *guard)
        .await?;
        let transfer = async {
            let mut file = tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&part)
                .await?;
            let mut stream = req.into_body().into_data_stream();
            let mut hash = Sha256::new();
            let mut bytes = 0u64;
            let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
            heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                let chunk = tokio::select! {
                    chunk = stream.next() => chunk,
                    _ = heartbeat.tick() => {
                        // A lost PostgreSQL session also loses its upload lock.
                        // Stop writing promptly if another receiver can take over.
                        sqlx::query("SELECT 1").execute(&mut *guard).await?;
                        continue;
                    }
                };
                let Some(chunk) = chunk else { break };
                let chunk = chunk?;
                bytes += chunk.len() as u64;
                ensure!(bytes <= input.size_bytes as u64, "source_size_mismatch");
                hash.update(&chunk);
                file.write_all(&chunk).await?;
            }
            ensure!(
                bytes == input.size_bytes as u64
                    && hex::encode(hash.finalize()) == input.archive_hash,
                "source_hash_mismatch"
            );
            file.sync_all().await?;
            drop(file);
            sqlx::query("SELECT 1").execute(&mut *guard).await?;
            tokio::fs::rename(&part, &archive).await?;
            let directory = root()?;
            tokio::task::spawn_blocking(move || std::fs::File::open(directory)?.sync_all())
                .await??;
            Ok::<_, anyhow::Error>(())
        };
        if !matches!(
            tokio::time::timeout(Duration::from_secs(300), transfer).await,
            Ok(Ok(()))
        ) {
            // Never remove shared paths after losing the lock connection: a
            // reconnect may already own them. Its upload replaces stale bytes.
            sqlx::query("SELECT 1").execute(&mut *guard).await?;
            let _ = tokio::fs::remove_file(part).await;
            sqlx::query("UPDATE app_builds SET step='upload_incomplete',error_code='invalid_source_archive',upload_expires_at=clock_timestamp(),updated_at=now() WHERE id=$1 AND status='receiving'").bind(id).execute(&mut *guard).await?;
            return Err(ApiError::bad("invalid_source_archive"));
        }
    }
    // Revocation while uploading cannot enqueue work under the old grant.
    authorize(&s, &p, &app).await?;
    sqlx::query("UPDATE app_builds SET status='queued',step='queued',error_code=NULL,updated_at=now() WHERE id=$1 AND status='receiving'").bind(id).execute(&mut *guard).await?;
    Ok(Json(receipt(&owned(&s, &p, id).await?, input.request_id)))
}
async fn verified_archive(path: &std::path::Path, size: i64, hash: &str) -> Result<bool> {
    let metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file()
        || metadata.len() != size as u64
        || metadata.len() > MAX_SOURCE_BYTES
    {
        return Ok(false);
    }
    let path = path.to_owned();
    Ok(tokio::task::spawn_blocking(move || crate::source_archive::digest(&path)).await?? == hash)
}

pub async fn status(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    Ok(Json(view(&owned(&s, &p, id).await?)))
}
pub async fn by_request(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path((app, request)): Path<(String, Uuid)>,
) -> ApiResult<Json<Value>> {
    authorize(&s, &p, &app).await?;
    let id: Uuid = sqlx::query_scalar("SELECT r.build_id FROM app_build_requests r JOIN app_builds b ON b.id=r.build_id WHERE r.actor=$1 AND r.request_id=$2 AND b.app_id=$3")
        .bind(&p.id).bind(request).bind(&app).fetch_optional(&s.pool).await?.ok_or_else(ApiError::not_found)?;
    Ok(Json(receipt(&owned(&s, &p, id).await?, request)))
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Logs {
    after: Option<i64>,
    limit: Option<i64>,
}
pub async fn logs(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
    Query(q): Query<Logs>,
) -> ApiResult<Json<Value>> {
    owned(&s, &p, id).await?;
    let after = q.after.unwrap_or(0);
    let limit = q.limit.unwrap_or(100);
    if after < 0 || !(1..=200).contains(&limit) {
        return Err(ApiError::bad("invalid_build_log_cursor"));
    }
    let mut tx = s.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let row = sqlx::query("SELECT * FROM app_builds WHERE id=$1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let maximum: i64 = row.get("log_sequence");
    if after > maximum {
        return Err(ApiError::bad("invalid_build_log_cursor"));
    }
    let rows = sqlx::query("SELECT sequence,created_at,text FROM app_build_logs WHERE build_id=$1 AND sequence>$2 ORDER BY sequence LIMIT $3")
        .bind(id).bind(after).bind(limit).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let next = rows
        .last()
        .map(|r| r.get::<i64, _>("sequence"))
        .unwrap_or(after);
    let lines: Vec<Value> = rows.into_iter().map(|r| json!({"sequence":r.get::<i64,_>("sequence"),"createdAt":r.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"text":r.get::<String,_>("text")})).collect();
    let status: String = row.get("status");
    Ok(Json(
        json!({"id":id,"jobId":id,"logs":lines,"nextSequence":next,"maxBytes":MAX_LOG_BYTES,"truncated":row.get::<bool,_>("log_truncated"),"terminal":matches!(status.as_str(),"succeeded"|"failed") && next==maximum,"status":status}),
    ))
}
fn bounded_text(text: &str, remaining: usize) -> &str {
    let mut end = text.len().min(remaining);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
pub async fn append_log(pool: &PgPool, id: Uuid, text: &str) -> Result<()> {
    // PostgreSQL text cannot contain NUL; terminal controls are not useful in
    // the public plain-text stream. Keep only ordinary whitespace controls.
    let text: String = text
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\r' | '\t'))
        .collect();
    if text.is_empty() {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    let row = sqlx::query("SELECT log_bytes,log_sequence FROM app_builds WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let used: i64 = row.get("log_bytes");
    let sequence: i64 = row.get("log_sequence");
    let part = bounded_text(
        &text,
        MAX_LOG_BYTES.saturating_sub(usize::try_from(used).context("invalid_build_log_size")?),
    );
    if !part.is_empty() {
        sqlx::query("INSERT INTO app_build_logs(build_id,sequence,text) VALUES($1,$2,$3)")
            .bind(id)
            .bind(sequence + 1)
            .bind(part)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE app_builds SET log_bytes=log_bytes+$2,log_sequence=log_sequence+$3,log_truncated=log_truncated OR $4 WHERE id=$1")
        .bind(id).bind(part.len() as i64).bind(i64::from(!part.is_empty())).bind(part.len()!=text.len()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn headers() -> HeaderMap {
        let mut h = HeaderMap::new();
        for (key, value) in [
            ("x-miy-source-commit", "a".repeat(40)),
            ("x-miy-archive-sha256", "b".repeat(64)),
            ("x-miy-platform", "linux/amd64".into()),
            ("content-length", "1024".into()),
            ("x-miy-request-id", Uuid::new_v4().to_string()),
        ] {
            h.insert(key, value.parse().unwrap());
        }
        h
    }
    #[test]
    fn upload_headers_bind_commit_hash_platform_size_and_request() {
        let mut h = headers();
        assert!(UploadInput::parse(&h).is_ok());
        for platform in ["linux/amd64", "linux/arm64"] {
            h.insert("x-miy-platform", platform.parse().unwrap());
            assert_eq!(UploadInput::parse(&h).unwrap().platform, platform);
        }
        for (key, value) in [
            ("x-miy-source-commit", "A".repeat(40)),
            ("x-miy-archive-sha256", "g".repeat(64)),
            ("x-miy-platform", "windows/amd64".into()),
            ("content-length", "0".into()),
            ("content-length", (MAX_SOURCE_BYTES + 1).to_string()),
            ("content-length", "-1".into()),
            ("x-miy-request-id", Uuid::nil().to_string()),
        ] {
            let mut invalid = headers();
            invalid.insert(key, value.parse().unwrap());
            assert!(UploadInput::parse(&invalid).is_err(), "{key}");
        }
        let mut maximum = headers();
        maximum.insert(
            "content-length",
            MAX_SOURCE_BYTES.to_string().parse().unwrap(),
        );
        assert!(UploadInput::parse(&maximum).is_ok());
    }
    #[test]
    fn missing_and_duplicate_identity_headers_are_rejected() {
        for name in [
            "content-length",
            "x-miy-source-commit",
            "x-miy-archive-sha256",
            "x-miy-platform",
            "x-miy-request-id",
        ] {
            let mut missing = headers();
            missing.remove(name);
            assert!(UploadInput::parse(&missing).is_err());
            let mut duplicate = headers();
            duplicate.append(name, "different".parse().unwrap());
            assert!(UploadInput::parse(&duplicate).is_err());
        }
    }
    #[test]
    fn log_limits_never_split_utf8_or_exceed_remaining_bytes() {
        assert_eq!(bounded_text("가나다", 0), "");
        assert_eq!(bounded_text("가나다", 2), "");
        assert_eq!(bounded_text("가나다", 5), "가");
        assert_eq!(bounded_text("a📝b", 5), "a📝");
        assert_eq!(bounded_text("a📝b", 6), "a📝b");
        let long = "한".repeat(MAX_LOG_BYTES / 3 + 1);
        let bounded = bounded_text(&long, MAX_LOG_BYTES);
        assert!(bounded.len() <= MAX_LOG_BYTES && long.starts_with(bounded));
    }
}
