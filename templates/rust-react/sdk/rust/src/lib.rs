//! miy runtime SDK v0.3.0. Verified user context and bounded core-mediated calls.
use axum::{
    Json, Router,
    extract::Request,
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::Deserialize;
use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::time::Duration;

pub use reqwest::Method;

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
pub struct Caller {
    pub app_id: String,
    pub installation_id: String,
    pub release_id: String,
}

/// The core checks current grants and every caller's installation approval on
/// each hop. These fields describe that context; they do not grant authority.
#[derive(Debug, Clone)]
pub struct InvocationContext {
    pub installation_id: String,
    pub release_id: String,
    /// None identifies a legacy gateway token with no contextual claims.
    pub environment: Option<String>,
    pub callers: Vec<Caller>,
    pub effective_permissions: Vec<String>,
}

#[derive(Clone)]
pub struct Identity {
    pub user_id: String,
    pub admin: bool,
    pub context: InvocationContext,
    delegation_token: Option<String>,
}
impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("user_id", &self.user_id)
            .field("admin", &self.admin)
            .field("context", &self.context)
            .field(
                "delegation_token",
                &self.delegation_token.as_ref().map(|_| "[redacted]"),
            )
            .finish()
    }
}
#[derive(Deserialize)]
struct Claims {
    sub: String,
    admin: bool,
    installation_id: String,
    release_id: String,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    callers: Vec<Caller>,
    #[serde(default)]
    effective_permissions: Vec<String>,
    #[serde(default)]
    delegation_token: Option<String>,
}
fn verify_identity(
    token: &str,
    secret: &str,
    installation: &str,
    release: &str,
) -> Option<Identity> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["miy-app"]);
    validation.set_issuer(&["miy"]);
    validation.leeway = 0;
    let claims = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .ok()?
    .claims;
    if claims.installation_id != installation
        || claims.release_id != release
        || claims.sub.is_empty()
        || claims.callers.len() > 8
        || claims
            .environment
            .as_deref()
            .is_some_and(|env| !matches!(env, "dev" | "prod"))
        || (claims.delegation_token.is_some() && claims.environment.is_none())
    {
        return None;
    }
    Some(Identity {
        user_id: claims.sub,
        admin: claims.admin,
        context: InvocationContext {
            installation_id: claims.installation_id,
            release_id: claims.release_id,
            environment: claims.environment,
            callers: claims.callers,
            effective_permissions: claims.effective_permissions,
        },
        delegation_token: claims.delegation_token,
    })
}
async fn identity(mut request: Request, next: Next) -> Response {
    let valid = (|| {
        verify_identity(
            request.headers().get("x-miy-identity")?.to_str().ok()?,
            &std::env::var("MIY_GATEWAY_SECRET").ok()?,
            &std::env::var("MIY_INSTALLATION_ID").ok()?,
            &std::env::var("MIY_RELEASE_ID").ok()?,
        )
    })();
    match valid {
        Some(user) => {
            request.headers_mut().remove("x-miy-identity");
            request.extensions_mut().insert(user);
            next.run(request).await
        }
        None => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":{"code":"invalid_gateway_identity"}})),
        )
            .into_response(),
    }
}

/// Errors deliberately omit response bodies, URLs and underlying client errors:
/// none of those are safe places to include a bearer proof or application data.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CallError {
    pub code: &'static str,
    pub status: Option<u16>,
}
impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)
    }
}
impl std::error::Error for CallError {}
fn call_error(code: &'static str) -> CallError {
    CallError { code, status: None }
}

fn valid_app_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 48
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
fn core_url(
    origin: &str,
    installation: &str,
    suffix: &str,
    allow_http: bool,
) -> Result<reqwest::Url, CallError> {
    let base = reqwest::Url::parse(origin).map_err(|_| call_error("invalid_core_origin"))?;
    if !(base.scheme() == "https" || (allow_http && base.scheme() == "http"))
        || !base.has_host()
        || !base.username().is_empty()
        || base.password().is_some()
        || base.path() != "/"
        || base.query().is_some()
        || base.fragment().is_some()
        || !valid_uuid(installation)
    {
        return Err(call_error("invalid_core_origin"));
    }
    let prefix = format!("/api/v1/installations/{installation}/");
    let url = base
        .join(&format!("{prefix}{suffix}"))
        .map_err(|_| call_error("invalid_api_path"))?;
    if url.origin() != base.origin() || !url.path().starts_with(&prefix) || url.fragment().is_some()
    {
        return Err(call_error("invalid_api_path"));
    }
    Ok(url)
}
fn valid_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}
fn app_suffix(app: &str, path: &str) -> Result<String, CallError> {
    if !valid_app_id(app)
        || !path.starts_with('/')
        || path.starts_with("//")
        || path.contains(['\\', '#'])
        || path.bytes().any(|c| c.is_ascii_control())
    {
        return Err(call_error("invalid_api_path"));
    }
    // Reject encoded separators/dot segments as well as plain traversal before
    // the URL implementation has an opportunity to normalize them away.
    let route = path
        .split('?')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if route
        .split('/')
        .any(|segment| matches!(segment, "." | ".."))
        || ["%2e", "%2f", "%5c", "%25"]
            .iter()
            .any(|encoded| route.contains(encoded))
    {
        return Err(call_error("invalid_api_path"));
    }
    Ok(format!("connections/{app}/api{path}"))
}

