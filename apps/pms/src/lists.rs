//! Task lists, folders, milestones, labels and the dashboard summary: AI-DO
//! service.py (list_task_lists, create_task_list), board_configuration.py,
//! router.py (milestones, labels, folders, dashboard) and dashboard_summary.py.
use std::collections::HashMap;

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{Duration, NaiveDate};
use miy_app_sdk::Identity;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};

use crate::access::{
    self, ensure_list_editor, ensure_list_manager, ensure_list_member, ensure_list_owner,
    ensure_space_access, ensure_space_editor, ensure_space_manager, ensure_task_list_active,
};
use crate::error::{ApiError, ApiResult, check_length};
use crate::input::{Body, Params, check_list_length, check_opt_length};
use crate::models::{
    TASK_LIST_COLUMNS, TaskListRow, iso_date, iso_datetime, new_id, paginate, task_reference,
    today_utc, utcnow,
};
use crate::statuses::{create_default_task_list_statuses, ensure_space_statuses};
use crate::summary::{self, ListCounts};
use crate::tasks;
use crate::users;
use crate::workflow::{
    ListWorkflow, PRIORITY_LABELS, auto_key_from_name, calculate_progress,
    is_overdue_exempt_status, priority_label, status_label,
};

pub async fn create_default_labels(conn: &mut PgConnection, list_id: &str) -> ApiResult<()> {
    let now = utcnow();
    for (name, color) in [
        ("blocked", "#b45309"),
        ("customer", "#1d4ed8"),
        ("qa", "#0f766e"),
    ] {
        sqlx::query(
            "INSERT INTO pms_labels (id, list_id, name, color, created_at) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(new_id())
        .bind(list_id)
        .bind(name)
        .bind(color)
        .bind(now)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// `_unique_key`: the name's key, then KEY1, KEY2, … until unused.
pub async fn unique_key(conn: &mut PgConnection, base_name: &str) -> ApiResult<String> {
    let base = auto_key_from_name(base_name);
    let mut resolved = base.clone();
    let mut counter = 1;
    loop {
        let taken: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pms_task_lists WHERE lower(key) = lower($1))",
        )
        .bind(&resolved)
        .fetch_one(&mut *conn)
        .await?;
        if !taken {
            return Ok(resolved);
        }
        resolved = format!("{base}{counter}");
        counter += 1;
    }
}

pub async fn validate_folder_membership(
    conn: &mut PgConnection,
    team_id: Option<&str>,
    folder_id: Option<&str>,
) -> ApiResult<()> {
    let Some(folder_id) = folder_id else {
        return Ok(());
    };
    let folder_team: Option<Option<String>> =
        sqlx::query_scalar("SELECT team_id FROM pms_folders WHERE id = $1")
            .bind(folder_id)
            .fetch_optional(conn)
            .await?;
    match folder_team {
        None => Err(ApiError::not_found("pms.folder_not_found")),
        Some(folder_team) if folder_team.as_deref() != team_id => {
            Err(ApiError::bad_request("pms.folder_same_space_required"))
        }
        Some(_) => Ok(()),
    }
}

/// `GET /v1/pms/lists` and `GET /v1/pms/spaces/{space_id}/lists`.
async fn list_task_lists_inner(
    pool: &PgPool,
    identity: &Identity,
    params: &Params,
    team_id: Option<String>,
) -> ApiResult<Json<Value>> {
    let page = params.int("page", 1, 1, None)?;
    let page_size = params.int("page_size", 20, 1, Some(100))?;
    let sort_by = params.string("sort_by", "updated_at");
    let sort_dir = params.choice("sort_dir", "desc", &["asc", "desc"])?;
    let q = params.string("q", "");
    let archived = params.opt_bool("archived")?;
    let mut conn = pool.acquire().await?;
    if let Some(team_id) = &team_id {
        ensure_space_access(&mut conn, &identity.user_id, team_id).await?;
    }
    let space_ids = access::accessible_space_ids(&mut conn, &identity.user_id).await?;
    let mut lists: Vec<TaskListRow> = sqlx::query_as::<_, TaskListRow>(&format!(
        "SELECT {TASK_LIST_COLUMNS} FROM pms_task_lists WHERE team_id = ANY($1)"
    ))
    .bind(&space_ids)
    .fetch_all(&mut *conn)
    .await?;
    let q_lower = q.trim().to_lowercase();
    lists.retain(|list| {
        archived.is_none_or(|archived| list.archived == archived)
            && team_id
                .as_ref()
                .is_none_or(|team| list.team_id.as_ref() == Some(team))
            && (q_lower.is_empty()
                || list.name.to_lowercase().contains(&q_lower)
                || list.key.to_lowercase().contains(&q_lower)
                || list.description.to_lowercase().contains(&q_lower))
    });
    let ids: Vec<String> = lists.iter().map(|list| list.id.clone()).collect();
    let infos = summary::load_lists(&mut conn, &ids).await?;
    let counts = summary::load_list_counts(&mut conn, &ids).await?;
    let empty = ListCounts::default();
    let reverse = sort_dir == "desc";
    match sort_by.as_str() {
        "name" => lists.sort_by_key(|list| list.name.to_lowercase()),
        "key" => lists.sort_by_key(|list| list.key.to_lowercase()),
        "progress" => lists.sort_by(|a, b| {
            let progress = |list: &TaskListRow| {
                calculate_progress(
                    counts
                        .get(&list.id)
                        .unwrap_or(&empty)
                        .tasks
                        .iter()
                        .map(|(status, archived, _)| (status.as_str(), *archived)),
                    None,
                )
            };
            progress(a).total_cmp(&progress(b))
        }),
        "sort_order" => lists.sort_by(|a, b| {
            (
                a.folder_id.clone().unwrap_or_default(),
                a.sort_order,
                a.name.to_lowercase(),
            )
                .cmp(&(
                    b.folder_id.clone().unwrap_or_default(),
                    b.sort_order,
                    b.name.to_lowercase(),
                ))
        }),
        _ => lists.sort_by_key(|list| list.updated_at),
    }
    // Python sorts stably with reverse=True; "sort_order" ignores the direction.
    if reverse && sort_by != "sort_order" {
        stable_reverse(&mut lists, |list| match sort_by.as_str() {
            "name" => SortKey::Text(list.name.to_lowercase()),
            "key" => SortKey::Text(list.key.to_lowercase()),
            "progress" => SortKey::Float(calculate_progress(
                counts
                    .get(&list.id)
                    .unwrap_or(&empty)
                    .tasks
                    .iter()
                    .map(|(status, archived, _)| (status.as_str(), *archived)),
                None,
            )),
            _ => SortKey::Time(list.updated_at),
        });
    }
    let team_ids: Vec<String> = lists
        .iter()
        .filter_map(|list| list.team_id.clone())
        .collect();
    let teams: HashMap<String, (String, String, i64)> = sqlx::query_as::<_, (String, String, String, i64)>(
        "SELECT s.id, s.name, s.key, (SELECT count(*) FROM pms_space_members m WHERE m.space_id = s.id) \
         FROM pms_spaces s WHERE s.id = ANY($1)",
    )
    .bind(&team_ids)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|(id, name, key, count)| (id, (name, key, count)))
    .collect();
    let roles: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT space_id, role FROM pms_space_members WHERE user_id = $1 AND space_id = ANY($2)",
    )
    .bind(&identity.user_id)
    .bind(&team_ids)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .collect();
    let folder_ids: Vec<String> = lists
        .iter()
        .filter_map(|list| list.folder_id.clone())
        .collect();
    let folders: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT id, name FROM pms_folders WHERE id = ANY($1)",
    )
    .bind(&folder_ids)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .collect();
    let default_workflow = ListWorkflow::default();
    let items: Vec<Value> = lists
        .iter()
        .map(|list| {
            let team = list.team_id.as_ref().and_then(|id| teams.get(id));
            let role = list
                .team_id
                .as_ref()
                .and_then(|id| roles.get(id))
                .and_then(|role| access::normalize_role(role))
                .unwrap_or_else(|| "viewer".into());
            summary::serialize_task_list(
                list,
                &role,
                team.map(|(name, _, _)| name.as_str()),
                team.map(|(_, key, _)| key.as_str()),
                list.folder_id
                    .as_ref()
                    .and_then(|id| folders.get(id))
                    .map(String::as_str),
                team.map(|(_, _, count)| *count).unwrap_or(0),
                infos
                    .get(&list.id)
                    .map(|info| &info.workflow)
                    .unwrap_or(&default_workflow),
                counts.get(&list.id).unwrap_or(&empty),
            )
        })
        .collect();
    let (page_items, total) = paginate(&items, page, page_size);
    Ok(Json(json!({
        "items": page_items, "total": total, "page": page, "page_size": page_size,
    })))
}

