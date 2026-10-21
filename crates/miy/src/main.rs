mod auth;
mod contract;
mod domain;
mod http;
#[cfg(test)]
mod integration;
mod mcp;
mod worker;

use anyhow::Result;
use clap::{Parser, Subcommand};
use sqlx::postgres::PgPoolOptions;
use std::{path::PathBuf, sync::Arc};

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Serve,
    Worker,
    Migrate,
    Seed,
}

#[derive(Clone)]
pub struct State {
    pub pool: sqlx::PgPool,
    pub auth: Arc<auth::Auth>,
    pub artifacts: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args = Args::parse();
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    if matches!(args.command, Command::Migrate) {
        sqlx::migrate!("../../migrations/core").run(&pool).await?;
        return Ok(());
    }
    if matches!(args.command, Command::Seed) {
        return domain::seed(&pool).await;
    }
    if matches!(args.command, Command::Worker) {
        return worker::run(pool).await;
    }
    let state = State {
        pool,
        auth: Arc::new(auth::Auth::new().await?),
        artifacts: std::env::var("MIY_ARTIFACTS")?.into(),
    };
    let listener = tokio::net::TcpListener::bind("0.0.0.0:18080").await?;
    tracing::info!(event = "core_listening", port = 18080);
    axum::serve(listener, http::router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
