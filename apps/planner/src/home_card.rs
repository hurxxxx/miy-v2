//! The portal home's summary card (miy platform → app call, manifest
//! `platformEndpoints.homeCard`): today's events of the person, in their time
//! zone. Read only. The Core sends the person's signed identity with
//! `platform=true` and their display settings in the body; this endpoint
//! answers only such calls.
use axum::{Extension, Json, extract::State};
use chrono::{Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use miy_app_sdk::Identity;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::error::{ApiError, ApiResult};
use crate::event_time::event_time_zone;
use crate::planner::{PlannerEvent, list_owner_events};

const MAX_ITEMS: usize = 5;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeCardRequest {
    #[serde(default)]
    time_zone: Option<String>,
    #[serde(default)]
    locale: Option<String>,
}

/// The UTC instants of the local day `date` in `zone` (stored instants are naive UTC).
fn day_bounds(date: NaiveDate, zone: chrono_tz::Tz) -> (NaiveDateTime, NaiveDateTime) {
    let start = |day: NaiveDate| {
        zone.from_local_datetime(&day.and_hms_opt(0, 0, 0).expect("midnight"))
            .earliest()
            .map(|at| at.with_timezone(&Utc).naive_utc())
            .unwrap_or_else(|| day.and_hms_opt(0, 0, 0).expect("midnight"))
    };
    (start(date), start(date + Duration::days(1)))
}

/// Whether `event` takes place on `date`. All-day events keep their dates as
/// naive midnights; timed events overlap the local day's instants.
fn on_day(event: &PlannerEvent, date: NaiveDate, bounds: (NaiveDateTime, NaiveDateTime)) -> bool {
    if event.all_day {
        event.start_at.date() <= date
            && date
                < event
                    .end_at
                    .date()
                    .max(event.start_at.date() + Duration::days(1))
    } else {
        event.end_at > bounds.0 && event.start_at < bounds.1
    }
}

/// The card for today's events (already chosen by [`on_day`]).
pub fn card(today: &[PlannerEvent], english: bool) -> Value {
    let all_day = if english { "All day" } else { "종일" };
    let items: Vec<Value> = today
        .iter()
        .take(MAX_ITEMS)
        .map(|event| {
            let subtitle = match (event.all_day, event.location.trim()) {
                (true, "") => Some(all_day.to_owned()),
                (true, place) => Some(format!("{all_day} · {place}")),
                (false, "") => None,
                (false, place) => Some(place.to_owned()),
            };
            json!({
                "title": event.title,
                "subtitle": subtitle,
                "at": (!event.all_day).then(|| Utc.from_utc_datetime(&event.start_at).to_rfc3339()),
                "path": format!("/?event={}", event.id),
            })
        })
        .collect();
    json!({
        "title": if english { "Today" } else { "오늘 일정" },
        "count": today.len(),
        "items": items,
    })
}

pub async fn home_card(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    body: Option<Json<HomeCardRequest>>,
) -> ApiResult<Json<Value>> {
    if !identity.context.platform {
        return Err(ApiError::new(
            axum::http::StatusCode::FORBIDDEN,
            "planner.platform_only",
            "Only the platform calls the home card.",
        ));
    }
    let request = body.map(|Json(body)| body).unwrap_or_default();
    let zone = event_time_zone(request.time_zone.as_deref());
    let date = Utc::now().with_timezone(&zone).date_naive();
    let local = day_bounds(date, zone);
    // Widen by a day on both sides so all-day events (naive dates) are found too.
    let events = list_owner_events(
        &pool,
        &identity.user_id,
        Some((local.0 - Duration::days(1), local.1 + Duration::days(1))),
    )
    .await?;
    let events: Vec<PlannerEvent> = events
        .into_iter()
        .filter(|event| on_day(event, date, local))
        .collect();
    let english = request.locale.as_deref() == Some("en-US");
    Ok(Json(card(&events, english)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn event(id: &str, all_day: bool, start: &str, end: &str) -> PlannerEvent {
        let at = |value: &str| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M").unwrap();
        PlannerEvent {
            id: id.into(),
            owner_id: "u".into(),
            title: format!("event {id}"),
            description: String::new(),
            location: if all_day {
                String::new()
            } else {
                "회의실 A".into()
            },
            time_zone: "Asia/Seoul".into(),
            all_day,
            start_has_time: !all_day,
            end_has_time: !all_day,
            start_at: at(start),
            end_at: at(end),
            created_at: at(start),
            updated_at: at(start),
        }
    }

    #[test]
    fn today_in_the_persons_zone() {
        let seoul = chrono_tz::Asia::Seoul;
        let date = NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
        let bounds = day_bounds(date, seoul);
        // 09:00 KST on the 22nd is 00:00 UTC; 23:30 KST on the 21st is not today.
        assert!(on_day(
            &event("a", false, "2026-11-22 00:00", "2026-11-22 01:00"),
            date,
            bounds
        ));
        assert!(!on_day(
            &event("b", false, "2026-11-21 14:30", "2026-11-21 14:50"),
            date,
            bounds
        ));
        // All-day events by their dates only: the 22nd yes, the 21st no.
        assert!(on_day(
            &event("c", true, "2026-11-22 00:00", "2026-11-23 00:00"),
            date,
            bounds
        ));
        assert!(!on_day(
            &event("d", true, "2026-11-21 00:00", "2026-11-22 00:00"),
            date,
            bounds
        ));
    }

    #[test]
    fn the_card_is_bounded_display_text_with_app_paths() {
        let events: Vec<PlannerEvent> = (0..7)
            .map(|i| {
                event(
                    &i.to_string(),
                    i == 0,
                    "2026-11-22 00:00",
                    "2026-11-23 00:00",
                )
            })
            .collect();
        let value = card(&events, false);
        assert_eq!(value["title"], "오늘 일정");
        assert_eq!(value["count"], 7);
        assert_eq!(value["items"].as_array().unwrap().len(), MAX_ITEMS);
        assert_eq!(value["items"][0]["subtitle"], "종일");
        assert_eq!(value["items"][0]["at"], Value::Null);
        assert_eq!(value["items"][1]["path"], "/?event=1");
        assert_eq!(value["items"][1]["subtitle"], "회의실 A");
        assert_eq!(card(&events, true)["title"], "Today");
    }
}
