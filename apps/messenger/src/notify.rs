//! Platform notifications (COMMON-FEATURES-PLAN "M2 플랫폼–앱 연결과 알림").
//!
//! AI-DO pushed realtime events and mobile pushes; messenger publishes to the
//! platform inbox through the Rust SDK while it serves the request:
//!
//! - send: `notify` to the other active participants, one line per
//!   conversation (`group_key` = conversation ID), title "보낸 사람 · 새 메시지
//!   N개" without the body. Muted participants and people who blocked the
//!   sender are left out, as AI-DO's mobile push did.
//! - edit, delete, moderation hide: people who had not read the message get
//!   the line replaced with the new count, or retracted when nothing unread
//!   is left.
//! - read: `mark_notifications_read(conversation)` for the reader.
//! - leave, removal, mute: `retract_notifications(conversation, [person])`.
//! - report: managers get a `report` line; resolving retracts it.
//!
//! Publishing never undoes the business write: failures are logged and the
//! request continues (no automatic resend).
use crate::{
    error::{ApiError, ApiResult},
    safety,
    store::{Conversation, MessageRow},
    users,
};
use axum::{Extension, Json, body::Bytes, extract::State};
use chrono::NaiveDateTime;
use miy_app_sdk::{Identity, Notification};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{collections::BTreeMap, future::Future, time::Duration};

pub const KIND_MESSAGE: &str = "message";
pub const KIND_REPORT: &str = "report";
const TITLE_MAX: usize = 200;
const CALL_TIMEOUT: Duration = Duration::from_secs(5);
const CHECK_ITEMS_MAX: usize = 500;
const HOME_ITEMS: i64 = 5;

pub fn conversation_path(conversation_id: &str) -> String {
    format!("/conversations/{conversation_id}")
}

pub fn report_group_key(report_id: &str) -> String {
    format!("report:{report_id}")
}

/// "보낸 사람 · 새 메시지 N개", at most 200 characters (the sender name is
/// shortened). Korean, AI-DO's default language: the platform keeps one title.
pub fn message_title(sender: &str, count: i64) -> String {
    bounded_message_title(sender, &format!(" · 새 메시지 {count}개"), TITLE_MAX)
}

fn bounded_message_title(sender: &str, suffix: &str, limit: usize) -> String {
    let room = limit.saturating_sub(suffix.chars().count());
    let sender: String = if sender.chars().count() > room {
        let mut short: String = sender.chars().take(room.saturating_sub(1)).collect();
        short.push('…');
        short
    } else {
        sender.to_string()
    };
    format!("{sender}{suffix}")
}

fn home_subtitle(sender: &str, count: i64, english: bool) -> String {
    let suffix = if english {
        let noun = if count == 1 { "message" } else { "messages" };
        format!(" · {count} new {noun}")
    } else {
        format!(" · 새 메시지 {count}개")
    };
    bounded_message_title(sender, &suffix, 160)
}

async fn bounded<T>(
    label: &str,
    call: impl Future<Output = Result<T, miy_app_sdk::CallError>>,
) -> Option<T> {
    match tokio::time::timeout(CALL_TIMEOUT, call).await {
        Ok(Ok(value)) => Some(value),
        Ok(Err(error)) => {
            eprintln!(
                "notification_{label}_failed: {} {:?}",
                error.code, error.status
            );
            None
        }
        Err(_) => {
            eprintln!("notification_{label}_timeout");
            None
        }
    }
}

async fn publish(identity: &Identity, notification: Notification) {
    if notification.recipients.is_empty() {
        return;
    }
    bounded("publish", identity.notify(&notification)).await;
}

/// Unread state per participant: (count, newest unread message ID and sender).
/// Only messages eligible to appear in the platform inbox.
async fn unread_state(
    pool: &PgPool,
    conversation_id: &str,
    user_ids: &[String],
) -> ApiResult<Vec<(String, i64, Option<String>, Option<String>)>> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_as(
        "SELECT p.user_id, count(m.id), (array_agg(m.id ORDER BY m.sequence DESC))[1], \
           (array_agg(m.sender_id ORDER BY m.sequence DESC))[1] \
         FROM dm_conversation_participants p \
         LEFT JOIN dm_messages lr ON lr.id = p.last_read_message_id \
         LEFT JOIN dm_messages m ON m.conversation_id = p.conversation_id AND m.created_at >= p.joined_at \
           AND m.sender_id <> p.user_id AND m.sequence > coalesce(lr.sequence, 0) \
           AND m.deleted_at IS NULL AND m.moderation_hidden_at IS NULL \
           AND NOT EXISTS (SELECT 1 FROM dm_user_blocks b WHERE b.blocker_user_id = p.user_id \
                           AND b.blocked_user_id = m.sender_id) \
         WHERE p.conversation_id = $1 AND p.left_at IS NULL AND p.muted_at IS NULL \
           AND p.user_id = ANY($2) \
         GROUP BY p.user_id",
    )
    .bind(conversation_id)
    .bind(user_ids)
    .fetch_all(pool)
    .await?)
}

