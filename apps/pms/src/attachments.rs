//! Task attachments: AI-DO attachments.py on miy small attachments
//! (`platform:files:read`/`write`, 10 MiB per file).
//!
//! AI-DO took one multipart upload into object storage and handed out signed
//! download URLs. miy stores bytes in the core, reached only from the app
//! backend, so an upload takes three steps:
//!
//! 1. `POST /v1/pms/tasks/{task_id}/attachments` `{request_id, filename,
//!    content_type, size_bytes}` records the intent (a pending row) and
//!    reserves the platform file with that request id.
//! 2. The frame streams the bytes with `runtime.fileApi` to
//!    `PUT /files/pms/attachments/{id}/content`; the backend uploads them to
//!    the core, commits the attachment and then attaches the file to the
//!    reference `pms_attachment:<id>`.
//! 3. Downloads are `GET /files/pms/attachments/{id}/content` (bytes through
//!    the platform file gateway, never a URL).
//!
//! `DELETE /v1/pms/attachments/{id}` removes the row and detaches (or
//! cancels) the platform file. Files of deleted tasks are queued in
//! pms_attachment_releases and detached after the commit, retried by later
//! requests until the core confirms.
use std::collections::HashMap;

use axum::{
    Extension, Json,
    body::Body as AxumBody,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use miy_app_sdk::{FileCreate, FileStatus, Identity};
use serde::Serialize;
use sqlx::{PgConnection, PgPool};

use crate::access::{self, ensure_list_editor, ensure_task_readable};
use crate::error::{ApiError, ApiResult, ErrorResponse, IdentityErrorResponse, check_length};
use crate::input::Body;
use crate::models::{ATTACHMENT_COLUMNS, AttachmentRow, iso_datetime, new_id, utcnow};
use crate::tasks::{actor_name, log_activity};
use crate::users;

pub const MAX_ATTACHMENT_BYTES: i64 = 10 * 1024 * 1024;
const RELEASE_BATCH: i64 = 20;

pub fn reference(attachment_id: &str) -> String {
    format!("pms_attachment:{attachment_id}")
}

pub fn content_path(attachment_id: &str) -> String {
    format!("/pms/attachments/{attachment_id}/content")
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
struct CreateTaskAttachmentRequest {
    #[schema(format = "uuid")]
    request_id: String,
    /// Trimmed filename, up to 255 characters. An empty or omitted value becomes `unnamed`.
    #[schema(value_type = String, required = false)]
    filename: Option<String>,
    /// Defaults to `application/octet-stream` if null, blank or omitted.
    #[schema(max_length = 120)]
    content_type: Option<String>,
    /// Exact number of bytes. The legacy API also accepts an integer string.
    #[schema(minimum = 0, maximum = 10485760)]
    size_bytes: i64,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[schema(rename_all = "snake_case")]
enum ReservationState {
    Pending,
}

#[derive(Serialize, utoipa::ToSchema)]
struct TaskAttachmentReservation {
    id: String,
    task_id: String,
    filename: String,
    content_type: String,
    size_bytes: i32,
    #[schema(value_type = ReservationState)]
    status: &'static str,
    /// App-relative path for `runtime.fileApi`.
    upload_path: String,
}

#[derive(utoipa::ToSchema)]
#[schema(format = Binary)]
struct TaskAttachmentBytes(#[allow(dead_code)] String);

/// AI-DO `AttachmentItem`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct TaskAttachment {
    id: String,
    task_id: String,
    filename: String,
    content_type: String,
    size_bytes: i32,
    /// App-relative path for `runtime.fileApi`.
    download_url: String,
    uploaded_by_id: String,
    uploaded_by_name: String,
    created_at: String,
}

pub fn serialize(attachment: &AttachmentRow, names: &HashMap<String, String>) -> TaskAttachment {
    TaskAttachment {
        id: attachment.id.clone(),
        task_id: attachment.task_id.clone(),
        filename: attachment.filename.clone(),
        content_type: attachment.content_type.clone(),
        size_bytes: attachment.size_bytes,
        download_url: content_path(&attachment.id),
        uploaded_by_id: attachment.uploaded_by_id.clone(),
        uploaded_by_name: names
            .get(&attachment.uploaded_by_id)
            .cloned()
            .unwrap_or_else(|| attachment.uploaded_by_id.clone()),
        created_at: iso_datetime(&attachment.created_at),
    }
}

fn file_error(error: miy_app_sdk::CallError) -> ApiError {
    eprintln!(
        "platform_file_call_failed: {} {:?}",
        error.code, error.status
    );
    match error.status {
        Some(409) => ApiError::new(
            StatusCode::CONFLICT,
            "file_upload_in_progress",
            "The file is being uploaded. Try again shortly.",
        ),
        Some(413) => ApiError::with_params(
            StatusCode::PAYLOAD_TOO_LARGE,
            "pms.file_size_limit_exceeded",
            &[("limit_mb", "10".into())],
        ),
        _ => ApiError::new(
            StatusCode::BAD_GATEWAY,
            "pms.attachment_upload_failed",
            "The attachment could not be stored. Try again.",
        ),
    }
}

async fn load_attachment(conn: &mut PgConnection, attachment_id: &str) -> ApiResult<AttachmentRow> {
    sqlx::query_as::<_, AttachmentRow>(&format!(
        "SELECT {ATTACHMENT_COLUMNS} FROM pms_attachments WHERE id = $1"
    ))
    .bind(attachment_id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| ApiError::not_found("pms.attachment_not_found"))
}

fn parse_uuid(field: &str, value: &str) -> ApiResult<uuid::Uuid> {
    uuid::Uuid::parse_str(value)
        .map_err(|_| ApiError::validation(format!("{field}: Input should be a valid UUID")))
}

/// Step 1: `POST /v1/pms/tasks/{task_id}/attachments`. Repeating a request id
/// returns the same pending attachment (a lost response is looked up, never
/// reserved twice).
#[utoipa::path(
    post, path = "/v1/pms/tasks/{task_id}/attachments", operation_id = "createPmsTaskAttachment", tag = "Attachments",
    params(("task_id" = String, Path)), request_body = CreateTaskAttachmentRequest,
    responses(
        (status = 200, description = "A completed attachment for this request ID", body = TaskAttachment),
        (status = 201, description = "The attachment is reserved for upload", body = TaskAttachmentReservation),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Task edit access is required", body = ErrorResponse),
        (status = 404, description = "Task or list not found", body = ErrorResponse),
        (status = 409, description = "The list is archived or has no space, the request ID is already in use, or the file is being uploaded", body = ErrorResponse),
        (status = 413, description = "The attachment exceeds 10 MiB", body = ErrorResponse),
        (status = 422, description = "Invalid attachment reservation", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 502, description = "The platform file could not be reserved", body = ErrorResponse),
    )
)]
pub async fn create_attachment(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(task_id): Path<String>,
    body: Body,
) -> ApiResult<Response> {
    let request_id = parse_uuid("request_id", &body.req_str("request_id")?)?;
    let filename = body.str_or("filename", "")?.trim().to_string();
    let filename = if filename.is_empty() {
        "unnamed".to_string()
    } else {
        filename
    };
    check_length("filename", &filename, 1, 255)?;
    let content_type = body
        .opt_str("content_type")?
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "application/octet-stream".into());
    check_length("content_type", &content_type, 1, 120)?;
    let size = body
        .opt_int("size_bytes")?
        .ok_or_else(|| ApiError::validation("size_bytes: Field required"))?;
    crate::error::check_range("size_bytes", size, 0, None)?;
    if size > MAX_ATTACHMENT_BYTES {
        return Err(ApiError::with_params(
            StatusCode::PAYLOAD_TOO_LARGE,
            "pms.file_size_limit_exceeded",
            &[("limit_mb", "10".into())],
        ));
    }
    let mut tx = pool.begin().await?;
    let task = access::load_task_for_update(&mut tx, &task_id).await?;
    ensure_list_editor(&mut tx, &identity.user_id, &task.list_id).await?;
    let existing: Option<AttachmentRow> = sqlx::query_as::<_, AttachmentRow>(&format!(
        "SELECT {ATTACHMENT_COLUMNS} FROM pms_attachments WHERE request_id = $1"
    ))
    .bind(request_id)
    .fetch_optional(&mut *tx)
    .await?;
    let attachment = match existing {
        Some(row)
            if row.task_id == task.id
                && row.uploaded_by_id == identity.user_id
                && row.filename == filename
                && row.content_type == content_type
                && i64::from(row.size_bytes) == size =>
        {
            row
        }
        Some(_) => {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "pms.attachment_request_conflict",
                "This request id was used for another attachment.",
            ));
        }
        None => {
            let row = AttachmentRow {
                id: new_id(),
                task_id: task.id.clone(),
                filename,
                content_type,
                size_bytes: size as i32,
                file_id: None,
                request_id,
                attach_request_id: uuid::Uuid::new_v4(),
                status: "pending".into(),
                uploaded_by_id: identity.user_id.clone(),
                created_at: utcnow(),
            };
            sqlx::query(&format!(
                "INSERT INTO pms_attachments ({ATTACHMENT_COLUMNS}) VALUES ($1, $2, $3, $4, $5, NULL, $6, $7, 'pending', $8, $9)"
            ))
            .bind(&row.id)
            .bind(&row.task_id)
            .bind(&row.filename)
            .bind(&row.content_type)
            .bind(row.size_bytes)
            .bind(row.request_id)
            .bind(row.attach_request_id)
            .bind(&row.uploaded_by_id)
            .bind(row.created_at)
            .execute(&mut *tx)
            .await?;
            row
        }
    };
    tx.commit().await?;
    if attachment.status == "attached" {
        let names = users::names(&identity, std::slice::from_ref(&attachment.uploaded_by_id)).await;
        return Ok((StatusCode::OK, Json(serialize(&attachment, &names))).into_response());
    }
    if attachment.file_id.is_none() {
        let reserved = match identity
            .files_create(&FileCreate {
                request_id: attachment.request_id.to_string(),
                name: attachment.filename.clone(),
                mime: attachment.content_type.clone(),
                size: attachment.size_bytes as u64,
            })
            .await
        {
            Ok(file) => file,
            Err(_) => identity
                .files_by_request(&attachment.request_id.to_string())
                .await
                .map_err(file_error)?,
        };
        let file_id = parse_uuid("file_id", &reserved.id)?;
        sqlx::query("UPDATE pms_attachments SET file_id = $2 WHERE id = $1")
            .bind(&attachment.id)
            .bind(file_id)
            .execute(&pool)
            .await?;
    }
    let upload_path = content_path(&attachment.id);
    Ok((
        StatusCode::CREATED,
        Json(TaskAttachmentReservation {
            id: attachment.id,
            task_id: attachment.task_id,
            filename: attachment.filename,
            content_type: attachment.content_type,
            size_bytes: attachment.size_bytes,
            status: "pending",
            upload_path,
        }),
    )
        .into_response())
}

