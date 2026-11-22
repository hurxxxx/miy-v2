//! Files attached to one page: AI-DO domains/docs/attachments.py and the
//! attachment routes of router.py.
//!
//! AI-DO took a multipart upload (50 MB) into its object store and streamed
//! downloads back through authenticated routes. miy uses the core storage
//! (`platform:files:*`, 10 MiB per file) in the contract's order:
//!
//! 1. `POST /v1/docs/pages/{page_id}/attachments` with `{request_id, filename,
//!    content_type, size_bytes}` records the intent, reserves a core file and
//!    answers the attachment (`status: pending`). Repeating the same request
//!    ID with the same file answers the same row (a lost answer is recovered
//!    this way, never by deleting).
//! 2. The frame streams the bytes with `runtime.fileApi(PUT
//!    /docs/attachments/{id}/content)`; the server stores them, attaches the
//!    core file to `docs_attachment:<id>` (request ID = attachment ID) and
//!    answers the finished attachment. Repeating it reports the stored state.
//! 3. `GET /files/docs/attachments/{id}/content` streams the bytes to anyone
//!    who may read the page; `DELETE /v1/docs/attachments/{id}` needs edit
//!    access and detaches the core file after the commit.
//!
//! Every file call checks the page access first. Lists show finished
//! attachments only.
use axum::{
    Extension, Json,
    body::Body,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::NaiveDateTime;
use miy_app_sdk::{FileCreate, FileStatus, Identity, storage::MAX_FILE_BYTES};
use serde::Deserialize;
use sqlx::{FromRow, PgPool};

use crate::{
    AppState,
    access::{self, Require, Viewer},
    error::{ApiError, ApiJson, ApiResult},
    files::{self, Kind},
    people,
    time::{naive_iso, now_utc},
};

pub const DEFAULT_FILENAME: &str = "unnamed";
pub const MAX_FILENAME_CHARS: usize = 255;
pub const DEFAULT_CONTENT_TYPE: &str = "application/octet-stream";

#[derive(Clone, Debug, FromRow)]
pub struct Attachment {
    pub id: String,
    pub page_id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i32,
    pub file_id: Option<String>,
    pub status: String,
    pub uploaded_by_id: String,
    pub created_at: NaiveDateTime,
}

const COLUMNS: &str = "id, page_id, filename, content_type, size_bytes, file_id, status, \
     uploaded_by_id, created_at";

/// AI-DO `normalize_page_attachment_filename`: the leaf name only, trimmed to
/// the column width with a short extension kept.
pub fn normalize_filename(filename: Option<&str>) -> String {
    let normalized = filename
        .unwrap_or("")
        .trim()
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if normalized.is_empty() {
        return DEFAULT_FILENAME.into();
    }
    let count = normalized.chars().count();
    if count <= MAX_FILENAME_CHARS {
        return normalized;
    }
    if let Some((stem, extension)) = normalized.rsplit_once('.') {
        let extension_chars = extension.chars().count();
        if (1..=16).contains(&extension_chars) {
            let keep = MAX_FILENAME_CHARS as isize - extension_chars as isize - 1;
            if keep > 0 {
                let stem: String = stem.chars().take(keep as usize).collect();
                return format!("{stem}.{extension}");
            }
        }
    }
    normalized.chars().take(MAX_FILENAME_CHARS).collect()
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct AttachmentItem {
    id: String,
    page_id: String,
    filename: String,
    content_type: String,
    size_bytes: i64,
    uploaded_by_id: String,
    uploaded_by_name: String,
    created_at: String,
    status: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct AttachmentListResponse {
    items: Vec<AttachmentItem>,
}

fn item(attachment: &Attachment, uploaded_by_name: &str) -> AttachmentItem {
    AttachmentItem {
        id: attachment.id.clone(),
        page_id: attachment.page_id.clone(),
        filename: attachment.filename.clone(),
        content_type: attachment.content_type.clone(),
        size_bytes: i64::from(attachment.size_bytes),
        uploaded_by_id: attachment.uploaded_by_id.clone(),
        uploaded_by_name: uploaded_by_name.to_owned(),
        created_at: naive_iso(attachment.created_at),
        status: attachment.status.clone(),
    }
}

fn attachment_not_found() -> ApiError {
    ApiError::not_found("docs.attachment_not_found", "Attachment not found.")
}

fn upload_failed() -> ApiError {
    ApiError::unavailable("media.upload_failed", "The file could not be stored.")
}

async fn load(pool: &PgPool, attachment_id: &str) -> ApiResult<Attachment> {
    let sql = format!("SELECT {COLUMNS} FROM docs_page_attachments WHERE id = $1");
    sqlx::query_as::<_, Attachment>(&sql)
        .bind(attachment_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(attachment_not_found)
}

/// AI-DO `list_page_attachments`: oldest first, readers may list.
#[utoipa::path(
    get, path = "/v1/docs/pages/{page_id}/attachments", operation_id = "listDocsPageAttachments", tag = "Attachments",
    params(("page_id" = String, Path)),
    responses(
        (status = 200, description = "Success", body = AttachmentListResponse),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(page_id): Path<String>,
) -> ApiResult<Json<AttachmentListResponse>> {
    let mut viewer = Viewer::new(&state, &identity);
    let context = access::page_or_403(&mut viewer, &page_id, Require::View).await?;
    let sql = format!(
        "SELECT {COLUMNS} FROM docs_page_attachments WHERE page_id = $1 AND status IN ('ready', 'attached') \
         ORDER BY created_at, id"
    );
    let rows = sqlx::query_as::<_, Attachment>(&sql)
        .bind(&context.page.id)
        .fetch_all(&state.pool)
        .await?;
    let names =
        people::display_names(&identity, rows.iter().map(|row| row.uploaded_by_id.clone())).await;
    Ok(Json(AttachmentListResponse {
        items: rows
            .iter()
            .map(|row| {
                item(
                    row,
                    names
                        .get(&row.uploaded_by_id)
                        .map(String::as_str)
                        .unwrap_or(""),
                )
            })
            .collect(),
    }))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ReserveRequest {
    request_id: String,
    #[serde(default)]
    filename: Option<String>,
    #[serde(default)]
    content_type: Option<String>,
    #[schema(minimum = 0, maximum = 10485760)]
    size_bytes: i64,
}

/// Step 1: record and reserve (AI-DO `upload_page_attachment` up to the
/// object write). Editing the page is required.
#[utoipa::path(
    post, path = "/v1/docs/pages/{page_id}/attachments", operation_id = "reserveDocsPageAttachment", tag = "Attachments",
    params(("page_id" = String, Path)),
    request_body = ReserveRequest,
    responses(
        (status = 201, description = "Created", body = AttachmentItem),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 409, description = "Current resource state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 413, description = "Request exceeds the size limit", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn reserve(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(page_id): Path<String>,
    ApiJson(payload): ApiJson<ReserveRequest>,
) -> ApiResult<impl IntoResponse> {
    let request_id = uuid::Uuid::parse_str(&payload.request_id)
        .map_err(|_| ApiError::validation("request_id: Input should be a valid UUID"))?;
    let filename = normalize_filename(payload.filename.as_deref());
    let content_type: String = payload
        .content_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
        .unwrap_or(DEFAULT_CONTENT_TYPE)
        .chars()
        .take(120)
        .collect();
    if payload.size_bytes < 0 {
        return Err(ApiError::validation(
            "size_bytes: Input should be greater than or equal to 0",
        ));
    }
    if payload.size_bytes as u64 > MAX_FILE_BYTES {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "docs.attachment_size_limit_exceeded",
            "Attachments may be at most 10MB.",
        ));
    }
    let mut tx = state.pool.begin().await?;
    let page_id = access::normalize_page_id(&page_id)?;
    let page = access::load_page(&state.pool, &page_id)
        .await?
        .ok_or_else(access::page_not_found)?;
    let context = access::lock_doc_for_write(&mut tx, &state, &identity, &page.doc_id).await?;
    let live: Option<String> = sqlx::query_scalar(
        "SELECT id FROM docs_native_doc_pages WHERE id = $1 AND trashed_at IS NULL FOR SHARE",
    )
    .bind(&page.id)
    .fetch_optional(&mut *tx)
    .await?;
    if !context.access.can_edit || live.is_none() || context.doc.trashed_at.is_some() {
        return Err(ApiError::forbidden(
            "docs.doc_edit_access_required",
            "Doc edit access required.",
        ));
    }
    // Persist the reservation intent before reserving (CONTRACT "Small attachments").
    let sql = format!(
        "INSERT INTO docs_page_attachments (id, page_id, filename, content_type, size_bytes, request_id, status, \
         uploaded_by_id, created_at) VALUES ($1,$2,$3,$4,$5,$6,'pending',$7,$8) \
         ON CONFLICT (request_id) DO UPDATE SET request_id = EXCLUDED.request_id RETURNING {COLUMNS}"
    );
    let row = sqlx::query_as::<_, Attachment>(&sql)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&page.id)
        .bind(&filename)
        .bind(&content_type)
        .bind(payload.size_bytes as i32)
        .bind(request_id)
        .bind(&identity.user_id)
        .bind(now_utc())
        .fetch_one(&mut *tx)
        .await?;
    if row.uploaded_by_id != identity.user_id
        || row.page_id != page.id
        || row.filename != filename
        || row.content_type != content_type
        || i64::from(row.size_bytes) != payload.size_bytes
    {
        return Err(ApiError::conflict(
            "docs.attachment_request_conflict",
            "The request ID is already in use.",
        ));
    }
    tx.commit().await?;
    let mut row = row;
    if row.file_id.is_none() && row.status == "pending" {
        let request = request_id.to_string();
        let file = match identity
            .files_create(&FileCreate {
                request_id: request.clone(),
                name: filename.clone(),
                mime: content_type.clone(),
                size: payload.size_bytes as u64,
            })
            .await
        {
            Ok(file) => file,
            // An uncertain reservation is looked up, never repeated blindly.
            Err(_) => identity
                .files_by_request(&request)
                .await
                .map_err(|_| upload_failed())?,
        };
        sqlx::query(
            "UPDATE docs_page_attachments SET file_id = $2 WHERE id = $1 AND file_id IS NULL",
        )
        .bind(&row.id)
        .bind(&file.id)
        .execute(&state.pool)
        .await?;
        row = load(&state.pool, &row.id).await?;
    }
    let names = people::display_names(&identity, [row.uploaded_by_id.clone()]).await;
    Ok((
        StatusCode::CREATED,
        Json(item(
            &row,
            names
                .get(&row.uploaded_by_id)
                .map(String::as_str)
                .unwrap_or(""),
        )),
    ))
}

/// Attach the core file to this attachment (request ID = attachment ID, so a
/// repeat after a lost answer is the same call) and record it.
async fn attach(
    pool: &PgPool,
    identity: &Identity,
    attachment: &Attachment,
    file_id: &str,
) -> bool {
    let reference = Kind::Attachment.reference(&attachment.id);
    let result = match identity
        .files_attach(file_id, &reference, &attachment.id)
        .await
    {
        Ok(file) => Ok(file),
        Err(_) => identity.files_get(file_id).await,
    };
    if result.is_ok_and(|file| {
        file.status == FileStatus::Attached && file.reference.as_deref() == Some(reference.as_str())
    }) {
        let _ = sqlx::query(
            "UPDATE docs_page_attachments SET status = 'attached' WHERE id = $1 AND status IN ('pending', 'ready')",
        )
        .bind(&attachment.id)
        .execute(pool)
        .await;
        return true;
    }
    false
}

/// Step 2: `PUT /files/docs/attachments/{id}/content` by the uploader, who
/// must still be able to edit the page.
#[utoipa::path(
    put, path = "/files/docs/attachments/{attachment_id}/content", operation_id = "uploadDocsPageAttachment", tag = "Attachments",
    params(("attachment_id" = String, Path)),
    request_body(content = crate::api_contract::BinaryBody, content_type = "application/octet-stream", description = "Raw file bytes, at most 10 MiB"),
    responses(
        (status = 200, description = "Success", body = AttachmentItem),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn upload_content(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(attachment_id): Path<String>,
    body: Body,
) -> ApiResult<Json<AttachmentItem>> {
    let attachment = load(&state.pool, &attachment_id).await?;
    if attachment.uploaded_by_id != identity.user_id
        || !matches!(attachment.status.as_str(), "pending" | "ready" | "attached")
    {
        return Err(attachment_not_found());
    }
    let mut viewer = Viewer::new(&state, &identity);
    access::page_or_403(&mut viewer, &attachment.page_id, Require::Edit).await?;
    let file_id = attachment.file_id.clone().ok_or_else(upload_failed)?;
    if attachment.status == "pending" {
        let file = identity
            .files_get(&file_id)
            .await
            .map_err(|_| upload_failed())?;
        let stored = match file.status {
            FileStatus::Pending => match identity.files_upload(&file, body).await {
                Ok(stored) => stored,
                Err(_) => identity
                    .files_get(&file_id)
                    .await
                    .map_err(|_| upload_failed())?,
            },
            _ => file,
        };
        if !matches!(stored.status, FileStatus::Ready | FileStatus::Attached) {
            return Err(upload_failed());
        }
        sqlx::query("UPDATE docs_page_attachments SET status = 'ready' WHERE id = $1 AND status = 'pending'")
            .bind(&attachment.id)
            .execute(&state.pool)
            .await?;
    }
    let current = load(&state.pool, &attachment.id).await?;
    if current.status == "ready" && !attach(&state.pool, &identity, &current, &file_id).await {
        return Err(upload_failed());
    }
    let current = load(&state.pool, &attachment.id).await?;
    if current.status != "attached" {
        // The page went away meanwhile (the release recorded its work).
        return Err(attachment_not_found());
    }
    let names = people::display_names(&identity, [current.uploaded_by_id.clone()]).await;
    Ok(Json(item(
        &current,
        names
            .get(&current.uploaded_by_id)
            .map(String::as_str)
            .unwrap_or(""),
    )))
}

/// AI-DO `open_page_attachment_content`: always a download of an opaque type.
#[utoipa::path(
    get, path = "/files/docs/attachments/{attachment_id}/content", operation_id = "downloadDocsPageAttachment", tag = "Attachments",
    params(("attachment_id" = String, Path)),
    responses(
        (status = 200, description = "Opaque attachment bytes", body = crate::api_contract::BinaryBody, content_type = "application/octet-stream", headers(("Content-Disposition" = String), ("Content-Length" = u64), ("X-Content-Type-Options" = String))),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 502, description = "Storage did not return the file", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn download(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(attachment_id): Path<String>,
) -> ApiResult<Response> {
    let attachment = load(&state.pool, &attachment_id).await?;
    if !matches!(attachment.status.as_str(), "ready" | "attached") {
        return Err(attachment_not_found());
    }
    let mut viewer = Viewer::new(&state, &identity);
    access::page_or_403(&mut viewer, &attachment.page_id, Require::View).await?;
    let file_id = attachment
        .file_id
        .as_deref()
        .ok_or_else(attachment_not_found)?;
    let mut response = identity.files_download(file_id).await.map_err(|_| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "docs.attachment_download_failed",
            "Could not download the attachment. Try again shortly.",
        )
    })?;
    // Rendering an uploaded text/html or image/svg+xml would be stored XSS
    // (AI-DO); the frame saves the blob under the attachment's name.
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(DEFAULT_CONTENT_TYPE),
    );
    Ok(response)
}

/// AI-DO `delete_page_attachment`: editing the page is required.
#[utoipa::path(
    delete, path = "/v1/docs/attachments/{attachment_id}", operation_id = "deleteDocsPageAttachment", tag = "Attachments",
    params(("attachment_id" = String, Path)),
    responses(
        (status = 204, description = "Completed with no response body"),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn delete(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(attachment_id): Path<String>,
) -> ApiResult<StatusCode> {
    let attachment = load(&state.pool, &attachment_id).await?;
    if !matches!(attachment.status.as_str(), "pending" | "ready" | "attached") {
        return Err(attachment_not_found());
    }
    let page = access::load_page(&state.pool, &attachment.page_id)
        .await?
        .ok_or_else(attachment_not_found)?;
    let mut tx = state.pool.begin().await?;
    let context = access::lock_doc_for_write(&mut tx, &state, &identity, &page.doc_id).await?;
    if !context.access.can_edit || page.trashed_at.is_some() || context.doc.trashed_at.is_some() {
        return Err(ApiError::forbidden(
            "docs.doc_edit_access_required",
            "Doc edit access required.",
        ));
    }
    let released = files::release_attachment(&mut tx, &attachment.id).await?;
    tx.commit().await?;
    files::detach_released(&state.pool, &identity, &released).await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filenames_are_reduced_to_a_leaf_name() {
        // AI-DO test_filename_is_reduced_to_a_leaf_name
        assert_eq!(normalize_filename(None), "unnamed");
        assert_eq!(normalize_filename(Some("  ")), "unnamed");
        assert_eq!(normalize_filename(Some("report.pdf")), "report.pdf");
        assert_eq!(normalize_filename(Some("a/b/report.pdf")), "report.pdf");
        assert_eq!(
            normalize_filename(Some("C:\\docs\\report.pdf")),
            "report.pdf"
        );
        assert_eq!(normalize_filename(Some("folder/")), "unnamed");
    }

    #[test]
    fn long_filenames_keep_their_extension() {
        // test_long_filename_is_trimmed_to_the_column_width
        let name = format!("{}.pdf", "a".repeat(300));
        let trimmed = normalize_filename(Some(&name));
        assert_eq!(trimmed.chars().count(), 255);
        assert!(trimmed.ends_with(".pdf"));
        // test_long_filename_without_a_usable_extension_is_still_trimmed
        assert_eq!(
            normalize_filename(Some(&"b".repeat(300))).chars().count(),
            255
        );
        // test_long_trailing_dot_segment_is_not_treated_as_an_extension
        let odd = format!("{}.{}", "c".repeat(10), "d".repeat(300));
        let trimmed = normalize_filename(Some(&odd));
        assert_eq!(trimmed.chars().count(), 255);
        assert!(trimmed.starts_with("cccccccccc."));
    }
}
