//! Public API response shapes: the one source for what the server answers, what the
//! CLI reads, the JSON Schema in `contracts/api-v1.schema.json` and the TypeScript
//! types generated from it (`packages/sdk/src/api.gen.ts`).
//!
//! Compatibility: a response only gains fields. A field that a published CLI may read
//! keeps its name and type until no supported CLI reads it
//! (`scripts/check-api-compat.py` against `contracts/api-v1.published.json`). Clients
//! ignore fields they do not know, so these types never deny unknown fields.
//!
//! The server always sends every field. A field that an older server still in use
//! does not send is read with a default (`#[serde(default)]`, marked "0.7.0 servers"),
//! so a newer CLI reads an older server.
// The client build reads only some of these responses.
#![cfg_attr(not(feature = "server"), allow(dead_code))]
use crate::runtime_contract::Environment;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An installation as the installation lists, the runtime management list and the app
/// detail return it. `canOpen`, `canManage` and `canPromote` are the decisions this
/// credential would get.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Installation {
    pub id: Uuid,
    pub app_id: String,
    /// The platform display name.
    pub name: String,
    /// The name in the release manifest.
    pub manifest_name: String,
    pub version: String,
    pub environment: Environment,
    pub release_id: Uuid,
    pub kind: InstallationKind,
    pub image_id: Option<String>,
    pub source_commit: Option<String>,
    pub db_status: String,
    pub runtime_status: String,
    pub desired_state: String,
    /// Control generation of the current runtime container.
    pub generation: i64,
    pub health: Health,
    /// 0.7.0 servers send it only in the default list.
    #[serde(default)]
    pub ui: UiModule,
    /// The latest runtime job of this app and environment.
    pub last_operation: Option<LastOperation>,
    /// The release's recorded test acceptance; present only with `canManage`.
    pub acceptance: Option<AcceptanceRecord>,
    /// The company switch stops use of this app (0.7.0 servers: runtime management list only).
    #[serde(default)]
    pub app_disabled: bool,
    /// Use: open the app now (0.7.0 servers: runtime management list only).
    #[serde(default)]
    pub can_open: bool,
    /// Runtime management read on this installation.
    pub can_manage: bool,
    /// A promotion request on the app's prod environment would be admitted.
    pub can_promote: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InstallationKind {
    DockerApp,
    ValidationFixture,
}

/// The latest service observation; process state stays in `runtimeStatus`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Health {
    pub status: HealthStatus,
    pub consecutive_failures: i32,
    pub checked_at: Option<DateTime<Utc>>,
    /// 0.7.0 servers send it only in the default list.
    #[serde(default)]
    pub error_code: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    NotApplicable,
    /// Not observed yet; a status this client does not know also reads as unknown.
    #[serde(other)]
    Unknown,
}

