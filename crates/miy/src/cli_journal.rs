//! Private, context/actor-bound recovery records. No credentials are persisted here.
use crate::{
    app_cli::Session,
    cli_config,
    cli_error::{Category, CliError},
    runtime_contract::PreflightInput,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Upload {
    pub request_id: Uuid,
    pub image_id: String,
    pub archive_path: PathBuf,
    pub archive_hash: Option<String>,
    pub size_bytes: Option<u64>,
    pub receipt_id: Option<Uuid>,
    pub status: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub schema_version: u32,
    pub request_id: Uuid,
    pub context: String,
    pub origin: String,
    pub user_id: String,
    pub app_id: String,
    pub input_hash: String,
    pub intent: PreflightInput,
    pub payload: Option<Value>,
    pub payload_hash: Option<String>,
    pub submit_path: Option<String>,
    pub retry_of: Option<Uuid>,
    pub submission_status: String,
    pub created_at: i64,
    pub job_id: Option<Uuid>,
    pub cursor: Option<String>,
    pub sequence: Option<u64>,
    pub upload: Option<Upload>,
}

fn directory() -> Result<PathBuf> {
    let root = cli_config::ensure_root()?.join("journal");
    crate::platform::reject_reparse(&root)?;
    std::fs::create_dir_all(&root)?;
    Ok(root)
}

fn path(id: Uuid) -> Result<PathBuf> {
    Ok(directory()?.join(format!("{id}.json")))
}

pub fn actor(session: &Session) -> Result<(&str, &str)> {
    let name = session.context.as_deref().ok_or_else(|| {
        CliError::new(
            Category::Auth,
            "context_session_required",
            "Sign in through a named server context before creating recovery records.",
        )
    })?;
    let user = session.user_id.as_deref().ok_or_else(|| {
        CliError::new(
            Category::Auth,
            "session_subject_required",
            "The session must identify its authenticated user.",
        )
    })?;
    Ok((name, user))
}

#[cfg(test)]
fn intent_hash(session: &Session, intent: &PreflightInput) -> Result<String> {
    operation_hash(session, intent, None)
}
pub fn operation_hash(
    session: &Session,
    intent: &PreflightInput,
    retry: Option<Uuid>,
) -> Result<String> {
    let (context, user) = actor(session)?;
    Ok(crate::contract::hash(serde_json::to_vec(
        &json!({"context":context,"origin":session.origin,"userId":user,"intent":intent,"retryOf":retry}),
    )?))
}

pub fn create(session: &Session, intent: PreflightInput, retry: Option<Uuid>) -> Result<Entry> {
    let (context, user) = actor(session)?;
    crate::runtime_contract::validate_input(&intent).map_err(|_| {
        CliError::new(
            Category::Usage,
            "invalid_runtime_input",
            "The runtime operation does not match its typed input contract.",
        )
    })?;
    let entry = Entry {
        schema_version: 1,
        request_id: Uuid::new_v4(),
        context: context.into(),
        origin: session.origin.clone(),
        user_id: user.into(),
        app_id: intent.target.app_id.clone(),
        input_hash: operation_hash(session, &intent, retry)?,
        intent,
        payload: None,
        payload_hash: None,
        submit_path: None,
        retry_of: retry,
        submission_status: "prepared".into(),
        created_at: chrono::Utc::now().timestamp(),
        job_id: None,
        cursor: None,
        sequence: None,
        upload: None,
    };
    Ok(entry)
}

/// Publish the lookup index only after the complete pre-send payload is durable.
pub fn publish(entry: &Entry) -> Result<()> {
    if entry.payload.is_none() || entry.payload_hash.is_none() || entry.submit_path.is_none() {
        return Err(CliError::new(
            Category::Environment,
            "submission_not_prepared",
            "The complete runtime request must be prepared before publishing its journal.",
        )
        .into());
    }
    save(entry)?;
    let head = directory()?.join(format!("head-{}.json", entry.input_hash));
    crate::platform::write_private(
        &head,
        &serde_json::to_vec(&json!({"requestId":entry.request_id}))?,
    )?;
    Ok(())
}

pub fn save(entry: &Entry) -> Result<()> {
    crate::platform::write_private(&path(entry.request_id)?, &serde_json::to_vec_pretty(entry)?)?;
    if let Some(job) = entry.job_id {
        crate::platform::write_private(
            &directory()?.join(format!("job-{job}.json")),
            &serde_json::to_vec(&json!({"requestId":entry.request_id}))?,
        )?;
    }
    Ok(())
}

fn read(path: &Path) -> Result<Entry> {
    crate::platform::reject_reparse(path)?;
    let metadata = std::fs::metadata(path).map_err(|_| {
        CliError::new(
            Category::NotFound,
            "journal_not_found",
            "No local recovery record was found for this request or job.",
        )
    })?;
    if !metadata.is_file() || metadata.len() > 512 * 1024 {
        return Err(CliError::new(
            Category::Environment,
            "invalid_journal",
            "The local recovery record is invalid.",
        )
        .into());
    }
    serde_json::from_slice(&std::fs::read(path)?).map_err(|_| {
        CliError::new(
            Category::Environment,
            "invalid_journal",
            "The local recovery record does not match this CLI's schema.",
        )
        .into()
    })
}

fn reference(path: &Path) -> Result<Option<Uuid>> {
    crate::platform::reject_reparse(path)?;
    match std::fs::read(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
        Ok(bytes) => {
            if bytes.len() > 4096 {
                return Err(CliError::new(
                    Category::Environment,
                    "invalid_journal_reference",
                    "The recovery record index is invalid.",
                )
                .into());
            }
            let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
                CliError::new(
                    Category::Environment,
                    "invalid_journal_reference",
                    "The recovery record index is invalid.",
                )
            })?;
            value["requestId"]
                .as_str()
                .and_then(|value| Uuid::parse_str(value).ok())
                .map(Some)
                .ok_or_else(|| {
                    CliError::new(
                        Category::Environment,
                        "invalid_journal_reference",
                        "The recovery record index is invalid.",
                    )
                    .into()
                })
        }
    }
}

