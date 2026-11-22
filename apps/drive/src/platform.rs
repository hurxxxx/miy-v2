//! Platform → app endpoints declared in manifest.json `platformEndpoints`.
//! Each request carries the person's signed identity with `platform == true`
//! and no token for further platform calls; they read only.
use axum::{Extension, Json, extract::State};
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::{AppState, access, error::ApiResult, notify, search::platform_only, tree, versions};

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileAccessRequest {
    file_id: String,
    #[serde(default)]
    reference: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct FileAccessResponse {
    allowed: bool,
}

/// `POST /v1/file-access` (platformEndpoints.fileAccess): before sending the
/// bytes of a download link, may this person still read the file? The same
/// rule as the screen (`require_file_access`): the file is not in the trash,
/// the person still holds a role in its space (directly or through the
/// current groups the call carries), the file is visible to them and every
/// folder above it is.
#[utoipa::path(
    post, path = "/v1/file-access", operation_id = "checkDriveDownloadAccess", tag = "Platform callbacks",
    request_body = FileAccessRequest,
    responses(
        (status = 200, description = "Current permission to stream this Storage file", body = FileAccessResponse),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 400, description = "Malformed request", body = String, content_type = "text/plain"),
        (status = 403, description = "Only the platform may call this endpoint", body = crate::error::ErrorResponse),
        (status = 415, description = "Unsupported content type", body = String, content_type = "text/plain"),
        (status = 422, description = "Invalid or unsupported request", body = String, content_type = "text/plain"),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn file_access(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Json(payload): Json<FileAccessRequest>,
) -> ApiResult<Json<FileAccessResponse>> {
    platform_only(&identity)?;
    Ok(Json(FileAccessResponse {
        allowed: allowed(&state, &identity, &payload).await?,
    }))
}

async fn allowed(
    state: &AppState,
    identity: &Identity,
    payload: &FileAccessRequest,
) -> ApiResult<bool> {
    let Some(version) = versions::by_storage_file(&state.pool, &payload.file_id).await? else {
        return Ok(false);
    };
    if version.status != "ready" {
        return Ok(false);
    }
    if let Some(reference) = &payload.reference
        && *reference != versions::reference(&version.id)
    {
        return Ok(false);
    }
    let Some(file) = tree::load_file(&state.pool, &version.file_id).await? else {
        return Ok(false);
    };
    if file.deleted_at.is_some() || file.current_version_id.is_none() {
        return Ok(false);
    }
    let Some(access) = access::platform_access(&state.pool, identity, &file.workspace_id).await?
    else {
        return Ok(false);
    };
    Ok(tree::check_file_readable(&state.pool, &access, &file)
        .await
        .is_ok())
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckItem {
    group_key: String,
    #[serde(default)]
    resource_id: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct NotificationCheckRequest {
    items: Vec<CheckItem>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct NotificationCheckResponse {
    allowed: Vec<String>,
}

/// `POST /platform/notification-check`: a `space:<id>` notification stays
/// while the person may still open that space (a direct grant or one of the
/// current groups the call carries). Read-only.
#[utoipa::path(
    post, path = "/platform/notification-check", operation_id = "checkDriveNotifications", tag = "Platform callbacks",
    request_body = NotificationCheckRequest,
    responses(
        (status = 200, description = "Notification group keys still visible to the user", body = NotificationCheckResponse),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 400, description = "Malformed request", body = String, content_type = "text/plain"),
        (status = 403, description = "Only the platform may call this endpoint", body = crate::error::ErrorResponse),
        (status = 415, description = "Unsupported content type", body = String, content_type = "text/plain"),
        (status = 422, description = "Invalid or unsupported request", body = String, content_type = "text/plain"),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse),
    )
)]
pub async fn notification_check(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Json(payload): Json<NotificationCheckRequest>,
) -> ApiResult<Json<NotificationCheckResponse>> {
    platform_only(&identity)?;
    let readable: HashSet<String> =
        access::roles_by_space(&state.pool, &identity, &access::platform_groups(&identity))
            .await?
            .into_iter()
            .map(|(id, _)| id)
            .collect();
    let allowed = payload
        .items
        .iter()
        .take(200)
        .filter(|item| {
            item.resource_id.as_deref().is_some_and(|space| {
                readable.contains(space) && item.group_key == notify::space_group_key(space)
            })
        })
        .map(|item| item.group_key.clone())
        .collect();
    Ok(Json(NotificationCheckResponse { allowed }))
}
