//! Public installation guidance bound to the configured platform origin.
use crate::api_error::{ApiError, ApiResult};
use axum::{Json, Router, extract::State, routing::get};
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
    let mut commands = Commands {
        unix: format!(
            "bash ./install-cli.sh --server {}",
            shell_quote(&server_url)
        ),
        windows: format!(
            "powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\\install-cli.ps1 -ArchivePath $Archive -Sha256 $ExpectedSha256 -Server {}",
            powershell_quote(&server_url)
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
    if let Some(windows) = downloads
        .iter()
        .find(|item| item["target"] == "x86_64-pc-windows-msvc")
    {
        let file = windows["file"].as_str().unwrap();
        let url = format!("{}{}", server_url, windows["url"].as_str().unwrap());
        let hash = windows["sha256"].as_str().unwrap();
        commands.windows = format!(
            "$Archive = Join-Path $env:TEMP {}\n$ExpectedSha256 = {}\nInvoke-WebRequest -UseBasicParsing -Uri {} -OutFile $Archive\nif ((Get-FileHash -Algorithm SHA256 -LiteralPath $Archive).Hash.ToLowerInvariant() -ne $ExpectedSha256) {{ throw 'CLI checksum mismatch' }}\n$MiyExtract = Join-Path $env:TEMP ('miy-install-' + [Guid]::NewGuid().ToString('N'))\nExpand-Archive -LiteralPath $Archive -DestinationPath $MiyExtract\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $MiyExtract 'install-cli.ps1') -ArchivePath $Archive -Sha256 $ExpectedSha256 -Server {} -AddToPath\nif ($LASTEXITCODE -ne 0) {{ throw 'CLI installation or server registration failed' }}",
            powershell_quote(file),
            powershell_quote(hash),
            powershell_quote(&url),
            powershell_quote(&server_url)
        );
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

async fn install(State(origin): State<String>) -> ApiResult<Json<InstallGuide>> {
    guide(&origin).map(Json)
}

pub fn router<S: Clone + Send + Sync + 'static>(origin: String) -> Router<S> {
    let mut router = Router::new()
        .route("/api/v1/cli/install", get(install))
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
                format!("bash ./install-cli.sh --server '{expected}'")
            );
            assert!(
                result
                    .commands
                    .windows
                    .ends_with(&format!("-Server '{expected}'"))
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
            "bash ./install-cli.sh --server 'https://configured.example'"
        );
        assert!(!body.to_string().contains("untrusted.example"));
        stop.send(()).unwrap();
        server.await.unwrap();
    }
}
