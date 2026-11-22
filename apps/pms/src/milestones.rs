//! Milestones: AI-DO router.py (list_milestones, create_milestone,
//! update_milestone), plus what miy adds for the reinforcement (docs/PORTING.md
//! §10.1): space-wide milestones (list_id NULL) next to list milestones, delete,
//! and an explicit `null` that clears a date.
//!
//! Reading follows the list or space; writing needs the space owner or admin
//! role, as AI-DO's `_require_list_owner`. Every write takes the parent lock
//! (space, then list) and checks the role after it.
use std::collections::HashMap;

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::NaiveDate;
use miy_app_sdk::Identity;
use sqlx::{PgConnection, PgPool};

use crate::access::{
    MANAGER_ROLES, ensure_list_member, ensure_list_owner, ensure_space_access, lock_list_for_write,
    lock_space_for_write,
};
use crate::error::{ApiError, ApiResult, check_choice, check_length};
use crate::input::{Body, Params, check_opt_length};
use crate::lists::{SortKey, stable_reverse};
use crate::models::{MilestoneRow, iso_date, iso_datetime, new_id, paginate, utcnow};
use crate::tasks::actor_name;
use crate::workflow::{calculate_progress, is_done_status};

pub const MILESTONE_COLUMNS: &str = "id, team_id, list_id, title, description, status, start_date, \
     due_date, sort_order, created_at, updated_at";
const MILESTONE_STATUSES: &[&str] = &["planned", "active", "complete"];

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[schema(rename_all = "snake_case")]
enum MilestoneStatus {
    Planned,
    Active,
    Complete,
}

#[derive(Clone, serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
enum MilestoneScope {
    List,
    Space,
}

