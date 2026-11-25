//! Local preview uses a server development database by default. Platform credentials stay in this gateway.
mod browser;
use crate::development_client::{DatabaseMode, Remote};
use crate::{
    api_error::{ApiError, ApiResult},
    app_cli::{self, Session},
    cli_error::{Category, CliError},
    contract, docker as runtime, identity as auth,
};
use anyhow::{Context, Result, ensure};
use axum::Extension;
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Path, Request, State},
    http::header,
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
};
use browser::{Browser, Sessions, VerifiedUser};
#[cfg(feature = "server")]
pub(crate) use browser::{challenge, valid_nonce};
use serde_json::{Value, json};
use sqlx::{Connection, Executor};
use std::{
    future::{Future, IntoFuture},
    path::PathBuf,
    sync::Arc,
};
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

#[derive(
    Clone, Copy, Debug, clap::ValueEnum, serde::Serialize, serde::Deserialize, PartialEq, Eq,
)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    Native,
    Docker,
}

fn saved_runtime() -> Result<Runtime> {
    match std::fs::read(".miy/preview-run.json") {
        Ok(bytes) => {
            let state: Value = serde_json::from_slice(&bytes).context("invalid_preview_owner")?;
            match state.get("runtime") {
                Some(value) => Ok(serde_json::from_value(value.clone())?),
                None => Ok(Runtime::Docker), // runners before native support
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Runtime::Native),
        Err(error) => Err(error.into()),
    }
}

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
    remove_owned(&format!("{name}-db"), &name).await?;
    remove_owned(&format!("{name}-tunnel"), &name).await
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
    let runtime = saved_runtime()?;
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
    if runtime == Runtime::Docker {
        remove_containers().await?;
    }
    crate::development_client::stop_saved().await?;
    remove_state();
    eprintln!(
        "Local preview stopped: gateway, backend, database tunnel and project lock released; database data retained."
    );
    Ok(())
}
#[derive(Clone)]
struct Preview {
    session: Session,
    manifest: contract::Manifest,
    sessions: Arc<Sessions>,
    installation: Uuid,
    release: Uuid,
    port: i32,
    secret: String,

