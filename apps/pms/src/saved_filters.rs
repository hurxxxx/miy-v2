//! Saved filters (miy reinforcement, docs/PORTING.md §10.4). AI-DO kept them
//! in browser storage per list (`pms_saved_filters_<list id>`); here they live
//! in the app DB per person and view: a list (`list_id`) or a space's combined
//! task view (`space_id`). A filter holds the filter bar (`params`, without the
//! search text), the sort and the list grouping. Space owners and admins can
//! share a filter with every member of the space. Nothing is migrated from
//! browser storage.
//!
//! Who sees what: a person sees their own filters and the shared filters of a
//! view they can read now; the owner manages a filter, and a space owner or
//! admin also manages (renames, changes, unshares, deletes) shared ones. Every
//! write share-locks the space and checks the caller's access after the lock.
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use miy_app_sdk::Identity;
use serde_json::{Map, Value, json};
use sqlx::{PgConnection, PgPool};

use crate::access::{MANAGER_ROLES, ensure_list_member, ensure_space_access, lock_space_for_write};
use crate::error::{ApiError, ApiResult, check_length};
use crate::input::{Body, Params, parse_date};
use crate::models::{iso_datetime, new_id, utcnow};
use crate::users;

const MAX_FILTERS_PER_VIEW: i64 = 100;
const MAX_PARAMS_BYTES: usize = 8 * 1024;
const GROUP_BY: &[&str] = &["none", "status", "assignee", "label"];
const ARCHIVED_STATES: &[&str] = &["active", "archived", "all"];

/// The view a saved filter belongs to.
#[derive(Debug, Clone)]
struct Scope {
    team_id: String,
    list_id: Option<String>,
    role: String,
}

impl Scope {
    fn key(&self) -> String {
        match &self.list_id {
            Some(list_id) => format!("list:{list_id}"),
            None => format!("space:{}", self.team_id),
        }
    }

    fn manager(&self) -> bool {
        MANAGER_ROLES.contains(&self.role.as_str())
    }
}

/// Read access to the view now (list membership through its space).
async fn resolve_scope(
    conn: &mut PgConnection,
    user_id: &str,
    list_id: Option<&str>,
    space_id: Option<&str>,
) -> ApiResult<Scope> {
    match (list_id, space_id) {
        (Some(list_id), None) => {
            let (list, role) = ensure_list_member(conn, user_id, list_id).await?;
            Ok(Scope {
                team_id: list
                    .team_id
                    .ok_or_else(|| ApiError::conflict("pms.task_list_space_missing"))?,
                list_id: Some(list.id),
                role,
            })
        }
        (None, Some(space_id)) => {
            let (space, role) = ensure_space_access(conn, user_id, space_id).await?;
            Ok(Scope {
                team_id: space.id,
                list_id: None,
                role,
            })
        }
        _ => Err(ApiError::validation(
            "list_id: Give exactly one of list_id and space_id",
        )),
    }
}

