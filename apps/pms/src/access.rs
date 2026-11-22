//! Access rules: AI-DO domains/pms/access.py.
//!
//! A space is readable by its direct members (viewer, member, admin, owner).
//! Neither an app administrator nor the platform administrator flag bypasses
//! membership (AI-DO's `resolve_space_access`: "an app admin manages settings,
//! not data in someone else's workspace"); `Identity::admin` is never read.
//! A single task is also readable through an active explicit grant
//! (pms_task_user_access), which gives read of that task only. AI-DO also
//! required the caller's PMS app access; in miy the platform only forwards
//! requests of people who may use this app.
use sqlx::PgConnection;

use crate::error::{ApiError, ApiResult};
use crate::models::{SpaceRow, TASK_COLUMNS, TASK_LIST_COLUMNS, TaskListRow, TaskRow};

pub const ROLES: &[&str] = &["viewer", "member", "admin", "owner"];
pub const MANAGER_ROLES: &[&str] = &["admin", "owner"];
pub const EDITOR_ROLES: &[&str] = &["member", "admin", "owner"];

/// `normalize_app_space_role`.
pub fn normalize_role(role: &str) -> Option<String> {
    let role = role.trim().to_lowercase();
    ROLES.contains(&role.as_str()).then_some(role)
}

pub async fn load_active_space(
    conn: &mut PgConnection,
    space_id: &str,
) -> ApiResult<Option<SpaceRow>> {
    Ok(sqlx::query_as::<_, SpaceRow>(
        "SELECT id, key, name, description, active, created_at, updated_at FROM pms_spaces \
         WHERE id = $1 AND active",
    )
    .bind(space_id)
    .fetch_optional(conn)
    .await?)
}

/// The caller's role in an active space, if any.
pub async fn space_role(
    conn: &mut PgConnection,
    user_id: &str,
    space_id: &str,
) -> ApiResult<Option<String>> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT m.role FROM pms_space_members m JOIN pms_spaces s ON s.id = m.space_id \
         WHERE m.space_id = $1 AND m.user_id = $2 AND s.active",
    )
    .bind(space_id)
    .bind(user_id)
    .fetch_optional(conn)
    .await?;
    Ok(role.and_then(|role| normalize_role(&role)))
}

pub async fn ensure_space_access(
    conn: &mut PgConnection,
    user_id: &str,
    space_id: &str,
) -> ApiResult<(SpaceRow, String)> {
    let Some(space) = load_active_space(conn, space_id).await? else {
        return Err(ApiError::not_found("pms.space_not_found"));
    };
    match space_role(conn, user_id, space_id).await? {
        Some(role) => Ok((space, role)),
        None => Err(ApiError::forbidden("pms.space_access_required")),
    }
}

pub async fn ensure_space_manager(
    conn: &mut PgConnection,
    user_id: &str,
    space_id: &str,
) -> ApiResult<(SpaceRow, String)> {
    let (space, role) = ensure_space_access(conn, user_id, space_id).await?;
    if MANAGER_ROLES.contains(&role.as_str()) {
        return Ok((space, role));
    }
    Err(ApiError::forbidden("pms.space_owner_admin_required"))
}

pub async fn ensure_space_editor(
    conn: &mut PgConnection,
    user_id: &str,
    space_id: &str,
) -> ApiResult<(SpaceRow, String)> {
    let (space, role) = ensure_space_access(conn, user_id, space_id).await?;
    if EDITOR_ROLES.contains(&role.as_str()) {
        return Ok((space, role));
    }
    Err(ApiError::forbidden("pms.space_viewer_modify_denied"))
}

/// Active spaces the caller is a direct member of.
pub async fn accessible_space_ids(
    conn: &mut PgConnection,
    user_id: &str,
) -> ApiResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT s.id FROM pms_spaces s JOIN pms_space_members m ON m.space_id = s.id \
         WHERE s.active AND m.user_id = $1 AND lower(trim(m.role)) = ANY($2)",
    )
    .bind(user_id)
    .bind(ROLES)
    .fetch_all(conn)
    .await?)
}

