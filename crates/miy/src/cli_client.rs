use crate::app_cli::{Session, api, save};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::path::Path;
pub fn origin(url: &str) -> Result<String> {
    let u = url::Url::parse(url)?;
    ensure!(
        u.username().is_empty()
            && u.password().is_none()
            && u.query().is_none()
            && u.fragment().is_none()
            && u.path() == "/"
            && (u.scheme() == "https"
                || (u.scheme() == "http"
                    && matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))),
        "platform_url_requires_https_origin_except_loopback"
    );
    Ok(u.origin().ascii_serialization())
}
pub fn client(ca: Option<&Path>, seconds: u64) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(seconds))
        .redirect(reqwest::redirect::Policy::none());
    if let Some(ca) = ca {
        builder =
            builder.add_root_certificate(reqwest::Certificate::from_pem(&std::fs::read(ca)?)?);
    }
    Ok(builder.build()?)
}
pub fn authenticate(s: &Session, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    if let Some(token) = &s.token {
        req.bearer_auth(token)
    } else {
        req.header("cookie", &s.cookie)
            .header("origin", &s.origin)
            .header("x-miy-csrf", &s.csrf)
    }
}
pub fn configure(root: &Path, url: &str, ca: Option<&Path>) -> Result<()> {
    let url = origin(url)?;
    let ca = ca.map(std::fs::canonicalize).transpose()?;
    if let Some(ref ca) = ca {
        reqwest::Certificate::from_pem(&std::fs::read(ca)?)?;
    }
    save(
        root.join(".miy/platform.json")
            .to_str()
            .context("invalid_path")?,
        &json!({"url":url,"caFile":ca}),
    )
}
pub async fn login(no_open: bool) -> Result<()> {
    let settings: Value = serde_json::from_slice(&std::fs::read(".miy/platform.json")?)?;
    let origin = settings["url"].as_str().context("missing_platform_url")?;
    let ca = settings["caFile"].as_str().map(std::path::PathBuf::from);
    let client = client(ca.as_deref(), 20)?;
    let verifier = crate::auth::random();
    let flow: Value = client
        .post(format!("{origin}/auth/cli/start"))
        .json(&json!({"challenge":crate::contract::hash(&verifier)}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let path = flow["verificationPath"]
        .as_str()
        .context("invalid_login_response")?;
    ensure!(
        path.starts_with("/cli/authorize/"),
        "invalid_verification_path"
    );
    let url = format!("{origin}{path}");
    eprintln!(
        "Open {url} and approve this CLI sign-in. Request: {}",
        flow["id"]
    );
    if !no_open {
        #[cfg(target_os = "macos")]
        let opener = "open";
        #[cfg(not(target_os = "macos"))]
        let opener = "xdg-open";
        let _ = std::process::Command::new(opener)
            .arg(&url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    for _ in 0..150 {
        let value: Value = client
            .post(format!("{origin}/auth/cli/exchange"))
            .json(&json!({"id":flow["id"],"verifier":verifier}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if value["status"] == "approved" {
            let s = Session {
                origin: origin.into(),
                token: Some(value["token"].as_str().context("missing_token")?.into()),
                cookie: String::new(),
                csrf: String::new(),
                ca_file: ca,
            };
            let me = api(&s, "/api/v1/me", None).await?;
            save(".miy/session.json", &serde_json::to_value(&s)?)?;
            println!(
                "{}",
                json!({"signedIn":me["name"],"expiresIn":28800,"authentication":"browser-approved CLI credential"})
            );
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    anyhow::bail!("cli_login_timed_out")
}
pub async fn upload(s: &Session, app: &str, image: &str) -> Result<()> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncReadExt;
    let path = std::env::current_dir()?.join(".miy/image-upload.tar");
    crate::app_cli::run(
        "docker",
        &[
            "save",
            "--output",
            path.to_str().context("invalid_path")?,
            image,
        ],
    )?;
    let result = async {
        let size = tokio::fs::metadata(&path).await?.len();
        ensure!(
            size <= crate::delivery::MAX_ARCHIVE,
            "image_archive_exceeds_1GiB"
        );
        let mut file = tokio::fs::File::open(&path).await?;
        let mut hash = Sha256::new();
        let mut buffer = vec![0u8; 65536];
        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        let request = client(s.ca_file.as_deref(), 300)?
            .post(format!("{}/api/v1/apps/{app}/images", s.origin))
            .header("content-length", size)
            .header("content-type", "application/x-tar")
            .header("x-miy-image", image)
            .header("x-miy-archive-sha256", hex::encode(hash.finalize()))
            .body(tokio::fs::File::open(&path).await?);
        let response = authenticate(s, request).send().await?;
        let status = response.status();
        let value: Value = response.json().await?;
        ensure!(
            status.is_success(),
            "image_upload_failed: {}",
            value["error"]["code"]
        );
        let id = value["id"].as_str().context("missing_upload_id")?;
        for _ in 0..300 {
            let status = api(s, &format!("/api/v1/image-uploads/{id}"), None).await?;
            if status["status"] == "ready" {
                return Ok(());
            }
            ensure!(
                status["status"] != "failed",
                "image_import_failed: {}",
                status["errorCode"]
            );
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        anyhow::bail!("image_import_timeout: {id}")
    }
    .await;
    let _ = tokio::fs::remove_file(path).await;
    result
}
