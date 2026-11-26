//! Manual, unauthenticated submission. Save before send; retries keep the same ID.
use crate::{
    cli_config,
    cli_error::{Category, CliError},
    support_contract::{Diagnostics, Failure, MAX_BODY, Report},
};
use anyhow::Result;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Subcommand)]
pub enum ReportCommand {
    /// Send a manual report without login. Does not read the credential store.
    Send {
        /// Describe the problem without passwords, tokens, personal data or logs.
        #[arg(
            long,
            required_unless_present = "message_file",
            conflicts_with = "message_file"
        )]
        message: Option<String>,
        /// Read up to 32,000 Unicode characters from a UTF-8 text file, or - for stdin.
        #[arg(long)]
        message_file: Option<PathBuf>,
        /// Group this submission with an earlier report using the first report ID.
        #[arg(long)]
        thread_id: Option<Uuid>,
        /// Explicit origin, also usable when the context configuration is broken.
        #[arg(long)]
        server: Option<String>,
        #[arg(long, requires = "server")]
        ca_file: Option<PathBuf>,
        /// Save a private report for later submission without contacting the server.
        #[arg(long, conflicts_with = "dry_run")]
        save_only: bool,
        /// Preview the redacted payload without saving or sending it.
        #[arg(long)]
        dry_run: bool,
    },
    /// Resend one saved report to its original server, without authentication.
    Retry { id: Uuid },
    /// Read the processing status and submitter-facing result using the saved private receipt.
    Result { id: Uuid },
    /// List locally saved reports and submission state.
    List,
    /// Inspect a locally saved redacted report.
    Show { id: Uuid },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Saved {
    report: Report,
    origin: String,
    ca_file: Option<PathBuf>,
    sent: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    result_key: Option<Uuid>,
}

fn directory(override_path: Option<&Path>) -> Result<PathBuf> {
    let path = match override_path {
        Some(path) => path.to_owned(),
        None => cli_config::ensure_root()?.canonicalize()?.join("reports"),
    };
    for parent in path.ancestors() {
        crate::platform::reject_reparse(parent)?;
    }
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        unsafe extern "C" {
            fn geteuid() -> u32;
        }
        let meta = std::fs::metadata(&path)?;
        if meta.mode() & 0o077 != 0 || meta.uid() != unsafe { geteuid() } {
            return Err(CliError::new(
                Category::Environment,
                "report_outbox_not_private",
                "Use an owned private outbox directory (0700).",
            )
            .into());
        }
    }
    Ok(path)
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    crate::platform::reject_reparse(path)?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take((MAX_BODY * 2 + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BODY * 2 {
        anyhow::bail!("report_file_too_large");
    }
    Ok(serde_json::from_slice(&bytes)?)
}

pub fn remember(error: &anyhow::Error) {
    // A single bounded local diagnostic; never the raw message, argv or details.
    // Do not let a reporting failure hide the command's original error.
    let _ = (|| -> Result<()> {
        let typed = crate::cli_error::classify(error);
        let field = |name: &str| {
            typed.details[name]
                .as_str()
                .and_then(crate::cli_error::safe_code)
                .map(str::to_owned)
        };
        let failure = Failure {
            occurred_at: chrono::Utc::now().timestamp(),
            code: typed.code,
            backend: field("backend"),
            operation: field("operation"),
            reason: field("reason"),
            os_status: typed.details["osStatus"]
                .as_i64()
                .and_then(|n| i32::try_from(n).ok()),
        };
        crate::platform::write_private(
            &directory(None)?.join("last-error.json"),
            &serde_json::to_vec(&failure)?,
        )
    })();
}

fn diagnostics() -> Diagnostics {
    let last_error = cli_config::root()
        .ok()
        .and_then(|root| read::<Failure>(&root.join("reports/last-error.json")).ok());
    let executable = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.parent()
                .and_then(Path::file_name)
                .map(|p| p == ".miy-tools")
        })
        .unwrap_or(false);
    Diagnostics {
        cli_version: env!("CARGO_PKG_VERSION").into(),
        bundle_version: crate::bundle::VERSION.into(),
        os: match std::env::consts::OS {
            "linux" | "macos" | "windows" => std::env::consts::OS,
            _ => "other",
        }
        .into(),
        arch: match std::env::consts::ARCH {
            "x86_64" | "aarch64" => std::env::consts::ARCH,
            _ => "other",
        }
        .into(),
        config_source: if std::env::var_os("MIY_CONFIG_DIR").is_some() {
            "override"
        } else if cfg!(windows) {
            "appdata"
        } else if std::env::var_os("XDG_CONFIG_HOME").is_some() {
            "xdg"
        } else {
            "home"
        }
        .into(),
        executable: if executable { "project" } else { "installed" }.into(),
        last_error,
    }
}

fn invalid_report(reason: &str) -> CliError {
    CliError::new(
        Category::Usage,
        reason,
        "Use a nonempty description of at most 32000 Unicode characters and the supported report format.",
    )
}

