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

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SpaceOut {
    id: String,
    name: String,
    description: String,
    created_by: String,
    can_manage: bool,
    #[serde(serialize_with = "crate::time::serialize")]
    #[schema(value_type = String, example = "2026-05-04T01:02:03")]
    created_at: NaiveDateTime,
    #[serde(serialize_with = "crate::time::serialize")]
    #[schema(value_type = String, example = "2026-05-04T01:02:03")]
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

/// Roles of the space ACL whose actions include `action` (both roles read).
pub fn roles_with(action: &str) -> Vec<String> {
    acl_roles()
        .into_iter()
        .filter(|role| role.actions.iter().any(|a| a == action))
        .map(|role| role.id)
        .collect()
}

/// The person's current group IDs that the Core signed into a platform call
/// (`identity.context.groups`, present when the app is approved for
/// `platform:directory:read`). A platform call cannot ask the directory, so
/// code that reads the ACL tables itself uses these, as the SDK ACL does.
/// Empty for other calls and without the approval: user grants only.
pub fn signed_groups(identity: &Identity) -> ApiResult<Vec<String>> {
    if !identity.context.platform {
        return Ok(Vec::new());
    }
    let groups = identity.context.groups.clone().unwrap_or_default();
    // The SDK ACL refuses the same malformed or oversized values.
    if groups.len() > 4096 || groups.iter().any(|g| uuid::Uuid::parse_str(g).is_err()) {
        return Err(ApiError::unavailable(
            "acl_directory_unavailable",
            "acl_directory_unavailable",
        ));
    }
    let mut groups: Vec<String> = groups.iter().map(|g| g.to_ascii_lowercase()).collect();
    groups.sort();
    groups.dedup();
    Ok(groups)
}

/// SQL condition: grant row `g` lets the person read the space. Binds: `$u`
/// the user ID, `$g` the signed group IDs (text[]), `$r` the reading roles.
pub fn grant_reads_sql(user: &str, groups: &str, roles: &str) -> String {
    format!(
        "g.role = ANY({roles}) AND ((g.subject_kind = 'user' AND g.subject_id = {user}) \
         OR (g.subject_kind = 'group' AND g.subject_id = ANY({groups})))"
    )
}

/// Group subjects of a space's ACL (any role), for notifications sent to
/// their current members (`Identity::notify_with_groups`).
pub async fn group_members(
    pool: &PgPool,
    identity: &Identity,
    space_id: &str,
) -> ApiResult<Vec<String>> {
    let Some(environment) = identity.context.environment.as_deref() else {
        return Ok(Vec::new());
    };
    Ok(sqlx::query_scalar(
        "SELECT subject_id FROM app.miy_acl_grants WHERE installation_id = $1::uuid AND environment = $2 \
         AND kind = $3 AND resource_id = $4 AND subject_kind = 'group' ORDER BY subject_id",
    )
    .bind(&identity.context.installation_id)
    .bind(environment)
    .bind(SPACE_KIND)
    .bind(space_id)
    .fetch_all(pool)
    .await?)
}

/// Direct user grants of a space (any role): notification recipients and the
/// people whose notices are retracted when they leave the space. Group
/// subjects are reached through `group_members`.
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

#[utoipa::path(
    get, path = "/v1/spaces", operation_id = "listBoardSpaces",
    responses(
        (status = 200, description = "Success", body = SpacesResponse),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 503, description = "A required service is unavailable", body = crate::error::ErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
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

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SpaceRequest {
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
    post, path = "/v1/spaces", operation_id = "createBoardSpace",
    request_body = SpaceRequest,
    responses(
        (status = 201, description = "Created", body = SpaceOut),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 400, description = "Invalid request for this resource", body = crate::error::ErrorResponse),
        (status = 409, description = "Current state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required service is unavailable", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn create_space(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<SpaceRequest>,
) -> ApiResult<(StatusCode, Json<SpaceOut>)> {
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

#[utoipa::path(
    patch, path = "/v1/spaces/{space_id}", operation_id = "updateBoardSpace",
    params(
        ("space_id" = String, Path),
    ),
    request_body = SpaceRequest,
    responses(
        (status = 200, description = "Success", body = SpaceOut),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 503, description = "A required service is unavailable", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
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

#[utoipa::path(
    delete, path = "/v1/spaces/{space_id}", operation_id = "deleteBoardSpace",
    params(
        ("space_id" = String, Path),
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

#[utoipa::path(
    get, path = "/v1/spaces/{space_id}/sharing", operation_id = "getBoardSpaceSharing",
    params(
        ("space_id" = String, Path),
    ),
    responses(
        (status = 200, description = "Success", body = SharingSnapshot),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 503, description = "A required service is unavailable", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
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

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SharingRequest {
    #[schema(minimum = 1)]
    expected_revision: i64,
    #[schema(format = "uuid")]
    request_id: String,
    #[schema(value_type = Vec<SharingGrant>, min_items = 1, max_items = 100)]
    entries: Vec<Grant>,
}

// The shared ACL SDK provides the wire DTOs. These schema definitions describe
// the SDK's serialized types without changing its request-validation rules.
#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
enum SharingSubjectKind {
    User,
    Group,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct SharingSubjectRef {
    kind: SharingSubjectKind,
    id: String,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct SharingGrant {
    subject: SharingSubjectRef,
    role: String,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
struct SharingSubject {
    kind: SharingSubjectKind,
    id: String,
    name: String,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
struct SharingEntry {
    subject: SharingSubject,
    role: String,
    protected: bool,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
struct SharingRole {
    id: String,
    label: String,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
struct SharingSnapshot {
    revision: i64,
    entries: Vec<SharingEntry>,
    roles: Vec<SharingRole>,
    #[schema(required = true)]
    default_role: Option<String>,
    can_manage: bool,
}

#[utoipa::path(
    put, path = "/v1/spaces/{space_id}/sharing", operation_id = "replaceBoardSpaceSharing",
    params(
        ("space_id" = String, Path),
    ),
    request_body = SharingRequest,
    responses(
        (status = 200, description = "Success", body = SharingSnapshot),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 409, description = "Current state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 503, description = "A required service is unavailable", body = crate::error::ErrorResponse),
        (status = 400, description = "Invalid request for this resource", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
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

#[utoipa::path(
    get, path = "/v1/spaces/{space_id}/sharing/requests/{request_id}", operation_id = "getBoardSpaceSharingResult",
    params(
        ("space_id" = String, Path),
        ("request_id" = String, Path),
    ),
    responses(
        (status = 200, description = "Success", body = Option<SharingSnapshot>),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 503, description = "A required service is unavailable", body = crate::error::ErrorResponse),
        (status = 400, description = "Invalid request for this resource", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
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

    #[test]
    fn both_roles_read_and_only_admin_manages_notices() {
        assert_eq!(roles_with(ACTION_READ), vec!["member", "admin"]);
        assert_eq!(roles_with(ACTION_NOTICE_MANAGE), vec!["admin"]);
    }

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
