//! PMS: spaces, folders, task lists and tasks, ported from AI-DO 0002af062
//! (see docs/PORTING.md). The SDK serves this router under `/api`, so
//! `/v1/pms/lists` is reached as `/api/v1/pms/lists`, the path AI-DO used.
use axum::Router;
use sqlx::PgPool;

mod access;
mod api_contract;
mod attachments;
mod copy;
mod custom_fields;
mod error;
mod error_catalog;
mod export;
mod home_card;
mod input;
mod lists;
mod milestones;
mod models;
mod notify;
mod saved_filters;
mod search;
mod spaces;
mod statuses;
mod summary;
mod task_items;
mod tasks;
mod templates;
mod users;
mod workflow;

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