/// For writes: the space is share-locked first, then access is read.
async fn lock_scope(
    conn: &mut PgConnection,
    user_id: &str,
    list_id: Option<&str>,
    space_id: Option<&str>,
) -> ApiResult<Scope> {
    let team_id = match (list_id, space_id) {
        (Some(list_id), None) => {
            let team: Option<Option<String>> =
                sqlx::query_scalar("SELECT team_id FROM pms_task_lists WHERE id = $1")
                    .bind(list_id)
                    .fetch_optional(&mut *conn)
                    .await?;
            team.ok_or_else(|| ApiError::not_found("pms.task_list_not_found"))?
                .ok_or_else(|| ApiError::conflict("pms.task_list_space_missing"))?
        }
        (None, Some(space_id)) => space_id.to_string(),
        _ => {
            return Err(ApiError::validation(
                "list_id: Give exactly one of list_id and space_id",
            ));
        }
    };
    lock_space_for_write(conn, user_id, &team_id).await?;
    resolve_scope(conn, user_id, list_id, space_id).await
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct FilterRow {
    id: String,
    owner_id: String,
    team_id: String,
    list_id: Option<String>,
    name: String,
    params: Value,
    sort: Option<Value>,
    group_by: Option<String>,
    shared: bool,
    created_at: chrono::NaiveDateTime,
    updated_at: chrono::NaiveDateTime,
}

const FILTER_COLUMNS: &str =
    "id, owner_id, team_id, list_id, name, params, sort, group_by, shared, created_at, updated_at";

fn can_manage(filter: &FilterRow, user_id: &str, scope: &Scope) -> bool {
    filter.owner_id == user_id || (filter.shared && scope.manager())
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[schema(rename_all = "snake_case")]
enum FilterArchivedState {
    Active,
    Archived,
    All,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[schema(rename_all = "snake_case")]
enum FilterGroup {
    None,
    Status,
    Assignee,
    Label,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[schema(rename_all = "snake_case")]
enum FilterSortDirection {
    Asc,
    Desc,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
struct SavedFilterSort {
    field: String,
    direction: FilterSortDirection,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct SavedFilterParams {
    #[schema(max_items = 50)]
    status: Option<Vec<String>>,
    priority: Option<String>,
    assignee_id: Option<String>,
    label_id: Option<String>,
    milestone_id: Option<String>,
    #[schema(format = Date)]
    due_date_from: Option<String>,
    #[schema(format = Date)]
    due_date_to: Option<String>,
    #[schema(format = Date)]
    start_date_from: Option<String>,
    #[schema(format = Date)]
    start_date_to: Option<String>,
    archived_state: Option<FilterArchivedState>,
    /// Up to 50 custom field IDs mapped to their conditions.
    custom_fields: Option<std::collections::BTreeMap<String, String>>,
    /// Accepted for compatibility and never persisted.
    q: Option<Value>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct SavedFilterItem {
    id: String,
    name: String,
    #[schema(value_type = SavedFilterParams)]
    params: Value,
    #[schema(required, value_type = Option<SavedFilterSort>)]
    sort: Option<Value>,
    #[schema(required, value_type = Option<FilterGroup>)]
    group_by: Option<String>,
    shared: bool,
    space_id: String,
    #[schema(required)]
    list_id: Option<String>,
    owner_id: String,
    owner_name: String,
    is_mine: bool,
    is_default: bool,
    can_manage: bool,
    created_at: String,
    updated_at: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct SavedFiltersResponse {
    items: Vec<SavedFilterItem>,
    #[schema(required)]
    default_filter_id: Option<String>,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
struct SavedFilterSortInput {
    /// A task sort key or cf_<custom field ID>.
    field: String,
    /// Omit to sort ascending.
    direction: Option<FilterSortDirection>,
}

/// Specify exactly one of list_id and space_id.
#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct SavedFilterCreateRequest {
    list_id: Option<String>,
    space_id: Option<String>,
    #[schema(min_length = 1, max_length = 80)]
    name: String,
    /// The normalized filter is at most 8 KiB of JSON.
    params: SavedFilterParams,
    sort: Option<SavedFilterSortInput>,
    group_by: Option<FilterGroup>,
    shared: Option<crate::lists::BooleanInput>,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct SavedFilterUpdateRequest {
    #[schema(min_length = 1, max_length = 80)]
    name: Option<String>,
    params: Option<SavedFilterParams>,
    /// Omit to keep the current sort, or send null to clear it.
    sort: Option<SavedFilterSortInput>,
    /// Omit to keep the current group, or send null to clear it.
    group_by: Option<FilterGroup>,
    shared: Option<crate::lists::BooleanInput>,
}

/// Specify exactly one of list_id and space_id.
#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct SavedFilterDefaultRequest {
    list_id: Option<String>,
    space_id: Option<String>,
    /// Omit or send null to clear the current default.
    filter_id: Option<String>,
}

fn serialize_filter(
    filter: &FilterRow,
    user_id: &str,
    scope: &Scope,
    owner_name: Option<&String>,
    default_id: Option<&str>,
) -> SavedFilterItem {
    SavedFilterItem {
        id: filter.id.clone(),
        name: filter.name.clone(),
        params: filter.params.clone(),
        sort: filter.sort.clone(),
        group_by: filter.group_by.clone(),
        shared: filter.shared,
        space_id: filter.team_id.clone(),
        list_id: filter.list_id.clone(),
        owner_id: filter.owner_id.clone(),
        owner_name: owner_name
            .cloned()
            .unwrap_or_else(|| filter.owner_id.clone()),
        is_mine: filter.owner_id == user_id,
        is_default: default_id == Some(filter.id.as_str()),
        can_manage: can_manage(filter, user_id, scope),
        created_at: iso_datetime(&filter.created_at),
        updated_at: iso_datetime(&filter.updated_at),
    }
}

/// The caller's filters and the shared ones of this view.
async fn visible_filters(
    conn: &mut PgConnection,
    user_id: &str,
    scope: &Scope,
) -> ApiResult<Vec<FilterRow>> {
    Ok(sqlx::query_as::<_, FilterRow>(&format!(
        "SELECT {FILTER_COLUMNS} FROM pms_saved_filters WHERE team_id = $1 \
         AND list_id IS NOT DISTINCT FROM $2 AND (owner_id = $3 OR shared) \
         ORDER BY lower(name), created_at, id"
    ))
    .bind(&scope.team_id)
    .bind(&scope.list_id)
    .bind(user_id)
    .fetch_all(conn)
    .await?)
}

/// A filter of this view the caller can see, locked for the write.
async fn visible_filter_for_update(
    conn: &mut PgConnection,
    user_id: &str,
    scope: &Scope,
    filter_id: &str,
) -> ApiResult<FilterRow> {
    sqlx::query_as::<_, FilterRow>(&format!(
        "SELECT {FILTER_COLUMNS} FROM pms_saved_filters WHERE id = $1 AND team_id = $2 \
         AND list_id IS NOT DISTINCT FROM $3 AND (owner_id = $4 OR shared) FOR UPDATE"
    ))
    .bind(filter_id)
    .bind(&scope.team_id)
    .bind(&scope.list_id)
    .bind(user_id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(not_found)
}

fn not_found() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "pms.saved_filter_not_found",
        "Saved filter not found.",
    )
}

async fn default_filter_id(
    conn: &mut PgConnection,
    user_id: &str,
    scope: &Scope,
) -> ApiResult<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT filter_id FROM pms_saved_filter_defaults WHERE user_id = $1 AND scope = $2",
    )
    .bind(user_id)
    .bind(scope.key())
    .fetch_optional(conn)
    .await?)
}

async fn filter_list(
    conn: &mut PgConnection,
    identity: &Identity,
    scope: &Scope,
) -> ApiResult<SavedFiltersResponse> {
    let filters = visible_filters(conn, &identity.user_id, scope).await?;
    // A default that is no longer visible (unshared, deleted) counts as none.
    let default_id = default_filter_id(conn, &identity.user_id, scope)
        .await?
        .filter(|id| filters.iter().any(|filter| &filter.id == id));
    let mut owners: Vec<String> = filters.iter().map(|f| f.owner_id.clone()).collect();
    owners.sort();
    owners.dedup();
    let names = users::names(identity, &owners).await;
    Ok(SavedFiltersResponse {
        items: filters
            .iter()
            .map(|filter| {
                serialize_filter(
                    filter,
                    &identity.user_id,
                    scope,
                    names.get(&filter.owner_id),
                    default_id.as_deref(),
                )
            })
            .collect(),
        default_filter_id: default_id,
    })
}

/// `GET /v1/pms/saved-filters?list_id=` or `?space_id=`: `{items,
/// default_filter_id}`, by name.
#[utoipa::path(
    get, path = "/v1/pms/saved-filters", operation_id = "listPmsSavedFilters", tag = "Saved task filters",
    params(
        ("list_id" = Option<String>, Query, description = "Provide exactly one of list_id and space_id"),
        ("space_id" = Option<String>, Query, description = "Provide exactly one of list_id and space_id")
    ),
    responses(
        (status = 200, description = "Success", body = SavedFiltersResponse),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 409, description = "Current resource state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_saved_filters(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    params: Params,
) -> ApiResult<Json<SavedFiltersResponse>> {
    let list_id = params.opt_string("list_id").filter(|id| !id.is_empty());
    let space_id = params.opt_string("space_id").filter(|id| !id.is_empty());
    let mut conn = pool.acquire().await?;
    let scope = resolve_scope(
        &mut conn,
        &identity.user_id,
        list_id.as_deref(),
        space_id.as_deref(),
    )
    .await?;
    Ok(Json(filter_list(&mut conn, &identity, &scope).await?))
}

// ── Content checks ─────────────────────────────────────────────────────

fn opt_string<'a>(params: &'a Map<String, Value>, key: &str) -> ApiResult<Option<&'a str>> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            check_length(&format!("params.{key}"), value, 0, 200)?;
            Ok(Some(value))
        }
        Some(_) => Err(ApiError::validation(format!(
            "params.{key}: Input should be a valid string"
        ))),
    }
}