pub async fn space_member_ids(conn: &mut PgConnection, space_id: &str) -> ApiResult<Vec<String>> {
    Ok(
        sqlx::query_scalar("SELECT user_id FROM pms_space_members WHERE space_id = $1")
            .bind(space_id)
            .fetch_all(conn)
            .await?,
    )
}

pub async fn load_list(conn: &mut PgConnection, list_id: &str) -> ApiResult<Option<TaskListRow>> {
    Ok(sqlx::query_as::<_, TaskListRow>(&format!(
        "SELECT {TASK_LIST_COLUMNS} FROM pms_task_lists WHERE id = $1"
    ))
    .bind(list_id)
    .fetch_optional(conn)
    .await?)
}

pub async fn ensure_list_member(
    conn: &mut PgConnection,
    user_id: &str,
    list_id: &str,
) -> ApiResult<(TaskListRow, String)> {
    let Some(list) = load_list(conn, list_id).await? else {
        return Err(ApiError::not_found("pms.task_list_not_found"));
    };
    let Some(team_id) = list.team_id.clone() else {
        return Err(ApiError::conflict("pms.task_list_space_missing"));
    };
    if load_active_space(conn, &team_id).await?.is_none() {
        return Err(ApiError::not_found("pms.task_list_not_found"));
    }
    match space_role(conn, user_id, &team_id).await? {
        Some(role) => Ok((list, role)),
        None => Err(ApiError::forbidden("pms.task_list_access_required")),
    }
}

pub fn ensure_task_list_active(list: &TaskListRow) -> ApiResult<()> {
    if list.archived {
        return Err(ApiError::conflict("pms.task_list_archived_read_only"));
    }
    Ok(())
}

/// List administration without the archived write guard.
pub async fn ensure_list_manager(
    conn: &mut PgConnection,
    user_id: &str,
    list_id: &str,
) -> ApiResult<(TaskListRow, String)> {
    let (list, role) = ensure_list_member(conn, user_id, list_id).await?;
    if !MANAGER_ROLES.contains(&role.as_str()) {
        return Err(ApiError::forbidden("pms.task_list_owner_admin_required"));
    }
    Ok((list, role))
}

pub async fn ensure_list_owner(
    conn: &mut PgConnection,
    user_id: &str,
    list_id: &str,
) -> ApiResult<(TaskListRow, String)> {
    let (list, role) = ensure_list_manager(conn, user_id, list_id).await?;
    ensure_task_list_active(&list)?;
    Ok((list, role))
}

pub async fn ensure_list_editor(
    conn: &mut PgConnection,
    user_id: &str,
    list_id: &str,
) -> ApiResult<(TaskListRow, String)> {
    let (list, role) = ensure_list_member(conn, user_id, list_id).await?;
    if role == "viewer" {
        return Err(ApiError::forbidden("pms.task_list_viewer_modify_denied"));
    }
    ensure_task_list_active(&list)?;
    Ok((list, role))
}

pub async fn load_task(conn: &mut PgConnection, task_id: &str) -> ApiResult<TaskRow> {
    sqlx::query_as::<_, TaskRow>(&format!(
        "SELECT {TASK_COLUMNS} FROM pms_tasks WHERE id = $1"
    ))
    .bind(task_id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| ApiError::not_found("pms.task_not_found"))
}

pub async fn load_task_for_update(conn: &mut PgConnection, task_id: &str) -> ApiResult<TaskRow> {
    sqlx::query_as::<_, TaskRow>(&format!(
        "SELECT {TASK_COLUMNS} FROM pms_tasks WHERE id = $1 FOR UPDATE"
    ))
    .bind(task_id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| ApiError::not_found("pms.task_not_found"))
}

// The queried task is `t`. Bind the viewer and valid space roles as $2/$3.
// A task-specific grant never confers access to another task in its hierarchy.
const TASK_READABLE: &str = "EXISTS ( \
    SELECT 1 FROM pms_task_lists l JOIN pms_spaces s ON s.id = l.team_id AND s.active \
    WHERE l.id = t.list_id AND ( \
        EXISTS (SELECT 1 FROM pms_space_members m WHERE m.space_id = s.id \
            AND m.user_id = $2 AND lower(trim(m.role)) = ANY($3)) \
        OR EXISTS (SELECT 1 FROM pms_task_user_access a WHERE a.task_id = t.id AND a.user_id = $2 \
            AND a.revoked_at IS NULL AND (a.expires_at IS NULL OR a.expires_at > (now() AT TIME ZONE 'UTC'))) \
    ))";

