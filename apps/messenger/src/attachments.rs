//! Attachments: AI-DO domains/dm/attachment_policy.py, attachment_records.py,
//! attachment_application.py and attachment_links.py, on the platform
//! Storage API (`Identity::files_*`, 10 MiB per file).
//!
//! AI-DO received a multipart upload and stored the bytes itself, then handed
//! out signed URLs. miy:
//! 1. `POST /v1/dm/conversations/{id}/attachments` checks the conversation
//!    ACL, block, terms and file name, persists the reservation intent and
//!    reserves Storage (`files_create`).
//! 2. The bytes stream through the file gateway (`runtime.fileApi`) to
//!    `PUT /files/dm/attachments/{id}/content` (`files_upload`); the first
//!    512 bytes decide the content type as AI-DO's sniffing did.
//! 3. Sending links the row to the message, commits, then `files_attach`.
//! 4. Downloads go through `GET /files/dm/attachments/{id}/content`
//!    (`files_download`) after the ACL check; there is no public URL.
//! 5. Deleting a message releases its files (`files_detach`) after commit,
//!    unless a report on the message is open.
use crate::{
    error::{ApiError, ApiJson, ApiResult},
    request::{is_uuid, route_id},
    safety,
    schemas::{DmAttachmentReserveRequest, DmAttachmentUrlResponse, DmMessageAttachmentItem},
    store::{self, AttachmentRow, new_id},
    time::now_utc,
    ugc, views,
};
use axum::{
    Extension, Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::Response,
};
use futures_util::StreamExt;
use miy_app_sdk::{FileCreate, FileStatus, Identity, storage::MAX_FILE_BYTES};
use serde::Deserialize;
use sqlx::PgPool;
use std::sync::{Arc, Mutex};

pub const DM_MAX_ATTACHMENT_SIZE: i64 = MAX_FILE_BYTES as i64;
pub const DM_ATTACHMENT_SNIFF_BYTES: usize = 512;
pub const DEFAULT_ATTACHMENT_CONTENT_TYPE: &str = "application/octet-stream";
const PREVIEWABLE: [&str; 4] = ["image/gif", "image/jpeg", "image/png", "image/webp"];

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[schema(value_type = String, format = Binary)]
pub struct FileBytes(Vec<u8>);

// --- attachment_policy ----------------------------------------------------------

fn valid_token(part: &str) -> bool {
    let mut chars = part.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "!#$&^_.+-".contains(c))
}

/// `normalize_content_type`.
pub fn normalize_content_type(value: Option<&str>) -> String {
    let content_type = value
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match content_type.split_once('/') {
        Some((kind, sub)) if valid_token(kind) && valid_token(sub) => content_type,
        _ => DEFAULT_ATTACHMENT_CONTENT_TYPE.to_string(),
    }
}

pub fn is_previewable_image_content_type(content_type: &str) -> bool {
    PREVIEWABLE.contains(&normalize_content_type(Some(content_type)).as_str())
}

/// `sniff_content_type`.
pub fn sniff_content_type(prefix: &[u8]) -> Option<&'static str> {
    if prefix.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if prefix.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if prefix.starts_with(b"GIF87a") || prefix.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if prefix.len() >= 12 && prefix.starts_with(b"RIFF") && &prefix[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// `prepare_upload_decision`: a sniffed image wins; a declared image that
/// does not sniff as one is downgraded to the default type.
pub fn decide_content_type(declared: Option<&str>, sniffed: Option<&str>) -> String {
    let normalized = normalize_content_type(declared);
    if let Some(sniffed) = sniffed {
        return sniffed.to_string();
    }
    if normalized.starts_with("image/") {
        return DEFAULT_ATTACHMENT_CONTENT_TYPE.to_string();
    }
    normalized
}

/// `safe_filename`.
pub fn safe_filename(value: Option<&str>) -> String {
    let raw = value.unwrap_or("unnamed").trim().replace('\\', "/");
    let mut filename = raw
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    // `^[A-Za-z]:+`
    let bytes = filename.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        filename = filename[1..].trim_start_matches(':').trim().to_string();
    }
    let mut cleaned = String::new();
    let mut in_run = false;
    for c in filename.chars() {
        let unsafe_char = (c as u32) < 0x20 || c == '\u{7f}' || "<>:\"|?*/\\".contains(c);
        if unsafe_char {
            if !in_run {
                cleaned.push('_');
            }
            in_run = true;
        } else {
            cleaned.push(c);
            in_run = false;
        }
    }
    let cleaned = cleaned.trim_matches(|c| c == ' ' || c == '.').to_string();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        return "unnamed".to_string();
    }
    cleaned.chars().take(512).collect()
}

