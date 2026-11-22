//! Folders and files of a space and who may see or change them: AI-DO
//! domains/files/service.py (`list_accessible_folders`, `list_accessible_files`,
//! `require_file_access`, `_record_visible`, `_ensure_can_manage_record`,
//! `_ensure_can_delete_rows`, `_ensure_can_ingest_under_parent`) and
//! browse_projection.py, without the RAG corpus branches.
use chrono::NaiveDateTime;
use serde::Serialize;
use sqlx::{FromRow, PgExecutor, Postgres, Transaction};
use std::collections::{BTreeSet, HashMap, HashSet};

use crate::{
    access::SpaceAccess,
    error::{ApiError, ApiResult},
};

pub const VISIBILITIES: [&str; 2] = ["private", "workspace"];

#[derive(Clone, Debug, FromRow)]
pub struct Folder {
    pub id: String,
    #[allow(dead_code)]
    pub workspace_id: String,
    pub parent_id: Option<String>,
    pub owner_id: String,
    pub name: String,
    pub visibility: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub trash_id: Option<String>,
    pub trash_root: bool,
    pub trashed_by: Option<String>,
}

pub const FOLDER_COLUMNS: &str = "id, workspace_id, parent_id, owner_id, name, visibility, \
     created_at, updated_at, deleted_at, trash_id, trash_root, trashed_by";

#[derive(Clone, Debug, FromRow)]
pub struct FileRow {
    pub id: String,
    pub workspace_id: String,
    pub folder_id: Option<String>,
    pub owner_id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub visibility: String,
    pub current_version_id: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub trash_id: Option<String>,
    pub trash_root: bool,
    pub trashed_by: Option<String>,
    pub version_number: Option<i32>,
    pub version_count: i64,
}

/// File columns plus the current version number and the number of versions.
pub const FILE_SELECT: &str = "SELECT f.id, f.workspace_id, f.folder_id, f.owner_id, f.filename, \
     f.content_type, f.size_bytes, f.visibility, f.current_version_id, f.created_at, f.updated_at, \
     f.deleted_at, f.trash_id, f.trash_root, f.trashed_by, \
     (SELECT v.version_number FROM file_manager_file_versions v WHERE v.id = f.current_version_id) AS version_number, \
     (SELECT count(*) FROM file_manager_file_versions v WHERE v.file_id = f.id AND v.status = 'ready') AS version_count \
     FROM file_manager_files f";

/// AI-DO `_record_visible`.
pub fn record_visible(visibility: &str, owner_id: &str, access: &SpaceAccess) -> bool {
    access.is_admin() || owner_id == access.user_id || visibility == "workspace"
}

/// AI-DO `serialize_folder_item(...).can_manage` / `serialize_file_item(...).can_delete`.
pub fn can_manage(owner_id: &str, access: &SpaceAccess) -> bool {
    access.is_admin() || owner_id == access.user_id
}

pub fn folder_not_found() -> ApiError {
    ApiError::not_found("files.folder_not_found", "Folder not found.")
}

pub fn file_not_found() -> ApiError {
    ApiError::not_found("files.file_not_found", "File not found.")
}

pub fn file_access_required() -> ApiError {
    ApiError::forbidden("files.file_access_required", "File access required.")
}

pub fn delete_access_required() -> ApiError {
    ApiError::forbidden(
        "files.delete_access_required",
        "Delete or update access required.",
    )
}

pub fn parent_manage_access_required() -> ApiError {
    ApiError::forbidden(
        "files.parent_manage_access_required",
        "Parent folder manage access required.",
    )
}

pub fn workspace_access_required() -> ApiError {
    ApiError::forbidden(
        "files.workspace_access_required",
        "You cannot add files or folders to this space.",
    )
}

pub fn normalize_visibility(value: &str) -> ApiResult<String> {
    let normalized = value.trim().to_lowercase();
    if !VISIBILITIES.contains(&normalized.as_str()) {
        return Err(ApiError::unprocessable(
            "files.invalid_visibility",
            "Unsupported file visibility.",
        ));
    }
    Ok(normalized)
}

/// AI-DO `_normalize_name`: trimmed, not empty, at most 255 characters.
pub fn normalize_name(value: &str) -> ApiResult<String> {
    let name = value.trim();
    if name.is_empty() {
        return Err(ApiError::unprocessable(
            "files.name_required",
            "Name is required.",
        ));
    }
    Ok(name.chars().take(255).collect())
}

/// Active (not trashed) folders of a space, AI-DO's order.
pub async fn active_folders<'e, E: PgExecutor<'e>>(
    executor: E,
    space_id: &str,
) -> ApiResult<Vec<Folder>> {
    let sql = format!(
        "SELECT {FOLDER_COLUMNS} FROM file_manager_folders WHERE workspace_id = $1 \
         AND deleted_at IS NULL ORDER BY name ASC, created_at ASC"
    );
    Ok(sqlx::query_as::<_, Folder>(&sql)
        .bind(space_id)
        .fetch_all(executor)
        .await?)
}

