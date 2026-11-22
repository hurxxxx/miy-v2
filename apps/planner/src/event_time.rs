//! Port of AI-DO `domains/planner/event_time.py`.
//!
//! Instants are stored as naive UTC (`timestamp`), exactly as AI-DO does.
//! `parse_iso_or_date` follows Python 3.12 `datetime.fromisoformat`, which
//! AI-DO relies on: a date-only value is read as a naive midnight (UTC), an
//! offset value is converted to naive UTC and a value without an offset is kept
//! as it is. ISO week dates (`2026-W19-1`) are the one fromisoformat form that
//! is not accepted here.
use chrono::{
    DateTime, Duration, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Offset, TimeZone, Utc,
};
use chrono_tz::Tz;

use crate::error::{ApiError, ApiResult};

pub const DEFAULT_TIME_ZONE: &str = "Asia/Seoul";

/// `event_time_zone`: the named zone, or Asia/Seoul when it is unknown.
pub fn event_time_zone(time_zone: Option<&str>) -> Tz {
    let candidate = time_zone
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_TIME_ZONE);
    candidate.parse::<Tz>().unwrap_or(chrono_tz::Asia::Seoul)
}

/// The IANA name AI-DO would store for a user's zone (validated, defaulted).
pub fn normalize_time_zone(time_zone: Option<&str>) -> String {
    event_time_zone(time_zone).name().to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedEventBounds {
    pub start_at: NaiveDateTime,
    pub end_at: NaiveDateTime,
    pub start_has_time: bool,
    pub end_has_time: bool,
}

/// Python's `ValueError` message for a value fromisoformat rejects.
fn invalid(value: &str) -> String {
    format!("Invalid isoformat string: '{value}'")
}

fn digits(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
}

fn number(value: &str) -> u32 {
    value.parse().unwrap_or(u32::MAX)
}

/// `date.fromisoformat`: `YYYY-MM-DD` or `YYYYMMDD`.
pub fn parse_date_only(value: &str) -> Result<NaiveDate, String> {
    let (year, month, day) = if value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && digits(&value[0..4])
        && digits(&value[5..7])
        && digits(&value[8..10])
    {
        (&value[0..4], &value[5..7], &value[8..10])
    } else if value.len() == 8 && digits(value) {
        (&value[0..4], &value[4..6], &value[6..8])
    } else {
        return Err(invalid(value));
    };
    let year = number(year) as i32;
    if year < 1 {
        return Err(invalid(value));
    }
    NaiveDate::from_ymd_opt(year, number(month), number(day)).ok_or_else(|| invalid(value))
}

/// Length of the date part of an ISO datetime (extended 10 or basic 8).
fn date_length(value: &str) -> usize {
    let bytes = value.as_bytes();
    if bytes.len() >= 10 && bytes[4] == b'-' {
        10
    } else {
        8
    }
}

/// `HH[:MM[:SS[.f+]]]` or basic `HH[MM[SS[.f+]]]`.
fn parse_time(value: &str) -> Option<NaiveTime> {
    let (clock, fraction) = match value.find(['.', ',']) {
        Some(index) => (&value[..index], Some(&value[index + 1..])),
        None => (value, None),
    };
    let parts: Vec<&str> = if clock.contains(':') {
        clock.split(':').collect()
    } else {
        if clock.len() % 2 != 0 || clock.is_empty() || clock.len() > 6 {
            return None;
        }
        (0..clock.len())
            .step_by(2)
            .map(|i| &clock[i..i + 2])
            .collect()
    };
    if parts.is_empty() || parts.len() > 3 || parts.iter().any(|p| p.len() != 2 || !digits(p)) {
        return None;
    }
    if fraction.is_some() && parts.len() != 3 {
        return None;
    }
    let micros = match fraction {
        None => 0,
        Some(f) if digits(f) => {
            let taken: String = f.chars().take(6).collect();
            number(&format!("{taken:0<6}"))
        }
        Some(_) => return None,
    };
    let hour = number(parts[0]);
    let minute = parts.get(1).map_or(0, |p| number(p));
    let second = parts.get(2).map_or(0, |p| number(p));
    NaiveTime::from_hms_micro_opt(hour, minute, second, micros)
}

/// `Z` or `±HH[:MM[:SS[.ffffff]]]` (also basic `±HHMM[SS]`).
fn parse_offset(value: &str) -> Option<FixedOffset> {
    if value == "Z" || value == "z" {
        return FixedOffset::east_opt(0);
    }
    let sign = match value.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let time = parse_time(&value[1..])?;
    use chrono::Timelike;
    if time.nanosecond() != 0 {
        // Python keeps fractional offsets; real zones never use them.
        return None;
    }
    let seconds = (time.hour() * 3600 + time.minute() * 60 + time.second()) as i32;
    if seconds >= 24 * 3600 {
        return None;
    }
    FixedOffset::east_opt(sign * seconds)
}

/// `datetime.fromisoformat`. Returns the wall time and its offset, if any.
pub fn parse_python_isoformat(value: &str) -> Result<(NaiveDateTime, Option<FixedOffset>), String> {
    if !value.is_ascii() {
        return Err(invalid(value));
    }
    let length = date_length(value);
    if value.len() < length {
        return Err(invalid(value));
    }
    let date = parse_date_only(&value[..length]).map_err(|_| invalid(value))?;
    if value.len() == length {
        return Ok((date.and_time(NaiveTime::MIN), None));
    }
    // Any single character may separate the date and the time.
    let rest = &value[length + 1..];
    if rest.is_empty() {
        return Err(invalid(value));
    }
    let offset_at = rest.find(['Z', 'z', '+', '-']);
    let (clock, offset) = match offset_at {
        Some(index) => (&rest[..index], Some(&rest[index..])),
        None => (rest, None),
    };
    let time = parse_time(clock).ok_or_else(|| invalid(value))?;
    let offset = match offset {
        Some(text) => Some(parse_offset(text).ok_or_else(|| invalid(value))?),
        None => None,
    };
    Ok((date.and_time(time), offset))
}

/// `parse_iso_or_date`: naive UTC, or naive as given when the value has no
/// offset. The time zone argument is accepted for parity; on Python 3.11+ a
/// date-only value never reaches AI-DO's time-zone branch.
pub fn parse_iso_or_date(value: &str, _time_zone: Option<&str>) -> Result<NaiveDateTime, String> {
    let (wall, offset) = parse_python_isoformat(value)?;
    Ok(match offset {
        Some(offset) => wall - Duration::seconds(offset.local_minus_utc() as i64),
        None => wall,
    })
}

/// Python `datetime.isoformat()` of a naive value with `+00:00`.
pub fn utc_iso(value: NaiveDateTime) -> String {
    format!("{}+00:00", naive_iso(value))
}

/// Python `datetime.isoformat()` of a naive value (microseconds when present).
pub fn naive_iso(value: NaiveDateTime) -> String {
    use chrono::Timelike;
    let micros = value.nanosecond() / 1000;
    if micros == 0 {
        value.format("%Y-%m-%dT%H:%M:%S").to_string()
    } else {
        format!("{}.{micros:06}", value.format("%Y-%m-%dT%H:%M:%S"))
    }
}

/// Wall time in `zone` to naive UTC, as `datetime.combine(..., tzinfo=zone)`
/// does with fold=0: an ambiguous time takes the earlier offset and a time in
/// a gap takes the offset in effect before the transition.
fn local_to_utc(zone: Tz, wall: NaiveDateTime) -> NaiveDateTime {
    match zone.from_local_datetime(&wall) {
        chrono::LocalResult::Single(value) => value.naive_utc(),
        chrono::LocalResult::Ambiguous(earlier, _) => earlier.naive_utc(),
        chrono::LocalResult::None => {
            let before = zone
                .offset_from_utc_datetime(&(wall - Duration::days(1)))
                .fix();
            wall - Duration::seconds(before.local_minus_utc() as i64)
        }
    }
}

pub fn local_date_string(value: NaiveDateTime, time_zone: Option<&str>) -> String {
    local_date(value, time_zone).format("%Y-%m-%d").to_string()
}

fn local_date(value: NaiveDateTime, time_zone: Option<&str>) -> NaiveDate {
    let utc: DateTime<Utc> = Utc.from_utc_datetime(&value);
    utc.with_timezone(&event_time_zone(time_zone)).date_naive()
}

pub fn local_day_start(value: NaiveDate, time_zone: Option<&str>) -> NaiveDateTime {
    local_to_utc(event_time_zone(time_zone), value.and_time(NaiveTime::MIN))
}

fn local_day_end(value: NaiveDate, time_zone: Option<&str>) -> NaiveDateTime {
    let max = NaiveTime::from_hms_micro_opt(23, 59, 59, 999_999).expect("valid time.max");
    local_to_utc(event_time_zone(time_zone), value.and_time(max))
}

fn has_time_component(value: &str) -> bool {
    value.contains('T')
}

fn all_day_date_required() -> ApiError {
    ApiError::bad_request(
        "planner.all_day_date_required",
        "All-day planner events require YYYY-MM-DD start/end.",
    )
}

fn event_end_after_start() -> ApiError {
    ApiError::bad_request(
        "planner.event_end_after_start",
        "Planner event end must be after start.",
    )
}

pub fn parse_event_bounds(
    all_day: bool,
    start: &str,
    end: &str,
    time_zone: Option<&str>,
) -> ApiResult<ParsedEventBounds> {
    if all_day {
        let start_date = parse_date_only(start).map_err(|_| all_day_date_required())?;
        let end_date = parse_date_only(end).map_err(|_| all_day_date_required())?;
        if end_date <= start_date {
            return Err(event_end_after_start());
        }
        return Ok(ParsedEventBounds {
            start_at: local_day_start(start_date, time_zone),
            end_at: local_day_start(end_date, time_zone),
            start_has_time: false,
            end_has_time: false,
        });
    }

    let start_has_time = has_time_component(start);
    let end_has_time = has_time_component(end);
    let start_at = if start_has_time {
        parse_iso_or_date(start, None)
    } else {
        parse_date_only(start).map(|date| local_day_start(date, time_zone))
    }
    .map_err(|_| all_day_date_required())?;
    let end_at = if end_has_time {
        parse_iso_or_date(end, None)
    } else {
        parse_date_only(end).map(|date| local_day_end(date, time_zone))
    }
    .map_err(|_| all_day_date_required())?;
    if end_at <= start_at {
        return Err(event_end_after_start());
    }
    Ok(ParsedEventBounds {
        start_at,
        end_at,
        start_has_time,
        end_has_time,
    })
}

pub fn serialize_event_bounds(
    all_day: bool,
    start_at: NaiveDateTime,
    end_at: NaiveDateTime,
    start_has_time: bool,
    end_has_time: bool,
    time_zone: Option<&str>,
) -> (String, String) {
    if all_day {
        return (
            local_date_string(start_at, time_zone),
            local_date_string(end_at, time_zone),
        );
    }
    (
        if start_has_time {
            utc_iso(start_at)
        } else {
            local_date_string(start_at, time_zone)
        },
        if end_has_time {
            utc_iso(end_at)
        } else {
            local_date_string(end_at, time_zone)
        },
    )
}

#[cfg(test)]
mod tests {
    //! Port of AI-DO apps/api/tests/test_planner_event_time.py, plus the
    //! fromisoformat forms the port has to accept.
    use super::*;

    fn dt(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, min, 0)
            .unwrap()
    }

    #[test]
    fn parse_iso_or_date_normalizes_offset_datetimes_to_naive_utc() {
        assert_eq!(
            parse_iso_or_date("2026-05-04T10:00:00+09:00", None).unwrap(),
            dt(2026, 5, 4, 1, 0)
        );
        assert_eq!(
            parse_iso_or_date("2026-05-04", None).unwrap(),
            dt(2026, 5, 4, 0, 0)
        );
        // A date-only value is a naive midnight even with a time zone.
        assert_eq!(
            parse_iso_or_date("2026-05-04", Some("Asia/Seoul")).unwrap(),
            dt(2026, 5, 4, 0, 0)
        );
    }

    #[test]
    fn all_day_bounds_round_trip_through_kst_dates() {
        let bounds = parse_event_bounds(true, "2026-05-06", "2026-05-08", None).unwrap();
        assert_eq!(bounds.start_at, dt(2026, 5, 5, 15, 0));
        assert_eq!(bounds.end_at, dt(2026, 5, 7, 15, 0));
        assert!(!bounds.start_has_time);
        assert!(!bounds.end_has_time);
        assert_eq!(
            serialize_event_bounds(true, bounds.start_at, bounds.end_at, true, true, None),
            ("2026-05-06".to_string(), "2026-05-08".to_string())
        );
    }

    #[test]
    fn timed_bounds_require_datetimes_and_emit_utc_iso() {
        let bounds = parse_event_bounds(
            false,
            "2026-05-04T10:00:00+09:00",
            "2026-05-04T12:00:00+09:00",
            None,
        )
        .unwrap();
        assert_eq!(bounds.start_at, dt(2026, 5, 4, 1, 0));
        assert_eq!(bounds.end_at, dt(2026, 5, 4, 3, 0));
        assert!(bounds.start_has_time);
        assert!(bounds.end_has_time);
        assert_eq!(
            serialize_event_bounds(false, bounds.start_at, bounds.end_at, true, true, None),
            (
                "2026-05-04T01:00:00+00:00".to_string(),
                "2026-05-04T03:00:00+00:00".to_string()
            )
        );
        assert_eq!(utc_iso(bounds.start_at), "2026-05-04T01:00:00+00:00");
    }

    #[test]
    fn non_all_day_bounds_allow_optional_start_and_end_times() {
        let start_only =
            parse_event_bounds(false, "2026-05-04T10:00:00+09:00", "2026-05-04", None).unwrap();
        assert!(start_only.start_has_time);
        assert!(!start_only.end_has_time);
        assert_eq!(
            serialize_event_bounds(
                false,
                start_only.start_at,
                start_only.end_at,
                start_only.start_has_time,
                start_only.end_has_time,
                None
            ),
            (
                "2026-05-04T01:00:00+00:00".to_string(),
                "2026-05-04".to_string()
            )
        );

        let end_only =
            parse_event_bounds(false, "2026-05-04", "2026-05-04T18:00:00+09:00", None).unwrap();
        assert!(!end_only.start_has_time);
        assert!(end_only.end_has_time);
        assert_eq!(
            serialize_event_bounds(
                false,
                end_only.start_at,
                end_only.end_at,
                end_only.start_has_time,
                end_only.end_has_time,
                None
            ),
            (
                "2026-05-04".to_string(),
                "2026-05-04T09:00:00+00:00".to_string()
            )
        );

        let no_time = parse_event_bounds(false, "2026-05-04", "2026-05-04", None).unwrap();
        assert!(!no_time.start_has_time);
        assert!(!no_time.end_has_time);
        assert_eq!(
            serialize_event_bounds(
                false,
                no_time.start_at,
                no_time.end_at,
                no_time.start_has_time,
                no_time.end_has_time,
                None
            ),
            ("2026-05-04".to_string(), "2026-05-04".to_string())
        );
    }

    #[test]
    fn invalid_event_bounds_raise_localized_http_errors() {
        let all_day = parse_event_bounds(true, "2026-05-05", "2026-05-04", None).unwrap_err();
        assert_eq!(all_day.status, axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(all_day.code, "planner_event_end_after_start");

        let ordered = parse_event_bounds(
            false,
            "2026-05-04T12:00:00+09:00",
            "2026-05-04T10:00:00+09:00",
            None,
        )
        .unwrap_err();
        assert_eq!(ordered.status, axum::http::StatusCode::BAD_REQUEST);

        let not_dates =
            parse_event_bounds(true, "2026-05-04T10:00", "2026-05-05", None).unwrap_err();
        assert_eq!(not_dates.code, "planner_all_day_date_required");
    }

    #[test]
    fn local_date_string_handles_aware_and_naive_utc_values() {
        assert_eq!(local_date_string(dt(2026, 5, 5, 15, 0), None), "2026-05-06");
        assert_eq!(
            local_date_string(
                parse_iso_or_date("2026-05-06T00:30:00+09:00", None).unwrap(),
                None
            ),
            "2026-05-06"
        );
    }

    #[test]
    fn fromisoformat_forms() {
        assert_eq!(
            parse_iso_or_date("2026-05-04T01:00:00Z", None).unwrap(),
            dt(2026, 5, 4, 1, 0)
        );
        assert_eq!(
            parse_iso_or_date("2026-05-04T10:00:00.000+09:00", None).unwrap(),
            dt(2026, 5, 4, 1, 0)
        );
        assert_eq!(
            parse_iso_or_date("2026-05-04 10:00", None).unwrap(),
            dt(2026, 5, 4, 10, 0)
        );
        assert_eq!(
            parse_iso_or_date("20260504T1000", None).unwrap(),
            dt(2026, 5, 4, 10, 0)
        );
        assert_eq!(
            parse_iso_or_date("2026-05-04T10:00:00-0500", None).unwrap(),
            dt(2026, 5, 4, 15, 0)
        );
        assert_eq!(
            parse_iso_or_date("not a date", None).unwrap_err(),
            "Invalid isoformat string: 'not a date'"
        );
        assert!(parse_iso_or_date("2026-02-30", None).is_err());
        assert!(parse_iso_or_date("2026-05-04T", None).is_err());
        assert!(parse_iso_or_date("2026-05-04T25:00", None).is_err());
        let micro = parse_iso_or_date("2026-05-04T10:00:00.1234567", None).unwrap();
        assert_eq!(naive_iso(micro), "2026-05-04T10:00:00.123456");
    }

    #[test]
    fn unknown_time_zones_fall_back_to_seoul() {
        assert_eq!(normalize_time_zone(Some("Europe/Berlin")), "Europe/Berlin");
        assert_eq!(normalize_time_zone(Some(" Not/AZone ")), "Asia/Seoul");
        assert_eq!(normalize_time_zone(Some("")), "Asia/Seoul");
        assert_eq!(normalize_time_zone(None), "Asia/Seoul");
        let berlin =
            parse_event_bounds(true, "2026-07-01", "2026-07-02", Some("Europe/Berlin")).unwrap();
        assert_eq!(berlin.start_at, dt(2026, 6, 30, 22, 0));
    }

    #[test]
    fn day_bounds_in_a_daylight_saving_gap_use_the_earlier_offset() {
        // America/Sao_Paulo used to start DST at midnight (2018-11-04 00:00 skipped).
        let bounds =
            parse_event_bounds(false, "2018-11-04", "2018-11-04", Some("America/Sao_Paulo"))
                .unwrap();
        assert_eq!(bounds.start_at, dt(2018, 11, 4, 3, 0));
    }
}
