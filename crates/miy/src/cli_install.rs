//! Public installation guidance bound to the configured platform origin.
use crate::api_error::{ApiError, ApiResult};
use axum::{Json, Router, extract::State, http::header, routing::get};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallGuide {
    schema_version: u8,
    server_url: String,
    cli_version: &'static str,
    commands: Commands,
    downloads: Vec<Value>,
    tunnel_images: Vec<Value>,
    source_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    windows_installer: Option<Value>,
}

#[derive(Serialize)]
struct Commands {
    unix: String,
    windows: String,
    login: &'static str,
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn guide(origin: &str) -> ApiResult<InstallGuide> {
    let server_url = crate::cli_client::origin(origin)
        .map_err(|_| ApiError::unavailable("invalid_platform_origin"))?;
    let commands = Commands {
        unix: format!(
            "curl -fsSL {} | bash",
            shell_quote(&format!("{server_url}/api/v1/cli/install.sh"))
        ),
        windows: format!(
            "& ([scriptblock]::Create((Invoke-RestMethod -Uri {})))",
            powershell_quote(&format!("{server_url}/api/v1/cli/install.ps1"))
        ),
        login: "miy auth login",
    };
    let mut downloads = vec![];
    let mut tunnel_images = vec![];
    let mut source_commit = None;
    if let Ok(root) = std::env::var("MIY_CLI_RELEASE_DIR") {
        let path = std::path::Path::new(&root).join("release-index.json");
        if path.exists() {
            let index: Value = serde_json::from_slice(
                &std::fs::read(path)
                    .map_err(|_| ApiError::unavailable("cli_release_unavailable"))?,
            )
            .map_err(|_| ApiError::unavailable("invalid_cli_release_index"))?;
            validate_index(&index)?;
            let commit = index["commit"]
                .as_str()
                .filter(|c| crate::helper_image::valid_hash(c, 40))
                .ok_or_else(|| ApiError::unavailable("invalid_cli_release_index"))?;
            if index["version"] != env!("CARGO_PKG_VERSION") {
                return Err(ApiError::unavailable("cli_release_version_mismatch"));
            }
            for row in index["assets"]
                .as_array()
                .ok_or_else(|| ApiError::unavailable("invalid_cli_release_index"))?
            {
                let file = asset_file(row)?;
                let mut download = row.clone();
                download["url"] = json!(format!("/cli/downloads/{commit}/{file}"));
                download["checksumUrl"] = json!(format!("/cli/downloads/{commit}/{file}.sha256"));
                downloads.push(download);
            }
            if let Some(rows) = index["tunnelImages"].as_array() {
                for row in rows {
                    let file = asset_file(row)?;
                    let mut item = row.clone();
                    item["url"] = json!(format!("/cli/downloads/{commit}/{file}"));
                    item["checksumUrl"] = json!(format!("/cli/downloads/{commit}/{file}.sha256"));
                    tunnel_images.push(item);
                }
            }
            source_commit = Some(commit.to_owned());
        }
    }
    let windows_installer = match std::env::var("MIY_CLI_INSTALLER_DIR") {
        Ok(root) => load_windows_installer(
            std::path::Path::new(&root),
            source_commit.as_deref(),
            &downloads,
        )?,
        Err(_) => None,
    };
    Ok(InstallGuide {
        schema_version: 1,
        server_url,
        cli_version: env!("CARGO_PKG_VERSION"),
        commands,
        downloads,
        tunnel_images,
        source_commit,
        windows_installer,
    })
}

fn load_windows_installer(
    root: &std::path::Path,
    cli_commit: Option<&str>,
    downloads: &[Value],
) -> ApiResult<Option<Value>> {
    let path = root.join("installer-index.json");
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(ApiError::unavailable("cli_installer_unavailable")),
    };
    let index: Value = serde_json::from_slice(&bytes)
        .map_err(|_| ApiError::unavailable("invalid_cli_installer_index"))?;
    windows_installer(&index, cli_commit, downloads).map(Some)
}

