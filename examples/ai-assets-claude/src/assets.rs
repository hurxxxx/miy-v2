//! 비품대장 API. Every query is scoped to the verified gateway identity; the
//! owner is never read from a request body.

use axum::{
    Extension, Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch},
};
use miy_app_sdk::Identity;
use serde::Serialize;
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use uuid::Uuid;

pub const NAME_MAX_CHARS: usize = 100;
pub const QUANTITY_MIN: i64 = 0;
pub const QUANTITY_MAX: i64 = 100_000;
pub const LOCATION_MAX_CHARS: usize = 100;

const ASSET_COLUMNS: &str = "id, name, quantity, location";

pub fn router(pool: PgPool) -> Router {
    Router::new()
        .route("/assets", get(list_assets).post(create_asset))
        .route("/assets/{id}", patch(update_asset).delete(delete_asset))
        .with_state(pool)
}

/// Public representation. The owner is intentionally not serialized.
#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct Asset {
    pub id: Uuid,
    pub name: String,
    pub quantity: i32,
    /// Optional storage location; `null` for records created before 0003.
    pub location: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewAsset {
    pub name: String,
    pub quantity: i32,
    pub location: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AssetPatch {
    pub name: Option<String>,
    pub quantity: Option<i32>,
    /// `None`: leave unchanged. `Some(None)`: clear. `Some(Some(v))`: set.
    pub location: Option<Option<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code,
            message: message.into(),
        }
    }

    fn not_found() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "asset_not_found",
            message: "비품을 찾을 수 없습니다.".into(),
        }
    }

    fn database(error: sqlx::Error) -> Self {
        // Log the cause server-side only; never echo connection details.
        eprintln!("assets database error: {error}");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "database_error",
            message: "데이터를 처리하지 못했습니다. 잠시 후 다시 시도하세요.".into(),
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

type ApiResult<T> = Result<T, ApiError>;

pub fn validate_name(value: &Value) -> ApiResult<String> {
    let raw = value
        .as_str()
        .ok_or_else(|| ApiError::bad_request("invalid_name", "비품명은 문자열이어야 합니다."))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request(
            "invalid_name",
            "비품명을 입력하세요.",
        ));
    }
    if trimmed.chars().count() > NAME_MAX_CHARS {
        return Err(ApiError::bad_request(
            "invalid_name",
            format!("비품명은 {NAME_MAX_CHARS}자 이하여야 합니다."),
        ));
    }
    Ok(trimmed.to_string())
}

pub fn validate_quantity(value: &Value) -> ApiResult<i32> {
    let range_message =
        format!("수량은 {QUANTITY_MIN} 이상 {QUANTITY_MAX} 이하의 정수여야 합니다.");
    // serde_json parses `1.0`, `1.5` and `1e3` as floats, so as_i64 rejects them.
    let number = match value {
        Value::Number(number) => number.as_i64(),
        _ => None,
    }
    .ok_or_else(|| ApiError::bad_request("invalid_quantity", range_message.clone()))?;
    if !(QUANTITY_MIN..=QUANTITY_MAX).contains(&number) {
        return Err(ApiError::bad_request("invalid_quantity", range_message));
    }
    Ok(number as i32)
}

