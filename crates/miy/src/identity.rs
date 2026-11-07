//! Shared app identity format and development-origin policy; no core session state.
use crate::api_error::{ApiError, ApiResult};
use axum::http::HeaderMap;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
#[cfg(any(feature = "server", test))]
use url::Url;
use uuid::Uuid;

pub fn random() -> String {
    let mut b = [0; 32];
    OsRng.fill_bytes(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}
pub fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    // Cookie scope does not include ports. Reject ambiguous authentication
    // values rather than choosing whichever host/path cookie arrived first.
    let mut found = None;
    for header in headers.get_all("cookie") {
        for part in header.to_str().ok()?.split(';') {
            let Some((key, value)) = part.trim().split_once('=') else {
                continue;
            };
            if key == name {
                if found.is_some() || value.is_empty() {
                    return None;
                }
                found = Some(value.to_owned());
            }
        }
    }
    found
}
/// Authentication must not fall back from an ambiguous development cookie to
/// another credential. Only the current purpose's names participate; unrelated
/// IdP cookies can legitimately have several paths.
#[cfg(feature = "server")]
pub fn unambiguous_cookies(headers: &HeaderMap, names: &[&str]) -> bool {
    let mut seen = std::collections::HashSet::new();
    for header in headers.get_all("cookie") {
        let Ok(value) = header.to_str() else {
            return false;
        };
        for part in value.split(';') {
            let Some((key, value)) = part.trim().split_once('=') else {
                continue;
            };
            if names.contains(&key) && (value.is_empty() || !seen.insert(key.to_owned())) {
                return false;
            }
        }
    }
    true
}
#[cfg(any(feature = "server", test))]
pub fn development_profile(flag: Option<&str>, origin: &str, development_origin: &str) -> bool {
    fn valid_origin(value: &str) -> Option<Url> {
        let url = Url::parse(value).ok()?;
        (matches!(url.scheme(), "http" | "https")
            && url.has_host()
            && url.username().is_empty()
            && url.password().is_none()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none()
            && url.origin().ascii_serialization() == value)
            .then_some(url)
    }
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
    match flag {
        Some("1") => local_origin(origin) && local_origin(development_origin),
        // Shared development is a separate explicit mode, confined to the
        // configured platform origin. Ordinary server profiles remain off.
        Some("shared") => origin == development_origin && valid_origin(origin).is_some(),
        _ => false,
    }
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Caller {
    pub app_id: String,
    pub installation_id: Uuid,
    pub release_id: Uuid,
}

/// Trusted gateway metadata. The opaque proof is issued by the core, never by
/// an application holding its own installation's gateway signing key.
#[derive(Clone, Deserialize, Serialize)]
pub struct IdentityContext {
    pub environment: String,
    pub callers: Vec<Caller>,
    pub effective_permissions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delegation_token: Option<String>,
}

impl std::fmt::Debug for IdentityContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentityContext")
            .field("environment", &self.environment)
            .field("callers", &self.callers)
            .field("effective_permissions", &self.effective_permissions)
            .field(
                "delegation_token",
                &self.delegation_token.as_ref().map(|_| "[redacted]"),
            )
            .finish()
    }
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
    #[serde(flatten)]
    context: IdentityContext,
}
pub(crate) fn sign_identity(
    user: &str,
    admin: bool,
    installation: Uuid,
    release: Uuid,
    secret: &str,
) -> ApiResult<String> {
    sign_identity_context(
        user,
        admin,
        installation,
        release,
        secret,
        &IdentityContext {
            environment: "dev".into(),
            callers: vec![],
            effective_permissions: vec![],
            delegation_token: None,
        },
    )
}