/// AI-DO `list_accessible_folders`: visible folders whose whole parent chain
/// is visible.
pub fn accessible_folders(folders: Vec<Folder>, access: &SpaceAccess) -> Vec<Folder> {
    let by_id: HashSet<&str> = folders.iter().map(|f| f.id.as_str()).collect();
    let mut accessible: HashSet<String> = HashSet::new();
    let mut changed = true;
    while changed {
        changed = false;
        for folder in &folders {
            if accessible.contains(&folder.id) {
                continue;
            }
            if !record_visible(&folder.visibility, &folder.owner_id, access) {
                continue;
            }
            if let Some(parent) = &folder.parent_id {
                if !by_id.contains(parent.as_str()) || !accessible.contains(parent) {
                    continue;
                }
            }
            accessible.insert(folder.id.clone());
            changed = true;
        }
    }
    folders
        .into_iter()
        .filter(|folder| accessible.contains(&folder.id))
        .collect()
}

pub async fn list_accessible_folders<'e, E: PgExecutor<'e>>(
    executor: E,
    access: &SpaceAccess,
) -> ApiResult<Vec<Folder>> {
    Ok(accessible_folders(
        active_folders(executor, &access.space.id).await?,
        access,
    ))
}

/// AI-DO `list_accessible_files`: visible ready files in accessible folders
/// (or at the root).
pub async fn list_accessible_files<'e, E: PgExecutor<'e>>(
    executor: E,
    access: &SpaceAccess,
    accessible_folder_ids: &HashSet<String>,
) -> ApiResult<Vec<FileRow>> {
    let sql = format!(
        "{FILE_SELECT} WHERE f.workspace_id = $1 AND f.deleted_at IS NULL \
         AND f.current_version_id IS NOT NULL ORDER BY f.filename ASC, f.created_at ASC"
    );
    let files = sqlx::query_as::<_, FileRow>(&sql)
        .bind(&access.space.id)
        .fetch_all(executor)
        .await?;
    Ok(files
        .into_iter()
        .filter(|file| {
            record_visible(&file.visibility, &file.owner_id, access)
                && file
                    .folder_id
                    .as_ref()
                    .is_none_or(|folder| accessible_folder_ids.contains(folder))
        })
        .collect())
}

pub async fn load_file<'e, E: PgExecutor<'e>>(
    executor: E,
    file_id: &str,
) -> ApiResult<Option<FileRow>> {
    let sql = format!("{FILE_SELECT} WHERE f.id = $1");
    Ok(sqlx::query_as::<_, FileRow>(&sql)
        .bind(file_id)
        .fetch_optional(executor)
        .await?)
}

/// AI-DO `require_file_access`: 404 for a missing, deleted or other-space
/// file, 403 when it is not visible or sits in an inaccessible folder.
pub async fn require_file_access(
    pool: &sqlx::PgPool,
    access: &SpaceAccess,
    file_id: &str,
) -> ApiResult<FileRow> {
    let file = load_file(pool, file_id)
        .await?
        .filter(|f| {
            f.deleted_at.is_none()
                && f.current_version_id.is_some()
                && f.workspace_id == access.space.id
        })
        .ok_or_else(file_not_found)?;
    check_file_readable(pool, access, &file).await?;
    Ok(file)
}

pub async fn check_file_readable(
    pool: &sqlx::PgPool,
    access: &SpaceAccess,
    file: &FileRow,
) -> ApiResult<()> {
    if !record_visible(&file.visibility, &file.owner_id, access) {
        return Err(file_access_required());
    }
    if let Some(folder_id) = &file.folder_id {
        let folders = list_accessible_folders(pool, access).await?;
        if !folders.iter().any(|f| &f.id == folder_id) {
            return Err(file_access_required());
        }
    }
    Ok(())
}

pub fn breadcrumbs(folders: &[Folder], current: Option<&str>) -> Vec<Folder> {
    let Some(current) = current else {
        return Vec::new();
    };
    let by_id: HashMap<&str, &Folder> = folders.iter().map(|f| (f.id.as_str(), f)).collect();
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut cursor = by_id.get(current).copied();
    while let Some(folder) = cursor {
        if !seen.insert(folder.id.as_str()) {
            break;
        }
        chain.push(folder.clone());
        cursor = folder
            .parent_id
            .as_deref()
            .and_then(|parent| by_id.get(parent).copied());
    }
    chain.reverse();
    chain
}

// ------------------------------------------------------------ serialization

