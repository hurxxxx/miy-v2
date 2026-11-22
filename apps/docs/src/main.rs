//! Docs (문서): AI-DO's block-editor docs, ported from AI-DO 0002af062 (see
//! docs/PORTING.md). The SDK serves this router under `/api`, so
//! `/v1/docs/hub` is reached as `/api/v1/docs/hub`, the path AI-DO used.
//! Binary routes under `/files/...` are reached through the platform file
//! gateway (`runtime.fileApi`).
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{delete, get, patch, post, put},
};
use miy_app_sdk::acl::Acl;
use sqlx::PgPool;

mod access;
mod attachments;
mod comments;
mod content_text;
mod error;
mod files;
mod items;
mod managers;
mod media;
mod notify;
mod pages;
mod people;
mod search;
mod spaces;
mod templates;
mod time;
mod trash;
mod versions;

#[cfg(test)]
mod api_tests;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub acl: Acl,
}

const FILE_BODY_LIMIT: usize = 11 * 1024 * 1024;

pub fn router(pool: PgPool) -> Router {
    let acl = Acl::new(pool.clone(), spaces::acl_roles()).expect("valid space ACL roles");
    let state = AppState { pool, acl };
    Router::new()
        // Hub and items (AI-DO domains/docs/router.py)
        .route("/v1/docs/hub", get(items::list_hub))
        .route("/v1/docs/items", post(items::create_item))
        .route(
            "/v1/docs/items/{item_id}",
            get(items::get_item)
                .patch(items::update_item)
                .delete(items::delete_item),
        )
        .route(
            "/v1/docs/items/{item_id}/duplicate",
            post(items::duplicate_item),
        )
        .route(
            "/v1/docs/items/{item_id}/pages",
            get(pages::list_pages).post(pages::create_page),
        )
        .route(
            "/v1/docs/items/{item_id}/target",
            put(items::update_target).delete(items::delete_target),
        )
        .route(
            "/v1/docs/items/{item_id}/favorite",
            patch(items::toggle_favorite),
        )
        .route("/v1/docs/items/{item_id}/view", post(items::record_view))
        .route("/v1/docs/items/{item_id}/sharing", get(items::get_sharing))
        .route(
            "/v1/docs/items/{item_id}/sharing/users/{user_id}",
            put(items::upsert_user_share).delete(items::delete_user_share),
        )
        .route(
            "/v1/docs/pages/{page_id}",
            get(pages::get_page)
                .patch(pages::update_page)
                .delete(pages::delete_page),
        )
        .route(
            "/v1/docs/pages/{page_id}/attachments",
            get(attachments::list).post(attachments::reserve),
        )
        .route(
            "/v1/docs/attachments/{attachment_id}",
            delete(attachments::delete),
        )
        .route(
            "/files/docs/attachments/{attachment_id}/content",
            put(attachments::upload_content)
                .get(attachments::download)
                .layer(DefaultBodyLimit::max(FILE_BODY_LIMIT)),
        )
        // 보강: version history, comments, trash, templates
        .route("/v1/docs/pages/{page_id}/versions", get(versions::list))
        .route(
            "/v1/docs/pages/{page_id}/versions/{version_id}",
            get(versions::get),
        )
        .route(
            "/v1/docs/pages/{page_id}/versions/{version_id}/restore",
            post(versions::restore),
        )
        .route(
            "/v1/docs/pages/{page_id}/comments",
            get(comments::list).post(comments::create),
        )
        .route(
            "/v1/docs/comments/{comment_id}",
            patch(comments::update).delete(comments::delete),
        )
        .route(
            "/v1/docs/comments/{comment_id}/resolve",
            post(comments::resolve),
        )
        .route(
            "/v1/docs/comments/{comment_id}/reopen",
            post(comments::reopen),
        )
        .route("/v1/docs/trash", get(trash::list))
        .route("/v1/docs/trash/items/{item_id}", delete(trash::purge_doc))
        .route(
            "/v1/docs/trash/items/{item_id}/restore",
            post(trash::restore_doc),
        )
        .route("/v1/docs/trash/pages/{page_id}", delete(trash::purge_page))
        .route(
            "/v1/docs/trash/pages/{page_id}/restore",
            post(trash::restore_page),
        )
        .route(
            "/v1/docs/templates",
            get(templates::list).post(templates::create),
        )
        .route(
            "/v1/docs/templates/{template_id}",
            get(templates::get).delete(templates::delete),
        )
        .route("/v1/docs/favorites", get(items::list_favorites))
        .route("/v1/docs/recent-pages", get(items::list_recent_pages))
        .route("/v1/docs/shareable-users", get(items::list_shareable_users))
        // Body images (AI-DO /media/upload, /media/link, media resolve)
        .route("/v1/media/upload", post(media::create_upload))
        .route("/v1/media/link", post(media::link_media))
        .route(
            "/files/media/{media_id}/content",
            put(media::upload_content)
                .get(media::download)
                .layer(DefaultBodyLimit::max(FILE_BODY_LIMIT)),
        )
        // Spaces (AI-DO app-owned docs workspaces)
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
        // Docs managers
        .route("/v1/docs/me", get(managers::me))
        .route("/v1/docs/managers/bootstrap", post(managers::bootstrap))
        .route(
            "/v1/docs/managers",
            get(managers::list_managers).post(managers::add_manager),
        )
        .route(
            "/v1/docs/managers/{user_id}",
            delete(managers::remove_manager),
        )
        // Platform → app (manifest platformEndpoints)
        .route(
            "/platform/notification-check",
            post(notify::notification_check),
        )
        .route("/v1/home-card", post(notify::home_card))
        .route("/v1/search", post(search::search))
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = miy_app_sdk::database().await?;
    miy_app_sdk::serve(router(pool)).await
}
