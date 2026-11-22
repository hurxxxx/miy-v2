//! Business app UIs in per-installation cross-origin frames (candidate design,
//! docs/APP-FRAMES.md). The frame listener never authenticates cookies. A one-time
//! launch code, issued to a portal session, opens a frame session. The frame then
//! presents an opaque token bound to its user, parent portal session, installation,
//! environment and release. Every request's Host must be that installation's
//! registered origin; the installation always comes from the credential, never from
//! the request path or body.
use crate::{
    State,
    api_contract::{
        AppFrameLaunch, AppFrameRenewal, AppFrameToken, FrameBootstrap, FrameLocation,
        FrameSettings,
    },
    api_error::{ApiError, ApiResult},
    auth::{BrowserPurpose, Principal},
    contract::hash,
    policy::{self, AppAction},
};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Path, Request, State as S},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use schemars::JsonSchema;
use serde::Deserialize;
use sqlx::Row;
use std::path::PathBuf;
use uuid::Uuid;

pub const TOKEN_PREFIX: &str = "miy_frame_";
pub const PROTOCOL: &str = "miy.frame/v1";
const CODE_SECONDS: i32 = 60;
/// Live frame sessions per person (docs/APP-FRAMES.md "보조 프레임").
const MAX_LIVE_FRAMES: i64 = 16;
const TOKEN_MINUTES: i32 = 10;
const LAUNCH_BODY: usize = 16 * 1024;

/// Ports browsers refuse to fetch (Fetch standard "bad port"); never allocated.
const BAD_PORTS: &[u16] = &[
    1, 7, 9, 11, 13, 15, 17, 19, 20, 21, 22, 23, 25, 37, 42, 43, 53, 69, 77, 79, 87, 95, 101, 102,
    103, 104, 109, 110, 111, 113, 115, 117, 119, 123, 135, 137, 139, 143, 161, 179, 389, 427, 465,
    512, 513, 514, 515, 526, 530, 531, 532, 540, 548, 554, 556, 563, 587, 601, 636, 989, 990, 993,
    995, 1719, 1720, 1723, 2049, 3659, 4045, 4190, 5060, 5061, 6000, 6566, 6665, 6666, 6667, 6668,
    6669, 6679, 6697, 10080,
];

/// How an installation's frame origin is formed.
#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    /// `https://v2-app-{key}.example.com`: the key is the installation id (32 hex),
    /// so a removed installation's origin is never formed again.
    Key { prefix: String, suffix: String },
    /// `https://192.0.2.10:{port}` with a port pool (deployments without a domain).
    Port {
        prefix: String,
        first: u16,
        last: u16,
    },
}

#[derive(Clone, Debug)]
pub struct Config {
    pub scheme: String,
    pub pattern: Pattern,
    /// Built frame runtime (`web/frame-runtime` with base `/_miy/runtime/`).
    pub runtime_dir: Option<PathBuf>,
    reserved: Vec<String>,
}

impl Config {
    /// `template` holds exactly one `{key}` (in the first host label) or `{port}`
    /// (the whole port, with `ports` as `first-last`). Every formed origin uses the
    /// portal's scheme and never equals a portal, dev portal or console origin.
    pub fn parse(
        template: &str,
        ports: Option<&str>,
        allow_http: bool,
        scheme: &str,
        reserved: &[&str],
    ) -> anyhow::Result<Self> {
        let keys = template.matches("{key}").count();
        let port_slots = template.matches("{port}").count();
        anyhow::ensure!(
            keys + port_slots == 1,
            "app_frame_origin_needs_one_key_or_port"
        );
        let pattern = if keys == 1 {
            let (prefix, suffix) = template.split_once("{key}").unwrap();
            let host = prefix.split_once("://").map(|(_, h)| h).unwrap_or("");
            anyhow::ensure!(
                !host.contains(['.', ':', '/', '@']) && suffix.starts_with('.'),
                "app_frame_key_must_be_in_the_first_host_label"
            );
            Pattern::Key {
                prefix: prefix.to_ascii_lowercase(),
                suffix: suffix.to_ascii_lowercase(),
            }
        } else {
            let prefix = template
                .strip_suffix("{port}")
                .filter(|p| p.ends_with(':'))
                .ok_or_else(|| anyhow::anyhow!("app_frame_port_must_be_the_origin_port"))?;
            let (first, last) = ports
                .and_then(|v| v.split_once('-'))
                .ok_or_else(|| anyhow::anyhow!("MIY_APP_FRAME_PORTS is required as first-last"))?;
            let (first, last): (u16, u16) = (first.trim().parse()?, last.trim().parse()?);
            anyhow::ensure!(first > 0 && first <= last, "invalid_app_frame_port_pool");
            Pattern::Port {
                prefix: prefix.to_ascii_lowercase(),
                first,
                last,
            }
        };
        let sample = match &pattern {
            Pattern::Key { prefix, suffix } => format!("{prefix}{}{suffix}", "0".repeat(32)),
            Pattern::Port { prefix, first, .. } => format!("{prefix}{first}"),
        };
        let normalized = crate::server_origin::validate(&sample, allow_http)?;
        anyhow::ensure!(
            normalized == sample,
            "app_frame_origin_must_be_a_plain_origin"
        );
        anyhow::ensure!(
            url::Url::parse(&sample)?.scheme() == scheme,
            "browser_origin_transports_must_match"
        );
        let config = Self {
            scheme: scheme.to_owned(),
            pattern,
            runtime_dir: std::env::var_os("MIY_FRAME_RUNTIME_DIR")
                .filter(|v| !v.is_empty())
                .map(Into::into),
            reserved: reserved.iter().map(|v| v.to_string()).collect(),
        };
        if let Pattern::Key { .. } = &config.pattern {
            anyhow::ensure!(
                reserved.iter().all(|origin| !config.formed(origin)),
                "app_frame_origins_must_not_include_platform_origins"
            );
        }
        Ok(config)
    }

    pub fn from_env(allow_http: bool, scheme: &str, reserved: &[&str]) -> anyhow::Result<Self> {
        let template = std::env::var("MIY_APP_FRAME_ORIGIN").map_err(|_| {
            anyhow::anyhow!("MIY_APP_FRAME_ORIGIN is required for business app frames")
        })?;
        Self::parse(
            &template,
            std::env::var("MIY_APP_FRAME_PORTS").ok().as_deref(),
            allow_http,
            scheme,
            reserved,
        )
    }

    /// Whether `origin` has the shape of a frame origin (the database decides
    /// whether it belongs to an installation).
    fn formed(&self, origin: &str) -> bool {
        match &self.pattern {
            Pattern::Key { prefix, suffix } => origin
                .strip_prefix(prefix.as_str())
                .and_then(|rest| rest.strip_suffix(suffix.as_str()))
                .is_some_and(|key| key.len() == 32 && key.bytes().all(|b| b.is_ascii_hexdigit())),
            Pattern::Port { prefix, .. } => origin
                .strip_prefix(prefix.as_str())
                .and_then(|port| port.parse::<u16>().ok())
                .is_some(),
        }
    }

    /// The normalized origin a request was sent to, from its Host header.
    pub fn request_origin(&self, headers: &HeaderMap) -> Option<String> {
        let mut hosts = headers.get_all(header::HOST).iter();
        let host = hosts.next()?.to_str().ok()?;
        if hosts.next().is_some() || host.is_empty() || host.contains(['/', '@', '\\', '?', '#']) {
            return None;
        }
        let origin = crate::cli_client::origin(&format!("{}://{host}", self.scheme)).ok()?;
        self.formed(&origin).then_some(origin)
    }

    /// The portal origin that may embed a frame of `environment`.
    pub fn portal_origin<'a>(auth: &'a crate::auth::Auth, environment: &str) -> &'a str {
        if environment == "dev" {
            &auth.dev_portal_origin
        } else {
            &auth.origin
        }
    }
}