fn size_limit_error() -> ApiError {
    ApiError::new(
        StatusCode::PAYLOAD_TOO_LARGE,
        "dm.attachment_size_limit_exceeded",
        "Attachments must be 10MB or smaller.",
    )
}

fn storage_failed(error: miy_app_sdk::CallError) -> ApiError {
    eprintln!("storage_call_failed: {} {:?}", error.code, error.status);
    ApiError::unavailable(
        "dm.attachment_upload_failed",
        "The attachment could not be uploaded.",
    )
}

// --- reservation ------------------------------------------------------------------

/// `POST /v1/dm/conversations/{id}/attachments` (miy: JSON reservation).
#[utoipa::path(
    post, path = "/v1/dm/conversations/{conversation_id}/attachments", operation_id = "reserveDmAttachment",
    description = "Reserves one file up to 10 MiB. Use the same request_id when a reservation reply is lost.",
    params(
        ("conversation_id" = String, Path),
    ),
    request_body = DmAttachmentReserveRequest,
    responses(
        (status = 201, description = "Created", body = DmMessageAttachmentItem),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 409, description = "Current state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 413, description = "Attachment exceeds the 10 MiB limit", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation or domain policy rejected the operation", body = crate::error::ErrorResponse),
        (status = 502, description = "A required upstream service failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn reserve_dm_attachment(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(conversation_id): Path<String>,
    ApiJson(request): ApiJson<DmAttachmentReserveRequest>,
) -> ApiResult<(StatusCode, Json<DmMessageAttachmentItem>)> {
    route_id(&conversation_id)?;
    if !is_uuid(&request.request_id) {
        return Err(ApiError::validation("request_id: Input should be a UUID"));
    }
    let request_id = uuid::Uuid::parse_str(&request.request_id)
        .map_err(|_| ApiError::validation("request_id"))?;
    let me = identity.user_id.as_str();
    let mut conn = pool.acquire().await?;
    let conversation =
        store::require_user_conversation(&mut conn, me, &conversation_id, false).await?;
    safety::ensure_direct_conversation_open(&mut conn, &conversation, me).await?;
    ugc::ensure_user_content_allowed(&mut conn, me, &[request.filename.as_str()]).await?;
    if request.size_bytes <= 0 {
        return Err(ApiError::unprocessable(
            "dm.attachment_empty",
            "Empty files cannot be attached.",
        ));
    }
    if request.size_bytes > DM_MAX_ATTACHMENT_SIZE {
        return Err(size_limit_error());
    }
    let filename = safe_filename(Some(&request.filename));
    let content_type = decide_content_type(request.content_type.as_deref(), None);
    sqlx::query(
        "INSERT INTO dm_message_attachments (id, conversation_id, message_id, uploader_id, filename, \
           content_type, size_bytes, created_at, storage_request_id, storage_status) \
         VALUES ($1, $2, NULL, $3, $4, $5, $6, $7, $8, 'reserved') \
         ON CONFLICT (uploader_id, storage_request_id) DO NOTHING",
    )
    .bind(new_id())
    .bind(&conversation_id)
    .bind(me)
    .bind(&filename)
    .bind(&content_type)
    .bind(request.size_bytes as i32)
    .bind(now_utc())
    .bind(request_id)
    .execute(&mut *conn)
    .await?;
    let row: AttachmentRow = sqlx::query_as(&format!(
        "SELECT {} FROM dm_message_attachments WHERE uploader_id = $1 AND storage_request_id = $2",
        store::ATTACHMENT_COLUMNS
    ))
    .bind(me)
    .bind(request_id)
    .fetch_one(&mut *conn)
    .await?;
    if row.conversation_id != conversation_id
        || row.filename != filename
        || i64::from(row.size_bytes) != request.size_bytes
    {
        return Err(ApiError::conflict(
            "dm.attachment_request_conflict",
            "This request ID was used for another attachment.",
        ));
    }
    let row = if row.storage_file_id.is_none() {
        let reserved = match identity
            .files_create(&FileCreate {
                request_id: request_id.to_string(),
                name: row.filename.clone(),
                mime: DEFAULT_ATTACHMENT_CONTENT_TYPE.to_string(),
                size: row.size_bytes as u64,
            })
            .await
        {
            Ok(file) => file,
            // An uncertain result is looked up by its request ID, never resent blindly.
            Err(_) => identity
                .files_by_request(&request_id.to_string())
                .await
                .map_err(storage_failed)?,
        };
        let file_id = uuid::Uuid::parse_str(&reserved.id).map_err(|_| ApiError::internal())?;
        sqlx::query("UPDATE dm_message_attachments SET storage_file_id = $2 WHERE id = $1")
            .bind(&row.id)
            .bind(file_id)
            .execute(&mut *conn)
            .await?;
        store::attachment_by_id(&mut conn, &row.id, false)
            .await?
            .ok_or_else(ApiError::internal)?
    } else {
        row
    };
    Ok((StatusCode::CREATED, Json(views::attachment_item(&row))))
}

