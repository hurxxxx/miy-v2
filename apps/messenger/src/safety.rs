//! AI-DO domains/dm/safety.py and safety_application.py: 1:1 blocks, reports
//! and the moderation-hidden state. Same rules; the commute-app relay source
//! is not ported.
use crate::{
    audit,
    error::{ApiError, ApiResult},
    store::{Conversation, new_id},
    time::now_utc,
    users,
};
use axum::http::StatusCode;
use chrono::{Duration, NaiveDateTime};
use miy_app_sdk::Identity;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool};
use std::collections::HashSet;

pub const SAFETY_VERSION: i32 = 1;
pub const REPORT_CATEGORIES: [&str; 7] = [
    "harassment",
    "hate",
    "sexual",
    "violence",
    "spam",
    "privacy",
    "other",
];
pub const REPORT_NOTE_MAX_LENGTH: usize = 1000;
pub const RESOLUTION_NOTE_MAX_LENGTH: usize = 1000;
pub const BLOCK_LIST_MAX: i64 = 500;
pub const SOURCE_WEB: &str = "web";
const REPORT_REQUEST_RETENTION_DAYS: i64 = 7;

pub fn category_label(category: &str) -> &str {
    match category {
        "harassment" => "괴롭힘",
        "hate" => "혐오",
        "sexual" => "성적인 내용",
        "violence" => "폭력·위협",
        "spam" => "스팸",
        "privacy" => "개인정보 노출",
        "other" => "기타",
        other => other,
    }
}

/// `(count, window)`: 10 per hour, 30 per day. Stored reports only.
fn report_limits() -> [(usize, Duration); 2] {
    [(10, Duration::hours(1)), (30, Duration::days(1))]
}

fn blocked_error() -> ApiError {
    ApiError::forbidden(
        "dm.conversation_blocked",
        "You cannot send messages in this conversation.",
    )
}

/// `safety.blocked_between`: a block in either direction.
pub async fn blocked_between(conn: &mut PgConnection, left: &str, right: &str) -> ApiResult<bool> {
    if left.is_empty() || right.is_empty() || left == right {
        return Ok(false);
    }
    let found: Option<String> = sqlx::query_scalar(
        "SELECT blocker_user_id FROM dm_user_blocks WHERE (blocker_user_id = $1 AND blocked_user_id = $2) \
         OR (blocker_user_id = $2 AND blocked_user_id = $1) LIMIT 1",
    )
    .bind(left)
    .bind(right)
    .fetch_optional(conn)
    .await?;
    Ok(found.is_some())
}

pub async fn ensure_not_blocked(conn: &mut PgConnection, left: &str, right: &str) -> ApiResult<()> {
    if blocked_between(conn, left, right).await? {
        return Err(blocked_error());
    }
    Ok(())
}

/// `safety.ensure_direct_conversation_open`.
pub async fn ensure_direct_conversation_open(
    conn: &mut PgConnection,
    conversation: &Conversation,
    user_id: &str,
) -> ApiResult<()> {
    if let Some(peer) = conversation.direct_peer_id(user_id) {
        ensure_not_blocked(conn, user_id, &peer).await?;
    }
    Ok(())
}

/// `safety.blocked_counterparts`.
pub async fn blocked_counterparts(
    conn: &mut PgConnection,
    user_id: &str,
    others: &[String],
) -> ApiResult<HashSet<String>> {
    let others: Vec<&String> = others
        .iter()
        .filter(|o| !o.is_empty() && *o != user_id)
        .collect();
    if others.is_empty() {
        return Ok(HashSet::new());
    }
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT blocker_user_id, blocked_user_id FROM dm_user_blocks \
         WHERE (blocker_user_id = $1 AND blocked_user_id = ANY($2)) \
            OR (blocked_user_id = $1 AND blocker_user_id = ANY($2))",
    )
    .bind(user_id)
    .bind(&others)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(blocker, blocked)| if blocker == user_id { blocked } else { blocker })
        .collect())
}