/// The filter bar as the screen keeps it (`TaskFilterParams` without `q`):
/// known keys only, strings, dates as `YYYY-MM-DD`, statuses as a list and
/// custom field conditions as `{field id: value}`. Empty values are dropped.
pub fn filter_params(value: &Value) -> ApiResult<Value> {
    let params = value
        .as_object()
        .ok_or_else(|| ApiError::validation("params: Input should be a valid dictionary"))?;
    let mut result = Map::new();
    for (key, item) in params {
        match key.as_str() {
            // The search text is never saved (AI-DO left it out too).
            "q" => {}
            "status" => match item {
                Value::Null => {}
                Value::Array(statuses) => {
                    if statuses.len() > 50 {
                        return Err(ApiError::validation(
                            "params.status: List should have at most 50 items after validation",
                        ));
                    }
                    let statuses: Vec<String> = statuses
                        .iter()
                        .map(|status| {
                            status
                                .as_str()
                                .filter(|status| status.chars().count() <= 40)
                                .map(str::to_string)
                                .ok_or_else(|| {
                                    ApiError::validation(
                                        "params.status: Input should be a valid string",
                                    )
                                })
                        })
                        .collect::<ApiResult<_>>()?;
                    result.insert(key.clone(), json!(statuses));
                }
                _ => {
                    return Err(ApiError::validation(
                        "params.status: Input should be a valid list",
                    ));
                }
            },
            "priority" | "assignee_id" | "label_id" | "milestone_id" => {
                if let Some(value) = opt_string(params, key)?.filter(|v| !v.is_empty()) {
                    result.insert(key.clone(), json!(value));
                }
            }
            "due_date_from" | "due_date_to" | "start_date_from" | "start_date_to" => {
                if let Some(value) = opt_string(params, key)?.filter(|v| !v.is_empty()) {
                    parse_date(&format!("params.{key}"), value)?;
                    result.insert(key.clone(), json!(value));
                }
            }
            "archived_state" => {
                if let Some(value) = opt_string(params, key)? {
                    if !ARCHIVED_STATES.contains(&value) {
                        return Err(ApiError::validation(
                            "params.archived_state: Input should be 'active', 'archived' or 'all'",
                        ));
                    }
                    result.insert(key.clone(), json!(value));
                }
            }
            "custom_fields" => match item {
                Value::Null => {}
                Value::Object(fields) => {
                    if fields.len() > 50 {
                        return Err(ApiError::validation(
                            "params.custom_fields: At most 50 fields",
                        ));
                    }
                    let mut kept = Map::new();
                    for (field_id, condition) in fields {
                        let condition = condition.as_str().ok_or_else(|| {
                            ApiError::validation(
                                "params.custom_fields: Input should be a valid string",
                            )
                        })?;
                        check_length("params.custom_fields", field_id, 1, 36)?;
                        check_length("params.custom_fields", condition, 0, 200)?;
                        if !condition.trim().is_empty() {
                            kept.insert(field_id.clone(), json!(condition));
                        }
                    }
                    if !kept.is_empty() {
                        result.insert(key.clone(), Value::Object(kept));
                    }
                }
                _ => {
                    return Err(ApiError::validation(
                        "params.custom_fields: Input should be a valid dictionary",
                    ));
                }
            },
            _ => {
                return Err(ApiError::validation(format!(
                    "params.{key}: Extra inputs are not permitted"
                )));
            }
        }
    }
    let value = Value::Object(result);
    if value.to_string().len() > MAX_PARAMS_BYTES {
        return Err(ApiError::validation("params: The filter is too large"));
    }
    Ok(value)
}