pub fn validate(entry: &Entry, session: &Session) -> Result<()> {
    let (context, user) = actor(session)?;
    if entry.schema_version != 1
        || entry.request_id.is_nil()
        || entry.context != context
        || entry.origin != session.origin
        || entry.user_id != user
    {
        return Err(CliError::new(
            Category::Conflict,
            "journal_identity_mismatch",
            "Select the original context and authenticated user before resuming this request.",
        )
        .into());
    }
    if entry.app_id != entry.intent.target.app_id
        || entry.input_hash != operation_hash(session, &entry.intent, entry.retry_of)?
        || crate::runtime_contract::validate_input(&entry.intent).is_err()
    {
        return Err(CliError::new(
            Category::Environment,
            "journal_input_mismatch",
            "The recovery record's runtime input is invalid or has changed.",
        )
        .into());
    }
    if let Some(payload) = &entry.payload {
        let expected = serde_json::from_value::<crate::runtime_contract::ExpectedState>(
            payload["expectedState"].clone(),
        )
        .map_err(|_| {
            CliError::new(
                Category::Environment,
                "journal_expected_state_invalid",
                "The saved expected state is invalid.",
            )
        })?;
        if expected.target != entry.intent.target
            || crate::runtime_contract::validate_state(&expected).is_err()
        {
            return Err(CliError::new(
                Category::Environment,
                "journal_expected_state_invalid",
                "The saved expected state does not match the operation target.",
            )
            .into());
        }
        let valid = payload["idempotencyKey"] == entry.request_id.to_string()
            && payload["environment"] == entry.intent.target.environment.as_str()
            && payload["expectedState"]["target"]["appId"] == entry.app_id
            && entry.payload_hash.as_deref()
                == Some(crate::contract::hash(serde_json::to_vec(payload)?).as_str());
        let normal = entry.submit_path.as_deref()
            == Some(format!("/api/v1/apps/{}/runtime", entry.app_id).as_str())
            && entry.retry_of.is_none()
            && payload["action"] == entry.intent.action.as_str()
            && payload["imageId"].as_str() == entry.intent.image_id.as_deref()
            && payload["releaseId"]
                .as_str()
                .and_then(|value| Uuid::parse_str(value).ok())
                == entry.intent.release_id;
        let retry = entry
            .submit_path
            .as_deref()
            .and_then(|path| path.strip_prefix("/api/v1/runtime/jobs/"))
            .and_then(|path| path.strip_suffix("/retry"))
            .is_some_and(|id| Uuid::parse_str(id).ok() == entry.retry_of)
            && payload["appId"] == entry.app_id;
        if !valid || !(normal || retry) {
            return Err(CliError::new(
                Category::Environment,
                "journal_payload_invalid",
                "The saved submission does not match its target and request identity.",
            )
            .into());
        }
    }
    if let Some(upload) = &entry.upload {
        let expected = archive_path(entry.request_id)?;
        if upload.request_id != entry.request_id
            || upload.archive_path != expected
            || entry.intent.image_id.as_deref() != Some(&upload.image_id)
            || upload.archive_hash.as_ref().is_some_and(|hash| {
                hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        {
            return Err(CliError::new(
                Category::Environment,
                "journal_upload_invalid",
                "The saved upload does not match its runtime request.",
            )
            .into());
        }
    }
    Ok(())
}

pub fn load(session: &Session, id: Uuid) -> Result<Entry> {
    let primary = path(id)?;
    let entry = if primary.exists() {
        read(&primary)?
    } else {
        let id = reference(&directory()?.join(format!("job-{id}.json")))?.ok_or_else(|| {
            CliError::new(
                Category::NotFound,
                "journal_not_found",
                "No local recovery record was found for this request or job.",
            )
        })?;
        read(&path(id)?)?
    };
    validate(&entry, session)?;
    Ok(entry)
}

pub fn latest(
    session: &Session,
    intent: &PreflightInput,
    retry: Option<Uuid>,
) -> Result<Option<Entry>> {
    match reference(&directory()?.join(format!(
        "head-{}.json",
        operation_hash(session, intent, retry)?
    )))? {
        Some(id) => load(session, id).map(Some),
        None => Ok(None),
    }
}

pub fn archive_path(id: Uuid) -> Result<PathBuf> {
    let directory = directory()?.join("archives");
    crate::platform::reject_reparse(&directory)?;
    std::fs::create_dir_all(&directory)?;
    Ok(directory.join(format!("{id}.tar")))
}

pub async fn operation_lock(hash: &str) -> Result<std::fs::File> {
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CliError::new(
            Category::Environment,
            "invalid_operation_key",
            "The local operation key is invalid.",
        )
        .into());
    }
    let path = directory()?.join(format!("lock-{hash}"));
    crate::platform::reject_reparse(&path)?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if file.try_lock().is_ok() {
            return Ok(file);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(CliError::new(
                Category::Conflict,
                "runtime_request_in_use",
                "Another CLI is submitting or recovering the same operation.",
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

pub fn recovery(entry: &Entry) -> crate::cli_error::Recovery {
    crate::cli_error::Recovery {
        executable: "miy".into(),
        args: vec![
            "--context".into(),
            entry.context.clone(),
            "--app".into(),
            entry.app_id.clone(),
            "job".into(),
            "resume".into(),
            entry.request_id.to_string(),
            "--environment".into(),
            entry.intent.target.environment.as_str().into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_contract::{Environment, Operation, RuntimeTarget};

    fn session(user: &str) -> Session {
        Session {
            context: Some("dev".into()),
            user_id: Some(user.into()),
            expires_at: None,
            origin: "https://platform.example".into(),
            token: None,
            cookie: String::new(),
            csrf: String::new(),
            ca_file: None,
        }
    }
    fn intent() -> PreflightInput {
        PreflightInput {
            target: RuntimeTarget {
                app_id: "demo".into(),
                environment: Environment::Dev,
            },
            action: Operation::Stop,
            image_id: None,
            release_id: None,
        }
    }
    #[test]
    fn operation_identity_binds_actor_origin_and_complete_typed_input() {
        let a = intent_hash(&session("alice"), &intent()).unwrap();
        assert_ne!(a, intent_hash(&session("bob"), &intent()).unwrap());
        let mut changed = intent();
        changed.target.environment = Environment::Prod;
        assert_ne!(a, intent_hash(&session("alice"), &changed).unwrap());
        let mut changed = session("alice");
        changed.origin = "https://another.example".into();
        assert_ne!(a, intent_hash(&changed, &intent()).unwrap());
        assert_ne!(
            a,
            operation_hash(&session("alice"), &intent(), Some(Uuid::new_v4())).unwrap()
        );
    }

    #[test]
    fn recovery_rejects_changed_payload_and_another_authenticated_actor() {
        let session = session("alice");
        let intent = intent();
        let id = Uuid::new_v4();
        let payload = json!({"action":"stop","environment":"dev","imageId":null,"releaseId":null,"confirmed":false,
            "idempotencyKey":id,"expectedState":{"target":{"appId":"demo","environment":"dev"},"installation":null}});
        let mut entry = Entry {
            schema_version: 1,
            request_id: id,
            context: "dev".into(),
            origin: session.origin.clone(),
            user_id: "alice".into(),
            app_id: "demo".into(),
            input_hash: intent_hash(&session, &intent).unwrap(),
            intent,
            payload_hash: Some(crate::contract::hash(serde_json::to_vec(&payload).unwrap())),
            payload: Some(payload),
            submit_path: Some("/api/v1/apps/demo/runtime".into()),
            retry_of: None,
            submission_status: "prepared".into(),
            created_at: 0,
            job_id: None,
            cursor: None,
            sequence: None,
            upload: None,
        };
        assert!(validate(&entry, &session).is_ok());
        let mut other = session.clone();
        other.user_id = Some("bob".into());
        assert_eq!(
            validate(&entry, &other)
                .unwrap_err()
                .downcast_ref::<CliError>()
                .unwrap()
                .category,
            Category::Conflict
        );
        entry.payload.as_mut().unwrap()["action"] = json!("remove");
        assert!(validate(&entry, &session).is_err());
    }
}
