//! Docs managers (문서 관리자), following the board's board_managers pattern.
//! They take the place of AI-DO's app administrator role for this app: they
//! create spaces (AI-DO `space_creation_role = "admin"`) and act as admins of
//! the global docs space (AI-DO: an explicit admin binding on the app_global
//! workspace), so they may manage space docs there. The platform
//! administrator flag grants nothing here.
//!
//! Bootstrap: while no manager exists, any person who may use the app can
//! claim the role once (`POST /v1/docs/managers/bootstrap`); the screen
//! offers it to everyone in that state. Afterwards only managers add or
//! remove managers, and the last one cannot be removed.
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::{
    AppState,
    error::{ApiError, ApiJson, ApiResult},
    people,
    time::now_utc,
};

pub async fn is_manager<'e>(executor: impl sqlx::PgExecutor<'e>, user_id: &str) -> ApiResult<bool> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM docs_managers WHERE user_id = $1)",
    )
    .bind(user_id)
    .fetch_one(executor)
    .await?)
}

/// Inside a write transaction: is the person a manager now? The row lock keeps
/// a concurrent removal from taking effect before this write commits.
pub async fn is_manager_locked(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: &str,
) -> ApiResult<bool> {
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT user_id FROM docs_managers WHERE user_id = $1 FOR SHARE",
    )
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await?
    .is_some())
}

pub fn manager_required() -> ApiError {
    ApiError::forbidden("docs.manager_required", "Docs manager access required.")
}

pub async fn require_manager(pool: &PgPool, identity: &Identity) -> ApiResult<()> {
    if is_manager(pool, &identity.user_id).await? {
        Ok(())
    } else {
        Err(manager_required())
    }
}

async fn lock_manager_changes(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> ApiResult<()> {
    sqlx::query("LOCK TABLE docs_managers IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DocsMe {
    user_id: String,
    is_manager: bool,
    can_bootstrap: bool,
}

#[utoipa::path(
    get, path = "/v1/docs/me", operation_id = "getDocsMe",
    responses(
        (status = 200, description = "Document access for the current user", body = DocsMe),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 500, description = "Could not read manager status", body = crate::error::ErrorResponse)
    )
)]
pub async fn me(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<DocsMe>> {
    let is_manager = is_manager(&state.pool, &identity.user_id).await?;
    let any: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM docs_managers)")
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(DocsMe {
        user_id: identity.user_id,
        is_manager,
        can_bootstrap: !any,
    }))
}

#[utoipa::path(
    post, path = "/v1/docs/managers/bootstrap", operation_id = "bootstrapDocsManager", tag = "Managers",
    responses(
        (status = 201, description = "Created", body = DocsMe),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 409, description = "Current resource state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse)
    )
)]
pub async fn bootstrap(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<impl IntoResponse> {
    let mut tx = state.pool.begin().await?;
    lock_manager_changes(&mut tx).await?;
    let any: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM docs_managers)")
        .fetch_one(&mut *tx)
        .await?;
    if any {
        return Err(ApiError::conflict(
            "docs.manager_exists",
            "A docs manager already exists.",
        ));
    }
    sqlx::query("INSERT INTO docs_managers (user_id, granted_by, created_at) VALUES ($1, $1, $2)")
        .bind(&identity.user_id)
        .bind(now_utc())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(DocsMe {
            user_id: identity.user_id,
            is_manager: true,
            can_bootstrap: false,
        }),
    ))
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManagerOut {
    user_id: String,
    #[schema(required)]
    name: Option<String>,
    granted_by: String,
    #[serde(serialize_with = "crate::time::serialize")]
    #[schema(value_type = String, example = "2026-11-22T09:00:00")]
    created_at: NaiveDateTime,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ManagersResponse {
    items: Vec<ManagerOut>,
}

#[utoipa::path(
    get, path = "/v1/docs/managers", operation_id = "listDocsManagers", tag = "Managers",
    responses(
        (status = 200, description = "Success", body = ManagersResponse),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_managers(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<ManagersResponse>> {
    require_manager(&state.pool, &identity).await?;
    let rows: Vec<(String, String, NaiveDateTime)> = sqlx::query_as(
        "SELECT user_id, granted_by, created_at FROM docs_managers ORDER BY created_at, user_id",
    )
    .fetch_all(&state.pool)
    .await?;
    let names = people::display_names(&identity, rows.iter().map(|row| row.0.clone())).await;
    Ok(Json(ManagersResponse {
        items: rows
            .into_iter()
            .map(|(user_id, granted_by, created_at)| ManagerOut {
                name: names.get(&user_id).cloned(),
                user_id,
                granted_by,
                created_at,
            })
            .collect(),
    }))
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManagerAddRequest {
    user_id: String,
}

#[utoipa::path(
    post, path = "/v1/docs/managers", operation_id = "addDocsManager", tag = "Managers",
    request_body = ManagerAddRequest,
    responses(
        (status = 201, description = "Created"),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse)
    )
)]
pub async fn add_manager(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<ManagerAddRequest>,
) -> ApiResult<impl IntoResponse> {
    require_manager(&state.pool, &identity).await?;
    let user_id = payload.user_id.trim().to_string();
    if user_id.is_empty() || user_id.chars().count() > 256 {
        return Err(ApiError::validation("userId: invalid"));
    }
    // Only a person the directory knows now (an active account).
    let names = people::display_names(&identity, [user_id.clone()]).await;
    if !names.contains_key(&user_id) {
        return Err(ApiError::bad_request(
            "docs.manager_unknown_user",
            "The person was not found in the directory.",
        ));
    }
    let mut tx = state.pool.begin().await?;
    lock_manager_changes(&mut tx).await?;
    if !is_manager(&mut *tx, &identity.user_id).await? {
        return Err(manager_required());
    }
    sqlx::query(
        "INSERT INTO docs_managers (user_id, granted_by, created_at) VALUES ($1, $2, $3) \
         ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(&user_id)
    .bind(&identity.user_id)
    .bind(now_utc())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::CREATED)
}

#[utoipa::path(
    delete, path = "/v1/docs/managers/{user_id}", operation_id = "removeDocsManager", tag = "Managers",
    params(("user_id" = String, Path)),
    responses(
        (status = 204, description = "Completed with no response body"),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 409, description = "Current resource state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse)
    )
)]
pub async fn remove_manager(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(user_id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.pool.begin().await?;
    lock_manager_changes(&mut tx).await?;
    if !is_manager(&mut *tx, &identity.user_id).await? {
        return Err(manager_required());
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM docs_managers")
        .fetch_one(&mut *tx)
        .await?;
    if !is_manager(&mut *tx, &user_id).await? {
        return Err(ApiError::not_found(
            "docs.manager_not_found",
            "The docs manager was not found.",
        ));
    }
    if count <= 1 {
        return Err(ApiError::conflict(
            "docs.last_manager",
            "The last docs manager cannot be removed.",
        ));
    }
    sqlx::query("DELETE FROM docs_managers WHERE user_id = $1")
        .bind(&user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
