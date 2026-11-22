//! Board spaces (공간): the miy stand-in for AI-DO workspaces, which owned
//! workspace notices (announcements scope "workspace").
//!
//! Membership is the SDK resource ACL (kind `space`), so a space can be shared
//! with people and directory groups. Roles keep AI-DO's workspace distinction:
//! `member` reads the space's notices; `admin` also writes them (AI-DO
//! `workspace_role_allows(role, "admin")`) and manages the members. Board
//! managers create spaces; the creator is the protected owner.
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::NaiveDateTime;
use miy_app_sdk::{
    Identity,
    acl::{Acl, Grant, MANAGE, ResourceKey, RoleDefinition, ShareSnapshot, SubjectKind},
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::collections::{BTreeSet, HashSet};

use crate::{
    AppState,
    error::{ApiError, ApiJson, ApiResult, check_length},
    managers::require_manager,
    notify,
    time::now_utc,
};

pub const SPACE_KIND: &str = "space";
pub const ACTION_READ: &str = "space.read";
pub const ACTION_NOTICE_MANAGE: &str = "notice.manage";

pub fn acl_roles() -> Vec<RoleDefinition> {
    vec![
        RoleDefinition {
            id: "member".into(),
            label: "멤버".into(),
            actions: vec![ACTION_READ.into()],
        },
        RoleDefinition {
            id: "admin".into(),
            label: "관리자".into(),
            actions: vec![
                ACTION_READ.into(),
                ACTION_NOTICE_MANAGE.into(),
                MANAGE.into(),
            ],
        },
    ]
}

pub fn space_key(space_id: &str) -> ResourceKey {
    ResourceKey {
        kind: SPACE_KIND.into(),
        id: space_id.into(),
    }
}

#[derive(Clone, Debug, FromRow)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub description: String,
    pub created_by: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

const SPACE_COLUMNS: &str = "id, name, description, created_by, created_at, updated_at";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceOut {
    id: String,
    name: String,
    description: String,
    created_by: String,
    can_manage: bool,
    #[serde(serialize_with = "crate::time::serialize")]
    created_at: NaiveDateTime,
    #[serde(serialize_with = "crate::time::serialize")]
    updated_at: NaiveDateTime,
}

fn space_out(space: &Space, can_manage: bool) -> SpaceOut {
    SpaceOut {
        id: space.id.clone(),
        name: space.name.clone(),
        description: space.description.clone(),
        created_by: space.created_by.clone(),
        can_manage,
        created_at: space.created_at,
        updated_at: space.updated_at,
    }
}

#[derive(Serialize)]
pub struct SpacesResponse {
    spaces: Vec<SpaceOut>,
}

pub fn space_not_found() -> ApiError {
    ApiError::not_found("board.space_not_found", "Space not found.")
}

pub async fn load_space(pool: &PgPool, space_id: &str) -> ApiResult<Option<Space>> {
    let sql = format!("SELECT {SPACE_COLUMNS} FROM board_spaces WHERE id = $1");
    Ok(sqlx::query_as::<_, Space>(&sql)
        .bind(space_id)
        .fetch_optional(pool)
        .await?)
}

/// The space, when the caller may read it (404 for an unknown space, the
/// ACL's 403 `acl_denied` for a non-member).
pub async fn require_space(
    acl: &Acl,
    pool: &PgPool,
    identity: &Identity,
    space_id: &str,
    action: &str,
) -> ApiResult<Space> {
    let space = load_space(pool, space_id)
        .await?
        .ok_or_else(space_not_found)?;
    acl.require(identity, &space_key(space_id), action).await?;
    Ok(space)
}

/// Every space ID the caller can act on, in the ACL's own SQL filter.
pub async fn resource_ids(acl: &Acl, identity: &Identity, action: &str) -> ApiResult<Vec<String>> {
    let mut ids = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..20 {
        let page = acl
            .list_resources(identity, SPACE_KIND, action, 100, cursor.as_deref())
            .await?;
        ids.extend(page.resources.into_iter().map(|key| key.id));
        match page.next_cursor {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    Ok(ids)
}

/// Direct user grants of a space (any role). Used where no directory call is
/// possible or wanted: notification recipients and platform rechecks.
/// Members through a group are not expanded (docs/PORTING.md §3.6).
pub async fn direct_user_members(
    pool: &PgPool,
    identity: &Identity,
    space_id: &str,
) -> ApiResult<Vec<String>> {
    let Some(environment) = identity.context.environment.as_deref() else {
        return Ok(Vec::new());
    };
    Ok(sqlx::query_scalar(
        "SELECT subject_id FROM app.miy_acl_grants WHERE installation_id = $1::uuid AND environment = $2 \
         AND kind = $3 AND resource_id = $4 AND subject_kind = 'user' ORDER BY subject_id",
    )
    .bind(&identity.context.installation_id)
    .bind(environment)
    .bind(SPACE_KIND)
    .bind(space_id)
    .fetch_all(pool)
    .await?)
}

pub async fn list_spaces(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<SpacesResponse>> {
    let readable = resource_ids(&state.acl, &identity, ACTION_READ).await?;
    let managed: HashSet<String> = resource_ids(&state.acl, &identity, ACTION_NOTICE_MANAGE)
        .await?
        .into_iter()
        .collect();
    let sql = format!(
        "SELECT {SPACE_COLUMNS} FROM board_spaces WHERE id = ANY($1) ORDER BY name ASC, created_at ASC"
    );
    let rows = sqlx::query_as::<_, Space>(&sql)
        .bind(&readable)
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(SpacesResponse {
        spaces: rows
            .iter()
            .map(|space| space_out(space, managed.contains(&space.id)))
            .collect(),
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceRequest {
    name: String,
    #[serde(default)]
    description: String,
}

fn validate_space(payload: &SpaceRequest) -> ApiResult<(String, String)> {
    check_length("name", &payload.name, 1, 120)?;
    check_length("description", &payload.description, 0, 5000)?;
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::validation("name: Value error, must not be blank"));
    }
    Ok((name, payload.description.trim().to_string()))
}

pub async fn create_space(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<SpaceRequest>,
) -> ApiResult<impl IntoResponse> {
    require_manager(&state.pool, &identity).await?;
    let (name, description) = validate_space(&payload)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_utc();
    let mut tx = state.pool.begin().await?;
    let sql = format!(
        "INSERT INTO board_spaces (id, name, description, created_by, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $5) RETURNING {SPACE_COLUMNS}"
    );
    let space = sqlx::query_as::<_, Space>(&sql)
        .bind(&id)
        .bind(&name)
        .bind(&description)
        .bind(&identity.user_id)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
    state
        .acl
        .create_resource_in(&mut tx, &identity, &space_key(&id), "admin")
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(space_out(&space, true))))
}

pub async fn update_space(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    ApiJson(payload): ApiJson<SpaceRequest>,
) -> ApiResult<Json<SpaceOut>> {
    let (name, description) = validate_space(&payload)?;
    load_space(&state.pool, &space_id)
        .await?
        .ok_or_else(space_not_found)?;
    let mut tx = state.pool.begin().await?;
    state
        .acl
        .require_in(&mut tx, &identity, &space_key(&space_id), MANAGE)
        .await?;
    let sql = format!(
        "UPDATE board_spaces SET name = $2, description = $3, \
         updated_at = CASE WHEN (name, description) IS DISTINCT FROM ($2, $3) THEN $4 ELSE updated_at END \
         WHERE id = $1 RETURNING {SPACE_COLUMNS}"
    );
    let space = sqlx::query_as::<_, Space>(&sql)
        .bind(&space_id)
        .bind(&name)
        .bind(&description)
        .bind(now_utc())
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(space_out(&space, true)))
}

pub async fn delete_space(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
) -> ApiResult<StatusCode> {
    load_space(&state.pool, &space_id)
        .await?
        .ok_or_else(space_not_found)?;
    let environment = identity
        .context
        .environment
        .clone()
        .ok_or_else(|| ApiError::forbidden("acl_denied", "acl_denied"))?;
    let mut tx = state.pool.begin().await?;
    state
        .acl
        .require_in(&mut tx, &identity, &space_key(&space_id), MANAGE)
        .await?;
    let notices: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM announcements WHERE workspace_id = $1 AND scope = 'workspace'",
    )
    .bind(&space_id)
    .fetch_all(&mut *tx)
    .await?;
    // Notices of the space go with it (foreign key ON DELETE CASCADE).
    sqlx::query("DELETE FROM board_spaces WHERE id = $1")
        .bind(&space_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "DELETE FROM app.miy_acl_resources WHERE installation_id = $1::uuid AND environment = $2 \
         AND kind = $3 AND resource_id = $4",
    )
    .bind(&identity.context.installation_id)
    .bind(&environment)
    .bind(SPACE_KIND)
    .bind(&space_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    for notice in notices {
        notify::retract(&identity, &notify::notice_group_key(&notice)).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_sharing(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
) -> ApiResult<Json<ShareSnapshot>> {
    load_space(&state.pool, &space_id)
        .await?
        .ok_or_else(space_not_found)?;
    Ok(Json(
        state.acl.sharing(&identity, &space_key(&space_id)).await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharingRequest {
    expected_revision: i64,
    request_id: String,
    entries: Vec<Grant>,
}

pub async fn replace_sharing(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    ApiJson(payload): ApiJson<SharingRequest>,
) -> ApiResult<Json<ShareSnapshot>> {
    load_space(&state.pool, &space_id)
        .await?
        .ok_or_else(space_not_found)?;
    let before: BTreeSet<String> = direct_user_members(&state.pool, &identity, &space_id)
        .await?
        .into_iter()
        .collect();
    let mut tx = state.pool.begin().await?;
    let snapshot = state
        .acl
        .replace_in(
            &mut tx,
            &identity,
            &space_key(&space_id),
            payload.expected_revision,
            &payload.request_id,
            &payload.entries,
        )
        .await?;
    tx.commit().await?;
    // People removed from the space lose its notices in the inbox too.
    let after: BTreeSet<String> = snapshot
        .entries
        .iter()
        .filter(|entry| entry.subject.kind == SubjectKind::User)
        .map(|entry| entry.subject.id.clone())
        .collect();
    let removed: Vec<String> = before.difference(&after).cloned().collect();
    if !removed.is_empty() {
        let notices: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM announcements WHERE workspace_id = $1 AND scope = 'workspace' \
             ORDER BY created_at DESC LIMIT 200",
        )
        .bind(&space_id)
        .fetch_all(&state.pool)
        .await?;
        for notice in notices {
            notify::retract_for(&identity, &notify::notice_group_key(&notice), &removed).await;
        }
    }
    Ok(Json(snapshot))
}

pub async fn sharing_result(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, request_id)): Path<(String, String)>,
) -> ApiResult<Json<Option<ShareSnapshot>>> {
    load_space(&state.pool, &space_id)
        .await?
        .ok_or_else(space_not_found)?;
    Ok(Json(
        state
            .acl
            .replacement_result(&identity, &space_key(&space_id), &request_id)
            .await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn roles_build_an_acl_with_member_as_the_default_role() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/unused")
            .unwrap();
        let acl = Acl::new(pool, acl_roles()).unwrap();
        drop(acl);
        let roles = acl_roles();
        assert!(
            roles[0]
                .actions
                .iter()
                .all(|a| roles[1].actions.contains(a))
        );
        assert!(!roles[0].actions.iter().any(|a| a == MANAGE));
    }
}
