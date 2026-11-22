//! Naive UTC timestamps, as AI-DO stores them (`utcnow_naive`).
use chrono::{DateTime, NaiveDateTime, Timelike, Utc};

/// Now in naive UTC, truncated to the microseconds PostgreSQL keeps.
pub fn now_utc() -> NaiveDateTime {
    let now = Utc::now().naive_utc();
    now.with_nanosecond(now.nanosecond() / 1000 * 1000)
        .unwrap_or(now)
}

/// Pydantic's JSON form of a naive datetime: `2026-05-04T01:00:00[.ffffff]`.
pub fn iso(value: NaiveDateTime) -> String {
    let micros = value.nanosecond() / 1000;
    if micros == 0 {
        value.format("%Y-%m-%dT%H:%M:%S").to_string()
    } else {
        format!("{}.{micros:06}", value.format("%Y-%m-%dT%H:%M:%S"))
    }
}

pub fn iso_opt(value: Option<NaiveDateTime>) -> Option<String> {
    value.map(iso)
}

/// A query datetime (`before`). An offset is converted to UTC; a value
/// without one is taken as UTC, as AI-DO compared it with naive UTC columns.
pub fn parse_query_datetime(value: &str) -> Option<NaiveDateTime> {
    let value = value.trim();
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Some(parsed.with_timezone(&Utc).naive_utc());
    }
    for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f"] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(value, format) {
            return Some(parsed);
        }
    }
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_has_microsecond_precision() {
        assert_eq!(now_utc().nanosecond() % 1000, 0);
    }

    #[test]
    fn iso_matches_pydantic_naive_output() {
        let value =
            NaiveDateTime::parse_from_str("2026-11-22 01:02:03", "%Y-%m-%d %H:%M:%S").unwrap();
        assert_eq!(iso(value), "2026-11-22T01:02:03");
        let value = value.with_nanosecond(5_000).unwrap();
        assert_eq!(iso(value), "2026-11-22T01:02:03.000005");
    }

    #[test]
    fn query_datetimes_accept_offsets_and_naive_values() {
        let utc = parse_query_datetime("2026-11-22T10:00:00+09:00").unwrap();
        assert_eq!(iso(utc), "2026-11-22T01:00:00");
        assert_eq!(
            iso(parse_query_datetime("2026-11-22T01:00:00.123Z").unwrap()),
            "2026-11-22T01:00:00.123000"
        );
        assert_eq!(
            iso(parse_query_datetime("2026-11-22T01:00:00").unwrap()),
            "2026-11-22T01:00:00"
        );
        assert!(parse_query_datetime("yesterday").is_none());
    }
}
