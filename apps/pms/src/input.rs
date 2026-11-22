//! Request input with FastAPI/Pydantic semantics: query parameters (repeated
//! keys for lists) and JSON bodies where a field left out differs from a field
//! sent as `null` (`model_fields_set`). Invalid input answers 422
//! `validation_request_invalid` like FastAPI.
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use chrono::NaiveDate;
use serde_json::{Map, Value};

use crate::error::{ApiError, ApiResult, check_choice, check_length, check_range};

/// Query parameters in order.
#[derive(Debug, Default, Clone)]
pub struct Params(pub Vec<(String, String)>);

impl<S: Send + Sync> FromRequestParts<S> for Params {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let query = parts.uri.query().unwrap_or("");
        Ok(Self(
            form_urlencoded::parse(query.as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect(),
        ))
    }
}

fn parse_bool(field: &str, value: &str) -> ApiResult<bool> {
    match value.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" | "t" | "y" => Ok(true),
        "false" | "0" | "no" | "off" | "f" | "n" => Ok(false),
        _ => Err(ApiError::validation(format!(
            "{field}: Input should be a valid boolean"
        ))),
    }
}

pub fn parse_date(field: &str, value: &str) -> ApiResult<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| ApiError::validation(format!("{field}: Input should be a valid date")))
}

impl Params {
    pub fn get(&self, name: &str) -> Option<&str> {
        // FastAPI reads the last value of a repeated scalar parameter.
        self.0
            .iter()
            .rev()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn all(&self, name: &str) -> Vec<String> {
        self.0
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .collect()
    }

    pub fn string(&self, name: &str, default: &str) -> String {
        self.get(name).unwrap_or(default).to_string()
    }

    pub fn opt_string(&self, name: &str) -> Option<String> {
        self.get(name).map(str::to_string)
    }

    /// `Query(default=..., ge=..., le=...)`.
    pub fn int(&self, name: &str, default: i64, min: i64, max: Option<i64>) -> ApiResult<i64> {
        let Some(value) = self.get(name) else {
            return Ok(default);
        };
        let value: i64 = value.trim().parse().map_err(|_| {
            ApiError::validation(format!(
                "{name}: Input should be a valid integer, unable to parse string as an integer"
            ))
        })?;
        check_range(name, value, min, max)?;
        Ok(value)
    }

    pub fn opt_int(&self, name: &str, min: i64, max: Option<i64>) -> ApiResult<Option<i64>> {
        if self.get(name).is_none() {
            return Ok(None);
        }
        self.int(name, 0, min, max).map(Some)
    }

    pub fn opt_bool(&self, name: &str) -> ApiResult<Option<bool>> {
        self.get(name)
            .map(|value| parse_bool(name, value))
            .transpose()
    }

    pub fn opt_date(&self, name: &str) -> ApiResult<Option<NaiveDate>> {
        self.get(name)
            .map(|value| parse_date(name, value))
            .transpose()
    }

    pub fn choice(&self, name: &str, default: &str, choices: &[&str]) -> ApiResult<String> {
        let value = self.string(name, default);
        check_choice(name, &value, choices)?;
        Ok(value)
    }
}

/// A JSON object body.
#[derive(Debug, Default, Clone)]
pub struct Body(pub Map<String, Value>);

impl<S: Send + Sync> FromRequest<S> for Body {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let crate::error::ApiJson(value) =
            crate::error::ApiJson::<Value>::from_request(request, state).await?;
        match value {
            Value::Object(map) => Ok(Self(map)),
            _ => Err(ApiError::validation(
                "body: Input should be a valid dictionary or object",
            )),
        }
    }
}

impl Body {
    pub fn has(&self, name: &str) -> bool {
        self.0.contains_key(name)
    }

    /// `ConfigDict(extra="forbid")`.
    pub fn forbid_extra(&self, allowed: &[&str]) -> ApiResult<()> {
        if let Some(key) = self.0.keys().find(|key| !allowed.contains(&key.as_str())) {
            return Err(ApiError::validation(format!(
                "{key}: Extra inputs are not permitted"
            )));
        }
        Ok(())
    }

    fn present(&self, name: &str) -> Option<&Value> {
        self.0.get(name).filter(|value| !value.is_null())
    }

    pub fn opt_str(&self, name: &str) -> ApiResult<Option<String>> {
        match self.present(name) {
            None => Ok(None),
            Some(Value::String(value)) => Ok(Some(value.clone())),
            Some(_) => Err(ApiError::validation(format!(
                "{name}: Input should be a valid string"
            ))),
        }
    }

