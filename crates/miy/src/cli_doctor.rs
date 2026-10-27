//! Bounded diagnostics for operators and app developers; never prints credentials.
use anyhow::Result;
use serde::Serialize;
use serde_json::{Value, json};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::io::AsyncReadExt;

const TIMEOUT: Duration = Duration::from_secs(8);
const OUTPUT_LIMIT: u64 = 64 * 1024;
const REQUIRED_FEATURES: &[&str] = &["runtime.preflight.v1"];

#[derive(clap::Args, Debug)]
pub struct DoctorArgs {
    /// Diagnose operation access, or the full local application development environment.
    #[arg(long = "for", default_value = "operate", value_parser = ["develop", "operate"])]
    pub for_mode: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Check {
    id: &'static str,
    required: bool,
    status: &'static str,
    code: &'static str,
    message: &'static str,
    details: Value,
}

impl Check {
    fn new(id: &'static str, ready: bool, code: &'static str, message: &'static str) -> Self {
        Self {
            id,
            required: true,
            status: if ready { "pass" } else { "fail" },
            code,
            message,
            details: json!({}),
        }
    }

    fn details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }

    fn skipped(id: &'static str, message: &'static str) -> Self {
        Self {
            status: "skipped",
            ..Self::new(id, false, "prerequisite_unavailable", message)
        }
    }
}

/// Returns all findings, including failures. The entry point chooses the exit status
/// from `ready`, preserving the report in `diagnostic_failed.details` on exit 7.
pub async fn inspect(args: DoctorArgs) -> Result<Value> {
    if !matches!(args.for_mode.as_str(), "operate" | "develop") {
        return Err(crate::cli_error::CliError::new(
            crate::cli_error::Category::Usage,
            "invalid_doctor_mode",
            "Choose doctor --for operate or doctor --for develop.",
        )
        .into());
    }
    let (mut checks, platform) = server_checks().await;
    if args.for_mode == "develop" {
        checks.extend(project_checks(Path::new(".")));
        checks.extend(
            futures_util::future::join_all(
                ["cargo", "rustc", "node", "pnpm", "git", "docker"]
                    .into_iter()
                    .map(tool_check),
            )
            .await,
        );
        checks.extend(docker_checks(platform.as_deref()).await);
    }
    let ready = ready(&checks);
    Ok(json!({
        "for":args.for_mode,"ready":ready,"cliVersion":env!("CARGO_PKG_VERSION"),
        "bundleVersion":crate::bundle::VERSION,"checks":checks,
    }))
}

fn ready(checks: &[Check]) -> bool {
    checks
        .iter()
        .all(|check| !check.required || check.status == "pass")
}

async fn server_checks() -> (Vec<Check>, Option<String>) {
    let mut checks = Vec::new();
    let session = crate::app_cli::session().ok();
    checks.push(Check::new(
        "savedSession",
        session.is_some(),
        if session.is_some() {
            "session_available"
        } else {
            "session_unavailable"
        },
        if session.is_some() {
            "A saved credential is available; server validation follows."
        } else {
            "Select a server context and sign in before operating the platform."
        },
    ));
    // A context without credentials can still diagnose the public capabilities API.
    // Never fall back to a legacy project origin when explicit context selection fails.
    let connection = crate::cli_config::selected()
        .ok()
        .map(|(_, context)| (context.origin, context.ca_file));
    let Some((origin, ca)) = connection else {
        checks.push(Check::new(
            "serverConnection",
            false,
            "connection_unavailable",
            "No valid server context is selected; add and select a context.",
        ));
        checks.push(Check::skipped(
            "serverCapabilities",
            "Capabilities cannot be checked without a server connection.",
        ));
        checks.push(Check::skipped(
            "authentication",
            "Authentication cannot be checked without a saved credential.",
        ));
        return (checks, None);
    };
    let Ok(origin) = crate::cli_client::origin(&origin) else {
        checks.push(Check::new("serverConnection", false, "invalid_server_origin", "The saved server origin is invalid; select a valid HTTPS context or loopback development server."));
        return (checks, None);
    };
    // Context selection and credential loading are separate reads. A concurrent
    // context change must never forward a credential to another server origin.
    let session = session.filter(|session| {
        crate::cli_client::origin(&session.origin).ok().as_deref() == Some(origin.as_str())
    });
    let client = match crate::cli_client::client(ca.as_deref(), TIMEOUT.as_secs()) {
        Ok(client) => client,
        Err(_) => {
            checks.push(Check::new(
                "serverConnection",
                false,
                "invalid_connection_configuration",
                "Check the saved CA certificate and HTTP client configuration.",
            ));
            return (checks, None);
        }
    };
    checks.push(Check::new(
        "serverConnection",
        true,
        "connection_configured",
        "The server origin and TLS configuration are valid.",
    ));
    let capabilities = get_json(client.get(format!("{origin}/api/v1/capabilities")));
    let authentication = async {
        if let Some(session) = session.as_ref() {
            let session = match crate::cli_login::refreshed(session).await {
                Ok(session) => session,
                Err(_) => return Some(Err(RequestFailure::Http(401))),
            };
            Some(
                get_json(crate::cli_client::authenticate(
                    &session,
                    client.get(format!("{origin}/api/v1/me")),
                ))
                .await,
            )
        } else {
            None
        }
    };
    let (capabilities, authentication) = tokio::join!(capabilities, authentication);
    let platform = match capabilities {
        Ok(value) => {
            let (capabilities, platform) = assess_capabilities(&value);
            checks.push(capabilities);
            platform
        }
        Err(reason) => {
            checks.push(request_check("serverCapabilities", reason));
            None
        }
    };
    checks.push(match authentication {
        Some(Ok(value)) => Check::new(
            "authentication",
            value["id"].as_str().is_some_and(|id| !id.is_empty()),
            if value["id"].as_str().is_some_and(|id| !id.is_empty()) {
                "authenticated"
            } else {
                "invalid_identity_response"
            },
            "Authentication requires a successful identity response from the selected server.",
        ),
        Some(Err(reason)) => request_check("authentication", reason),
        None => Check::skipped("authentication", "Sign in to validate platform access."),
    });
    (checks, platform)
}

