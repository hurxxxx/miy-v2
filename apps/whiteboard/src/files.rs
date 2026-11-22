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
    error::{ApiError, ApiJson, ApiResult, check_length},
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

#[derive(Deserialize)]
pub struct ReserveRequest {
    request_id: String,
    /// Excalidraw's file ID (the key in `scene.files`).
    file_id: String,
    name: String,
    mime: String,
    size: u64,
}

#[derive(Serialize)]
pub struct FileResponse {
    id: String,
    file_id: String,
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
