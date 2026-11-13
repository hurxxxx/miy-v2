//! App API responses have an explicit header allowlist: Content-Type only.
//! HTML/SVG returned by an API must not execute as a platform document.
use crate::api_error::{ApiError, ApiResult};
use axum::{
    body::{Body, Bytes},
    http::{HeaderValue, StatusCode, header},
    response::Response,
};
use std::time::Duration;
use tokio::sync::{Semaphore, SemaphorePermit};

/// Largest app API response a gateway holds in memory: the request limit and
/// the Rust SDK's own response limit. Files get a separate streaming path.
pub const LIMIT: usize = 4 * 1024 * 1024;
/// Concurrent buffered responses, including bodies waiting for downstream.
/// This does not cap HTTP/socket buffers or total process memory.
static RESPONSES: Semaphore = Semaphore::const_new(64);

/// Keep the slot with the bytes, even after Body has handed them to Hyper.
/// Field order releases the allocation before admitting another response.
struct BufferedResponse {
    bytes: Vec<u8>,
    _slot: SemaphorePermit<'static>,
}

impl AsRef<[u8]> for BufferedResponse {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

/// Marks a response built from an app's answer. Never sent: listeners use it
/// to tell their own refusals from what an app returned.
#[derive(Clone, Copy, Debug)]
pub struct FromApp;

/// Read an app's response body, stopping at [`LIMIT`] bytes whatever its
/// Content-Length says (absent, chunked or wrong). `gateway` prefixes the
/// stable error codes (`app`, `preview`).
pub async fn read(response: reqwest::Response, gateway: &str) -> ApiResult<Bytes> {
    read_with_slots(response, gateway, &RESPONSES, Duration::from_secs(10)).await
}

async fn read_with_slots(
    mut response: reqwest::Response,
    gateway: &str,
    slots: &'static Semaphore,
    wait: Duration,
) -> ApiResult<Bytes> {
    let too_large = || ApiError::bad_gateway(&format!("{gateway}_response_too_large"));
    let failed = || ApiError::unavailable(&format!("{gateway}_response_failed"));
    if response.content_length().is_some_and(|n| n > LIMIT as u64) {
        return Err(too_large());
    }
    let slot = tokio::time::timeout(wait, slots.acquire())
        .await
        .map_err(|_| ApiError::unavailable(&format!("{gateway}_gateway_busy")))?
        .map_err(|_| failed())?;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| failed())? {
        if chunk.len() > LIMIT - body.len() {
            return Err(too_large());
        }
        let needed = body.len() + chunk.len();
        if needed > body.capacity() {
            // Retain amortized growth without doubling beyond the body limit.
            let capacity = needed.max(body.capacity().saturating_mul(2)).min(LIMIT);
            body.reserve_exact(capacity - body.len());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Bytes::from_owner(BufferedResponse {
        bytes: body,
        _slot: slot,
    }))
}

pub fn api(
    status: StatusCode,
    content_type: Option<HeaderValue>,
    body: impl Into<Body>,
) -> Response {
    let content_type = content_type
        .filter(|value| {
            value.to_str().is_ok_and(|value| {
                let media = value.split(';').next().unwrap_or("").trim();
                let token = |part: &str| {
                    !part.is_empty()
                        && part
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
                };
                value.len() <= 256
                    && media
                        .split_once('/')
                        .is_some_and(|(a, b)| token(a) && token(b))
            })
        })
        .unwrap_or_else(|| HeaderValue::from_static("application/octet-stream"));
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-store")
        .header(
            header::CONTENT_SECURITY_POLICY,
            "sandbox; default-src 'none'; frame-ancestors 'none'; base-uri 'none'",
        )
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::X_FRAME_OPTIONS, "DENY")
        .extension(FromApp)
        .body(body.into())
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{StreamExt, poll, stream};
    use std::task::Poll;

    fn answer(body: impl Into<reqwest::Body>) -> reqwest::Response {
        axum::http::Response::new(body.into()).into()
    }

