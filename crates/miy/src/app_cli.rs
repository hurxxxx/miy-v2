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
    Setup,
    /// Upgrade a supported SDK bundle while preserving business code and applied SQL.
    Upgrade,
    /// Check manifest, managed files, components and both language toolchains.
    Check,
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
    },
    /// Stop this project's owned development processes; preserve its database volume.
    DevStop,
    /// Build an immutable Linux image for the selected server platform.
    Build {
        #[arg(long,value_parser=["linux/amd64","linux/arm64"])]
        platform: Option<String>,
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
fn output(program: &str, args: &[&str]) -> Result<String> {
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
pub(crate) fn check() -> Result<Manifest> {
    let m = manifest()?;
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
    run("cargo", &["fmt", "--all", "--check"])?;
    run("cargo", &["test", "--locked"])?;
    run("pnpm", &["install", "--frozen-lockfile"])?;
    run("pnpm", &["typecheck"])?;
    run("pnpm", &["build"])?;
    crate::cli_error::validation(bundle::ui(Path::new("dist/ui")), "ui_invalid")?;
    Ok(m)
}
fn removed(args: &[&str]) -> Result<Outcome> {
    Err(CliError::new(Category::Usage,"removed_command","This command was removed. Use the replacement shown in recovery; launch Codex or Claude directly after app setup.").with_recovery("miy",args).into())
}
fn selected_app() -> Result<String> {
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
        } => crate::preview::run(port, no_open, database).await?,
        AppCommand::DevStop => crate::preview::stop().await?,
        AppCommand::Upgrade => return Ok(Outcome::Data(crate::upgrade::run().await?)),
        AppCommand::Setup => return Ok(Outcome::Data(crate::agent_tools::setup(Path::new("."))?)),
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
            note,
            release,
            report,
            input,
        } => {
            let app = selected_app()?;
            let s = session()?;
            let value: crate::acceptance_contract::ReleaseCheck = if let Some(input) = input {
                crate::cli_input::read(&input)?
            } else {
                let note = note.ok_or_else(|| {
                    CliError::new(
                        Category::Usage,
                        "acceptance_note_required",
                        "Provide --note or a complete --input document.",
                    )
                })?;
                if let Some(report) = report {
                    #[derive(Deserialize)]
                    #[serde(rename_all = "camelCase", deny_unknown_fields)]
                    struct Report {
                        release_id: uuid::Uuid,
                        checks: Vec<crate::acceptance_contract::AcceptanceCheck>,
                    }
                    let r: Report = crate::cli_input::read(&format!("@{}", report.display()))?;
                    anyhow::ensure!(
                        !r.checks.is_empty(),
                        CliError::new(
                            Category::Usage,
                            "acceptance_checks_required",
                            "The report must include acceptance checks."
                        )
                    );
                    crate::acceptance_contract::ReleaseCheck {
                        release_id: r.release_id,
                        note,
                        checks: r.checks,
                    }
                } else {
                    let release_id = if let Some(release) = release {
                        release
                    } else {
                        let detail = api(&s, &format!("/api/v1/apps/{app}"), None).await?;
                        let id = detail["installations"]
                            .as_array()
                            .and_then(|items| items.iter().find(|v| v["environment"] == "dev"))
                            .and_then(|v| v["releaseId"].as_str())
                            .ok_or_else(|| {
                                CliError::new(
                                    Category::Environment,
                                    "dev_release_required",
                                    "Deploy to dev before accepting a release.",
                                )
                            })?;
                        uuid::Uuid::parse_str(id)?
                    };
                    crate::acceptance_contract::ReleaseCheck {
                        release_id,
                        note,
                        checks: vec![],
                    }
                }
            };
            crate::acceptance_contract::validate(&value).map_err(|code| {
                CliError::new(
                    Category::Usage,
                    code,
                    "Acceptance input must contain a release, note and valid evidence.",
                )
            })?;
            return Ok(Outcome::Data(
                api(
                    &s,
                    &format!("/api/v1/apps/{app}/test-acceptance"),
                    Some(serde_json::to_value(value)?),
                )
                .await?,
            ));
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
            crate::agent_tools::setup(&path)?;
            let mut next = Vec::new();
            if selected.is_none() {
                next.push("miy context init (enter the target server URL)");
            }
            next.extend([
                "miy auth login",
                "miy component sync",
                "miy doctor --for develop",
                "miy app register (administrator approval required before app dev)",
                "codex or claude (approve project trust/MCP when prompted)",
                ".miy-tools/miy app check",
            ]);
            return Ok(Outcome::Data(
                json!({"created":path.canonicalize()?,"cliVersion":crate::cli_guidance::VERSION,"cliSkill":".agents/skills/miy-cli/SKILL.md","cliManual":"docs/CLI-REFERENCE.md","appId":id,"bundleVersion":bundle::VERSION,"components":"Declared in miy.components.json; fetch after login","next":next}),
            ));
        }
        AppCommand::Fmt => run("cargo", &["fmt", "--package", "miy-app"])?,
        AppCommand::Check => {
            let m = check()?;
            return Ok(Outcome::Data(
                json!({"valid":true,"appId":m.id,"bundleVersion":bundle::VERSION,"checks":["manifest","bundle-integrity","cli-guidance-integrity","component-integrity","migration-contract","rust-format","rust-tests","frozen-dependencies","typescript","ui-build","federation-assets"],"browserIntegration":"requires platform acceptance test"}),
            ));
        }
        AppCommand::Build { platform } => {
            let platform = crate::runtime_platform::build_target(platform.as_deref()).await?;
            let m = manifest()?;
            let commit = output("git", &["rev-parse", "HEAD"])?;
            anyhow::ensure!(
                output("git", &["status", "--porcelain"])?.is_empty(),
                CliError::new(
                    Category::Conflict,
                    "commit_source_before_building",
                    "commit_source_before_building"
                )
            );
            check()?;
            anyhow::ensure!(
                output("git", &["status", "--porcelain"])?.is_empty(),
                CliError::new(
                    Category::Conflict,
                    "check_modified_source",
                    "check_modified_source: review and commit before building"
                )
            );
            anyhow::ensure!(
                Path::new("dist/ui").join(&m.ui.module_entry).is_file(),
                CliError::new(
                    Category::Environment,
                    "missing_ui_entry",
                    "missing_ui_entry"
                )
            );
            save(
                "dist/build.json",
                &json!({"bundleVersion":bundle::VERSION,"sourceCommit":commit,"appId":m.id,"version":m.version}),
            )?;
            let tag = format!("miy-app/{}:{}", m.id, m.version);
            run(
                "docker",
                &[
                    "build",
                    "--platform",
                    &platform,
                    "--build-arg",
                    &format!("MIY_APP_ID={}", m.id),
                    "--build-arg",
                    &format!("MIY_SOURCE_COMMIT={commit}"),
                    "-t",
                    &tag,
                    ".",
                ],
            )?;
            let image = output("docker", &["image", "inspect", "--format", "{{.Id}}", &tag])?;
            let metadata: Value =
                serde_json::from_str(&output("docker", &["image", "inspect", &image])?)?;
            anyhow::ensure!(
                crate::runtime_platform::image(&metadata[0]).as_deref() == Some(platform.as_str()),
                CliError::new(
                    Category::Environment,
                    "built_image_platform_mismatch",
                    "Docker produced an image for a different platform."
                )
            );
            let value = json!({"platform":platform,"imageId":image,"tag":tag,"sourceCommit":commit,"appId":m.id,"version":m.version,"bundleVersion":bundle::VERSION});
            save("dist/image.json", &value)?;
            return Ok(Outcome::Data(value));
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
