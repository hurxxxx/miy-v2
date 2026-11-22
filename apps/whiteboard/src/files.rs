//! Board images in the core storage (보강, `platform:files:*`).
//!
//! AI-DO kept every image of a board inline in the scene: Excalidraw's
//! `files` map held `{id, mimeType, dataURL, …}` and the whole scene was
//! saved with it. In miy the bytes go to the core storage (10 MiB per file)
//! and the scene keeps only `{id, mimeType, created, storage: "<row id>"}`:
//!
//! 1. `POST …/items/{id}/files` records the intent (request ID) in the app DB
//!    and reserves a core file; a lost answer is repeated with the same
//!    request ID and returns the same row and its status;
//! 2. the frame streams the bytes with `runtime.fileApi(PUT
//!    /whiteboard/items/{id}/files/{row})`; a lost answer is repeated and
//!    reports the stored state;
//! 3. a scene save that names the row attaches it in the core once, after the
//!    app commit (request ID = row ID, so a repeat is the same attach);
//! 4. viewers read the bytes with `runtime.fileApi(GET …)` after the board's
//!    access is checked again;
//! 5. deleting the board permanently releases its images (detach, or cancel
//!    for bytes never attached).
//!
//! Every call checks the board's current access first. Raster images only
//! (PNG, JPEG, GIF, WebP): the gateway and common preview do not serve SVG.
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
use std::collections::HashMap;

use crate::{
    AppState,
    api_contract::BinaryBody,
    error::{ApiError, ApiJson, ApiResult, ErrorResponse, IdentityErrorResponse, check_length},
    spaces,
    time::now_utc,
    whiteboard::{self, Board},
};

pub const ALLOWED_IMAGE_TYPES: [&str; 4] = ["image/gif", "image/jpeg", "image/png", "image/webp"];

#[derive(Clone, Debug, FromRow)]
pub struct StoredFile {
    pub id: String,
    pub whiteboard_id: Option<String>,
    pub excalidraw_file_id: String,
    pub file_id: Option<String>,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i32,
    pub uploaded_by_id: String,
    pub status: String,
}

const FILE_COLUMNS: &str = "id, whiteboard_id, excalidraw_file_id, file_id, filename, content_type, \
     size_bytes, uploaded_by_id, status";

/// A row released with its board (status `detaching`).
#[derive(Clone, Debug)]
pub struct Released {
    pub id: String,
    pub file_id: String,
}

fn reference(row_id: &str) -> String {
    format!("whiteboard_file:{row_id}")
}

fn file_not_found() -> ApiError {
    ApiError::not_found("media.not_found", "Media file not found.")
}

fn upload_failed() -> ApiError {
    ApiError::unavailable("media.upload_failed", "The file could not be stored.")
}

/// After the scene that names them commits: attach the board's `ready` rows
/// in the core. The row ID is the attach request ID, so a repeated attach is
/// the same request; an uncertain answer is looked up, and a failure leaves
/// the row `ready` for the next save.
pub async fn attach_referenced(
    pool: &PgPool,
    identity: &Identity,
    board_id: &str,
    row_ids: &[String],
) {
    if row_ids.is_empty() {
        return;
    }
    let Ok(rows) = sqlx::query_as::<_, (String, String)>(
        "SELECT id, file_id FROM whiteboard_files WHERE id = ANY($1) AND whiteboard_id = $2 \
         AND status = 'ready' AND file_id IS NOT NULL",
    )
    .bind(row_ids)
    .bind(board_id)
    .fetch_all(pool)
    .await
    else {
        return;
    };
    for (id, file_id) in rows {
        let reference = reference(&id);
        let attachment = match identity.files_attach(&file_id, &reference, &id).await {
            Ok(file) => Ok(file),
            Err(_) => identity.files_get(&file_id).await,
        };
        if attachment.is_ok_and(|file| {
            file.status == FileStatus::Attached
                && file.reference.as_deref() == Some(reference.as_str())
        }) {
            let _ = sqlx::query(
                "UPDATE whiteboard_files SET status = 'attached' WHERE id = $1 AND status = 'ready'",
            )
            .bind(&id)
            .execute(pool)
            .await;
        }
    }
}

/// With the board's permanent delete: every stored image of the board is
/// marked `detaching` in the same transaction and unlinked from the board.
pub async fn release_for_board(
    tx: &mut Transaction<'_, Postgres>,
    board_id: &str,
) -> ApiResult<Vec<Released>> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "UPDATE whiteboard_files SET whiteboard_id = NULL, \
           status = CASE WHEN file_id IS NULL THEN 'detached' ELSE 'detaching' END \
         WHERE whiteboard_id = $1 AND status IN ('pending', 'ready', 'attached') \
         RETURNING id, file_id",
    )
    .bind(board_id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, file_id)| file_id.map(|file_id| Released { id, file_id }))
        .collect())
}

