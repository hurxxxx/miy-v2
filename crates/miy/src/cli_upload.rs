//! Whole-archive upload receipts survive lost acknowledgements. No byte-range resume.
use crate::{
    app_cli::Session,
    cli_error::{Category, CliError},
    cli_journal::{self, Entry, Upload},
};
use anyhow::Result;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{process::Stdio, time::Duration};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

fn recovery_error(entry: &Entry, category: Category, code: &str, message: &str) -> anyhow::Error {
    let mut error=CliError::new(category,code,message).with_details(json!({"requestId":entry.request_id,
        "uploadId":entry.upload.as_ref().and_then(|upload|upload.receipt_id),"remoteWorkCancelled":false,"byteRangeResume":false}));
    error.recovery = Some(cli_journal::recovery(entry));
    error.into()
}

fn receipt_error(entry: &mut Entry, error: anyhow::Error) -> anyhow::Error {
    let failure = crate::cli_error::classify(&error);
    if failure.code == "upload_expired" {
        if let Some(upload) = &mut entry.upload {
            upload.status = "failed".into();
        }
        if let Err(error) = cli_journal::save(entry) {
            return error;
        }
    }
    recovery_error(entry, failure.category, &failure.code, &failure.message)
}

async fn prepare(entry: &mut Entry) -> Result<()> {
    let image = entry
        .intent
        .image_id
        .clone()
        .filter(|image| crate::runtime_contract::valid_image_id(image))
        .ok_or_else(|| {
            CliError::new(
                Category::Usage,
                "upload_image_required",
                "Deployment requires an immutable image ID.",
            )
        })?;
    let path = cli_journal::archive_path(entry.request_id)?;
    if entry.upload.is_none() {
        entry.upload = Some(Upload {
            request_id: entry.request_id,
            image_id: image.clone(),
            archive_path: path.clone(),
            archive_hash: None,
            size_bytes: None,
            receipt_id: None,
            status: "preparing".into(),
        });
        cli_journal::save(entry)?;
    }
    crate::platform::reject_reparse(&path)?;
    let path_argument = path.to_str().ok_or_else(|| {
        CliError::new(
            Category::Environment,
            "invalid_archive_path",
            "The image archive path must be valid Unicode.",
        )
    })?;
    let command = crate::platform::command("docker", &["save", "--output", path_argument, &image])
        .map_err(|_| {
            recovery_error(
                entry,
                Category::Environment,
                "docker_unavailable",
                "Docker is required to export the immutable image archive.",
            )
        })?;
    let mut command = tokio::process::Command::from(command);
    command
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    let (mut child, _guard) = crate::owned_process::spawn(&mut command).await?;
    let status = tokio::time::timeout(Duration::from_secs(300), child.wait())
        .await
        .map_err(|_| {
            recovery_error(
                entry,
                Category::Environment,
                "image_export_timeout",
                "Docker did not finish exporting the archive within five minutes.",
            )
        })??;
    if !status.success() {
        return Err(recovery_error(
            entry,
            Category::Environment,
            "image_export_failed",
            "Docker could not export the immutable image.",
        ));
    }
    let (size, hash) = archive_hash(&path).await?;
    let upload = entry.upload.as_mut().unwrap();
    upload.size_bytes = Some(size);
    upload.archive_hash = Some(hash);
    upload.status = "prepared".into();
    cli_journal::save(entry)
}

async fn archive_hash(path: &std::path::Path) -> Result<(u64, String)> {
    crate::platform::reject_reparse(path)?;
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > crate::image_contract::MAX_ARCHIVE
    {
        return Err(CliError::new(Category::Environment,"invalid_image_archive","The image archive must be a regular, nonempty file within the server's archive size limit.").into());
    }
    let mut file = tokio::fs::File::open(path).await?;
    let mut hash = Sha256::new();
    let mut size = 0u64;
    // Keep the archive buffer out of every enclosing command future/stack frame.
    let mut buffer = vec![0u8; 65536];
    loop {
        let count = file.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        size += count as u64;
        if size > crate::image_contract::MAX_ARCHIVE {
            return Err(CliError::new(
                Category::Environment,
                "image_archive_too_large",
                "The archive exceeds the server's size limit.",
            )
            .into());
        }
        hash.update(&buffer[..count]);
    }
    if size != metadata.len() {
        return Err(CliError::new(
            Category::Conflict,
            "image_archive_changed",
            "The archive changed while it was being read.",
        )
        .into());
    }
    Ok((size, hex::encode(hash.finalize())))
}

async fn lookup(session: &Session, entry: &Entry) -> Result<Value> {
    let path = if let Some(id) = entry.upload.as_ref().and_then(|upload| upload.receipt_id) {
        format!("/api/v1/image-uploads/{id}")
    } else {
        format!(
            "/api/v1/apps/{}/image-uploads/by-request/{}",
            entry.app_id, entry.request_id
        )
    };
    crate::app_cli::api(session, &path, None).await
}

async fn send(session: &Session, entry: &Entry) -> Result<Value> {
    let upload = entry.upload.as_ref().ok_or_else(|| {
        CliError::new(
            Category::Environment,
            "upload_record_missing",
            "The upload recovery record is missing.",
        )
    })?;
    let (size, hash) = archive_hash(&upload.archive_path).await?;
    if Some(size) != upload.size_bytes || Some(&hash) != upload.archive_hash.as_ref() {
        return Err(recovery_error(
            entry,
            Category::Conflict,
            "image_archive_changed",
            "The saved archive no longer matches the original request.",
        ));
    }
    let session = crate::cli_login::refreshed(session).await?;
    let request = crate::cli_client::client(session.ca_file.as_deref(), 300)?
        .post(format!(
            "{}/api/v1/apps/{}/images",
            session.origin, entry.app_id
        ))
        .header("content-length", size)
        .header("content-type", "application/x-tar")
        .header("x-miy-image", &upload.image_id)
        .header("x-miy-archive-sha256", hash)
        .header("x-miy-request-id", entry.request_id.to_string())
        .body(tokio::fs::File::open(&upload.archive_path).await?);
    crate::cli_error::response(
        crate::cli_client::authenticate(&session, request)
            .send()
            .await
            .map_err(crate::cli_error::transport)?,
    )
    .await
}