/// Step 2: `PUT /files/pms/attachments/{id}/content` (bytes via `fileApi`).
#[utoipa::path(
    put, path = "/files/pms/attachments/{attachment_id}/content", operation_id = "uploadPmsTaskAttachment", tag = "Attachments",
    description = "Send the exact reserved file bytes, up to 10 MiB, using the authenticated frame file gateway. Only the uploader may do this while they can edit the task's list.",
    params(("attachment_id" = String, Path)),
    request_body(content = TaskAttachmentBytes, content_type = "application/octet-stream"),
    responses(
        (status = 200, description = "Completed attachment", body = TaskAttachment),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller may not upload this attachment", body = ErrorResponse),
        (status = 404, description = "Attachment, task, or list not found", body = ErrorResponse),
        (status = 409, description = "The list is archived or has no space, the attachment is not pending, or the file is being uploaded", body = ErrorResponse),
        (status = 410, description = "The file reservation expired", body = ErrorResponse),
        (status = 413, description = "The attachment exceeds 10 MiB", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 502, description = "The platform file could not be uploaded or attached", body = ErrorResponse),
    )
)]
pub async fn upload_content(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(attachment_id): Path<String>,
    request: Request,
) -> ApiResult<Json<TaskAttachment>> {
    let mut conn = pool.acquire().await?;
    let attachment = load_attachment(&mut conn, &attachment_id).await?;
    let task = access::load_task(&mut conn, &attachment.task_id).await?;
    ensure_list_editor(&mut conn, &identity.user_id, &task.list_id).await?;
    drop(conn);
    if attachment.uploaded_by_id != identity.user_id {
        return Err(ApiError::forbidden("pms.task_access_required"));
    }
    let Some(file_id) = attachment.file_id else {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "pms.attachment_not_pending",
            "The attachment is not waiting for an upload.",
        ));
    };
    if attachment.status != "attached" {
        let file = identity
            .files_get(&file_id.to_string())
            .await
            .map_err(file_error)?;
        if file.status == FileStatus::Pending {
            identity
                .files_upload(&file, AxumBody::new(request.into_body()))
                .await
                .map_err(file_error)?;
        } else if matches!(file.status, FileStatus::Cancelled | FileStatus::Expired) {
            return Err(ApiError::new(
                StatusCode::GONE,
                "pms.attachment_expired",
                "The upload expired. Attach the file again.",
            ));
        }
        let name = actor_name(&identity).await;
        let mut tx = pool.begin().await?;
        let current = sqlx::query_as::<_, AttachmentRow>(&format!(
            "SELECT {ATTACHMENT_COLUMNS} FROM pms_attachments WHERE id = $1 FOR UPDATE"
        ))
        .bind(&attachment.id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ApiError::not_found("pms.attachment_not_found"))?;
        // The upload and row lock can both wait. Check current edit access
        // before making the pending attachment visible on the task.
        let task = access::load_task(&mut tx, &current.task_id).await?;
        ensure_list_editor(&mut tx, &identity.user_id, &task.list_id).await?;
        if current.status == "pending" {
            sqlx::query("UPDATE pms_attachments SET status = 'attached' WHERE id = $1")
                .bind(&attachment.id)
                .execute(&mut *tx)
                .await?;
            let reference_text = crate::tasks::list_reference(&mut tx, &task).await?;
            log_activity(
                &mut tx,
                &task.id,
                Some(&identity.user_id),
                "attachment_added",
                &format!(
                    "{name} attached {} to {reference_text}.",
                    attachment.filename
                ),
                None,
                None,
                None,
            )
            .await?;
        }
        tx.commit().await?;
    }
    // The app reference is committed; now attach. The same attach request id
    // makes a repeated call (after an uncertain result) return the same file.
    identity
        .files_attach(
            &file_id.to_string(),
            &reference(&attachment.id),
            &attachment.attach_request_id.to_string(),
        )
        .await
        .map_err(file_error)?;
    let mut conn = pool.acquire().await?;
    let attachment = load_attachment(&mut conn, &attachment_id).await?;
    let names = users::names(&identity, std::slice::from_ref(&attachment.uploaded_by_id)).await;
    Ok(Json(serialize(&attachment, &names)))
}

