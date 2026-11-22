//! Board (게시판): AI-DO's community board and announcements, ported from
//! AI-DO 0002af062 (see docs/PORTING.md). The SDK serves this router under
//! `/api`, so `/v1/community/channels` is reached as `/api/v1/community/...`,
//! the path AI-DO used. Binary routes under `/files/...` are reached through
//! the platform file gateway (`runtime.fileApi`).
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{delete, get, patch, post, put},
};
use miy_app_sdk::acl::Acl;
use sqlx::PgPool;

mod announcements;
mod community;
mod error;
mod managers;
mod media;
mod notify;
mod password;
mod people;
mod spaces;
mod time;
mod ugc;

#[cfg(test)]
mod api_tests;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub acl: Acl,
}

pub fn router(pool: PgPool) -> Router {
    let acl = Acl::new(pool.clone(), spaces::acl_roles()).expect("valid space ACL roles");
    let state = AppState { pool, acl };
    Router::new()
        // Community (AI-DO domains/community/router.py)
        .route("/v1/community/channels", get(community::list_channels))
        .route(
            "/v1/community/admin/channels",
            get(community::list_admin_channels).post(community::create_channel),
        )
        .route(
            "/v1/community/admin/channels/{channel_key}",
            patch(community::update_channel).delete(community::delete_channel),
        )
        .route(
            "/v1/community/channels/{channel_key}/posts",
            get(community::list_posts).post(community::create_post),
        )
        .route(
            "/v1/community/posts/{post_id}",
            get(community::get_post)
                .patch(community::update_post)
                .delete(community::delete_post),
        )
        .route(
            "/v1/community/posts/{post_id}/read",
            post(community::mark_read),
        )
        .route(
            "/v1/community/posts/{post_id}/unlock",
            post(community::unlock_post),
        )
        .route(
            "/v1/community/posts/{post_id}/comments",
            post(community::create_comment),
        )
        .route(
            "/v1/community/posts/{post_id}/comments/{comment_id}",
            patch(community::update_comment).delete(community::delete_comment),
        )
        // Banned words (AI-DO /admin/dm-safety/filter-terms)
        .route(
            "/v1/community/admin/filter-terms",
            get(ugc::list_filter_terms).post(ugc::add_filter_term),
        )
        .route(
            "/v1/community/admin/filter-terms/{term_id}",
            delete(ugc::remove_filter_term),
        )
        // Embedded images (AI-DO /media/upload and media resolve)
        .route("/v1/media/upload", post(media::create_upload))
        .route(
            "/files/media/{media_id}/content",
            put(media::upload_content)
                .get(media::download)
                .layer(DefaultBodyLimit::max(11 * 1024 * 1024)),
        )
        .route(
            "/files/community/posts/{post_id}/media/{media_id}",
            post(media::download_post_media),
        )
        // Notices (AI-DO domains/announcements/router.py)
        .route(
            "/v1/announcements",
            get(announcements::list_company).post(announcements::create_company),
        )
        .route(
            "/v1/announcements/{announcement_id}",
            get(announcements::get_company)
                .patch(announcements::update_company)
                .delete(announcements::delete_company),
        )
        .route(
            "/v1/spaces/{space_id}/announcements",
            get(announcements::list_in_space).post(announcements::create_in_space),
        )
        .route(
            "/v1/spaces/{space_id}/announcements/{announcement_id}",
            get(announcements::get_in_space)
                .patch(announcements::update_in_space)
                .delete(announcements::delete_in_space),
        )
        .route("/v1/board/notices/recent", get(announcements::recent))
        // Spaces (AI-DO workspaces, for notices)
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
        // Board managers
        .route("/v1/board/me", get(managers::me))
        .route("/v1/board/managers/bootstrap", post(managers::bootstrap))
        .route(
            "/v1/board/managers",
            get(managers::list_managers).post(managers::add_manager),
        )
        .route(
            "/v1/board/managers/{user_id}",
            delete(managers::remove_manager),
        )
        // Platform → app (manifest platformEndpoints)
        .route(
            "/platform/notification-check",
            post(notify::notification_check),
        )
        .route("/platform/home-card", post(notify::home_card))
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = miy_app_sdk::database().await?;
    miy_app_sdk::serve(router(pool)).await
}
