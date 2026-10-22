use axum::{Json, Router, routing::get};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = miy_app_sdk::database().await?;
    let app = Router::new()
        .route(
            "/ping",
            get(|| async { Json(json!({"message":"Ready to build"})) }),
        )
        .with_state(pool);
    miy_app_sdk::serve(app).await
}
