//! The planner's own source of AI-DO's unified calendar endpoint
//! (`domains/calendar`: router, query_policy, source_catalog.PlannerCalendarSource,
//! source_projection.project_planner_calendar_event).
//!
//! AI-DO aggregated meetings, PMS tasks, planner events and Google Calendar
//! feeds here. Cross-app aggregation is not part of this release, so the only
//! source type is `planner_event`; the path, query and response shape stay the
//! same so the ported screen reads its events exactly as before.
use axum::{
    Extension, Json,
    extract::{Query, State},
};
use chrono::{Duration, NaiveDateTime};
use miy_app_sdk::Identity;
use serde::Serialize;
use sqlx::PgPool;
use std::collections::HashMap;

use crate::error::{ApiError, ApiResult};
use crate::event_time::{local_date_string, parse_iso_or_date, utc_iso};
use crate::planner::{PlannerEvent, list_owner_events};

pub const MAX_CALENDAR_RANGE_DAYS: i64 = 366;
pub const DEFAULT_CALENDAR_SOURCES: &[&str] = &["planner_event"];
/// source_projection.SOURCE_COLORS["planner_event"] (teal-500).
pub const PLANNER_EVENT_COLOR: &str = "#14b8a6";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEventMetadata {
    pub meeting_id: Option<String>,
    pub attendee_count: Option<i64>,
    pub task_list_id: Option<String>,
    pub task_list_key: Option<String>,
    pub task_number: Option<i64>,
    pub status: Option<String>,
    pub assignee_ids: Option<Vec<String>>,
    pub planner_event_id: Option<String>,
    pub owner_id: Option<String>,
    pub owner_name: Option<String>,
    pub location: Option<String>,
    pub planner_all_day: Option<bool>,
    pub planner_start_has_time: Option<bool>,
    pub planner_end_has_time: Option<bool>,
    pub planner_time_zone: Option<String>,
    pub external_calendar_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CalendarWorkspaceRef {
    pub id: String,
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEventOut {
    pub id: String,
    pub title: String,
    pub start: String,
    pub end: String,
    pub all_day: bool,
    pub source_type: String,
    pub source_id: String,
    pub color: String,
    pub workspace: Option<CalendarWorkspaceRef>,
    pub metadata: CalendarEventMetadata,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalendarEventsResponse {
    pub items: Vec<CalendarEventOut>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarEventsQuery {
    pub from_at: NaiveDateTime,
    pub to_at: NaiveDateTime,
    pub sources: Vec<&'static str>,
}

/// query_policy.parse_calendar_events_query.
pub fn parse_calendar_events_query(
    from_param: &str,
    to_param: &str,
    sources_param: &str,
) -> ApiResult<CalendarEventsQuery> {
    let parse = |value: &str| {
        parse_iso_or_date(value, None).map_err(|error| {
            ApiError::bad_request(
                "calendar.invalid_iso_datetime",
                format!("Invalid ISO date/datetime: {error}"),
            )
        })
    };
    let from_at = parse(from_param)?;
    let to_at = parse(to_param)?;
    if to_at <= from_at {
        return Err(ApiError::bad_request(
            "calendar.range_to_after_from",
            "Range 'to' must be strictly after 'from'.",
        ));
    }
    if to_at - from_at > Duration::days(MAX_CALENDAR_RANGE_DAYS) {
        return Err(ApiError::bad_request(
            "calendar.range_too_large",
            format!("Range exceeds maximum {MAX_CALENDAR_RANGE_DAYS} days."),
        ));
    }
    Ok(CalendarEventsQuery {
        from_at,
        to_at,
        sources: normalize_calendar_sources(sources_param)?,
    })
}

/// query_policy.normalize_calendar_sources.
pub fn normalize_calendar_sources(sources_param: &str) -> ApiResult<Vec<&'static str>> {
    let requested: std::collections::BTreeSet<&str> = sources_param
        .split(',')
        .map(str::trim)
        .filter(|source| !source.is_empty())
        .collect();
    let invalid: Vec<&str> = requested
        .iter()
        .copied()
        .filter(|source| !DEFAULT_CALENDAR_SOURCES.contains(source))
        .collect();
    if !invalid.is_empty() {
        return Err(ApiError::bad_request(
            "calendar.unknown_source_types",
            format!("Unknown source types: {}", invalid.join(", ")),
        ));
    }
    Ok(DEFAULT_CALENDAR_SOURCES
        .iter()
        .copied()
        .filter(|source| requested.contains(source))
        .collect())
}

fn add_date_string_days(value: &str, days: i64) -> String {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|date| (date + Duration::days(days)).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|_| value.to_string())
}

/// source_projection._project_planner_event_render_bounds.
fn render_bounds(event: &PlannerEvent) -> (String, String, bool) {
    let zone = Some(event.time_zone.as_str());
    if event.all_day {
        return (
            local_date_string(event.start_at, zone),
            local_date_string(event.end_at, zone),
            true,
        );
    }
    if event.start_has_time && event.end_has_time {
        return (utc_iso(event.start_at), utc_iso(event.end_at), false);
    }
    if event.start_has_time {
        let mut marker_end = event.end_at.min(event.start_at + Duration::minutes(30));
        if marker_end <= event.start_at {
            marker_end = event.start_at + Duration::minutes(30);
        }
        return (utc_iso(event.start_at), utc_iso(marker_end), false);
    }
    if event.end_has_time {
        let mut marker_start = event.start_at.max(event.end_at - Duration::minutes(30));
        if marker_start >= event.end_at {
            marker_start = event.end_at - Duration::minutes(30);
        }
        return (utc_iso(marker_start), utc_iso(event.end_at), false);
    }
    let start = local_date_string(event.start_at, zone);
    let end = add_date_string_days(&local_date_string(event.end_at, zone), 1);
    (start, end, true)
}

/// source_projection.project_planner_calendar_event. The owner's name is
/// not available to the app (AI-DO: `owner.full_name`), so it stays null.
pub fn project_planner_calendar_event(event: &PlannerEvent) -> CalendarEventOut {
    let (start, end, all_day) = render_bounds(event);
    CalendarEventOut {
        id: format!("planner-event-{}", event.id),
        title: event.title.clone(),
        start,
        end,
        all_day,
        source_type: "planner_event".into(),
        source_id: event.id.clone(),
        color: PLANNER_EVENT_COLOR.into(),
        workspace: None,
        metadata: CalendarEventMetadata {
            planner_event_id: Some(event.id.clone()),
            owner_id: Some(event.owner_id.clone()),
            owner_name: None,
            location: (!event.location.is_empty()).then(|| event.location.clone()),
            planner_all_day: Some(event.all_day),
            planner_start_has_time: Some(event.start_has_time),
            planner_end_has_time: Some(event.end_has_time),
            planner_time_zone: Some(event.time_zone.clone()),
            ..Default::default()
        },
    }
}

pub async fn list_calendar_events(
    pool: &PgPool,
    owner_id: &str,
    query: &CalendarEventsQuery,
) -> ApiResult<CalendarEventsResponse> {
    let mut items = Vec::new();
    if query.sources.contains(&"planner_event") {
        let events = list_owner_events(pool, owner_id, Some((query.from_at, query.to_at))).await?;
        items.extend(events.iter().map(project_planner_calendar_event));
    }
    // service.list_calendar_events: a stable sort on the wire `start` text.
    items.sort_by(|left, right| left.start.cmp(&right.start));
    Ok(CalendarEventsResponse { items })
}

fn required(params: &HashMap<String, String>, name: &str) -> ApiResult<String> {
    params
        .get(name)
        .cloned()
        .ok_or_else(|| ApiError::validation(format!("{name}: Field required")))
}

/// GET /v1/calendar/events. The caller's own events only; there is no
/// user/assignee parameter (AI-DO ENG-CRIT-1).
pub async fn list_events(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<CalendarEventsResponse>> {
    let from_param = required(&params, "from")?;
    let to_param = required(&params, "to")?;
    let sources = params
        .get("sources")
        .cloned()
        .unwrap_or_else(|| DEFAULT_CALENDAR_SOURCES.join(","));
    let query = parse_calendar_events_query(&from_param, &to_param, &sources)?;
    if query.sources.is_empty() {
        return Ok(Json(CalendarEventsResponse { items: vec![] }));
    }
    Ok(Json(
        list_calendar_events(&pool, &identity.user_id, &query).await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn dt(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, min, 0)
            .unwrap()
    }

    fn event(all_day: bool, start_has_time: bool, end_has_time: bool) -> PlannerEvent {
        PlannerEvent {
            id: "e1".into(),
            owner_id: "u1".into(),
            title: "t".into(),
            description: String::new(),
            location: String::new(),
            time_zone: "Asia/Seoul".into(),
            all_day,
            start_has_time,
            end_has_time,
            start_at: dt(2026, 5, 12, 0, 0),
            end_at: dt(2026, 5, 12, 14, 59),
            created_at: dt(2026, 5, 1, 0, 0),
            updated_at: dt(2026, 5, 1, 0, 0),
        }
    }

    #[test]
    fn range_cap_and_order_are_enforced() {
        let too_large = parse_calendar_events_query("2020-01-01", "2099-12-31", "").unwrap_err();
        assert_eq!(too_large.code, "calendar_range_too_large");
        assert!(too_large.message.contains("366"));
        let reversed = parse_calendar_events_query("2026-04-30", "2026-04-01", "").unwrap_err();
        assert_eq!(reversed.code, "calendar_range_to_after_from");
        let invalid = parse_calendar_events_query("not a date", "also not", "").unwrap_err();
        assert_eq!(invalid.code, "calendar_invalid_iso_datetime");
    }

    #[test]
    fn unknown_sources_are_rejected_and_named() {
        let error = normalize_calendar_sources("planner_event,bogus").unwrap_err();
        assert_eq!(error.code, "calendar_unknown_source_types");
        assert!(error.message.contains("bogus"));
        assert!(normalize_calendar_sources("").unwrap().is_empty());
        assert_eq!(
            normalize_calendar_sources(" planner_event , planner_event").unwrap(),
            vec!["planner_event"]
        );
    }

    #[test]
    fn date_only_and_offset_ranges_read_the_same_instants_as_ai_do() {
        // Date-only bounds are naive UTC midnights (Python fromisoformat).
        let date_only = parse_calendar_events_query("2026-04-15", "2026-04-16", "").unwrap();
        assert_eq!(date_only.from_at, dt(2026, 4, 15, 0, 0));
        let offset = parse_calendar_events_query(
            "2026-04-15T00:00:00+09:00",
            "2026-04-16T00:00:00+09:00",
            "",
        )
        .unwrap();
        assert_eq!(offset.from_at, dt(2026, 4, 14, 15, 0));
    }

    #[test]
    fn projection_renders_partial_times_as_markers() {
        let start_only = project_planner_calendar_event(&event(false, true, false));
        assert!(!start_only.all_day);
        assert_eq!(start_only.start, "2026-05-12T00:00:00+00:00");
        assert_eq!(start_only.end, "2026-05-12T00:30:00+00:00");
        assert_eq!(start_only.metadata.planner_end_has_time, Some(false));

        let end_only = project_planner_calendar_event(&event(false, false, true));
        assert!(!end_only.all_day);
        assert_eq!(end_only.start, "2026-05-12T14:29:00+00:00");
        assert_eq!(end_only.end, "2026-05-12T14:59:00+00:00");

        let mut untimed = event(false, false, false);
        untimed.start_at = dt(2026, 5, 13, 15, 0);
        untimed.end_at = NaiveDate::from_ymd_opt(2026, 5, 14)
            .unwrap()
            .and_hms_micro_opt(14, 59, 59, 999_999)
            .unwrap();
        let no_time = project_planner_calendar_event(&untimed);
        assert!(no_time.all_day);
        assert_eq!(no_time.start, "2026-05-14");
        assert_eq!(no_time.end, "2026-05-15");
        assert_eq!(no_time.metadata.planner_all_day, Some(false));

        let mut whole = event(true, false, false);
        whole.start_at = dt(2026, 5, 5, 15, 0);
        whole.end_at = dt(2026, 5, 7, 15, 0);
        let all_day = project_planner_calendar_event(&whole);
        assert_eq!(
            (all_day.start.as_str(), all_day.end.as_str()),
            ("2026-05-06", "2026-05-08")
        );
        assert_eq!(all_day.id, "planner-event-e1");
        assert_eq!(all_day.color, "#14b8a6");
        assert!(all_day.workspace.is_none());
        assert_eq!(all_day.metadata.location, None);
    }

    #[test]
    fn wire_shape_matches_ai_do() {
        let json = serde_json::to_value(project_planner_calendar_event(&event(false, true, true)))
            .unwrap();
        assert_eq!(json["sourceType"], "planner_event");
        assert_eq!(json["sourceId"], "e1");
        assert_eq!(json["allDay"], false);
        assert!(json["workspace"].is_null());
        assert_eq!(json["metadata"]["plannerEventId"], "e1");
        assert_eq!(json["metadata"]["plannerTimeZone"], "Asia/Seoul");
        assert!(json["metadata"]["meetingId"].is_null());
        assert!(json["metadata"]["externalCalendarId"].is_null());
    }
}
