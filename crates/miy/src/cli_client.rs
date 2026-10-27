use crate::app_cli::{Session, save};
use crate::cli_error::{Category, CliError};
use anyhow::{Context, Result};
use serde_json::json;
use std::path::Path;
pub fn origin(url: &str) -> Result<String> {
    let invalid = || {
        CliError::new(
            Category::Usage,
            "invalid_platform_origin",
            "Enter an explicit http:// or https:// server origin, optionally with a port, without credentials, path, query or fragment.",
        )
    };
    let u = url::Url::parse(url).map_err(|_| invalid())?;
    if !(url.contains("://")
        && u.has_host()
        && u.username().is_empty()
        && u.password().is_none()
        && u.query().is_none()
        && u.fragment().is_none()
        && u.path() == "/"
        && matches!(u.scheme(), "http" | "https"))
    {
        return Err(invalid().into());
    }
    Ok(u.origin().ascii_serialization())
}

pub fn client(ca: Option<&Path>, seconds: u64) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .retry(reqwest::retry::never())
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

#[cfg(test)]
mod tests {
    use super::origin;

    #[test]
    fn explicit_http_and_https_origins_include_remote_ip_and_domain() {
        for (input, expected) in [
            ("http://192.168.10.4:8080", "http://192.168.10.4:8080"),
            ("http://INTRANET.example:80/", "http://intranet.example"),
            ("http://[fd00::4]:8080", "http://[fd00::4]:8080"),
            ("https://PLATFORM.example:443/", "https://platform.example"),
        ] {
            assert_eq!(origin(input).unwrap(), expected);
        }
        for input in [
            "192.168.10.4:8080",
            "platform.example",
            "http:platform.example",
            "https://user:password@platform.example",
            "https://platform.example/path",
            "http://platform.example/?x=1",
            "http://platform.example/#fragment",
            "ftp://platform.example",
        ] {
            assert!(origin(input).is_err());
        }
    }
}
