//! Trash (보강). AI-DO soft-deleted rows (`deleted_at`) and removed their
//! storage objects at once; miy keeps deleted items restorable: a delete
//! moves the selected items with their subtrees to the trash (files.rs
//! `trash_items`), and only an explicit permanent delete or emptying the
//! trash removes the rows and then, after that commit, releases the Core
//! files (cleanup.rs).
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use serde::Serialize;
use sqlx::{Postgres, Transaction};
use std::collections::{HashMap, HashSet};

use crate::{
    AppState,
    access::{self, SpaceAccess},
    cleanup,
    error::{ApiError, ApiJson, ApiResult},
    files::{FileBulkRequest, check_bulk},
    people,
    tree::{self, FileRow, Folder},
};

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[schema(rename_all = "snake_case")]
pub(crate) enum TrashKind {
    File,
    Folder,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TrashItem {
    #[schema(value_type = TrashKind)]
    kind: &'static str,
    id: String,
    name: String,
    #[schema(required = true)]
    content_type: Option<String>,
    size_bytes: i64,
    file_count: i64,
    #[schema(value_type = crate::tree::FileVisibility)]
    visibility: String,
    owner_id: String,
    owner_name: String,
    trashed_by: String,
    trashed_by_name: String,
    /// The folder path the item was in (`""` for the space root).
    original_path: String,
    #[serde(serialize_with = "crate::time::serialize")]
    #[schema(value_type = String)]
    deleted_at: NaiveDateTime,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TrashResponse {
    space: crate::files::SpaceSummary,
    items: Vec<TrashItem>,
}

fn folder_path(folders: &HashMap<String, Folder>, start: Option<&str>) -> String {
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    let mut cursor = start.and_then(|id| folders.get(id));
    while let Some(folder) = cursor {
        if !seen.insert(folder.id.clone()) {
            break;
        }
        names.push(folder.name.clone());
        cursor = folder.parent_id.as_deref().and_then(|id| folders.get(id));
    }
    names.reverse();
    names.join(" / ")
}

/// `GET /v1/spaces/{space_id}/trash`: the trashed items the caller may
/// restore or delete (their own; everything for a space admin).
#[utoipa::path(
    get, path = "/v1/spaces/{space_id}/trash", operation_id = "listDriveTrash", tag = "Trash",
    params(("space_id" = String, Path)),
    responses(
        (status = 200, description = "Trash entries the caller may manage", body = TrashResponse),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
) -> ApiResult<Json<TrashResponse>> {
    let access = access::read_access(&state, &identity, &space_id).await?;
    let sql = format!(
        "SELECT {} FROM file_manager_folders WHERE workspace_id = $1",
        tree::FOLDER_COLUMNS
    );
    let all_folders: HashMap<String, Folder> = sqlx::query_as::<_, Folder>(&sql)
        .bind(&space_id)
        .fetch_all(&state.pool)
        .await?
        .into_iter()
        .map(|f| (f.id.clone(), f))
        .collect();
    let sql = format!(
        "{} WHERE f.workspace_id = $1 AND f.trash_root AND f.deleted_at IS NOT NULL",
        tree::FILE_SELECT
    );
    let files = sqlx::query_as::<_, FileRow>(&sql)
        .bind(&space_id)
        .fetch_all(&state.pool)
        .await?;
    let batch_sizes: HashMap<String, (i64, i64)> = sqlx::query_as::<_, (String, i64, i64)>(
        "SELECT trash_id, count(*), COALESCE(sum(size_bytes), 0)::bigint FROM file_manager_files \
         WHERE workspace_id = $1 AND trash_id IS NOT NULL GROUP BY trash_id",
    )
    .bind(&space_id)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .map(|(id, count, size)| (id, (count, size)))
    .collect();
    let mut people_ids = Vec::new();
    let mut items = Vec::new();
    for folder in all_folders.values().filter(|f| f.trash_root) {
        if !tree::can_manage(&folder.owner_id, &access) {
            continue;
        }
        let (count, size) = folder
            .trash_id
            .as_ref()
            .and_then(|id| batch_sizes.get(id))
            .copied()
            .unwrap_or((0, 0));
        people_ids.push(folder.owner_id.clone());
        people_ids.extend(folder.trashed_by.clone());
        items.push(TrashItem {
            kind: "folder",
            id: folder.id.clone(),
            name: folder.name.clone(),
            content_type: None,
            size_bytes: size,
            file_count: count,
            visibility: folder.visibility.clone(),
            owner_id: folder.owner_id.clone(),
            owner_name: String::new(),
            trashed_by: folder.trashed_by.clone().unwrap_or_default(),
            trashed_by_name: String::new(),
            original_path: folder_path(&all_folders, folder.parent_id.as_deref()),
            deleted_at: folder.deleted_at.unwrap_or(folder.updated_at),
        });
    }
    for file in files
        .iter()
        .filter(|f| tree::can_manage(&f.owner_id, &access))
    {
        people_ids.push(file.owner_id.clone());
        people_ids.extend(file.trashed_by.clone());
        items.push(TrashItem {
            kind: "file",
            id: file.id.clone(),
            name: file.filename.clone(),
            content_type: Some(file.content_type.clone()),
            size_bytes: file.size_bytes,
            file_count: 1,
            visibility: file.visibility.clone(),
            owner_id: file.owner_id.clone(),
            owner_name: String::new(),
            trashed_by: file.trashed_by.clone().unwrap_or_default(),
            trashed_by_name: String::new(),
            original_path: folder_path(&all_folders, file.folder_id.as_deref()),
            deleted_at: file.deleted_at.unwrap_or(file.updated_at),
        });
    }
    let names = people::display_names(&identity, people_ids).await;
    for item in &mut items {
        item.owner_name = names.get(&item.owner_id).cloned().unwrap_or_default();
        item.trashed_by_name = names.get(&item.trashed_by).cloned().unwrap_or_default();
    }
    items.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at).then(a.id.cmp(&b.id)));
    Ok(Json(TrashResponse {
        space: crate::files::space_summary(&access),
        items,
    }))
}