/// `{field, direction}`: a task list sort (`board_position`, dates, `title`,
/// `priority` …, or `cf_<field id>`) and `asc`/`desc`.
pub fn filter_sort(value: &Value) -> ApiResult<Option<Value>> {
    if value.is_null() {
        return Ok(None);
    }
    let sort = value
        .as_object()
        .ok_or_else(|| ApiError::validation("sort: Input should be a valid dictionary"))?;
    let field = sort
        .get("field")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::validation("sort.field: Field required"))?;
    check_length("sort.field", field, 1, 60)?;
    if !field
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return Err(ApiError::validation(
            "sort.field: Input should be a sort key",
        ));
    }
    let direction = sort
        .get("direction")
        .and_then(Value::as_str)
        .unwrap_or("asc");
    if direction != "asc" && direction != "desc" {
        return Err(ApiError::validation(
            "sort.direction: Input should be 'asc' or 'desc'",
        ));
    }
    Ok(Some(json!({"field": field, "direction": direction})))
}

fn filter_group_by(body: &Body) -> ApiResult<Option<String>> {
    let group_by = body.opt_str("group_by")?;
    if let Some(group_by) = &group_by
        && !GROUP_BY.contains(&group_by.as_str())
    {
        return Err(ApiError::validation(
            "group_by: Input should be 'none', 'status', 'assignee' or 'label'",
        ));
    }
    Ok(group_by)
}

