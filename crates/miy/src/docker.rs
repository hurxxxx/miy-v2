//! Docker process helpers shared by the local preview and Linux worker.
use crate::app_health as runtime_health;
use anyhow::{Context, Result};
use serde_json::Value;
use std::time::Duration;
use tokio::process::Command;
use uuid::Uuid;

pub(crate) async fn docker(args: &[&str]) -> Result<String> {
    let child = Command::new("docker")
        .args(args)
        .kill_on_drop(true)
        .output();
    let out = tokio::time::timeout(Duration::from_secs(90), child)
        .await
        .context("docker_timeout")??;
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
