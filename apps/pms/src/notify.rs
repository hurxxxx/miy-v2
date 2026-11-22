//! Platform notifications (miy M2 contract), replacing AI-DO's in-app
//! pms_notifications rows and the commute-app push relay.
//!
//! AI-DO created a notification for: assignment (`assigned`, update only),
//! status change (`status_changed`, to assignees, followers and the reporter),
//! a comment (`commented`, same people) and a mention in a comment
//! (`mentioned`, people mentioned who are not already notified). The same
//! triggers publish here with the kinds `assigned`, `status`, `comment` and
//! `mention`, one line per person and task (`group_key` `task:<id>`), opening
//! `/tasks/<id>` in this app's frame.
//!
//! * Only people who can read the task now receive it. AI-DO notified others
//!   with a title-only line; the platform's recheck (`notificationCheck`) would
//!   hide such a line at once, so it is not sent.
//! * Titles are Korean (the platform default language; a notification has no
//!   per-recipient language). They follow AI-DO's push display table
//!   (`mobile_push_display.NOTIFICATION_TITLES`, `task_status_label_ko`) and
//!   carry no comment text.
//! * Publishing happens after the business commit and never undoes it. A failed
//!   publish is logged; there is no resend (no service principal).
//! * pms_notification_deliveries remembers who got which task's line, so a
//!   deleted task or a person removed from the space has it retracted.
use std::collections::HashSet;

use axum::{Extension, Json, extract::State, http::StatusCode};
use miy_app_sdk::{Identity, Notification};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::access;
use crate::error::{ApiError, ApiResult};
use crate::models::{new_id, utcnow};
use crate::workflow::ListWorkflow;

pub const KIND_ASSIGNED: &str = "assigned";
pub const KIND_STATUS: &str = "status";
pub const KIND_COMMENT: &str = "comment";
pub const KIND_MENTION: &str = "mention";

const SUBJECT_MAX: usize = 100;
const STATE_MAX: usize = 30;
const TITLE_MAX: usize = 200;
/// Retractions done inside one request; beyond it the platform recheck hides
/// the remaining lines.
const RETRACT_LIMIT: usize = 200;

pub fn group_key(task_id: &str) -> String {
    format!("task:{task_id}")
}

pub fn task_path(task_id: &str) -> String {
    format!("/tasks/{task_id}")
}

fn clip(value: &str, limit: usize) -> String {
    let value = value.trim();
    if value.chars().count() <= limit {
        return value.to_string();
    }
    let mut out: String = value.chars().take(limit.saturating_sub(1)).collect();
    out.push('…');
    out
}

const STATUS_LABELS_KO: &[(&str, &str)] = &[
    ("todo", "할 일"),
    ("in_progress", "진행 중"),
    ("review", "검토"),
    ("done", "완료"),
    ("complete", "완료 처리"),
    ("canceled", "취소됨"),
    ("planned", "예정"),
    ("on_hold", "보류"),
    ("discarded", "폐기"),
];
const ENGLISH_DEFAULT_STATUS_NAMES: &[&str] = &[
    "to do",
    "todo",
    "in progress",
    "review",
    "done",
    "complete",
    "completed",
    "canceled",
    "cancelled",
    "planned",
    "on hold",
    "discarded",
];

/// AI-DO `task_status_label_ko`.
pub fn status_label_ko(status: &str, workflow: Option<&ListWorkflow>) -> String {
    let name = workflow
        .and_then(|workflow| {
            workflow
                .definitions()
                .iter()
                .find(|definition| definition.slug == status)
                .map(|definition| definition.name.trim().to_string())
        })
        .unwrap_or_default();
    let korean = STATUS_LABELS_KO
        .iter()
        .find(|(slug, _)| *slug == status)
        .map(|(_, label)| *label);
    if !name.is_empty()
        && !(korean.is_some()
            && ENGLISH_DEFAULT_STATUS_NAMES.contains(&name.to_lowercase().as_str()))
    {
        return name;
    }
    korean.map(str::to_string).unwrap_or(if name.is_empty() {
        status.to_string()
    } else {
        name
    })
}

/// The display title: "<actor> · <kind title> · <task> [· <state>]".
pub fn title(actor: &str, kind: &str, task_label: &str, state: Option<&str>) -> String {
    let kind_title = match kind {
        KIND_ASSIGNED => "업무 배정",
        KIND_STATUS => "업무 상태 변경",
        KIND_COMMENT => "업무 댓글",
        KIND_MENTION => "업무 댓글에서 언급",
        _ => "업무 알림",
    };
    let mut parts = vec![
        clip(actor, 40),
        kind_title.to_string(),
        clip(task_label, SUBJECT_MAX),
    ];
    if let Some(state) = state.filter(|state| !state.trim().is_empty()) {
        parts.push(clip(state, STATE_MAX));
    }
    clip(&parts.join(" · "), TITLE_MAX)
}

