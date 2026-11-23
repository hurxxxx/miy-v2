//! Custom fields per list and their values on tasks: AI-DO
//! task_list_customization.py and router.py (list/create/delete fields, read
//! and set values), with what miy adds for the reinforcement (docs/PORTING.md
//! §10.2): multi select and user fields, editing a field's name and options,
//! reordering, typed validation of values, and filters and sorting by a field.
//!
//! Fields are managed by the space owner or admin (AI-DO `_require_list_owner`)
//! under the list lock; values are written by list editors under the task lock.
use std::collections::{HashMap, HashSet};

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::NaiveDate;
use miy_app_sdk::Identity;
use serde_json::{Map, Value, json};
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder};

use crate::access::{
    ensure_list_member, ensure_list_owner, ensure_task_readable, lock_list_for_write,
    task_for_editor,
};
use crate::error::{ApiError, ApiResult, check_choice, check_length};
use crate::error::{ErrorResponse, IdentityErrorResponse};
use crate::input::{Body, check_list_length};
use crate::models::{TaskRow, new_id, utcnow};
use crate::tasks::{natural_title_key, validate_member_users};
use crate::users;

/// AI-DO had text, number, date and select (single). miy adds multi_select and
/// user (one space member).
pub const FIELD_TYPES: &[&str] = &["text", "number", "date", "select", "multi_select", "user"];
const MAX_FIELDS_PER_LIST: i64 = 50;
const MAX_OPTIONS: usize = 50;
const MAX_TEXT_VALUE: usize = 2000;
/// Query parameter prefix of a field filter (`cf_<field id>=value`) and of a
/// sort key (`sort_by=cf_<field id>`).
pub const PARAM_PREFIX: &str = "cf_";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FieldRow {
    pub id: String,
    pub list_id: String,
    pub name: String,
    pub field_type: String,
    pub options: Option<Value>,
    pub sort_order: i32,
}

const FIELD_COLUMNS: &str = "id, list_id, name, field_type, options, sort_order";

