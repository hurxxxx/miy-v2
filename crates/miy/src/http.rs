use crate::policy::{self, AppAction};
use crate::{
    State,
    auth::{BrowserPurpose, Principal},
    contract, domain,
};
use axum::{
    Json, Router,
    extract::{Path, Query, Request, State as S},
    http::{HeaderMap, header},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

pub use crate::api_error::{ApiError, ApiResult};

async fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}

pub fn router(state: State) -> Router {
    router_for(state, BrowserPurpose::Portal)
}

pub fn router_for(state: State, purpose: BrowserPurpose) -> Router {
    let api = Router::new()
        .route(
            "/apps/{app}/dev-sessions",
            axum::routing::post(crate::development::create)
                .layer(axum::extract::DefaultBodyLimit::max(
                    crate::development::MAX_BODY,
                ))
                .layer(middleware::map_response(no_store)),
        )
        .route(
            "/dev-sessions/{id}",
            axum::routing::get(crate::development::get).delete(crate::development::close),
        )
        .route(
            "/dev-sessions/{id}/heartbeat",
            axum::routing::post(crate::development::heartbeat),
        )
        .route(
            "/apps/{app}/dev-sessions/by-request/{request}",
            axum::routing::get(crate::development::by_request),
        )
        .route(
            "/components",
            axum::routing::get(crate::component_registry::catalog)
                .post(crate::component_registry::publish)
                .layer(axum::extract::DefaultBodyLimit::max(
                    crate::component_registry::MAX_PACKAGE,
                )),
        )
        .route(
            "/components/{name}/{version}",
            axum::routing::get(crate::component_registry::get),
        )
        .route(
            "/installations/{id}/platform/directory",
            axum::routing::get(crate::connections::directory),
        )
        .route(
            "/installations/{id}/connections/{target}/api/{*path}",
            axum::routing::any(crate::connections::connect),
        )
        .route(
            "/preview/apps/{app}/platform/directory",
            axum::routing::get(crate::connections::preview_directory),
        )
        .route(
            "/preview/apps/{app}/connections/{target}/api/{*path}",
            axum::routing::any(crate::connections::preview_connect),
        )
        .route("/me", axum::routing::get(me))
        .route(
            "/delegations",
            axum::routing::get(crate::delegations::list).post(crate::delegations::create),
        )
        .route(
            "/delegations/{id}",
            axum::routing::delete(crate::delegations::revoke),
        )
        .route(
            "/installations/{id}/permissions",
            axum::routing::get(crate::permissions::get).put(crate::permissions::put),
        )
        .route(
            "/cli-authorizations/{id}/approve",
            axum::routing::post(crate::cli_auth::approve),
        )
        .route("/cli/logout", axum::routing::post(crate::cli_auth::revoke))
        .route(
            "/cli/devices/{id}",
            axum::routing::get(crate::auth_sessions::device_status),
        )
        .route(
            "/cli/devices/{id}/decision",
            axum::routing::post(crate::auth_sessions::approve),
        )
        .route(
            "/cli/sessions",
            axum::routing::get(crate::auth_sessions::list_sessions),
        )
        .route(
            "/cli/sessions/{id}",
            axum::routing::delete(crate::auth_sessions::revoke_session),
        )
        .route(
            "/cli/legacy-sessions",
            axum::routing::get(crate::cli_auth::list_legacy),
        )
        .route(
            "/cli/legacy-sessions/{id}",
            axum::routing::delete(crate::cli_auth::revoke_legacy),
        )
        .route(
            "/apps/{app}/test-acceptance",
            axum::routing::post(check_release),
        )
        .route(
            "/apps/{app}/images",
            axum::routing::post(crate::delivery::upload)
                .layer(axum::extract::DefaultBodyLimit::disable()),
        )
        .route(
            "/apps/{app}/builds",
            axum::routing::post(crate::server_build::upload)
                .layer(axum::extract::DefaultBodyLimit::disable()),
        )
        .route(
            "/builds/{id}",
            axum::routing::get(crate::server_build::status),
        )
        .route(
            "/builds/{id}/logs",
            axum::routing::get(crate::server_build::logs),
        )
        .route(
            "/apps/{app}/build-requests/{request}",
            axum::routing::get(crate::server_build::by_request),
        )
        .route(
            "/image-uploads/{id}",
            axum::routing::get(crate::delivery::status),
        )
        .route(
            "/apps/{app}/image-uploads/by-request/{request}",
            axum::routing::get(crate::delivery::by_request),
        )
        .route(
            "/apps",
            axum::routing::get(crate::read_api::apps).post(create_app),
        )
        .route("/apps/{app}", axum::routing::get(crate::read_api::app))
        .route("/apps/{app}/name", axum::routing::post(rename_app))
        .route(
            "/app-registrations",
            axum::routing::get(registrations).post(request_registration),
        )
        .route(
            "/app-registrations/{id}/decision",
            axum::routing::post(decide_registration),
        )
        .route("/users", axum::routing::get(users))
        .route("/users/{id}/state", axum::routing::post(set_user_state))
        .route("/apps/{app}/releases", axum::routing::post(create_release))
        .route(
            "/installations",
            axum::routing::get(installations).post(create_installation),
        )
        .route(
            "/grants",
            axum::routing::get(crate::read_api::grants).put(grant),
        )
        .route("/audit", axum::routing::get(audit_list))
        .route(
            "/groups",
            axum::routing::get(groups_list).post(groups_create),
        )
        .route("/groups/{id}", axum::routing::patch(groups_update))
        .route("/groups/{id}/parent", axum::routing::put(groups_parent))
        .route(
            "/groups/{id}/members",
            axum::routing::get(groups_members).put(groups_set_member),
        )
        .route(
            "/users/{id}/primary-group",
            axum::routing::put(user_primary_group),
        )
        .route(
            "/group-grants",
            axum::routing::get(group_grants_list).put(group_grants_set),
        )
        .route("/apps/{app}/access", axum::routing::get(app_access))
        .route("/apps/{app}/admission", axum::routing::put(app_admission))
        .route("/apps/{app}/control", axum::routing::put(app_control))
        .route("/contracts", axum::routing::get(get_contract))
        .route(
            "/contracts/validate",
            axum::routing::post(validate_manifest),
        )
        .route("/catalog", axum::routing::get(catalog))
        .route("/openapi.json", axum::routing::get(openapi))
        .route("/database/prepare", axum::routing::post(prepare))
        .route("/jobs/{id}", axum::routing::get(get_job))
        .route("/apps/{app}/runtime", axum::routing::post(runtime_request))
        .route(
            "/apps/{app}/runtime/preflight",
            axum::routing::post(runtime_preflight),
        )
        .route(
            "/apps/{app}/logs",
            axum::routing::post(crate::runtime_logs::request),
        )
        .route(
            "/runtime/logs/{id}",
            axum::routing::get(crate::runtime_logs::get),
        )
        .route("/runtime/jobs", axum::routing::get(crate::read_api::jobs))
        .route(
            "/runtime/installations",
            axum::routing::get(managed_installations),
        )
        .route("/runtime/jobs/{id}", axum::routing::get(runtime_job))
        .route(
            "/runtime/jobs/{id}/events",
            axum::routing::get(crate::job_api::events),
        )
        .route(
            "/runtime/jobs/{id}/retry",
            axum::routing::post(crate::job_api::retry),
        )
        .route(
            "/installations/{id}/api/{*path}",
            axum::routing::any(crate::runtime::proxy),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), session_guard));
    let artifacts = Router::new()
        .route("/artifacts/{hash}/ui/{*path}", axum::routing::get(ui_asset))
        .route_layer(middleware::from_fn_with_state(state.clone(), session_guard));
    Router::new()
        .nest("/api/v1", api)
        .route(
            "/api/v1/dev-sessions/{id}/database",
            axum::routing::get(crate::development::database),
        )
        .route("/api/v1/capabilities", axum::routing::get(capabilities))
        .merge(crate::cli_install::router(state.auth.origin.clone()))
        .route(
            "/auth/cli/device",
            axum::routing::post(crate::auth_sessions::start),
        )
        .route(
            "/auth/cli/token",
            axum::routing::post(crate::auth_sessions::token),
        )
        .route(
            "/auth/cli/revoke",
            axum::routing::post(crate::auth_sessions::revoke),
        )
        .route(
            "/auth/cli/start",
            axum::routing::post(crate::cli_auth::start),
        )
        .route(
            "/auth/cli/exchange",
            axum::routing::post(crate::cli_auth::exchange),
        )
        .route("/auth/login", axum::routing::get(login))
        .route("/auth/options", axum::routing::get(login_options))
        .route("/auth/development", axum::routing::post(development_login))
        .route("/auth/callback", axum::routing::get(callback))
        .route("/auth/logout", axum::routing::post(logout))
        .route("/cli/authorize/{id}", axum::routing::get(approval_redirect))
        .route("/cli/device/{id}", axum::routing::get(approval_redirect))
        .route(
            "/.well-known/oauth-protected-resource",
            axum::routing::get(metadata),
        )
        .route(
            "/.well-known/oauth-protected-resource/mcp",
            axum::routing::get(metadata),
        )
        .route(
            "/api/health",
            axum::routing::get(|| async { Json(json!({"status":"ok"})) }),
        )
        .merge(crate::mcp::router(state.clone()))
        .merge(artifacts)
        .layer(middleware::from_fn_with_state(purpose, browser_boundary))
        .layer(axum::Extension(purpose))
        .with_state(state)
}