/// `null`, empty or whitespace-only strings mean "no location" and are stored as NULL.
pub fn validate_location(value: &Value) -> ApiResult<Option<String>> {
    let raw = match value {
        Value::Null => return Ok(None),
        Value::String(raw) => raw,
        _ => {
            return Err(ApiError::bad_request(
                "invalid_location",
                "보관 위치는 문자열이어야 합니다.",
            ));
        }
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > LOCATION_MAX_CHARS {
        return Err(ApiError::bad_request(
            "invalid_location",
            format!("보관 위치는 {LOCATION_MAX_CHARS}자 이하여야 합니다."),
        ));
    }
    Ok(Some(trimmed.to_string()))
}

fn body_object(body: Result<Json<Value>, JsonRejection>) -> ApiResult<Map<String, Value>> {
    let Json(value) = body.map_err(|_| {
        ApiError::bad_request("invalid_json", "요청 본문은 올바른 JSON이어야 합니다.")
    })?;
    match value {
        Value::Object(map) => Ok(map),
        _ => Err(ApiError::bad_request(
            "invalid_body",
            "요청 본문은 JSON 객체여야 합니다.",
        )),
    }
}

fn reject_unknown_fields(map: &Map<String, Value>) -> ApiResult<()> {
    match map
        .keys()
        .find(|key| !matches!(key.as_str(), "name" | "quantity" | "location"))
    {
        Some(key) => Err(ApiError::bad_request(
            "unknown_field",
            format!("허용되지 않는 필드입니다: {key}"),
        )),
        None => Ok(()),
    }
}

pub fn parse_new_asset(map: &Map<String, Value>) -> ApiResult<NewAsset> {
    reject_unknown_fields(map)?;
    let name = map
        .get("name")
        .ok_or_else(|| ApiError::bad_request("invalid_name", "비품명을 입력하세요."))?;
    let quantity = map
        .get("quantity")
        .ok_or_else(|| ApiError::bad_request("invalid_quantity", "수량을 입력하세요."))?;
    Ok(NewAsset {
        name: validate_name(name)?,
        quantity: validate_quantity(quantity)?,
        location: match map.get("location") {
            Some(value) => validate_location(value)?,
            None => None,
        },
    })
}

pub fn parse_asset_patch(map: &Map<String, Value>) -> ApiResult<AssetPatch> {
    reject_unknown_fields(map)?;
    let patch = AssetPatch {
        name: map.get("name").map(validate_name).transpose()?,
        quantity: map.get("quantity").map(validate_quantity).transpose()?,
        location: map.get("location").map(validate_location).transpose()?,
    };
    if patch.name.is_none() && patch.quantity.is_none() && patch.location.is_none() {
        return Err(ApiError::bad_request(
            "empty_patch",
            "수정할 비품명, 수량 또는 보관 위치를 입력하세요.",
        ));
    }
    Ok(patch)
}

fn parse_id(raw: &str) -> ApiResult<Uuid> {
    Uuid::parse_str(raw)
        .map_err(|_| ApiError::bad_request("invalid_id", "비품 ID 형식이 올바르지 않습니다."))
}

async fn list_assets(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<Value>> {
    let assets = sqlx::query_as::<_, Asset>(&format!(
        "SELECT {ASSET_COLUMNS} FROM app.assets WHERE owner = $1 ORDER BY created_at, id"
    ))
    .bind(&identity.user_id)
    .fetch_all(&pool)
    .await
    .map_err(ApiError::database)?;
    Ok(Json(json!({ "assets": assets })))
}

async fn create_asset(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    body: Result<Json<Value>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<Asset>)> {
    let input = parse_new_asset(&body_object(body)?)?;
    let asset = sqlx::query_as::<_, Asset>(&format!(
        "INSERT INTO app.assets (id, owner, name, quantity, location) \
         VALUES ($1, $2, $3, $4, $5) RETURNING {ASSET_COLUMNS}"
    ))
    .bind(Uuid::new_v4())
    .bind(&identity.user_id)
    .bind(&input.name)
    .bind(input.quantity)
    .bind(&input.location)
    .fetch_one(&pool)
    .await
    .map_err(ApiError::database)?;
    Ok((StatusCode::CREATED, Json(asset)))
}

async fn update_asset(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(id): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> ApiResult<Json<Asset>> {
    let id = parse_id(&id)?;
    let input = parse_asset_patch(&body_object(body)?)?;
    let (location_given, location) = match input.location {
        Some(location) => (true, location),
        None => (false, None),
    };
    sqlx::query_as::<_, Asset>(&format!(
        "UPDATE app.assets SET name = COALESCE($3, name), quantity = COALESCE($4, quantity), \
         location = CASE WHEN $5 THEN $6 ELSE location END, updated_at = now() \
         WHERE id = $1 AND owner = $2 RETURNING {ASSET_COLUMNS}"
    ))
    .bind(id)
    .bind(&identity.user_id)
    .bind(input.name)
    .bind(input.quantity)
    .bind(location_given)
    .bind(location)
    .fetch_optional(&pool)
    .await
    .map_err(ApiError::database)?
    .map(Json)
    .ok_or_else(ApiError::not_found)
}

async fn delete_asset(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let id = parse_id(&id)?;
    let deleted: Option<Uuid> =
        sqlx::query_scalar("DELETE FROM app.assets WHERE id = $1 AND owner = $2 RETURNING id")
            .bind(id)
            .bind(&identity.user_id)
            .fetch_optional(&pool)
            .await
            .map_err(ApiError::database)?;
    match deleted {
        Some(id) => Ok(Json(json!({ "id": id, "deleted": true }))),
        None => Err(ApiError::not_found()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(value: Value) -> Map<String, Value> {
        value.as_object().cloned().expect("object")
    }

    fn identity() -> Identity {
        Identity {
            user_id: "user-a".into(),
            admin: false,
        }
    }

    /// A pool that never connects; validation must fail before any DB access.
    fn offline_pool() -> PgPool {
        PgPool::connect_lazy("postgres://nobody@127.0.0.1:1/none").expect("lazy pool")
    }

    async fn error_body(response: Response) -> (StatusCode, Value) {
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("body");
        (status, serde_json::from_slice(&bytes).expect("json"))
    }

    #[test]
    fn name_is_trimmed_and_bounded() {
        assert_eq!(validate_name(&json!("  노트북  ")).unwrap(), "노트북");
        assert_eq!(
            validate_name(&json!("가".repeat(100)))
                .unwrap()
                .chars()
                .count(),
            100
        );
        assert_eq!(
            validate_name(&json!(format!("  {}  ", "a".repeat(100))))
                .unwrap()
                .len(),
            100
        );
        for bad in [
            json!(""),
            json!("   \t\n"),
            json!("가".repeat(101)),
            json!(3),
            json!(null),
            json!(["a"]),
        ] {
            assert_eq!(
                validate_name(&bad).unwrap_err().code,
                "invalid_name",
                "{bad}"
            );
        }
    }

    #[test]
    fn quantity_must_be_integer_in_range() {
        assert_eq!(validate_quantity(&json!(0)).unwrap(), 0);
        assert_eq!(validate_quantity(&json!(100_000)).unwrap(), 100_000);
        let floats: Value = serde_json::from_str("[1.5, 1.0, 1e3]").unwrap();
        let mut bad = floats.as_array().unwrap().clone();
        bad.extend([
            json!(-1),
            json!(100_001),
            json!(u64::MAX),
            json!("3"),
            json!(null),
            json!(true),
        ]);
        for value in bad {
            assert_eq!(
                validate_quantity(&value).unwrap_err().code,
                "invalid_quantity",
                "{value}"
            );
        }
    }

    #[test]
    fn create_requires_both_fields_and_rejects_owner() {
        assert_eq!(
            parse_new_asset(&obj(json!({"name": " 모니터 ", "quantity": 3}))).unwrap(),
            NewAsset {
                name: "모니터".into(),
                quantity: 3,
                location: None
            }
        );
        assert_eq!(
            parse_new_asset(&obj(json!({"quantity": 3})))
                .unwrap_err()
                .code,
            "invalid_name"
        );
        assert_eq!(
            parse_new_asset(&obj(json!({"name": "a"})))
                .unwrap_err()
                .code,
            "invalid_quantity"
        );
        for field in ["owner", "id"] {
            let mut body = obj(json!({"name": "a", "quantity": 1}));
            body.insert(field.into(), json!("user-b"));
            assert_eq!(parse_new_asset(&body).unwrap_err().code, "unknown_field");
        }
    }

    #[test]
    fn patch_is_partial_but_not_empty() {
        assert_eq!(
            parse_asset_patch(&obj(json!({"name": "의자"}))).unwrap(),
            AssetPatch {
                name: Some("의자".into()),
                quantity: None,
                location: None
            }
        );
        assert_eq!(
            parse_asset_patch(&obj(json!({"quantity": 0}))).unwrap(),
            AssetPatch {
                name: None,
                quantity: Some(0),
                location: None
            }
        );
        assert_eq!(
            parse_asset_patch(&obj(json!({}))).unwrap_err().code,
            "empty_patch"
        );
        assert_eq!(
            parse_asset_patch(&obj(json!({"name": null})))
                .unwrap_err()
                .code,
            "invalid_name"
        );
        assert_eq!(
            parse_asset_patch(&obj(json!({"quantity": -2})))
                .unwrap_err()
                .code,
            "invalid_quantity"
        );
        assert_eq!(
            parse_asset_patch(&obj(json!({"owner": "user-b"})))
                .unwrap_err()
                .code,
            "unknown_field"
        );
    }

    #[test]
    fn asset_json_exposes_only_public_fields() {
        let id = Uuid::nil();
        let value = serde_json::to_value(Asset {
            id,
            name: "펜".into(),
            quantity: 7,
            location: Some("3층 창고".into()),
        })
        .unwrap();
        assert_eq!(
            value,
            json!({"id": id, "name": "펜", "quantity": 7, "location": "3층 창고"})
        );
        // Records created before migration 0003 have no location and serialize as null.
        let legacy = serde_json::to_value(Asset {
            id,
            name: "펜".into(),
            quantity: 7,
            location: None,
        })
        .unwrap();
        assert_eq!(legacy["location"], Value::Null);
        assert_eq!(legacy.as_object().unwrap().len(), 4);
    }

    #[test]
    fn location_is_optional_trimmed_and_bounded() {
        assert_eq!(validate_location(&json!(null)).unwrap(), None);
        assert_eq!(validate_location(&json!("")).unwrap(), None);
        assert_eq!(validate_location(&json!("  \t ")).unwrap(), None);
        assert_eq!(
            validate_location(&json!("  2층 비품실 ")).unwrap(),
            Some("2층 비품실".into())
        );
        assert_eq!(
            validate_location(&json!("가".repeat(100)))
                .unwrap()
                .unwrap()
                .chars()
                .count(),
            100
        );
        for bad in [
            json!("가".repeat(101)),
            json!(3),
            json!(true),
            json!(["a"]),
            json!({}),
        ] {
            assert_eq!(
                validate_location(&bad).unwrap_err().code,
                "invalid_location",
                "{bad}"
            );
        }
    }

    #[test]
    fn create_accepts_optional_location() {
        let without = parse_new_asset(&obj(json!({"name": "책상", "quantity": 1}))).unwrap();
        assert_eq!(without.location, None);
        let with = parse_new_asset(&obj(
            json!({"name": "책상", "quantity": 1, "location": " 본관 101호 "}),
        ))
        .unwrap();
        assert_eq!(with.location, Some("본관 101호".into()));
        assert_eq!(
            parse_new_asset(&obj(
                json!({"name": "책상", "quantity": 1, "location": "x".repeat(101)})
            ))
            .unwrap_err()
            .code,
            "invalid_location"
        );
    }

    #[test]
    fn patch_location_distinguishes_absent_set_and_clear() {
        assert_eq!(
            parse_asset_patch(&obj(json!({"location": "창고"}))).unwrap(),
            AssetPatch {
                name: None,
                quantity: None,
                location: Some(Some("창고".into()))
            }
        );
        for clear in [json!(null), json!(""), json!("   ")] {
            assert_eq!(
                parse_asset_patch(&obj(json!({ "location": clear })))
                    .unwrap()
                    .location,
                Some(None)
            );
        }
        // Absent location leaves the stored value untouched.
        assert_eq!(
            parse_asset_patch(&obj(json!({"quantity": 2})))
                .unwrap()
                .location,
            None
        );
        assert_eq!(
            parse_asset_patch(&obj(json!({"location": 5})))
                .unwrap_err()
                .code,
            "invalid_location"
        );
    }

    #[tokio::test]
    async fn error_response_has_code_and_message() {
        let (status, body) = error_body(ApiError::not_found().into_response()).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "asset_not_found");
        assert!(body["error"]["message"].is_string());
    }

    #[tokio::test]
    async fn handlers_reject_invalid_input_before_database() {
        let create = create_asset(
            State(offline_pool()),
            Extension(identity()),
            Ok(Json(json!({"name": "a", "quantity": 1, "owner": "user-b"}))),
        )
        .await
        .unwrap_err();
        let (status, body) = error_body(create.into_response()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "unknown_field");

        let not_object = create_asset(
            State(offline_pool()),
            Extension(identity()),
            Ok(Json(json!([1]))),
        )
        .await
        .unwrap_err();
        assert_eq!(not_object.code, "invalid_body");

        let bad_id = update_asset(
            State(offline_pool()),
            Extension(identity()),
            Path("not-a-uuid".into()),
            Ok(Json(json!({"quantity": 1}))),
        )
        .await
        .unwrap_err();
        assert_eq!(bad_id.code, "invalid_id");

        let bad_location = update_asset(
            State(offline_pool()),
            Extension(identity()),
            Path(Uuid::nil().to_string()),
            Ok(Json(json!({"location": ["not", "a", "string"]}))),
        )
        .await
        .unwrap_err();
        assert_eq!(bad_location.code, "invalid_location");

        let bad_delete = delete_asset(
            State(offline_pool()),
            Extension(identity()),
            Path("x".into()),
        )
        .await
        .unwrap_err();
        assert_eq!(bad_delete.status, StatusCode::BAD_REQUEST);
    }
}