impl FieldRow {
    pub fn option_names(&self) -> Vec<String> {
        self.options
            .as_ref()
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn has_options(&self) -> bool {
        matches!(self.field_type.as_str(), "select" | "multi_select")
    }
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[schema(rename_all = "snake_case")]
enum CustomFieldKind {
    Text,
    Number,
    Date,
    Select,
    MultiSelect,
    User,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct CustomFieldDefinition {
    id: String,
    list_id: String,
    name: String,
    #[schema(value_type = CustomFieldKind)]
    field_type: String,
    #[schema(value_type = Option<Vec<String>>, required = true)]
    options: Option<Value>,
    sort_order: i32,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct CustomFieldPage {
    items: Vec<CustomFieldDefinition>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct CustomFieldValue {
    field_id: String,
    /// Stored string. Multi-select values contain a JSON-encoded string array.
    value: String,
}

#[allow(dead_code)]
#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct CreateCustomFieldRequest {
    /// Trimmed name of 1 to 100 characters, unique in the list.
    name: String,
    /// Defaults to `text`.
    #[schema(value_type = CustomFieldKind, required = false)]
    field_type: Option<String>,
    /// Required for `select` and `multi_select`. These fields allow 1 to 50 distinct options.
    options: Option<Vec<String>>,
    /// A field without an explicit order goes last.
    sort_order: Option<i64>,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
struct CustomFieldOptionRename {
    from: String,
    to: String,
}

#[allow(dead_code)]
#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct UpdateCustomFieldRequest {
    /// Trimmed name of 1 to 100 characters, unique in the list.
    name: Option<String>,
    /// New select options. Supply `expected_options` in the same request.
    #[schema(min_items = 1, max_items = 50)]
    options: Option<Vec<String>>,
    /// The complete option list read before editing, in order.
    expected_options: Option<Vec<String>>,
    #[schema(value_type = Option<Vec<CustomFieldOptionRename>>)]
    option_renames: Option<Vec<Value>>,
}

#[allow(dead_code)]
#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct ReorderCustomFieldsRequest {
    #[schema(min_items = 1, max_items = 50)]
    field_ids: Vec<String>,
}

#[allow(dead_code)]
#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct SetCustomFieldValueRequest {
    field_id: String,
    /// An empty or omitted value clears the field. Multi-select values use a JSON-encoded string array.
    #[schema(value_type = String, required = false)]
    value: Option<String>,
}

fn serialize_field(field: &FieldRow) -> CustomFieldDefinition {
    CustomFieldDefinition {
        id: field.id.clone(),
        list_id: field.list_id.clone(),
        name: field.name.clone(),
        field_type: field.field_type.clone(),
        options: field.options.clone(),
        sort_order: field.sort_order,
    }
}

fn invalid(message: &str) -> ApiError {
    ApiError::new(
        StatusCode::BAD_REQUEST,
        "pms.custom_field_value_invalid",
        message.to_string(),
    )
}

pub async fn list_fields(conn: &mut PgConnection, list_id: &str) -> ApiResult<Vec<FieldRow>> {
    Ok(sqlx::query_as::<_, FieldRow>(&format!(
        "SELECT {FIELD_COLUMNS} FROM pms_custom_fields WHERE list_id = $1 ORDER BY sort_order, created_at, id"
    ))
    .bind(list_id)
    .fetch_all(conn)
    .await?)
}

/// `GET /v1/pms/lists/{list_id}/custom-fields`.
#[utoipa::path(
    get, path = "/v1/pms/lists/{list_id}/custom-fields", operation_id = "listPmsCustomFields", tag = "Custom fields",
    params(("list_id" = String, Path)),
    responses(
        (status = 200, description = "List custom field definitions", body = CustomFieldPage),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller lacks access", body = ErrorResponse),
        (status = 404, description = "The requested resource was not found", body = ErrorResponse),
        (status = 409, description = "The current list or resource state conflicts with the request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn list_custom_fields(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
) -> ApiResult<Json<CustomFieldPage>> {
    let mut conn = pool.acquire().await?;
    ensure_list_member(&mut conn, &identity.user_id, &list_id).await?;
    let fields = list_fields(&mut conn, &list_id).await?;
    Ok(Json(CustomFieldPage {
        items: fields.iter().map(serialize_field).collect(),
    }))
}

/// Options of a select field: trimmed, 1–100 characters, unique ignoring case,
/// at most 50, in the given order.
pub fn normalize_options(options: &[String]) -> ApiResult<Vec<String>> {
    check_list_length("options", options.len(), 1, MAX_OPTIONS)?;
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for option in options {
        let option = option.trim().to_string();
        check_length("options", &option, 1, 100)?;
        if !seen.insert(option.to_lowercase()) {
            return Err(ApiError::new(
                StatusCode::BAD_REQUEST,
                "pms.custom_field_option_duplicate",
                "Each option of a custom field must be different.",
            ));
        }
        result.push(option);
    }
    Ok(result)
}

async fn ensure_unique_name(
    conn: &mut PgConnection,
    list_id: &str,
    name: &str,
    except: Option<&str>,
) -> ApiResult<()> {
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pms_custom_fields WHERE list_id = $1 \
         AND lower(name) = lower($2) AND id IS DISTINCT FROM $3)",
    )
    .bind(list_id)
    .bind(name)
    .bind(except)
    .fetch_one(conn)
    .await?;
    if taken {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "pms.custom_field_name_exists",
            "A custom field with this name already exists in this list.",
        ));
    }
    Ok(())
}

/// `POST /v1/pms/lists/{list_id}/custom-fields`. miy: names are unique in a
/// list (copying matches fields by name and type), select fields need options,
/// and a field without an explicit order goes last.
#[utoipa::path(
    post, path = "/v1/pms/lists/{list_id}/custom-fields", operation_id = "createPmsCustomField", tag = "Custom fields",
    params(("list_id" = String, Path)),
    request_body = CreateCustomFieldRequest,
    responses(
        (status = 201, description = "Created custom field definition", body = CustomFieldDefinition),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller lacks access", body = ErrorResponse),
        (status = 404, description = "The requested resource was not found", body = ErrorResponse),
        (status = 409, description = "The list is read-only, reached its field limit, or already has this field name", body = ErrorResponse),
        (status = 422, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn create_custom_field(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    body.forbid_extra(&["name", "field_type", "options", "sort_order"])?;
    let name = body.req_str("name")?.trim().to_string();
    check_length("name", &name, 1, 100)?;
    let field_type = body.str_or("field_type", "text")?;
    check_choice("field_type", &field_type, FIELD_TYPES)?;
    let options = match (field_type.as_str(), body.opt_str_list("options")?) {
        ("select" | "multi_select", Some(options)) => Some(normalize_options(&options)?),
        ("select" | "multi_select", None) => {
            return Err(ApiError::validation("options: Field required"));
        }
        _ => None,
    };
    let sort_order = body.opt_int("sort_order")?.map(|order| order as i32);
    let mut tx = pool.begin().await?;
    lock_list_for_write(&mut tx, &list_id).await?;
    ensure_list_owner(&mut tx, &identity.user_id, &list_id).await?;
    let (count, last): (i64, Option<i32>) = sqlx::query_as(
        "SELECT count(*), max(sort_order) FROM pms_custom_fields WHERE list_id = $1",
    )
    .bind(&list_id)
    .fetch_one(&mut *tx)
    .await?;
    if count >= MAX_FIELDS_PER_LIST {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "pms.custom_field_limit_reached",
            "A list can have at most 50 custom fields.",
        ));
    }
    ensure_unique_name(&mut tx, &list_id, &name, None).await?;
    let field = FieldRow {
        id: new_id(),
        list_id: list_id.clone(),
        name,
        field_type,
        options: options.map(|options| json!(options)),
        sort_order: sort_order.unwrap_or_else(|| last.map_or(0, |order| order + 1)),
    };
    sqlx::query(
        "INSERT INTO pms_custom_fields (id, list_id, name, field_type, options, sort_order, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&field.id)
    .bind(&field.list_id)
    .bind(&field.name)
    .bind(&field.field_type)
    .bind(&field.options)
    .bind(field.sort_order)
    .bind(utcnow())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(serialize_field(&field))))
}

