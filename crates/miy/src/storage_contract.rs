//! Small-attachment storage contract. The owning app keeps references and ACLs.
#![cfg_attr(not(feature = "server"), allow(dead_code))]
use axum::http::Method;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One-request (`single`) upload limit.
pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
/// A `parts` reservation (M10 large transfer).
pub const MAX_LARGE_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// One range of a `parts` upload; stays under the frame file gateway's limit.
pub const MAX_PART_BYTES: u64 = 8 * 1024 * 1024;
/// Per-installation defaults while an administrator has set none.
pub const DEFAULT_INSTALLATION_BYTES: u64 = 20 * 1024 * 1024 * 1024;
pub const DEFAULT_INSTALLATION_FILES: u64 = 100_000;
pub const TRANSFER_SECONDS: u64 = 20;

/// The one list of Storage routes below an installation (`platform/files...`).
/// Core's app-call admission, the preview endpoint and the CLI's local backend
/// proxy all use it, so a new route cannot be reachable through only one of them.
pub fn allowed_route(suffix: &str, method: &Method) -> bool {
    if suffix == "platform/files" {
        return *method == Method::POST;
    }
    let Some(tail) = suffix.strip_prefix("platform/files/") else {
        return false;
    };
    if let Some(id) = tail.strip_prefix("by-request/") {
        return *method == Method::GET && Uuid::parse_str(id).is_ok();
    }
    let mut parts = tail.split('/');
    if !parts.next().is_some_and(|id| Uuid::parse_str(id).is_ok()) {
        return false;
    }
    let action = parts.next();
    if parts.next().is_some() {
        return false;
    }
    match action {
        None => matches!(*method, Method::GET | Method::DELETE),
        Some("content" | "parts") if *method == Method::PUT => true,
        Some("content") => *method == Method::GET,
        Some("attach" | "detach" | "complete" | "download") => *method == Method::POST,
        _ => false,
    }
}

/// How a reservation's bytes arrive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileTransfer {
    /// One `PUT content` request, up to 10 MiB.
    #[default]
    Single,
    /// Contiguous `PUT parts` ranges of up to 8 MiB, then `POST complete`; up to 2 GiB.
    Parts,
}

impl FileTransfer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Parts => "parts",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReserveFile {
    pub request_id: Uuid,
    pub name: String,
    pub mime: String,
    pub size: u64,
    #[serde(default)]
    pub transfer: FileTransfer,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttachFile {
    pub request_id: Uuid,
    /// Opaque app-owned reference. The core does not interpret it as a path or ACL.
    pub reference: String,
}

/// Release an attached file. The app sends this only after the transaction that
/// removed its own reference has committed; Core then deletes the bytes.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DetachFile {
    /// The reference given at attach. A different value is refused.
    pub reference: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Pending,
    Ready,
    Attached,
    Cancelled,
    Expired,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoredFile {
    pub id: Uuid,
    pub version: u32,
    pub installation_id: Uuid,
    pub request_id: Uuid,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub sha256: Option<String>,
    pub status: FileStatus,
    pub reference: Option<String>,
    pub transfer: FileTransfer,
    /// Bytes committed so far: a `parts` upload resumes from here.
    pub received: u64,
    /// Only incomplete uploads expire. Ready and attached bytes are retained.
    #[schemars(with = "Option<String>")]
    pub expires_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_routes_are_exact() {
        let id = Uuid::new_v4();
        assert!(allowed_route("platform/files", &Method::POST));
        assert!(!allowed_route("platform/files", &Method::GET));
        assert!(allowed_route(
            &format!("platform/files/by-request/{id}"),
            &Method::GET
        ));
        assert!(allowed_route(
            &format!("platform/files/{id}/content"),
            &Method::PUT
        ));
        assert!(allowed_route(
            &format!("platform/files/{id}/detach"),
            &Method::POST
        ));
        assert!(allowed_route(
            &format!("platform/files/{id}/parts"),
            &Method::PUT
        ));
        assert!(allowed_route(
            &format!("platform/files/{id}/complete"),
            &Method::POST
        ));
        assert!(!allowed_route(
            &format!("platform/files/{id}/parts"),
            &Method::GET
        ));
        assert!(!allowed_route(
            &format!("platform/files/{id}/content/extra"),
            &Method::GET
        ));
        assert!(!allowed_route("platform/files/../content", &Method::GET));
        for action in ["attach", "detach"] {
            assert!(!allowed_route(
                &format!("platform/files/{id}/{action}"),
                &Method::DELETE
            ));
        }
        assert!(!allowed_route("platform/directory", &Method::GET));
    }
}
