//! Project-scoped tooling. No changes to the user's shell, Codex model, or global config.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn cargo_bin() -> Option<PathBuf> {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cargo")))
        .map(|p| p.join("bin"))
}
fn executable_in(program: &str, path: &std::ffi::OsStr, cargo: Option<&Path>) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let executable = |p: &Path| {
        p.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    std::env::split_paths(path)
        .map(|p| p.join(program))
        .find(|p| executable(p))
        .or_else(|| {
            if matches!(program, "cargo" | "rustup" | "rustc") {
                cargo.map(|p| p.join(program)).filter(|p| executable(p))
            } else {
                None
            }
        })
}
pub fn executable(program: &str) -> Result<PathBuf> {
    executable_in(program,&std::env::var_os("PATH").unwrap_or_default(),cargo_bin().as_deref())
        .with_context(||format!("missing_tool: {program}; run miy app doctor. Rust: install rustup or add $HOME/.cargo/bin to PATH"))
}
pub fn tool_path() -> Result<std::ffi::OsString> {
    let mut paths: Vec<_> =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    if let Some(p) = cargo_bin()
        && !paths.contains(&p)
    {
        paths.insert(0, p);
    }
    Ok(std::env::join_paths(paths)?)
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
            Ok(path) => match Command::new(&path).arg("--version").env("PATH",tool_path().unwrap_or_default()).output() {
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
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    std::io::Write::write_all(&mut f, bytes)?;
    Ok(())
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
            if let Ok(out) = Command::new("security")
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
pub fn codex(args: &[String]) -> Result<()> {
    if Path::new(".codex/config.toml").is_file() {
        let status = Command::new(executable("codex")?)
            .arg("--no-daemon")
            .args(args)
            .env("PATH", tool_path()?)
            .status()?;
        ensure!(status.success(), "codex_failed");
        return Ok(());
    }
    let s = settings()?;
    let mut command = Command::new(executable("codex")?);
    // A daemon may predate this project's environment. Keep this launch self-contained.
    command.arg("--no-daemon");
    for value in overrides(&s.url) {
        command.args(["-c", &value]);
    }
    command.args(args).env("PATH", tool_path()?);
    if let Some(path) = s.ca_file {
        ensure!(
            path.is_file(),
            "missing_codex_ca_file: rerun app setup-codex"
        );
        command
            .env("SSL_CERT_FILE", &path)
            .env("NODE_EXTRA_CA_CERTS", &path);
    }
    let code = command.status()?.code().unwrap_or(1);
    ensure!(
        code == 0,
        "codex_exit_{code}: check MCP login and environment with app doctor"
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn cargo_fallback_works_without_changing_process_environment() {
        let d = tempfile::tempdir().unwrap();
        let cargo = d.path().join("cargo");
        std::fs::write(&cargo, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            executable_in(
                "cargo",
                std::ffi::OsStr::new("/no/such/directory"),
                Some(d.path())
            ),
            Some(cargo)
        );
        assert_eq!(
            executable_in(
                "pnpm",
                std::ffi::OsStr::new("/no/such/directory"),
                Some(d.path())
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
