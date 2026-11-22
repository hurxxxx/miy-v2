//! Planner (플래너): personal calendar events, to-dos and memo, ported from
//! AI-DO 0002af062 (see docs/PORTING.md). The SDK serves this router under
//! `/api`, so `/v1/planner/events` is reached as `/api/v1/planner/events`,
//! the same path AI-DO used.
use axum::Router;
use sqlx::PgPool;

mod api_contract;
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
    api_contract::router().0.with_state(pool)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if api_contract::handle_cli()? {
        return Ok(());
    }
    let pool = miy_app_sdk::database().await?;
    miy_app_sdk::serve(router(pool)).await
}