/// `PUT /files/dm/attachments/{id}/content`: the bytes of a reservation.
#[utoipa::path(
    put, path = "/files/dm/attachments/{attachment_id}/content", operation_id = "uploadDmAttachmentContent",
    description = "Streams the reserved bytes for the uploader. Content-Length must match the reservation.",
    params(
        ("attachment_id" = String, Path),
        ("Content-Length" = i64, Header, minimum = 1, maximum = 10485760, description = "Must equal the reserved size"),
    ),
    request_body(content = inline(FileBytes), content_type = "*/*"),
    responses(
        (status = 200, description = "Success", body = DmMessageAttachmentItem),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation or domain policy rejected the operation", body = crate::error::ErrorResponse),
        (status = 502, description = "A required upstream service failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn upload_dm_attachment_content(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(attachment_id): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> ApiResult<Json<DmMessageAttachmentItem>> {
    route_id(&attachment_id)?;
    let me = identity.user_id.as_str();
    let mut conn = pool.acquire().await?;
    let row = store::attachment_by_id(&mut conn, &attachment_id, false)
        .await?
        .filter(|row| row.uploader_id == me && row.message_id.is_none())
        .ok_or_else(ApiError::attachment_not_found)?;
    // The uploader must still be in the conversation and the 1:1 open.
    let conversation =
        store::require_user_conversation(&mut conn, me, &row.conversation_id, false).await?;
    safety::ensure_direct_conversation_open(&mut conn, &conversation, me).await?;
    drop(conn);
    let length = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok());
    if length != Some(i64::from(row.size_bytes)) {
        return Err(ApiError::unprocessable(
            "dm.attachment_size_mismatch",
            "The file size does not match the reservation.",
        ));
    }
    if row.storage_status == "uploaded" {
        return Ok(Json(views::attachment_item(&row)));
    }
    if row.storage_status != "reserved" {
        return Err(ApiError::attachment_not_found());
    }
    let file_id = row
        .storage_file_id
        .ok_or_else(ApiError::attachment_not_found)?;
    let file = identity
        .files_get(&file_id.to_string())
        .await
        .map_err(storage_failed)?;
    let (file, content_type) = if file.status == FileStatus::Pending {
        let prefix = Arc::new(Mutex::new(Vec::with_capacity(DM_ATTACHMENT_SNIFF_BYTES)));
        let seen = prefix.clone();
        let stream = body.into_data_stream().map(move |chunk| {
            if let Ok(bytes) = &chunk
                && let Ok(mut prefix) = seen.lock()
                && prefix.len() < DM_ATTACHMENT_SNIFF_BYTES
            {
                let take = (DM_ATTACHMENT_SNIFF_BYTES - prefix.len()).min(bytes.len());
                prefix.extend_from_slice(&bytes[..take]);
            }
            chunk
        });
        let uploaded = match identity
            .files_upload(&file, Body::from_stream(stream))
            .await
        {
            Ok(file) => file,
            Err(error) => identity
                .files_get(&file.id)
                .await
                .ok()
                .filter(|file| file.status == FileStatus::Ready)
                .ok_or_else(|| storage_failed(error))?,
        };
        let sniffed = prefix
            .lock()
            .ok()
            .and_then(|prefix| sniff_content_type(&prefix));
        (
            uploaded,
            decide_content_type(Some(&row.content_type), sniffed),
        )
    } else {
        // An earlier request committed these bytes without recording its sniffed type.
        (file, row.content_type.clone())
    };
    if file.status != FileStatus::Ready {
        return Err(ApiError::unavailable(
            "dm.attachment_upload_failed",
            "The attachment could not be uploaded.",
        ));
    }
    sqlx::query(
        "UPDATE dm_message_attachments SET storage_status = 'uploaded', content_type = $2 \
         WHERE id = $1 AND storage_status = 'reserved'",
    )
    .bind(&row.id)
    .bind(&content_type)
    .execute(&pool)
    .await?;
    let mut conn = pool.acquire().await?;
    let row = store::attachment_by_id(&mut conn, &attachment_id, false)
        .await?
        .filter(|row| row.storage_status == "uploaded" && row.message_id.is_none())
        .ok_or_else(ApiError::attachment_not_found)?;
    Ok(Json(views::attachment_item(&row)))
}

