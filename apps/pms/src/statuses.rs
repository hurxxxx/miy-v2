//! Workflow statuses of spaces and task lists: AI-DO status_router.py and
//! status_lifecycle.py.
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use miy_app_sdk::Identity;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};

use crate::access::{
    ensure_list_member, ensure_list_owner, ensure_space_access, ensure_space_manager,
};
use crate::error::{ApiError, ApiResult, check_choice};
use crate::input::{Body, check_opt_length};
use crate::models::{TaskListRow, new_id, utcnow};
use crate::tasks::lock_task_list_order;
use crate::workflow::{
    DEFAULT_TASK_LIST_STATUSES, STATUS_CATEGORIES, StatusDef, incompatible_statuses_for_inherit,
    normalize_status_category, status_slug, violates_single_closed_status_rule,
};

const STATUS_COLUMNS: &str = "id, slug, name, color, category, sort_order";

pub fn serialize_status(status: &StatusDef) -> Value {
    json!({
        "id": status.id,
        "slug": status.slug,
        "name": status.name,
        "color": status.color,
        "category": normalize_status_category(&status.category),
        "sort_order": status.sort_order,
    })
}

/// `create_default_space_statuses` (idempotent: on conflict do nothing).
pub async fn create_default_space_statuses(
    conn: &mut PgConnection,
    team_id: &str,
) -> ApiResult<()> {
    let now = utcnow();
    for (slug, name, color, category, order) in DEFAULT_TASK_LIST_STATUSES {
        sqlx::query(
            "INSERT INTO pms_space_statuses (id, team_id, slug, name, color, category, sort_order, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT ON CONSTRAINT uq_pms_space_status_slug DO NOTHING",
        )
        .bind(new_id())
        .bind(team_id)
        .bind(slug)
        .bind(name)
        .bind(color)
        .bind(category)
        .bind(order)
        .bind(now)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

pub async fn create_default_task_list_statuses(
    conn: &mut PgConnection,
    list_id: &str,
) -> ApiResult<()> {
    let now = utcnow();
    for (slug, name, color, category, order) in DEFAULT_TASK_LIST_STATUSES {
        sqlx::query(
            "INSERT INTO pms_task_list_statuses (id, list_id, slug, name, color, category, sort_order, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(new_id())
        .bind(list_id)
        .bind(slug)
        .bind(name)
        .bind(color)
        .bind(category)
        .bind(order)
        .bind(now)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

async fn space_statuses(conn: &mut PgConnection, team_id: &str) -> ApiResult<Vec<StatusDef>> {
    Ok(sqlx::query_as::<_, StatusDef>(&format!(
        "SELECT {STATUS_COLUMNS} FROM pms_space_statuses WHERE team_id = $1 ORDER BY sort_order, created_at"
    ))
    .bind(team_id)
    .fetch_all(conn)
    .await?)
}

async fn list_statuses(conn: &mut PgConnection, list_id: &str) -> ApiResult<Vec<StatusDef>> {
    Ok(sqlx::query_as::<_, StatusDef>(&format!(
        "SELECT {STATUS_COLUMNS} FROM pms_task_list_statuses WHERE list_id = $1 ORDER BY sort_order, created_at"
    ))
    .bind(list_id)
    .fetch_all(conn)
    .await?)
}

/// `ensure_space_statuses`.
pub async fn ensure_space_statuses(
    conn: &mut PgConnection,
    team_id: &str,
) -> ApiResult<Vec<StatusDef>> {
    let statuses = space_statuses(conn, team_id).await?;
    if !statuses.is_empty() {
        return Ok(statuses);
    }
    create_default_space_statuses(conn, team_id).await?;
    space_statuses(conn, team_id).await
}

/// `ensure_task_list_custom_statuses`.
pub async fn ensure_task_list_custom_statuses(
    conn: &mut PgConnection,
    list_id: &str,
) -> ApiResult<Vec<StatusDef>> {
    let statuses = list_statuses(conn, list_id).await?;
    if !statuses.is_empty() {
        return Ok(statuses);
    }
    create_default_task_list_statuses(conn, list_id).await?;
    list_statuses(conn, list_id).await
}

/// `copy_space_statuses_to_task_list`.
pub async fn copy_space_statuses_to_task_list(
    conn: &mut PgConnection,
    list: &TaskListRow,
) -> ApiResult<Vec<StatusDef>> {
    let Some(team_id) = &list.team_id else {
        return ensure_task_list_custom_statuses(conn, &list.id).await;
    };
    let source = ensure_space_statuses(conn, team_id).await?;
    sqlx::query("DELETE FROM pms_task_list_statuses WHERE list_id = $1")
        .bind(&list.id)
        .execute(&mut *conn)
        .await?;
    let now = utcnow();
    for status in &source {
        sqlx::query(
            "INSERT INTO pms_task_list_statuses (id, list_id, slug, name, color, category, sort_order, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(new_id())
        .bind(&list.id)
        .bind(&status.slug)
        .bind(&status.name)
        .bind(&status.color)
        .bind(normalize_status_category(&status.category))
        .bind(status.sort_order)
        .bind(now)
        .execute(&mut *conn)
        .await?;
    }
    list_statuses(conn, &list.id).await
}

fn ensure_closed_status_rule(
    statuses: &[StatusDef],
    next_category: &str,
    exclude_id: Option<&str>,
) -> ApiResult<()> {
    if violates_single_closed_status_rule(statuses, next_category, exclude_id) {
        return Err(ApiError::conflict("pms.closed_status_single"));
    }
    Ok(())
}

fn statuses_response(mode: &str, statuses: &[StatusDef]) -> Value {
    json!({
        "mode": mode,
        "source": if mode == "inherit" { "space" } else { "list" },
        "items": statuses.iter().map(serialize_status).collect::<Vec<_>>(),
    })
}

/// `GET /v1/pms/lists/{list_id}/statuses`.
pub async fn list_task_list_statuses(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let mut tx = pool.begin().await?;
    let (list, _) = ensure_list_member(&mut tx, &identity.user_id, &list_id).await?;
    let response = match (&list.status_mode[..], &list.team_id) {
        ("inherit", Some(team_id)) => {
            statuses_response("inherit", &ensure_space_statuses(&mut tx, team_id).await?)
        }
        _ => statuses_response(
            "custom",
            &ensure_task_list_custom_statuses(&mut tx, &list.id).await?,
        ),
    };
    tx.commit().await?;
    Ok(Json(response))
}

/// `PATCH /v1/pms/lists/{list_id}/status-mode`.
pub async fn update_task_list_status_mode(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
    body: Body,
) -> ApiResult<Json<Value>> {
    let mode = body.req_str("mode")?;
    check_choice("mode", &mode, &["inherit", "custom"])?;
    let mut tx = pool.begin().await?;
    lock_task_list_order(&mut tx, &list_id).await?;
    let (list, _) = ensure_list_owner(&mut tx, &identity.user_id, &list_id).await?;
    if mode == "inherit" {
        let Some(team_id) = &list.team_id else {
            return Err(ApiError::conflict("pms.status_inherit_requires_space"));
        };
        let statuses = ensure_space_statuses(&mut tx, team_id).await?;
        if list.status_mode == mode {
            tx.commit().await?;
            return Ok(Json(statuses_response("inherit", &statuses)));
        }
        let used: Vec<String> =
            sqlx::query_scalar("SELECT DISTINCT status FROM pms_tasks WHERE list_id = $1")
                .bind(&list.id)
                .fetch_all(&mut *tx)
                .await?;
        let allowed: Vec<String> = statuses.iter().map(|status| status.slug.clone()).collect();
        let incompatible = incompatible_statuses_for_inherit(&used, &allowed);
        if !incompatible.is_empty() {
            return Err(ApiError::with_params(
                StatusCode::CONFLICT,
                "pms.status_mode_incompatible",
                &[("statuses", incompatible.join(", "))],
            ));
        }
        sqlx::query("UPDATE pms_task_lists SET status_mode = 'inherit' WHERE id = $1")
            .bind(&list.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(Json(statuses_response("inherit", &statuses)));
    }
    let statuses = if list.status_mode == mode {
        ensure_task_list_custom_statuses(&mut tx, &list.id).await?
    } else {
        let statuses = copy_space_statuses_to_task_list(&mut tx, &list).await?;
        sqlx::query("UPDATE pms_task_lists SET status_mode = 'custom' WHERE id = $1")
            .bind(&list.id)
            .execute(&mut *tx)
            .await?;
        statuses
    };
    tx.commit().await?;
    Ok(Json(statuses_response("custom", &statuses)))
}

/// `GET /v1/pms/spaces/{space_id}/statuses`.
pub async fn list_space_statuses(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let mut tx = pool.begin().await?;
    ensure_space_access(&mut tx, &identity.user_id, &space_id).await?;
    let statuses = ensure_space_statuses(&mut tx, &space_id).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"items": statuses.iter().map(serialize_status).collect::<Vec<_>>()}),
    ))
}

struct StatusInput {
    name: Option<String>,
    color: Option<String>,
    category: Option<String>,
    sort_order: Option<i64>,
}

fn status_input(body: &Body, create: bool) -> ApiResult<StatusInput> {
    let name = if create {
        Some(body.req_str("name")?)
    } else {
        body.opt_str("name")?
    };
    check_opt_length("name", &name, 1, 60)?;
    let color = if create {
        Some(body.str_or("color", "#6b7280")?)
    } else {
        body.opt_str("color")?
    };
    check_opt_length("color", &color, 0, 24)?;
    let category = if create {
        Some(body.str_or("category", "active")?)
    } else {
        body.opt_str("category")?
    };
    if let Some(category) = &category {
        check_choice("category", category, STATUS_CATEGORIES)?;
    }
    let sort_order = if create {
        Some(body.opt_int("sort_order")?.unwrap_or(0))
    } else {
        body.opt_int("sort_order")?
    };
    Ok(StatusInput {
        name,
        color,
        category,
        sort_order,
    })
}

/// `POST /v1/pms/spaces/{space_id}/statuses`.
pub async fn create_space_status(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    let input = status_input(&body, true)?;
    let name = input.name.unwrap_or_default();
    let mut tx = pool.begin().await?;
    ensure_space_manager(&mut tx, &identity.user_id, &space_id).await?;
    let statuses = ensure_space_statuses(&mut tx, &space_id).await?;
    let category = normalize_status_category(&input.category.unwrap_or_default());
    ensure_closed_status_rule(&statuses, &category, None)?;
    let slug = status_slug(&name);
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pms_space_statuses WHERE team_id = $1 AND slug = $2)",
    )
    .bind(&space_id)
    .bind(&slug)
    .fetch_one(&mut *tx)
    .await?;
    if exists {
        return Err(ApiError::conflict("pms.status_name_exists"));
    }
    let status = StatusDef {
        id: new_id(),
        slug,
        name: name.trim().to_string(),
        color: input.color.unwrap_or_default(),
        category,
        sort_order: input.sort_order.unwrap_or(0) as i32,
    };
    sqlx::query(
        "INSERT INTO pms_space_statuses (id, team_id, slug, name, color, category, sort_order, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&status.id)
    .bind(&space_id)
    .bind(&status.slug)
    .bind(&status.name)
    .bind(&status.color)
    .bind(&status.category)
    .bind(status.sort_order)
    .bind(utcnow())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(serialize_status(&status))))
}