/// `safety.ensure_no_blocked_invitees`: only actor ↔ invitee pairs.
pub async fn ensure_no_blocked_invitees(
    conn: &mut PgConnection,
    actor: &str,
    invitees: &[String],
) -> ApiResult<()> {
    if !blocked_counterparts(conn, actor, invitees)
        .await?
        .is_empty()
    {
        return Err(blocked_error());
    }
    Ok(())
}

/// `safety.users_blocked_by`: people `blocker` blocked among `ids`.
pub async fn users_blocked_by(
    conn: &mut PgConnection,
    blocker: &str,
    ids: &[String],
) -> ApiResult<HashSet<String>> {
    if ids.is_empty() {
        return Ok(HashSet::new());
    }
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT blocked_user_id FROM dm_user_blocks WHERE blocker_user_id = $1 AND blocked_user_id = ANY($2)",
    )
    .bind(blocker)
    .bind(ids)
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().collect())
}

/// `safety.blocked_user_ids`.
pub async fn blocked_user_ids(
    conn: &mut PgConnection,
    blocker: &str,
) -> ApiResult<HashSet<String>> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT blocked_user_id FROM dm_user_blocks WHERE blocker_user_id = $1 LIMIT $2",
    )
    .bind(blocker)
    .bind(BLOCK_LIST_MAX + 1)
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Recipients among `recipients` who blocked `sender` (no personal
/// notification, as AI-DO's mobile push `blocking_pairs`).
pub async fn recipients_blocking(
    conn: &mut PgConnection,
    recipients: &[String],
    sender: &str,
) -> ApiResult<HashSet<String>> {
    if recipients.is_empty() {
        return Ok(HashSet::new());
    }
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT blocker_user_id FROM dm_user_blocks WHERE blocked_user_id = $1 AND blocker_user_id = ANY($2)",
    )
    .bind(sender)
    .bind(recipients)
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().collect())
}

async fn lock_key(conn: &mut PgConnection, key: &str) -> ApiResult<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(key)
        .execute(conn)
        .await?;
    Ok(())
}

pub struct BlockChange {
    pub user_id: String,
    pub blocked: bool,
    pub changed: bool,
}

/// `safety.set_block`. A new block needs a person the directory lists now
/// (AI-DO: an existing user row); removing one needs only a valid ID.
pub async fn set_block(
    conn: &mut PgConnection,
    pool: &PgPool,
    identity: &Identity,
    target: &str,
    blocked: bool,
) -> ApiResult<BlockChange> {
    let actor = identity.user_id.as_str();
    if target.is_empty() || target == actor {
        return Err(ApiError::unprocessable(
            "dm_safety.block_target_invalid",
            "This person cannot be blocked.",
        ));
    }
    let changed = if !blocked {
        let result = sqlx::query(
            "DELETE FROM dm_user_blocks WHERE blocker_user_id = $1 AND blocked_user_id = $2",
        )
        .bind(actor)
        .bind(target)
        .execute(&mut *conn)
        .await?;
        result.rows_affected() > 0
    } else {
        let active = users::active_users(pool, identity, &[target.to_string()]).await?;
        if !active.contains_key(target) {
            return Err(ApiError::not_found(
                "dm_safety.user_not_found",
                "User not found.",
            ));
        }
        insert_block(conn, actor, target).await?
    };
    if changed {
        audit::record(
            conn,
            if blocked {
                "dm_safety.block_set"
            } else {
                "dm_safety.block_removed"
            },
            "user",
            Some(target),
            if blocked {
                "DM 1:1 차단을 켰다"
            } else {
                "DM 1:1 차단을 풀었다"
            },
            actor,
            json!({"source": SOURCE_WEB}),
        )
        .await?;
    }
    Ok(BlockChange {
        user_id: target.to_string(),
        blocked,
        changed,
    })
}

