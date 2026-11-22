//! Images embedded in page bodies (`media:<id>` in BlockNote image blocks).
//!
//! AI-DO uploaded them to its object storage (`POST /media/upload`), linked
//! them to the page (`POST /media/link`, resource type `docs_native_page`,
//! and again by `sync_embedded_media` on every save) and resolved
//! `media:<id>` to a signed proxy URL for viewers who may read the page. In
//! miy the bytes live in the core storage (`platform:files:*`, 10 MiB) and
//! reach the frame only through this app (same flow as the board app):
//!
//! 1. `POST /v1/media/upload` records the intent (request ID), reserves a core
//!    file and answers AI-DO's `{id, url: "media:<id>"}`;
//! 2. the frame streams the bytes with `runtime.fileApi('/media/<id>/content',
//!    {method: 'PUT'})`;
//! 3. `POST /v1/media/link` or the page save links the uploader's unlinked
//!    images to the page and attaches them in the core once;
//! 4. viewers fetch `GET /files/media/<id>/content` after the page's view check
//!    (AI-DO `can_resolve_media` / `can_access_docs_native_page`);
//! 5. permanently deleting the page releases them (files.rs, trash.rs).
use axum::{
    Extension, Json,
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use miy_app_sdk::{FileCreate, FileStatus, Identity, storage::MAX_FILE_BYTES};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, Transaction};

use crate::{
    AppState,
    access::{self, Require, Viewer},
    error::{ApiError, ApiJson, ApiResult},
    files::Kind,
    time::now_utc,
};

pub const RESOURCE_DOCS_NATIVE_PAGE: &str = "docs_native_page";
/// AI-DO accepted SVG too; the miy file gateway and the common preview embed
/// raster images only, so SVG is left out.
const ALLOWED_IMAGE_TYPES: [&str; 4] = ["image/gif", "image/jpeg", "image/png", "image/webp"];

#[derive(Clone, Debug, FromRow)]
pub struct MediaFile {
    pub id: String,
    pub file_id: Option<String>,
    pub uploaded_by_id: String,
    pub status: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
}

const MEDIA_COLUMNS: &str = "id, file_id, uploaded_by_id, status, resource_type, resource_id";

/// A media row whose core file still needs attaching after the commit.
#[derive(Clone, Debug)]
pub struct Linked {
    pub id: String,
    pub file_id: String,
}

/// AI-DO `MEDIA_ID_PATTERN = media:([0-9a-f-]{36})`, all distinct matches.
pub fn media_ids_in(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0;
    while let Some(found) = text[start..].find("media:") {
        let at = start + found + "media:".len();
        if let Some(candidate) = bytes.get(at..at + 36) {
            if candidate
                .iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b) || *b == b'-')
            {
                let id = String::from_utf8_lossy(candidate).into_owned();
                if !ids.contains(&id) {
                    ids.push(id);
                }
                start = at + 36;
                continue;
            }
        }
        start = at;
    }
    ids
}

/// Scan a page body and link the uploader's new media to the page (AI-DO
/// `sync_embedded_media`). AI-DO also unlinked media that left the body; miy
/// keeps them linked while the page exists because older versions (version
/// history, 보강) still show them. They are released when the page is
/// permanently deleted. Linked rows still `ready` are returned for attaching
/// after the commit (a failed attach is retried on the next save).
pub async fn sync_embedded_media(
    tx: &mut Transaction<'_, Postgres>,
    body: &str,
    page_id: &str,
    user_id: &str,
) -> ApiResult<Vec<Linked>> {
    let current = media_ids_in(body);
    let mut linked = Vec::new();
    if !current.is_empty() {
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "UPDATE media_files SET resource_type = $1, resource_id = $2 \
             WHERE id = ANY($3) AND status IN ('ready', 'attached') AND file_id IS NOT NULL \
               AND ((resource_type IS NULL AND uploaded_by_id = $4) \
                 OR (resource_type = $1 AND resource_id = $2 AND status = 'ready')) \
             RETURNING id, file_id, status",
        )
        .bind(RESOURCE_DOCS_NATIVE_PAGE)
        .bind(page_id)
        .bind(&current)
        .bind(user_id)
        .fetch_all(&mut **tx)
        .await?;
        linked = rows
            .into_iter()
            .filter(|(_, _, status)| status == "ready")
            .map(|(id, file_id, _)| Linked { id, file_id })
            .collect();
    }
    Ok(linked)
}

/// Attach newly linked files in the core after the app commit, with the media
/// ID as the request ID so that a retry after a lost answer is the same call.
pub async fn attach_linked(pool: &PgPool, identity: &Identity, linked: &[Linked]) {
    if identity.context.platform {
        return;
    }
    for item in linked {
        let reference = Kind::Media.reference(&item.id);
        let attachment = match identity
            .files_attach(&item.file_id, &reference, &item.id)
            .await
        {
            Ok(file) => Ok(file),
            Err(_) => identity.files_get(&item.file_id).await,
        };
        if attachment.is_ok_and(|file| {
            file.status == FileStatus::Attached
                && file.reference.as_deref() == Some(reference.as_str())
        }) {
            let _ = sqlx::query(
                "UPDATE media_files SET status = 'attached' WHERE id = $1 AND status = 'ready'",
            )
            .bind(&item.id)
            .execute(pool)
            .await;
        }
    }
}

