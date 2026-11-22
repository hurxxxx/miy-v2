//! Naive UTC timestamps, as AI-DO stores them (`utcnow_naive`).
use chrono::{NaiveDateTime, Timelike, Utc};

/// Now in naive UTC, truncated to the microseconds PostgreSQL keeps.
pub fn now_utc() -> NaiveDateTime {
    let now = Utc::now().naive_utc();
    now.with_nanosecond(now.nanosecond() / 1000 * 1000)
        .unwrap_or(now)
}

/// Pydantic's JSON form of a naive datetime: `2026-05-04T01:00:00[.ffffff]`.
pub fn serialize_naive(value: NaiveDateTime) -> String {
    crate::event_time::naive_iso(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_has_microsecond_precision() {
        assert_eq!(now_utc().nanosecond() % 1000, 0);
    }
}