/// The trashed roots and every row of their trash entries, locked.
struct Batches {
    trash_ids: Vec<String>,
    root_folders: Vec<Folder>,
    root_files: Vec<FileRow>,
    folders: Vec<Folder>,
    files: Vec<FileRow>,
}

async fn lock_batches(
    tx: &mut Transaction<'_, Postgres>,
    space_id: &str,
    folder_ids: &[String],
    file_ids: &[String],
) -> ApiResult<Batches> {
    let trash_ids: Vec<String> = sqlx::query_scalar(
        "SELECT trash_id FROM file_manager_folders WHERE workspace_id = $1 AND id = ANY($2) AND trash_root \
         UNION SELECT trash_id FROM file_manager_files WHERE workspace_id = $1 AND id = ANY($3) AND trash_root",
    )
    .bind(space_id)
    .bind(folder_ids)
    .bind(file_ids)
    .fetch_all(&mut **tx)
    .await?;
    sqlx::query(
        "SELECT id FROM file_manager_folders WHERE workspace_id = $1 AND trash_id = ANY($2) ORDER BY id FOR UPDATE",
    )
    .bind(space_id)
    .bind(&trash_ids)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "SELECT id FROM file_manager_files WHERE workspace_id = $1 AND trash_id = ANY($2) ORDER BY id FOR UPDATE",
    )
    .bind(space_id)
    .bind(&trash_ids)
    .execute(&mut **tx)
    .await?;
    let sql = format!(
        "SELECT {} FROM file_manager_folders WHERE workspace_id = $1 AND trash_id = ANY($2) ORDER BY id",
        tree::FOLDER_COLUMNS
    );
    let folders = sqlx::query_as::<_, Folder>(&sql)
        .bind(space_id)
        .bind(&trash_ids)
        .fetch_all(&mut **tx)
        .await?;
    let sql = format!(
        "{} WHERE f.workspace_id = $1 AND f.trash_id = ANY($2) ORDER BY f.id",
        tree::FILE_SELECT
    );
    let files = sqlx::query_as::<_, FileRow>(&sql)
        .bind(space_id)
        .bind(&trash_ids)
        .fetch_all(&mut **tx)
        .await?;
    let root_folders: Vec<Folder> = folders
        .iter()
        .filter(|f| f.trash_root && folder_ids.contains(&f.id))
        .cloned()
        .collect();
    let root_files: Vec<FileRow> = files
        .iter()
        .filter(|f| f.trash_root && file_ids.contains(&f.id))
        .cloned()
        .collect();
    if root_folders.len() != folder_ids.len() || root_files.len() != file_ids.len() {
        return Err(ApiError::not_found(
            "files.trash_item_not_found",
            "The item is not in the trash.",
        ));
    }
    Ok(Batches {
        trash_ids,
        root_folders,
        root_files,
        folders,
        files,
    })
}

