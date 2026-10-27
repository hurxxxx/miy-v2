//! Local preview uses its own PostgreSQL and app container. Server credentials stay in this gateway.
use crate::{
    api_error::{ApiError, ApiResult},
    app_cli::{self, Session},
    cli_error::{Category, CliError},
    contract, docker as runtime, identity as auth,
};
use anyhow::{Context, Result, ensure};
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{Path, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
};
use serde_json::{Value, json};
use sqlx::{Connection, Executor};
use std::{future::Future, path::PathBuf, sync::Arc};
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

fn prefix() -> Result<String> {
    Ok(format!(
        "miy-preview-{}",
        &contract::hash(
            std::env::current_dir()?
                .canonicalize()?
                .to_string_lossy()
                .as_bytes()
        )[..16]
    ))
}
async fn remove_owned(name: &str, project: &str) -> Result<()> {
    let ids = runtime::docker(&[
        "ps",
        "-a",
        "--filter",
        &format!("name=^{name}$"),
        "--format",
        "{{.ID}}",
    ])
    .await?;
    if !ids.trim().is_empty() {
        let meta = runtime::inspect(name).await?;
        ensure!(
            meta["Config"]["Labels"]["org.miy.preview"] == project,
            "unmanaged_preview_container"
        );
        runtime::docker(&["rm", "-f", name]).await?;
    }
    Ok(())
}
fn lock_file() -> Result<std::fs::File> {
    std::fs::create_dir_all(".miy")?;
    Ok(std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(".miy/preview.lock")?)
}
async fn remove_containers() -> Result<()> {
    let name = prefix()?;
    remove_owned(&format!("{name}-app"), &name).await?;
    remove_owned(&format!("{name}-db"), &name).await
}
fn remove_state() {
    for path in [
        ".miy/preview.json",
        ".miy/preview-run.json",
        ".miy/preview-stop.json",
    ] {
        let _ = std::fs::remove_file(path);
    }
}
fn stop_requested(run: &str) -> bool {
    std::fs::read(".miy/preview-stop.json")
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .is_some_and(|v| v["runId"] == run)
}
pub async fn stop() -> Result<()> {
    let lock = lock_file()?;
    if lock.try_lock().is_err() {
        let v: Value = std::fs::read(".miy/preview-run.json").ok()
            .and_then(|v| serde_json::from_slice(&v).ok())
            .context("legacy_preview_runner: stop app dev with Ctrl-C in its terminal, then use the updated CLI for the next preview; no containers were removed")?;
        ensure!(
            v["protocol"] == 1 && v["project"] == prefix()?,
            "invalid_preview_owner"
        );
        let run = v["runId"].as_str().context("invalid_preview_owner")?;
        app_cli::save(".miy/preview-stop.json", &json!({"runId":run}))?;
        // Never signal a PID or remove containers while their owner is still using them.
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(45);
        loop {
            if lock.try_lock().is_ok() {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(CliError::new(Category::Network, "preview_shutdown_pending", "The stop request was saved, but preview startup or cleanup has not finished yet.")
                    .with_recovery("miy", &["app", "dev-stop"])
                    .into());
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
    remove_containers().await?;
    remove_state();
    eprintln!(
        "Local preview stopped: gateway, port, project lock and containers released; local database volume retained."
    );
    Ok(())
}
#[derive(Clone)]
struct Preview {
    session: Session,
    manifest: contract::Manifest,
    user: Value,
    installation: Uuid,
    release: Uuid,
    port: i32,
    secret: String,
    cookie: String,
    csrf: String,
    origin: String,
    cookie_name: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shutdown {
    Running,
    Requested,
    Interrupted,
    SignalUnavailable,
}

async fn shutdown_reason(
    signal: impl Future<Output = std::io::Result<()>>,
    requested: impl Future<Output = ()>,
) -> Shutdown {
    tokio::select! {
        result = signal => if result.is_ok() { Shutdown::Interrupted } else { Shutdown::SignalUnavailable },
        _ = requested => Shutdown::Requested,
    }
}

pub async fn run(port: u16, no_open: bool) -> Result<()> {
    let preview_lock = lock_file()?;
    preview_lock.try_lock().map_err(|_| {
        CliError::new(
            Category::Conflict,
            "preview_already_running_for_this_project",
            "This project already has an active preview runner.",
        )
        .with_recovery("miy", &["app", "dev-stop"])
    })?;
    remove_state();
    let run = Uuid::new_v4().to_string();
    app_cli::save(
        ".miy/preview-run.json",
        &json!({"protocol":1,"project":prefix()?,"runId":run,"pid":std::process::id()}),
    )?;
    let (shutdown, rx) = tokio::sync::watch::channel(Shutdown::Running);
    let observed = rx.clone();
    let watcher = tokio::spawn(async move {
        let reason = shutdown_reason(tokio::signal::ctrl_c(), async {
            loop {
                if stop_requested(&run) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        })
        .await;
        let _ = shutdown.send(reason);
    });
    // Startup completes its current build/migration before cleanup, avoiding orphaned children.
    let result = serve_preview(port, no_open, rx).await;
    watcher.abort();
    let reason = *observed.borrow();
    let cleaned = remove_containers().await;
    remove_state();
    if reason == Shutdown::Interrupted {
        if cleaned.is_err() {
            return Err(CliError::new(
                Category::Environment,
                "preview_cleanup_failed",
                "Preview was interrupted, but its owned containers could not be fully cleaned up.",
            )
            .with_details(json!({"interrupted":true,"cleanupComplete":false}))
            .with_recovery("miy", &["app", "dev-stop"])
            .into());
        }
        return Err(CliError::new(
            Category::Interrupted,
            "preview_interrupted",
            "Local preview was interrupted after its owned resources were cleaned up.",
        )
        .with_details(json!({"cleanupComplete":true,"databaseVolumePreserved":true}))
        .into());
    }
    result?;
    cleaned?;
    if reason == Shutdown::SignalUnavailable {
        return Err(CliError::new(
            Category::Environment,
            "preview_signal_unavailable",
            "The preview shutdown signal handler could not be initialized.",
        )
        .into());
    }
    Ok(())
}
async fn serve_preview(
    port: u16,
    no_open: bool,
    mut shutdown: tokio::sync::watch::Receiver<Shutdown>,
) -> Result<()> {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .map_err(|error| {
            let (category, code, message) = if error.kind() == std::io::ErrorKind::AddrInUse {
                (
                    Category::Conflict,
                    "preview_port_in_use",
                    "The requested local preview port is already in use.",
                )
            } else {
                (
                    Category::Environment,
                    "preview_listener_unavailable",
                    "The local preview listener could not be opened.",
                )
            };
            CliError::new(category, code, message).with_details(json!({"port":port}))
        })?;
    let port = listener.local_addr()?.port();
    let session = app_cli::session()?;
    let user = app_cli::api(&session, "/api/v1/me", None).await?;
    let m = app_cli::check()?;
    if *shutdown.borrow() != Shutdown::Running {
        return Ok(());
    }
    // This endpoint checks developer authority, even when the app has no deployed release yet.
    app_cli::api(
        &session,
        &format!("/api/v1/preview/apps/{}/platform/directory", m.id),
        None,
    )
    .await?;
    app_cli::run("pnpm", &["build:preview"])?;
    app_cli::save(
        "dist/build.json",
        &json!({"appId":m.id,"version":m.version,"sourceCommit":"0000000000000000000000000000000000000000","bundleVersion":crate::bundle::VERSION,"preview":true}),
    )?;
    let _ = std::fs::remove_file("dist/image.json"); // Preview is not a releasable build.
    let project = prefix()?;
    let image = format!("miy-preview/{}:local", &project[12..]);
    app_cli::run(
        "docker",
        &[
            "build",
            "--build-arg",
            &format!("MIY_APP_ID={}", m.id),
            "--build-arg",
            "MIY_SOURCE_COMMIT=0000000000000000000000000000000000000000",
            "-t",
            &image,
            ".",
        ],
    )?;
    if *shutdown.borrow() != Shutdown::Running {
        return Ok(());
    }
    remove_containers().await?;
    let result = start(&project, &image, &m).await;
    let (backend_port, secret, installation, release) = match result {
        Ok(v) => v,
        Err(e) => {
            let _ = remove_containers().await;
            return Err(e);
        }
    };
    if *shutdown.borrow() != Shutdown::Running {
        return Ok(());
    }
    let origin = format!("http://127.0.0.1:{port}");
    let state = Arc::new(Preview {
        session,
        manifest: m.clone(),
        user,
        installation,
        release,
        port: backend_port,
        secret,
        cookie: auth::random(),
        csrf: auth::random(),
        origin: origin.clone(),
        cookie_name: format!("miy_preview_{port}"),
    });
    let url = format!("{origin}/?preview={}", state.cookie);
    // Only the short-lived preview entry token appears locally; no platform credential does.
    app_cli::save(
        ".miy/preview.json",
        &json!({"url":url,"appId":m.id,"port":port}),
    )?;
    eprintln!(
        "{}",
        json!({"url":url,"appId":m.id,"database":"local-only","platformEnvironment":"dev"})
    );
    let app = Router::new()
        .route("/__miy/context", axum::routing::get(context))
        .route("/__miy/api/{*path}", axum::routing::any(backend))
        .route("/__miy/platform/directory", axum::routing::get(directory))
        .route(
            "/__miy/connections/{target}/api/{*path}",
            axum::routing::any(connection),
        )
        .fallback_service(
            ServeDir::new("dist/preview").fallback(ServeFile::new("dist/preview/index.html")),
        )
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state);
    if !no_open {
        let _ = crate::platform::open_browser(&url);
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown
                .wait_for(|reason| *reason != Shutdown::Running)
                .await;
        })
        .await?;
    Ok(())
}

async fn start(
    project: &str,
    image: &str,
    m: &contract::Manifest,
) -> Result<(i32, String, Uuid, Uuid)> {
    let path = PathBuf::from(".miy/local-db.json");
    let cfg: Value = if path.exists() {
        serde_json::from_slice(&std::fs::read(path)?)?
    } else {
        let v = json!({"admin":auth::random(),"migration":auth::random(),"runtime":auth::random()});
        app_cli::save(".miy/local-db.json", &v)?;
        v
    };
    if runtime::docker(&["network", "inspect", project])
        .await
        .is_err()
    {
        runtime::docker(&[
            "network",
            "create",
            "--label",
            &format!("org.miy.preview={project}"),
            project,
        ])
        .await?;
    }
    let env = std::env::current_dir()?.join(".miy/preview-db.env");
    private_env(
        &env,
        &format!(
            "POSTGRES_PASSWORD={}\n",
            cfg["admin"].as_str().context("invalid_local_db_config")?
        ),
    )?;
    let db = format!("{project}-db");
    let created = runtime::docker(&[
        "run",
        "-d",
        "--name",
        &db,
        "--label",
        &format!("org.miy.preview={project}"),
        "--network",
        project,
        "--network-alias",
        "postgres",
        "--publish",
        "127.0.0.1::5432",
        "--env-file",
        env.to_str().unwrap(),
        "--volume",
        &format!("{project}-data:/var/lib/postgresql"),
        "postgres:18.3@sha256:7e32e9833a6fb1c92c32552794cb6ed569d51b445a54907d35fc112ef39684db",
    ])
    .await;
    let _ = std::fs::remove_file(env);
    created?;
    let meta = runtime::inspect(&db).await?;
    let db_port = meta["NetworkSettings"]["Ports"]["5432/tcp"][0]["HostPort"]
        .as_str()
        .context("missing_preview_db_port")?;
    let base = format!(
        "postgres://postgres:{}@127.0.0.1:{db_port}/postgres",
        cfg["admin"].as_str().unwrap()
    );
    let mut conn = None;
    for _ in 0..60 {
        if let Ok(c) = sqlx::PgConnection::connect(&base).await {
            conn = Some(c);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    let mut conn = conn.context("local_database_not_ready")?;
    for (role, secret) in [
        ("miy_local_owner", None),
        ("miy_local_migration", cfg["migration"].as_str()),
        ("miy_local_runtime", cfg["runtime"].as_str()),
    ] {
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_roles WHERE rolname=$1)")
                .bind(role)
                .fetch_one(&mut conn)
                .await?;
        if !exists {
            let suffix = if let Some(p) = secret {
                ensure!(
                    p.bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
                    "invalid_local_secret"
                );
                format!("LOGIN PASSWORD '{p}'")
            } else {
                "NOLOGIN".into()
            };
            conn.execute(
                format!(
                    "CREATE ROLE {role} {suffix} NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION"
                )
                .as_str(),
            )
            .await?;
        }
    }
    conn.execute("GRANT miy_local_owner TO miy_local_migration; REVOKE ALL ON DATABASE postgres FROM PUBLIC; GRANT CONNECT ON DATABASE postgres TO miy_local_migration,miy_local_runtime; REVOKE ALL ON SCHEMA public FROM PUBLIC; CREATE SCHEMA IF NOT EXISTS app AUTHORIZATION miy_local_owner; GRANT USAGE ON SCHEMA app TO miy_local_runtime; ALTER DEFAULT PRIVILEGES FOR ROLE miy_local_owner IN SCHEMA app GRANT SELECT,INSERT,UPDATE,DELETE ON TABLES TO miy_local_runtime; ALTER DEFAULT PRIVILEGES FOR ROLE miy_local_owner IN SCHEMA app GRANT USAGE,SELECT ON SEQUENCES TO miy_local_runtime; ALTER ROLE miy_local_runtime IN DATABASE postgres SET search_path=app; ALTER ROLE miy_local_migration IN DATABASE postgres SET search_path=app;").await?;
    let migration_url = format!(
        "postgres://miy_local_migration:{}@127.0.0.1:{db_port}/postgres",
        cfg["migration"].as_str().unwrap()
    );
    let mut migrator = sqlx::PgConnection::connect(&migration_url).await?;
    migrator.execute("SET ROLE miy_local_owner").await?;
    sqlx::migrate::Migrator::new(std::path::Path::new(&m.migrations.directory))
        .await?
        .run(&mut migrator)
        .await?;
    migrator
        .execute("REVOKE ALL ON app._sqlx_migrations FROM miy_local_runtime")
        .await?;
    let secret = auth::random();
    let installation = Uuid::new_v4();
    let release = Uuid::new_v4();
    let env = std::env::current_dir()?.join(".miy/preview-app.env");
    private_env(
        &env,
        &format!(
            "DATABASE_URL=postgres://miy_local_runtime:{}@postgres:5432/postgres\nMIY_GATEWAY_SECRET={secret}\nMIY_INSTALLATION_ID={installation}\nMIY_RELEASE_ID={release}\n",
            cfg["runtime"].as_str().unwrap()
        ),
    )?;
    let name = format!("{project}-app");
    let created = runtime::docker(&[
        "run",
        "-d",
        "--name",
        &name,
        "--label",
        &format!("org.miy.preview={project}"),
        "--network",
        project,
        "--publish",
        "127.0.0.1::8080",
        "--env-file",
        env.to_str().unwrap(),
        "--user",
        "10001:10001",
        "--read-only",
        "--tmpfs",
        "/tmp:rw,noexec,nosuid,size=16m",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges",
        "--memory",
        "256m",
        "--cpus",
        "1",
        "--pids-limit",
        "128",
        image,
    ])
    .await;
    let _ = std::fs::remove_file(env);
    created?;
    let meta = runtime::inspect(&name).await?;
    let port: i32 = meta["NetworkSettings"]["Ports"]["8080/tcp"][0]["HostPort"]
        .as_str()
        .context("missing_preview_port")?
        .parse()?;
    runtime::healthy(port, release).await?;
    Ok((port, secret, installation, release))
}
fn private_env(path: &std::path::Path, value: &str) -> Result<()> {
    crate::platform::write_private(path, value.as_bytes())
}
async fn guard(State(s): State<Arc<Preview>>, req: Request, next: Next) -> Response {
    let expected = s.origin.strip_prefix("http://").unwrap();
    if req.headers().get("host").and_then(|h| h.to_str().ok()) != Some(expected) {
        return ApiError::forbidden().into_response();
    }
    if req.uri().path() == "/" && req.method() == axum::http::Method::GET {
        let valid = req
            .uri()
            .query()
            .map(|q| {
                url::form_urlencoded::parse(q.as_bytes())
                    .any(|(k, v)| k == "preview" && v == s.cookie)
            })
            .unwrap_or(false);
        if valid {
            let mut r = Redirect::to(&format!("/apps/{}", s.manifest.id)).into_response();
            r.headers_mut().insert(
                header::SET_COOKIE,
                format!(
                    "{}={}; HttpOnly; SameSite=Strict; Path=/",
                    s.cookie_name, s.cookie
                )
                .parse()
                .unwrap(),
            );
            r.headers_mut()
                .insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
            return r;
        }
    }
    if auth::cookie(req.headers(), &s.cookie_name).as_deref() != Some(&s.cookie) {
        return ApiError::unauthorized().into_response();
    }
    if !matches!(
        *req.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    ) && (req.headers().get("origin").and_then(|v| v.to_str().ok()) != Some(&s.origin)
        || req
            .headers()
            .get("x-miy-csrf")
            .and_then(|v| v.to_str().ok())
            != Some(&s.csrf))
    {
        return ApiError::forbidden().into_response();
    }
    let mut response = next.run(req).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}
async fn context(State(s): State<Arc<Preview>>) -> Json<Value> {
    Json(
        json!({"appId":s.manifest.id,"installationId":s.installation,"releaseId":s.release,"user":{"id":s.user["id"],"name":s.user["name"]},"csrf":s.csrf}),
    )
}
async fn directory(State(s): State<Arc<Preview>>) -> ApiResult<Json<Value>> {
    if !s
        .manifest
        .requested_permissions
        .contains(&"platform:directory:read".into())
    {
        return Err(ApiError::forbidden());
    }
    Ok(Json(
        app_cli::api(
            &s.session,
            &format!("/api/v1/preview/apps/{}/platform/directory", s.manifest.id),
            None,
        )
        .await?,
    ))
}
async fn backend(
    State(s): State<Arc<Preview>>,
    Path(path): Path<String>,
    req: Request,
) -> ApiResult<Response> {
    // Revalidate the actual CLI user on each API request; expiration/revocation is effective locally too.
    let user = app_cli::api(&s.session, "/api/v1/me", None)
        .await
        .map_err(|_| ApiError::unauthorized())?;
    let token = auth::sign_identity(
        user["id"].as_str().ok_or_else(ApiError::unauthorized)?,
        user["admin"] == true,
        s.installation,
        s.release,
        &s.secret,
    )?;
    forward(
        &s,
        &format!("http://127.0.0.1:{}/api/{path}", s.port),
        req,
        Some(token),
    )
    .await
}
async fn connection(
    State(s): State<Arc<Preview>>,
    Path((target, path)): Path<(String, String)>,
    req: Request,
) -> ApiResult<Response> {
    let access = if matches!(
        *req.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    ) {
        "read"
    } else {
        "write"
    };
    if !contract::valid_id(&target)
        || !s
            .manifest
            .requested_permissions
            .contains(&format!("app:{target}:{access}"))
    {
        return Err(ApiError::forbidden());
    }
    forward(
        &s,
        &format!(
            "{}/api/v1/preview/apps/{}/connections/{target}/api/{path}",
            s.session.origin, s.manifest.id
        ),
        req,
        None,
    )
    .await
}
async fn forward(
    s: &Preview,
    url: &str,
    req: Request,
    identity: Option<String>,
) -> ApiResult<Response> {
    if req.uri().path().contains('\\') || req.uri().path().split('/').any(|v| v == ".." || v == ".")
    {
        return Err(ApiError::bad("invalid_api_path"));
    }
    let url = format!(
        "{}{}",
        url,
        req.uri()
            .query()
            .map(|q| format!("?{q}"))
            .unwrap_or_default()
    );
    let method = req.method().clone();
    let content_type = req.headers().get(header::CONTENT_TYPE).cloned();
    let bytes = to_bytes(req.into_body(), 4 * 1024 * 1024)
        .await
        .map_err(|_| ApiError::bad("request_too_large"))?;
    let client = crate::cli_client::client(s.session.ca_file.as_deref(), 20)?;
    let mut request = client.request(method, url).body(bytes);
    if let Some(ct) = content_type {
        request = request.header(header::CONTENT_TYPE, ct)
    }
    request = if let Some(token) = identity {
        request.header("x-miy-identity", token)
    } else {
        crate::cli_client::authenticate(&s.session, request)
    };
    let response = request
        .send()
        .await
        .map_err(|_| ApiError::unavailable("preview_upstream_failed"))?;
    let status = response.status();
    let ct = response.headers().get(header::CONTENT_TYPE).cloned();
    let body = response
        .bytes()
        .await
        .map_err(|_| ApiError::unavailable("preview_response_failed"))?;
    let mut out = Response::builder().status(StatusCode::from_u16(status.as_u16()).unwrap());
    if let Some(ct) = ct {
        out = out.header(header::CONTENT_TYPE, ct)
    }
    Ok(out.body(Body::from(body)).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::{pending, ready};

    #[tokio::test]
    async fn cooperative_stop_and_interrupt_have_distinct_completion_reasons() {
        assert_eq!(
            shutdown_reason(pending(), ready(())).await,
            Shutdown::Requested
        );
        assert_eq!(
            shutdown_reason(ready(Ok(())), pending()).await,
            Shutdown::Interrupted
        );
        assert_eq!(
            shutdown_reason(
                ready(Err(std::io::Error::other("signal unavailable"))),
                pending()
            )
            .await,
            Shutdown::SignalUnavailable
        );
    }
}
