//! Bounded release-aware health checks used by preview and server runtime.
use anyhow::Result;
use serde_json::Value;
use std::time::Duration;
use uuid::Uuid;

pub fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}
pub async fn probe(
    client: &reqwest::Client,
    port: i32,
    release: Uuid,
) -> std::result::Result<(), &'static str> {
    // The deadline includes response-body reads; never buffer an unbounded health response.
    tokio::time::timeout(Duration::from_secs(2), async {
        let mut response = client
            .get(format!("http://127.0.0.1:{port}/health"))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    "health_timeout"
                } else {
                    "health_unreachable"
                }
            })?;
        if !response.status().is_success() {
            return Err("health_http_failed");
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| {
            if e.is_timeout() {
                "health_timeout"
            } else {
                "health_invalid_body"
            }
        })? {
            if body.len() + chunk.len() > 4096 {
                return Err("health_invalid_body");
            }
            body.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&body).map_err(|_| "health_invalid_body")?;
        if value["releaseId"] != release.to_string() {
            return Err("health_release_mismatch");
        }
        Ok(())
    })
    .await
    .map_err(|_| "health_timeout")?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn probes_validate_release_body_redirects_and_timeout() -> Result<()> {
        use axum::{Router, response::IntoResponse, routing::get};
        let release = Uuid::new_v4();
        for mode in [
            "healthy",
            "wrong_release",
            "invalid",
            "large",
            "redirect",
            "503",
            "timeout",
        ] {
            let app = Router::new().route(
                "/health",
                get(move || async move {
                    match mode {
                        "healthy" => {
                            axum::Json(serde_json::json!({"releaseId":release})).into_response()
                        }
                        "wrong_release" => {
                            axum::Json(serde_json::json!({"releaseId":Uuid::new_v4()}))
                                .into_response()
                        }
                        "invalid" => "oops".into_response(),
                        "large" => "x".repeat(4097).into_response(),
                        "redirect" => {
                            axum::response::Redirect::temporary("/health").into_response()
                        }
                        "503" => axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response(),
                        _ => {
                            tokio::time::sleep(Duration::from_secs(10)).await;
                            "late".into_response()
                        }
                    }
                }),
            );
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let port = listener.local_addr()?.port();
            let server = tokio::spawn(async move { axum::serve(listener, app).await });
            let started = std::time::Instant::now();
            let result = probe(&client()?, i32::from(port), release).await;
            assert_eq!(result.is_ok(), mode == "healthy", "{mode}: {result:?}");
            if mode == "timeout" {
                assert_eq!(result, Err("health_timeout"));
                assert!(started.elapsed() < Duration::from_secs(4));
            }
            server.abort();
        }
        Ok(())
    }
}