/// Publish (or retract) the conversation line for `recipients` from their
/// current unread state.
async fn publish_counts(
    pool: &PgPool,
    identity: &Identity,
    conversation_id: &str,
    recipients: &[String],
    retract_when_read: bool,
) -> ApiResult<()> {
    let state = unread_state(pool, conversation_id, recipients).await?;
    let senders: Vec<String> = state.iter().filter_map(|row| row.3.clone()).collect();
    let names = users::names(pool, identity, senders).await;
    let mut groups: BTreeMap<(i64, String, String), Vec<String>> = BTreeMap::new();
    let mut nothing_left = Vec::new();
    for (user_id, count, latest, sender) in state {
        match (count, latest, sender) {
            (count, Some(latest), Some(sender)) if count > 0 => {
                groups
                    .entry((count, sender, latest))
                    .or_default()
                    .push(user_id);
            }
            _ => nothing_left.push(user_id),
        }
    }
    for ((count, sender, latest), recipients) in groups {
        publish(
            identity,
            Notification {
                request_id: uuid::Uuid::new_v4().to_string(),
                recipients,
                kind: KIND_MESSAGE.to_string(),
                group_key: conversation_id.to_string(),
                title: message_title(&names.name(&sender), count),
                path: conversation_path(conversation_id),
                resource_id: Some(latest),
            },
        )
        .await;
    }
    if retract_when_read && !nothing_left.is_empty() {
        retract(identity, conversation_id, Some(&nothing_left)).await;
    }
    Ok(())
}

/// Recipients of a new message.
pub async fn message_created(
    pool: &PgPool,
    identity: &Identity,
    conversation: &Conversation,
    message: &MessageRow,
) {
    if message.hidden() {
        return;
    }
    let candidates: Vec<String> = conversation
        .active()
        .filter(|p| p.user_id != message.sender_id && p.muted_at.is_none())
        .map(|p| p.user_id.clone())
        .collect();
    let result = async {
        let mut conn = pool.acquire().await?;
        let blocking =
            safety::recipients_blocking(&mut conn, &candidates, &message.sender_id).await?;
        drop(conn);
        let recipients: Vec<String> = candidates
            .into_iter()
            .filter(|id| !blocking.contains(id))
            .collect();
        publish_counts(pool, identity, conversation.id(), &recipients, false).await
    }
    .await;
    if let Err(error) = result {
        eprintln!("notification_prepare_failed: {}", error.code);
    }
}

/// After an edit, delete or moderation change of `message`: people who had
/// not read it get their line replaced or retracted.
pub async fn message_changed(
    pool: &PgPool,
    identity: &Identity,
    conversation: &Conversation,
    message: &MessageRow,
) {
    let candidates: Vec<String> = conversation
        .active()
        .filter(|p| {
            p.user_id != message.sender_id
                && p.muted_at.is_none()
                && p.joined_at <= message.created_at
                && p.last_read_sequence
                    .is_none_or(|seq| seq < message.sequence)
        })
        .map(|p| p.user_id.clone())
        .collect();
    let result = async {
        let mut conn = pool.acquire().await?;
        let blocking =
            safety::recipients_blocking(&mut conn, &candidates, &message.sender_id).await?;
        drop(conn);
        let (blocked, open): (Vec<String>, Vec<String>) =
            candidates.into_iter().partition(|id| blocking.contains(id));
        publish_counts(pool, identity, conversation.id(), &open, true).await?;
        if !blocked.is_empty() {
            retract(identity, conversation.id(), Some(&blocked)).await;
        }
        Ok::<(), ApiError>(())
    }
    .await;
    if let Err(error) = result {
        eprintln!("notification_prepare_failed: {}", error.code);
    }
}

pub async fn retract(identity: &Identity, group_key: &str, recipients: Option<&[String]>) {
    bounded(
        "retract",
        identity.retract_notifications(group_key, recipients),
    )
    .await;
}

