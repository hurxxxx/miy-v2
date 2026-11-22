//! Port of AI-DO `domains/planner` (router, service, schemas, models,
//! event_application, event_projection, event_access) at 0002af062.
//!
//! The owner is the verified miy caller (`Identity::user_id`). AI-DO read the
//! event time zone from the user's profile; miy keeps display settings in the
//! portal, so the client sends its zone as `?timeZone=` on create (the one
//! request that stores it). Google Calendar feeds, AI tools, approval previews,
//! RAG partitions and source access are not part of this release.
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;

use crate::error::{ApiError, ApiJson, ApiResult, check_length};
use crate::event_time::{
    normalize_time_zone, parse_event_bounds, parse_iso_or_date, serialize_event_bounds,
};
use crate::time::{now_utc, serialize_naive};

pub const MAX_LIST_RANGE_DAYS: i64 = 366;

/// `planner_events` row (models.PlannerEvent).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PlannerEvent {
    pub id: String,
    pub owner_id: String,
    pub title: String,
    pub description: String,
    pub location: String,
    pub time_zone: String,
    pub all_day: bool,
    pub start_has_time: bool,
    pub end_has_time: bool,
    pub start_at: NaiveDateTime,
    pub end_at: NaiveDateTime,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

const EVENT_COLUMNS: &str = "id, owner_id, title, description, location, time_zone, all_day, \
     start_has_time, end_has_time, start_at, end_at, created_at, updated_at";

/// schemas.PlannerEventCreateRequest (camelCase, snake_case accepted).
#[derive(Debug, Clone, Deserialize)]
pub struct PlannerEventCreateRequest {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub location: String,
    #[serde(default, rename = "allDay", alias = "all_day")]
    pub all_day: bool,
    pub start: String,
    pub end: String,
}

impl PlannerEventCreateRequest {
    fn validate(&self) -> ApiResult<()> {
        check_length("title", &self.title, 1, 200)?;
        check_length("description", &self.description, 0, 4000)?;
        check_length("location", &self.location, 0, 240)
    }
}

/// schemas.PlannerEventUpdateRequest. `null` means "not given", as in Pydantic.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PlannerEventUpdateRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default, rename = "allDay", alias = "all_day")]
    pub all_day: Option<bool>,
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub end: Option<String>,
}

impl PlannerEventUpdateRequest {
    fn validate(&self) -> ApiResult<()> {
        if let Some(title) = &self.title {
            check_length("title", title, 1, 200)?;
        }
        if let Some(description) = &self.description {
            check_length("description", description, 0, 4000)?;
        }
        if let Some(location) = &self.location {
            check_length("location", location, 0, 240)?;
        }
        Ok(())
    }
}

/// schemas.PlannerEventOut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannerEventOut {
    pub id: String,
    pub owner_id: String,
    pub owner_name: String,
    pub title: String,
    pub description: String,
    pub location: String,
    pub time_zone: String,
    pub all_day: bool,
    pub start_has_time: bool,
    pub end_has_time: bool,
    pub start: String,
    pub end: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlannerEventsResponse {
    pub items: Vec<PlannerEventOut>,
}

/// event_projection.project_planner_event. AI-DO shows the owner's full name;
/// the app has no access to names (no directory permission), so it uses
/// AI-DO's own fallback, the owner id.
pub fn project_planner_event(event: &PlannerEvent) -> PlannerEventOut {
    let (start, end) = serialize_event_bounds(
        event.all_day,
        event.start_at,
        event.end_at,
        event.start_has_time,
        event.end_has_time,
        Some(&event.time_zone),
    );
    PlannerEventOut {
        id: event.id.clone(),
        owner_id: event.owner_id.clone(),
        owner_name: event.owner_id.clone(),
        title: event.title.clone(),
        description: event.description.clone(),
        location: event.location.clone(),
        time_zone: event.time_zone.clone(),
        all_day: event.all_day,
        start_has_time: event.start_has_time,
        end_has_time: event.end_has_time,
        start,
        end,
        created_at: serialize_naive(event.created_at),
        updated_at: serialize_naive(event.updated_at),
    }
}

