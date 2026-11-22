//! Messenger (메신저): 1:1 and group conversations ported from AI-DO
//! 0002af062 `domains/dm` (see docs/PORTING.md). The SDK serves this router
//! under `/api`, so `/v1/dm/conversations` is reached as
//! `/api/v1/dm/conversations`, the path AI-DO used. File bytes use the file
//! gateway (`runtime.fileApi`), which forwards to `/api/files/...`.
use axum::Router;
use sqlx::PgPool;

mod api_contract;
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
