//! Error responses.
//!
//! AI-DO raises `localized_http_exception(code="pms.task_not_found")` and the
//! client shows the server's translated `detail`. miy's browser transport
//! (`runtime.api`) keeps only `error.code`, and accepts it only when it matches
//! `^[a-z][a-z0-9_]*$`. So the code is AI-DO's code with `.` replaced by `_`,
//! the message is AI-DO's English catalog text (src/error_catalog.rs), and the
//! UI translates the code with AI-DO's Korean/English catalog.
use axum::{
    Json,
    extract::{FromRequest, Request, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Serialize, de::DeserializeOwned};

use crate::error_catalog;

#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct ErrorResponse {
    error: ErrorDetails,
}

#[derive(Serialize, utoipa::ToSchema)]
struct ErrorDetails {
    code: String,
    message: String,
}

/// Error produced by the SDK gateway's signed-identity check.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub(crate) struct IdentityErrorResponse {
    error: IdentityErrorDetails,
}

#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct IdentityErrorDetails {
    code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: String,
    pub message: String,
}

impl ApiError {
    /// `code` is AI-DO's dotted catalog code; the message comes from its catalog.
    pub fn code(status: StatusCode, code: &str) -> Self {
        let message = error_catalog::message(code).unwrap_or("Request failed.");
        Self::new(status, code, message)
    }

    pub fn new(status: StatusCode, code: &str, message: impl Into<String>) -> Self {
        Self {
            status,
            code: code.replace('.', "_"),
            message: message.into(),
        }
    }

    /// A catalog message with `{name}` parameters filled in.
    pub fn with_params(status: StatusCode, code: &str, params: &[(&str, String)]) -> Self {
        let mut message = error_catalog::message(code)
            .unwrap_or("Request failed.")
            .to_string();
        for (name, value) in params {
            message = message.replace(&format!("{{{name}}}"), value);
        }
        Self::new(status, code, message)
    }

    pub fn bad_request(code: &str) -> Self {
        Self::code(StatusCode::BAD_REQUEST, code)
    }

    pub fn forbidden(code: &str) -> Self {
        Self::code(StatusCode::FORBIDDEN, code)
    }

    pub fn not_found(code: &str) -> Self {
        Self::code(StatusCode::NOT_FOUND, code)
    }

    pub fn conflict(code: &str) -> Self {
        Self::code(StatusCode::CONFLICT, code)
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

    /// The platform directory could not answer (miy addition).
    pub fn directory_unavailable() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "pms.directory_unavailable",
            "The user directory is not available. Try again later.",
        )
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorResponse {
                error: ErrorDetails {
                    code: self.code,
                    message: self.message,
                },
            }),
        )
            .into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        // Never return database details (connection strings, SQL) to callers.
        eprintln!("database_error: {}", database_error_kind(&error));
        if let sqlx::Error::Database(db) = &error {
            eprintln!("database_error_code: {:?}", db.code());
        }
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

/// Whether a database error is a unique-constraint violation.
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

/// `Field(ge=..., le=...)` on an integer query/body value.
pub fn check_range(field: &str, value: i64, min: i64, max: Option<i64>) -> ApiResult<()> {
    if value < min {
        return Err(ApiError::validation(format!(
            "{field}: Input should be greater than or equal to {min}"
        )));
    }
    if let Some(max) = max
        && value > max
    {
        return Err(ApiError::validation(format!(
            "{field}: Input should be less than or equal to {max}"
        )));
    }
    Ok(())
}

/// A `Literal[...]` field.
pub fn check_choice(field: &str, value: &str, choices: &[&str]) -> ApiResult<()> {
    if choices.contains(&value) {
        return Ok(());
    }
    Err(ApiError::validation(format!(
        "{field}: Input should be {}",
        choices
            .iter()
            .map(|choice| format!("'{choice}'"))
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_safe_for_the_browser_transport() {
        let error = ApiError::not_found("pms.task_not_found");
        assert_eq!(error.code, "pms_task_not_found");
        assert_eq!(error.message, "Task not found.");
        assert!(
            error
                .code
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        );
    }

    #[test]
    fn parameters_fill_the_catalog_text() {
        let error = ApiError::with_params(
            StatusCode::CONFLICT,
            "pms.status_in_use",
            &[("count", "3".into())],
        );
        assert_eq!(
            error.message,
            "Cannot delete status: 3 task(s) are using it."
        );
    }

    #[test]
    fn lengths_count_characters() {
        assert!(check_length("title", "가나다", 1, 3).is_ok());
        assert!(check_length("title", "가나다라", 1, 3).is_err());
        assert_eq!(
            check_length("title", "", 1, 3).unwrap_err().status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}