async fn browser_boundary(S(purpose): S<BrowserPurpose>, request: Request, next: Next) -> Response {
    let route = request
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map(|p| p.as_str());
    let rule = route.and_then(|route| crate::browser_boundary::access(route, request.method()));
    let bearer = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("Bearer "));
    let allowed = rule.is_some_and(|rule| {
        crate::browser_boundary::allowed(
            rule,
            purpose,
            bearer,
            request.headers().contains_key(header::ORIGIN),
        )
    }) && !request.headers().contains_key("service-worker");
    let mut response = if allowed {
        next.run(request).await
    } else {
        ApiError::forbidden().into_response()
    };
    response
        .headers_mut()
        .insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    response
        .headers_mut()
        .insert(header::X_FRAME_OPTIONS, "DENY".parse().unwrap());
    response
        .headers_mut()
        .entry(header::CONTENT_SECURITY_POLICY)
        .or_insert(
            "frame-ancestors 'none'; object-src 'none'; base-uri 'none'"
                .parse()
                .unwrap(),
        );
    response
        .headers_mut()
        .entry(header::CACHE_CONTROL)
        .or_insert("no-store".parse().unwrap());
    response
}

async fn approval_redirect(
    S(s): S<State>,
    axum::Extension(purpose): axum::Extension<BrowserPurpose>,
    Path(id): Path<Uuid>,
    uri: axum::http::Uri,
) -> ApiResult<Redirect> {
    if purpose != BrowserPurpose::Portal {
        return Err(ApiError::not_found());
    }
    let kind = if uri.path().starts_with("/cli/device/") {
        "device"
    } else {
        "authorize"
    };
    Ok(Redirect::to(&format!(
        "{}/cli/{kind}/{id}",
        s.auth.console_origin
    )))
}
async fn ui_asset(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path((hash, path)): Path<(String, String)>,
) -> ApiResult<Response> {
    if hash.len() != 64
        || !hash.bytes().all(|c| c.is_ascii_hexdigit())
        || !contract::safe_relative(&path)
    {
        return Err(ApiError::not_found());
    }
    let installations=sqlx::query("SELECT i.app_id,i.environment FROM installations i JOIN releases r ON r.id=i.release_id WHERE r.artifact_hash=$1").bind(&hash).fetch_all(&s.pool).await?;
    let mut allowed = false;
    for row in installations {
        if policy::require(
            &s,
            &p,
            row.get("app_id"),
            row.get("environment"),
            AppAction::Use,
        )
        .await
        .is_ok()
        {
            allowed = true;
            break;
        }
    }
    if !allowed {
        return Err(ApiError::forbidden());
    }
    let file = s.artifacts.join(hash).join("ui").join(path);
    let bytes = tokio::fs::read(&file)
        .await
        .map_err(|_| ApiError::not_found())?;
    let mime = match file.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "js" | "mjs" => "text/javascript",
        "css" => "text/css",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        _ => "application/octet-stream",
    };
    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "private, no-cache"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes,
    )
        .into_response())
}
/// Browser cookies are shared by every port of a host and SameSite treats those
/// ports as one site, so another origin's code can make a cookie-bearing request
/// to this listener. Reject such requests before any session lookup: only this
/// listener's own pages (or a user-typed navigation) may use its cookies. A
/// separate development web server is allowed only in the explicit development
/// login profile. Browsers without Fetch Metadata that omit Origin remain a
/// documented limit.
fn foreign_cookie_request(own: &str, development: Option<&str>, headers: &HeaderMap) -> bool {
    let value = |name: &str| {
        headers
            .get(name)
            .map(|value| value.to_str().unwrap_or("\u{0}"))
    };
    let origin = value("origin");
    let trusted = |origin: &str| origin == own || development == Some(origin);
    if origin.is_some_and(|origin| !trusted(origin)) {
        return true;
    }
    match value("sec-fetch-site") {
        None | Some("same-origin" | "none") => false,
        Some(_) => !origin.is_some_and(trusted),
    }
}