enum RequestFailure {
    Http(u16),
    Transport,
    InvalidResponse,
}

async fn get_json(request: reqwest::RequestBuilder) -> std::result::Result<Value, RequestFailure> {
    let mut response = request
        .send()
        .await
        .map_err(|_| RequestFailure::Transport)?;
    if !response.status().is_success() {
        return Err(RequestFailure::Http(response.status().as_u16()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| RequestFailure::Transport)?
    {
        if bytes.len() + chunk.len() > OUTPUT_LIMIT as usize {
            return Err(RequestFailure::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| RequestFailure::InvalidResponse)
}

fn request_check(id: &'static str, reason: RequestFailure) -> Check {
    match reason {
        RequestFailure::Http(status) => Check::new(id, false,
            if matches!(status, 401 | 403) { "access_denied" } else if status == 404 { "endpoint_unavailable" } else { "server_request_failed" },
            "The server rejected the diagnostic request; check authentication, permissions and server compatibility.",
        ).details(json!({"httpStatus":status})),
        RequestFailure::Transport => Check::new(id, false, "server_unreachable", "Check connectivity, the server origin and the trusted CA certificate."),
        RequestFailure::InvalidResponse => Check::new(id, false, "invalid_server_response", "The server response did not match the required diagnostic contract."),
    }
}

fn assess_capabilities(value: &Value) -> (Check, Option<String>) {
    let features = value["features"].as_array();
    let missing: Vec<_> = REQUIRED_FEATURES
        .iter()
        .filter(|expected| {
            !features.is_some_and(|features| {
                features
                    .iter()
                    .any(|feature| feature.as_str() == Some(**expected))
            })
        })
        .copied()
        .collect();
    let platform = match value["runtimePlatform"].as_str() {
        Some("linux/amd64") => Some("linux/amd64".to_owned()),
        Some("linux/arm64") => Some("linux/arm64".to_owned()),
        _ => None,
    };
    let valid = value["schemaVersion"] == 1
        && value["cliContractVersion"] == 1
        && features.is_some()
        && missing.is_empty()
        && platform.is_some();
    (Check::new("serverCapabilities", valid,
        if valid { "server_compatible" } else { "server_capabilities_missing" },
        "The server must advertise CLI contract 1, the required runtime features and a supported Linux runtime platform.",
    ).details(json!({"requiredFeatures":REQUIRED_FEATURES,"missingFeatures":missing,
        "schemaVersionCompatible":value["schemaVersion"] == 1,
        "cliContractVersionCompatible":value["cliContractVersion"] == 1,
        "runtimePlatform":platform})), platform)
}

fn project_checks(root: &Path) -> Vec<Check> {
    let manifest = std::fs::read(root.join("manifest.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<crate::contract::Manifest>(&bytes).ok());
    let manifest_valid = manifest
        .as_ref()
        .is_some_and(|manifest| crate::contract::validate(manifest).is_ok());
    vec![
        Check::new(
            "projectManifest",
            manifest_valid,
            if manifest_valid {
                "manifest_valid"
            } else {
                "manifest_invalid"
            },
            "Develop mode requires a valid app manifest in the current directory.",
        ),
        validation_check(
            "projectBundle",
            crate::bundle::verify(root).is_ok(),
            "The project bundle and its protected files must match this CLI's supported bundle.",
        ),
        validation_check(
            "projectGuidance",
            crate::cli_guidance::verify(root).is_ok(),
            "The project CLI instructions and MCP configuration must match this CLI; run app setup when compatible.",
        ),
        validation_check(
            "projectComponents",
            crate::component_cli::verify(root).is_ok(),
            "Declared components must be installed with valid locks and unmodified protected files.",
        ),
    ]
}

fn validation_check(id: &'static str, valid: bool, message: &'static str) -> Check {
    Check::new(
        id,
        valid,
        if valid {
            "validation_passed"
        } else {
            "validation_failed"
        },
        message,
    )
}

enum ProbeFailure {
    Unavailable,
    Failed,
    TimedOut,
    OutputLimit,
}

async fn probe(program: &str, args: &[&str]) -> std::result::Result<Vec<u8>, ProbeFailure> {
    let command = crate::platform::command(program, args).map_err(|_| ProbeFailure::Unavailable)?;
    let mut command = tokio::process::Command::from(command);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env("RUSTUP_AUTO_INSTALL", "0")
        .kill_on_drop(true);
    let (mut child, _process_guard) = crate::owned_process::spawn(&mut command)
        .await
        .map_err(|_| ProbeFailure::Unavailable)?;
    let result = tokio::time::timeout(TIMEOUT, async {
        let stdout = child.stdout.take().ok_or(ProbeFailure::Failed)?;
        let mut bytes = Vec::new();
        stdout
            .take(OUTPUT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| ProbeFailure::Failed)?;
        if bytes.len() > OUTPUT_LIMIT as usize {
            return Err(ProbeFailure::OutputLimit);
        }
        let status = child.wait().await.map_err(|_| ProbeFailure::Failed)?;
        if !status.success() {
            return Err(ProbeFailure::Failed);
        }
        Ok(bytes)
    })
    .await;
    result.unwrap_or(Err(ProbeFailure::TimedOut))
}

async fn tool_check(name: &'static str) -> Check {
    let bytes = match probe(name, &["--version"]).await {
        Ok(bytes) => bytes,
        Err(reason) => return probe_check(name, reason),
    };
    let version = parse_version(&bytes);
    let compatible = version.is_some_and(|version| version_compatible(name, version));
    Check::new(name, compatible,
        if compatible { "tool_ready" } else { "tool_version_incompatible" },
        "The tool must be available on the CLI's discovered PATH and report a supported version.",
    ).details(json!({"version":version.map(|v| format!("{}.{}.{}",v.0,v.1,v.2)),"expected":version_requirement(name)}))
}

fn probe_check(name: &'static str, reason: ProbeFailure) -> Check {
    Check::new(
        name,
        false,
        match reason {
            ProbeFailure::Unavailable => "tool_unavailable",
            ProbeFailure::Failed => "tool_probe_failed",
            ProbeFailure::TimedOut => "tool_probe_timed_out",
            ProbeFailure::OutputLimit => "tool_probe_output_limit",
        },
        "Install or repair the required tool and ensure its version/daemon probe completes within eight seconds.",
    )
}

fn parse_version(bytes: &[u8]) -> Option<(u64, u64, u64)> {
    let text = std::str::from_utf8(bytes).ok()?;
    for candidate in text.split_ascii_whitespace().take(8) {
        let candidate = candidate.trim_start_matches('v').trim_end_matches(',');
        let mut parts = candidate.split('.');
        let major = parts.next()?.parse::<u64>();
        let minor = parts.next().and_then(|value| value.parse::<u64>().ok());
        let patch = parts
            .next()
            .and_then(|value| value.split('-').next())
            .and_then(|value| value.parse::<u64>().ok());
        if let (Ok(major), Some(minor), Some(patch)) = (major, minor, patch) {
            return Some((major, minor, patch));
        }
    }
    None
}

fn version_requirement(name: &str) -> &'static str {
    match name {
        "cargo" | "rustc" => ">=1.99.1 (project Rust toolchain)",
        "node" => "20.19+ on Node 20, or >=22.12 (bundled Vite engine requirement)",
        "pnpm" => "10.34.5 (project packageManager)",
        _ => "an available version reporting major.minor.patch",
    }
}

fn version_compatible(name: &str, version: (u64, u64, u64)) -> bool {
    match name {
        "cargo" | "rustc" => version >= (1, 99, 1),
        "node" => (version.0 == 20 && version >= (20, 19, 0)) || version >= (22, 12, 0),
        "pnpm" => version == (10, 34, 5),
        _ => true,
    }
}

async fn docker_checks(server_platform: Option<&str>) -> Vec<Check> {
    match probe("docker", &["info", "--format", "{{json .}}"]).await {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => assess_docker(&value, server_platform),
            Err(_) => vec![Check::new(
                "dockerEngine",
                false,
                "invalid_docker_info",
                "Docker must return structured engine information.",
            )],
        },
        Err(reason) => vec![probe_check("dockerEngine", reason)],
    }
}

fn assess_docker(value: &Value, server_platform: Option<&str>) -> Vec<Check> {
    let os = match value["OSType"].as_str() {
        Some("linux") => Some("linux"),
        Some("windows") => Some("windows"),
        _ => None,
    };
    let arch = match value["Architecture"].as_str() {
        Some("amd64" | "x86_64") => Some("amd64"),
        Some("arm64" | "aarch64") => Some("arm64"),
        _ => None,
    };
    let supported = os == Some("linux") && arch.is_some();
    let engine = Check::new(
        "dockerEngine",
        supported,
        if supported {
            "linux_engine_ready"
        } else {
            "linux_engine_required"
        },
        "Development requires a reachable Docker daemon using Linux containers on amd64 or arm64.",
    )
    .details(json!({"osType":os,"architecture":arch}));
    let comparison = if let (Some(arch), Some(server)) = (arch, server_platform) {
        let matches = os == Some("linux") && server == format!("linux/{arch}");
        Check::new("imagePlatform", matches,
            if matches { "image_platform_matches" } else { "image_platform_mismatch" },
            "Local image builds must target the server's runtime platform; this CLI does not implicitly enable cross-platform emulation.",
        ).details(json!({"enginePlatform":format!("{}/{arch}",os.unwrap_or("unknown")),"serverPlatform":server}))
    } else {
        Check::skipped(
            "imagePlatform",
            "A supported Docker architecture and server runtime platform are required to compare image compatibility.",
        )
    };
    vec![engine, comparison]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_readiness_does_not_require_development_tools() {
        let checks = vec![
            Check::new("authentication", true, "authenticated", "Authenticated."),
            Check::new(
                "serverCapabilities",
                true,
                "server_compatible",
                "Compatible.",
            ),
        ];
        assert!(ready(&checks));
        let mut checks = checks;
        checks.push(Check::skipped(
            "imagePlatform",
            "A required development prerequisite is missing.",
        ));
        assert!(!ready(&checks));
    }

    #[test]
    fn capability_contract_rejects_missing_features_and_unknown_platforms() {
        let value = json!({"schemaVersion":1,"cliContractVersion":1,"features":["runtime.preflight.v1"],"runtimePlatform":"linux/amd64"});
        assert_eq!(assess_capabilities(&value).0.status, "pass");
        let mut invalid = value.clone();
        invalid["features"] = json!([]);
        assert_eq!(assess_capabilities(&invalid).0.status, "fail");
        invalid = value;
        invalid["runtimePlatform"] = json!("TOKEN-SENTINEL");
        let (check, platform) = assess_capabilities(&invalid);
        assert!(platform.is_none());
        assert!(
            !serde_json::to_string(&check)
                .unwrap()
                .contains("TOKEN-SENTINEL")
        );
    }

    #[test]
    fn engine_checks_linux_mode_and_normalizes_architecture() {
        let checks = assess_docker(
            &json!({"OSType":"linux","Architecture":"x86_64"}),
            Some("linux/amd64"),
        );
        assert!(ready(&checks));
        let checks = assess_docker(
            &json!({"OSType":"windows","Architecture":"x86_64"}),
            Some("linux/amd64"),
        );
        assert_eq!(checks[0].code, "linux_engine_required");
        let checks = assess_docker(
            &json!({"OSType":"linux","Architecture":"aarch64"}),
            Some("linux/amd64"),
        );
        assert_eq!(checks[1].code, "image_platform_mismatch");
        assert!(!ready(&checks));
    }

    #[test]
    fn tool_versions_are_numeric_and_enforce_the_bundled_toolchain() {
        assert_eq!(
            parse_version(b"cargo 1.99.1 (abcdef 2026-01-01)"),
            Some((1, 99, 1))
        );
        assert_eq!(
            parse_version(b"Docker version 29.0.1, build abcdef"),
            Some((29, 0, 1))
        );
        assert_eq!(parse_version(b"v24.2.0\nTOKEN-SENTINEL"), Some((24, 2, 0)));
        assert_eq!(parse_version(b"TOKEN-SENTINEL"), None);
        assert!(!version_compatible("rustc", (1, 98, 0)));
        assert!(!version_compatible("node", (22, 11, 0)));
        assert!(!version_compatible("node", (21, 9, 0)));
        assert!(version_compatible("node", (20, 19, 0)));
        assert!(version_compatible("node", (24, 0, 0)));
        assert!(!version_compatible("pnpm", (10, 0, 0)));
    }
}
