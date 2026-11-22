//! The portal's integrated search (miy platform → app call, manifest
//! `platformEndpoints.search`), a miy addition: tasks this person may read
//! whose title or reference matches, newest first. AI-DO's own search
//! projection is not ported (M12). Read only; platform calls only.
use axum::{Extension, Json, extract::State};
use chrono::{TimeZone, Utc};
use miy_app_sdk::Identity;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::error::{ApiError, ApiResult};
use crate::models::task_reference;
use crate::notify::{platform_only, task_path};

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    #[serde(default)]
    query: String,
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    locale: Option<String>,
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
pub async fn search(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    body: Option<Json<SearchRequest>>,
) -> ApiResult<Json<Value>> {
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
    let items: Vec<Value> = rows
        .into_iter()
        .map(|(id, title, number, key, list_name, updated_at)| {
            json!({
                "title": title,
                "snippet": format!("{} · {list_name}", task_reference(&key, number)),
                "path": task_path(&id),
                "at": Utc.from_utc_datetime(&updated_at).to_rfc3339(),
                "kind": if english { "Task" } else { "업무" },
            })
        })
        .collect();
    Ok(Json(json!({"items": items})))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_patterns_escape_wildcards() {
        assert_eq!(like_pattern("50%_a\\b"), "%50\\%\\_a\\\\b%");
    }
}