async fn insert_block(conn: &mut PgConnection, actor: &str, target: &str) -> ApiResult<bool> {
    lock_key(conn, &format!("dm_block:{actor}")).await?;
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT blocker_user_id FROM dm_user_blocks WHERE blocker_user_id = $1 AND blocked_user_id = $2",
    )
    .bind(actor)
    .bind(target)
    .fetch_optional(&mut *conn)
    .await?;
    if exists.is_some() {
        return Ok(false);
    }
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM dm_user_blocks WHERE blocker_user_id = $1")
            .bind(actor)
            .fetch_one(&mut *conn)
            .await?;
    if count >= BLOCK_LIST_MAX {
        return Err(ApiError::unprocessable(
            "dm_safety.block_limit_reached",
            "You cannot block more people.",
        ));
    }
    let inserted: Option<String> = sqlx::query_scalar(
        "INSERT INTO dm_user_blocks (blocker_user_id, blocked_user_id, source, created_at) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (blocker_user_id, blocked_user_id) DO NOTHING RETURNING blocker_user_id",
    )
    .bind(actor)
    .bind(target)
    .bind(SOURCE_WEB)
    .bind(now_utc())
    .fetch_optional(conn)
    .await?;
    Ok(inserted.is_some())
}

/// `safety.list_blocks`, newest first.
pub async fn list_blocks(
    conn: &mut PgConnection,
    actor: &str,
) -> ApiResult<Vec<(String, NaiveDateTime)>> {
    Ok(sqlx::query_as(
        "SELECT blocked_user_id, created_at FROM dm_user_blocks WHERE blocker_user_id = $1 \
         ORDER BY created_at DESC, blocked_user_id ASC LIMIT $2",
    )
    .bind(actor)
    .bind(BLOCK_LIST_MAX)
    .fetch_all(conn)
    .await?)
}

// --- reports ------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ReportRequest {
    pub client_report_id: String,
    pub conversation_id: String,
    pub category: String,
    pub message_id: Option<String>,
    pub reported_user_id: Option<String>,
    pub note: Option<String>,
    pub block: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportOutcome {
    pub report_id: String,
    pub duplicate: bool,
    pub blocked: bool,
    /// A new report was stored (reviewers are notified after commit).
    pub created: Option<CreatedReport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedReport {
    pub category: String,
    pub reporter_user_id: String,
    pub reported_user_id: String,
}

struct ReportTarget {
    message_id: Option<String>,
    reported_user_id: String,
    dedupe_key: String,
}

/// `safety.report_request_hash`: Python's `json.dumps(sort_keys=True,
/// ensure_ascii=False, separators=(",", ":"))` of the request fields.
pub fn report_request_hash(request: &ReportRequest) -> String {
    let material = json!({
        "block": request.block,
        "category": request.category,
        "conversation_id": request.conversation_id,
        "message_id": request.message_id,
        "note": request.note,
        "reported_user_id": request.reported_user_id,
    });
    // serde_json keeps object keys sorted (BTreeMap) and writes compact UTF-8.
    hex::encode(Sha256::digest(material.to_string().as_bytes()))
}

pub fn dedupe_key(
    conversation_id: &str,
    message_id: Option<&str>,
    reported_user_id: &str,
) -> String {
    match message_id {
        Some(message_id) if !message_id.is_empty() => format!("{conversation_id}:{message_id}"),
        _ => format!("{conversation_id}:user:{reported_user_id}"),
    }
}

fn invalid_target() -> ApiError {
    ApiError::unprocessable(
        "dm_safety.report_target_invalid",
        "This report target is not valid.",
    )
}

/// `safety.report_retry_after`: seconds until the window drops below the limit.
pub fn report_retry_after(
    created_in_window: &[NaiveDateTime],
    limit: usize,
    window: Duration,
    now: NaiveDateTime,
) -> Option<u64> {
    if created_in_window.len() < limit {
        return None;
    }
    let expiring = created_in_window[created_in_window.len() - limit];
    let wait = (expiring + window - now).num_microseconds().unwrap_or(0) as f64 / 1_000_000.0;
    Some(wait.ceil().max(1.0) as u64)
}

async fn enforce_report_rate(
    conn: &mut PgConnection,
    reporter: &str,
    now: NaiveDateTime,
) -> ApiResult<()> {
    let limits = report_limits();
    let longest = limits.iter().map(|(_, window)| *window).max().unwrap();
    let created: Vec<NaiveDateTime> = sqlx::query_scalar(
        "SELECT created_at FROM dm_abuse_reports WHERE reporter_user_id = $1 AND created_at > $2 \
         ORDER BY created_at ASC",
    )
    .bind(reporter)
    .bind(now - longest)
    .fetch_all(conn)
    .await?;
    let retry_after = limits
        .iter()
        .filter_map(|(limit, window)| {
            let in_window: Vec<NaiveDateTime> = created
                .iter()
                .copied()
                .filter(|value| *value > now - *window)
                .collect();
            report_retry_after(&in_window, *limit, *window, now)
        })
        .max();
    if let Some(seconds) = retry_after {
        let mut error = ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "dm_safety.report_rate_limited",
            "Too many reports. Try again later.",
        );
        error.retry_after = Some(seconds);
        return Err(error);
    }
    Ok(())
}