#[derive(PartialEq, PartialOrd)]
pub(crate) enum SortKey {
    Text(String),
    Float(f64),
    Time(chrono::NaiveDateTime),
}

/// Python's `sort(key, reverse=True)` keeps equal keys in their original
/// order. After an ascending stable sort, reverse each run of equal keys back
/// and then the whole list.
pub(crate) fn stable_reverse<T>(items: &mut [T], key: impl Fn(&T) -> SortKey) {
    let mut start = 0;
    while start < items.len() {
        let mut end = start + 1;
        while end < items.len() && key(&items[end]) == key(&items[start]) {
            end += 1;
        }
        items[start..end].reverse();
        start = end;
    }
    items.reverse();
}

pub async fn list_task_lists(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let team_id = params.opt_string("team_id");
    list_task_lists_inner(&pool, &identity, &params, team_id).await
}

pub async fn list_space_lists(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    params: Params,
) -> ApiResult<Json<Value>> {
    list_task_lists_inner(&pool, &identity, &params, Some(space_id)).await
}

/// `POST /v1/pms/lists`: a new list inherits its space's workflow.
pub async fn create_task_list(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    let key = body.opt_str("key")?;
    if let Some(key) = &key {
        check_length("key", key, 2, 24)?;
        if !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(ApiError::validation(
                "key: String should match pattern '^[A-Za-z0-9_-]+$'",
            ));
        }
    }
    let name = body.req_str("name")?;
    check_length("name", &name, 2, 140)?;
    let description = body.str_or("description", "")?;
    check_length("description", &description, 0, 4000)?;
    let team_id = body.opt_str("team_id")?;
    let folder_id = body.opt_str("folder_id")?;
    let mut tx = pool.begin().await?;
    let resolved_key = match &key {
        Some(key) => key.to_uppercase(),
        None => unique_key(&mut tx, &name).await?,
    };
    // AI-DO fell back to the request's app space; miy has no such context.
    let Some(team_id) = team_id else {
        return Err(ApiError::conflict("pms.task_list_space_missing"));
    };
    let (_, role) = ensure_space_editor(&mut tx, &identity.user_id, &team_id).await?;
    validate_folder_membership(&mut tx, Some(&team_id), folder_id.as_deref()).await?;
    let now = utcnow();
    let list = TaskListRow {
        id: new_id(),
        key: resolved_key,
        name: name.trim().to_string(),
        description: description.trim().to_string(),
        status: "active".into(),
        status_mode: "inherit".into(),
        archived: false,
        team_id: Some(team_id.clone()),
        folder_id,
        sort_order: 0,
        created_by_id: identity.user_id.clone(),
        created_at: now,
        updated_at: now,
    };
    let inserted = sqlx::query(
        "INSERT INTO pms_task_lists (id, key, name, description, status, status_mode, archived, team_id, \
         folder_id, sort_order, created_by_id, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, 'active', 'inherit', false, $5, $6, 0, $7, $8, $8)",
    )
    .bind(&list.id)
    .bind(&list.key)
    .bind(&list.name)
    .bind(&list.description)
    .bind(&list.team_id)
    .bind(&list.folder_id)
    .bind(&list.created_by_id)
    .bind(now)
    .execute(&mut *tx)
    .await;
    if let Err(error) = inserted {
        if crate::error::is_unique_violation(&error) {
            // AI-DO let the unique index fail; answer a clear conflict instead.
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "pms.task_list_key_exists",
                "A task list with this key already exists.",
            ));
        }
        return Err(error.into());
    }
    ensure_space_statuses(&mut tx, &team_id).await?;
    create_default_labels(&mut tx, &list.id).await?;
    let item = summary::task_list_item(&mut tx, &list, &role).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(item)))
}