fn windows_installer(
    index: &Value,
    cli_commit: Option<&str>,
    downloads: &[Value],
) -> ApiResult<Value> {
    let invalid = || ApiError::unavailable("invalid_cli_installer_index");
    let windows = downloads
        .iter()
        .find(|item| item["target"] == "x86_64-pc-windows-msvc")
        .ok_or_else(invalid)?;
    let version = env!("CARGO_PKG_VERSION");
    let installer_commit = index["installerCommit"]
        .as_str()
        .filter(|commit| crate::helper_image::valid_hash(commit, 40))
        .ok_or_else(invalid)?;
    let file = index["file"].as_str().ok_or_else(invalid)?;
    if index["schemaVersion"] != 1
        || index["platform"] != "windows-x64"
        || index["version"] != version
        || cli_commit.is_none()
        || index["cliCommit"].as_str() != cli_commit
        || index["cliBinarySha256"] != windows["binarySha256"]
        || index["archiveSha256"] != windows["sha256"]
        || file != format!("MiySetup-{version}-windows-x64.exe")
        || !index["bytes"]
            .as_u64()
            .is_some_and(|size| size > 0 && size <= 532 * 1024 * 1024)
        || !["sha256", "cliBinarySha256", "archiveSha256"]
            .iter()
            .all(|field| {
                index[field]
                    .as_str()
                    .is_some_and(|hash| crate::helper_image::valid_hash(hash, 64))
            })
    {
        return Err(invalid());
    }
    let mut installer = index.clone();
    installer["url"] = json!(format!("/cli/installers/{installer_commit}/{file}"));
    installer["checksumUrl"] = json!(format!("/cli/installers/{installer_commit}/{file}.sha256"));
    Ok(installer)
}

fn validate_index(index: &Value) -> ApiResult<()> {
    let invalid = || ApiError::unavailable("invalid_cli_release_index");
    let commit = index["commit"]
        .as_str()
        .filter(|c| crate::helper_image::valid_hash(c, 40))
        .ok_or_else(invalid)?;
    let version = index["version"].as_str().ok_or_else(invalid)?;
    if index["schemaVersion"] != 1 || version != env!("CARGO_PKG_VERSION") {
        return Err(invalid());
    }
    let assets = index["assets"].as_array().ok_or_else(invalid)?;
    let targets = [
        "x86_64-unknown-linux-gnu",
        "aarch64-unknown-linux-gnu",
        "x86_64-apple-darwin",
        "aarch64-apple-darwin",
        "x86_64-pc-windows-msvc",
    ];
    let mut seen = std::collections::BTreeSet::new();
    for row in assets {
        let target = row["target"]
            .as_str()
            .filter(|target| targets.contains(target))
            .ok_or_else(invalid)?;
        let extension = if target == "x86_64-pc-windows-msvc" {
            "zip"
        } else {
            "tar.gz"
        };
        if !seen.insert(target)
            || row["schemaVersion"] != 1
            || row["commit"] != commit
            || row["version"] != version
            || asset_file(row)? != format!("miy-{version}-{target}.{extension}")
            || !row["binarySha256"]
                .as_str()
                .is_some_and(|hash| crate::helper_image::valid_hash(hash, 64))
        {
            return Err(invalid());
        }
    }
    if seen.len() != targets.len() {
        return Err(invalid());
    }
    seen.clear();
    for row in index["tunnelImages"].as_array().ok_or_else(invalid)? {
        crate::helper_image::Metadata::from_row(row, commit, version).map_err(|_| invalid())?;
        let platform = row["platform"].as_str().ok_or_else(invalid)?;
        if !seen.insert(platform) {
            return Err(invalid());
        }
    }
    if seen.len() != 2 {
        return Err(invalid());
    }
    Ok(())
}

fn asset_file(row: &Value) -> ApiResult<&str> {
    let file = row["file"]
        .as_str()
        .filter(|f| {
            !f.is_empty()
                && !f.contains("..")
                && f.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        })
        .ok_or_else(|| ApiError::unavailable("invalid_cli_release_index"))?;
    if !row["sha256"]
        .as_str()
        .is_some_and(|h| crate::helper_image::valid_hash(h, 64))
    {
        return Err(ApiError::unavailable("invalid_cli_release_index"));
    }
    Ok(file)
}

