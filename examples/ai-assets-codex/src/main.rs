use axum::{
    Extension, Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch},
};
use miy_app_sdk::Identity;
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug)]
struct ApiError(StatusCode, &'static str, &'static str);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.0,
            Json(json!({"error":{"code":self.1,"message":self.2}})),
        )
            .into_response()
    }
}
fn invalid() -> ApiError {
    ApiError(
        StatusCode::BAD_REQUEST,
        "invalid_input",
        "비품명은 공백 제외 1~100자, 수량은 0~100000의 정수여야 합니다. 보관 위치는 최대 100자입니다. owner는 지정할 수 없습니다.",
    )
}
fn db_error(_: sqlx::Error) -> ApiError {
    ApiError(
        StatusCode::INTERNAL_SERVER_ERROR,
        "database_error",
        "처리하지 못했습니다. 잠시 후 다시 시도해 주세요.",
    )
}
fn missing() -> ApiError {
    ApiError(
        StatusCode::NOT_FOUND,
        "asset_not_found",
        "비품을 찾을 수 없습니다.",
    )
}
#[derive(Debug, Serialize, sqlx::FromRow)]
struct Asset {
    id: Uuid,
    name: String,
    quantity: i32,
    location: Option<String>,
}
#[derive(Debug, PartialEq)]
struct AssetInput {
    name: Option<String>,
    quantity: Option<i32>,
    // None: omitted; Some(None): clear; Some(Some(text)): replace.
    location: Option<Option<String>>,
}
// Explicit key checks distinguish omitted PATCH fields from null and reject owner spoofing.
fn validate(value: Value, partial: bool) -> Result<AssetInput, ApiError> {
    let object = value.as_object().ok_or_else(invalid)?;
    if object
        .keys()
        .any(|key| key != "name" && key != "quantity" && key != "location")
    {
        return Err(invalid());
    }
    let name = match object.get("name") {
        Some(value) => {
            let text = value.as_str().ok_or_else(invalid)?.trim();
            let count = text.chars().filter(|ch| !ch.is_whitespace()).count();
            if !(1..=100).contains(&count) {
                return Err(invalid());
            }
            Some(text.to_owned())
        }
        None if partial => None,
        None => return Err(invalid()),
    };
    let quantity = match object.get("quantity") {
        Some(value) => {
            let number = value.as_i64().ok_or_else(invalid)?;
            if !(0..=100000).contains(&number) {
                return Err(invalid());
            }
            Some(number as i32)
        }
        None if partial => None,
        None => return Err(invalid()),
    };
    let location = match object.get("location") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(text)) if text.chars().count() <= 100 => Some(Some(text.clone())),
        Some(_) => return Err(invalid()),
    };
    if name.is_none() && quantity.is_none() && location.is_none() {
        return Err(invalid());
    }
    Ok(AssetInput {
        name,
        quantity,
        location,
    })
}
fn payload(
    body: Result<Json<Value>, JsonRejection>,
    partial: bool,
) -> Result<AssetInput, ApiError> {
    validate(body.map_err(|_| invalid())?.0, partial)
}
fn asset_id(id: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(id).map_err(|_| {
        ApiError(
            StatusCode::BAD_REQUEST,
            "invalid_id",
            "잘못된 비품 ID입니다.",
        )
    })
}
async fn list_assets(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
) -> Result<Json<Value>, ApiError> {
    let assets = sqlx::query_as::<_, Asset>(
        "SELECT id, name, quantity, location FROM app.assets WHERE owner = $1 ORDER BY name, id",
    )
    .bind(&identity.user_id)
    .fetch_all(&pool)
    .await
    .map_err(db_error)?;
    Ok(Json(json!({"assets":assets})))
}
async fn create_asset(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<Asset>), ApiError> {
    let input = payload(body, false)?;
    let asset = sqlx::query_as::<_, Asset>("INSERT INTO app.assets (id, name, quantity, owner, location) VALUES ($1, $2, $3, $4, $5) RETURNING id, name, quantity, location")
        .bind(Uuid::new_v4()).bind(input.name).bind(input.quantity).bind(&identity.user_id).bind(input.location.flatten())
        .fetch_one(&pool).await.map_err(db_error)?;
    Ok((StatusCode::CREATED, Json(asset)))
}
async fn update_asset(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(id): Path<String>,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Asset>, ApiError> {
    let id = asset_id(&id)?;
    let input = payload(body, true)?;
    let asset = sqlx::query_as::<_, Asset>("UPDATE app.assets SET name = COALESCE($1, name), quantity = COALESCE($2, quantity), location = CASE WHEN $3 THEN $4::text ELSE location END WHERE id = $5 AND owner = $6 RETURNING id, name, quantity, location")
        .bind(input.name).bind(input.quantity).bind(input.location.is_some()).bind(input.location.flatten()).bind(id).bind(&identity.user_id)
        .fetch_optional(&pool).await.map_err(db_error)?.ok_or_else(missing)?;
    Ok(Json(asset))
}
async fn delete_asset(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let result = sqlx::query("DELETE FROM app.assets WHERE id = $1 AND owner = $2")
        .bind(asset_id(&id)?)
        .bind(&identity.user_id)
        .execute(&pool)
        .await
        .map_err(db_error)?;
    if result.rows_affected() == 0 {
        return Err(missing());
    }
    Ok(Json(json!({"deleted":true})))
}
fn router(pool: PgPool) -> Router {
    Router::new()
        .route("/assets", get(list_assets).post(create_asset))
        .route("/assets/{id}", patch(update_asset).delete(delete_asset))
        .with_state(pool)
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    miy_app_sdk::serve(router(miy_app_sdk::database().await?)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn creation_normalizes_and_accepts_boundaries() {
        for quantity in [0, 100000] {
            let input =
                validate(json!({"name":"  사무용 의자  ","quantity":quantity}), false).unwrap();
            assert_eq!(input.name.as_deref(), Some("사무용 의자"));
            assert_eq!(input.quantity, Some(quantity));
        }
        assert!(validate(json!({"name":"가".repeat(100),"quantity":1}), false).is_ok());
        assert!(validate(json!({"name":"가".repeat(101),"quantity":1}), false).is_err());
    }
    #[test]
    fn rejects_invalid_names_quantities_types_and_owner() {
        for name in [
            json!(""),
            json!(" \t\n\u{2003}"),
            json!(null),
            json!(12),
            json!([]),
        ] {
            assert!(validate(json!({"name":name,"quantity":1}), false).is_err());
        }
        for quantity in [
            json!(-1),
            json!(100001),
            json!(1.5),
            json!(1.0),
            json!("1"),
            json!(null),
            json!(true),
        ] {
            assert!(validate(json!({"name":"책상","quantity":quantity}), false).is_err());
        }
        for partial in [false, true] {
            assert!(
                validate(
                    json!({"name":"책상","quantity":1,"owner":"victim"}),
                    partial
                )
                .is_err()
            );
        }
        for value in [
            json!([]),
            json!(null),
            json!({"name":"책상"}),
            json!({"quantity":1}),
        ] {
            assert!(validate(value, false).is_err());
        }
    }
    #[test]
    fn patch_preserves_omitted_fields_and_rejects_null_or_empty() {
        assert_eq!(
            validate(json!({"quantity":0}), true).unwrap(),
            AssetInput {
                name: None,
                quantity: Some(0),
                location: None,
            }
        );
        assert_eq!(
            validate(json!({"name":"모니터"}), true).unwrap(),
            AssetInput {
                name: Some("모니터".into()),
                quantity: None,
                location: None,
            }
        );
        for value in [json!({}), json!({"name":null}), json!({"quantity":null})] {
            assert!(validate(value, true).is_err());
        }
    }
    #[test]
    fn public_asset_has_no_owner() {
        let value = serde_json::to_value(Asset {
            id: Uuid::new_v4(),
            name: "책상".into(),
            quantity: 1,
            location: None,
        })
        .unwrap();
        assert_eq!(value.as_object().unwrap().len(), 4);
        assert!(value.get("owner").is_none());
        assert_eq!(value["location"], Value::Null);
    }
    #[test]
    fn location_is_optional_and_accepts_unicode_boundary() {
        let legacy = validate(json!({"name":"책상","quantity":1}), false).unwrap();
        assert_eq!(legacy.location, None);
        for location in [
            json!(null),
            json!(""),
            json!("회의실 A"),
            json!("가".repeat(100)),
            json!("📦".repeat(100)),
        ] {
            assert!(
                validate(
                    json!({"name":"책상","quantity":1,"location":location}),
                    false
                )
                .is_ok()
            );
        }
    }
    #[test]
    fn location_rejects_overlength_and_non_string_values() {
        for location in [
            json!("가".repeat(101)),
            json!(" ".repeat(101)),
            json!(1),
            json!(true),
            json!([]),
            json!({}),
        ] {
            for partial in [false, true] {
                assert!(
                    validate(
                        json!({"name":"책상","quantity":1,"location":location}),
                        partial
                    )
                    .is_err()
                );
            }
        }
    }
    #[test]
    fn location_only_patch_distinguishes_omission_clear_and_replacement() {
        let replace = validate(json!({"location":"창고"}), true).unwrap();
        assert_eq!(
            replace,
            AssetInput {
                name: None,
                quantity: None,
                location: Some(Some("창고".into()))
            }
        );
        let clear = validate(json!({"location":null}), true).unwrap();
        assert_eq!(clear.location, Some(None));
        let empty = validate(json!({"location":""}), true).unwrap();
        assert_eq!(empty.location, Some(Some(String::new())));
        let unchanged = validate(json!({"quantity":2}), true).unwrap();
        assert_eq!(unchanged.location, None);
        assert!(validate(json!({"location":"창고"}), false).is_err());
    }
}