/// Step 3: `GET /files/pms/attachments/{id}/content`.
#[utoipa::path(
    get, path = "/files/pms/attachments/{attachment_id}/content", operation_id = "downloadPmsTaskAttachment", tag = "Attachments",
    description = "Rechecks task read access before streaming the stored attachment through the authenticated frame file gateway. The response uses the stored media type. Attachments are up to 10 MiB.",
    params(("attachment_id" = String, Path)),
    responses(
        (status = 200, description = "Stored attachment bytes",
            content((TaskAttachmentBytes = "*/*")),
            headers(
                ("Content-Length" = u64, description = "Size of the response in bytes"),
                ("Content-Disposition" = String, description = "Stored filename and download disposition"),
                ("ETag" = String, description = "Storage object entity tag"),
                ("X-Miy-File-Id" = String, description = "Core storage file ID"),
                ("X-Miy-File-Version" = String, description = "Core storage file version"),
                ("X-Miy-File-Sha256" = String, description = "SHA-256 digest of the stored file"),
                ("Content-Security-Policy" = String, description = "sandbox"),
                ("X-Content-Type-Options" = String, description = "nosniff")
            )
        ),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Task read access is required", body = ErrorResponse),
        (status = 404, description = "Attachment, task, or list not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 502, description = "The platform file could not be downloaded", body = ErrorResponse),
    )
)]
pub async fn download_content(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(attachment_id): Path<String>,
) -> ApiResult<Response> {
    let mut conn = pool.acquire().await?;
    let attachment = load_attachment(&mut conn, &attachment_id).await?;
    ensure_task_readable(&mut conn, &identity.user_id, &attachment.task_id).await?;
    drop(conn);
    let (Some(file_id), "attached") = (attachment.file_id, attachment.status.as_str()) else {
        return Err(ApiError::not_found("pms.attachment_not_found"));
    };
    identity
        .files_download(&file_id.to_string())
        .await
        .map_err(|error| {
            eprintln!(
                "platform_file_download_failed: {} {:?}",
                error.code, error.status
            );
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "pms.attachment_download_failed",
                "Could not download the attachment.",
            )
        })
}

