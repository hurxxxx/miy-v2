//! Task templates: AI-DO task_list_customization.py and router.py (list,
//! create, update, delete; the new-task dialog fills title, status and priority
//! from one). miy reinforcement (docs/PORTING.md §10.3): a template belongs to
//! a space instead of a list, holds the whole task (description, checklist,
//! labels by name and color, direct subtasks with their checklists and labels),
//! can be saved from an existing task, and creating a task with `template_id`
//! adds the checklist, labels and subtasks in the same transaction.
//!
//! Space members read templates; members, admins and owners (editors) create,
//! change and delete them, as AI-DO let list editors. Writes share-lock the
//! space and check the caller's role after the lock.
use std::collections::{HashMap, HashSet};

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use miy_app_sdk::Identity;
use serde_json::{Map, Value, json};
use sqlx::{PgConnection, PgPool};

use crate::access::{
    EDITOR_ROLES, ensure_list_member, ensure_space_access, ensure_task_readable, load_task,
    lock_space_for_write,
};
use crate::copy::initial_status;
use crate::error::{ApiError, ApiResult, check_choice, check_length};
use crate::input::{Body, check_opt_length};
use crate::models::{
    TASK_COLUMNS, TaskListRow, TaskRow, iso_datetime, new_id, task_reference, utcnow,
};
use crate::tasks::{list_workflow, log_activity, next_board_position, next_task_number};
use crate::workflow::PRIORITIES;

const MAX_CHECKLIST: usize = 100;
const MAX_LABELS: usize = 20;
const MAX_SUBTASKS: usize = 50;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TemplateRow {
    pub id: String,
    pub team_id: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub description_blocks: Option<Value>,
    pub default_status: String,
    pub default_priority: String,
    pub checklist_items: Option<Value>,
    pub labels: Option<Value>,
    pub subtasks: Option<Value>,
    pub created_by_id: Option<String>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

const TEMPLATE_COLUMNS: &str = "id, team_id, name, title, description, description_blocks, \
     default_status, default_priority, checklist_items, labels, subtasks, created_by_id, \
     created_at, updated_at";

fn array_len(value: &Option<Value>) -> usize {
    value.as_ref().and_then(Value::as_array).map_or(0, Vec::len)
}

fn serialize_template(template: &TemplateRow) -> Value {
    json!({
        "id": template.id,
        "team_id": template.team_id,
        "name": template.name,
        "title": template.title,
        "description": template.description,
        "description_blocks": template.description_blocks,
        "default_status": template.default_status,
        "default_priority": template.default_priority,
        "checklist_items": template.checklist_items,
        "labels": template.labels,
        "subtasks": template.subtasks,
        "subtask_count": array_len(&template.subtasks),
        "checklist_count": array_len(&template.checklist_items),
        "created_by_id": template.created_by_id,
        "created_at": iso_datetime(&template.created_at),
        "updated_at": iso_datetime(&template.updated_at),
    })
}

// ── Content checks ─────────────────────────────────────────────────────

fn field_str<'a>(item: &'a Map<String, Value>, name: &str) -> ApiResult<Option<&'a str>> {
    match item.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(ApiError::validation(format!(
            "{name}: Input should be a valid string"
        ))),
    }
}

fn objects<'a>(value: &'a Value, name: &str, max: usize) -> ApiResult<Vec<&'a Map<String, Value>>> {
    let items = value
        .as_array()
        .ok_or_else(|| ApiError::validation(format!("{name}: Input should be a valid list")))?;
    if items.len() > max {
        return Err(ApiError::validation(format!(
            "{name}: List should have at most {max} items after validation"
        )));
    }
    items
        .iter()
        .map(|item| {
            item.as_object().ok_or_else(|| {
                ApiError::validation(format!("{name}: Input should be a valid dictionary"))
            })
        })
        .collect()
}

