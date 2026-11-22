//! Board (게시판): AI-DO's community board and announcements, ported from
//! AI-DO 0002af062 (see docs/PORTING.md). The SDK serves this router under
//! `/api`, so `/v1/community/channels` is reached as `/api/v1/community/...`,
//! the path AI-DO used. Binary routes under `/files/...` are reached through
//! the platform file gateway (`runtime.fileApi`).
use axum::Router;
use miy_app_sdk::acl::Acl;
use sqlx::PgPool;

mod announcements;
mod api_contract;
mod community;
mod error;
mod managers;
mod media;
mod notify;
mod password;
mod people;
mod spaces;
mod time;
mod ugc;

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
