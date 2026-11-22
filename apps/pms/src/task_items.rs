//! Task work items: activity logs, checklists, assignees and followers
//! (router.py, task_work_items.py). Templates and custom fields, which AI-DO
//! kept in task_list_customization.py, are in templates.rs and
//! custom_fields.rs.
use std::collections::HashMap;

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use miy_app_sdk::Identity;
use serde::Serialize;
use sqlx::PgPool;

use crate::access::{ensure_list_editor, ensure_task_readable, task_for_editor};
use crate::error::{ApiError, ApiResult, ErrorResponse, IdentityErrorResponse, check_length};
use crate::input::{Body, Params, check_list_length, check_opt_length};
use crate::models::{
    ActivityRow, ChecklistRow, iso_datetime, new_id, paginate, task_reference, utcnow,
};
use crate::tasks::{
    TaskChecklistItem, TaskUpdate, actor_name, apply_task_update, log_activity,
    serialize_checklist_item, validate_member_users,
};
use crate::users;

#[derive(Serialize, utoipa::ToSchema)]
struct TaskActivityLog {
    id: String,
    task_id: String,
    #[schema(required = true)]
    actor_id: Option<String>,
    #[schema(required = true)]
    actor_name: Option<String>,
    action: String,
    #[schema(required = true)]
    field_name: Option<String>,
    #[schema(required = true)]
    from_value: Option<String>,
    #[schema(required = true)]
    to_value: Option<String>,
    message: String,
    created_at: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TaskActivityPage {
    items: Vec<TaskActivityLog>,
    total: usize,
    page: i64,
    page_size: i64,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
struct CreateChecklistItemRequest {
    #[schema(min_length = 1, max_length = 500)]
    text: String,
    /// Defaults to 0.
    sort_order: Option<i64>,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
struct UpdateChecklistItemRequest {
    #[schema(min_length = 1, max_length = 500)]
    text: Option<String>,
    completed: Option<bool>,
    sort_order: Option<i64>,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
struct ReorderChecklistRequest {
    item_ids: Vec<String>,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
struct SetTaskAssigneesRequest {
    #[schema(max_items = 20)]
    user_ids: Vec<String>,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
struct SetTaskFollowersRequest {
    #[schema(max_items = 50)]
    user_ids: Vec<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TaskUserAssignment {
    user_id: String,
    full_name: String,
}

/// `GET /v1/pms/tasks/{task_id}/activity-logs`: newest first, without the
/// noisy description_blocks entries.
#[utoipa::path(
    get, path = "/v1/pms/tasks/{task_id}/activity-logs", operation_id = "listPmsTaskActivityLogs", tag = "Task work items",
    params(("task_id" = String, Path), ("page" = Option<i64>, Query, minimum = 1, description = "Defaults to 1."), ("page_size" = Option<i64>, Query, minimum = 1, maximum = 100, description = "Defaults to 50.")),
    responses(
        (status = 200, description = "Activity history, newest first", body = TaskActivityPage),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller lacks access", body = ErrorResponse),
        (status = 404, description = "The requested resource was not found", body = ErrorResponse),
        (status = 422, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn list_activity_logs(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(task_id): Path<String>,
    params: Params,
) -> ApiResult<Json<TaskActivityPage>> {
    let page = params.int("page", 1, 1, None)?;
    let page_size = params.int("page_size", 50, 1, Some(100))?;
    let mut conn = pool.acquire().await?;
    let task = ensure_task_readable(&mut conn, &identity.user_id, &task_id).await?;
    let logs: Vec<ActivityRow> = sqlx::query_as::<_, ActivityRow>(
        "SELECT id, task_id, actor_id, action, field_name, from_value, to_value, message, created_at \
         FROM pms_task_activity_logs WHERE task_id = $1 \
         AND (field_name IS NULL OR field_name <> 'description_blocks') ORDER BY created_at DESC, id",
    )
    .bind(&task.id)
    .fetch_all(&mut *conn)
    .await?;
    drop(conn);
    let (page_logs, total) = paginate(&logs, page, page_size);
    let actors: Vec<String> = page_logs
        .iter()
        .filter_map(|log| log.actor_id.clone())
        .collect();
    let names = users::names(&identity, &actors).await;
    let items = page_logs
        .into_iter()
        .map(|log| {
            let actor_name = log
                .actor_id
                .as_ref()
                .map(|id| names.get(id).cloned().unwrap_or_else(|| id.clone()));
            TaskActivityLog {
                id: log.id,
                task_id: log.task_id,
                actor_id: log.actor_id,
                actor_name,
                action: log.action,
                field_name: log.field_name,
                from_value: log.from_value,
                to_value: log.to_value,
                message: log.message,
                created_at: iso_datetime(&log.created_at),
            }
        })
        .collect();
    Ok(Json(TaskActivityPage {
        items,
        total,
        page,
        page_size,
    }))
}

// ── Checklist ──────────────────────────────────────────────────────────

/// `POST /v1/pms/tasks/{task_id}/checklist`.
#[utoipa::path(
    post, path = "/v1/pms/tasks/{task_id}/checklist", operation_id = "createPmsChecklistItem", tag = "Task work items",
    params(("task_id" = String, Path)),
    request_body = CreateChecklistItemRequest,
    responses(
        (status = 201, description = "Created checklist item", body = TaskChecklistItem),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller lacks access", body = ErrorResponse),
        (status = 404, description = "The requested resource was not found", body = ErrorResponse),
        (status = 409, description = "The current list or resource state conflicts with the request", body = ErrorResponse),
        (status = 422, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn create_checklist_item(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(task_id): Path<String>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    let text = body.req_str("text")?;
    check_length("text", &text, 1, 500)?;
    let sort_order = body.opt_int("sort_order")?.unwrap_or(0);
    let name = actor_name(&identity).await;
    let mut tx = pool.begin().await?;
    let (task, _) = task_for_editor(&mut tx, &identity.user_id, &task_id).await?;
    let item = ChecklistRow {
        id: new_id(),
        task_id: task.id.clone(),
        text,
        completed: false,
        sort_order: sort_order as i32,
        created_at: utcnow(),
    };
    sqlx::query(
        "INSERT INTO pms_checklist_items (id, task_id, text, completed, sort_order, created_at) \
         VALUES ($1, $2, $3, false, $4, $5)",
    )
    .bind(&item.id)
    .bind(&item.task_id)
    .bind(&item.text)
    .bind(item.sort_order)
    .bind(item.created_at)
    .execute(&mut *tx)
    .await?;
    log_activity(
        &mut tx,
        &task.id,
        Some(&identity.user_id),
        "checklist_added",
        &format!("{name} added checklist item."),
        None,
        None,
        None,
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(serialize_checklist_item(&item))))
}

async fn load_checklist_item(
    conn: &mut sqlx::PgConnection,
    item_id: &str,
) -> ApiResult<(ChecklistRow, String)> {
    #[derive(sqlx::FromRow)]
    struct Row {
        #[sqlx(flatten)]
        item: ChecklistRow,
        list_id: String,
    }
    let row = sqlx::query_as::<_, Row>(
        "SELECT c.id, c.task_id, c.text, c.completed, c.sort_order, c.created_at, t.list_id \
         FROM pms_checklist_items c JOIN pms_tasks t ON t.id = c.task_id WHERE c.id = $1 FOR UPDATE OF c",
    )
    .bind(item_id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| ApiError::not_found("pms.checklist_item_not_found"))?;
    Ok((row.item, row.list_id))
}

/// `PATCH /v1/pms/checklist/{item_id}`.
#[utoipa::path(
    patch, path = "/v1/pms/checklist/{item_id}", operation_id = "updatePmsChecklistItem", tag = "Task work items",
    params(("item_id" = String, Path)),
    request_body = UpdateChecklistItemRequest,
    responses(
        (status = 200, description = "Updated checklist item", body = TaskChecklistItem),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller lacks access", body = ErrorResponse),
        (status = 404, description = "The requested resource was not found", body = ErrorResponse),
        (status = 409, description = "The current list or resource state conflicts with the request", body = ErrorResponse),
        (status = 422, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn update_checklist_item(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(item_id): Path<String>,
    body: Body,
) -> ApiResult<Json<TaskChecklistItem>> {
    let text = body.opt_str("text")?;
    check_opt_length("text", &text, 1, 500)?;
    let completed = body.opt_bool("completed")?;
    let sort_order = body.opt_int("sort_order")?;
    let name = actor_name(&identity).await;
    let mut tx = pool.begin().await?;
    let (mut item, list_id) = load_checklist_item(&mut tx, &item_id).await?;
    ensure_list_editor(&mut tx, &identity.user_id, &list_id).await?;
    if let Some(text) = text {
        item.text = text;
    }
    if let Some(completed) = completed
        && completed != item.completed
    {
        item.completed = completed;
        log_activity(
            &mut tx,
            &item.task_id,
            Some(&identity.user_id),
            if completed {
                "checklist_checked"
            } else {
                "checklist_unchecked"
            },
            &format!(
                "{name} {} \"{}\".",
                if completed { "checked" } else { "unchecked" },
                item.text
            ),
            None,
            None,
            None,
        )
        .await?;
    }
    if let Some(order) = sort_order {
        item.sort_order = order as i32;
    }
    sqlx::query(
        "UPDATE pms_checklist_items SET text = $2, completed = $3, sort_order = $4 WHERE id = $1",
    )
    .bind(&item.id)
    .bind(&item.text)
    .bind(item.completed)
    .bind(item.sort_order)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(serialize_checklist_item(&item)))
}

/// `DELETE /v1/pms/checklist/{item_id}`.
#[utoipa::path(
    delete, path = "/v1/pms/checklist/{item_id}", operation_id = "deletePmsChecklistItem", tag = "Task work items",
    params(("item_id" = String, Path)),
    responses(
        (status = 204, description = "Checklist item deleted"),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller lacks access", body = ErrorResponse),
        (status = 404, description = "The requested resource was not found", body = ErrorResponse),
        (status = 409, description = "The current list or resource state conflicts with the request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn delete_checklist_item(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(item_id): Path<String>,
) -> ApiResult<StatusCode> {
    let name = actor_name(&identity).await;
    let mut tx = pool.begin().await?;
    let (item, list_id) = load_checklist_item(&mut tx, &item_id).await?;
    ensure_list_editor(&mut tx, &identity.user_id, &list_id).await?;
    log_activity(
        &mut tx,
        &item.task_id,
        Some(&identity.user_id),
        "checklist_removed",
        &format!("{name} removed checklist item \"{}\".", item.text),
        None,
        None,
        None,
    )
    .await?;
    sqlx::query("DELETE FROM pms_checklist_items WHERE id = $1")
        .bind(&item.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PATCH /v1/pms/tasks/{task_id}/checklist/reorder`: unknown ids are
/// ignored; items left out keep their order value.
#[utoipa::path(
    patch, path = "/v1/pms/tasks/{task_id}/checklist/reorder", operation_id = "reorderPmsChecklist", tag = "Task work items",
    params(("task_id" = String, Path)),
    request_body = ReorderChecklistRequest,
    responses(
        (status = 204, description = "Checklist positions updated"),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller lacks access", body = ErrorResponse),
        (status = 404, description = "The requested resource was not found", body = ErrorResponse),
        (status = 409, description = "The current list or resource state conflicts with the request", body = ErrorResponse),
        (status = 422, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn reorder_checklist(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(task_id): Path<String>,
    body: Body,
) -> ApiResult<StatusCode> {
    let item_ids = body
        .opt_str_list("item_ids")?
        .ok_or_else(|| ApiError::validation("item_ids: Field required"))?;
    let mut tx = pool.begin().await?;
    let (task, _) = task_for_editor(&mut tx, &identity.user_id, &task_id).await?;
    for (index, item_id) in item_ids.iter().enumerate() {
        sqlx::query(
            "UPDATE pms_checklist_items SET sort_order = $3 WHERE id = $1 AND task_id = $2",
        )
        .bind(item_id)
        .bind(&task.id)
        .bind(index as i32)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Assignees and followers ────────────────────────────────────────────

/// `PUT /v1/pms/tasks/{task_id}/assignees`: an update of `assignee_ids`.
#[utoipa::path(
    put, path = "/v1/pms/tasks/{task_id}/assignees", operation_id = "setPmsTaskAssignees", tag = "Task work items",
    params(("task_id" = String, Path)),
    request_body = SetTaskAssigneesRequest,
    responses(
        (status = 200, description = "Current assignees", body = [TaskUserAssignment]),
        (status = 400, description = "The selection or related fields are invalid", body = ErrorResponse),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller lacks access", body = ErrorResponse),
        (status = 404, description = "The requested resource was not found", body = ErrorResponse),
        (status = 409, description = "The current list or resource state conflicts with the request", body = ErrorResponse),
        (status = 422, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn set_assignees(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(task_id): Path<String>,
    body: Body,
) -> ApiResult<Json<Vec<TaskUserAssignment>>> {
    let user_ids = body
        .opt_str_list("user_ids")?
        .ok_or_else(|| ApiError::validation("user_ids: Field required"))?;
    check_list_length("user_ids", user_ids.len(), 0, 20)?;
    let task = apply_task_update(
        &pool,
        &identity,
        &task_id,
        TaskUpdate::assignees_only(user_ids),
    )
    .await?;
    Ok(Json(
        task.assignee_ids
            .into_iter()
            .enumerate()
            .map(|(index, user_id)| TaskUserAssignment {
                full_name: task
                    .assignee_names
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| user_id.clone()),
                user_id,
            })
            .collect(),
    ))
}

/// `PUT /v1/pms/tasks/{task_id}/followers`.
#[utoipa::path(
    put, path = "/v1/pms/tasks/{task_id}/followers", operation_id = "setPmsTaskFollowers", tag = "Task work items",
    params(("task_id" = String, Path)),
    request_body = SetTaskFollowersRequest,
    responses(
        (status = 200, description = "Current followers", body = [TaskUserAssignment]),
        (status = 400, description = "The selection or related fields are invalid", body = ErrorResponse),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller lacks access", body = ErrorResponse),
        (status = 404, description = "The requested resource was not found", body = ErrorResponse),
        (status = 409, description = "The current list or resource state conflicts with the request", body = ErrorResponse),
        (status = 422, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn set_followers(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(task_id): Path<String>,
    body: Body,
) -> ApiResult<Json<Vec<TaskUserAssignment>>> {
    let user_ids = body
        .opt_str_list("user_ids")?
        .ok_or_else(|| ApiError::validation("user_ids: Field required"))?;
    check_list_length("user_ids", user_ids.len(), 0, 50)?;
    let name = actor_name(&identity).await;
    let mut tx = pool.begin().await?;
    let (task, list) = task_for_editor(&mut tx, &identity.user_id, &task_id).await?;
    let followers = validate_member_users(
        &mut tx,
        &list,
        &user_ids,
        "pms.followers_task_list_members_required",
    )
    .await?;
    sqlx::query("DELETE FROM pms_task_followers WHERE task_id = $1")
        .bind(&task.id)
        .execute(&mut *tx)
        .await?;
    for (position, user_id) in followers.iter().enumerate() {
        sqlx::query(
            "INSERT INTO pms_task_followers (id, task_id, user_id, position) VALUES ($1, $2, $3, $4)",
        )
        .bind(new_id())
        .bind(&task.id)
        .bind(user_id)
        .bind(position as i32)
        .execute(&mut *tx)
        .await?;
    }
    log_activity(
        &mut tx,
        &task.id,
        Some(&identity.user_id),
        "updated",
        &format!(
            "{name} updated followers for {}.",
            task_reference(&list.key, task.task_number)
        ),
        Some("follower_ids"),
        None,
        (!followers.is_empty()).then(|| followers.join(",")),
    )
    .await?;
    tx.commit().await?;
    let names: HashMap<String, String> = users::names(&identity, &followers).await;
    Ok(Json(
        followers
            .into_iter()
            .map(|user_id| TaskUserAssignment {
                full_name: names
                    .get(&user_id)
                    .cloned()
                    .unwrap_or_else(|| user_id.clone()),
                user_id,
            })
            .collect(),
    ))
}