/// `[{text}]`, 1–500 characters each. AI-DO stored free dictionaries; only the
/// text is kept (a checklist made from a template starts unchecked).
pub fn checklist_content(value: &Value) -> ApiResult<Value> {
    let mut result = Vec::new();
    for item in objects(value, "checklist_items", MAX_CHECKLIST)? {
        let text = field_str(item, "text")?.unwrap_or("").trim().to_string();
        check_length("checklist_items.text", &text, 1, 500)?;
        result.push(json!({"text": text}));
    }
    Ok(Value::Array(result))
}

/// `[{name, color}]`: label names (1–48) unique ignoring case, color as given.
pub fn labels_content(value: &Value) -> ApiResult<Value> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for item in objects(value, "labels", MAX_LABELS)? {
        let name = field_str(item, "name")?.unwrap_or("").trim().to_string();
        check_length("labels.name", &name, 1, 48)?;
        let color = field_str(item, "color")?
            .unwrap_or("#1f2d38")
            .trim()
            .to_string();
        check_length("labels.color", &color, 0, 24)?;
        if seen.insert(name.to_lowercase()) {
            result.push(json!({"name": name, "color": color}));
        }
    }
    Ok(Value::Array(result))
}

/// `[{title, description, description_blocks, priority, checklist_items,
/// labels}]`, at most 50, with the task rules for title and description.
pub fn subtasks_content(value: &Value) -> ApiResult<Value> {
    let mut result = Vec::new();
    for item in objects(value, "subtasks", MAX_SUBTASKS)? {
        let title = field_str(item, "title")?.unwrap_or("").trim().to_string();
        check_length("subtasks.title", &title, 2, 180)?;
        let description = field_str(item, "description")?
            .unwrap_or("")
            .trim()
            .to_string();
        check_length("subtasks.description", &description, 0, 4000)?;
        let blocks = match item.get("description_blocks") {
            None | Some(Value::Null) => Value::Null,
            Some(Value::Array(blocks)) => Value::Array(blocks.clone()),
            Some(_) => {
                return Err(ApiError::validation(
                    "subtasks.description_blocks: Input should be a valid list",
                ));
            }
        };
        let priority = field_str(item, "priority")?.unwrap_or("medium").to_string();
        check_choice("subtasks.priority", &priority, PRIORITIES)?;
        let checklist = match item.get("checklist_items") {
            None | Some(Value::Null) => json!([]),
            Some(value) => checklist_content(value)?,
        };
        let labels = match item.get("labels") {
            None | Some(Value::Null) => json!([]),
            Some(value) => labels_content(value)?,
        };
        result.push(json!({
            "title": title, "description": description, "description_blocks": blocks,
            "priority": priority, "checklist_items": checklist, "labels": labels,
        }));
    }
    Ok(Value::Array(result))
}

fn opt_content(
    body: &Body,
    name: &str,
    check: fn(&Value) -> ApiResult<Value>,
) -> ApiResult<Option<Value>> {
    match body.0.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => check(value).map(Some),
    }
}

const TEMPLATE_FIELDS: &[&str] = &[
    "name",
    "title",
    "description",
    "description_blocks",
    "default_status",
    "default_priority",
    "checklist_items",
    "labels",
    "subtasks",
];

// ── Reads ──────────────────────────────────────────────────────────────

async fn space_templates(conn: &mut PgConnection, team_id: &str) -> ApiResult<Vec<TemplateRow>> {
    Ok(sqlx::query_as::<_, TemplateRow>(&format!(
        "SELECT {TEMPLATE_COLUMNS} FROM pms_task_templates WHERE team_id = $1 \
         ORDER BY created_at DESC, id"
    ))
    .bind(team_id)
    .fetch_all(conn)
    .await?)
}

fn template_list(templates: &[TemplateRow]) -> Json<Value> {
    Json(json!({"items": templates.iter().map(serialize_template).collect::<Vec<_>>()}))
}

