use crate::{
    bundle,
    contract::{self, Manifest},
};
use anyhow::{Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Subcommand)]
pub enum AppCommand {
    /// Diagnose local tools and the saved Codex MCP connection.
    Doctor,
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
    },
    /// Sign into the explicitly enabled localhost development profile.
    Login {
        #[arg(long, default_value = "http://localhost:18000")]
        url: String,
        #[arg(long, default_value = "developer")]
        account: String,
    },
    /// Request registration. An administrator must approve before deployment.
    Register,
    /// List your registration requests (administrators see all requests).
    Registrations,
    /// Approve a registration and grant its requester develop access to dev only.
    Approve {
        id: uuid::Uuid,
    },
    Reject {
        id: uuid::Uuid,
    },
    /// List platform user IDs (administrator only).
    Users,
    /// Grant or revoke access to this app (administrator only).
    Grant {
        #[arg(long)]
        user: String,
        #[arg(long, default_value="use", value_parser=["use","develop"])]
        permission: String,
        #[arg(long, default_value="dev", value_parser=["dev","prod"])]
        environment: String,
        #[arg(long)]
        revoke: bool,
    },
    /// Check the manifest, required bundle files and both language toolchains.
    Check,
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
#[derive(Serialize, Deserialize)]
struct Session {
    origin: String,
    cookie: String,
    csrf: String,
}
fn run(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(crate::dev_tools::executable(program)?)
        .args(args)
        .env("PATH", crate::dev_tools::tool_path()?)
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
    let out = Command::new(program).args(args).output()?;
    anyhow::ensure!(out.status.success(), "command_failed: {program}");
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}
fn manifest() -> Result<Manifest> {
    let m: Manifest = serde_json::from_slice(
        &std::fs::read("manifest.json").context("run_inside_an_app_directory")?,
    )?;
    contract::validate(&m).map_err(anyhow::Error::msg)?;
    Ok(m)
}
fn session() -> Result<Session> {
    Ok(serde_json::from_slice(
        &std::fs::read(".miy/session.json").context("run_miy_app_login_first")?,
    )?)
}
async fn api(s: &Session, path: &str, body: Option<Value>) -> Result<Value> {
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
async fn api_method(
    s: &Session,
    path: &str,
    method: reqwest::Method,
    body: Option<Value>,
) -> Result<Value> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let mut req = client.request(method, format!("{}{path}", s.origin));
    if let Some(body) = body {
        req = req.json(&body);
    }
    req = req
        .header("cookie", &s.cookie)
        .header("origin", &s.origin)
        .header("x-miy-csrf", &s.csrf);
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
fn save(path: &str, value: &Value) -> Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = std::fs::OpenOptions::new();
    let temporary = Path::new(path).with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    let mut file = options
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    std::io::Write::write_all(&mut file, serde_json::to_string_pretty(value)?.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(temporary, path)?;
    Ok(())
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
fn check() -> Result<Manifest> {
    let m = manifest()?;
    bundle::verify(Path::new("."))?;
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
        AppCommand::Doctor => {
            println!("{}", crate::dev_tools::doctor()?);
        }
        AppCommand::SetupCodex { url, ca_file } => {
            println!("{}", crate::dev_tools::setup(&url, ca_file.as_deref())?);
        }
        AppCommand::Codex { args } => {
            crate::dev_tools::codex(&args)?;
        }
        AppCommand::Init { path, id, name } => {
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
            std::fs::copy(std::env::current_exe()?, path.join(".miy-tools/miy"))?;
            println!(
                "{}",
                json!({"created":path.canonicalize()?,"appId":id,"bundleVersion":bundle::VERSION,"next":[".miy-tools/miy app doctor",".miy-tools/miy app setup-codex --ca-file /path/to/platform-ca.crt (local CA only)",".miy-tools/miy app check",".miy-tools/miy app login",".miy-tools/miy app register"]})
            );
        }
        AppCommand::Login { url, account } => {
            let u = url::Url::parse(&url)?;
            anyhow::ensure!(
                contract::valid_id(&account)
                    && crate::auth::development_profile(Some("1"), &url, &url),
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
                cookie,
                csrf: me["csrf"].as_str().context("no_csrf")?.into(),
            };
            save(".miy/session.json", &serde_json::to_value(session)?)?;
            println!("{}", json!({"signedIn":me["name"],"development":true}));
        }
        AppCommand::Check => {
            let m = check()?;
            println!(
                "{}",
                json!({"valid":true,"appId":m.id,"bundleVersion":bundle::VERSION,"checks":["manifest","bundle-integrity","migration-contract","rust-format","rust-tests","frozen-dependencies","typescript","ui-build","federation-assets"],"browserIntegration":"requires platform acceptance test"})
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
                inst["url"] = json!(format!("{}/apps/{}", s.origin, m.id));
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