/// `GET /v1/pms/lists/{list_id}`.
pub async fn get_task_list(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let mut conn = pool.acquire().await?;
    let (list, role) = ensure_list_member(&mut conn, &identity.user_id, &list_id).await?;
    Ok(Json(
        summary::task_list_item(&mut conn, &list, &role).await?,
    ))
}

/// `PATCH /v1/pms/lists/{list_id}`: folder/sort order alone is an editor's
/// reorder; anything else needs an owner or admin, and an archived list only
/// accepts (un)archiving.
pub async fn update_task_list(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
    body: Body,
) -> ApiResult<Json<Value>> {
    let known = [
        "name",
        "description",
        "status",
        "archived",
        "folder_id",
        "sort_order",
    ];
    let provided: Vec<&str> = known
        .iter()
        .copied()
        .filter(|field| body.has(field))
        .collect();
    let name = body.opt_str("name")?;
    check_opt_length("name", &name, 2, 140)?;
    let description = body.opt_str("description")?;
    check_opt_length("description", &description, 0, 4000)?;
    let status = body.opt_choice("status", &["planned", "active", "on_hold", "done"])?;
    let archived = body.opt_bool("archived")?;
    let folder_id = body.opt_str("folder_id")?;
    let sort_order = body.opt_int("sort_order")?;
    let reorder_only = !provided.is_empty()
        && provided
            .iter()
            .all(|field| matches!(*field, "folder_id" | "sort_order"));
    let mut tx = pool.begin().await?;
    tasks::lock_task_list_order(&mut tx, &list_id).await?;
    let (mut list, role) = if reorder_only {
        ensure_list_editor(&mut tx, &identity.user_id, &list_id).await?
    } else {
        let (list, role) = ensure_list_manager(&mut tx, &identity.user_id, &list_id).await?;
        if provided.iter().any(|field| *field != "archived") {
            ensure_task_list_active(&list)?;
        }
        (list, role)
    };
    let mut changed = false;
    if provided.contains(&"folder_id") {
        validate_folder_membership(&mut tx, list.team_id.as_deref(), folder_id.as_deref()).await?;
        changed |= list.folder_id != folder_id;
        list.folder_id = folder_id;
    }
    if let Some(order) = sort_order {
        changed |= list.sort_order != order as i32;
        list.sort_order = order as i32;
    }
    if let Some(name) = name {
        let name = name.trim().to_string();
        changed |= list.name != name;
        list.name = name;
    }
    if let Some(description) = description {
        let description = description.trim().to_string();
        changed |= list.description != description;
        list.description = description;
    }
    if let Some(status) = status {
        changed |= list.status != status;
        list.status = status;
    }
    if let Some(archived) = archived {
        changed |= list.archived != archived;
        list.archived = archived;
    }
    if changed {
        list.updated_at = utcnow();
        sqlx::query(
            "UPDATE pms_task_lists SET name = $2, description = $3, status = $4, archived = $5, \
             folder_id = $6, sort_order = $7, updated_at = $8 WHERE id = $1",
        )
        .bind(&list.id)
        .bind(&list.name)
        .bind(&list.description)
        .bind(&list.status)
        .bind(list.archived)
        .bind(&list.folder_id)
        .bind(list.sort_order)
        .bind(list.updated_at)
        .execute(&mut *tx)
        .await?;
    }
    let item = summary::task_list_item(&mut tx, &list, &role).await?;
    tx.commit().await?;
    Ok(Json(item))
}