async fn resolve_target(
    conn: &mut PgConnection,
    reporter: &str,
    request: &ReportRequest,
) -> ApiResult<ReportTarget> {
    let joined_at: Option<NaiveDateTime> = sqlx::query_scalar(
        "SELECT joined_at FROM dm_conversation_participants WHERE conversation_id = $1 AND user_id = $2 \
         AND left_at IS NULL",
    )
    .bind(&request.conversation_id)
    .bind(reporter)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(joined_at) = joined_at else {
        return Err(ApiError::thread_not_found());
    };
    let reported_user_id = if let Some(message_id) = request.message_id.as_deref() {
        let message: Option<(String, NaiveDateTime, Option<NaiveDateTime>)> = sqlx::query_as(
            "SELECT sender_id, created_at, deleted_at FROM dm_messages WHERE id = $1 AND conversation_id = $2",
        )
        .bind(message_id)
        .bind(&request.conversation_id)
        .fetch_optional(&mut *conn)
        .await?;
        // miy: a message its author deleted is gone for new reports.
        let Some((sender_id, created_at, None)) = message else {
            return Err(ApiError::message_not_found());
        };
        if created_at < joined_at {
            return Err(ApiError::message_not_found());
        }
        if request
            .reported_user_id
            .as_deref()
            .is_some_and(|id| id != sender_id)
            || sender_id == reporter
        {
            return Err(invalid_target());
        }
        sender_id
    } else {
        let reported = request.reported_user_id.clone().unwrap_or_default();
        if reported.is_empty() || reported == reporter {
            return Err(invalid_target());
        }
        // AI-DO first checked that the user row exists; a past or present
        // participant always existed there.
        let member: Option<String> = sqlx::query_scalar(
            "SELECT id FROM dm_conversation_participants WHERE conversation_id = $1 AND user_id = $2 LIMIT 1",
        )
        .bind(&request.conversation_id)
        .bind(&reported)
        .fetch_optional(&mut *conn)
        .await?;
        if member.is_none() {
            return Err(invalid_target());
        }
        reported
    };
    Ok(ReportTarget {
        dedupe_key: dedupe_key(
            &request.conversation_id,
            request.message_id.as_deref(),
            &reported_user_id,
        ),
        message_id: request.message_id.clone(),
        reported_user_id,
    })
}

async fn block_exists(conn: &mut PgConnection, blocker: &str, blocked: &str) -> ApiResult<bool> {
    let found: Option<String> = sqlx::query_scalar(
        "SELECT blocker_user_id FROM dm_user_blocks WHERE blocker_user_id = $1 AND blocked_user_id = $2",
    )
    .bind(blocker)
    .bind(blocked)
    .fetch_optional(conn)
    .await?;
    Ok(found.is_some())
}

/// `safety._apply_report_block`: block with the report; a target that cannot
/// be blocked keeps the report and answers false. miy: a directory failure
/// is treated the same way.
async fn apply_report_block(
    conn: &mut PgConnection,
    pool: &PgPool,
    identity: &Identity,
    reported: &str,
) -> ApiResult<bool> {
    sqlx::query("SAVEPOINT report_block")
        .execute(&mut *conn)
        .await?;
    match set_block(conn, pool, identity, reported, true).await {
        Ok(_) => {
            sqlx::query("RELEASE SAVEPOINT report_block")
                .execute(&mut *conn)
                .await?;
        }
        Err(error)
            if error.status == StatusCode::NOT_FOUND
                || error.status == StatusCode::UNPROCESSABLE_ENTITY
                || error.status == StatusCode::BAD_GATEWAY =>
        {
            sqlx::query("ROLLBACK TO SAVEPOINT report_block")
                .execute(&mut *conn)
                .await?;
        }
        Err(error) => return Err(error),
    }
    block_exists(conn, &identity.user_id, reported).await
}

