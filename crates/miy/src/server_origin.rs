//! Server-owned transport policy. Client URLs remain explicit user selections.
use anyhow::Result;
use url::{Host, Url};

pub fn validate(origin: &str, allow_http: bool) -> Result<String> {
    let origin = crate::cli_client::origin(origin)?;
    let url = Url::parse(&origin)?;
    let loopback = match url.host() {
        Some(Host::Domain("localhost")) => true,
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    anyhow::ensure!(
        url.scheme() == "https" || loopback || allow_http,
        "HTTP outside loopback requires MIY_ALLOW_HTTP=1; set MIY_ORIGIN explicitly"
    );
    Ok(origin)
}

pub fn issuer(value: &str, allow_http: bool) -> Result<String> {
    let url = Url::parse(value)?;
    anyhow::ensure!(
        value.contains("://")
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "invalid_identity_issuer"
    );
    validate(&url.origin().ascii_serialization(), allow_http)?;
    Ok(value.trim_end_matches('/').to_owned())
}

pub fn cookie(origin: &str, name: &str, value: &str, path: &str, max_age: u32) -> String {
    let secure = if origin.starts_with("https://") {
        "; Secure"
    } else {
        ""
    };
    format!("{name}={value}; HttpOnly{secure}; SameSite=Lax; Path={path}; Max-Age={max_age}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_http_requires_server_opt_in_and_does_not_change_https() {
        for origin in [
            "http://10.2.3.4:8080",
            "http://intranet.example",
            "http://[fd00::4]",
        ] {
            assert!(validate(origin, false).is_err());
            assert_eq!(validate(origin, true).unwrap(), origin);
        }
        for origin in [
            "http://localhost:18000",
            "http://127.0.0.2",
            "http://[::1]",
            "https://v2.example",
        ] {
            assert_eq!(validate(origin, false).unwrap(), origin);
        }
        assert!(validate("http://localhost.example", false).is_err());
        assert!(validate("https://user:password@example.com", true).is_err());
    }

    #[test]
    fn identity_transport_obeys_explicit_http_policy() {
        assert!(issuer("http://10.2.3.4/identity/realms/miy", false).is_err());
        assert_eq!(
            issuer("http://10.2.3.4/identity/realms/miy/", true).unwrap(),
            "http://10.2.3.4/identity/realms/miy"
        );
        assert!(issuer("https://example.com/identity?query=bad", false).is_err());
    }

    #[test]
    fn cookies_follow_configured_origin_including_expiration() {
        for max_age in [0, 300, 28800] {
            assert_eq!(
                cookie("http://10.2.3.4", "miy_session", "value", "/", max_age),
                format!("miy_session=value; HttpOnly; SameSite=Lax; Path=/; Max-Age={max_age}")
            );
            assert_eq!(
                cookie("https://v2.example", "miy_session", "value", "/", max_age),
                format!(
                    "miy_session=value; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={max_age}"
                )
            );
        }
    }
}