/// `DELETE /v1/pms/lists/{list_id}`: only an archived list, with its tasks.
pub async fn delete_task_list(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = pool.begin().await?;
    tasks::lock_task_list_order(&mut tx, &list_id).await?;
    let (list, _) = ensure_list_manager(&mut tx, &identity.user_id, &list_id).await?;
    if !list.archived {
        return Err(ApiError::conflict("pms.task_list_archive_before_delete"));
    }
    let task_ids: Vec<String> = sqlx::query_scalar("SELECT id FROM pms_tasks WHERE list_id = $1")
        .bind(&list.id)
        .fetch_all(&mut *tx)
        .await?;
    let deleted = tasks::lock_task_tree(&mut tx, &task_ids).await?;
    ensure_list_manager(&mut tx, &identity.user_id, &list_id).await?;
    tasks::delete_task_rows(&mut tx, &deleted).await?;
    sqlx::query("DELETE FROM pms_task_lists WHERE id = $1")
        .bind(&list.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tasks::after_tasks_deleted(&pool, &identity, &deleted).await;
    Ok(StatusCode::NO_CONTENT)
}

/// `PATCH /v1/pms/spaces/{space_id}/lists/reorder`.
pub async fn reorder_space_lists(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    body: Body,
) -> ApiResult<StatusCode> {
    let items = match body.0.get("items") {
        Some(Value::Array(items)) => items.clone(),
        Some(_) => return Err(ApiError::validation("items: Input should be a valid list")),
        None => return Err(ApiError::validation("items: Field required")),
    };
    check_list_length("items", items.len(), 1, 200)?;
    let mut parsed: Vec<(String, Option<String>, i64)> = Vec::new();
    for item in items {
        let Value::Object(map) = item else {
            return Err(ApiError::validation(
                "items: Input should be a valid dictionary",
            ));
        };
        let item = Body(map);
        let id = item.req_str("id")?;
        let folder_id = item.opt_str("folder_id")?;
        let order = item.opt_int("sort_order")?.unwrap_or(0);
        crate::error::check_range("sort_order", order, 0, None)?;
        parsed.push((id, folder_id, order));
    }
    let mut tx = pool.begin().await?;
    ensure_space_editor(&mut tx, &identity.user_id, &space_id).await?;
    let mut ids: Vec<String> = parsed.iter().map(|(id, _, _)| id.clone()).collect();
    ids.sort();
    ids.dedup();
    if ids.len() != parsed.len() {
        return Err(ApiError::bad_request("pms.duplicate_task_list_ids"));
    }
    let lists: Vec<TaskListRow> = sqlx::query_as::<_, TaskListRow>(&format!(
        "SELECT {TASK_LIST_COLUMNS} FROM pms_task_lists WHERE team_id = $1 AND id = ANY($2)"
    ))
    .bind(&space_id)
    .bind(&ids)
    .fetch_all(&mut *tx)
    .await?;
    if lists.len() != ids.len() {
        return Err(ApiError::not_found("pms.task_list_not_found"));
    }
    for list in &lists {
        ensure_task_list_active(list)?;
    }
    for (id, folder_id, order) in parsed {
        validate_folder_membership(&mut tx, Some(&space_id), folder_id.as_deref()).await?;
        sqlx::query("UPDATE pms_task_lists SET folder_id = $2, sort_order = $3 WHERE id = $1")
            .bind(&id)
            .bind(&folder_id)
            .bind(order as i32)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Labels ─────────────────────────────────────────────────────────────

/// `GET /v1/pms/lists/{list_id}/labels`.
pub async fn list_labels(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let page = params.int("page", 1, 1, None)?;
    let page_size = params.int("page_size", 100, 1, Some(200))?;
    let mut conn = pool.acquire().await?;
    ensure_list_member(&mut conn, &identity.user_id, &list_id).await?;
    let labels: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, name, color FROM pms_labels WHERE list_id = $1 ORDER BY name")
            .bind(&list_id)
            .fetch_all(&mut *conn)
            .await?;
    let items: Vec<Value> = labels
        .into_iter()
        .map(|(id, name, color)| json!({"id": id, "name": name, "color": color}))
        .collect();
    let (page_items, total) = paginate(&items, page, page_size);
    Ok(Json(json!({
        "items": page_items, "total": total, "page": page, "page_size": page_size,
    })))
}

async fn label_name_taken(
    conn: &mut PgConnection,
    list_id: &str,
    name: &str,
    except: Option<&str>,
) -> ApiResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pms_labels WHERE list_id = $1 AND lower(name) = lower($2) \
         AND ($3::text IS NULL OR id <> $3))",
    )
    .bind(list_id)
    .bind(name)
    .bind(except)
    .fetch_one(conn)
    .await?)
}