/// `GET /v1/pms/lists/{list_id}/templates`: the templates of the list's space,
/// newest first (the new-task dialog knows the list).
pub async fn list_list_templates(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let mut conn = pool.acquire().await?;
    let (list, _) = ensure_list_member(&mut conn, &identity.user_id, &list_id).await?;
    let team_id = list
        .team_id
        .ok_or_else(|| ApiError::conflict("pms.task_list_space_missing"))?;
    Ok(template_list(&space_templates(&mut conn, &team_id).await?))
}

/// `GET /v1/pms/spaces/{space_id}/templates`.
pub async fn list_space_templates(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let mut conn = pool.acquire().await?;
    let (space, _) = ensure_space_access(&mut conn, &identity.user_id, &space_id).await?;
    Ok(template_list(&space_templates(&mut conn, &space.id).await?))
}

async fn load_template(conn: &mut PgConnection, template_id: &str) -> ApiResult<TemplateRow> {
    sqlx::query_as::<_, TemplateRow>(&format!(
        "SELECT {TEMPLATE_COLUMNS} FROM pms_task_templates WHERE id = $1"
    ))
    .bind(template_id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| ApiError::not_found("pms.template_not_found"))
}

/// `GET /v1/pms/templates/{template_id}` (miy).
pub async fn get_template(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(template_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let mut conn = pool.acquire().await?;
    let template = load_template(&mut conn, &template_id).await?;
    ensure_space_access(&mut conn, &identity.user_id, &template.team_id).await?;
    Ok(Json(serialize_template(&template)))
}

// ── Writes ─────────────────────────────────────────────────────────────

fn require_editor(role: &str) -> ApiResult<()> {
    if EDITOR_ROLES.contains(&role) {
        Ok(())
    } else {
        Err(ApiError::forbidden("pms.space_viewer_modify_denied"))
    }
}

async fn insert_template(conn: &mut PgConnection, template: &TemplateRow) -> ApiResult<()> {
    sqlx::query(&format!(
        "INSERT INTO pms_task_templates ({TEMPLATE_COLUMNS}) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)"
    ))
    .bind(&template.id)
    .bind(&template.team_id)
    .bind(&template.name)
    .bind(&template.title)
    .bind(&template.description)
    .bind(&template.description_blocks)
    .bind(&template.default_status)
    .bind(&template.default_priority)
    .bind(&template.checklist_items)
    .bind(&template.labels)
    .bind(&template.subtasks)
    .bind(&template.created_by_id)
    .bind(template.created_at)
    .bind(template.updated_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// `POST /v1/pms/spaces/{space_id}/templates`: a template written by hand.
pub async fn create_template(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    body.forbid_extra(TEMPLATE_FIELDS)?;
    let name = body.req_str("name")?.trim().to_string();
    check_length("name", &name, 1, 140)?;
    let title = body.str_or("title", "")?.trim().to_string();
    check_length("title", &title, 0, 180)?;
    let description = body.str_or("description", "")?.trim().to_string();
    check_length("description", &description, 0, 4000)?;
    let description_blocks = body.opt_dict_list("description_blocks")?;
    let default_status = body.str_or("default_status", "todo")?;
    check_length("default_status", &default_status, 1, 40)?;
    let default_priority = body.str_or("default_priority", "medium")?;
    check_choice("default_priority", &default_priority, PRIORITIES)?;
    let checklist_items = opt_content(&body, "checklist_items", checklist_content)?;
    let labels = opt_content(&body, "labels", labels_content)?;
    let subtasks = opt_content(&body, "subtasks", subtasks_content)?;
    let mut tx = pool.begin().await?;
    let (space, role) = lock_space_for_write(&mut tx, &identity.user_id, &space_id).await?;
    require_editor(&role)?;
    let now = utcnow();
    let template = TemplateRow {
        id: new_id(),
        team_id: space.id,
        name,
        title,
        description,
        description_blocks,
        default_status,
        default_priority,
        checklist_items,
        labels,
        subtasks,
        created_by_id: Some(identity.user_id.clone()),
        created_at: now,
        updated_at: now,
    };
    insert_template(&mut tx, &template).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(serialize_template(&template))))
}

/// Share-lock the template's space, check the caller edits it now, then lock
/// the template row.
async fn template_for_editor(
    conn: &mut PgConnection,
    user_id: &str,
    template_id: &str,
) -> ApiResult<TemplateRow> {
    let team_id: String =
        sqlx::query_scalar("SELECT team_id FROM pms_task_templates WHERE id = $1")
            .bind(template_id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or_else(|| ApiError::not_found("pms.template_not_found"))?;
    let (_, role) = lock_space_for_write(conn, user_id, &team_id).await?;
    require_editor(&role)?;
    sqlx::query_as::<_, TemplateRow>(&format!(
        "SELECT {TEMPLATE_COLUMNS} FROM pms_task_templates WHERE id = $1 FOR UPDATE"
    ))
    .bind(template_id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| ApiError::not_found("pms.template_not_found"))
}

/// `PATCH /v1/pms/templates/{template_id}`: the fields sent with a value;
/// `description_blocks: null` clears the formatted description.
pub async fn update_template(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(template_id): Path<String>,
    body: Body,
) -> ApiResult<Json<Value>> {
    body.forbid_extra(TEMPLATE_FIELDS)?;
    let name = body.opt_str("name")?.map(|name| name.trim().to_string());
    check_opt_length("name", &name, 1, 140)?;
    let title = body.opt_str("title")?.map(|title| title.trim().to_string());
    check_opt_length("title", &title, 0, 180)?;
    let description = body
        .opt_str("description")?
        .map(|text| text.trim().to_string());
    check_opt_length("description", &description, 0, 4000)?;
    let description_blocks = body.opt_dict_list("description_blocks")?;
    let default_status = body.opt_str("default_status")?;
    check_opt_length("default_status", &default_status, 1, 40)?;
    let default_priority = body.opt_choice("default_priority", PRIORITIES)?;
    let checklist_items = opt_content(&body, "checklist_items", checklist_content)?;
    let labels = opt_content(&body, "labels", labels_content)?;
    let subtasks = opt_content(&body, "subtasks", subtasks_content)?;
    let mut tx = pool.begin().await?;
    let mut template = template_for_editor(&mut tx, &identity.user_id, &template_id).await?;
    if let Some(name) = name {
        template.name = name;
    }
    if let Some(title) = title {
        template.title = title;
    }
    if let Some(description) = description {
        template.description = description;
    }
    if body.has("description_blocks") {
        template.description_blocks = description_blocks;
    }
    if let Some(status) = default_status {
        template.default_status = status;
    }
    if let Some(priority) = default_priority {
        template.default_priority = priority;
    }
    if checklist_items.is_some() {
        template.checklist_items = checklist_items;
    }
    if labels.is_some() {
        template.labels = labels;
    }
    if subtasks.is_some() {
        template.subtasks = subtasks;
    }
    template.updated_at = utcnow();
    sqlx::query(
        "UPDATE pms_task_templates SET name = $2, title = $3, description = $4, \
         description_blocks = $5, default_status = $6, default_priority = $7, \
         checklist_items = $8, labels = $9, subtasks = $10, updated_at = $11 WHERE id = $1",
    )
    .bind(&template.id)
    .bind(&template.name)
    .bind(&template.title)
    .bind(&template.description)
    .bind(&template.description_blocks)
    .bind(&template.default_status)
    .bind(&template.default_priority)
    .bind(&template.checklist_items)
    .bind(&template.labels)
    .bind(&template.subtasks)
    .bind(template.updated_at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(serialize_template(&template)))
}

/// `DELETE /v1/pms/templates/{template_id}`.
pub async fn delete_template(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(template_id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = pool.begin().await?;
    let template = template_for_editor(&mut tx, &identity.user_id, &template_id).await?;
    sqlx::query("DELETE FROM pms_task_templates WHERE id = $1")
        .bind(&template.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Save a task as a template ──────────────────────────────────────────

async fn checklist_of(conn: &mut PgConnection, task_id: &str) -> ApiResult<Value> {
    let texts: Vec<String> = sqlx::query_scalar(
        "SELECT text FROM pms_checklist_items WHERE task_id = $1 ORDER BY sort_order, created_at, id",
    )
    .bind(task_id)
    .fetch_all(conn)
    .await?;
    Ok(Value::Array(
        texts
            .into_iter()
            .take(MAX_CHECKLIST)
            .map(|text| json!({"text": text}))
            .collect(),
    ))
}

async fn labels_of(conn: &mut PgConnection, task_id: &str) -> ApiResult<Value> {
    let labels: Vec<(String, String)> = sqlx::query_as(
        "SELECT l.name, l.color FROM pms_task_labels tl JOIN pms_labels l ON l.id = tl.label_id \
         WHERE tl.task_id = $1 ORDER BY tl.position, l.name",
    )
    .bind(task_id)
    .fetch_all(conn)
    .await?;
    Ok(Value::Array(
        labels
            .into_iter()
            .take(MAX_LABELS)
            .map(|(name, color)| json!({"name": name, "color": color}))
            .collect(),
    ))
}

/// Task titles are at least two characters; a template subtask too.
fn subtask_title(title: &str) -> String {
    let title = title.trim();
    if title.chars().count() >= 2 {
        title.to_string()
    } else {
        format!("{title}  ").chars().take(2).collect()
    }
}

/// `POST /v1/pms/tasks/{task_id}/template` (miy): `{name}`. Saves the task in
/// its space: title, description, status, priority, checklist, labels and its
/// direct subtasks that are not archived (each with its description, priority,
/// checklist and labels), at most 50 subtasks and 100 checklist items each.
/// Assignees, dates, attachments and comments are not part of a template.
pub async fn save_task_as_template(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(task_id): Path<String>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    body.forbid_extra(&["name"])?;
    let name = body.req_str("name")?.trim().to_string();
    check_length("name", &name, 1, 140)?;
    let mut tx = pool.begin().await?;
    let probe = load_task(&mut tx, &task_id).await?;
    let team_id: Option<String> =
        sqlx::query_scalar("SELECT team_id FROM pms_task_lists WHERE id = $1")
            .bind(&probe.list_id)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
    let team_id = team_id.ok_or_else(|| ApiError::not_found("pms.task_not_found"))?;
    let (_, role) = lock_space_for_write(&mut tx, &identity.user_id, &team_id).await?;
    require_editor(&role)?;
    let task = ensure_task_readable(&mut tx, &identity.user_id, &task_id).await?;
    let subtasks: Vec<TaskRow> = sqlx::query_as::<_, TaskRow>(&format!(
        "SELECT {TASK_COLUMNS} FROM pms_tasks WHERE parent_id = $1 AND NOT archived \
         ORDER BY board_position, task_number LIMIT {MAX_SUBTASKS}"
    ))
    .bind(&task.id)
    .fetch_all(&mut *tx)
    .await?;
    let mut subtask_items = Vec::new();
    for subtask in &subtasks {
        subtask_items.push(json!({
            "title": subtask_title(&subtask.title),
            "description": subtask.description,
            "description_blocks": subtask.description_blocks,
            "priority": subtask.priority,
            "checklist_items": checklist_of(&mut tx, &subtask.id).await?,
            "labels": labels_of(&mut tx, &subtask.id).await?,
        }));
    }
    let now = utcnow();
    let template = TemplateRow {
        id: new_id(),
        team_id,
        name,
        title: task.title.clone(),
        description: task.description.clone(),
        description_blocks: task.description_blocks.clone(),
        default_status: task.status.clone(),
        default_priority: task.priority.clone(),
        checklist_items: Some(checklist_of(&mut tx, &task.id).await?),
        labels: Some(labels_of(&mut tx, &task.id).await?),
        subtasks: Some(Value::Array(subtask_items)),
        created_by_id: Some(identity.user_id.clone()),
        created_at: now,
        updated_at: now,
    };
    insert_template(&mut tx, &template).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(serialize_template(&template))))
}

// ── Create a task from a template ──────────────────────────────────────

/// The template a new task of `list` names; it must belong to the list's
/// space. Share-locked so it cannot change while the task is made.
pub async fn template_for_list(
    conn: &mut PgConnection,
    template_id: &str,
    list: &TaskListRow,
) -> ApiResult<TemplateRow> {
    let template = sqlx::query_as::<_, TemplateRow>(&format!(
        "SELECT {TEMPLATE_COLUMNS} FROM pms_task_templates WHERE id = $1 FOR SHARE"
    ))
    .bind(template_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| ApiError::not_found("pms.template_not_found"))?;
    if list.team_id.as_deref() != Some(template.team_id.as_str()) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "pms.template_wrong_space",
            "Template does not belong to this list's space.",
        ));
    }
    Ok(template)
}

