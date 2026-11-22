//! The portal's integrated search (miy platform → app call, manifest
//! `platformEndpoints.search`): boards the person may open now whose title
//! matches. Read only; platform calls only. Access is the same app-DB rule as
//! the notification recheck (notify.rs `readable_board_condition`, with the
//! person's signed groups); boards the person cannot open are never counted
//! or named.
use axum::{Extension, Json, extract::State};
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    error::{ApiResult, ErrorResponse, IdentityErrorResponse},
    notify::{self, platform_only},
    spaces::GLOBAL,
    time::utc_iso,
    whiteboard::board_path,
};

const MAX_ITEMS: i64 = 10;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    /// A trimmed query of 1 to 200 characters returns eligible matches.
    query: String,
    /// Defaults to 5. Supplied values are clamped to the range 1 to 10.
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    locale: Option<String>,
}

/// `%`, `_` and `\` match themselves.
pub fn like_pattern(query: &str) -> String {
    let escaped: String = query
        .chars()
        .flat_map(|c| match c {
            '%' | '_' | '\\' => vec!['\\', c],
            c => vec![c],
        })
        .collect();
    format!("%{escaped}%")
}

type Row = (String, String, String, NaiveDateTime, Option<String>, bool);

#[derive(Serialize, utoipa::ToSchema)]
struct SearchItem {
    title: String,
    snippet: String,
    path: String,
    #[schema(format = DateTime)]
    at: String,
    kind: &'static str,
}

#[derive(Default, Serialize, utoipa::ToSchema)]
pub struct SearchResponse {
    items: Vec<SearchItem>,
}

fn hits(rows: Vec<Row>, english: bool) -> SearchResponse {
    let items = rows
        .into_iter()
        .map(
            |(id, title, workspace_id, updated_at, space_name, shared)| {
                // AI-DO's location labels: the space's shared boards, or private.
                let snippet = if !shared {
                    if english { "Private" } else { "비공개" }.to_string()
                } else if workspace_id == GLOBAL {
                    if english {
                        "Workspace whiteboards"
                    } else {
                        "워크스페이스 화이트보드"
                    }
                    .to_string()
                } else {
                    space_name.unwrap_or_default()
                };
                SearchItem {
                    title: notify::shorten(&title, 120),
                    snippet,
                    path: board_path(&id, &workspace_id),
                    at: utc_iso(updated_at),
                    kind: if english {
                        "Whiteboard"
                    } else {
                        "화이트보드"
                    },
                }
            },
        )
        .collect();
    SearchResponse { items }
}

#[utoipa::path(
    post, path = "/v1/search", operation_id = "searchWhiteboardForPlatform", tag = "Platform callbacks",
    description = "Search titles of boards the person can open. Invalid JSON, missing or invalid search fields, and blank or overly long queries return an empty result. The platform's timeZone field is ignored.",
    request_body = SearchRequest,
    responses(
        (status = 200, description = "Currently readable search results", body = SearchResponse),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Only the platform may call this endpoint", body = ErrorResponse),
        (status = 413, description = "Request body exceeds the application limit", body = String, content_type = "text/plain"),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "Signed directory information is unavailable", body = ErrorResponse),
    )
)]
pub async fn search(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    body: axum::body::Bytes,
) -> ApiResult<Json<SearchResponse>> {
    platform_only(&identity)?;
    let Ok(request) = serde_json::from_slice::<SearchRequest>(&body) else {
        return Ok(Json(SearchResponse::default()));
    };
    let query = request.query.trim();
    if query.is_empty() || query.chars().count() > 200 {
        return Ok(Json(SearchResponse::default()));
    }
    let limit = request.limit.unwrap_or(5).clamp(1, MAX_ITEMS);
    let english = request.locale.as_deref() == Some("en-US");
    let sql = format!(
        "SELECT w.id, w.title, w.workspace_id, w.updated_at, sp.name, \
           EXISTS (SELECT 1 FROM whiteboard_targets t WHERE t.whiteboard_id = w.id AND t.target_app = 'whiteboard' \
             AND t.target_type = 'workspace_sidebar' AND t.target_id = w.workspace_id) \
         FROM whiteboards w LEFT JOIN whiteboard_spaces sp ON sp.id = w.workspace_id \
         WHERE w.title ILIKE $1 ESCAPE '\\' AND {} ORDER BY w.updated_at DESC, w.id LIMIT $6",
        notify::readable_board_condition()
    );
    let rows: Vec<Row> = sqlx::query_as(&sql)
        .bind(like_pattern(query))
        .bind(&identity.user_id)
        .bind(&identity.context.installation_id)
        .bind(identity.context.environment.as_deref())
        .bind(notify::signed_groups(&identity)?)
        .bind(limit)
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(hits(rows, english)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_patterns_escape_wildcards() {
        assert_eq!(like_pattern("a%b_c\\d"), "%a\\%b\\_c\\\\d%");
    }
}
