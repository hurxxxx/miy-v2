//! Anonymous submission only; reading and triage require a current administrator.
use crate::{
    State,
    api_error::{ApiError, ApiResult},
    auth::{BrowserPurpose, Principal},
    policy::{PlatformAction, Write},
    support_contract::{MAX_BODY, Report},
};
use axum::{
    Extension, Json, Router,
    extract::{DefaultBodyLimit, Path, Query, State as S},
    http::{HeaderMap, StatusCode},
    routing::post,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

pub fn public_router() -> Router<State> {
    Router::new()
        .route("/api/v1/support/reports", post(submit))
        .layer(DefaultBodyLimit::max(MAX_BODY))
}

async fn submit(
    S(s): S<State>,
    Extension(purpose): Extension<BrowserPurpose>,
    headers: HeaderMap,
    Json(mut report): Json<Report>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    // No session/credential access, even when a client supplies a broken token.
    // Browser submission is not needed; reject browser origins on this CLI path.
    if purpose != BrowserPurpose::Portal
        || headers.contains_key("origin")
        || headers.contains_key("sec-fetch-site")
    {
        return Err(ApiError::forbidden());
    }
    report.validate().map_err(ApiError::bad)?;
    report.redact();
    let payload = serde_json::to_value(&report)?;
    let mut tx = s.pool.begin().await?;
    // Bounded database-wide intake, shared by all server processes. Never trust
    // X-Forwarded-For, and never store IP addresses or user-supplied identities.
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(73310424)")
        .fetch_one(&mut *tx)
        .await?;
    if !locked {
        return Err(limited());
    }
    if let Some(existing) =
        sqlx::query_scalar::<_, Value>("SELECT payload FROM support_reports WHERE id=$1")
            .bind(report.id)
            .fetch_optional(&mut *tx)
            .await?
    {
        if existing != payload {
            return Err(ApiError::conflict("report_id_conflict"));
        }
        return Ok((
            StatusCode::OK,
            Json(json!({"id":report.id,"status":"received"})),
        ));
    }
    let counts = sqlx::query("SELECT count(*) AS total, count(*) FILTER (WHERE created_at > now()-interval '1 hour') AS recent FROM support_reports")
        .fetch_one(&mut *tx).await?;
    if counts.get::<i64, _>("total") >= 20000 || counts.get::<i64, _>("recent") >= 120 {
        return Err(limited());
    }
    sqlx::query("INSERT INTO support_reports(id,payload) VALUES($1,$2)")
        .bind(report.id)
        .bind(payload)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"id":report.id,"status":"received"})),
    ))
}

fn limited() -> ApiError {
    ApiError {
        status: StatusCode::TOO_MANY_REQUESTS,
        code: "report_intake_limited".into(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Filter {
    pub status: Option<String>,
    pub before: Option<i64>,
    pub thread_id: Option<Uuid>,
}
fn valid_status(s: &str) -> bool {
    matches!(s, "open" | "investigating" | "resolved" | "closed")
}

pub async fn list(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Query(q): Query<Filter>,
) -> ApiResult<Json<Value>> {
    crate::domain::admin(&s, &p).await?;
    if q.status.as_deref().is_some_and(|v| !valid_status(v)) || q.before.is_some_and(|v| v <= 0) {
        return Err(ApiError::bad("invalid_report_filter"));
    }
    let rows = sqlx::query("SELECT sequence,id,payload,status,note,revision,created_at,updated_at,updated_by FROM support_reports WHERE ($1::text IS NULL OR status=$1) AND ($2::bigint IS NULL OR sequence<$2) AND ($3::uuid IS NULL OR id=$3 OR payload->>'threadId'=$3::text) ORDER BY sequence DESC LIMIT 51")
        .bind(q.status).bind(q.before).bind(q.thread_id).fetch_all(&s.pool).await?;
    let next = if rows.len() > 50 {
        Some(rows[49].get::<i64, _>("sequence"))
    } else {
        None
    };
    Ok(Json(
        json!({"entries":rows.iter().take(50).map(view).collect::<Vec<_>>(),"nextCursor":next}),
    ))
}

fn view(r: &sqlx::postgres::PgRow) -> Value {
    json!({"id":r.get::<Uuid,_>("id"), "report":r.get::<Value,_>("payload"),
        "status":r.get::<String,_>("status"), "note":r.get::<String,_>("note"),
        "revision":r.get::<i64,_>("revision"), "createdAt":r.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),
        "updatedAt":r.get::<chrono::DateTime<chrono::Utc>,_>("updated_at"), "updatedBy":r.get::<Option<String>,_>("updated_by")})
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Update {
    pub status: String,
    pub note: String,
    pub revision: i64,
}

pub async fn update(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
    Json(input): Json<Update>,
) -> ApiResult<Json<Value>> {
    crate::domain::admin(&s, &p).await?;
    if !valid_status(&input.status)
        || input.note.chars().count() > 4000
        || input.revision < 1
        || input
            .note
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(ApiError::bad("invalid_report_update"));
    }
    let mut tx = Write::begin(&s.pool)
        .await?
        .decide_platform(&p, PlatformAction::Administer)
        .await?;
    let row = sqlx::query("UPDATE support_reports SET status=$2,note=$3,revision=revision+1,updated_at=now(),updated_by=$4 WHERE id=$1 AND revision=$5 RETURNING sequence,id,payload,status,note,revision,created_at,updated_at,updated_by")
        .bind(id).bind(&input.status).bind(crate::support_contract::redact(&input.note)).bind(&p.id).bind(input.revision)
        .fetch_optional(&mut *tx).await?.ok_or_else(|| ApiError::conflict("report_changed_or_missing"))?;
    crate::audit::record(
        &mut tx,
        &p.id,
        "support.report.updated",
        &format!("report:{id}:{}", input.status),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(view(&row)))
}
