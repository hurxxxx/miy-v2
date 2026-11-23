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
//!   space's shared boards, `viewer` may read them, and `admin` also manages them (rename, trash, share)
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
    error::{ApiError, ApiJson, ApiResult, ErrorResponse, IdentityErrorResponse, check_length},
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
            id: "viewer".into(),
            label: "보기 전용".into(),
            actions: vec![ACTION_READ.into()],
        },
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
    Viewer,
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

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SpaceOut {
    id: String,
    name: String,
    description: String,
    created_by: String,
    can_manage: bool,
    #[serde(serialize_with = "crate::time::serialize")]
    #[schema(value_type = String)]
    created_at: NaiveDateTime,
    #[serde(serialize_with = "crate::time::serialize")]
    #[schema(value_type = String)]
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

#[derive(Serialize, utoipa::ToSchema)]
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
    for (action, role) in ROLE_ACTIONS {
        match acl.require(identity, &key, action).await {
            Ok(()) => return Ok(role),
            Err(error) if is_denied(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(SpaceRole::None)
}

const ROLE_ACTIONS: [(&str, SpaceRole); 3] = [
    (ACTION_MANAGE_BOARDS, SpaceRole::Admin),
    (ACTION_EDIT, SpaceRole::Member),
    (ACTION_READ, SpaceRole::Viewer),
];

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
    for (action, role) in ROLE_ACTIONS {
        match acl.require_in(tx, identity, &key, action).await {
            Ok(()) => return Ok(role),
            Err(error) if is_denied(&error) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(SpaceRole::None)
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

#[utoipa::path(
    get, path = "/v1/spaces", operation_id = "listWhiteboardSpaces", tag = "Spaces",
    responses(
        (status = 200, description = "Spaces visible to the current user", body = SpacesResponse),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow space membership lookup", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
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

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SpaceRequest {
    /// The space name must contain a non-whitespace character.
    #[schema(min_length = 1, max_length = 120)]
    name: String,
    #[serde(default)]
    #[schema(max_length = 5000)]
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

#[utoipa::path(
    post, path = "/v1/spaces", operation_id = "createWhiteboardSpace", tag = "Spaces", request_body = SpaceRequest,
    responses(
        (status = 201, description = "Created space", body = SpaceOut),
        (status = 400, description = "The creator could not be added as an ACL subject", body = ErrorResponse),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = ErrorResponse),
        (status = 409, description = "An ACL resource already exists for the new ID", body = ErrorResponse),
        (status = 422, description = "Invalid space request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
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

#[utoipa::path(
    patch, path = "/v1/spaces/{space_id}", operation_id = "updateWhiteboardSpace", tag = "Spaces",
    params(("space_id" = String, Path)),
    request_body = SpaceRequest,
    responses(
        (status = 200, description = "Updated space", body = SpaceOut),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller cannot manage this space", body = ErrorResponse),
        (status = 404, description = "Space not found", body = ErrorResponse),
        (status = 422, description = "Invalid space request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
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
#[utoipa::path(
    delete, path = "/v1/spaces/{space_id}", operation_id = "deleteWhiteboardSpace", tag = "Spaces",
    params(("space_id" = String, Path)),
    responses(
        (status = 204, description = "Empty space deleted"),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller cannot manage this space", body = ErrorResponse),
        (status = 404, description = "Space not found", body = ErrorResponse),
        (status = 409, description = "The space still contains boards", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
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

#[utoipa::path(
    get, path = "/v1/spaces/{space_id}/sharing", operation_id = "getWhiteboardSpaceSharing", tag = "Spaces",
    params(("space_id" = String, Path)),
    responses(
        (status = 200, description = "Current space ACL and its revision", body = SpaceShareSnapshot),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller cannot manage this space", body = ErrorResponse),
        (status = 404, description = "Space not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
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

// The SDK owns Grant and ShareSnapshot serialization. These schemas describe
// that wire format without changing how the SDK accepts or returns ACL data.
#[allow(dead_code)]
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
enum SpaceSubjectKind {
    User,
    Group,
}

#[allow(dead_code)]
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct SpaceSubjectRef {
    kind: SpaceSubjectKind,
    id: String,
}

#[allow(dead_code)]
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct SpaceGrant {
    subject: SpaceSubjectRef,
    role: String,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
struct SpaceShareSubject {
    kind: SpaceSubjectKind,
    id: String,
    name: String,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
struct SpaceShareEntry {
    subject: SpaceShareSubject,
    role: String,
    protected: bool,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
struct SpaceShareRole {
    id: String,
    label: String,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[schema(rename_all = "camelCase")]
struct SpaceShareSnapshot {
    revision: i64,
    entries: Vec<SpaceShareEntry>,
    roles: Vec<SpaceShareRole>,
    #[schema(required = true)]
    default_role: Option<String>,
    can_manage: bool,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SharingRequest {
    #[schema(minimum = 1)]
    expected_revision: i64,
    #[schema(format = "uuid")]
    request_id: String,
    #[schema(value_type = Vec<SpaceGrant>)]
    entries: Vec<Grant>,
}

#[utoipa::path(
    put, path = "/v1/spaces/{space_id}/sharing", operation_id = "replaceWhiteboardSpaceSharing", tag = "Spaces",
    params(("space_id" = String, Path)),
    request_body = SharingRequest,
    responses(
        (status = 200, description = "Current space ACL and its revision", body = SpaceShareSnapshot),
        (status = 400, description = "Invalid ACL request, role or subject", body = ErrorResponse),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller cannot manage this space", body = ErrorResponse),
        (status = 404, description = "Space not found", body = ErrorResponse),
        (status = 409, description = "Request or revision conflict, protected owner, or last manager", body = ErrorResponse),
        (status = 422, description = "Invalid JSON request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
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

#[utoipa::path(
    get, path = "/v1/spaces/{space_id}/sharing/requests/{request_id}", operation_id = "getWhiteboardSpaceSharingResult", tag = "Spaces",
    params(("space_id" = String, Path), ("request_id" = String, Path, format = "uuid")),
    responses(
        (status = 200, description = "Recorded ACL replacement result, or null if it has not been recorded", body = Option<SpaceShareSnapshot>),
        (status = 400, description = "Invalid ACL request ID", body = ErrorResponse),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "The caller cannot manage this space", body = ErrorResponse),
        (status = 404, description = "Space not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
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
    async fn roles_build_an_acl_with_read_only_as_the_least_privileged_role() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/unused")
            .unwrap();
        let acl = Acl::new(pool, acl_roles()).unwrap();
        drop(acl);
        let roles = acl_roles();
        assert_eq!(roles[0].actions, [ACTION_READ]);
        assert!(
            roles[1]
                .actions
                .iter()
                .all(|a| roles[2].actions.contains(a))
        );
        assert!(!roles[1].actions.iter().any(|a| a == MANAGE));
        assert!(SpaceRole::Admin > SpaceRole::Member);
        assert!(SpaceRole::Member > SpaceRole::Viewer);
        assert!(SpaceRole::Viewer > SpaceRole::None);
    }
}
