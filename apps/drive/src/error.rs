//! Error responses.
//!
//! AI-DO raises `localized_http_exception(code="files.file_not_found")` and
//! the client shows the server's translated `detail`. miy's browser transport
//! (`runtime.api`) keeps only `error.code`, and accepts it only when it matches
//! `^[a-z][a-z0-9_]*$`. So the code is AI-DO's code with `.` replaced by `_`,
//! the message is AI-DO's English catalog text (for logs and API callers), and
//! the UI translates the code with AI-DO's Korean/English catalog
//! (web/src/platform/api/api-error.ts).
use axum::{
    Json,
    extract::{FromRequest, Request, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Serialize, de::DeserializeOwned};

#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct ErrorResponse {
    error: ErrorDetails,
}

#[derive(Serialize, utoipa::ToSchema)]
struct ErrorDetails {
    code: String,
    message: String,
}

// A rejected signed identity comes from the SDK before the app router.
#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
pub(crate) struct IdentityErrorResponse {
    error: IdentityErrorDetails,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
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
    /// `code` is AI-DO's dotted catalog code.
    pub fn new(status: StatusCode, code: &str, message: impl Into<String>) -> Self {
        Self {
            status,
            code: code.replace('.', "_"),
            message: message.into(),
        }
    }

    pub fn not_found(code: &str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, code, message)
    }

    pub fn forbidden(code: &str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, message)
    }

    pub fn conflict(code: &str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    pub fn unprocessable(code: &str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, code, message)
    }

    pub fn unavailable(code: &str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, code, message)
    }

    pub fn too_large(code: &str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::PAYLOAD_TOO_LARGE, code, message)
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

/// The SDK resource ACL answers `{error:{code}}` with codes such as
/// `acl_denied`; keep its status and code.
impl From<miy_app_sdk::acl::AclError> for ApiError {
    fn from(error: miy_app_sdk::acl::AclError) -> Self {
        Self {
            status: StatusCode::from_u16(error.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            code: error.code.to_string(),
            message: error.code.to_string(),
        }
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
        let error = ApiError::not_found("files.file_not_found", "File not found.");
        assert_eq!(error.code, "files_file_not_found");
        assert!(
            error
                .code
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        );
    }

    #[test]
    fn lengths_count_characters() {
        assert!(check_length("title", "가나다", 1, 3).is_ok());
        assert!(check_length("title", "가나다라", 1, 3).is_err());
        assert!(check_length("title", "", 1, 3).is_err());
        assert_eq!(
            check_length("title", "", 1, 3).unwrap_err().status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}
