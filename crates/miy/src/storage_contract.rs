//! Small-attachment storage contract. The owning app keeps references and ACLs.
#![cfg_attr(not(feature = "server"), allow(dead_code))]
use axum::http::Method;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
pub const INSTALLATION_BYTES: u64 = 256 * 1024 * 1024;
pub const TOTAL_BYTES: u64 = 1024 * 1024 * 1024;
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
        Some("content") => matches!(*method, Method::GET | Method::PUT),
        Some("attach" | "detach") => *method == Method::POST,
        _ => false,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReserveFile {
    pub request_id: Uuid,
    pub name: String,
    pub mime: String,
    pub size: u64,
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