impl Identity {
    /// Sensitive short-lived bearer proof. Do not return it to the browser,
    /// persist it, log it, or substitute the installation's HS256 identity JWT.
    pub fn delegation_token(&self) -> Option<&str> {
        self.delegation_token.as_deref()
    }

    pub async fn app_api(
        &self,
        app: &str,
        path: &str,
        method: Method,
        body: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, CallError> {
        if !matches!(
            method,
            Method::GET
                | Method::HEAD
                | Method::POST
                | Method::PUT
                | Method::PATCH
                | Method::DELETE
        ) {
            return Err(call_error("unsupported_api_method"));
        }
        self.call(&app_suffix(app, path)?, method, body).await
    }

    pub async fn platform_api(&self, resource: &str) -> Result<serde_json::Value, CallError> {
        if resource != "directory" {
            return Err(call_error("unknown_platform_resource"));
        }
        self.call("platform/directory", Method::GET, None).await
    }

    async fn call(
        &self,
        suffix: &str,
        method: Method,
        body: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, CallError> {
        if self.delegation_token().is_none_or(str::is_empty) {
            return Err(call_error("delegation_unavailable"));
        }
        let origin =
            std::env::var("MIY_CORE_ORIGIN").map_err(|_| call_error("core_origin_unavailable"))?;
        let allow_http = std::env::var("MIY_CORE_ALLOW_HTTP").as_deref() == Ok("1");
        self.call_at(&origin, allow_http, suffix, method, body)
            .await
    }

    async fn call_at(
        &self,
        origin: &str,
        allow_http: bool,
        suffix: &str,
        method: Method,
        body: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value, CallError> {
        let token = self
            .delegation_token()
            .filter(|token| !token.is_empty())
            .ok_or_else(|| call_error("delegation_unavailable"))?;
        let url = core_url(origin, &self.context.installation_id, suffix, allow_http)?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| call_error("core_client_unavailable"))?;
        let mut request = client
            .request(method, url)
            .bearer_auth(token)
            .header("x-miy-release", &self.context.release_id);
        if let Some(body) = body {
            let bytes = serde_json::to_vec(body).map_err(|_| call_error("invalid_api_body"))?;
            if bytes.len() > 4 * 1024 * 1024 {
                return Err(call_error("request_too_large"));
            }
            request = request
                .header("content-type", "application/json")
                .body(bytes);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| call_error("core_unreachable"))?;
        if !response.status().is_success() {
            return Err(CallError {
                code: "core_call_rejected",
                status: Some(response.status().as_u16()),
            });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| call_error("core_response_failed"))?
        {
            if bytes.len() + chunk.len() > 4 * 1024 * 1024 {
                return Err(call_error("response_too_large"));
            }
            bytes.extend_from_slice(&chunk);
        }
        if bytes.is_empty() {
            return Ok(serde_json::Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(|_| call_error("invalid_core_response"))
    }
}
pub async fn database() -> anyhow::Result<PgPool> {
    Ok(PgPoolOptions::new()
        .max_connections(5)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?)
}
pub async fn serve(api: Router) -> anyhow::Result<()> {
    // No direct browser identity headers are trusted. API access always passes through the gateway.
    anyhow::ensure!(
        std::env::var("MIY_GATEWAY_SECRET")?.len() >= 32,
        "missing_gateway_secret"
    );
    let app = Router::new()
        .nest("/api", api.route_layer(middleware::from_fn(identity)))
        .route("/health",get(|| async { Json(json!({"status":"ok","releaseId":std::env::var("MIY_RELEASE_ID").unwrap_or_default()})) }));
    // Containers retain their existing address; native CLI launches use a
    // private loopback port and the same signed gateway identity contract.
    let address = std::env::var("MIY_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{EncodingKey, Header, encode};

    const INSTALLATION: &str = "00000000-0000-4000-8000-000000000001";
    const RELEASE: &str = "00000000-0000-4000-8000-000000000002";
    const SECRET: &str = "sdk-test-gateway-secret-never-a-core-delegation-key";
    fn claims() -> serde_json::Value {
        json!({"sub":"original-user","admin":false,"installation_id":INSTALLATION,
            "release_id":RELEASE,"iss":"miy","aud":"miy-app",
            "exp":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()+30})
    }
    fn signed(claims: &serde_json::Value) -> String {
        encode(
            &Header::new(Algorithm::HS256),
            claims,
            &EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .unwrap()
    }
    #[test]
    fn contextual_identity_preserves_original_user_and_redacts_proof() {
        let mut value = claims();
        value["environment"] = json!("dev");
        value["callers"] =
            json!([{"app_id":"source-app","installation_id":INSTALLATION,"release_id":RELEASE}]);
        value["effective_permissions"] = json!(["app:next-app:read"]);
        value["delegation_token"] = json!("SENSITIVE-OPAQUE-PROOF");
        let identity = verify_identity(&signed(&value), SECRET, INSTALLATION, RELEASE).unwrap();
        assert_eq!(identity.user_id, "original-user");
        assert!(!identity.admin);
        assert_eq!(identity.context.callers[0].app_id, "source-app");
        assert_eq!(
            identity.context.effective_permissions,
            ["app:next-app:read"]
        );
        assert_eq!(identity.delegation_token(), Some("SENSITIVE-OPAQUE-PROOF"));
        assert!(!format!("{identity:?}").contains("SENSITIVE-OPAQUE-PROOF"));
        assert!(verify_identity(&signed(&value), SECRET, RELEASE, RELEASE).is_none());
        value["environment"] = json!("staging");
        assert!(verify_identity(&signed(&value), SECRET, INSTALLATION, RELEASE).is_none());
    }
    #[tokio::test]
    async fn legacy_preview_identity_is_valid_but_cannot_delegate() {
        let identity = verify_identity(&signed(&claims()), SECRET, INSTALLATION, RELEASE).unwrap();
        assert_eq!(identity.context.environment, None);
        assert!(identity.context.callers.is_empty());
        assert_eq!(
            identity.platform_api("directory").await.unwrap_err().code,
            "delegation_unavailable"
        );
    }
    #[test]
    fn expired_wrong_audience_and_excessive_depth_are_rejected() {
        for (field, value) in [
            ("exp", json!(1)),
            ("aud", json!("miy-core")),
            (
                "callers",
                json!(vec![
                    json!({"app_id":"a","installation_id":INSTALLATION,"release_id":RELEASE});
                    9
                ]),
            ),
        ] {
            let mut data = claims();
            data[field] = value;
            assert!(verify_identity(&signed(&data), SECRET, INSTALLATION, RELEASE).is_none());
        }
    }
    #[test]
    fn core_calls_cannot_escape_configured_origin_or_connection_route() {
        for origin in [
            "https://user:secret@example.com",
            "https://example.com/path",
            "https://example.com?x=1",
            "file:///etc/passwd",
        ] {
            assert!(core_url(origin, INSTALLATION, "platform/directory", false).is_err());
        }
        for path in [
            "https://example.com",
            "//example.com",
            "/../platform/directory",
            "/%2e%2e/secret",
            "/%252e%252e/secret",
            "/a%2fb",
            "/a\\b",
            "/a#fragment",
        ] {
            assert!(app_suffix("target-app", path).is_err(), "accepted {path}");
        }
        let suffix = app_suffix("target-app", "/records?limit=10").unwrap();
        let url = core_url("https://miy.example", INSTALLATION, &suffix, false).unwrap();
        assert_eq!(
            url.as_str(),
            format!(
                "https://miy.example/api/v1/installations/{INSTALLATION}/connections/target-app/api/records?limit=10"
            )
        );
        assert!(core_url("http://127.0.0.1:1", INSTALLATION, &suffix, false).is_err());
        assert!(core_url("http://127.0.0.1:1", INSTALLATION, &suffix, true).is_ok());
    }

    #[tokio::test]
    async fn outbound_proof_and_release_are_sent_only_to_core_without_redirect_or_error_leaks() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let token = "PRIVATE-OUTBOUND-PROOF";
        let mut contextual = claims();
        contextual["environment"] = json!("dev");
        contextual["delegation_token"] = json!(token);
        let identity =
            verify_identity(&signed(&contextual), SECRET, INSTALLATION, RELEASE).unwrap();
        for status in [200, 302, 403] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut buffer = [0; 1024];
                    let n = stream.read(&mut buffer).await.unwrap();
                    assert!(n > 0 && request.len() < 8192);
                    request.extend_from_slice(&buffer[..n]);
                    if request.windows(4).any(|part| part == b"\r\n\r\n") {
                        break;
                    }
                }
                let headers = String::from_utf8(request).unwrap().to_ascii_lowercase();
                assert!(headers.starts_with(&format!(
                    "get /api/v1/installations/{INSTALLATION}/platform/directory http/1.1"
                )));
                assert!(headers.contains("authorization: bearer private-outbound-proof\r\n"));
                assert!(headers.contains(&format!("x-miy-release: {RELEASE}\r\n")));
                assert!(!headers.contains("x-miy-identity:"));
                let body = if status == 200 {
                    "{\"users\":[]}"
                } else {
                    "PRIVATE-OUTBOUND-PROOF"
                };
                let response = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nLocation: http://127.0.0.1:1/leak\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            });
            let result = identity
                .call_at(&origin, true, "platform/directory", Method::GET, None)
                .await;
            server.await.unwrap();
            if status == 200 {
                assert_eq!(result.unwrap(), json!({"users":[]}));
            } else {
                let error = result.unwrap_err();
                assert_eq!(error.status, Some(status));
                assert_eq!(error.code, "core_call_rejected");
                assert!(!format!("{error:?} {error}").contains(token));
            }
        }
    }
}