/// `DELETE /v1/pms/attachments/{attachment_id}`: also cancels a pending upload.
#[utoipa::path(
    delete, path = "/v1/pms/attachments/{attachment_id}", operation_id = "deletePmsTaskAttachment", tag = "Attachments",
    params(("attachment_id" = String, Path)),
    responses(
        (status = 204, description = "Attachment removed"),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Task edit access is required", body = ErrorResponse),
        (status = 404, description = "Attachment, task, or list not found", body = ErrorResponse),
        (status = 409, description = "The list is archived or has no space", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn delete_attachment(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(attachment_id): Path<String>,
) -> ApiResult<StatusCode> {
    let name = actor_name(&identity).await;
    let mut tx = pool.begin().await?;
    let attachment = sqlx::query_as::<_, AttachmentRow>(&format!(
        "SELECT {ATTACHMENT_COLUMNS} FROM pms_attachments WHERE id = $1 FOR UPDATE"
    ))
    .bind(&attachment_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| ApiError::not_found("pms.attachment_not_found"))?;
    let task = access::load_task(&mut tx, &attachment.task_id).await?;
    ensure_list_editor(&mut tx, &identity.user_id, &task.list_id).await?;
    if attachment.status == "attached" {
        let reference_text = crate::tasks::list_reference(&mut tx, &task).await?;
        log_activity(
            &mut tx,
            &task.id,
            Some(&identity.user_id),
            "attachment_removed",
            &format!(
                "{name} removed {} from {reference_text}.",
                attachment.filename
            ),
            None,
            None,
            None,
        )
        .await?;
    }
    queue_release(&mut tx, &attachment).await?;
    sqlx::query("DELETE FROM pms_attachments WHERE id = $1")
        .bind(&attachment.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    process_releases(&pool, &identity).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn queue_release(conn: &mut PgConnection, attachment: &AttachmentRow) -> ApiResult<()> {
    if let Some(file_id) = attachment.file_id {
        sqlx::query(
            "INSERT INTO pms_attachment_releases (file_id, reference, created_at) VALUES ($1, $2, $3) \
             ON CONFLICT (file_id) DO NOTHING",
        )
        .bind(file_id)
        .bind(reference(&attachment.id))
        .bind(utcnow())
        .execute(conn)
        .await?;
    }
    Ok(())
}

/// Queue the platform files of tasks about to be deleted (same transaction).
pub async fn queue_release_for_tasks(
    conn: &mut PgConnection,
    task_ids: &[String],
) -> ApiResult<()> {
    let rows: Vec<AttachmentRow> = sqlx::query_as::<_, AttachmentRow>(&format!(
        "SELECT {ATTACHMENT_COLUMNS} FROM pms_attachments WHERE task_id = ANY($1)"
    ))
    .bind(task_ids)
    .fetch_all(&mut *conn)
    .await?;
    for row in &rows {
        queue_release(conn, row).await?;
    }
    Ok(())
}

/// Detach or cancel queued files (bounded per request; failures stay queued).
pub async fn process_releases(pool: &PgPool, identity: &Identity) {
    let Ok(rows) = sqlx::query_as::<_, (uuid::Uuid, String)>(
        "SELECT file_id, reference FROM pms_attachment_releases ORDER BY created_at LIMIT $1",
    )
    .bind(RELEASE_BATCH)
    .fetch_all(pool)
    .await
    else {
        return;
    };
    for (file_id, file_reference) in rows {
        let id = file_id.to_string();
        let released = match identity.files_get(&id).await {
            Ok(file) => match file.status {
                FileStatus::Attached => identity.files_detach(&id, &file_reference).await.is_ok(),
                FileStatus::Pending | FileStatus::Ready => identity.files_cancel(&id).await.is_ok(),
                FileStatus::Cancelled | FileStatus::Expired => true,
            },
            Err(error) if error.status == Some(404) => true,
            Err(_) => false,
        };
        if released {
            let _ = sqlx::query("DELETE FROM pms_attachment_releases WHERE file_id = $1")
                .bind(file_id)
                .execute(pool)
                .await;
        }
    }
}