#[derive(Clone, serde::Serialize, utoipa::ToSchema)]
pub struct MilestoneItem {
    id: String,
    team_id: String,
    #[schema(required)]
    list_id: Option<String>,
    #[schema(required)]
    list_name: Option<String>,
    scope: MilestoneScope,
    title: String,
    description: String,
    #[schema(value_type = MilestoneStatus)]
    status: String,
    #[schema(required, format = Date)]
    start_date: Option<String>,
    #[schema(required, format = Date)]
    due_date: Option<String>,
    sort_order: i32,
    progress: f64,
    task_count: usize,
    completed_task_count: usize,
    updated_at: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct MilestonePage {
    items: Vec<MilestoneItem>,
    total: usize,
    page: i64,
    page_size: i64,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
struct MilestoneCreateRequest {
    #[schema(min_length = 2, max_length = 140)]
    title: String,
    #[schema(required = false, value_type = String, max_length = 4000)]
    description: Option<String>,
    #[schema(required = false, value_type = MilestoneStatus)]
    status: Option<MilestoneStatus>,
    #[schema(format = Date)]
    start_date: Option<String>,
    /// Must be on or after start_date when both are present.
    #[schema(format = Date)]
    due_date: Option<String>,
    /// Omit or send null to append after the current last milestone.
    sort_order: Option<crate::lists::IntegerInput>,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
struct MilestoneUpdateRequest {
    #[schema(min_length = 2, max_length = 140)]
    title: Option<String>,
    #[schema(max_length = 4000)]
    description: Option<String>,
    status: Option<MilestoneStatus>,
    /// Omit to keep this date; null clears it.
    #[schema(format = Date)]
    start_date: Option<String>,
    /// Omit to keep this date; null clears it.
    #[schema(format = Date)]
    due_date: Option<String>,
    sort_order: Option<crate::lists::IntegerInput>,
}

/// `_serialize_milestone`: progress over the tasks that carry the milestone
/// (any list of the space for a space-wide one). `list_name` is filled for list
/// milestones when the caller asks for a whole space.
async fn serialize_milestones(
    conn: &mut PgConnection,
    milestones: &[MilestoneRow],
) -> ApiResult<Vec<MilestoneItem>> {
    let ids: Vec<String> = milestones.iter().map(|m| m.id.clone()).collect();
    let mut tasks_by_milestone: HashMap<String, Vec<(String, bool)>> = HashMap::new();
    for (milestone_id, status, archived) in sqlx::query_as::<_, (String, String, bool)>(
        "SELECT milestone_id, status, archived FROM pms_tasks WHERE milestone_id = ANY($1)",
    )
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?
    {
        tasks_by_milestone
            .entry(milestone_id)
            .or_default()
            .push((status, archived));
    }
    let list_ids: Vec<String> = milestones
        .iter()
        .filter_map(|m| m.list_id.clone())
        .collect();
    let list_names: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT id, name FROM pms_task_lists WHERE id = ANY($1)",
    )
    .bind(&list_ids)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .collect();
    Ok(milestones
        .iter()
        .map(|milestone| {
            let tasks = tasks_by_milestone
                .get(&milestone.id)
                .cloned()
                .unwrap_or_default();
            // AI-DO judged milestone completion without the list's workflow.
            let completed = tasks
                .iter()
                .filter(|(status, _)| is_done_status(status, None))
                .count();
            MilestoneItem {
                id: milestone.id.clone(),
                team_id: milestone.team_id.clone(),
                list_id: milestone.list_id.clone(),
                list_name: milestone
                    .list_id
                    .as_ref()
                    .and_then(|id| list_names.get(id))
                    .cloned(),
                scope: if milestone.list_id.is_some() {
                    MilestoneScope::List
                } else {
                    MilestoneScope::Space
                },
                title: milestone.title.clone(),
                description: milestone.description.clone(),
                status: milestone.status.clone(),
                start_date: milestone.start_date.as_ref().map(iso_date),
                due_date: milestone.due_date.as_ref().map(iso_date),
                sort_order: milestone.sort_order,
                progress: calculate_progress(
                    tasks
                        .iter()
                        .map(|(status, archived)| (status.as_str(), *archived)),
                    None,
                ),
                task_count: tasks.len(),
                completed_task_count: completed,
                updated_at: iso_datetime(&milestone.updated_at),
            }
        })
        .collect())
}

/// AI-DO's `sort_by`/`sort_dir` (sort_order, due_date, updated_at) and pages.
async fn milestone_page(
    conn: &mut PgConnection,
    params: &Params,
    mut milestones: Vec<MilestoneRow>,
) -> ApiResult<MilestonePage> {
    let page = params.int("page", 1, 1, None)?;
    let page_size = params.int("page_size", 20, 1, Some(100))?;
    let sort_by = params.string("sort_by", "sort_order");
    let sort_dir = params.choice("sort_dir", "asc", &["asc", "desc"])?;
    match sort_by.as_str() {
        "due_date" => milestones.sort_by_key(|m| m.due_date.unwrap_or(NaiveDate::MAX)),
        "updated_at" => milestones.sort_by_key(|m| m.updated_at),
        _ => milestones.sort_by_key(|m| m.sort_order),
    }
    if sort_dir == "desc" {
        let key = |m: &MilestoneRow| match sort_by.as_str() {
            "due_date" => SortKey::Text(iso_date(&m.due_date.unwrap_or(NaiveDate::MAX))),
            "updated_at" => SortKey::Time(m.updated_at),
            _ => SortKey::Float(m.sort_order as f64),
        };
        stable_reverse(&mut milestones, key);
    }
    let items = serialize_milestones(conn, &milestones).await?;
    let (page_items, total) = paginate(&items, page, page_size);
    Ok(MilestonePage {
        items: page_items,
        total,
        page,
        page_size,
    })
}

/// `GET /v1/pms/lists/{list_id}/milestones`: the list's milestones and the
/// space-wide milestones of its space (a task of the list may carry either).
#[utoipa::path(
    get, path = "/v1/pms/lists/{list_id}/milestones", operation_id = "listPmsListMilestones", tag = "Milestones",
    params(
        ("list_id" = String, Path),
        ("page" = Option<i64>, Query, minimum = 1, description = "Defaults to 1"),
        ("page_size" = Option<i64>, Query, minimum = 1, maximum = 100, description = "Defaults to 20"),
        ("sort_by" = Option<String>, Query, description = "sort_order, due_date or updated_at; omitted uses sort_order"),
        ("sort_dir" = Option<String>, Query, description = "asc or desc; omitted uses asc")
    ),
    responses(
        (status = 200, description = "Success", body = MilestonePage),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 409, description = "Current resource state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_milestones(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
    params: Params,
) -> ApiResult<Json<MilestonePage>> {
    let mut conn = pool.acquire().await?;
    let (list, _) = ensure_list_member(&mut conn, &identity.user_id, &list_id).await?;
    let milestones: Vec<MilestoneRow> = sqlx::query_as::<_, MilestoneRow>(&format!(
        "SELECT {MILESTONE_COLUMNS} FROM pms_milestones \
         WHERE list_id = $1 OR (list_id IS NULL AND team_id = $2) ORDER BY created_at, id"
    ))
    .bind(&list.id)
    .bind(&list.team_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(Json(milestone_page(&mut conn, &params, milestones).await?))
}

/// `GET /v1/pms/spaces/{space_id}/milestones` (miy): every milestone of the
/// space, space-wide ones and those of its lists.
#[utoipa::path(
    get, path = "/v1/pms/spaces/{space_id}/milestones", operation_id = "listPmsSpaceMilestones", tag = "Milestones",
    params(
        ("space_id" = String, Path),
        ("page" = Option<i64>, Query, minimum = 1, description = "Defaults to 1"),
        ("page_size" = Option<i64>, Query, minimum = 1, maximum = 100, description = "Defaults to 20"),
        ("sort_by" = Option<String>, Query, description = "sort_order, due_date or updated_at; omitted uses sort_order"),
        ("sort_dir" = Option<String>, Query, description = "asc or desc; omitted uses asc")
    ),
    responses(
        (status = 200, description = "Success", body = MilestonePage),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_space_milestones(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    params: Params,
) -> ApiResult<Json<MilestonePage>> {
    let mut conn = pool.acquire().await?;
    let (space, _) = ensure_space_access(&mut conn, &identity.user_id, &space_id).await?;
    let milestones: Vec<MilestoneRow> = sqlx::query_as::<_, MilestoneRow>(&format!(
        "SELECT {MILESTONE_COLUMNS} FROM pms_milestones WHERE team_id = $1 ORDER BY created_at, id"
    ))
    .bind(&space.id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(Json(milestone_page(&mut conn, &params, milestones).await?))
}

struct MilestoneInput {
    title: String,
    description: String,
    status: String,
    start_date: Option<NaiveDate>,
    due_date: Option<NaiveDate>,
    sort_order: Option<i32>,
}

fn create_input(body: &Body) -> ApiResult<MilestoneInput> {
    let title = body.req_str("title")?;
    check_length("title", &title, 2, 140)?;
    let description = body.str_or("description", "")?;
    check_length("description", &description, 0, 4000)?;
    let status = body.str_or("status", "planned")?;
    check_choice("status", &status, MILESTONE_STATUSES)?;
    let start_date = body.opt_date("start_date")?;
    let due_date = body.opt_date("due_date")?;
    check_milestone_dates(start_date, due_date)?;
    Ok(MilestoneInput {
        title: title.trim().to_string(),
        description: description.trim().to_string(),
        status,
        start_date,
        due_date,
        sort_order: body.opt_int("sort_order")?.map(|order| order as i32),
    })
}

/// miy: a milestone cannot end before it starts (tasks have the same rule).
fn check_milestone_dates(start: Option<NaiveDate>, due: Option<NaiveDate>) -> ApiResult<()> {
    if let (Some(start), Some(due)) = (start, due)
        && start > due
    {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "pms.milestone_date_range_invalid",
            "Milestone start date cannot be after its due date.",
        ));
    }
    Ok(())
}

async fn insert_milestone(
    conn: &mut PgConnection,
    team_id: &str,
    list_id: Option<&str>,
    input: MilestoneInput,
) -> ApiResult<MilestoneRow> {
    // Without an explicit order a new milestone goes last.
    let sort_order = match input.sort_order {
        Some(order) => order,
        None => {
            let current: Option<i32> = sqlx::query_scalar(
                "SELECT max(sort_order) FROM pms_milestones WHERE team_id = $1 \
                 AND list_id IS NOT DISTINCT FROM $2",
            )
            .bind(team_id)
            .bind(list_id)
            .fetch_one(&mut *conn)
            .await?;
            current.map_or(0, |order| order + 1)
        }
    };
    let now = utcnow();
    let milestone = MilestoneRow {
        id: new_id(),
        team_id: team_id.to_string(),
        list_id: list_id.map(str::to_string),
        title: input.title,
        description: input.description,
        status: input.status,
        start_date: input.start_date,
        due_date: input.due_date,
        sort_order,
        created_at: now,
        updated_at: now,
    };
    sqlx::query(&format!(
        "INSERT INTO pms_milestones ({MILESTONE_COLUMNS}) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)"
    ))
    .bind(&milestone.id)
    .bind(&milestone.team_id)
    .bind(&milestone.list_id)
    .bind(&milestone.title)
    .bind(&milestone.description)
    .bind(&milestone.status)
    .bind(milestone.start_date)
    .bind(milestone.due_date)
    .bind(milestone.sort_order)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(milestone)
}

/// `POST /v1/pms/lists/{list_id}/milestones`: a list milestone.
#[utoipa::path(
    post, path = "/v1/pms/lists/{list_id}/milestones", operation_id = "createPmsListMilestone", tag = "Milestones",
    params(
        ("list_id" = String, Path)
    ),
    request_body = MilestoneCreateRequest,
    responses(
        (status = 201, description = "Created", body = MilestoneItem),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 409, description = "Current resource state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse)
    )
)]
pub async fn create_milestone(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    let input = create_input(&body)?;
    let mut tx = pool.begin().await?;
    lock_list_for_write(&mut tx, &list_id).await?;
    let (list, _) = ensure_list_owner(&mut tx, &identity.user_id, &list_id).await?;
    let team_id = list
        .team_id
        .clone()
        .ok_or_else(|| ApiError::conflict("pms.task_list_space_missing"))?;
    let milestone = insert_milestone(&mut tx, &team_id, Some(&list.id), input).await?;
    let item = serialize_milestones(&mut tx, std::slice::from_ref(&milestone))
        .await?
        .remove(0);
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(item)))
}

