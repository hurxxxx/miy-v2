//! Bounded streaming for the installed-app and local-preview file gateways.
//! These slots are separate from Storage slots: a gateway may call Storage.
use crate::{
    api_error::{ApiError, ApiResult},
    storage_contract::{MAX_FILE_BYTES, TRANSFER_SECONDS},
};
use axum::{
    body::Body,
    http::{HeaderMap, header},
    response::Response,
};
use futures_util::StreamExt;
use std::{io, time::Duration};
use tokio::sync::{Semaphore, SemaphorePermit};

static TRANSFERS: Semaphore = Semaphore::const_new(4);
// A native preview's browser gateway can call its private backend gateway.
// Separate budgets prevent four outer requests from starving all inner calls.
static BACKEND_TRANSFERS: Semaphore = Semaphore::const_new(4);

pub fn acquire() -> ApiResult<SemaphorePermit<'static>> {
    TRANSFERS
        .try_acquire()
        .map_err(|_| ApiError::unavailable("file_transfer_busy"))
}

pub fn acquire_backend() -> ApiResult<SemaphorePermit<'static>> {
    BACKEND_TRANSFERS
        .try_acquire()
        .map_err(|_| ApiError::unavailable("file_transfer_busy"))
}

pub fn check_length(headers: &HeaderMap) -> ApiResult<()> {
    if let Some(value) = headers.get(header::CONTENT_LENGTH) {
        let size = value
            .to_str()
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or_else(|| ApiError::bad("invalid_file_content_length"))?;
        if size > MAX_FILE_BYTES {
            return Err(ApiError::bad("file_too_large"));
        }
    }
    Ok(())
}

pub fn request_body(body: Body) -> reqwest::Body {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(TRANSFER_SECONDS);
    let stream = futures_util::stream::try_unfold(
        (Box::pin(body.into_data_stream()), 0u64),
        move |(mut source, count)| async move {
            let chunk = tokio::time::timeout_at(deadline, source.next())
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "file_transfer_timeout"))?;
            let Some(chunk) = chunk else {
                return Ok(None);
            };
            let chunk = chunk.map_err(|_| io::Error::other("file_transfer_failed"))?;
            let count = count.saturating_add(chunk.len() as u64);
            if count > MAX_FILE_BYTES {
                return Err(io::Error::other("file_too_large"));
            }
            Ok(Some((chunk, (source, count))))
        },
    );
    reqwest::Body::wrap_stream(stream)
}

pub fn secure_headers(headers: &mut HeaderMap) {
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        "sandbox; default-src 'none'".parse().unwrap(),
    );
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    headers.insert(header::X_FRAME_OPTIONS, "DENY".parse().unwrap());
}

pub fn response(
    upstream: reqwest::Response,
    slot: SemaphorePermit<'static>,
) -> ApiResult<Response> {
    check_length(upstream.headers())?;
    let status = upstream.status();
    let mut headers = HeaderMap::new();
    // No Set-Cookie, Clear-Site-Data, Location or arbitrary X-Miy-* headers.
    for name in [
        "content-type",
        "content-length",
        "etag",
        "x-miy-file-id",
        "x-miy-file-version",
        "x-miy-file-sha256",
    ] {
        if let Some(value) = upstream.headers().get(name) {
            headers.insert(name, value.clone());
        }
    }
    let disposition = upstream
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .filter(|v| v.to_str().is_ok_and(|v| v.starts_with("attachment;")))
        .cloned()
        .unwrap_or_else(|| "attachment; filename=\"download\"".parse().unwrap());
    headers.insert(header::CONTENT_DISPOSITION, disposition);
    secure_headers(&mut headers);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(TRANSFER_SECONDS);
    let stream = futures_util::stream::try_unfold(
        (upstream, 0u64, slot),
        move |(mut response, count, slot)| async move {
            let chunk = tokio::time::timeout_at(deadline, response.chunk())
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "file_transfer_timeout"))?
                .map_err(|_| io::Error::other("file_transfer_failed"))?;
            let Some(chunk) = chunk else {
                return Ok(None);
            };
            let count = count.saturating_add(chunk.len() as u64);
            if count > MAX_FILE_BYTES {
                return Err(io::Error::other("file_too_large"));
            }
            Ok(Some((chunk, (response, count, slot))))
        },
    );
    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
        .extensions_mut()
        .insert(crate::app_response::FromApp);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_length_is_bounded_independently_from_json_api() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_LENGTH,
            MAX_FILE_BYTES.to_string().parse().unwrap(),
        );
        assert!(check_length(&headers).is_ok());
        headers.insert(
            header::CONTENT_LENGTH,
            (MAX_FILE_BYTES + 1).to_string().parse().unwrap(),
        );
        assert!(check_length(&headers).is_err());
        headers.insert(header::CONTENT_LENGTH, "invalid".parse().unwrap());
        assert!(check_length(&headers).is_err());
    }

    #[tokio::test]
    async fn file_response_forwards_only_the_explicit_attachment_headers() {
        let upstream = axum::http::Response::builder()
            .status(401)
            .header("content-type", "text/html")
            .header("content-length", "7")
            .header("content-disposition", "inline")
            .header("set-cookie", "unsafe=test")
            .header("clear-site-data", "\"cookies\"")
            .header("location", "https://invalid.example/")
            .header("x-miy-file-id", "public-id")
            .header("x-miy-file-private", "blocked")
            .body("payload")
            .unwrap();
        let reply = response(reqwest::Response::from(upstream), acquire().unwrap()).unwrap();
        assert_eq!(reply.status(), 401);
        assert!(
            reply
                .extensions()
                .get::<crate::app_response::FromApp>()
                .is_some()
        );
        for forbidden in [
            "set-cookie",
            "clear-site-data",
            "location",
            "x-miy-file-private",
        ] {
            assert!(!reply.headers().contains_key(forbidden));
        }
        assert_eq!(reply.headers()["x-miy-file-id"], "public-id");
        assert_eq!(
            reply.headers()["content-security-policy"],
            "sandbox; default-src 'none'"
        );
        assert!(
            reply.headers()["content-disposition"]
                .to_str()
                .unwrap()
                .starts_with("attachment;")
        );
        assert_eq!(
            axum::body::to_bytes(reply.into_body(), 100).await.unwrap(),
            "payload"
        );
    }
}
