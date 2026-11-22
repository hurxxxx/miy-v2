//! AI-DO domains/files/router.py (browse, folders, delete, archive, download,
//! preview) on drive spaces, plus the miy additions: rename and move (보강),
//! deleting into the trash (보강) and versions.
use axum::{
    Extension, Json,
    body::Body,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

use crate::{
    AppState,
    access::{self, SpaceAccess},
    archive_planner::{self, ArchivePlanFile, ArchivePlanFolder},
    error::{ApiError, ApiJson, ApiResult, check_length},
    people,
    time::now_utc,
    tree::{self, FileFolderItem, FileItem, FileRow, Folder},
    versions::{self, Version},
};

/// AI-DO MAX_ARCHIVE_FILE_COUNT. The 512 MB total is gone: files are no
/// longer zipped on the server (docs/PORTING.md §3.5).
pub const MAX_ARCHIVE_FILE_COUNT: usize = 500;
/// AI-DO FileBulkRequest list limit.
const MAX_BULK_IDS: usize = 500;
/// Images up to the backend download limit are previewed in the frame.
const MAX_PREVIEW_BYTES: i64 = 10 * 1024 * 1024;
/// AI-DO `_SAFE_INLINE_IMAGE_CONTENT_TYPES`.
const PREVIEW_TYPES: [&str; 4] = ["image/gif", "image/jpeg", "image/png", "image/webp"];

/// `Some(None)` for an explicit `null`, `None` when the field is absent.
pub fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

async fn owner_names(identity: &Identity, owners: Vec<String>) -> HashMap<String, String> {
    people::display_names(identity, owners).await
}

// ------------------------------------------------------------------ browse

#[derive(Serialize, utoipa::ToSchema)]
pub struct SpaceSummary {
    pub id: String,
    pub name: String,
    #[schema(value_type = crate::access::Role)]
    pub role: &'static str,
    pub can_manage: bool,
}

pub fn space_summary(access: &SpaceAccess) -> SpaceSummary {
    SpaceSummary {
        id: access.space.id.clone(),
        name: access.space.name.clone(),
        role: access.role.id(),
        can_manage: access.is_admin(),
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct FileBrowseResponse {
    space: SpaceSummary,
    #[schema(required = true)]
    current_folder: Option<FileFolderItem>,
    breadcrumbs: Vec<FileFolderItem>,
    folders: Vec<FileFolderItem>,
    files: Vec<FileItem>,
    all_folders: Vec<FileFolderItem>,
    can_ingest: bool,
    #[schema(value_type = Option<crate::tree::IngestBlockReason>, required = true)]
    ingest_block_reason: Option<&'static str>,
}

#[derive(Deserialize)]
pub struct BrowseQuery {
    folder_id: Option<String>,
}

/// `GET /v1/spaces/{space_id}/files?folder_id=` (AI-DO `GET /files`).
#[utoipa::path(
    get, path = "/v1/spaces/{space_id}/files", operation_id = "browseDriveFiles", tag = "Files and folders",
    params(("space_id" = String, Path), ("folder_id" = Option<String>, Query, description = "Omit for the space root")),
    responses(
        (status = 200, description = "Visible files, folders and current ingest policy", body = FileBrowseResponse),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn browse(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    Query(query): Query<BrowseQuery>,
) -> ApiResult<Json<FileBrowseResponse>> {
    let access = access::read_access(&state, &identity, &space_id).await?;
    let folder_id = query.folder_id.filter(|id| !id.is_empty());
    let folders = tree::list_accessible_folders(&state.pool, &access).await?;
    let folder_ids: HashSet<String> = folders.iter().map(|f| f.id.clone()).collect();
    if let Some(id) = &folder_id
        && !folder_ids.contains(id)
    {
        return Err(tree::folder_not_found());
    }
    let files = tree::list_accessible_files(&state.pool, &access, &folder_ids).await?;
    let visible: Vec<&FileRow> = files
        .iter()
        .filter(|file| file.folder_id == folder_id)
        .collect();
    let owners: Vec<String> = folders
        .iter()
        .map(|f| f.owner_id.clone())
        .chain(visible.iter().map(|f| f.owner_id.clone()))
        .collect();
    let names = owner_names(&identity, owners).await;
    let current = folder_id
        .as_deref()
        .and_then(|id| folders.iter().find(|f| f.id == id));
    if folder_id.is_none() {
        // Opening the space reads its share notification (off the answer path).
        let reader = identity.clone();
        let key = crate::notify::space_group_key(&space_id);
        tokio::spawn(async move { crate::notify::mark_read(&reader, &key).await });
    }
    let block = tree::ingest_block_reason(&access, current);
    Ok(Json(FileBrowseResponse {
        space: space_summary(&access),
        current_folder: current.map(|f| tree::folder_item(f, &access, &names)),
        breadcrumbs: tree::breadcrumbs(&folders, folder_id.as_deref())
            .iter()
            .map(|f| tree::folder_item(f, &access, &names))
            .collect(),
        folders: folders
            .iter()
            .filter(|f| f.parent_id == folder_id)
            .map(|f| tree::folder_item(f, &access, &names))
            .collect(),
        files: visible
            .iter()
            .map(|f| tree::file_item(f, &access, &names))
            .collect(),
        all_folders: folders
            .iter()
            .map(|f| tree::folder_item(f, &access, &names))
            .collect(),
        can_ingest: block.is_none(),
        ingest_block_reason: block,
    }))
}

// ----------------------------------------------------------------- folders

#[derive(Deserialize, utoipa::ToSchema)]
pub struct FolderCreateRequest {
    name: String,
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default = "default_visibility")]
    #[schema(value_type = crate::tree::FileVisibility)]
    visibility: String,
}

pub fn default_visibility() -> String {
    "private".into()
}

/// `POST /v1/spaces/{space_id}/files/folders` (AI-DO `create_folder`).
#[utoipa::path(
    post, path = "/v1/spaces/{space_id}/files/folders", operation_id = "createDriveFolder", tag = "Files and folders",
    request_body = FolderCreateRequest,
    params(("space_id" = String, Path)),
    responses(
        (status = 201, description = "Created folder", body = FileFolderItem),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 422, description = "Invalid or unsupported request", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn create_folder(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    ApiJson(payload): ApiJson<FolderCreateRequest>,
) -> ApiResult<impl IntoResponse> {
    check_length("name", &payload.name, 1, 255)?;
    if let Some(parent) = &payload.parent_id {
        check_length("parent_id", parent, 0, 36)?;
    }
    let parent_id = payload.parent_id.clone().filter(|id| !id.is_empty());
    let groups = access::frame_groups(&identity).await?;
    let mut tx = state.pool.begin().await?;
    let space = access::lock_space(&mut tx, &space_id).await?;
    let parent = match &parent_id {
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
    let name = tree::normalize_name(&payload.name)?;
    let now = now_utc();
    let sql = format!(
        "INSERT INTO file_manager_folders (id, workspace_id, parent_id, owner_id, name, visibility, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $7) RETURNING {}",
        tree::FOLDER_COLUMNS
    );
    let folder = sqlx::query_as::<_, Folder>(&sql)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&space_id)
        .bind(&parent_id)
        .bind(&identity.user_id)
        .bind(&name)
        .bind(&visibility)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    let names = owner_names(&identity, vec![folder.owner_id.clone()]).await;
    Ok((
        StatusCode::CREATED,
        Json(tree::folder_item(&folder, &access, &names)),
    ))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct FolderUpdateRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    #[schema(value_type = Option<crate::tree::FileVisibility>)]
    visibility: Option<String>,
    /// miy (보강): move under another folder (`null` = the space root).
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    parent_id: Option<Option<String>>,
}

/// `PATCH /v1/spaces/{space_id}/files/folders/{folder_id}` (AI-DO
/// `update_folder`: rename; visibility is fixed; miy adds a move).
#[utoipa::path(
    patch, path = "/v1/spaces/{space_id}/files/folders/{folder_id}", operation_id = "updateDriveFolder", tag = "Files and folders",
    request_body = FolderUpdateRequest,
    params(("space_id" = String, Path), ("folder_id" = String, Path)),
    responses(
        (status = 200, description = "Updated folder", body = FileFolderItem),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 409, description = "Resource state changed or conflicts with this request", body = crate::error::ErrorResponse),
        (status = 422, description = "Invalid or unsupported request", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn update_folder(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, folder_id)): Path<(String, String)>,
    ApiJson(payload): ApiJson<FolderUpdateRequest>,
) -> ApiResult<Json<FileFolderItem>> {
    if let Some(name) = &payload.name {
        check_length("name", name, 1, 255)?;
    }
    let groups = access::frame_groups(&identity).await?;
    let mut tx = state.pool.begin().await?;
    let space = access::lock_space(&mut tx, &space_id).await?;
    let folder = tree::lock_folder(&mut tx, &space_id, &folder_id, true).await?;
    let access = access::recheck(&state, &mut tx, &identity, space, &groups).await?;
    let accessible = tree::list_accessible_folders(&mut *tx, &access).await?;
    if !accessible.iter().any(|f| f.id == folder.id) {
        return Err(tree::folder_not_found());
    }
    if !tree::can_manage(&folder.owner_id, &access) {
        return Err(tree::delete_access_required());
    }
    if let Some(visibility) = &payload.visibility
        && tree::normalize_visibility(visibility)? != folder.visibility
    {
        return Err(ApiError::unprocessable(
            "files.visibility_change_not_allowed",
            "Folder visibility cannot be changed after creation.",
        ));
    }
    if let Some(name) = &payload.name {
        let name = tree::normalize_name(name)?;
        sqlx::query(
            "UPDATE file_manager_folders SET name = $2, \
             updated_at = CASE WHEN name IS DISTINCT FROM $2 THEN $3 ELSE updated_at END WHERE id = $1",
        )
        .bind(&folder.id)
        .bind(&name)
        .bind(now_utc())
        .execute(&mut *tx)
        .await?;
    }
    if let Some(target) = payload.parent_id {
        let target = target.filter(|id| !id.is_empty());
        if target != folder.parent_id {
            move_in_tx(
                &mut tx,
                &access,
                std::slice::from_ref(&folder.id),
                &[],
                target.as_deref(),
            )
            .await?;
        }
    }
    let sql = format!(
        "SELECT {} FROM file_manager_folders WHERE id = $1",
        tree::FOLDER_COLUMNS
    );
    let updated = sqlx::query_as::<_, Folder>(&sql)
        .bind(&folder.id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    let names = owner_names(&identity, vec![updated.owner_id.clone()]).await;
    Ok(Json(tree::folder_item(&updated, &access, &names)))
}

/// `DELETE /v1/spaces/{space_id}/files/folders/{folder_id}`: to the trash.
#[utoipa::path(
    delete, path = "/v1/spaces/{space_id}/files/folders/{folder_id}", operation_id = "trashDriveFolder", tag = "Files and folders",
    params(("space_id" = String, Path), ("folder_id" = String, Path)),
    responses(
        (status = 204, description = "Folder and its contents moved to the trash"),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 409, description = "Resource state changed or conflicts with this request", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn delete_folder(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, folder_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    trash_items(&state, &identity, &space_id, &[folder_id], &[]).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------- trash

#[derive(Deserialize, utoipa::ToSchema)]
pub struct FileBulkRequest {
    #[serde(default)]
    #[schema(max_items = 500)]
    pub file_ids: Vec<String>,
    #[serde(default)]
    #[schema(max_items = 500)]
    pub folder_ids: Vec<String>,
}

pub fn check_bulk(payload: &FileBulkRequest) -> ApiResult<(Vec<String>, Vec<String>)> {
    if payload.file_ids.len() > MAX_BULK_IDS || payload.folder_ids.len() > MAX_BULK_IDS {
        return Err(ApiError::validation(format!(
            "List should have at most {MAX_BULK_IDS} items"
        )));
    }
    let files: Vec<String> = payload
        .file_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let folders: Vec<String> = payload
        .folder_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if files.is_empty() && folders.is_empty() {
        return Err(ApiError::unprocessable(
            "files.selection_required",
            "Select at least one file or folder.",
        ));
    }
    Ok((files, folders))
}

/// The selected roots of a scope: selected folders not inside another
/// selected folder, and selected files outside every selected folder. Every
/// other row belongs to the root above it.
fn root_of(folders: &[Folder], selected_folders: &[String]) -> HashMap<String, String> {
    let parent: HashMap<&str, Option<&str>> = folders
        .iter()
        .map(|f| (f.id.as_str(), f.parent_id.as_deref()))
        .collect();
    let selected: HashSet<&str> = selected_folders.iter().map(String::as_str).collect();
    let mut result = HashMap::new();
    for folder in folders {
        // The highest selected folder on the chain.
        let mut top: Option<&str> = None;
        let mut cursor = Some(folder.id.as_str());
        let mut seen = HashSet::new();
        while let Some(id) = cursor {
            if !seen.insert(id) {
                break;
            }
            if selected.contains(id) {
                top = Some(id);
            }
            cursor = parent.get(id).copied().flatten();
        }
        if let Some(top) = top {
            result.insert(folder.id.clone(), top.to_string());
        }
    }
    result
}

/// AI-DO `delete_items` / `delete_folder` / `delete_file` soft deletes, as a
/// trash: each selected root gets its own trash entry with its subtree.
pub async fn trash_items(
    state: &AppState,
    identity: &Identity,
    space_id: &str,
    folder_ids: &[String],
    file_ids: &[String],
) -> ApiResult<()> {
    let groups = access::frame_groups(identity).await?;
    let mut tx = state.pool.begin().await?;
    let space = access::lock_space(&mut tx, space_id).await?;
    let (folders, files) = tree::lock_tree_scope(&mut tx, space_id, folder_ids, file_ids).await?;
    let access = access::recheck(state, &mut tx, identity, space, &groups).await?;
    tree::ensure_can_change_rows(&access, &folders, &files)?;
    let roots = root_of(&folders, folder_ids);
    let mut trash_ids: HashMap<String, String> = HashMap::new();
    let mut trash_of = |root: &str| -> String {
        trash_ids
            .entry(root.to_string())
            .or_insert_with(|| uuid::Uuid::new_v4().to_string())
            .clone()
    };
    let now = now_utc();
    for folder in &folders {
        let root = roots
            .get(&folder.id)
            .cloned()
            .unwrap_or_else(|| folder.id.clone());
        sqlx::query(
            "UPDATE file_manager_folders SET deleted_at = $2, trash_id = $3, trash_root = $4, trashed_by = $5 WHERE id = $1",
        )
        .bind(&folder.id)
        .bind(now)
        .bind(trash_of(&root))
        .bind(root == folder.id)
        .bind(&identity.user_id)
        .execute(&mut *tx)
        .await?;
    }
    for file in &files {
        let root = file
            .folder_id
            .as_ref()
            .and_then(|folder| roots.get(folder))
            .cloned();
        let (trash, is_root) = match root {
            Some(root) => (trash_of(&root), false),
            None => (trash_of(&file.id), true),
        };
        sqlx::query(
            "UPDATE file_manager_files SET deleted_at = $2, trash_id = $3, trash_root = $4, trashed_by = $5 WHERE id = $1",
        )
        .bind(&file.id)
        .bind(now)
        .bind(trash)
        .bind(is_root)
        .bind(&identity.user_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// `POST /v1/spaces/{space_id}/files/bulk-delete` (AI-DO `bulk_delete`).
#[utoipa::path(
    post, path = "/v1/spaces/{space_id}/files/bulk-delete", operation_id = "trashDriveSelection", tag = "Files and folders",
    request_body = FileBulkRequest,
    params(("space_id" = String, Path)),
    responses(
        (status = 204, description = "Selection moved to the trash"),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 409, description = "Resource state changed or conflicts with this request", body = crate::error::ErrorResponse),
        (status = 422, description = "Invalid or unsupported request", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn bulk_delete(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    ApiJson(payload): ApiJson<FileBulkRequest>,
) -> ApiResult<StatusCode> {
    let (files, folders) = check_bulk(&payload)?;
    trash_items(&state, &identity, &space_id, &folders, &files).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /v1/spaces/{space_id}/files/{file_id}` (AI-DO `delete_file`).
#[utoipa::path(
    delete, path = "/v1/spaces/{space_id}/files/{file_id}", operation_id = "trashDriveFile", tag = "Files and folders",
    params(("space_id" = String, Path), ("file_id" = String, Path)),
    responses(
        (status = 204, description = "File moved to the trash"),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 409, description = "Resource state changed or conflicts with this request", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn delete_file(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, file_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    trash_items(&state, &identity, &space_id, &[], &[file_id]).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------- rename and move

#[derive(Deserialize, utoipa::ToSchema)]
pub struct FileUpdateRequest {
    #[serde(default)]
    filename: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    folder_id: Option<Option<String>>,
}

/// `PATCH /v1/spaces/{space_id}/files/{file_id}` (보강): rename or move.
#[utoipa::path(
    patch, path = "/v1/spaces/{space_id}/files/{file_id}", operation_id = "updateDriveFile", tag = "Files and folders",
    request_body = FileUpdateRequest,
    params(("space_id" = String, Path), ("file_id" = String, Path)),
    responses(
        (status = 200, description = "Updated file", body = FileItem),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 409, description = "Resource state changed or conflicts with this request", body = crate::error::ErrorResponse),
        (status = 422, description = "Invalid or unsupported request", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn update_file(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, file_id)): Path<(String, String)>,
    ApiJson(payload): ApiJson<FileUpdateRequest>,
) -> ApiResult<Json<FileItem>> {
    if let Some(name) = &payload.filename {
        check_length("filename", name, 1, 512)?;
    }
    let groups = access::frame_groups(&identity).await?;
    let mut tx = state.pool.begin().await?;
    let space = access::lock_space(&mut tx, &space_id).await?;
    let file = tree::lock_file(&mut tx, &space_id, &file_id).await?;
    let access = access::recheck(&state, &mut tx, &identity, space, &groups).await?;
    if !tree::record_visible(&file.visibility, &file.owner_id, &access) {
        return Err(tree::file_access_required());
    }
    let accessible = tree::list_accessible_folders(&mut *tx, &access).await?;
    if let Some(folder) = &file.folder_id
        && !accessible.iter().any(|f| &f.id == folder)
    {
        return Err(tree::file_access_required());
    }
    if !tree::can_manage(&file.owner_id, &access) {
        return Err(tree::delete_access_required());
    }
    if let Some(name) = &payload.filename {
        let name = tree::normalize_name(name)?;
        let name = archive_planner::safe_filename(Some(&name));
        sqlx::query(
            "UPDATE file_manager_files SET filename = $2, \
             updated_at = CASE WHEN filename IS DISTINCT FROM $2 THEN $3 ELSE updated_at END WHERE id = $1",
        )
        .bind(&file.id)
        .bind(&name)
        .bind(now_utc())
        .execute(&mut *tx)
        .await?;
    }
    if let Some(target) = payload.folder_id {
        let target = target.filter(|id| !id.is_empty());
        if target != file.folder_id {
            move_in_tx(
                &mut tx,
                &access,
                &[],
                std::slice::from_ref(&file.id),
                target.as_deref(),
            )
            .await?;
        }
    }
    let updated = tree::load_file(&mut *tx, &file.id)
        .await?
        .ok_or_else(tree::file_not_found)?;
    tx.commit().await?;
    let names = owner_names(&identity, vec![updated.owner_id.clone()]).await;
    Ok(Json(tree::file_item(&updated, &access, &names)))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct MoveRequest {
    #[serde(default)]
    #[schema(max_items = 500)]
    file_ids: Vec<String>,
    #[serde(default)]
    #[schema(max_items = 500)]
    folder_ids: Vec<String>,
    #[serde(default)]
    target_folder_id: Option<String>,
}

/// `POST /v1/spaces/{space_id}/files/move` (보강): move a selection.
#[utoipa::path(
    post, path = "/v1/spaces/{space_id}/files/move", operation_id = "moveDriveSelection", tag = "Files and folders",
    request_body = MoveRequest,
    params(("space_id" = String, Path)),
    responses(
        (status = 204, description = "Selection moved"),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 409, description = "Resource state changed or conflicts with this request", body = crate::error::ErrorResponse),
        (status = 422, description = "Invalid or unsupported request", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn move_items(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    ApiJson(payload): ApiJson<MoveRequest>,
) -> ApiResult<StatusCode> {
    let (files, folders) = check_bulk(&FileBulkRequest {
        file_ids: payload.file_ids,
        folder_ids: payload.folder_ids,
    })?;
    let target = payload.target_folder_id.filter(|id| !id.is_empty());
    let groups = access::frame_groups(&identity).await?;
    let mut tx = state.pool.begin().await?;
    let space = access::lock_space(&mut tx, &space_id).await?;
    tree::lock_tree_scope(&mut tx, &space_id, &folders, &files).await?;
    let access = access::recheck(&state, &mut tx, &identity, space, &groups).await?;
    move_in_tx(&mut tx, &access, &folders, &files, target.as_deref()).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Move selected roots under `target` (or the root). The caller changes the
/// whole subtree, so the delete rule applies to it; the target must accept
/// new items from the caller (the upload rule).
async fn move_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    access: &SpaceAccess,
    folder_ids: &[String],
    file_ids: &[String],
    target: Option<&str>,
) -> ApiResult<()> {
    let space_id = access.space.id.clone();
    let (folders, files) = tree::lock_tree_scope(tx, &space_id, folder_ids, file_ids).await?;
    let accessible = tree::list_accessible_folders(&mut **tx, access).await?;
    let accessible_ids: HashSet<&str> = accessible.iter().map(|f| f.id.as_str()).collect();
    for folder in folders.iter().filter(|f| folder_ids.contains(&f.id)) {
        if !accessible_ids.contains(folder.id.as_str()) {
            return Err(tree::folder_not_found());
        }
    }
    for file in files.iter().filter(|f| file_ids.contains(&f.id)) {
        if !tree::record_visible(&file.visibility, &file.owner_id, access)
            || file
                .folder_id
                .as_deref()
                .is_some_and(|folder| !accessible_ids.contains(folder))
        {
            return Err(tree::file_access_required());
        }
    }
    tree::ensure_can_change_rows(access, &folders, &files)?;
    let target_folder = match target {
        Some(id) => {
            if folders.iter().any(|f| f.id == id) {
                return Err(ApiError::unprocessable(
                    "files.move_into_itself",
                    "A folder cannot be moved into itself or one of its subfolders.",
                ));
            }
            let folder = tree::lock_folder(tx, &space_id, id, false).await?;
            if !accessible_ids.contains(folder.id.as_str()) {
                return Err(tree::folder_not_found());
            }
            Some(folder)
        }
        None => None,
    };
    tree::ensure_can_ingest(access, target_folder.as_ref())?;
    let now = now_utc();
    sqlx::query(
        "UPDATE file_manager_folders SET parent_id = $2, updated_at = $3 WHERE id = ANY($1) \
         AND parent_id IS DISTINCT FROM $2",
    )
    .bind(folder_ids)
    .bind(target)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE file_manager_files SET folder_id = $2, updated_at = $3 WHERE id = ANY($1) \
         AND folder_id IS DISTINCT FROM $2",
    )
    .bind(file_ids)
    .bind(target)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ------------------------------------------------------- bundle download

#[derive(Serialize, utoipa::ToSchema)]
pub struct ArchiveEntryOut {
    path: String,
    #[schema(required = true)]
    file_id: Option<String>,
    size_bytes: i64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ArchivePlanResponse {
    entries: Vec<ArchiveEntryOut>,
    file_count: usize,
    total_size: i64,
}

/// `POST /v1/spaces/{space_id}/files/archive`: AI-DO `list_archive_entries`
/// and its limits. AI-DO streamed a ZIP; miy answers the planned entries and
/// the screen downloads each file with its own link (docs/PORTING.md §3.5).
#[utoipa::path(
    post, path = "/v1/spaces/{space_id}/files/archive", operation_id = "planDriveDownloads", tag = "Files and folders",
    request_body = FileBulkRequest,
    params(("space_id" = String, Path)),
    responses(
        (status = 200, description = "Plan of accessible files for separate downloads", body = ArchivePlanResponse),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 413, description = "File or selection exceeds the supported limit", body = crate::error::ErrorResponse),
        (status = 422, description = "Invalid or unsupported request", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn archive(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    ApiJson(payload): ApiJson<FileBulkRequest>,
) -> ApiResult<Json<ArchivePlanResponse>> {
    if payload.file_ids.len() > MAX_BULK_IDS || payload.folder_ids.len() > MAX_BULK_IDS {
        return Err(ApiError::validation(format!(
            "List should have at most {MAX_BULK_IDS} items"
        )));
    }
    let mut seen = HashSet::new();
    let selected_files: Vec<String> = payload
        .file_ids
        .iter()
        .filter(|id| seen.insert((*id).clone()))
        .cloned()
        .collect();
    let selected_folders: BTreeSet<String> = payload.folder_ids.iter().cloned().collect();
    if selected_files.is_empty() && selected_folders.is_empty() {
        return Err(ApiError::unprocessable(
            "files.selection_required",
            "Select at least one file or folder.",
        ));
    }
    let access = access::read_access(&state, &identity, &space_id).await?;
    let folders = tree::list_accessible_folders(&state.pool, &access).await?;
    let folder_ids: HashSet<String> = folders.iter().map(|f| f.id.clone()).collect();
    if selected_folders.iter().any(|id| !folder_ids.contains(id)) {
        return Err(tree::folder_not_found());
    }
    let plan_folders: Vec<ArchivePlanFolder> = folders
        .iter()
        .map(|f| ArchivePlanFolder {
            id: f.id.clone(),
            parent_id: f.parent_id.clone(),
            name: f.name.clone(),
        })
        .collect();
    let mut by_id: HashMap<String, FileRow> = HashMap::new();
    let mut folder_files = Vec::new();
    if !selected_folders.is_empty() {
        for file in tree::list_accessible_files(&state.pool, &access, &folder_ids).await? {
            folder_files.push(ArchivePlanFile {
                id: file.id.clone(),
                folder_id: file.folder_id.clone(),
                filename: file.filename.clone(),
            });
            by_id.insert(file.id.clone(), file);
        }
    }
    let mut explicit = Vec::new();
    for file_id in &selected_files {
        let file = tree::require_file_access(&state.pool, &access, file_id).await?;
        explicit.push(ArchivePlanFile {
            id: file.id.clone(),
            folder_id: file.folder_id.clone(),
            filename: file.filename.clone(),
        });
        by_id.insert(file.id.clone(), file);
    }
    let planned = archive_planner::plan_archive_entries(
        &selected_folders,
        &plan_folders,
        &folder_files,
        &explicit,
    );
    if planned.is_empty() {
        return Err(tree::file_not_found());
    }
    let file_count = planned.iter().filter(|e| e.file_id.is_some()).count();
    if file_count > MAX_ARCHIVE_FILE_COUNT {
        return Err(ApiError::too_large(
            "files.archive_limit_exceeded",
            format!("Bundle downloads are limited to {MAX_ARCHIVE_FILE_COUNT} files."),
        ));
    }
    let entries: Vec<ArchiveEntryOut> = planned
        .into_iter()
        .map(|entry| ArchiveEntryOut {
            size_bytes: entry
                .file_id
                .as_ref()
                .and_then(|id| by_id.get(id))
                .map_or(0, |f| f.size_bytes),
            path: entry.archive_path,
            file_id: entry.file_id,
        })
        .collect();
    let total_size = entries.iter().map(|e| e.size_bytes).sum();
    Ok(Json(ArchivePlanResponse {
        entries,
        file_count,
        total_size,
    }))
}

// ---------------------------------------------------------- download link

#[derive(Serialize, utoipa::ToSchema)]
pub struct FileDownloadResponse {
    url: String,
    expires_at: String,
}

fn download_failed() -> ApiError {
    ApiError::new(
        StatusCode::BAD_GATEWAY,
        "files.storage_download_failed",
        "File storage download failed.",
    )
}

async fn download_link(
    state: &AppState,
    identity: &Identity,
    version: &Version,
) -> ApiResult<FileDownloadResponse> {
    let storage = version
        .storage_file_id
        .as_deref()
        .filter(|_| version.status == "ready")
        .ok_or_else(tree::file_not_found)?;
    let reference = versions::ensure_attached(identity, &state.pool, version).await;
    match identity
        .files_download_link(storage, reference.as_deref())
        .await
    {
        Ok(link) => Ok(FileDownloadResponse {
            url: link.url,
            expires_at: link.expires_at,
        }),
        Err(error) if error.status == Some(403) => Err(ApiError::forbidden(
            "files.download_frame_required",
            "Downloads start from the drive screen.",
        )),
        Err(_) => Err(download_failed()),
    }
}

/// `GET /v1/spaces/{space_id}/files/{file_id}/download` (AI-DO
/// `get_file_download`): a five-minute link for `runtime.download`.
#[utoipa::path(
    get, path = "/v1/spaces/{space_id}/files/{file_id}/download", operation_id = "getDriveDownloadLink", tag = "File transfer",
    params(("space_id" = String, Path), ("file_id" = String, Path)),
    responses(
        (status = 200, description = "Short-lived link bound to the current app frame", body = FileDownloadResponse),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current file access and an app frame are required", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 502, description = "Storage rejected the operation", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn download(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, file_id)): Path<(String, String)>,
) -> ApiResult<Json<FileDownloadResponse>> {
    let access = access::read_access(&state, &identity, &space_id).await?;
    let file = tree::require_file_access(&state.pool, &access, &file_id).await?;
    let version_id = file
        .current_version_id
        .as_deref()
        .ok_or_else(tree::file_not_found)?;
    let version = versions::load(&state.pool, version_id)
        .await?
        .ok_or_else(tree::file_not_found)?;
    Ok(Json(download_link(&state, &identity, &version).await?))
}

/// `GET /v1/spaces/{space_id}/files/{file_id}/versions/{version_id}/download`.
#[utoipa::path(
    get, path = "/v1/spaces/{space_id}/files/{file_id}/versions/{version_id}/download", operation_id = "getDriveVersionDownloadLink", tag = "File transfer",
    params(("space_id" = String, Path), ("file_id" = String, Path), ("version_id" = String, Path)),
    responses(
        (status = 200, description = "Short-lived version link bound to the current app frame", body = FileDownloadResponse),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current file access and an app frame are required", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 502, description = "Storage rejected the operation", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn download_version(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, file_id, version_id)): Path<(String, String, String)>,
) -> ApiResult<Json<FileDownloadResponse>> {
    let access = access::read_access(&state, &identity, &space_id).await?;
    let file = tree::require_file_access(&state.pool, &access, &file_id).await?;
    let version = versions::load(&state.pool, &version_id)
        .await?
        .filter(|v| v.file_id == file.id && v.status == "ready")
        .ok_or_else(tree::file_not_found)?;
    Ok(Json(download_link(&state, &identity, &version).await?))
}

// ----------------------------------------------------------------- preview

fn normalized_type(value: &str) -> String {
    value.split(';').next().unwrap_or("").trim().to_lowercase()
}

/// AI-DO `_matches_inline_image_signature`.
pub fn matches_image_signature(content_type: &str, prefix: &[u8]) -> bool {
    match content_type {
        "image/png" => prefix.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => prefix.starts_with(b"\xff\xd8\xff"),
        "image/gif" => prefix.starts_with(b"GIF87a") || prefix.starts_with(b"GIF89a"),
        "image/webp" => prefix.starts_with(b"RIFF") && prefix.get(8..12) == Some(b"WEBP"),
        _ => false,
    }
}

fn preview_unsupported() -> ApiError {
    ApiError::new(
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "files.preview_unsupported_type",
        "This file type cannot be previewed.",
    )
}

/// `GET /files/items/{file_id}/preview` (fileApi; AI-DO `get_file_preview`
/// returned an inline URL): the image bytes for a frame Blob URL. Raster
/// types only, signature checked, up to 10 MiB.
#[utoipa::path(
    get, path = "/files/items/{file_id}/preview", operation_id = "previewDriveImage", tag = "File transfer",
    params(("file_id" = String, Path)),
    responses(
        (status = 200, description = "Verified raster image up to 10 MiB",
            content(
                (super::api_contract::BinaryBody = "image/gif"),
                (super::api_contract::BinaryBody = "image/jpeg"),
                (super::api_contract::BinaryBody = "image/png"),
                (super::api_contract::BinaryBody = "image/webp")
            ),
            headers(
                ("Cache-Control" = String, description = "private, no-store"),
                ("Content-Disposition" = String, description = "inline"),
                ("X-Content-Type-Options" = String, description = "nosniff")
            )
        ),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 413, description = "File or selection exceeds the supported limit", body = crate::error::ErrorResponse),
        (status = 415, description = "Unsupported content type", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 502, description = "Storage rejected the operation", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn preview(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(file_id): Path<String>,
) -> ApiResult<Response> {
    let file = tree::load_file(&state.pool, &file_id)
        .await?
        .ok_or_else(tree::file_not_found)?;
    let access = access::read_access(&state, &identity, &file.workspace_id).await?;
    let file = tree::require_file_access(&state.pool, &access, &file_id).await?;
    let content_type = normalized_type(&file.content_type);
    if !PREVIEW_TYPES.contains(&content_type.as_str()) {
        return Err(preview_unsupported());
    }
    if file.size_bytes > MAX_PREVIEW_BYTES {
        return Err(ApiError::too_large(
            "files.preview_too_large",
            "Images larger than 10 MB cannot be previewed. Download the file instead.",
        ));
    }
    let version = versions::load(
        &state.pool,
        file.current_version_id.as_deref().unwrap_or(""),
    )
    .await?
    .ok_or_else(tree::file_not_found)?;
    let storage = version
        .storage_file_id
        .as_deref()
        .ok_or_else(tree::file_not_found)?;
    let response = identity
        .files_download(storage)
        .await
        .map_err(|_| download_failed())?;
    let bytes = axum::body::to_bytes(response.into_body(), MAX_PREVIEW_BYTES as usize + 1)
        .await
        .map_err(|_| download_failed())?;
    if !matches_image_signature(&content_type, &bytes) {
        return Err(preview_unsupported());
    }
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "private, no-store".to_string()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
            (header::CONTENT_DISPOSITION, "inline".to_string()),
        ],
        Body::from(bytes),
    )
        .into_response())
}

// ---------------------------------------------------------------- versions

#[derive(Serialize, utoipa::ToSchema)]
pub struct VersionOut {
    id: String,
    version_number: i32,
    filename: String,
    content_type: String,
    size_bytes: i64,
    uploaded_by: String,
    uploader_name: String,
    #[schema(required = true)]
    sha256: Option<String>,
    is_current: bool,
    #[serde(serialize_with = "crate::time::serialize")]
    #[schema(value_type = String)]
    created_at: NaiveDateTime,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct VersionsResponse {
    file: FileItem,
    versions: Vec<VersionOut>,
}

/// `GET /v1/spaces/{space_id}/files/{file_id}/versions`.
#[utoipa::path(
    get, path = "/v1/spaces/{space_id}/files/{file_id}/versions", operation_id = "listDriveFileVersions", tag = "Files and folders",
    params(("space_id" = String, Path), ("file_id" = String, Path)),
    responses(
        (status = 200, description = "File and version list", body = VersionsResponse),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn list_versions(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, file_id)): Path<(String, String)>,
) -> ApiResult<Json<VersionsResponse>> {
    let access = access::read_access(&state, &identity, &space_id).await?;
    let file = tree::require_file_access(&state.pool, &access, &file_id).await?;
    let rows = versions::ready_versions(&state.pool, &file.id).await?;
    let names = people::display_names(
        &identity,
        rows.iter()
            .map(|v| v.uploaded_by.clone())
            .chain([file.owner_id.clone()]),
    )
    .await;
    Ok(Json(VersionsResponse {
        file: tree::file_item(&file, &access, &names),
        versions: rows
            .iter()
            .map(|v| VersionOut {
                id: v.id.clone(),
                version_number: v.version_number,
                filename: v.filename.clone(),
                content_type: v.content_type.clone(),
                size_bytes: v.size_bytes,
                uploaded_by: v.uploaded_by.clone(),
                uploader_name: names.get(&v.uploaded_by).cloned().unwrap_or_default(),
                sha256: v.sha256.clone(),
                is_current: file.current_version_id.as_deref() == Some(v.id.as_str()),
                created_at: v.completed_at.unwrap_or(v.created_at),
            })
            .collect(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_signatures_match_ai_do() {
        assert!(matches_image_signature(
            "image/png",
            b"\x89PNG\r\n\x1a\npng-bytes"
        ));
        assert!(!matches_image_signature("image/png", b"<svg"));
        assert!(matches_image_signature("image/gif", b"GIF89a..."));
        assert!(matches_image_signature(
            "image/webp",
            b"RIFF\0\0\0\0WEBPVP8 "
        ));
        assert!(!matches_image_signature("image/svg+xml", b"<svg"));
        assert_eq!(normalized_type("Image/PNG; charset=binary"), "image/png");
    }

    #[test]
    fn roots_are_the_highest_selected_folders() {
        let folder = |id: &str, parent: Option<&str>| Folder {
            id: id.into(),
            workspace_id: "s".into(),
            parent_id: parent.map(Into::into),
            owner_id: "u".into(),
            name: id.into(),
            visibility: "private".into(),
            created_at: now_utc(),
            updated_at: now_utc(),
            deleted_at: None,
            trash_id: None,
            trash_root: false,
            trashed_by: None,
        };
        let folders = vec![
            folder("a", None),
            folder("b", Some("a")),
            folder("c", Some("b")),
        ];
        let roots = root_of(&folders, &["a".into(), "c".into()]);
        assert_eq!(roots["c"], "a");
        assert_eq!(roots["b"], "a");
        let roots = root_of(&folders, &["b".into()]);
        assert_eq!(roots["c"], "b");
        assert!(!roots.contains_key("a"));
    }
}
