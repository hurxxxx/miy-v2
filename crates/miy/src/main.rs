mod agent_tools;
mod app_cli;
mod auth;
mod bundle;
mod cli_auth;
mod cli_client;
mod cli_guidance;
mod component_cli;
mod component_registry;
mod connections;
mod contract;
mod delivery;
mod dev_tools;
mod domain;
mod http;
#[cfg(test)]
mod integration;
mod mcp;
mod preview;
mod registration;
mod runtime;
mod upgrade;
mod worker;

use anyhow::Result;
use clap::{Parser, Subcommand};
use sqlx::postgres::PgPoolOptions;
use std::{path::PathBuf, sync::Arc};

#[derive(Parser)]
#[command(version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    App {
        #[command(subcommand)]
        command: app_cli::AppCommand,
    },
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
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args = Args::parse();
    if let Command::App { command } = args.command {
        if let Err(error) = app_cli::execute(command).await {
            eprintln!(
                "{}",
                serde_json::json!({"error":{"message":format!("{error:#}")}})
            );
            std::process::exit(1);
        }
        return Ok(());
    }
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    if matches!(args.command, Command::Migrate) {
        sqlx::migrate!("../../migrations/core").run(&pool).await?;
        return Ok(());
    }
    if matches!(args.command, Command::Seed) {
        domain::seed(&pool).await?;
        return component_registry::seed(&pool).await;
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
