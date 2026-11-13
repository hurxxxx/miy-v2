//! App API responses have an explicit header allowlist: Content-Type only.
//! HTML/SVG returned by an API must not execute as a platform document.
use crate::api_error::{ApiError, ApiResult};
use axum::{
    body::Body,
    http::{HeaderValue, StatusCode, header},
    response::Response,
};
use std::time::Duration;

/// Largest app API response a gateway holds in memory: the request limit and
/// the Rust SDK's own response limit. Files get a separate streaming path.
pub const LIMIT: usize = 4 * 1024 * 1024;
/// Bodies read at the same time. With [`LIMIT`] this bounds the gateway's
/// growing buffers (64 x 4 MiB) whatever the number of open requests.
static READS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(64);

/// Marks a response built from an app's answer. Never sent: listeners use it
/// to tell their own refusals from what an app returned.
#[derive(Clone, Copy, Debug)]
pub struct FromApp;

/// Read an app's response body, stopping at [`LIMIT`] bytes whatever its
/// Content-Length says (absent, chunked or wrong). `gateway` prefixes the
/// stable error codes (`app`, `preview`).
pub async fn read(mut response: reqwest::Response, gateway: &str) -> ApiResult<Vec<u8>> {
    let too_large = || ApiError::bad_gateway(&format!("{gateway}_response_too_large"));
    let failed = || ApiError::unavailable(&format!("{gateway}_response_failed"));
    if response.content_length().is_some_and(|n| n > LIMIT as u64) {
        return Err(too_large());
    }
    let _slot = tokio::time::timeout(Duration::from_secs(10), READS.acquire())
        .await
        .map_err(|_| ApiError::unavailable(&format!("{gateway}_gateway_busy")))?
        .map_err(|_| failed())?;
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| failed())? {
        if body.len() + chunk.len() > LIMIT {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
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