// Each response pins all URLs and hashes to one validated catalog snapshot.
// Scripts need neither jq/Python nor a second mutable index fetch on the PC.
fn online_script(guide: &InstallGuide, windows: bool) -> ApiResult<String> {
    if guide.downloads.is_empty() {
        return Err(ApiError::unavailable("cli_release_unavailable"));
    }
    let find = |target: &str| {
        guide
            .downloads
            .iter()
            .find(|row| row["target"] == target)
            .ok_or_else(|| ApiError::unavailable("cli_release_unavailable"))
    };
    let field = |row: &Value, key: &str| -> ApiResult<String> {
        row[key]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| ApiError::unavailable("invalid_cli_release_index"))
    };
    if windows {
        let row = find("x86_64-pc-windows-msvc")?;
        Ok(include_str!("../../../scripts/install-cli-online.ps1")
            .replace("@MIY_SERVER@", &powershell_quote(&guide.server_url))
            .replace("@MIY_VERSION@", &powershell_quote(guide.cli_version))
            .replace(
                "@MIY_URL@",
                &powershell_quote(&format!("{}{}", guide.server_url, field(row, "url")?)),
            )
            .replace("@MIY_SHA256@", &powershell_quote(&field(row, "sha256")?))
            .replace(
                "@MIY_BINARY_SHA256@",
                &powershell_quote(&field(row, "binarySha256")?),
            ))
    } else {
        let mut cases = Vec::new();
        for (pattern, target) in [
            ("Linux:x86_64", "x86_64-unknown-linux-gnu"),
            ("Linux:aarch64|Linux:arm64", "aarch64-unknown-linux-gnu"),
            ("Darwin:x86_64", "x86_64-apple-darwin"),
            ("Darwin:arm64|Darwin:aarch64", "aarch64-apple-darwin"),
        ] {
            let row = find(target)?;
            cases.push(format!(
                "    {pattern}) url={}; sha256={}; binary_sha256={} ;;",
                shell_quote(&format!("{}{}", guide.server_url, field(row, "url")?)),
                shell_quote(&field(row, "sha256")?),
                shell_quote(&field(row, "binarySha256")?)
            ));
        }
        Ok(include_str!("../../../scripts/install-cli-online.sh")
            .replace("@MIY_SERVER@", &shell_quote(&guide.server_url))
            .replace("@MIY_VERSION@", &shell_quote(guide.cli_version))
            .replace("# @MIY_ASSETS@", &cases.join("\n")))
    }
}

async fn unix_script(State(origin): State<String>) -> ApiResult<impl axum::response::IntoResponse> {
    Ok((
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        online_script(&guide(&origin)?, false)?,
    ))
}

async fn windows_script(
    State(origin): State<String>,
) -> ApiResult<impl axum::response::IntoResponse> {
    Ok((
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        online_script(&guide(&origin)?, true)?,
    ))
}

async fn install(State(origin): State<String>) -> ApiResult<Json<InstallGuide>> {
    guide(&origin).map(Json)
}

async fn agent_setup(State(origin): State<String>) -> ApiResult<impl axum::response::IntoResponse> {
    let origin = crate::cli_client::origin(&origin)
        .map_err(|_| ApiError::unavailable("invalid_platform_origin"))?;
    let text = include_str!("agent-setup.txt")
        .replace(
            "@MIY_UNIX_URL@",
            &shell_quote(&format!("{origin}/api/v1/cli/install.sh")),
        )
        .replace(
            "@MIY_WINDOWS_URL@",
            &powershell_quote(&format!("{origin}/api/v1/cli/install.ps1")),
        )
        .replace("@MIY_ORIGIN@", &origin);
    Ok((
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        text,
    ))
}