/// The installation's frame origin, allocated once and never reassigned.
async fn ensure_origin(
    tx: &mut sqlx::PgConnection,
    config: &Config,
    installation: Uuid,
) -> ApiResult<String> {
    let current: Option<String> =
        sqlx::query_scalar("SELECT origin FROM app_frame_origins WHERE installation_id=$1")
            .bind(installation)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(origin) = current {
        return Ok(origin);
    }
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('app-frame-origins',0))")
        .execute(&mut *tx)
        .await?;
    let current: Option<String> =
        sqlx::query_scalar("SELECT origin FROM app_frame_origins WHERE installation_id=$1")
            .bind(installation)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(origin) = current {
        return Ok(origin);
    }
    let (origin, port) = match &config.pattern {
        Pattern::Key { prefix, suffix } => {
            (format!("{prefix}{}{suffix}", installation.simple()), None)
        }
        Pattern::Port {
            prefix,
            first,
            last,
        } => {
            let used: std::collections::HashSet<i32> =
                sqlx::query_scalar("SELECT port FROM app_frame_origins WHERE port IS NOT NULL")
                    .fetch_all(&mut *tx)
                    .await?
                    .into_iter()
                    .collect();
            let port = (*first..=*last)
                .filter(|port| !BAD_PORTS.contains(port) && !used.contains(&i32::from(*port)))
                .find(|port| !config.reserved.contains(&format!("{prefix}{port}")))
                .ok_or_else(|| ApiError::unavailable("app_frame_origin_pool_exhausted"))?;
            (format!("{prefix}{port}"), Some(i32::from(port)))
        }
    };
    let inserted = sqlx::query(
        "INSERT INTO app_frame_origins(origin,installation_id,port) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
    )
    .bind(&origin)
    .bind(installation)
    .bind(port)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    // An existing row with this origin belonged to another (or a removed)
    // installation. Never hand it out again.
    if inserted != 1 {
        return Err(ApiError::conflict("app_frame_origin_taken"));
    }
    Ok(origin)
}

/// A Docker app opens only while it runs. A UI-only validation fixture (a release
/// without an image) has no runtime and opens like before; its API calls still
/// require a running runtime in `runtime::proxy`.
fn running_or_fixture(inst: &sqlx::postgres::PgRow) -> bool {
    inst.get::<bool, _>("fixture")
        || (inst.get::<String, _>("runtime_status") == "running"
            && inst.get::<String, _>("desired_state") == "running")
}

/// The parent browser session of a portal request, as `session_guard` recorded it.
struct Parent {
    session: Option<String>,
    development: Option<String>,
}
fn parent(origin: Option<&crate::app_calls::Origin>) -> ApiResult<Parent> {
    match origin {
        Some(o) if o.kind == "browser" => Ok(Parent {
            session: Some(o.digest.clone()),
            development: None,
        }),
        Some(o) if o.kind == "development" => Ok(Parent {
            session: None,
            development: Some(o.digest.clone()),
        }),
        _ => Err(ApiError::forbidden()),
    }
}

/// Lock the user and live parent before touching rows that cascade from the parent.
async fn lock_parent(
    tx: &mut sqlx::PgConnection,
    user: &str,
    purpose: &str,
    parent: &Parent,
    allow_development: bool,
) -> ApiResult<DateTime<Utc>> {
    let enabled: Option<bool> =
        sqlx::query_scalar("SELECT enabled FROM users WHERE id=$1 FOR SHARE")
            .bind(user)
            .fetch_optional(&mut *tx)
            .await?;
    if enabled != Some(true) {
        return Err(ApiError::unauthorized());
    }
    let deadline: Option<DateTime<Utc>> = sqlx::query_scalar(concat!(
        "SELECT COALESCE(\
            (SELECT s.created_at+interval '8 hours' FROM sessions s \
             WHERE s.id_hash=$2 AND s.user_id=$1 AND s.browser_purpose=$4 \
             AND NOT EXISTS(SELECT 1 FROM password_resets r WHERE r.user_id=s.user_id) AND ",
        crate::user_switch::issuer_current_sql!(),
        " FOR KEY SHARE OF s), \
            CASE WHEN $5 THEN (SELECT d.expires_at FROM development_sessions d \
             WHERE d.id_hash=$3 AND d.user_id=$1 AND d.browser_purpose=$4 FOR KEY SHARE OF d) END)",
    ))
    .bind(user)
    .bind(&parent.session)
    .bind(&parent.development)
    .bind(purpose)
    .bind(allow_development)
    .fetch_one(&mut *tx)
    .await?;
    deadline
        .filter(|deadline| *deadline > Utc::now())
        .ok_or_else(ApiError::unauthorized)
}