/// Lock the field's list (space share lock, then the list row), check the
/// caller manages it now, then lock the field row.
async fn field_for_manager(
    conn: &mut PgConnection,
    user_id: &str,
    field_id: &str,
) -> ApiResult<FieldRow> {
    let list_id: String = sqlx::query_scalar("SELECT list_id FROM pms_custom_fields WHERE id = $1")
        .bind(field_id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| ApiError::not_found("pms.custom_field_not_found"))?;
    lock_list_for_write(conn, &list_id).await?;
    ensure_list_owner(conn, user_id, &list_id).await?;
    sqlx::query_as::<_, FieldRow>(&format!(
        "SELECT {FIELD_COLUMNS} FROM pms_custom_fields WHERE id = $1 FOR UPDATE"
    ))
    .bind(field_id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| ApiError::not_found("pms.custom_field_not_found"))
}

/// Rewrite one stored select or multi-select value after the options changed:
/// renamed options follow their new name, removed ones drop out. `None` means
/// the value is now empty.
pub fn remap_option_value(
    field_type: &str,
    value: &str,
    renames: &HashMap<String, String>,
    options: &[String],
) -> Option<String> {
    let follow = |option: &str| renames.get(option).cloned().unwrap_or(option.to_string());
    if field_type == "multi_select" {
        let chosen: HashSet<String> = serde_json::from_str::<Vec<String>>(value)
            .unwrap_or_default()
            .iter()
            .map(|option| follow(option))
            .collect();
        let kept: Vec<&String> = options.iter().filter(|o| chosen.contains(*o)).collect();
        return (!kept.is_empty()).then(|| json!(kept).to_string());
    }
    let next = follow(value);
    options.contains(&next).then_some(next)
}

