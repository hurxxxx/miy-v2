//! Port of AI-DO `domains/personal_widgets` (router, service, schemas,
//! models) at 0002af062: the caller's own to-do list and single memo.
//! The PMS "assigned tasks" endpoint belongs to the PMS app and is not ported.
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;

use crate::error::{ApiError, ApiJson, ApiResult, check_length};
use crate::time::{now_utc, serialize_naive};

pub const TODO_SORT_STEP: i32 = 1000;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PersonalTodoItem {
    pub id: String,
    pub title: String,
    pub completed: bool,
    pub sort_order: i32,
    pub completed_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// Row columns read back (the owner is the WHERE clause, not output).
const TODO_COLUMNS: &str = "id, title, completed, sort_order, completed_at, created_at, updated_at";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PersonalMemo {
    pub id: String,
    pub body: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalTodoItemOut {
    pub id: String,
    pub title: String,
    pub completed: bool,
    pub sort_order: i32,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonalTodoListResponse {
    pub items: Vec<PersonalTodoItemOut>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalMemoOut {
    pub id: Option<String>,
    pub body: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PersonalTodoCreateRequest {
    pub title: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PersonalTodoUpdateRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub completed: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonalMemoUpdateRequest {
    pub body: String,
    // Required even for a first save, where the GET response has null.
    #[serde(deserialize_with = "Option::deserialize")]
    pub expected_updated_at: Option<NaiveDateTime>,
}

/// schemas.normalize_title: the length constraint applies to the raw value,
/// then the stripped title must not be empty.
pub fn normalize_title(value: &str) -> ApiResult<String> {
    check_length("title", value, 1, 240)?;
    let normalized = value.trim();
    if normalized.is_empty() {
        return Err(ApiError::validation(
            "title: Value error, title must not be empty",
        ));
    }
    Ok(normalized.to_string())
}

fn project_todo(row: &PersonalTodoItem) -> PersonalTodoItemOut {
    PersonalTodoItemOut {
        id: row.id.clone(),
        title: row.title.clone(),
        completed: row.completed,
        sort_order: row.sort_order,
        completed_at: row.completed_at.map(serialize_naive),
        created_at: serialize_naive(row.created_at),
        updated_at: serialize_naive(row.updated_at),
    }
}

fn project_memo(row: &PersonalMemo) -> PersonalMemoOut {
    PersonalMemoOut {
        id: Some(row.id.clone()),
        body: row.body.clone(),
        created_at: Some(serialize_naive(row.created_at)),
        updated_at: Some(serialize_naive(row.updated_at)),
    }
}

fn todo_not_found() -> ApiError {
    ApiError::not_found(
        "personal_widgets.todo_not_found",
        "Personal todo not found.",
    )
}

pub async fn list_todos(
    pool: &PgPool,
    user_id: &str,
    include_completed: bool,
) -> ApiResult<PersonalTodoListResponse> {
    let filter = if include_completed {
        ""
    } else {
        " AND completed = false"
    };
    let rows = sqlx::query_as::<_, PersonalTodoItem>(&format!(
        "SELECT {TODO_COLUMNS} FROM personal_todo_items WHERE user_id = $1{filter} \
         ORDER BY completed ASC, sort_order ASC, created_at DESC"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(PersonalTodoListResponse {
        items: rows.iter().map(project_todo).collect(),
    })
}

pub async fn create_todo(
    pool: &PgPool,
    user_id: &str,
    title: &str,
) -> ApiResult<PersonalTodoItemOut> {
    let now = now_utc();
    // One statement computes the next sort order and inserts, like AI-DO's
    // max(sort_order) + TODO_SORT_STEP.
    let row = sqlx::query_as::<_, PersonalTodoItem>(&format!(
        "INSERT INTO personal_todo_items \
         (id, user_id, title, completed, sort_order, completed_at, created_at, updated_at) \
         SELECT $1, $2, $3, false, \
           COALESCE((SELECT MAX(sort_order) FROM personal_todo_items WHERE user_id = $2), 0) + $4, \
           NULL, $5, $5 \
         RETURNING {TODO_COLUMNS}"
    ))
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(user_id)
    .bind(title)
    .bind(TODO_SORT_STEP)
    .bind(now)
    .fetch_one(pool)
    .await?;
    Ok(project_todo(&row))
}

pub async fn update_todo(
    pool: &PgPool,
    user_id: &str,
    todo_id: &str,
    title: Option<String>,
    completed: Option<bool>,
) -> ApiResult<PersonalTodoItemOut> {
    let mut tx = pool.begin().await?;
    let mut row = sqlx::query_as::<_, PersonalTodoItem>(&format!(
        "SELECT {TODO_COLUMNS} FROM personal_todo_items WHERE id = $1 AND user_id = $2 FOR UPDATE"
    ))
    .bind(todo_id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(todo_not_found)?;
    let mut changed = false;
    if let Some(title) = title
        && title != row.title
    {
        row.title = title;
        changed = true;
    }
    if let Some(completed) = completed
        && completed != row.completed
    {
        row.completed = completed;
        row.completed_at = completed.then(now_utc);
        changed = true;
    }
    if changed {
        row.updated_at = now_utc();
        sqlx::query(
            "UPDATE personal_todo_items SET title = $3, completed = $4, completed_at = $5, \
             updated_at = $6 WHERE id = $1 AND user_id = $2",
        )
        .bind(&row.id)
        .bind(user_id)
        .bind(&row.title)
        .bind(row.completed)
        .bind(row.completed_at)
        .bind(row.updated_at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(project_todo(&row))
}

pub async fn delete_todo(pool: &PgPool, user_id: &str, todo_id: &str) -> ApiResult<()> {
    let deleted = sqlx::query("DELETE FROM personal_todo_items WHERE id = $1 AND user_id = $2")
        .bind(todo_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    if deleted.rows_affected() == 0 {
        return Err(todo_not_found());
    }
    Ok(())
}

pub async fn get_memo(pool: &PgPool, user_id: &str) -> ApiResult<PersonalMemoOut> {
    let row = sqlx::query_as::<_, PersonalMemo>(
        "SELECT id, body, created_at, updated_at FROM personal_memos WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        Some(row) => project_memo(&row),
        None => PersonalMemoOut {
            id: None,
            body: String::new(),
            created_at: None,
            updated_at: None,
        },
    })
}

pub async fn save_memo(
    pool: &PgPool,
    user_id: &str,
    body: &str,
    expected_updated_at: Option<NaiveDateTime>,
) -> ApiResult<PersonalMemoOut> {
    let now = now_utc();
    // Compare in the write itself so two editors with the same saved revision
    // cannot both change it. A new revision always advances at DB precision.
    let row = if let Some(expected) = expected_updated_at {
        sqlx::query_as::<_, PersonalMemo>(
            "UPDATE personal_memos SET body = $2, updated_at = CASE \
             WHEN body = $2 THEN updated_at \
             ELSE GREATEST($3, updated_at + interval '1 microsecond') END \
             WHERE user_id = $1 AND updated_at = $4 \
             RETURNING id, body, created_at, updated_at",
        )
        .bind(user_id)
        .bind(body)
        .bind(now)
        .bind(expected)
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query_as::<_, PersonalMemo>(
            "INSERT INTO personal_memos (id, user_id, body, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $4) ON CONFLICT (user_id) DO NOTHING \
             RETURNING id, body, created_at, updated_at",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(user_id)
        .bind(body)
        .bind(now)
        .fetch_optional(pool)
        .await?
    };
    let row = row.ok_or_else(|| {
        ApiError::new(
            StatusCode::CONFLICT,
            "personal_widgets.memo_conflict",
            "The saved memo has changed.",
        )
    })?;
    Ok(project_memo(&row))
}

/// Pydantic's lax bool parsing of a query value.
fn parse_query_bool(name: &str, value: &str) -> ApiResult<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "on" | "t" | "true" | "y" | "yes" => Ok(true),
        "0" | "off" | "f" | "false" | "n" | "no" => Ok(false),
        _ => Err(ApiError::validation(format!(
            "{name}: Input should be a valid boolean"
        ))),
    }
}

// ── Router handlers (router.py) ─────────────────────────────────────────────

pub async fn list_personal_todos(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<PersonalTodoListResponse>> {
    let include_completed = match params.get("include_completed") {
        Some(value) => parse_query_bool("include_completed", value)?,
        None => true,
    };
    Ok(Json(
        list_todos(&pool, &identity.user_id, include_completed).await?,
    ))
}

pub async fn create_personal_todo(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<PersonalTodoCreateRequest>,
) -> ApiResult<impl IntoResponse> {
    let title = normalize_title(&payload.title)?;
    let created = create_todo(&pool, &identity.user_id, &title).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update_personal_todo(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(todo_id): Path<String>,
    ApiJson(payload): ApiJson<PersonalTodoUpdateRequest>,
) -> ApiResult<Json<PersonalTodoItemOut>> {
    let title = payload.title.as_deref().map(normalize_title).transpose()?;
    Ok(Json(
        update_todo(&pool, &identity.user_id, &todo_id, title, payload.completed).await?,
    ))
}

pub async fn delete_personal_todo(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(todo_id): Path<String>,
) -> ApiResult<StatusCode> {
    delete_todo(&pool, &identity.user_id, &todo_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_personal_memo(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<PersonalMemoOut>> {
    Ok(Json(get_memo(&pool, &identity.user_id).await?))
}

pub async fn save_personal_memo(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<PersonalMemoUpdateRequest>,
) -> ApiResult<Json<PersonalMemoOut>> {
    check_length("body", &payload.body, 0, 20000)?;
    Ok(Json(
        save_memo(
            &pool,
            &identity.user_id,
            &payload.body,
            payload.expected_updated_at,
        )
        .await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memo_save_requires_an_explicit_revision() {
        use serde_json::{from_value, json};

        assert!(from_value::<PersonalMemoUpdateRequest>(json!({"body":"memo"})).is_err());
        assert!(
            from_value::<PersonalMemoUpdateRequest>(json!({"expectedUpdatedAt":null})).is_err()
        );
        let first: PersonalMemoUpdateRequest =
            from_value(json!({"body":"memo", "expectedUpdatedAt":null})).unwrap();
        assert!(first.expected_updated_at.is_none());
        let now = now_utc();
        let update: PersonalMemoUpdateRequest =
            from_value(json!({"body":"memo", "expectedUpdatedAt":serialize_naive(now)})).unwrap();
        assert_eq!(update.expected_updated_at, Some(now));
    }

    #[test]
    fn title_is_checked_before_and_after_strip() {
        assert_eq!(normalize_title("  First task  ").unwrap(), "First task");
        assert_eq!(
            normalize_title("   ").unwrap_err().status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert!(normalize_title(&"x".repeat(241)).is_err());
        assert!(normalize_title(&"x".repeat(240)).is_ok());
        // Surrounding spaces count toward the limit, as in Pydantic.
        assert!(normalize_title(&format!(" {}", "x".repeat(240))).is_err());
    }

    #[test]
    fn query_bools_follow_pydantic() {
        assert!(!parse_query_bool("include_completed", "false").unwrap());
        assert!(!parse_query_bool("include_completed", "0").unwrap());
        assert!(parse_query_bool("include_completed", "TRUE").unwrap());
        assert!(parse_query_bool("include_completed", "maybe").is_err());
    }
}
