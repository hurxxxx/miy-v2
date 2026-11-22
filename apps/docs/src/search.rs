//! The portal's integrated search (manifest `platformEndpoints.search`): docs
//! the person may read now whose title, page titles or page text match. Read
//! only, platform calls only. Readability is decided in the same statement as
//! the match (notify::READABLE_BY_CALLER), so nothing hidden is counted or
//! shown. AI-DO searched docs through its RAG/OpenSearch projection
//! (search_projection.py, excluded); this matches text with ILIKE.
use axum::{Extension, Json, extract::State};
use miy_app_sdk::Identity;
use serde::Deserialize;

use crate::{
    AppState,
    error::ApiResult,
    items,
    notify::{self, READABLE_BY_CALLER, shorten},
    time::utc_iso,
};

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    query: String,
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

/// A short window of `text` around the first case-insensitive match.
pub fn snippet(text: &str, query: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let needle = query.to_lowercase();
    let chars: Vec<char> = text.chars().collect();
    let position = lower.find(&needle)?;
    // Character offset of the match (lowercasing can change byte lengths).
    let at = lower[..position].chars().count().min(chars.len());
    let start = at.saturating_sub(60);
    let end = (at + needle.chars().count() + 140).min(chars.len());
    let mut window: String = chars[start..end].iter().collect();
    if start > 0 {
        window = format!("…{}", window.trim_start());
    }
    if end < chars.len() {
        window.push('…');
    }
    Some(shorten(&window, 240))
}

#[derive(Default, serde::Serialize, utoipa::ToSchema)]
pub struct SearchResponse {
    #[schema(max_items = 10)]
    items: Vec<SearchHit>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
struct SearchHit {
    title: String,
    #[schema(required)]
    snippet: Option<String>,
    path: String,
    #[schema(format = DateTime)]
    at: String,
    kind: &'static str,
}

#[utoipa::path(
    post, path = "/v1/search", operation_id = "searchDocsForPlatform", tag = "Platform callbacks",
    request_body = SearchRequest,
    responses(
        (status = 200, description = "Success", body = SearchResponse),
        (status = 400, description = "Invalid request", body = String, content_type = "text/plain"),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 413, description = "Request exceeds the size limit", body = String, content_type = "text/plain"),
        (status = 415, description = "Unsupported or missing content type", body = String, content_type = "text/plain"),
        (status = 422, description = "Request validation failed", body = String, content_type = "text/plain"),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse)
    )
)]
pub async fn search(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Json(request): Json<SearchRequest>,
) -> ApiResult<Json<SearchResponse>> {
    notify::platform_only(&identity)?;
    let query = request.query.trim();
    if query.is_empty() || query.chars().count() > 200 {
        return Ok(Json(SearchResponse::default()));
    }
    let limit = request.limit.unwrap_or(5).clamp(1, 10);
    let english = request.locale.as_deref() == Some("en-US");
    let sql = format!(
        "SELECT d.id, d.title, d.updated_at, m.page_id, m.page_title, m.search_text \
         FROM docs_native_docs d \
         LEFT JOIN LATERAL (SELECT pg.id AS page_id, pg.title AS page_title, pg.search_text \
             FROM docs_native_doc_pages pg WHERE pg.doc_id = d.id AND pg.trashed_at IS NULL \
               AND (pg.title ILIKE $5 OR pg.search_text ILIKE $5) \
             ORDER BY pg.updated_at DESC LIMIT 1) m ON true \
         WHERE {READABLE_BY_CALLER} AND (d.title ILIKE $5 OR m.page_id IS NOT NULL) \
         ORDER BY d.updated_at DESC LIMIT $6"
    );
    let rows: Vec<(
        String,
        String,
        chrono::NaiveDateTime,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(&sql)
        .bind(&identity.user_id)
        .bind(&identity.context.installation_id)
        .bind(identity.context.environment.as_deref())
        .bind(notify::signed_groups(&identity))
        .bind(like_pattern(query))
        .bind(limit)
        .fetch_all(&state.pool)
        .await?;
    let kind = if english { "Doc" } else { "문서" };
    Ok(Json(SearchResponse {
        items: rows
            .into_iter()
            .map(|(doc_id, title, updated_at, page_id, page_title, text)| {
                let mut path = items::doc_path(&doc_id);
                if let Some(page_id) = &page_id {
                    path.push_str(&format!("?page={}", items::urlencode_segment(page_id)));
                }
                let text_snippet = text.as_deref().and_then(|text| snippet(text, query));
                let snippet = match (page_title, text_snippet) {
                    (Some(page_title), Some(found)) if page_title != title => {
                        Some(format!("{page_title} · {found}"))
                    }
                    (_, Some(found)) => Some(found),
                    (Some(page_title), None) if page_title != title => Some(page_title),
                    _ => None,
                };
                SearchHit {
                    title,
                    snippet,
                    path,
                    at: utc_iso(updated_at),
                    kind,
                }
            })
            .collect(),
    }))
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
    fn snippets_window_the_first_match() {
        assert_eq!(
            snippet("Hello World", "world").as_deref(),
            Some("Hello World")
        );
        let long = format!("{}needle{}", "a".repeat(100), "b".repeat(300));
        let found = snippet(&long, "NEEDLE").unwrap();
        assert!(found.starts_with('…') && found.contains("needle") && found.ends_with('…'));
        assert!(snippet("nothing", "x").is_none());
    }
}
