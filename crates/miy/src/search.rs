//! Integrated search (M12, docs/CORE-PERMISSIONS.md "플랫폼 → 앱 호출"). For
//! each app the person may use in this portal's environment that declares
//! `platformEndpoints.search`, a read-only platform → app call with the query.
//! Each app decides what this person may see; the platform shows only what the
//! apps return, cut to the search contract. An app that does not answer is
//! marked unavailable instead of holding up the others.
use crate::{
    State,
    api_contract::{SearchGroup, SearchItem, SearchPage},
    auth::Principal,
    home::{app_path, text},
    http::{ApiError, ApiResult},
    platform_calls::{self, Endpoint, Failure},
    policy,
};
use axum::{
    Json,
    extract::{Query, State as S},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

/// Apps one search waits for.
const MAX_APPS: usize = 12;
const DEFAULT_LIMIT: usize = 5;
const MAX_LIMIT: usize = 10;
const MAX_QUERY: usize = 200;

#[derive(Debug, Deserialize)]
pub struct Params {
    q: Option<String>,
    limit: Option<usize>,
}

/// The query as apps receive it: trimmed, 1-200 characters, no control characters.
fn query(value: Option<&str>) -> ApiResult<String> {
    let value = value.unwrap_or("").trim();
    if value.is_empty() || value.chars().count() > MAX_QUERY || value.chars().any(char::is_control)
    {
        return Err(ApiError::bad("invalid_search_query"));
    }
    Ok(value.to_owned())
}

/// Keep only what the search contract allows; the portal renders text only.
fn items(value: &Value, limit: usize) -> Vec<SearchItem> {
    value["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(SearchItem {
                        title: text(&item["title"], 120)?,
                        snippet: text(&item["snippet"], 240),
                        path: app_path(&item["path"]),
                        at: item["at"]
                            .as_str()
                            .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
                            .map(|at| at.with_timezone(&Utc)),
                        kind: text(&item["kind"], 40),
                    })
                })
                .take(limit)
                .collect()
        })
        .unwrap_or_default()
}

pub async fn search(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Query(params): Query<Params>,
) -> ApiResult<Json<SearchPage>> {
    crate::delegations::unrestricted(&p)?;
    let env = p
        .browser_purpose
        .and_then(|purpose| purpose.environment())
        .ok_or_else(ApiError::forbidden)?;
    let query = query(params.q.as_deref())?;
    let limit = params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let rows = sqlx::query(&format!(
        "SELECT i.id,i.app_id,a.name FROM installations i JOIN releases r ON r.id=i.release_id \
         JOIN apps a ON a.id=i.app_id \
         WHERE i.environment=$2 AND jsonb_typeof(r.manifest->'platformEndpoints'->'search')='string' \
         AND {} ORDER BY a.name,i.app_id LIMIT $3",
        policy::may_use_sql("$1", "i.app_id", "i.environment")
    ))
    .bind(&p.id)
    .bind(env)
    .bind(MAX_APPS as i64)
    .fetch_all(&s.pool)
    .await?;
    let (time_zone, locale): (String, String) = sqlx::query_as(
        "SELECT COALESCE((SELECT time_zone FROM user_preferences WHERE user_id=$1),'Asia/Seoul'),\
         COALESCE((SELECT locale FROM user_preferences WHERE user_id=$1),'ko-KR')",
    )
    .bind(&p.id)
    .fetch_one(&s.pool)
    .await?;
    let body = serde_json::json!({
        "query": query, "limit": limit, "timeZone": time_zone, "locale": locale,
    });
    let answers = futures_util::future::join_all(rows.iter().map(|row| {
        let id: Uuid = row.get("id");
        let (s, p, body) = (&s, &p, &body);
        async move { platform_calls::call(s, p, id, Endpoint::Search, body).await }
    }))
    .await;
    let groups = rows
        .iter()
        .zip(answers)
        .filter_map(|(row, answer)| {
            let group = |status: &str, items| SearchGroup {
                installation_id: row.get("id"),
                app_id: row.get("app_id"),
                app_name: row.get("name"),
                status: status.to_owned(),
                items,
            };
            match answer {
                Ok(value) => Some(group("ok", items(&value, limit))),
                // Lost access between the list and the call: nothing at all.
                Err(Failure::Denied | Failure::NotDeclared) => None,
                Err(Failure::Unavailable) => Some(group("unavailable", Vec::new())),
            }
        })
        .collect();
    Ok(Json(SearchPage { query, groups }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn queries_are_bounded_text() {
        assert_eq!(super::query(Some("  회의  ")).unwrap(), "회의");
        assert!(super::query(Some("   ")).is_err());
        assert!(super::query(None).is_err());
        assert!(super::query(Some("a\u{0}b")).is_err());
        assert!(super::query(Some(&"가".repeat(201))).is_err());
        assert!(super::query(Some(&"가".repeat(200))).is_ok());
    }

    #[test]
    fn hits_keep_only_bounded_text_and_app_paths() {
        let items = super::items(
            &json!({"items": [
                {"title": "주간 회의", "snippet": "회의실 A", "path": "/?event=1", "at": "2026-11-22T10:00:00+09:00", "kind": "일정", "html": "<b>x</b>"},
                {"title": "", "path": "/x"},
                {"title": "외부", "path": "//evil.example/x"},
                {"title": "c"}, {"title": "d"}
            ]}),
            3,
        );
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].kind.as_deref(), Some("일정"));
        assert_eq!(
            items[0].at.unwrap().to_rfc3339(),
            "2026-11-22T01:00:00+00:00"
        );
        assert_eq!(items[1].title, "외부");
        assert_eq!(items[1].path, None);
    }
}
