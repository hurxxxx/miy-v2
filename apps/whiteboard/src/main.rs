//! Whiteboard (화이트보드): AI-DO's Excalidraw boards, ported from AI-DO
//! 0002af062 domains/whiteboard (see docs/PORTING.md). The SDK serves this
//! router under `/api`, so `/v1/whiteboard/hub` is reached as
//! `/api/v1/whiteboard/hub`, AI-DO's path. Image bytes go through the
//! platform file gateway (`runtime.fileApi`) to `/files/...`.
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{delete, get, patch, post, put},
};
use miy_app_sdk::acl::Acl;
use sqlx::PgPool;

mod error;
mod files;
mod notify;
mod people;
mod scene;
mod search;
mod spaces;
mod time;
mod whiteboard;

#[cfg(test)]
mod api_tests;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub acl: Acl,
}

/// Scenes keep their elements inline (images are stored apart), so a large
/// board stays well under this.
const SCENE_BODY_LIMIT: usize = 16 * 1024 * 1024;

/// AI-DO domains/whiteboard/router.py (`/whiteboard/...`), without the
/// collaboration, link-share and context-slot routes.
fn whiteboard_routes() -> Router<AppState> {
    Router::new()
        .route("/hub", get(whiteboard::hub))
        .route("/items", post(whiteboard::create_item))
        .route(
            "/items/{item_id}",
            get(whiteboard::get_item)
                .patch(whiteboard::update_item)
                .delete(whiteboard::trash_item),
        )
        .route("/items/{item_id}/restore", post(whiteboard::restore_item))
        .route(
            "/items/{item_id}/permanent",
            delete(whiteboard::delete_item_permanently),
        )
        .route(
            "/items/{item_id}/target",
            put(whiteboard::update_target).delete(whiteboard::delete_target),
        )
        .route(
            "/items/{item_id}/favorite",
            patch(whiteboard::toggle_favorite),
        )
        .route("/items/{item_id}/view", post(whiteboard::record_view))
        .route("/items/{item_id}/files", post(files::reserve))
        .route("/shareable-users", get(whiteboard::shareable_users))
        .route("/items/{item_id}/sharing", get(whiteboard::get_sharing))
        .route(
            "/items/{item_id}/sharing/users/{user_id}",
            put(whiteboard::upsert_user_share).delete(whiteboard::delete_user_share),
        )
        .layer(DefaultBodyLimit::max(SCENE_BODY_LIMIT))
}

pub fn router(pool: PgPool) -> Router {
    let acl = Acl::new(pool.clone(), spaces::acl_roles()).expect("valid space ACL roles");
    let state = AppState { pool, acl };
    Router::new()
        // The app-global space (AI-DO app_global) and an app space
        // (AI-DO `/api/v1/workspaces/{slug}/whiteboard/...`).
        .nest("/v1/whiteboard", whiteboard_routes())
        .nest("/v1/workspaces/{space_id}/whiteboard", whiteboard_routes())
        // Image bytes (runtime.fileApi).
        .route(
            "/files/whiteboard/items/{item_id}/files/{file_id}",
            put(files::upload)
                .get(files::download)
                .layer(DefaultBodyLimit::max(11 * 1024 * 1024)),
        )
        // Spaces (AI-DO app spaces of the whiteboard app)
        .route(
            "/v1/spaces",
            get(spaces::list_spaces).post(spaces::create_space),
        )
        .route(
            "/v1/spaces/{space_id}",
            patch(spaces::update_space).delete(spaces::delete_space),
        )
        .route(
            "/v1/spaces/{space_id}/sharing",
            get(spaces::get_sharing).put(spaces::replace_sharing),
        )
        .route(
            "/v1/spaces/{space_id}/sharing/requests/{request_id}",
            get(spaces::sharing_result),
        )
        // Platform → app (manifest platformEndpoints)
        .route(
            "/platform/notification-check",
            post(notify::notification_check),
        )
        .route("/v1/search", post(search::search))
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = miy_app_sdk::database().await?;
    miy_app_sdk::serve(router(pool)).await
}
