//! A Linux image target is independent of the CLI host OS and CPU.
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
