//! Uploads (AI-DO `POST /files/upload`, multipart up to 250 MB, stored in
//! MinIO). miy keeps bytes only in platform Storage:
//!
//! 1. `POST /v1/spaces/{space}/files/uploads` writes the file and version rows
//!    with the request ID, then reserves Storage: one request (`single`, up
//!    to 10 MiB) or ranges (`parts`, up to 2 GiB). The same request ID
//!    recovers a lost answer.
//! 2. The browser streams the bytes through `runtime.fileApi`:
//!    `PUT /files/uploads/{id}/content` (single) or
//!    `PUT /files/uploads/{id}/parts?offset=N` with up to 8 MiB per request
//!    (parts), resuming from the committed `received` after a failure.
//! 3. `POST .../uploads/{id}/complete` publishes the bytes (Core hashes a
//!    parts file), rechecks access under locks, commits the version as the
//!    file's current one and only then attaches the Core file.
//!
//! `DELETE .../uploads/{id}` cancels an unfinished upload.
use axum::{
    Extension, Json,
    body::Body,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use miy_app_sdk::{
    FileCreate, FileMetadata, FileStatus, Identity,
    storage::{MAX_FILE_BYTES, MAX_LARGE_FILE_BYTES, MAX_PART_BYTES},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState, access,
    archive_planner::safe_filename,
    error::{ApiError, ApiJson, ApiResult},
    people,
    time::now_utc,
    tree::{self, FileItem},
    versions::{self, Version},
};

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[schema(rename_all = "snake_case")]
pub(crate) enum UploadTransfer {
    Single,
    Parts,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[schema(rename_all = "snake_case")]
pub(crate) enum UploadStatus {
    Reserving,
    Uploading,
    Ready,
    Completed,
    Cancelled,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct UploadOut {
    id: String,
    file_id: String,
    request_id: String,
    filename: String,
    content_type: String,
    size_bytes: i64,
    #[schema(value_type = UploadTransfer)]
    transfer: String,
    /// `reserving`, `uploading`, `ready` (all bytes stored), `completed`
    /// (the file shows this version) or `cancelled`.
    #[schema(value_type = UploadStatus)]
    status: String,
    received: u64,
    part_size: u64,
}

fn upload_out(version: &Version, received: u64, status: &str) -> UploadOut {
    UploadOut {
        id: version.id.clone(),
        file_id: version.file_id.clone(),
        request_id: version.request_id.clone(),
        filename: version.filename.clone(),
        content_type: version.content_type.clone(),
        size_bytes: version.size_bytes,
        transfer: version.transfer.clone(),
        status: status.to_string(),
        received,
        part_size: MAX_PART_BYTES,
    }
}

fn upload_not_found() -> ApiError {
    ApiError::not_found("files.upload_not_found", "Upload not found.")
}

fn upload_failed() -> ApiError {
    ApiError::new(
        StatusCode::BAD_GATEWAY,
        "files.storage_upload_failed",
        "File storage upload failed.",
    )
}

fn result_unknown() -> ApiError {
    ApiError::unavailable(
        "files.upload_result_unknown",
        "The storage answer was lost. Retry with the same request.",
    )
}

/// A definite Core refusal of a reservation. The SDK keeps only the status:
/// 409 is the installation quota (`file_quota_exceeded`), 503 low disk.
fn reservation_refused(status: u16) -> ApiError {
    match status {
        409 => ApiError::conflict(
            "files.storage_quota_exceeded",
            "The storage limit of this app has been reached.",
        ),
        503 => ApiError::unavailable(
            "files.storage_unavailable",
            "File storage is running low. Try again later.",
        ),
        413 | 422 | 400 => ApiError::too_large(
            "files.file_size_limit_exceeded",
            format!(
                "Files must be {} MB or smaller.",
                MAX_LARGE_FILE_BYTES / 1024 / 1024
            ),
        ),
        _ => upload_failed(),
    }
}

/// The upload's version and its space, for the person who started it.
async fn own_upload(
    state: &AppState,
    identity: &Identity,
    upload_id: &str,
) -> ApiResult<(Version, String)> {
    let version = versions::load(&state.pool, upload_id)
        .await?
        .filter(|v| v.uploaded_by == identity.user_id)
        .ok_or_else(upload_not_found)?;
    let space_id: String =
        sqlx::query_scalar("SELECT workspace_id FROM file_manager_files WHERE id = $1")
            .bind(&version.file_id)
            .fetch_optional(&state.pool)
            .await?
            .ok_or_else(upload_not_found)?;
    Ok((version, space_id))
}

/// The upload as Core sees it now.
async fn current_state(
    _state: &AppState,
    identity: &Identity,
    version: &Version,
) -> ApiResult<UploadOut> {
    if version.status == "cancelled" {
        return Ok(upload_out(version, 0, "cancelled"));
    }
    if version.status == "ready" {
        return Ok(upload_out(version, version.size_bytes as u64, "completed"));
    }
    let Some(storage) = version.storage_file_id.as_deref() else {
        return Ok(upload_out(version, 0, "reserving"));
    };
    let file = identity
        .files_get(storage)
        .await
        .map_err(|_| result_unknown())?;
    Ok(match file.status {
        FileStatus::Pending => upload_out(version, file.received, "uploading"),
        FileStatus::Ready | FileStatus::Attached => upload_out(version, file.size, "ready"),
        FileStatus::Cancelled | FileStatus::Expired => upload_out(version, 0, "cancelled"),
    })
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UploadRequest {
    request_id: String,
    filename: String,
    #[serde(default)]
    content_type: Option<String>,
    size_bytes: i64,
    #[serde(default)]
    folder_id: Option<String>,
    #[serde(default = "crate::files::default_visibility")]
    #[schema(value_type = crate::tree::FileVisibility)]
    visibility: String,
    /// miy (versions): upload a new version of this file.
    #[serde(default)]
    file_id: Option<String>,
}

fn content_type_of(value: Option<&str>) -> String {
    match value.map(str::trim) {
        Some(value)
            if !value.is_empty()
                && value.chars().count() <= 160
                && !value.chars().any(char::is_control) =>
        {
            value.to_string()
        }
        _ => "application/octet-stream".to_string(),
    }
}

/// Reserve Core storage for a version whose request ID is recorded.
async fn reserve(state: &AppState, identity: &Identity, version: &Version) -> ApiResult<UploadOut> {
    let input = FileCreate {
        request_id: version.request_id.clone(),
        name: version.filename.clone(),
        mime: version.content_type.clone(),
        size: version.size_bytes as u64,
    };
    let result = if version.transfer == "parts" {
        identity.files_create_parts(&input).await
    } else {
        identity.files_create(&input).await
    };
    let file: FileMetadata = match result {
        Ok(file) => file,
        Err(error) if error.status.is_some() => {
            // Definite refusal: nothing was reserved.
            sqlx::query("UPDATE file_manager_file_versions SET status = 'cancelled' WHERE id = $1")
                .bind(&version.id)
                .execute(&state.pool)
                .await?;
            abandon_new_file(state, version).await?;
            return Err(reservation_refused(error.status.unwrap_or(502)));
        }
        Err(_) => match identity.files_by_request(&version.request_id).await {
            Ok(file) => file,
            Err(_) => return Err(result_unknown()),
        },
    };
    let storage = uuid::Uuid::parse_str(&file.id).map_err(|_| upload_failed())?;
    sqlx::query(
        "UPDATE file_manager_file_versions SET storage_file_id = $2, status = 'uploading' \
         WHERE id = $1 AND status = 'reserving'",
    )
    .bind(&version.id)
    .bind(storage)
    .execute(&state.pool)
    .await?;
    let mut updated = version.clone();
    updated.storage_file_id = Some(file.id.clone());
    updated.status = "uploading".into();
    Ok(match file.status {
        FileStatus::Pending => upload_out(&updated, file.received, "uploading"),
        FileStatus::Ready | FileStatus::Attached => upload_out(&updated, file.size, "ready"),
        _ => upload_out(&updated, 0, "cancelled"),
    })
}

/// A first upload that never completed leaves no visible file.
async fn abandon_new_file(state: &AppState, version: &Version) -> ApiResult<()> {
    sqlx::query(
        "UPDATE file_manager_files SET deleted_at = $2 WHERE id = $1 \
         AND current_version_id IS NULL AND deleted_at IS NULL",
    )
    .bind(&version.file_id)
    .bind(now_utc())
    .execute(&state.pool)
    .await?;
    Ok(())
}

/// `POST /v1/spaces/{space_id}/files/uploads`.
#[utoipa::path(
    post, path = "/v1/spaces/{space_id}/files/uploads", operation_id = "reserveDriveUpload", tag = "File transfer",
    request_body = UploadRequest,
    params(("space_id" = String, Path)),
    responses(
        (status = 201, description = "New reservation", body = UploadOut),
        (status = 200, description = "Same reservation recovered by request ID", body = UploadOut),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 409, description = "Request ID conflict or Storage quota exceeded", body = crate::error::ErrorResponse),
        (status = 413, description = "File or selection exceeds the supported limit", body = crate::error::ErrorResponse),
        (status = 422, description = "Invalid or unsupported request", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 502, description = "Storage rejected the operation", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn create_upload(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    ApiJson(payload): ApiJson<UploadRequest>,
) -> ApiResult<impl IntoResponse> {
    let request_id = uuid::Uuid::parse_str(&payload.request_id)
        .map_err(|_| ApiError::validation("request_id: Input should be a valid UUID"))?;
    if payload.size_bytes < 0 {
        return Err(ApiError::validation(
            "size_bytes: Input should be greater than or equal to 0",
        ));
    }
    if payload.size_bytes as u64 > MAX_LARGE_FILE_BYTES {
        return Err(ApiError::too_large(
            "files.file_size_limit_exceeded",
            format!(
                "Files must be {} MB or smaller.",
                MAX_LARGE_FILE_BYTES / 1024 / 1024
            ),
        ));
    }
    // The same request ID returns the first upload (a lost answer).
    let sql = format!(
        "SELECT {} FROM file_manager_file_versions WHERE request_id = $1",
        versions::VERSION_COLUMNS
    );
    if let Some(existing) = sqlx::query_as::<_, Version>(&sql)
        .bind(request_id)
        .fetch_optional(&state.pool)
        .await?
    {
        let (existing, existing_space) = own_upload(&state, &identity, &existing.id).await?;
        if existing_space != space_id {
            return Err(upload_not_found());
        }
        let out = if existing.status == "reserving" {
            reserve(&state, &identity, &existing).await?
        } else {
            current_state(&state, &identity, &existing).await?
        };
        return Ok((StatusCode::OK, Json(out)));
    }
    let filename = safe_filename(Some(&payload.filename));
    let content_type = content_type_of(payload.content_type.as_deref());
    let transfer = if payload.size_bytes as u64 > MAX_FILE_BYTES {
        "parts"
    } else {
        "single"
    };
    let groups = access::frame_groups(&identity).await?;
    let mut tx = state.pool.begin().await?;
    let space = access::lock_space(&mut tx, &space_id).await?;
    let now = now_utc();
    let version_id = uuid::Uuid::new_v4().to_string();
    let (file_id, version_number) = match payload.file_id.as_deref().filter(|id| !id.is_empty()) {
        Some(file_id) => {
            let file = tree::lock_file(&mut tx, &space_id, file_id).await?;
            let access = access::recheck(&state, &mut tx, &identity, space, &groups).await?;
            if !tree::record_visible(&file.visibility, &file.owner_id, &access) {
                return Err(tree::file_access_required());
            }
            if !tree::can_manage(&file.owner_id, &access) {
                return Err(tree::delete_access_required());
            }
            let next: i32 = sqlx::query_scalar(
                "SELECT COALESCE(max(version_number), 0) + 1 FROM file_manager_file_versions WHERE file_id = $1",
            )
            .bind(&file.id)
            .fetch_one(&mut *tx)
            .await?;
            (file.id, next)
        }
        None => {
            let folder_id = payload.folder_id.clone().filter(|id| !id.is_empty());
            let parent = match &folder_id {
                Some(id) => Some(tree::lock_folder(&mut tx, &space_id, id, false).await?),
                None => None,
            };
            let access = access::recheck(&state, &mut tx, &identity, space, &groups).await?;
            if let Some(parent) = &parent {
                let accessible = tree::list_accessible_folders(&mut *tx, &access).await?;
                if !accessible.iter().any(|f| f.id == parent.id) {
                    return Err(tree::folder_not_found());
                }
            }
            tree::ensure_can_ingest(&access, parent.as_ref())?;
            let visibility = match &parent {
                Some(parent) => parent.visibility.clone(),
                None => tree::normalize_visibility(&payload.visibility)?,
            };
            let file_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO file_manager_files (id, workspace_id, folder_id, owner_id, filename, content_type, \
                 size_bytes, visibility, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)",
            )
            .bind(&file_id)
            .bind(&space_id)
            .bind(&folder_id)
            .bind(&identity.user_id)
            .bind(&filename)
            .bind(&content_type)
            .bind(payload.size_bytes)
            .bind(&visibility)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            (file_id, 1)
        }
    };
    let sql = format!(
        "INSERT INTO file_manager_file_versions (id, file_id, version_number, request_id, attach_request_id, \
         filename, content_type, size_bytes, transfer, status, uploaded_by, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'reserving', $10, $11) RETURNING {}",
        versions::VERSION_COLUMNS
    );
    let version = sqlx::query_as::<_, Version>(&sql)
        .bind(&version_id)
        .bind(&file_id)
        .bind(version_number)
        .bind(request_id)
        .bind(uuid::Uuid::new_v4())
        .bind(&filename)
        .bind(&content_type)
        .bind(payload.size_bytes)
        .bind(transfer)
        .bind(&identity.user_id)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    let out = reserve(&state, &identity, &version).await?;
    Ok((StatusCode::CREATED, Json(out)))
}

/// `GET /v1/spaces/{space_id}/files/uploads/{upload_id}`: the committed
/// offset to resume from.
#[utoipa::path(
    get, path = "/v1/spaces/{space_id}/files/uploads/{upload_id}", operation_id = "getDriveUpload", tag = "File transfer",
    params(("space_id" = String, Path), ("upload_id" = String, Path)),
    responses(
        (status = 200, description = "Upload and its committed byte offset", body = UploadOut),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn get_upload(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, upload_id)): Path<(String, String)>,
) -> ApiResult<Json<UploadOut>> {
    let (version, version_space) = own_upload(&state, &identity, &upload_id).await?;
    if version_space != space_id {
        return Err(upload_not_found());
    }
    access::read_access(&state, &identity, &space_id).await?;
    Ok(Json(current_state(&state, &identity, &version).await?))
}

/// Bytes go only to the person's own unfinished upload in a space they may
/// still open; completion checks the full rules again under locks.
async fn sendable(
    state: &AppState,
    identity: &Identity,
    upload_id: &str,
    transfer: &str,
) -> ApiResult<(Version, String)> {
    let (version, space_id) = own_upload(state, identity, upload_id).await?;
    access::read_access(state, identity, &space_id).await?;
    if version.transfer != transfer {
        return Err(ApiError::conflict(
            "files.upload_transfer_mismatch",
            "This upload expects another transfer.",
        ));
    }
    if version.status == "cancelled" {
        return Err(ApiError::conflict(
            "files.upload_cancelled",
            "This upload was cancelled.",
        ));
    }
    let storage = version.storage_file_id.clone().ok_or_else(result_unknown)?;
    Ok((version, storage))
}

/// `PUT /files/uploads/{upload_id}/content` (fileApi): a single upload.
#[utoipa::path(
    put, path = "/files/uploads/{upload_id}/content", operation_id = "putDriveUploadContent", tag = "File transfer",
    request_body(content = super::api_contract::BinaryBody, content_type = "application/octet-stream"),
    params(("upload_id" = String, Path)),
    responses(
        (status = 200, description = "Committed upload state after sending at most 10 MiB", body = UploadOut),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 409, description = "Resource state changed or conflicts with this request", body = crate::error::ErrorResponse),
        (status = 413, description = "File or selection exceeds the supported limit", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 502, description = "Storage rejected the operation", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn upload_content(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(upload_id): Path<String>,
    body: Body,
) -> ApiResult<Json<UploadOut>> {
    let (version, storage) = sendable(&state, &identity, &upload_id, "single").await?;
    let file = identity
        .files_get(&storage)
        .await
        .map_err(|_| result_unknown())?;
    if file.status == FileStatus::Pending {
        if let Err(error) = identity.files_upload(&file, body).await {
            // Uncertain or refused: report what Core holds now.
            let now = identity
                .files_get(&storage)
                .await
                .map_err(|_| result_unknown())?;
            if now.status == FileStatus::Pending {
                return Err(if error.status.is_some() {
                    upload_failed()
                } else {
                    result_unknown()
                });
            }
        }
    }
    Ok(Json(current_state(&state, &identity, &version).await?))
}

#[derive(Deserialize)]
pub struct PartQuery {
    offset: u64,
}

/// `PUT /files/uploads/{upload_id}/parts?offset=N` (fileApi): one range of a
/// large upload, at the committed offset. The answer carries the new
/// `received`; after a failure it carries the offset to resume from.
#[utoipa::path(
    put, path = "/files/uploads/{upload_id}/parts", operation_id = "putDriveUploadPart", tag = "File transfer",
    request_body(content = super::api_contract::BinaryBody, content_type = "application/octet-stream"),
    params(("upload_id" = String, Path), ("offset" = u64, Query, description = "Committed byte offset at which the part starts")),
    responses(
        (status = 200, description = "Upload state after sending a part of at most 8 MiB, including the committed next offset", body = UploadOut),
        (status = 400, description = "Missing or invalid offset query parameter", body = String, content_type = "text/plain"),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 409, description = "Offset does not match the committed position, or upload state changed", body = crate::error::ErrorResponse),
        (status = 413, description = "Part exceeds 8 MiB", body = crate::error::ErrorResponse),
        (status = 422, description = "Part is empty or would extend beyond the reserved file size", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn upload_part(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(upload_id): Path<String>,
    Query(query): Query<PartQuery>,
    body: Body,
) -> ApiResult<Json<UploadOut>> {
    let (version, storage) = sendable(&state, &identity, &upload_id, "parts").await?;
    let bytes = axum::body::to_bytes(body, MAX_PART_BYTES as usize)
        .await
        .map_err(|_| ApiError::too_large("files.part_too_large", "A part is at most 8 MiB."))?;
    let file = identity
        .files_get(&storage)
        .await
        .map_err(|_| result_unknown())?;
    if file.status != FileStatus::Pending {
        return Ok(Json(current_state(&state, &identity, &version).await?));
    }
    if query.offset != file.received {
        return Err(ApiError::conflict(
            "files.part_offset",
            "Resume from the committed offset.",
        ));
    }
    if bytes.is_empty() || file.received + bytes.len() as u64 > file.size {
        return Err(ApiError::validation("The part does not fit the file."));
    }
    match identity.files_upload_part(&file, bytes).await {
        Ok(next) => Ok(Json(upload_out(&version, next.received, "uploading"))),
        Err(_) => Ok(Json(current_state(&state, &identity, &version).await?)),
    }
}

/// `POST /v1/spaces/{space_id}/files/uploads/{upload_id}/complete`.
#[utoipa::path(
    post, path = "/v1/spaces/{space_id}/files/uploads/{upload_id}/complete", operation_id = "completeDriveUpload", tag = "File transfer",
    params(("space_id" = String, Path), ("upload_id" = String, Path)),
    responses(
        (status = 200, description = "Published file after successful completion", body = FileItem),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 409, description = "Upload cancelled, incomplete or still finalizing. Retry finalizing with the same upload ID", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn complete_upload(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, upload_id)): Path<(String, String)>,
) -> ApiResult<Json<FileItem>> {
    let (version, version_space) = own_upload(&state, &identity, &upload_id).await?;
    if version_space != space_id {
        return Err(upload_not_found());
    }
    if version.status == "cancelled" {
        return Err(ApiError::conflict(
            "files.upload_cancelled",
            "This upload was cancelled.",
        ));
    }
    // A completed version stays completed (a newer one may be current now).
    if version.status != "ready" {
        let storage = version.storage_file_id.clone().ok_or_else(result_unknown)?;
        let file = if version.transfer == "parts" {
            match identity.files_complete(&storage).await {
                Ok(file) => file,
                Err(error) if matches!(error.status, Some(401 | 403)) => {
                    return Err(ApiError::forbidden(
                        "files.storage_access_required",
                        "Storage write access is required to complete the upload.",
                    ));
                }
                Err(_) => identity
                    .files_get(&storage)
                    .await
                    .map_err(|_| result_unknown())?,
            }
        } else {
            identity
                .files_get(&storage)
                .await
                .map_err(|_| result_unknown())?
        };
        if !matches!(file.status, FileStatus::Ready | FileStatus::Attached) {
            if file.status == FileStatus::Pending && file.received == file.size {
                return Err(ApiError::conflict(
                    "files.upload_finalizing",
                    "Storage is verifying the uploaded bytes. Retry completion with the same upload.",
                ));
            }
            return Err(ApiError::conflict(
                "files.upload_incomplete",
                "Not all bytes have arrived yet.",
            ));
        }
        let groups = access::frame_groups(&identity).await?;
        let mut tx = state.pool.begin().await?;
        let space = access::lock_space(&mut tx, &space_id).await?;
        sqlx::query("SELECT id FROM file_manager_files WHERE id = $1 FOR UPDATE")
            .bind(&version.file_id)
            .execute(&mut *tx)
            .await?;
        let locked = versions::load(&mut *tx, &version.id)
            .await?
            .ok_or_else(upload_not_found)?;
        let row = tree::load_file(&mut *tx, &version.file_id)
            .await?
            .ok_or_else(upload_not_found)?;
        if locked.status == "cancelled" {
            return Err(ApiError::conflict(
                "files.upload_cancelled",
                "This upload was cancelled.",
            ));
        }
        if locked.status == "ready" {
            // A concurrent request completed it.
            tx.rollback().await?;
            return finish(&state, &identity, &space_id, &version).await;
        }
        if row.deleted_at.is_some() {
            return Err(tree::file_not_found());
        }
        let parent = match (&row.current_version_id, &row.folder_id) {
            (None, Some(folder)) => {
                Some(tree::lock_folder(&mut tx, &space_id, folder, false).await?)
            }
            _ => None,
        };
        let access = access::recheck(&state, &mut tx, &identity, space, &groups).await?;
        if row.current_version_id.is_none() {
            if let Some(parent) = &parent {
                let accessible = tree::list_accessible_folders(&mut *tx, &access).await?;
                if !accessible.iter().any(|f| f.id == parent.id) {
                    return Err(tree::folder_not_found());
                }
            }
            tree::ensure_can_ingest(&access, parent.as_ref())?;
        } else if !tree::can_manage(&row.owner_id, &access) {
            return Err(tree::delete_access_required());
        }
        let now = now_utc();
        sqlx::query(
            "UPDATE file_manager_file_versions SET status = 'ready', sha256 = $2, completed_at = $3 WHERE id = $1",
        )
        .bind(&version.id)
        .bind(&file.sha256)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE file_manager_files SET current_version_id = $2, size_bytes = $3, content_type = $4, \
             updated_at = $5 WHERE id = $1",
        )
        .bind(&version.file_id)
        .bind(&version.id)
        .bind(file.size as i64)
        .bind(&version.content_type)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
    }
    finish(&state, &identity, &space_id, &version).await
}

/// Attach only after the app reference has committed, then answer the file.
async fn finish(
    state: &AppState,
    identity: &Identity,
    space_id: &str,
    version: &Version,
) -> ApiResult<Json<FileItem>> {
    let committed = versions::load(&state.pool, &version.id)
        .await?
        .ok_or_else(upload_not_found)?;
    versions::ensure_attached(identity, &state.pool, &committed).await;
    let access = access::read_access(state, identity, space_id).await?;
    let row = tree::load_file(&state.pool, &version.file_id)
        .await?
        .ok_or_else(tree::file_not_found)?;
    let names = people::display_names(identity, [row.owner_id.clone()]).await;
    Ok(Json(tree::file_item(&row, &access, &names)))
}

/// `DELETE /v1/spaces/{space_id}/files/uploads/{upload_id}`: cancel an
/// upload that has not completed. A completed upload is a file: delete it.
#[utoipa::path(
    delete, path = "/v1/spaces/{space_id}/files/uploads/{upload_id}", operation_id = "cancelDriveUpload", tag = "File transfer",
    params(("space_id" = String, Path), ("upload_id" = String, Path)),
    responses(
        (status = 204, description = "Unfinished upload cancelled"),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 409, description = "Resource state changed or conflicts with this request", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn cancel_upload(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, upload_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let (version, version_space) = own_upload(&state, &identity, &upload_id).await?;
    if version_space != space_id {
        return Err(upload_not_found());
    }
    let mut tx = state.pool.begin().await?;
    sqlx::query("SELECT id FROM file_manager_files WHERE id = $1 FOR UPDATE")
        .bind(&version.file_id)
        .execute(&mut *tx)
        .await?;
    let locked = versions::load(&mut *tx, &version.id)
        .await?
        .ok_or_else(upload_not_found)?;
    if locked.status == "ready" {
        return Err(ApiError::conflict(
            "files.upload_completed",
            "This upload has completed.",
        ));
    }
    sqlx::query("UPDATE file_manager_file_versions SET status = 'cancelled' WHERE id = $1")
        .bind(&version.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE file_manager_files SET deleted_at = $2 WHERE id = $1 \
         AND current_version_id IS NULL AND deleted_at IS NULL",
    )
    .bind(&version.file_id)
    .bind(now_utc())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    // No app reference exists: release the reservation now (it would also
    // expire after 24 hours).
    let storage = match locked.storage_file_id {
        Some(storage) => Some(storage),
        None => identity
            .files_by_request(&locked.request_id)
            .await
            .ok()
            .map(|file| file.id),
    };
    if let Some(storage) = storage {
        let _ = identity.files_cancel(&storage).await;
    }
    Ok(StatusCode::NO_CONTENT)
}