pub(crate) fn sign_identity_context(
    user: &str,
    admin: bool,
    installation: Uuid,
    release: Uuid,
    secret: &str,
    context: &IdentityContext,
) -> ApiResult<String> {
    if !matches!(context.environment.as_str(), "dev" | "prod")
        || context.callers.len() > 8
        || context
            .effective_permissions
            .iter()
            .any(|permission| !crate::contract::valid_permission(permission))
    {
        return Err(ApiError::unavailable("invalid_gateway_context"));
    }
    let claims = Claims {
        sub: user.into(),
        admin,
        installation_id: installation,
        release_id: release,
        iss: "miy",
        aud: "miy-app",
        exp: chrono::Utc::now().timestamp() + 30,
        context: context.clone(),
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
    use super::*;
    #[test]
    fn session_cookie_requires_one_unambiguous_value_across_all_headers() {
        let mut headers = HeaderMap::new();
        headers.append(
            "cookie",
            "miy_portal_session=portal; miy_console_session=console"
                .parse()
                .unwrap(),
        );
        assert_eq!(
            cookie(&headers, "miy_portal_session").as_deref(),
            Some("portal")
        );
        assert_eq!(
            cookie(&headers, "miy_console_session").as_deref(),
            Some("console")
        );
        headers.append("cookie", "miy_portal_session=other".parse().unwrap());
        assert_eq!(cookie(&headers, "miy_portal_session"), None);
        headers.insert(
            "cookie",
            "miy_portal_session=portal; miy_portal_session=portal"
                .parse()
                .unwrap(),
        );
        assert_eq!(cookie(&headers, "miy_portal_session"), None);
        headers.insert("cookie", "miy_portal_session=".parse().unwrap());
        assert_eq!(cookie(&headers, "miy_portal_session"), None);
    }
    #[test]
    fn contextual_identity_preserves_actor_target_chain_and_private_proof() {
        let source = Uuid::new_v4();
        let source_release = Uuid::new_v4();
        let target = Uuid::new_v4();
        let target_release = Uuid::new_v4();
        let context = IdentityContext {
            environment: "prod".into(),
            callers: vec![Caller {
                app_id: "source-app".into(),
                installation_id: source,
                release_id: source_release,
            }],
            effective_permissions: vec!["app:next-app:read".into()],
            delegation_token: Some("private-core-proof".into()),
        };
        let secret = "test-installation-gateway-secret-at-least-32-characters";
        let token = sign_identity_context(
            "original-user",
            false,
            target,
            target_release,
            secret,
            &context,
        )
        .unwrap();
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
        validation.set_audience(&["miy-app"]);
        validation.set_issuer(&["miy"]);
        let decoded = jsonwebtoken::decode::<serde_json::Value>(
            &token,
            &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
            &validation,
        )
        .unwrap()
        .claims;
        assert_eq!(decoded["sub"], "original-user");
        assert_eq!(decoded["admin"], false);
        assert_eq!(decoded["installation_id"], target.to_string());
        assert_eq!(decoded["release_id"], target_release.to_string());
        assert_eq!(decoded["callers"][0]["installation_id"], source.to_string());
        assert_eq!(
            decoded["effective_permissions"],
            serde_json::json!(["app:next-app:read"])
        );
        assert_eq!(decoded["delegation_token"], "private-core-proof");
        assert!(!format!("{context:?}").contains("private-core-proof"));
        let mut too_deep = context.clone();
        too_deep.callers = vec![context.callers[0].clone(); 9];
        assert!(
            sign_identity_context("user", false, target, target_release, secret, &too_deep)
                .is_err()
        );
    }
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

    #[test]
    fn shared_development_requires_explicit_mode_and_exact_origin() {
        for origin in ["https://development.example", "http://10.2.3.4:8080"] {
            assert!(development_profile(Some("shared"), origin, origin));
            for flag in [None, Some("0"), Some("1"), Some("true")] {
                assert!(!development_profile(flag, origin, origin));
            }
            for other in ["https://another.example", "http://localhost:18000"] {
                assert!(!development_profile(Some("shared"), origin, other));
            }
        }
        for invalid in [
            "https://user:password@example.com",
            "https://example.com/path",
            "https://example.com?query=1",
            "https://example.com#fragment",
            "https://example.com/",
            "file:///tmp/development",
            "not-an-origin",
        ] {
            assert!(!development_profile(Some("shared"), invalid, invalid));
        }
    }
}
