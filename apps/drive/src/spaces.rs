//! Drive spaces (공간): the miy stand-in for AI-DO workspaces (access.rs).
//! Any person who may use the app creates a space and becomes its protected
//! owner (`admin`); admins rename it, manage its members with the common
//! ShareEditor (people and directory groups by name) and delete it.
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::NaiveDateTime;
use miy_app_sdk::{
    Identity,
    acl::{Grant, MANAGE, ShareSnapshot, SubjectKind, SubjectRef},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

use crate::{
    AppState,
    access::{self, Role, SPACE_KIND, Space, space_key, space_not_found},
    cleanup,
    error::{ApiError, ApiJson, ApiResult, check_length},
    notify, people,
    time::now_utc,
};

#[derive(Serialize, utoipa::ToSchema)]
pub struct SpaceOut {
    id: String,
    name: String,
    role: Role,
    can_manage: bool,
    can_upload: bool,
    created_by: String,
    #[serde(serialize_with = "crate::time::serialize")]
    #[schema(value_type = String)]
    created_at: NaiveDateTime,
    #[serde(serialize_with = "crate::time::serialize")]
    #[schema(value_type = String)]
    updated_at: NaiveDateTime,
}

fn space_out(space: &Space, role: Role) -> SpaceOut {
    SpaceOut {
        id: space.id.clone(),
        name: space.name.clone(),
        role,
        can_manage: role == Role::Admin,
        can_upload: role >= Role::Member,
        created_by: space.created_by.clone(),
        created_at: space.created_at,
        updated_at: space.updated_at,
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SpacesResponse {
    /// The caller (for the share editor's "you" marker).
    user_id: String,
    spaces: Vec<SpaceOut>,
}

/// `GET /v1/spaces`: the spaces the caller may open, by name.
#[utoipa::path(
    get, path = "/v1/spaces", operation_id = "listDriveSpaces", tag = "Spaces",
    responses(
        (status = 200, description = "Spaces visible to the current user", body = SpacesResponse),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "Directory service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_spaces(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<SpacesResponse>> {
    let groups = access::frame_groups(&identity).await?;
    let roles: HashMap<String, Role> = access::roles_by_space(&state.pool, &identity, &groups)
        .await?
        .into_iter()
        .collect();
    let ids: Vec<String> = roles.keys().cloned().collect();
    let sql = format!(
        "SELECT {} FROM drive_spaces WHERE id = ANY($1) ORDER BY name ASC, created_at ASC",
        access::SPACE_COLUMNS
    );
    let rows = sqlx::query_as::<_, Space>(&sql)
        .bind(&ids)
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(SpacesResponse {
        user_id: identity.user_id.clone(),
        spaces: rows
            .iter()
            .filter_map(|space| roles.get(&space.id).map(|role| space_out(space, *role)))
            .collect(),
    }))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SpaceRequest {
    #[schema(min_length = 1, max_length = 120)]
    name: String,
}

fn validate_name(payload: &SpaceRequest) -> ApiResult<String> {
    check_length("name", &payload.name, 1, 120)?;
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::unprocessable(
            "files.name_required",
            "Name is required.",
        ));
    }
    Ok(name)
}

/// `POST /v1/spaces`.
#[utoipa::path(
    post, path = "/v1/spaces", operation_id = "createDriveSpace", tag = "Spaces", request_body = SpaceRequest,
    responses(
        (status = 201, description = "Created space", body = SpaceOut),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 422, description = "Invalid space request", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "Resource ACL is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn create_space(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<SpaceRequest>,
) -> ApiResult<impl IntoResponse> {
    let name = validate_name(&payload)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_utc();
    let mut tx = state.pool.begin().await?;
    let sql = format!(
        "INSERT INTO drive_spaces (id, name, created_by, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $4) RETURNING {}",
        access::SPACE_COLUMNS
    );
    let space = sqlx::query_as::<_, Space>(&sql)
        .bind(&id)
        .bind(&name)
        .bind(&identity.user_id)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
    state
        .acl
        .create_resource_in(&mut tx, &identity, &space_key(&id), Role::Admin.id())
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(space_out(&space, Role::Admin))))
}

