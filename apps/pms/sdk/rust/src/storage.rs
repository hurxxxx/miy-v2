//! App-owned attachments backed by Core storage. The app must check its current
//! resource ACL before every call; possession of an opaque file ID grants nothing.
//! Binary bodies stream separately from the SDK's bounded JSON API. No automatic
//! retry or compensation deletion is performed after an uncertain write result.
use crate::{CallError, Identity, Method, call_error, valid_uuid};
use axum::{body::Body, http::header, response::Response};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{io, time::Duration};

pub const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileStatus {
    Pending,
    Ready,
    Attached,
    Cancelled,
    Expired,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMetadata {
    pub id: String,
    pub version: u32,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub sha256: Option<String>,
    pub status: FileStatus,
    pub reference: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCreate {
    pub request_id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
}

fn valid_reference(reference: &str) -> bool {
    !reference.trim().is_empty()
        && reference.chars().count() <= 512
        && !reference.chars().any(char::is_control)
}

fn file_path(id: &str, tail: &str) -> Result<String, CallError> {
    if !valid_uuid(id) {
        return Err(call_error("invalid_file_id"));
    }
    Ok(format!("platform/files/{id}{tail}"))
}

fn metadata(
    value: serde_json::Value,
    expected_id: Option<&str>,
) -> Result<FileMetadata, CallError> {
    let file: FileMetadata =
        serde_json::from_value(value).map_err(|_| call_error("invalid_file_response"))?;
    if !valid_uuid(&file.id)
        // Core answers in lowercase; a caller may hold the same ID in uppercase.
        || expected_id.is_some_and(|id| !id.eq_ignore_ascii_case(&file.id))
        || file.version != 1
        || file.size > MAX_FILE_BYTES
        || file.name.is_empty()
        || file
            .sha256
            .as_ref()
            .is_some_and(|value| value.len() != 64 || !value.bytes().all(|c| c.is_ascii_hexdigit()))
        || (matches!(file.status, FileStatus::Ready | FileStatus::Attached)
            && file.sha256.is_none())
    {
        return Err(call_error("invalid_file_response"));
    }
    Ok(file)
}

async fn response_json(mut response: reqwest::Response) -> Result<serde_json::Value, CallError> {
    if !response.status().is_success() {
        return Err(CallError {
            code: "core_call_rejected",
            status: Some(response.status().as_u16()),
        });
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| call_error("file_result_unknown"))?
    {
        if bytes.len() + chunk.len() > 16 * 1024 {
            return Err(call_error("invalid_file_response"));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| call_error("file_result_unknown"))
}

impl Identity {
    /// Reserve storage after the app has authorized creation of an attachment.
    pub async fn files_create(&self, input: &FileCreate) -> Result<FileMetadata, CallError> {
        if !valid_uuid(&input.request_id) || input.size > MAX_FILE_BYTES {
            return Err(call_error("invalid_file_request"));
        }
        let body = serde_json::to_value(input).map_err(|_| call_error("invalid_file_request"))?;
        metadata(
            self.call("platform/files", Method::POST, Some(&body))
                .await?,
            None,
        )
    }

    /// Resolve an uncertain reservation result by its original request ID.
    pub async fn files_by_request(&self, request_id: &str) -> Result<FileMetadata, CallError> {
        if !valid_uuid(request_id) {
            return Err(call_error("invalid_file_request"));
        }
        metadata(
            self.call(
                &format!("platform/files/by-request/{request_id}"),
                Method::GET,
                None,
            )
            .await?,
            None,
        )
    }

    pub async fn files_get(&self, id: &str) -> Result<FileMetadata, CallError> {
        metadata(
            self.call(&file_path(id, "")?, Method::GET, None).await?,
            Some(id),
        )
    }

    /// Mark a reference only after the owning app's attachment transaction has
    /// committed. An uncertain result must be queried, never compensated by delete.
    pub async fn files_attach(
        &self,
        id: &str,
        reference: &str,
        request_id: &str,
    ) -> Result<FileMetadata, CallError> {
        if !valid_uuid(request_id) || !valid_reference(reference) {
            return Err(call_error("invalid_file_reference"));
        }
        let body = serde_json::json!({"reference":reference,"requestId":request_id});
        metadata(
            self.call(&file_path(id, "/attach")?, Method::POST, Some(&body))
                .await?,
            Some(id),
        )
    }

    /// Cancels only an explicitly abandoned, unattached object. Ready/attached
    /// state and uncertainty about the app transaction are not evidence of an orphan.
    /// An attached file is released with [`Identity::files_detach`].
    pub async fn files_cancel(&self, id: &str) -> Result<FileMetadata, CallError> {
        metadata(
            self.call(&file_path(id, "")?, Method::DELETE, None).await?,
            Some(id),
        )
    }

    /// Release an attached file: Core deletes the bytes and returns the quota.
    /// Call it only after the app transaction that removed the reference has
    /// committed, with the reference given at attach. The result is `cancelled`.
    /// Repeating the call is safe, so an uncertain result is simply sent again;
    /// until it succeeds the file stays attached and keeps its quota.
    pub async fn files_detach(&self, id: &str, reference: &str) -> Result<FileMetadata, CallError> {
        if !valid_reference(reference) {
            return Err(call_error("invalid_file_reference"));
        }
        let body = serde_json::json!({"reference":reference});
        metadata(
            self.call(&file_path(id, "/detach")?, Method::POST, Some(&body))
                .await?,
            Some(id),
        )
    }

    /// Stream one reserved file. The app must use the metadata returned by Core
    /// and recheck its current upload/resource ACL before handing over the body.
    pub async fn files_upload(
        &self,
        file: &FileMetadata,
        body: Body,
    ) -> Result<FileMetadata, CallError> {
        if file.status != FileStatus::Pending || file.size > MAX_FILE_BYTES {
            return Err(call_error("file_not_pending"));
        }
        let path = file_path(&file.id, "/content")?;
        let expected = file.size;
        let mut count = 0_u64;
        let stream = body.into_data_stream().map(move |chunk| {
            let bytes = chunk.map_err(|_| io::Error::other("file_body_failed"))?;
            count = count.saturating_add(bytes.len() as u64);
            if count > expected {
                return Err(io::Error::other("file_size_mismatch"));
            }
            Ok(bytes)
        });
        let response = self
            .backend_request(&path, Method::PUT, Duration::from_secs(20))?
            .header(header::CONTENT_TYPE, "application/octet-stream")
            .header(header::CONTENT_LENGTH, file.size)
            .body(reqwest::Body::wrap_stream(stream))
            .send()
            .await
            .map_err(|_| call_error("file_result_unknown"))?;
        metadata(response_json(response).await?, Some(&file.id))
    }

    /// Stream an immutable object through the owning app's authenticated file
    /// route. The app must authorize download each time; no public URL is issued.
    pub async fn files_download(&self, id: &str) -> Result<Response, CallError> {
        let response = self
            .backend_request(
                &file_path(id, "/content")?,
                Method::GET,
                Duration::from_secs(20),
            )?
            .send()
            .await
            .map_err(|_| call_error("file_download_failed"))?;
        if !response.status().is_success() {
            return Err(CallError {
                code: "core_call_rejected",
                status: Some(response.status().as_u16()),
            });
        }
        let size = response
            .content_length()
            .ok_or_else(|| call_error("invalid_file_response"))?;
        if size > MAX_FILE_BYTES {
            return Err(call_error("file_too_large"));
        }
        let mut builder = Response::builder().status(response.status());
        for name in [
            "content-type",
            "content-length",
            "content-disposition",
            "etag",
            "x-miy-file-id",
            "x-miy-file-version",
            "x-miy-file-sha256",
        ] {
            if let Some(value) = response.headers().get(name) {
                builder = builder.header(name, value);
            }
        }
        let mut count = 0_u64;
        let stream = response.bytes_stream().map(move |chunk| {
            let bytes = chunk.map_err(|_| io::Error::other("file_download_failed"))?;
            count = count.saturating_add(bytes.len() as u64);
            if count > size {
                return Err(io::Error::other("file_size_mismatch"));
            }
            Ok(bytes)
        });
        builder
            .header("content-security-policy", "sandbox")
            .header("x-content-type-options", "nosniff")
            .body(Body::from_stream(stream))
            .map_err(|_| call_error("invalid_file_response"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn storage_responses_cannot_substitute_a_file_or_skip_integrity_metadata() {
        let id = "00000000-0000-4000-8000-000000000001";
        let mut value = serde_json::json!({"id":id,"version":1,"name":"report.txt","mime":"text/plain","size":3,"sha256":"a".repeat(64),"status":"ready","reference":null});
        assert!(metadata(value.clone(), Some(id)).is_ok());
        let upper = "00000000-0000-4000-8000-00000000000A";
        let mut lower = value.clone();
        lower["id"] = upper.to_ascii_lowercase().into();
        assert!(metadata(lower, Some(upper)).is_ok());
        assert!(metadata(value.clone(), Some("00000000-0000-4000-8000-000000000002")).is_err());
        value["sha256"] = serde_json::Value::Null;
        assert!(metadata(value.clone(), Some(id)).is_err());
        value["status"] = "pending".into();
        assert!(metadata(value, Some(id)).is_ok());
        assert!(file_path("../another-installation", "/content").is_err());
        assert!(valid_reference("document:1"));
        for reference in ["", " ", "line\nbreak", &"x".repeat(513)] {
            assert!(!valid_reference(reference));
        }
    }
}