pub async fn mark_read(identity: &Identity, group_key: &str) {
    bounded("read", identity.mark_notifications_read(group_key)).await;
}

/// `safety.notify_reviewers`: managers other than the two parties, category
/// only (no body, note or name).
pub async fn report_created(
    pool: &PgPool,
    identity: &Identity,
    report_id: &str,
    report: &safety::CreatedReport,
) {
    let managers: Result<Vec<String>, sqlx::Error> =
        sqlx::query_scalar("SELECT user_id FROM dm_safety_managers ORDER BY user_id")
            .fetch_all(pool)
            .await;
    let Ok(managers) = managers else {
        eprintln!("notification_prepare_failed: managers");
        return;
    };
    let recipients: Vec<String> = managers
        .into_iter()
        .filter(|id| *id != report.reporter_user_id && *id != report.reported_user_id)
        .collect();
    publish(
        identity,
        Notification {
            request_id: uuid::Uuid::new_v4().to_string(),
            recipients,
            kind: KIND_REPORT.to_string(),
            group_key: report_group_key(report_id),
            title: format!(
                "{} 신고가 접수됐습니다.",
                safety::category_label(&report.category)
            ),
            path: format!("/safety/reports/{report_id}"),
            resource_id: Some(report_id.to_string()),
        },
    )
    .await;
}

#[derive(Debug, Deserialize)]
pub struct CheckItem {
    #[serde(rename = "groupKey")]
    pub group_key: String,
    #[serde(rename = "resourceId", default)]
    pub resource_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CheckRequest {
    pub items: Vec<CheckItem>,
}

/// `POST /platform/notification-check` (manifest `platformEndpoints`): the
/// groups that still point at something this person may open. Read-only.
pub async fn notification_check(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    crate::error::ApiJson(request): crate::error::ApiJson<CheckRequest>,
) -> ApiResult<Json<Value>> {
    if !identity.context.platform {
        return Err(ApiError::forbidden(
            "platform_call_required",
            "Only the platform calls this endpoint.",
        ));
    }
    if request.items.len() > CHECK_ITEMS_MAX {
        return Err(ApiError::validation("items: at most 500"));
    }
    let me = identity.user_id.as_str();
    let (reports, messages): (Vec<(&str, &str)>, Vec<(&str, &str)>) = request
        .items
        .iter()
        .filter_map(|item| Some((item.group_key.as_str(), item.resource_id.as_deref()?)))
        .partition(|(group_key, _)| group_key.starts_with("report:"));
    let (conversation_ids, message_ids): (Vec<_>, Vec<_>) = messages.into_iter().unzip();
    let mut allowed: Vec<String> = if conversation_ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_scalar(
            "SELECT DISTINCT p.conversation_id FROM unnest($2::text[], $3::text[]) requested(conversation_id, message_id) \
             JOIN dm_messages m ON m.id = requested.message_id AND m.conversation_id = requested.conversation_id \
             JOIN dm_conversation_participants p ON p.conversation_id = m.conversation_id \
             WHERE p.user_id = $1 AND p.left_at IS NULL AND p.muted_at IS NULL AND m.created_at >= p.joined_at \
               AND m.deleted_at IS NULL AND m.moderation_hidden_at IS NULL \
               AND NOT EXISTS (SELECT 1 FROM dm_user_blocks b WHERE b.blocker_user_id = p.user_id \
                               AND b.blocked_user_id = m.sender_id)",
        )
        .bind(me)
        .bind(&conversation_ids)
        .bind(&message_ids)
        .fetch_all(&pool)
        .await?
    };
    if !reports.is_empty() {
        let manager: Option<String> =
            sqlx::query_scalar("SELECT user_id FROM dm_safety_managers WHERE user_id = $1")
                .bind(me)
                .fetch_optional(&pool)
                .await?;
        if manager.is_some() {
            let ids: Vec<&str> = reports
                .iter()
                .filter_map(|(group, resource)| {
                    (group.strip_prefix("report:") == Some(*resource)).then_some(*resource)
                })
                .collect();
            let open: Vec<String> = sqlx::query_scalar(
                "SELECT id FROM dm_abuse_reports WHERE id = ANY($1) AND reporter_user_id <> $2 \
                 AND reported_user_id <> $2",
            )
            .bind(&ids)
            .bind(me)
            .fetch_all(&pool)
            .await?;
            allowed.extend(open.iter().map(|id| report_group_key(id)));
        }
    }
    Ok(Json(json!({ "allowed": allowed })))
}