pub fn router<S: Clone + Send + Sync + 'static>(origin: String) -> Router<S> {
    let mut router = Router::new()
        .route("/api/v1/cli/install", get(install))
        .route("/api/v1/cli/install.sh", get(unix_script))
        .route("/api/v1/cli/install.ps1", get(windows_script))
        .route("/api/v1/cli/agent-setup", get(agent_setup))
        .with_state(origin);
    if let Ok(root) = std::env::var("MIY_CLI_RELEASE_DIR") {
        router = router.nest_service("/cli/downloads", tower_http::services::ServeDir::new(root));
    }
    if let Ok(root) = std::env::var("MIY_CLI_INSTALLER_DIR") {
        router = router.nest_service("/cli/installers", tower_http::services::ServeDir::new(root));
    }
    router
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use serde_json::Value;

    fn installer_fixture() -> (Value, Value, String) {
        let commit = "a".repeat(40);
        let windows = json!({
            "target": "x86_64-pc-windows-msvc",
            "binarySha256": "b".repeat(64),
            "sha256": "c".repeat(64),
        });
        let installer = json!({
            "schemaVersion": 1,
            "platform": "windows-x64",
            "version": env!("CARGO_PKG_VERSION"),
            "installerCommit": "d".repeat(40),
            "cliCommit": commit,
            "cliBinarySha256": windows["binarySha256"],
            "archiveSha256": windows["sha256"],
            "file": format!("MiySetup-{}-windows-x64.exe", env!("CARGO_PKG_VERSION")),
            "sha256": "e".repeat(64),
            "bytes": 123456,
        });
        (installer, windows, commit)
    }

    #[test]
    fn installer_download_uses_its_own_commit_and_matches_the_selected_cli() {
        let (index, windows, commit) = installer_fixture();
        let result = windows_installer(&index, Some(&commit), &[windows]).unwrap();
        let url = format!(
            "/cli/installers/{}/{}",
            "d".repeat(40),
            index["file"].as_str().unwrap()
        );
        assert_eq!(result["url"], url);
        assert_eq!(result["checksumUrl"], format!("{url}.sha256"));
        assert_eq!(result["cliCommit"], commit);
        assert_ne!(result["cliCommit"], result["installerCommit"]);
    }

    #[test]
    fn installer_rejects_mixed_cli_versions_hashes_and_unsafe_downloads() {
        let (index, windows, commit) = installer_fixture();
        for (field, value) in [
            ("schemaVersion", json!(2)),
            ("platform", json!("windows-arm64")),
            ("version", json!("0.0.0")),
            ("installerCommit", json!("../escape")),
            ("cliCommit", json!("f".repeat(40))),
            ("cliBinarySha256", json!("f".repeat(64))),
            ("archiveSha256", json!("f".repeat(64))),
            ("file", json!("../MiySetup-0.6.0-windows-x64.exe")),
            ("file", json!("https://foreign.example/setup.exe")),
            ("sha256", json!("z".repeat(64))),
            ("bytes", json!(0)),
            ("bytes", json!(-1)),
            ("bytes", json!(532_u64 * 1024 * 1024 + 1)),
        ] {
            let mut changed = index.clone();
            changed[field] = value;
            assert_eq!(
                windows_installer(&changed, Some(&commit), std::slice::from_ref(&windows))
                    .unwrap_err()
                    .code,
                "invalid_cli_installer_index",
                "{field}",
            );
        }
        assert!(windows_installer(&index, None, std::slice::from_ref(&windows)).is_err());
        assert!(windows_installer(&index, Some(&commit), &[]).is_err());
    }

    #[test]
    fn missing_installer_index_is_optional_but_invalid_index_is_not_advertised() {
        let root = tempfile::tempdir().unwrap();
        let (index, windows, commit) = installer_fixture();
        let downloads = [windows];
        assert!(
            load_windows_installer(root.path(), Some(&commit), &downloads)
                .unwrap()
                .is_none()
        );
        let path = root.path().join("installer-index.json");
        std::fs::write(&path, b"not json").unwrap();
        assert_eq!(
            load_windows_installer(root.path(), Some(&commit), &downloads)
                .unwrap_err()
                .code,
            "invalid_cli_installer_index"
        );
        std::fs::write(&path, serde_json::to_vec(&index).unwrap()).unwrap();
        assert!(
            load_windows_installer(root.path(), Some(&commit), &downloads)
                .unwrap()
                .is_some()
        );
        assert!(load_windows_installer(root.path(), None, &[]).is_err());
    }

    #[test]
    fn catalog_rejects_mixed_candidates_and_development_helpers() {
        let commit = "a".repeat(40);
        let version = env!("CARGO_PKG_VERSION");
        let assets: Vec<Value> = [
            "x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu",
            "x86_64-apple-darwin", "aarch64-apple-darwin", "x86_64-pc-windows-msvc",
        ].iter().map(|target| {
            let extension = if *target == "x86_64-pc-windows-msvc" { "zip" } else { "tar.gz" };
            json!({"schemaVersion":1,"version":version,"commit":commit,"target":target,
                "file":format!("miy-{version}-{target}.{extension}"),"sha256":"b".repeat(64),"binarySha256":"c".repeat(64)})
        }).collect();
        let helpers: Vec<Value> = ["amd64", "arm64"].iter().map(|arch| {
            json!({"schemaVersion":1,"platform":format!("linux/{arch}"),"version":version,"commit":commit,
                "imageTag":format!("miy-db-tunnel:{commit}-{arch}"),"configSha256":"d".repeat(64),
                "file":format!("miy-db-tunnel-{version}-{arch}.oci.tar"),"sha256":"e".repeat(64),"development":false})
        }).collect();
        let index = json!({"schemaVersion":1,"version":version,"commit":commit,"assets":assets,"tunnelImages":helpers});
        validate_index(&index).unwrap();
        for (collection, field, value) in [
            ("assets", "commit", json!("f".repeat(40))),
            ("assets", "schemaVersion", json!(2)),
            ("assets", "binarySha256", json!("Z".repeat(64))),
            ("tunnelImages", "development", json!(true)),
            ("tunnelImages", "version", json!("0.5.0")),
            ("tunnelImages", "imageTag", json!("miy-db-tunnel:latest")),
        ] {
            let mut changed = index.clone();
            changed[collection][0][field] = value;
            assert!(validate_index(&changed).is_err());
        }
        for collection in ["assets", "tunnelImages"] {
            let mut changed = index.clone();
            changed[collection][1] = changed[collection][0].clone();
            assert!(validate_index(&changed).is_err());
            changed[collection].as_array_mut().unwrap().pop();
            assert!(validate_index(&changed).is_err());
        }
    }

    #[test]
    fn commands_follow_the_canonical_configured_server() {
        for (input, expected) in [
            ("https://FIRST.example:443/", "https://first.example"),
            ("https://second.example:8443", "https://second.example:8443"),
            ("http://localhost:18000", "http://localhost:18000"),
            ("http://192.168.0.10:8080", "http://192.168.0.10:8080"),
        ] {
            let result = guide(input).unwrap();
            assert_eq!(result.server_url, expected);
            assert_eq!(result.cli_version, env!("CARGO_PKG_VERSION"));
            assert_eq!(
                result.commands.unix,
                format!("curl -fsSL '{expected}/api/v1/cli/install.sh' | bash")
            );
            assert!(
                result
                    .commands
                    .windows
                    .contains(&format!("'{expected}/api/v1/cli/install.ps1'"))
            );
        }
        for invalid in [
            "http://",
            "https://user:password@example.com",
            "https://example.com/path",
            "https://example.com/?command=$(touch%20file)",
            "https://example.com/#';echo injected",
        ] {
            assert_eq!(
                guide(invalid).err().unwrap().code,
                "invalid_platform_origin"
            );
        }
    }

    #[test]
    fn online_installers_pin_all_targets_and_preserve_ip_port_origins() {
        for origin in [
            "https://configured.example",
            "http://192.168.1.10:8080",
            "http://[::1]:8080",
        ] {
            let mut fixture = guide(origin).unwrap();
            assert_eq!(
                online_script(&fixture, false).unwrap_err().code,
                "cli_release_unavailable"
            );
            for target in [
                "x86_64-unknown-linux-gnu",
                "aarch64-unknown-linux-gnu",
                "x86_64-apple-darwin",
                "aarch64-apple-darwin",
                "x86_64-pc-windows-msvc",
            ] {
                fixture.downloads.push(json!({"target": target, "url": format!("/cli/downloads/{}/{target}", "a".repeat(40)), "sha256": "b".repeat(64), "binarySha256": "c".repeat(64)}));
            }
            for windows in [false, true] {
                let script = online_script(&fixture, windows).unwrap();
                assert!(!script.contains("@MIY_"));
                assert!(script.contains(&format!("{origin}/cli/downloads/{}", "a".repeat(40))));
                assert!(script.contains(&"b".repeat(64)));
                assert!(script.contains(&"c".repeat(64)));
                if !windows {
                    let file = tempfile::NamedTempFile::new().unwrap();
                    std::fs::write(file.path(), script).unwrap();
                    assert!(
                        std::process::Command::new("bash")
                            .arg("-n")
                            .arg(file.path())
                            .status()
                            .unwrap()
                            .success()
                    );
                }
            }
        }
    }

    #[test]
    fn shell_literals_keep_quotes_and_expansion_characters_in_one_argument() {
        let value = "https://a'b$HOME;`echo bad`.example";
        assert_eq!(
            shell_quote(value),
            "'https://a'\"'\"'b$HOME;`echo bad`.example'"
        );
        assert_eq!(
            powershell_quote(value),
            "'https://a''b$HOME;`echo bad`.example'"
        );
        #[cfg(unix)]
        {
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("printf '%s' {}", shell_quote(value)))
                .output()
                .unwrap();
            assert!(output.status.success());
            assert_eq!(String::from_utf8(output.stdout).unwrap(), value);
        }
    }

    #[tokio::test]
    async fn public_route_uses_server_configuration_even_with_foreign_host_headers() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            axum::serve(listener, router::<()>("https://configured.example/".into()))
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
                .unwrap();
        });
        let response = reqwest::Client::new()
            .get(format!("http://{address}/api/v1/cli/install"))
            .header("host", "untrusted.example")
            .header("x-forwarded-host", "other.example")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["serverUrl"], "https://configured.example");
        assert_eq!(
            body["commands"]["unix"],
            "curl -fsSL 'https://configured.example/api/v1/cli/install.sh' | bash"
        );
        assert!(!body.to_string().contains("untrusted.example"));
        let setup = reqwest::Client::new()
            .get(format!("http://{address}/api/v1/cli/agent-setup"))
            .header("host", "untrusted.example")
            .header("x-forwarded-host", "other.example")
            .send()
            .await
            .unwrap();
        assert_eq!(setup.status(), StatusCode::OK);
        assert_eq!(
            setup.headers()[header::CONTENT_TYPE],
            "text/plain; charset=utf-8"
        );
        assert_eq!(setup.headers()[header::CACHE_CONTROL], "no-store");
        let text = setup.text().await.unwrap();
        for reference in [
            "https://configured.example/api/v1/cli/install",
            "'https://configured.example/api/v1/cli/install.sh'",
            "'https://configured.example/api/v1/cli/install.ps1'",
            "https://configured.example/mcp",
            "auth login --device-code --no-open --no-wait",
            "auth wait LOGIN_ID",
            "app init ./miy-agent-workspace",
            "app setup",
            "get_cli_help({\"topic\":\"api\"})",
            "get_contract({})",
            "data.authenticated",
            "data.verificationUrl",
            "data.userCode",
            "data.loginId",
            "data.expiresIn",
            "end this turn with the approval instructions",
            "login_wait_interrupted",
            "login_not_approved",
            "credential_storage_unavailable",
            "platformBundleVersion",
            "docs/CLI-REFERENCE.md",
            "docs/ACCEPTANCE.md",
            "miy app source-pin",
            "miy app deploy --environment dev --dry-run",
            "miy job resume BUILD_ID --environment dev",
            "miy app accept --input @acceptance.json",
            "app promote --environment prod --release RELEASE_ID --dry-run",
        ] {
            assert!(
                text.contains(reference),
                "missing setup reference {reference}"
            );
        }
        assert!(!text.contains("untrusted.example"));
        assert!(!text.contains("other.example"));
        assert!(!text.contains("@MIY_"));
        for extension in ["sh", "ps1"] {
            let response = reqwest::get(format!("http://{address}/api/v1/cli/install.{extension}"))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        }
        stop.send(()).unwrap();
        server.await.unwrap();
    }
}
