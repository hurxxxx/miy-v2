//! Short-lived receipts for native frame downloads. Only the issuing frame
//! can read the receipt, and no file bytes are retained here.
use crate::api_error::{ApiError, ApiResult};
use axum::{body::Body, http::header, response::Response};
use futures_util::StreamExt;
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};
use uuid::Uuid;

const RETENTION: Duration = Duration::from_secs(600);
const MAX_RECEIPTS: usize = 2048;
type Receipts = HashMap<String, (Uuid, Status, Instant)>;
static RECEIPTS: LazyLock<Mutex<Receipts>> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Status {
    Pending,
    Streaming,
    Sent,
    Failed,
}

pub(crate) fn status(key: &str, frame: Uuid) -> Option<Status> {
    RECEIPTS
        .lock()
        .unwrap()
        .get(key)
        .filter(|(issuer, _, _)| *issuer == frame)
        .map(|(_, status, _)| *status)
}

pub(crate) struct Transfer {
    key: String,
    sent: bool,
}

impl Transfer {
    pub(crate) fn start(key: String, frame: Uuid) -> ApiResult<Self> {
        let mut receipts = RECEIPTS.lock().unwrap();
        receipts.retain(|_, (_, status, changed)| {
            matches!(status, Status::Pending | Status::Streaming) || changed.elapsed() < RETENTION
        });
        if receipts.contains_key(&key) {
            return Err(ApiError::conflict("download_already_started"));
        }
        if receipts.len() >= MAX_RECEIPTS {
            let oldest = receipts
                .iter()
                .filter(|(_, (_, status, _))| matches!(status, Status::Sent | Status::Failed))
                .min_by_key(|(_, (_, _, changed))| *changed)
                .map(|(key, _)| key.clone());
            receipts.remove(&oldest.ok_or_else(|| ApiError::unavailable("file_download_busy"))?);
        }
        receipts.insert(key.clone(), (frame, Status::Pending, Instant::now()));
        Ok(Self { key, sent: false })
    }

    fn set(&self, status: Status) {
        if let Some((_, current, changed)) = RECEIPTS.lock().unwrap().get_mut(&self.key) {
            *current = status;
            *changed = Instant::now();
        }
    }

    fn sent(&mut self) {
        self.sent = true;
        self.set(Status::Sent);
    }

    pub(crate) fn wrap(mut self, response: Response) -> ApiResult<Response> {
        let length: u64 = response
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|header| header.to_str().ok())
            .and_then(|header| header.parse().ok())
            .ok_or_else(|| ApiError::unavailable("file_bytes_invalid"))?;
        // A new queued transfer is for the whole file. Native untracked links
        // still allow resuming at any byte.
        let full_range = length.checked_sub(1).is_some_and(|end| {
            let expected = format!("bytes 0-{end}/{length}");
            response
                .headers()
                .get(header::CONTENT_RANGE)
                .is_some_and(|value| value == expected.as_str())
        });
        if response.status() != axum::http::StatusCode::OK
            && !(response.status() == axum::http::StatusCode::PARTIAL_CONTENT && full_range)
        {
            return Err(ApiError::conflict("download_whole_file_required"));
        }
        if length == 0 {
            self.sent();
            return Ok(response);
        }
        self.set(Status::Streaming);
        let (parts, body) = response.into_parts();
        let stream = futures_util::stream::try_unfold(
            (Some(body.into_data_stream()), self, length),
            |(input, mut transfer, remaining)| async move {
                let Some(mut input) = input else {
                    return Ok::<_, axum::Error>(None);
                };
                let invalid =
                    || axum::Error::new(std::io::Error::other("incomplete file transfer"));
                let bytes = input.next().await.transpose()?.ok_or_else(invalid)?;
                let remaining = remaining
                    .checked_sub(bytes.len() as u64)
                    .ok_or_else(invalid)?;
                if remaining == 0 {
                    drop(input);
                    transfer.sent();
                    return Ok(Some((bytes, (None, transfer, remaining))));
                }
                Ok(Some((bytes, (Some(input), transfer, remaining))))
            },
        );
        Ok(Response::from_parts(parts, Body::from_stream(stream)))
    }
}

impl Drop for Transfer {
    fn drop(&mut self) {
        if !self.sent {
            self.set(Status::Failed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response() -> Response {
        Response::builder()
            .header(header::CONTENT_LENGTH, 3)
            .body(Body::from("abc"))
            .unwrap()
    }

    #[tokio::test]
    async fn receipts_distinguish_a_finished_stream_from_a_dropped_one() {
        let frame = Uuid::new_v4();
        let first = Uuid::new_v4().to_string();
        let response = Transfer::start(first.clone(), frame)
            .unwrap()
            .wrap(response());
        assert!(status(&first, Uuid::new_v4()).is_none());
        assert!(matches!(status(&first, frame), Some(Status::Streaming)));
        let bytes = axum::body::to_bytes(response.unwrap().into_body(), 3)
            .await
            .unwrap();
        assert_eq!(bytes.as_ref(), b"abc");
        assert!(matches!(status(&first, frame), Some(Status::Sent)));

        let second = Uuid::new_v4().to_string();
        drop(
            Transfer::start(second.clone(), frame)
                .unwrap()
                .wrap(self::response())
                .unwrap(),
        );
        assert!(matches!(status(&second, frame), Some(Status::Failed)));
    }
}
