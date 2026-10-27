//! Safe, typed failures at the command-line boundary.
use serde::Serialize;
use serde_json::{Value, json};
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Category {
    Internal,
    Usage,
    Auth,
    Forbidden,
    NotFound,
    Conflict,
    Environment,
    Network,
    RemoteFailure,
    Interrupted,
}

impl Category {
    pub fn exit(self) -> i32 {
        match self {
            Self::Internal => 1,
            Self::Usage => 2,
            Self::Auth => 3,
            Self::Forbidden => 4,
            Self::NotFound => 5,
            Self::Conflict => 6,
            Self::Environment => 7,
            Self::Network => 8,
            Self::RemoteFailure => 9,
            Self::Interrupted => 130,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Recovery {
    pub executable: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CliError {
    #[serde(skip)]
    pub category: Category,
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub details: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery: Option<Recovery>,
}

impl CliError {
    pub fn new(category: Category, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            category,
            code: code.into(),
            message: message.into(),
            retryable: category == Category::Network,
            details: json!({}),
            recovery: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.details = details;
        self
    }

    pub fn with_recovery(mut self, executable: &str, args: &[&str]) -> Self {
        self.recovery = Some(Recovery {
            executable: executable.into(),
            args: args.iter().map(|argument| (*argument).into()).collect(),
        });
        self
    }

    pub fn exit(&self) -> i32 {
        self.category.exit()
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CliError {}

/// Only bounded identifiers may cross from an untrusted response into diagnostics.
pub fn safe_code(value: &str) -> Option<&str> {
    (!value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')))
    .then_some(value)
}

fn http_failure(status: reqwest::StatusCode) -> CliError {
    let (category, code, message) = match status.as_u16() {
        400 | 422 => (
            Category::Usage,
            "invalid_request",
            "The server rejected the request input.",
        ),
        401 => (Category::Auth, "unauthorized", "Sign in again to continue."),
        403 => (
            Category::Forbidden,
            "forbidden",
            "The current user does not have permission for this operation.",
        ),
        404 => (
            Category::NotFound,
            "not_found",
            "The requested resource was not found.",
        ),
        409 => (
            Category::Conflict,
            "conflict",
            "The request conflicts with the current resource state.",
        ),
        429 => (
            Category::Network,
            "rate_limited",
            "The server is receiving too many requests. Try again later.",
        ),
        500..=599 => (
            Category::Network,
            "service_unavailable",
            "The server could not complete the request. Try again later.",
        ),
        _ => (
            Category::Network,
            "unexpected_http_status",
            "The server returned an unexpected HTTP status.",
        ),
    };
    CliError::new(category, code, message).with_details(json!({"httpStatus":status.as_u16()}))
}

fn transport_ref(error: &reqwest::Error) -> CliError {
    if let Some(status) = error.status() {
        return http_failure(status);
    }
    if error.is_builder() {
        return CliError::new(
            Category::Environment,
            "invalid_http_configuration",
            "The HTTP client configuration is invalid. Check the server URL and CA configuration.",
        );
    }
    if error.is_timeout() {
        return CliError::new(
            Category::Network,
            "network_timeout",
            "The request timed out. Check the connection and recover the saved request before retrying.",
        );
    }
    if error.is_decode() {
        return CliError::new(
            Category::Network,
            "invalid_response",
            "The server returned an invalid response.",
        );
    }
    CliError::new(
        Category::Network,
        "network_request_failed",
        "The request could not be completed. Check the connection and recover the saved request before retrying.",
    )
}

pub fn transport(error: reqwest::Error) -> CliError {
    transport_ref(&error)
}

/// Preserve explicit domain categories, never infer them from display strings.
pub fn classify(error: &anyhow::Error) -> CliError {
    if let Some(typed) = error.downcast_ref::<CliError>() {
        return typed.clone();
    }
    if let Some(http) = error.downcast_ref::<reqwest::Error>() {
        return transport_ref(http);
    }
    if error.downcast_ref::<std::io::Error>().is_some() {
        return CliError::new(
            Category::Environment,
            "local_io_failed",
            "A local file or process operation failed. Check the path, permissions and available tools.",
        );
    }
    CliError::new(
        Category::Internal,
        "internal_error",
        "The command could not be completed because of an internal error.",
    )
}

/// Wrap only first-party, local validation errors whose messages contain no secrets.
/// Never use this around authentication, remote responses or process execution.
pub fn validation<T>(result: anyhow::Result<T>, code: &'static str) -> anyhow::Result<T> {
    result.map_err(|error| {
        if error.downcast_ref::<CliError>().is_some() {
            error
        } else {
            CliError::new(Category::Environment, code, format!("{error:#}")).into()
        }
    })
}

/// Classify an HTTP failure before considering whether its body is valid JSON.
pub async fn response(response: reqwest::Response) -> anyhow::Result<Value> {
    let status = response.status();
    let header_request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(safe_code)
        .map(str::to_owned);
    let body = response.json::<Value>().await;
    if status.is_success() {
        return body.map_err(|_| {
            CliError::new(
                Category::Network,
                "invalid_response",
                "The server returned an invalid JSON response.",
            )
            .into()
        });
    }

    let mut failure = http_failure(status);
    let body = body.ok();
    if let Some(code) = body
        .as_ref()
        .and_then(|value| value["error"]["code"].as_str())
        .and_then(safe_code)
    {
        failure.code = code.into();
    }
    let request_id = header_request_id.or_else(|| {
        body.as_ref()
            .and_then(|value| value["error"]["requestId"].as_str())
            .and_then(safe_code)
            .map(str::to_owned)
    });
    if let Some(request_id) = request_id {
        failure.details["requestId"] = json!(request_id);
    }
    Err(failure.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(status: u16, body: &str) -> reqwest::Response {
        axum::http::Response::builder()
            .status(status)
            .body(body.to_owned())
            .unwrap()
            .into()
    }

    #[test]
    fn category_exit_codes_are_stable() {
        for (category, expected) in [
            (Category::Internal, 1),
            (Category::Usage, 2),
            (Category::Auth, 3),
            (Category::Forbidden, 4),
            (Category::NotFound, 5),
            (Category::Conflict, 6),
            (Category::Environment, 7),
            (Category::Network, 8),
            (Category::RemoteFailure, 9),
            (Category::Interrupted, 130),
        ] {
            assert_eq!(category.exit(), expected);
        }
    }

    #[test]
    fn typed_failures_survive_context_without_serializing_context_or_category() {
        let error = anyhow::Error::new(
            CliError::new(
                Category::Conflict,
                "state_changed",
                "대상 상태가 변경되었습니다.",
            )
            .with_details(json!({"appId":"example"}))
            .with_recovery("miy", &["app", "status", "space in one argument"]),
        )
        .context("secret-token-in-arbitrary-context");
        let classified = classify(&error);
        assert_eq!(classified.exit(), 6);
        let value = serde_json::to_value(classified).unwrap();
        assert_eq!(value["message"], "대상 상태가 변경되었습니다.");
        assert_eq!(value["recovery"]["args"][2], "space in one argument");
        assert!(value.get("category").is_none());
        assert!(!value.to_string().contains("secret-token"));
    }

    #[test]
    fn unknown_io_and_http_errors_never_expose_their_original_messages() {
        let secret = "secret-token-in-url-or-command-arguments";
        let unknown = anyhow::anyhow!("permission denied {secret}");
        assert_eq!(classify(&unknown).exit(), 1);
        assert!(
            !serde_json::to_string(&classify(&unknown))
                .unwrap()
                .contains(secret)
        );
        let io = anyhow::Error::new(std::io::Error::other(secret)).context(secret);
        assert_eq!(classify(&io).exit(), 7);
        assert!(
            !serde_json::to_string(&classify(&io))
                .unwrap()
                .contains(secret)
        );
        let http = reqwest::Client::new()
            .get(format!("http://[invalid/{secret}"))
            .build()
            .unwrap_err();
        let http = anyhow::Error::new(http).context(secret);
        assert_eq!(classify(&http).exit(), 7);
        assert!(
            !serde_json::to_string(&classify(&http))
                .unwrap()
                .contains(secret)
        );
    }

    #[test]
    fn local_validation_preserves_typed_precedence_and_actionable_local_messages() {
        let typed = anyhow::Error::new(CliError::new(
            Category::Conflict,
            "project_in_use",
            "Close the project MCP before retrying.",
        ))
        .context("local validation context");
        let error = validation::<()>(Err(typed), "bundle_validation_failed").unwrap_err();
        assert_eq!(classify(&error).exit(), 6);
        assert_eq!(classify(&error).code, "project_in_use");
        let error = validation::<()>(
            Err(anyhow::anyhow!(
                "protected file changed: sdk/browser/index.ts"
            )),
            "bundle_validation_failed",
        )
        .unwrap_err();
        assert_eq!(classify(&error).exit(), 7);
        assert_eq!(classify(&error).code, "bundle_validation_failed");
        assert_eq!(
            classify(&error).message,
            "protected file changed: sdk/browser/index.ts"
        );
        assert_eq!(validation(Ok(123), "unused").unwrap(), 123);
    }

    #[tokio::test]
    async fn status_classification_does_not_depend_on_a_json_error_body() {
        for (status, exit) in [
            (400, 2),
            (422, 2),
            (401, 3),
            (403, 4),
            (404, 5),
            (409, 6),
            (429, 8),
            (500, 8),
            (503, 8),
        ] {
            let error = response(reply(status, "<html>secret body</html>"))
                .await
                .unwrap_err();
            let classified = classify(&error);
            assert_eq!(classified.exit(), exit, "HTTP {status}");
            assert_eq!(classified.details["httpStatus"], status);
            assert!(
                !serde_json::to_string(&classified)
                    .unwrap()
                    .contains("secret body")
            );
        }
    }

    #[tokio::test]
    async fn only_safe_server_identifiers_are_retained() {
        let error = response(reply(409, r#"{"error":{"code":"state_changed","requestId":"request-123","message":"secret server text","details":{"token":"secret"}}}"#))
            .await.unwrap_err();
        let classified = classify(&error);
        assert_eq!(classified.code, "state_changed");
        assert_eq!(
            classified.details,
            json!({"httpStatus":409,"requestId":"request-123"})
        );
        assert!(
            !serde_json::to_string(&classified)
                .unwrap()
                .contains("secret")
        );

        let error = response(reply(
            401,
            r#"{"error":{"code":"unsafe code\nsecret","requestId":"Bearer secret"}}"#,
        ))
        .await
        .unwrap_err();
        let classified = classify(&error);
        assert_eq!(classified.code, "unauthorized");
        assert_eq!(classified.details, json!({"httpStatus":401}));
        assert!(safe_code(&"a".repeat(97)).is_none());
    }

    #[tokio::test]
    async fn successful_json_keeps_utf8_and_invalid_json_is_network_failure() {
        assert_eq!(
            response(reply(200, r#"{"name":"한글 앱"}"#)).await.unwrap(),
            json!({"name":"한글 앱"})
        );
        let error = response(reply(200, "secret invalid response"))
            .await
            .unwrap_err();
        let classified = classify(&error);
        assert_eq!(classified.exit(), 8);
        assert_eq!(classified.code, "invalid_response");
        assert!(
            !serde_json::to_string(&classified)
                .unwrap()
                .contains("secret")
        );
    }
}