/// A portal session (not a bearer) of the portal for the installation's environment.
fn portal_purpose(p: &Principal) -> ApiResult<BrowserPurpose> {
    match p.browser_purpose {
        Some(purpose) if purpose.portal() && p.csrf.is_some() => Ok(purpose),
        _ => Err(ApiError::forbidden()),
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchRequest {
    pub installation_id: Uuid,
}

/// Worker bookkeeping: expired frame credentials have no further use (codes live
/// 60 seconds, frames at most 8 hours). Removing a frame session also removes
/// its renew codes; continuations of a removed frame are refused like expired ones.
pub async fn prune(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM app_frame_codes WHERE expires_at < now() - interval '1 hour'")
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM app_frame_sessions WHERE expires_at < now() - interval '1 hour'")
        .execute(pool)
        .await?;
    Ok(())
}

/// `POST /api/v1/app-frames`: a one-time code that opens one frame of an
/// installation the portal session may use now.
pub async fn launch_code(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    origin: Option<axum::Extension<crate::app_calls::Origin>>,
    headers: HeaderMap,
    Json(input): Json<LaunchRequest>,
) -> ApiResult<Json<AppFrameLaunch>> {
    let purpose = portal_purpose(&p)?;
    let parent = parent(origin.as_ref().map(|o| &o.0))?;
    // session_guard already required this exact portal Origin for the write.
    let portal_origin = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(ApiError::forbidden)?
        .to_owned();
    let mut tx = policy::Write::begin(&s.pool).await?;
    lock_parent(
        &mut tx,
        &p.id,
        purpose.as_str(),
        &parent,
        s.auth.development_login,
    )
    .await?;
    let inst = sqlx::query(
        "SELECT i.app_id,i.environment,i.release_id,i.runtime_status,i.desired_state,r.image_id IS NULL AS fixture FROM installations i JOIN releases r ON r.id=i.release_id WHERE i.id=$1 FOR SHARE OF i",
    )
    .bind(input.installation_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(ApiError::not_found)?;
    let (app, env): (String, String) = (inst.get("app_id"), inst.get("environment"));
    if purpose.environment() != Some(env.as_str()) {
        return Err(ApiError::forbidden());
    }
    if !running_or_fixture(&inst) {
        return Err(ApiError::unavailable("app_runtime_not_running"));
    }
    let frame_origin = ensure_origin(&mut tx, &s.auth.frames, input.installation_id).await?;
    let code = crate::auth::random();
    // The frame's id is fixed now, so the portal renews its own launch later.
    let frame_id = Uuid::new_v4();
    let release: Uuid = inst.get("release_id");
    // One clock reading for created_at and expires_at keeps the lifetime check exact.
    let expires_at: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO app_frame_codes(code_hash,purpose,frame_id,user_id,parent_session_hash,parent_development_hash,browser_purpose,installation_id,release_id,environment,portal_origin,created_at,expires_at) SELECT $1,'launch',$11,$2,$3,$4,$5,$6,$7,$8,$9,n.t,n.t+make_interval(secs=>$10) FROM (SELECT clock_timestamp() AS t) n RETURNING expires_at",
    )
    .bind(hash(&code))
    .bind(&p.id)
    .bind(&parent.session)
    .bind(&parent.development)
    .bind(purpose.as_str())
    .bind(input.installation_id)
    .bind(release)
    .bind(&env)
    .bind(&portal_origin)
    .bind(CODE_SECONDS)
    .bind(frame_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.decide(&p, &app, &env, AppAction::Use)
        .await?
        .commit()
        .await?;
    Ok(Json(AppFrameLaunch {
        frame_id,
        installation_id: input.installation_id,
        release_id: release,
        launch_url: format!("{frame_origin}/_miy/launch"),
        frame_origin,
        code,
        expires_at,
    }))
}

/// `POST /api/v1/app-frames/{id}/renew`: a one-time code that renews this portal
/// session's own frame, keeping its installation, environment and release.
pub async fn renew_code(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    origin: Option<axum::Extension<crate::app_calls::Origin>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<AppFrameRenewal>> {
    let purpose = portal_purpose(&p)?;
    let parent = parent(origin.as_ref().map(|o| &o.0))?;
    let portal_origin = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(ApiError::forbidden)?
        .to_owned();
    let mut tx = policy::Write::begin(&s.pool).await?;
    lock_parent(
        &mut tx,
        &p.id,
        purpose.as_str(),
        &parent,
        s.auth.development_login,
    )
    .await?;
    let frame = sqlx::query(
        "SELECT f.installation_id,f.release_id,f.environment,i.app_id,i.release_id AS current_release FROM app_frame_sessions f JOIN installations i ON i.id=f.installation_id WHERE f.id=$1 AND f.user_id=$2 AND f.browser_purpose=$3 AND f.parent_session_hash IS NOT DISTINCT FROM $4 AND f.parent_development_hash IS NOT DISTINCT FROM $5 AND f.revoked_at IS NULL AND f.expires_at>clock_timestamp() FOR SHARE OF f",
    )
    .bind(id)
    .bind(&p.id)
    .bind(purpose.as_str())
    .bind(&parent.session)
    .bind(&parent.development)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(ApiError::not_found)?;
    let release: Uuid = frame.get("release_id");
    if frame.get::<Uuid, _>("current_release") != release {
        return Err(ApiError::conflict("release_changed_reload_required"));
    }
    let (app, env): (String, String) = (frame.get("app_id"), frame.get("environment"));
    let code = crate::auth::random();
    let expires_at: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO app_frame_codes(code_hash,purpose,frame_id,user_id,parent_session_hash,parent_development_hash,browser_purpose,frame_session_id,installation_id,release_id,environment,portal_origin,created_at,expires_at) SELECT $1,'renew',$6,$2,$3,$4,$5,$6,$7,$8,$9,$10,n.t,n.t+make_interval(secs=>$11) FROM (SELECT clock_timestamp() AS t) n RETURNING expires_at",
    )
    .bind(hash(&code))
    .bind(&p.id)
    .bind(&parent.session)
    .bind(&parent.development)
    .bind(purpose.as_str())
    .bind(id)
    .bind(frame.get::<Uuid, _>("installation_id"))
    .bind(release)
    .bind(&env)
    .bind(&portal_origin)
    .bind(CODE_SECONDS)
    .fetch_one(&mut *tx)
    .await?;
    tx.decide(&p, &app, &env, AppAction::Use)
        .await?
        .commit()
        .await?;
    Ok(Json(AppFrameRenewal { code, expires_at }))
}

async fn lock_frame_slots(tx: &mut sqlx::PgConnection, user_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('app-frame-slots:' || $1,0))")
        .bind(user_id)
        .execute(tx)
        .await?;
    Ok(())
}

/// Release a frame slot and cancel a pending launch owned by this portal session.
pub async fn close(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    origin: Option<axum::Extension<crate::app_calls::Origin>>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let purpose = portal_purpose(&p)?;
    let parent = parent(origin.as_ref().map(|o| &o.0))?;
    let mut tx = s.pool.begin().await?;
    lock_frame_slots(&mut tx, &p.id).await?;
    sqlx::query(
        "DELETE FROM app_frame_codes WHERE frame_id=$1 AND user_id=$2 AND browser_purpose=$3 \
         AND purpose='launch' AND parent_session_hash IS NOT DISTINCT FROM $4 \
         AND parent_development_hash IS NOT DISTINCT FROM $5",
    )
    .bind(id)
    .bind(&p.id)
    .bind(purpose.as_str())
    .bind(&parent.session)
    .bind(&parent.development)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE app_frame_sessions SET revoked_at=clock_timestamp() \
         WHERE id=$1 AND user_id=$2 AND browser_purpose=$3 AND revoked_at IS NULL \
         AND parent_session_hash IS NOT DISTINCT FROM $4 \
         AND parent_development_hash IS NOT DISTINCT FROM $5",
    )
    .bind(id)
    .bind(&p.id)
    .bind(purpose.as_str())
    .bind(&parent.session)
    .bind(&parent.development)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The frame listener. No cookies, no shared browser routes, no anti-frame
/// default: frame documents allow exactly their portal as ancestor.
pub fn router(state: State) -> Router {
    Router::new()
        .route("/_miy/launch", axum::routing::get(launch).post(launch))
        .route("/_miy/renew", axum::routing::post(renew))
        .route("/_miy/relaunch.js", axum::routing::get(relaunch_script))
        .route("/_miy/runtime/{*path}", axum::routing::get(runtime_file))
        .route("/_miy/a/{capability}/ui/{*path}", axum::routing::get(asset))
        .route("/_miy/api/{*path}", axum::routing::any(api))
        .route(
            "/_miy/files/{*path}",
            axum::routing::any(files).layer(axum::extract::DefaultBodyLimit::disable()),
        )
        .route(
            "/_miy/connections/{target}/api/{*path}",
            axum::routing::any(connection),
        )
        .route("/_miy/platform/directory", axum::routing::get(directory))
        .route("/_miy/live", axum::routing::get(live))
        .fallback(|| async { ApiError::not_found() })
        .layer(middleware::from_fn(frame_headers))
        .with_state(state)
}

/// Set by the frame listener on 401 responses that Core produced before any
/// app saw the request (web/frame-runtime resends only those).
pub const CREDENTIAL_REFUSED: &str = "x-miy-frame-credential";

async fn frame_headers(request: Request, next: Next) -> Response {
    // Frames never register service workers (no worker exception in the PoC).
    let mut response = if request.headers().contains_key("service-worker") {
        ApiError::forbidden().into_response()
    } else {
        next.run(request).await
    };
    // A 401 that Core produced means the request never reached the app, so the
    // frame runtime may renew and resend it whatever the method. An app's own
    // 401 carries no mark: app headers never pass the gateway.
    let refused = response.status() == StatusCode::UNAUTHORIZED
        && response
            .extensions()
            .get::<crate::app_response::FromApp>()
            .is_none();
    let headers = response.headers_mut();
    headers.remove(CREDENTIAL_REFUSED);
    if refused {
        headers.insert(CREDENTIAL_REFUSED, HeaderValue::from_static("rejected"));
    }
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers
        .entry(header::REFERRER_POLICY)
        .or_insert(HeaderValue::from_static("no-referrer"));
    headers
        .entry(header::CONTENT_SECURITY_POLICY)
        .or_insert(HeaderValue::from_static(
            "default-src 'none'; frame-ancestors 'none'; base-uri 'none'",
        ));
    headers
        .entry(header::CACHE_CONTROL)
        .or_insert(HeaderValue::from_static("no-store"));
    response
}

/// The registered installation behind the request's Host.
struct Site {
    origin: String,
    installation_id: Uuid,
    environment: String,
}
async fn site(s: &State, headers: &HeaderMap) -> ApiResult<Site> {
    let origin = s
        .auth
        .frames
        .request_origin(headers)
        .ok_or_else(ApiError::not_found)?;
    let row = sqlx::query(
        "SELECT o.installation_id,i.environment FROM app_frame_origins o JOIN installations i ON i.id=o.installation_id WHERE o.origin=$1",
    )
    .bind(&origin)
    .fetch_optional(&s.pool)
    .await?
    .ok_or_else(ApiError::not_found)?;
    Ok(Site {
        origin,
        installation_id: row.get("installation_id"),
        environment: row.get("environment"),
    })
}

/// Browser request rules of the frame listener (APP-FRAMES §3.2): a write carries
/// exactly this origin; a read is same-origin by Fetch Metadata and its Origin, if
/// any, is this origin. `null`, malformed and duplicate Origins are refused.
pub fn browser_request_allowed(method: &Method, headers: &HeaderMap, own: &str) -> bool {
    let mut origins = headers.get_all(header::ORIGIN).iter();
    let origin = origins.next().map(|v| v.to_str().unwrap_or("\u{0}"));
    if origins.next().is_some() || origin.is_some_and(|o| o != own) {
        return false;
    }
    if matches!(*method, Method::GET | Method::HEAD) {
        let mut sites = headers.get_all("sec-fetch-site").iter();
        sites.next().and_then(|v| v.to_str().ok()) == Some("same-origin") && sites.next().is_none()
    } else {
        origin == Some(own)
    }
}

fn frame_token_shape(value: &str) -> bool {
    value.strip_prefix(TOKEN_PREFIX).is_some_and(|secret| {
        secret.len() == 43
            && secret
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    })
}

/// A frame's live connection (crate::live): a WebSocket on the frame's own
/// origin. The frame token comes in the first message, never in the address.
async fn live(
    S(s): S<State>,
    headers: HeaderMap,
    upgrade: axum::extract::ws::WebSocketUpgrade,
) -> Response {
    let site = match site(&s, &headers).await {
        Ok(site) => site,
        Err(error) => return error.into_response(),
    };
    let mut origins = headers.get_all(header::ORIGIN).iter();
    let origin = origins.next().and_then(|value| value.to_str().ok());
    if origin != Some(site.origin.as_str()) || origins.next().is_some() {
        return ApiError::forbidden().into_response();
    }
    upgrade
        .max_message_size(crate::live::MAX_MESSAGE_BYTES)
        .max_frame_size(crate::live::MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| live_session(s, site, socket))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LiveAuth {
    r#type: String,
    token: String,
}

async fn live_session(s: State, site: Site, mut socket: axum::extract::ws::WebSocket) {
    use crate::live::{close, close_with};
    use axum::extract::ws::Message;
    let token = match tokio::time::timeout(crate::live::AUTH_TIMEOUT, socket.recv()).await {
        Ok(Some(Ok(Message::Text(text)))) => serde_json::from_str::<LiveAuth>(text.as_str())
            .ok()
            .filter(|auth| auth.r#type == "auth" && frame_token_shape(&auth.token))
            .map(|auth| auth.token),
        _ => None,
    };
    let Some(token) = token else {
        close_with(&mut socket, close::BAD_AUTH, "auth_required").await;
        return;
    };
    let (frame, p) = match load_frame(&s, &site, "token_hash", &token, true).await {
        Ok(found) => found,
        Err(error) if error.status == StatusCode::CONFLICT => {
            close_with(&mut socket, close::RELEASE_CHANGED, "release_changed").await;
            return;
        }
        Err(_) => {
            close_with(&mut socket, close::SESSION_ENDED, "session_ended").await;
            return;
        }
    };
    if policy::require(&s, &p, &frame.app_id, &frame.environment, AppAction::Use)
        .await
        .is_err()
    {
        close_with(&mut socket, close::ACCESS_REVOKED, "access_revoked").await;
        return;
    }
    let Some(_slot) = crate::live::acquire(&p.id, frame.installation_id) else {
        close_with(&mut socket, close::BUDGET, "too_many_connections").await;
        return;
    };
    // The same signed identity a frame API request gets, for the app's /api/live.
    let target = async {
        let context = frame.context(&s).await?;
        let (mut parts, ()) = axum::http::Request::builder()
            .method(Method::GET)
            .uri("/_miy/live")
            .body(())
            .map_err(|_| ApiError::unavailable("live_request_failed"))?
            .into_parts();
        parts
            .extensions
            .insert(crate::app_calls::ForwardContext(context));
        crate::runtime::app_target(
            &s,
            &p,
            frame.installation_id,
            &parts,
            "live",
            "invalid_api_path",
        )
        .await
    };
    let (port, identity) = match target.await {
        Ok(found) => found,
        Err(error) if error.status == StatusCode::FORBIDDEN => {
            close_with(&mut socket, close::ACCESS_REVOKED, "access_revoked").await;
            return;
        }
        Err(_) => {
            close_with(&mut socket, close::APP_UNAVAILABLE, "app_unavailable").await;
            return;
        }
    };
    let upstream = async {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut request = format!("ws://127.0.0.1:{port}/api/live")
            .into_client_request()
            .ok()?;
        request
            .headers_mut()
            .insert("x-miy-identity", HeaderValue::from_str(&identity).ok()?);
        let config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(Some(crate::live::MAX_MESSAGE_BYTES))
            .max_frame_size(Some(crate::live::MAX_MESSAGE_BYTES));
        tokio::time::timeout(
            crate::live::AUTH_TIMEOUT,
            tokio_tungstenite::connect_async_with_config(request, Some(config), false),
        )
        .await
        .ok()?
        .ok()
        .map(|(stream, _)| stream)
    };
    let Some(upstream) = upstream.await else {
        close_with(&mut socket, close::APP_UNAVAILABLE, "app_unavailable").await;
        return;
    };
    // The frame runtime treats this first message as "open"; every later
    // message comes from the app.
    if !crate::live::send_with_deadline(
        &mut socket,
        Message::Text(r#"{"type":"miy.ready"}"#.into()),
    )
    .await
    {
        return;
    }
    let (frame_id, release) = (frame.id, frame.release_id);
    let recheck = || {
        let (s, p, frame_app, frame_env) = (
            s.clone(),
            p.clone(),
            frame.app_id.clone(),
            frame.environment.clone(),
        );
        async move {
            let row = sqlx::query(&format!(
                "SELECT i.release_id FROM app_frame_sessions f JOIN installations i ON i.id=f.installation_id \
                 WHERE f.id=$2 AND f.revoked_at IS NULL AND f.expires_at>clock_timestamp() AND {PARENT_LIVE}"
            ))
            .bind(s.auth.development_login)
            .bind(frame_id)
            .fetch_optional(&s.pool)
            .await
            .map_err(|_| crate::live::Ended::Session)?
            .ok_or(crate::live::Ended::Session)?;
            if row.get::<Uuid, _>("release_id") != release {
                return Err(crate::live::Ended::Release);
            }
            policy::require(&s, &p, &frame_app, &frame_env, AppAction::Use)
                .await
                .map_err(|_| crate::live::Ended::Access)
        }
    };
    crate::live::relay(socket, upstream, recheck).await;
}

/// A live frame session and the principal it acts as.
pub struct Frame {
    pub id: Uuid,
    pub installation_id: Uuid,
    pub release_id: Uuid,
    pub app_id: String,
    pub environment: String,
}
impl Frame {
    fn origin(&self) -> crate::app_calls::Origin {
        crate::app_calls::Origin {
            kind: "frame".into(),
            digest: self.id.to_string(),
        }
    }

    /// Preserve the authenticated release through body collection, connection
    /// resolution and the final authorization check. A later installation read
    /// must never turn an old frame into a caller of a newly deployed release.
    async fn context(&self, s: &State) -> ApiResult<crate::app_calls::Context> {
        Ok(crate::app_calls::Context {
            installation_id: self.installation_id,
            release_id: self.release_id,
            environment: self.environment.clone(),
            callers: vec![],
            permissions_used: vec![],
            effective_permissions: crate::permissions::effective(
                &s.pool,
                self.installation_id,
                self.release_id,
            )
            .await?,
            expires_at: Utc::now() + Duration::seconds(30),
            origin: Some(self.origin()),
        })
    }
}

/// Whether the frame's parent portal session is still signed in (logout and a
/// disabled user delete it; frame rows cascade). Identity provider revocation is
/// enforced when the portal renews the short frame token.
pub const PARENT_LIVE: &str = concat!(
    "EXISTS(SELECT 1 FROM users u WHERE u.id=f.user_id AND u.enabled) ",
    "AND (EXISTS(SELECT 1 FROM sessions s WHERE s.id_hash=f.parent_session_hash AND s.user_id=f.user_id ",
    "AND s.browser_purpose=f.browser_purpose AND s.created_at>clock_timestamp()-interval '8 hours' ",
    "AND NOT EXISTS(SELECT 1 FROM password_resets r WHERE r.user_id=s.user_id) AND ",
    crate::user_switch::issuer_current_sql!(),
    ") OR ($1::boolean AND EXISTS(SELECT 1 FROM development_sessions d WHERE d.id_hash=f.parent_development_hash ",
    "AND d.user_id=f.user_id AND d.browser_purpose=f.browser_purpose AND d.expires_at>clock_timestamp())))"
);

fn principal(user: String, purpose: &str, development: bool) -> ApiResult<Principal> {
    let browser_purpose = match purpose {
        "portal" => BrowserPurpose::Portal,
        "dev_portal" => BrowserPurpose::DevPortal,
        _ => return Err(ApiError::forbidden()),
    };
    Ok(Principal {
        browser_purpose: Some(browser_purpose),
        cli_session_id: None,
        delegation_id: None,
        id: user,
        scopes: vec!["miy.read".into()],
        csrf: None,
        development,
    })
}

async fn load_frame(
    s: &State,
    site: &Site,
    column: &str,
    secret: &str,
    token: bool,
) -> ApiResult<(Frame, Principal)> {
    let expiry = if token {
        "AND f.token_expires_at>clock_timestamp()"
    } else {
        ""
    };
    let row = sqlx::query(&format!(
        "SELECT f.id,f.user_id,f.browser_purpose,f.installation_id,f.release_id,f.environment,f.origin,f.parent_development_hash IS NOT NULL AS development,i.app_id,i.release_id AS current_release FROM app_frame_sessions f JOIN installations i ON i.id=f.installation_id WHERE f.{column}=$2 AND f.revoked_at IS NULL AND f.expires_at>clock_timestamp() {expiry} AND {PARENT_LIVE}"
    ))
    .bind(s.auth.development_login)
    .bind(hash(secret))
    .fetch_optional(&s.pool)
    .await?
    .ok_or_else(ApiError::unauthorized)?;
    if row.get::<Uuid, _>("installation_id") != site.installation_id
        || row.get::<String, _>("origin") != site.origin
    {
        return Err(ApiError::forbidden());
    }
    let release: Uuid = row.get("release_id");
    if row.get::<Uuid, _>("current_release") != release {
        return Err(ApiError::conflict("release_changed_reload_required"));
    }
    let p = principal(
        row.get("user_id"),
        &row.get::<String, _>("browser_purpose"),
        row.get("development"),
    )?;
    crate::auth_sessions::ensure_active(&s.pool, &p.id).await?;
    Ok((
        Frame {
            id: row.get("id"),
            installation_id: site.installation_id,
            release_id: release,
            app_id: row.get("app_id"),
            environment: row.get("environment"),
        },
        p,
    ))
}

/// Authenticate a frame API request: Host, browser headers and the bearer token.
async fn authenticate(
    s: &State,
    method: &Method,
    headers: &HeaderMap,
) -> ApiResult<(Frame, Principal, crate::app_calls::Context)> {
    let site = site(s, headers).await?;
    if !browser_request_allowed(method, headers, &site.origin) {
        return Err(ApiError::forbidden());
    }
    let mut tokens = headers.get_all(header::AUTHORIZATION).iter();
    let token = tokens
        .next()
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| {
            v.strip_prefix(TOKEN_PREFIX).is_some_and(|secret| {
                secret.len() == 43
                    && secret
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
            })
        })
        .ok_or_else(ApiError::unauthorized)?;
    if tokens.next().is_some() {
        return Err(ApiError::unauthorized());
    }
    let (frame, p) = load_frame(s, &site, "token_hash", token, true).await?;
    let context = frame.context(s).await?;
    Ok((frame, p, context))
}

async fn audit(s: &State, actor: &str, method: &Method, status: StatusCode, target: String) {
    let id = Uuid::new_v4().to_string();
    tracing::info!(event="frame_request",request_id=%id,actor=%actor,method=%method,path=%target,status=status.as_u16());
    if (*method != Method::GET || !status.is_success() || !target.contains("/api/"))
        && sqlx::query("INSERT INTO audit(actor,action,target,request_id) VALUES($1,$2,$3,$4)")
            .bind(actor)
            .bind(format!("{method}:{}", status.as_u16()))
            .bind(target)
            .bind(id)
            .execute(&s.pool)
            .await
            .is_err()
    {
        tracing::error!(event = "audit_write_failed");
    }
}

async fn files(S(s): S<State>, Path(path): Path<String>, mut request: Request) -> Response {
    let (method, headers) = (request.method().clone(), request.headers().clone());
    let (frame, p, context) = match authenticate(&s, &method, &headers).await {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    request
        .extensions_mut()
        .insert(crate::app_calls::ForwardContext(context));
    let target = format!(
        "frame:/installations/{}/files/{path}",
        frame.installation_id
    );
    let response = crate::file_gateway::proxy(
        S(s.clone()),
        axum::Extension(p.clone()),
        Path((frame.installation_id, path)),
        request,
    )
    .await
    .into_response();
    audit(&s, &p.id, &method, response.status(), target).await;
    response
}

pub(crate) async fn api(
    S(s): S<State>,
    Path(path): Path<String>,
    mut request: Request,
) -> Response {
    let (method, headers) = (request.method().clone(), request.headers().clone());
    let (frame, p, context) = match authenticate(&s, &method, &headers).await {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    request
        .extensions_mut()
        .insert(crate::app_calls::ForwardContext(context));
    let target = format!("frame:/installations/{}/api/{path}", frame.installation_id);
    let response = crate::runtime::proxy(
        S(s.clone()),
        axum::Extension(p.clone()),
        Path((frame.installation_id, path)),
        request,
    )
    .await
    .into_response();
    audit(&s, &p.id, &method, response.status(), target).await;
    response
}

async fn connection(
    S(s): S<State>,
    Path((target, path)): Path<(String, String)>,
    mut request: Request,
) -> Response {
    let (method, headers) = (request.method().clone(), request.headers().clone());
    let (frame, p, context) = match authenticate(&s, &method, &headers).await {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    request.extensions_mut().insert(context);
    let audit_target = format!(
        "frame:/installations/{}/connections/{target}/api/{path}",
        frame.installation_id
    );
    let response = crate::connections::connect(
        S(s.clone()),
        axum::Extension(p.clone()),
        Path((frame.installation_id, target, path)),
        request,
    )
    .await
    .into_response();
    audit(&s, &p.id, &method, response.status(), audit_target).await;
    response
}

async fn directory(S(s): S<State>, mut request: Request) -> Response {
    let (method, headers) = (request.method().clone(), request.headers().clone());
    let (frame, p, context) = match authenticate(&s, &method, &headers).await {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    request.extensions_mut().insert(context);
    let target = format!(
        "frame:/installations/{}/platform/directory",
        frame.installation_id
    );
    let response = crate::connections::directory(
        S(s.clone()),
        axum::Extension(p.clone()),
        Path(frame.installation_id),
        request,
    )
    .await
    .into_response();
    audit(&s, &p.id, &Method::GET, response.status(), target).await;
    response
}

/// `/_miy/a/{capability}/ui/{path}`: the frame's release artifact. The capability
/// is the frame's fixed asset value; current use and release are checked on every
/// request, including conditional ones.
async fn asset(
    S(s): S<State>,
    Path((capability, path)): Path<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let site = site(&s, &headers).await?;
    if !browser_request_allowed(&Method::GET, &headers, &site.origin) || capability.len() != 43 {
        return Err(ApiError::not_found());
    }
    let (frame, p) = load_frame(&s, &site, "asset_hash", &capability, false)
        .await
        .map_err(|_| ApiError::not_found())?;
    policy::require(&s, &p, &frame.app_id, &frame.environment, AppAction::Use).await?;
    let artifact: String = sqlx::query_scalar("SELECT artifact_hash FROM releases WHERE id=$1")
        .bind(frame.release_id)
        .fetch_one(&s.pool)
        .await?;
    let etag = format!("\"{}\"", &hash(format!("{artifact}/{path}"))[..32]);
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(etag.as_str())
    {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [
                (header::ETAG, etag.as_str()),
                (header::CACHE_CONTROL, "private, no-cache"),
            ],
        )
            .into_response());
    }
    let (mime, bytes) = crate::http::artifact_file(&s, &artifact, &path).await?;
    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "private, no-cache"),
            (header::ETAG, etag.as_str()),
        ],
        bytes,
    )
        .into_response())
}

/// The platform frame runtime: shared static code without user or app data.
async fn runtime_file(
    S(s): S<State>,
    Path(path): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let site = site(&s, &headers).await?;
    if !browser_request_allowed(&Method::GET, &headers, &site.origin)
        || !crate::contract::safe_relative(&path)
        || path == "index.html"
    {
        return Err(ApiError::not_found());
    }
    let dir = s
        .auth
        .frames
        .runtime_dir
        .as_ref()
        .ok_or_else(ApiError::not_found)?;
    let file = dir.join(&path);
    let bytes = tokio::fs::read(&file)
        .await
        .map_err(|_| ApiError::not_found())?;
    Ok((
        [
            (header::CONTENT_TYPE, crate::http::mime(&file)),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        bytes,
    )
        .into_response())
}

const RELAUNCH_SCRIPT: &str = "const parent = document.querySelector('meta[name=\"miy-parent-origin\"]')?.content;\nif (parent && window.parent !== window) {\n  window.parent.postMessage({ protocol: \"miy.frame/v1\", type: \"frame.relaunch\", id: String(Date.now()), payload: {} }, parent);\n}\n";

async fn relaunch_script(S(s): S<State>, headers: HeaderMap) -> ApiResult<Response> {
    let site = site(&s, &headers).await?;
    if !browser_request_allowed(&Method::GET, &headers, &site.origin) {
        return Err(ApiError::not_found());
    }
    Ok((
        [
            (header::CONTENT_TYPE, "text/javascript"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        RELAUNCH_SCRIPT,
    )
        .into_response())
}

/// The answer to a reloaded, replayed or refused launch: no credential, no app
/// data. Its script asks the portal to start a fresh frame.
fn relaunch(s: &State, environment: &str) -> Response {
    let portal = Config::portal_origin(&s.auth, environment);
    (
        StatusCode::GONE,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8".to_owned()),
            (
                header::CONTENT_SECURITY_POLICY,
                format!("default-src 'none'; script-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors {portal}"),
            ),
        ],
        format!(
            "<!doctype html><html lang=\"ko\"><head><meta charset=\"utf-8\"><meta name=\"miy-parent-origin\" content=\"{portal}\"><script src=\"/_miy/relaunch.js\"></script></head><body></body></html>"
        ),
    )
        .into_response()
}

/// Launch form fields (urlencoded): `code` and the app location to open.
struct LaunchForm {
    code: Option<String>,
    location: FrameLocation,
    base_path: String,
    settings: FrameSettings,
}
/// The first SDK bundle whose apps read display settings.
const SETTINGS_BUNDLE: (u32, u32, u32) = (0, 12, 0);
fn settings_aware(bundle: Option<&str>) -> bool {
    let mut parts = bundle.unwrap_or_default().split('.').map(str::parse::<u32>);
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(Ok(major)), Some(Ok(minor)), Some(Ok(patch)), None) => {
            (major, minor, patch) >= SETTINGS_BUNDLE
        }
        _ => false,
    }
}
/// An IANA-shaped zone name; whether the zone exists is the reader's concern.
pub(crate) fn time_zone_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.starts_with(|c: char| c.is_ascii_alphabetic())
        && value.split('/').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-'))
        })
}
fn launch_form(body: &[u8]) -> LaunchForm {
    let mut form = LaunchForm {
        code: None,
        location: FrameLocation {
            pathname: "/".into(),
            search: String::new(),
            hash: String::new(),
        },
        base_path: "/".into(),
        settings: FrameSettings::default(),
    };
    // A display setting the portal sent; anything unknown keeps the default.
    fn choice<T: serde::de::DeserializeOwned>(value: &str) -> Option<T> {
        serde_json::from_value(serde_json::Value::String(value.to_owned())).ok()
    }
    // Empty, or starting with `first`; bounded; no control characters.
    let clean = |v: &str, first: char, max: usize| {
        (v.len() <= max
            && !v.chars().any(char::is_control)
            && (v.is_empty() || v.starts_with(first)))
        .then(|| v.to_owned())
    };
    for (key, value) in url::form_urlencoded::parse(body) {
        match key.as_ref() {
            "code" if value.len() == 43 => form.code = Some(value.into_owned()),
            "pathname" => {
                if let Some(v) = clean(&value, '/', 2048).filter(|v| !v.is_empty()) {
                    form.location.pathname = v;
                }
            }
            "search" => form.location.search = clean(&value, '?', 2048).unwrap_or_default(),
            "hash" => form.location.hash = clean(&value, '#', 2048).unwrap_or_default(),
            "basePath" => {
                if let Some(v) = clean(&value, '/', 512).filter(|v| !v.is_empty()) {
                    form.base_path = v;
                }
            }
            "theme" => {
                if let Some(v) = choice(&value) {
                    form.settings.theme = v;
                }
            }
            "locale" => {
                if let Some(v) = choice(&value) {
                    form.settings.locale = v;
                }
            }
            "timeZone" if time_zone_name(&value) => form.settings.time_zone = value.into_owned(),
            "dateFormat" => {
                if let Some(v) = choice(&value) {
                    form.settings.date_format = v;
                }
            }
            _ => {}
        }
    }
    form
}

