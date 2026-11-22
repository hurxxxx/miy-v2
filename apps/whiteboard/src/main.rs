//! Whiteboard (화이트보드): AI-DO's Excalidraw boards, ported from AI-DO
//! 0002af062 domains/whiteboard (see docs/PORTING.md). The SDK serves this
//! router under `/api`, so `/v1/whiteboard/hub` is reached as
//! `/api/v1/whiteboard/hub`, AI-DO's path. Image bytes go through the
//! platform file gateway (`runtime.fileApi`) to `/files/...`.
use axum::Router;
use miy_app_sdk::acl::Acl;
use sqlx::PgPool;

mod api_contract;
mod error;
mod files;
mod notify;
mod people;
mod scene;
mod search;
mod spaces;
mod time;
mod whiteboard;

#[cfg(test)]
mod api_tests;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub acl: Acl,
}

pub fn router(pool: PgPool) -> Router {
    let acl = Acl::new(pool.clone(), spaces::acl_roles()).expect("valid space ACL roles");
    let state = AppState { pool, acl };
    api_contract::router().0.with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if api_contract::handle_cli()? {
        return Ok(());
    }
    let pool = miy_app_sdk::database().await?;
    miy_app_sdk::serve(router(pool)).await
}
