//! Planner (플래너): personal calendar events, to-dos and memo, ported from
//! AI-DO 0002af062 (see docs/PORTING.md). The SDK serves this router under
//! `/api`, so `/v1/planner/events` is reached as `/api/v1/planner/events`,
//! the same path AI-DO used.
use axum::{
    Router,
    routing::{get, patch, post},
};
use sqlx::PgPool;

mod calendar;
mod error;
mod event_time;
mod home_card;
mod personal_widgets;
mod planner;
mod search;
mod time;

#[cfg(test)]
mod api_tests;

pub fn router(pool: PgPool) -> Router {
    Router::new()
        .route(
            "/v1/planner/events",
            get(planner::list_planner_events).post(planner::create_planner_event),
        )
        .route(
            "/v1/planner/events/{event_id}",
            get(planner::get_planner_event)
                .patch(planner::update_planner_event)
                .delete(planner::delete_planner_event),
        )
        .route("/v1/calendar/events", get(calendar::list_events))
        .route(
            "/v1/personal-widgets/todos",
            get(personal_widgets::list_personal_todos).post(personal_widgets::create_personal_todo),
        )
        .route(
            "/v1/personal-widgets/todos/{todo_id}",
            patch(personal_widgets::update_personal_todo)
                .delete(personal_widgets::delete_personal_todo),
        )
        .route(
            "/v1/personal-widgets/memo",
            get(personal_widgets::get_personal_memo).put(personal_widgets::save_personal_memo),
        )
        // The portal home's summary card: platform calls only (manifest
        // `platformEndpoints.homeCard`).
        .route("/v1/home-card", post(home_card::home_card))
        // The portal's integrated search: platform calls only (manifest
        // `platformEndpoints.search`).
        .route("/v1/search", post(search::search))
        .with_state(pool)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = miy_app_sdk::database().await?;
    miy_app_sdk::serve(router(pool)).await
}