async fn update_status_row(
    conn: &mut PgConnection,
    table: &str,
    owner_column: &str,
    current: &StatusDef,
    owner: &str,
    statuses: &[StatusDef],
    input: StatusInput,
) -> ApiResult<StatusDef> {
    let next_category = normalize_status_category(
        input
            .category
            .as_deref()
            .unwrap_or(current.category.as_str()),
    );
    ensure_closed_status_rule(statuses, &next_category, Some(&current.id))?;
    let mut next = current.clone();
    if let Some(name) = input.name {
        let normalized = name.trim().to_string();
        let exists: bool = sqlx::query_scalar(&format!(
            "SELECT EXISTS (SELECT 1 FROM {table} WHERE {owner_column} = $1 AND id <> $2 AND lower(name) = lower($3))"
        ))
        .bind(owner)
        .bind(&current.id)
        .bind(&normalized)
        .fetch_one(&mut *conn)
        .await?;
        if exists {
            return Err(ApiError::conflict("pms.status_name_exists"));
        }
        next.name = normalized;
    }
    if let Some(color) = input.color {
        next.color = color;
    }
    if input.category.is_some() {
        next.category = next_category;
    }
    if let Some(order) = input.sort_order {
        next.sort_order = order as i32;
    }
    sqlx::query(&format!(
        "UPDATE {table} SET name = $2, color = $3, category = $4, sort_order = $5 WHERE id = $1"
    ))
    .bind(&next.id)
    .bind(&next.name)
    .bind(&next.color)
    .bind(&next.category)
    .bind(next.sort_order)
    .execute(&mut *conn)
    .await?;
    Ok(next)
}