/// Remove one released file from the core: detach it when attached, cancel
/// it when it never was. Repeating is safe.
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
/// Work an earlier request left unconfirmed is finished too (at most ten).
pub async fn detach_released(pool: &PgPool, identity: &Identity, released: &[Released]) {
    let mut work = released.to_vec();
    if let Ok(rows) = sqlx::query_as::<_, (String, String)>(
        "SELECT id, file_id FROM whiteboard_files WHERE status = 'detaching' AND file_id IS NOT NULL \
         AND NOT (id = ANY($1)) ORDER BY created_at LIMIT 10",
    )
    .bind(released.iter().map(|item| item.id.clone()).collect::<Vec<_>>())
    .fetch_all(pool)
    .await
    {
        work.extend(rows.into_iter().map(|(id, file_id)| Released { id, file_id }));
    }
    for item in work {
        if release_in_core(identity, &item).await {
            let _ = sqlx::query(
                "UPDATE whiteboard_files SET status = 'detached' WHERE id = $1 AND status = 'detaching'",
            )
            .bind(&item.id)
            .execute(pool)
            .await;
        }
    }
}

// ---------------------------------------------------------------- routes

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ReserveRequest {
    #[schema(format = "uuid")]
    request_id: String,
    /// Excalidraw's file ID (the key in `scene.files`).
    #[schema(min_length = 1, max_length = 128)]
    file_id: String,
    /// The trimmed filename may contain up to 255 characters and no control characters.
    name: String,
    /// PNG, JPEG, GIF or WebP. The server trims and lowercases this value.
    mime: String,
    /// Exact size in bytes, up to 10 MiB.
    #[schema(minimum = 1, maximum = 10485760)]
    size: u64,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[schema(rename_all = "snake_case")]
enum WhiteboardFileStatus {
    Pending,
    Ready,
    Attached,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct FileResponse {
    /// Board file row ID to save as `storage` in the scene.
    id: String,
    /// Excalidraw's file ID.
    file_id: String,
    #[schema(value_type = WhiteboardFileStatus)]
    status: String,
}

fn response(row: &StoredFile, status: &str) -> FileResponse {
    FileResponse {
        id: row.id.clone(),
        file_id: row.excalidraw_file_id.clone(),
        status: status.to_string(),
    }
}

/// `POST …/items/{item_id}/files`: reserve a stored image of the board.
/// Needs edit access to the board. Repeating the same request ID with the
/// same file answers the same row (and its current status); another payload
/// under that request ID is a conflict.
#[utoipa::path(
    post, path = "/items/{item_id}/files", operation_id = "reserveWhiteboardFile", tag = "Whiteboard files",
    params(("item_id" = String, Path)),
    request_body = ReserveRequest,
    responses(
        (status = 201, description = "Image reservation and its current status", body = FileResponse),
        (status = 400, description = "The image media type is unsupported", body = ErrorResponse),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller cannot edit this board", body = ErrorResponse),
        (status = 404, description = "The workspace or board is missing or is not visible", body = ErrorResponse),
        (status = 409, description = "This request ID already belongs to a different reservation", body = ErrorResponse),
        (status = 413, description = "File size must be between 1 byte and 10 MiB", body = ErrorResponse),
        (status = 422, description = "Invalid image reservation", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn reserve(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(payload): ApiJson<ReserveRequest>,
) -> ApiResult<impl IntoResponse> {
    let space_id = params
        .get("space_id")
        .cloned()
        .unwrap_or_else(|| spaces::GLOBAL.to_string());
    let item_id = params
        .get("item_id")
        .cloned()
        .ok_or_else(whiteboard::not_found)?;
    let request_id = uuid::Uuid::parse_str(&payload.request_id)
        .map_err(|_| ApiError::validation("request_id: Input should be a valid UUID"))?;
    check_length("file_id", &payload.file_id, 1, 128)?;
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
    if payload.size == 0 || payload.size > MAX_FILE_BYTES {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "media.file_size_limit_exceeded",
            "File size exceeds the 10 MB limit.",
        ));
    }
    spaces::ensure_exists(&state.pool, &space_id).await?;

    let mut tx = state.pool.begin().await?;
    let loaded = whiteboard::lock_for_user(&mut tx, &state, &identity, &space_id, &item_id).await?;
    if loaded.board.trashed_at.is_some() {
        return Err(whiteboard::not_found());
    }
    if !loaded.access.can_edit {
        return Err(ApiError::forbidden(
            "whiteboard.edit_access_required",
            "Whiteboard edit access required.",
        ));
    }
    // Persist the reservation intent before reserving (CONTRACT "Small attachments").
    let sql = format!(
        "INSERT INTO whiteboard_files (id, whiteboard_id, excalidraw_file_id, request_id, filename, content_type, \
           size_bytes, uploaded_by_id, status, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending', $9) \
         ON CONFLICT (request_id) DO UPDATE SET request_id = EXCLUDED.request_id RETURNING {FILE_COLUMNS}"
    );
    let row = sqlx::query_as::<_, StoredFile>(&sql)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&loaded.board.id)
        .bind(&payload.file_id)
        .bind(request_id)
        .bind(&name)
        .bind(&mime)
        .bind(payload.size as i32)
        .bind(&identity.user_id)
        .bind(now_utc())
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    if row.uploaded_by_id != identity.user_id
        || row.whiteboard_id.as_deref() != Some(loaded.board.id.as_str())
        || row.excalidraw_file_id != payload.file_id
        || row.filename != name
        || row.content_type != mime
        || row.size_bytes != payload.size as i32
    {
        return Err(ApiError::conflict(
            "media.request_conflict",
            "The request ID is already in use.",
        ));
    }
    if row.file_id.is_none() {
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
        sqlx::query("UPDATE whiteboard_files SET file_id = $2 WHERE id = $1 AND file_id IS NULL")
            .bind(&row.id)
            .bind(&file.id)
            .execute(&state.pool)
            .await?;
    }
    let status = row.status.clone();
    Ok((StatusCode::CREATED, Json(response(&row, &status))))
}

async fn load_row(pool: &PgPool, board: &Board, row_id: &str) -> ApiResult<StoredFile> {
    let sql =
        format!("SELECT {FILE_COLUMNS} FROM whiteboard_files WHERE id = $1 AND whiteboard_id = $2");
    sqlx::query_as::<_, StoredFile>(&sql)
        .bind(row_id)
        .bind(&board.id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(file_not_found)
}

/// The board of a file route with the caller's current access. File routes
/// carry no space; the board's own space decides (the same answer its item
/// routes give).
async fn board_of(
    state: &AppState,
    identity: &Identity,
    item_id: &str,
) -> ApiResult<whiteboard::Loaded> {
    let space_id: String = sqlx::query_scalar("SELECT workspace_id FROM whiteboards WHERE id = $1")
        .bind(item_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(whiteboard::not_found)?;
    whiteboard::load_for_user(state, identity, &space_id, item_id).await
}

/// `PUT /files/whiteboard/items/{item_id}/files/{file_id}`: the uploader
/// streams the reserved bytes while they may still edit the board. A repeat
/// after a lost answer reports the stored state.
#[utoipa::path(
    put, path = "/files/whiteboard/items/{item_id}/files/{file_id}", operation_id = "uploadWhiteboardFile", tag = "Whiteboard files",
    description = "Use the authenticated frame file gateway. Send the exact number of reserved bytes, up to 10 MiB. Only the person who reserved the file may upload it, while they can edit the board.",
    params(("item_id" = String, Path), ("file_id" = String, Path, description = "Board file row ID returned as id by the reservation")),
    request_body(content = BinaryBody, content_type = "application/octet-stream"),
    responses(
        (status = 200, description = "Current image file status", body = FileResponse),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller cannot edit this board", body = ErrorResponse),
        (status = 404, description = "The board or image is missing or is not available to the caller", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "The image could not be stored or a required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn upload(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((item_id, row_id)): Path<(String, String)>,
    body: Body,
) -> ApiResult<Json<FileResponse>> {
    let loaded = board_of(&state, &identity, &item_id).await?;
    if loaded.board.trashed_at.is_some() {
        return Err(whiteboard::not_found());
    }
    if !loaded.access.can_edit {
        return Err(ApiError::forbidden(
            "whiteboard.edit_access_required",
            "Whiteboard edit access required.",
        ));
    }
    let row = load_row(&state.pool, &loaded.board, &row_id).await?;
    if row.uploaded_by_id != identity.user_id {
        return Err(file_not_found());
    }
    if !matches!(row.status.as_str(), "pending" | "ready" | "attached") {
        return Err(file_not_found());
    }
    let file_id = row.file_id.clone().ok_or_else(file_not_found)?;
    if row.status == "pending" {
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
        sqlx::query(
            "UPDATE whiteboard_files SET status = 'ready' WHERE id = $1 AND status = 'pending'",
        )
        .bind(&row.id)
        .execute(&state.pool)
        .await?;
        return Ok(Json(response(&row, "ready")));
    }
    Ok(Json(response(&row, &row.status)))
}

/// `GET /files/whiteboard/items/{item_id}/files/{file_id}`: the bytes, for
/// anyone who may currently see the board.
#[utoipa::path(
    get, path = "/files/whiteboard/items/{item_id}/files/{file_id}", operation_id = "downloadWhiteboardFile", tag = "Whiteboard files",
    description = "Use the authenticated frame file gateway. Rechecks current board access before streaming an available image, up to 10 MiB.",
    params(("item_id" = String, Path), ("file_id" = String, Path, description = "Board file row ID returned as id by the reservation")),
    responses(
        (status = 200, description = "Stored image bytes",
            content((BinaryBody = "image/gif"), (BinaryBody = "image/jpeg"), (BinaryBody = "image/png"), (BinaryBody = "image/webp")),
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
        (status = 404, description = "The board or image is missing or is not visible", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "The image could not be read or a required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn download(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((item_id, row_id)): Path<(String, String)>,
) -> ApiResult<Response> {
    let loaded = board_of(&state, &identity, &item_id).await?;
    let row = load_row(&state.pool, &loaded.board, &row_id).await?;
    if !matches!(row.status.as_str(), "ready" | "attached") {
        return Err(file_not_found());
    }
    let file_id = row.file_id.as_deref().ok_or_else(file_not_found)?;
    identity
        .files_download(file_id)
        .await
        .map_err(|_| ApiError::unavailable("media.download_failed", "The file could not be read."))
}
