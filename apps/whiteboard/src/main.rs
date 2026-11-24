//! Whiteboard (화이트보드): AI-DO's Excalidraw boards, ported from AI-DO
//! 0002af062 domains/whiteboard (see docs/PORTING.md). The SDK serves this
//! router under `/api`, so `/v1/whiteboard/hub` is reached as
//! `/api/v1/whiteboard/hub`, AI-DO's path. Image bytes go through the
//! platform file gateway (`runtime.fileApi`) to `/files/...`.
use axum::Router;
use miy_app_sdk::acl::Acl;
use sqlx::PgPool;

mod api_contract;
mod collab_api;
mod collaboration;
mod error;
mod files;
mod notify;
mod people;
mod scene;
mod search;
mod spaces;
mod time;
mod versions;
mod whiteboard;

#[cfg(test)]
mod api_tests;

#[derive(Clone)]
pub struct AppState {
    pub rooms: miy_app_collab::Hub,
    pub pool: PgPool,
    pub acl: Acl,
}

pub fn router(pool: PgPool) -> Router {
    api_contract::router().0.with_state(app_state(pool))
}

fn app_state(pool: PgPool) -> AppState {
    let acl = Acl::new(pool.clone(), spaces::acl_roles()).expect("valid space ACL roles");
    AppState {
        pool,
        acl,
        rooms: Default::default(),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if api_contract::handle_cli()? {
        return Ok(());
    }
    let pool = miy_app_sdk::database().await?;
    let state = app_state(pool);
    let router = api_contract::router().0.with_state(state.clone());
    miy_app_sdk::serve_with_shutdown(router, async move { state.rooms.shutdown(&state).await })
        .await
}