async fn replayed_report(
    conn: &mut PgConnection,
    reporter: &str,
    request: &ReportRequest,
    request_hash: &str,
) -> ApiResult<Option<ReportOutcome>> {
    let mut row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT id, reported_user_id, request_hash FROM dm_abuse_reports \
         WHERE reporter_user_id = $1 AND client_report_id = $2",
    )
    .bind(reporter)
    .bind(&request.client_report_id)
    .fetch_optional(&mut *conn)
    .await?;
    if row.is_none() {
        row = sqlx::query_as(
            "SELECT r.id, r.reported_user_id, q.request_hash FROM dm_abuse_report_requests q \
             JOIN dm_abuse_reports r ON r.id = q.report_id \
             WHERE q.reporter_user_id = $1 AND q.client_report_id = $2",
        )
        .bind(reporter)
        .bind(&request.client_report_id)
        .fetch_optional(&mut *conn)
        .await?;
    }
    let Some((report_id, reported_user_id, stored_hash)) = row else {
        return Ok(None);
    };
    if stored_hash != request_hash {
        return Err(ApiError::conflict(
            "dm_safety.report_conflict",
            "This report ID was already used for another report.",
        ));
    }
    let blocked = request.block && block_exists(conn, reporter, &reported_user_id).await?;
    Ok(Some(ReportOutcome {
        report_id,
        duplicate: true,
        blocked,
        created: None,
    }))
}