/// JSON for a `<script type="application/json">` element: no markup can end it.
pub fn script_json(value: &impl serde::Serialize) -> serde_json::Result<String> {
    Ok(serde_json::to_string(value)?
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029"))
}

/// `POST /_miy/launch` (and the reload GET): consume the launch code once and
/// answer the frame document, or the relaunch document.
async fn launch(S(s): S<State>, method: Method, headers: HeaderMap, request: Request) -> Response {
    let Ok(site) = site(&s, &headers).await else {
        return ApiError::not_found().into_response();
    };
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    if method != Method::POST {
        return relaunch(&s, &site.environment);
    }
    let Ok(body) = to_bytes(request.into_body(), LAUNCH_BODY).await else {
        return relaunch(&s, &site.environment);
    };
    let form = launch_form(&body);
    match open_frame(&s, &site, origin.as_deref(), &form).await {
        Ok(response) => response,
        Err(error) => {
            tracing::info!(
                event = "frame_launch_refused",
                status = error.status.as_u16()
            );
            relaunch(&s, &site.environment)
        }
    }
}

async fn open_frame(
    s: &State,
    site: &Site,
    origin: Option<&str>,
    form: &LaunchForm,
) -> ApiResult<Response> {
    let code = form.code.as_deref().ok_or_else(ApiError::unauthorized)?;
    let origin = origin
        .filter(|o| *o != "null")
        .ok_or_else(ApiError::forbidden)?;
    // Consume first, in its own statement: a refused attempt still uses the code up.
    let row = sqlx::query(
        "UPDATE app_frame_codes SET used_at=clock_timestamp() WHERE code_hash=$1 AND purpose='launch' AND used_at IS NULL AND expires_at>clock_timestamp() RETURNING frame_id,user_id,parent_session_hash,parent_development_hash,browser_purpose,installation_id,release_id,environment,portal_origin",
    )
    .bind(hash(code))
    .fetch_optional(&s.pool)
    .await?
    .ok_or_else(ApiError::unauthorized)?;
    if row.get::<Uuid, _>("installation_id") != site.installation_id
        || row.get::<String, _>("portal_origin") != origin
    {
        return Err(ApiError::forbidden());
    }
    let user: String = row.get("user_id");
    let purpose: String = row.get("browser_purpose");
    let parent = Parent {
        session: row.get("parent_session_hash"),
        development: row.get("parent_development_hash"),
    };
    let release: Uuid = row.get("release_id");
    let p = principal(user.clone(), &purpose, parent.development.is_some())?;
    let mut tx = policy::Write::begin(&s.pool).await?;
    lock_frame_slots(&mut tx, &user).await?;
    let still_opening: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM app_frame_codes WHERE code_hash=$1 AND expires_at>clock_timestamp())",
    )
    .bind(hash(code))
    .fetch_one(&mut *tx)
    .await?;
    if !still_opening {
        return Err(ApiError::unauthorized());
    }
    // The parent's absolute end bounds the frame; a frame is never extended
    // past its own maximum lifetime even if the parent expiry is further out.
    let deadline = lock_parent(&mut tx, &user, &purpose, &parent, s.auth.development_login)
        .await?
        .min(Utc::now() + Duration::hours(8));
    let inst = sqlx::query(
        "SELECT i.app_id,i.environment,i.release_id,i.runtime_status,i.desired_state,r.image_id IS NULL AS fixture,r.manifest->'ui' AS ui,(SELECT b.bundle_version FROM app_builds b WHERE b.app_id=i.app_id AND b.image_id=r.image_id AND b.status='succeeded' ORDER BY b.finished_at DESC NULLS LAST LIMIT 1) AS bundle_version FROM installations i JOIN releases r ON r.id=i.release_id WHERE i.id=$1 FOR SHARE OF i",
    )
    .bind(site.installation_id)
    .fetch_one(&mut *tx)
    .await?;
    if inst.get::<Uuid, _>("release_id") != release {
        return Err(ApiError::conflict("release_changed_reload_required"));
    }
    let bundle: Option<String> = inst.get("bundle_version");
    if !running_or_fixture(&inst) {
        return Err(ApiError::unavailable("app_runtime_not_running"));
    }
    let (app, env): (String, String) = (inst.get("app_id"), inst.get("environment"));
    let ui: serde_json::Value = inst.get("ui");
    let id: Uuid = row.get("frame_id");
    // Each person keeps at most MAX_LIVE_FRAMES frame sessions (main frames,
    // side panels, other tabs). A new one retires the oldest beyond that; its
    // frame then asks to be reopened, like any ended frame session.
    sqlx::query(
        "UPDATE app_frame_sessions SET revoked_at=clock_timestamp() WHERE id IN (\
         SELECT id FROM app_frame_sessions WHERE user_id=$1 AND revoked_at IS NULL \
         AND expires_at>clock_timestamp() ORDER BY created_at DESC,id DESC OFFSET $2)",
    )
    .bind(&user)
    .bind(MAX_LIVE_FRAMES - 1)
    .execute(&mut *tx)
    .await?;
    let token = format!("{TOKEN_PREFIX}{}", crate::auth::random());
    let capability = crate::auth::random();
    let token_expires_at: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO app_frame_sessions(id,user_id,parent_session_hash,parent_development_hash,browser_purpose,installation_id,release_id,environment,origin,token_hash,created_at,token_expires_at,asset_hash,expires_at) SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,n.t,LEAST(n.t+make_interval(mins=>$11),$12),$13,$12 FROM (SELECT clock_timestamp() AS t) n RETURNING token_expires_at",
    )
    .bind(id)
    .bind(&user)
    .bind(&parent.session)
    .bind(&parent.development)
    .bind(&purpose)
    .bind(site.installation_id)
    .bind(release)
    .bind(&env)
    .bind(&site.origin)
    .bind(hash(&token))
    .bind(TOKEN_MINUTES)
    .bind(deadline)
    .bind(hash(&capability))
    .fetch_one(&mut *tx)
    .await?;
    tx.decide(&p, &app, &env, AppAction::Use)
        .await?
        .commit()
        .await?;
    let text = |key: &str| {
        ui.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned()
    };
    let bootstrap = FrameBootstrap {
        protocol: PROTOCOL.into(),
        frame_id: id,
        installation_id: site.installation_id,
        app_id: app,
        release_id: release,
        environment: serde_json::from_value(serde_json::Value::String(env))?,
        parent_origin: origin.to_owned(),
        base_path: form.base_path.clone(),
        location: form.location.clone(),
        token,
        token_expires_at,
        frame_expires_at: deadline,
        asset_base: format!("/_miy/a/{capability}/ui/"),
        module_entry: text("moduleEntry"),
        module_export: text("moduleExport"),
        contract_version: text("contractVersion"),
        settings: form.settings.clone(),
        settings_aware: settings_aware(bundle.as_deref()),
    };
    frame_document(s, origin, &bootstrap)
}

