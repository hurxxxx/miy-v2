//! Messenger (메신저): 1:1 and group conversations ported from AI-DO
//! 0002af062 `domains/dm` (see docs/PORTING.md). The SDK serves this router
//! under `/api`, so `/v1/dm/conversations` is reached as
//! `/api/v1/dm/conversations`, the path AI-DO used. File bytes use the file
//! gateway (`runtime.fileApi`), which forwards to `/api/files/...`.
use axum::{
    Router,
    routing::{delete, get, patch, post, put},
};
use sqlx::PgPool;

mod attachments;
mod audit;
mod conversations;
mod error;
mod live;
mod mentions;
mod messages;
mod notify;
mod reactions;
mod request;
mod safety;
mod safety_admin;
mod schemas;
mod search;
mod store;
mod time;
mod ugc;
mod users;
mod views;

#[cfg(test)]
mod api_tests;

pub fn router(pool: PgPool) -> Router {
    use attachments as a;
    use conversations as c;
    use messages as m;
    use safety_admin as s;
    Router::new()
        .route("/v1/dm/users", get(c::search_dm_users))
        .route("/v1/dm/me", get(c::get_dm_me))
        // Live hints relayed from `runtime.live` (M4).
        .route("/live", get(live::live))
        .route(
            "/v1/dm/conversations",
            get(c::list_dm_conversations).post(c::create_dm_conversation),
        )
        // AI-DO's hidden aliases for the older thread language.
        .route(
            "/v1/dm/threads",
            get(c::list_dm_conversations).post(c::create_dm_conversation),
        )
        .route(
            "/v1/dm/conversations/{conversation_id}",
            patch(c::update_dm_conversation),
        )
        .route(
            "/v1/dm/conversations/{conversation_id}/participants",
            post(c::add_dm_conversation_participants),
        )
        .route(
            "/v1/dm/conversations/{conversation_id}/participants/me",
            delete(c::leave_dm_conversation),
        )
        .route(
            "/v1/dm/conversations/{conversation_id}/participants/{user_id}",
            delete(c::remove_dm_conversation_participant),
        )
        .route(
            "/v1/dm/conversations/{conversation_id}/messages",
            get(m::list_dm_messages).post(m::send_dm_message),
        )
        .route(
            "/v1/dm/threads/{conversation_id}/messages",
            get(m::list_dm_messages).post(m::send_dm_message),
        )
        .route(
            "/v1/dm/conversations/{conversation_id}/messages/{message_id}",
            patch(m::edit_dm_message).delete(m::delete_dm_message),
        )
        .route(
            "/v1/dm/conversations/{conversation_id}/messages/{message_id}/reactions/{reaction}",
            put(reactions::add_reaction).delete(reactions::remove_reaction),
        )
        .route("/v1/dm/search", get(search::search_messages))
        .route(
            "/v1/dm/conversations/{conversation_id}/attachments",
            post(a::reserve_dm_attachment),
        )
        .route(
            "/v1/dm/attachments/{attachment_id}",
            delete(a::cancel_dm_attachment),
        )
        .route(
            "/v1/dm/attachments/{attachment_id}/download",
            get(a::get_dm_attachment_download_url),
        )
        .route(
            "/v1/dm/attachments/{attachment_id}/preview",
            get(a::get_dm_attachment_preview_url),
        )
        .route(
            "/files/dm/attachments/{attachment_id}/content",
            get(a::download_dm_attachment_content).put(a::upload_dm_attachment_content),
        )
        .route(
            "/v1/dm/conversations/{conversation_id}/read",
            patch(c::mark_dm_conversation_read),
        )
        .route(
            "/v1/dm/threads/{conversation_id}/read",
            patch(c::mark_dm_conversation_read),
        )
        .route(
            "/v1/dm/conversations/{conversation_id}/mute",
            put(c::mute_dm_conversation),
        )
        .route("/v1/dm/reports", post(s::report_dm))
        .route("/v1/dm/blocks", get(s::list_dm_blocks))
        .route(
            "/v1/dm/blocks/{user_id}",
            put(s::block_dm_user).delete(s::unblock_dm_user),
        )
        .route("/v1/dm/terms", get(s::get_dm_terms))
        .route("/v1/dm/terms/accept", post(s::accept_dm_terms))
        // Report review for messenger managers (AI-DO /admin/dm-safety).
        .route("/v1/dm-safety/me", get(s::manager_status))
        .route(
            "/v1/dm-safety/managers",
            get(s::list_managers).post(s::grant_manager),
        )
        .route("/v1/dm-safety/managers/claim", post(s::claim_manager))
        .route(
            "/v1/dm-safety/managers/{user_id}",
            delete(s::revoke_manager),
        )
        .route(
            "/v1/dm-safety/terms-settings",
            get(s::get_terms_settings).put(s::put_terms_settings),
        )
        .route("/v1/dm-safety/reports", get(s::list_reports))
        .route("/v1/dm-safety/reports/{report_id}", get(s::get_report))
        .route(
            "/v1/dm-safety/reports/{report_id}/evidence",
            get(s::get_evidence),
        )
        .route(
            "/files/dm-safety/reports/{report_id}/evidence/attachments/{attachment_id}/content",
            get(s::evidence_attachment_content),
        )
        .route(
            "/v1/dm-safety/reports/{report_id}/resolve",
            post(s::resolve_report),
        )
        .route(
            "/v1/dm-safety/reports/{report_id}/message-visibility",
            post(s::set_message_visibility),
        )
        .route(
            "/v1/dm-safety/filter-terms",
            get(s::list_filter_terms).post(s::add_filter_term),
        )
        .route(
            "/v1/dm-safety/filter-terms/{term_id}",
            delete(s::remove_filter_term),
        )
        // Platform → app calls (manifest platformEndpoints).
        .route(
            "/platform/notification-check",
            post(notify::notification_check),
        )
        .route("/platform/home-card", post(notify::home_card))
        .route("/platform/search", post(search::platform_search))
        .with_state(pool)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = miy_app_sdk::database().await?;
    miy_app_sdk::serve(router(pool)).await
}
