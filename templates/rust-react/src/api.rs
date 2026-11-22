use axum::Json;
use serde::Serialize;
use sqlx::PgPool;
use std::{fs, path::Path};
use utoipa::{ToSchema, openapi::Info};
use utoipa_axum::{router::OpenApiRouter, routes};

#[derive(Serialize, ToSchema)]
struct PingResponse {
    message: String,
}

/// Check that the app API is reachable from its own frame.
#[utoipa::path(
    get,
    path = "/ping",
    operation_id = "getPing",
    responses((status = 200, description = "App is ready", body = PingResponse))
)]
async fn ping() -> Json<PingResponse> {
    Json(PingResponse {
        message: "개발 준비가 완료됐습니다.".into(),
    })
}

fn router() -> OpenApiRouter<PgPool> {
    let mut router = OpenApiRouter::new().routes(routes!(ping));
    router.get_openapi_mut().info = Info::new("App API", env!("CARGO_PKG_VERSION"));
    router
}

pub(super) async fn serve() -> anyhow::Result<()> {
    let pool = miy_app_sdk::database().await?;
    miy_app_sdk::serve(router().with_state(pool).into()).await
}

pub(super) fn handle_cli(command: Option<&str>) -> anyhow::Result<bool> {
    let path = Path::new("docs/openapi.json");
    match command {
        Some("--write-openapi") => {
            let json = serde_json::to_string_pretty(router().get_openapi())? + "\n";
            fs::write(path, json)?;
        }
        Some("--check-openapi") => {
            let current: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
            anyhow::ensure!(
                current == serde_json::to_value(router().get_openapi())?,
                "docs/openapi.json is out of date. Run pnpm api:generate."
            );
        }
        _ => return Ok(false),
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn documented_ping_serves_json() {
        let (router, spec) = router().split_for_parts();
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgresql://127.0.0.1/test")
            .unwrap();
        let response = router
            .with_state(pool)
            .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 512).await.unwrap();
        assert!(serde_json::from_slice::<serde_json::Value>(&body).unwrap()["message"].is_string());
        assert_eq!(
            serde_json::to_value(spec).unwrap()["paths"]["/ping"]["get"]["operationId"],
            "getPing"
        );
    }
}
