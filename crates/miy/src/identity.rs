//! Shared app identity format and local-origin policy; no core session state.
use crate::api_error::{ApiError, ApiResult};
use axum::http::HeaderMap;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rngs::OsRng};
use serde::Serialize;
#[cfg(any(feature = "server", test))]
use url::Url;
use uuid::Uuid;

pub fn random() -> String {
    let mut b = [0; 32];
    OsRng.fill_bytes(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}
pub fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|c| {
            let (k, v) = c.trim().split_once('=')?;
            (k == name).then(|| v.to_string())
        })
}
#[cfg(any(feature = "server", test))]
pub fn development_profile(flag: Option<&str>, origin: &str, development_origin: &str) -> bool {
    fn local_origin(value: &str) -> bool {
        let Ok(url) = Url::parse(value) else {
            return false;
        };
        matches!(url.scheme(), "http" | "https")
            && url.username().is_empty()
            && url.password().is_none()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none()
            && match url.host() {
                Some(url::Host::Domain("localhost")) => true,
                Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
                Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
                _ => false,
            }
    }
    flag == Some("1") && local_origin(origin) && local_origin(development_origin)
}
#[derive(Serialize)]
struct Claims {
    sub: String,
    admin: bool,
    installation_id: Uuid,
    release_id: Uuid,
    iss: &'static str,
    aud: &'static str,
    exp: i64,
}
pub(crate) fn sign_identity(
    user: &str,
    admin: bool,
    installation: Uuid,
    release: Uuid,
    secret: &str,
) -> ApiResult<String> {
    let claims = Claims {
        sub: user.into(),
        admin,
        installation_id: installation,
        release_id: release,
        iss: "miy",
        aud: "miy-app",
        exp: chrono::Utc::now().timestamp() + 30,
    };
    jsonwebtoken::encode(
        &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|_| ApiError::unavailable("gateway_identity_failed"))
}
#[cfg(test)]
mod tests {
    use super::development_profile;
    #[test]
    fn development_login_requires_explicit_local_profile() {
        assert!(!development_profile(
            None,
            "https://localhost:18443",
            "http://localhost:18000"
        ));
        assert!(!development_profile(
            Some("1"),
            "https://miy.example.com",
            "http://localhost:18000"
        ));
        assert!(!development_profile(
            Some("1"),
            "https://localhost:18443",
            "http://localhost.evil.example:18000"
        ));
        assert!(development_profile(
            Some("1"),
            "https://localhost:18443",
            "http://localhost:18000"
        ));
    }
}