/// `POST /v1/pms/lists/{list_id}/labels`.
pub async fn create_label(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    let name = body.req_str("name")?;
    check_length("name", &name, 1, 48)?;
    let color = body.str_or("color", "#1f2d38")?;
    check_length("color", &color, 0, 24)?;
    let mut tx = pool.begin().await?;
    ensure_list_owner(&mut tx, &identity.user_id, &list_id).await?;
    let name = name.trim().to_string();
    if label_name_taken(&mut tx, &list_id, &name, None).await? {
        return Err(ApiError::conflict("pms.label_name_exists"));
    }
    let id = new_id();
    sqlx::query(
        "INSERT INTO pms_labels (id, list_id, name, color, created_at) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&id)
    .bind(&list_id)
    .bind(&name)
    .bind(&color)
    .bind(utcnow())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"id": id, "name": name, "color": color})),
    ))
}

/// `PATCH /v1/pms/labels/{label_id}`.
pub async fn update_label(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(label_id): Path<String>,
    body: Body,
) -> ApiResult<Json<Value>> {
    let name = body.opt_str("name")?;
    check_opt_length("name", &name, 1, 48)?;
    let color = body.opt_str("color")?;
    check_opt_length("color", &color, 0, 24)?;
    let mut tx = pool.begin().await?;
    let (list_id, mut current_name, mut current_color): (String, String, String) =
        sqlx::query_as("SELECT list_id, name, color FROM pms_labels WHERE id = $1 FOR UPDATE")
            .bind(&label_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ApiError::not_found("pms.label_not_found"))?;
    ensure_list_owner(&mut tx, &identity.user_id, &list_id).await?;
    if let Some(name) = name {
        let name = name.trim().to_string();
        if label_name_taken(&mut tx, &list_id, &name, Some(&label_id)).await? {
            return Err(ApiError::conflict("pms.label_name_exists"));
        }
        current_name = name;
    }
    if let Some(color) = color {
        current_color = color;
    }
    sqlx::query("UPDATE pms_labels SET name = $2, color = $3 WHERE id = $1")
        .bind(&label_id)
        .bind(&current_name)
        .bind(&current_color)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"id": label_id, "name": current_name, "color": current_color}),
    ))
}

