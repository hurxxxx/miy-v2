use crate::{AppState, files, platform, search, spaces, trash, uploads};
use axum::{Router, extract::DefaultBodyLimit};
use std::{fs, path::Path};
use utoipa::openapi::{Info, OpenApi};
use utoipa_axum::{router::OpenApiRouter, routes};

/// Binary bodies flow through the file gateway. This only describes their wire format.
#[derive(utoipa::ToSchema)]
#[schema(format = Binary)]
pub(crate) struct BinaryBody(#[allow(dead_code)] String);

/// Gateway limits: one request upload is up to 10 MiB, a part up to 8 MiB.
const SINGLE_BODY_LIMIT: usize = 10 * 1024 * 1024 + 64 * 1024;
const PART_BODY_LIMIT: usize = 8 * 1024 * 1024 + 64 * 1024;

pub(crate) fn router() -> (Router<AppState>, OpenApi) {
    let content = OpenApiRouter::new()
        .routes(routes!(uploads::upload_content))
        .layer(DefaultBodyLimit::max(SINGLE_BODY_LIMIT));
    let parts = OpenApiRouter::new()
        .routes(routes!(uploads::upload_part))
        .layer(DefaultBodyLimit::max(PART_BODY_LIMIT));
    let (router, mut spec) = OpenApiRouter::new()
        .routes(routes!(spaces::list_spaces, spaces::create_space))
        .routes(routes!(spaces::update_space, spaces::delete_space))
        .routes(routes!(spaces::get_sharing, spaces::replace_sharing))
        .routes(routes!(spaces::sharing_result))
        .routes(routes!(files::browse))
        .routes(routes!(search::search_files))
        .routes(routes!(files::create_folder))
        .routes(routes!(files::update_folder, files::delete_folder))
        .routes(routes!(uploads::create_upload))
        .routes(routes!(uploads::get_upload, uploads::cancel_upload))
        .routes(routes!(uploads::complete_upload))
        .merge(content)
        .merge(parts)
        .routes(routes!(files::archive))
        .routes(routes!(files::bulk_delete))
        .routes(routes!(files::move_items))
        .routes(routes!(files::update_file, files::delete_file))
        .routes(routes!(files::download))
        .routes(routes!(files::list_versions))
        .routes(routes!(files::download_version))
        .routes(routes!(files::preview))
        .routes(routes!(trash::list))
        .routes(routes!(trash::restore))
        .routes(routes!(trash::delete_forever))
        .routes(routes!(trash::empty))
        .routes(routes!(platform::file_access))
        .routes(routes!(search::platform_search))
        .routes(routes!(platform::notification_check))
        .split_for_parts();
    spec.info = Info::new("드라이브 API", env!("CARGO_PKG_VERSION"));
    (router, spec)
}

/// Export or check the local API contract without a database connection.
pub(crate) fn handle_cli() -> anyhow::Result<bool> {
    let path = Path::new("docs/openapi.json");
    match std::env::args().nth(1).as_deref() {
        Some("--write-openapi") => {
            fs::write(path, serde_json::to_string_pretty(&router().1)? + "\n")?;
        }
        Some("--check-openapi") => {
            let current: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
            anyhow::ensure!(
                current == serde_json::to_value(router().1)?,
                "docs/openapi.json is out of date. Run pnpm api:generate."
            );
        }
        _ => return Ok(false),
    }
    Ok(true)
}
