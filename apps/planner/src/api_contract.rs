use crate::{calendar, home_card, personal_widgets, planner, search};
use axum::Router;
use std::{fs, path::Path};
use utoipa::{
    OpenApi as _,
    openapi::{Info, OpenApi},
};
use utoipa_axum::{router::OpenApiRouter, routes};

#[derive(utoipa::OpenApi)]
#[openapi(components(schemas(home_card::HomeCardRequest)))]
struct PlannerApiComponents;

pub(crate) fn router() -> (Router<sqlx::PgPool>, OpenApi) {
    let (router, mut spec) = OpenApiRouter::with_openapi(PlannerApiComponents::openapi())
        .routes(routes!(
            planner::list_planner_events,
            planner::create_planner_event
        ))
        .routes(routes!(
            planner::get_planner_event,
            planner::update_planner_event,
            planner::delete_planner_event
        ))
        .routes(routes!(calendar::list_events))
        .routes(routes!(
            personal_widgets::list_personal_todos,
            personal_widgets::create_personal_todo
        ))
        .routes(routes!(
            personal_widgets::update_personal_todo,
            personal_widgets::delete_personal_todo
        ))
        .routes(routes!(
            personal_widgets::get_personal_memo,
            personal_widgets::save_personal_memo
        ))
        .routes(routes!(home_card::home_card))
        .routes(routes!(search::search))
        .split_for_parts();
    spec.info = Info::new("플래너 API", env!("CARGO_PKG_VERSION"));
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