fn require_manager(role: &str) -> ApiResult<()> {
    if MANAGER_ROLES.contains(&role) {
        Ok(())
    } else {
        Err(ApiError::forbidden("pms.space_owner_admin_required"))
    }
}

/// `POST /v1/pms/spaces/{space_id}/milestones` (miy): a space-wide milestone.
#[utoipa::path(
    post, path = "/v1/pms/spaces/{space_id}/milestones", operation_id = "createPmsSpaceMilestone", tag = "Milestones",
    params(
        ("space_id" = String, Path)
    ),
    request_body = MilestoneCreateRequest,
    responses(
        (status = 201, description = "Created", body = MilestoneItem),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse)
    )
)]
pub async fn create_space_milestone(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    let input = create_input(&body)?;
    let mut tx = pool.begin().await?;
    let (space, role) = lock_space_for_write(&mut tx, &identity.user_id, &space_id).await?;
    require_manager(&role)?;
    let milestone = insert_milestone(&mut tx, &space.id, None, input).await?;
    let item = serialize_milestones(&mut tx, std::slice::from_ref(&milestone))
        .await?
        .remove(0);
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(item)))
}

/// Lock the milestone's parents (space, then list), check the caller manages
/// them now, then lock and return the milestone row. `NO KEY UPDATE` keeps a
/// task write that sets this milestone (a foreign-key share lock) from waiting
/// on an edit; a delete takes the full row lock only at its final DELETE,
/// after the task rows.
async fn milestone_for_manager(
    conn: &mut PgConnection,
    user_id: &str,
    milestone_id: &str,
) -> ApiResult<MilestoneRow> {
    let scope: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT team_id, list_id FROM pms_milestones WHERE id = $1")
            .bind(milestone_id)
            .fetch_optional(&mut *conn)
            .await?;
    let Some((team_id, list_id)) = scope else {
        return Err(ApiError::not_found("pms.milestone_not_found"));
    };
    match &list_id {
        Some(list_id) => {
            lock_list_for_write(conn, list_id).await?;
            ensure_list_owner(conn, user_id, list_id).await?;
        }
        None => {
            let (_, role) = lock_space_for_write(conn, user_id, &team_id).await?;
            require_manager(&role)?;
        }
    }
    sqlx::query_as::<_, MilestoneRow>(&format!(
        "SELECT {MILESTONE_COLUMNS} FROM pms_milestones WHERE id = $1 FOR NO KEY UPDATE"
    ))
    .bind(milestone_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| ApiError::not_found("pms.milestone_not_found"))
}

