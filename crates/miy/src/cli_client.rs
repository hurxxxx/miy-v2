use crate::app_cli::{Session, api, save};
use crate::cli_error::{self, Category, CliError};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::path::Path;
pub fn origin(url: &str) -> Result<String> {
    let invalid = || {
        CliError::new(
            Category::Usage,
            "invalid_platform_origin",
            "Use an HTTPS server origin without credentials, path, query or fragment. HTTP is allowed only for loopback servers.",
        )
    };
    let u = url::Url::parse(url).map_err(|_| invalid())?;
    if !(u.username().is_empty()
        && u.password().is_none()
        && u.query().is_none()
        && u.fragment().is_none()
        && u.path() == "/"
        && (u.scheme() == "https"
            || (u.scheme() == "http"
                && matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))))
    {
        return Err(invalid().into());
    }
    Ok(u.origin().ascii_serialization())
}
pub fn client(ca: Option<&Path>, seconds: u64) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(seconds))
        .redirect(reqwest::redirect::Policy::none());
    if let Some(ca) = ca {
        let bytes = std::fs::read(ca).map_err(|_| {
            CliError::new(
                Category::Environment,
                "ca_file_unreadable",
                "The configured CA certificate file could not be read.",
            )
        })?;
        let certificate = reqwest::Certificate::from_pem(&bytes).map_err(|_| {
            CliError::new(
                Category::Environment,
                "invalid_ca_certificate",
                "The configured CA certificate is not valid PEM.",
            )
        })?;
        builder = builder.add_root_certificate(certificate);
    }
    builder.build().map_err(|_| {
        CliError::new(
            Category::Environment,
            "http_client_unavailable",
            "The HTTP client could not be initialized.",
        )
        .into()
    })
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
pub async fn login(no_open: bool) -> Result<Value> {
    let settings: Value = serde_json::from_slice(&std::fs::read(".miy/platform.json")?)?;
    let origin = settings["url"].as_str().context("missing_platform_url")?;
    let ca = settings["caFile"].as_str().map(std::path::PathBuf::from);
    let client = client(ca.as_deref(), 20)?;
    let verifier = crate::identity::random();
    let flow = cli_error::response(
        client
            .post(format!("{origin}/auth/cli/start"))
            .json(&json!({"challenge":crate::contract::hash(&verifier)}))
            .send()
            .await
            .map_err(cli_error::transport)?,
    )
    .await?;
    let invalid_response = || {
        CliError::new(
            Category::Network,
            "invalid_login_response",
            "The server returned an invalid CLI login response.",
        )
    };
    let id = flow["id"]
        .as_str()
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .ok_or_else(invalid_response)?;
    let path = flow["verificationPath"]
        .as_str()
        .ok_or_else(invalid_response)?;
    if path != format!("/cli/authorize/{id}") {
        return Err(invalid_response().into());
    }
    let url = format!("{origin}{path}");
    eprintln!("Open {url} and approve this CLI sign-in. Request: {}", id);
    if !no_open {
        let _ = crate::platform::open_browser(&url);
    }
    for _ in 0..150 {
        let value = cli_error::response(
            client
                .post(format!("{origin}/auth/cli/exchange"))
                .json(&json!({"id":flow["id"],"verifier":verifier}))
                .send()
                .await
                .map_err(cli_error::transport)?,
        )
        .await?;
        if value["status"] == "approved" {
            let s = Session {
                origin: origin.into(),
                token: Some(value["token"].as_str().ok_or_else(invalid_response)?.into()),
                cookie: String::new(),
                csrf: String::new(),
                ca_file: ca,
            };
            let me = api(&s, "/api/v1/me", None).await?;
            save(".miy/session.json", &serde_json::to_value(&s)?)?;
            return Ok(
                json!({"signedIn":me["name"],"expiresIn":28800,"authentication":"browser-approved CLI credential"}),
            );
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    Err(CliError::new(
        Category::Network,
        "cli_login_timed_out",
        "CLI login approval was not received before the timeout.",
    )
    .with_details(json!({"loginId":id}))
    .with_recovery("miy", &["app", "login"])
    .into())
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
        if size > crate::image_contract::MAX_ARCHIVE {
            return Err(CliError::new(Category::Environment, "image_archive_exceeds_1GiB", "The image archive exceeds the server's 1 GiB upload limit.").into());
        }
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
        let response = authenticate(s, request).send().await.map_err(cli_error::transport)?;
        let value = cli_error::response(response).await?;
        let id = value["id"].as_str().and_then(|value| uuid::Uuid::parse_str(value).ok())
            .ok_or_else(|| CliError::new(Category::Network, "invalid_upload_response", "The server did not return a valid image upload ID."))?;
        for _ in 0..300 {
            let status = api(s, &format!("/api/v1/image-uploads/{id}"), None).await?;
            if status["status"] == "ready" {
                return Ok(());
            }
            if status["status"] == "failed" {
                return Err(CliError::new(Category::RemoteFailure, "image_import_failed", "The server could not import the uploaded image.")
                    .with_details(json!({"uploadId":id,"serverCode":status["errorCode"].as_str().and_then(cli_error::safe_code)}))
                    .into());
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        Err(CliError::new(Category::Network, "image_import_timeout", "Image import did not finish before the observation timeout.")
            .with_details(json!({"uploadId":id,"remoteWorkCancelled":false}))
            .into())
    }
    .await;
    let _ = tokio::fs::remove_file(path).await;
    result
}