fn share_denied() -> ApiError {
    ApiError::forbidden("pms.space_owner_admin_required")
}

// ── Writes ─────────────────────────────────────────────────────────────

/// `POST /v1/pms/saved-filters`: `{list_id | space_id, name, params, sort?,
/// group_by?, shared?}` (201). Only a space owner or admin may share.
#[utoipa::path(
    post, path = "/v1/pms/saved-filters", operation_id = "createPmsSavedFilter", tag = "Saved task filters",
    request_body = SavedFilterCreateRequest,
    responses(
        (status = 201, description = "Created", body = SavedFilterItem),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 409, description = "Current resource state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse)
    )
)]
pub async fn create_saved_filter(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    body.forbid_extra(&[
        "list_id", "space_id", "name", "params", "sort", "group_by", "shared",
    ])?;
    let list_id = body.opt_str("list_id")?;
    let space_id = body.opt_str("space_id")?;
    let name = body.req_str("name")?.trim().to_string();
    check_length("name", &name, 1, 80)?;
    let params = filter_params(
        body.0
            .get("params")
            .ok_or_else(|| ApiError::validation("params: Field required"))?,
    )?;
    let sort = filter_sort(body.0.get("sort").unwrap_or(&Value::Null))?;
    let group_by = filter_group_by(&body)?;
    let shared = body.opt_bool("shared")?.unwrap_or(false);
    let mut tx = pool.begin().await?;
    let scope = lock_scope(
        &mut tx,
        &identity.user_id,
        list_id.as_deref(),
        space_id.as_deref(),
    )
    .await?;
    if shared && !scope.manager() {
        return Err(share_denied());
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pms_saved_filters WHERE owner_id = $1 AND team_id = $2 \
         AND list_id IS NOT DISTINCT FROM $3",
    )
    .bind(&identity.user_id)
    .bind(&scope.team_id)
    .bind(&scope.list_id)
    .fetch_one(&mut *tx)
    .await?;
    if count >= MAX_FILTERS_PER_VIEW {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "pms.saved_filter_limit_reached",
            "A view can have at most 100 saved filters per person.",
        ));
    }
    let now = utcnow();
    let filter = FilterRow {
        id: new_id(),
        owner_id: identity.user_id.clone(),
        team_id: scope.team_id.clone(),
        list_id: scope.list_id.clone(),
        name,
        params,
        sort,
        group_by,
        shared,
        created_at: now,
        updated_at: now,
    };
    sqlx::query(&format!(
        "INSERT INTO pms_saved_filters ({FILTER_COLUMNS}) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"
    ))
    .bind(&filter.id)
    .bind(&filter.owner_id)
    .bind(&filter.team_id)
    .bind(&filter.list_id)
    .bind(&filter.name)
    .bind(&filter.params)
    .bind(&filter.sort)
    .bind(&filter.group_by)
    .bind(filter.shared)
    .bind(filter.created_at)
    .bind(filter.updated_at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let names = users::names(&identity, std::slice::from_ref(&filter.owner_id)).await;
    Ok((
        StatusCode::CREATED,
        Json(serialize_filter(
            &filter,
            &identity.user_id,
            &scope,
            names.get(&filter.owner_id),
            None,
        )),
    ))
}

