//! Drive (드라이브): AI-DO's Files module, ported from AI-DO 0002af062 (see
//! docs/PORTING.md). The SDK serves this router under `/api`; AI-DO's
//! workspace-scoped `/api/v1/files/...` paths become
//! `/api/v1/spaces/{space_id}/files/...`. Binary routes under `/files/...`
//! are reached through the platform file gateway (`runtime.fileApi`).
use axum::Router;
use miy_app_sdk::acl::Acl;
use sqlx::PgPool;

mod access;
mod api_contract;
mod archive_planner;
mod cleanup;
mod error;
mod files;
mod notify;
mod people;
mod platform;
mod search;
mod spaces;
mod time;
mod trash;
mod tree;
mod uploads;
mod versions;

#[cfg(test)]
mod api_tests;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub acl: Acl,
}

pub fn router(pool: PgPool) -> Router {
    let acl = Acl::new(pool.clone(), access::acl_roles()).expect("valid space ACL roles");
    api_contract::router().0.with_state(AppState { pool, acl })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if api_contract::handle_cli()? {
        return Ok(());
    }
    let pool = miy_app_sdk::database().await?;
    miy_app_sdk::serve(router(pool)).await
}