    origin: String,
    cookie_name: String,
    backend_origin: String,
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

pub async fn run(port: u16, no_open: bool, database: DatabaseMode, runtime: Runtime) -> Result<()> {
    ensure!(
        runtime != Runtime::Native || database == DatabaseMode::Server,
        CliError::new(
            Category::Usage,
            "native_requires_server_database",
            "Native development uses the server development database. Use --runtime docker --database local only for a separate container database."
        )
    );
    let preview_lock = lock_file()?;
    preview_lock.try_lock().map_err(|_| {
        CliError::new(
            Category::Conflict,
            "preview_already_running_for_this_project",
            "This project already has an active preview runner.",
        )
        .with_recovery("miy", &["app", "dev-stop"])
    })?;
    ensure!(
        !std::path::Path::new(".miy/preview-run.json").exists() || saved_runtime()? == runtime,
        CliError::new(
            Category::Conflict,
            "preview_runtime_cleanup_required",
            "Stop the previous preview runtime before changing runtimes."
        )
        .with_recovery("miy", &["app", "dev-stop"])
    );
    remove_state();
    let run = Uuid::new_v4().to_string();
    app_cli::save(
        ".miy/preview-run.json",
        &json!({"protocol":1,"project":prefix()?,"runId":run,"pid":std::process::id(),"runtime":runtime}),
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
    let result = serve_preview(port, no_open, database, runtime, rx).await;
    watcher.abort();
    let reason = *observed.borrow();
    let cleaned = if runtime == Runtime::Docker {
        remove_containers().await
    } else {
        Ok(())
    };
    // Preserve uncertain requests for recovery without creating duplicate work.
    let uncertain = reason == Shutdown::Running
        && result
            .as_ref()
            .err()
            .is_some_and(|error| crate::cli_error::classify(error).category == Category::Network);
    let released = if uncertain {
        Ok(())
    } else {
        crate::development_client::stop_saved().await
    };
    remove_state();
    if reason == Shutdown::Interrupted {
        if cleaned.is_err() || released.is_err() {
            return Err(CliError::new(
                Category::Environment,
                "preview_cleanup_failed",
                "Preview was interrupted, but its owned resources could not be fully cleaned up.",
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
    if reason != Shutdown::Requested {
        result?;
    }
    cleaned?;
    released?;
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
pub(crate) async fn startup_installation(
    session: &Session,
    app: &str,
    subject: &str,
) -> Result<Uuid> {
    let context: crate::api_contract::PreviewContext = serde_json::from_value(
        app_cli::api(
            session,
            &format!("/api/v1/preview/apps/{app}/browser/context"),
            None,
        )
        .await?,
    )?;
    ensure!(
        context.environment == crate::runtime_contract::Environment::Dev
            && context.app_id == app
            && context.subject_id == subject,
        "invalid_preview_context"
    );
    Ok(context.installation_id)
}

async fn serve_preview(
    port: u16,
    no_open: bool,
    database: DatabaseMode,
    runtime: Runtime,
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
    if database == DatabaseMode::Server && runtime == Runtime::Docker {
        crate::development_client::validate_origin(&session.origin)?;
    }
    let user = app_cli::api(&session, "/api/v1/me", None).await?;
    let m = app_cli::check_project()?;
    if *shutdown.borrow() != Shutdown::Running {
        return Ok(());
    }
    let installation = startup_installation(
        &session,
        &m.id,
        user["id"].as_str().context("missing_preview_subject")?,
    )
    .await?;
    app_cli::run("pnpm", &["install", "--frozen-lockfile"])?;
    // The Dockerfile copies the packaged UI. Native preview serves the
    // standalone preview bundle; node-profile apps may also build a sidecar.
    if runtime == Runtime::Docker || m.runtime.profile == "rust-web-node-v1" {
        app_cli::run("pnpm", &["build"])?;
        crate::cli_error::validation(
            crate::bundle::ui(std::path::Path::new("dist/ui")),
            "ui_invalid",
        )?;
    }
    app_cli::run("pnpm", &["build:preview"])?;
    app_cli::save(
        "dist/build.json",
        &json!({"appId":m.id,"version":m.version,"sourceCommit":"0000000000000000000000000000000000000000","bundleVersion":crate::bundle::VERSION,"preview":true}),
    )?;
    let _ = std::fs::remove_file("dist/image.json"); // Preview is not a releasable build.
    let project = prefix()?;
    let image = format!("miy-preview/{}:local", &project[12..]);
    let executable = if runtime == Runtime::Native {
        Some(crate::native_preview::build()?)
    } else {
        let app_id = format!("MIY_APP_ID={}", m.id);
        let mut build = vec!["build"];
        if m.runtime.profile == "rust-web-node-v1" {
            build.extend(["--target", "rust-web-node-v1"]);
        }
        build.extend([
            "--build-arg",
            &app_id,
            "--build-arg",
            "MIY_SOURCE_COMMIT=0000000000000000000000000000000000000000",
            "-t",
            &image,
            ".",
        ]);
        app_cli::run("docker", &build)?;
        None
    };
    if *shutdown.borrow() != Shutdown::Running {
        return Ok(());
    }
    if runtime == Runtime::Docker {
        remove_containers().await?;
        ensure_network(&project).await?;
    }
    let remote = if database == DatabaseMode::Server {
        Some(
            Remote::start(
                &session,
                &m,
                &project,
                || *shutdown.borrow() != Shutdown::Running,
                runtime,
            )
            .await?,
        )
    } else {
        None
    };
    // The backend listener is reachable only on loopback or this project's own
    // Docker bridge gateway. No all-interface listener or browser credential is used.
    let gateway = if runtime == Runtime::Docker {
        Some(preview_bridge_address(&project).await?)
    } else {
        None
    };
    let (backend_listener, backend_reachable) = bind_backend(gateway).await?;
    if !backend_reachable {
        eprintln!(
            "{}",
            json!({"notice":"preview_platform_backend_unavailable","detail":"This Docker engine keeps its bridge gateway off the host (Docker Desktop or rootless). The preview runs, but backend platform directory/file calls are refused; use the default native runtime for them."})
        );
    }
    let backend_origin = format!("http://{}", backend_listener.local_addr()?);
    let serving = async {
        let database_url = if let Some(ref remote) = remote {
            remote.database_url.clone()
        } else {
            local_database(&project, &m).await?
        };
        let mut native = if let Some(ref executable) = executable {
            Some(
                crate::native_preview::Backend::start(
                    executable,
                    &database_url,
                    installation,
                    &backend_origin,
                )
                .await?,
            )
        } else {
            None
        };
        let result = if let Some(ref backend) = native {
            Ok((
                backend.port,
                backend.secret.clone(),
                backend.installation,
                backend.release,
            ))
        } else {
            start_app(
                &project,
                &image,
                &database_url,
                installation,
                backend_reachable.then_some(backend_origin.as_str()),
            )
            .await
        };
        let (backend_port, secret, installation, release) = match result {
            Ok(v) => v,
            Err(e) => {
                if runtime == Runtime::Docker {
                    let _ = remove_containers().await;
                }
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
            sessions: Arc::default(),
            installation,
            release,
            port: backend_port,
            secret,
            origin: origin.clone(),
            cookie_name: format!("miy_preview_{port}"),
            backend_origin,
        });
        let url = format!("{origin}/");
        app_cli::save(
            ".miy/preview.json",
            &json!({"url":url,"appId":m.id,"port":port,"runtime":runtime,"backendPort":backend_port,"databaseId":remote.as_ref().map(|r|r.database_id)}),
        )?;
        eprintln!(
            "{}",
            json!({"url":url,"appId":m.id,"runtime":runtime,"database":if remote.is_some() { "server-dev" } else { "local-only" },"platformEnvironment":"dev","databaseId":remote.as_ref().map(|r|r.database_id),"schemaFingerprint":remote.as_ref().map(|r|r.schema_fingerprint.as_str())})
        );
        let app = browser_router(state.clone(), std::path::Path::new("dist/preview"));
        let backend_app = backend_router(state);
        let mut backend_shutdown = shutdown.clone();
        let backend_server = axum::serve(backend_listener, backend_app)
            .with_graceful_shutdown(async move {
                let _ = backend_shutdown
                    .wait_for(|reason| *reason != Shutdown::Running)
                    .await;
            })
            .into_future();
        if !no_open {
            let _ = crate::platform::open_browser(&url);
        }
        let server = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown
                    .wait_for(|reason| *reason != Shutdown::Running)
                    .await;
            })
            .into_future();
        let servers = async {
            tokio::try_join!(server, backend_server)
                .map(|_| ())
                .map_err(anyhow::Error::from)
        };
        let result = if let Some(ref mut backend) = native {
            tokio::select! { result = servers => result, result = backend.exited() => result }
        } else {
            servers.await
        };
        if let Some(ref mut backend) = native {
            backend.stop().await?;
        }
        result?;
        Ok::<(), anyhow::Error>(())
    };
    if let Some(ref remote) = remote {
        tokio::select! {
            result = serving => result?,
            result = remote.maintain() => result?,
        }
    } else {
        serving.await?;
    }
    Ok(())
}

fn backend_router(state: Arc<Preview>) -> Router {
    let backend_routes = Router::new()
        .route(
            "/connections/{target}/api/{*path}",
            axum::routing::any(connection),
        )
        .route("/platform/directory", axum::routing::get(directory))
        .route("/platform/files", axum::routing::any(platform_files_root))
        .route(
            "/platform/files/{*path}",
            axum::routing::any(platform_files),
        )
        .route(
            "/platform/notifications",
            axum::routing::post(preview_notify),
        )
        .route(
            "/platform/notifications/retract",
            axum::routing::post(preview_retract),
        )
        .route(
            "/platform/notifications/read",
            axum::routing::post(preview_mark_read),
        )
        .route(
            "/platform/notifications/by-request/{request}",
            axum::routing::get(preview_notification_by_request),
        )
        .layer(middleware::from_fn_with_state(state.clone(), backend_guard));
    Router::new()
        .nest(
            &format!("/api/v1/installations/{}", state.installation),
            backend_routes,
        )
        .with_state(state)
}

fn browser_router(state: Arc<Preview>, dist: &std::path::Path) -> Router {
    Router::new()
        .route("/__miy/context", axum::routing::get(context))
        .route("/__miy/api/{*path}", axum::routing::any(backend))
        .route("/__miy/files/{*path}", axum::routing::any(files))
        .route("/__miy/platform/directory", axum::routing::get(directory))
        .route("/__miy/live", axum::routing::get(preview_live))
        .route(
            "/__miy/connections/{target}/api/{*path}",
            axum::routing::any(connection),
        )
        .fallback_service(ServeDir::new(dist).fallback(ServeFile::new(dist.join("index.html"))))
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state)
}

async fn ensure_network(project: &str) -> Result<()> {
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
    } else {
        let network: Value =
            serde_json::from_str(&runtime::docker(&["network", "inspect", project]).await?)?;
        ensure!(
            network[0]["Labels"]["org.miy.preview"] == project,
            "unmanaged_preview_network"
        );
    }
    Ok(())
}

/// Whether `address` is assigned to this host. A successful bind cannot tell:
/// with `net.ipv4.ip_nonlocal_bind=1` the kernel accepts any address, and the
/// listener would then sit on an address no container can reach. Connecting a
/// UDP socket sends nothing; the kernel only chooses the source address, and for
/// one of its own addresses that source is the address itself.
fn host_address(address: std::net::Ipv4Addr) -> bool {
    std::net::UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|socket| {
            socket.connect((address, 9))?;
            socket.local_addr()
        })
        .is_ok_and(|local| local.ip() == address)
}

/// Bind the private backend listener. A native backend uses loopback. A
/// container reaches the host through its bridge gateway, which is a host
/// address only with a native Linux engine. Docker Desktop and rootless engines
/// keep it inside their own network: the listener then stays on loopback and
/// the result is `false`, so the preview starts without backend platform calls
/// instead of failing outright.
async fn bind_backend(
    gateway: Option<std::net::Ipv4Addr>,
) -> Result<(tokio::net::TcpListener, bool)> {
    if let Some(address) = gateway.filter(|address| host_address(*address)) {
        match tokio::net::TcpListener::bind((address, 0)).await {
            Ok(listener) => return Ok((listener, true)),
            Err(error) if error.kind() == std::io::ErrorKind::AddrNotAvailable => {}
            Err(error) => return Err(error).context("preview_backend_listener_unavailable"),
        }
    }
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .context("preview_backend_listener_unavailable")?;
    Ok((listener, gateway.is_none()))
}

async fn preview_bridge_address(project: &str) -> Result<std::net::Ipv4Addr> {
    let network: Value =
        serde_json::from_str(&runtime::docker(&["network", "inspect", project]).await?)?;
    ensure!(
        network[0]["Labels"]["org.miy.preview"] == project,
        "unmanaged_preview_network"
    );
    let address = network[0]["IPAM"]["Config"]
        .as_array()
        .and_then(|values| {
            values
                .iter()
                .filter_map(|value| {
                    value["Gateway"]
                        .as_str()?
                        .parse::<std::net::Ipv4Addr>()
                        .ok()
                })
                .find(std::net::Ipv4Addr::is_private)
        })
        .context("preview_bridge_gateway_unavailable")?;
    Ok(address)
}

async fn local_database(project: &str, m: &contract::Manifest) -> Result<String> {
    let path = PathBuf::from(".miy/local-db.json");
    let cfg: Value = if path.exists() {
        serde_json::from_slice(&std::fs::read(path)?)?
    } else {
        let v = json!({"admin":auth::random(),"migration":auth::random(),"runtime":auth::random()});
        app_cli::save(".miy/local-db.json", &v)?;
        v
    };
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
    Ok(format!(
        "postgres://miy_local_runtime:{}@postgres:5432/postgres",
        cfg["runtime"].as_str().unwrap()
    ))
}

async fn start_app(
    project: &str,
    image: &str,
    database_url: &str,
    installation: Uuid,
    platform: Option<&str>,
) -> Result<(i32, String, Uuid, Uuid)> {
    let secret = auth::random();
    let release = Uuid::new_v4();
    let env = std::env::current_dir()?.join(".miy/preview-app.env");
    // Without a reachable backend listener the SDK reports that platform calls
    // are unavailable instead of waiting on an address inside the container.
    let platform = platform
        .map(|origin| format!("MIY_CORE_ORIGIN={origin}\nMIY_CORE_ALLOW_HTTP=1\n"))
        .unwrap_or_default();
    private_env(
        &env,
        &format!(
            "DATABASE_URL={database_url}\nMIY_GATEWAY_SECRET={secret}\nMIY_INSTALLATION_ID={installation}\nMIY_RELEASE_ID={release}\n{platform}"
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
    browser::handle(s, req, next)
        .await
        .unwrap_or_else(IntoResponse::into_response)
}
fn backend_actor(s: &Preview, req: &Request) -> Option<Browser> {
    let headers = req.headers();
    if headers.contains_key(header::ORIGIN)
        || headers.contains_key(header::COOKIE)
        || headers.get(header::HOST)?.to_str().ok()? != s.backend_origin.strip_prefix("http://")?
        || headers.get("x-miy-release")?.to_str().ok()? != s.release.to_string()
    {
        return None;
    }
    s.sessions.backend(
        headers
            .get(header::AUTHORIZATION)?
            .to_str()
            .ok()?
            .strip_prefix("Bearer ")?,
    )
}
async fn backend_guard(State(s): State<Arc<Preview>>, mut req: Request, next: Next) -> Response {
    let Some(actor) = backend_actor(&s, &req) else {
        return ApiError::forbidden().into_response();
    };
    if let Err(e) = actor.user(&s).await {
        return e.into_response();
    }
    req.extensions_mut().insert(actor);
    let mut response = next.run(req).await;
    crate::file_transfer::secure_headers(response.headers_mut());
    response
}

async fn context(
    State(s): State<Arc<Preview>>,
    Extension(browser): Extension<Browser>,
    Extension(user): Extension<VerifiedUser>,
) -> Json<Value> {
    Json(
        json!({"appId":s.manifest.id,"installationId":s.installation,"releaseId":s.release,"user":user.0,"csrf":browser.csrf}),
    )
}

fn signed(s: &Preview, browser: &Browser, user: &Value) -> ApiResult<String> {
    auth::sign_identity(
        user["id"].as_str().ok_or_else(ApiError::unauthorized)?,
        false,
        s.installation,
        s.release,
        &s.secret,
        s.sessions.continuation(browser)?,
    )
}
fn identity_for(s: &Preview, req: &Request) -> ApiResult<String> {
    signed(
        s,
        req.extensions()
            .get::<Browser>()
            .ok_or_else(ApiError::unauthorized)?,
        &req.extensions()
            .get::<VerifiedUser>()
            .ok_or_else(ApiError::unauthorized)?
            .0,
    )
}
async fn directory(State(s): State<Arc<Preview>>, req: Request) -> ApiResult<Response> {
    if !s
        .manifest
        .requested_permissions
        .contains(&"platform:directory:read".into())
    {
        return Err(ApiError::forbidden());
    }
    forward(
        &s,
        &format!(
            "{}/api/v1/preview/apps/{}/platform/directory",
            s.session.origin, s.manifest.id
        ),
        req,
        None,
    )
    .await
}
async fn backend(
    State(s): State<Arc<Preview>>,
    Path(path): Path<String>,
    req: Request,
) -> ApiResult<Response> {
    let token = identity_for(&s, &req)?;
    forward(
        &s,
        &format!("http://127.0.0.1:{}/api/{path}", s.port),
        req,
        Some(token),
    )
    .await
}
/// The same portal session is rechecked while this socket stays open.
async fn preview_live(
    State(s): State<Arc<Preview>>,
    Extension(browser): Extension<Browser>,
    Extension(user): Extension<VerifiedUser>,
    headers: axum::http::HeaderMap,
    upgrade: axum::extract::ws::WebSocketUpgrade,
) -> Response {
    if headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) != Some(s.origin.as_str()) {
        return ApiError::forbidden().into_response();
    }
    upgrade
        .max_message_size(crate::live::MAX_MESSAGE_BYTES)
        .max_frame_size(crate::live::MAX_MESSAGE_BYTES)
        .on_upgrade(move |mut socket| async move {
            use crate::live::{close, close_with};
            let identity = match signed(&s, &browser, &user.0) {
                Ok(token) => token,
                Err(_) => {
                    close_with(&mut socket, close::SESSION_ENDED, "session_ended").await;
                    return;
                }
            };
            let upstream = async {
                use tokio_tungstenite::tungstenite::client::IntoClientRequest;
                let mut request = format!("ws://127.0.0.1:{}/api/live", s.port)
                    .into_client_request()
                    .ok()?;
                request
                    .headers_mut()
                    .insert("x-miy-identity", identity.parse().ok()?);
                tokio_tungstenite::connect_async(request)
                    .await
                    .ok()
                    .map(|(stream, _)| stream)
            };
            let Some(upstream) = upstream.await else {
                close_with(&mut socket, close::APP_UNAVAILABLE, "app_unavailable").await;
                return;
            };
            if socket
                .send(axum::extract::ws::Message::Text(
                    r#"{"type":"miy.ready"}"#.into(),
                ))
                .await
                .is_err()
            {
                return;
            }
            let state = s.clone();
            crate::live::relay(socket, upstream, move || {
                let state = state.clone();
                let browser = browser.clone();
                async move {
                    browser
                        .user(&state)
                        .await
                        .map(|_| ())
                        .map_err(|_| crate::live::Ended::Session)
                }
            })
            .await;
        })
}

/// Browser file bytes go only to the owning app; that backend checks the
/// current resource/file ACL before using the process-only Storage capability.
async fn files(
    State(s): State<Arc<Preview>>,
    Path(path): Path<String>,
    req: Request,
) -> ApiResult<Response> {
    let token = identity_for(&s, &req)?;
    let url = file_url(
        &format!("http://127.0.0.1:{}", s.port),
        &["api", "files"],
        &path,
        req.uri().query(),
    )?;
    file_forward(&s, url, req, Some(token)).await
}

fn file_url(
    origin: &str,
    prefix: &[&str],
    path: &str,
    query: Option<&str>,
) -> ApiResult<reqwest::Url> {
    if path.contains('\\')
        || path.split('/').any(|part| matches!(part, "." | ".."))
        || path.chars().any(char::is_control)
    {
        return Err(ApiError::bad("invalid_file_api_path"));
    }
    let mut url = reqwest::Url::parse(origin)?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| ApiError::bad("invalid_file_api_path"))?;
        segments.clear();
        for segment in prefix {
            segments.push(segment);
        }
        if !path.is_empty() {
            for segment in path.split('/') {
                segments.push(segment);
            }
        }
    }
    url.set_query(query);
    Ok(url)
}

/// Local preview has no inbox: notification calls are checked like the
/// platform checks them and answered as delivered to nobody, with a notice on
/// stderr, so the app's own code path still runs.
fn preview_notifications_allowed(s: &Preview) -> ApiResult<()> {
    if s.manifest
        .requested_permissions
        .iter()
        .any(|permission| permission == "platform:notifications:write")
    {
        Ok(())
    } else {
        Err(ApiError::forbidden())
    }
}
async fn preview_notify(
    State(s): State<Arc<Preview>>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    preview_notifications_allowed(&s)?;
    let request = body["requestId"]
        .as_str()
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .ok_or_else(|| ApiError::bad("invalid_notification"))?;
    let recipients = body["recipients"].as_array().map_or(0, Vec::len);
    if recipients == 0 || recipients > 500 {
        return Err(ApiError::bad("invalid_notification"));
    }
    eprintln!(
        "{}",
        json!({"preview":"notification_not_delivered","requestId":request,"kind":body["kind"],"groupKey":body["groupKey"],"recipients":recipients})
    );
    Ok(Json(json!({"requestId":request,"accepted":0})))
}
async fn preview_retract(
    State(s): State<Arc<Preview>>,
    Json(_): Json<Value>,
) -> ApiResult<Json<Value>> {
    preview_notifications_allowed(&s)?;
    Ok(Json(json!({"retracted":0})))
}
async fn preview_mark_read(
    State(s): State<Arc<Preview>>,
    Json(_): Json<Value>,
) -> ApiResult<axum::http::StatusCode> {
    preview_notifications_allowed(&s)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
async fn preview_notification_by_request(
    State(s): State<Arc<Preview>>,
    Path(request): Path<uuid::Uuid>,
) -> ApiResult<Json<Value>> {
    preview_notifications_allowed(&s)?;
    Ok(Json(json!({"requestId":request,"accepted":0})))
}

async fn platform_files_root(
    State(s): State<Arc<Preview>>,
    request: Request,
) -> ApiResult<Response> {
    platform_files_request(&s, "", request).await
}
async fn platform_files(
    State(s): State<Arc<Preview>>,
    Path(path): Path<String>,
    request: Request,
) -> ApiResult<Response> {
    platform_files_request(&s, &path, request).await
}
/// `path` is what follows `/platform/files/`. Core's own Storage route list
/// decides, so the local proxy cannot drift from it.
fn platform_file_method(path: &str, method: &axum::http::Method) -> bool {
    let suffix = if path.is_empty() {
        "platform/files".to_owned()
    } else {
        format!("platform/files/{path}")
    };
    crate::storage_contract::allowed_route(&suffix, method)
}
async fn platform_files_request(s: &Preview, path: &str, request: Request) -> ApiResult<Response> {
    if !platform_file_method(path, request.method()) || request.uri().query().is_some() {
        return Err(ApiError::bad("invalid_file_api_path"));
    }
    let access = if request.method() == axum::http::Method::GET {
        "read"
    } else {
        "write"
    };
    if !s
        .manifest
        .requested_permissions
        .contains(&format!("platform:files:{access}"))
    {
        return Err(ApiError::forbidden());
    }
    let url = file_url(
        &s.session.origin,
        &[
            "api",
            "v1",
            "preview",
            "apps",
            &s.manifest.id,
            "platform",
            "files",
        ],
        path,
        None,
    )?;
    file_forward(s, url, request, None).await
}
async fn file_forward(
    s: &Preview,
    url: reqwest::Url,
    request: Request,
    token: Option<String>,
) -> ApiResult<Response> {
    let slot = if token.is_some() {
        crate::file_transfer::acquire()?
    } else {
        crate::file_transfer::acquire_backend()?
    };
    crate::file_transfer::check_length(request.headers())?;
    let (parts, body) = request.into_parts();
    let client = if token.is_some() {
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(std::time::Duration::from_secs(
                crate::storage_contract::TRANSFER_SECONDS,
            ))
            .build()
            .map_err(|_| ApiError::unavailable("preview_upstream_failed"))?
    } else {
        crate::cli_client::client(
            s.session.ca_file.as_deref(),
            crate::storage_contract::TRANSFER_SECONDS,
        )?
    };
    let mut target = client
        .request(parts.method, url)
        .body(crate::file_transfer::request_body(body));
    for name in [header::CONTENT_TYPE, header::CONTENT_LENGTH] {
        if let Some(value) = parts.headers.get(&name) {
            target = target.header(name, value);
        }
    }
    target = if let Some(token) = token {
        target.header("x-miy-identity", token)
    } else {
        parts
            .extensions
            .get::<Browser>()
            .ok_or_else(ApiError::unauthorized)?
            .authenticate(s, target)
    };
    let upstream = target
        .send()
        .await
        .map_err(|_| ApiError::unavailable("preview_upstream_failed"))?;
    crate::file_transfer::response(upstream, slot)
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
    let browser = req.extensions().get::<Browser>().cloned();
    let bytes = to_bytes(req.into_body(), 4 * 1024 * 1024)
        .await
        .map_err(|_| ApiError::bad("request_too_large"))?;
    let client = if identity.is_some() {
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|_| ApiError::unavailable("preview_upstream_failed"))?
    } else {
        crate::cli_client::client(s.session.ca_file.as_deref(), 20)?
    };
    let mut request = client.request(method, url).body(bytes);
    if let Some(ct) = content_type {
        request = request.header(header::CONTENT_TYPE, ct)
    }
    request = if let Some(token) = identity {
        request.header("x-miy-identity", token)
    } else {
        browser
            .ok_or_else(ApiError::unauthorized)?
            .authenticate(s, request)
    };
    let response = request
        .send()
        .await
        .map_err(|_| ApiError::unavailable("preview_upstream_failed"))?;
    let status = response.status();
    let ct = response.headers().get(header::CONTENT_TYPE).cloned();
    let body = crate::app_response::read(response, "preview").await?;
    Ok(crate::app_response::api(status, ct, body))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::future::{pending, ready};

    #[cfg(feature = "server")]
    pub(crate) async fn browser_acceptance(
        core: &crate::State,
        cli: &Session,
        manifest: &contract::Manifest,
        installation: Uuid,
    ) -> Result<()> {
        let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let gateway = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let private = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let origin = format!("http://{}", gateway.local_addr()?);
        let mut s = test_preview();
        s.session = cli.clone();
        s.installation = installation;
        s.manifest = manifest.clone();
        s.manifest
            .requested_permissions
            .push("platform:directory:read".into());
        s.port = i32::from(socket.local_addr()?.port());
        s.origin = origin.clone();
        s.backend_origin = format!("http://{}", private.local_addr()?);
        let s = Arc::new(s);
        let business=Router::new().route("/api/who",axum::routing::get(|State(s):State<Arc<Preview>>,headers:axum::http::HeaderMap| async move {
            let mut validation=jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
            validation.set_audience(&["miy-app"]);
            validation.set_issuer(&["miy"]);
            let token=headers["x-miy-identity"].to_str().unwrap();
            let user=jsonwebtoken::decode::<Value>(token,&jsonwebtoken::DecodingKey::from_secret(s.secret.as_bytes()),&validation).unwrap().claims;
            assert_eq!(user["installation_id"],s.installation.to_string());
            let response=reqwest::Client::new().get(format!("{}/api/v1/installations/{}/platform/directory",s.backend_origin,s.installation))
                .bearer_auth(user["delegation_token"].as_str().unwrap()).header("x-miy-release",user["release_id"].as_str().unwrap())
                .send().await.unwrap();
            assert!(response.status().is_success(),"backend directory {}",response.status());
            let page:Value=response.json().await.unwrap();
            Json(json!({"id":user["sub"],"admin":user["admin"],"directoryUser":page["subjectId"]}))
        })).with_state(s.clone());
        let dist = tempfile::tempdir()?;
        std::fs::write(
            dist.path().join("index.html"),
            "<!doctype html><title>Local preview test</title>",
        )?;
        let browser = browser_router(s.clone(), dist.path());
        let backend = backend_router(s.clone());
        let servers = [
            tokio::spawn(async move { axum::serve(socket, business).await }),
            tokio::spawn(async move { axum::serve(private, backend).await }),
            tokio::spawn(async move { axum::serve(gateway, browser).await }),
        ];
        let test = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/check-preview-browser.mjs");
        let status = tokio::process::Command::new("node")
            .arg(test)
            .arg(origin)
            .arg(&core.auth.dev_portal_origin)
            .status()
            .await?;
        for server in servers {
            server.abort();
        }
        ensure!(status.success(), "preview browser acceptance failed");
        Ok(())
    }

    fn test_preview() -> Preview {
        Preview {
            session: serde_json::from_value(json!({"origin":"http://127.0.0.1:1"})).unwrap(),
            manifest: serde_json::from_value(json!({"contractVersion":contract::VERSION,"id":"test-app","name":"test","version":"1.0.0","runtime":{"profile":"rust-web-v1"},"ui":{"delivery":"module","moduleEntry":"mf-manifest.json","moduleExport":"./AppRoot","contractVersion":"miy.ui/v1alpha1"},"resources":{"database":"postgresql"},"migrations":{"directory":"migrations"}})).unwrap(),
            sessions:Arc::default(), installation:Uuid::new_v4(), release:Uuid::new_v4(), port:1,
            secret:"unused".into(),origin:"http://127.0.0.1:2".into(),cookie_name:"preview-test".into(),
            backend_origin:"http://127.0.0.1:3".into(),
        }
    }
    #[tokio::test]
    async fn directory_query_preserves_the_release_permission_refusal() {
        let mut preview = test_preview();
        let request = || {
            Request::builder()
                .extension(Browser::default())
                .uri("/platform/directory?limit=1")
                .body(axum::body::Body::empty())
                .unwrap()
        };
        let denied = directory(State(Arc::new(preview.clone())), request())
            .await
            .unwrap_err();
        assert_eq!(denied.status, axum::http::StatusCode::FORBIDDEN);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        preview.session.origin = format!("http://{}", listener.local_addr().unwrap());
        preview
            .manifest
            .requested_permissions
            .push("platform:directory:read".into());
        let upstream = Router::new().route("/api/v1/preview/apps/test-app/platform/directory",
            axum::routing::get(|req: Request| async move {
                assert_eq!(req.uri().query(), Some("limit=1"));
                (axum::http::StatusCode::FORBIDDEN, Json(json!({"error":{"code":"installation_permission_denied","requestId":"fixture-request"}})))
            }));
        let server = tokio::spawn(async move { axum::serve(listener, upstream).await });
        let response = directory(State(Arc::new(preview)), request())
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        let error: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(error["error"]["requestId"], "fixture-request");
        server.abort();
    }

    #[test]
    fn backend_proofs_are_bound_to_each_browser_and_release() {
        let s = test_preview();
        let alice = Browser::fixture("alice");
        let bob = Browser::fixture("bob");
        let a = s.sessions.continuation(&alice).unwrap();
        let b = s.sessions.continuation(&bob).unwrap();
        let request = |proof: &str| {
            Request::builder()
                .header("host", "127.0.0.1:3")
                .header("authorization", format!("Bearer {proof}"))
                .header("x-miy-release", s.release.to_string())
                .body(axum::body::Body::empty())
                .unwrap()
        };
        assert_eq!(backend_actor(&s, &request(&a)).unwrap().csrf, "alice");
        assert_eq!(backend_actor(&s, &request(&b)).unwrap().csrf, "bob");
        assert!(backend_actor(&s, &request("alice")).is_none());
        for (header, value) in [
            ("origin", "null"),
            ("cookie", "a=b"),
            ("host", "127.0.0.1:4"),
            ("x-miy-release", "wrong"),
        ] {
            let mut req = request(&a);
            req.headers_mut().insert(header, value.parse().unwrap());
            assert!(backend_actor(&s, &req).is_none());
        }
    }
    #[test]
    fn file_proxy_keeps_decoded_path_characters_out_of_origin_and_query() {
        let url = file_url(
            "http://127.0.0.1:3",
            &["api", "files"],
            "a?overwrite=true#fragment",
            Some("download=1"),
        )
        .unwrap();
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        assert_eq!(url.path(), "/api/files/a%3Foverwrite=true%23fragment");
        assert_eq!(url.query(), Some("download=1"));
        assert!(url.fragment().is_none());
        for path in ["../directory", "id/../../admin", "id\\content"] {
            assert!(file_url("http://127.0.0.1:3", &["api", "files"], path, None).is_err());
        }
    }
    #[test]
    fn preview_platform_file_methods_are_a_closed_allowlist() {
        use axum::http::Method;
        let id = "00000000-0000-4000-8000-000000000001";
        assert!(platform_file_method("", &Method::POST));
        assert!(platform_file_method(&format!("{id}/content"), &Method::PUT));
        assert!(platform_file_method(&format!("{id}/content"), &Method::GET));
        assert!(platform_file_method(
            &format!("by-request/{id}"),
            &Method::GET
        ));
        for path in ["../directory", "directory", "admin", "by-request/not-uuid"] {
            assert!(!platform_file_method(path, &Method::GET));
        }
        assert!(!platform_file_method(
            &format!("{id}/content"),
            &Method::POST
        ));
        assert!(!platform_file_method(id, &Method::PATCH));
        assert!(platform_file_method(&format!("{id}/detach"), &Method::POST));
    }

    #[tokio::test]
    async fn backend_listener_falls_back_when_the_gateway_is_not_a_host_address() {
        let (listener, reachable) = bind_backend(None).await.unwrap();
        assert!(reachable && listener.local_addr().unwrap().ip().is_loopback());
        // TEST-NET-1 is never assigned to a local interface, like a bridge
        // gateway that exists only inside Docker Desktop's own network.
        let gateway = std::net::Ipv4Addr::new(192, 0, 2, 1);
        let (listener, reachable) = bind_backend(Some(gateway)).await.unwrap();
        assert!(!reachable && listener.local_addr().unwrap().ip().is_loopback());
        assert!(host_address(std::net::Ipv4Addr::LOCALHOST) && !host_address(gateway));
    }

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