/// `PATCH /v1/spaces/{space_id}`: rename (space admins).
#[utoipa::path(
    patch, path = "/v1/spaces/{space_id}", operation_id = "renameDriveSpace", tag = "Spaces",
    request_body = SpaceRequest,
    params(("space_id" = String, Path)),
    responses(
        (status = 200, description = "Renamed space", body = SpaceOut),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 422, description = "Invalid or unsupported request", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn update_space(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    ApiJson(payload): ApiJson<SpaceRequest>,
) -> ApiResult<Json<SpaceOut>> {
    let name = validate_name(&payload)?;
    let mut tx = state.pool.begin().await?;
    access::lock_space(&mut tx, &space_id).await?;
    state
        .acl
        .require_in(&mut tx, &identity, &space_key(&space_id), MANAGE)
        .await?;
    let sql = format!(
        "UPDATE drive_spaces SET name = $2, \
         updated_at = CASE WHEN name IS DISTINCT FROM $2 THEN $3 ELSE updated_at END \
         WHERE id = $1 RETURNING {}",
        access::SPACE_COLUMNS
    );
    let space = sqlx::query_as::<_, Space>(&sql)
        .bind(&space_id)
        .bind(&name)
        .bind(now_utc())
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(space_out(&space, Role::Admin)))
}