#[derive(Default, Deserialize)]
struct HomeCardRequest {
    locale: Option<String>,
}

#[derive(sqlx::FromRow)]
struct HomeUnread {
    conversation_id: String,
    unread: i64,
    created_at: NaiveDateTime,
    sender_id: String,
    total: i64,
}

/// `POST /platform/home-card` (manifest `platformEndpoints.homeCard`): the
/// person's unread conversations for the portal home, newest first, at most
/// five, without message bodies. Read-only; names come from the app's cache
/// (this call carries no token for further platform calls).
pub async fn home_card(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    body: Bytes,
) -> ApiResult<Json<Value>> {
    if !identity.context.platform {
        return Err(ApiError::forbidden(
            "platform_call_required",
            "Only the platform calls this endpoint.",
        ));
    }
    let request: HomeCardRequest = serde_json::from_slice(&body).unwrap_or_default();
    let english = request.locale.as_deref() == Some("en-US");
    let me = identity.user_id.as_str();
    // Conversation metadata and membership must agree with the unread check.
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let rows: Vec<HomeUnread> = sqlx::query_as(
        "SELECT conversation_id, unread, created_at, sender_id, count(*) OVER () AS total FROM (\
            SELECT DISTINCT ON (p.conversation_id) p.conversation_id, \
                count(*) OVER (PARTITION BY p.conversation_id) AS unread, m.created_at, m.sender_id \
            FROM dm_conversation_participants p \
            LEFT JOIN dm_messages lr ON lr.id = p.last_read_message_id \
            JOIN dm_messages m ON m.conversation_id = p.conversation_id AND m.created_at >= p.joined_at \
                AND m.sender_id <> p.user_id AND m.sequence > coalesce(lr.sequence, 0) AND m.deleted_at IS NULL \
            WHERE p.user_id = $1 AND p.left_at IS NULL \
            ORDER BY p.conversation_id, m.sequence DESC\
         ) latest ORDER BY created_at DESC, conversation_id LIMIT $2",
    )
    .bind(me)
    .bind(HOME_ITEMS)
    .fetch_all(&mut *tx)
    .await?;
    let count = rows.first().map_or(0, |row| row.total);
    let mut conversations = Vec::with_capacity(rows.len());
    for row in &rows {
        conversations
            .push(crate::store::load_conversation(&mut tx, &row.conversation_id, false).await?);
    }
    tx.commit().await?;
    let mut ids: Vec<String> = rows.iter().map(|row| row.sender_id.clone()).collect();
    for conversation in conversations.iter().flatten() {
        ids.extend(conversation.active().map(|p| p.user_id.clone()));
    }
    let names = users::names(&pool, &identity, ids).await;
    let items: Vec<Value> = rows
        .iter()
        .zip(conversations.iter())
        .filter_map(|(row, conversation)| {
            let conversation = conversation.as_ref()?;
            let title: String = crate::views::conversation_display_name(conversation, me, &names)
                .chars()
                .take(120)
                .collect();
            Some(json!({
                "title": title,
                "subtitle": home_subtitle(&names.name(&row.sender_id), row.unread, english),
                "path": conversation_path(&row.conversation_id),
                "at": format!("{}Z", crate::time::iso(row.created_at)),
            }))
        })
        .collect();
    let title = if english {
        "Unread conversations"
    } else {
        "안 읽은 대화"
    };
    Ok(Json(
        json!({"title": title, "count": count, "items": items}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_carry_no_body_and_fit_the_platform_limit() {
        assert_eq!(message_title("김하나", 3), "김하나 · 새 메시지 3개");
        let long = message_title(&"가".repeat(300), 12);
        assert_eq!(long.chars().count(), 200);
        assert!(long.ends_with("… · 새 메시지 12개"));
    }

    #[test]
    fn home_subtitles_are_localized_and_keep_the_unread_count() {
        assert_eq!(home_subtitle("김하나", 3, false), "김하나 · 새 메시지 3개");
        assert_eq!(home_subtitle("Alice", 1, true), "Alice · 1 new message");
        let long = home_subtitle(&"가".repeat(300), 12, true);
        assert_eq!(long.chars().count(), 160);
        assert!(long.ends_with("… · 12 new messages"));
    }

    #[test]
    fn paths_and_group_keys() {
        assert_eq!(conversation_path("c1"), "/conversations/c1");
        assert_eq!(report_group_key("r1"), "report:r1");
    }
}
