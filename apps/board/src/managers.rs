//! Board managers (게시판 관리자). They take the place of AI-DO's platform
//! administrator for this app: channel management, banned words, company-wide
//! notices and spaces, and what AI-DO showed only to administrators (authors
//! of anonymous posts, admin-only channel posts, editing in read-only
//! channels). The platform administrator flag grants nothing here.
//!
//! Bootstrap: while no manager exists, any person who may use the app can
//! claim the role once (`POST /v1/board/managers/bootstrap`); the screen
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
        "SELECT EXISTS(SELECT 1 FROM board_managers WHERE user_id = $1)",
    )
    .bind(user_id)
    .fetch_one(executor)
    .await?)
}

pub fn manager_required() -> ApiError {
    ApiError::forbidden("board.manager_required", "Board manager access required.")
}

pub async fn require_manager(pool: &PgPool, identity: &Identity) -> ApiResult<()> {
    if is_manager(pool, &identity.user_id).await? {
        Ok(())
    } else {
        Err(manager_required())
    }
}

async fn lock_manager_changes(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> ApiResult<()> {
    sqlx::query("LOCK TABLE board_managers IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardMe {
    user_id: String,
    is_manager: bool,
    can_bootstrap: bool,
}

pub async fn me(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<BoardMe>> {
    let is_manager = is_manager(&state.pool, &identity.user_id).await?;
    let any: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM board_managers)")
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(BoardMe {
        user_id: identity.user_id,
        is_manager,
        can_bootstrap: !any,
    }))
}

pub async fn bootstrap(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<impl IntoResponse> {
    let mut tx = state.pool.begin().await?;
    lock_manager_changes(&mut tx).await?;
    let any: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM board_managers)")
        .fetch_one(&mut *tx)
        .await?;
    if any {
        return Err(ApiError::conflict(
            "board.manager_exists",
            "A board manager already exists.",
        ));
    }
    sqlx::query("INSERT INTO board_managers (user_id, granted_by, created_at) VALUES ($1, $1, $2)")
        .bind(&identity.user_id)
        .bind(now_utc())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(BoardMe {
            user_id: identity.user_id,
            is_manager: true,
            can_bootstrap: false,
        }),
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerOut {
    user_id: String,
    name: Option<String>,
    granted_by: String,
    #[serde(serialize_with = "crate::time::serialize")]
    created_at: NaiveDateTime,
}

#[derive(Serialize)]
pub struct ManagersResponse {
    items: Vec<ManagerOut>,
}

pub async fn list_managers(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<ManagersResponse>> {
    require_manager(&state.pool, &identity).await?;
    let rows: Vec<(String, String, NaiveDateTime)> = sqlx::query_as(
        "SELECT user_id, granted_by, created_at FROM board_managers ORDER BY created_at, user_id",
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerAddRequest {
    user_id: String,
}

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
            "board.manager_unknown_user",
            "The person was not found in the directory.",
        ));
    }
    let mut tx = state.pool.begin().await?;
    lock_manager_changes(&mut tx).await?;
    if !is_manager(&mut *tx, &identity.user_id).await? {
        return Err(manager_required());
    }
    sqlx::query(
        "INSERT INTO board_managers (user_id, granted_by, created_at) VALUES ($1, $2, $3) \
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
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM board_managers")
        .fetch_one(&mut *tx)
        .await?;
    if !is_manager(&mut *tx, &user_id).await? {
        return Err(ApiError::not_found(
            "board.manager_not_found",
            "The board manager was not found.",
        ));
    }
    if count <= 1 {
        return Err(ApiError::conflict(
            "board.last_manager",
            "The last board manager cannot be removed.",
        ));
    }
    sqlx::query("DELETE FROM board_managers WHERE user_id = $1")
        .bind(&user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