/// `DELETE /v1/spaces/{space_id}` (space admins): the space, its folders,
/// files and trash. The Core files are released after the commit.
#[utoipa::path(
    delete, path = "/v1/spaces/{space_id}", operation_id = "deleteDriveSpace", tag = "Spaces",
    params(("space_id" = String, Path)),
    responses(
        (status = 204, description = "Space deleted"),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
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
        .ok_or_else(access::denied)?;
    let mut tx = state.pool.begin().await?;
    let sql = format!(
        "SELECT {} FROM drive_spaces WHERE id = $1 FOR UPDATE",
        access::SPACE_COLUMNS
    );
    sqlx::query_as::<_, Space>(&sql)
        .bind(&space_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(space_not_found)?;
    state
        .acl
        .require_in(&mut tx, &identity, &space_key(&space_id), MANAGE)
        .await?;
    let release = cleanup::enqueue_for_space(&mut tx, &space_id).await?;
    // Folders, files and versions go with the space (ON DELETE CASCADE).
    sqlx::query("DELETE FROM drive_spaces WHERE id = $1")
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
    notify::retract(&identity, &notify::space_group_key(&space_id)).await;
    cleanup::run(&identity, &state.pool, &release).await;
    Ok(StatusCode::NO_CONTENT)
}

/// The user or group subjects granted on a space (notification recipients).
async fn direct_subjects(
    state: &AppState,
    identity: &Identity,
    space_id: &str,
    subject_kind: &str,
) -> ApiResult<Vec<String>> {
    let Some(environment) = identity.context.environment.as_deref() else {
        return Ok(Vec::new());
    };
    Ok(sqlx::query_scalar(
        "SELECT subject_id FROM app.miy_acl_grants WHERE installation_id = $1::uuid AND environment = $2 \
         AND kind = $3 AND resource_id = $4 AND subject_kind = $5 ORDER BY subject_id",
    )
    .bind(&identity.context.installation_id)
    .bind(environment)
    .bind(SPACE_KIND)
    .bind(space_id)
    .bind(subject_kind)
    .fetch_all(&state.pool)
    .await?)
}

/// `GET /v1/spaces/{space_id}/sharing` (space admins).
#[utoipa::path(
    get, path = "/v1/spaces/{space_id}/sharing", operation_id = "getDriveSpaceSharing", tag = "Spaces",
    params(("space_id" = String, Path)),
    responses(
        (status = 200, description = "Sharing rules and current revision", body = SharingSnapshot),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Sharing management permission is required", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn get_sharing(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
) -> ApiResult<Json<SharingSnapshot>> {
    access::load_space(&state.pool, &space_id)
        .await?
        .ok_or_else(space_not_found)?;
    Ok(Json(
        state
            .acl
            .sharing(&identity, &space_key(&space_id))
            .await?
            .into(),
    ))
}

#[derive(Deserialize, Serialize, utoipa::ToSchema)]
#[serde(remote = "SubjectKind", rename_all = "lowercase")]
enum SharingSubjectKind {
    User,
    Group,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct SharingSubjectRef {
    #[serde(with = "SharingSubjectKind")]
    #[schema(value_type = SharingSubjectKind)]
    kind: SubjectKind,
    id: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct SharingGrant {
    subject: SharingSubjectRef,
    role: String,
}

impl From<SharingGrant> for Grant {
    fn from(value: SharingGrant) -> Self {
        Self {
            subject: SubjectRef {
                kind: value.subject.kind,
                id: value.subject.id,
            },
            role: value.role,
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
struct SharingSubject {
    #[serde(with = "SharingSubjectKind")]
    #[schema(value_type = SharingSubjectKind)]
    kind: SubjectKind,
    id: String,
    name: String,
}

#[derive(Serialize, utoipa::ToSchema)]
struct SharingEntry {
    subject: SharingSubject,
    role: String,
    protected: bool,
}

#[derive(Serialize, utoipa::ToSchema)]
struct SharingRole {
    id: String,
    label: String,
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SharingSnapshot {
    revision: i64,
    entries: Vec<SharingEntry>,
    roles: Vec<SharingRole>,
    #[schema(required = true)]
    default_role: Option<String>,
    can_manage: bool,
}

impl From<ShareSnapshot> for SharingSnapshot {
    fn from(value: ShareSnapshot) -> Self {
        Self {
            revision: value.revision,
            entries: value
                .entries
                .into_iter()
                .map(|entry| SharingEntry {
                    subject: SharingSubject {
                        kind: entry.subject.kind,
                        id: entry.subject.id,
                        name: entry.subject.name,
                    },
                    role: entry.role,
                    protected: entry.protected,
                })
                .collect(),
            roles: value
                .roles
                .into_iter()
                .map(|role| SharingRole {
                    id: role.id,
                    label: role.label,
                })
                .collect(),
            default_role: value.default_role,
            can_manage: value.can_manage,
        }
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SharingRequest {
    expected_revision: i64,
    request_id: String,
    entries: Vec<SharingGrant>,
}

/// `PUT /v1/spaces/{space_id}/sharing` (space admins). People and groups
/// added get a `share` notification; people taken off lose it.
#[utoipa::path(
    put, path = "/v1/spaces/{space_id}/sharing", operation_id = "replaceDriveSpaceSharing", tag = "Spaces",
    request_body = SharingRequest,
    params(("space_id" = String, Path)),
    responses(
        (status = 200, description = "Updated sharing snapshot", body = SharingSnapshot),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 400, description = "Malformed request", body = crate::error::ErrorResponse),
        (status = 403, description = "Sharing management permission is required", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 409, description = "ACL revision or request ID conflict", body = crate::error::ErrorResponse),
        (status = 422, description = "Invalid or unsupported request", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn replace_sharing(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    ApiJson(payload): ApiJson<SharingRequest>,
) -> ApiResult<Json<SharingSnapshot>> {
    let space = access::load_space(&state.pool, &space_id)
        .await?
        .ok_or_else(space_not_found)?;
    let before: BTreeSet<String> = direct_subjects(&state, &identity, &space_id, "user")
        .await?
        .into_iter()
        .collect();
    let before_groups: BTreeSet<String> = direct_subjects(&state, &identity, &space_id, "group")
        .await?
        .into_iter()
        .collect();
    let mut tx = state.pool.begin().await?;
    let grants: Vec<_> = payload.entries.into_iter().map(Grant::from).collect();
    let snapshot = state
        .acl
        .replace_in(
            &mut tx,
            &identity,
            &space_key(&space_id),
            payload.expected_revision,
            &payload.request_id,
            &grants,
        )
        .await?;
    tx.commit().await?;
    let subjects = |kind: SubjectKind| -> BTreeSet<String> {
        snapshot
            .entries
            .iter()
            .filter(|entry| entry.subject.kind == kind)
            .map(|entry| entry.subject.id.clone())
            .collect()
    };
    let after = subjects(SubjectKind::User);
    let after_groups = subjects(SubjectKind::Group);
    let added: Vec<String> = after.difference(&before).cloned().collect();
    let added_groups: Vec<String> = after_groups.difference(&before_groups).cloned().collect();
    let removed: Vec<String> = before.difference(&after).cloned().collect();
    let group_key = notify::space_group_key(&space_id);
    if !added.is_empty() || !added_groups.is_empty() {
        let names = people::display_names(&identity, [identity.user_id.clone()]).await;
        let title = notify::share_title(
            names.get(&identity.user_id).map(String::as_str),
            &space.name,
        );
        notify::publish(
            &identity,
            added,
            added_groups,
            notify::KIND_SHARE,
            &group_key,
            &title,
            &notify::space_path(&space_id),
            &space_id,
        )
        .await;
    }
    notify::retract_for(&identity, &group_key, &removed).await;
    Ok(Json(snapshot.into()))
}

/// `GET /v1/spaces/{space_id}/sharing/requests/{request_id}`: the result of a
/// replacement whose answer was lost.
#[utoipa::path(
    get, path = "/v1/spaces/{space_id}/sharing/requests/{request_id}", operation_id = "getDriveSharingResult", tag = "Spaces",
    params(("space_id" = String, Path), ("request_id" = String, Path)),
    responses(
        (status = 200, description = "Recorded result, or null when no result was recorded", body = Option<SharingSnapshot>),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 400, description = "Malformed request", body = crate::error::ErrorResponse),
        (status = 403, description = "Sharing management permission is required", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource was not found or is not visible", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn sharing_result(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, request_id)): Path<(String, String)>,
) -> ApiResult<Json<Option<SharingSnapshot>>> {
    access::load_space(&state.pool, &space_id)
        .await?
        .ok_or_else(space_not_found)?;
    Ok(Json(
        state
            .acl
            .replacement_result(&identity, &space_key(&space_id), &request_id)
            .await?
            .map(Into::into),
    ))
}
