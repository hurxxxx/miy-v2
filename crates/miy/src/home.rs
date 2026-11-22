//! The portal home's summary cards (M5, docs/CORE-PERMISSIONS.md "플랫폼 → 앱
//! 호출"). For each app the person may use in this portal's environment that
//! declares `platformEndpoints.homeCard`, a read-only platform → app call. An
//! app that does not answer only marks its own card unavailable.
use crate::{
    State,
    api_contract::{HomeCard, HomeCardItem, HomeCardPage},
    auth::Principal,
    http::{ApiError, ApiResult},
    platform_calls::{self, Endpoint, Failure},
    policy,
};
use axum::{Json, extract::State as S};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

/// Cards one home page waits for.
const MAX_CARDS: usize = 12;
const MAX_ITEMS: usize = 5;

pub(crate) fn text(value: &Value, max: usize) -> Option<String> {
    let value = value.as_str()?.trim();
    (!value.is_empty() && !value.chars().any(char::is_control))
        .then(|| value.chars().take(max).collect())
}

pub(crate) fn app_path(value: &Value) -> Option<String> {
    let path = value.as_str()?;
    (path.starts_with('/')
        && !path.starts_with("//")
        && path.len() <= 1024
        && !path.contains('\\')
        && !path.chars().any(char::is_control))
    .then(|| path.to_owned())
}

/// Keep only what the card contract allows; the portal renders text only.
fn card(value: &Value) -> (Option<String>, Option<u32>, Vec<HomeCardItem>) {
    let items = value["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(HomeCardItem {
                        title: text(&item["title"], 120)?,
                        subtitle: text(&item["subtitle"], 160),
                        path: app_path(&item["path"]),
                        at: item["at"]
                            .as_str()
                            .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
                            .map(|at| at.with_timezone(&Utc)),
                    })
                })
                .take(MAX_ITEMS)
                .collect()
        })
        .unwrap_or_default();
    let count = value["count"]
        .as_u64()
        .and_then(|count| u32::try_from(count).ok());
    (text(&value["title"], 80), count, items)
}

pub async fn cards(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
) -> ApiResult<Json<HomeCardPage>> {
    crate::delegations::unrestricted(&p)?;
    let env = p
        .browser_purpose
        .and_then(|purpose| purpose.environment())
        .ok_or_else(ApiError::forbidden)?;
    let rows = sqlx::query(&format!(
        "SELECT i.id,i.app_id,a.name FROM installations i JOIN releases r ON r.id=i.release_id \
         JOIN apps a ON a.id=i.app_id \
         WHERE i.environment=$2 AND jsonb_typeof(r.manifest->'platformEndpoints'->'homeCard')='string' \
         AND {} ORDER BY a.name,i.app_id LIMIT $3",
        policy::may_use_sql("$1", "i.app_id", "i.environment")
    ))
    .bind(&p.id)
    .bind(env)
    .bind(MAX_CARDS as i64)
    .fetch_all(&s.pool)
    .await?;
    // The person's display settings, so an app can say "today" in their zone.
    let (time_zone, locale): (String, String) = sqlx::query_as(
        "SELECT COALESCE((SELECT time_zone FROM user_preferences WHERE user_id=$1),'Asia/Seoul'),\
         COALESCE((SELECT locale FROM user_preferences WHERE user_id=$1),'ko-KR')",
    )
    .bind(&p.id)
    .fetch_one(&s.pool)
    .await?;
    let body = serde_json::json!({ "timeZone": time_zone, "locale": locale });
    let answers = futures_util::future::join_all(rows.iter().map(|row| {
        let id: Uuid = row.get("id");
        let (s, p, body) = (&s, &p, &body);
        async move { platform_calls::call(s, p, id, Endpoint::HomeCard, body).await }
    }))
    .await;
    let cards = rows
        .iter()
        .zip(answers)
        .filter_map(|(row, answer)| {
            let base = |status: &str, title, count, items| HomeCard {
                installation_id: row.get("id"),
                app_id: row.get("app_id"),
                app_name: row.get("name"),
                status: status.to_owned(),
                title,
                count,
                items,
            };
            match answer {
                Ok(value) => {
                    let (title, count, items) = card(&value);
                    Some(base("ok", title, count, items))
                }
                // Lost access between the list and the call: no card at all.
                Err(Failure::Denied | Failure::NotDeclared) => None,
                Err(Failure::Unavailable) => Some(base("unavailable", None, None, Vec::new())),
            }
        })
        .collect();
    Ok(Json(HomeCardPage { cards }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn cards_keep_only_bounded_text_and_app_paths() {
        let (title, count, items) = super::card(&json!({
            "title": "오늘 일정", "count": 3,
            "items": [
                {"title": "주간 회의", "subtitle": "10:00", "path": "/events/1", "at": "2026-11-22T01:00:00Z"},
                {"title": "", "path": "/x"},
                {"title": "외부 링크", "path": "https://evil.example"},
                {"title": "a"}, {"title": "b"}, {"title": "c"}, {"title": "d"}
            ]
        }));
        assert_eq!(title.as_deref(), Some("오늘 일정"));
        assert_eq!(count, Some(3));
        assert_eq!(items.len(), 5);
        assert_eq!(items[0].path.as_deref(), Some("/events/1"));
        assert!(items[0].at.is_some());
        assert_eq!(items[1].title, "외부 링크");
        assert_eq!(items[1].path, None);
    }
}
