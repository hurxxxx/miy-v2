//! miy runtime SDK v0.2.0. The platform verifies sessions and signs each forwarded identity.
use axum::{
    Json, Router,
    extract::Request,
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::Deserialize;
use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions};

#[derive(Debug, Clone)]
pub struct Identity {
    pub user_id: String,
    pub admin: bool,
}
#[derive(Deserialize)]
struct Claims {
    sub: String,
    admin: bool,
    installation_id: String,
    release_id: String,
}
async fn identity(mut request: Request, next: Next) -> Response {
    let valid = (|| -> Option<Identity> {
        let token = request.headers().get("x-miy-identity")?.to_str().ok()?;
        let secret = std::env::var("MIY_GATEWAY_SECRET").ok()?;
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_audience(&["miy-app"]);
        validation.set_issuer(&["miy"]);
        validation.leeway = 0;
        let claims = decode::<Claims>(
            token,
            &DecodingKey::from_secret(secret.as_bytes()),
            &validation,
        )
        .ok()?
        .claims;
        if claims.installation_id != std::env::var("MIY_INSTALLATION_ID").ok()?
            || claims.release_id != std::env::var("MIY_RELEASE_ID").ok()?
        {
            return None;
        }
        Some(Identity {
            user_id: claims.sub,
            admin: claims.admin,
        })
    })();
    match valid {
        Some(user) => {
            request.extensions_mut().insert(user);
            next.run(request).await
        }
        None => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":{"code":"invalid_gateway_identity"}})),
        )
            .into_response(),
    }
}
pub async fn database() -> anyhow::Result<PgPool> {
    Ok(PgPoolOptions::new()
        .max_connections(5)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?)
}
pub async fn serve(api: Router) -> anyhow::Result<()> {
    // No direct browser identity headers are trusted. API access always passes through the gateway.
    anyhow::ensure!(
        std::env::var("MIY_GATEWAY_SECRET")?.len() >= 32,
        "missing_gateway_secret"
    );
    let app = Router::new()
        .nest("/api", api.route_layer(middleware::from_fn(identity)))
        .route("/health",get(|| async { Json(json!({"status":"ok","releaseId":std::env::var("MIY_RELEASE_ID").unwrap_or_default()})) }));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
