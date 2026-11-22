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
    response::IntoResponse,
};
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnouncementOut {
    id: String,
    workspace_id: Option<String>,
    author_id: String,
    author_name: String,
    scope: String,
    title: String,
    body: String,
    is_pinned: bool,
    can_manage: bool,
    #[serde(serialize_with = "crate::time::serialize")]
    created_at: NaiveDateTime,
    #[serde(serialize_with = "crate::time::serialize")]
    updated_at: NaiveDateTime,
}

#[derive(Serialize)]
pub struct AnnouncementsResponse {
    items: Vec<AnnouncementOut>,
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

async fn ensure_can_write(
    state: &AppState,
    identity: &Identity,
    scope: &str,
    space: Option<&Space>,
) -> ApiResult<()> {
    if can_write(state, identity, scope, space).await? {
        return Ok(());
    }
    Err(if scope == "company" {
        company_admin_required()
    } else {
        admin_required()
    })
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
    let space = route_space(&state, &identity, space_id.as_deref()).await?;
    let bounded = limit.clamp(1, MAX_LIST_LIMIT);
    let rows = if scope == "workspace" {
        let space = space.as_ref().ok_or_else(space_required)?;
        let sql = format!(
            "SELECT {ANNOUNCEMENT_COLUMNS} FROM announcements WHERE scope = 'workspace' AND workspace_id = $1 \
             ORDER BY is_pinned DESC, created_at DESC LIMIT $2"
        );
        sqlx::query_as::<_, Announcement>(&sql)
            .bind(&space.id)
            .bind(bounded)
            .fetch_all(&state.pool)
            .await?
    } else {
        let sql = format!(
            "SELECT {ANNOUNCEMENT_COLUMNS} FROM announcements WHERE scope = 'company' \
             ORDER BY is_pinned DESC, created_at DESC LIMIT $1"
        );
        sqlx::query_as::<_, Announcement>(&sql)
            .bind(bounded)
            .fetch_all(&state.pool)
            .await?
    };
    let can_manage = can_write(&state, &identity, scope, space.as_ref()).await?;
    let names = names_for(&identity, &rows).await;
    Ok(Json(AnnouncementsResponse {
        items: rows
            .iter()
            .map(|row| project(row, &names, can_manage))
            .collect(),
    }))
}

pub async fn list_company(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<AnnouncementsResponse>> {
    list(state, identity, None, query).await
}

pub async fn list_in_space(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<AnnouncementsResponse>> {
    list(state, identity, Some(space_id), query).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequest {
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default, alias = "is_pinned")]
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
    ensure_can_write(&state, &identity, scope, space.as_ref()).await?;
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
        .fetch_one(&state.pool)
        .await?;
    notify::notice_published(&state.pool, &identity, &row, space.as_ref()).await;
    let names = names_for(&identity, std::slice::from_ref(&row)).await;
    Ok((StatusCode::CREATED, Json(project(&row, &names, true))))
}

pub async fn create_company(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<CreateRequest>,
) -> ApiResult<impl IntoResponse> {
    create(state, identity, None, payload).await
}

pub async fn create_in_space(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    ApiJson(payload): ApiJson<CreateRequest>,
) -> ApiResult<impl IntoResponse> {
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

pub async fn get_company(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(id): Path<String>,
) -> ApiResult<Json<AnnouncementOut>> {
    get(state, identity, None, id).await
}

pub async fn get_in_space(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, id)): Path<(String, String)>,
) -> ApiResult<Json<AnnouncementOut>> {
    get(state, identity, Some(space_id), id).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRequest {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
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
    ensure_can_write(&state, &identity, &row.scope, own_space.as_ref()).await?;
    let title = payload
        .title
        .as_deref()
        .map(|title| title.trim().to_string())
        .unwrap_or_else(|| row.title.clone());
    let body = payload.body.clone().unwrap_or_else(|| row.body.clone());
    let is_pinned = payload.is_pinned.unwrap_or(row.is_pinned);
    let sql = format!(
        "UPDATE announcements SET title = $2, body = $3, is_pinned = $4, \
         updated_at = CASE WHEN (title, body, is_pinned) IS DISTINCT FROM ($2, $3, $4) THEN $5 ELSE updated_at END \
         WHERE id = $1 RETURNING {ANNOUNCEMENT_COLUMNS}"
    );
    let updated = sqlx::query_as::<_, Announcement>(&sql)
        .bind(&row.id)
        .bind(&title)
        .bind(&body)
        .bind(is_pinned)
        .bind(now_utc())
        .fetch_one(&state.pool)
        .await?;
    let names = names_for(&identity, std::slice::from_ref(&updated)).await;
    Ok(Json(project(&updated, &names, true)))
}

pub async fn update_company(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(id): Path<String>,
    ApiJson(payload): ApiJson<UpdateRequest>,
) -> ApiResult<Json<AnnouncementOut>> {
    update(state, identity, None, id, payload).await
}

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
    ensure_can_write(&state, &identity, &row.scope, own_space.as_ref()).await?;
    sqlx::query("DELETE FROM announcements WHERE id = $1")
        .bind(&row.id)
        .execute(&state.pool)
        .await?;
    notify::retract(&identity, &notify::notice_group_key(&row.id)).await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_company(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    delete(state, identity, None, id).await
}

pub async fn delete_in_space(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    delete(state, identity, Some(space_id), id).await
}

// ---------------------------------------------------------------- recent notices (M5)

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentNotice {
    id: String,
    scope: String,
    space_id: Option<String>,
    space_name: Option<String>,
    title: String,
    is_pinned: bool,
    author_name: String,
    path: String,
    #[serde(serialize_with = "crate::time::serialize")]
    created_at: NaiveDateTime,
}

#[derive(Serialize)]
pub struct RecentNoticesResponse {
    items: Vec<RecentNotice>,
}

pub fn notice_path(row: &Announcement) -> String {
    match (&row.scope[..], &row.workspace_id) {
        ("workspace", Some(space)) => format!("/notices/{}?space={space}", row.id),
        _ => format!("/notices/{}", row.id),
    }
}

/// Company notices and notices of the given spaces, newest first or pinned
/// first, with their space names.
pub async fn readable_notices(
    pool: &PgPool,
    space_ids: &[String],
    limit: i64,
    pinned_first: bool,
) -> ApiResult<Vec<(Announcement, Option<String>)>> {
    let order = if pinned_first {
        "a.is_pinned DESC, a.created_at DESC"
    } else {
        "a.created_at DESC"
    };
    let sql = format!(
        "SELECT a.id, a.workspace_id, a.author_id, a.scope, a.title, a.body, a.is_pinned, a.created_at, \
           a.updated_at, s.name AS space_name \
         FROM announcements a LEFT JOIN board_spaces s ON s.id = a.workspace_id \
         WHERE a.scope = 'company' OR (a.scope = 'workspace' AND a.workspace_id = ANY($1)) \
         ORDER BY {order} LIMIT $2"
    );
    let rows: Vec<(
        String,
        Option<String>,
        String,
        String,
        String,
        String,
        bool,
        NaiveDateTime,
        NaiveDateTime,
        Option<String>,
    )> = sqlx::query_as(&sql)
        .bind(space_ids)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                workspace_id,
                author_id,
                scope,
                title,
                body,
                is_pinned,
                created_at,
                updated_at,
                space_name,
            )| {
                (
                    Announcement {
                        id,
                        workspace_id,
                        author_id,
                        scope,
                        title,
                        body,
                        is_pinned,
                        created_at,
                        updated_at,
                    },
                    space_name,
                )
            },
        )
        .collect())
}

#[derive(Deserialize)]
pub struct RecentQuery {
    limit: Option<String>,
}

/// `GET /v1/board/notices/recent?limit=`: the newest notices the caller may
/// read (company notices and notices of spaces they belong to), for the
/// portal home notice card (M5). Read-only.
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
    let rows = readable_notices(&state.pool, &space_ids, limit, false).await?;
    let names =
        people::display_names(&identity, rows.iter().map(|(row, _)| row.author_id.clone())).await;
    Ok(Json(RecentNoticesResponse {
        items: rows
            .into_iter()
            .map(|(row, space_name)| RecentNotice {
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
            })
            .collect(),
    }))
}