/// `can_read_pms_task_in_space`: membership of the task's active space or an
/// active read grant on this task.
pub async fn can_read_task(
    conn: &mut PgConnection,
    user_id: &str,
    task_id: &str,
) -> ApiResult<bool> {
    Ok(sqlx::query_scalar::<_, bool>(&format!(
        "SELECT EXISTS (SELECT 1 FROM pms_tasks t WHERE t.id = $1 AND {TASK_READABLE})"
    ))
    .bind(task_id)
    .bind(user_id)
    .bind(ROLES)
    .fetch_one(conn)
    .await?)
}

pub async fn readable_subtasks(
    conn: &mut PgConnection,
    user_id: &str,
    parent_id: &str,
) -> ApiResult<Vec<TaskRow>> {
    Ok(sqlx::query_as::<_, TaskRow>(&format!(
        "SELECT {TASK_COLUMNS} FROM pms_tasks t WHERE t.parent_id = $1 AND NOT t.archived \
         AND {TASK_READABLE} ORDER BY t.created_at, t.id"
    ))
    .bind(parent_id)
    .bind(user_id)
    .bind(ROLES)
    .fetch_all(conn)
    .await?)
}

pub async fn readable_subtask_counts(
    conn: &mut PgConnection,
    user_id: &str,
    parent_ids: &[&str],
) -> ApiResult<std::collections::HashMap<String, i64>> {
    Ok(sqlx::query_as::<_, (String, i64)>(&format!(
        "SELECT t.parent_id, count(*) FROM pms_tasks t WHERE t.parent_id = ANY($1) \
         AND {TASK_READABLE} GROUP BY t.parent_id"
    ))
    .bind(parent_ids)
    .bind(user_id)
    .bind(ROLES)
    .fetch_all(conn)
    .await?
    .into_iter()
    .collect())
}

pub async fn ensure_task_readable(
    conn: &mut PgConnection,
    user_id: &str,
    task_id: &str,
) -> ApiResult<TaskRow> {
    let task = load_task(conn, task_id).await?;
    if can_read_task(conn, user_id, task_id).await? {
        return Ok(task);
    }
    Err(ApiError::forbidden("pms.task_access_required"))
}

/// AI-DO `_get_task_for_user(require_editor=True)`: the task row locked and the
/// list checked for editing.
pub async fn task_for_editor(
    conn: &mut PgConnection,
    user_id: &str,
    task_id: &str,
) -> ApiResult<(TaskRow, TaskListRow)> {
    let task = load_task_for_update(conn, task_id).await?;
    let (list, _) = ensure_list_editor(conn, user_id, &task.list_id).await?;
    Ok((task, list))
}

/// For notifications: which of `user_ids` may read `task` now.
pub async fn readers_among(
    conn: &mut PgConnection,
    task: &TaskRow,
    user_ids: &[String],
) -> ApiResult<Vec<String>> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_scalar(
        "SELECT u.id FROM unnest($1::text[]) AS u(id) WHERE EXISTS ( \
           SELECT 1 FROM pms_task_lists l JOIN pms_spaces s ON s.id = l.team_id AND s.active \
           WHERE l.id = $2) \
         AND (EXISTS (SELECT 1 FROM pms_task_lists l JOIN pms_space_members m \
           ON m.space_id = l.team_id AND m.user_id = u.id WHERE l.id = $2) \
         OR EXISTS (SELECT 1 FROM pms_task_user_access a WHERE a.task_id = $3 AND a.user_id = u.id \
           AND a.revoked_at IS NULL AND (a.expires_at IS NULL OR a.expires_at > (now() AT TIME ZONE 'UTC'))))",
    )
    .bind(user_ids)
    .bind(&task.list_id)
    .bind(&task.id)
    .fetch_all(conn)
    .await?)
}