pub async fn execute(command: ReportCommand, outbox: Option<PathBuf>) -> Result<Value> {
    match command {
        ReportCommand::Send {
            message,
            message_file,
            thread_id,
            server,
            ca_file,
            save_only,
            dry_run,
        } => {
            let (origin, ca_file) = if let Some(server) = server {
                (
                    crate::cli_client::origin(&server)?,
                    ca_file.map(std::fs::canonicalize).transpose()?,
                )
            } else {
                let (_, context) = cli_config::selected()?;
                (context.origin, context.ca_file)
            };
            let message = match (message, message_file) {
                (Some(message), None) => message,
                (None, Some(path)) => {
                    let reader: Box<dyn Read> = if path == Path::new("-") {
                        Box::new(std::io::stdin())
                    } else {
                        Box::new(
                            std::fs::File::open(path)
                                .map_err(|_| invalid_report("report_file_unavailable"))?,
                        )
                    };
                    let mut bytes = Vec::new();
                    reader.take((MAX_BODY + 1) as u64).read_to_end(&mut bytes)?;
                    if bytes.len() > MAX_BODY {
                        return Err(invalid_report("report_too_large").into());
                    }
                    std::str::from_utf8(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes))
                        .map_err(|_| invalid_report("report_file_not_utf8"))?
                        .to_owned()
                }
                _ => return Err(invalid_report("report_message_required").into()),
            };
            let mut report = Report {
                schema_version: 1,
                id: Uuid::new_v4(),
                message: message.replace("\r\n", "\n"),
                thread_id,
                diagnostics: diagnostics(),
            };
            report.validate().map_err(invalid_report)?;
            report.redact();
            if dry_run {
                return Ok(json!({"status":"preview","server":origin,"report":report}));
            }
            let path = directory(outbox.as_deref())?.join(format!("{}.json", report.id));
            let mut saved = Saved {
                report,
                origin,
                ca_file,
                sent: false,
                result_key: Some(Uuid::new_v4()),
            };
            crate::platform::write_private(&path, &serde_json::to_vec(&saved)?)?;
            if save_only {
                return Ok(json!({"status":"saved","id":saved.report.id,"file":path}));
            }
            send(&mut saved, &path, outbox.as_deref()).await
        }
        ReportCommand::Retry { id } | ReportCommand::Result { id } => {
            let path = directory(outbox.as_deref())?.join(format!("{id}.json"));
            let mut saved: Saved = read(&path)?;
            saved.report.validate().map_err(invalid_report)?;
            if saved.report.id != id || crate::cli_client::origin(&saved.origin)? != saved.origin {
                return Err(invalid_report("invalid_saved_report").into());
            }
            if matches!(command, ReportCommand::Result { .. }) {
                let value = request(&saved, "/api/v1/support/reports/result").await?;
                if value["id"] != id.to_string()
                    || !matches!(
                        value["status"].as_str(),
                        Some("open" | "investigating" | "resolved" | "closed")
                    )
                    || !value["result"].is_string()
                {
                    return Err(CliError::new(
                        Category::Network,
                        "invalid_report_result",
                        "The server returned an invalid report result.",
                    )
                    .into());
                }
                return Ok(json!({"id":id,"status":value["status"],"result":value["result"]}));
            }
            send(&mut saved, &path, outbox.as_deref()).await
        }
        ReportCommand::Show { id } => {
            let saved: Saved = read(&directory(outbox.as_deref())?.join(format!("{id}.json")))?;
            Ok(json!({"id":id,"server":saved.origin,"sent":saved.sent,"report":saved.report}))
        }
        ReportCommand::List => {
            let mut entries = Vec::new();
            for file in std::fs::read_dir(directory(outbox.as_deref())?)? {
                let file = file?;
                if file
                    .path()
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .and_then(|n| Uuid::parse_str(n).ok())
                    .is_none()
                {
                    continue;
                }
                let saved: Saved = read(&file.path())?;
                entries.push(json!({"id":saved.report.id,"server":saved.origin,"sent":saved.sent}));
            }
            Ok(json!({"reports":entries}))
        }
    }
}

async fn request(saved: &Saved, endpoint: &str) -> Result<Value> {
    let mut request = crate::cli_client::client(saved.ca_file.as_deref(), 15)?
        .post(format!("{}{endpoint}", saved.origin))
        .json(&saved.report);
    if let Some(key) = saved.result_key {
        request = request.header(crate::support_contract::RECEIPT_HEADER, key.to_string());
    }
    let response = request.send().await.map_err(crate::cli_error::transport)?;
    crate::cli_error::response(response).await
}

async fn send(saved: &mut Saved, path: &Path, outbox: Option<&Path>) -> Result<Value> {
    let submitted = async {
        let value = request(saved, "/api/v1/support/reports").await?;
        if value["id"] != saved.report.id.to_string() || value["status"] != "received" {
            return Err(CliError::new(
                Category::Network,
                "invalid_report_receipt",
                "The server did not acknowledge this report.",
            )
            .into());
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if let Err(error) = submitted {
        let cause = crate::cli_error::classify(&error);
        let mut args = vec!["report".to_owned()];
        if let Some(outbox) = outbox {
            args.extend(["--outbox".into(), outbox.to_string_lossy().into_owned()]);
        }
        args.extend(["retry".into(), saved.report.id.to_string()]);
        return Err(CliError::new(
            cause.category,
            "report_queued",
            "The report is saved locally. Retry the same report ID when the server is available.",
        )
        .with_details(json!({"id":saved.report.id,"file":path,"cause":cause.code}))
        .with_recovery("miy", &args.iter().map(String::as_str).collect::<Vec<_>>())
        .into());
    }
    saved.sent = true;
    let updated = crate::platform::write_private(path, &serde_json::to_vec(saved)?).is_ok();
    let mut args = vec!["report".to_owned()];
    if let Some(outbox) = outbox {
        args.extend(["--outbox".into(), outbox.to_string_lossy().into_owned()]);
    }
    args.extend(["result".into(), saved.report.id.to_string()]);
    Ok(
        json!({"status":"received","id":saved.report.id,"file":path,"localReceiptSaved":updated,
        "resultCommand":{"executable":"miy","args":args}}),
    )
}