/// event_application.new_planner_event (Python `str.strip()` on text fields).
pub fn new_planner_event(
    owner_id: &str,
    command: &PlannerEventCreateRequest,
    time_zone: &str,
    id: String,
    now: NaiveDateTime,
) -> ApiResult<PlannerEvent> {
    let bounds = parse_event_bounds(
        command.all_day,
        &command.start,
        &command.end,
        Some(time_zone),
    )?;
    Ok(PlannerEvent {
        id,
        owner_id: owner_id.to_string(),
        title: command.title.trim().to_string(),
        description: command.description.trim().to_string(),
        location: command.location.trim().to_string(),
        time_zone: time_zone.to_string(),
        all_day: command.all_day,
        start_has_time: bounds.start_has_time,
        end_has_time: bounds.end_has_time,
        start_at: bounds.start_at,
        end_at: bounds.end_at,
        created_at: now,
        updated_at: now,
    })
}

/// event_application.apply_planner_event_update. Returns whether anything
/// changed (AI-DO's RagSyncOperation.UPSERT marker).
pub fn apply_planner_event_update(
    event: &mut PlannerEvent,
    command: &PlannerEventUpdateRequest,
) -> ApiResult<bool> {
    let mut changed = false;
    let next_all_day = command.all_day.unwrap_or(event.all_day);
    let mut next_start = command.start.clone();
    let mut next_end = command.end.clone();
    if next_start.is_none() != next_end.is_none() {
        return Err(ApiError::bad_request(
            "planner.update_start_end_required",
            "Planner event updates must provide both start and end together.",
        ));
    }
    if command.all_day.is_some() && next_start.is_none() {
        let (start, end) = serialize_event_bounds(
            next_all_day,
            event.start_at,
            event.end_at,
            event.start_has_time,
            event.end_has_time,
            Some(&event.time_zone),
        );
        next_start = Some(start);
        next_end = Some(end);
    }
    if let (Some(start), Some(end)) = (&next_start, &next_end) {
        let bounds = parse_event_bounds(next_all_day, start, end, Some(&event.time_zone))?;
        event.start_at = bounds.start_at;
        event.end_at = bounds.end_at;
        event.start_has_time = bounds.start_has_time;
        event.end_has_time = bounds.end_has_time;
        changed = true;
    }
    event.all_day = next_all_day;
    if command.all_day.is_some() {
        changed = true;
    }
    if let Some(title) = &command.title {
        event.title = title.trim().to_string();
        changed = true;
    }
    if let Some(description) = &command.description {
        event.description = description.trim().to_string();
        changed = true;
    }
    if let Some(location) = &command.location {
        event.location = location.trim().to_string();
        changed = true;
    }
    Ok(changed)
}

fn event_not_found() -> ApiError {
    ApiError::not_found("planner.event_not_found", "Planner event not found.")
}

