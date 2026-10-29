//! Server development database lifecycle owned by a local preview runner.
use crate::{
    app_cli::{self, Session},
    cli_error::{Category, CliError},
    contract, docker,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};
use uuid::Uuid;

const STATE: &str = ".miy/remote-dev.json";
#[derive(Clone, Copy, Debug, clap::ValueEnum, PartialEq, Eq)]
pub enum DatabaseMode {
    Server,
    Local,
}

pub fn validate_origin(origin: &str) -> Result<()> {
    let parsed = url::Url::parse(origin)?;
    let container_local = match parsed.host() {
        Some(url::Host::Domain(host)) => {
            let host = host.trim_end_matches('.');
            host == "localhost" || host.ends_with(".localhost")
        }
        Some(url::Host::Ipv4(ip)) => ip.is_loopback() || ip.is_unspecified(),
        Some(url::Host::Ipv6(ip)) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip
                    .to_ipv4_mapped()
                    .is_some_and(|ip| ip.is_loopback() || ip.is_unspecified())
        }
        None => true,
    };
    ensure!(
        !container_local,
        CliError::new(
            Category::Environment,
            "server_development_origin_unreachable",
            "Server database development needs a server IP or domain reachable from Docker. Loopback addresses point to the helper container itself. Register a reachable server URL; use --database local only for a separate local database."
        )
    );
    Ok(())
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    app_id: String,
    request_id: Uuid,
    payload_hash: String,
    origin: String,
    context: Option<String>,
    user_id: Option<String>,
    id: Option<Uuid>,
}

pub struct Remote {
    pub id: Uuid,
    pub database_id: Uuid,
    pub schema_fingerprint: String,
    pub database_url: String,
    session: Session,
    native_tunnel: Option<NativeTunnel>,
}

