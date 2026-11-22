//! The portal's integrated search (miy platform → app call, manifest
//! `platformEndpoints.search`): the person's own events and to-dos whose text
//! matches. Read only; platform calls only. Everything here is the caller's
//! own data, so no further access check applies.
use axum::{Extension, Json, extract::State};
use chrono::{TimeZone, Utc};
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::{ApiError, ApiErrorResponse, ApiResult, GatewayErrorResponse};
use crate::planner::PlannerEvent;

const MAX_ITEMS: i64 = 10;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    /// Leading and trailing whitespace is ignored. Blank queries or queries
    /// longer than 200 characters return no items.
    query: String,
    /// Defaults to 5. Other values are clamped to the range 1 to 10.
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    locale: Option<String>,
}

#[derive(Default, Serialize, utoipa::ToSchema)]
pub struct SearchResponse {
    #[schema(max_items = 10)]
    items: Vec<SearchHit>,
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(untagged)]
enum SearchHit {
    Event(EventSearchHit),
    Todo(TodoSearchHit),
}

#[derive(Serialize, utoipa::ToSchema)]
struct EventSearchHit {
    title: String,
    #[schema(required)]
    snippet: Option<String>,
    #[schema(required, format = DateTime)]
    at: Option<String>,
    path: String,
    kind: &'static str,
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct TodoSearchHit {
    title: String,
    #[schema(required)]
    snippet: Option<&'static str>,
    path: &'static str,
    kind: &'static str,
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

fn hits(
    events: &[PlannerEvent],
    todos: &[(String, String, bool)],
    english: bool,
) -> SearchResponse {
    let mut items: Vec<_> = events
        .iter()
        .map(|event| {
            SearchHit::Event(EventSearchHit {
                title: event.title.clone(),
                snippet: (!event.location.trim().is_empty())
                    .then(|| event.location.trim().to_owned()),
                at: (!event.all_day && event.start_has_time)
                    .then(|| Utc.from_utc_datetime(&event.start_at).to_rfc3339()),
                path: format!("/?event={}", event.id),
                kind: if english { "Event" } else { "일정" },
            })
        })
        .collect();
    items.extend(todos.iter().map(|(_, title, completed)| {
        SearchHit::Todo(TodoSearchHit {
            title: title.clone(),
            snippet: completed.then(|| if english { "Done" } else { "완료" }),
            path: "/widgets/todos",
            kind: if english { "To-do" } else { "할 일" },
        })
    }));
    SearchResponse { items }
}

/// Platform callback for integrated search.
#[utoipa::path(
    post, path = "/v1/search", operation_id = "searchPlanner",
    request_body = SearchRequest,
    responses(
        (status = 200, description = "Matching events and to-dos for the signed-in user", body = SearchResponse),
        (status = 400, description = "Malformed JSON", body = String, content_type = "text/plain"),
        (status = 401, description = "Platform identity could not be verified", body = GatewayErrorResponse),
        (status = 403, description = "Platform calls only", body = ApiErrorResponse),
        (status = 413, description = "Request body too large", body = String, content_type = "text/plain"),
        (status = 415, description = "Unsupported or missing content type", body = String, content_type = "text/plain"),
        (status = 422, description = "Invalid search request", body = String, content_type = "text/plain"),
        (status = 500, description = "Could not search the events and to-dos", body = ApiErrorResponse)
    )
)]
pub async fn search(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Json(request): Json<SearchRequest>,
) -> ApiResult<Json<SearchResponse>> {
    if !identity.context.platform {
        return Err(ApiError::new(
            axum::http::StatusCode::FORBIDDEN,
            "planner.platform_only",
            "Only the platform calls the search.",
        ));
    }
    let query = request.query.trim();
    if query.is_empty() || query.chars().count() > 200 {
        return Ok(Json(SearchResponse::default()));
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
        let value = serde_json::to_value(hits(&[], &[("t1".into(), "보고서".into(), true)], false))
            .unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "items": [{
                "title": "보고서",
                "kind": "할 일",
                "path": "/widgets/todos",
                "snippet": "완료",
            }] }),
        );
    }
}
