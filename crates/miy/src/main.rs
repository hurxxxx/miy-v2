#[cfg(all(windows, feature = "server"))]
compile_error!(
    "The server requires Linux/Unix; build the Windows client with --no-default-features."
);

mod agent_tools;
mod api_error;
mod app_cli;
mod app_health;
#[cfg(feature = "server")]
mod auth;
mod bundle;
#[cfg(feature = "server")]
mod cli_auth;
mod cli_client;
mod cli_guidance;
mod component_cli;
mod component_contract;
#[cfg(feature = "server")]
mod component_registry;
#[cfg(feature = "server")]
mod connections;
mod contract;
#[cfg(feature = "server")]
mod delivery;
mod dev_tools;
mod docker;
#[cfg(feature = "server")]
mod domain;
#[cfg(feature = "server")]
mod http;
mod identity;
mod image_contract;
#[cfg(all(test, feature = "server"))]
mod integration;
#[cfg(feature = "server")]
mod mcp;
mod platform;
mod preview;
#[cfg(feature = "server")]
mod registration;
#[cfg(feature = "server")]
mod runtime;
#[cfg(feature = "server")]
mod runtime_health;
#[cfg(all(test, feature = "server"))]
mod runtime_regression;
mod upgrade;
#[cfg(feature = "server")]
mod worker;

use anyhow::Result;
use clap::{Parser, Subcommand};
#[cfg(feature = "server")]
use sqlx::postgres::PgPoolOptions;
#[cfg(feature = "server")]
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
    #[cfg(feature = "server")]
    Serve,
    #[cfg(feature = "server")]
    Worker,
    #[cfg(feature = "server")]
    Migrate,
    #[cfg(feature = "server")]
    Seed,
}

#[cfg(feature = "server")]
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
    match args.command {
        Command::App { command } => {
            if let Err(error) = app_cli::execute(command).await {
                eprintln!(
                    "{}",
                    serde_json::json!({"error":{"message":format!("{error:#}")}})
                );
                std::process::exit(1);
            }
            Ok(())
        }
        #[cfg(feature = "server")]
        command => run_server(command).await,
    }
}

#[cfg(feature = "server")]
async fn run_server(command: Command) -> Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    if matches!(command, Command::Migrate) {
        sqlx::migrate!("../../migrations/core").run(&pool).await?;
        return Ok(());
    }
    if matches!(command, Command::Seed) {
        domain::seed(&pool).await?;
        return component_registry::seed(&pool).await;
    }
    if matches!(command, Command::Worker) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn server_commands_follow_build_feature() {
        let command = Args::command();
        for name in ["serve", "worker", "migrate", "seed"] {
            assert_eq!(
                command.find_subcommand(name).is_some(),
                cfg!(feature = "server")
            );
        }
        assert!(command.find_subcommand("app").is_some());
    }
}