async fn load_space_status(
    conn: &mut PgConnection,
    status_id: &str,
) -> ApiResult<(StatusDef, String)> {
    #[derive(sqlx::FromRow)]
    struct Row {
        #[sqlx(flatten)]
        status: StatusDef,
        team_id: String,
    }
    let row = sqlx::query_as::<_, Row>(&format!(
        "SELECT {STATUS_COLUMNS}, team_id FROM pms_space_statuses WHERE id = $1"
    ))
    .bind(status_id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| ApiError::not_found("pms.status_not_found"))?;
    Ok((row.status, row.team_id))
}

async fn load_list_status(
    conn: &mut PgConnection,
    status_id: &str,
) -> ApiResult<(StatusDef, String)> {
    #[derive(sqlx::FromRow)]
    struct Row {
        #[sqlx(flatten)]
        status: StatusDef,
        list_id: String,
    }
    let row = sqlx::query_as::<_, Row>(&format!(
        "SELECT {STATUS_COLUMNS}, list_id FROM pms_task_list_statuses WHERE id = $1"
    ))
    .bind(status_id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| ApiError::not_found("pms.status_not_found"))?;
    Ok((row.status, row.list_id))
}

/// `PATCH /v1/pms/space-statuses/{status_id}`.
pub async fn update_space_status(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(status_id): Path<String>,
    body: Body,
) -> ApiResult<Json<Value>> {
    let input = status_input(&body, false)?;
    let mut tx = pool.begin().await?;
    let (current, team_id) = load_space_status(&mut tx, &status_id).await?;
    ensure_space_manager(&mut tx, &identity.user_id, &team_id).await?;
    let statuses = ensure_space_statuses(&mut tx, &team_id).await?;
    let next = update_status_row(
        &mut tx,
        "pms_space_statuses",
        "team_id",
        &current,
        &team_id,
        &statuses,
        input,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(serialize_status(&next)))
}