/// `PATCH /v1/pms/custom-fields/{field_id}` (miy): rename, or replace the
/// options of a select field. `option_renames` (`[{from, to}]`) carries values
/// over to a renamed option; values of removed options are cleared. An option
/// edit must supply the unchanged options it was based on.
#[utoipa::path(
    patch, path = "/v1/pms/custom-fields/{field_id}", operation_id = "updatePmsCustomField", tag = "Custom fields",
    params(("field_id" = String, Path)),
    request_body = UpdateCustomFieldRequest,
    responses(
        (status = 200, description = "Updated custom field definition", body = CustomFieldDefinition),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller lacks access", body = ErrorResponse),
        (status = 404, description = "The requested resource was not found", body = ErrorResponse),
        (status = 409, description = "The list is read-only, the name is already used, or the options changed since the editor loaded them", body = ErrorResponse),
        (status = 422, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn update_custom_field(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(field_id): Path<String>,
    body: Body,
) -> ApiResult<Json<CustomFieldDefinition>> {
    body.forbid_extra(&["name", "options", "expected_options", "option_renames"])?;
    let name = body.opt_str("name")?.map(|name| name.trim().to_string());
    if let Some(name) = &name {
        check_length("name", name, 1, 100)?;
    }
    let options = body.opt_str_list("options")?;
    let expected_options = body.opt_str_list("expected_options")?;
    let option_update = match (options, expected_options) {
        (Some(options), Some(expected)) => Some((options, expected)),
        (Some(_), None) => {
            return Err(ApiError::validation(
                "expected_options: Required when changing options",
            ));
        }
        (None, _) => None,
    };
    let renames: Vec<(String, String)> = match body.0.get("option_renames") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                let from = item.get("from").and_then(Value::as_str);
                let to = item.get("to").and_then(Value::as_str);
                match (from, to) {
                    (Some(from), Some(to)) => Ok((from.trim().to_string(), to.trim().to_string())),
                    _ => Err(ApiError::validation(
                        "option_renames: Input should be a list of {from, to}",
                    )),
                }
            })
            .collect::<ApiResult<_>>()?,
        Some(_) => {
            return Err(ApiError::validation(
                "option_renames: Input should be a valid list",
            ));
        }
    };
    let mut tx = pool.begin().await?;
    let mut field = field_for_manager(&mut tx, &identity.user_id, &field_id).await?;
    if let Some(name) = name {
        ensure_unique_name(&mut tx, &field.list_id, &name, Some(&field.id)).await?;
        field.name = name;
    }
    if let Some((options, expected)) = option_update {
        if !field.has_options() {
            return Err(ApiError::validation(
                "options: Only select fields have options",
            ));
        }
        let options = normalize_options(&options)?;
        let previous = field.option_names();
        if expected != previous {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "pms.custom_field_options_changed",
                "The options changed. Reload the custom fields before editing them again.",
            ));
        }
        let renames: HashMap<String, String> = renames
            .into_iter()
            .filter(|(from, to)| from != to && previous.contains(from) && options.contains(to))
            .collect();
        let values: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, value FROM pms_custom_field_values WHERE field_id = $1 ORDER BY id FOR UPDATE",
        )
        .bind(&field.id)
        .fetch_all(&mut *tx)
        .await?;
        for (value_id, value) in values {
            match remap_option_value(&field.field_type, &value, &renames, &options) {
                Some(next) if next == value => {}
                Some(next) => {
                    sqlx::query("UPDATE pms_custom_field_values SET value = $2 WHERE id = $1")
                        .bind(&value_id)
                        .bind(&next)
                        .execute(&mut *tx)
                        .await?;
                }
                None => {
                    sqlx::query("DELETE FROM pms_custom_field_values WHERE id = $1")
                        .bind(&value_id)
                        .execute(&mut *tx)
                        .await?;
                }
            }
        }
        field.options = Some(json!(options));
    }
    sqlx::query("UPDATE pms_custom_fields SET name = $2, options = $3 WHERE id = $1")
        .bind(&field.id)
        .bind(&field.name)
        .bind(&field.options)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(serialize_field(&field)))
}

