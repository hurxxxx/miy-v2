use axum::{
    Extension, Json, Router,
    extract::{DefaultBodyLimit, Path, State, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch},
};
use chrono::{DateTime, Utc};
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

const MAX_TITLE_LENGTH: usize = 200;

pub fn router() -> Router<PgPool> {
    Router::new()
        .route("/todos", get(list).post(create))
        .route("/todos/{id}", patch(set_completed).delete(remove))
        .layer(DefaultBodyLimit::max(16 * 1024))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
struct Todo {
    id: Uuid,
    title: String,
    completed: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateTodo {
    title: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetCompleted {
    completed: bool,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
}

impl ApiError {
    fn invalid_title() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_title",
            message: "할 일은 앞뒤 공백 제거 후 1~200자로 입력하고 제어 문자를 사용하지 마세요.",
        }
    }

    fn not_found() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "todo_not_found",
            message: "할 일을 찾을 수 없습니다. 목록을 새로고침해 주세요.",
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({"error": {"code": self.code, "message": self.message}})),
        )
            .into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(_: sqlx::Error) -> Self {
        // Never expose SQL, connection strings, or other owners' data.
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "database_error",
            message: "저장소에 연결할 수 없습니다. 잠시 후 목록을 새로고침해 주세요.",
        }
    }
}

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        Self {
            status: match rejection.status() {
                StatusCode::UNSUPPORTED_MEDIA_TYPE => StatusCode::UNSUPPORTED_MEDIA_TYPE,
                StatusCode::PAYLOAD_TOO_LARGE => StatusCode::PAYLOAD_TOO_LARGE,
                _ => StatusCode::BAD_REQUEST,
            },
            code: "invalid_json",
            message: "요청의 JSON 형식과 필드를 확인해 주세요.",
        }
    }
}

fn validate_title(title: &str) -> Result<String, ApiError> {
    let trimmed = title.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > MAX_TITLE_LENGTH
        || title.chars().any(char::is_control)
    {
        return Err(ApiError::invalid_title());
    }
    Ok(trimmed.to_owned())
}

fn parse_id(value: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(value).map_err(|_| ApiError {
        status: StatusCode::BAD_REQUEST,
        code: "invalid_id",
        message: "올바른 할 일 ID가 아닙니다.",
    })
}

async fn list(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let items = sqlx::query_as::<_, Todo>(
        "SELECT id, title, completed, created_at, updated_at FROM app.todos
         WHERE owner_id = $1 ORDER BY created_at DESC, id DESC",
    )
    .bind(&identity.user_id)
    .fetch_all(&pool)
    .await?;
    Ok(Json(json!({"items": items})))
}

async fn create(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    body: Result<Json<CreateTodo>, JsonRejection>,
) -> Result<(StatusCode, Json<Todo>), ApiError> {
    let Json(input) = body?;
    let title = validate_title(&input.title)?;
    let item = sqlx::query_as::<_, Todo>(
        "INSERT INTO app.todos (id, owner_id, title) VALUES ($1, $2, $3)
         RETURNING id, title, completed, created_at, updated_at",
    )
    .bind(Uuid::new_v4())
    .bind(&identity.user_id)
    .bind(title)
    .fetch_one(&pool)
    .await?;
    Ok((StatusCode::CREATED, Json(item)))
}

async fn set_completed(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(id): Path<String>,
    body: Result<Json<SetCompleted>, JsonRejection>,
) -> Result<Json<Todo>, ApiError> {
    let id = parse_id(&id)?;
    let Json(input) = body?;
    let item = sqlx::query_as::<_, Todo>(
        "UPDATE app.todos SET completed = $1,
             updated_at = CASE WHEN completed <> $1 THEN now() ELSE updated_at END
         WHERE id = $2 AND owner_id = $3
         RETURNING id, title, completed, created_at, updated_at",
    )
    .bind(input.completed)
    .bind(id)
    .bind(&identity.user_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(ApiError::not_found)?;
    Ok(Json(item))
}

async fn remove(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = parse_id(&id)?;
    let result = sqlx::query("DELETE FROM app.todos WHERE id = $1 AND owner_id = $2")
        .bind(id)
        .bind(&identity.user_id)
        .execute(&pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    Ok(Json(json!({"deleted": true, "id": id})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_surrounding_unicode_whitespace_and_keeps_internal_spaces() {
        assert_eq!(validate_title("　 장보기  목록  ").unwrap(), "장보기  목록");
    }

    #[test]
    fn rejects_blank_or_control_characters() {
        for title in ["", "   ", "　", "할\n일", "\t할 일", "할\0일", "할\u{7f}일"] {
            assert_eq!(validate_title(title).unwrap_err().code, "invalid_title");
        }
    }

    #[test]
    fn counts_unicode_characters_instead_of_utf8_bytes() {
        for character in ["가", "📝"] {
            assert!(validate_title(&character.repeat(200)).is_ok());
            assert!(validate_title(&character.repeat(201)).is_err());
        }
    }

    #[test]
    fn rejects_caller_supplied_ownership_and_admin_fields() {
        for field in ["owner_id", "userId", "admin"] {
            let mut input = json!({"title": "테스트"});
            input[field] = json!("forged");
            assert!(serde_json::from_value::<CreateTodo>(input).is_err());
        }
        assert!(
            serde_json::from_value::<SetCompleted>(
                json!({"completed": true, "owner_id": "forged"})
            )
            .is_err()
        );
    }

    #[test]
    fn requires_an_explicit_boolean_completion_state() {
        for input in [
            json!({}),
            json!({"completed": "true"}),
            json!({"completed": null}),
        ] {
            assert!(serde_json::from_value::<SetCompleted>(input).is_err());
        }
        assert!(
            !serde_json::from_value::<SetCompleted>(json!({"completed": false}))
                .unwrap()
                .completed
        );
    }

    #[test]
    fn malformed_ids_and_database_errors_are_safe() {
        assert_eq!(
            parse_id("not-a-uuid").unwrap_err().status,
            StatusCode::BAD_REQUEST
        );
        let error = ApiError::from(sqlx::Error::Protocol("private connection details".into()));
        assert_eq!(error.code, "database_error");
        assert!(!error.message.contains("private"));
    }
}
