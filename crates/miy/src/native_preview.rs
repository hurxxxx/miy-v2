//! Run an application's native Rust binary; the CLI retains gateway credentials.
use crate::{
    app_cli,
    cli_error::{Category, CliError},
    identity, owned_process,
};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{path::PathBuf, process::Stdio, time::Duration};
use uuid::Uuid;

pub fn build() -> Result<PathBuf> {
    let version = app_cli::output("rustc", &["-vV"])?;
    let host = version
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .context("rust_host_unavailable")?;
    ensure!(
        host.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')),
        "invalid_rust_host"
    );
    app_cli::run(
        "cargo",
        &["build", "--locked", "--bin", "miy-app", "--target", host],
    )?;
    let metadata: Value = serde_json::from_str(&app_cli::output(
        "cargo",
        &["metadata", "--no-deps", "--format-version", "1"],
    )?)?;
    let executable = PathBuf::from(
        metadata["target_directory"]
            .as_str()
            .context("cargo_target_unavailable")?,
    )
    .join(host)
    .join("debug")
    .join(format!("miy-app{}", std::env::consts::EXE_SUFFIX));
    ensure!(
        executable.is_file(),
        CliError::new(
            Category::Environment,
            "native_backend_missing",
            "Cargo did not produce the native application executable."
        )
    );
    Ok(executable)
}

pub struct Backend {
    child: tokio::process::Child,
    _guard: owned_process::Guard,
    pub port: i32,
    pub secret: String,
    pub installation: Uuid,
    pub release: Uuid,
}

impl Backend {
    pub async fn start(executable: &std::path::Path, database_url: &str) -> Result<Self> {
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let port = listener.local_addr()?.port();
        let secret = identity::random();
        let installation = Uuid::new_v4();
        let release = Uuid::new_v4();
        let mut command = tokio::process::Command::new(executable);
        command
            .env("DATABASE_URL", database_url)
            .env("MIY_GATEWAY_SECRET", &secret)
            .env("MIY_INSTALLATION_ID", installation.to_string())
            .env("MIY_RELEASE_ID", release.to_string())
            .env("MIY_BIND_ADDR", format!("127.0.0.1:{port}"))
            .stdin(Stdio::null())
            .stdout(Stdio::from(std::io::stderr()))
            .stderr(Stdio::inherit());
        drop(listener);
        let (child, guard) = owned_process::spawn(&mut command).await?;
        let mut backend = Self {
            child,
            _guard: guard,
            port: i32::from(port),
            secret,
            installation,
            release,
        };
        // Release-aware health checks cannot mistake another process for this app.
        let ready = crate::docker::healthy(backend.port, release).await;
        if ready.is_err() {
            backend.stop().await?;
        }
        ready?;
        Ok(backend)
    }

    pub async fn exited(&mut self) -> Result<()> {
        let status = self.child.wait().await?;
        Err(CliError::new(
            Category::Environment,
            "native_backend_exited",
            "The native application stopped. Inspect its diagnostic output.",
        )
        .with_details(serde_json::json!({"exitCode":status.code()}))
        .into())
    }

    pub async fn stop(&mut self) -> Result<()> {
        if self.child.try_wait()?.is_none() {
            self.child.start_kill()?;
            tokio::time::timeout(Duration::from_secs(5), self.child.wait())
                .await
                .context("native_backend_shutdown_pending")??;
        }
        Ok(())
    }
}