#[derive(Serialize, Clone, Debug)]
pub struct FileFolderItem {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub visibility: String,
    pub owner_id: String,
    pub owner_name: String,
    pub can_manage: bool,
    #[serde(serialize_with = "crate::time::serialize")]
    pub created_at: NaiveDateTime,
    #[serde(serialize_with = "crate::time::serialize")]
    pub updated_at: NaiveDateTime,
}

#[derive(Serialize, Clone, Debug)]
pub struct FileItem {
    pub id: String,
    pub folder_id: Option<String>,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub visibility: String,
    pub owner_id: String,
    pub owner_name: String,
    pub can_delete: bool,
    pub version_number: i32,
    pub version_count: i64,
    #[serde(serialize_with = "crate::time::serialize")]
    pub created_at: NaiveDateTime,
    #[serde(serialize_with = "crate::time::serialize")]
    pub updated_at: NaiveDateTime,
}

pub fn folder_item(
    folder: &Folder,
    access: &SpaceAccess,
    names: &HashMap<String, String>,
) -> FileFolderItem {
    FileFolderItem {
        id: folder.id.clone(),
        parent_id: folder.parent_id.clone(),
        name: folder.name.clone(),
        visibility: folder.visibility.clone(),
        owner_id: folder.owner_id.clone(),
        owner_name: names.get(&folder.owner_id).cloned().unwrap_or_default(),
        can_manage: can_manage(&folder.owner_id, access),
        created_at: folder.created_at,
        updated_at: folder.updated_at,
    }
}

pub fn file_item(
    file: &FileRow,
    access: &SpaceAccess,
    names: &HashMap<String, String>,
) -> FileItem {
    FileItem {
        id: file.id.clone(),
        folder_id: file.folder_id.clone(),
        filename: file.filename.clone(),
        content_type: file.content_type.clone(),
        size_bytes: file.size_bytes,
        visibility: file.visibility.clone(),
        owner_id: file.owner_id.clone(),
        owner_name: names.get(&file.owner_id).cloned().unwrap_or_default(),
        can_delete: can_manage(&file.owner_id, access),
        version_number: file.version_number.unwrap_or(1),
        version_count: file.version_count,
        created_at: file.created_at,
        updated_at: file.updated_at,
    }
}

// ------------------------------------------------------------ write scopes

/// Lock one active folder of the space (`FOR UPDATE` or `FOR SHARE`).
pub async fn lock_folder(
    tx: &mut Transaction<'_, Postgres>,
    space_id: &str,
    folder_id: &str,
    exclusive: bool,
) -> ApiResult<Folder> {
    let sql = format!(
        "SELECT {FOLDER_COLUMNS} FROM file_manager_folders WHERE workspace_id = $1 AND id = $2 \
         AND deleted_at IS NULL {}",
        if exclusive { "FOR UPDATE" } else { "FOR SHARE" }
    );
    sqlx::query_as::<_, Folder>(&sql)
        .bind(space_id)
        .bind(folder_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(folder_not_found)
}

/// Lock one active, ready file of the space for update.
pub async fn lock_file(
    tx: &mut Transaction<'_, Postgres>,
    space_id: &str,
    file_id: &str,
) -> ApiResult<FileRow> {
    sqlx::query("SELECT id FROM file_manager_files WHERE id = $1 FOR UPDATE")
        .bind(file_id)
        .execute(&mut **tx)
        .await?;
    load_file(&mut **tx, file_id)
        .await?
        .filter(|f| {
            f.deleted_at.is_none() && f.current_version_id.is_some() && f.workspace_id == space_id
        })
        .ok_or_else(file_not_found)
}

/// The active folders under `roots` (included) and the active ready files in
/// them plus `file_ids`.
async fn discover_scope(
    tx: &mut Transaction<'_, Postgres>,
    space_id: &str,
    roots: &[String],
    file_ids: &[String],
) -> ApiResult<(BTreeSet<String>, BTreeSet<String>)> {
    let folders: Vec<String> = sqlx::query_scalar(
        "WITH RECURSIVE tree AS ( \
           SELECT id FROM file_manager_folders WHERE workspace_id = $1 AND id = ANY($2) AND deleted_at IS NULL \
           UNION SELECT c.id FROM file_manager_folders c JOIN tree t ON c.parent_id = t.id \
           WHERE c.workspace_id = $1 AND c.deleted_at IS NULL) SELECT id FROM tree",
    )
    .bind(space_id)
    .bind(roots)
    .fetch_all(&mut **tx)
    .await?;
    let files: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM file_manager_files WHERE workspace_id = $1 AND deleted_at IS NULL \
         AND current_version_id IS NOT NULL AND (id = ANY($2) OR folder_id = ANY($3))",
    )
    .bind(space_id)
    .bind(file_ids)
    .bind(&folders)
    .fetch_all(&mut **tx)
    .await?;
    Ok((folders.into_iter().collect(), files.into_iter().collect()))
}

