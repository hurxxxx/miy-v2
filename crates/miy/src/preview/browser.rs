//! Loopback login state. Only opaque, per-browser handles leave the gateway.
use super::*;
use axum::http::{HeaderMap, Method};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

pub(crate) fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}
pub(crate) fn valid_nonce(value: &str) -> bool {
    value.len() == 43
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
}

#[derive(Default)]
struct Cache<T>(Mutex<HashMap<String, (Instant, T)>>);
impl<T: Clone> Cache<T> {
    fn insert(&self, value: T, seconds: u64) -> ApiResult<String> {
        let mut cache = self.0.lock().unwrap();
        let now = Instant::now();
        cache.retain(|_, (until, _)| *until > now);
        if cache.len() >= 4096 {
            return Err(ApiError::unavailable("preview_session_capacity"));
        }
        let key = auth::random();
        cache.insert(key.clone(), (now + Duration::from_secs(seconds), value));
        Ok(key)
    }
    fn get(&self, key: &str) -> Option<T> {
        self.0
            .lock()
            .unwrap()
            .get(key)
            .filter(|(until, _)| *until > Instant::now())
            .map(|(_, v)| v.clone())
    }
    fn remove(&self, key: &str) {
        self.0.lock().unwrap().remove(key);
    }
}

#[derive(Clone, Default)]
struct Flow {
    state: String,
    verifier: String,
}
#[derive(Clone, Default)]
pub(super) struct Browser {
    grant: String,
    pub(super) csrf: String,
}
#[derive(Default)]
pub(super) struct Sessions {
    pending: Cache<Flow>,
    browsers: Cache<Browser>,
    continuations: Cache<Browser>,
}
impl Sessions {
    pub(super) fn continuation(&self, viewer: &Browser) -> ApiResult<String> {
        self.continuations.insert(viewer.clone(), 30)
    }
    pub(super) fn backend(&self, token: &str) -> Option<Browser> {
        self.continuations.get(token)
    }
}

impl Browser {
    #[cfg(test)]
    pub(super) fn fixture(actor: &str) -> Self {
        Self {
            grant: auth::random(),
            csrf: actor.into(),
        }
    }
    pub(super) fn authenticate(
        &self,
        s: &Preview,
        req: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        req.bearer_auth(&self.grant)
            .header("x-miy-preview-instance", s.release.to_string())
    }
    pub(super) async fn user(&self, s: &Preview) -> ApiResult<Value> {
        let client = crate::cli_client::client(s.session.ca_file.as_deref(), 20)?;
        let url = format!(
            "{}/api/v1/preview/apps/{}/browser/viewer",
            s.session.origin, s.manifest.id
        );
        let response = self
            .authenticate(s, client.get(url))
            .send()
            .await
            .map_err(|_| ApiError::unavailable("preview_portal_unavailable"))?;
        match response.status() {
            reqwest::StatusCode::UNAUTHORIZED => return Err(ApiError::unauthorized()),
            reqwest::StatusCode::FORBIDDEN => return Err(ApiError::forbidden()),
            status if !status.is_success() => {
                return Err(ApiError::unavailable("preview_portal_unavailable"));
            }
            _ => {}
        }
        let user: Value = response
            .json()
            .await
            .map_err(|_| ApiError::unavailable("invalid_preview_subject"))?;
        if user["id"].as_str().is_none_or(str::is_empty) {
            return Err(ApiError::unauthorized());
        }
        Ok(user)
    }
}

fn redirect(location: &str) -> Response {
    let mut r = Redirect::to(location).into_response();
    r.headers_mut()
        .insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    r.headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    r
}
fn cookie(response: &mut Response, name: &str, value: &str, path: &str, ttl: u32) {
    response.headers_mut().append(
        header::SET_COOKIE,
        format!("{name}={value}; HttpOnly; SameSite=Lax; Path={path}; Max-Age={ttl}")
            .parse()
            .unwrap(),
    );
}
const CALLBACK: &str = "/__miy/login/callback";

async fn login(s: &Preview) -> ApiResult<Response> {
    let flow = Flow {
        state: auth::random(),
        verifier: auth::random(),
    };
    let port = s
        .origin
        .rsplit_once(':')
        .and_then(|(_, v)| v.parse::<u16>().ok())
        .ok_or_else(ApiError::unauthorized)?;
    let value=app_cli::api(&s.session,&format!("/api/v1/preview/apps/{}/browser/start",s.manifest.id),Some(json!({"instance":s.release,"port":port,"state":flow.state,"challenge":challenge(&flow.verifier)}))).await.map_err(|_|ApiError::unavailable("preview_login_start_failed"))?;
    let location = value["authorizationUrl"]
        .as_str()
        .ok_or_else(|| ApiError::unavailable("invalid_preview_login_url"))?;
    let url = url::Url::parse(location)?;
    if !matches!(url.scheme(), "http" | "https") || !url.has_host() {
        return Err(ApiError::bad("invalid_preview_login_url"));
    }
    let mut response = redirect(location);
    let name = format!("{}_{}", s.cookie_name, flow.state);
    let key = s.sessions.pending.insert(flow, 300)?;
    cookie(&mut response, &name, &key, CALLBACK, 300);
    Ok(response)
}