/// The scope of an existing filter, locked like a new one.
async fn lock_filter_scope(
    conn: &mut PgConnection,
    user_id: &str,
    filter_id: &str,
) -> ApiResult<Scope> {
    let scope: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT team_id, list_id FROM pms_saved_filters WHERE id = $1")
            .bind(filter_id)
            .fetch_optional(&mut *conn)
            .await?;
    let (team_id, list_id) = scope.ok_or_else(not_found)?;
    match &list_id {
        Some(list_id) => lock_scope(conn, user_id, Some(list_id), None).await,
        None => lock_scope(conn, user_id, None, Some(&team_id)).await,
    }
}

/// `PATCH /v1/pms/saved-filters/{filter_id}`: rename or replace the content
/// (`name`, `params`, `sort`, `group_by`), or share and unshare (`shared`, space
/// owner or admin only). The owner manages a filter; space owners and admins
/// also manage shared ones. A filter the caller cannot see is not found.
#[utoipa::path(
    patch, path = "/v1/pms/saved-filters/{filter_id}", operation_id = "updatePmsSavedFilter", tag = "Saved task filters",
    params(
        ("filter_id" = String, Path)
    ),
    request_body = SavedFilterUpdateRequest,
    responses(
        (status = 200, description = "Success", body = SavedFilterItem),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 409, description = "Current resource state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse)
    )
)]
pub async fn update_saved_filter(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(filter_id): Path<String>,
    body: Body,
) -> ApiResult<Json<SavedFilterItem>> {
    body.forbid_extra(&["name", "params", "sort", "group_by", "shared"])?;
    let name = body.opt_str("name")?.map(|name| name.trim().to_string());
    if let Some(name) = &name {
        check_length("name", name, 1, 80)?;
    }
    let params = match body.0.get("params") {
        None | Some(Value::Null) => None,
        Some(value) => Some(filter_params(value)?),
    };
    let sort = match body.0.get("sort") {
        None => None,
        Some(value) => Some(filter_sort(value)?),
    };
    let group_by = filter_group_by(&body)?;
    let shared = body.opt_bool("shared")?;
    let mut tx = pool.begin().await?;
    let scope = lock_filter_scope(&mut tx, &identity.user_id, &filter_id).await?;
    let mut filter =
        visible_filter_for_update(&mut tx, &identity.user_id, &scope, &filter_id).await?;
    if !can_manage(&filter, &identity.user_id, &scope) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "pms.saved_filter_manage_denied",
            "Only the owner or a space owner or admin can change this saved filter.",
        ));
    }
    if let Some(shared) = shared
        && shared != filter.shared
    {
        if !scope.manager() {
            return Err(share_denied());
        }
        filter.shared = shared;
    }
    if let Some(name) = name {
        filter.name = name;
    }
    if let Some(params) = params {
        filter.params = params;
    }
    if let Some(sort) = sort {
        filter.sort = sort;
    }
    if body.has("group_by") {
        filter.group_by = group_by;
    }
    filter.updated_at = utcnow();
    sqlx::query(
        "UPDATE pms_saved_filters SET name = $2, params = $3, sort = $4, group_by = $5, \
         shared = $6, updated_at = $7 WHERE id = $1",
    )
    .bind(&filter.id)
    .bind(&filter.name)
    .bind(&filter.params)
    .bind(&filter.sort)
    .bind(&filter.group_by)
    .bind(filter.shared)
    .bind(filter.updated_at)
    .execute(&mut *tx)
    .await?;
    let default_id = default_filter_id(&mut tx, &identity.user_id, &scope).await?;
    tx.commit().await?;
    let names = users::names(&identity, std::slice::from_ref(&filter.owner_id)).await;
    Ok(Json(serialize_filter(
        &filter,
        &identity.user_id,
        &scope,
        names.get(&filter.owner_id),
        default_id.as_deref(),
    )))
}

