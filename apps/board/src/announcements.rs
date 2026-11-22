//! Notices (공지): AI-DO domains/announcements/{router,service,schemas}.py.
//!
//! Scope `workspace` notices belong to one board space (AI-DO workspace) and
//! are read by its members; scope `company` notices are read by everyone who
//! may use the app. Space admins (ACL action `notice.manage`) write space
//! notices; board managers write company notices (AI-DO: platform admins).
//! Pinned notices come first, then the newest.
//!
//! AI-DO always carried a workspace in the path; company notices here can be
//! listed and written without one (`/v1/announcements`), and a company notice
//! keeps no space so deleting a space never removes it.
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use std::collections::HashMap;

use crate::{
    AppState,
    error::{ApiError, ApiJson, ApiResult, check_length},
    managers::is_manager,
    notify, people,
    spaces::{self, ACTION_NOTICE_MANAGE, ACTION_READ, Space},
    time::now_utc,
};

const DEFAULT_LIST_LIMIT: i64 = 20;
const MAX_LIST_LIMIT: i64 = 100;

#[derive(Clone, Debug, FromRow)]
pub struct Announcement {
    pub id: String,
    pub workspace_id: Option<String>,
    pub author_id: String,
    pub scope: String,
    pub title: String,
    pub body: String,
    pub is_pinned: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

pub const ANNOUNCEMENT_COLUMNS: &str =
    "id, workspace_id, author_id, scope, title, body, is_pinned, created_at, updated_at";

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnnouncementScope {
    Company,
    Workspace,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
enum CompanyAnnouncementScope {
    Company,
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnnouncementOut {
    id: String,
    #[schema(required = true)]
    workspace_id: Option<String>,
    author_id: String,
    author_name: String,
    #[schema(value_type = AnnouncementScope)]
    scope: String,
    title: String,
    body: String,
    is_pinned: bool,
    can_manage: bool,
    #[serde(serialize_with = "crate::time::serialize")]
    #[schema(value_type = String, example = "2026-05-04T01:02:03")]
    created_at: NaiveDateTime,
    #[serde(serialize_with = "crate::time::serialize")]
    #[schema(value_type = String, example = "2026-05-04T01:02:03")]
    updated_at: NaiveDateTime,
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnnouncementsResponse {
    items: Vec<AnnouncementOut>,
    has_more: bool,
}

fn project(
    row: &Announcement,
    names: &HashMap<String, String>,
    can_manage: bool,
) -> AnnouncementOut {
    AnnouncementOut {
        id: row.id.clone(),
        workspace_id: row.workspace_id.clone(),
        author_id: row.author_id.clone(),
        // AI-DO: the author's full name, else the author ID.
        author_name: names
            .get(&row.author_id)
            .cloned()
            .unwrap_or_else(|| row.author_id.clone()),
        scope: row.scope.clone(),
        title: row.title.clone(),
        body: row.body.clone(),
        is_pinned: row.is_pinned,
        can_manage,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn not_found() -> ApiError {
    ApiError::not_found("announcements.not_found", "Announcement not found.")
}

fn admin_required() -> ApiError {
    ApiError::forbidden(
        "announcements.admin_required",
        "Only space admins can create, edit, or delete announcements.",
    )
}

fn company_admin_required() -> ApiError {
    ApiError::forbidden(
        "announcements.company_admin_required",
        "Only board managers can manage company-wide announcements.",
    )
}

fn space_required() -> ApiError {
    ApiError::bad_request(
        "announcements.space_required",
        "Space announcements need a space.",
    )
}

fn parse_scope(value: &str) -> ApiResult<&'static str> {
    match value {
        "workspace" => Ok("workspace"),
        "company" => Ok("company"),
        _ => Err(ApiError::validation(
            "scope: Input should be 'workspace' or 'company'",
        )),
    }
}

/// Whether the caller may write notices of `scope` (AI-DO `_ensure_can_write`).
async fn can_write(
    state: &AppState,
    identity: &Identity,
    scope: &str,
    space: Option<&Space>,
) -> ApiResult<bool> {
    if scope == "company" {
        return is_manager(&state.pool, &identity.user_id).await;
    }
    let Some(space) = space else {
        return Ok(false);
    };
    match state
        .acl
        .require(
            identity,
            &spaces::space_key(&space.id),
            ACTION_NOTICE_MANAGE,
        )
        .await
    {
        Ok(()) => Ok(true),
        Err(error) if error.status == 403 => Ok(false),
        Err(error) => Err(error.into()),
    }
}

async fn ensure_can_write_in(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    identity: &Identity,
    scope: &str,
    space: Option<&Space>,
) -> ApiResult<()> {
    if scope == "company" {
        let manager: Option<String> =
            sqlx::query_scalar("SELECT user_id FROM board_managers WHERE user_id = $1 FOR SHARE")
                .bind(&identity.user_id)
                .fetch_optional(&mut **tx)
                .await?;
        return manager.map(|_| ()).ok_or_else(company_admin_required);
    }
    let space = space.ok_or_else(admin_required)?;
    match state
        .acl
        .require_in(
            tx,
            identity,
            &spaces::space_key(&space.id),
            ACTION_NOTICE_MANAGE,
        )
        .await
    {
        Ok(()) => Ok(()),
        Err(error) if error.status == 403 => Err(admin_required()),
        Err(error) => Err(error.into()),
    }
}

async fn load(
    pool: &PgPool,
    space: Option<&Space>,
    announcement_id: &str,
) -> ApiResult<Announcement> {
    let row = match space {
        Some(space) => {
            let sql = format!(
                "SELECT {ANNOUNCEMENT_COLUMNS} FROM announcements \
                 WHERE id = $1 AND (workspace_id = $2 OR scope = 'company')"
            );
            sqlx::query_as::<_, Announcement>(&sql)
                .bind(announcement_id)
                .bind(&space.id)
                .fetch_optional(pool)
                .await?
        }
        None => {
            let sql = format!(
                "SELECT {ANNOUNCEMENT_COLUMNS} FROM announcements WHERE id = $1 AND scope = 'company'"
            );
            sqlx::query_as::<_, Announcement>(&sql)
                .bind(announcement_id)
                .fetch_optional(pool)
                .await?
        }
    };
    row.ok_or_else(not_found)
}

async fn names_for(identity: &Identity, rows: &[Announcement]) -> HashMap<String, String> {
    people::display_names(identity, rows.iter().map(|row| row.author_id.clone())).await
}

/// The space of a `/v1/spaces/{id}/...` route, readable by the caller.
async fn route_space(
    state: &AppState,
    identity: &Identity,
    space_id: Option<&str>,
) -> ApiResult<Option<Space>> {
    match space_id {
        Some(id) => Ok(Some(
            spaces::require_space(&state.acl, &state.pool, identity, id, ACTION_READ).await?,
        )),
        None => Ok(None),
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    scope: Option<String>,
    limit: Option<String>,
    offset: Option<String>,
}

async fn list(
    state: AppState,
    identity: Identity,
    space_id: Option<String>,
    query: ListQuery,
) -> ApiResult<Json<AnnouncementsResponse>> {
    let scope = parse_scope(query.scope.as_deref().unwrap_or("workspace"))?;
    let limit = match query.limit.as_deref() {
        None => DEFAULT_LIST_LIMIT,
        Some(raw) => {
            let value: i64 = raw
                .trim()
                .parse()
                .map_err(|_| ApiError::validation("limit: Input should be a valid integer"))?;
            if !(1..=MAX_LIST_LIMIT).contains(&value) {
                return Err(ApiError::validation("limit: out of range"));
            }
            value
        }
    };
    let offset = query
        .offset
        .as_deref()
        .map(|raw| raw.trim().parse::<i64>())
        .transpose()
        .map_err(|_| ApiError::validation("offset: Input should be a valid integer"))?
        .unwrap_or_default();
    if offset < 0 {
        return Err(ApiError::validation("offset: out of range"));
    }
    let space = route_space(&state, &identity, space_id.as_deref()).await?;
    let mut rows = if scope == "workspace" {
        let space = space.as_ref().ok_or_else(space_required)?;
        let sql = format!(
            "SELECT {ANNOUNCEMENT_COLUMNS} FROM announcements WHERE scope = 'workspace' AND workspace_id = $1 \
             ORDER BY is_pinned DESC, created_at DESC, id DESC LIMIT $2 OFFSET $3"
        );
        sqlx::query_as::<_, Announcement>(&sql)
            .bind(&space.id)
            .bind(limit + 1)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?
    } else {
        let sql = format!(
            "SELECT {ANNOUNCEMENT_COLUMNS} FROM announcements WHERE scope = 'company' \
             ORDER BY is_pinned DESC, created_at DESC, id DESC LIMIT $1 OFFSET $2"
        );
        sqlx::query_as::<_, Announcement>(&sql)
            .bind(limit + 1)
            .bind(offset)
            .fetch_all(&state.pool)
            .await?
    };
    let has_more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let can_manage = can_write(&state, &identity, scope, space.as_ref()).await?;
    let names = names_for(&identity, &rows).await;
    Ok(Json(AnnouncementsResponse {
        items: rows
            .iter()
            .map(|row| project(row, &names, can_manage))
            .collect(),
        has_more,
    }))
}

#[utoipa::path(
    get, path = "/v1/announcements", operation_id = "listCompanyAnnouncements",
    params(
        ("scope" = CompanyAnnouncementScope, Query, description = "Supply company for the company-wide route"),
        ("limit" = Option<i64>, Query, minimum = 1, maximum = 100, description = "Page size (default 20)"),
        ("offset" = Option<i64>, Query, minimum = 0, description = "Results to skip (default 0)"),
    ),
    responses(
        (status = 200, description = "Success", body = AnnouncementsResponse),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 400, description = "Invalid request for this resource", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_company(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<AnnouncementsResponse>> {
    list(state, identity, None, query).await
}

#[utoipa::path(
    get, path = "/v1/spaces/{space_id}/announcements", operation_id = "listSpaceAnnouncements",
    params(
        ("space_id" = String, Path),
        ("scope" = Option<AnnouncementScope>, Query, description = "Select workspace (default) or company notices in this space context"),
        ("limit" = Option<i64>, Query, minimum = 1, maximum = 100, description = "Page size (default 20)"),
        ("offset" = Option<i64>, Query, minimum = 0, description = "Results to skip (default 0)"),
    ),
    responses(
        (status = 200, description = "Success", body = AnnouncementsResponse),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 400, description = "Invalid request for this resource", body = crate::error::ErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 503, description = "A required service is unavailable", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_in_space(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<AnnouncementsResponse>> {
    list(state, identity, Some(space_id), query).await
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequest {
    #[schema(min_length = 1, max_length = 200)]
    title: String,
    #[serde(default)]
    #[schema(max_length = 20000)]
    body: String,
    /// Omit or send null to select workspace notices.
    #[serde(default)]
    #[schema(value_type = Option<AnnouncementScope>)]
    scope: Option<String>,
    #[serde(default, alias = "is_pinned")]
    is_pinned: bool,
}

// The server parses both routes with CreateRequest so invalid company scopes
// keep the existing errors. A successful company request requires "company".
#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
struct CreateCompanyAnnouncementRequest {
    #[schema(min_length = 1, max_length = 200)]
    title: String,
    #[serde(default)]
    #[schema(max_length = 20000)]
    body: String,
    scope: CompanyAnnouncementScope,
    #[serde(default)]
    is_pinned: bool,
}

async fn create(
    state: AppState,
    identity: Identity,
    space_id: Option<String>,
    payload: CreateRequest,
) -> ApiResult<(StatusCode, Json<AnnouncementOut>)> {
    check_length("title", &payload.title, 1, 200)?;
    check_length("body", &payload.body, 0, 20000)?;
    let scope = parse_scope(payload.scope.as_deref().unwrap_or("workspace"))?;
    let space = route_space(&state, &identity, space_id.as_deref()).await?;
    if scope == "workspace" && space.is_none() {
        return Err(space_required());
    }
    let mut tx = state.pool.begin().await?;
    ensure_can_write_in(&state, &mut tx, &identity, scope, space.as_ref()).await?;
    let now = now_utc();
    let sql = format!(
        "INSERT INTO announcements (id, workspace_id, author_id, scope, title, body, is_pinned, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8) RETURNING {ANNOUNCEMENT_COLUMNS}"
    );
    let row = sqlx::query_as::<_, Announcement>(&sql)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(if scope == "workspace" {
            space.as_ref().map(|space| space.id.clone())
        } else {
            None
        })
        .bind(&identity.user_id)
        .bind(scope)
        .bind(payload.title.trim())
        .bind(&payload.body)
        .bind(payload.is_pinned)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    notify::notice_published(&state.pool, &identity, &row, space.as_ref()).await;
    let names = names_for(&identity, std::slice::from_ref(&row)).await;
    Ok((StatusCode::CREATED, Json(project(&row, &names, true))))
}

#[utoipa::path(
    post, path = "/v1/announcements", operation_id = "createCompanyAnnouncement",
    request_body = CreateCompanyAnnouncementRequest,
    responses(
        (status = 201, description = "Created", body = AnnouncementOut),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 400, description = "Invalid request for this resource", body = crate::error::ErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn create_company(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<CreateRequest>,
) -> ApiResult<(StatusCode, Json<AnnouncementOut>)> {
    create(state, identity, None, payload).await
}

#[utoipa::path(
    post, path = "/v1/spaces/{space_id}/announcements", operation_id = "createSpaceAnnouncement",
    params(
        ("space_id" = String, Path),
    ),
    request_body = CreateRequest,
    responses(
        (status = 201, description = "Created", body = AnnouncementOut),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 400, description = "Invalid request for this resource", body = crate::error::ErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 503, description = "A required service is unavailable", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn create_in_space(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    ApiJson(payload): ApiJson<CreateRequest>,
) -> ApiResult<(StatusCode, Json<AnnouncementOut>)> {
    create(state, identity, Some(space_id), payload).await
}

/// The notice's own space, for permission checks on a company-path request.
async fn notice_space(
    state: &AppState,
    row: &Announcement,
    route: Option<Space>,
) -> ApiResult<Option<Space>> {
    if row.scope == "company" {
        return Ok(route);
    }
    match &row.workspace_id {
        Some(id) => spaces::load_space(&state.pool, id).await,
        None => Ok(None),
    }
}

async fn get(
    state: AppState,
    identity: Identity,
    space_id: Option<String>,
    id: String,
) -> ApiResult<Json<AnnouncementOut>> {
    let space = route_space(&state, &identity, space_id.as_deref()).await?;
    let row = load(&state.pool, space.as_ref(), &id).await?;
    let own_space = notice_space(&state, &row, space).await?;
    let can_manage = can_write(&state, &identity, &row.scope, own_space.as_ref()).await?;
    notify::mark_read(&identity, &notify::notice_group_key(&row.id)).await;
    let names = names_for(&identity, std::slice::from_ref(&row)).await;
    Ok(Json(project(&row, &names, can_manage)))
}

#[utoipa::path(
    get, path = "/v1/announcements/{announcement_id}", operation_id = "getCompanyAnnouncement",
    params(
        ("announcement_id" = String, Path),
    ),
    responses(
        (status = 200, description = "Success", body = AnnouncementOut),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn get_company(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(id): Path<String>,
) -> ApiResult<Json<AnnouncementOut>> {
    get(state, identity, None, id).await
}

#[utoipa::path(
    get, path = "/v1/spaces/{space_id}/announcements/{announcement_id}", operation_id = "getSpaceAnnouncement",
    params(
        ("space_id" = String, Path),
        ("announcement_id" = String, Path),
    ),
    responses(
        (status = 200, description = "Success", body = AnnouncementOut),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 503, description = "A required service is unavailable", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn get_in_space(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, id)): Path<(String, String)>,
) -> ApiResult<Json<AnnouncementOut>> {
    get(state, identity, Some(space_id), id).await
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRequest {
    #[serde(default)]
    #[schema(min_length = 1, max_length = 200)]
    title: Option<String>,
    #[serde(default)]
    #[schema(max_length = 20000)]
    body: Option<String>,
    #[serde(default, alias = "is_pinned")]
    is_pinned: Option<bool>,
}

async fn update(
    state: AppState,
    identity: Identity,
    space_id: Option<String>,
    id: String,
    payload: UpdateRequest,
) -> ApiResult<Json<AnnouncementOut>> {
    if let Some(title) = &payload.title {
        check_length("title", title, 1, 200)?;
    }
    if let Some(body) = &payload.body {
        check_length("body", body, 0, 20000)?;
    }
    let space = route_space(&state, &identity, space_id.as_deref()).await?;
    let row = load(&state.pool, space.as_ref(), &id).await?;
    let own_space = notice_space(&state, &row, space).await?;
    let mut tx = state.pool.begin().await?;
    ensure_can_write_in(&state, &mut tx, &identity, &row.scope, own_space.as_ref()).await?;
    let sql = format!(
        "UPDATE announcements SET title = COALESCE($2, title), body = COALESCE($3, body), \
         is_pinned = COALESCE($4, is_pinned), \
         updated_at = CASE WHEN (title, body, is_pinned) IS DISTINCT FROM \
            (COALESCE($2, title), COALESCE($3, body), COALESCE($4, is_pinned)) THEN $5 ELSE updated_at END \
         WHERE id = $1 RETURNING {ANNOUNCEMENT_COLUMNS}"
    );
    let updated = sqlx::query_as::<_, Announcement>(&sql)
        .bind(&row.id)
        .bind(payload.title.as_deref().map(str::trim))
        .bind(&payload.body)
        .bind(payload.is_pinned)
        .bind(now_utc())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(not_found)?;
    tx.commit().await?;
    let names = names_for(&identity, std::slice::from_ref(&updated)).await;
    Ok(Json(project(&updated, &names, true)))
}

#[utoipa::path(
    patch, path = "/v1/announcements/{announcement_id}", operation_id = "updateCompanyAnnouncement",
    params(
        ("announcement_id" = String, Path),
    ),
    request_body = UpdateRequest,
    responses(
        (status = 200, description = "Success", body = AnnouncementOut),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn update_company(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(id): Path<String>,
    ApiJson(payload): ApiJson<UpdateRequest>,
) -> ApiResult<Json<AnnouncementOut>> {
    update(state, identity, None, id, payload).await
}

#[utoipa::path(
    patch, path = "/v1/spaces/{space_id}/announcements/{announcement_id}", operation_id = "updateSpaceAnnouncement",
    params(
        ("space_id" = String, Path),
        ("announcement_id" = String, Path),
    ),
    request_body = UpdateRequest,
    responses(
        (status = 200, description = "Success", body = AnnouncementOut),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 503, description = "A required service is unavailable", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn update_in_space(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, id)): Path<(String, String)>,
    ApiJson(payload): ApiJson<UpdateRequest>,
) -> ApiResult<Json<AnnouncementOut>> {
    update(state, identity, Some(space_id), id, payload).await
}

async fn delete(
    state: AppState,
    identity: Identity,
    space_id: Option<String>,
    id: String,
) -> ApiResult<StatusCode> {
    let space = route_space(&state, &identity, space_id.as_deref()).await?;
    let row = load(&state.pool, space.as_ref(), &id).await?;
    let own_space = notice_space(&state, &row, space).await?;
    let mut tx = state.pool.begin().await?;
    ensure_can_write_in(&state, &mut tx, &identity, &row.scope, own_space.as_ref()).await?;
    let deleted = sqlx::query("DELETE FROM announcements WHERE id = $1")
        .bind(&row.id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if deleted == 0 {
        return Err(not_found());
    }
    tx.commit().await?;
    notify::retract(&identity, &notify::notice_group_key(&row.id)).await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete, path = "/v1/announcements/{announcement_id}", operation_id = "deleteCompanyAnnouncement",
    params(
        ("announcement_id" = String, Path),
    ),
    responses(
        (status = 204, description = "No content"),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn delete_company(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    delete(state, identity, None, id).await
}

#[utoipa::path(
    delete, path = "/v1/spaces/{space_id}/announcements/{announcement_id}", operation_id = "deleteSpaceAnnouncement",
    params(
        ("space_id" = String, Path),
        ("announcement_id" = String, Path),
    ),
    responses(
        (status = 204, description = "No content"),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 503, description = "A required service is unavailable", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn delete_in_space(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    delete(state, identity, Some(space_id), id).await
}

// ---------------------------------------------------------------- recent notices (M5)

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecentNotice {
    id: String,
    #[schema(value_type = AnnouncementScope)]
    scope: String,
    #[schema(required = true)]
    space_id: Option<String>,
    #[schema(required = true)]
    space_name: Option<String>,
    title: String,
    is_pinned: bool,
    author_name: String,
    path: String,
    #[serde(serialize_with = "crate::time::serialize")]
    #[schema(value_type = String, example = "2026-05-04T01:02:03")]
    created_at: NaiveDateTime,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct RecentNoticesResponse {
    items: Vec<RecentNotice>,
}

pub fn notice_path(row: &Announcement) -> String {
    match (&row.scope[..], &row.workspace_id) {
        ("workspace", Some(space)) => format!("/notices/{}?space={space}", row.id),
        _ => format!("/notices/{}", row.id),
    }
}

#[derive(FromRow)]
pub(crate) struct NoticeWithSpace {
    #[sqlx(flatten)]
    pub notice: Announcement,
    pub space_name: Option<String>,
}

const NOTICES_WITH_SPACES: &str = "SELECT a.id, a.workspace_id, a.author_id, a.scope, a.title, a.body, \
    a.is_pinned, a.created_at, a.updated_at, s.name AS space_name \
    FROM announcements a LEFT JOIN board_spaces s ON s.id = a.workspace_id";

/// Notices a platform call may show: company notices and notices of spaces
/// the person reads through a user grant or through one of the group IDs the
/// Core signed into the call. Grants and announcements are read in the same
/// statement snapshot.
pub(crate) async fn platform_notices(
    pool: &PgPool,
    identity: &Identity,
    limit: i64,
) -> ApiResult<Vec<NoticeWithSpace>> {
    let groups = spaces::signed_groups(identity)?;
    Ok(sqlx::query_as(&format!(
        "{NOTICES_WITH_SPACES} WHERE a.scope = 'company' OR (a.scope = 'workspace' AND EXISTS (\
            SELECT 1 FROM app.miy_acl_grants g WHERE g.installation_id = $1::uuid AND g.environment = $2 \
              AND g.kind = $3 AND g.resource_id = a.workspace_id AND {}\
         )) ORDER BY a.is_pinned DESC, a.created_at DESC LIMIT $5",
        spaces::grant_reads_sql("$4", "$6", "$7")
    ))
    .bind(&identity.context.installation_id)
    .bind(identity.context.environment.as_deref())
    .bind(spaces::SPACE_KIND)
    .bind(&identity.user_id)
    .bind(limit)
    .bind(&groups)
    .bind(spaces::roles_with(ACTION_READ))
    .fetch_all(pool)
    .await?)
}

#[derive(Deserialize)]
pub struct RecentQuery {
    limit: Option<String>,
}

/// `GET /v1/board/notices/recent?limit=`: the newest notices the caller may
/// read (company notices and notices of spaces they belong to), for the
/// portal home notice card (M5). Read-only.
#[utoipa::path(
    get, path = "/v1/board/notices/recent", operation_id = "listRecentBoardNotices",
    params(
        ("limit" = Option<i64>, Query, minimum = 1, maximum = 20, description = "Number of notices to return (default 5)"),
    ),
    responses(
        (status = 200, description = "Success", body = RecentNoticesResponse),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 503, description = "A required service is unavailable", body = crate::error::ErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn recent(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Query(query): Query<RecentQuery>,
) -> ApiResult<Json<RecentNoticesResponse>> {
    let limit = match query.limit.as_deref() {
        None => 5,
        Some(raw) => {
            let value: i64 = raw
                .trim()
                .parse()
                .map_err(|_| ApiError::validation("limit: Input should be a valid integer"))?;
            if !(1..=20).contains(&value) {
                return Err(ApiError::validation("limit: out of range"));
            }
            value
        }
    };
    let space_ids = spaces::resource_ids(&state.acl, &identity, ACTION_READ).await?;
    let rows: Vec<NoticeWithSpace> = sqlx::query_as(&format!(
        "{NOTICES_WITH_SPACES} WHERE a.scope = 'company' OR (a.scope = 'workspace' AND a.workspace_id = ANY($1)) \
         ORDER BY a.created_at DESC LIMIT $2"
    ))
    .bind(&space_ids)
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;
    let mut visible = Vec::with_capacity(rows.len());
    let mut readable_spaces = HashMap::<String, bool>::new();
    let mut tx = state.pool.begin().await?;
    for row in rows {
        if row.notice.scope == "company" {
            visible.push(row);
            continue;
        }
        let Some(space_id) = &row.notice.workspace_id else {
            continue;
        };
        let allowed = if let Some(&allowed) = readable_spaces.get(space_id) {
            allowed
        } else {
            // Refuse grants removed after the initial candidate lookup.
            let allowed = match state
                .acl
                .require_in(
                    &mut tx,
                    &identity,
                    &spaces::space_key(space_id),
                    ACTION_READ,
                )
                .await
            {
                Ok(()) => true,
                Err(error) if error.status == 403 => false,
                Err(error) => return Err(error.into()),
            };
            readable_spaces.insert(space_id.clone(), allowed);
            allowed
        };
        if allowed {
            visible.push(row);
        }
    }
    tx.commit().await?;
    let names = people::display_names(
        &identity,
        visible.iter().map(|row| row.notice.author_id.clone()),
    )
    .await;
    Ok(Json(RecentNoticesResponse {
        items: visible
            .into_iter()
            .map(
                |NoticeWithSpace {
                     notice: row,
                     space_name,
                 }| RecentNotice {
                    path: notice_path(&row),
                    author_name: names
                        .get(&row.author_id)
                        .cloned()
                        .unwrap_or_else(|| row.author_id.clone()),
                    id: row.id,
                    scope: row.scope,
                    space_id: row.workspace_id,
                    space_name,
                    title: row.title,
                    is_pinned: row.is_pinned,
                    created_at: row.created_at,
                },
            )
            .collect(),
    }))
}