    /// A local upstream that answers `/{size}/{mode}`: `declared` sets
    /// Content-Length, `chunked` streams 64 KiB chunks without one.
    async fn upstream() -> String {
        use axum::{Router, extract::Path, routing::get};
        let app = Router::new().route(
            "/{size}/{mode}",
            get(|Path((size, mode)): Path<(usize, String)>| async move {
                if mode == "chunked" {
                    let chunks = (0..size.div_ceil(65536)).map(move |i| {
                        let len = (size - i * 65536).min(65536);
                        Ok::<_, std::io::Error>(axum::body::Bytes::from(vec![b'x'; len]))
                    });
                    Body::from_stream(futures_util::stream::iter(chunks))
                } else {
                    Body::from(vec![b'x'; size])
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn read_stops_at_the_limit_with_or_without_content_length() {
        let base = upstream().await;
        let client = reqwest::Client::new();
        for mode in ["declared", "chunked"] {
            let fits = client
                .get(format!("{base}/{LIMIT}/{mode}"))
                .send()
                .await
                .unwrap();
            assert_eq!(read(fits, "app").await.unwrap().len(), LIMIT);
            let over = client
                .get(format!("{base}/{}/{mode}", LIMIT + 1))
                .send()
                .await
                .unwrap();
            let error = read(over, "app").await.unwrap_err();
            assert_eq!(
                (error.status, error.code.as_str()),
                (StatusCode::BAD_GATEWAY, "app_response_too_large")
            );
        }
    }

    #[tokio::test]
    async fn completed_responses_count_towards_the_limit_until_dropped() {
        static SLOTS: Semaphore = Semaphore::const_new(64);
        let mut held = Vec::new();
        for _ in 0..64 {
            let bytes = read_with_slots(answer("body"), "app", &SLOTS, Duration::from_secs(1))
                .await
                .unwrap();
            held.push(api(StatusCode::OK, None, bytes));
        }
        assert_eq!(SLOTS.available_permits(), 0);

        let mut next = Box::pin(read_with_slots(
            answer("next"),
            "app",
            &SLOTS,
            Duration::from_secs(1),
        ));
        assert!(matches!(poll!(next.as_mut()), Poll::Pending));
        drop(held.pop());
        let bytes = next.await.unwrap();
        assert_eq!(bytes.as_ref(), b"next");
        assert_eq!(SLOTS.available_permits(), 0);
        drop(bytes);
        drop(held);
        assert_eq!(SLOTS.available_permits(), 64);
    }

    #[tokio::test]
    async fn slot_follows_data_after_body_is_consumed_and_dropped() {
        static SLOTS: Semaphore = Semaphore::const_new(1);
        let bytes = read_with_slots(answer("body"), "app", &SLOTS, Duration::from_secs(1))
            .await
            .unwrap();
        let response = api(StatusCode::OK, None, bytes);
        let mut stream = response.into_body().into_data_stream();
        // Model a transport retaining the yielded frame while the client is slow.
        let data = stream.next().await.unwrap().unwrap();
        assert!(stream.next().await.is_none());
        drop(stream);
        let clone = data.clone();
        let slice = data.slice(1..);
        drop(data);
        drop(clone);
        assert_eq!(SLOTS.available_permits(), 0);
        assert_eq!(slice.as_ref(), b"ody");
        drop(slice);
        assert_eq!(SLOTS.available_permits(), 1);
    }

    #[tokio::test]
    async fn slow_tcp_client_keeps_the_slot_until_disconnect() {
        use axum::{Router, routing::get};
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::TcpSocket,
            time::{sleep, timeout},
        };

        static SLOTS: Semaphore = Semaphore::const_new(1);
        let app = Router::new().route(
            "/",
            get(|| async {
                let bytes = read_with_slots(
                    answer(vec![b'x'; LIMIT]),
                    "app",
                    &SLOTS,
                    Duration::from_secs(1),
                )
                .await?;
                Ok::<_, ApiError>(api(StatusCode::OK, None, bytes))
            }),
        );
        let socket = TcpSocket::new_v4().unwrap();
        socket.set_send_buffer_size(4096).unwrap();
        socket.bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let listener = socket.listen(16).unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        timeout(Duration::from_secs(10), async {
            let socket = TcpSocket::new_v4().unwrap();
            socket.set_recv_buffer_size(4096).unwrap();
            let mut client = socket.connect(address).await.unwrap();
            client
                .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
            // Read the first response byte, then leave the 4 MiB body unread.
            client.read_exact(&mut [0; 1]).await.unwrap();
            sleep(Duration::from_millis(25)).await;
            assert_eq!(SLOTS.available_permits(), 0);
            let error = read_with_slots(answer("next"), "app", &SLOTS, Duration::from_millis(1))
                .await
                .unwrap_err();
            assert_eq!(error.code, "app_gateway_busy");

            drop(client);
            while SLOTS.available_permits() == 0 {
                sleep(Duration::from_millis(1)).await;
            }
            // A normal client can receive the complete response after cancellation.
            let response = reqwest::get(format!("http://{address}/")).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = response.bytes().await.unwrap();
            assert_eq!(body.len(), LIMIT);
            assert!(body.iter().all(|byte| *byte == b'x'));
            assert_eq!(SLOTS.available_permits(), 1);
        })
        .await
        .expect("slow client should release its response slot on disconnect");
        server.abort();
        let _ = server.await;
    }

    #[tokio::test]
    async fn full_slots_timeout_and_cancel_without_losing_capacity() {
        static SLOTS: Semaphore = Semaphore::const_new(1);
        let held = read_with_slots(answer("body"), "app", &SLOTS, Duration::from_secs(1))
            .await
            .unwrap();
        for gateway in ["app", "preview"] {
            let error =
                read_with_slots(answer("waiting"), gateway, &SLOTS, Duration::from_millis(1))
                    .await
                    .unwrap_err();
            assert_eq!(error.status, StatusCode::SERVICE_UNAVAILABLE);
            assert_eq!(error.code, format!("{gateway}_gateway_busy"));
        }
        let mut waiting = Box::pin(read_with_slots(
            answer("cancelled"),
            "app",
            &SLOTS,
            Duration::from_secs(1),
        ));
        assert!(matches!(poll!(waiting.as_mut()), Poll::Pending));
        drop(waiting);
        assert_eq!(SLOTS.available_permits(), 0);
        drop(held);
        assert_eq!(SLOTS.available_permits(), 1);
        let next = read_with_slots(answer("next"), "app", &SLOTS, Duration::from_secs(1))
            .await
            .unwrap();
        drop(next);
        assert_eq!(SLOTS.available_permits(), 1);
    }

    #[tokio::test]
    async fn cancellation_during_collection_releases_the_slot() {
        static SLOTS: Semaphore = Semaphore::const_new(1);
        let chunks = stream::iter([Ok::<_, std::io::Error>(Bytes::from_static(b"partial"))])
            .chain(stream::pending());
        let response = answer(reqwest::Body::wrap_stream(chunks));
        let mut reading = Box::pin(read_with_slots(
            response,
            "app",
            &SLOTS,
            Duration::from_secs(1),
        ));
        assert!(matches!(poll!(reading.as_mut()), Poll::Pending));
        assert_eq!(SLOTS.available_permits(), 0);
        drop(reading);
        assert_eq!(SLOTS.available_permits(), 1);
    }

    #[tokio::test]
    async fn upstream_failure_and_oversize_release_the_slot() {
        static SLOTS: Semaphore = Semaphore::const_new(1);
        for gateway in ["app", "preview"] {
            let chunks = stream::iter([
                Ok(Bytes::from_static(b"partial")),
                Err(std::io::Error::other("upstream closed")),
            ]);
            let error = read_with_slots(
                answer(reqwest::Body::wrap_stream(chunks)),
                gateway,
                &SLOTS,
                Duration::from_secs(1),
            )
            .await
            .unwrap_err();
            assert_eq!(error.status, StatusCode::SERVICE_UNAVAILABLE);
            assert_eq!(error.code, format!("{gateway}_response_failed"));
            assert_eq!(SLOTS.available_permits(), 1);

            let chunks = stream::iter([
                Ok::<_, std::io::Error>(Bytes::from(vec![b'x'; LIMIT])),
                Ok(Bytes::from_static(b"x")),
            ]);
            let error = read_with_slots(
                answer(reqwest::Body::wrap_stream(chunks)),
                gateway,
                &SLOTS,
                Duration::from_secs(1),
            )
            .await
            .unwrap_err();
            assert_eq!(error.status, StatusCode::BAD_GATEWAY);
            assert_eq!(error.code, format!("{gateway}_response_too_large"));
            assert_eq!(SLOTS.available_permits(), 1);
        }
    }

    #[test]
    fn app_responses_are_marked_for_the_frame_listener() {
        let response = api(StatusCode::UNAUTHORIZED, None, "body");
        assert!(response.extensions().get::<FromApp>().is_some());
    }
    #[test]
    fn api_documents_are_sandboxed_without_changing_json_or_download_content() {
        for value in [
            "text/html",
            "image/svg+xml",
            "application/json; charset=utf-8",
            "application/pdf",
        ] {
            let response = api(
                StatusCode::OK,
                Some(HeaderValue::from_str(value).unwrap()),
                "body",
            );
            assert_eq!(response.headers()[header::CONTENT_TYPE], value);
            assert_eq!(
                response.headers()[header::X_CONTENT_TYPE_OPTIONS],
                "nosniff"
            );
            assert!(
                response.headers()[header::CONTENT_SECURITY_POLICY]
                    .to_str()
                    .unwrap()
                    .starts_with("sandbox;")
            );
            assert!(!response.headers().contains_key("set-cookie"));
            assert!(!response.headers().contains_key("clear-site-data"));
        }
        for value in ["invalid", "text/html, application/json", "text/"] {
            let response = api(
                StatusCode::OK,
                Some(HeaderValue::from_str(value).unwrap()),
                "body",
            );
            assert_eq!(
                response.headers()[header::CONTENT_TYPE],
                "application/octet-stream"
            );
        }
    }
}
