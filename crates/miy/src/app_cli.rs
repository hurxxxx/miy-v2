use crate::{
    bundle,
    contract::{self, Manifest},
};
use anyhow::{Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum AppCommand {
    /// Discover, install and explicitly update components from the platform registry.
    Components {
        #[command(subcommand)]
        command: crate::component_cli::ComponentCommand,
    },
    /// Diagnose local tools and the saved Codex MCP connection.
    Doctor,
    /// Run the working tree locally with a private local DB and authenticated server connections.
    Dev {
        #[arg(long, default_value = "18100")]
        port: u16,
        #[arg(long)]
        no_open: bool,
    },
    /// Stop this project's preview gateway and containers, preserving its database volume.
    DevStop,
    /// Show the complete agent-readable CLI command reference.
    Describe {
        /// JSON for tools, or Markdown for the versioned workflow and full command reference.
        #[arg(long, default_value="json", value_parser=["json","markdown"])]
        format: String,
    },
    /// Serve read-only development guidance over stdio MCP using the CLI login.
    Mcp,
    /// Update this compatible project CLI, skills/manual and Codex/Claude MCP without changing global settings.
    Setup,
    /// Upgrade a clean supported SDK bundle to 0.6, preserving business code and SQL.
    Upgrade,
    /// Revoke the saved browser-approved CLI credential.
    Logout,
    /// Record acceptance of the currently deployed dev release after testing.
    Accept {
        #[arg(long)]
        note: String,
        /// JSON {releaseId, checks:[{scope,actor,result:passed|not_run,evidence}]}.
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Set the platform display name only (administrator); never changes source/manifest.
    Rename {
        #[arg(long)]
        name: String,
    },
    /// Promote an accepted dev release unchanged to prod (requires deploy permission).
    Promote {
        #[arg(long)]
        release: uuid::Uuid,
        #[arg(long)]
        no_wait: bool,
        #[arg(long)]
        retry: bool,
    },
    /// Save project-local Codex MCP settings and a trusted CA bundle.
    SetupCodex {
        #[arg(long, default_value = "https://localhost:18443/mcp")]
        url: String,
        #[arg(long)]
        ca_file: Option<PathBuf>,
    },
    /// Launch Codex with this project's MCP and Rust environment (use -- before Codex args).
    Codex {
        #[arg(last = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Create an independent Rust + React app with SDKs and development instructions.
    Init {
        path: PathBuf,
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "http://localhost:18000")]
        url: String,
        #[arg(long)]
        ca_file: Option<PathBuf>,
    },
    /// Sign in through the browser. --account is an explicit localhost development shortcut.
    Login {
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        ca_file: Option<PathBuf>,
        #[arg(long)]
        no_open: bool,
    },
    /// Request registration. An administrator must approve before deployment.
    Register,
    /// List your registration requests (administrators see all requests).
    Registrations,
    /// Approve a registration and grant its requester develop access to dev only.
    Approve { id: uuid::Uuid },
    /// Reject a pending registration request (administrator only).
    Reject { id: uuid::Uuid },
    /// List platform user IDs (administrator only).
    Users,
    /// Grant or revoke access to this app (administrator only).
    Grant {
        #[arg(long)]
        user: String,
        #[arg(long, default_value="use", value_parser=["use","develop","deploy"])]
        permission: String,
        #[arg(long, default_value="dev", value_parser=["dev","prod"])]
        environment: String,
        #[arg(long)]
        revoke: bool,
    },
    /// Check the manifest, required bundle files and both language toolchains.
    Check,
    /// Format only the app Rust package using the discovered rustup toolchain.
    Fmt,
    /// Build UI + Rust backend into one immutable local Docker image.
    Build,
    /// Deploy the built image using the platform's DB and Docker runner.
    Deploy {
        #[arg(long, default_value = "dev", value_parser=["dev","prod"])]
        environment: String,
        #[arg(long)]
        no_wait: bool,
        /// Submit a new job after a previously failed attempt.
        #[arg(long)]
        retry: bool,
    },
    /// Reconnect to the last submitted job without creating a deployment.
    Resume,
    /// Show deployments, versions, image match, acceptance records and the last job.
    Status,
    /// start, stop, restart, or remove (DB is preserved).
    Control {
        #[arg(value_parser=["start","stop","restart","remove"])]
        action: String,
        #[arg(long, default_value = "dev", value_parser=["dev","prod"])]
        environment: String,
        #[arg(long)]
        no_wait: bool,
        #[arg(long)]
        retry: bool,
    },
}
#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct Session {
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
    let status = crate::platform::command(program, args)?
        .env("CI", "true")
        .stdout(std::process::Stdio::from(std::io::stderr()))
        .status()
        .with_context(|| format!("cannot_run_{program}"))?;
    anyhow::ensure!(
        status.success(),
        "command_failed: {program} {}",
        args.join(" ")
    );
    Ok(())
}
fn output(program: &str, args: &[&str]) -> Result<String> {
    let out = crate::platform::command(program, args)?.output()?;
    anyhow::ensure!(out.status.success(), "command_failed: {program}");
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}
pub(crate) fn manifest() -> Result<Manifest> {
    let m: Manifest = serde_json::from_slice(
        &std::fs::read("manifest.json").context("run_inside_an_app_directory")?,
    )?;
    contract::validate(&m).map_err(anyhow::Error::msg)?;
    Ok(m)
}
pub(crate) fn session() -> Result<Session> {
    Ok(serde_json::from_slice(
        &std::fs::read(".miy/session.json").context("run_miy_app_login_first")?,
    )?)
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
    let client = crate::cli_client::client(s.ca_file.as_deref(), 20)?;
    let mut req = client.request(method, format!("{}{path}", s.origin));
    if let Some(body) = body {
        req = req.json(&body);
    }
    req = crate::cli_client::authenticate(s, req);
    let response = req.send().await?;
    let status = response.status();
    let value: Value = response.json().await?;
    anyhow::ensure!(
        status.is_success(),
        "platform_error {}: {} requestId={}",
        status,
        value["error"]["code"],
        value["error"]["requestId"]
    );
    Ok(value)
}
pub(crate) fn save(path: &str, value: &Value) -> Result<()> {
    crate::platform::write_private(Path::new(path), &serde_json::to_vec_pretty(value)?)
}
async fn wait(s: &Session, job: Value) -> Result<Value> {
    let id = job["id"].as_str().context("missing_job_id")?;
    for _ in 0..600 {
        let value = api(s, &format!("/api/v1/runtime/jobs/{id}"), None).await?;
        if value["status"] == "succeeded" {
            return Ok(value);
        }
        if value["status"] == "failed" {
            anyhow::bail!(
                "deployment_failed: {} (step {}, job {}); {}",
                value["errorCode"],
                value["step"],
                id,
                job_hint(value["errorCode"].as_str().unwrap_or(""))
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    anyhow::bail!("job_wait_timed_out: {id}; use miy app resume")
}
fn job_hint(code: &str) -> &'static str {
    match code {
        "immutable_release_conflict" => {
            "This version already belongs to a different image. Increase manifest.json version, commit, then run app build and app deploy. The previous deployment was retained."
        }
        "migration_failed" => {
            "Inspect migration SQL/history; preserve applied files and publish a corrected new release. Do not assume the app is still running; use app status."
        }
        _ => "Inspect app status and the job error before retrying.",
    }
}
pub(crate) fn check() -> Result<Manifest> {
    let m = manifest()?;
    bundle::verify(Path::new("."))?;
    crate::cli_guidance::verify(Path::new("."))?;
    crate::component_cli::verify(Path::new("."))?;
    for path in [
        "src/main.rs",
        "web/src/AppRoot.tsx",
        "Cargo.lock",
        "pnpm-lock.yaml",
    ] {
        anyhow::ensure!(Path::new(path).is_file(), "missing_bundle_file: {path}");
    }
    bundle::migrations(Path::new("migrations"))?;
    run("cargo", &["fmt", "--all", "--check"])?;
    run("cargo", &["test", "--locked"])?;
    run("pnpm", &["install", "--frozen-lockfile"])?;
    run("pnpm", &["typecheck"])?;
    run("pnpm", &["build"])?;
    bundle::ui(Path::new("dist/ui"))?;
    Ok(m)
}
async fn submit(
    s: &Session,
    app: &str,
    mut body: Value,
    no_wait: bool,
    retry: bool,
) -> Result<Value> {
    let me = api(s, "/api/v1/me", None).await?;
    let key = contract::hash(format!("{}:{}:{app}:{body}", s.origin, me["id"]));
    let path = format!(".miy/jobs/{key}.json");
    let mut cached = if Path::new(&path).exists() {
        Some(serde_json::from_slice::<Value>(&std::fs::read(&path)?)?)
    } else {
        None
    };
    if let Some(ref entry) = cached {
        if let Some(id) = entry["jobId"].as_str() {
            let job = api(s, &format!("/api/v1/runtime/jobs/{id}"), None).await?;
            match job["status"].as_str() {
                Some("succeeded") => cached = None,
                Some("failed") => {
                    anyhow::ensure!(
                        retry,
                        "previous_job_failed: {id}; inspect with app resume, then use --retry"
                    );
                    cached = None;
                }
                _ => anyhow::ensure!(!retry, "job_still_in_progress: use app resume"),
            }
        } else {
            anyhow::ensure!(
                !retry,
                "submission_status_unknown: rerun without --retry to recover the same request"
            );
        }
    }
    let mut entry=cached.unwrap_or_else(||json!({"origin":s.origin,"actor":me["id"],"appId":app,"body":body,"idempotencyKey":uuid::Uuid::new_v4()}));
    // Persist before sending so a lost response can be retried with the SAME key.
    save(&path, &entry)?;
    save(".miy/last-job.json", &entry)?;
    body["idempotencyKey"] = entry["idempotencyKey"].clone();
    let job = api(s, &format!("/api/v1/apps/{app}/runtime"), Some(body)).await?;
    entry["jobId"] = job["id"].clone();
    save(&path, &entry)?;
    save(".miy/last-job.json", &entry)?;
    if no_wait { Ok(job) } else { wait(s, job).await }
}
pub async fn execute(command: AppCommand) -> Result<()> {
    match command {
        AppCommand::Components { command } => {
            println!("{}", crate::component_cli::execute(command).await?)
        }
        AppCommand::Dev { port, no_open } => crate::preview::run(port, no_open).await?,
        AppCommand::DevStop => crate::preview::stop().await?,
        AppCommand::Describe { format } => {
            if format == "markdown" {
                print!("{}", crate::cli_guidance::manual());
            } else {
                println!("{}", crate::agent_tools::cli_help());
            }
        }
        AppCommand::Mcp => crate::agent_tools::serve().await?,
        AppCommand::Upgrade => println!("{}", crate::upgrade::run().await?),
        AppCommand::Setup => println!("{}", crate::agent_tools::setup(Path::new("."))?),
        AppCommand::Logout => {
            let s = session()?;
            if s.token.is_some() {
                api(&s, "/api/v1/cli/logout", Some(json!({}))).await?;
            } else {
                api(&s, "/auth/logout", Some(json!({}))).await?;
            }
            std::fs::remove_file(".miy/session.json")?;
            println!("{}", json!({"signedOut":true}));
        }
        AppCommand::Rename { name } => {
            let m = manifest()?;
            println!(
                "{}",
                api(
                    &session()?,
                    &format!("/api/v1/apps/{}/name", m.id),
                    Some(json!({"name":name}))
                )
                .await?
            );
        }
        AppCommand::Accept { note, report } => {
            let m = manifest()?;
            let s = session()?;
            let all = api(&s, "/api/v1/installations", None).await?;
            let inst = all
                .as_array()
                .context("invalid_installations")?
                .iter()
                .find(|v| v["appId"] == m.id && v["environment"] == "dev")
                .context("deploy_to_dev_first")?;
            let mut body = json!({"releaseId":inst["releaseId"],"note":note});
            if let Some(path) = report {
                anyhow::ensure!(
                    std::fs::metadata(&path)?.len() <= 160_000,
                    "acceptance_report_too_large"
                );
                let report: Value = serde_json::from_slice(&std::fs::read(path)?)?;
                anyhow::ensure!(
                    report["releaseId"] == inst["releaseId"],
                    "acceptance_report_release_mismatch"
                );
                anyhow::ensure!(
                    report["checks"].as_array().is_some_and(|c| !c.is_empty()),
                    "acceptance_checks_required"
                );
                body["checks"] = report["checks"].clone();
            }
            println!(
                "{}",
                api(
                    &s,
                    &format!("/api/v1/apps/{}/test-acceptance", m.id),
                    Some(body)
                )
                .await?
            );
        }
        AppCommand::Promote {
            release,
            no_wait,
            retry,
        } => {
            let m = manifest()?;
            let s = session()?;
            println!(
                "{}",
                submit(
                    &s,
                    &m.id,
                    json!({"action":"promote","environment":"prod","releaseId":release}),
                    no_wait,
                    retry
                )
                .await?
            );
        }
        AppCommand::Doctor => {
            let mut value = crate::dev_tools::doctor()?;
            value["cliVersion"] = json!(crate::cli_guidance::VERSION);
            value["cliGuidance"] = match crate::cli_guidance::verify(Path::new(".")) {
                Ok(v) => v,
                Err(e) => {
                    json!({"valid":false,"reason":e.to_string(),"hint":"Run the intended compatible CLI's app setup; use app upgrade for a different SDK bundle."})
                }
            };
            value["platform"] = match session() {
                Ok(s) => match api(&s, "/api/v1/contracts", None).await {
                    Ok(contract) => {
                        json!({"connected":true,"origin":s.origin,"bundleVersion":contract["bundleVersion"]})
                    }
                    Err(_) => {
                        json!({"connected":false,"hint":"Run app login; verify server URL/CA and connection."})
                    }
                },
                Err(_) => {
                    json!({"connected":false,"hint":"Run app login; project configuration alone is not authentication."})
                }
            };
            println!("{value}");
        }
        AppCommand::SetupCodex { url, ca_file } => {
            println!("{}", crate::dev_tools::setup(&url, ca_file.as_deref())?);
        }
        AppCommand::Codex { args } => {
            crate::dev_tools::codex(&args)?;
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
                "invalid_app_identity"
            );
            anyhow::ensure!(
                !path.exists() || std::fs::read_dir(&path)?.next().is_none(),
                "destination_must_be_empty"
            );
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
            crate::cli_client::configure(&path, &url, ca_file.as_deref())?;
            crate::agent_tools::setup(&path)?;
            println!(
                "{}",
                json!({"created":path.canonicalize()?,"cliVersion":crate::cli_guidance::VERSION,"cliSkill":".agents/skills/miy-cli/SKILL.md","cliManual":"docs/CLI-REFERENCE.md","appId":id,"bundleVersion":bundle::VERSION,"components":"Declared in miy.components.json; fetch after login","next":[".miy-tools/miy app doctor",".miy-tools/miy app login",".miy-tools/miy app components sync", "codex or claude (approve project trust/MCP when prompted)",".miy-tools/miy app check",".miy-tools/miy app register"]})
            );
        }
        AppCommand::Login {
            url,
            account,
            ca_file,
            no_open,
        } => {
            let saved: Value = std::fs::read(".miy/platform.json")
                .ok()
                .and_then(|v| serde_json::from_slice(&v).ok())
                .unwrap_or(json!({}));
            let url = url.unwrap_or_else(|| {
                saved["url"]
                    .as_str()
                    .unwrap_or("http://localhost:18000")
                    .into()
            });
            let ca_file = ca_file.or_else(|| saved["caFile"].as_str().map(PathBuf::from));
            crate::cli_client::configure(Path::new("."), &url, ca_file.as_deref())?;
            if account.is_none() {
                crate::cli_client::login(no_open).await?;
                return Ok(());
            }
            let account = account.unwrap();
            let u = url::Url::parse(&url)?;
            anyhow::ensure!(
                contract::valid_id(&account)
                    && crate::identity::development_profile(Some("1"), &url, &url),
                "development_login_requires_loopback_origin"
            );
            let origin = u.origin().ascii_serialization();
            let r = reqwest::Client::new()
                .post(format!("{origin}/auth/development"))
                .header("origin", &origin)
                .json(&json!({"account":account}))
                .send()
                .await?;
            anyhow::ensure!(r.status().is_success(), "development_login_unavailable");
            let cookie = r
                .headers()
                .get(reqwest::header::SET_COOKIE)
                .context("no_session")?
                .to_str()?
                .split(';')
                .next()
                .unwrap()
                .to_string();
            let client = reqwest::Client::new();
            let me: Value = client
                .get(format!("{origin}/api/v1/me"))
                .header("cookie", &cookie)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let session = Session {
                origin,
                token: None,
                ca_file: None,
                cookie,
                csrf: me["csrf"].as_str().context("no_csrf")?.into(),
            };
            save(".miy/session.json", &serde_json::to_value(session)?)?;
            println!("{}", json!({"signedIn":me["name"],"development":true}));
        }
        AppCommand::Fmt => run("cargo", &["fmt", "--package", "miy-app"])?,
        AppCommand::Check => {
            let m = check()?;
            println!(
                "{}",
                json!({"valid":true,"appId":m.id,"bundleVersion":bundle::VERSION,"checks":["manifest","bundle-integrity","cli-guidance-integrity","component-integrity","migration-contract","rust-format","rust-tests","frozen-dependencies","typescript","ui-build","federation-assets"],"browserIntegration":"requires platform acceptance test"})
            );
        }
        AppCommand::Build => {
            let m = manifest()?;
            let commit = output("git", &["rev-parse", "HEAD"])?;
            anyhow::ensure!(
                output("git", &["status", "--porcelain"])?.is_empty(),
                "commit_source_before_building"
            );
            check()?;
            anyhow::ensure!(
                output("git", &["status", "--porcelain"])?.is_empty(),
                "check_modified_source: review and commit before building"
            );
            anyhow::ensure!(
                Path::new("dist/ui").join(&m.ui.module_entry).is_file(),
                "missing_ui_entry"
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
            let value = json!({"imageId":image,"tag":tag,"sourceCommit":commit,"appId":m.id,"version":m.version,"bundleVersion":bundle::VERSION});
            save("dist/image.json", &value)?;
            println!("{value}");
        }
        AppCommand::Deploy {
            environment,
            no_wait,
            retry,
        } => {
            let m = manifest()?;
            let s = session()?;
            let image: Value = serde_json::from_slice(
                &std::fs::read("dist/image.json").context("run_miy_app_build_first")?,
            )?;
            anyhow::ensure!(
                image["appId"] == m.id
                    && image["version"] == m.version
                    && image["sourceCommit"] == output("git", &["rev-parse", "HEAD"])?
                    && output("git", &["status", "--porcelain"])?.is_empty(),
                "stale_build: commit changes and run app build"
            );
            anyhow::ensure!(
                environment == "dev",
                "use_app_accept_then_app_promote_for_prod"
            );
            crate::cli_client::upload(
                &s,
                &m.id,
                image["imageId"].as_str().context("missing_image_id")?,
            )
            .await?;
            println!(
                "{}",
                submit(
                    &s,
                    &m.id,
                    json!({"action":"deploy","environment":environment,"imageId":image["imageId"]}),
                    no_wait,
                    retry
                )
                .await?
            );
        }
        AppCommand::Control {
            action,
            environment,
            no_wait,
            retry,
        } => {
            let m = manifest()?;
            let s = session()?;
            println!(
                "{}",
                submit(
                    &s,
                    &m.id,
                    json!({"action":action,"environment":environment}),
                    no_wait,
                    retry
                )
                .await?
            );
        }
        AppCommand::Resume => {
            let s = session()?;
            let entry: Value = serde_json::from_slice(
                &std::fs::read(".miy/last-job.json").context("no_saved_job")?,
            )?;
            let me = api(&s, "/api/v1/me", None).await?;
            anyhow::ensure!(
                entry["origin"] == s.origin && entry["actor"] == me["id"],
                "job_session_mismatch: sign in as the original requester"
            );
            if let Some(id) = entry["jobId"].as_str() {
                println!("{}", wait(&s, json!({"id":id})).await?);
            } else {
                println!(
                    "{}",
                    submit(
                        &s,
                        entry["appId"].as_str().context("invalid_saved_job")?,
                        entry["body"].clone(),
                        false,
                        false
                    )
                    .await?
                );
            }
        }
        AppCommand::Register => {
            let m = manifest()?;
            println!(
                "{}",
                api(
                    &session()?,
                    "/api/v1/app-registrations",
                    Some(json!({"id":m.id,"name":m.name}))
                )
                .await?
            );
        }
        AppCommand::Registrations => {
            println!(
                "{}",
                api(&session()?, "/api/v1/app-registrations", None).await?
            );
        }
        AppCommand::Approve { id } | AppCommand::Reject { id } => {
            let approve = matches!(command, AppCommand::Approve { .. });
            println!(
                "{}",
                api(
                    &session()?,
                    &format!("/api/v1/app-registrations/{id}/decision"),
                    Some(json!({"approve":approve}))
                )
                .await?
            );
        }
        AppCommand::Users => {
            println!("{}", api(&session()?, "/api/v1/users", None).await?);
        }
        AppCommand::Grant {
            user,
            permission,
            environment,
            revoke,
        } => {
            let m = manifest()?;
            println!("{}",api_method(&session()?,"/api/v1/grants",reqwest::Method::PUT,Some(json!({"userId":user,"appId":m.id,"environment":environment,"permission":permission,"enabled":!revoke}))).await?);
        }
        AppCommand::Status => {
            let m = manifest()?;
            let s = session()?;
            let all = api(&s, "/api/v1/installations", None).await?;
            let mut installations: Vec<Value> = all
                .as_array()
                .context("invalid_installations_response")?
                .iter()
                .filter(|v| v["appId"] == m.id)
                .cloned()
                .collect();
            let local_image: Option<Value> = std::fs::read("dist/image.json")
                .ok()
                .and_then(|v| serde_json::from_slice(&v).ok());
            let local_job: Option<Value> = std::fs::read(".miy/last-job.json")
                .ok()
                .and_then(|v| serde_json::from_slice(&v).ok());
            let last = if let Some(ref entry) = local_job {
                if entry["origin"] == s.origin && entry["appId"] == m.id {
                    if let Some(id) = entry["jobId"].as_str() {
                        Some(api(&s, &format!("/api/v1/runtime/jobs/{id}"), None).await?)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            for inst in &mut installations {
                inst["localManifestName"] = json!(m.name);
                inst["displayNameDiffers"] = json!(inst["name"] != m.name);
                inst["url"] = json!(format!(
                    "{}/environments/{}/apps/{}",
                    s.origin,
                    inst["environment"].as_str().unwrap_or("dev"),
                    m.id
                ));
                if let Some(ref image) = local_image {
                    inst["localBuild"] = json!({"version":image["version"],"sourceCommit":image["sourceCommit"],"imageId":image["imageId"],"matchesRunningImage":image["imageId"]==inst["imageId"] && inst["runtimeStatus"]=="running"});
                }
                if let Some(ref job) = last
                    && job["environment"] == inst["environment"]
                {
                    inst["lastOperation"] = job.clone();
                    if job["status"] == "failed" {
                        inst["lastOperation"]["hint"] =
                            json!(job_hint(job["errorCode"].as_str().unwrap_or("")));
                    }
                }
            }
            println!("{}", json!(installations));
        }
    }
    Ok(())
}
