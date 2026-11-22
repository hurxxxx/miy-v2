//! Error responses.
//!
//! AI-DO raises `localized_http_exception(code="dm.thread_not_found")` and the
//! client shows the server's translated `detail`. miy's browser transport
//! (`runtime.api`) keeps only `error.code`, and accepts it only when it matches
//! `^[a-z][a-z0-9_]*$`. So the code is AI-DO's code with `.` replaced by `_`,
//! the message is AI-DO's English catalog text (for logs and API callers), and
//! the UI translates the code with AI-DO's Korean/English catalog
//! (web/src/platform/api/api-error.ts). Same approach as the planner.
use axum::{
    Json,
    extract::{FromRequest, Request, rejection::JsonRejection},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Serialize, de::DeserializeOwned};

#[derive(Serialize, utoipa::ToSchema)]
pub struct ErrorResponse {
    error: ErrorDetail,
}

#[derive(Serialize, utoipa::ToSchema)]
struct ErrorDetail {
    code: String,
    message: String,
}

// The SDK rejects unsigned requests before reaching an application handler.
#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
pub struct GatewayIdentityError {
    error: GatewayIdentityErrorDetail,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
struct GatewayIdentityErrorDetail {
    #[schema(example = "invalid_gateway_identity")]
    code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: String,
    pub message: String,
    /// `Retry-After` seconds (report rate limit).
    pub retry_after: Option<u64>,
}

impl ApiError {
    /// `code` is AI-DO's dotted catalog code.
    pub fn new(status: StatusCode, code: &str, message: impl Into<String>) -> Self {
        Self {
            status,
            code: code.replace('.', "_"),
            message: message.into(),
            retry_after: None,
        }
    }

    pub fn forbidden(code: &str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, message)
    }

    pub fn not_found(code: &str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message)
    }

    pub fn conflict(code: &str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    pub fn unprocessable(code: &str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, code, message)
    }

    /// FastAPI/Pydantic request validation failures answer 422.
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation.request_invalid",
            message,
        )
    }

    pub fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "The request could not be completed.",
        )
    }

    /// A platform call this request depends on (directory, storage) failed.
    pub fn unavailable(code: &str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_GATEWAY, code, message)
    }

    // Shared AI-DO codes.
    pub fn thread_not_found() -> Self {
        Self::not_found("dm.thread_not_found", "DM thread not found.")
    }

    pub fn message_not_found() -> Self {
        Self::not_found("dm.message_not_found", "DM message not found.")
    }

    pub fn attachment_not_found() -> Self {
        Self::not_found("dm.attachment_not_found", "DM attachment not found.")
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (
            self.status,
            Json(ErrorResponse {
                error: ErrorDetail {
                    code: self.code,
                    message: self.message,
                },
            }),
        )
            .into_response();
        if let Some(seconds) = self.retry_after
            && let Ok(value) = HeaderValue::from_str(&seconds.to_string())
        {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
        response
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        // Never return database details (connection strings, SQL) to callers.
        eprintln!("database_error: {}", database_error_kind(&error));
        Self::internal()
    }
}

fn database_error_kind(error: &sqlx::Error) -> &'static str {
    match error {
        sqlx::Error::Database(_) => "database",
        sqlx::Error::PoolTimedOut => "pool_timed_out",
        sqlx::Error::PoolClosed => "pool_closed",
        sqlx::Error::Io(_) => "io",
        sqlx::Error::RowNotFound => "row_not_found",
        sqlx::Error::ColumnDecode { .. } | sqlx::Error::Decode(_) => "decode",
        _ => "other",
    }
}

/// True when the error is a unique-constraint violation (SQLSTATE 23505).
pub fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505"))
}

pub type ApiResult<T> = Result<T, ApiError>;

/// JSON body whose rejections use the error shape above (422 like FastAPI).
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(request, state).await {
            Ok(Json(value)) => Ok(Self(value)),
            Err(rejection) => Err(json_rejection(rejection)),
        }
    }
}

fn json_rejection(rejection: JsonRejection) -> ApiError {
    let reason = match rejection {
        JsonRejection::JsonDataError(error) => error.body_text(),
        JsonRejection::JsonSyntaxError(_) => "JSON decode error".to_string(),
        JsonRejection::MissingJsonContentType(_) => {
            "Expected request with `Content-Type: application/json`".to_string()
        }
        _ => "Invalid request body".to_string(),
    };
    ApiError::validation(reason)
}

/// Pydantic `min_length`/`max_length` count characters, not bytes.
pub fn check_length(field: &str, value: &str, min: usize, max: usize) -> ApiResult<()> {
    let length = value.chars().count();
    if length < min {
        return Err(ApiError::validation(format!(
            "{field}: String should have at least {min} character{}",
            if min == 1 { "" } else { "s" }
        )));
    }
    if length > max {
        return Err(ApiError::validation(format!(
            "{field}: String should have at most {max} characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_safe_for_the_browser_transport() {
        for error in [
            ApiError::thread_not_found(),
            ApiError::forbidden("ugc_terms.acceptance_required", "x"),
            ApiError::unprocessable("dm_safety.report_target_invalid", "x"),
        ] {
            assert!(
                error
                    .code
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
                "{}",
                error.code
            );
        }
        assert_eq!(ApiError::thread_not_found().code, "dm_thread_not_found");
    }

    #[test]
    fn lengths_count_characters() {
        assert!(check_length("title", "가나다", 1, 3).is_ok());
        assert!(check_length("title", "가나다라", 1, 3).is_err());
        assert!(check_length("title", "", 1, 3).is_err());
    }

    #[test]
    fn rate_limit_answers_retry_after() {
        let mut error = ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "dm_safety.report_rate_limited",
            "x",
        );
        error.retry_after = Some(42);
        let response = error.into_response();
        assert_eq!(response.headers()[header::RETRY_AFTER], "42");
    }
}