fn accept_receipt(entry: &mut Entry, value: &Value) -> Result<()> {
    let upload = entry.upload.as_ref().ok_or_else(|| {
        CliError::new(
            Category::Environment,
            "upload_record_missing",
            "The upload recovery record is missing.",
        )
    })?;
    let id = value["id"]
        .as_str()
        .and_then(|value| Uuid::parse_str(value).ok())
        .filter(|id| !id.is_nil());
    let status = value["status"].as_str().filter(|status| {
        matches!(
            *status,
            "receiving"
                | "queued"
                | "pending"
                | "running"
                | "loading"
                | "importing"
                | "ready"
                | "failed"
        )
    });
    let bound = id.is_some()
        && status.is_some()
        && value["requestId"] == entry.request_id.to_string()
        && value["appId"] == entry.app_id
        && value["imageId"] == upload.image_id
        && value["archiveHash"].as_str() == upload.archive_hash.as_deref()
        && value["sizeBytes"].as_u64() == upload.size_bytes
        && upload.receipt_id.is_none_or(|prior| Some(prior) == id);
    if !bound {
        return Err(recovery_error(
            entry,
            Category::Network,
            "upload_receipt_mismatch",
            "The server receipt does not match the saved upload request.",
        ));
    }
    let upload = entry.upload.as_mut().unwrap();
    upload.receipt_id = id;
    upload.status = status.unwrap().into();
    cli_journal::save(entry)
}

/// `resume` is the only path allowed to retransmit an unacknowledged archive after
/// an authoritative receipt lookup reports no request. It always retains the key.
pub async fn ensure(session: &Session, entry: &mut Entry, resume: bool) -> Result<()> {
    let result = tokio::select! {
        result=ensure_inner(session,entry,resume)=>Some(result),
        _=tokio::signal::ctrl_c()=>None,
    };
    result.unwrap_or_else(||Err(recovery_error(entry,Category::Interrupted,"upload_interrupted","The local upload was interrupted; its saved request ID can be used to discover the remote receipt before resubmission.")))
}

async fn ensure_inner(session: &Session, entry: &mut Entry, resume: bool) -> Result<()> {
    cli_journal::validate(entry, session)?;
    let fresh = entry.upload.is_none();
    if fresh
        || entry
            .upload
            .as_ref()
            .is_some_and(|upload| upload.archive_hash.is_none())
    {
        prepare(entry).await?;
    }
    let first = if fresh {
        send(session, entry).await
    } else {
        match lookup(session, entry).await {
            Err(error)
                if resume && crate::cli_error::classify(&error).category == Category::NotFound =>
            {
                send(session, entry).await
            }
            result => result,
        }
    };
    let mut value = match first {
        Ok(value) => value,
        Err(error) if crate::cli_error::classify(&error).category == Category::Network => {
            // A read is safe after a lost acknowledgement; never silently re-POST here.
            lookup(session,entry).await.map_err(|_|recovery_error(entry,Category::Network,"upload_result_unknown","The upload acknowledgement is unavailable. Resume this request to look up its receipt before any explicit retransmission."))?
        }
        Err(error) => return Err(receipt_error(entry, error)),
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);
    loop {
        accept_receipt(entry, &value)?;
        match entry.upload.as_ref().unwrap().status.as_str() {
            "ready" => {
                // The durable receipt is sufficient for runtime recovery once import is ready.
                let _ = tokio::fs::remove_file(&entry.upload.as_ref().unwrap().archive_path).await;
                return Ok(());
            }
            "failed" => {
                return Err(recovery_error(
                    entry,
                    Category::RemoteFailure,
                    crate::runtime_contract::FailureCode::from_code(
                        value["errorCode"].as_str().unwrap_or(""),
                    )
                    .as_str(),
                    "The accepted image upload failed. Byte-range continuation is not supported; inspect the failure before starting a new deployment.",
                ));
            }
            _ => {}
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(recovery_error(
                entry,
                Category::Network,
                "image_import_timeout",
                "Image import remains in progress; resume observation using the saved request ID.",
            ));
        }
        tokio::select! {
            _=&mut interrupt=>return Err(recovery_error(entry,Category::Interrupted,"upload_wait_interrupted","Stopped local observation; the saved upload request and remote import remain available.")),
            _=tokio::time::sleep(Duration::from_millis(500))=>{}
        }
        let result = tokio::select! {
            _=&mut interrupt=>return Err(recovery_error(entry,Category::Interrupted,"upload_wait_interrupted","Stopped local observation; the saved upload request and remote import remain available.")),
            value=lookup(session,entry)=>value,
        };
        value = result.map_err(|error| receipt_error(entry, error))?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn archive_digest_covers_exact_bytes_without_modifying_them() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("archive.tar");
        std::fs::write(&path, b"archive\0payload").unwrap();
        let (size, digest) = archive_hash(&path).await.unwrap();
        assert_eq!(size, 15);
        assert_eq!(digest, crate::contract::hash(b"archive\0payload"));
        assert_eq!(std::fs::read(&path).unwrap(), b"archive\0payload");
    }
}
