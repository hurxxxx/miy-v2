//! Drive (드라이브): AI-DO's Files module, ported from AI-DO 0002af062 (see
//! docs/PORTING.md). The SDK serves this router under `/api`; AI-DO's
//! workspace-scoped `/api/v1/files/...` paths become
//! `/api/v1/spaces/{space_id}/files/...`. Binary routes under `/files/...`
//! are reached through the platform file gateway (`runtime.fileApi`).
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, patch, post, put},
};
use miy_app_sdk::acl::Acl;
use sqlx::PgPool;

mod access;
mod archive_planner;
mod cleanup;
mod error;
mod files;
mod notify;
mod people;
mod platform;
mod search;
mod spaces;
mod time;
mod trash;
mod tree;
mod uploads;
mod versions;

#[cfg(test)]
mod api_tests;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub acl: Acl,
}

/// Gateway limits: one request upload is up to 10 MiB, a part up to 8 MiB.
const SINGLE_BODY_LIMIT: usize = 10 * 1024 * 1024 + 64 * 1024;
const PART_BODY_LIMIT: usize = 8 * 1024 * 1024 + 64 * 1024;

pub fn router(pool: PgPool) -> Router {
    let acl = Acl::new(pool.clone(), access::acl_roles()).expect("valid space ACL roles");
    let state = AppState { pool, acl };
    Router::new()
        // Spaces (AI-DO workspaces)
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
        // Files (AI-DO domains/files/router.py)
        .route("/v1/spaces/{space_id}/files", get(files::browse))
        .route(
            "/v1/spaces/{space_id}/files/search",
            post(search::search_files),
        )
        .route(
            "/v1/spaces/{space_id}/files/folders",
            post(files::create_folder),
        )
        .route(
            "/v1/spaces/{space_id}/files/folders/{folder_id}",
            patch(files::update_folder).delete(files::delete_folder),
        )
        .route(
            "/v1/spaces/{space_id}/files/uploads",
            post(uploads::create_upload),
        )
        .route(
            "/v1/spaces/{space_id}/files/uploads/{upload_id}",
            get(uploads::get_upload).delete(uploads::cancel_upload),
        )
        .route(
            "/v1/spaces/{space_id}/files/uploads/{upload_id}/complete",
            post(uploads::complete_upload),
        )
        .route("/v1/spaces/{space_id}/files/archive", post(files::archive))
        .route(
            "/v1/spaces/{space_id}/files/bulk-delete",
            post(files::bulk_delete),
        )
        .route("/v1/spaces/{space_id}/files/move", post(files::move_items))
        .route(
            "/v1/spaces/{space_id}/files/{file_id}",
            patch(files::update_file).delete(files::delete_file),
        )
        .route(
            "/v1/spaces/{space_id}/files/{file_id}/download",
            get(files::download),
        )
        .route(
            "/v1/spaces/{space_id}/files/{file_id}/versions",
            get(files::list_versions),
        )
        .route(
            "/v1/spaces/{space_id}/files/{file_id}/versions/{version_id}/download",
            get(files::download_version),
        )
        // Trash (보강)
        .route("/v1/spaces/{space_id}/trash", get(trash::list))
        .route("/v1/spaces/{space_id}/trash/restore", post(trash::restore))
        .route(
            "/v1/spaces/{space_id}/trash/delete",
            post(trash::delete_forever),
        )
        .route("/v1/spaces/{space_id}/trash/empty", post(trash::empty))
        // Binary routes (runtime.fileApi)
        .route(
            "/files/uploads/{upload_id}/content",
            put(uploads::upload_content).layer(DefaultBodyLimit::max(SINGLE_BODY_LIMIT)),
        )
        .route(
            "/files/uploads/{upload_id}/parts",
            put(uploads::upload_part).layer(DefaultBodyLimit::max(PART_BODY_LIMIT)),
        )
        .route("/files/items/{file_id}/preview", get(files::preview))
        // Platform → app (manifest platformEndpoints)
        .route("/v1/file-access", post(platform::file_access))
        .route("/v1/search", post(search::platform_search))
        .route(
            "/platform/notification-check",
            post(platform::notification_check),
        )
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = miy_app_sdk::database().await?;
    miy_app_sdk::serve(router(pool)).await
}