/// `DELETE /v1/pms/space-statuses/{status_id}`.
pub async fn delete_space_status(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(status_id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = pool.begin().await?;
    let (status, team_id) = load_space_status(&mut tx, &status_id).await?;
    ensure_space_manager(&mut tx, &identity.user_id, &team_id).await?;
    if normalize_status_category(&status.category) == "closed" {
        return Err(ApiError::conflict("pms.closed_status_required"));
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pms_tasks t JOIN pms_task_lists l ON l.id = t.list_id \
         WHERE l.team_id = $1 AND l.status_mode = 'inherit' AND t.status = $2",
    )
    .bind(&team_id)
    .bind(&status.slug)
    .fetch_one(&mut *tx)
    .await?;
    if count > 0 {
        return Err(ApiError::with_params(
            StatusCode::CONFLICT,
            "pms.status_in_use",
            &[("count", count.to_string())],
        ));
    }
    sqlx::query("DELETE FROM pms_space_statuses WHERE id = $1")
        .bind(&status_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/pms/lists/{list_id}/statuses`: switches an inheriting list to
/// its own copy of the space workflow first.
pub async fn create_task_list_status(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    let input = status_input(&body, true)?;
    let name = input.name.unwrap_or_default();
    let mut tx = pool.begin().await?;
    lock_task_list_order(&mut tx, &list_id).await?;
    let (list, _) = ensure_list_owner(&mut tx, &identity.user_id, &list_id).await?;
    if list.status_mode != "custom" {
        copy_space_statuses_to_task_list(&mut tx, &list).await?;
        sqlx::query("UPDATE pms_task_lists SET status_mode = 'custom' WHERE id = $1")
            .bind(&list.id)
            .execute(&mut *tx)
            .await?;
    }
    let statuses = ensure_task_list_custom_statuses(&mut tx, &list.id).await?;
    let category = normalize_status_category(&input.category.unwrap_or_default());
    ensure_closed_status_rule(&statuses, &category, None)?;
    let slug = status_slug(&name);
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pms_task_list_statuses WHERE list_id = $1 AND slug = $2)",
    )
    .bind(&list_id)
    .bind(&slug)
    .fetch_one(&mut *tx)
    .await?;
    if exists {
        return Err(ApiError::conflict("pms.status_name_exists"));
    }
    let status = StatusDef {
        id: new_id(),
        slug,
        name: name.trim().to_string(),
        color: input.color.unwrap_or_default(),
        category,
        sort_order: input.sort_order.unwrap_or(0) as i32,
    };
    sqlx::query(
        "INSERT INTO pms_task_list_statuses (id, list_id, slug, name, color, category, sort_order, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&status.id)
    .bind(&list_id)
    .bind(&status.slug)
    .bind(&status.name)
    .bind(&status.color)
    .bind(&status.category)
    .bind(status.sort_order)
    .bind(utcnow())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(serialize_status(&status))))
}

/// `PATCH /v1/pms/task-list-statuses/{status_id}`.
pub async fn update_task_list_status(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(status_id): Path<String>,
    body: Body,
) -> ApiResult<Json<Value>> {
    let input = status_input(&body, false)?;
    let mut tx = pool.begin().await?;
    let (_, list_id) = load_list_status(&mut tx, &status_id).await?;
    lock_task_list_order(&mut tx, &list_id).await?;
    let (current, _) = load_list_status(&mut tx, &status_id).await?;
    let (list, _) = ensure_list_owner(&mut tx, &identity.user_id, &list_id).await?;
    let statuses = ensure_task_list_custom_statuses(&mut tx, &list.id).await?;
    let next = update_status_row(
        &mut tx,
        "pms_task_list_statuses",
        "list_id",
        &current,
        &list_id,
        &statuses,
        input,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(serialize_status(&next)))
}

/// `DELETE /v1/pms/task-list-statuses/{status_id}`.
pub async fn delete_task_list_status(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(status_id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = pool.begin().await?;
    let (_, list_id) = load_list_status(&mut tx, &status_id).await?;
    lock_task_list_order(&mut tx, &list_id).await?;
    let (status, _) = load_list_status(&mut tx, &status_id).await?;
    ensure_list_owner(&mut tx, &identity.user_id, &list_id).await?;
    if normalize_status_category(&status.category) == "closed" {
        return Err(ApiError::conflict("pms.closed_status_required"));
    }
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pms_tasks WHERE list_id = $1 AND status = $2")
            .bind(&list_id)
            .bind(&status.slug)
            .fetch_one(&mut *tx)
            .await?;
    if count > 0 {
        return Err(ApiError::with_params(
            StatusCode::CONFLICT,
            "pms.status_in_use",
            &[("count", count.to_string())],
        ));
    }
    sqlx::query("DELETE FROM pms_task_list_statuses WHERE id = $1")
        .bind(&status_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