fn ensure_can_handle(access: &SpaceAccess, batches: &Batches) -> ApiResult<()> {
    tree::ensure_can_change_rows(access, &batches.folders, &batches.files)
}

/// `POST /v1/spaces/{space_id}/trash/restore`: back where they were, or to
/// the space root when that folder is gone or still in the trash.
#[utoipa::path(
    post, path = "/v1/spaces/{space_id}/trash/restore", operation_id = "restoreDriveTrash", tag = "Trash",
    request_body = FileBulkRequest,
    params(("space_id" = String, Path)),
    responses(
        (status = 204, description = "Trash selection restored"),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 409, description = "Resource state changed or conflicts with this request", body = crate::error::ErrorResponse),
        (status = 422, description = "Invalid or unsupported request", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn restore(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    ApiJson(payload): ApiJson<FileBulkRequest>,
) -> ApiResult<StatusCode> {
    let (file_ids, folder_ids) = check_bulk(&payload)?;
    let groups = access::frame_groups(&identity).await?;
    let mut tx = state.pool.begin().await?;
    let space = access::lock_space(&mut tx, &space_id).await?;
    let batches = lock_batches(&mut tx, &space_id, &folder_ids, &file_ids).await?;
    let access = access::recheck(&state, &mut tx, &identity, space, &groups).await?;
    ensure_can_handle(&access, &batches)?;
    let active: HashSet<String> = sqlx::query_scalar(
        "SELECT id FROM file_manager_folders WHERE workspace_id = $1 AND deleted_at IS NULL",
    )
    .bind(&space_id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    let batch_folders: HashSet<&str> = batches.folders.iter().map(|f| f.id.as_str()).collect();
    for folder in &batches.root_folders {
        if let Some(parent) = &folder.parent_id
            && !active.contains(parent)
            && !batch_folders.contains(parent.as_str())
        {
            sqlx::query("UPDATE file_manager_folders SET parent_id = NULL WHERE id = $1")
                .bind(&folder.id)
                .execute(&mut *tx)
                .await?;
        }
    }
    for file in &batches.root_files {
        if let Some(parent) = &file.folder_id
            && !active.contains(parent)
            && !batch_folders.contains(parent.as_str())
        {
            sqlx::query("UPDATE file_manager_files SET folder_id = NULL WHERE id = $1")
                .bind(&file.id)
                .execute(&mut *tx)
                .await?;
        }
    }
    sqlx::query(
        "UPDATE file_manager_folders SET deleted_at = NULL, trash_id = NULL, trash_root = false, trashed_by = NULL \
         WHERE workspace_id = $1 AND trash_id = ANY($2)",
    )
    .bind(&space_id)
    .bind(&batches.trash_ids)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE file_manager_files SET deleted_at = NULL, trash_id = NULL, trash_root = false, trashed_by = NULL \
         WHERE workspace_id = $1 AND trash_id = ANY($2)",
    )
    .bind(&space_id)
    .bind(&batches.trash_ids)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Remove the rows of locked batches; returns the Core files to release.
async fn purge_batches(
    tx: &mut Transaction<'_, Postgres>,
    space_id: &str,
    batches: &Batches,
) -> ApiResult<Vec<String>> {
    let file_ids: Vec<String> = batches.files.iter().map(|f| f.id.clone()).collect();
    let folder_ids: Vec<String> = batches.folders.iter().map(|f| f.id.clone()).collect();
    let release = cleanup::enqueue_for_files(tx, &file_ids).await?;
    // Items trashed earlier from inside these folders keep their own trash
    // entries: they move to the root instead of disappearing with the folder.
    sqlx::query(
        "UPDATE file_manager_folders SET parent_id = NULL WHERE workspace_id = $1 \
         AND parent_id = ANY($2) AND NOT (id = ANY($2))",
    )
    .bind(space_id)
    .bind(&folder_ids)
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM file_manager_files WHERE id = ANY($1)")
        .bind(&file_ids)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM file_manager_folders WHERE id = ANY($1)")
        .bind(&folder_ids)
        .execute(&mut **tx)
        .await?;
    Ok(release)
}

/// `POST /v1/spaces/{space_id}/trash/delete`: delete permanently.
#[utoipa::path(
    post, path = "/v1/spaces/{space_id}/trash/delete", operation_id = "purgeDriveTrash", tag = "Trash",
    request_body = FileBulkRequest,
    params(("space_id" = String, Path)),
    responses(
        (status = 204, description = "Trash selection permanently deleted"),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 409, description = "Resource state changed or conflicts with this request", body = crate::error::ErrorResponse),
        (status = 422, description = "Invalid or unsupported request", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn delete_forever(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    ApiJson(payload): ApiJson<FileBulkRequest>,
) -> ApiResult<StatusCode> {
    let (file_ids, folder_ids) = check_bulk(&payload)?;
    let groups = access::frame_groups(&identity).await?;
    let mut tx = state.pool.begin().await?;
    let space = access::lock_space(&mut tx, &space_id).await?;
    let batches = lock_batches(&mut tx, &space_id, &folder_ids, &file_ids).await?;
    let access = access::recheck(&state, &mut tx, &identity, space, &groups).await?;
    ensure_can_handle(&access, &batches)?;
    let release = purge_batches(&mut tx, &space_id, &batches).await?;
    tx.commit().await?;
    cleanup::run(&identity, &state.pool, &release).await;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/spaces/{space_id}/trash/empty`: permanently delete every trashed
/// item the caller may delete.
#[utoipa::path(
    post, path = "/v1/spaces/{space_id}/trash/empty", operation_id = "emptyDriveTrash", tag = "Trash",
    params(("space_id" = String, Path)),
    responses(
        (status = 204, description = "Accessible trash emptied"),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 409, description = "Resource state changed or conflicts with this request", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn empty(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
) -> ApiResult<StatusCode> {
    let groups = access::frame_groups(&identity).await?;
    let mut tx = state.pool.begin().await?;
    let space = access::lock_space(&mut tx, &space_id).await?;
    let access = access::recheck(&state, &mut tx, &identity, space, &groups).await?;
    let owner_filter = if access.is_admin() {
        None
    } else {
        Some(identity.user_id.clone())
    };
    let folder_ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM file_manager_folders WHERE workspace_id = $1 AND trash_root \
         AND ($2::text IS NULL OR owner_id = $2)",
    )
    .bind(&space_id)
    .bind(&owner_filter)
    .fetch_all(&mut *tx)
    .await?;
    let file_ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM file_manager_files WHERE workspace_id = $1 AND trash_root \
         AND ($2::text IS NULL OR owner_id = $2)",
    )
    .bind(&space_id)
    .bind(&owner_filter)
    .fetch_all(&mut *tx)
    .await?;
    let mut release = Vec::new();
    if !folder_ids.is_empty() || !file_ids.is_empty() {
        let batches = lock_batches(&mut tx, &space_id, &folder_ids, &file_ids).await?;
        // A non-admin's entry holding someone else's row stays.
        let mut keep_folders = Vec::new();
        let mut keep_files = Vec::new();
        if !access.is_admin() {
            let foreign: HashSet<&str> = batches
                .folders
                .iter()
                .filter(|f| f.owner_id != identity.user_id)
                .filter_map(|f| f.trash_id.as_deref())
                .chain(
                    batches
                        .files
                        .iter()
                        .filter(|f| f.owner_id != identity.user_id)
                        .filter_map(|f| f.trash_id.as_deref()),
                )
                .collect();
            keep_folders = batches
                .folders
                .iter()
                .filter(|f| f.trash_id.as_deref().is_some_and(|t| foreign.contains(t)))
                .map(|f| f.id.clone())
                .collect();
            keep_files = batches
                .files
                .iter()
                .filter(|f| f.trash_id.as_deref().is_some_and(|t| foreign.contains(t)))
                .map(|f| f.id.clone())
                .collect();
        }
        let batches = Batches {
            trash_ids: batches.trash_ids,
            root_folders: Vec::new(),
            root_files: Vec::new(),
            folders: batches
                .folders
                .into_iter()
                .filter(|f| !keep_folders.contains(&f.id))
                .collect(),
            files: batches
                .files
                .into_iter()
                .filter(|f| !keep_files.contains(&f.id))
                .collect(),
        };
        release = purge_batches(&mut tx, &space_id, &batches).await?;
    }
    tx.commit().await?;
    cleanup::run(&identity, &state.pool, &release).await;
    Ok(StatusCode::NO_CONTENT)
}
