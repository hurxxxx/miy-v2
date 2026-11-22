//! Images embedded in posts and comments (`media:<id>` in the Markdown).
//!
//! AI-DO stored them in its object storage (domains/media) and handed the
//! viewer signed proxy URLs (`POST /community/posts/{id}/media/resolve`). In
//! miy the bytes live in the core storage (`platform:files:*`, 10 MiB per
//! file) and reach the frame only through this app:
//!
//! 1. `POST /v1/media/upload` records the intent (request ID) and reserves a
//!    core file, answering AI-DO's `{id, url: "media:<id>"}`;
//! 2. the frame streams the bytes with `runtime.fileApi('/media/<id>/content',
//!    {method: 'PUT'})`;
//! 3. saving a post or comment links the uploader's unlinked images to it
//!    (AI-DO `sync_embedded_media`) and attaches them in the core once;
//! 4. viewers fetch bytes with `runtime.fileApi` after the same checks AI-DO's
//!    resolve applied (secret posts accept the post password in a POST body);
//! 5. deleting the post or comment detaches its images (bytes removed).
use axum::{
    Extension, Json,
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
};
use miy_app_sdk::{FileCreate, FileStatus, Identity, storage::MAX_FILE_BYTES};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, Transaction};

use crate::{
    AppState,
    community::{self, LOCKED_REASON_ADMIN_ONLY},
    error::{ApiError, ApiJson, ApiResult},
    time::now_utc,
};

pub const RESOURCE_COMMUNITY_POST: &str = "community_post";
pub const RESOURCE_COMMUNITY_COMMENT: &str = "community_comment";
/// AI-DO accepted SVG too; the miy file gateway and common preview embed
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

/// A media row released by a deleted post or comment (status `detaching`).
#[derive(Clone, Debug)]
pub struct Released {
    pub id: String,
    pub file_id: String,
}

fn reference(media_id: &str) -> String {
    format!("board_media:{media_id}")
}

