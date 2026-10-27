//! HTTP error contract shared by the core and the local preview gateway.
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use uuid::Uuid;

pub type ApiResult<T> = Result<T, ApiError>;
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: String,
}
impl ApiError {
    pub fn bad(code: &str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: code.into(),
        }
    }
    #[cfg(feature = "server")]
    pub fn bad_owned(code: String) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code,
        }
    }
    pub fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized".into(),
        }
    }
    pub fn forbidden() -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "forbidden".into(),
        }
    }
    #[cfg(feature = "server")]
    pub fn not_found() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found".into(),
        }
    }
    pub fn unavailable(code: &str) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: code.into(),
        }
    }
    pub fn conflict(code: &str) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: code.into(),
        }
    }
}
impl From<anyhow::Error> for ApiError {
    fn from(_: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error".into(),
        }
    }
}
impl From<std::io::Error> for ApiError {
    fn from(_: std::io::Error) -> Self {
        Self::bad("artifact_unavailable")
    }
}
impl From<serde_json::Error> for ApiError {
    fn from(_: serde_json::Error) -> Self {
        Self::bad("invalid_json")
    }
}
impl From<url::ParseError> for ApiError {
    fn from(_: url::ParseError) -> Self {
        Self::bad("invalid_url")
    }
}
impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        if let Some(db) = e.as_database_error() {
            if db.is_unique_violation() {
                return Self::conflict("already_exists");
            }
            if db.is_foreign_key_violation() {
                return Self::bad("unknown_reference");
            }
        }
        tracing::error!(event = "database_operation_failed");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "database_error".into(),
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let id = Uuid::new_v4().to_string();
        (self.status,[("x-request-id",id.clone())],Json(json!({"error":{"code":self.code,"requestId":id,"message":"Check the requested contract, permissions and resource state."}}))).into_response()
    }
}