/// AI-DO `_lock_tree_delete_scope` / `_lock_leaf_files_for_delete`: lock the
/// selected folders with their subtrees and the selected files in id order,
/// then discover again so rows added before the locks are covered too. Every
/// selected item must exist (404 otherwise).
pub async fn lock_tree_scope(
    tx: &mut Transaction<'_, Postgres>,
    space_id: &str,
    folder_ids: &[String],
    file_ids: &[String],
) -> ApiResult<(Vec<Folder>, Vec<FileRow>)> {
    let mut locked_folders: BTreeSet<String> = BTreeSet::new();
    let mut locked_files: BTreeSet<String> = BTreeSet::new();
    for _ in 0..5 {
        let (folders, files) = discover_scope(tx, space_id, folder_ids, file_ids).await?;
        let new_folders: Vec<String> = folders.difference(&locked_folders).cloned().collect();
        let new_files: Vec<String> = files.difference(&locked_files).cloned().collect();
        if new_folders.is_empty() && new_files.is_empty() {
            let folder_ids_all: Vec<String> = folders.into_iter().collect();
            let file_ids_all: Vec<String> = files.into_iter().collect();
            let sql = format!(
                "SELECT {FOLDER_COLUMNS} FROM file_manager_folders WHERE id = ANY($1) ORDER BY id"
            );
            let folder_rows = sqlx::query_as::<_, Folder>(&sql)
                .bind(&folder_ids_all)
                .fetch_all(&mut **tx)
                .await?;
            let sql = format!("{FILE_SELECT} WHERE f.id = ANY($1) ORDER BY f.id");
            let file_rows = sqlx::query_as::<_, FileRow>(&sql)
                .bind(&file_ids_all)
                .fetch_all(&mut **tx)
                .await?;
            let found_folders: HashSet<&str> = folder_rows.iter().map(|f| f.id.as_str()).collect();
            if folder_ids
                .iter()
                .any(|id| !found_folders.contains(id.as_str()))
            {
                return Err(folder_not_found());
            }
            let found_files: HashSet<&str> = file_rows.iter().map(|f| f.id.as_str()).collect();
            if file_ids.iter().any(|id| !found_files.contains(id.as_str())) {
                return Err(file_not_found());
            }
            return Ok((folder_rows, file_rows));
        }
        sqlx::query(
            "SELECT id FROM file_manager_folders WHERE id = ANY($1) ORDER BY id FOR UPDATE",
        )
        .bind(&new_folders)
        .execute(&mut **tx)
        .await?;
        sqlx::query("SELECT id FROM file_manager_files WHERE id = ANY($1) ORDER BY id FOR UPDATE")
            .bind(&new_files)
            .execute(&mut **tx)
            .await?;
        locked_folders.extend(new_folders);
        locked_files.extend(new_files);
    }
    Err(ApiError::conflict(
        "files.scope_changed",
        "The selection changed while it was being processed.",
    ))
}

/// AI-DO `_ensure_can_delete_rows`: admins may; anyone else must own every
/// row of the scope (a subtree with someone else's file is refused).
pub fn ensure_can_change_rows(
    access: &SpaceAccess,
    folders: &[Folder],
    files: &[FileRow],
) -> ApiResult<()> {
    if access.is_admin() {
        return Ok(());
    }
    let others = folders.iter().any(|f| f.owner_id != access.user_id)
        || files.iter().any(|f| f.owner_id != access.user_id);
    if others {
        return Err(delete_access_required());
    }
    Ok(())
}

/// AI-DO `_ensure_can_ingest_under_parent` and `resolve_file_ingress_policy`:
/// workspace members add at the root; inside a folder they must own it
/// unless they are a space admin.
pub fn ensure_can_ingest(access: &SpaceAccess, parent: Option<&Folder>) -> ApiResult<()> {
    if !access.is_member() {
        return Err(workspace_access_required());
    }
    if let Some(parent) = parent
        && parent.owner_id != access.user_id
        && !access.is_admin()
    {
        return Err(parent_manage_access_required());
    }
    Ok(())
}

/// The browse response's `ingest_block_reason` (AI-DO FileIngressBlockReason
/// without the corpus reasons).
pub fn ingest_block_reason(access: &SpaceAccess, parent: Option<&Folder>) -> Option<&'static str> {
    match ensure_can_ingest(access, parent) {
        Ok(()) => None,
        Err(error) if error.code == "files_workspace_access_required" => {
            Some("workspace_access_required")
        }
        Err(_) => Some("parent_manage_required"),
    }
}