/// The frame document: the built runtime page with the bootstrap JSON added, or a
/// page carrying the bootstrap and a visible notice when no runtime is installed.
fn frame_document(s: &State, portal: &str, bootstrap: &FrameBootstrap) -> ApiResult<Response> {
    let script = format!(
        "<script id=\"miy-frame-bootstrap\" type=\"application/json\">{}</script>",
        script_json(bootstrap)?
    );
    let page = s
        .auth
        .frames
        .runtime_dir
        .as_ref()
        .and_then(|dir| std::fs::read_to_string(dir.join("index.html")).ok());
    let html = match page.as_deref().and_then(|p| p.split_once("</head>")) {
        Some((head, rest)) => format!("{head}{script}</head>{rest}"),
        // The launch is valid but nothing can run the app: say so instead of
        // leaving an empty frame for the person and no trace for the operator.
        None => {
            tracing::warn!(event = "frame_runtime_missing");
            format!(
                "<!doctype html><html lang=\"ko\"><head><meta charset=\"utf-8\">{script}</head><body><p role=\"alert\" data-miy-error=\"frame_runtime_missing\">앱 실행 화면이 이 서버에 설치되지 않았습니다. 관리자에게 알려 주세요. (frame_runtime_missing)</p></body></html>"
            )
        }
    };
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8".to_owned()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
            (
                header::CONTENT_SECURITY_POLICY,
                format!(
                    "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' blob:; font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; worker-src 'none'; frame-src 'none'; frame-ancestors {portal}"
                ),
            ),
        ],
        html,
    )
        .into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RenewInput {
    code: String,
}

