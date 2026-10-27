use crate::{State, auth::Principal, contract, domain};
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

pub fn router(state: State) -> Router {
    let api = Router::new()
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
            "/cli-authorizations/{id}/approve",
            axum::routing::post(crate::cli_auth::approve),
        )
        .route("/cli/logout", axum::routing::post(crate::cli_auth::revoke))
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
            "/apps/{app}/test-acceptance",
            axum::routing::post(check_release),
        )
        .route(
            "/apps/{app}/images",
            axum::routing::post(crate::delivery::upload)
                .layer(axum::extract::DefaultBodyLimit::disable()),
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
        .route("/api/v1/capabilities", axum::routing::get(capabilities))
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
        .with_state(state)
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
        if domain::authorize(&s, &p, row.get("app_id"), row.get("environment"), false)
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
async fn session_guard(S(state): S<State>, mut request: Request, next: Next) -> Response {
    let bearer = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    let result = if let Some(token) = bearer {
        // CLI credentials cannot be used as a browser cross-origin authentication bypass.
        if request.headers().contains_key("origin") {
            return ApiError::forbidden().into_response();
        }
        crate::cli_auth::principal(&state, token).await
    } else {
        state.auth.session(&state, request.headers()).await
    };
    match result {
        Ok(p) => {
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
                    || !state.auth.allowed_origin(request.headers(), p.development))
            {
                return ApiError::forbidden().into_response();
            }
            let actor = p.id.clone();
            let path = request.uri().path().to_owned();
            let method = request.method().to_string();
            request.extensions_mut().insert(p);
            let mut response = next.run(request).await;
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
            if method != "GET"
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
type P = axum::Extension<Principal>;
async fn capabilities() -> Json<Value> {
    Json(
        json!({"schemaVersion":1,"cliContractVersion":1,"serverVersion":env!("CARGO_PKG_VERSION"),
        "runtimePlatform":crate::runtime_platform::server(),
        "userActivationSource":"core.users.enabled",
        "userActivationPropagation":"each authenticated request and queued operation; no Keycloak synchronization implied",
        "features":["runtime.preflight.v1","auth.sessions.v1","auth.device.v1","runtime.jobs.v1","runtime.events.v1","runtime.retry.v1","images.idempotency.v1","runtime.logs.v1","lists.pagination.v1"]}),
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
    let mut tx = s.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(73490130)")
        .execute(&mut *tx)
        .await?;
    let target = sqlx::query("SELECT admin,enabled FROM users WHERE id=$1 FOR UPDATE")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if !input.enabled && target.get::<bool, _>("admin") && target.get::<bool, _>("enabled") {
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
async fn login(S(s): S<State>) -> ApiResult<Response> {
    let (url, value) = s.auth.login(&s).await?;
    let mut r = Redirect::to(&url).into_response();
    r.headers_mut().append(
        header::SET_COOKIE,
        format!("miy_login={value}; HttpOnly; Secure; SameSite=Lax; Path=/auth; Max-Age=300")
            .parse()
            .unwrap(),
    );
    r.headers_mut().append(
        header::SET_COOKIE,
        "miy_development=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"
            .parse()
            .unwrap(),
    );
    Ok(r)
}
async fn login_options(S(s): S<State>) -> Json<Value> {
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
    Json(
        json!({"developmentLogin":s.auth.development_login,"developmentOrigin":s.auth.development_origin,"accounts":accounts,"oauthUrl":format!("{}/auth/login",s.auth.origin)}),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DevelopmentLogin {
    account: String,
}
async fn development_login(
    S(s): S<State>,
    headers: HeaderMap,
    Json(v): Json<DevelopmentLogin>,
) -> ApiResult<Response> {
    if !s.auth.development_login {
        return Err(ApiError::not_found());
    }
    if !s.auth.allowed_origin(&headers, true) {
        return Err(ApiError::forbidden());
    }
    let session = s.auth.development_session(&s, &v.account).await?;
    let mut response = Json(json!({"redirect":"/"})).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response.headers_mut().append(
        header::SET_COOKIE,
        format!("miy_development={session}; HttpOnly; SameSite=Lax; Path=/; Max-Age=28800")
            .parse()
            .unwrap(),
    );
    Ok(response)
}
#[derive(Deserialize)]
struct Callback {
    code: Option<String>,
    state: String,
    error: Option<String>,
}
async fn callback(
    S(s): S<State>,
    headers: HeaderMap,
    Query(v): Query<Callback>,
) -> ApiResult<Response> {
    if v.error.is_some() {
        return Err(ApiError::unauthorized());
    }
    let code = v.code.ok_or_else(ApiError::unauthorized)?;
    let session = s.auth.callback(&s, &code, &v.state, &headers).await?;
    let mut r = Redirect::to("/").into_response();
    r.headers_mut().append(
        header::SET_COOKIE,
        format!("miy_session={session}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=28800")
            .parse()
            .unwrap(),
    );
    r.headers_mut().append(
        header::SET_COOKIE,
        "miy_login=; HttpOnly; Secure; SameSite=Lax; Path=/auth; Max-Age=0"
            .parse()
            .unwrap(),
    );
    Ok(r)
}
async fn logout(S(s): S<State>, headers: HeaderMap) -> ApiResult<Response> {
    let p = s.auth.session(&s, &headers).await?;
    if headers.get("x-miy-csrf").and_then(|v| v.to_str().ok()) != p.csrf.as_deref()
        || !s.auth.allowed_origin(&headers, p.development)
    {
        return Err(ApiError::forbidden());
    }
    let url = s.auth.logout(&s, &headers).await?;
    let mut r = Json(json!({"redirect":url})).into_response();
    r.headers_mut().append(
        header::SET_COOKIE,
        "miy_session=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0"
            .parse()
            .unwrap(),
    );
    r.headers_mut().append(
        header::SET_COOKIE,
        "miy_development=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"
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
async fn openapi() -> Json<Value> {
    let mut paths = serde_json::Map::new();
    for item in domain::catalog().as_array().unwrap() {
        let method = item["method"].as_str().unwrap().to_lowercase();
        let path = item["path"].as_str().unwrap();
        let mut operation = json!({"operationId":item["id"],"summary":item["description"],"security":[{"session":[]}],"responses":{"200":{"description":"Success","content":{"application/json":{"schema":{}}}},"400":{"description":"Invalid contract/input"},"401":{"description":"Authentication required"},"403":{"description":"Permission denied"},"409":{"description":"Conflict"}}});
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
    doc["components"] = json!({"schemas":definitions,"securitySchemes":{"session":{"type":"apiKey","in":"cookie","name":"miy_session"}}});
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