/// AI-DO `MEDIA_ID_PATTERN = media:([0-9a-f-]{36})`, all matches.
pub fn media_ids_in(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0;
    while let Some(found) = text[start..].find("media:") {
        let at = start + found + "media:".len();
        let candidate = bytes.get(at..at + 36);
        if let Some(candidate) = candidate {
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

/// Full re-scan: link the uploader's new media, unlink removed media
/// (AI-DO `sync_embedded_media`; `can_link_unlinked_media` is the uploader).
pub async fn sync_embedded_media(
    tx: &mut Transaction<'_, Postgres>,
    body: &str,
    resource_type: &str,
    resource_id: &str,
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
        .bind(resource_type)
        .bind(resource_id)
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
    sqlx::query(
        "UPDATE media_files SET resource_type = NULL, resource_id = NULL \
         WHERE resource_type = $1 AND resource_id = $2 AND NOT (id = ANY($3))",
    )
    .bind(resource_type)
    .bind(resource_id)
    .bind(&current)
    .execute(&mut **tx)
    .await?;
    Ok(linked)
}

/// Attach newly linked files in the core after the app commit. A failure
/// leaves the row `ready`; the next save retries with the same request ID.
pub async fn attach_linked(pool: &PgPool, identity: &Identity, linked: &[Linked]) {
    for item in linked {
        let reference = reference(&item.id);
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

async fn release_where(
    tx: &mut Transaction<'_, Postgres>,
    condition: &str,
    value: &str,
) -> ApiResult<Vec<Released>> {
    // The detach work is recorded with the business delete (status
    // `detaching`) and cleared only once the core confirms it.
    let sql = format!(
        "UPDATE media_files SET resource_type = NULL, resource_id = NULL, status = 'detaching' \
         WHERE {condition} AND file_id IS NOT NULL AND status IN ('ready', 'attached') \
         RETURNING id, file_id"
    );
    let rows: Vec<(String, String)> = sqlx::query_as(&sql)
        .bind(value)
        .fetch_all(&mut **tx)
        .await?;
    Ok(rows
        .into_iter()
        .map(|(id, file_id)| Released { id, file_id })
        .collect())
}

/// Media of a post and of its comments, released with the post.
pub async fn release_for_post(
    tx: &mut Transaction<'_, Postgres>,
    post_id: &str,
) -> ApiResult<Vec<Released>> {
    release_where(
        tx,
        "((resource_type = 'community_post' AND resource_id = $1) OR \
          (resource_type = 'community_comment' AND resource_id IN \
             (SELECT id FROM community_comments WHERE post_id = $1)))",
        post_id,
    )
    .await
}

pub async fn release_for_comment(
    tx: &mut Transaction<'_, Postgres>,
    comment_id: &str,
) -> ApiResult<Vec<Released>> {
    release_where(
        tx,
        "resource_type = 'community_comment' AND resource_id = $1",
        comment_id,
    )
    .await
}

/// Remove one released file from the core: detach it when attached, cancel
/// it when it never was. Repeating is safe; an uncertain result is looked up
/// on the next attempt instead of being compensated.
async fn release_in_core(identity: &Identity, item: &Released) -> bool {
    let Ok(file) = identity.files_get(&item.file_id).await else {
        return false;
    };
    let result = match file.status {
        FileStatus::Attached => identity
            .files_detach(&item.file_id, &reference(&item.id))
            .await
            .map(|_| ()),
        FileStatus::Ready | FileStatus::Pending => {
            identity.files_cancel(&item.file_id).await.map(|_| ())
        }
        FileStatus::Cancelled | FileStatus::Expired => Ok(()),
    };
    result.is_ok()
}

/// After the app commit: release the files, then clear their recorded work.
pub async fn detach_released(pool: &PgPool, identity: &Identity, released: &[Released]) {
    for item in released {
        if release_in_core(identity, item).await {
            let _ = sqlx::query(
                "UPDATE media_files SET status = 'detached' WHERE id = $1 AND status = 'detaching'",
            )
            .bind(&item.id)
            .execute(pool)
            .await;
        }
    }
    retry_pending(pool, identity, released).await;
}

/// Finish detach work an earlier request left unconfirmed (at most ten files
/// per request; the core keeps their quota until then).
async fn retry_pending(pool: &PgPool, identity: &Identity, skip: &[Released]) {
    let Ok(rows) = sqlx::query_as::<_, (String, String)>(
        "SELECT id, file_id FROM media_files WHERE status = 'detaching' AND file_id IS NOT NULL \
         AND NOT (id = ANY($1)) ORDER BY created_at LIMIT 10",
    )
    .bind(skip.iter().map(|item| item.id.clone()).collect::<Vec<_>>())
    .fetch_all(pool)
    .await
    else {
        return;
    };
    for (id, file_id) in rows {
        let item = Released { id, file_id };
        if release_in_core(identity, &item).await {
            let _ = sqlx::query(
                "UPDATE media_files SET status = 'detached' WHERE id = $1 AND status = 'detaching'",
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
    #[schema(format = "uuid")]
    request_id: String,
    #[schema(min_length = 1, max_length = 255)]
    name: String,
    /// JPEG, PNG, GIF or WebP.
    mime: String,
    #[schema(minimum = 0, maximum = 10485760)]
    size: u64,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
enum BoardMediaStatus {
    Pending,
    Ready,
    Attached,
    Detaching,
    Detached,
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadResponse {
    id: String,
    /// A media reference of the form `media:<id>`.
    url: String,
    #[schema(value_type = BoardMediaStatus)]
    status: String,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[schema(value_type = String, format = Binary)]
struct ImageBytes(Vec<u8>);

fn media_not_found() -> ApiError {
    ApiError::not_found("media.not_found", "Media file not found.")
}

fn upload_failed() -> ApiError {
    ApiError::unavailable("media.upload_failed", "The file could not be stored.")
}

#[utoipa::path(
    post, path = "/v1/media/upload", operation_id = "reserveBoardMedia",
    request_body = UploadRequest,
    responses(
        (status = 201, description = "Created", body = UploadResponse),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 400, description = "Invalid request for this resource", body = crate::error::ErrorResponse),
        (status = 409, description = "Current state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 413, description = "Image exceeds 10 MiB", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 503, description = "A required service is unavailable", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn create_upload(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<UploadRequest>,
) -> ApiResult<(StatusCode, Json<UploadResponse>)> {
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
#[utoipa::path(
    put, path = "/files/media/{media_id}/content", operation_id = "uploadBoardMedia",
    params(
        ("media_id" = String, Path),
    ),
    request_body(content = inline(ImageBytes), content_type = "application/octet-stream"),
    description = "Streams the raw bytes of a reserved image through the app file gateway. The reservation and stored image are limited to 10 MiB.",
    responses(
        (status = 200, description = "Image stored; status is ready", body = UploadResponse),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 503, description = "A required service is unavailable", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
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

async fn stream(identity: &Identity, media: &MediaFile) -> ApiResult<Response> {
    if !matches!(media.status.as_str(), "ready" | "attached") {
        return Err(media_not_found());
    }
    let file_id = media.file_id.as_deref().ok_or_else(media_not_found)?;
    identity
        .files_download(file_id)
        .await
        .map_err(|_| ApiError::unavailable("media.download_failed", "The file could not be read."))
}

/// Can `identity` see a linked post's content without a password (AI-DO
/// `can_resolve_media` for community resources)?
async fn post_readable(pool: &PgPool, identity: &Identity, post_id: &str) -> ApiResult<bool> {
    let Some(loaded) = community::load_post(pool, post_id).await? else {
        return Ok(false);
    };
    let viewer = community::viewer(pool, identity).await?;
    Ok(community::post_read_locked_reason(&loaded, &viewer).is_none())
}

async fn comment_post(pool: &PgPool, comment_id: &str) -> ApiResult<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT post_id FROM community_comments WHERE id = $1 AND NOT is_deleted",
    )
    .bind(comment_id)
    .fetch_optional(pool)
    .await?)
}

/// `GET /files/media/{id}/content`: the uploader's own image (editor preview)
/// or an image of a post or comment the caller can read without a password.
#[utoipa::path(
    get, path = "/files/media/{media_id}/content", operation_id = "getBoardMedia",
    params(
        ("media_id" = String, Path),
    ),
    responses(
        (status = 200, description = "Image bytes",
         content(
            (inline(ImageBytes) = "image/png"),
            (inline(ImageBytes) = "image/jpeg"),
            (inline(ImageBytes) = "image/gif"),
            (inline(ImageBytes) = "image/webp")
         ),
         headers(
            ("Content-Length" = u64, description = "Number of image bytes"),
            ("Content-Disposition" = String, description = "Attachment filename"),
            ("ETag" = String, description = "Stored file entity tag"),
            ("X-Miy-File-Id" = String, description = "Core file ID"),
            ("X-Miy-File-Version" = String, description = "Core file version"),
            ("X-Miy-File-Sha256" = String, description = "SHA-256 digest"),
            ("Content-Security-Policy" = String, description = "Sandbox policy"),
            ("X-Content-Type-Options" = String, description = "nosniff")
         )),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 503, description = "A required service is unavailable", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
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
        (Some(RESOURCE_COMMUNITY_POST), Some(post_id)) => {
            post_readable(&state.pool, &identity, post_id).await?
        }
        (Some(RESOURCE_COMMUNITY_COMMENT), Some(comment_id)) => {
            match comment_post(&state.pool, comment_id).await? {
                Some(post_id) => post_readable(&state.pool, &identity, &post_id).await?,
                None => false,
            }
        }
        _ => false,
    };
    if !allowed {
        return Err(media_not_found());
    }
    stream(&identity, &media).await
}

#[derive(Deserialize, Default, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PostMediaRequest {
    #[serde(default)]
    #[schema(max_length = 200)]
    password: Option<String>,
}

/// `POST /files/community/posts/{post_id}/media/{id}`: AI-DO's
/// `resolve_post_media_urls` for one image, answering the bytes. A secret
/// post accepts its password here.
#[utoipa::path(
    post, path = "/files/community/posts/{post_id}/media/{media_id}", operation_id = "getCommunityPostMedia",
    params(
        ("post_id" = String, Path),
        ("media_id" = String, Path),
    ),
    request_body = ref("#/components/schemas/PostMediaRequest"),
    responses(
        (status = 200, description = "Image bytes",
         content(
            (inline(ImageBytes) = "image/png"),
            (inline(ImageBytes) = "image/jpeg"),
            (inline(ImageBytes) = "image/gif"),
            (inline(ImageBytes) = "image/webp")
         ),
         headers(
            ("Content-Length" = u64, description = "Number of image bytes"),
            ("Content-Disposition" = String, description = "Attachment filename"),
            ("ETag" = String, description = "Stored file entity tag"),
            ("X-Miy-File-Id" = String, description = "Core file ID"),
            ("X-Miy-File-Version" = String, description = "Core file version"),
            ("X-Miy-File-Sha256" = String, description = "SHA-256 digest"),
            ("Content-Security-Policy" = String, description = "Sandbox policy"),
            ("X-Content-Type-Options" = String, description = "nosniff")
         )),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 413, description = "Request body exceeds the size limit", body = String, content_type = "text/plain"),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 503, description = "A required service is unavailable", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn download_post_media(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((post_id, media_id)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> ApiResult<Response> {
    let payload: PostMediaRequest = if body.is_empty() {
        PostMediaRequest::default()
    } else {
        serde_json::from_slice(&body).map_err(|_| ApiError::validation("Invalid request body"))?
    };
    if let Some(password) = &payload.password {
        crate::error::check_length("password", password, 0, 200)?;
    }
    let loaded = community::load_post(&state.pool, &post_id)
        .await?
        .ok_or_else(|| {
            ApiError::not_found("community.post_not_found", "Community post not found.")
        })?;
    let viewer = community::viewer(&state.pool, &identity).await?;
    let denied = || ApiError::forbidden("community.invalid_password", "Password is invalid.");
    if community::post_read_locked_reason(&loaded, &viewer) == Some(LOCKED_REASON_ADMIN_ONLY) {
        return Err(denied());
    }
    if loaded.post.is_secret
        && loaded.post.author_id != viewer.id
        && !viewer.is_admin
        && !community::verify_post_password(&loaded.post, payload.password.as_deref()).await
    {
        return Err(denied());
    }
    let media = load_media(&state.pool, &media_id).await?;
    let belongs = match (media.resource_type.as_deref(), media.resource_id.as_deref()) {
        (Some(RESOURCE_COMMUNITY_POST), Some(id)) => id == loaded.post.id,
        (Some(RESOURCE_COMMUNITY_COMMENT), Some(id)) => {
            comment_post(&state.pool, id).await?.as_deref() == Some(loaded.post.id.as_str())
        }
        _ => false,
    };
    if !belongs {
        return Err(media_not_found());
    }
    stream(&identity, &media).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_ids_are_found_like_ai_do_pattern() {
        let a = "00000000-0000-4000-8000-000000000001";
        let b = "00000000-0000-4000-8000-0000000000ab";
        let body = format!("![x](media:{a}) text media:{b} again media:{a} media:XYZ");
        assert_eq!(media_ids_in(&body), vec![a.to_string(), b.to_string()]);
        assert!(media_ids_in("media:short").is_empty());
        assert!(media_ids_in("").is_empty());
    }
}
