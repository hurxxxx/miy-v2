//! Docs spaces (공간): the miy stand-in for AI-DO's app-owned workspaces of the
//! docs app (`Workspace.kind` `app_global` / `app_space`, owner app `docs`).
//!
//! - The global space (`id = 'global'`, seeded by migration 0003) is AI-DO's
//!   `app_global` docs workspace: every person who may use the app is a
//!   `member` there without a binding (AI-DO `decide_space_access`), and docs
//!   managers are its admins (AI-DO: an explicit admin binding).
//! - Other spaces are AI-DO's `app_space`s. Membership is the SDK resource ACL
//!   (kind `space`) with AI-DO's app-space roles `viewer`, `member` and
//!   `admin` (owner = the protected creator with `admin`). Docs managers create
//!   them (AI-DO `space_creation_role = "admin"`).
//!
//! A space role projects onto the docs whose primary target is that space's
//! sidebar (`docs/workspace_sidebar/<space id>`): viewer reads, member edits,
//! admin also manages and shares (AI-DO `project_app_workspace_target_access`).
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::NaiveDateTime;
use miy_app_sdk::{
    Identity,
    acl::{Acl, Grant, MANAGE, ResourceKey, RoleDefinition, ShareSnapshot},
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use std::collections::{HashMap, HashSet};

use crate::{
    AppState,
    error::{ApiError, ApiJson, ApiResult, check_length},
    files,
    managers::{is_manager, is_manager_locked, manager_required},
    time::now_utc,
};

pub const SPACE_KIND: &str = "space";
pub const GLOBAL_SPACE_ID: &str = "global";
pub const ACTION_READ: &str = "space.read";
pub const ACTION_WRITE: &str = "doc.write";
pub const ACTION_ADMIN: &str = "space.admin";

pub fn acl_roles() -> Vec<RoleDefinition> {
    vec![
        RoleDefinition {
            id: "viewer".into(),
            label: "보기".into(),
            actions: vec![ACTION_READ.into()],
        },
        RoleDefinition {
            id: "member".into(),
            label: "멤버".into(),
            actions: vec![ACTION_READ.into(), ACTION_WRITE.into()],
        },
        RoleDefinition {
            id: "admin".into(),
            label: "관리자".into(),
            actions: vec![
                ACTION_READ.into(),
                ACTION_WRITE.into(),
                ACTION_ADMIN.into(),
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

/// AI-DO `AppSpaceRole` without `owner` (the ACL's protected creator).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SpaceRole {
    Viewer,
    Member,
    Admin,
}

impl SpaceRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Member => "member",
            Self::Admin => "admin",
        }
    }

    fn from_grant(role: &str) -> Option<Self> {
        match role {
            "viewer" => Some(Self::Viewer),
            "member" => Some(Self::Member),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, FromRow)]
pub struct Space {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub description: String,
    pub created_by: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl Space {
    pub fn is_global(&self) -> bool {
        self.kind == "app_global"
    }
}

pub const SPACE_COLUMNS: &str = "id, kind, name, description, created_by, created_at, updated_at";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceOut {
    id: String,
    key: String,
    kind: String,
    name: String,
    description: String,
    created_by: Option<String>,
    role: &'static str,
    can_manage: bool,
    #[serde(serialize_with = "crate::time::serialize")]
    created_at: NaiveDateTime,
    #[serde(serialize_with = "crate::time::serialize")]
    updated_at: NaiveDateTime,
}

pub fn space_out(space: &Space, role: SpaceRole) -> SpaceOut {
    SpaceOut {
        id: space.id.clone(),
        key: space.id.clone(),
        kind: space.kind.clone(),
        name: space.name.clone(),
        description: space.description.clone(),
        created_by: space.created_by.clone(),
        role: role.as_str(),
        // Members of the global space are managed by the platform (app Use);
        // its admins are the docs managers.
        can_manage: role == SpaceRole::Admin && !space.is_global(),
        created_at: space.created_at,
        updated_at: space.updated_at,
    }
}

pub fn space_not_found() -> ApiError {
    ApiError::not_found("workspace.not_found", "Workspace not found.")
}

/// AI-DO `docs.requests_workspace_context_required` (403): the caller has no
/// role in the space the request names.
pub fn space_access_required() -> ApiError {
    ApiError::forbidden(
        "docs.requests_workspace_context_required",
        "Docs requests require a workspace context.",
    )
}

pub async fn load_space(pool: &PgPool, space_id: &str) -> ApiResult<Option<Space>> {
    let sql = format!("SELECT {SPACE_COLUMNS} FROM docs_spaces WHERE id = $1");
    Ok(sqlx::query_as::<_, Space>(&sql)
        .bind(space_id)
        .fetch_optional(pool)
        .await?)
}

/// `Ok(true)` when the ACL allows `action`, `Ok(false)` when it denies it, the
/// error when the check itself failed (directory unavailable).
async fn allows(acl: &Acl, identity: &Identity, space_id: &str, action: &str) -> ApiResult<bool> {
    match acl.require(identity, &space_key(space_id), action).await {
        Ok(()) => Ok(true),
        Err(error) if error.code == "acl_denied" => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// The caller's role in a space for a frame request (AI-DO
/// `resolve_space_access(...).role`); `None` when they are not a member.
pub async fn space_role(
    state: &AppState,
    identity: &Identity,
    space: &Space,
) -> ApiResult<Option<SpaceRole>> {
    if identity.context.platform {
        return platform_space_role(&state.pool, identity, space).await;
    }
    if space.is_global() {
        return Ok(Some(if is_manager(&state.pool, &identity.user_id).await? {
            SpaceRole::Admin
        } else {
            SpaceRole::Member
        }));
    }
    if allows(&state.acl, identity, &space.id, ACTION_ADMIN).await? {
        return Ok(Some(SpaceRole::Admin));
    }
    if allows(&state.acl, identity, &space.id, ACTION_WRITE).await? {
        return Ok(Some(SpaceRole::Member));
    }
    if allows(&state.acl, identity, &space.id, ACTION_READ).await? {
        return Ok(Some(SpaceRole::Viewer));
    }
    Ok(None)
}

async fn allows_in(
    tx: &mut Transaction<'_, Postgres>,
    acl: &Acl,
    identity: &Identity,
    space_id: &str,
    action: &str,
) -> ApiResult<bool> {
    match acl
        .require_in(tx, identity, &space_key(space_id), action)
        .await
    {
        Ok(()) => Ok(true),
        Err(error) if error.code == "acl_denied" => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// The caller's role decided again inside a write transaction: the ACL
/// resource share lock (`require_in`) or the manager row lock is held until
/// the write commits, so a concurrent removal cannot slip in between.
pub async fn space_role_in(
    tx: &mut Transaction<'_, Postgres>,
    state: &AppState,
    identity: &Identity,
    space: &Space,
) -> ApiResult<Option<SpaceRole>> {
    if space.is_global() {
        return Ok(Some(if is_manager_locked(tx, &identity.user_id).await? {
            SpaceRole::Admin
        } else {
            SpaceRole::Member
        }));
    }
    for (action, role) in [
        (ACTION_ADMIN, SpaceRole::Admin),
        (ACTION_WRITE, SpaceRole::Member),
        (ACTION_READ, SpaceRole::Viewer),
    ] {
        if allows_in(tx, &state.acl, identity, &space.id, action).await? {
            return Ok(Some(role));
        }
    }
    Ok(None)
}

/// A platform call carries no token for directory calls; the caller's role
/// comes from their user grants and the grants of the groups the core signed
/// into the call (docs/PORTING.md §3.2), read straight from the ACL tables.
pub async fn platform_space_role(
    pool: &PgPool,
    identity: &Identity,
    space: &Space,
) -> ApiResult<Option<SpaceRole>> {
    if space.is_global() {
        return Ok(Some(if is_manager(pool, &identity.user_id).await? {
            SpaceRole::Admin
        } else {
            SpaceRole::Member
        }));
    }
    Ok(direct_roles_of(pool, identity)
        .await?
        .get(&space.id)
        .copied())
}

/// The caller's space roles by space ID on a platform call: their user
/// grants and the grants of the groups the core signed into the call
/// (`notify::signed_groups`), the highest role per space.
pub async fn direct_roles_of(
    pool: &PgPool,
    identity: &Identity,
) -> ApiResult<HashMap<String, SpaceRole>> {
    let Some(environment) = identity.context.environment.as_deref() else {
        return Ok(HashMap::new());
    };
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT resource_id, role FROM app.miy_acl_grants WHERE installation_id = $1::uuid \
         AND environment = $2 AND kind = $3 \
         AND ((subject_kind = 'user' AND subject_id = $4) \
           OR (subject_kind = 'group' AND subject_id = ANY($5::text[])))",
    )
    .bind(&identity.context.installation_id)
    .bind(environment)
    .bind(SPACE_KIND)
    .bind(&identity.user_id)
    .bind(crate::notify::signed_groups(identity))
    .fetch_all(pool)
    .await?;
    let mut roles: HashMap<String, SpaceRole> = HashMap::new();
    for (id, role) in rows {
        if let Some(role) = SpaceRole::from_grant(&role) {
            let entry = roles.entry(id).or_insert(role);
            if role > *entry {
                *entry = role;
            }
        }
    }
    Ok(roles)
}

/// Every space ID the caller can act on, in the ACL's own SQL filter.
async fn resource_ids(acl: &Acl, identity: &Identity, action: &str) -> ApiResult<HashSet<String>> {
    let mut ids = HashSet::new();
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

/// The spaces the caller may enter with their role: the global space first,
/// then the others by name (AI-DO `resolve_space_roles`).
pub async fn enterable_spaces(
    state: &AppState,
    identity: &Identity,
) -> ApiResult<Vec<(Space, SpaceRole)>> {
    let sql = format!(
        "SELECT {SPACE_COLUMNS} FROM docs_spaces ORDER BY (kind = 'app_global') DESC, name, created_at"
    );
    let all = sqlx::query_as::<_, Space>(&sql)
        .fetch_all(&state.pool)
        .await?;
    let (readable, writable, admin) = if identity.context.platform {
        let direct = direct_roles_of(&state.pool, identity).await?;
        let pick = |min: SpaceRole| -> HashSet<String> {
            direct
                .iter()
                .filter(|(_, role)| **role >= min)
                .map(|(id, _)| id.clone())
                .collect()
        };
        (
            pick(SpaceRole::Viewer),
            pick(SpaceRole::Member),
            pick(SpaceRole::Admin),
        )
    } else {
        (
            resource_ids(&state.acl, identity, ACTION_READ).await?,
            resource_ids(&state.acl, identity, ACTION_WRITE).await?,
            resource_ids(&state.acl, identity, ACTION_ADMIN).await?,
        )
    };
    let manager = is_manager(&state.pool, &identity.user_id).await?;
    let mut spaces = Vec::new();
    for space in all {
        let role = if space.is_global() {
            Some(if manager {
                SpaceRole::Admin
            } else {
                SpaceRole::Member
            })
        } else if admin.contains(&space.id) {
            Some(SpaceRole::Admin)
        } else if writable.contains(&space.id) {
            Some(SpaceRole::Member)
        } else if readable.contains(&space.id) {
            Some(SpaceRole::Viewer)
        } else {
            None
        };
        if let Some(role) = role {
            spaces.push((space, role));
        }
    }
    Ok(spaces)
}

/// The space named by a request (`?space=`, the global space when absent) and
/// the caller's role there; 404 for an unknown space, 403 for a non-member.
pub async fn require_space(
    state: &AppState,
    identity: &Identity,
    space_id: Option<&str>,
) -> ApiResult<(Space, SpaceRole)> {
    let space_id = space_id
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or(GLOBAL_SPACE_ID);
    let space = load_space(&state.pool, space_id)
        .await?
        .ok_or_else(space_not_found)?;
    let role = space_role(state, identity, &space)
        .await?
        .ok_or_else(space_access_required)?;
    Ok((space, role))
}

// ---------------------------------------------------------------- routes

#[derive(Serialize)]
pub struct SpacesResponse {
    spaces: Vec<SpaceOut>,
}

pub async fn list_spaces(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<SpacesResponse>> {
    let spaces = enterable_spaces(&state, &identity).await?;
    Ok(Json(SpacesResponse {
        spaces: spaces
            .iter()
            .map(|(space, role)| space_out(space, *role))
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
    if !is_manager_locked(&mut tx, &identity.user_id).await? {
        return Err(manager_required());
    }
    let sql = format!(
        "INSERT INTO docs_spaces (id, kind, name, description, created_by, created_at, updated_at) \
         VALUES ($1, 'app_space', $2, $3, $4, $5, $5) RETURNING {SPACE_COLUMNS}"
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
    Ok((
        StatusCode::CREATED,
        Json(space_out(&space, SpaceRole::Admin)),
    ))
}

async fn load_named_space(pool: &PgPool, space_id: &str) -> ApiResult<Space> {
    let space = load_space(pool, space_id)
        .await?
        .ok_or_else(space_not_found)?;
    if space.is_global() {
        // The global space has no ACL: its members are the app's users.
        return Err(ApiError::bad_request(
            "docs.global_space_protected",
            "The global docs space cannot be changed here.",
        ));
    }
    Ok(space)
}

pub async fn update_space(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    ApiJson(payload): ApiJson<SpaceRequest>,
) -> ApiResult<Json<SpaceOut>> {
    let (name, description) = validate_space(&payload)?;
    load_named_space(&state.pool, &space_id).await?;
    let mut tx = state.pool.begin().await?;
    state
        .acl
        .require_in(&mut tx, &identity, &space_key(&space_id), MANAGE)
        .await?;
    let sql = format!(
        "UPDATE docs_spaces SET name = $2, description = $3, \
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
    Ok(Json(space_out(&space, SpaceRole::Admin)))
}

/// Delete a space. A space that still holds live docs is refused (409); the
/// trashed docs left in it go with it, and their files are released in the
/// core (AI-DO trashed a workspace instead; miy has no workspace trash).
pub async fn delete_space(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
) -> ApiResult<StatusCode> {
    load_named_space(&state.pool, &space_id).await?;
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
    let live: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM docs_native_docs WHERE workspace_id = $1 AND trashed_at IS NULL)",
    )
    .bind(&space_id)
    .fetch_one(&mut *tx)
    .await?;
    if live {
        return Err(ApiError::conflict(
            "docs.space_not_empty",
            "The space still holds docs. Move or delete them first.",
        ));
    }
    let page_ids: Vec<String> = sqlx::query_scalar(
        "SELECT p.id FROM docs_native_doc_pages p JOIN docs_native_docs d ON d.id = p.doc_id \
         WHERE d.workspace_id = $1",
    )
    .bind(&space_id)
    .fetch_all(&mut *tx)
    .await?;
    let released = files::release_for_pages(&mut tx, &page_ids).await?;
    sqlx::query("DELETE FROM docs_spaces WHERE id = $1")
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
    files::detach_released(&state.pool, &identity, &released).await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_sharing(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
) -> ApiResult<Json<ShareSnapshot>> {
    load_named_space(&state.pool, &space_id).await?;
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
    load_named_space(&state.pool, &space_id).await?;
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
    // People removed from the space lose its comment notifications.
    crate::comments::retract_lost_access_in_space(&state.pool, &identity, &space_id).await;
    Ok(Json(snapshot))
}

pub async fn sharing_result(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, request_id)): Path<(String, String)>,
) -> ApiResult<Json<Option<ShareSnapshot>>> {
    load_named_space(&state.pool, &space_id).await?;
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
    async fn roles_build_an_acl_with_viewer_as_the_default_role() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/unused")
            .unwrap();
        assert!(Acl::new(pool, acl_roles()).is_ok());
        let roles = acl_roles();
        for role in &roles[1..] {
            assert!(roles[0].actions.iter().all(|a| role.actions.contains(a)));
        }
        assert!(!roles[0].actions.iter().any(|a| a == MANAGE));
        assert!(roles[2].actions.iter().any(|a| a == MANAGE));
    }

    #[test]
    fn roles_are_ordered_like_ai_do_app_space_roles() {
        assert!(SpaceRole::Viewer < SpaceRole::Member);
        assert!(SpaceRole::Member < SpaceRole::Admin);
        assert_eq!(SpaceRole::from_grant("member"), Some(SpaceRole::Member));
        assert_eq!(SpaceRole::from_grant("owner"), None);
    }
}
