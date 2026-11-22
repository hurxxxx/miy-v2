//! Message search (COMMON-FEATURES-PLAN §2.2 "보강"; AI-DO 0002af062 had
//! none) and the portal's integrated search (`platformEndpoints.search`).
//!
//! Text only: the message body, case-insensitive substring. Only conversations
//! the caller is an active participant of now, only messages they can see
//! there (joined before), never deleted or moderation-hidden messages.
use crate::{
    conversations::query_limit,
    error::{ApiError, ApiResult},
    notify,
    request::route_id,
    store::{self, Conversation},
    time::{iso, parse_query_datetime},
    users, views,
};
use axum::{
    Extension, Json,
    extract::{Query, State},
};
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use std::collections::HashMap;

pub const QUERY_MAX: usize = 200;
const SNIPPET_RADIUS: usize = 40;
const PLATFORM_SNIPPET_MAX: usize = 240;

/// `ILIKE` pattern with `%`, `_` and `\` escaped (ESCAPE '\').
pub fn like_pattern(query: &str) -> String {
    let mut escaped = String::with_capacity(query.len() + 2);
    escaped.push('%');
    for c in query.chars() {
        if matches!(c, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped.push('%');
    escaped
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// A short excerpt around the first match, `…` where cut. Newlines become spaces.
pub fn snippet(body: &str, query: &str) -> String {
    let chars: Vec<char> = body
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    let needle: Vec<char> = query.chars().map(fold).collect();
    let found = (!needle.is_empty() && needle.len() <= chars.len())
        .then(|| {
            (0..=chars.len() - needle.len())
                .find(|&i| (0..needle.len()).all(|j| fold(chars[i + j]) == needle[j]))
        })
        .flatten()
        .unwrap_or(0);
    let start = found.saturating_sub(SNIPPET_RADIUS);
    let end = (found + needle.len() + SNIPPET_RADIUS).min(chars.len());
    let mut text: String = chars[start..end].iter().collect();
    if start > 0 {
        text.insert(0, '…');
    }
    if end < chars.len() {
        text.push('…');
    }
    text
}

fn normalize_query(value: &str) -> ApiResult<String> {
    let query = value.trim();
    let length = query.chars().count();
    if length == 0 || length > QUERY_MAX {
        return Err(ApiError::validation("q: 1-200 characters"));
    }
    Ok(query.to_string())
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct Hit {
    id: String,
    conversation_id: String,
    sequence: i32,
    sender_id: String,
    body: String,
    created_at: NaiveDateTime,
}

async fn find(
    conn: &mut PgConnection,
    user: &str,
    query: &str,
    conversation: Option<&str>,
    before: Option<(NaiveDateTime, String)>,
    limit: i64,
) -> ApiResult<Vec<Hit>> {
    let (before_at, before_id) = before.unzip();
    Ok(sqlx::query_as::<_, Hit>(
        "SELECT m.id, m.conversation_id, m.sequence, m.sender_id, m.body, m.created_at \
         FROM dm_messages m JOIN dm_conversation_participants p \
           ON p.conversation_id = m.conversation_id AND p.user_id = $1 AND p.left_at IS NULL \
         WHERE m.created_at >= p.joined_at AND m.deleted_at IS NULL AND m.moderation_hidden_at IS NULL \
           AND m.body ILIKE $2 ESCAPE '\\' \
           AND ($3::text IS NULL OR m.conversation_id = $3) \
           AND ($4::timestamp IS NULL OR (m.created_at, m.id) < ($4, $5)) \
         ORDER BY m.created_at DESC, m.id DESC LIMIT $6",
    )
    .bind(user)
    .bind(like_pattern(query))
    .bind(conversation)
    .bind(before_at)
    .bind(before_id)
    .bind(limit)
    .fetch_all(conn)
    .await?)
}

/// Conversations of the hits with display names for `me`.
async fn conversation_names(
    pool: &PgPool,
    identity: &Identity,
    hits: &[Hit],
) -> ApiResult<(HashMap<String, Conversation>, users::Names)> {
    let mut conn = pool.acquire().await?;
    let mut conversations = HashMap::new();
    for hit in hits {
        if conversations.contains_key(&hit.conversation_id) {
            continue;
        }
        if let Some(conversation) =
            store::load_conversation(&mut conn, &hit.conversation_id, false).await?
        {
            conversations.insert(hit.conversation_id.clone(), conversation);
        }
    }
    drop(conn);
    let mut ids: Vec<String> = hits.iter().map(|hit| hit.sender_id.clone()).collect();
    for conversation in conversations.values() {
        ids.extend(conversation.active().map(|p| p.user_id.clone()));
    }
    let names = users::names(pool, identity, ids).await;
    Ok((conversations, names))
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    conversation_id: Option<String>,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    limit: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchItem {
    message_id: String,
    conversation_id: String,
    conversation_name: String,
    conversation_type: String,
    sequence: i32,
    sender_id: String,
    sender_name: String,
    snippet: String,
    created_at: String,
}

fn parse_cursor(value: &str) -> ApiResult<(NaiveDateTime, String)> {
    let (at, id) = value
        .split_once('|')
        .ok_or_else(|| ApiError::validation("cursor"))?;
    route_id(id)?;
    let at = parse_query_datetime(at).ok_or_else(|| ApiError::validation("cursor"))?;
    Ok((at, id.to_string()))
}

/// `GET /v1/dm/search?q&conversation_id&cursor&limit`.
pub async fn search_messages(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Query(query): Query<SearchQuery>,
) -> ApiResult<Json<Value>> {
    let text = normalize_query(query.q.as_deref().unwrap_or_default())?;
    let limit = i64::from(query_limit(query.limit.as_deref(), 20)?.min(50));
    let before = query.cursor.as_deref().map(parse_cursor).transpose()?;
    let me = identity.user_id.as_str();
    let mut conn = pool.acquire().await?;
    if let Some(conversation_id) = query.conversation_id.as_deref() {
        route_id(conversation_id)?;
        store::require_user_conversation(&mut conn, me, conversation_id, false).await?;
    }
    let mut hits = find(
        &mut conn,
        me,
        &text,
        query.conversation_id.as_deref(),
        before,
        limit + 1,
    )
    .await?;
    drop(conn);
    let next_cursor = (hits.len() as i64 > limit).then(|| {
        hits.truncate(limit as usize);
        hits.last()
            .map(|hit| format!("{}|{}", iso(hit.created_at), hit.id))
    });
    let (conversations, names) = conversation_names(&pool, &identity, &hits).await?;
    let items: Vec<SearchItem> = hits
        .iter()
        .filter_map(|hit| {
            let conversation = conversations.get(&hit.conversation_id)?;
            Some(SearchItem {
                message_id: hit.id.clone(),
                conversation_id: hit.conversation_id.clone(),
                conversation_name: views::conversation_display_name(conversation, me, &names),
                conversation_type: conversation.row.conversation_type.clone(),
                sequence: hit.sequence,
                sender_id: hit.sender_id.clone(),
                sender_name: names.name(&hit.sender_id),
                snippet: snippet(&hit.body, &text),
                created_at: iso(hit.created_at),
            })
        })
        .collect();
    Ok(Json(
        json!({"items": items, "next_cursor": next_cursor.flatten()}),
    ))
}

#[derive(Debug, Deserialize)]
pub struct PlatformSearchRequest {
    query: String,
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    locale: Option<String>,
}

pub fn message_path(conversation_id: &str, message_id: &str) -> String {
    format!(
        "{}?message={message_id}",
        notify::conversation_path(conversation_id)
    )
}

/// `POST /platform/search` (manifest `platformEndpoints.search`): the same
/// rule as `GET /v1/dm/search`, at most `limit` (1-10) items, read-only.
pub async fn platform_search(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    crate::error::ApiJson(request): crate::error::ApiJson<PlatformSearchRequest>,
) -> ApiResult<Json<Value>> {
    if !identity.context.platform {
        return Err(ApiError::forbidden(
            "platform_call_required",
            "Only the platform calls this endpoint.",
        ));
    }
    let text = normalize_query(&request.query)?;
    let limit = request.limit.unwrap_or(10).clamp(1, 10);
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    // The platform waits 3 seconds; give up before that.
    sqlx::query("SET LOCAL statement_timeout = 2500")
        .execute(&mut *tx)
        .await?;
    let hits = find(&mut tx, me, &text, None, None, limit).await?;
    tx.commit().await?;
    let (conversations, names) = conversation_names(&pool, &identity, &hits).await?;
    let english = request.locale.as_deref() == Some("en-US");
    let items: Vec<Value> = hits
        .iter()
        .filter_map(|hit| {
            let conversation = conversations.get(&hit.conversation_id)?;
            let snippet: String = format!(
                "{}: {}",
                names.name(&hit.sender_id),
                snippet(&hit.body, &text)
            )
            .chars()
            .take(PLATFORM_SNIPPET_MAX)
            .collect();
            Some(json!({
                "title": views::conversation_display_name(conversation, me, &names)
                    .chars()
                    .take(120)
                    .collect::<String>(),
                "snippet": snippet,
                "path": message_path(&hit.conversation_id, &hit.id),
                "at": format!("{}Z", iso(hit.created_at)),
                "kind": if english { "Message" } else { "메시지" },
            }))
        })
        .collect();
    Ok(Json(json!({ "items": items })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_patterns_escape_wildcards() {
        assert_eq!(like_pattern("50%_off\\x"), "%50\\%\\_off\\\\x%");
        assert_eq!(like_pattern("회의"), "%회의%");
    }

    #[test]
    fn snippets_cut_around_the_first_match() {
        let body = format!("{}회의 자료 공유{}", "가".repeat(60), "나".repeat(60));
        let text = snippet(&body, "회의");
        assert!(text.starts_with('…') && text.ends_with('…'));
        assert!(text.contains("회의 자료 공유"));
        assert_eq!(text.chars().count(), 40 + 2 + 40 + 2);
        assert_eq!(snippet("Hello\nWorld", "world"), "Hello World");
        assert_eq!(snippet("short", "zzz"), "short");
    }

    #[test]
    fn cursors_and_queries_are_validated() {
        assert!(parse_cursor("2026-11-22T01:00:00.000001|abc-1").is_ok());
        assert!(parse_cursor("nope").is_err());
        assert!(parse_cursor("2026-11-22T01:00:00|a/b").is_err());
        assert!(normalize_query("  ").is_err());
        assert!(normalize_query(&"가".repeat(201)).is_err());
        assert_eq!(normalize_query("  회의 ").unwrap(), "회의");
        assert_eq!(message_path("c1", "m1"), "/conversations/c1?message=m1");
    }
}