/// AI-DO `_task_notification_label`: the title, or the reference when blank.
pub fn task_label(title: &str, reference: &str) -> String {
    let title = title.trim();
    if title.is_empty() {
        reference.to_string()
    } else {
        title.to_string()
    }
}

/// One notification to publish after the commit.
#[derive(Debug, Clone)]
pub struct Outgoing {
    pub task_id: String,
    pub kind: &'static str,
    pub recipients: Vec<String>,
    pub title: String,
}

/// Publish after the business commit. Recipients who may not read the task
/// now are left out; the rest get one line per task.
pub async fn publish(pool: &PgPool, identity: &Identity, outgoing: Vec<Outgoing>) {
    for item in outgoing {
        if let Err(error) = publish_one(pool, identity, &item).await {
            eprintln!("notification_publish_failed: {} {}", item.kind, error.code);
        }
    }
}

async fn publish_one(pool: &PgPool, identity: &Identity, item: &Outgoing) -> ApiResult<()> {
    let mut recipients: Vec<String> = item
        .recipients
        .iter()
        .filter(|id| **id != identity.user_id)
        .cloned()
        .collect();
    recipients.sort();
    recipients.dedup();
    if recipients.is_empty() {
        return Ok(());
    }
    let mut conn = pool.acquire().await?;
    let Ok(task) = access::load_task(&mut conn, &item.task_id).await else {
        return Ok(());
    };
    let readers = access::readers_among(&mut conn, &task, &recipients).await?;
    drop(conn);
    if readers.is_empty() {
        return Ok(());
    }
    let request_id = uuid::Uuid::new_v4();
    let key = group_key(&item.task_id);
    let now = utcnow();
    for reader in &readers {
        sqlx::query(
            "INSERT INTO pms_notification_deliveries (id, request_id, task_id, user_id, kind, group_key, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(new_id())
        .bind(request_id)
        .bind(&item.task_id)
        .bind(reader)
        .bind(item.kind)
        .bind(&key)
        .bind(now)
        .execute(pool)
        .await?;
    }
    let notification = Notification {
        request_id: request_id.to_string(),
        recipients: readers,
        kind: item.kind.to_string(),
        group_key: key,
        title: item.title.clone(),
        path: task_path(&item.task_id),
        resource_id: Some(item.task_id.clone()),
    };
    if let Err(error) = identity.notify(&notification).await {
        eprintln!(
            "notification_publish_failed: {} {:?}",
            error.code, error.status
        );
    }
    Ok(())
}

/// Retract every line of the deleted tasks (after the delete committed).
pub async fn retract_tasks(pool: &PgPool, identity: &Identity, task_ids: &[String]) {
    if task_ids.is_empty() {
        return;
    }
    let keys: Vec<String> = match sqlx::query_scalar(
        "DELETE FROM pms_notification_deliveries WHERE task_id = ANY($1) RETURNING group_key",
    )
    .bind(task_ids)
    .fetch_all(pool)
    .await
    {
        Ok(keys) => keys,
        Err(_) => return,
    };
    let unique: HashSet<String> = keys.into_iter().collect();
    for key in unique.into_iter().take(RETRACT_LIMIT) {
        if let Err(error) = identity.retract_notifications(&key, None).await {
            eprintln!(
                "notification_retract_failed: {} {:?}",
                error.code, error.status
            );
        }
    }
}

/// Retract the lines a person got for tasks of a space they no longer belong
/// to (after the membership change committed).
pub async fn retract_space_member(
    pool: &PgPool,
    identity: &Identity,
    space_id: &str,
    user_id: &str,
) {
    let keys: Vec<String> = match sqlx::query_scalar(
        "DELETE FROM pms_notification_deliveries d USING pms_tasks t, pms_task_lists l \
         WHERE d.task_id = t.id AND t.list_id = l.id AND l.team_id = $1 AND d.user_id = $2 \
         AND NOT EXISTS (SELECT 1 FROM pms_task_user_access a WHERE a.task_id = t.id AND a.user_id = $2 \
           AND a.revoked_at IS NULL) \
         RETURNING d.group_key",
    )
    .bind(space_id)
    .bind(user_id)
    .fetch_all(pool)
    .await
    {
        Ok(keys) => keys,
        Err(_) => return,
    };
    let unique: HashSet<String> = keys.into_iter().collect();
    let recipients = vec![user_id.to_string()];
    for key in unique.into_iter().take(RETRACT_LIMIT) {
        if let Err(error) = identity
            .retract_notifications(&key, Some(&recipients))
            .await
        {
            eprintln!(
                "notification_retract_failed: {} {:?}",
                error.code, error.status
            );
        }
    }
}

/// Retract the lines of every task in a deleted (archived) space.
pub async fn retract_space(pool: &PgPool, identity: &Identity, space_id: &str) {
    let task_ids: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT d.task_id FROM pms_notification_deliveries d JOIN pms_tasks t ON t.id = d.task_id \
         JOIN pms_task_lists l ON l.id = t.list_id WHERE l.team_id = $1",
    )
    .bind(space_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    retract_tasks(pool, identity, &task_ids).await;
}

/// Mark the caller's line for this task read when they open it in the app.
pub async fn mark_read(pool: &PgPool, identity: &Identity, task_id: &str) {
    let updated = sqlx::query(
        "UPDATE pms_notification_deliveries SET read_at = $3 \
         WHERE task_id = $1 AND user_id = $2 AND read_at IS NULL",
    )
    .bind(task_id)
    .bind(&identity.user_id)
    .bind(utcnow())
    .execute(pool)
    .await
    .map(|result| result.rows_affected())
    .unwrap_or(0);
    if updated == 0 {
        return;
    }
    if let Err(error) = identity.mark_notifications_read(&group_key(task_id)).await {
        eprintln!(
            "notification_read_failed: {} {:?}",
            error.code, error.status
        );
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckItem {
    group_key: String,
    #[serde(default)]
    #[allow(dead_code)]
    resource_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CheckRequest {
    #[serde(default)]
    items: Vec<CheckItem>,
}

const CHECK_LIMIT: usize = 1000;

/// `platformEndpoints.notificationCheck`: which group keys still point at a
/// task this person may read. Read only; platform calls only.
pub async fn notification_check(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    body: Option<Json<CheckRequest>>,
) -> ApiResult<Json<Value>> {
    if !identity.context.platform {
        return Err(platform_only());
    }
    let items = body.map(|Json(body)| body.items).unwrap_or_default();
    let task_ids: Vec<String> = items
        .iter()
        .take(CHECK_LIMIT)
        .filter_map(|item| item.group_key.strip_prefix("task:").map(str::to_string))
        .collect();
    if task_ids.is_empty() {
        return Ok(Json(json!({"allowed": []})));
    }
    let readable: Vec<String> = sqlx::query_scalar(
        "SELECT t.id FROM pms_tasks t JOIN pms_task_lists l ON l.id = t.list_id \
         JOIN pms_spaces s ON s.id = l.team_id AND s.active \
         WHERE t.id = ANY($1) AND ( \
           EXISTS (SELECT 1 FROM pms_space_members m WHERE m.space_id = s.id AND m.user_id = $2) \
           OR EXISTS (SELECT 1 FROM pms_task_user_access a WHERE a.task_id = t.id AND a.user_id = $2 \
             AND a.revoked_at IS NULL AND (a.expires_at IS NULL OR a.expires_at > (now() AT TIME ZONE 'UTC'))))",
    )
    .bind(&task_ids)
    .bind(&identity.user_id)
    .fetch_all(&pool)
    .await?;
    let readable: HashSet<String> = readable.into_iter().collect();
    let allowed: Vec<&str> = items
        .iter()
        .take(CHECK_LIMIT)
        .filter(|item| {
            item.group_key
                .strip_prefix("task:")
                .is_some_and(|id| readable.contains(id))
        })
        .map(|item| item.group_key.as_str())
        .collect();
    Ok(Json(json!({"allowed": allowed})))
}

pub fn platform_only() -> ApiError {
    ApiError::new(
        StatusCode::FORBIDDEN,
        "pms.platform_only",
        "Only the platform calls this endpoint.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::StatusDef;

    #[test]
    fn titles_follow_the_ai_do_display_table() {
        assert_eq!(
            title("홍길동", KIND_ASSIGNED, "로그인 개선", None),
            "홍길동 · 업무 배정 · 로그인 개선"
        );
        assert_eq!(
            title("홍길동", KIND_STATUS, "로그인 개선", Some("완료")),
            "홍길동 · 업무 상태 변경 · 로그인 개선 · 완료"
        );
        assert!(
            title("a", KIND_COMMENT, &"x".repeat(300), None)
                .chars()
                .count()
                <= TITLE_MAX
        );
        assert_eq!(task_label("  ", "WEB-3"), "WEB-3");
    }

    #[test]
    fn korean_status_names() {
        assert_eq!(status_label_ko("in_progress", None), "진행 중");
        let workflow = ListWorkflow {
            status_mode: "custom".into(),
            team_id: None,
            list_statuses: vec![
                StatusDef {
                    id: "1".into(),
                    slug: "done".into(),
                    name: "Done".into(),
                    color: "#000".into(),
                    category: "done".into(),
                    sort_order: 0,
                },
                StatusDef {
                    id: "2".into(),
                    slug: "qa".into(),
                    name: "품질 확인".into(),
                    color: "#000".into(),
                    category: "active".into(),
                    sort_order: 1,
                },
            ],
            space_statuses: vec![],
        };
        assert_eq!(status_label_ko("done", Some(&workflow)), "완료");
        assert_eq!(status_label_ko("qa", Some(&workflow)), "품질 확인");
        assert_eq!(status_label_ko("mystery", Some(&workflow)), "mystery");
    }

    #[test]
    fn group_keys_and_paths() {
        assert_eq!(group_key("t1"), "task:t1");
        assert_eq!(task_path("t1"), "/tasks/t1");
    }
}
