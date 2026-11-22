use crate::{
    attachments, comments, items, managers, media, notify, pages, search, spaces, templates, trash,
    versions,
};
use axum::{Router, extract::DefaultBodyLimit};
use std::{fs, path::Path};
use utoipa::openapi::{Info, OpenApi};
use utoipa_axum::{
    router::{OpenApiRouter, UtoipaMethodRouterExt},
    routes,
};

const FILE_BODY_LIMIT: usize = 11 * 1024 * 1024;

#[derive(utoipa::ToSchema)]
#[schema(format = Binary)]
pub(crate) struct BinaryBody(#[allow(dead_code)] String);

/// A compatibility field that is always emitted as JSON null.
#[derive(serde::Serialize)]
pub(crate) struct NullValue;

impl utoipa::PartialSchema for NullValue {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::{ObjectBuilder, Type};
        ObjectBuilder::new().schema_type(Type::Null).into()
    }
}

impl utoipa::ToSchema for NullValue {}

pub(crate) fn router() -> (Router<crate::AppState>, OpenApi) {
    let (router, mut spec) = OpenApiRouter::new()
        .routes(routes!(managers::me))
        .routes(routes!(items::list_hub))
        .routes(routes!(items::create_item))
        .routes(routes!(
            items::get_item,
            items::update_item,
            items::delete_item
        ))
        .routes(routes!(items::duplicate_item))
        .routes(routes!(pages::list_pages, pages::create_page))
        .routes(routes!(items::update_target, items::delete_target))
        .routes(routes!(items::toggle_favorite))
        .routes(routes!(items::record_view))
        .routes(routes!(items::get_sharing))
        .routes(routes!(items::upsert_user_share, items::delete_user_share))
        .routes(routes!(
            pages::get_page,
            pages::update_page,
            pages::delete_page
        ))
        .routes(routes!(attachments::list, attachments::reserve))
        .routes(routes!(attachments::delete))
        .routes(
            routes!(attachments::upload_content, attachments::download)
                .layer(DefaultBodyLimit::max(FILE_BODY_LIMIT)),
        )
        .routes(routes!(versions::list))
        .routes(routes!(versions::get))
        .routes(routes!(versions::restore))
        .routes(routes!(comments::list, comments::create))
        .routes(routes!(comments::update, comments::delete))
        .routes(routes!(comments::resolve))
        .routes(routes!(comments::reopen))
        .routes(routes!(trash::list))
        .routes(routes!(trash::purge_doc))
        .routes(routes!(trash::restore_doc))
        .routes(routes!(trash::purge_page))
        .routes(routes!(trash::restore_page))
        .routes(routes!(templates::list, templates::create))
        .routes(routes!(templates::get, templates::delete))
        .routes(routes!(items::list_favorites))
        .routes(routes!(items::list_recent_pages))
        .routes(routes!(items::list_shareable_users))
        .routes(routes!(media::create_upload))
        .routes(routes!(media::link_media))
        .routes(
            routes!(media::upload_content, media::download)
                .layer(DefaultBodyLimit::max(FILE_BODY_LIMIT)),
        )
        .routes(routes!(spaces::list_spaces, spaces::create_space))
        .routes(routes!(spaces::update_space, spaces::delete_space))
        .routes(routes!(spaces::get_sharing, spaces::replace_sharing))
        .routes(routes!(spaces::sharing_result))
        .routes(routes!(managers::bootstrap))
        .routes(routes!(managers::list_managers, managers::add_manager))
        .routes(routes!(managers::remove_manager))
        .routes(routes!(notify::notification_check))
        .routes(routes!(notify::home_card))
        .routes(routes!(search::search))
        .split_for_parts();
    spec.info = Info::new("문서 API", env!("CARGO_PKG_VERSION"));
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