/// Ids of the list's labels named in a template's label list (ignoring case);
/// names the list does not have are left out.
pub async fn label_ids_for(
    conn: &mut PgConnection,
    list_id: &str,
    labels: &Option<Value>,
) -> ApiResult<Vec<String>> {
    let wanted: Vec<String> = labels
        .as_ref()
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("name").and_then(Value::as_str))
                .map(|name| name.trim().to_lowercase())
                .collect()
        })
        .unwrap_or_default();
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    let mut by_name: HashMap<String, String> = HashMap::new();
    for (id, name) in sqlx::query_as::<_, (String, String)>(
        "SELECT id, name FROM pms_labels WHERE list_id = $1 ORDER BY created_at, id",
    )
    .bind(list_id)
    .fetch_all(&mut *conn)
    .await?
    {
        by_name.entry(name.trim().to_lowercase()).or_insert(id);
    }
    Ok(wanted
        .iter()
        .filter_map(|name| by_name.get(name).cloned())
        .collect())
}

async fn insert_checklist(
    conn: &mut PgConnection,
    task_id: &str,
    items: &Option<Value>,
) -> ApiResult<()> {
    let texts: Vec<&str> = items
        .as_ref()
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .filter(|text| !text.trim().is_empty())
                .collect()
        })
        .unwrap_or_default();
    let now = utcnow();
    for (index, text) in texts.iter().take(MAX_CHECKLIST).enumerate() {
        sqlx::query(
            "INSERT INTO pms_checklist_items (id, task_id, text, completed, sort_order, created_at) \
             VALUES ($1, $2, $3, false, $4, $5)",
        )
        .bind(new_id())
        .bind(task_id)
        .bind(text.trim())
        .bind(index as i32)
        .bind(now)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

async fn link_labels(
    conn: &mut PgConnection,
    task_id: &str,
    label_ids: &[String],
) -> ApiResult<()> {
    for (position, label_id) in label_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO pms_task_labels (id, task_id, label_id, position) VALUES ($1, $2, $3, $4) \
             ON CONFLICT ON CONSTRAINT uq_pms_task_label DO NOTHING",
        )
        .bind(new_id())
        .bind(task_id)
        .bind(label_id)
        .bind(position as i32)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// After the new task is inserted: the template's checklist on it and its
/// subtasks under it, each with its checklist and the labels the list has.
/// Subtasks start in the list's first status, reported by the caller.
pub async fn apply_template_extras(
    conn: &mut PgConnection,
    identity: &Identity,
    actor_name: &str,
    list: &TaskListRow,
    task: &TaskRow,
    template: &TemplateRow,
) -> ApiResult<()> {
    insert_checklist(conn, &task.id, &template.checklist_items).await?;
    let subtasks: Vec<Value> = template
        .subtasks
        .as_ref()
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if subtasks.is_empty() {
        return Ok(());
    }
    let mut definitions = list_workflow(conn, &list.id).await?.definitions().to_vec();
    definitions.sort_by_key(|definition| definition.sort_order);
    let status = initial_status(&definitions);
    let mut position = next_board_position(conn, &list.id, Some(&task.id)).await?;
    let mut number = next_task_number(conn, &list.id).await?;
    for item in subtasks.iter().take(MAX_SUBTASKS) {
        let title = item.get("title").and_then(Value::as_str).unwrap_or("");
        let now = utcnow();
        let subtask = TaskRow {
            id: new_id(),
            list_id: list.id.clone(),
            task_number: number,
            title: subtask_title(title),
            description: item
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            description_blocks: item
                .get("description_blocks")
                .filter(|blocks| blocks.is_array())
                .cloned(),
            status: status.clone(),
            priority: item
                .get("priority")
                .and_then(Value::as_str)
                .filter(|priority| PRIORITIES.contains(priority))
                .unwrap_or("medium")
                .to_string(),
            assignee_id: None,
            reporter_id: identity.user_id.clone(),
            parent_id: Some(task.id.clone()),
            milestone_id: None,
            start_date: None,
            due_date: None,
            completed_date: None,
            board_position: position,
            recurrence_rule: None,
            archived: false,
            created_at: now,
            updated_at: now,
        };
        sqlx::query(&format!(
            "INSERT INTO pms_tasks ({TASK_COLUMNS}) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, \
             $11, $12, $13, $14, $15, $16, $17, $18, $19, $19)"
        ))
        .bind(&subtask.id)
        .bind(&subtask.list_id)
        .bind(subtask.task_number)
        .bind(&subtask.title)
        .bind(&subtask.description)
        .bind(&subtask.description_blocks)
        .bind(&subtask.status)
        .bind(&subtask.priority)
        .bind(&subtask.assignee_id)
        .bind(&subtask.reporter_id)
        .bind(&subtask.parent_id)
        .bind(&subtask.milestone_id)
        .bind(subtask.start_date)
        .bind(subtask.due_date)
        .bind(subtask.completed_date)
        .bind(subtask.board_position)
        .bind(&subtask.recurrence_rule)
        .bind(subtask.archived)
        .bind(now)
        .execute(&mut *conn)
        .await?;
        insert_checklist(conn, &subtask.id, &item.get("checklist_items").cloned()).await?;
        let label_ids = label_ids_for(conn, &list.id, &item.get("labels").cloned()).await?;
        link_labels(conn, &subtask.id, &label_ids).await?;
        log_activity(
            conn,
            &subtask.id,
            Some(&identity.user_id),
            "created",
            &format!(
                "{actor_name} created {}.",
                task_reference(&list.key, subtask.task_number)
            ),
            None,
            None,
            None,
        )
        .await?;
        position += 1;
        number += 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_content_is_checked_and_trimmed() {
        assert_eq!(
            checklist_content(&json!([{"text": " a ", "completed": true}])).unwrap(),
            json!([{"text": "a"}])
        );
        assert!(checklist_content(&json!([{"text": ""}])).is_err());
        assert!(checklist_content(&json!(["a"])).is_err());
        assert_eq!(
            labels_content(&json!([{"name": "QA"}, {"name": "qa", "color": "#fff"}])).unwrap(),
            json!([{"name": "QA", "color": "#1f2d38"}])
        );
        let subtasks = subtasks_content(&json!([{"title": "Write", "priority": "high",
            "checklist_items": [{"text": "x"}]}]))
        .unwrap();
        assert_eq!(subtasks[0]["priority"], "high");
        assert_eq!(subtasks[0]["labels"], json!([]));
        assert!(subtasks_content(&json!([{"title": "W"}])).is_err());
        assert!(subtasks_content(&json!([{"title": "Write", "priority": "urgent"}])).is_err());
        let many: Vec<Value> = (0..51).map(|_| json!({"title": "Step"})).collect();
        assert!(subtasks_content(&Value::Array(many)).is_err());
    }

    #[test]
    fn subtask_titles_meet_the_task_minimum() {
        assert_eq!(subtask_title("Plan"), "Plan");
        assert_eq!(subtask_title("A").chars().count(), 2);
    }
}
