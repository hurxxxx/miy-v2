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
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::access::{ensure_list_editor, ensure_task_readable, task_for_editor};
use crate::error::{ApiError, ApiResult, check_length};
use crate::input::{Body, Params, check_list_length, check_opt_length};
use crate::models::{
    ActivityRow, ChecklistRow, iso_datetime, new_id, paginate, task_reference, utcnow,
};
use crate::tasks::{
    TaskUpdate, actor_name, apply_task_update, log_activity, serialize_checklist_item,
    validate_member_users,
};
use crate::users;

/// `GET /v1/pms/tasks/{task_id}/activity-logs`: newest first, without the
/// noisy description_blocks entries.
pub async fn list_activity_logs(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(task_id): Path<String>,
    params: Params,
) -> ApiResult<Json<Value>> {
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
    let items: Vec<Value> = page_logs
        .iter()
        .map(|log| {
            json!({
                "id": log.id,
                "task_id": log.task_id,
                "actor_id": log.actor_id,
                "actor_name": log.actor_id.as_ref().map(|id| names.get(id).cloned().unwrap_or_else(|| id.clone())),
                "action": log.action,
                "field_name": log.field_name,
                "from_value": log.from_value,
                "to_value": log.to_value,
                "message": log.message,
                "created_at": iso_datetime(&log.created_at),
            })
        })
        .collect();
    Ok(Json(json!({
        "items": items, "total": total, "page": page, "page_size": page_size,
    })))
}

// ── Checklist ──────────────────────────────────────────────────────────

/// `POST /v1/pms/tasks/{task_id}/checklist`.
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
pub async fn update_checklist_item(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(item_id): Path<String>,
    body: Body,
) -> ApiResult<Json<Value>> {
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
pub async fn set_assignees(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(task_id): Path<String>,
    body: Body,
) -> ApiResult<Json<Value>> {
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
    let ids = task["assignee_ids"].as_array().cloned().unwrap_or_default();
    let names = task["assignee_names"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    Ok(Json(Value::Array(
        ids.iter()
            .enumerate()
            .map(|(index, id)| {
                json!({
                    "user_id": id,
                    "full_name": names.get(index).cloned().unwrap_or_else(|| id.clone()),
                })
            })
            .collect(),
    )))
}

/// `PUT /v1/pms/tasks/{task_id}/followers`.
pub async fn set_followers(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(task_id): Path<String>,
    body: Body,
) -> ApiResult<Json<Value>> {
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
    Ok(Json(Value::Array(
        followers
            .iter()
            .map(|id| json!({"user_id": id, "full_name": names.get(id).cloned().unwrap_or_else(|| id.clone())}))
            .collect(),
    )))
}
