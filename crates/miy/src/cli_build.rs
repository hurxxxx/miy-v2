//! Committed source transfer and durable observation of server Docker builds.
use crate::{
    app_cli::{self, Session},
    cli_config,
    cli_error::{Category, CliError},
    cli_output::{Outcome, OutputMode},
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

const MAX_SOURCE: u64 = crate::source_archive::MAX_SOURCE_BYTES;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerBuildReceipt {
    pub context: String,
    pub origin: String,
    pub user_id: String,
    pub job_id: Uuid,
    pub app_id: String,
    pub version: String,
    pub bundle_version: String,
    pub source_commit: String,
    pub archive_hash: String,
    pub platform: String,
    pub image_id: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    schema_version: u32,
    request_id: Uuid,
    context: String,
    origin: String,
    user_id: String,
    app_id: String,
    version: String,
    bundle_version: String,
    source_commit: String,
    archive_hash: String,
    size_bytes: u64,
    platform: String,
    project_root: PathBuf,
    input_hash: String,
    submission_status: String,
    job_id: Option<Uuid>,
    sequence: u64,
}

fn hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn directory() -> Result<PathBuf> {
    let path = cli_config::ensure_root()?.join("build-journal");
    crate::platform::reject_reparse(&path)?;
    std::fs::create_dir_all(&path)?;
    Ok(path)
}
fn archive_path(id: Uuid) -> Result<PathBuf> {
    Ok(directory()?.join(format!("{id}.tar")))
}
fn record_path(id: Uuid) -> Result<PathBuf> {
    Ok(directory()?.join(format!("{id}.json")))
}
fn save(entry: &Entry) -> Result<()> {
    crate::platform::write_private(
        &record_path(entry.request_id)?,
        &serde_json::to_vec_pretty(entry)?,
    )?;
    if let Some(id) = entry.job_id {
        crate::platform::write_private(
            &directory()?.join(format!("job-{id}.json")),
            &serde_json::to_vec(&json!({"requestId":entry.request_id}))?,
        )?;
    }
    Ok(())
}
fn read(path: &Path) -> Result<Option<Vec<u8>>> {
    crate::platform::reject_reparse(path)?;
    match std::fs::metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
        Ok(meta) if !meta.is_file() || meta.len() > 512 * 1024 => Err(CliError::new(
            Category::Environment,
            "invalid_build_journal",
            "The private build recovery record is invalid.",
        )
        .into()),
        Ok(_) => Ok(Some(std::fs::read(path)?)),
    }
}
fn reference(path: &Path) -> Result<Option<Uuid>> {
    read(path)?
        .map(|bytes| {
            let value: Value = serde_json::from_slice(&bytes)?;
            value["requestId"]
                .as_str()
                .and_then(|v| Uuid::parse_str(v).ok())
                .filter(|id| !id.is_nil())
                .ok_or_else(|| {
                    CliError::new(
                        Category::Environment,
                        "invalid_build_journal",
                        "The build recovery index is invalid.",
                    )
                    .into()
                })
        })
        .transpose()
}
fn input_hash(entry: &Entry) -> Result<String> {
    Ok(crate::contract::hash(serde_json::to_vec(&json!({
        "context":entry.context,"origin":entry.origin,"userId":entry.user_id,
        "appId":entry.app_id,"version":entry.version,"bundleVersion":entry.bundle_version,
        "sourceCommit":entry.source_commit,"archiveHash":entry.archive_hash,
        "sizeBytes":entry.size_bytes,"platform":entry.platform,"projectRoot":entry.project_root,
    }))?))
}
fn validate_entry(entry: &Entry, session: &Session) -> Result<()> {
    let (context, user) = crate::cli_journal::actor(session)?;
    if entry.context != context || entry.origin != session.origin || entry.user_id != user {
        return Err(CliError::new(
            Category::Conflict,
            "build_identity_mismatch",
            "Select the original context and authenticated user before resuming this build.",
        )
        .into());
    }
    if entry.schema_version != 1
        || entry.request_id.is_nil()
        || entry.job_id.is_some_and(|id| id.is_nil())
        || !crate::contract::valid_id(&entry.app_id)
        || !hex(&entry.source_commit, 40)
        || !hex(&entry.archive_hash, 64)
        || !(1..=MAX_SOURCE).contains(&entry.size_bytes)
        || !matches!(entry.platform.as_str(), "linux/amd64" | "linux/arm64")
        || !entry.project_root.is_absolute()
        || input_hash(entry)? != entry.input_hash
        || !matches!(
            entry.submission_status.as_str(),
            "prepared" | "sending" | "accepted" | "rejected"
        )
    {
        return Err(CliError::new(
            Category::Environment,
            "invalid_build_journal",
            "The saved build input is invalid or has changed.",
        )
        .into());
    }
    if cli_config::options()
        .app
        .as_deref()
        .is_some_and(|app| app != entry.app_id)
    {
        return Err(CliError::new(
            Category::Conflict,
            "resume_target_mismatch",
            "The selected app does not match the saved build.",
        )
        .into());
    }
    Ok(())
}
fn load(session: &Session, id: Uuid) -> Result<Option<Entry>> {
    let bytes = if let Some(bytes) = read(&record_path(id)?)? {
        Some(bytes)
    } else if let Some(request) = reference(&directory()?.join(format!("job-{id}.json")))? {
        read(&record_path(request)?)?
    } else {
        None
    };
    bytes
        .map(|bytes| {
            let entry: Entry = serde_json::from_slice(&bytes).map_err(|_| {
                CliError::new(
                    Category::Environment,
                    "invalid_build_journal",
                    "The private build recovery record is invalid.",
                )
            })?;
            validate_entry(&entry, session)?;
            Ok(entry)
        })
        .transpose()
}
fn recovery(entry: &Entry, category: Category, code: &str, message: &str) -> anyhow::Error {
    CliError::new(category,code,message)
        .with_details(json!({"requestId":entry.request_id,"jobId":entry.job_id,"remoteWorkCancelled":false,"byteRangeResume":false}))
        .with_recovery("miy",&["--context",&entry.context,"--app",&entry.app_id,"job","resume",&entry.request_id.to_string(),"--environment","dev"]).into()
}
fn job_id(job: &Value) -> Result<Uuid> {
    job["id"]
        .as_str()
        .and_then(|v| Uuid::parse_str(v).ok())
        .filter(|id| !id.is_nil())
        .ok_or_else(|| {
            CliError::new(
                Category::Network,
                "invalid_build_response",
                "The server returned an invalid build identifier.",
            )
            .into()
        })
}
fn validate_job(session: &Session, job: &Value, id: Option<Uuid>) -> Result<()> {
    let (_, user) = crate::cli_journal::actor(session)?;
    let actual = job_id(job)?;
    if id.is_some_and(|id| id != actual)
        || job["action"] != "build"
        || job["environment"] != "dev"
        || job["actor"] != user
        || job["userId"] != user
        || !job["appId"].as_str().is_some_and(crate::contract::valid_id)
        || !job["sourceCommit"].as_str().is_some_and(|v| hex(v, 40))
        || !job["archiveHash"].as_str().is_some_and(|v| hex(v, 64))
        || !job["sizeBytes"]
            .as_u64()
            .is_some_and(|v| (1..=MAX_SOURCE).contains(&v))
        || !matches!(
            job["platform"].as_str(),
            Some("linux/amd64" | "linux/arm64")
        )
        || !matches!(
            job["status"].as_str(),
            Some("receiving" | "queued" | "running" | "succeeded" | "failed")
        )
        || cli_config::options()
            .app
            .as_deref()
            .is_some_and(|app| job["appId"] != app)
    {
        return Err(CliError::new(
            Category::Network,
            "build_response_mismatch",
            "The returned build does not match the authenticated actor, target, or build contract.",
        )
        .into());
    }
    if job["status"] == "succeeded"
        && (!job["imageId"]
            .as_str()
            .is_some_and(crate::runtime_contract::valid_image_id)
            || job["version"].as_str().is_none()
            || job["bundleVersion"].as_str().is_none())
    {
        return Err(CliError::new(
            Category::Network,
            "invalid_build_result",
            "The completed build has no valid immutable image receipt.",
        )
        .into());
    }
    Ok(())
}
fn validate_bound(
    session: &Session,
    entry: &Entry,
    job: &Value,
    request_bound: bool,
) -> Result<()> {
    validate_entry(entry, session)?;
    validate_job(session, job, entry.job_id)?;
    if job["appId"] != entry.app_id
        || job["sourceCommit"] != entry.source_commit
        || job["archiveHash"] != entry.archive_hash
        || job["platform"] != entry.platform
        || job["sizeBytes"] != entry.size_bytes
        || (request_bound && job["requestId"] != entry.request_id.to_string())
        || (!job["version"].is_null() && job["version"] != entry.version)
        || (!job["bundleVersion"].is_null() && job["bundleVersion"] != entry.bundle_version)
    {
        return Err(recovery(
            entry,
            Category::Network,
            "build_receipt_mismatch",
            "The server build receipt differs from the saved source and request identity.",
        ));
    }
    Ok(())
}
fn bind(session: &Session, entry: &mut Entry, job: &Value, request_bound: bool) -> Result<()> {
    validate_bound(session, entry, job, request_bound)?;
    entry.job_id = Some(job_id(job)?);
    entry.submission_status = "accepted".into();
    save(entry)
}
async fn lookup(session: &Session, entry: &Entry) -> Result<Value> {
    let job = app_cli::api(
        session,
        &format!(
            "/api/v1/apps/{}/build-requests/{}",
            entry.app_id, entry.request_id
        ),
        None,
    )
    .await?;
    validate_bound(session, entry, &job, true)?;
    Ok(job)
}
async fn send(session: &Session, entry: &mut Entry) -> Result<Value> {
    let path = archive_path(entry.request_id)?;
    crate::platform::reject_reparse(&path)?;
    let mut file=std::fs::File::open(&path).map_err(|_|recovery(entry,Category::Environment,"build_source_unavailable","The original source archive is missing. Recover the accepted request before creating a new build."))?;
    if !file.metadata()?.is_file() || file.metadata()?.len() != entry.size_bytes {
        return Err(recovery(
            entry,
            Category::Conflict,
            "build_source_changed",
            "The saved source archive no longer matches this build.",
        ));
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    if hex::encode(hasher.finalize()) != entry.archive_hash {
        return Err(recovery(
            entry,
            Category::Conflict,
            "build_source_changed",
            "The saved source archive no longer matches this build.",
        ));
    }
    let session = crate::cli_login::refreshed(session).await?;
    let request = crate::cli_client::client(session.ca_file.as_deref(), 300)?
        .post(format!(
            "{}/api/v1/apps/{}/builds",
            session.origin, entry.app_id
        ))
        .header("content-type", "application/x-tar")
        .header("content-length", entry.size_bytes)
        .header("x-miy-source-commit", &entry.source_commit)
        .header("x-miy-archive-sha256", &entry.archive_hash)
        .header("x-miy-platform", &entry.platform)
        .header("x-miy-request-id", entry.request_id.to_string())
        .body(tokio::fs::File::open(path).await?);
    entry.submission_status = "sending".into();
    save(entry)?;
    let result = tokio::select! {
        result=crate::cli_client::authenticate(&session,request).send()=>Some(result),
        _=tokio::signal::ctrl_c()=>None,
    };
    let Some(result) = result else {
        return Err(recovery(
            entry,
            Category::Interrupted,
            "build_upload_interrupted",
            "Stopped source transfer; resume the saved request to discover or complete its upload.",
        ));
    };
    let result = match result {
        Ok(response) => crate::cli_error::response(response).await,
        Err(error) => Err(crate::cli_error::transport(error).into()),
    };
    match result {
        Ok(job) => {
            bind(&session, entry, &job, true)?;
            Ok(job)
        }
        Err(error) => {
            let failure = crate::cli_error::classify(&error);
            if matches!(
                failure.category,
                Category::Usage | Category::Forbidden | Category::Conflict
            ) {
                entry.submission_status = "rejected".into();
                save(entry)?;
            }
            Err(recovery(
                entry,
                failure.category,
                &failure.code,
                &failure.message,
            ))
        }
    }
}
fn receipt(entry: &Entry, job: &Value) -> Result<ServerBuildReceipt> {
    Ok(ServerBuildReceipt {
        context: entry.context.clone(),
        origin: entry.origin.clone(),
        user_id: entry.user_id.clone(),
        job_id: job_id(job)?,
        app_id: entry.app_id.clone(),
        version: entry.version.clone(),
        bundle_version: entry.bundle_version.clone(),
        source_commit: entry.source_commit.clone(),
        archive_hash: entry.archive_hash.clone(),
        platform: entry.platform.clone(),
        image_id: job["imageId"]
            .as_str()
            .ok_or_else(|| {
                CliError::new(
                    Category::Network,
                    "invalid_build_result",
                    "The completed build has no immutable image ID.",
                )
            })?
            .into(),
    })
}
fn materialize(entry: &Entry, job: &Value) -> Result<Value> {
    let mut result = job.clone();
    let receipt = receipt(entry, job)?;
    result["serverBuild"] = serde_json::to_value(&receipt)?;
    crate::platform::reject_reparse(&entry.project_root)?;
    let mut git = crate::platform::command("git", &["rev-parse", "HEAD"])?;
    git.current_dir(&entry.project_root);
    let head = crate::owned_process::output(&mut git)?;
    let manifest = std::fs::read(entry.project_root.join("manifest.json"))
        .ok()
        .and_then(|v| serde_json::from_slice::<crate::contract::Manifest>(&v).ok());
    if !head.status.success()
        || String::from_utf8_lossy(&head.stdout).trim() != entry.source_commit
        || !manifest.is_some_and(|m| m.id == entry.app_id && m.version == entry.version)
    {
        result["metadataWritten"] = json!(false);
        result["metadataReason"] = json!("project_revision_changed");
        return Ok(result);
    }
    let dist = entry.project_root.join("dist");
    crate::platform::reject_reparse(&dist)?;
    std::fs::create_dir_all(&dist)?;
    let build = json!({"bundleVersion":entry.bundle_version,"sourceCommit":entry.source_commit,"appId":entry.app_id,"version":entry.version});
    let image = json!({"bundleVersion":entry.bundle_version,"sourceCommit":entry.source_commit,"appId":entry.app_id,"version":entry.version,
        "platform":entry.platform,"imageId":receipt.image_id,"serverBuild":receipt});
    crate::platform::write_private(
        &dist.join("build.json"),
        &serde_json::to_vec_pretty(&build)?,
    )?;
    crate::platform::write_private(
        &dist.join("image.json"),
        &serde_json::to_vec_pretty(&image)?,
    )?;
    result["metadataWritten"] = json!(true);
    Ok(result)
}

pub fn validate_receipt_identity(
    receipt: &ServerBuildReceipt,
    session: &Session,
    app: &str,
    image: &str,
) -> Result<()> {
    let (context, user) = crate::cli_journal::actor(session)?;
    if receipt.context != context
        || receipt.origin != session.origin
        || receipt.user_id != user
        || receipt.app_id != app
        || receipt.image_id != image
        || receipt.job_id.is_nil()
        || !hex(&receipt.source_commit, 40)
        || !hex(&receipt.archive_hash, 64)
        || !crate::runtime_contract::valid_image_id(&receipt.image_id)
        || !matches!(receipt.platform.as_str(), "linux/amd64" | "linux/arm64")
    {
        return Err(CliError::new(
            Category::Conflict,
            "server_build_identity_mismatch",
            "The server build receipt belongs to another context, actor, app, or image.",
        )
        .into());
    }
    Ok(())
}
pub async fn verify_receipt(session: &Session, receipt: &ServerBuildReceipt) -> Result<()> {
    validate_receipt_identity(receipt, session, &receipt.app_id, &receipt.image_id)?;
    let job = app_cli::api(session, &format!("/api/v1/builds/{}", receipt.job_id), None).await?;
    validate_job(session, &job, Some(receipt.job_id))?;
    if job["status"] != "succeeded"
        || job["appId"] != receipt.app_id
        || job["imageId"] != receipt.image_id
        || job["sourceCommit"] != receipt.source_commit
        || job["archiveHash"] != receipt.archive_hash
        || job["platform"] != receipt.platform
        || job["version"] != receipt.version
        || job["bundleVersion"] != receipt.bundle_version
    {
        return Err(CliError::new(
            Category::Conflict,
            "server_build_not_ready",
            "The server has no successful build matching this immutable image receipt.",
        )
        .into());
    }
    Ok(())
}

pub async fn execute(platform: Option<String>, no_wait: bool, timeout: u64) -> Result<Outcome> {
    let session = app_cli::session()?;
    let capabilities = crate::cli_remote::require_features(&session, &["app.builds.v1"]).await?;
    let required = capabilities["runtimePlatform"]
        .as_str()
        .filter(|p| matches!(*p, "linux/amd64" | "linux/arm64"))
        .ok_or_else(|| {
            CliError::new(
                Category::Environment,
                "unsupported_runtime_platform",
                "The server must advertise a supported Linux build platform.",
            )
        })?;
    if platform.as_deref().is_some_and(|value| value != required) {
        return Err(CliError::new(
            Category::Usage,
            "build_platform_mismatch",
            "The requested platform must match the selected server's runtime platform.",
        )
        .with_details(json!({"requiredPlatform":required}))
        .into());
    }
    let request_id = Uuid::new_v4();
    let archive = archive_path(request_id)?;
    let root = std::env::current_dir()?.canonicalize()?;
    let packed = crate::source_archive::pack(&root, &archive).map_err(|error| {
        let code = error.to_string();
        let category = if matches!(code.as_str(), "commit_source_before_building" | "source_changed_during_archive") { Category::Conflict } else { Category::Environment };
        CliError::new(category, crate::cli_error::safe_code(&code).unwrap_or("invalid_build_source"),
            "Prepare a clean committed application with intact SDK/components before requesting a server build.")
    })?;
    let (context, user) = crate::cli_journal::actor(&session)?;
    let mut entry = Entry {
        schema_version: 1,
        request_id,
        context: context.into(),
        origin: session.origin.clone(),
        user_id: user.into(),
        app_id: packed.manifest.id,
        version: packed.manifest.version,
        bundle_version: crate::bundle::VERSION.into(),
        source_commit: packed.source_commit,
        archive_hash: packed.archive_hash,
        size_bytes: packed.size_bytes,
        platform: required.into(),
        project_root: root,
        input_hash: String::new(),
        submission_status: "prepared".into(),
        job_id: None,
        sequence: 0,
    };
    entry.input_hash = input_hash(&entry)?;
    validate_entry(&entry, &session)?;
    let lock = crate::cli_journal::operation_lock(&entry.input_hash).await?;
    let head = directory()?.join(format!("head-{}.json", entry.input_hash));
    if let Some(id) = reference(&head)? {
        let previous = load(&session, id)?.ok_or_else(|| {
            CliError::new(
                Category::Environment,
                "build_journal_missing",
                "The existing build recovery record is missing.",
            )
        })?;
        let previous_failed = lookup(&session, &previous)
            .await
            .is_ok_and(|job| job["status"] == "failed");
        if !previous_failed {
            let _ = std::fs::remove_file(&archive);
            drop(lock);
            return recover(&session, previous, no_wait, timeout, false).await;
        }
        // Running the build command after an observed terminal failure is a
        // new attempt. Resuming the old request never creates that attempt.
    }
    save(&entry)?;
    crate::platform::write_private(
        &head,
        &serde_json::to_vec(&json!({"requestId":entry.request_id}))?,
    )?;
    let sent = send(&session, &mut entry).await;
    let job = match sent {
        Ok(job) => job,
        Err(error) if crate::cli_error::classify(&error).category == Category::Network => {
            let job=lookup(&session,&entry).await.map_err(|_|recovery(&entry,Category::Network,"build_submission_unknown","The source upload acknowledgement was lost; resume the saved request before starting another build."))?;
            bind(&session, &mut entry, &job, true)?;
            job
        }
        Err(error) => return Err(error),
    };
    drop(lock);
    if no_wait {
        finish_or_pending(&session, Some(&entry), job)
    } else {
        observe(&session, Some(entry), job, timeout, OutputMode::Json, None).await
    }
}
fn finish_or_pending(session: &Session, entry: Option<&Entry>, job: Value) -> Result<Outcome> {
    if let Some(entry) = entry {
        validate_bound(session, entry, &job, false)?;
    } else {
        validate_job(session, &job, None)?;
    }
    if job["status"] == "failed" {
        let code = job["errorCode"]
            .as_str()
            .and_then(crate::cli_error::safe_code)
            .unwrap_or("remote_build_failed");
        if let Some(entry) = entry {
            return Err(recovery(
                entry,
                Category::RemoteFailure,
                code,
                "The accepted server build failed. Inspect its logs before changing source or starting another build.",
            ));
        }
        return Err(CliError::new(
            Category::RemoteFailure,
            code,
            "The accepted server build failed; inspect job logs.",
        )
        .with_details(json!({"jobId":job["id"]}))
        .into());
    }
    let result = if job["status"] == "succeeded" {
        if let Some(entry) = entry {
            materialize(entry, &job).map_err(|_| recovery(entry,Category::Environment,"build_metadata_unavailable","The server build succeeded, but local deployment metadata could not be written. Resume this build to restore its receipt."))?
        } else {
            job
        }
    } else {
        job
    };
    Ok(Outcome::Data(result))
}
async fn recover(
    session: &Session,
    mut entry: Entry,
    no_wait: bool,
    timeout: u64,
    resubmit: bool,
) -> Result<Outcome> {
    let lock = crate::cli_journal::operation_lock(&entry.input_hash).await?;
    entry = load(session, entry.request_id)?.ok_or_else(|| {
        CliError::new(
            Category::Environment,
            "build_journal_missing",
            "The saved build record is missing.",
        )
    })?;
    let found = lookup(session, &entry).await;
    let job = match found {
        Ok(job) => {
            bind(session, &mut entry, &job, true)?;
            if resubmit && job["status"] == "receiving" {
                send(session, &mut entry).await?
            } else {
                job
            }
        }
        Err(error)
            if crate::cli_error::classify(&error).category == Category::NotFound
                && (resubmit || entry.submission_status == "prepared") =>
        {
            if entry.submission_status == "rejected" {
                return Err(recovery(
                    &entry,
                    Category::Conflict,
                    "build_submission_rejected",
                    "The original request was rejected; inspect the failure before starting new work.",
                ));
            }
            send(session, &mut entry).await?
        }
        Err(error) => {
            return Err(recovery(
                &entry,
                crate::cli_error::classify(&error).category,
                "build_lookup_unavailable",
                "The original build result could not be discovered. Resume this saved request before submitting again.",
            ));
        }
    };
    drop(lock);
    if no_wait {
        finish_or_pending(session, Some(&entry), job)
    } else {
        observe(session, Some(entry), job, timeout, OutputMode::Json, None).await
    }
}

pub async fn try_get(session: &Session, id: Uuid) -> Result<Option<Value>> {
    let entry = load(session, id)?;
    let job = if let Some(entry) = &entry {
        lookup(session, entry).await?
    } else {
        match app_cli::api(session, &format!("/api/v1/builds/{id}"), None).await {
            Ok(job) => job,
            Err(error) if crate::cli_error::classify(&error).category == Category::NotFound => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        }
    };
    if let Some(entry) = &entry {
        validate_bound(session, entry, &job, true)?;
    } else {
        validate_job(session, &job, Some(id))?;
    }
    Ok(Some(job))
}
pub async fn try_resume(
    session: &Session,
    id: Uuid,
    no_wait: bool,
    environment: Option<&str>,
) -> Result<Option<Outcome>> {
    if let Some(entry) = load(session, id)? {
        if environment.is_some_and(|env| env != "dev") {
            return Err(CliError::new(
                Category::Conflict,
                "resume_target_mismatch",
                "Server builds belong to the dev environment.",
            )
            .into());
        }
        crate::cli_remote::require_features(session, &["app.builds.v1"]).await?;
        return recover(session, entry, no_wait, 1800, true).await.map(Some);
    }
    if let Some(job) = try_get(session, id).await? {
        if environment.is_some_and(|env| env != "dev") {
            return Err(CliError::new(
                Category::Conflict,
                "resume_target_mismatch",
                "Server builds belong to the dev environment.",
            )
            .into());
        }
        return if no_wait {
            finish_or_pending(session, None, job).map(Some)
        } else {
            observe(session, None, job, 1800, OutputMode::Json, None)
                .await
                .map(Some)
        };
    }
    Ok(None)
}
pub async fn try_watch(
    session: &Session,
    id: Uuid,
    after: Option<String>,
    timeout: u64,
    mode: OutputMode,
) -> Result<Option<Outcome>> {
    let Some(job) = try_get(session, id).await? else {
        return Ok(None);
    };
    let after = after
        .map(|s| -> Result<u64> {
            Ok(s.parse::<u64>().map_err(|_| {
                CliError::new(
                    Category::Usage,
                    "invalid_build_log_cursor",
                    "Use the nonnegative build log sequence returned by job logs.",
                )
            })?)
        })
        .transpose()?;
    observe(session, load(session, id)?, job, timeout, mode, after)
        .await
        .map(Some)
}
pub async fn logs(session: &Session, id: Uuid, after: u64) -> Result<Value> {
    let job = try_get(session, id).await?.ok_or_else(|| {
        CliError::new(
            Category::Usage,
            "build_logs_required",
            "job logs accepts a server build ID; use app logs for a running application.",
        )
    })?;
    log_page(session, job_id(&job)?, after).await
}
async fn log_page(session: &Session, id: Uuid, after: u64) -> Result<Value> {
    let page = app_cli::api(
        session,
        &format!("/api/v1/builds/{id}/logs?after={after}&limit=200"),
        None,
    )
    .await?;
    if page["id"] != id.to_string()
        || page["jobId"] != id.to_string()
        || page["nextSequence"].as_u64().is_none_or(|n| n < after)
        || page["logs"].as_array().is_none_or(|logs| logs.len() > 200)
    {
        return Err(CliError::new(
            Category::Network,
            "invalid_build_logs",
            "The returned build log page does not match the requested job and sequence.",
        )
        .into());
    }
    let mut sequence = after;
    let mut bytes = 0usize;
    for line in page["logs"].as_array().unwrap() {
        let next = line["sequence"]
            .as_u64()
            .filter(|n| *n > sequence)
            .ok_or_else(|| {
                CliError::new(
                    Category::Network,
                    "invalid_build_logs",
                    "Build log sequences must advance.",
                )
            })?;
        if line["text"]
            .as_str()
            .is_none_or(|text| text.len() > 2 * 1024 * 1024)
        {
            return Err(CliError::new(
                Category::Network,
                "invalid_build_logs",
                "The build log entry is invalid.",
            )
            .into());
        }
        bytes += line["text"].as_str().unwrap().len();
        if bytes > 2 * 1024 * 1024 {
            return Err(CliError::new(
                Category::Network,
                "invalid_build_logs",
                "The build log page exceeds its bound.",
            )
            .into());
        }
        sequence = next;
    }
    if page["nextSequence"].as_u64().unwrap() < sequence {
        return Err(CliError::new(
            Category::Network,
            "invalid_build_logs",
            "The next build log sequence is inconsistent.",
        )
        .into());
    }
    Ok(page)
}
fn observation_failure(
    entry: Option<&Entry>,
    id: Uuid,
    category: Category,
    code: &str,
    message: &str,
) -> anyhow::Error {
    if let Some(entry) = entry {
        recovery(entry, category, code, message)
    } else {
        CliError::new(category, code, message)
            .with_details(json!({"jobId":id,"remoteWorkCancelled":false}))
            .with_recovery("miy", &["job", "watch", &id.to_string()])
            .into()
    }
}
fn observation_transport(entry: Option<&Entry>, id: Uuid, error: anyhow::Error) -> anyhow::Error {
    let failure = crate::cli_error::classify(&error);
    observation_failure(entry, id, failure.category, &failure.code, &failure.message)
}
async fn observe(
    session: &Session,
    mut entry: Option<Entry>,
    mut job: Value,
    timeout: u64,
    mode: OutputMode,
    after: Option<u64>,
) -> Result<Outcome> {
    let id = job_id(&job)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout);
    let mut sequence = after
        .or_else(|| entry.as_ref().map(|e| e.sequence))
        .unwrap_or(0);
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);
    loop {
        if let Some(entry) = &entry {
            validate_bound(session, entry, &job, false)?;
        } else {
            validate_job(session, &job, Some(id))?;
        }
        if mode == OutputMode::Jsonl {
            let page = tokio::select! {
                result = log_page(session,id,sequence) => result.map_err(|error| observation_transport(entry.as_ref(),id,error))?,
                _ = &mut interrupt => return Err(observation_failure(entry.as_ref(),id,Category::Interrupted,"build_watch_interrupted","Stopped local observation; the accepted server build continues.")),
                _ = tokio::time::sleep_until(deadline) => return Err(observation_failure(entry.as_ref(),id,Category::Network,"build_watch_timeout","Observation timed out; the accepted server build continues.")),
            };
            for line in page["logs"].as_array().unwrap() {
                let mut stdout = std::io::stdout().lock();
                serde_json::to_writer(
                    &mut stdout,
                    &json!({"schemaVersion":1,"command":"job watch","ok":true,"data":{"cursor":line["sequence"].to_string(),"sequence":line["sequence"],"createdAt":line["createdAt"],"text":line["text"],"job":job},"error":null,"meta":{"cursor":line["sequence"].to_string()}}),
                )?;
                stdout.write_all(b"\n")?;
                stdout.flush()?;
            }
            sequence = page["nextSequence"].as_u64().unwrap();
            if let Some(entry) = &mut entry {
                entry.sequence = sequence;
                save(entry)?;
            }
            if page["logs"].as_array().unwrap().len() == 200 {
                continue;
            }
        }
        if matches!(job["status"].as_str(), Some("succeeded" | "failed")) {
            let result = finish_or_pending(session, entry.as_ref(), job)?;
            if mode == OutputMode::Jsonl {
                if let Outcome::Data(data) = result {
                    let mut stdout = std::io::stdout().lock();
                    serde_json::to_writer(
                        &mut stdout,
                        &json!({"schemaVersion":1,"command":"job watch","ok":true,"data":data,"error":null,"meta":{"cursor":sequence.to_string()}}),
                    )?;
                    stdout.write_all(b"\n")?;
                    stdout.flush()?;
                }
                return Ok(Outcome::Raw);
            }
            return Ok(result);
        }
        let failure = tokio::select! {
            _=&mut interrupt=>Some((Category::Interrupted,"build_watch_interrupted","Stopped local observation; the accepted server build continues.")),
            _=tokio::time::sleep_until(deadline)=>Some((Category::Network,"build_watch_timeout","Observation timed out; the server build continues and can be resumed.")),
            _=tokio::time::sleep(Duration::from_secs(1))=>None,
        };
        if let Some((category, code, message)) = failure {
            return Err(if let Some(entry) = &entry {
                recovery(entry, category, code, message)
            } else {
                CliError::new(category, code, message)
                    .with_details(json!({"jobId":id,"remoteWorkCancelled":false}))
                    .with_recovery("miy", &["job", "watch", &id.to_string()])
                    .into()
            });
        }
        let path = format!("/api/v1/builds/{id}");
        job = tokio::select! {
            result=app_cli::api(session,&path,None)=>result.map_err(|error|observation_transport(entry.as_ref(),id,error))?,
            _=&mut interrupt=>return Err(observation_failure(entry.as_ref(),id,Category::Interrupted,"build_watch_interrupted","Stopped local observation; the accepted server build continues.")),
            _=tokio::time::sleep_until(deadline)=>return Err(observation_failure(entry.as_ref(),id,Category::Network,"build_watch_timeout","Observation timed out; the accepted server build continues.")),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn session() -> Session {
        Session {
            context: Some("fixture".into()),
            user_id: Some("developer".into()),
            origin: "https://platform.example".into(),
            expires_at: None,
            cookie: String::new(),
            csrf: String::new(),
            token: None,
            ca_file: None,
        }
    }
    fn receipt() -> ServerBuildReceipt {
        ServerBuildReceipt {
            context: "fixture".into(),
            origin: "https://platform.example".into(),
            user_id: "developer".into(),
            job_id: Uuid::new_v4(),
            app_id: "demo".into(),
            version: "1.0.0".into(),
            bundle_version: crate::bundle::VERSION.into(),
            source_commit: "a".repeat(40),
            archive_hash: "b".repeat(64),
            platform: "linux/amd64".into(),
            image_id: format!("sha256:{}", "c".repeat(64)),
        }
    }
    #[test]
    fn server_image_receipt_binds_actor_context_origin_app_and_image() {
        let original = receipt();
        let session = session();
        assert!(validate_receipt_identity(&original, &session, "demo", &original.image_id).is_ok());
        for field in ["context", "origin", "userId", "appId", "imageId"] {
            let mut value = serde_json::to_value(&original).unwrap();
            value[field] = json!("different");
            let changed: ServerBuildReceipt = serde_json::from_value(value).unwrap();
            assert!(
                validate_receipt_identity(&changed, &session, "demo", &original.image_id).is_err(),
                "{field}"
            );
        }
    }
    #[test]
    fn build_receipt_rejects_malformed_and_foreign_server_results() {
        let receipt = receipt();
        let session = session();
        let good = json!({"id":receipt.job_id,"requestId":Uuid::new_v4(),"actor":"developer","userId":"developer","appId":"demo","environment":"dev","action":"build","sourceCommit":receipt.source_commit,"archiveHash":receipt.archive_hash,"sizeBytes":1024,"platform":"linux/amd64","status":"succeeded","imageId":receipt.image_id,"version":"1.0.0","bundleVersion":crate::bundle::VERSION});
        assert!(validate_job(&session, &good, Some(receipt.job_id)).is_ok());
        for (field, value) in [
            ("actor", json!("admin")),
            ("imageId", json!("mutable-tag")),
            ("platform", json!("windows/amd64")),
            ("sizeBytes", json!(MAX_SOURCE + 1)),
            ("status", json!("unknown")),
        ] {
            let mut bad = good.clone();
            bad[field] = value;
            assert!(
                validate_job(&session, &bad, Some(receipt.job_id)).is_err(),
                "{field}"
            );
        }
    }
}
