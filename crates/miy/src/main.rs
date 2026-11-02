#[cfg(all(windows, feature = "server"))]
compile_error!(
    "The server requires Linux/Unix; build the Windows client with --no-default-features."
);

mod acceptance_contract;
mod agent_tools;
mod api_error;
mod app_cli;
mod app_health;
#[cfg(feature = "server")]
mod auth;
#[cfg(feature = "server")]
mod auth_sessions;
#[cfg(all(test, feature = "server"))]
mod auth_sessions_regression;
mod bundle;
#[cfg(feature = "server")]
mod cli_auth;
mod cli_build;
mod cli_client;
mod cli_config;
mod cli_discovery;
mod cli_doctor;
mod cli_error;
mod cli_guidance;
mod cli_input;
#[cfg(feature = "server")]
mod cli_install;
mod cli_journal;
mod cli_login;
mod cli_output;
#[cfg(all(test, feature = "server"))]
mod cli_regression;
mod cli_remote;
mod cli_upload;
mod component_cli;
mod component_contract;
#[cfg(feature = "server")]
mod component_registry;
#[cfg(feature = "server")]
mod connections;
mod contract;
mod credential_store;
mod db_tunnel;
#[cfg(feature = "server")]
mod delivery;
#[cfg(feature = "server")]
mod development;
mod development_client;
mod docker;
#[cfg(feature = "server")]
mod domain;
mod file_transaction;
mod helper_image;
#[cfg(feature = "server")]
mod http;
mod identity;
mod image_contract;
#[cfg(feature = "server")]
mod image_import;
#[cfg(all(test, feature = "server"))]
mod image_import_regression;
#[cfg(all(test, feature = "server"))]
mod integration;
#[cfg(feature = "server")]
mod job_api;
#[cfg(feature = "server")]
mod mcp;
mod native_preview;
#[cfg(all(test, feature = "server"))]
mod operations_regression;
mod owned_process;
mod platform;
mod preview;
#[cfg(feature = "server")]
mod read_api;
#[cfg(feature = "server")]
mod registration;
#[cfg(feature = "server")]
mod runtime;
mod runtime_contract;
#[cfg(feature = "server")]
mod runtime_health;
#[cfg(feature = "server")]
mod runtime_logs;
mod runtime_platform;
#[cfg(feature = "server")]
mod runtime_preflight;
#[cfg(all(test, feature = "server"))]
mod runtime_regression;
#[cfg(feature = "server")]
mod server_build;
#[cfg(all(test, feature = "server"))]
mod server_build_regression;
#[cfg(feature = "server")]
mod server_build_worker;
#[cfg(feature = "server")]
mod server_origin;
mod source_archive;
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
    #[command(flatten)]
    options: cli_config::Options,
    /// Output for application commands: auto uses text on a terminal and JSON otherwise.
    #[arg(long, global = true, value_enum, default_value = "auto")]
    output: OutputMode,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Manage named server contexts and the active server.
    Context {
        #[command(subcommand)]
        command: cli_config::ContextCommand,
    },
    /// Sign in and manage user sessions for the selected context.
    Auth {
        #[command(subcommand)]
        command: cli_login::AuthCommand,
    },
    /// Diagnose the requirements for remote operation or local development.
    Doctor(cli_doctor::DoctorArgs),
    /// Discover command inputs, outputs, permissions and effects.
    Describe(cli_discovery::DescribeArgs),
    /// Generate completion for a supported shell.
    Completion {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Discover and synchronize reusable components.
    Component {
        #[command(subcommand)]
        command: component_cli::ComponentCommand,
    },
    /// Inspect and recover remote jobs.
    Job {
        #[command(subcommand)]
        command: cli_remote::JobCommand,
    },
    Registration {
        #[command(subcommand)]
        command: cli_remote::RegistrationCommand,
    },
    User {
        #[command(subcommand)]
        command: cli_remote::UserCommand,
    },
    Grant {
        #[command(subcommand)]
        command: cli_remote::GrantCommand,
    },
    /// Serve the project read-only JSON-RPC MCP interface over stdio.
    Mcp,
    /// Internal Docker helper; configured only by app dev.
    #[command(hide = true)]
    InternalDbTunnel,
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
    if mode == OutputMode::Jsonl
        && !matches!(
            &args.command,
            Command::Completion { .. }
                | Command::Job {
                    command: cli_remote::JobCommand::Watch { .. }
                }
        )
    {
        let result = Err(CliError::new(
            Category::Usage,
            "unsupported_output",
            "This command does not provide a JSONL event stream.",
        )
        .into());
        std::process::exit(cli_output::emit(mode, &command_name, &result, false)?);
    }
    if let Err(error) = cli_config::initialize(args.options) {
        std::process::exit(cli_output::emit(mode, &command_name, &Err(error), false)?);
    }
    if let Some(fields) = &cli_config::options().fields
        && let Err(error) = cli_input::validate_fields(&command_name, fields)
    {
        std::process::exit(cli_output::emit(mode, &command_name, &Err(error), false)?);
    }
    let raw = matches!(
        &args.command,
        Command::Mcp | Command::InternalDbTunnel | Command::Completion { .. }
    );
    let result = match args.command {
        Command::Context { command } => cli_config::execute(command).map(cli_output::Outcome::Data),
        Command::Auth { command } => Box::pin(cli_login::execute(command))
            .await
            .map(cli_output::Outcome::Data),
        Command::Component { command } => Box::pin(component_cli::execute(command))
            .await
            .map(cli_output::Outcome::Data),
        Command::Job { command } => Box::pin(cli_remote::execute_job(command, mode)).await,
        Command::Registration { command } => {
            Box::pin(cli_remote::execute_registration(command)).await
        }
        Command::User { command } => Box::pin(cli_remote::execute_user(command)).await,
        Command::Grant { command } => Box::pin(cli_remote::execute_grant(command)).await,
        Command::App { command } => Box::pin(app_cli::execute(command)).await,
        Command::Doctor(options) => {
            Box::pin(cli_doctor::inspect(options))
                .await
                .and_then(|value| {
                    if value["ready"] == true {
                        Ok(cli_output::Outcome::Data(value))
                    } else {
                        Err(CliError::new(
                            Category::Environment,
                            "diagnostic_failed",
                            "Some required conditions are not ready.",
                        )
                        .with_details(value)
                        .into())
                    }
                })
        }
        Command::Describe(options) => {
            cli_discovery::inspect(options).map(cli_output::Outcome::Data)
        }
        Command::Completion { shell } => {
            cli_discovery::completion(shell).map(|()| cli_output::Outcome::Raw)
        }
        Command::Mcp => Box::pin(agent_tools::serve())
            .await
            .map(|()| cli_output::Outcome::Raw),
        Command::InternalDbTunnel => Box::pin(db_tunnel::run())
            .await
            .map(|()| cli_output::Outcome::Raw),
        #[cfg(feature = "server")]
        command => {
            if args.output != OutputMode::Auto {
                Err(CliError::new(
                    Category::Usage,
                    "unsupported_output",
                    "--output is supported by client commands only.",
                )
                .into())
            } else {
                return Box::pin(run_server(command)).await;
            }
        }
    };
    let code = cli_output::emit(mode, &command_name, &result, raw)?;
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
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
    let bind = std::env::var("MIY_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:18080".into());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(event = "core_listening", address = %bind);
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