// ---------------------------------------------------------------- routes

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadRequest {
    request_id: String,
    name: String,
    mime: String,
    #[schema(maximum = 10485760)]
    size: u64,
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadResponse {
    id: String,
    url: String,
    status: String,
}

fn media_not_found() -> ApiError {
    ApiError::not_found("media.not_found", "Media file not found.")
}

fn upload_failed() -> ApiError {
    ApiError::unavailable("media.upload_failed", "The file could not be stored.")
}

#[utoipa::path(
    post, path = "/v1/media/upload", operation_id = "reserveDocsMedia", tag = "Embedded media",
    request_body = UploadRequest,
    responses(
        (status = 201, description = "Created", body = UploadResponse),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 409, description = "Current resource state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 413, description = "Request exceeds the size limit", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn create_upload(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<UploadRequest>,
) -> ApiResult<impl IntoResponse> {
    let request_id = uuid::Uuid::parse_str(&payload.request_id)
        .map_err(|_| ApiError::validation("requestId: Input should be a valid UUID"))?;
    let name = payload.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 255 || name.chars().any(char::is_control) {
        return Err(ApiError::validation("name: invalid file name"));
    }
    let mime = payload.mime.trim().to_ascii_lowercase();
    if !ALLOWED_IMAGE_TYPES.contains(&mime.as_str()) {
        return Err(ApiError::bad_request(
            "media.unsupported_file_type",
            format!(
                "Unsupported file type: {mime}. Allowed: {}",
                ALLOWED_IMAGE_TYPES.join(", ")
            ),
        ));
    }
    if payload.size > MAX_FILE_BYTES {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "media.file_size_limit_exceeded",
            "File size exceeds the 10 MB limit.",
        ));
    }
    // Persist the reservation intent before reserving (CONTRACT "Small attachments").
    let row: (String, String, Option<String>, String, String, String, i32) = sqlx::query_as(
        "INSERT INTO media_files (id, request_id, filename, content_type, size_bytes, uploaded_by_id, status, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, 'pending', $7) \
         ON CONFLICT (request_id) DO UPDATE SET request_id = EXCLUDED.request_id \
         RETURNING id, uploaded_by_id, file_id, status, filename, content_type, size_bytes",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(request_id)
    .bind(&name)
    .bind(&mime)
    .bind(payload.size as i32)
    .bind(&identity.user_id)
    .bind(now_utc())
    .fetch_one(&state.pool)
    .await?;
    let (media_id, owner, file_id, status, original_name, original_mime, original_size) = row;
    if owner != identity.user_id
        || original_name != name
        || original_mime != mime
        || original_size != payload.size as i32
    {
        return Err(ApiError::conflict(
            "media.request_conflict",
            "The request ID is already in use.",
        ));
    }
    if file_id.is_none() {
        let request = request_id.to_string();
        let file = match identity
            .files_create(&FileCreate {
                request_id: request.clone(),
                name: name.clone(),
                mime: mime.clone(),
                size: payload.size,
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
        sqlx::query("UPDATE media_files SET file_id = $2 WHERE id = $1 AND file_id IS NULL")
            .bind(&media_id)
            .bind(&file.id)
            .execute(&state.pool)
            .await?;
    }
    Ok((
        StatusCode::CREATED,
        Json(UploadResponse {
            url: format!("media:{media_id}"),
            id: media_id,
            status,
        }),
    ))
}

async fn load_media(pool: &PgPool, media_id: &str) -> ApiResult<MediaFile> {
    let sql = format!("SELECT {MEDIA_COLUMNS} FROM media_files WHERE id = $1");
    sqlx::query_as::<_, MediaFile>(&sql)
        .bind(media_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(media_not_found)
}

/// `PUT /files/media/{id}/content`: the uploader streams the reserved bytes.
/// Repeating it after a lost answer reports the stored state.
#[utoipa::path(
    put, path = "/files/media/{media_id}/content", operation_id = "uploadDocsMedia", tag = "Embedded media",
    params(("media_id" = String, Path)),
    request_body(content = crate::api_contract::BinaryBody, content_type = "application/octet-stream", description = "Raw file bytes, at most 10 MiB"),
    responses(
        (status = 200, description = "Success", body = UploadResponse),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn upload_content(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(media_id): Path<String>,
    body: Body,
) -> ApiResult<Json<UploadResponse>> {
    let media = load_media(&state.pool, &media_id).await?;
    if media.uploaded_by_id != identity.user_id {
        return Err(media_not_found());
    }
    let file_id = media.file_id.clone().ok_or_else(media_not_found)?;
    if !matches!(media.status.as_str(), "pending" | "ready" | "attached") {
        return Err(media_not_found());
    }
    if media.status == "pending" {
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
        sqlx::query("UPDATE media_files SET status = 'ready' WHERE id = $1 AND status = 'pending'")
            .bind(&media.id)
            .execute(&state.pool)
            .await?;
    }
    Ok(Json(UploadResponse {
        url: format!("media:{}", media.id),
        id: media.id,
        status: "ready".into(),
    }))
}

/// `GET /files/media/{id}/content`: the uploader's own unlinked image (editor
/// preview) or an image of a page the caller may read (AI-DO
/// `can_resolve_media` for `docs_native_page`).
#[utoipa::path(
    get, path = "/files/media/{media_id}/content", operation_id = "downloadDocsMedia", tag = "Embedded media",
    params(("media_id" = String, Path)),
    responses(
        (status = 200, description = "Stored image", content((crate::api_contract::BinaryBody = "image/jpeg"), (crate::api_contract::BinaryBody = "image/png"), (crate::api_contract::BinaryBody = "image/gif"), (crate::api_contract::BinaryBody = "image/webp")), headers(("Content-Disposition" = String), ("Content-Length" = u64), ("X-Content-Type-Options" = String))),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn download(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(media_id): Path<String>,
) -> ApiResult<Response> {
    let media = load_media(&state.pool, &media_id).await?;
    let allowed = match (media.resource_type.as_deref(), media.resource_id.as_deref()) {
        (None, _) => media.uploaded_by_id == identity.user_id,
        (Some(RESOURCE_DOCS_NATIVE_PAGE), Some(page_id)) => {
            let mut viewer = Viewer::new(&state, &identity);
            access::page_or_403(&mut viewer, page_id, Require::View)
                .await
                .is_ok()
        }
        _ => false,
    };
    if !allowed || !matches!(media.status.as_str(), "ready" | "attached") {
        return Err(media_not_found());
    }
    let file_id = media.file_id.as_deref().ok_or_else(media_not_found)?;
    identity.files_download(file_id).await.map_err(|_| {
        ApiError::unavailable("media.storage_download_failed", "Storage download failed.")
    })
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct LinkRequest {
    #[serde(default)]
    media_ids: Vec<String>,
    resource_type: String,
    resource_id: String,
}

/// `POST /v1/media/link` (AI-DO media/router.py `link_media`): link the
/// caller's unlinked uploads to a page they may edit.
#[utoipa::path(
    post, path = "/v1/media/link", operation_id = "linkDocsMedia", tag = "Embedded media",
    request_body = LinkRequest,
    responses(
        (status = 204, description = "Completed with no response body"),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn link_media(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<LinkRequest>,
) -> ApiResult<StatusCode> {
    if payload.media_ids.is_empty() {
        return Ok(StatusCode::NO_CONTENT);
    }
    if payload.resource_type != RESOURCE_DOCS_NATIVE_PAGE {
        return Err(ApiError::bad_request(
            "media.unsupported_resource_type",
            "Unsupported media resource type.",
        ));
    }
    let page_id = access::normalize_page_id(&payload.resource_id)?;
    let page = access::load_page(&state.pool, &page_id)
        .await?
        .ok_or_else(access::page_not_found)?;
    let mut tx = state.pool.begin().await?;
    let context = access::lock_doc_for_write(&mut tx, &state, &identity, &page.doc_id).await?;
    if !context.access.can_edit || context.doc.trashed_at.is_some() || page.trashed_at.is_some() {
        return Err(ApiError::forbidden(
            "docs.doc_edit_access_required",
            "Doc edit access required.",
        ));
    }
    let rows: Vec<(String, String)> = sqlx::query_as(
        "UPDATE media_files SET resource_type = $1, resource_id = $2 \
         WHERE id = ANY($3) AND resource_type IS NULL AND uploaded_by_id = $4 \
           AND status = 'ready' AND file_id IS NOT NULL RETURNING id, file_id",
    )
    .bind(RESOURCE_DOCS_NATIVE_PAGE)
    .bind(&page.id)
    .bind(&payload.media_ids)
    .bind(&identity.user_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let linked: Vec<Linked> = rows
        .into_iter()
        .map(|(id, file_id)| Linked { id, file_id })
        .collect();
    attach_linked(&state.pool, &identity, &linked).await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_ids_are_found_like_ai_do_pattern() {
        let a = "00000000-0000-4000-8000-000000000001";
        let b = "00000000-0000-4000-8000-0000000000ab";
        let body = format!(
            r#"[{{"type":"image","props":{{"url":"media:{a}"}}}},"media:{b}","media:{a}","media:XYZ"]"#
        );
        assert_eq!(media_ids_in(&body), vec![a.to_string(), b.to_string()]);
        assert!(media_ids_in("media:short").is_empty());
        assert!(media_ids_in("").is_empty());
    }
}
