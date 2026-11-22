//! Whiteboard spaces (공간): the miy stand-in for AI-DO workspaces.
//!
//! AI-DO ran the whiteboard in app-owned mode: one `app_global` space where
//! every person with app access is a member, plus `app_space` workspaces with
//! member/admin bindings. Here:
//!
//! - [`GLOBAL`] (`"global"`) is the app-global space. Every person who may use
//!   the app is a `member` (AI-DO `decide_space_access`: no binding in the
//!   global space means member). Nobody is its admin: AI-DO's admin bindings
//!   there were given by platform administrators, and the platform admin flag
//!   grants nothing in miy.
//! - A row of `whiteboard_spaces` is an app space. Its members are the SDK
//!   resource ACL (kind `space`) with AI-DO's two roles: `member` edits the
//!   space's shared boards, `admin` also manages them (rename, trash, share)
//!   and the members. Any app user may create a space and becomes its
//!   protected admin.
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::NaiveDateTime;
use miy_app_sdk::{
    Identity,
    acl::{Acl, AclError, Grant, MANAGE, ResourceKey, RoleDefinition, ShareSnapshot},
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use std::collections::HashSet;

use crate::{
    AppState,
    error::{ApiError, ApiJson, ApiResult, check_length},
    time::now_utc,
};

pub const GLOBAL: &str = "global";
pub const SPACE_KIND: &str = "space";
pub const ACTION_READ: &str = "space.read";
pub const ACTION_EDIT: &str = "whiteboard.edit";
pub const ACTION_MANAGE_BOARDS: &str = "whiteboard.manage";

pub fn acl_roles() -> Vec<RoleDefinition> {
    vec![
        RoleDefinition {
            id: "member".into(),
            label: "멤버".into(),
            actions: vec![ACTION_READ.into(), ACTION_EDIT.into()],
        },
        RoleDefinition {
            id: "admin".into(),
            label: "관리자".into(),
            actions: vec![
                ACTION_READ.into(),
                ACTION_EDIT.into(),
                ACTION_MANAGE_BOARDS.into(),
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

/// The caller's role in a space (AI-DO `app_space_role_allows`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpaceRole {
    None,
    Member,
    Admin,
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

/// AI-DO `workspace.not_found`.
pub fn space_not_found() -> ApiError {
    ApiError::not_found("workspace.not_found", "Workspace not found.")
}

pub async fn load_space<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    space_id: &str,
) -> ApiResult<Option<Space>> {
    let sql = format!("SELECT {SPACE_COLUMNS} FROM whiteboard_spaces WHERE id = $1");
    Ok(sqlx::query_as::<_, Space>(&sql)
        .bind(space_id)
        .fetch_optional(executor)
        .await?)
}

/// The space exists (the global space always does).
pub async fn ensure_exists(pool: &PgPool, space_id: &str) -> ApiResult<()> {
    if space_id == GLOBAL || load_space(pool, space_id).await?.is_some() {
        Ok(())
    } else {
        Err(space_not_found())
    }
}

fn is_denied(error: &AclError) -> bool {
    error.code == "acl_denied"
}

/// The caller's current role. A failed directory lookup is an error, never a
/// lower role taken for the answer.
pub async fn role(acl: &Acl, identity: &Identity, space_id: &str) -> ApiResult<SpaceRole> {
    if space_id == GLOBAL {
        return Ok(SpaceRole::Member);
    }
    let key = space_key(space_id);
    match acl.require(identity, &key, ACTION_MANAGE_BOARDS).await {
        Ok(()) => return Ok(SpaceRole::Admin),
        Err(error) if is_denied(&error) => {}
        Err(error) => return Err(error.into()),
    }
    match acl.require(identity, &key, ACTION_READ).await {
        Ok(()) => Ok(SpaceRole::Member),
        Err(error) if is_denied(&error) => Ok(SpaceRole::None),
        Err(error) => Err(error.into()),
    }
}

/// [`role`] inside a write transaction: the ACL share lock is held until the
/// business write commits, so a concurrent member change waits for it.
pub async fn role_in(
    tx: &mut Transaction<'_, Postgres>,
    acl: &Acl,
    identity: &Identity,
    space_id: &str,
) -> ApiResult<SpaceRole> {
    if space_id == GLOBAL {
        return Ok(SpaceRole::Member);
    }
    let key = space_key(space_id);
    match acl
        .require_in(tx, identity, &key, ACTION_MANAGE_BOARDS)
        .await
    {
        Ok(()) => return Ok(SpaceRole::Admin),
        Err(error) if is_denied(&error) => {}
        Err(error) => return Err(error.into()),
    }
    match acl.require_in(tx, identity, &key, ACTION_READ).await {
        Ok(()) => Ok(SpaceRole::Member),
        Err(error) if is_denied(&error) => Ok(SpaceRole::None),
        Err(error) => Err(error.into()),
    }
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

/// Is `user_id` a direct user member of the space? Used where no directory
/// lookup can answer for another person: the directory reports group
/// memberships of the caller only, so a share target in an app space must be
/// a direct member (docs/PORTING.md §3.2).
pub async fn is_direct_member<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    identity: &Identity,
    space_id: &str,
    user_id: &str,
) -> ApiResult<bool> {
    if space_id == GLOBAL {
        return Ok(true);
    }
    let Some(environment) = identity.context.environment.as_deref() else {
        return Ok(false);
    };
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM app.miy_acl_grants WHERE installation_id = $1::uuid \
         AND environment = $2 AND kind = $3 AND resource_id = $4 AND subject_kind = 'user' \
         AND subject_id = $5)",
    )
    .bind(&identity.context.installation_id)
    .bind(environment)
    .bind(SPACE_KIND)
    .bind(space_id)
    .bind(user_id)
    .fetch_one(executor)
    .await?)
}

pub async fn list_spaces(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<SpacesResponse>> {
    let readable = resource_ids(&state.acl, &identity, ACTION_READ).await?;
    let managed: HashSet<String> = resource_ids(&state.acl, &identity, MANAGE)
        .await?
        .into_iter()
        .collect();
    let sql = format!(
        "SELECT {SPACE_COLUMNS} FROM whiteboard_spaces WHERE id = ANY($1) ORDER BY name ASC, created_at ASC"
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
    let (name, description) = validate_space(&payload)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_utc();
    let mut tx = state.pool.begin().await?;
    let sql = format!(
        "INSERT INTO whiteboard_spaces (id, name, description, created_by, created_at, updated_at) \
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
    let mut tx = state.pool.begin().await?;
    load_space(&mut *tx, &space_id)
        .await?
        .ok_or_else(space_not_found)?;
    state
        .acl
        .require_in(&mut tx, &identity, &space_key(&space_id), MANAGE)
        .await?;
    let sql = format!(
        "UPDATE whiteboard_spaces SET name = $2, description = $3, \
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

/// A space that still holds boards (trashed ones too) is not deleted: its
/// boards' images and shares would have to go with it. Empty the space first
/// (docs/PORTING.md §4).
pub async fn delete_space(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
) -> ApiResult<StatusCode> {
    let environment = identity
        .context
        .environment
        .clone()
        .ok_or_else(|| ApiError::forbidden("acl_denied", "acl_denied"))?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("SELECT id FROM whiteboard_spaces WHERE id = $1 FOR UPDATE")
        .bind(&space_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(space_not_found)?;
    state
        .acl
        .require_in(&mut tx, &identity, &space_key(&space_id), MANAGE)
        .await?;
    let boards: i64 =
        sqlx::query_scalar("SELECT count(*) FROM whiteboards WHERE workspace_id = $1")
            .bind(&space_id)
            .fetch_one(&mut *tx)
            .await?;
    if boards > 0 {
        return Err(ApiError::conflict(
            "whiteboard.space_not_empty",
            "Delete or move the space's whiteboards first.",
        ));
    }
    sqlx::query("DELETE FROM whiteboard_spaces WHERE id = $1")
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
        assert!(SpaceRole::Admin > SpaceRole::Member && SpaceRole::Member > SpaceRole::None);
    }
}
