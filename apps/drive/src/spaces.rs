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
    acl::{Grant, MANAGE, ShareSnapshot, SubjectKind},
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

#[derive(Serialize)]
pub struct SpaceOut {
    id: String,
    name: String,
    role: &'static str,
    can_manage: bool,
    can_upload: bool,
    created_by: String,
    #[serde(serialize_with = "crate::time::serialize")]
    created_at: NaiveDateTime,
    #[serde(serialize_with = "crate::time::serialize")]
    updated_at: NaiveDateTime,
}

fn space_out(space: &Space, role: Role) -> SpaceOut {
    SpaceOut {
        id: space.id.clone(),
        name: space.name.clone(),
        role: role.id(),
        can_manage: role == Role::Admin,
        can_upload: role >= Role::Member,
        created_by: space.created_by.clone(),
        created_at: space.created_at,
        updated_at: space.updated_at,
    }
}

#[derive(Serialize)]
pub struct SpacesResponse {
    /// The caller (for the share editor's "you" marker).
    user_id: String,
    spaces: Vec<SpaceOut>,
}

/// `GET /v1/spaces`: the spaces the caller may open, by name.
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

#[derive(Deserialize)]
pub struct SpaceRequest {
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
pub async fn get_sharing(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
) -> ApiResult<Json<ShareSnapshot>> {
    access::load_space(&state.pool, &space_id)
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

/// `PUT /v1/spaces/{space_id}/sharing` (space admins). People and groups
/// added get a `share` notification; people taken off lose it.
pub async fn replace_sharing(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    ApiJson(payload): ApiJson<SharingRequest>,
) -> ApiResult<Json<ShareSnapshot>> {
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
    Ok(Json(snapshot))
}

/// `GET /v1/spaces/{space_id}/sharing/requests/{request_id}`: the result of a
/// replacement whose answer was lost.
pub async fn sharing_result(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((space_id, request_id)): Path<(String, String)>,
) -> ApiResult<Json<Option<ShareSnapshot>>> {
    access::load_space(&state.pool, &space_id)
        .await?
        .ok_or_else(space_not_found)?;
    Ok(Json(
        state
            .acl
            .replacement_result(&identity, &space_key(&space_id), &request_id)
            .await?,
    ))
}
