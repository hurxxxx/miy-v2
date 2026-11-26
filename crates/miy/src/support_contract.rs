//! Deliberately small, credential-free manual support reports. No arbitrary diagnostics map.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const RECEIPT_HEADER: &str = "x-miy-report-key";

pub const MAX_BODY: usize = 256 * 1024;
pub const MAX_MESSAGE: usize = 32_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Failure {
    pub occurred_at: i64,
    pub code: String,
    pub backend: Option<String>,
    pub operation: Option<String>,
    pub reason: Option<String>,
    pub os_status: Option<i32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Diagnostics {
    pub cli_version: String,
    pub bundle_version: String,
    pub os: String,
    pub arch: String,
    pub config_source: String,
    pub executable: String,
    pub last_error: Option<Failure>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Report {
    pub schema_version: u32,
    pub id: Uuid,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<Uuid>,
    pub diagnostics: Diagnostics,
}

impl Report {
    pub fn validate(&self) -> Result<(), &'static str> {
        let d = &self.diagnostics;
        let identifier = |v: &str| crate::cli_error::safe_code(v).is_some();
        if self.schema_version != 1
            || self.id.get_version_num() != 4
            || self.thread_id.is_some_and(|id| id.get_version_num() != 4)
        {
            return Err("invalid_report_id_or_version");
        }
        if self.message.trim().is_empty()
            || self.message.chars().count() > MAX_MESSAGE
            || self
                .message
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err("invalid_report_message");
        }
        if !identifier(&d.cli_version)
            || !identifier(&d.bundle_version)
            || !matches!(d.os.as_str(), "linux" | "macos" | "windows" | "other")
            || !matches!(d.arch.as_str(), "x86_64" | "aarch64" | "other")
            || !matches!(
                d.config_source.as_str(),
                "override" | "xdg" | "appdata" | "home"
            )
            || !matches!(d.executable.as_str(), "project" | "installed")
        {
            return Err("invalid_report_diagnostics");
        }
        if let Some(e) = &d.last_error
            && (e.occurred_at < 0
                || !identifier(&e.code)
                || e.backend
                    .as_deref()
                    .is_some_and(|v| !matches!(v, "macos_keychain"))
                || e.reason.as_deref().is_some_and(|v| !identifier(v))
                || e.operation
                    .as_deref()
                    .is_some_and(|v| !matches!(v, "read" | "write" | "remove")))
        {
            return Err("invalid_report_error");
        }
        if serde_json::to_vec(self)
            .map_err(|_| "invalid_report")?
            .len()
            > MAX_BODY
        {
            return Err("report_too_large");
        }
        Ok(())
    }

    pub fn redact(&mut self) {
        self.message = redact(&self.message);
    }
}

/// Conservative help for explicitly entered text, not a claim that arbitrary logs
/// can be made safe. Never collect logs, environment, credentials or source files.
pub fn redact(text: &str) -> String {
    let mut private_key = false;
    text.lines()
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            let end_key = lower.contains("end ") && lower.contains("private key");
            private_key |= lower.contains("begin ") && lower.contains("private key");
            let sensitive = private_key
                || [
                    "password",
                    "passwd",
                    "authorization",
                    "bearer ",
                    "cookie",
                    "secret",
                    "token",
                    "miy_access_",
                    "miy_refresh_",
                ]
                .iter()
                .any(|term| lower.contains(term))
                || line.split_whitespace().any(|word| {
                    (word.len() > 48
                        && word
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || "._-+/=".contains(c)))
                        || (word.contains("://") && word.contains('@'))
                });
            if end_key {
                private_key = false;
            }
            if sensitive {
                "[redacted sensitive line]"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_credential_lines_and_multiline_private_keys() {
        let text = "인증 실패\nAuthorization: Bearer abc\npassword=abc\nhttps://u:p@example.test\n-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----\n마지막";
        let safe = redact(text);
        assert_eq!(safe.matches("[redacted sensitive line]").count(), 6);
        assert!(!safe.contains("abc"));
        assert!(safe.starts_with("인증 실패\n") && safe.ends_with("\n마지막"));
        assert_eq!(redact(&safe), safe);
    }
}