/// `safety.create_report` (in the caller's transaction).
pub async fn create_report(
    conn: &mut PgConnection,
    pool: &PgPool,
    identity: &Identity,
    request: &ReportRequest,
) -> ApiResult<ReportOutcome> {
    let reporter = identity.user_id.as_str();
    if !REPORT_CATEGORIES.contains(&request.category.as_str())
        || request
            .note
            .as_ref()
            .is_some_and(|note| note.chars().count() > REPORT_NOTE_MAX_LENGTH)
    {
        return Err(invalid_target());
    }
    let now = now_utc();
    lock_key(conn, &format!("dm_report:{reporter}")).await?;
    let request_hash = report_request_hash(request);
    if let Some(replayed) = replayed_report(conn, reporter, request, &request_hash).await? {
        return Ok(replayed);
    }
    let target = resolve_target(conn, reporter, request).await?;
    let open: Option<String> = sqlx::query_scalar(
        "SELECT id FROM dm_abuse_reports WHERE reporter_user_id = $1 AND open_dedupe_key = $2",
    )
    .bind(reporter)
    .bind(&target.dedupe_key)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(open_id) = open {
        let blocked = if request.block {
            apply_report_block(conn, pool, identity, &target.reported_user_id).await?
        } else {
            false
        };
        sqlx::query(
            "DELETE FROM dm_abuse_report_requests WHERE reporter_user_id = $1 AND created_at < $2",
        )
        .bind(reporter)
        .bind(now - Duration::days(REPORT_REQUEST_RETENTION_DAYS))
        .execute(&mut *conn)
        .await?;
        sqlx::query(
            "INSERT INTO dm_abuse_report_requests (reporter_user_id, client_report_id, report_id, request_hash, created_at) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(reporter)
        .bind(&request.client_report_id)
        .bind(&open_id)
        .bind(&request_hash)
        .bind(now)
        .execute(&mut *conn)
        .await?;
        return Ok(ReportOutcome {
            report_id: open_id,
            duplicate: true,
            blocked,
            created: None,
        });
    }
    enforce_report_rate(conn, reporter, now).await?;
    let report_id = new_id();
    sqlx::query(
        "INSERT INTO dm_abuse_reports (id, reporter_user_id, reported_user_id, conversation_id, message_id, category, \
           note, source, client_report_id, request_hash, open_dedupe_key, status, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 'open', $12, $12)",
    )
    .bind(&report_id)
    .bind(reporter)
    .bind(&target.reported_user_id)
    .bind(&request.conversation_id)
    .bind(&target.message_id)
    .bind(&request.category)
    .bind(request.note.as_deref().filter(|n| !n.is_empty()))
    .bind(SOURCE_WEB)
    .bind(&request.client_report_id)
    .bind(&request_hash)
    .bind(&target.dedupe_key)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    let blocked = if request.block {
        apply_report_block(conn, pool, identity, &target.reported_user_id).await?
    } else {
        false
    };
    audit::record(
        conn,
        "dm_safety.report_created",
        "dm_abuse_report",
        Some(&report_id),
        "DM 을 신고했다",
        reporter,
        json!({
            "source": SOURCE_WEB,
            "category": request.category,
            "conversation_id": request.conversation_id,
            "message_id": target.message_id,
            "reported_user_id": target.reported_user_id,
            "has_note": request.note.is_some(),
            "blocked": blocked,
        }),
    )
    .await?;
    Ok(ReportOutcome {
        report_id,
        duplicate: false,
        blocked,
        created: Some(CreatedReport {
            category: request.category.clone(),
            reporter_user_id: reporter.to_string(),
            reported_user_id: target.reported_user_id,
        }),
    })
}

/// Open reports on `message_id` (miy: delete keeps the body copy and the
/// attachments while one is open).
pub async fn message_has_open_report(conn: &mut PgConnection, message_id: &str) -> ApiResult<bool> {
    let found: Option<String> = sqlx::query_scalar(
        "SELECT id FROM dm_abuse_reports WHERE message_id = $1 AND status = 'open' LIMIT 1",
    )
    .bind(message_id)
    .fetch_optional(conn)
    .await?;
    Ok(found.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> NaiveDateTime {
        chrono::DateTime::from_timestamp(1_700_000_000 + seconds, 0)
            .unwrap()
            .naive_utc()
    }

    // test_dm_safety.py::test_report_retry_after*
    #[test]
    fn retry_after_waits_for_the_oldest_report_in_the_window() {
        let window = Duration::hours(1);
        let now = at(3600);
        let created: Vec<_> = (0..10).map(|i| at(600 + i * 60)).collect();
        assert_eq!(report_retry_after(&created, 10, window, now), Some(600));
        assert_eq!(report_retry_after(&created[..9], 10, window, now), None);
        let created: Vec<_> = (0..11).map(|i| at(i)).collect();
        // The window would already be clear: at least one second.
        assert_eq!(report_retry_after(&created, 10, window, at(7200)), Some(1));
    }

    #[test]
    fn dedupe_keys_distinguish_message_and_person_reports() {
        assert_eq!(dedupe_key("c", Some("m"), "u"), "c:m");
        assert_eq!(dedupe_key("c", None, "u"), "c:user:u");
        assert_eq!(dedupe_key("c", Some(""), "u"), "c:user:u");
    }

    // The hash matches Python's json.dumps(sort_keys=True, ensure_ascii=False,
    // separators=(",", ":")) of the same fields.
    #[test]
    fn request_hash_matches_python_serialization() {
        let request = ReportRequest {
            client_report_id: "x".into(),
            conversation_id: "c1".into(),
            category: "spam".into(),
            message_id: Some("m1".into()),
            reported_user_id: None,
            note: Some("메모".into()),
            block: true,
        };
        let expected = hex::encode(Sha256::digest(
            r#"{"block":true,"category":"spam","conversation_id":"c1","message_id":"m1","note":"메모","reported_user_id":null}"#
                .as_bytes(),
        ));
        assert_eq!(report_request_hash(&request), expected);
    }

    #[test]
    fn category_labels_are_ai_do_korean_labels() {
        assert_eq!(category_label("privacy"), "개인정보 노출");
        assert_eq!(category_label("unknown"), "unknown");
    }
}