/// `PATCH /v1/pms/milestones/{milestone_id}`: AI-DO applies only the fields
/// sent with a value (null leaves title, description, status and order as they
/// are). miy: an explicit `null` start or due date clears it, so a date set by
/// mistake can be removed from the screen.
#[utoipa::path(
    patch, path = "/v1/pms/milestones/{milestone_id}", operation_id = "updatePmsMilestone", tag = "Milestones",
    params(
        ("milestone_id" = String, Path)
    ),
    request_body = MilestoneUpdateRequest,
    responses(
        (status = 200, description = "Success", body = MilestoneItem),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 409, description = "Current resource state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse)
    )
)]
pub async fn update_milestone(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(milestone_id): Path<String>,
    body: Body,
) -> ApiResult<Json<MilestoneItem>> {
    let title = body.opt_str("title")?;
    check_opt_length("title", &title, 2, 140)?;
    let description = body.opt_str("description")?;
    check_opt_length("description", &description, 0, 4000)?;
    let status = body.opt_choice("status", MILESTONE_STATUSES)?;
    let start_date = body.opt_date("start_date")?;
    let due_date = body.opt_date("due_date")?;
    let sort_order = body.opt_int("sort_order")?;
    let mut tx = pool.begin().await?;
    let mut milestone = milestone_for_manager(&mut tx, &identity.user_id, &milestone_id).await?;
    let mut changed = false;
    if let Some(title) = title {
        let title = title.trim().to_string();
        changed |= milestone.title != title;
        milestone.title = title;
    }
    if let Some(description) = description {
        let description = description.trim().to_string();
        changed |= milestone.description != description;
        milestone.description = description;
    }
    if let Some(status) = status {
        changed |= milestone.status != status;
        milestone.status = status;
    }
    if body.has("start_date") {
        changed |= milestone.start_date != start_date;
        milestone.start_date = start_date;
    }
    if body.has("due_date") {
        changed |= milestone.due_date != due_date;
        milestone.due_date = due_date;
    }
    check_milestone_dates(milestone.start_date, milestone.due_date)?;
    if let Some(order) = sort_order {
        changed |= milestone.sort_order != order as i32;
        milestone.sort_order = order as i32;
    }
    if changed {
        milestone.updated_at = utcnow();
        sqlx::query(
            "UPDATE pms_milestones SET title = $2, description = $3, status = $4, start_date = $5, \
             due_date = $6, sort_order = $7, updated_at = $8 WHERE id = $1",
        )
        .bind(&milestone.id)
        .bind(&milestone.title)
        .bind(&milestone.description)
        .bind(&milestone.status)
        .bind(milestone.start_date)
        .bind(milestone.due_date)
        .bind(milestone.sort_order)
        .bind(milestone.updated_at)
        .execute(&mut *tx)
        .await?;
    }
    let item = serialize_milestones(&mut tx, std::slice::from_ref(&milestone))
        .await?
        .remove(0);
    tx.commit().await?;
    Ok(Json(item))
}

