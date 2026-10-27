//! Shared typed acceptance evidence used by CLI parsing and server validation.
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceCheck {
    pub scope: String,
    pub actor: String,
    pub result: String,
    pub evidence: String,
}
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseCheck {
    pub release_id: Uuid,
    pub note: String,
    #[serde(default)]
    pub checks: Vec<AcceptanceCheck>,
}
pub fn validate_checks(checks: &[AcceptanceCheck]) -> Result<(), &'static str> {
    if checks.len() > 64
        || checks.iter().any(|c| {
            c.scope.trim().is_empty()
                || c.scope.len() > 120
                || c.actor.trim().is_empty()
                || c.actor.len() > 120
                || c.evidence.trim().is_empty()
                || c.evidence.len() > 2000
                || !["passed", "not_run"].contains(&c.result.as_str())
        })
        || (!checks.is_empty() && !checks.iter().any(|c| c.result == "passed"))
    {
        return Err("invalid_acceptance_checks");
    }
    Ok(())
}
pub fn validate(value: &ReleaseCheck) -> Result<(), &'static str> {
    if value.note.trim().is_empty() || value.note.len() > 2000 || value.release_id.is_nil() {
        return Err("invalid_acceptance_input");
    }
    validate_checks(&value.checks)
}