/// service._load_event: only the owner's own events exist for the caller.
pub async fn load_event(pool: &PgPool, owner_id: &str, event_id: &str) -> ApiResult<PlannerEvent> {
    sqlx::query_as::<_, PlannerEvent>(&format!(
        "SELECT {EVENT_COLUMNS} FROM planner_events WHERE id = $1 AND owner_id = $2"
    ))
    .bind(event_id)
    .bind(owner_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(event_not_found)
}

/// Events of `owner_id` overlapping `[from_at, to_at)`, by start (service and
/// calendar source share this query).
pub async fn list_owner_events(
    pool: &PgPool,
    owner_id: &str,
    range: Option<(NaiveDateTime, NaiveDateTime)>,
) -> ApiResult<Vec<PlannerEvent>> {
    let rows = match range {
        Some((from_at, to_at)) => {
            sqlx::query_as::<_, PlannerEvent>(&format!(
                "SELECT {EVENT_COLUMNS} FROM planner_events \
                 WHERE owner_id = $1 AND end_at > $2 AND start_at < $3 ORDER BY start_at ASC"
            ))
            .bind(owner_id)
            .bind(from_at)
            .bind(to_at)
            .fetch_all(pool)
            .await?
        }
        None => sqlx::query_as::<_, PlannerEvent>(&format!(
            "SELECT {EVENT_COLUMNS} FROM planner_events WHERE owner_id = $1 ORDER BY start_at ASC"
        ))
        .bind(owner_id)
        .fetch_all(pool)
        .await?,
    };
    Ok(rows)
}

pub async fn list_events(
    pool: &PgPool,
    owner_id: &str,
    from_at: Option<NaiveDateTime>,
    to_at: Option<NaiveDateTime>,
) -> ApiResult<PlannerEventsResponse> {
    let range = match (from_at, to_at) {
        (None, None) => None,
        (Some(from_at), Some(to_at)) => {
            if to_at <= from_at {
                return Err(ApiError::bad_request(
                    "calendar.range_to_after_from",
                    "Range 'to' must be strictly after 'from'.",
                ));
            }
            if to_at - from_at > chrono::Duration::days(MAX_LIST_RANGE_DAYS) {
                return Err(ApiError::bad_request(
                    "calendar.range_too_large",
                    format!("Range exceeds maximum {MAX_LIST_RANGE_DAYS} days."),
                ));
            }
            Some((from_at, to_at))
        }
        _ => {
            return Err(ApiError::bad_request(
                "planner.list_range_required",
                "Planner event list requires both 'from' and 'to' together.",
            ));
        }
    };
    let events = list_owner_events(pool, owner_id, range).await?;
    Ok(PlannerEventsResponse {
        items: events.iter().map(project_planner_event).collect(),
    })
}

pub async fn create_event(
    pool: &PgPool,
    owner_id: &str,
    command: &PlannerEventCreateRequest,
    time_zone: &str,
) -> ApiResult<PlannerEventOut> {
    let event = new_planner_event(
        owner_id,
        command,
        time_zone,
        uuid::Uuid::new_v4().to_string(),
        now_utc(),
    )?;
    sqlx::query(
        "INSERT INTO planner_events (id, owner_id, title, description, location, time_zone, \
         all_day, start_has_time, end_has_time, start_at, end_at, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
    )
    .bind(&event.id)
    .bind(&event.owner_id)
    .bind(&event.title)
    .bind(&event.description)
    .bind(&event.location)
    .bind(&event.time_zone)
    .bind(event.all_day)
    .bind(event.start_has_time)
    .bind(event.end_has_time)
    .bind(event.start_at)
    .bind(event.end_at)
    .bind(event.created_at)
    .bind(event.updated_at)
    .execute(pool)
    .await?;
    Ok(project_planner_event(
        &load_event(pool, owner_id, &event.id).await?,
    ))
}

pub async fn update_event(
    pool: &PgPool,
    owner_id: &str,
    event_id: &str,
    command: &PlannerEventUpdateRequest,
) -> ApiResult<PlannerEventOut> {
    let mut tx = pool.begin().await?;
    let mut event = sqlx::query_as::<_, PlannerEvent>(&format!(
        "SELECT {EVENT_COLUMNS} FROM planner_events WHERE id = $1 AND owner_id = $2 FOR UPDATE"
    ))
    .bind(event_id)
    .bind(owner_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(event_not_found)?;
    let before = event.clone();
    apply_planner_event_update(&mut event, command)?;
    if !same_columns(&before, &event) {
        // SQLAlchemy `onupdate` refreshes updated_at when a column changes.
        event.updated_at = now_utc();
    }
    sqlx::query(
        "UPDATE planner_events SET title = $3, description = $4, location = $5, all_day = $6, \
         start_has_time = $7, end_has_time = $8, start_at = $9, end_at = $10, updated_at = $11 \
         WHERE id = $1 AND owner_id = $2",
    )
    .bind(&event.id)
    .bind(owner_id)
    .bind(&event.title)
    .bind(&event.description)
    .bind(&event.location)
    .bind(event.all_day)
    .bind(event.start_has_time)
    .bind(event.end_has_time)
    .bind(event.start_at)
    .bind(event.end_at)
    .bind(event.updated_at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(project_planner_event(
        &load_event(pool, owner_id, event_id).await?,
    ))
}

fn same_columns(left: &PlannerEvent, right: &PlannerEvent) -> bool {
    left.title == right.title
        && left.description == right.description
        && left.location == right.location
        && left.all_day == right.all_day
        && left.start_has_time == right.start_has_time
        && left.end_has_time == right.end_has_time
        && left.start_at == right.start_at
        && left.end_at == right.end_at
}

pub async fn delete_event(pool: &PgPool, owner_id: &str, event_id: &str) -> ApiResult<()> {
    let deleted = sqlx::query("DELETE FROM planner_events WHERE id = $1 AND owner_id = $2")
        .bind(event_id)
        .bind(owner_id)
        .execute(pool)
        .await?;
    if deleted.rows_affected() == 0 {
        return Err(event_not_found());
    }
    Ok(())
}

fn parse_range_param(value: Option<&String>) -> ApiResult<Option<NaiveDateTime>> {
    value
        .map(|value| {
            parse_iso_or_date(value, None).map_err(|error| {
                ApiError::bad_request(
                    "calendar.invalid_iso_datetime",
                    format!("Invalid ISO date/datetime: {error}"),
                )
            })
        })
        .transpose()
}

// ── Router handlers (router.py) ─────────────────────────────────────────────

pub async fn list_planner_events(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<PlannerEventsResponse>> {
    let from_at = parse_range_param(params.get("from"))?;
    let to_at = parse_range_param(params.get("to"))?;
    Ok(Json(
        list_events(&pool, &identity.user_id, from_at, to_at).await?,
    ))
}

pub async fn create_planner_event(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Query(params): Query<HashMap<String, String>>,
    ApiJson(payload): ApiJson<PlannerEventCreateRequest>,
) -> ApiResult<impl IntoResponse> {
    payload.validate()?;
    let time_zone = normalize_time_zone(params.get("timeZone").map(String::as_str));
    let created = create_event(&pool, &identity.user_id, &payload, &time_zone).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn get_planner_event(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(event_id): Path<String>,
) -> ApiResult<Json<PlannerEventOut>> {
    Ok(Json(project_planner_event(
        &load_event(&pool, &identity.user_id, &event_id).await?,
    )))
}

pub async fn update_planner_event(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(event_id): Path<String>,
    ApiJson(payload): ApiJson<PlannerEventUpdateRequest>,
) -> ApiResult<Json<PlannerEventOut>> {
    payload.validate()?;
    Ok(Json(
        update_event(&pool, &identity.user_id, &event_id, &payload).await?,
    ))
}

pub async fn delete_planner_event(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(event_id): Path<String>,
) -> ApiResult<StatusCode> {
    delete_event(&pool, &identity.user_id, &event_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    //! Port of AI-DO apps/api/tests/test_planner_event_application.py.
    //! (`event_access` principal checks have no counterpart: every request
    //! carries exactly one verified user, and AI callers are not ported.)
    use super::*;
    use chrono::NaiveDate;

    fn dt(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, min, 0)
            .unwrap()
    }

    fn stored_event() -> PlannerEvent {
        PlannerEvent {
            id: "event-1".into(),
            owner_id: "user-1".into(),
            title: "Old".into(),
            description: String::new(),
            location: String::new(),
            time_zone: "Asia/Seoul".into(),
            all_day: false,
            start_has_time: true,
            end_has_time: true,
            start_at: dt(2026, 5, 4, 1, 0),
            end_at: dt(2026, 5, 4, 2, 0),
            created_at: dt(2026, 5, 1, 0, 0),
            updated_at: dt(2026, 5, 1, 0, 0),
        }
    }

    #[test]
    fn new_planner_event_builds_model_from_command() {
        let event = new_planner_event(
            "user-1",
            &PlannerEventCreateRequest {
                title: "  Planning sync  ".into(),
                description: "  Align  ".into(),
                location: "  Seoul  ".into(),
                all_day: false,
                start: "2026-05-04T10:00:00+09:00".into(),
                end: "2026-05-04T11:00:00+09:00".into(),
            },
            "Asia/Seoul",
            "event-1".into(),
            dt(2026, 5, 1, 0, 0),
        )
        .unwrap();
        assert_eq!(event.id, "event-1");
        assert_eq!(event.title, "Planning sync");
        assert_eq!(event.description, "Align");
        assert_eq!(event.location, "Seoul");
        assert_eq!(event.start_at, dt(2026, 5, 4, 1, 0));
        assert_eq!(event.end_at, dt(2026, 5, 4, 2, 0));
        assert!(event.start_has_time);
        assert!(event.end_has_time);
    }

    #[test]
    fn apply_planner_event_update_marks_content_for_projection_refresh() {
        let mut event = stored_event();
        let changed = apply_planner_event_update(
            &mut event,
            &PlannerEventUpdateRequest {
                title: Some("  New title  ".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(changed);
        assert_eq!(event.title, "New title");
    }

    #[test]
    fn apply_planner_event_update_rejects_partial_bounds() {
        let mut event = stored_event();
        let error = apply_planner_event_update(
            &mut event,
            &PlannerEventUpdateRequest {
                start: Some("2026-05-04".into()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, "planner_update_start_end_required");
    }

    #[test]
    fn switching_to_all_day_without_dates_reuses_the_stored_local_dates() {
        let mut event = stored_event();
        // 10:00-11:00 KST on one day becomes [2026-05-04, 2026-05-04): rejected
        // exactly like AI-DO (the all-day end must be after the start).
        let error = apply_planner_event_update(
            &mut event,
            &PlannerEventUpdateRequest {
                all_day: Some(true),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "planner_event_end_after_start");
    }

    #[test]
    fn create_request_accepts_camel_and_snake_case_and_validates_lengths() {
        let camel: PlannerEventCreateRequest = serde_json::from_value(serde_json::json!({
            "title": "t", "allDay": true, "start": "2026-05-06", "end": "2026-05-07"
        }))
        .unwrap();
        assert!(camel.all_day);
        assert_eq!(camel.description, "");
        let snake: PlannerEventCreateRequest = serde_json::from_value(serde_json::json!({
            "title": "t", "all_day": true, "start": "2026-05-06", "end": "2026-05-07"
        }))
        .unwrap();
        assert!(snake.all_day);
        let long = PlannerEventCreateRequest {
            title: "x".repeat(201),
            ..camel.clone()
        };
        assert_eq!(
            long.validate().unwrap_err().status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let empty = PlannerEventCreateRequest {
            title: String::new(),
            ..camel
        };
        assert!(empty.validate().is_err());
        let null_update: PlannerEventUpdateRequest =
            serde_json::from_value(serde_json::json!({"title": null, "location": "x"})).unwrap();
        assert!(null_update.title.is_none());
        assert_eq!(null_update.location.as_deref(), Some("x"));
    }

    #[test]
    fn projection_serializes_bounds_in_the_event_time_zone() {
        let mut event = stored_event();
        event.all_day = true;
        event.start_has_time = false;
        event.end_has_time = false;
        event.start_at = dt(2026, 5, 5, 15, 0);
        event.end_at = dt(2026, 5, 7, 15, 0);
        let out = project_planner_event(&event);
        assert_eq!(out.start, "2026-05-06");
        assert_eq!(out.end, "2026-05-08");
        assert_eq!(out.owner_name, "user-1");
        assert_eq!(out.created_at, "2026-05-01T00:00:00");
        let json = serde_json::to_value(&out).unwrap();
        for key in [
            "ownerId",
            "ownerName",
            "timeZone",
            "allDay",
            "startHasTime",
            "endHasTime",
            "createdAt",
            "updatedAt",
        ] {
            assert!(json.get(key).is_some(), "{key}");
        }
    }
}
