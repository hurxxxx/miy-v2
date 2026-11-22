//! Docs (문서): AI-DO's block-editor docs, ported from AI-DO 0002af062 (see
//! docs/PORTING.md). The SDK serves this router under `/api`, so
//! `/v1/docs/hub` is reached as `/api/v1/docs/hub`, the path AI-DO used.
//! Binary routes under `/files/...` are reached through the platform file
//! gateway (`runtime.fileApi`).
use axum::Router;
use miy_app_sdk::acl::Acl;
use sqlx::PgPool;

mod access;
mod api_contract;
mod attachments;
mod comments;
mod content_text;
mod error;
mod files;
mod items;
mod managers;
mod media;
mod notify;
mod pages;
mod people;
mod search;
mod spaces;
mod templates;
mod time;
mod trash;
mod versions;

#[cfg(test)]
mod api_tests;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub acl: Acl,
}

pub fn router(pool: PgPool) -> Router {
    let acl = Acl::new(pool.clone(), spaces::acl_roles()).expect("valid space ACL roles");
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
