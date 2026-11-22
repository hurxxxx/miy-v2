//! The portal's integrated search (miy platform → app call, manifest
//! `platformEndpoints.search`), a miy addition: tasks this person may read
//! whose title or reference matches, newest first. AI-DO's own search
//! projection is not ported (M12). Read only; platform calls only.
use axum::{Extension, Json, extract::State};
use chrono::{TimeZone, Utc};
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::{ApiError, ApiResult};
use crate::models::task_reference;
use crate::notify::{platform_only, task_path};

#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    /// Matches task titles and references. Use 1 to 200 characters after trimming.
    #[serde(default)]
    #[schema(required)]
    query: String,
    /// Defaults to 10. Values outside 1 through 10 are clamped.
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    locale: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SearchResponse {
    #[schema(max_items = 10)]
    items: Vec<SearchItem>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct SearchItem {
    title: String,
    snippet: String,
    path: String,
    #[schema(format = DateTime)]
    at: String,
    kind: &'static str,
}

/// A `LIKE` pattern matching `text` anywhere, with `%`, `_` and `\` escaped.
pub fn like_pattern(text: &str) -> String {
    let mut out = String::from("%");
    for ch in text.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('%');
    out
}

/// `POST /v1/search`.
#[utoipa::path(
    post, path = "/v1/search", operation_id = "searchPmsPlatform", tag = "Platform callbacks",
    description = "Platform-only, read-only search across tasks visible to the verified caller.",
    request_body = SearchRequest,
    responses(
        (status = 200, description = "Success", body = SearchResponse),
        (status = 401, description = "Platform identity could not be verified", body = crate::error::IdentityErrorResponse),
        (status = 400, description = "Malformed JSON", body = String, content_type = "text/plain"),
        (status = 403, description = "Current permissions do not allow the operation", body = crate::error::ErrorResponse),
        (status = 413, description = "JSON body exceeds the extractor limit", body = String, content_type = "text/plain"),
        (status = 415, description = "Expected application/json", body = String, content_type = "text/plain"),
        (status = 422, description = "Invalid JSON payload or invalid query", content((crate::error::ErrorResponse = "application/json"), (String = "text/plain"))),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn search(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    body: Option<Json<SearchRequest>>,
) -> ApiResult<Json<SearchResponse>> {
    if !identity.context.platform {
        return Err(platform_only());
    }
    let request = body.map(|Json(body)| body).unwrap_or_default();
    let query = request.query.trim().to_string();
    if query.is_empty() || query.chars().count() > 200 {
        return Err(ApiError::validation("query: 1-200 characters"));
    }
    let limit = request.limit.unwrap_or(10).clamp(1, 10);
    let english = request.locale.as_deref() == Some("en-US");
    let pattern = like_pattern(&query);
    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, String, i32, String, String, chrono::NaiveDateTime)> = sqlx::query_as(
        "SELECT t.id, t.title, t.task_number, l.key, l.name, t.updated_at \
         FROM pms_tasks t JOIN pms_task_lists l ON l.id = t.list_id \
         JOIN pms_spaces s ON s.id = l.team_id AND s.active \
         WHERE NOT t.archived \
         AND (t.title ILIKE $1 ESCAPE '\\' OR (l.key || '-' || t.task_number) ILIKE $1 ESCAPE '\\') \
         AND (EXISTS (SELECT 1 FROM pms_space_members m WHERE m.space_id = s.id AND m.user_id = $2) \
           OR EXISTS (SELECT 1 FROM pms_task_user_access a WHERE a.task_id = t.id AND a.user_id = $2 \
             AND a.revoked_at IS NULL AND (a.expires_at IS NULL OR a.expires_at > (now() AT TIME ZONE 'UTC')))) \
         ORDER BY t.updated_at DESC, t.id LIMIT $3",
    )
    .bind(&pattern)
    .bind(&identity.user_id)
    .bind(limit)
    .fetch_all(&pool)
    .await?;
    let items = rows
        .into_iter()
        .map(
            |(id, title, number, key, list_name, updated_at)| SearchItem {
                title,
                snippet: format!("{} · {list_name}", task_reference(&key, number)),
                path: task_path(&id),
                at: Utc.from_utc_datetime(&updated_at).to_rfc3339(),
                kind: if english { "Task" } else { "업무" },
            },
        )
        .collect();
    Ok(Json(SearchResponse { items }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_patterns_escape_wildcards() {
        assert_eq!(like_pattern("50%_a\\b"), "%50\\%\\_a\\\\b%");
    }
}
