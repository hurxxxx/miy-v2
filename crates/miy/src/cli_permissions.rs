//! Installation permission management uses the same server policy as HTTP/MCP.
use crate::{
    app_cli,
    cli_error::{Category, CliError},
};
use anyhow::Result;
use clap::Subcommand;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Subcommand)]
pub enum PermissionCommand {
    /// Read requested/approved external permissions and the revision before changing them.
    Get {
        #[arg(long)]
        installation: Uuid,
        #[arg(long)]
        release: Option<Uuid>,
    },
    /// Replace approvals for exactly one release, using a reviewed JSON input and revision.
    Set {
        #[arg(long)]
        installation: Uuid,
        #[arg(long)]
        input: String,
    },
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionInput {
    pub release_id: Uuid,
    pub expected_revision: u64,
    pub permissions: Vec<String>,
}
pub async fn execute(command: PermissionCommand) -> Result<Value> {
    let session = app_cli::session()?;
    match command {
        PermissionCommand::Get {
            installation,
            release,
        } => {
            let query = release
                .map(|v| format!("?releaseId={v}"))
                .unwrap_or_default();
            app_cli::api(
                &session,
                &format!("/api/v1/installations/{installation}/permissions{query}"),
                None,
            )
            .await
        }
        PermissionCommand::Set {
            installation,
            input,
        } => {
            let input: PermissionInput = crate::cli_input::read(&input)?;
            if input.expected_revision >= i64::MAX as u64
                || input.permissions.len() > 128
                || input
                    .permissions
                    .iter()
                    .any(|p| p == "database" || !crate::contract::valid_permission(p))
            {
                return Err(CliError::new(Category::Usage,"invalid_installation_permissions","Use the observed revision and an explicit subset of requested external permissions; an empty list revokes all.").into());
            }
            app_cli::api_method(&session,&format!("/api/v1/installations/{installation}/permissions"),reqwest::Method::PUT,Some(json!({"releaseId":input.release_id,"expectedRevision":input.expected_revision,"permissions":input.permissions}))).await
        }
    }
}
