//! Reconnectable event observations and explicit, typed retries.
use crate::{
    State,
    auth::Principal,
    domain,
    http::{ApiError, ApiResult},
    runtime,
    runtime_contract::{ExpectedState, FailureCode, JobStatus, RuntimeFailure},
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State as S},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Events {
    after: Option<String>,
    limit: Option<i64>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    v: u8,
    job: Uuid,
    user: String,
    seq: i64,
}
fn cursor(job: Uuid, user: &str, seq: i64) -> String {
    URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&Cursor {
            v: 1,
            job,
            user: user.into(),
            seq,
        })
        .unwrap(),
    )
}
fn after(raw: Option<&str>, id: Uuid, user: &str) -> ApiResult<i64> {
    let Some(raw) = raw else { return Ok(0) };
    if raw.len() > 2048 {
        return Err(ApiError::bad("invalid_event_cursor"));
    }
    let c: Cursor = URL_SAFE_NO_PAD
        .decode(raw)
        .ok()
        .and_then(|v| serde_json::from_slice(&v).ok())
        .ok_or_else(|| ApiError::bad("invalid_event_cursor"))?;
    if c.v != 1 || c.job != id || c.user != user || c.seq < 0 {
        return Err(ApiError::bad("invalid_event_cursor"));
    }
    Ok(c.seq)
}
pub async fn events(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
    Query(q): Query<Events>,
) -> ApiResult<Json<Value>> {
    runtime::job(&s, &p, id).await?;
    let limit = q.limit.unwrap_or(100);
    if !(1..=200).contains(&limit) {
        return Err(ApiError::bad("invalid_page_limit"));
    }
    let sequence = after(q.after.as_deref(), id, &p.id)?;
    let mut tx = s.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let status: String = sqlx::query_scalar("SELECT status FROM runtime_jobs WHERE id=$1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    let maximum: i64 = sqlx::query_scalar(
        "SELECT COALESCE(max(sequence),0) FROM runtime_job_events WHERE job_id=$1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if sequence > maximum {
        return Err(ApiError::bad("invalid_event_cursor"));
    }
    let rows=sqlx::query("SELECT sequence,created_at,payload FROM runtime_job_events WHERE job_id=$1 AND sequence>$2 ORDER BY sequence LIMIT $3").bind(id).bind(sequence).bind(limit).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let last = rows
        .last()
        .map(|r| r.get::<i64, _>("sequence"))
        .unwrap_or(sequence);
    let events:Vec<Value>=rows.into_iter().map(|r|json!({"cursor":cursor(id,&p.id,r.get("sequence")),"sequence":r.get::<i64,_>("sequence"),"createdAt":r.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"job":r.get::<Value,_>("payload")})).collect();
    Ok(Json(
        json!({"jobId":id,"events":events,"nextCursor":cursor(id,&p.id,last),"terminal":(["succeeded","failed"].contains(&status.as_str())&&last==maximum),"status":status}),
    ))
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Retry {
    pub app_id: String,
    pub environment: String,
    pub idempotency_key: String,
    pub expected_state: ExpectedState,
    pub confirmed: bool,
}
pub async fn retry(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
    Json(input): Json<Retry>,
) -> ApiResult<Json<Value>> {
    let row = sqlx::query("SELECT * FROM runtime_jobs WHERE id=$1")
        .bind(id)
        .fetch_optional(&s.pool)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let app: String = row.get("app_id");
    let env: String = row.get("environment");
    domain::manage(&s, &p, &app, &env).await?;
    if input.app_id != app || input.environment != env {
        return Err(ApiError::bad("retry_target_mismatch"));
    }
    let status: JobStatus = row
        .get::<String, _>("status")
        .parse()
        .map_err(|_| ApiError::conflict("job_not_retryable"))?;
    let failure = RuntimeFailure::new(FailureCode::from_code(
        row.get::<Option<String>, _>("error_code")
            .as_deref()
            .unwrap_or(""),
    ));
    if !crate::runtime_contract::can_retry(status, &failure) || !row.get::<bool, _>("retryable") {
        return Err(ApiError::conflict("job_not_retryable"));
    }
    let action: String = row.get("action");
    let original: Value = row
        .get::<Option<Value>, _>("request_data")
        .ok_or_else(|| ApiError::conflict("legacy_job_not_retryable"))?;
    let request = runtime::RuntimeRequest {
        action: action.clone(),
        environment: env,
        image_id: if action == "deploy" {
            row.get("image_id")
        } else {
            None
        },
        release_id: serde_json::from_value(original["releaseId"].clone())
            .map_err(|_| ApiError::conflict("invalid_original_request"))?,
        idempotency_key: input.idempotency_key,
        expected_state: Some(input.expected_state),
        confirmed: input.confirmed,
        retry_of: Some(id),
    };
    Ok(Json(runtime::request(&s, &p, &app, request).await?))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn event_cursor_scope() {
        let id = Uuid::new_v4();
        let c = cursor(id, "a", 7);
        assert_eq!(after(Some(&c), id, "a").unwrap(), 7);
        assert!(after(Some(&c), id, "b").is_err());
        assert!(after(Some(&c), Uuid::new_v4(), "a").is_err());
    }
}