/// Where the platform loads the app's UI module.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiModule {
    pub entry: String,
    pub export: String,
    pub contract_version: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LastOperation {
    pub id: Uuid,
    pub action: String,
    pub status: String,
    pub step: String,
    pub error_code: Option<String>,
    pub retryable: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceRecord {
    pub recorded_by: String,
    pub note: String,
    pub checks: Vec<RecordedCheck>,
    /// The record has structured checks, not only a note (0.7.0 servers: default list only).
    #[serde(default)]
    pub structured: bool,
    pub checked_at: DateTime<Utc>,
}

/// A stored acceptance check, as the test acceptance request recorded it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RecordedCheck {
    pub scope: String,
    pub actor: String,
    pub result: String,
    pub evidence: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallationPage {
    pub installations: Vec<Installation>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct App {
    pub id: String,
    pub name: String,
}

/// `GET /api/v1/apps/{app}`: the app and the installations this credential may use.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AppDetail {
    pub app: App,
    pub installations: Vec<Installation>,
}

/// A runtime job: the receipt of a runtime request and every job read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeJob {
    pub id: Uuid,
    /// The idempotency key the request was submitted with.
    pub request_id: String,
    pub app_id: String,
    pub environment: Environment,
    pub action: String,
    pub status: String,
    pub step: String,
    pub error_code: Option<String>,
    pub failure_class: Option<String>,
    pub retryable: bool,
    pub retry_of: Option<Uuid>,
    pub image_id: Option<String>,
    /// 0.7.0 servers send it only for a single job.
    #[serde(default)]
    pub installation_id: Option<Uuid>,
    pub release_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeJobPage {
    pub jobs: Vec<RuntimeJob>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub id: i64,
    pub at: DateTime<Utc>,
    pub actor: Option<String>,
    pub actor_name: Option<String>,
    pub action: String,
    pub target: Option<String>,
    pub request_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditPage {
    pub entries: Vec<AuditEntry>,
    /// Pass as `before` to read older entries.
    pub next_cursor: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub source: GroupSource,
    pub parent_id: Option<Uuid>,
    pub active: bool,
    /// Members now: users whose primary group it is (HR) or listed members (local).
    pub members: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroupSource {
    Hr,
    Local,
}

/// The response types this contract publishes, by schema name.
#[cfg(test)]
pub fn schema() -> serde_json::Value {
    let mut generator = schemars::generate::SchemaSettings::draft2020_12()
        .for_serialize()
        .into_generator();
    generator.subschema_for::<Vec<Installation>>();
    generator.subschema_for::<InstallationPage>();
    generator.subschema_for::<AppDetail>();
    generator.subschema_for::<RuntimeJobPage>();
    generator.subschema_for::<AuditPage>();
    generator.subschema_for::<Vec<Group>>();
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "miy API v1 responses",
        "description": "Generated from crates/miy/src/api_contract.rs; do not edit.",
        "$defs": generator.take_definitions(true),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMITTED: &str = include_str!("../../../contracts/api-v1.schema.json");

    /// `contracts/api-v1.schema.json` is generated from these types. Set
    /// MIY_WRITE_API_SCHEMA=1 to rewrite it, then run `node scripts/api-types.mjs`.
    #[test]
    fn committed_schema_matches_the_types() {
        let generated = format!("{}\n", serde_json::to_string_pretty(&schema()).unwrap());
        if std::env::var_os("MIY_WRITE_API_SCHEMA").is_some() {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../contracts/api-v1.schema.json");
            std::fs::write(path, &generated).unwrap();
            return;
        }
        assert!(
            generated == COMMITTED,
            "contracts/api-v1.schema.json is stale: run MIY_WRITE_API_SCHEMA=1 cargo test api_contract, then node scripts/api-types.mjs"
        );
    }

    /// Clients read newer servers: unknown fields and health states are accepted.
    #[test]
    fn readers_accept_fields_and_states_they_do_not_know() {
        let job = serde_json::json!({
            "id": Uuid::nil(), "requestId": "r", "appId": "a", "environment": "dev",
            "action": "start", "status": "queued", "step": "queued", "errorCode": null,
            "failureClass": null, "retryable": false, "retryOf": null, "imageId": null,
            "installationId": null, "releaseId": null, "createdAt": "2026-11-05T00:00:00Z",
            "addedLater": {"any": "shape"},
        });
        assert_eq!(
            serde_json::from_value::<RuntimeJob>(job).unwrap().status,
            "queued"
        );
        let health: Health = serde_json::from_value(serde_json::json!({
            "status": "recovering", "consecutiveFailures": 0, "checkedAt": null, "errorCode": null,
        }))
        .unwrap();
        assert_eq!(health.status, HealthStatus::Unknown);
    }

    /// The paged installation row of a 0.7.0 server (7b43789) reads with defaults.
    #[test]
    fn a_newer_client_reads_an_older_server() {
        let row = serde_json::json!({
            "id": Uuid::nil(), "appId": "a", "name": "A", "environment": "prod",
            "releaseId": Uuid::nil(), "imageId": null, "sourceCommit": null, "version": "1.0.0",
            "manifestName": "A", "kind": "docker_app", "canPromote": false, "lastOperation": null,
            "generation": 3, "dbStatus": "ready", "runtimeStatus": "running", "desiredState": "running",
            "health": {"status": "healthy", "checkedAt": null, "consecutiveFailures": 0},
            "canManage": true,
            "acceptance": {"recordedBy": "u", "note": "n", "checks": [], "checkedAt": "2026-11-03T00:00:00Z"},
        });
        let item: Installation = serde_json::from_value(row).unwrap();
        assert!(!item.can_open && !item.app_disabled && item.ui.entry.is_empty());
        assert!(item.health.error_code.is_none());
    }
}