async fn session_guard(S(state): S<State>, mut request: Request, next: Next) -> Response {
    let Some(purpose) = request.extensions().get::<BrowserPurpose>().copied() else {
        return ApiError::forbidden().into_response();
    };
    let bearer = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let development = (purpose == BrowserPurpose::Portal && state.auth.development_login)
        .then_some(state.auth.development_origin.as_str());
    if bearer.is_none()
        && foreign_cookie_request(
            state.auth.browser_origin(purpose),
            development,
            request.headers(),
        )
    {
        return ApiError::forbidden().into_response();
    }
    let result = if let Some(token) = bearer {
        // CLI credentials cannot be used as a browser cross-origin authentication bypass.
        if request.headers().contains_key("origin") {
            return ApiError::forbidden().into_response();
        }
        if token.starts_with(crate::app_calls::PREFIX) {
            match crate::app_calls::authenticate(&state, token).await {
                Ok((p, context)) => {
                    let path = request
                        .extensions()
                        .get::<axum::extract::OriginalUri>()
                        .map(|u| u.0.path())
                        .unwrap_or(request.uri().path());
                    if !crate::app_calls::allowed_route(&context, path, request.method()) {
                        crate::app_calls::audit_rejected(&state, token, path).await;
                        return ApiError::forbidden().into_response();
                    }
                    request.extensions_mut().insert(context);
                    Ok(p)
                }
                Err(e) => {
                    let path = request
                        .extensions()
                        .get::<axum::extract::OriginalUri>()
                        .map(|u| u.0.path())
                        .unwrap_or(request.uri().path());
                    crate::app_calls::audit_rejected(&state, token, path).await;
                    Err(e)
                }
            }
        } else {
            crate::cli_auth::principal(&state, token).await
        }
    } else {
        state.auth.session(&state, request.headers(), purpose).await
    };
    match result {
        Ok(p) => {
            if let Some(origin) = crate::app_calls::origin(&state.auth, &p, request.headers()) {
                request.extensions_mut().insert(origin);
            }
            if let Err(error) = crate::auth_sessions::ensure_active(&state.pool, &p.id).await {
                return error.into_response();
            }
            if !matches!(
                *request.method(),
                axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS
            ) && p.csrf.is_some()
                && (request
                    .headers()
                    .get("x-miy-csrf")
                    .and_then(|v| v.to_str().ok())
                    != p.csrf.as_deref()
                    || !state
                        .auth
                        .allowed_origin(request.headers(), p.development, purpose))
            {
                return ApiError::forbidden().into_response();
            }
            let actor = p.id.clone();
            let path = request.uri().path().to_owned();
            let method = request.method().to_string();
            let route = request
                .extensions()
                .get::<axum::extract::MatchedPath>()
                .map(|p| p.as_str())
                .unwrap_or("");
            let allowed = p.delegation_id.is_none() || delegated_route(route, request.method());
            request.extensions_mut().insert(p);
            let mut response = if allowed {
                next.run(request).await
            } else {
                ApiError::forbidden().into_response()
            };
            let id = response
                .headers()
                .get("x-request-id")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            response
                .headers_mut()
                .insert("x-request-id", id.parse().unwrap());
            tracing::info!(event="api_request",request_id=%id,actor=%actor,method=%method,path=%path,status=response.status().as_u16());
            if (method != "GET"
                || !response.status().is_success()
                || path.contains("/connections/")
                || path.ends_with("/platform/directory"))
                && sqlx::query(
                    "INSERT INTO audit(actor,action,target,request_id) VALUES($1,$2,$3,$4)",
                )
                .bind(actor)
                .bind(format!("{method}:{}", response.status().as_u16()))
                .bind(path)
                .bind(id)
                .execute(&state.pool)
                .await
                .is_err()
            {
                tracing::error!(event = "audit_write_failed");
            }
            response
        }
        Err(e) => e.into_response(),
    }
}
// Only routes with explicit delegated action/resource checks may accept a
// restricted credential. A future unrelated account/admin API stays denied.
fn delegated_route(route: &str, method: &axum::http::Method) -> bool {
    use axum::http::Method;
    let route = route.strip_prefix("/api/v1").unwrap_or(route);
    match (method, route) {
        (
            &Method::GET,
            "/me"
            | "/contracts"
            | "/catalog"
            | "/openapi.json"
            | "/components"
            | "/components/{name}/{version}"
            | "/apps"
            | "/apps/{app}"
            | "/installations"
            | "/grants"
            | "/installations/{id}/permissions"
            | "/jobs/{id}"
            | "/runtime/jobs"
            | "/runtime/installations"
            | "/runtime/jobs/{id}"
            | "/runtime/jobs/{id}/events"
            | "/runtime/logs/{id}"
            | "/builds/{id}"
            | "/builds/{id}/logs"
            | "/apps/{app}/build-requests/{request}"
            | "/image-uploads/{id}"
            | "/apps/{app}/image-uploads/by-request/{request}"
            | "/dev-sessions/{id}"
            | "/apps/{app}/dev-sessions/by-request/{request}"
            | "/artifacts/{hash}/ui/{*path}",
        ) => true,
        (
            &Method::POST,
            "/contracts/validate"
            | "/apps/{app}/runtime"
            | "/apps/{app}/runtime/preflight"
            | "/runtime/jobs/{id}/retry"
            | "/apps/{app}/logs"
            | "/apps/{app}/builds"
            | "/apps/{app}/images"
            | "/apps/{app}/dev-sessions"
            | "/dev-sessions/{id}/heartbeat"
            | "/apps/{app}/test-acceptance"
            | "/database/prepare"
            | "/cli/logout",
        ) => true,
        (&Method::DELETE, "/dev-sessions/{id}") => true,
        (
            &Method::GET,
            "/installations/{id}/platform/directory" | "/preview/apps/{app}/platform/directory",
        ) => true,
        (
            _,
            "/installations/{id}/api/{*path}"
            | "/installations/{id}/connections/{target}/api/{*path}"
            | "/preview/apps/{app}/connections/{target}/api/{*path}",
        ) => matches!(
            *method,
            Method::GET
                | Method::HEAD
                | Method::POST
                | Method::PUT
                | Method::PATCH
                | Method::DELETE
        ),
        _ => false,
    }
}
type P = axum::Extension<Principal>;
async fn capabilities() -> Json<Value> {
    Json(
        json!({"schemaVersion":1,"cliContractVersion":1,"serverVersion":env!("CARGO_PKG_VERSION"),
        "runtimePlatform":crate::runtime_platform::server(),
        "userActivationSource":"core.users.enabled",
        "userActivationPropagation":"each authenticated request and queued operation; no Keycloak synchronization implied",
        "features":["installation.permissions.v1","auth.delegations.v1","app.calls.v1","app.builds.v1","runtime.preflight.v2","development.sessions.v1","auth.sessions.v1","auth.device.v1","runtime.jobs.v1","runtime.events.v1","runtime.retry.v1","images.idempotency.v1","runtime.logs.v1","lists.pagination.v1"]}),
    )
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct UserState {
    enabled: bool,
}
async fn set_user_state(
    S(s): S<State>,
    axum::Extension(p): P,
    Path(id): Path<String>,
    Json(input): Json<UserState>,
) -> ApiResult<Json<Value>> {
    domain::admin(&s, &p).await?;
    let mut tx = crate::policy::Write::begin(&s.pool).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(73490130)")
        .execute(&mut *tx)
        .await?;
    // Lock actor and target in one ID order, as primary-group changes do, so the
    // recheck holds the actor's authority until commit without a lock cycle.
    let users = sqlx::query(
        "SELECT id,admin,enabled FROM users WHERE id=ANY($1) ORDER BY id FOR NO KEY UPDATE",
    )
    .bind(vec![p.id.as_str(), id.as_str()])
    .fetch_all(&mut *tx)
    .await?;
    let target = users
        .iter()
        .find(|row| row.get::<String, _>("id") == id)
        .ok_or_else(ApiError::not_found)?;
    let (target_admin, before) = (
        target.get::<bool, _>("admin"),
        target.get::<bool, _>("enabled"),
    );
    // Before the update: an administrator may still disable their own account.
    let mut tx = tx
        .decide_platform(&p, crate::policy::PlatformAction::Administer)
        .await?;
    if !input.enabled && target_admin && before {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE admin AND enabled")
            .fetch_one(&mut *tx)
            .await?;
        if count <= 1 {
            return Err(ApiError::conflict("last_enabled_administrator"));
        }
    }
    sqlx::query("UPDATE users SET enabled=$2 WHERE id=$1")
        .bind(&id)
        .bind(input.enabled)
        .execute(&mut *tx)
        .await?;
    let action = if input.enabled {
        "user.enabled"
    } else {
        "user.disabled"
    };
    let change = format!("user:{id}:from:{before}:to:{}", input.enabled);
    crate::audit::record(&mut tx, &p.id, action, &change).await?;
    tx.commit().await?;
    Ok(Json(
        json!({"userId":id,"enabled":input.enabled,"source":"core","sessionsRevoked":!input.enabled}),
    ))
}
async fn registrations(
    S(s): S<State>,
    axum::Extension(p): P,
    Query(q): Query<crate::read_api::Page>,
) -> ApiResult<Json<Value>> {
    if q.limit.is_some()
        || q.cursor.is_some()
        || q.app_id.is_some()
        || q.environment.is_some()
        || q.request_id.is_some()
    {
        Ok(Json(
            crate::read_api::page(&s, &p, "registrations", q).await?,
        ))
    } else {
        Ok(Json(crate::registration::list(&s, &p).await?))
    }
}
async fn request_registration(
    S(s): S<State>,
    axum::Extension(p): P,
    Json(v): Json<domain::NewApp>,
) -> ApiResult<Json<Value>> {
    Ok(Json(crate::registration::request(&s, &p, v).await?))
}
async fn decide_registration(
    S(s): S<State>,
    axum::Extension(p): P,
    Path(id): Path<Uuid>,
    Json(v): Json<crate::registration::Decision>,
) -> ApiResult<Json<Value>> {
    Ok(Json(crate::registration::decide(&s, &p, id, v).await?))
}
async fn users(
    S(s): S<State>,
    axum::Extension(p): P,
    Query(q): Query<crate::read_api::Page>,
) -> ApiResult<Json<Value>> {
    if q.limit.is_some()
        || q.cursor.is_some()
        || q.app_id.is_some()
        || q.environment.is_some()
        || q.request_id.is_some()
    {
        Ok(Json(crate::read_api::page(&s, &p, "users", q).await?))
    } else {
        Ok(Json(crate::registration::users(&s, &p).await?))
    }
}
async fn check_release(
    S(s): S<State>,
    axum::Extension(p): P,
    Path(app): Path<String>,
    Json(v): Json<crate::runtime::ReleaseCheck>,
) -> ApiResult<Json<Value>> {
    Ok(Json(crate::runtime::check_release(&s, &p, &app, v).await?))
}
async fn runtime_request(
    S(s): S<State>,
    axum::Extension(p): P,
    Path(app): Path<String>,
    Json(v): Json<crate::runtime::RuntimeRequest>,
) -> ApiResult<Json<Value>> {
    Ok(Json(crate::runtime::request(&s, &p, &app, v).await?))
}
async fn runtime_preflight(
    S(s): S<State>,
    axum::Extension(p): P,
    Path(app): Path<String>,
    Json(input): Json<crate::runtime_contract::PreflightInput>,
) -> ApiResult<Json<crate::runtime_contract::PreflightResult>> {
    Ok(Json(
        crate::runtime_preflight::inspect(&s, &p, &app, input).await?,
    ))
}
async fn runtime_job(
    S(s): S<State>,
    axum::Extension(p): P,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    Ok(Json(crate::runtime::job(&s, &p, id).await?))
}
async fn me(S(s): S<State>, axum::Extension(p): P) -> ApiResult<Json<Value>> {
    Ok(Json(domain::me(&s, &p).await?))
}
async fn create_app(
    S(s): S<State>,
    axum::Extension(p): P,
    Json(v): Json<domain::NewApp>,
) -> ApiResult<Json<Value>> {
    Ok(Json(domain::create_app(&s, &p, v).await?))
}
async fn create_release(
    S(s): S<State>,
    axum::Extension(p): P,
    Path(app): Path<String>,
    Json(v): Json<domain::NewRelease>,
) -> ApiResult<Json<Value>> {
    Ok(Json(domain::create_release(&s, &p, &app, v).await?))
}
async fn installations(
    S(s): S<State>,
    axum::Extension(p): P,
    Query(q): Query<crate::read_api::Page>,
) -> ApiResult<Json<Value>> {
    if q.limit.is_some()
        || q.cursor.is_some()
        || q.app_id.is_some()
        || q.environment.is_some()
        || q.request_id.is_some()
    {
        Ok(Json(
            crate::read_api::page(&s, &p, "installations", q).await?,
        ))
    } else {
        Ok(Json(domain::installations(&s, &p).await?))
    }
}
async fn managed_installations(S(s): S<State>, axum::Extension(p): P) -> ApiResult<Json<Value>> {
    Ok(Json(domain::managed_installations(&s, &p).await?))
}
async fn create_installation(
    S(s): S<State>,
    axum::Extension(p): P,
    Json(v): Json<domain::NewInstallation>,
) -> ApiResult<Json<Value>> {
    Ok(Json(domain::create_installation(&s, &p, v).await?))
}
async fn grant(
    S(s): S<State>,
    axum::Extension(p): P,
    Json(v): Json<domain::Grant>,
) -> ApiResult<Json<Value>> {
    Ok(Json(domain::grant(&s, &p, v).await?))
}
async fn audit_list(
    S(s): S<State>,
    axum::Extension(p): P,
    Query(q): Query<crate::audit::AuditQuery>,
) -> ApiResult<Json<Value>> {
    Ok(Json(crate::audit::list(&s, &p, q).await?))
}
async fn groups_list(S(s): S<State>, axum::Extension(p): P) -> ApiResult<Json<Value>> {
    Ok(Json(crate::groups::list(&s, &p).await?))
}
async fn groups_create(
    S(s): S<State>,
    axum::Extension(p): P,
    Json(v): Json<crate::groups::NewGroup>,
) -> ApiResult<Json<Value>> {
    Ok(Json(crate::groups::create(&s, &p, v).await?))
}
async fn groups_update(
    S(s): S<State>,
    axum::Extension(p): P,
    Path(id): Path<Uuid>,
    Json(v): Json<crate::groups::GroupChange>,
) -> ApiResult<Json<Value>> {
    Ok(Json(crate::groups::update(&s, &p, id, v).await?))
}
async fn groups_parent(
    S(s): S<State>,
    axum::Extension(p): P,
    Path(id): Path<Uuid>,
    Json(v): Json<crate::groups::GroupParent>,
) -> ApiResult<Json<Value>> {
    Ok(Json(crate::groups::set_parent(&s, &p, id, v).await?))
}
async fn groups_members(
    S(s): S<State>,
    axum::Extension(p): P,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    Ok(Json(crate::groups::members(&s, &p, id).await?))
}
async fn groups_set_member(
    S(s): S<State>,
    axum::Extension(p): P,
    Path(id): Path<Uuid>,
    Json(v): Json<crate::groups::Membership>,
) -> ApiResult<Json<Value>> {
    Ok(Json(crate::groups::set_member(&s, &p, id, v).await?))
}
async fn user_primary_group(
    S(s): S<State>,
    axum::Extension(p): P,
    Path(id): Path<String>,
    Json(v): Json<crate::groups::PrimaryGroup>,
) -> ApiResult<Json<Value>> {
    Ok(Json(
        crate::groups::set_primary_group(&s, &p, &id, v).await?,
    ))
}
async fn group_grants_list(S(s): S<State>, axum::Extension(p): P) -> ApiResult<Json<Value>> {
    Ok(Json(crate::groups::grants(&s, &p).await?))
}
async fn group_grants_set(
    S(s): S<State>,
    axum::Extension(p): P,
    Json(v): Json<crate::groups::GroupGrant>,
) -> ApiResult<Json<Value>> {
    Ok(Json(crate::groups::set_grant(&s, &p, v).await?))
}
async fn app_access(
    S(s): S<State>,
    axum::Extension(p): P,
    Path(app): Path<String>,
) -> ApiResult<Json<Value>> {
    Ok(Json(crate::groups::access(&s, &p, &app).await?))
}
async fn app_admission(
    S(s): S<State>,
    axum::Extension(p): P,
    Path(app): Path<String>,
    Json(v): Json<crate::groups::Admission>,
) -> ApiResult<Json<Value>> {
    Ok(Json(crate::groups::set_admission(&s, &p, &app, v).await?))
}
async fn app_control(
    S(s): S<State>,
    axum::Extension(p): P,
    Path(app): Path<String>,
    Json(v): Json<crate::groups::AppControl>,
) -> ApiResult<Json<Value>> {
    Ok(Json(crate::groups::set_control(&s, &p, &app, v).await?))
}
async fn get_contract() -> Json<Value> {
    Json(domain::get_contract())
}
async fn catalog() -> Json<Value> {
    Json(domain::catalog())
}
async fn validate_manifest(Json(v): Json<Value>) -> ApiResult<Json<Value>> {
    let m: contract::Manifest = serde_json::from_value(v)?;
    contract::validate(&m).map_err(ApiError::bad_owned)?;
    Ok(Json(json!({"valid":true})))
}
async fn prepare(
    S(s): S<State>,
    axum::Extension(p): P,
    Json(v): Json<domain::Prepare>,
) -> ApiResult<Json<Value>> {
    Ok(Json(domain::prepare(&s, &p, v).await?))
}
async fn get_job(
    S(s): S<State>,
    axum::Extension(p): P,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    Ok(Json(domain::get_job(&s, &p, id).await?))
}
/// Sign-in is a top-level navigation. An image, script or fetch from another
/// origin must not create login flows or clear the development cookie.
fn login_navigation(headers: &HeaderMap) -> bool {
    headers
        .get("sec-fetch-mode")
        .is_none_or(|mode| mode.as_bytes() == b"navigate")
}

async fn login(
    S(s): S<State>,
    axum::Extension(purpose): axum::Extension<BrowserPurpose>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    if !login_navigation(&headers) {
        return Err(ApiError::forbidden());
    }
    let (url, value) = s.auth.login(&s, purpose).await?;
    let mut r = Redirect::to(&url).into_response();
    r.headers_mut().append(
        header::SET_COOKIE,
        s.auth
            .cookie(purpose, "login", &value, 300)
            .parse()
            .unwrap(),
    );
    r.headers_mut().append(
        header::SET_COOKIE,
        s.auth
            .cookie(purpose, "development", "", 0)
            .parse()
            .unwrap(),
    );
    Ok(r)
}
async fn login_options(
    S(s): S<State>,
    axum::Extension(purpose): axum::Extension<BrowserPurpose>,
) -> Json<Value> {
    let accounts = if s.auth.development_login {
        json!([
            {"account":"admin","label":"관리자로 시작","description":"전체 앱과 관리 기능"},
            {"account":"developer","label":"개발자","description":"앱 개발 권한 확인"},
            {"account":"alice","label":"사용자 A","description":"일반 사용자 권한 확인"},
            {"account":"bob","label":"사용자 B","description":"다른 사용자 접근 확인"}
        ])
    } else {
        json!([])
    };
    Json(json!({"developmentLogin":s.auth.development_login,
            "developmentOrigin":if purpose == BrowserPurpose::Portal { &s.auth.development_origin } else { &s.auth.console_origin },
            "accounts":accounts,"oauthUrl":format!("{}/auth/login",s.auth.browser_origin(purpose)),
            "surface":purpose,"portalOrigin":s.auth.origin,"consoleOrigin":s.auth.console_origin}))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DevelopmentLogin {
    account: String,
}
async fn development_login(
    S(s): S<State>,
    axum::Extension(purpose): axum::Extension<BrowserPurpose>,
    headers: HeaderMap,
    Json(v): Json<DevelopmentLogin>,
) -> ApiResult<Response> {
    if !s.auth.development_login {
        return Err(ApiError::not_found());
    }
    if !s.auth.allowed_origin(&headers, true, purpose) {
        return Err(ApiError::forbidden());
    }
    let session = s.auth.development_session(&s, &v.account, purpose).await?;
    let mut response = Json(json!({"redirect":"/"})).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response.headers_mut().append(
        header::SET_COOKIE,
        s.auth
            .cookie(purpose, "development", &session, 28800)
            .parse()
            .unwrap(),
    );
    Ok(response)
}
#[derive(Deserialize)]
struct Callback {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}
async fn callback(
    S(s): S<State>,
    axum::Extension(purpose): axum::Extension<BrowserPurpose>,
    headers: HeaderMap,
    Query(v): Query<Callback>,
) -> ApiResult<Response> {
    let result = match (v.error, v.code, v.state) {
        (None, Some(code), Some(state)) => {
            s.auth.callback(&s, purpose, &code, &state, &headers).await
        }
        _ => Err(ApiError::unauthorized()),
    };
    let session = match result {
        Ok(session) => session,
        Err(error) => {
            let target = match error.status.as_u16() {
                401 => "/?login=expired",
                403 => "/?login=forbidden",
                503 => "/?login=unavailable",
                _ => return Err(error),
            };
            tracing::info!(event = "browser_login_failed", code = %error.code);
            let mut response = Redirect::to(target).into_response();
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
            response.headers_mut().append(
                header::SET_COOKIE,
                s.auth.cookie(purpose, "login", "", 0).parse().unwrap(),
            );
            return Ok(response);
        }
    };
    let mut r = Redirect::to("/").into_response();
    r.headers_mut().append(
        header::SET_COOKIE,
        s.auth
            .cookie(purpose, "session", &session, 28800)
            .parse()
            .unwrap(),
    );
    r.headers_mut().append(
        header::SET_COOKIE,
        s.auth.cookie(purpose, "login", "", 0).parse().unwrap(),
    );
    Ok(r)
}
async fn logout(
    S(s): S<State>,
    axum::Extension(purpose): axum::Extension<BrowserPurpose>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let p = s.auth.session(&s, &headers, purpose).await?;
    if headers.get("x-miy-csrf").and_then(|v| v.to_str().ok()) != p.csrf.as_deref()
        || !s.auth.allowed_origin(&headers, p.development, purpose)
    {
        return Err(ApiError::forbidden());
    }
    let url = s.auth.logout(&s, &headers, purpose).await?;
    let mut r = Json(json!({"redirect":url})).into_response();
    r.headers_mut().append(
        header::SET_COOKIE,
        s.auth.cookie(purpose, "session", "", 0).parse().unwrap(),
    );
    r.headers_mut().append(
        header::SET_COOKIE,
        s.auth
            .cookie(purpose, "development", "", 0)
            .parse()
            .unwrap(),
    );
    Ok(r)
}
async fn metadata(S(s): S<State>) -> Json<Value> {
    Json(
        json!({"resource":format!("{}/mcp",s.auth.origin),"authorization_servers":[s.auth.issuer],"scopes_supported":["miy.read","miy.dev"],"bearer_methods_supported":["header"]}),
    )
}
async fn openapi(
    S(s): S<State>,
    axum::Extension(purpose): axum::Extension<BrowserPurpose>,
) -> Json<Value> {
    let mut paths = serde_json::Map::new();
    for item in domain::catalog().as_array().unwrap() {
        let method = item["method"].as_str().unwrap().to_lowercase();
        let path = item["path"].as_str().unwrap();
        let mut operation = json!({"operationId":item["id"],"summary":item["description"],"security":[{"session":[]},{"bearer":[]}],"responses":{"200":{"description":"Success","content":{"application/json":{"schema":{}}}},"400":{"description":"Invalid contract/input"},"401":{"description":"Authentication required"},"403":{"description":"Permission denied"},"409":{"description":"Conflict"}}});
        if !item["input"].is_null() {
            operation["requestBody"] =
                json!({"required":true,"content":{"application/json":{"schema":item["input"]}}});
        }
        if item["binary"] == true {
            operation["requestBody"] = json!({"required":true,"content":{"application/x-tar":{"schema":{"type":"string","format":"binary"}}}});
        }
        let parameters=path.split('/').filter(|p|p.starts_with('{')).map(|p|json!({"name":p.trim_matches(&['{','}'][..]),"in":"path","required":true,"schema":{"type":"string"}})).collect::<Vec<_>>();
        if !parameters.is_empty() {
            operation["parameters"] = json!(parameters);
        }
        paths.entry(path.to_owned()).or_insert_with(|| json!({}))[method] = operation;
    }
    // Each request embeds a standalone JSON Schema. Move local definitions to
    // OpenAPI components and rewrite references so generated clients resolve them.
    let mut definitions = serde_json::Map::new();
    fn normalize(value: &mut Value, definitions: &mut serde_json::Map<String, Value>) {
        match value {
            Value::Object(obj) => {
                obj.remove("$schema");
                if let Some(Value::Object(defs)) = obj.remove("$defs") {
                    for (k, mut v) in defs {
                        normalize(&mut v, definitions);
                        definitions.insert(k, v);
                    }
                }
                if let Some(Value::String(r)) = obj.get_mut("$ref") {
                    *r = r.replace("#/$defs/", "#/components/schemas/");
                }
                for v in obj.values_mut() {
                    normalize(v, definitions);
                }
            }
            Value::Array(a) => {
                for v in a {
                    normalize(v, definitions);
                }
            }
            _ => {}
        }
    }
    let mut doc = json!({"openapi":"3.1.0","info":{"title":"miy V0-V1 validation API","version":"0.1.0"},"paths":paths});
    normalize(&mut doc, &mut definitions);
    doc["components"] = json!({"schemas":definitions,"securitySchemes":{"session":{"type":"apiKey","in":"cookie","name":s.auth.cookie_name(purpose,"session")},"bearer":{"type":"http","scheme":"bearer"}}});
    Json(doc)
}

async fn rename_app(
    S(s): S<State>,
    axum::Extension(p): P,
    Path(app): Path<String>,
    Json(v): Json<crate::registration::DisplayName>,
) -> ApiResult<Json<Value>> {
    Ok(Json(crate::registration::rename(&s, &p, &app, v).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.insert(
                axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                value.parse().unwrap(),
            );
        }
        headers
    }

    #[test]
    fn cookie_requests_from_another_origin_on_the_same_host_are_rejected() {
        let own = "https://192.168.10.20:8443";
        let other = "https://192.168.10.20:9443";
        let dev = "http://127.0.0.1:5173";
        for allowed in [
            headers(&[]),
            headers(&[("sec-fetch-site", "same-origin")]),
            headers(&[("sec-fetch-site", "same-origin"), ("origin", own)]),
            headers(&[("sec-fetch-site", "none")]),
            headers(&[("origin", own)]),
        ] {
            assert!(!foreign_cookie_request(own, None, &allowed));
        }
        for rejected in [
            headers(&[("sec-fetch-site", "same-site")]),
            headers(&[("sec-fetch-site", "cross-site")]),
            headers(&[("sec-fetch-site", "same-site"), ("origin", other)]),
            headers(&[("origin", other)]),
            headers(&[("origin", "null")]),
            // A top-level navigation from another origin still carries cookies.
            headers(&[
                ("sec-fetch-site", "same-site"),
                ("sec-fetch-mode", "navigate"),
            ]),
            headers(&[("sec-fetch-site", "same-site"), ("origin", dev)]),
        ] {
            assert!(foreign_cookie_request(own, None, &rejected));
        }
        let development = headers(&[("sec-fetch-site", "same-site"), ("origin", dev)]);
        assert!(!foreign_cookie_request(own, Some(dev), &development));
        assert!(foreign_cookie_request(
            own,
            Some(dev),
            &headers(&[("sec-fetch-site", "same-site"), ("origin", other)])
        ));
    }

    #[test]
    fn sign_in_starts_only_from_a_navigation() {
        assert!(login_navigation(&headers(&[])));
        assert!(login_navigation(&headers(&[(
            "sec-fetch-mode",
            "navigate"
        )])));
        for mode in ["no-cors", "cors", "same-origin"] {
            assert!(!login_navigation(&headers(&[("sec-fetch-mode", mode)])));
        }
    }
}
