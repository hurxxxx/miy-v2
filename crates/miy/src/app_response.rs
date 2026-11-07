//! App API responses have an explicit header allowlist: Content-Type only.
//! HTML/SVG returned by an API must not execute as a platform document.
use axum::{
    body::Body,
    http::{HeaderValue, StatusCode, header},
    response::Response,
};

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
        .body(body.into())
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
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