/// `DELETE /v1/pms/milestones/{milestone_id}` (miy; AI-DO had no delete). Its
/// tasks keep everything else and lose the milestone (the foreign key sets it
/// to NULL); each gets an activity entry. The task rows are locked first, in
/// id order, so a task write waiting on its row cannot deadlock with the
/// cascade.
#[utoipa::path(
    delete, path = "/v1/pms/milestones/{milestone_id}", operation_id = "deletePmsMilestone", tag = "Milestones",
    params(
        ("milestone_id" = String, Path)
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
pub async fn delete_milestone(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(milestone_id): Path<String>,
) -> ApiResult<StatusCode> {
    let name = actor_name(&identity).await;
    let mut tx = pool.begin().await?;
    let milestone = milestone_for_manager(&mut tx, &identity.user_id, &milestone_id).await?;
    let task_ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM pms_tasks WHERE milestone_id = $1 ORDER BY id FOR UPDATE",
    )
    .bind(&milestone.id)
    .fetch_all(&mut *tx)
    .await?;
    if !task_ids.is_empty() {
        sqlx::query(
            "INSERT INTO pms_task_activity_logs \
             (id, task_id, actor_id, action, field_name, from_value, to_value, message, created_at) \
             SELECT gen_random_uuid()::text, t.id, $2, 'updated', 'milestone_id', $3, NULL, $4, $5 \
             FROM unnest($1::text[]) AS t(id)",
        )
        .bind(&task_ids)
        .bind(&identity.user_id)
        .bind(&milestone.id)
        .bind(format!(
            "{name} deleted milestone \"{}\".",
            milestone.title
        ))
        .bind(utcnow())
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("DELETE FROM pms_milestones WHERE id = $1")
        .bind(&milestone.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
