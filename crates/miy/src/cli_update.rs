//! Advisory comparison with the selected portal's published CLI installer.
use crate::{
    Command, app_cli::AppCommand, cli_client, cli_config, cli_report::ReportCommand, platform,
};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    path::{Path, PathBuf},
};

pub type Target = (String, Option<PathBuf>);

pub fn selected() -> Option<Target> {
    let (_, context) = cli_config::selected().ok()?;
    Some((context.origin, context.ca_file))
}

pub fn target(command: &Command) -> Option<Target> {
    match command {
        Command::Report { command, outbox } => match command {
            ReportCommand::Send { dry_run: true, .. }
            | ReportCommand::Send {
                save_only: true, ..
            } => None,
            ReportCommand::Send {
                server: Some(url),
                ca_file,
                ..
            } => Some((url.clone(), ca_file.clone())),
            ReportCommand::Retry { id }
            | ReportCommand::Result { id }
            | ReportCommand::Show { id } => crate::cli_report::saved_target(outbox.as_deref(), *id),
            _ => selected(),
        },
        Command::Context {
            command: cli_config::ContextCommand::Show { name: Some(name) },
        } => {
            let context = cli_config::named(name).ok()?;
            Some((context.origin, context.ca_file))
        }
        Command::Auth {
            command: crate::cli_login::AuthCommand::Logout { local_only: true },
        } => None,
        Command::Auth { .. }
        | Command::Doctor(_)
        | Command::Component { .. }
        | Command::Job { .. }
        | Command::Registration { .. }
        | Command::User { .. }
        | Command::Grant { .. }
        | Command::Permissions { .. }
        | Command::Context {
            command: cli_config::ContextCommand::Show { .. },
        }
        | Command::App {
            command:
                AppCommand::Remote(_)
                | AppCommand::Source(_)
                | AppCommand::Build { .. }
                | AppCommand::Register
                | AppCommand::Rename { .. }
                | AppCommand::Accept { .. }
                | AppCommand::Dev { .. },
        } => selected(),
        _ => None,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub status: &'static str,
    installed_version: &'static str,
    published_version: String,
    install_url: String,
}

impl std::fmt::Display for Notice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.status == "update_available" {
            write!(
                f,
                "miy CLI update available ({} -> {}). Update using {}",
                self.installed_version, self.published_version, self.install_url
            )
        } else {
            write!(
                f,
                "Installed miy CLI {} is newer than the portal's published CLI {} ({}).",
                self.installed_version, self.published_version, self.install_url
            )
        }
    }
}

// The installer publishes numeric major.minor.patch releases.
fn version(s: &str) -> Option<[u64; 3]> {
    let mut parts = s.split('.');
    let mut numbers = [0; 3];
    for n in &mut numbers {
        let p = parts.next()?;
        if p.is_empty()
            || (p.len() > 1 && p.starts_with('0'))
            || !p.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        *n = p.parse().ok()?;
    }
    parts.next().is_none().then_some(numbers)
}

fn compare(origin: &str, published: String) -> Option<Notice> {
    let installed = env!("CARGO_PKG_VERSION");
    let status = match version(installed)?.cmp(&version(&published)?) {
        Ordering::Less => "update_available",
        Ordering::Greater => "client_newer",
        Ordering::Equal => return None,
    };
    Some(Notice {
        status,
        installed_version: installed,
        published_version: published,
        install_url: format!("{origin}/cli/install"),
    })
}

#[derive(Deserialize, Serialize)]
struct Cached {
    checked_at: i64,
    published: Option<String>,
}

async fn fetch(origin: &str, ca: Option<&Path>) -> Option<String> {
    let client = cli_client::client(ca, 1).ok()?;
    let mut response = client
        .get(format!("{origin}/api/v1/cli/install"))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len() + chunk.len() > 65536 {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    let guide: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    if guide["schemaVersion"] != 1
        || guide["serverUrl"] != origin
        || guide["downloads"].as_array()?.is_empty()
    {
        return None;
    }
    let published = guide["cliVersion"].as_str()?;
    version(published)?;
    Some(published.to_owned())
}

pub async fn check(target: Option<Target>) -> Option<Notice> {
    let (url, ca) = target?;
    let origin = cli_client::origin(&url).ok()?;
    let now = chrono::Utc::now().timestamp();
    let filename = format!("cli-version-{}.json", crate::contract::hash(&origin));
    if let Some(cached) = cli_config::root().ok().and_then(|root| {
        let path = root.join(&filename);
        platform::reject_reparse(&path).ok()?;
        let metadata = std::fs::metadata(&path).ok()?;
        if !metadata.is_file() || metadata.len() > 512 {
            return None;
        }
        serde_json::from_slice::<Cached>(&std::fs::read(path).ok()?).ok()
    }) {
        let ttl = if cached.published.is_some() {
            3600
        } else {
            300
        };
        if (0..ttl).contains(&now.saturating_sub(cached.checked_at)) {
            return compare(&origin, cached.published?);
        }
    }
    let entry = Cached {
        checked_at: now,
        published: fetch(&origin, ca.as_deref()).await,
    };
    if let Ok(root) = cli_config::ensure_root()
        && let Ok(bytes) = serde_json::to_vec(&entry)
    {
        let _ = platform::write_private(&root.join(filename), &bytes);
    }
    compare(&origin, entry.published?)
}