/// `DELETE /v1/pms/labels/{label_id}`.
pub async fn delete_label(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(label_id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = pool.begin().await?;
    let list_id: String = sqlx::query_scalar("SELECT list_id FROM pms_labels WHERE id = $1")
        .bind(&label_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ApiError::not_found("pms.label_not_found"))?;
    ensure_list_owner(&mut tx, &identity.user_id, &list_id).await?;
    sqlx::query("DELETE FROM pms_labels WHERE id = $1")
        .bind(&label_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Folders ────────────────────────────────────────────────────────────

fn folder_item(
    id: &str,
    team_id: &Option<String>,
    name: &str,
    sort_order: i32,
    count: i64,
) -> Value {
    json!({"id": id, "team_id": team_id, "name": name, "sort_order": sort_order, "list_count": count})
}

/// `GET /v1/pms/folders`.
pub async fn list_folders(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let team_id = params.opt_string("team_id").filter(|id| !id.is_empty());
    let mut conn = pool.acquire().await?;
    let team_ids = match &team_id {
        Some(team_id) => {
            ensure_space_access(&mut conn, &identity.user_id, team_id).await?;
            vec![team_id.clone()]
        }
        None => access::accessible_space_ids(&mut conn, &identity.user_id).await?,
    };
    let rows: Vec<(String, Option<String>, String, i32, i64)> = sqlx::query_as(
        "SELECT f.id, f.team_id, f.name, f.sort_order, \
         (SELECT count(*) FROM pms_task_lists l WHERE l.folder_id = f.id) \
         FROM pms_folders f WHERE f.team_id = ANY($1) ORDER BY f.sort_order, f.created_at",
    )
    .bind(&team_ids)
    .fetch_all(&mut *conn)
    .await?;
    Ok(Json(json!({
        "items": rows.iter().map(|(id, team, name, order, count)| folder_item(id, team, name, *order, *count)).collect::<Vec<_>>(),
    })))
}

/// `POST /v1/pms/folders`: also creates a default list "List" in the folder.
pub async fn create_folder(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    let name = body.req_str("name")?;
    check_length("name", &name, 1, 140)?;
    let team_id = body.opt_str("team_id")?;
    let sort_order = body.opt_int("sort_order")?.unwrap_or(0);
    let Some(team_id) = team_id else {
        return Err(ApiError::conflict("pms.task_list_space_missing"));
    };
    let mut tx = pool.begin().await?;
    ensure_space_editor(&mut tx, &identity.user_id, &team_id).await?;
    let folder_id = new_id();
    let now = utcnow();
    let name = name.trim().to_string();
    sqlx::query(
        "INSERT INTO pms_folders (id, team_id, name, sort_order, created_at) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(&folder_id)
    .bind(&team_id)
    .bind(&name)
    .bind(sort_order as i32)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    let key = unique_key(&mut tx, "List").await?;
    let list_id = new_id();
    sqlx::query(
        "INSERT INTO pms_task_lists (id, key, name, description, status, status_mode, archived, team_id, \
         folder_id, sort_order, created_by_id, created_at, updated_at) \
         VALUES ($1, $2, 'List', '', 'active', 'custom', false, $3, $4, 0, $5, $6, $6)",
    )
    .bind(&list_id)
    .bind(&key)
    .bind(&team_id)
    .bind(&folder_id)
    .bind(&identity.user_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    create_default_task_list_statuses(&mut tx, &list_id).await?;
    create_default_labels(&mut tx, &list_id).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(folder_item(
            &folder_id,
            &Some(team_id),
            &name,
            sort_order as i32,
            1,
        )),
    ))
}

/// `PATCH /v1/pms/folders/{folder_id}`.
pub async fn update_folder(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(folder_id): Path<String>,
    body: Body,
) -> ApiResult<Json<Value>> {
    let name = body.opt_str("name")?;
    check_opt_length("name", &name, 1, 140)?;
    let sort_order = body.opt_int("sort_order")?;
    let mut tx = pool.begin().await?;
    let (team_id, mut current_name, mut current_order): (Option<String>, String, i32) =
        sqlx::query_as(
            "SELECT team_id, name, sort_order FROM pms_folders WHERE id = $1 FOR UPDATE",
        )
        .bind(&folder_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ApiError::not_found("pms.folder_not_found"))?;
    let Some(team) = &team_id else {
        return Err(ApiError::conflict("pms.folder_space_missing"));
    };
    ensure_space_manager(&mut tx, &identity.user_id, team).await?;
    if let Some(name) = name {
        current_name = name.trim().to_string();
    }
    if let Some(order) = sort_order {
        current_order = order as i32;
    }
    sqlx::query("UPDATE pms_folders SET name = $2, sort_order = $3 WHERE id = $1")
        .bind(&folder_id)
        .bind(&current_name)
        .bind(current_order)
        .execute(&mut *tx)
        .await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM pms_task_lists WHERE folder_id = $1")
        .bind(&folder_id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(folder_item(
        &folder_id,
        &team_id,
        &current_name,
        current_order,
        count,
    )))
}

/// `DELETE /v1/pms/folders/{folder_id}`: its lists stay, outside any folder.
pub async fn delete_folder(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(folder_id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = pool.begin().await?;
    let team_id: Option<String> =
        sqlx::query_scalar::<_, Option<String>>("SELECT team_id FROM pms_folders WHERE id = $1")
            .bind(&folder_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ApiError::not_found("pms.folder_not_found"))?;
    let Some(team) = &team_id else {
        return Err(ApiError::conflict("pms.folder_space_missing"));
    };
    ensure_space_manager(&mut tx, &identity.user_id, team).await?;
    let lists: Vec<TaskListRow> = sqlx::query_as::<_, TaskListRow>(&format!(
        "SELECT {TASK_LIST_COLUMNS} FROM pms_task_lists WHERE folder_id = $1"
    ))
    .bind(&folder_id)
    .fetch_all(&mut *tx)
    .await?;
    for list in &lists {
        ensure_task_list_active(list)?;
    }
    sqlx::query("UPDATE pms_task_lists SET folder_id = NULL WHERE folder_id = $1")
        .bind(&folder_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM pms_folders WHERE id = $1")
        .bind(&folder_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Dashboard ──────────────────────────────────────────────────────────

/// `GET /v1/pms/dashboard/summary`: AI-DO dashboard_summary.py over the
/// caller's active (unarchived) lists.
pub async fn dashboard_summary(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let list_filter = params.opt_string("list_id").filter(|id| !id.is_empty());
    let mut conn = pool.acquire().await?;
    let space_ids = access::accessible_space_ids(&mut conn, &identity.user_id).await?;
    let mut lists: Vec<TaskListRow> = sqlx::query_as::<_, TaskListRow>(&format!(
        "SELECT {TASK_LIST_COLUMNS} FROM pms_task_lists WHERE team_id = ANY($1) AND NOT archived ORDER BY created_at"
    ))
    .bind(&space_ids)
    .fetch_all(&mut *conn)
    .await?;
    if let Some(list_id) = &list_filter {
        lists.retain(|list| &list.id == list_id);
    }
    let ids: Vec<String> = lists.iter().map(|list| list.id.clone()).collect();
    let infos = summary::load_lists(&mut conn, &ids).await?;
    #[allow(clippy::type_complexity)]
    let tasks: Vec<(String, String, String, bool, Option<NaiveDate>, Option<String>)> = sqlx::query_as(
        "SELECT list_id, status, priority, archived, due_date, assignee_id FROM pms_tasks WHERE list_id = ANY($1)",
    )
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    let milestone_dues: Vec<Option<NaiveDate>> = sqlx::query_scalar(
        "SELECT due_date FROM pms_milestones WHERE list_id = ANY($1) OR (list_id IS NULL \
             AND team_id IN (SELECT team_id FROM pms_task_lists WHERE id = ANY($1)))",
    )
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    let today = today_utc();
    let default_workflow = ListWorkflow::default();
    let workflow = |list_id: &str| {
        infos
            .get(list_id)
            .map(|info| &info.workflow)
            .unwrap_or(&default_workflow)
    };
    let active: Vec<&(
        String,
        String,
        String,
        bool,
        Option<NaiveDate>,
        Option<String>,
    )> = tasks
        .iter()
        .filter(|(list_id, status, _, archived, _, _)| {
            !archived && !is_overdue_exempt_status(status, Some(workflow(list_id)))
        })
        .collect();
    let overdue = active
        .iter()
        .filter(|(_, _, _, _, due, _)| due.is_some_and(|due| due < today))
        .count();
    let my = active
        .iter()
        .filter(|(_, _, _, _, _, assignee)| assignee.as_deref() == Some(identity.user_id.as_str()))
        .count();
    let due_by = today + Duration::days(14);
    let milestone_due_soon = milestone_dues
        .iter()
        .filter(|due| due.is_some_and(|due| due <= due_by))
        .count();
    let unarchived: Vec<_> = tasks.iter().filter(|task| !task.3).collect();
    let mut status_labels: std::collections::BTreeMap<String, String> = Default::default();
    for (list_id, status, ..) in &unarchived {
        status_labels.insert(
            status.clone(),
            status_label(status, Some(workflow(list_id))),
        );
    }
    let status_counts: Vec<Value> = status_labels
        .iter()
        .map(|(status, label)| {
            json!({
                "status": status,
                "label": label,
                "count": unarchived.iter().filter(|task| &task.1 == status).count(),
            })
        })
        .collect();
    let priority_counts: Vec<Value> = PRIORITY_LABELS
        .iter()
        .map(|(priority, _)| {
            json!({
                "priority": priority,
                "label": priority_label(priority),
                "count": unarchived.iter().filter(|task| task.2 == *priority).count(),
            })
        })
        .collect();
    let list_items: Vec<Value> = lists
        .iter()
        .map(|list| {
            let flow = workflow(&list.id);
            let scope: Vec<_> = unarchived.iter().filter(|task| task.0 == list.id).collect();
            let open: Vec<_> = scope
                .iter()
                .filter(|task| !is_overdue_exempt_status(&task.1, Some(flow)))
                .collect();
            let mut dues: Vec<NaiveDate> = open.iter().filter_map(|task| task.4).collect();
            dues.sort();
            json!({
                "list_id": list.id,
                "key": list.key,
                "name": list.name,
                "progress": calculate_progress(scope.iter().map(|task| (task.1.as_str(), task.3)), Some(flow)),
                "open_task_count": open.len(),
                "overdue_task_count": open.iter().filter(|task| task.4.is_some_and(|due| due < today)).count(),
                "next_due_date": dues.first().map(iso_date),
            })
        })
        .collect();
    #[allow(clippy::type_complexity)]
    let logs: Vec<(String, String, String, Option<String>, chrono::NaiveDateTime, String, i32)> = sqlx::query_as(
        "SELECT a.id, a.task_id, a.message, a.actor_id, a.created_at, l.key, t.task_number \
         FROM pms_task_activity_logs a JOIN pms_tasks t ON t.id = a.task_id JOIN pms_task_lists l ON l.id = t.list_id \
         WHERE t.list_id = ANY($1) AND (a.field_name IS NULL OR a.field_name <> 'description_blocks') \
         ORDER BY a.created_at DESC LIMIT 8",
    )
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    drop(conn);
    let actor_ids: Vec<String> = logs.iter().filter_map(|log| log.3.clone()).collect();
    let names = users::names(&identity, &actor_ids).await;
    let recent: Vec<Value> = logs
        .iter()
        .map(|(id, task_id, message, actor_id, created_at, key, number)| {
            json!({
                "id": id,
                "task_id": task_id,
                "task_reference": task_reference(key, *number),
                "message": message,
                "actor_name": actor_id.as_ref().map(|actor| names.get(actor).cloned().unwrap_or_else(|| actor.clone())),
                "created_at": iso_datetime(created_at),
            })
        })
        .collect();
    Ok(Json(json!({
        "list_count": lists.len(),
        "active_task_count": active.len(),
        "overdue_task_count": overdue,
        "my_task_count": my,
        "milestone_due_soon_count": milestone_due_soon,
        "status_counts": status_counts,
        "priority_counts": priority_counts,
        "lists": list_items,
        "recent_activity": recent,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_reverse_sort_keeps_ties_in_order() {
        let mut items = vec![(1, 'a'), (2, 'b'), (1, 'c'), (3, 'd')];
        items.sort_by_key(|item| item.0);
        stable_reverse(&mut items, |item| SortKey::Float(item.0 as f64));
        assert_eq!(items, vec![(3, 'd'), (2, 'b'), (1, 'a'), (1, 'c')]);
    }
}
