use crate::{AppState, files, notify, search, spaces, whiteboard};
use axum::{Router, extract::DefaultBodyLimit};
use std::{fs, path::Path};
use utoipa::openapi::{
    Info, OpenApi, Required,
    path::{ParameterBuilder, ParameterIn},
    schema::{Object, Type},
};
use utoipa_axum::{router::OpenApiRouter, routes};

/// The JSON frame gateway allows 4 MiB in either direction.
pub(crate) const SCENE_BODY_LIMIT: usize = 4 * 1024 * 1024;
const IMAGE_BODY_LIMIT: usize = 11 * 1024 * 1024;

/// Bodies streamed through the file gateway.
#[derive(utoipa::ToSchema)]
#[schema(format = Binary)]
pub(crate) struct BinaryBody(#[allow(dead_code)] String);

fn whiteboard_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(whiteboard::hub))
        .routes(routes!(whiteboard::create_item))
        .routes(routes!(
            whiteboard::get_item,
            whiteboard::update_item,
            whiteboard::trash_item
        ))
        .routes(routes!(whiteboard::restore_item))
        .routes(routes!(whiteboard::get_reference))
        .routes(routes!(whiteboard::delete_item_permanently))
        .routes(routes!(
            whiteboard::update_target,
            whiteboard::delete_target
        ))
        .routes(routes!(whiteboard::toggle_favorite))
        .routes(routes!(whiteboard::record_view))
        .routes(routes!(files::reserve))
        .routes(routes!(whiteboard::shareable_users))
        .routes(routes!(whiteboard::get_sharing))
        .routes(routes!(
            whiteboard::upsert_user_share,
            whiteboard::delete_user_share
        ))
        .layer(DefaultBodyLimit::max(SCENE_BODY_LIMIT))
}

fn workspace_routes() -> OpenApiRouter<AppState> {
    let mut router = whiteboard_routes();
    let space_id = ParameterBuilder::new()
        .name("space_id")
        .parameter_in(ParameterIn::Path)
        .required(Required::True)
        .description(Some("Whiteboard app space ID"))
        .schema(Some(Object::with_type(Type::String)))
        .build();

    for path in router.get_openapi_mut().paths.paths.values_mut() {
        for operation in [
            &mut path.get,
            &mut path.post,
            &mut path.put,
            &mut path.patch,
            &mut path.delete,
        ]
        .into_iter()
        .flatten()
        {
            operation
                .operation_id
                .as_mut()
                .expect("whiteboard route has an operation ID")
                .push_str("InSpace");
            operation
                .parameters
                .get_or_insert_with(Vec::new)
                .push(space_id.clone().into());
        }
    }
    router
}

pub(crate) fn router() -> (Router<AppState>, OpenApi) {
    let images = OpenApiRouter::new()
        .routes(routes!(files::upload, files::download))
        .layer(DefaultBodyLimit::max(IMAGE_BODY_LIMIT));
    let (router, mut spec) = OpenApiRouter::new()
        .routes(routes!(spaces::list_spaces, spaces::create_space))
        .routes(routes!(spaces::update_space, spaces::delete_space))
        .routes(routes!(spaces::get_sharing, spaces::replace_sharing))
        .routes(routes!(spaces::sharing_result))
        .nest("/v1/whiteboard", whiteboard_routes())
        .nest("/v1/workspaces/{space_id}/whiteboard", workspace_routes())
        .merge(images)
        .routes(routes!(notify::notification_check))
        .routes(routes!(search::search))
        .split_for_parts();
    spec.info = Info::new("화이트보드 API", env!("CARGO_PKG_VERSION"));
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
