//! The portal's integrated search (miy platform → app call, manifest
//! `platformEndpoints.search`): the person's own events and to-dos whose text
//! matches. Read only; platform calls only. Everything here is the caller's
//! own data, so no further access check applies.
use axum::{Extension, Json, extract::State};
use chrono::{TimeZone, Utc};
use miy_app_sdk::Identity;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::error::{ApiError, ApiResult};
use crate::planner::PlannerEvent;

const MAX_ITEMS: i64 = 10;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    query: String,
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    locale: Option<String>,
}

/// `%`, `_` and `\` match themselves.
fn like_pattern(query: &str) -> String {
    let escaped: String = query
        .chars()
        .flat_map(|c| match c {
            '%' | '_' | '\\' => vec!['\\', c],
            c => vec![c],
        })
        .collect();
    format!("%{escaped}%")
}

pub fn hits(events: &[PlannerEvent], todos: &[(String, String, bool)], english: bool) -> Value {
    let mut items: Vec<Value> = events
        .iter()
        .map(|event| {
            json!({
                "title": event.title,
                "snippet": (!event.location.trim().is_empty()).then(|| event.location.trim().to_owned()),
                "at": (!event.all_day && event.start_has_time)
                    .then(|| Utc.from_utc_datetime(&event.start_at).to_rfc3339()),
                "path": format!("/?event={}", event.id),
                "kind": if english { "Event" } else { "일정" },
            })
        })
        .collect();
    items.extend(todos.iter().map(|(_, title, completed)| {
        json!({
            "title": title,
            "snippet": completed.then(|| if english { "Done" } else { "완료" }),
            "path": "/widgets/todos",
            "kind": if english { "To-do" } else { "할 일" },
        })
    }));
    json!({ "items": items })
}

pub async fn search(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Json(request): Json<SearchRequest>,
) -> ApiResult<Json<Value>> {
    if !identity.context.platform {
        return Err(ApiError::new(
            axum::http::StatusCode::FORBIDDEN,
            "planner.platform_only",
            "Only the platform calls the search.",
        ));
    }
    let query = request.query.trim();
    if query.is_empty() || query.chars().count() > 200 {
        return Ok(Json(json!({ "items": [] })));
    }
    let limit = request.limit.unwrap_or(5).clamp(1, MAX_ITEMS);
    let pattern = like_pattern(query);
    let events = sqlx::query_as::<_, PlannerEvent>(
        "SELECT id, owner_id, title, description, location, time_zone, all_day, \
         start_has_time, end_has_time, start_at, end_at, created_at, updated_at \
         FROM planner_events WHERE owner_id = $1 \
         AND (title ILIKE $2 OR location ILIKE $2 OR description ILIKE $2) \
         ORDER BY start_at DESC LIMIT $3",
    )
    .bind(&identity.user_id)
    .bind(&pattern)
    .bind(limit)
    .fetch_all(&pool)
    .await?;
    let todos: Vec<(String, String, bool)> = sqlx::query_as(
        "SELECT id, title, completed FROM personal_todo_items WHERE user_id = $1 AND title ILIKE $2 \
         ORDER BY completed, sort_order LIMIT $3",
    )
    .bind(&identity.user_id)
    .bind(&pattern)
    .bind((limit - events.len() as i64).max(0))
    .fetch_all(&pool)
    .await?;
    let english = request.locale.as_deref() == Some("en-US");
    Ok(Json(hits(&events, &todos, english)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_patterns_match_text_literally() {
        assert_eq!(like_pattern("회의"), "%회의%");
        assert_eq!(like_pattern("50%_a\\"), "%50\\%\\_a\\\\%");
    }

    #[test]
    fn todos_open_the_todo_panel_screen() {
        let value = hits(&[], &[("t1".into(), "보고서".into(), true)], false);
        assert_eq!(value["items"][0]["kind"], "할 일");
        assert_eq!(value["items"][0]["path"], "/widgets/todos");
        assert_eq!(value["items"][0]["snippet"], "완료");
    }
}