/// `POST /_miy/renew`: exchange a portal-issued renew code for a new token of the
/// same frame. The frame session's end, release and asset prefix never change.
async fn renew(
    S(s): S<State>,
    method: Method,
    headers: HeaderMap,
    request: Request,
) -> ApiResult<Json<AppFrameToken>> {
    let site = site(&s, &headers).await?;
    if !browser_request_allowed(&method, &headers, &site.origin) {
        return Err(ApiError::forbidden());
    }
    let body = to_bytes(request.into_body(), 4096)
        .await
        .map_err(|_| ApiError::bad("request_too_large"))?;
    let input: RenewInput =
        serde_json::from_slice(&body).map_err(|_| ApiError::bad("invalid_renew_request"))?;
    let row = sqlx::query(
        "UPDATE app_frame_codes SET used_at=clock_timestamp() WHERE code_hash=$1 AND purpose='renew' AND used_at IS NULL AND expires_at>clock_timestamp() RETURNING frame_session_id,installation_id,release_id",
    )
    .bind(hash(&input.code))
    .fetch_optional(&s.pool)
    .await?
    .ok_or_else(ApiError::unauthorized)?;
    if row.get::<Uuid, _>("installation_id") != site.installation_id {
        return Err(ApiError::forbidden());
    }
    let id: Uuid = row.get("frame_session_id");
    let frame = sqlx::query(&format!(
        "SELECT f.user_id,f.browser_purpose,f.release_id,f.environment,f.origin,f.parent_development_hash IS NOT NULL AS development,i.app_id,i.release_id AS current_release FROM app_frame_sessions f JOIN installations i ON i.id=f.installation_id WHERE f.id=$2 AND f.revoked_at IS NULL AND f.expires_at>clock_timestamp() AND {PARENT_LIVE}"
    ))
    .bind(s.auth.development_login)
    .bind(id)
    .fetch_optional(&s.pool)
    .await?
    .ok_or_else(ApiError::unauthorized)?;
    let release: Uuid = frame.get("release_id");
    if frame.get::<String, _>("origin") != site.origin
        || row.get::<Uuid, _>("release_id") != release
    {
        return Err(ApiError::forbidden());
    }
    if frame.get::<Uuid, _>("current_release") != release {
        return Err(ApiError::conflict("release_changed_reload_required"));
    }
    let p = principal(
        frame.get("user_id"),
        &frame.get::<String, _>("browser_purpose"),
        frame.get("development"),
    )?;
    let (app, env): (String, String) = (frame.get("app_id"), frame.get("environment"));
    let token = format!("{TOKEN_PREFIX}{}", crate::auth::random());
    // Account writers lock the user before deleting a browser session and its
    // frames. Take that lock first and still decide current use before commit.
    let mut tx = policy::Write::begin(&s.pool).await?;
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR SHARE")
        .bind(&p.id)
        .execute(&mut *tx)
        .await?;
    let token_expires_at: DateTime<Utc> = sqlx::query_scalar(&format!(
        "UPDATE app_frame_sessions f SET token_hash=$2,token_expires_at=LEAST(clock_timestamp()+make_interval(mins=>$3),expires_at) \
         WHERE f.id=$4 AND f.revoked_at IS NULL AND f.expires_at>clock_timestamp() AND {PARENT_LIVE} RETURNING token_expires_at"
    ))
    .bind(s.auth.development_login)
    .bind(hash(&token))
    .bind(TOKEN_MINUTES)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(ApiError::unauthorized)?;
    tx.decide(&p, &app, &env, AppAction::Use)
        .await?
        .commit()
        .await?;
    Ok(Json(AppFrameToken {
        token,
        token_expires_at,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (k, v) in pairs {
            map.append(
                axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                v.parse().unwrap(),
            );
        }
        map
    }

    #[test]
    fn templates_form_one_origin_per_installation_and_never_a_platform_origin() {
        let key = Config::parse(
            "https://v2-app-{key}.example.com",
            None,
            false,
            "https",
            &["https://v2.example.com", "https://v2-console.example.com"],
        )
        .unwrap();
        let id = Uuid::new_v4();
        let origin = format!("https://v2-app-{}.example.com", id.simple());
        assert!(key.formed(&origin));
        assert!(!key.formed("https://v2.example.com"));
        assert!(!key.formed("https://v2-app-short.example.com"));
        assert_eq!(
            key.request_origin(&headers(&[(
                "host",
                &format!("V2-APP-{}.example.com:443", id.simple())
            )]))
            .as_deref(),
            Some(origin.as_str())
        );
        let port = Config::parse(
            "https://192.0.2.10:{port}",
            Some("20000-20010"),
            false,
            "https",
            &["https://192.0.2.10:8443"],
        )
        .unwrap();
        assert!(port.formed("https://192.0.2.10:20001"));
        assert_eq!(
            port.request_origin(&headers(&[("host", "192.0.2.10:20001")]))
                .as_deref(),
            Some("https://192.0.2.10:20001")
        );
        assert!(
            port.request_origin(&headers(&[("host", "192.0.2.10:20001"), ("host", "x")]))
                .is_none()
        );
        for (template, ports, code) in [
            ("https://{key}.{key}.example.com", None, "one_key_or_port"),
            ("https://app.{key}.example.com", None, "first_host_label"),
            ("https://192.0.2.10:{port}", None, "MIY_APP_FRAME_PORTS"),
            (
                "https://192.0.2.10:{port}",
                Some("30-20"),
                "invalid_app_frame_port_pool",
            ),
            (
                "http://192.0.2.10:{port}",
                Some("20000-20001"),
                "HTTP outside loopback",
            ),
            ("https://v2-{key}.example.com", None, "platform_origins"),
        ] {
            let error = Config::parse(
                template,
                ports,
                false,
                "https",
                &[&format!("https://v2-{}.example.com", "a".repeat(32))],
            )
            .unwrap_err()
            .to_string();
            assert!(error.contains(code), "{template}: {error}");
        }
    }

    #[test]
    fn requests_follow_the_frame_header_rules() {
        let own = "https://v2-app-x.example.com";
        let get = Method::GET;
        let post = Method::POST;
        assert!(browser_request_allowed(
            &get,
            &headers(&[("sec-fetch-site", "same-origin")]),
            own
        ));
        assert!(browser_request_allowed(
            &get,
            &headers(&[("sec-fetch-site", "same-origin"), ("origin", own)]),
            own
        ));
        assert!(!browser_request_allowed(&get, &headers(&[]), own));
        assert!(!browser_request_allowed(
            &get,
            &headers(&[("sec-fetch-site", "same-site")]),
            own
        ));
        assert!(!browser_request_allowed(
            &get,
            &headers(&[("sec-fetch-site", "same-origin"), ("origin", "null")]),
            own
        ));
        assert!(browser_request_allowed(
            &post,
            &headers(&[("origin", own)]),
            own
        ));
        assert!(!browser_request_allowed(&post, &headers(&[]), own));
        assert!(!browser_request_allowed(
            &post,
            &headers(&[("origin", "null")]),
            own
        ));
        assert!(!browser_request_allowed(
            &post,
            &headers(&[("origin", own), ("origin", own)]),
            own
        ));
        assert!(!browser_request_allowed(
            &post,
            &headers(&[("origin", "https://v2.example.com")]),
            own
        ));
    }

    #[test]
    fn bootstrap_json_cannot_close_its_script_element() {
        let text = script_json(&serde_json::json!({"x":"</script><b>&\u{2028}"})).unwrap();
        assert!(!text.contains('<') && !text.contains('>') && !text.contains('&'));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&text).unwrap()["x"],
            "</script><b>&\u{2028}"
        );
    }

    #[test]
    fn launch_form_keeps_only_safe_locations() {
        let code = "a".repeat(43);
        let form = launch_form(
            format!("code={code}&pathname=%2Fitems%2F1&search=%3Fview%3Dfull&hash=%23top&basePath=%2Fapps%2Fx")
                .as_bytes(),
        );
        assert_eq!(form.code.as_deref(), Some(code.as_str()));
        assert_eq!(form.location.pathname, "/items/1");
        assert_eq!(form.location.search, "?view=full");
        assert_eq!(form.location.hash, "#top");
        assert_eq!(form.base_path, "/apps/x");
        let bad = launch_form(b"code=short&pathname=items&search=view&hash=%0a&basePath=x");
        assert!(bad.code.is_none());
        assert_eq!(bad.location.pathname, "/");
        assert_eq!(
            (bad.location.search.as_str(), bad.location.hash.as_str()),
            ("", "")
        );
        assert_eq!(bad.base_path, "/");
    }

    #[test]
    fn launch_form_accepts_only_known_display_settings() {
        use crate::api_contract::{DateFormat, Locale, ThemePreference};
        let form =
            launch_form(b"theme=dark&locale=en-US&timeZone=America%2FNew_York&dateFormat=iso");
        assert_eq!(form.settings.theme, ThemePreference::Dark);
        assert_eq!(form.settings.locale, Locale::EnUs);
        assert_eq!(form.settings.time_zone, "America/New_York");
        assert_eq!(form.settings.date_format, DateFormat::Iso);
        let bad = launch_form(b"theme=neon&locale=fr-FR&timeZone=..%2Fetc&dateFormat=");
        assert_eq!(bad.settings, FrameSettings::default());
        assert!(time_zone_name("Etc/GMT+9") && time_zone_name("UTC"));
        assert!(
            !time_zone_name("/UTC")
                && !time_zone_name("Asia//Seoul")
                && !time_zone_name(&"a".repeat(65))
        );
    }

    #[test]
    fn only_new_bundles_are_settings_aware() {
        assert!(settings_aware(Some("0.12.0")) && settings_aware(Some("1.0.0")));
        assert!(!settings_aware(Some("0.11.0")) && !settings_aware(Some("0.12")));
        assert!(!settings_aware(None) && !settings_aware(Some("latest")));
    }
}