/// `DELETE /v1/pms/saved-filters/{filter_id}`: same managers as an update.
/// Anyone's "open with" choice of it goes with it.
#[utoipa::path(
    delete, path = "/v1/pms/saved-filters/{filter_id}", operation_id = "deletePmsSavedFilter", tag = "Saved task filters",
    params(
        ("filter_id" = String, Path)
    ),
    responses(
        (status = 204, description = "Completed with no response body"),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 409, description = "Current resource state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse)
    )
)]
pub async fn delete_saved_filter(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(filter_id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = pool.begin().await?;
    let scope = lock_filter_scope(&mut tx, &identity.user_id, &filter_id).await?;
    let filter = visible_filter_for_update(&mut tx, &identity.user_id, &scope, &filter_id).await?;
    if !can_manage(&filter, &identity.user_id, &scope) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "pms.saved_filter_manage_denied",
            "Only the owner or a space owner or admin can change this saved filter.",
        ));
    }
    sqlx::query("DELETE FROM pms_saved_filters WHERE id = $1")
        .bind(&filter.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /v1/pms/saved-filters/default`: `{list_id | space_id, filter_id}` sets
/// the filter the caller opens this view with (`null` clears it). The filter
/// must be one the caller sees in that view. Answers the view's list.
#[utoipa::path(
    put, path = "/v1/pms/saved-filters/default", operation_id = "setPmsDefaultSavedFilter", tag = "Saved task filters",
    request_body = SavedFilterDefaultRequest,
    responses(
        (status = 200, description = "Success", body = SavedFiltersResponse),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 409, description = "Current resource state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse)
    )
)]
pub async fn set_default_saved_filter(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    body: Body,
) -> ApiResult<Json<SavedFiltersResponse>> {
    body.forbid_extra(&["list_id", "space_id", "filter_id"])?;
    let list_id = body.opt_str("list_id")?;
    let space_id = body.opt_str("space_id")?;
    let filter_id = body.opt_str("filter_id")?;
    let mut tx = pool.begin().await?;
    let scope = lock_scope(
        &mut tx,
        &identity.user_id,
        list_id.as_deref(),
        space_id.as_deref(),
    )
    .await?;
    match &filter_id {
        Some(filter_id) => {
            visible_filter_for_update(&mut tx, &identity.user_id, &scope, filter_id).await?;
            sqlx::query(
                "INSERT INTO pms_saved_filter_defaults (user_id, scope, filter_id) VALUES ($1, $2, $3) \
                 ON CONFLICT (user_id, scope) DO UPDATE SET filter_id = EXCLUDED.filter_id",
            )
            .bind(&identity.user_id)
            .bind(scope.key())
            .bind(filter_id)
            .execute(&mut *tx)
            .await?;
        }
        None => {
            sqlx::query("DELETE FROM pms_saved_filter_defaults WHERE user_id = $1 AND scope = $2")
                .bind(&identity.user_id)
                .bind(scope.key())
                .execute(&mut *tx)
                .await?;
        }
    }
    let list = filter_list(&mut tx, &identity, &scope).await?;
    tx.commit().await?;
    Ok(Json(list))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_params_keep_known_keys_without_the_search() {
        let params = filter_params(&json!({
            "q": "find me", "status": ["todo"], "priority": "high", "assignee_id": "",
            "due_date_from": "2026-11-01", "archived_state": "all",
            "custom_fields": {"f1": "A", "f2": " "},
        }))
        .unwrap();
        assert_eq!(
            params,
            json!({"status": ["todo"], "priority": "high", "due_date_from": "2026-11-01",
                "archived_state": "all", "custom_fields": {"f1": "A"}})
        );
        assert!(filter_params(&json!({"unknown": 1})).is_err());
        assert!(filter_params(&json!({"due_date_to": "11/22"})).is_err());
        assert!(filter_params(&json!({"archived_state": "gone"})).is_err());
        assert!(filter_params(&json!([])).is_err());
    }

    #[test]
    fn sorts_name_a_key_and_a_direction() {
        assert_eq!(
            filter_sort(&json!({"field": "cf_0f-1", "direction": "desc"})).unwrap(),
            Some(json!({"field": "cf_0f-1", "direction": "desc"}))
        );
        assert_eq!(filter_sort(&Value::Null).unwrap(), None);
        assert!(filter_sort(&json!({"field": "x; drop"})).is_err());
        assert!(filter_sort(&json!({"field": "title", "direction": "up"})).is_err());
    }
}
