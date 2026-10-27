//! Project-scoped tooling. No changes to the user's shell, Codex model, or global config.
use crate::cli_error::{Category, CliError};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
pub fn executable(program: &str) -> Result<PathBuf> {
    crate::platform::executable(program)
}
pub fn tool_path() -> Result<std::ffi::OsString> {
    crate::platform::tool_path()
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    url: String,
    ca_file: Option<PathBuf>,
}
fn settings() -> Result<Settings> {
    Ok(serde_json::from_slice(&std::fs::read(".miy/codex.json").context("codex_not_configured: run app setup-codex --url https://PLATFORM/mcp [--ca-file /path/to/ca.crt]")?)?)
}
pub fn doctor() -> Result<Value> {
    let tools:Vec<_>=["cargo","pnpm","docker","codex","claude"].iter().map(|name|{
        match executable(name) {
            Ok(path) => match crate::platform::command(name, &["--version"]).and_then(|mut command| Ok(command.output()?)) {
                Ok(out) => json!({"tool":name,"path":path,"ready":out.status.success(),"version":String::from_utf8_lossy(&out.stdout).trim()}),
                Err(_) => json!({"tool":name,"path":path,"ready":false}),
            },
            Err(_) => json!({"tool":name,"ready":false,"hint":"Install this tool and ensure its executable is on PATH."}),
        }
    }).collect();
    let mcp = if Path::new(".codex/config.toml").is_file() && Path::new(".mcp.json").is_file() {
        json!({"configured":true,"transport":"stdio","clients":["Codex","Claude Code"],"authentication":"CLI login; verify with get_contract"})
    } else {
        match settings() {
            Ok(s) => {
                json!({"configured":true,"url":s.url,"caFile":s.ca_file,"authentication":"Run app codex -- mcp login miy_app if not authenticated; configuration alone does not prove authentication."})
            }
            Err(_) => {
                json!({"configured":false,"hint":"Run app setup for project Codex/Claude MCP, then app login for authentication."})
            }
        }
    };
    Ok(
        json!({"tools":tools,"mcp":mcp,"shellHint":"Direct cargo commands require export PATH=\"$HOME/.cargo/bin:$PATH\". app check/build and app codex locate rustup automatically."}),
    )
}
fn private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    crate::platform::write_private(path, bytes)
}
pub fn setup(url: &str, ca_file: Option<&Path>) -> Result<Value> {
    ensure!(
        Path::new("manifest.json").is_file(),
        "run_inside_an_app_directory"
    );
    let u = url::Url::parse(url)?;
    ensure!(
        u.username().is_empty()
            && u.password().is_none()
            && u.query().is_none()
            && u.fragment().is_none()
            && u.host_str().is_some()
            && (u.scheme() == "https"
                || (u.scheme() == "http"
                    && matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))),
        "invalid_mcp_url: HTTPS required except loopback"
    );
    std::fs::create_dir_all(".miy")?;
    let ca = if let Some(path) = ca_file {
        let custom = std::fs::read(path).context("cannot_read_ca_file")?;
        reqwest::Certificate::from_pem(&custom).context("invalid_ca_certificate")?;
        let mut pem = Vec::new();
        for p in std::env::var_os("SSL_CERT_FILE")
            .map(PathBuf::from)
            .into_iter()
            .chain([PathBuf::from("/etc/ssl/cert.pem")])
        {
            if let Ok(bytes) = std::fs::read(p) {
                pem.extend(bytes);
                pem.push(b'\n');
            }
        }
        #[cfg(target_os = "macos")]
        for keychain in [
            "/System/Library/Keychains/SystemRootCertificates.keychain",
            "/Library/Keychains/System.keychain",
        ] {
            if let Ok(out) = std::process::Command::new("security")
                .args(["find-certificate", "-a", "-p", keychain])
                .output()
                && out.status.success()
            {
                pem.extend(out.stdout);
                pem.push(b'\n');
            }
        }
        pem.extend(custom);
        let dest = std::env::current_dir()?.join(".miy/codex-ca.pem");
        private_file(&dest, &pem)?;
        Some(dest)
    } else {
        None
    };
    let settings = Settings {
        url: u.to_string(),
        ca_file: ca,
    };
    private_file(
        Path::new(".miy/codex.json"),
        &serde_json::to_vec_pretty(&settings)?,
    )?;
    Ok(
        json!({"configured":true,"mcpUrl":settings.url,"scope":"project-local launcher; global Codex config unchanged","next":[".miy-tools/miy app codex -- mcp login miy_app",".miy-tools/miy app codex",".miy-tools/miy app codex -- resume SESSION_ID"]}),
    )
}
fn overrides(url: &str) -> Vec<String> {
    vec![
        format!("mcp_servers.miy_app.url={}",json!(url)),
        "mcp_servers.miy_app.required=true".into(),
        "mcp_servers.miy_app.oauth.client_id=\"miy-codex\"".into(),
        "mcp_servers.miy_app.oauth.callback_url=\"http://127.0.0.1:18555/callback\"".into(),
        "mcp_servers.miy_app.oauth.callback_port=18555".into(),
        "mcp_servers.miy_app.scopes=[\"miy.read\",\"miy.dev\",\"offline_access\"]".into(),
        "mcp_servers.miy_app.enabled_tools=[\"get_contract\",\"search_api\",\"get_api\",\"validate_manifest\"]".into(),
    ]
}
fn codex_command(arguments: &[&str]) -> Result<std::process::Command> {
    crate::platform::command("codex", arguments).map_err(|_| {
        CliError::new(
            Category::Environment,
            "codex_unavailable",
            "Codex could not be launched with the requested local arguments.",
        )
        .with_recovery("miy", &["app", "doctor"])
        .into()
    })
}
fn codex_status(command: &mut std::process::Command) -> Result<()> {
    // Preserve the interactive child's inherited stdout/stderr; no JSON is mixed in.
    let status = command.status().map_err(|_| {
        CliError::new(
            Category::Environment,
            "codex_launch_failed",
            "The local Codex process could not be started.",
        )
    })?;
    if !status.success() {
        return Err(CliError::new(
            Category::Environment,
            "codex_failed",
            "The local Codex process exited unsuccessfully.",
        )
        .with_details(json!({"program":"codex","exitCode":status.code()}))
        .with_recovery("miy", &["app", "doctor"])
        .into());
    }
    Ok(())
}
pub fn codex(args: &[String]) -> Result<()> {
    if Path::new(".codex/config.toml").is_file() {
        let mut arguments = vec!["--no-daemon"];
        arguments.extend(args.iter().map(String::as_str));
        return codex_status(&mut codex_command(&arguments)?);
    }
    let s = settings()?;
    let mut arguments = vec!["--no-daemon".to_owned()];
    // A daemon may predate this project's environment. Keep this launch self-contained.
    for value in overrides(&s.url) {
        arguments.extend(["-c".into(), value]);
    }
    arguments.extend_from_slice(args);
    let mut command = codex_command(&arguments.iter().map(String::as_str).collect::<Vec<_>>())?;
    if let Some(path) = s.ca_file {
        if !path.is_file() {
            return Err(CliError::new(
                Category::Environment,
                "missing_codex_ca_file",
                "The configured Codex CA file is missing; configure the project connection again.",
            )
            .into());
        }
        command
            .env("SSL_CERT_FILE", &path)
            .env("NODE_EXTRA_CA_CERTS", &path);
    }
    codex_status(&mut command)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(unix)]
    #[test]
    fn cargo_fallback_works_without_changing_process_environment() {
        let d = tempfile::tempdir().unwrap();
        let cargo = d.path().join("cargo");
        std::fs::write(&cargo, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            crate::platform::executable_in(
                "cargo",
                std::ffi::OsStr::new("/no/such/directory"),
                Some(d.path()),
                None
            ),
            Some(cargo)
        );
        assert_eq!(
            crate::platform::executable_in(
                "pnpm",
                std::ffi::OsStr::new("/no/such/directory"),
                Some(d.path()),
                None
            ),
            None
        );
    }
    #[test]
    fn launcher_sets_required_mcp_without_model_or_permission_overrides() {
        let values = overrides("https://localhost:18443/mcp");
        assert!(
            values
                .iter()
                .any(|v| v == "mcp_servers.miy_app.required=true")
        );
        assert!(values.iter().all(|v| v.starts_with("mcp_servers.miy_app.")));
    }
}
