//! Shared, side-effect-free runtime admission, state comparison and retry contracts.
//!
//! `generation` is the installation's control_generation, not health_generation.
//! The server must load/check a snapshot under its app/environment operation lock.
//! A worker rechecks authorization and expected state before its first mutation;
//! recovery of that same job must retain the persisted admission/check marker.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(any(feature = "server", test))]
use std::collections::BTreeSet;
use std::{fmt, str::FromStr};
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseEnumError(&'static str);

impl fmt::Display for ParseEnumError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid {}", self.0)
    }
}
impl std::error::Error for ParseEnumError {}

macro_rules! string_enum {
    ($name:ident { $($(#[$attribute:meta])* $variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, JsonSchema)]
        pub enum $name {
            $($(#[$attribute])* #[serde(rename = $wire)] $variant),+
        }
        impl $name {
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $wire),+ }
            }
        }
        impl FromStr for $name {
            type Err = ParseEnumError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value { $($wire => Ok(Self::$variant)),+, _ => Err(ParseEnumError(stringify!($name))) }
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

string_enum!(Operation {
    Deploy => "deploy", Promote => "promote", Start => "start",
    Stop => "stop", Restart => "restart", Remove => "remove",
});
string_enum!(Environment { Dev => "dev", Prod => "prod" });
string_enum!(DesiredState { Running => "running", Stopped => "stopped" });
string_enum!(RuntimeStatus {
    NotDeployed => "not_deployed", Deploying => "deploying", Starting => "starting",
    Running => "running", Stopping => "stopping", Stopped => "stopped",
    Removed => "removed", Failed => "failed", Unreachable => "unreachable",
});
string_enum!(DbStatus {
    NotPrepared => "not_prepared", Provisioning => "provisioning",
    Ready => "ready", Failed => "failed",
});
string_enum!(JobStatus {
    Queued => "queued", Running => "running", Succeeded => "succeeded", Failed => "failed",
});
string_enum!(RetryClass {
    Never => "never",
    AfterPreflight => "after_preflight",
});
string_enum!(FailureCode {
    InvalidRequest => "invalid_request",
    AuthenticationRequired => "authentication_required",
    UserDisabled => "user_disabled",
    PermissionRevoked => "permission_revoked",
    TargetNotFound => "target_not_found",
    InvalidImageContract => "invalid_image_contract",
    InvalidArtifact => "invalid_artifact",
    MigrationFailed => "migration_failed",
    MigrationHistoryMismatch => "migration_history_mismatch",
    ImmutableReleaseConflict => "immutable_release_conflict",
    AppHealthFailed => "app_health_failed",
    RuntimeOperationFailed => "runtime_operation_failed",
    StateChanged => "state_changed",
    OperationInProgress => "operation_in_progress",
    ActiveDevelopmentSession => "active_dev_session",
    ImageUnavailable => "image_unavailable",
    ImagePlatformMismatch => "image_platform_mismatch",
    DatabaseUnavailable => "database_unavailable",
    DatabaseNotReady => "database_not_ready",
    DatabasePrepareFailed => "database_prepare_failed",
    ContainerOwnershipMismatch => "container_ownership_mismatch",
    ReleaseNotAccepted => "release_not_accepted",
    DockerUnavailable => "docker_unavailable",
    DockerTimeout => "docker_timeout",
    NetworkUnavailable => "network_unavailable",
    ImageImportFailed => "image_import_failed",
    ArchiveHashMismatch => "archive_hash_mismatch",
    UploadExpired => "upload_expired",
    RetryNotAllowed => "retry_not_allowed",
    #[serde(other)] Unknown => "unknown",
});

impl FailureCode {
    /// Existing or future persisted codes are interpreted by exact identity only.
    pub fn from_code(code: &str) -> Self {
        code.parse().unwrap_or(Self::Unknown)
    }

    pub const fn retry_class(self) -> RetryClass {
        match self {
            Self::DatabaseUnavailable
            | Self::DockerUnavailable
            | Self::DockerTimeout
            | Self::NetworkUnavailable => RetryClass::AfterPreflight,
            _ => RetryClass::Never,
        }
    }
}

/// A producer attaches this type at the precise failing operation. Arbitrary
/// anyhow/io/sqlx error text never grants retry permission.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeFailure {
    pub code: FailureCode,
}

impl RuntimeFailure {
    pub const fn new(code: FailureCode) -> Self {
        Self { code }
    }
    pub const fn code(&self) -> &'static str {
        self.code.as_str()
    }
    pub const fn retry_class(&self) -> RetryClass {
        self.code.retry_class()
    }
}
impl fmt::Display for RuntimeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}
impl std::error::Error for RuntimeFailure {}

#[cfg(any(feature = "server", test))]
pub fn classify_failure(error: &anyhow::Error) -> RuntimeFailure {
    error
        .downcast_ref::<RuntimeFailure>()
        .cloned()
        .unwrap_or_else(|| RuntimeFailure::new(FailureCode::RuntimeOperationFailed))
}

/// This only classifies eligibility. A retry still requires a fresh permission,
/// state, target/impact confirmation and idempotency check at server admission.
pub fn can_retry(status: JobStatus, failure: &RuntimeFailure) -> bool {
    status == JobStatus::Failed && failure.retry_class() == RetryClass::AfterPreflight
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeTarget {
    pub app_id: String,
    pub environment: Environment,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreflightInput {
    pub target: RuntimeTarget,
    pub action: Operation,
    pub image_id: Option<String>,
    pub release_id: Option<Uuid>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallationState {
    pub installation_id: Uuid,
    pub release_id: Uuid,
    /// Monotonic control_generation. Health observations must not advance it.
    pub generation: u64,
    pub desired_state: DesiredState,
    pub runtime_status: RuntimeStatus,
    pub db_status: DbStatus,
    pub image_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DatabaseState {
    pub database_id: Uuid,
    pub status: DbStatus,
    pub schema_generation: u64,
    pub schema_fingerprint: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpectedState {
    pub target: RuntimeTarget,
    /// None explicitly means this target has no installation yet.
    pub installation: Option<InstallationState>,
    /// Shared database identity and SQL history are independently revisioned.
    pub database: Option<DatabaseState>,
}

string_enum!(StateField {
    AppId => "appId", Environment => "environment", InstallationId => "installationId",
    ReleaseId => "releaseId", Generation => "generation", DesiredState => "desiredState",
    RuntimeStatus => "runtimeStatus", DbStatus => "dbStatus", ImageId => "imageId",
    DatabaseId => "databaseId", DatabaseResourceStatus => "databaseResourceStatus",
    SchemaGeneration => "schemaGeneration", SchemaFingerprint => "schemaFingerprint",
});

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContractError {
    pub code: FailureCode,
    pub field: Option<String>,
    pub changed_fields: Vec<StateField>,
}
impl ContractError {
    fn invalid(field: &str) -> Self {
        Self {
            code: FailureCode::InvalidRequest,
            field: Some(field.into()),
            changed_fields: vec![],
        }
    }
}
impl fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}
impl std::error::Error for ContractError {}

impl Operation {
    pub const fn requires_confirmation(self, environment: Environment) -> bool {
        matches!(environment, Environment::Prod) || matches!(self, Self::Remove)
    }

    #[cfg(any(feature = "server", test))]
    pub const fn desired_state(self) -> DesiredState {
        match self {
            Self::Stop | Self::Remove => DesiredState::Stopped,
            Self::Deploy | Self::Promote | Self::Start | Self::Restart => DesiredState::Running,
        }
    }
}

pub fn valid_image_id(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hash| {
        hash.len() == 64
            && hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

pub fn validate_input(input: &PreflightInput) -> Result<(), ContractError> {
    if !crate::contract::valid_id(&input.target.app_id) {
        return Err(ContractError::invalid("target.appId"));
    }
    match input.action {
        Operation::Deploy => {
            if input.target.environment != Environment::Dev {
                return Err(ContractError::invalid("target.environment"));
            }
            if !input.image_id.as_deref().is_some_and(valid_image_id) {
                return Err(ContractError::invalid("imageId"));
            }
            if input.release_id.is_some() {
                return Err(ContractError::invalid("releaseId"));
            }
        }
        Operation::Promote => {
            if input.target.environment != Environment::Prod {
                return Err(ContractError::invalid("target.environment"));
            }
            if !input.release_id.is_some_and(|id| !id.is_nil()) {
                return Err(ContractError::invalid("releaseId"));
            }
            if input.image_id.is_some() {
                return Err(ContractError::invalid("imageId"));
            }
        }
        _ => {
            if input.image_id.is_some() {
                return Err(ContractError::invalid("imageId"));
            }
            if input.release_id.is_some() {
                return Err(ContractError::invalid("releaseId"));
            }
        }
    }
    Ok(())
}

pub fn validate_state(state: &ExpectedState) -> Result<(), ContractError> {
    if !crate::contract::valid_id(&state.target.app_id) {
        return Err(ContractError::invalid("target.appId"));
    }
    if let Some(database) = &state.database
        && (database.database_id.is_nil()
            || database.schema_generation > i64::MAX as u64
            || database.schema_fingerprint.as_ref().is_some_and(|value| {
                value.len() != 64 || !value.bytes().all(|c| c.is_ascii_hexdigit())
            }))
    {
        return Err(ContractError::invalid("database"));
    }
    if let Some(installation) = &state.installation {
        if installation.installation_id.is_nil() || installation.release_id.is_nil() {
            return Err(ContractError::invalid("installation"));
        }
        if installation.generation > i64::MAX as u64 {
            return Err(ContractError::invalid("installation.generation"));
        }
        if installation
            .image_id
            .as_deref()
            .is_some_and(|id| !valid_image_id(id))
        {
            return Err(ContractError::invalid("installation.imageId"));
        }
    }
    Ok(())
}

#[cfg(any(feature = "server", test))]
pub fn compare_expected(
    expected: &ExpectedState,
    current: &ExpectedState,
) -> Result<(), ContractError> {
    validate_state(expected)?;
    validate_state(current)?;
    let mut changed_fields = vec![];
    if expected.target.app_id != current.target.app_id {
        changed_fields.push(StateField::AppId);
    }
    if expected.target.environment != current.target.environment {
        changed_fields.push(StateField::Environment);
    }
    match (&expected.database, &current.database) {
        (None, None) => {}
        (Some(before), Some(after)) => {
            for (changed, field) in [
                (
                    before.database_id != after.database_id,
                    StateField::DatabaseId,
                ),
                (
                    before.status != after.status,
                    StateField::DatabaseResourceStatus,
                ),
                (
                    before.schema_generation != after.schema_generation,
                    StateField::SchemaGeneration,
                ),
                (
                    before.schema_fingerprint != after.schema_fingerprint,
                    StateField::SchemaFingerprint,
                ),
            ] {
                if changed {
                    changed_fields.push(field);
                }
            }
        }
        _ => changed_fields.push(StateField::DatabaseId),
    }
    match (&expected.installation, &current.installation) {
        (None, None) => {}
        (Some(before), Some(after)) => {
            for (changed, field) in [
                (
                    before.installation_id != after.installation_id,
                    StateField::InstallationId,
                ),
                (before.release_id != after.release_id, StateField::ReleaseId),
                (
                    before.generation != after.generation,
                    StateField::Generation,
                ),
                (
                    before.desired_state != after.desired_state,
                    StateField::DesiredState,
                ),
                (
                    before.runtime_status != after.runtime_status,
                    StateField::RuntimeStatus,
                ),
                (before.db_status != after.db_status, StateField::DbStatus),
                (before.image_id != after.image_id, StateField::ImageId),
            ] {
                if changed {
                    changed_fields.push(field);
                }
            }
        }
        _ => changed_fields.push(StateField::InstallationId),
    }
    if changed_fields.is_empty() {
        Ok(())
    } else {
        Err(ContractError {
            code: FailureCode::StateChanged,
            field: None,
            changed_fields,
        })
    }
}

string_enum!(CheckName {
    Input => "input", Target => "target", Authorization => "authorization",
    PendingOperation => "pendingOperation", CurrentState => "currentState",
    Database => "database", Image => "image", ImagePlatform => "imagePlatform",
    ReleaseAcceptance => "releaseAcceptance", ContainerOwnership => "containerOwnership",
    MigrationCompatibility => "migrationCompatibility",
});
string_enum!(CheckStatus { Passed => "passed", Failed => "failed", NotChecked => "not_checked" });

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreflightCheck {
    pub name: CheckName,
    pub status: CheckStatus,
    pub code: Option<FailureCode>,
    pub reason: Option<String>,
}
#[cfg(any(feature = "server", test))]
impl PreflightCheck {
    pub const fn passed(name: CheckName) -> Self {
        Self {
            name,
            status: CheckStatus::Passed,
            code: None,
            reason: None,
        }
    }
    pub const fn failed(name: CheckName, code: FailureCode) -> Self {
        Self {
            name,
            status: CheckStatus::Failed,
            code: Some(code),
            reason: None,
        }
    }
    pub fn not_checked(name: CheckName, reason: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::NotChecked,
            code: None,
            reason: Some(reason.into()),
        }
    }
    fn valid(&self) -> bool {
        match self.status {
            CheckStatus::Passed => self.code.is_none() && self.reason.is_none(),
            CheckStatus::Failed => self.code.is_some() && self.reason.is_none(),
            CheckStatus::NotChecked => {
                self.code.is_none()
                    && self
                        .reason
                        .as_deref()
                        .is_some_and(|reason| !reason.trim().is_empty() && reason.len() <= 512)
            }
        }
    }
}

string_enum!(PlannedChange {
    CreateInstallation => "create_installation", ReplaceRelease => "replace_release",
    ApplyMigrations => "apply_migrations", StartRuntime => "start_runtime",
    StopRuntime => "stop_runtime", RestartRuntime => "restart_runtime", RemoveRuntime => "remove_runtime",
});

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreflightResult {
    pub input: PreflightInput,
    pub expected_state: ExpectedState,
    pub can_submit: bool,
    pub fully_checked: bool,
    pub requires_confirmation: bool,
    pub preserves_database: bool,
    pub desired_state: DesiredState,
    pub changes: Vec<PlannedChange>,
    pub checks: Vec<PreflightCheck>,
}

/// Produce a plan without I/O or mutations. Caller-supplied checks are trusted
/// server observations, never check results accepted from a CLI request.
#[cfg(any(feature = "server", test))]
pub fn preflight(
    input: &PreflightInput,
    current: &ExpectedState,
    external_checks: Vec<PreflightCheck>,
) -> Result<PreflightResult, ContractError> {
    validate_input(input)?;
    validate_state(current)?;
    if input.target != current.target {
        return Err(ContractError::invalid("target"));
    }
    let mut names = BTreeSet::new();
    for check in &external_checks {
        if !check.valid()
            || matches!(
                check.name,
                CheckName::Input | CheckName::CurrentState | CheckName::Database
            )
            || !names.insert(check.name)
        {
            return Err(ContractError::invalid("checks"));
        }
    }
    let mut checks = external_checks;
    checks.push(PreflightCheck::passed(CheckName::Input));
    let control = !matches!(input.action, Operation::Deploy | Operation::Promote);
    let state_failure = match &current.installation {
        None if control => Some(FailureCode::TargetNotFound),
        Some(state)
            if matches!(
                state.runtime_status,
                RuntimeStatus::Deploying | RuntimeStatus::Starting | RuntimeStatus::Stopping
            ) =>
        {
            Some(FailureCode::OperationInProgress)
        }
        Some(state)
            if control
                && (state.runtime_status == RuntimeStatus::NotDeployed
                    || (state.runtime_status == RuntimeStatus::Removed
                        && input.action != Operation::Remove)
                    || state.image_id.is_none()) =>
        {
            Some(FailureCode::ImageUnavailable)
        }
        _ => None,
    };
    checks.push(match state_failure {
        Some(code) => PreflightCheck::failed(CheckName::CurrentState, code),
        None => PreflightCheck::passed(CheckName::CurrentState),
    });
    if matches!(input.action, Operation::Start | Operation::Restart) {
        checks.push(
            if current
                .installation
                .as_ref()
                .is_some_and(|state| state.db_status == DbStatus::Ready)
            {
                PreflightCheck::passed(CheckName::Database)
            } else {
                PreflightCheck::failed(CheckName::Database, FailureCode::DatabaseNotReady)
            },
        );
    }
    for (name, reason) in [
        (
            CheckName::Target,
            "The server must verify that the target app exists.",
        ),
        (
            CheckName::Authorization,
            "The server must verify the current user's management permission.",
        ),
        (
            CheckName::PendingOperation,
            "The server must check the app/environment operation queue under its lock.",
        ),
    ] {
        if !names.contains(&name) {
            checks.push(PreflightCheck::not_checked(name, reason));
        }
    }
    if input.action == Operation::Promote && !names.contains(&CheckName::ReleaseAcceptance) {
        checks.push(PreflightCheck::not_checked(
            CheckName::ReleaseAcceptance,
            "The server must verify that this exact release was accepted in dev.",
        ));
    }
    if !control {
        for (name, reason) in [
            (
                CheckName::Image,
                "The worker must verify the immutable image and its app ownership.",
            ),
            (
                CheckName::ImagePlatform,
                "The image platform must match the deployment server.",
            ),
            (
                CheckName::MigrationCompatibility,
                "The worker must validate migration history before applying changes.",
            ),
        ] {
            if !names.contains(&name) {
                checks.push(PreflightCheck::not_checked(name, reason));
            }
        }
    }
    if current
        .installation
        .as_ref()
        .is_some_and(|state| state.image_id.is_some())
        && !names.contains(&CheckName::ContainerOwnership)
    {
        checks.push(PreflightCheck::not_checked(
            CheckName::ContainerOwnership,
            "The worker must verify the current container's ownership and execution generation.",
        ));
    }
    checks.sort_by_key(|check| check.name);
    let mut required = vec![
        CheckName::Target,
        CheckName::Authorization,
        CheckName::PendingOperation,
    ];
    if input.action == Operation::Promote {
        required.push(CheckName::ReleaseAcceptance);
    }
    let can_submit = !checks
        .iter()
        .any(|check| check.status == CheckStatus::Failed)
        && required.iter().all(|name| {
            checks
                .iter()
                .any(|check| check.name == *name && check.status == CheckStatus::Passed)
        });
    let changes = match input.action {
        Operation::Deploy | Operation::Promote => {
            let mut changes = vec![];
            if current.installation.is_none() {
                changes.push(PlannedChange::CreateInstallation);
            } else {
                changes.push(PlannedChange::StopRuntime);
            }
            changes.extend([
                PlannedChange::ReplaceRelease,
                PlannedChange::ApplyMigrations,
                PlannedChange::StartRuntime,
            ]);
            changes
        }
        Operation::Start => vec![PlannedChange::StartRuntime],
        Operation::Stop => vec![PlannedChange::StopRuntime],
        Operation::Restart => vec![PlannedChange::RestartRuntime],
        Operation::Remove => vec![PlannedChange::StopRuntime, PlannedChange::RemoveRuntime],
    };
    Ok(PreflightResult {
        input: input.clone(),
        expected_state: current.clone(),
        can_submit,
        fully_checked: checks
            .iter()
            .all(|check| check.status == CheckStatus::Passed),
        requires_confirmation: input.action.requires_confirmation(input.target.environment),
        preserves_database: true,
        desired_state: input.action.desired_state(),
        changes,
        checks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn image() -> String {
        format!("sha256:{}", "a".repeat(64))
    }
    fn target(environment: Environment) -> RuntimeTarget {
        RuntimeTarget {
            app_id: "test-app".into(),
            environment,
        }
    }
    fn current() -> ExpectedState {
        ExpectedState {
            target: target(Environment::Dev),
            database: None,
            installation: Some(InstallationState {
                installation_id: Uuid::from_u128(1),
                release_id: Uuid::from_u128(2),
                generation: 3,
                desired_state: DesiredState::Running,
                runtime_status: RuntimeStatus::Running,
                db_status: DbStatus::Ready,
                image_id: Some(image()),
            }),
        }
    }
    fn input(action: Operation, environment: Environment) -> PreflightInput {
        PreflightInput {
            target: target(environment),
            action,
            image_id: (action == Operation::Deploy).then(image),
            release_id: (action == Operation::Promote).then(|| Uuid::from_u128(2)),
        }
    }
    fn authorized() -> Vec<PreflightCheck> {
        [
            CheckName::Target,
            CheckName::Authorization,
            CheckName::PendingOperation,
        ]
        .map(PreflightCheck::passed)
        .into()
    }

    #[test]
    fn action_inputs_enforce_dev_deploy_and_exact_release_promotion() {
        for action in [
            Operation::Deploy,
            Operation::Promote,
            Operation::Start,
            Operation::Stop,
            Operation::Restart,
            Operation::Remove,
        ] {
            let environment = if action == Operation::Promote {
                Environment::Prod
            } else {
                Environment::Dev
            };
            assert!(validate_input(&input(action, environment)).is_ok());
        }
        assert!(validate_input(&input(Operation::Deploy, Environment::Prod)).is_err());
        assert!(validate_input(&input(Operation::Promote, Environment::Dev)).is_err());
        let mut promote = input(Operation::Promote, Environment::Prod);
        promote.image_id = Some(image());
        assert_eq!(
            validate_input(&promote).unwrap_err().field.as_deref(),
            Some("imageId")
        );
        let mut deploy = input(Operation::Deploy, Environment::Dev);
        deploy.image_id = Some(format!("sha256:{}", "A".repeat(64)));
        assert!(validate_input(&deploy).is_err());
        let mut control = input(Operation::Start, Environment::Dev);
        control.release_id = Some(Uuid::from_u128(9));
        assert!(validate_input(&control).is_err());
    }

    #[test]
    fn expected_state_detects_aba_restart_and_every_admission_field() {
        let expected = current();
        assert!(compare_expected(&expected, &expected).is_ok());
        type StateMutation = fn(&mut ExpectedState);
        let mutations: &[(StateField, StateMutation)] = &[
            (StateField::AppId, |state| {
                state.target.app_id = "other-app".into()
            }),
            (StateField::Environment, |state| {
                state.target.environment = Environment::Prod
            }),
            (StateField::InstallationId, |state| {
                state.installation.as_mut().unwrap().installation_id = Uuid::from_u128(8)
            }),
            (StateField::ReleaseId, |state| {
                state.installation.as_mut().unwrap().release_id = Uuid::from_u128(8)
            }),
            (StateField::Generation, |state| {
                state.installation.as_mut().unwrap().generation += 1
            }),
            (StateField::DesiredState, |state| {
                state.installation.as_mut().unwrap().desired_state = DesiredState::Stopped
            }),
            (StateField::RuntimeStatus, |state| {
                state.installation.as_mut().unwrap().runtime_status = RuntimeStatus::Stopped
            }),
            (StateField::DbStatus, |state| {
                state.installation.as_mut().unwrap().db_status = DbStatus::Failed
            }),
            (StateField::ImageId, |state| {
                state.installation.as_mut().unwrap().image_id =
                    Some(format!("sha256:{}", "b".repeat(64)))
            }),
        ];
        for (field, mutate) in mutations {
            let mut changed = expected.clone();
            mutate(&mut changed);
            let error = compare_expected(&expected, &changed).unwrap_err();
            assert_eq!(error.code, FailureCode::StateChanged);
            assert_eq!(error.changed_fields, vec![*field]);
        }
        let absent = ExpectedState {
            target: expected.target.clone(),
            installation: None,
            database: None,
        };
        assert!(compare_expected(&absent, &absent).is_ok());
        assert!(compare_expected(&absent, &expected).is_err());
        assert!(compare_expected(&expected, &absent).is_err());
    }

    #[test]
    fn schema_changes_invalidate_a_preflight_without_a_runtime_change() {
        let mut before = current();
        before.database = Some(DatabaseState {
            database_id: Uuid::from_u128(9),
            status: DbStatus::Ready,
            schema_generation: 1,
            schema_fingerprint: Some("a".repeat(64)),
        });
        let mut after = before.clone();
        after.database.as_mut().unwrap().schema_generation += 1;
        after.database.as_mut().unwrap().schema_fingerprint = Some("b".repeat(64));
        let error = compare_expected(&before, &after).unwrap_err();
        assert_eq!(
            error.changed_fields,
            vec![StateField::SchemaGeneration, StateField::SchemaFingerprint]
        );
    }

    #[test]
    fn snapshot_validation_rejects_impossible_identifiers_and_generations() {
        let mut snapshot = current();
        snapshot.installation.as_mut().unwrap().generation = i64::MAX as u64 + 1;
        assert_eq!(
            validate_state(&snapshot).unwrap_err().field.as_deref(),
            Some("installation.generation")
        );
        snapshot.installation.as_mut().unwrap().generation = 0;
        assert!(validate_state(&snapshot).is_ok());
        snapshot.installation.as_mut().unwrap().release_id = Uuid::nil();
        assert!(validate_state(&snapshot).is_err());
        let mut value = serde_json::to_value(current()).unwrap();
        value["installation"]["generation"] = json!(-1);
        assert!(serde_json::from_value::<ExpectedState>(value).is_err());
    }

    #[test]
    fn redeploy_of_stopped_installation_keeps_existing_start_after_deploy_policy() {
        let mut snapshot = current();
        snapshot.installation.as_mut().unwrap().desired_state = DesiredState::Stopped;
        snapshot.installation.as_mut().unwrap().runtime_status = RuntimeStatus::Stopped;
        let plan = preflight(
            &input(Operation::Deploy, Environment::Dev),
            &snapshot,
            authorized(),
        )
        .unwrap();
        assert!(plan.can_submit);
        assert_eq!(plan.desired_state, DesiredState::Running);
        assert_eq!(plan.changes.last(), Some(&PlannedChange::StartRuntime));
        assert_eq!(plan.expected_state, snapshot);
    }

    #[test]
    fn preflight_requires_authority_and_queue_checks_without_pretending_worker_checks_ran() {
        let snapshot = current();
        let request = input(Operation::Deploy, Environment::Dev);
        let unchecked = preflight(&request, &snapshot, vec![]).unwrap();
        assert!(!unchecked.can_submit);
        assert!(!unchecked.fully_checked);
        let plan = preflight(&request, &snapshot, authorized()).unwrap();
        assert!(plan.can_submit);
        assert!(!plan.fully_checked);
        assert_eq!(plan.expected_state, snapshot);
        assert_eq!(plan.input, request);
        assert!(plan.preserves_database);
        assert_eq!(plan.desired_state, DesiredState::Running);
        assert!(
            plan.checks
                .iter()
                .any(|check| check.name == CheckName::MigrationCompatibility
                    && check.status == CheckStatus::NotChecked
                    && check.reason.is_some())
        );
        let mut denied = authorized();
        denied[1] =
            PreflightCheck::failed(CheckName::Authorization, FailureCode::PermissionRevoked);
        assert!(!preflight(&request, &snapshot, denied).unwrap().can_submit);
        let mut busy = authorized();
        busy[2] = PreflightCheck::failed(
            CheckName::PendingOperation,
            FailureCode::OperationInProgress,
        );
        assert!(!preflight(&request, &snapshot, busy).unwrap().can_submit);
    }

    #[test]
    fn control_requires_installation_and_ready_database_and_remove_preserves_database() {
        let snapshot = current();
        let absent = ExpectedState {
            target: snapshot.target.clone(),
            installation: None,
            database: None,
        };
        assert!(
            !preflight(
                &input(Operation::Start, Environment::Dev),
                &absent,
                authorized()
            )
            .unwrap()
            .can_submit
        );
        let mut failed_db = snapshot.clone();
        failed_db.installation.as_mut().unwrap().db_status = DbStatus::Failed;
        assert!(
            !preflight(
                &input(Operation::Restart, Environment::Dev),
                &failed_db,
                authorized()
            )
            .unwrap()
            .can_submit
        );
        let removed = preflight(
            &input(Operation::Remove, Environment::Dev),
            &snapshot,
            authorized(),
        )
        .unwrap();
        assert!(removed.can_submit && removed.requires_confirmation && removed.preserves_database);
        assert_eq!(removed.desired_state, DesiredState::Stopped);
        assert!(removed.changes.contains(&PlannedChange::RemoveRuntime));
    }

    #[test]
    fn promote_requires_acceptance_and_every_prod_operation_requires_fresh_confirmation() {
        let mut snapshot = current();
        snapshot.target.environment = Environment::Prod;
        let request = input(Operation::Promote, Environment::Prod);
        assert!(
            !preflight(&request, &snapshot, authorized())
                .unwrap()
                .can_submit
        );
        let mut checks = authorized();
        checks.push(PreflightCheck::passed(CheckName::ReleaseAcceptance));
        let plan = preflight(&request, &snapshot, checks).unwrap();
        assert!(plan.can_submit && plan.requires_confirmation);
        for action in [
            Operation::Deploy,
            Operation::Promote,
            Operation::Start,
            Operation::Stop,
            Operation::Restart,
            Operation::Remove,
        ] {
            assert!(action.requires_confirmation(Environment::Prod));
            assert_eq!(
                action.requires_confirmation(Environment::Dev),
                action == Operation::Remove
            );
        }
    }

    #[test]
    fn invalid_duplicate_or_spoofed_checks_are_rejected() {
        let snapshot = current();
        let request = input(Operation::Stop, Environment::Dev);
        let mut duplicate = authorized();
        duplicate.push(PreflightCheck::failed(
            CheckName::Authorization,
            FailureCode::PermissionRevoked,
        ));
        assert!(preflight(&request, &snapshot, duplicate).is_err());
        assert!(
            preflight(
                &request,
                &snapshot,
                vec![PreflightCheck::passed(CheckName::CurrentState)]
            )
            .is_err()
        );
        assert!(
            preflight(
                &request,
                &snapshot,
                vec![PreflightCheck::not_checked(CheckName::Image, "")]
            )
            .is_err()
        );
    }

    #[test]
    fn retry_only_allows_explicit_transient_terminal_failures() {
        for code in [
            FailureCode::PermissionRevoked,
            FailureCode::AuthenticationRequired,
            FailureCode::UserDisabled,
            FailureCode::InvalidImageContract,
            FailureCode::MigrationFailed,
            FailureCode::MigrationHistoryMismatch,
            FailureCode::ImmutableReleaseConflict,
            FailureCode::AppHealthFailed,
            FailureCode::RuntimeOperationFailed,
            FailureCode::Unknown,
        ] {
            assert!(!can_retry(JobStatus::Failed, &RuntimeFailure::new(code)));
        }
        let transient = RuntimeFailure::new(FailureCode::DockerUnavailable);
        assert!(can_retry(JobStatus::Failed, &transient));
        for status in [JobStatus::Queued, JobStatus::Running, JobStatus::Succeeded] {
            assert!(!can_retry(status, &transient));
        }
        let misleading =
            anyhow::anyhow!("docker_timeout network_unavailable permission_revoked secret");
        assert_eq!(
            classify_failure(&misleading).code,
            FailureCode::RuntimeOperationFailed
        );
        assert_eq!(
            classify_failure(&misleading).retry_class(),
            RetryClass::Never
        );
        let typed =
            anyhow::Error::new(transient.clone()).context("migration_failed is only context");
        assert_eq!(classify_failure(&typed), transient);
        assert_eq!(
            FailureCode::from_code("future_transient_code").retry_class(),
            RetryClass::Never
        );
    }

    #[test]
    fn serde_and_schema_preserve_enums_and_reject_unrecognized_request_fields() {
        let snapshot = current();
        let value = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(value["installation"]["runtimeStatus"], "running");
        assert_eq!(
            serde_json::from_value::<ExpectedState>(value).unwrap(),
            snapshot
        );
        let mut value = serde_json::to_value(input(Operation::Stop, Environment::Dev)).unwrap();
        value["approved"] = json!(true);
        assert!(serde_json::from_value::<PreflightInput>(value).is_err());
        assert!(serde_json::from_value::<Environment>(json!("production")).is_err());
        assert_eq!(
            serde_json::from_value::<RuntimeFailure>(json!({"code":"future_error"}))
                .unwrap()
                .retry_class(),
            RetryClass::Never
        );
        assert!(
            serde_json::from_value::<RuntimeFailure>(
                json!({"code":"migration_failed","retryClass":"after_preflight"})
            )
            .is_err()
        );
        assert_eq!(
            "not_deployed".parse::<RuntimeStatus>().unwrap(),
            RuntimeStatus::NotDeployed
        );
        assert_eq!(
            "not_prepared".parse::<DbStatus>().unwrap(),
            DbStatus::NotPrepared
        );
        let schema = serde_json::to_value(schemars::schema_for!(PreflightInput)).unwrap();
        assert_eq!(schema["additionalProperties"], false);
        assert!(
            schema["$defs"]["Operation"]["enum"]
                .as_array()
                .unwrap()
                .contains(&json!("promote"))
        );
    }
}
