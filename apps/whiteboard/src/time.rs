//! Naive UTC timestamps, as AI-DO stores them (`utcnow_naive`).
use chrono::{NaiveDateTime, Timelike, Utc};
use serde::Serializer;

/// Now in naive UTC, truncated to the microseconds PostgreSQL keeps.
pub fn now_utc() -> NaiveDateTime {
    let now = Utc::now().naive_utc();
    now.with_nanosecond(now.nanosecond() / 1000 * 1000)
        .unwrap_or(now)
}

/// Pydantic's JSON form of a naive datetime: `2026-05-04T01:00:00[.ffffff]`.
pub fn naive_iso(value: NaiveDateTime) -> String {
    let micros = value.nanosecond() / 1000;
    if micros == 0 {
        value.format("%Y-%m-%dT%H:%M:%S").to_string()
    } else {
        format!("{}.{micros:06}", value.format("%Y-%m-%dT%H:%M:%S"))
    }
}

/// RFC 3339 in UTC (`2026-05-04T01:00:00Z`), for platform display data.
pub fn utc_iso(value: NaiveDateTime) -> String {
    format!("{}Z", value.format("%Y-%m-%dT%H:%M:%S"))
}

pub fn serialize<S: Serializer>(value: &NaiveDateTime, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&naive_iso(*value))
}

/// `serialize` for optional timestamps (`null` when absent).
pub fn serialize_opt<S: Serializer>(
    value: &Option<NaiveDateTime>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => serializer.serialize_str(&naive_iso(*value)),
        None => serializer.serialize_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_has_microsecond_precision() {
        assert_eq!(now_utc().nanosecond() % 1000, 0);
    }

    #[test]
    fn naive_values_serialize_like_pydantic() {
        let value = chrono::NaiveDate::from_ymd_opt(2026, 5, 4)
            .unwrap()
            .and_hms_micro_opt(1, 2, 3, 0)
            .unwrap();
        assert_eq!(naive_iso(value), "2026-05-04T01:02:03");
        assert_eq!(utc_iso(value), "2026-05-04T01:02:03Z");
        let value = value.with_nanosecond(45_000).unwrap();
        assert_eq!(naive_iso(value), "2026-05-04T01:02:03.000045");
    }
}
