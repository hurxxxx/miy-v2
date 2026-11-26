use crate::{
    bundle,
    cli_error::{Category, CliError},
    cli_output::Outcome,
    contract::{self, Manifest},
};
use anyhow::{Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(clap::Args)]
pub struct RemovedArgs {
    #[arg(num_args=0..,trailing_var_arg=true,allow_hyphen_values=true)]
    arguments: Vec<String>,
}
#[derive(Subcommand)]
pub enum AppCommand {
    #[command(flatten)]
    Remote(crate::cli_remote::RemoteAppCommand),
    /// Create an independent Rust + React application.
    Init {
        path: PathBuf,
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        ca_file: Option<PathBuf>,
    },
    /// Transactionally update the compatible project CLI, guidance and MCP settings.
    #[command(
        long_about = "Copy this executable and its CLI guidance into the current compatible project, and configure Codex/Claude MCP. First update the global CLI using your server's installer, then run the new global miy app setup here. This command does not download a newer CLI or reset authentication. Restart the agent afterward. See miy describe --topic updates; a different SDK bundle requires an explicit app upgrade."
    )]
    Setup,
    /// Upgrade a supported SDK bundle while preserving business code and applied SQL.
    Upgrade,
    /// Check app integration, managed files, components and both language toolchains.
    Check {
        /// Review app source integration offline without compiling or running the app.
        #[arg(long)]
        harness_only: bool,
    },
    Fmt,
    /// Run local development against the server's development database.
    Dev {
        #[arg(long, default_value = "18100")]
        port: u16,
        #[arg(long)]
        no_open: bool,
        /// Use the server development DB, or explicitly select an isolated local DB.
        #[arg(long, value_enum, default_value = "server")]
        database: crate::development_client::DatabaseMode,
        /// Run Rust directly, or explicitly use a Linux Docker engine.
        #[arg(long, value_enum, default_value = "native")]
        runtime: crate::preview::Runtime,
    },
    /// Stop this project's owned development processes; preserve its database volume.
    DevStop,
    /// Print this committed app's ID, version, source commit and server archive SHA-256 offline.
    SourcePin,
    /// Download or restore the exact source of an existing app release.
    #[command(subcommand)]
    Source(crate::cli_release_source::SourceCommand),
    /// Build clean committed source on the selected server. The server runs the format
    /// check, tests and integrity checks; only its builds can be deployed. For local
    /// testing use `app dev`.
    Build {
        #[arg(long)]
        no_wait: bool,
        #[arg(long, default_value = "1800", value_parser=clap::value_parser!(u64).range(1..=86400))]
        timeout: u64,
    },
    /// Request administrator approval for this project's application.
    Register,
    /// Change only the platform display name (administrator).
    Rename {
        #[arg(long)]
        name: String,
    },
    /// Record evidence for a dev release before promotion.
    Accept {
        /// Inspect the selected dev release, version and evidence without recording acceptance.
        #[arg(long)]
        dry_run: bool,
        #[arg(long, conflicts_with = "input")]
        note: Option<String>,
        #[arg(long, conflicts_with = "input")]
        release: Option<uuid::Uuid>,
        /// Strict JSON {releaseId,checks:[{scope,actor,result,evidence}]}.
        #[arg(long,conflicts_with_all=["input","release"])]
        report: Option<PathBuf>,
        /// Strict JSON {releaseId,note,checks} from @file or stdin (-).
        #[arg(long,conflicts_with_all=["note","release","report"])]
        input: Option<String>,
    },
    #[command(hide = true)]
    Login(RemovedArgs),
    #[command(hide = true)]
    Logout(RemovedArgs),
    #[command(hide = true)]
    Doctor(RemovedArgs),
    #[command(hide = true)]
    Describe(RemovedArgs),
    #[command(hide = true)]
    Components(RemovedArgs),
    #[command(hide = true)]
    Mcp(RemovedArgs),
    #[command(hide = true)]
    Registrations(RemovedArgs),
    #[command(hide = true)]
    Approve(RemovedArgs),
    #[command(hide = true)]
    Reject(RemovedArgs),
    #[command(hide = true)]
    Users(RemovedArgs),
    #[command(hide = true)]
    Grant(RemovedArgs),
    #[command(hide = true)]
    Resume(RemovedArgs),
    #[command(hide = true)]
    Control(RemovedArgs),
    #[command(hide = true)]
    SetupCodex(RemovedArgs),
    #[command(hide = true)]
    Codex(RemovedArgs),
}
#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct Session {
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
    pub origin: String,
    #[serde(default)]
    pub cookie: String,
    #[serde(default)]
    pub csrf: String,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub ca_file: Option<PathBuf>,
}
pub(crate) fn run(program: &str, args: &[&str]) -> Result<()> {
    let mut command = crate::platform::command(program, args)?;
    command
        .env("CI", "true")
        .stdout(std::process::Stdio::from(std::io::stderr()));
    let status = crate::owned_process::status(&mut command)
        .with_context(|| format!("cannot_run_{program}"))?;
    anyhow::ensure!(
        status.success(),
        CliError::new(
            Category::Environment,
            "command_failed",
            format!("{program} failed; inspect its diagnostic output.")
        )
    );
    Ok(())
}
pub(crate) fn output(program: &str, args: &[&str]) -> Result<String> {
    let out = crate::owned_process::output(&mut crate::platform::command(program, args)?)?;
    anyhow::ensure!(
        out.status.success(),
        CliError::new(
            Category::Environment,
            "command_failed",
            format!("{program} failed.")
        )
    );
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}
pub(crate) fn manifest() -> Result<Manifest> {
    let m: Manifest = serde_json::from_slice(&std::fs::read("manifest.json").map_err(|_| {
        CliError::new(
            Category::Environment,
            "manifest_unavailable",
            "Run inside an app directory with a readable manifest.json.",
        )
    })?)
    .map_err(|_| {
        CliError::new(
            Category::Environment,
            "invalid_manifest",
            "manifest.json must match the application manifest schema.",
        )
    })?;
    contract::validate(&m)
        .map_err(|message| CliError::new(Category::Environment, "invalid_manifest", message))?;
    Ok(m)
}
pub(crate) fn session() -> Result<Session> {
    crate::cli_login::session()
}
pub(crate) async fn api(s: &Session, path: &str, body: Option<Value>) -> Result<Value> {
    api_method(
        s,
        path,
        if body.is_some() {
            reqwest::Method::POST
        } else {
            reqwest::Method::GET
        },
        body,
    )
    .await
}
pub(crate) async fn api_method(
    s: &Session,
    path: &str,
    method: reqwest::Method,
    body: Option<Value>,
) -> Result<Value> {
    let s = crate::cli_login::refreshed(s).await?;
    let client = crate::cli_client::client(s.ca_file.as_deref(), 20)?;
    let mut req = client.request(method, format!("{}{path}", s.origin));
    if let Some(body) = body {
        req = req.json(&body);
    }
    req = crate::cli_client::authenticate(&s, req);
    crate::cli_error::response(req.send().await?).await
}
pub(crate) fn save(path: &str, value: &Value) -> Result<()> {
    crate::platform::write_private(Path::new(path), &serde_json::to_vec_pretty(value)?)
}
pub(crate) fn check_project() -> Result<Manifest> {
    let m = manifest()?;
    crate::cli_error::validation(
        crate::app_harness::verify_api_files(Path::new("."), &m),
        "app_api_contract_invalid",
    )?;
    crate::cli_error::validation(bundle::verify(Path::new(".")), "bundle_invalid")?;
    crate::cli_error::validation(
        crate::cli_guidance::verify(Path::new(".")),
        "cli_guidance_invalid",
    )?;
    crate::component_cli::verify(Path::new("."))?;
    for path in [
        "src/main.rs",
        "web/src/AppRoot.tsx",
        "Cargo.lock",
        "pnpm-lock.yaml",
    ] {
        anyhow::ensure!(
            Path::new(path).is_file(),
            CliError::new(
                Category::Environment,
                "missing_bundle_file",
                format!("missing_bundle_file: {path}")
            )
        );
    }
    crate::cli_error::validation(
        bundle::migrations(Path::new("migrations")),
        "migration_contract_invalid",
    )?;
    Ok(m)
}
pub(crate) fn check() -> Result<Manifest> {
    let m = check_project()?;
    run("cargo", &["fmt", "--all", "--check"])?;
    run("cargo", &["test", "--locked"])?;
    run("pnpm", &["install", "--frozen-lockfile"])?;
    if m.api_contract.is_some() {
        run(
            "cargo",
            &[
                "run",
                "--locked",
                "--bin",
                "miy-app",
                "--",
                "--check-openapi",
            ],
        )?;
        run("node", &["web/api-tools/generate.mjs", "--check"])?;
    }
    run("pnpm", &["typecheck"])?;
    run("pnpm", &["build"])?;
    crate::cli_error::validation(bundle::ui(Path::new("dist/ui")), "ui_invalid")?;
    Ok(m)
}
fn removed(args: &[&str]) -> Result<Outcome> {
    Err(CliError::new(Category::Usage,"removed_command","This command was removed. Use the replacement shown in recovery; launch Codex or Claude directly after app setup.").with_recovery("miy",args).into())
}
pub(crate) fn selected_app() -> Result<String> {
    let app = if let Some(app) = &crate::cli_config::options().app {
        app.clone()
    } else {
        manifest()?.id
    };
    anyhow::ensure!(
        contract::valid_id(&app),
        CliError::new(
            Category::Usage,
            "invalid_app_id",
            "Use a valid app identifier."
        )
    );
    Ok(app)
}
async fn accept(
    dry_run: bool,
    note: Option<String>,
    release: Option<uuid::Uuid>,
    report: Option<PathBuf>,
    input: Option<String>,
) -> Result<Outcome> {
    use crate::acceptance_contract::{AcceptanceCheck, ReleaseCheck};

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Report {
        release_id: uuid::Uuid,
        checks: Vec<AcceptanceCheck>,
    }

    let (release, note, checks) = if let Some(input) = input {
        let value: ReleaseCheck = crate::cli_input::read(&input)?;
        (Some(value.release_id), Some(value.note), value.checks)
    } else if let Some(path) = report {
        let report: Report = crate::cli_input::read(&format!("@{}", path.display()))?;
        anyhow::ensure!(
            !report.checks.is_empty(),
            CliError::new(
                Category::Usage,
                "acceptance_checks_required",
                "The report must include acceptance checks."
            )
        );
        (Some(report.release_id), note, report.checks)
    } else {
        (release, note, Vec::new())
    };

    if !dry_run && release.is_none() {
        return Err(CliError::new(Category::Usage, "acceptance_release_required",
            "Review the dev release with app accept --dry-run, then pass --release, --report or --input.").into());
    }
    if note.is_none() && (!dry_run || !checks.is_empty()) {
        return Err(CliError::new(
            Category::Usage,
            "acceptance_note_required",
            "Provide --note or a complete --input document.",
        )
        .into());
    }
    let app = selected_app()?;
    let s = session()?;
    let (release_id, target) = if dry_run {
        let detail: crate::api_contract::AppDetail =
            serde_json::from_value(api(&s, &format!("/api/v1/apps/{app}"), None).await?)?;
        let dev = detail
            .installations
            .into_iter()
            .find(|i| i.environment == crate::runtime_contract::Environment::Dev)
            .ok_or_else(|| {
                CliError::new(
                    Category::Environment,
                    "dev_release_required",
                    "Deploy to dev before accepting a release.",
                )
            })?;
        let target = json!({"appId":app,"releaseId":dev.release_id,"version":dev.version,"sourceCommit":dev.source_commit,
            "sourceInLocalHistory":dev.source_commit.as_deref().and_then(|commit|manifest().ok().filter(|m|m.id==app).and_then(|_|crate::cli_build::source_in_local_history(commit)))});
        if release.is_some_and(|id| id != dev.release_id) {
            return Err(CliError::new(
                Category::Conflict,
                "dev_release_changed",
                "The supplied release differs from the current dev installation.",
            )
            .with_details(target)
            .into());
        }
        if note.is_none() {
            return Ok(Outcome::Data(
                json!({"target":target,"acceptance":null,"dryRun":true}),
            ));
        }
        (dev.release_id, Some(target))
    } else {
        (release.context("missing explicit acceptance target")?, None)
    };
    let value = ReleaseCheck {
        release_id,
        note: note.context("missing acceptance note")?,
        checks,
    };
    crate::acceptance_contract::validate(&value).map_err(|code| {
        CliError::new(
            Category::Usage,
            code,
            "Acceptance input must contain a release, note and valid evidence.",
        )
    })?;
    if let Some(target) = target {
        return Ok(Outcome::Data(
            json!({"target":target,"acceptance":value,"dryRun":true}),
        ));
    }
    // The server compares this exact ID to the healthy dev release under its deployment lock.
    Ok(Outcome::Data(
        api(
            &s,
            &format!("/api/v1/apps/{app}/test-acceptance"),
            Some(serde_json::to_value(value)?),
        )
        .await?,
    ))
}