/// `PATCH /v1/pms/lists/{list_id}/custom-fields/reorder` (miy): `field_ids`
/// in the new order. Unknown ids are ignored; fields left out keep their
/// relative order after the given ones.
#[utoipa::path(
    patch, path = "/v1/pms/lists/{list_id}/custom-fields/reorder", operation_id = "reorderPmsCustomFields", tag = "Custom fields",
    params(("list_id" = String, Path)),
    request_body = ReorderCustomFieldsRequest,
    responses(
        (status = 200, description = "Custom field definitions in the new order", body = CustomFieldPage),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller lacks access", body = ErrorResponse),
        (status = 404, description = "The requested resource was not found", body = ErrorResponse),
        (status = 409, description = "The current list or resource state conflicts with the request", body = ErrorResponse),
        (status = 422, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn reorder_custom_fields(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
    body: Body,
) -> ApiResult<Json<CustomFieldPage>> {
    body.forbid_extra(&["field_ids"])?;
    let field_ids = body
        .opt_str_list("field_ids")?
        .ok_or_else(|| ApiError::validation("field_ids: Field required"))?;
    check_list_length(
        "field_ids",
        field_ids.len(),
        1,
        MAX_FIELDS_PER_LIST as usize,
    )?;
    let mut tx = pool.begin().await?;
    lock_list_for_write(&mut tx, &list_id).await?;
    ensure_list_owner(&mut tx, &identity.user_id, &list_id).await?;
    let fields = list_fields(&mut tx, &list_id).await?;
    let mut ordered: Vec<&FieldRow> = Vec::new();
    for id in &field_ids {
        if let Some(field) = fields.iter().find(|field| &field.id == id)
            && !ordered.iter().any(|seen| seen.id == field.id)
        {
            ordered.push(field);
        }
    }
    for field in &fields {
        if !ordered.iter().any(|seen| seen.id == field.id) {
            ordered.push(field);
        }
    }
    for (index, field) in ordered.iter().enumerate() {
        sqlx::query("UPDATE pms_custom_fields SET sort_order = $2 WHERE id = $1")
            .bind(&field.id)
            .bind(index as i32)
            .execute(&mut *tx)
            .await?;
    }
    let fields = list_fields(&mut tx, &list_id).await?;
    tx.commit().await?;
    Ok(Json(CustomFieldPage {
        items: fields.iter().map(serialize_field).collect(),
    }))
}

/// `DELETE /v1/pms/custom-fields/{field_id}`: its values go with it.
#[utoipa::path(
    delete, path = "/v1/pms/custom-fields/{field_id}", operation_id = "deletePmsCustomField", tag = "Custom fields",
    params(("field_id" = String, Path)),
    responses(
        (status = 204, description = "Custom field deleted"),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller lacks access", body = ErrorResponse),
        (status = 404, description = "The requested resource was not found", body = ErrorResponse),
        (status = 409, description = "The current list or resource state conflicts with the request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn delete_custom_field(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(field_id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = pool.begin().await?;
    let field = field_for_manager(&mut tx, &identity.user_id, &field_id).await?;
    sqlx::query("DELETE FROM pms_custom_field_values WHERE field_id = $1")
        .bind(&field.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM pms_custom_fields WHERE id = $1")
        .bind(&field.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/pms/tasks/{task_id}/custom-field-values`.
#[utoipa::path(
    get, path = "/v1/pms/tasks/{task_id}/custom-field-values", operation_id = "listPmsCustomFieldValues", tag = "Custom fields",
    params(("task_id" = String, Path)),
    responses(
        (status = 200, description = "Values stored on the task", body = [CustomFieldValue]),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller lacks access", body = ErrorResponse),
        (status = 404, description = "The requested resource was not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn list_custom_field_values(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(task_id): Path<String>,
) -> ApiResult<Json<Vec<CustomFieldValue>>> {
    let mut conn = pool.acquire().await?;
    let task = ensure_task_readable(&mut conn, &identity.user_id, &task_id).await?;
    let values: Vec<(String, String)> = sqlx::query_as(
        "SELECT field_id, value FROM pms_custom_field_values WHERE task_id = $1 ORDER BY id",
    )
    .bind(&task.id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(Json(
        values
            .into_iter()
            .map(|(field_id, value)| CustomFieldValue { field_id, value })
            .collect(),
    ))
}

/// A number in one canonical spelling, so equal numbers are equal strings
/// (filters compare the stored text): integers without a fraction, others in
/// the shortest form that reads back the same.
pub fn canonical_number(raw: &str) -> Option<String> {
    let value: f64 = raw.trim().parse().ok()?;
    if !value.is_finite() {
        return None;
    }
    if value.fract() == 0.0 && value.abs() < 1e15 {
        return Some(format!("{}", value as i64));
    }
    Some(format!("{value}"))
}

/// The stored form of a value, checked against the field type (users are
/// checked by the caller). `Ok(None)` clears the value.
pub fn normalize_value(field: &FieldRow, raw: &str) -> ApiResult<Option<String>> {
    if raw.trim().is_empty() {
        return Ok(None);
    }
    match field.field_type.as_str() {
        "number" => canonical_number(raw)
            .map(Some)
            .ok_or_else(|| invalid("The value must be a number.")),
        "date" => NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d")
            .map(|date| Some(date.format("%Y-%m-%d").to_string()))
            .map_err(|_| invalid("The value must be a date (YYYY-MM-DD).")),
        "select" => {
            let value = raw.trim().to_string();
            if field.option_names().contains(&value) {
                Ok(Some(value))
            } else {
                Err(invalid("The value must be one of the field's options."))
            }
        }
        "multi_select" => {
            let chosen: Vec<String> = serde_json::from_str(raw)
                .map_err(|_| invalid("The value must be a JSON list of options."))?;
            let options = field.option_names();
            if chosen.iter().any(|option| !options.contains(option)) {
                return Err(invalid("Every value must be one of the field's options."));
            }
            let kept: Vec<&String> = options.iter().filter(|o| chosen.contains(o)).collect();
            Ok((!kept.is_empty()).then(|| json!(kept).to_string()))
        }
        "user" => Ok(Some(raw.trim().to_string())),
        _ => {
            if raw.chars().count() > MAX_TEXT_VALUE {
                return Err(invalid("The value is too long."));
            }
            Ok(Some(raw.to_string()))
        }
    }
}

/// `PUT /v1/pms/tasks/{task_id}/custom-field-values`: `{field_id, value}`.
/// miy: the task row is locked and the caller's edit right checked after the
/// lock; the value is checked against the field type and stored in one
/// spelling (a multi select is a JSON list in option order, a user field a
/// current member of the space); an empty value removes it.
#[utoipa::path(
    put, path = "/v1/pms/tasks/{task_id}/custom-field-values", operation_id = "setPmsCustomFieldValue", tag = "Custom fields",
    params(("task_id" = String, Path)),
    request_body = SetCustomFieldValueRequest,
    responses(
        (status = 200, description = "Current value, or an empty string if cleared", body = CustomFieldValue),
        (status = 400, description = "The selection or related fields are invalid", body = ErrorResponse),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller lacks access", body = ErrorResponse),
        (status = 404, description = "The requested resource was not found", body = ErrorResponse),
        (status = 409, description = "The current list or resource state conflicts with the request", body = ErrorResponse),
        (status = 422, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
    )
)]
pub async fn set_custom_field_value(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(task_id): Path<String>,
    body: Body,
) -> ApiResult<Json<CustomFieldValue>> {
    body.forbid_extra(&["field_id", "value"])?;
    let field_id = body.req_str("field_id")?;
    let raw = body.str_or("value", "")?;
    let mut tx = pool.begin().await?;
    let (task, list) = task_for_editor(&mut tx, &identity.user_id, &task_id).await?;
    let field: Option<FieldRow> = sqlx::query_as::<_, FieldRow>(&format!(
        "SELECT {FIELD_COLUMNS} FROM pms_custom_fields WHERE id = $1 FOR SHARE"
    ))
    .bind(&field_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(field) = field.filter(|field| field.list_id == task.list_id) else {
        return Err(ApiError::bad_request("pms.custom_field_wrong_list"));
    };
    let value = normalize_value(&field, &raw)?;
    if field.field_type == "user"
        && let Some(user_id) = &value
    {
        validate_member_users(
            &mut tx,
            &list,
            std::slice::from_ref(user_id),
            "pms.custom_field_value_invalid",
        )
        .await
        .map_err(|_| invalid("The user must be a member of the space."))?;
    }
    match &value {
        Some(value) => {
            sqlx::query(
                "INSERT INTO pms_custom_field_values (id, task_id, field_id, value) VALUES ($1, $2, $3, $4) \
                 ON CONFLICT ON CONSTRAINT uq_pms_cf_value DO UPDATE SET value = EXCLUDED.value",
            )
            .bind(new_id())
            .bind(&task.id)
            .bind(&field.id)
            .bind(value)
            .execute(&mut *tx)
            .await?;
        }
        None => {
            sqlx::query("DELETE FROM pms_custom_field_values WHERE task_id = $1 AND field_id = $2")
                .bind(&task.id)
                .bind(&field.id)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(Json(CustomFieldValue {
        field_id: field.id,
        value: value.unwrap_or_default(),
    }))
}

/// Values of these tasks by task then field (task summaries carry them).
pub async fn values_by_task(
    conn: &mut PgConnection,
    task_ids: &[String],
) -> ApiResult<HashMap<String, Map<String, Value>>> {
    let mut values: HashMap<String, Map<String, Value>> = HashMap::new();
    for (task_id, field_id, value) in sqlx::query_as::<_, (String, String, String)>(
        "SELECT task_id, field_id, value FROM pms_custom_field_values WHERE task_id = ANY($1)",
    )
    .bind(task_ids)
    .fetch_all(conn)
    .await?
    {
        values
            .entry(task_id)
            .or_default()
            .insert(field_id, json!(value));
    }
    Ok(values)
}

// ── Filters and sorting ────────────────────────────────────────────────

/// One `cf_<field id>=value` condition, resolved against the list's fields.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldFilter {
    pub field_id: String,
    pub field_type: String,
    pub value: String,
}

/// The `cf_<field id>` parameters of a task query, raw.
pub fn raw_filters(params: &[(String, String)]) -> Vec<(String, String)> {
    params
        .iter()
        .filter_map(|(key, value)| {
            let field_id = key.strip_prefix(PARAM_PREFIX)?;
            (!field_id.is_empty() && !value.trim().is_empty())
                .then(|| (field_id.to_string(), value.clone()))
        })
        .collect()
}

/// Resolve raw conditions against the list's fields. A field that no longer
/// exists (a saved filter can outlive it) is ignored; a number or date that
/// does not parse is a validation error.
pub fn resolve_filters(
    fields: &[FieldRow],
    raw: &[(String, String)],
) -> ApiResult<Vec<FieldFilter>> {
    let mut result = Vec::new();
    for (field_id, value) in raw {
        let Some(field) = fields.iter().find(|field| &field.id == field_id) else {
            continue;
        };
        let value = match field.field_type.as_str() {
            "number" => canonical_number(value).ok_or_else(|| {
                ApiError::validation(format!(
                    "{PARAM_PREFIX}{field_id}: Input should be a number"
                ))
            })?,
            "date" => NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
                .map(|date| date.format("%Y-%m-%d").to_string())
                .map_err(|_| {
                    ApiError::validation(format!(
                        "{PARAM_PREFIX}{field_id}: Input should be a valid date"
                    ))
                })?,
            "text" => value.clone(),
            _ => value.trim().to_string(),
        };
        result.push(FieldFilter {
            field_id: field.id.clone(),
            field_type: field.field_type.clone(),
            value,
        });
    }
    Ok(result)
}

/// Add the conditions to a task query (alias `t`): text contains (ignoring
/// case), a multi select contains the option, the others are equal.
pub fn push_filters(query: &mut QueryBuilder<'_, Postgres>, filters: &[FieldFilter]) {
    for filter in filters {
        query
            .push(" AND EXISTS (SELECT 1 FROM pms_custom_field_values v WHERE v.task_id = t.id AND v.field_id = ")
            .push_bind(filter.field_id.clone());
        match filter.field_type.as_str() {
            "text" => {
                query
                    .push(" AND strpos(lower(v.value), lower(")
                    .push_bind(filter.value.clone())
                    .push(")) > 0)");
            }
            "multi_select" => {
                query
                    .push(" AND v.value::jsonb @> jsonb_build_array(")
                    .push_bind(filter.value.clone())
                    .push("::text))");
            }
            _ => {
                query
                    .push(" AND v.value = ")
                    .push_bind(filter.value.clone())
                    .push(")");
            }
        }
    }
}

/// The field a `sort_by=cf_<id>` names, if it is one of the list's.
pub fn sort_field<'a>(fields: &'a [FieldRow], sort_by: &str) -> Option<&'a FieldRow> {
    let field_id = sort_by.strip_prefix(PARAM_PREFIX)?;
    fields.iter().find(|field| field.id == field_id)
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
enum ValueKey {
    Number(f64),
    Text(String),
    Rank(usize, usize),
}

/// Sort tasks by a field's value: numbers numerically, dates and texts in
/// order (texts naturally, as titles), options in the field's option order, a
/// user field by name. Tasks without a value come last in either direction;
/// equal values keep the task number order.
pub async fn sort_tasks(
    conn: &mut PgConnection,
    identity: &Identity,
    field: &FieldRow,
    tasks: &mut Vec<TaskRow>,
    descending: bool,
) -> ApiResult<()> {
    let ids: Vec<String> = tasks.iter().map(|task| task.id.clone()).collect();
    let values: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT task_id, value FROM pms_custom_field_values WHERE field_id = $1 AND task_id = ANY($2)",
    )
    .bind(&field.id)
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .collect();
    let names = if field.field_type == "user" {
        let people: Vec<String> = values.values().cloned().collect();
        users::names(identity, &people).await
    } else {
        HashMap::new()
    };
    let options = field.option_names();
    let key = |value: &str| -> Option<ValueKey> {
        match field.field_type.as_str() {
            "number" => value.parse().ok().map(ValueKey::Number),
            "select" => options
                .iter()
                .position(|option| option == value)
                .map(|index| ValueKey::Rank(index, 0)),
            "multi_select" => {
                let chosen: Vec<String> = serde_json::from_str(value).unwrap_or_default();
                options
                    .iter()
                    .position(|option| chosen.contains(option))
                    .map(|index| ValueKey::Rank(index, usize::MAX - chosen.len()))
            }
            "user" => Some(ValueKey::Text(
                names
                    .get(value)
                    .cloned()
                    .unwrap_or_else(|| value.to_string())
                    .to_lowercase(),
            )),
            "date" => Some(ValueKey::Text(value.to_string())),
            _ => Some(ValueKey::Text(natural_title_key(value))),
        }
    };
    let (mut with, mut without): (Vec<(ValueKey, TaskRow)>, Vec<TaskRow>) =
        (Vec::new(), Vec::new());
    for task in tasks.drain(..) {
        match values.get(&task.id).and_then(|value| key(value)) {
            Some(key) => with.push((key, task)),
            None => without.push(task),
        }
    }
    with.sort_by(|(a, ta), (b, tb)| {
        let ordering = a
            .partial_cmp(b)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(ta.task_number.cmp(&tb.task_number));
        if descending {
            ordering.reverse()
        } else {
            ordering
        }
    });
    without.sort_by(|a, b| {
        let ordering = a.task_number.cmp(&b.task_number);
        if descending {
            ordering.reverse()
        } else {
            ordering
        }
    });
    tasks.extend(with.into_iter().map(|(_, task)| task));
    tasks.extend(without);
    Ok(())
}

/// For copies: whether a stored value fits the target field, and its form
/// there. Options must exist in the target (a multi select keeps the options
/// it has), a user must be a member of the target space.
pub fn copied_value(
    target: &FieldRow,
    value: &str,
    target_members: &HashSet<String>,
) -> Option<String> {
    match target.field_type.as_str() {
        "select" => target
            .option_names()
            .contains(&value.to_string())
            .then(|| value.to_string()),
        "multi_select" => {
            let chosen: Vec<String> = serde_json::from_str(value).ok()?;
            let options = target.option_names();
            if chosen.iter().any(|option| !options.contains(option)) {
                return None;
            }
            Some(value.to_string())
        }
        "user" => target_members.contains(value).then(|| value.to_string()),
        _ => Some(value.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(field_type: &str, options: &[&str]) -> FieldRow {
        FieldRow {
            id: "f".into(),
            list_id: "l".into(),
            name: "F".into(),
            field_type: field_type.into(),
            options: (!options.is_empty()).then(|| json!(options)),
            sort_order: 0,
        }
    }

    #[test]
    fn numbers_have_one_spelling() {
        assert_eq!(canonical_number("3.50").as_deref(), Some("3.5"));
        assert_eq!(canonical_number(" 007 ").as_deref(), Some("7"));
        assert_eq!(canonical_number("-2.0").as_deref(), Some("-2"));
        assert_eq!(canonical_number("1e3").as_deref(), Some("1000"));
        assert_eq!(canonical_number("abc"), None);
        assert_eq!(canonical_number("NaN"), None);
        assert_eq!(canonical_number("inf"), None);
    }

    #[test]
    fn values_are_checked_by_type() {
        assert_eq!(normalize_value(&field("text", &[]), "  ").unwrap(), None);
        assert_eq!(
            normalize_value(&field("date", &[]), "2026-11-22")
                .unwrap()
                .as_deref(),
            Some("2026-11-22")
        );
        assert!(normalize_value(&field("date", &[]), "22/11/2026").is_err());
        assert!(normalize_value(&field("number", &[]), "x").is_err());
        let select = field("select", &["A", "B"]);
        assert_eq!(normalize_value(&select, "B").unwrap().as_deref(), Some("B"));
        assert!(normalize_value(&select, "C").is_err());
        let multi = field("multi_select", &["A", "B", "C"]);
        assert_eq!(
            normalize_value(&multi, r#"["C","A","C"]"#)
                .unwrap()
                .as_deref(),
            Some(r#"["A","C"]"#)
        );
        assert_eq!(normalize_value(&multi, "[]").unwrap(), None);
        assert!(normalize_value(&multi, r#"["Z"]"#).is_err());
        assert!(normalize_value(&multi, "A").is_err());
    }

    #[test]
    fn options_are_trimmed_and_unique() {
        assert_eq!(
            normalize_options(&[" a ".into(), "b".into()]).unwrap(),
            vec!["a".to_string(), "b".to_string()]
        );
        assert!(normalize_options(&["a".into(), "A".into()]).is_err());
        assert!(normalize_options(&[]).is_err());
        assert!(normalize_options(&["  ".into()]).is_err());
    }

    #[test]
    fn renamed_options_carry_values_and_removed_ones_clear_them() {
        let renames: HashMap<String, String> = [("Old".to_string(), "New".to_string())].into();
        let options = vec!["New".to_string(), "Kept".to_string()];
        assert_eq!(
            remap_option_value("select", "Old", &renames, &options).as_deref(),
            Some("New")
        );
        assert_eq!(
            remap_option_value("select", "Gone", &renames, &options),
            None
        );
        assert_eq!(
            remap_option_value(
                "multi_select",
                r#"["Kept","Old","Gone"]"#,
                &renames,
                &options
            )
            .as_deref(),
            Some(r#"["New","Kept"]"#)
        );
        assert_eq!(
            remap_option_value("multi_select", r#"["Gone"]"#, &renames, &options),
            None
        );
    }

    #[test]
    fn filters_resolve_against_the_lists_fields() {
        let fields = vec![
            FieldRow {
                id: "n".into(),
                ..field("number", &[])
            },
            FieldRow {
                id: "t".into(),
                ..field("text", &[])
            },
        ];
        let raw = raw_filters(&[
            ("cf_n".into(), "2.50".into()),
            ("cf_t".into(), " Ab ".into()),
            ("cf_gone".into(), "x".into()),
            ("cf_".into(), "x".into()),
            ("q".into(), "x".into()),
        ]);
        let resolved = resolve_filters(&fields, &raw).unwrap();
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].value, "2.5");
        assert_eq!(resolved[1].value, " Ab ");
        assert!(resolve_filters(&fields, &[("n".into(), "x".into())]).is_err());
    }
}
