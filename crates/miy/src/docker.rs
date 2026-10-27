//! Docker process helpers shared by the local preview and Linux worker.
use crate::app_health as runtime_health;
use anyhow::Result;
use serde_json::Value;
use std::time::Duration;
use tokio::process::Command;
use uuid::Uuid;

pub(crate) async fn docker(args: &[&str]) -> Result<String> {
    let mut command = Command::new("docker");
    command
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let (child, _process_guard) =
        crate::owned_process::spawn(&mut command)
            .await
            .map_err(|_| {
                crate::runtime_contract::RuntimeFailure::new(
                    crate::runtime_contract::FailureCode::DockerUnavailable,
                )
            })?;
    let child = child.wait_with_output();
    let out = tokio::time::timeout(Duration::from_secs(90), child)
        .await
        .map_err(|_| {
            crate::runtime_contract::RuntimeFailure::new(
                crate::runtime_contract::FailureCode::DockerTimeout,
            )
        })?
        .map_err(|_| {
            crate::runtime_contract::RuntimeFailure::new(
                crate::runtime_contract::FailureCode::DockerUnavailable,
            )
        })?;
    anyhow::ensure!(out.status.success(), "docker_command_failed");
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}
pub(crate) async fn inspect(name: &str) -> Result<Value> {
    let value: Value = serde_json::from_str(&docker(&["inspect", name]).await?)?;
    Ok(value[0].clone())
}
pub(crate) async fn healthy(port: i32, release: Uuid) -> Result<()> {
    let client = runtime_health::client()?;
    for _ in 0..40 {
        if runtime_health::probe(&client, port, release).await.is_ok() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    anyhow::bail!("app_health_failed")
}
