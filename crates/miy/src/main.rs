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
mod cli_error;
mod cli_guidance;
mod cli_output;
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
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use cli_error::{Category, CliError};
use cli_output::OutputMode;
#[cfg(feature = "server")]
use sqlx::postgres::PgPoolOptions;
use std::io::IsTerminal;
#[cfg(feature = "server")]
use std::{path::PathBuf, sync::Arc};

#[derive(Parser)]
#[command(version)]
struct Args {
    /// Output for application commands: auto uses text on a terminal and JSON otherwise.
    #[arg(long, global = true, value_enum, default_value = "auto")]
    output: OutputMode,
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
    let argv: Vec<_> = std::env::args_os().collect();
    let terminal = std::io::stdout().is_terminal();
    let matches = match Args::command().try_get_matches_from(&argv) {
        Ok(matches) => matches,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                error.print()?;
                return Ok(());
            }
            let mode = output_hint(&argv).resolve(terminal);
            // Clap diagnostics may echo argument values. Return stable, non-secret diagnostics.
            let result = Err(CliError::new(
                Category::Usage,
                "invalid_arguments",
                "Invalid command arguments. Run miy --help or miy app <command> --help.",
            )
            .into());
            std::process::exit(cli_output::emit(mode, "miy", &result, false)?);
        }
    };
    let command_name = command_name(&matches);
    let args = Args::from_arg_matches(&matches)?;
    let mode = args.output.resolve(terminal);
    match args.command {
        Command::App { command } => {
            let raw = matches!(
                &command,
                app_cli::AppCommand::Mcp | app_cli::AppCommand::Codex { .. }
            ) || matches!(&command, app_cli::AppCommand::Describe { format } if format == "markdown");
            let result = if matches!(mode, OutputMode::Jsonl) {
                Err(CliError::new(Category::Usage, "unsupported_output", "JSONL requires an event-stream command; no application command currently supports it.").into())
            } else {
                app_cli::execute(command).await
            };
            let code = cli_output::emit(
                mode,
                &command_name,
                &result,
                raw && !matches!(mode, OutputMode::Jsonl),
            )?;
            if code != 0 {
                std::process::exit(code);
            }
            Ok(())
        }
        #[cfg(feature = "server")]
        command => {
            if args.output != OutputMode::Auto {
                let result = Err(CliError::new(
                    Category::Usage,
                    "unsupported_output",
                    "--output is supported by application commands only.",
                )
                .into());
                std::process::exit(cli_output::emit(mode, &command_name, &result, false)?);
            }
            run_server(command).await
        }
    }
}

fn command_name(matches: &clap::ArgMatches) -> String {
    let mut names = Vec::new();
    let mut current = matches;
    while let Some((name, child)) = current.subcommand() {
        names.push(name);
        current = child;
    }
    names.join(" ")
}

// Used only after failed parsing. Stop before child-command passthrough arguments.
fn output_hint(argv: &[std::ffi::OsString]) -> OutputMode {
    let mut args = argv.iter().skip(1);
    let mut mode = OutputMode::Auto;
    while let Some(arg) = args.next() {
        if arg == "--" {
            break;
        }
        let value = if arg == "--output" {
            args.next().and_then(|arg| arg.to_str())
        } else {
            arg.to_str().and_then(|arg| arg.strip_prefix("--output="))
        };
        if let Some(value) = value {
            mode = match value {
                "text" => OutputMode::Text,
                "json" => OutputMode::Json,
                "jsonl" => OutputMode::Jsonl,
                _ => OutputMode::Auto,
            };
        }
    }
    mode
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

    #[test]
    fn output_hint_stops_at_child_passthrough_boundary() {
        let argv: Vec<_> = [
            "miy",
            "--output=json",
            "app",
            "codex",
            "--",
            "--output",
            "text",
        ]
        .into_iter()
        .map(std::ffi::OsString::from)
        .collect();
        assert_eq!(output_hint(&argv), OutputMode::Json);
    }
}