pub async fn execute(command: AppCommand) -> Result<Outcome> {
    match command {
        AppCommand::Remote(command) => {
            return Box::pin(crate::cli_remote::execute_app(command)).await;
        }
        AppCommand::Login(_) => return removed(&["auth", "login"]),
        AppCommand::Logout(_) => return removed(&["auth", "logout"]),
        AppCommand::Doctor(_) => return removed(&["doctor", "--for", "develop"]),
        AppCommand::Describe(_) => return removed(&["describe"]),
        AppCommand::Components(_) => return removed(&["component", "--help"]),
        AppCommand::Mcp(_) => return removed(&["mcp"]),
        AppCommand::Registrations(_) => return removed(&["registration", "list"]),
        AppCommand::Approve(_) => return removed(&["registration", "approve", "--help"]),
        AppCommand::Reject(_) => return removed(&["registration", "reject", "--help"]),
        AppCommand::Users(_) => return removed(&["user", "list"]),
        AppCommand::Grant(_) => return removed(&["grant", "--help"]),
        AppCommand::Resume(_) => return removed(&["job", "resume", "--help"]),
        AppCommand::Control(_) => return removed(&["app", "--help"]),
        AppCommand::SetupCodex(_) | AppCommand::Codex(_) => return removed(&["app", "setup"]),
        AppCommand::Dev {
            port,
            no_open,
            database,
            runtime,
        } => crate::preview::run(port, no_open, database, runtime).await?,
        AppCommand::DevStop => crate::preview::stop().await?,
        AppCommand::SourcePin => {
            // The scratch directory contains the full proprietary app source.
            let temp = tempfile::tempdir_in(crate::cli_config::ensure_root()?)?;
            let source =
                crate::cli_build::pack_source(Path::new("."), &temp.path().join("source.tar"))?;
            return Ok(Outcome::Data(json!({
                "appId": source.manifest.id, "version": source.manifest.version,
                "sourceCommit": source.source_commit, "archiveSha256": source.archive_hash,
            })));
        }
        AppCommand::Source(command) => return crate::cli_release_source::execute(command).await,
        AppCommand::Upgrade => return Ok(Outcome::Data(crate::upgrade::run().await?)),
        AppCommand::Setup => {
            return Ok(Outcome::Data(crate::agent_tools::setup(
                Path::new("."),
                None,
            )?));
        }
        AppCommand::Rename { name } => {
            let app = selected_app()?;
            return Ok(Outcome::Data(
                api(
                    &session()?,
                    &format!("/api/v1/apps/{app}/name"),
                    Some(json!({"name":name})),
                )
                .await?,
            ));
        }
        AppCommand::Accept {
            dry_run,
            note,
            release,
            report,
            input,
        } => {
            return accept(dry_run, note, release, report, input).await;
        }
        AppCommand::Init {
            path,
            id,
            name,
            url,
            ca_file,
        } => {
            anyhow::ensure!(
                contract::valid_id(&id) && !name.trim().is_empty(),
                CliError::new(
                    Category::Usage,
                    "invalid_app_identity",
                    "invalid_app_identity"
                )
            );
            anyhow::ensure!(
                !path.exists() || std::fs::read_dir(&path)?.next().is_none(),
                CliError::new(
                    Category::Conflict,
                    "destination_must_be_empty",
                    "destination_must_be_empty"
                )
            );
            let selected = crate::cli_config::optional_selected()?;
            let (url, ca_file) = if let Some((_, context)) = &selected {
                if let Some(ref explicit) = url {
                    anyhow::ensure!(
                        crate::cli_client::origin(explicit)? == context.origin,
                        CliError::new(
                            Category::Conflict,
                            "server_context_mismatch",
                            "The supplied URL differs from the selected context. Add and select the intended context first."
                        )
                    );
                }
                (Some(context.origin.clone()), context.ca_file.clone())
            } else {
                (url, ca_file)
            };
            let url = url.as_deref().map(crate::cli_client::origin).transpose()?;
            crate::cli_client::client(ca_file.as_deref(), 20)?;
            std::fs::create_dir_all(&path)?;
            for (relative, content) in bundle::template() {
                let target = path.join(relative);
                std::fs::create_dir_all(target.parent().unwrap())?;
                let content = content
                    .replace("{{APP_ID}}", &id)
                    .replace("{{APP_MODULE}}", &id.replace('-', "_"))
                    .replace("{{APP_NAME}}", &name);
                std::fs::write(target, content)?;
            }
            // Serialize user-supplied names without relying on text-template escaping.
            let mut m: Value = serde_json::from_str(
                bundle::template()
                    .iter()
                    .find(|(p, _)| *p == "manifest.json")
                    .unwrap()
                    .1,
            )?;
            m["id"] = json!(id);
            m["name"] = json!(name);
            std::fs::write(
                path.join("manifest.json"),
                serde_json::to_string_pretty(&m)?,
            )?;
            std::fs::write(
                path.join("miy.bundle.json"),
                serde_json::to_string_pretty(
                    &json!({"bundleVersion":bundle::VERSION,"protectedFiles":bundle::protected_hashes()}),
                )?,
            )?;
            std::fs::create_dir_all(path.join(".miy-tools"))?;
            crate::platform::install_executable(
                &std::env::current_exe()?,
                &crate::platform::project_cli(&path),
            )?;
            if let Some(ref url) = url {
                crate::cli_client::configure(&path, url, ca_file.as_deref())?;
            }
            crate::cli_config::bind_project(&path)?;
            let template_version = bundle::template_version();
            crate::agent_tools::setup(&path, Some(&template_version))?;
            let mut next = Vec::new();
            if selected.is_none() {
                next.push("miy context init (enter the target server URL)");
            }
            next.extend([
                "Read docs/API.md before implementing app API changes",
                "miy auth login",
                "miy component sync",
                "miy doctor --for develop",
                "miy app register (administrator approval required before app dev)",
                "codex or claude (approve project trust/MCP when prompted)",
                ".miy-tools/miy app check",
            ]);
            return Ok(Outcome::Data(
                json!({"created":path.canonicalize()?,"cliVersion":crate::cli_guidance::VERSION,"guidanceVersion":crate::cli_guidance::GUIDANCE_VERSION,"templateVersion":template_version,"cliSkill":".agents/skills/miy-cli/SKILL.md","cliManual":"docs/CLI-REFERENCE.md","apiGuide":crate::cli_guidance::API_GUIDE_PATH,"appId":id,"bundleVersion":bundle::VERSION,"components":"Declared in miy.components.json; fetch after login","next":next}),
            ));
        }
        AppCommand::Fmt => run("cargo", &["fmt", "--package", "miy-app"])?,
        AppCommand::Check { harness_only } => {
            let m = if harness_only {
                let manifest = manifest()?;
                crate::cli_error::validation(
                    crate::app_harness::verify_api_files(Path::new("."), &manifest),
                    "app_api_contract_invalid",
                )?;
                manifest
            } else {
                check()?
            };
            let findings = crate::cli_error::validation(
                crate::app_harness::inspect(Path::new("."), &m),
                "app_harness_unavailable",
            )?;
            let guidance = crate::cli_error::validation(
                crate::cli_guidance::versions(Path::new(".")),
                "cli_guidance_invalid",
            )?;
            if harness_only {
                return Ok(Outcome::Data(
                    json!({"appId":m.id,"mode":"harness-only","guidance":guidance,"findings":findings,
                        "warnings": if guidance["setupRequired"] == true { vec!["app dev and full app check require current CLI guidance. Run the global miy app setup and restart the project MCP."] } else { vec![] }}),
                ));
            }
            let mut checks = vec![
                "manifest",
                "bundle-integrity",
                "cli-guidance-integrity",
                "component-integrity",
                "migration-contract",
                "rust-format",
                "rust-tests",
                "frozen-dependencies",
                "typescript",
                "ui-build",
                "federation-assets",
            ];
            if m.api_contract.is_some() {
                checks.push("app-api-contract");
            }
            return Ok(Outcome::Data(
                json!({"valid":true,"appId":m.id,"bundleVersion":bundle::VERSION,"checks":checks,"guidance":guidance,"harnessFindings":findings,"browserIntegration":"requires platform acceptance test"}),
            ));
        }
        AppCommand::Build { no_wait, timeout } => {
            return crate::cli_build::execute(no_wait, timeout).await;
        }
        AppCommand::Register => {
            let m = manifest()?;
            return Ok(Outcome::Data(
                api(
                    &session()?,
                    "/api/v1/app-registrations",
                    Some(json!({"id":m.id,"name":m.name})),
                )
                .await?,
            ));
        }
    }
    Ok(Outcome::Data(json!({"completed":true})))
}