struct NativeTunnel(tokio::task::JoinHandle<Result<()>>);
impl Drop for NativeTunnel {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn same_owner(saved: &Saved, session: &Session) -> Result<()> {
    ensure!(
        saved.origin == session.origin
            && saved.context == session.context
            && saved.user_id == session.user_id,
        CliError::new(
            Category::Conflict,
            "development_session_owner_mismatch",
            "Select the original server and user before stopping or recovering this development session."
        )
    );
    Ok(())
}

pub async fn stop_saved() -> Result<()> {
    let bytes = match std::fs::read(STATE) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let saved: Saved =
        serde_json::from_slice(&bytes).context("invalid_development_session_state")?;
    let session = app_cli::session()?;
    same_owner(&saved, &session)?;
    let id = if let Some(id) = saved.id {
        Some(id)
    } else {
        match app_cli::api(
            &session,
            &format!(
                "/api/v1/apps/{}/dev-sessions/by-request/{}",
                saved.app_id, saved.request_id
            ),
            None,
        )
        .await
        {
            Ok(value) => Some(serde_json::from_value(value["id"].clone())?),
            Err(e) if crate::cli_error::classify(&e).category == Category::NotFound => None,
            Err(e) => return Err(e),
        }
    };
    if let Some(id) = id
        && let Err(e) = app_cli::api_method(
            &session,
            &format!("/api/v1/dev-sessions/{id}"),
            reqwest::Method::DELETE,
            None,
        )
        .await
    {
        let error = crate::cli_error::classify(&e);
        if !matches!(error.category, Category::NotFound) {
            return Err(e);
        }
    }
    std::fs::remove_file(STATE)?;
    Ok(())
}

impl Remote {
    pub async fn start(
        session: &Session,
        manifest: &contract::Manifest,
        project: &str,
        cancelled: impl Fn() -> bool,
        runtime: crate::preview::Runtime,
    ) -> Result<Self> {
        if runtime == crate::preview::Runtime::Docker {
            validate_origin(&session.origin)?;
        }
        let capability = app_cli::api(session, "/api/v1/capabilities", None).await?;
        ensure!(
            capability["features"]
                .as_array()
                .is_some_and(|features| features.iter().any(|f| f == "development.sessions.v1")),
            CliError::new(
                Category::Environment,
                "unsupported_server_capability",
                "This server does not support shared development databases. Upgrade the server before starting local development."
            )
        );
        let image = if runtime == crate::preview::Runtime::Docker {
            Some(helper_image(session).await?)
        } else {
            None
        };
        ensure!(
            !cancelled(),
            CliError::new(
                Category::Interrupted,
                "preview_stopped",
                "Preview startup was stopped."
            )
        );
        let files = crate::bundle::migrations(Path::new(&manifest.migrations.directory))?;
        let migrations: Vec<Value> = files
            .keys()
            .map(|name| {
                let sql =
                    std::fs::read_to_string(Path::new(&manifest.migrations.directory).join(name))?;
                Ok(json!({"name":name,"sql":sql}))
            })
            .collect::<Result<_>>()?;
        let payload = json!({"manifest":manifest,"migrations":migrations});
        let payload_hash = contract::hash(serde_json::to_vec(&payload)?);
        let mut saved = if Path::new(STATE).exists() {
            let saved: Saved = serde_json::from_slice(&std::fs::read(STATE)?)?;
            same_owner(&saved, session)?;
            ensure!(
                saved.payload_hash == payload_hash,
                CliError::new(
                    Category::Conflict,
                    "development_recovery_required",
                    "The previous development request has different migrations. Stop or recover it before changing the schema."
                )
            );
            saved
        } else {
            Saved {
                app_id: manifest.id.clone(),
                request_id: Uuid::new_v4(),
                payload_hash,
                origin: session.origin.clone(),
                context: session.context.clone(),
                user_id: session.user_id.clone(),
                id: None,
            }
        };
        crate::platform::write_private(Path::new(STATE), &serde_json::to_vec(&saved)?)?;
        let mut payload = payload;
        payload["idempotencyKey"] = json!(saved.request_id.to_string());
        let created = app_cli::api(
            session,
            &format!("/api/v1/apps/{}/dev-sessions", manifest.id),
            Some(payload),
        )
        .await?;
        let id: Uuid = serde_json::from_value(created["id"].clone())?;
        saved.id = Some(id);
        crate::platform::write_private(Path::new(STATE), &serde_json::to_vec(&saved)?)?;
        ensure!(matches!(created["status"].as_str(), Some("queued" | "preparing" | "ready")),
            CliError::new(Category::RemoteFailure, "development_session_ended", "The saved development session has ended. Run app dev-stop to clear its local recovery state, then start development again.")
                .with_details(json!({"sessionId":id,"status":created["status"],"errorCode":created["errorCode"]})));
        let database_id: Uuid = serde_json::from_value(created["databaseId"].clone())?;
        let connection = &created["connection"];
        let database = connection["database"]
            .as_str()
            .context("invalid_development_connection")?;
        let user = connection["user"]
            .as_str()
            .context("invalid_development_connection")?;
        let password = connection["password"]
            .as_str()
            .context("invalid_development_connection")?;
        let token = connection["tunnelToken"]
            .as_str()
            .context("invalid_development_connection")?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
        let mut state = created.clone();
        let mut heartbeat = tokio::time::Instant::now();
        loop {
            ensure!(
                !cancelled(),
                CliError::new(
                    Category::Interrupted,
                    "preview_stopped",
                    "Preview startup was stopped."
                )
            );
            match state["status"].as_str() {
                Some("ready") => break,
                Some("queued" | "preparing") => {},
                _ => return Err(CliError::new(Category::RemoteFailure, "development_preparation_failed", "The server could not prepare this development database. Inspect the failure and preserve the migration history.")
                    .with_details(json!({"sessionId":id,"errorCode":state["errorCode"]})).into()),
            }
            ensure!(
                tokio::time::Instant::now() < deadline,
                CliError::new(
                    Category::Network,
                    "development_preparation_pending",
                    "Database preparation is still pending. Retry the same app dev request to recover it."
                )
            );
            if heartbeat.elapsed() >= Duration::from_secs(30) {
                app_cli::api(
                    session,
                    &format!("/api/v1/dev-sessions/{id}/heartbeat"),
                    Some(json!({})),
                )
                .await?;
                heartbeat = tokio::time::Instant::now();
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
            state = app_cli::api(session, &format!("/api/v1/dev-sessions/{id}"), None).await?;
        }
        // Refresh immediately after a potentially slow download, before opening DB traffic.
        app_cli::api(
            session,
            &format!("/api/v1/dev-sessions/{id}/heartbeat"),
            Some(json!({})),
        )
        .await?;
        let scheme = if session.origin.starts_with("https://") {
            "wss"
        } else {
            "ws"
        };
        let origin = url::Url::parse(&session.origin)?;
        let mut tunnel_url = origin.clone();
        tunnel_url
            .set_scheme(scheme)
            .map_err(|_| anyhow::anyhow!("invalid_tunnel_scheme"))?;
        tunnel_url.set_path(&format!("/api/v1/dev-sessions/{id}/database"));
        let config = crate::db_tunnel::Config {
            url: tunnel_url.to_string(),
            token: token.to_owned(),
            ca_pem: session
                .ca_file
                .as_ref()
                .map(std::fs::read_to_string)
                .transpose()?,
        };
        let (host, port, native_tunnel) = if let Some(image) = image {
            let env = std::env::current_dir()?.join(".miy/preview-tunnel.env");
            crate::platform::write_private(
                &env,
                format!("MIY_TUNNEL_CONFIG={}\n", serde_json::to_string(&config)?).as_bytes(),
            )?;
            let name = format!("{project}-tunnel");
            let result = docker::docker(&[
                "run",
                "-d",
                "--name",
                &name,
                "--label",
                &format!("org.miy.preview={project}"),
                "--network",
                project,
                "--network-alias",
                "postgres",
                "--env-file",
                env.to_str().context("invalid_path")?,
                "--user",
                "10001:10001",
                "--read-only",
                "--cap-drop",
                "ALL",
                "--security-opt",
                "no-new-privileges",
                "--memory",
                "96m",
                "--pids-limit",
                "64",
                &image,
            ])
            .await;
            let _ = std::fs::remove_file(&env);
            result?;
            ("postgres", 5432, None)
        } else {
            let listener =
                tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
            let port = listener.local_addr()?.port();
            let tunnel = NativeTunnel(tokio::spawn(crate::db_tunnel::serve(listener, config)));
            ("127.0.0.1", port, Some(tunnel))
        };
        let mut database_url = url::Url::parse(&format!("postgres://{host}:{port}/"))?;
        database_url
            .set_username(user)
            .map_err(|_| anyhow::anyhow!("invalid_development_user"))?;
        database_url
            .set_password(Some(password))
            .map_err(|_| anyhow::anyhow!("invalid_development_password"))?;
        database_url.set_path(database);
        database_url
            .query_pairs_mut()
            .append_pair("sslmode", "disable");
        Ok(Self {
            id,
            database_id,
            schema_fingerprint: state["schemaFingerprint"].as_str().unwrap_or("").to_owned(),
            database_url: database_url.to_string(),
            session: session.clone(),
            native_tunnel,
        })
    }

    pub async fn maintain(&self) -> Result<()> {
        loop {
            ensure!(
                !self
                    .native_tunnel
                    .as_ref()
                    .is_some_and(|tunnel| tunnel.0.is_finished()),
                CliError::new(
                    Category::Network,
                    "database_tunnel_stopped",
                    "The local development database tunnel stopped."
                )
            );
            app_cli::api(
                &self.session,
                &format!("/api/v1/dev-sessions/{}/heartbeat", self.id),
                Some(json!({})),
            )
            .await?;
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    }
}

async fn helper_image(session: &Session) -> Result<String> {
    crate::helper_image::resolve(session).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_database_requires_a_docker_reachable_address() {
        for origin in [
            "http://localhost:18000",
            "https://localhost.:18443",
            "http://dev.localhost",
            "http://127.0.0.1",
            "http://127.1.2.3",
            "http://[::1]",
            "http://[::ffff:127.0.0.1]",
            "http://0.0.0.0",
            "http://[::]",
        ] {
            let error = validate_origin(origin).unwrap_err();
            assert_eq!(
                crate::cli_error::classify(&error).code,
                "server_development_origin_unreachable"
            );
        }
        for origin in [
            "http://10.2.3.4:8080",
            "http://intranet.example",
            "https://v2.example",
            "http://[fd00::4]:8080",
        ] {
            validate_origin(origin).unwrap();
        }
    }
}
