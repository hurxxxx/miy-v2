//! A Linux image target is independent of the CLI host OS and CPU.
use crate::cli_error::{Category, CliError};
use anyhow::Result;
use serde_json::Value;
pub fn normalize(os: &str, arch: &str) -> Option<String> {
    if os != "linux" {
        return None;
    }
    match arch {
        "amd64" | "x86_64" => Some("linux/amd64".into()),
        "arm64" | "aarch64" => Some("linux/arm64".into()),
        _ => None,
    }
}
#[cfg(feature = "server")]
pub fn server() -> Option<String> {
    match std::env::var("MIY_RUNTIME_PLATFORM") {
        Ok(value) if ["linux/amd64", "linux/arm64"].contains(&value.as_str()) => Some(value),
        Ok(_) => None,
        Err(_) => normalize("linux", std::env::consts::ARCH),
    }
}
pub fn image(meta: &Value) -> Option<String> {
    normalize(meta["Os"].as_str()?, meta["Architecture"].as_str()?)
}
pub async fn build_target(explicit: Option<&str>) -> Result<String> {
    let info: Value = serde_json::from_str(
        &crate::docker::docker(&["info", "--format", "{{json .}}"])
            .await
            .map_err(|_| {
                CliError::new(
                    Category::Environment,
                    "docker_daemon_unavailable",
                    "Start the Docker daemon before building.",
                )
            })?,
    )?;
    let local = normalize(
        info["OSType"].as_str().unwrap_or(""),
        info["Architecture"].as_str().unwrap_or(""),
    )
    .ok_or_else(|| {
        CliError::new(
            Category::Environment,
            "linux_docker_required",
            "Use a supported Linux container engine (amd64 or arm64).",
        )
    })?;
    let target = if let Some((_, context)) = crate::cli_config::optional_selected()? {
        let response = crate::cli_client::client(context.ca_file.as_deref(), 15)?
            .get(format!("{}/api/v1/capabilities", context.origin))
            .send()
            .await?;
        let value = crate::cli_error::response(response).await?;
        value["runtimePlatform"]
            .as_str()
            .filter(|p| ["linux/amd64", "linux/arm64"].contains(p))
            .ok_or_else(|| {
                CliError::new(
                    Category::Environment,
                    "unsupported_capability",
                    "The server does not advertise a supported runtime platform.",
                )
            })?
            .to_owned()
    } else {
        local.clone()
    };
    if let Some(explicit) = explicit {
        if explicit != target {
            return Err(CliError::new(
                Category::Usage,
                "build_platform_mismatch",
                "The explicit build platform must match the selected server's runtime platform.",
            )
            .with_details(serde_json::json!({"requiredPlatform":target}))
            .into());
        }
    } else if local != target {
        return Err(CliError::new(Category::Environment,"build_platform_required","Local Docker and the selected server use different CPUs. Choose --platform explicitly and configure Docker emulation if needed.").with_details(serde_json::json!({"localPlatform":local,"requiredPlatform":target})).into());
    }
    Ok(target)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_names_and_os() {
        assert_eq!(
            normalize("linux", "aarch64").as_deref(),
            Some("linux/arm64")
        );
        assert_eq!(normalize("linux", "x86_64").as_deref(), Some("linux/amd64"));
        assert!(normalize("windows", "amd64").is_none());
        assert!(normalize("linux", "mips").is_none());
    }
}