async fn callback_parts(
    s: &Preview,
    uri: &axum::http::Uri,
    headers: &HeaderMap,
) -> ApiResult<Response> {
    let query = uri.query().unwrap_or_default();
    let fields: HashMap<_, _> = url::form_urlencoded::parse(query.as_bytes()).collect();
    let state = fields
        .get("state")
        .filter(|v| valid_nonce(v))
        .ok_or_else(ApiError::unauthorized)?;
    let code = fields
        .get("code")
        .filter(|v| valid_nonce(v))
        .ok_or_else(ApiError::unauthorized)?;
    let name = format!("{}_{}", s.cookie_name, state);
    let key = auth::cookie(headers, &name).ok_or_else(ApiError::unauthorized)?;
    let flow = s
        .sessions
        .pending
        .get(&key)
        .filter(|f| f.state == *state)
        .ok_or_else(ApiError::unauthorized)?;
    s.sessions.pending.remove(&key);
    let value = app_cli::api(
        &s.session,
        &format!("/api/v1/preview/apps/{}/browser/exchange", s.manifest.id),
        Some(json!({"instance":s.release,"code":code,"verifier":flow.verifier})),
    )
    .await
    .map_err(|_| ApiError::unauthorized())?;
    let grant = value["grant"]
        .as_str()
        .filter(|v| v.strip_prefix("miy_preview_").is_some_and(valid_nonce))
        .ok_or_else(ApiError::unauthorized)?;
    let browser = Browser {
        grant: grant.into(),
        csrf: auth::random(),
    };
    browser.user(s).await?;
    let key = s.sessions.browsers.insert(browser, 28800)?;
    let mut response = redirect(&format!("/apps/{}", s.manifest.id));
    cookie(&mut response, &name, "", CALLBACK, 0);
    cookie(&mut response, &s.cookie_name, &key, "/", 28800);
    Ok(response)
}

fn navigation(headers: &HeaderMap) -> bool {
    headers
        .get("sec-fetch-mode")
        .is_none_or(|v| v == "navigate")
}

pub(super) async fn handle(s: Arc<Preview>, mut req: Request, next: Next) -> ApiResult<Response> {
    let headers = req.headers();
    if headers.get("host").and_then(|v| v.to_str().ok()) != s.origin.strip_prefix("http://") {
        return Err(ApiError::forbidden());
    }
    let path = req.uri().path().to_owned();
    if path == CALLBACK && req.method() == Method::GET && navigation(headers) {
        // Extract before the first await. Request bodies need not implement Sync.
        let (parts, _) = req.into_parts();
        return callback_parts(&s, &parts.uri, &parts.headers).await;
    }
    let page = req.method() == Method::GET
        && navigation(headers)
        && (path == "/" || path.starts_with("/apps/"));
    let key = auth::cookie(headers, &s.cookie_name);
    let Some(browser) = key.as_ref().and_then(|k| s.sessions.browsers.get(k)) else {
        return if page {
            login(&s).await
        } else {
            Err(ApiError::unauthorized())
        };
    };
    if !matches!(*req.method(), Method::GET | Method::HEAD)
        && (headers.get("origin").and_then(|v| v.to_str().ok()) != Some(s.origin.as_str())
            || headers.get("x-miy-csrf").and_then(|v| v.to_str().ok())
                != Some(browser.csrf.as_str()))
    {
        return Err(ApiError::forbidden());
    }
    // The two direct proxies revalidate this grant upstream. Every local asset
    // and business handler needs its own current browser identity.
    if path != "/__miy/platform/directory" && !path.starts_with("/__miy/connections/") {
        match browser.user(&s).await {
            Ok(user) => {
                req.extensions_mut().insert(VerifiedUser(user));
            }
            Err(e) if e.status == reqwest::StatusCode::UNAUTHORIZED && page => {
                if let Some(k) = key {
                    s.sessions.browsers.remove(&k);
                }
                return login(&s).await;
            }
            Err(e) => return Err(e),
        }
    }
    if req.uri().path() == "/" {
        return Ok(redirect(&format!("/apps/{}", s.manifest.id)));
    }
    req.extensions_mut().insert(browser);
    let mut response = next.run(req).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    Ok(response)
}
#[derive(Clone)]
pub(super) struct VerifiedUser(pub(super) Value);