    /// A required string field.
    pub fn req_str(&self, name: &str) -> ApiResult<String> {
        if !self.has(name) {
            return Err(ApiError::validation(format!("{name}: Field required")));
        }
        self.opt_str(name)?
            .ok_or_else(|| ApiError::validation(format!("{name}: Input should be a valid string")))
    }

    /// A string field with a default (null is invalid unless `nullable`).
    pub fn str_or(&self, name: &str, default: &str) -> ApiResult<String> {
        if self.0.get(name).is_some_and(Value::is_null) {
            return Err(ApiError::validation(format!(
                "{name}: Input should be a valid string"
            )));
        }
        Ok(self.opt_str(name)?.unwrap_or_else(|| default.to_string()))
    }

    pub fn opt_bool(&self, name: &str) -> ApiResult<Option<bool>> {
        match self.present(name) {
            None => Ok(None),
            Some(Value::Bool(value)) => Ok(Some(*value)),
            Some(Value::String(value)) => parse_bool(name, value).map(Some),
            Some(Value::Number(number)) if number.as_i64() == Some(0) => Ok(Some(false)),
            Some(Value::Number(number)) if number.as_i64() == Some(1) => Ok(Some(true)),
            Some(_) => Err(ApiError::validation(format!(
                "{name}: Input should be a valid boolean"
            ))),
        }
    }

    pub fn opt_int(&self, name: &str) -> ApiResult<Option<i64>> {
        match self.present(name) {
            None => Ok(None),
            Some(Value::Number(number)) => number
                .as_i64()
                .or_else(|| {
                    number
                        .as_f64()
                        .filter(|value| value.fract() == 0.0)
                        .map(|value| value as i64)
                })
                .map(Some)
                .ok_or_else(|| {
                    ApiError::validation(format!("{name}: Input should be a valid integer"))
                }),
            Some(Value::String(value)) => value.trim().parse().map(Some).map_err(|_| {
                ApiError::validation(format!("{name}: Input should be a valid integer"))
            }),
            Some(_) => Err(ApiError::validation(format!(
                "{name}: Input should be a valid integer"
            ))),
        }
    }

    pub fn opt_date(&self, name: &str) -> ApiResult<Option<NaiveDate>> {
        match self.present(name) {
            None => Ok(None),
            Some(Value::String(value)) => parse_date(name, value).map(Some),
            Some(_) => Err(ApiError::validation(format!(
                "{name}: Input should be a valid date"
            ))),
        }
    }

    pub fn opt_str_list(&self, name: &str) -> ApiResult<Option<Vec<String>>> {
        match self.present(name) {
            None => Ok(None),
            Some(Value::Array(items)) => items
                .iter()
                .map(|item| {
                    item.as_str().map(str::to_string).ok_or_else(|| {
                        ApiError::validation(format!("{name}: Input should be a valid string"))
                    })
                })
                .collect::<ApiResult<Vec<_>>>()
                .map(Some),
            Some(_) => Err(ApiError::validation(format!(
                "{name}: Input should be a valid list"
            ))),
        }
    }

    /// `list[dict] | None`.
    pub fn opt_dict_list(&self, name: &str) -> ApiResult<Option<Value>> {
        match self.present(name) {
            None => Ok(None),
            Some(Value::Array(items)) => {
                if items.iter().any(|item| !item.is_object()) {
                    return Err(ApiError::validation(format!(
                        "{name}: Input should be a valid dictionary"
                    )));
                }
                Ok(Some(Value::Array(items.clone())))
            }
            Some(_) => Err(ApiError::validation(format!(
                "{name}: Input should be a valid list"
            ))),
        }
    }

    pub fn opt_choice(&self, name: &str, choices: &[&str]) -> ApiResult<Option<String>> {
        let value = self.opt_str(name)?;
        if let Some(value) = &value {
            check_choice(name, value, choices)?;
        }
        Ok(value)
    }
}

/// `Field(min_length, max_length)` on an optional string.
pub fn check_opt_length(
    field: &str,
    value: &Option<String>,
    min: usize,
    max: usize,
) -> ApiResult<()> {
    if let Some(value) = value {
        check_length(field, value, min, max)?;
    }
    Ok(())
}

/// `list[...] = Field(min_length, max_length)`.
pub fn check_list_length(field: &str, len: usize, min: usize, max: usize) -> ApiResult<()> {
    if len < min {
        return Err(ApiError::validation(format!(
            "{field}: List should have at least {min} item{} after validation",
            if min == 1 { "" } else { "s" }
        )));
    }
    if len > max {
        return Err(ApiError::validation(format!(
            "{field}: List should have at most {max} items after validation"
        )));
    }
    Ok(())
}
