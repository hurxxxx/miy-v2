//! User-to-owning-app file gateway. The app checks its current row/file ACL;
//! only that backend receives the continuation proof for Core Storage.
use crate::{
    State,
    auth::Principal,
    http::{ApiError, ApiResult},
};
use axum::{
    Extension,
    extract::{Path, Request, State as S},
    http::{Method, header},
    response::Response,
};
use std::time::Duration;
use uuid::Uuid;

pub async fn proxy(
    S(state): S<State>,
    Extension(p): Extension<Principal>,
    Path((id, path)): Path<(Uuid, String)>,
    request: Request,
) -> ApiResult<Response> {
    if !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    ) {
        return Err(ApiError::bad("unsupported_file_method"));
    }
    let slot = crate::file_transfer::acquire()?;
    crate::file_transfer::check_length(request.headers())?;
    let (parts, body) = request.into_parts();
    // The same admission as the JSON gateway; only the transport below differs.
    let (port, token) =
        crate::runtime::app_target(&state, &p, id, &parts, &path, "invalid_file_api_path").await?;
    // Construct path segments, not string-concatenated URLs: a decoded '?' or '#'
    // in an app path must never change the target endpoint or query.
    let mut url = reqwest::Url::parse(&format!("http://127.0.0.1:{port}/"))?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| ApiError::bad("invalid_file_api_path"))?;
        segments.clear().push("api").push("files");
        for segment in path.split('/') {
            segments.push(segment);
        }
    }
    url.set_query(parts.uri.query());
    let client = reqwest::Client::builder()
        .no_proxy()
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(
            crate::storage_contract::TRANSFER_SECONDS,
        ))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| ApiError::unavailable("app_gateway_failed"))?;
    let mut target = client
        .request(parts.method, url)
        .header("x-miy-identity", token)
        .body(crate::file_transfer::request_body(body));
    for name in [header::CONTENT_TYPE, header::CONTENT_LENGTH] {
        if let Some(value) = parts.headers.get(&name) {
            target = target.header(name, value);
        }
    }
    let upstream = target
        .send()
        .await
        .map_err(|_| ApiError::unavailable("app_runtime_unreachable"))?;
    crate::file_transfer::response(upstream, slot)
}