/// miy: `DELETE /v1/dm/attachments/{id}` cancels an unsent attachment the
/// caller reserved (AI-DO left removed composer files unreferenced).
#[utoipa::path(
    delete, path = "/v1/dm/attachments/{attachment_id}", operation_id = "cancelDmAttachment",
    params(
        ("attachment_id" = String, Path),
    ),
    responses(
        (status = 204, description = "No content"),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation or domain policy rejected the operation", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn cancel_dm_attachment(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(attachment_id): Path<String>,
) -> ApiResult<StatusCode> {
    route_id(&attachment_id)?;
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    let row = store::attachment_by_id(&mut tx, &attachment_id, true)
        .await?
        .filter(|row| row.uploader_id == me)
        .ok_or_else(ApiError::attachment_not_found)?;
    if row.message_id.is_some() {
        return Err(ApiError::unprocessable(
            "dm.attachment_already_sent",
            "This attachment was already sent.",
        ));
    }
    if row.storage_status == "cancelled" {
        return Ok(StatusCode::NO_CONTENT);
    }
    sqlx::query("UPDATE dm_message_attachments SET storage_status = 'cancelled', released_at = $2 WHERE id = $1")
        .bind(&row.id)
        .bind(now_utc())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    if let Some(file_id) = row.storage_file_id
        && let Err(error) = identity.files_cancel(&file_id.to_string()).await
    {
        // An unattached reservation also expires on its own (pending) or stays
        // ready without a reference; nothing is retried automatically.
        eprintln!("storage_cancel_failed: {} {:?}", error.code, error.status);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// After a send committed the message link: `files_attach` per file.
pub async fn attach_linked(pool: &PgPool, identity: &Identity, attachment_ids: &[String]) {
    for id in attachment_ids {
        let Ok(mut conn) = pool.acquire().await else {
            return;
        };
        let Ok(Some(row)) = store::attachment_by_id(&mut conn, id, false).await else {
            continue;
        };
        drop(conn);
        attach_one(pool, identity, &row).await;
    }
}

async fn attach_one(pool: &PgPool, identity: &Identity, row: &AttachmentRow) {
    let (Some(file_id), Some(request_id)) = (row.storage_file_id, row.attach_request_id) else {
        return;
    };
    if row.storage_status != "uploaded" {
        return;
    }
    match identity
        .files_attach(
            &file_id.to_string(),
            &row.reference(),
            &request_id.to_string(),
        )
        .await
    {
        Ok(_) => {
            let _ = sqlx::query(
                "UPDATE dm_message_attachments SET storage_status = 'attached' WHERE id = $1 AND storage_status = 'uploaded'",
            )
            .bind(&row.id)
            .execute(pool)
            .await;
        }
        // The file stays ready (retained); the next download tries again.
        Err(error) => eprintln!("storage_attach_failed: {} {:?}", error.code, error.status),
    }
}

/// Release the files of a deleted message (no open report): detach attached
/// files, cancel ones never attached. The durable record of pending work is
/// the row itself (message deleted, status still `uploaded`/`attached`, no open
/// report); [`release_pending_files`] retries such rows later.
pub async fn release_deleted_message_files(pool: &PgPool, identity: &Identity, message_id: &str) {
    release_where(pool, identity, Some(message_id), 50).await;
}

/// Retry a few releases that failed earlier (any conversation of this
/// installation). Called on message delete and report resolution; there is
/// no background worker before the platform offers a service principal.
pub async fn release_pending_files(pool: &PgPool, identity: &Identity) {
    release_where(pool, identity, None, 10).await;
}

/// While a release is running, `released_at` of a row still `uploaded` or
/// `attached` holds the claim time, so concurrent passes (a request's own
/// release and another request's sweep) never call Storage twice for one
/// file. A claim older than RELEASE_CLAIM_SECONDS is taken over.
const RELEASE_CLAIM_SECONDS: i64 = 60;

async fn release_where(pool: &PgPool, identity: &Identity, message_id: Option<&str>, limit: i64) {
    let stale = now_utc() - chrono::Duration::seconds(RELEASE_CLAIM_SECONDS);
    let rows: Result<Vec<AttachmentRow>, sqlx::Error> = sqlx::query_as(&format!(
        "SELECT a.{} FROM dm_message_attachments a JOIN dm_messages m ON m.id = a.message_id \
         WHERE ($1::text IS NULL OR a.message_id = $1) AND m.deleted_at IS NOT NULL \
         AND a.storage_status IN ('uploaded', 'attached') \
         AND (a.released_at IS NULL OR a.released_at < $3) \
         AND NOT EXISTS (SELECT 1 FROM dm_abuse_reports r WHERE r.message_id = m.id AND r.status = 'open') \
         ORDER BY m.deleted_at LIMIT $2",
        store::ATTACHMENT_COLUMNS.replace(", ", ", a.")
    ))
    .bind(message_id)
    .bind(limit)
    .bind(stale)
    .fetch_all(pool)
    .await;
    let Ok(rows) = rows else {
        eprintln!("storage_release_query_failed");
        return;
    };
    for row in rows {
        let Some(file_id) = row.storage_file_id else {
            continue;
        };
        let claimed = sqlx::query(
            "UPDATE dm_message_attachments SET released_at = $2 WHERE id = $1 \
             AND storage_status IN ('uploaded', 'attached') AND (released_at IS NULL OR released_at < $3)",
        )
        .bind(&row.id)
        .bind(now_utc())
        .bind(stale)
        .execute(pool)
        .await
        .map(|result| result.rows_affected() == 1)
        .unwrap_or(false);
        if !claimed {
            continue;
        }
        let file_id = file_id.to_string();
        let storage = if row.storage_status == "attached" {
            FileStatus::Attached
        } else {
            match identity.files_get(&file_id).await {
                Ok(file)
                    if file.status == FileStatus::Attached
                        && file.reference.as_deref() != Some(&row.reference()) =>
                {
                    unclaim(pool, &row.id).await;
                    continue;
                }
                Ok(file) => file.status,
                Err(error) => {
                    eprintln!(
                        "storage_release_lookup_failed: {} {:?}",
                        error.code, error.status
                    );
                    unclaim(pool, &row.id).await;
                    continue;
                }
            }
        };
        let (result, status) = match storage {
            FileStatus::Attached => (
                identity
                    .files_detach(&file_id, &row.reference())
                    .await
                    .map(|_| ()),
                "released",
            ),
            // Already gone: a concurrent release (this request's own pass and a
            // sweep of another request) finished first. Record it, call nothing.
            FileStatus::Cancelled | FileStatus::Expired => (Ok(()), "cancelled"),
            FileStatus::Pending | FileStatus::Ready => (
                identity.files_cancel(&file_id).await.map(|_| ()),
                "cancelled",
            ),
        };
        match result {
            Ok(_) => {
                let _ = sqlx::query(
                    "UPDATE dm_message_attachments SET storage_status = $2, released_at = $3 \
                     WHERE id = $1 AND storage_status IN ('uploaded', 'attached')",
                )
                .bind(&row.id)
                .bind(status)
                .bind(now_utc())
                .execute(pool)
                .await;
            }
            Err(error) => {
                eprintln!("storage_release_failed: {} {:?}", error.code, error.status);
                unclaim(pool, &row.id).await;
            }
        }
    }
}

/// Give a claimed release back for a later retry.
async fn unclaim(pool: &PgPool, id: &str) {
    let _ = sqlx::query(
        "UPDATE dm_message_attachments SET released_at = NULL WHERE id = $1 \
         AND storage_status IN ('uploaded', 'attached')",
    )
    .bind(id)
    .execute(pool)
    .await;
}

// --- downloads --------------------------------------------------------------------

/// `DmAttachmentAccessGate.require_attachment`, stricter for miy: an unsent
/// file only for its uploader, a sent one only for participants who see the
/// message (joined before it, not hidden, not deleted).
async fn require_attachment_access(
    pool: &PgPool,
    me: &str,
    attachment_id: &str,
) -> ApiResult<AttachmentRow> {
    route_id(attachment_id)?;
    let mut conn = pool.acquire().await?;
    let row = store::attachment_by_id(&mut conn, attachment_id, false)
        .await?
        .ok_or_else(ApiError::attachment_not_found)?;
    let conversation =
        store::require_user_conversation(&mut conn, me, &row.conversation_id, false).await?;
    let participant = conversation.require_participant(me)?;
    match &row.message_id {
        None => {
            if row.uploader_id != me {
                return Err(ApiError::attachment_not_found());
            }
        }
        Some(message_id) => {
            let message = store::message_by_id(&mut conn, &row.conversation_id, message_id, false)
                .await?
                .ok_or_else(ApiError::attachment_not_found)?;
            if message.hidden() || message.deleted() || message.created_at < participant.joined_at {
                return Err(ApiError::attachment_not_found());
            }
        }
    }
    if !matches!(row.storage_status.as_str(), "uploaded" | "attached") {
        return Err(ApiError::attachment_not_found());
    }
    Ok(row)
}

/// `GET /v1/dm/attachments/{id}/download`.
#[utoipa::path(
    get, path = "/v1/dm/attachments/{attachment_id}/download", operation_id = "getDmAttachmentDownloadUrl",
    params(
        ("attachment_id" = String, Path),
    ),
    responses(
        (status = 200, description = "Success", body = DmAttachmentUrlResponse),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation or domain policy rejected the operation", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn get_dm_attachment_download_url(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(attachment_id): Path<String>,
) -> ApiResult<Json<DmAttachmentUrlResponse>> {
    let row = require_attachment_access(&pool, &identity.user_id, &attachment_id).await?;
    Ok(Json(DmAttachmentUrlResponse {
        url: views::attachment_content_path(&row.id),
    }))
}

/// `GET /v1/dm/attachments/{id}/preview`: images only (415 otherwise).
#[utoipa::path(
    get, path = "/v1/dm/attachments/{attachment_id}/preview", operation_id = "getDmAttachmentPreviewUrl",
    params(
        ("attachment_id" = String, Path),
    ),
    responses(
        (status = 200, description = "Success", body = DmAttachmentUrlResponse),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 415, description = "Attachment type cannot be previewed", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation or domain policy rejected the operation", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn get_dm_attachment_preview_url(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(attachment_id): Path<String>,
) -> ApiResult<Json<DmAttachmentUrlResponse>> {
    let row = require_attachment_access(&pool, &identity.user_id, &attachment_id).await?;
    let item = views::attachment_item(&row);
    let url = item.preview_url.ok_or_else(|| {
        ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "dm.attachment_preview_unsupported",
            "This attachment cannot be previewed.",
        )
    })?;
    Ok(Json(DmAttachmentUrlResponse { url }))
}

#[derive(Debug, Deserialize)]
pub struct ContentQuery {
    #[serde(default)]
    #[allow(dead_code)]
    disposition: Option<String>,
}

/// Stream the bytes with this app's content type and file name.
pub async fn stream_file(identity: &Identity, row: &AttachmentRow) -> ApiResult<Response> {
    let file_id = row
        .storage_file_id
        .ok_or_else(ApiError::attachment_not_found)?;
    let mut response = identity
        .files_download(&file_id.to_string())
        .await
        .map_err(|error| {
            eprintln!("storage_download_failed: {} {:?}", error.code, error.status);
            ApiError::unavailable(
                "dm.attachment_download_failed",
                "The attachment could not be downloaded.",
            )
        })?;
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(&row.content_type) {
        headers.insert(header::CONTENT_TYPE, value);
    }
    let encoded: String = row
        .filename
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect();
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename*=UTF-8''{encoded}")) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

/// `GET /files/dm/attachments/{id}/content` (AI-DO's signed `content` URL).
#[utoipa::path(
    get, path = "/files/dm/attachments/{attachment_id}/content", operation_id = "downloadDmAttachmentContent",
    description = "Streams an authorized attachment with the stored media type.",
    params(
        ("attachment_id" = String, Path),
        ("disposition" = Option<String>, Query, description = "Legacy hint. Response uses attachment disposition."),
    ),
    responses(
        (status = 200, description = "Attachment bytes with the stored media type",
         body = inline(crate::attachments::FileBytes), content_type = "*/*",
         headers(
            ("Content-Length" = u64, description = "Number of bytes"),
            ("Content-Disposition" = String, description = "Attachment filename, with UTF-8 encoding"),
            ("Cache-Control" = String, description = "private, no-store"),
            ("ETag" = String, description = "Stored file entity tag"),
            ("X-Miy-File-Id" = String, description = "Core file ID"),
            ("X-Miy-File-Version" = String, description = "Core file version"),
            ("X-Miy-File-Sha256" = String, description = "SHA-256 digest"),
            ("Content-Security-Policy" = String, description = "Sandbox policy"),
            ("X-Content-Type-Options" = String, description = "nosniff"),
         )),
        (status = 400, description = "Malformed query parameters", body = String, content_type = "text/plain"),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation or domain policy rejected the operation", body = crate::error::ErrorResponse),
        (status = 502, description = "A required upstream service failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn download_dm_attachment_content(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(attachment_id): Path<String>,
    Query(_query): Query<ContentQuery>,
) -> ApiResult<Response> {
    let row = require_attachment_access(&pool, &identity.user_id, &attachment_id).await?;
    if row.message_id.is_some() && row.storage_status == "uploaded" {
        // A lost attach after send: try once more (same request ID).
        attach_one(&pool, &identity, &row).await;
    }
    stream_file(&identity, &row).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // test_dm_attachment_policy.py
    #[test]
    fn filenames_are_made_safe() {
        assert_eq!(
            safe_filename(Some("C:\\Users\\me\\report.pdf")),
            "report.pdf"
        );
        assert_eq!(safe_filename(Some("../../etc/passwd")), "passwd");
        assert_eq!(safe_filename(Some("a<b>c?.txt")), "a_b_c_.txt");
        assert_eq!(safe_filename(Some("  ..  ")), "unnamed");
        assert_eq!(safe_filename(None), "unnamed");
        assert_eq!(safe_filename(Some("C:file.txt")), "file.txt");
        assert_eq!(safe_filename(Some(&"가".repeat(600))).chars().count(), 512);
    }

    #[test]
    fn content_types_are_normalized_and_sniffed() {
        assert_eq!(
            normalize_content_type(Some("Image/PNG; charset=x")),
            "image/png"
        );
        assert_eq!(
            normalize_content_type(Some("not a type")),
            DEFAULT_ATTACHMENT_CONTENT_TYPE
        );
        assert_eq!(
            normalize_content_type(None),
            DEFAULT_ATTACHMENT_CONTENT_TYPE
        );
        assert_eq!(
            sniff_content_type(b"\x89PNG\r\n\x1a\nrest"),
            Some("image/png")
        );
        assert_eq!(sniff_content_type(b"\xff\xd8\xff\xe0"), Some("image/jpeg"));
        assert_eq!(sniff_content_type(b"GIF89a"), Some("image/gif"));
        assert_eq!(
            sniff_content_type(b"RIFF\0\0\0\0WEBPVP8 "),
            Some("image/webp")
        );
        assert_eq!(sniff_content_type(b"%PDF-1.7"), None);
    }

    #[test]
    fn declared_images_need_matching_bytes() {
        assert_eq!(
            decide_content_type(Some("image/png"), None),
            DEFAULT_ATTACHMENT_CONTENT_TYPE
        );
        assert_eq!(
            decide_content_type(Some("image/png"), Some("image/png")),
            "image/png"
        );
        assert_eq!(
            decide_content_type(Some("application/pdf"), None),
            "application/pdf"
        );
        assert_eq!(
            decide_content_type(Some("application/octet-stream"), Some("image/gif")),
            "image/gif"
        );
        assert!(is_previewable_image_content_type("image/webp"));
        assert!(!is_previewable_image_content_type("image/svg+xml"));
    }
}
