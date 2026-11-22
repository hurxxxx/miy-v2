//! Page comments (보강, not in AI-DO 0002af062: AI-DO docs had no comments
//! and no working mentions).
//!
//! - Anyone who may read a page comments on it; a thread is a root comment
//!   and its replies (one level, like the board's comments).
//! - The author edits a comment; the author or someone who manages the doc
//!   deletes it. A deleted comment with replies stays as a placeholder.
//! - Someone who may edit the doc, or the thread's author, resolves and
//!   reopens a thread.
//! - `@` mentions name people from the platform directory (`mention_ids`).
//!
//! Notifications (`platform:notifications:write`), one line per comment:
//!
//! | kind | recipients | group key |
//! | --- | --- | --- |
//! | `comment` | the doc owner and the thread's participants, except the author and the people mentioned | `comment:<id>` |
//! | `mention` | the people mentioned, except the author | `mention:<id>` |
//!
//! Only people who may read the page now receive them (the platform-call
//! readability rule, notify::user_can_read_page). The app cannot see another
//! person's groups (the directory answers only the caller's own), so people
//! who read the page only through a group are left out (fail closed). Deleting the comment retracts both groups;
//! removing a mention retracts it for that person; trashing the page or doc,
//! removing a share, making a doc private or removing someone from a space
//! retracts them for everyone who can no longer read the page. Opening the
//! page's comments marks them read.
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::NaiveDateTime;
use miy_app_sdk::{Identity, Notification};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    time::Duration,
};

use crate::{
    AppState,
    access::{self, NativePage, PAGE_COLUMNS, Require, Viewer},
    error::{ApiError, ApiJson, ApiResult, check_length},
    items, notify, people,
    time::{naive_iso, now_utc},
};

const CALL_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_MENTIONS: usize = 20;
const MAX_BODY_CHARS: usize = 10_000;

#[derive(Clone, Debug, FromRow)]
pub struct Comment {
    pub id: String,
    pub page_id: String,
    pub parent_id: Option<String>,
    pub author_id: String,
    pub body: String,
    pub mention_ids: Value,
    pub resolved_at: Option<NaiveDateTime>,
    pub resolved_by_id: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
}

const COLUMNS: &str = "id, page_id, parent_id, author_id, body, mention_ids, resolved_at, \
     resolved_by_id, created_at, updated_at, deleted_at";

pub fn comment_group_key(comment_id: &str) -> String {
    format!("comment:{comment_id}")
}

pub fn mention_group_key(comment_id: &str) -> String {
    format!("mention:{comment_id}")
}

fn mentions_of(comment: &Comment) -> Vec<String> {
    comment
        .mention_ids
        .as_array()
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn comment_not_found() -> ApiError {
    ApiError::not_found("docs.comment_not_found", "Comment not found.")
}

// ---------------------------------------------------------------- list

pub async fn list(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(page_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let mut viewer = Viewer::new(&state, &identity);
    let context = access::page_or_403(&mut viewer, &page_id, Require::View).await?;
    let sql = format!(
        "SELECT {COLUMNS} FROM docs_page_comments WHERE page_id = $1 ORDER BY created_at, id"
    );
    let rows = sqlx::query_as::<_, Comment>(&sql)
        .bind(&context.page.id)
        .fetch_all(&state.pool)
        .await?;
    let doc = access::load_doc(&state.pool, &context.page.doc_id)
        .await?
        .ok_or_else(access::page_not_found)?;
    let targets = access::load_targets(&state.pool, &doc.id).await?;
    let access = viewer.access(&doc, &targets).await?;
    let mut ids: BTreeSet<String> = BTreeSet::new();
    for row in &rows {
        ids.insert(row.author_id.clone());
        ids.extend(mentions_of(row));
        if let Some(by) = &row.resolved_by_id {
            ids.insert(by.clone());
        }
    }
    let names = people::display_names(&identity, ids).await;
    let mut replies: HashMap<String, Vec<&Comment>> = HashMap::new();
    for row in rows.iter().filter(|row| row.parent_id.is_some()) {
        replies
            .entry(row.parent_id.clone().unwrap_or_default())
            .or_default()
            .push(row);
    }
    let serialize = |row: &Comment, root: Option<&Comment>| -> Value {
        let deleted = row.deleted_at.is_some();
        let thread_author = root.map_or(&row.author_id, |root| &root.author_id);
        json!({
            "id": row.id,
            "page_id": row.page_id,
            "parent_id": row.parent_id,
            "author_id": row.author_id,
            "author_name": names.get(&row.author_id).cloned().unwrap_or_default(),
            "body": if deleted { String::new() } else { row.body.clone() },
            "mentions": if deleted { vec![] } else {
                mentions_of(row).into_iter().map(|id| json!({
                    "user_id": id,
                    "name": names.get(&id).cloned().unwrap_or_else(|| id.clone()),
                })).collect::<Vec<_>>()
            },
            "is_deleted": deleted,
            "resolved_at": row.resolved_at.map(naive_iso),
            "resolved_by_id": row.resolved_by_id,
            "resolved_by_name": row.resolved_by_id.as_ref().and_then(|id| names.get(id).cloned()),
            "created_at": naive_iso(row.created_at),
            "updated_at": naive_iso(row.updated_at),
            "can_edit": !deleted && row.author_id == identity.user_id,
            "can_delete": !deleted && (row.author_id == identity.user_id || access.can_manage),
            "can_resolve": root.is_none() && !deleted
                && (access.can_edit || *thread_author == identity.user_id),
        })
    };
    let mut threads = Vec::new();
    for root in rows.iter().filter(|row| row.parent_id.is_none()) {
        let thread_replies = replies.get(&root.id).cloned().unwrap_or_default();
        let live_replies = thread_replies
            .iter()
            .filter(|reply| reply.deleted_at.is_none())
            .count();
        if root.deleted_at.is_some() && live_replies == 0 {
            continue;
        }
        let mut item = serialize(root, None);
        item["replies"] = Value::Array(
            thread_replies
                .into_iter()
                .filter(|reply| reply.deleted_at.is_none())
                .map(|reply| serialize(reply, Some(root)))
                .collect(),
        );
        threads.push(item);
    }
    // Opening the page's comments reads their notifications (the newest
    // twenty; older lines were replaced or read long ago).
    let notified: Vec<(String, String)> = sqlx::query_as(
        "SELECT n.kind, n.comment_id FROM docs_comment_notifications n \
         JOIN docs_page_comments c ON c.id = n.comment_id \
         WHERE c.page_id = $1 AND n.recipient_id = $2 ORDER BY c.created_at DESC LIMIT 20",
    )
    .bind(&context.page.id)
    .bind(&identity.user_id)
    .fetch_all(&state.pool)
    .await?;
    for (kind, comment_id) in notified {
        notify::mark_read(&identity, &format!("{kind}:{comment_id}")).await;
    }
    Ok(Json(json!({
        "items": threads,
        "can_comment": true,
    })))
}

// ---------------------------------------------------------------- writes

/// Lock the page's doc for a comment write and require view access to the
/// live page (decided again under the lock).
async fn lock_page_for_comment(
    tx: &mut Transaction<'_, Postgres>,
    state: &AppState,
    identity: &Identity,
    page_id: &str,
) -> ApiResult<(access::DocContext, NativePage)> {
    let page_id = access::normalize_page_id(page_id)?;
    let page = access::load_page(&state.pool, &page_id)
        .await?
        .ok_or_else(access::page_not_found)?;
    let context = access::lock_doc_for_write(tx, state, identity, &page.doc_id).await?;
    let sql = format!("SELECT {PAGE_COLUMNS} FROM docs_native_doc_pages WHERE id = $1 FOR SHARE");
    let page = sqlx::query_as::<_, NativePage>(&sql)
        .bind(&page_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(access::page_not_found)?;
    if !context.access.can_view || page.trashed_at.is_some() || context.doc.trashed_at.is_some() {
        return Err(ApiError::forbidden(
            "docs.page_access_required",
            "Page access required.",
        ));
    }
    Ok((context, page))
}

fn validate_body(body: &str) -> ApiResult<String> {
    check_length("body", body, 1, MAX_BODY_CHARS)?;
    let trimmed = body.trim().to_string();
    if trimmed.is_empty() {
        return Err(ApiError::validation("body: Value error, must not be blank"));
    }
    Ok(trimmed)
}

/// Distinct mention IDs of people the directory knows now.
async fn validate_mentions(identity: &Identity, ids: &[String]) -> ApiResult<Vec<String>> {
    let mut unique: Vec<String> = Vec::new();
    for id in ids {
        let id = id.trim().to_string();
        if !id.is_empty() && !unique.contains(&id) {
            unique.push(id);
        }
    }
    if unique.len() > MAX_MENTIONS {
        return Err(ApiError::validation(format!(
            "mention_ids: List should have at most {MAX_MENTIONS} items"
        )));
    }
    if unique.is_empty() {
        return Ok(unique);
    }
    let names = people::display_names(identity, unique.iter().cloned()).await;
    if unique.iter().any(|id| !names.contains_key(id)) {
        return Err(ApiError::bad_request(
            "docs.mention_unknown_user",
            "A mentioned person was not found in the directory.",
        ));
    }
    Ok(unique)
}

#[derive(Deserialize)]
pub struct CreateRequest {
    body: String,
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    mention_ids: Vec<String>,
    request_id: String,
}

pub async fn create(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(page_id): Path<String>,
    ApiJson(payload): ApiJson<CreateRequest>,
) -> ApiResult<impl IntoResponse> {
    let request_id = uuid::Uuid::parse_str(&payload.request_id)
        .map_err(|_| ApiError::validation("request_id: Input should be a valid UUID"))?;
    let body = validate_body(&payload.body)?;
    let mentions = validate_mentions(&identity, &payload.mention_ids).await?;
    let mut tx = state.pool.begin().await?;
    let (context, page) = lock_page_for_comment(&mut tx, &state, &identity, &page_id).await?;
    let parent = match payload.parent_id.as_deref().filter(|id| !id.is_empty()) {
        Some(parent_id) => {
            let sql = format!("SELECT {COLUMNS} FROM docs_page_comments WHERE id = $1 FOR SHARE");
            let parent = sqlx::query_as::<_, Comment>(&sql)
                .bind(parent_id)
                .fetch_optional(&mut *tx)
                .await?
                .filter(|parent| {
                    parent.page_id == page.id
                        && parent.parent_id.is_none()
                        && parent.deleted_at.is_none()
                })
                .ok_or_else(comment_not_found)?;
            Some(parent)
        }
        None => None,
    };
    let now = now_utc();
    let sql = format!(
        "INSERT INTO docs_page_comments (id, page_id, parent_id, author_id, body, mention_ids, request_id, \
         created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$8) \
         ON CONFLICT (request_id) DO UPDATE SET request_id = EXCLUDED.request_id RETURNING {COLUMNS}"
    );
    let comment = sqlx::query_as::<_, Comment>(&sql)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&page.id)
        .bind(parent.as_ref().map(|parent| parent.id.clone()))
        .bind(&identity.user_id)
        .bind(&body)
        .bind(json!(mentions))
        .bind(request_id)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
    let repeated = comment.created_at != now;
    if comment.author_id != identity.user_id
        || comment.page_id != page.id
        || comment.parent_id != parent.as_ref().map(|parent| parent.id.clone())
        || (repeated && comment.body != body)
    {
        return Err(ApiError::conflict(
            "docs.comment_request_conflict",
            "The request ID is already in use.",
        ));
    }
    tx.commit().await?;
    if !repeated {
        notify_new(
            &state.pool,
            &identity,
            &context.doc,
            &page,
            &comment,
            parent.as_ref(),
        )
        .await;
    }
    Ok((
        if repeated {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
        Json(json!({ "id": comment.id, "created_at": naive_iso(comment.created_at) })),
    ))
}

async fn load_comment_for_write(
    tx: &mut Transaction<'_, Postgres>,
    state: &AppState,
    identity: &Identity,
    comment_id: &str,
) -> ApiResult<(access::DocContext, NativePage, Comment)> {
    let sql = format!("SELECT {COLUMNS} FROM docs_page_comments WHERE id = $1");
    let comment = sqlx::query_as::<_, Comment>(&sql)
        .bind(comment_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(comment_not_found)?;
    let (context, page) = lock_page_for_comment(tx, state, identity, &comment.page_id).await?;
    let sql = format!("SELECT {COLUMNS} FROM docs_page_comments WHERE id = $1 FOR UPDATE");
    let comment = sqlx::query_as::<_, Comment>(&sql)
        .bind(comment_id)
        .fetch_optional(&mut **tx)
        .await?
        .filter(|comment| comment.deleted_at.is_none())
        .ok_or_else(comment_not_found)?;
    Ok((context, page, comment))
}

#[derive(Deserialize)]
pub struct UpdateRequest {
    body: String,
    #[serde(default)]
    mention_ids: Vec<String>,
}

pub async fn update(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(comment_id): Path<String>,
    ApiJson(payload): ApiJson<UpdateRequest>,
) -> ApiResult<StatusCode> {
    let body = validate_body(&payload.body)?;
    let mentions = validate_mentions(&identity, &payload.mention_ids).await?;
    let mut tx = state.pool.begin().await?;
    let (context, page, comment) =
        load_comment_for_write(&mut tx, &state, &identity, &comment_id).await?;
    if comment.author_id != identity.user_id {
        return Err(ApiError::forbidden(
            "docs.comment_author_required",
            "Only the author can edit this comment.",
        ));
    }
    let before: BTreeSet<String> = mentions_of(&comment).into_iter().collect();
    let after: BTreeSet<String> = mentions.iter().cloned().collect();
    sqlx::query(
        "UPDATE docs_page_comments SET body = $2, mention_ids = $3, \
         updated_at = CASE WHEN body IS DISTINCT FROM $2 OR mention_ids::text IS DISTINCT FROM $3::text \
           THEN $4 ELSE updated_at END WHERE id = $1",
    )
    .bind(&comment.id)
    .bind(&body)
    .bind(json!(mentions))
    .bind(now_utc())
    .execute(&mut *tx)
    .await?;
    let removed: Vec<String> = before.difference(&after).cloned().collect();
    // Only the people who were notified have something to retract.
    let removed: Vec<String> = if removed.is_empty() {
        removed
    } else {
        sqlx::query_scalar(
            "DELETE FROM docs_comment_notifications WHERE comment_id = $1 AND kind = 'mention' \
             AND recipient_id = ANY($2) RETURNING recipient_id",
        )
        .bind(&comment.id)
        .bind(&removed)
        .fetch_all(&mut *tx)
        .await?
    };
    tx.commit().await?;
    if !removed.is_empty() {
        notify::retract_for(&identity, &mention_group_key(&comment.id), &removed).await;
    }
    let added: Vec<String> = after.difference(&before).cloned().collect();
    if !added.is_empty() {
        let readers =
            readers_among(&state.pool, &identity, &context.doc.id, &page.id, &added).await;
        publish(
            &state.pool,
            &identity,
            &context.doc,
            &page,
            &comment.id,
            "mention",
            readers,
        )
        .await;
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(comment_id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.pool.begin().await?;
    let (context, _page, comment) =
        load_comment_for_write(&mut tx, &state, &identity, &comment_id).await?;
    if comment.author_id != identity.user_id && !context.access.can_manage {
        return Err(ApiError::forbidden(
            "docs.comment_author_required",
            "Only the author can delete this comment.",
        ));
    }
    let now = now_utc();
    sqlx::query(
        "UPDATE docs_page_comments SET body = '', mention_ids = '[]', deleted_at = $2, updated_at = $2 \
         WHERE id = $1",
    )
    .bind(&comment.id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM docs_comment_notifications WHERE comment_id = $1")
        .bind(&comment.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    notify::retract(&identity, &comment_group_key(&comment.id)).await;
    notify::retract(&identity, &mention_group_key(&comment.id)).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_resolved(
    state: &AppState,
    identity: &Identity,
    comment_id: &str,
    resolved: bool,
) -> ApiResult<StatusCode> {
    let mut tx = state.pool.begin().await?;
    let (context, _page, comment) =
        load_comment_for_write(&mut tx, state, identity, comment_id).await?;
    if comment.parent_id.is_some() {
        return Err(ApiError::bad_request(
            "docs.comment_reply_not_resolvable",
            "Only a thread's first comment can be resolved.",
        ));
    }
    if !context.access.can_edit && comment.author_id != identity.user_id {
        return Err(ApiError::forbidden(
            "docs.doc_edit_access_required",
            "Doc edit access required.",
        ));
    }
    if resolved {
        sqlx::query(
            "UPDATE docs_page_comments SET resolved_at = COALESCE(resolved_at, $2), \
             resolved_by_id = COALESCE(resolved_by_id, $3) WHERE id = $1",
        )
        .bind(&comment.id)
        .bind(now_utc())
        .bind(&identity.user_id)
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query(
            "UPDATE docs_page_comments SET resolved_at = NULL, resolved_by_id = NULL WHERE id = $1",
        )
        .bind(&comment.id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn resolve(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(comment_id): Path<String>,
) -> ApiResult<StatusCode> {
    set_resolved(&state, &identity, &comment_id, true).await
}

pub async fn reopen(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(comment_id): Path<String>,
) -> ApiResult<StatusCode> {
    set_resolved(&state, &identity, &comment_id, false).await
}

// ---------------------------------------------------------------- notifications

/// Who among `candidates` may read the live page now (platform-call rule).
async fn readers_among(
    pool: &PgPool,
    identity: &Identity,
    doc_id: &str,
    page_id: &str,
    candidates: &[String],
) -> Vec<String> {
    let mut readers = Vec::new();
    for candidate in candidates {
        if notify::user_can_read_page(pool, identity, candidate, doc_id, page_id)
            .await
            .unwrap_or(false)
        {
            readers.push(candidate.clone());
        }
    }
    readers
}

async fn publish(
    pool: &PgPool,
    identity: &Identity,
    doc: &access::NativeDoc,
    page: &NativePage,
    comment_id: &str,
    kind: &str,
    recipients: Vec<String>,
) {
    let recipients: Vec<String> = recipients
        .into_iter()
        .filter(|id| id != &identity.user_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if recipients.is_empty() || identity.context.platform {
        return;
    }
    // Recorded before publishing, so a later loss of access can retract it.
    for recipient in &recipients {
        let _ = sqlx::query(
            "INSERT INTO docs_comment_notifications (comment_id, recipient_id, kind) VALUES ($1,$2,$3) \
             ON CONFLICT DO NOTHING",
        )
        .bind(comment_id)
        .bind(recipient)
        .bind(kind)
        .execute(pool)
        .await;
    }
    let actor = match people::display_names(identity, [identity.user_id.clone()])
        .await
        .remove(&identity.user_id)
    {
        Some(name) => format!("{name}님이"),
        None => "다른 사용자가".to_string(),
    };
    let page_title = notify::shorten(&page.title, 120);
    let title = if kind == "mention" {
        format!("{actor} \"{page_title}\"에서 회원님을 언급했습니다")
    } else {
        format!("{actor} \"{page_title}\"에 댓글을 남겼습니다")
    };
    let path = format!(
        "{}?page={}&comment={}",
        items::doc_path(&doc.id),
        items::urlencode_segment(&page.id),
        items::urlencode_segment(comment_id)
    );
    let notification = Notification {
        request_id: uuid::Uuid::new_v4().to_string(),
        recipients,
        kind: kind.to_string(),
        group_key: format!("{kind}:{comment_id}"),
        title: notify::shorten(&title, 200),
        path,
        resource_id: Some(comment_id.to_string()),
    };
    match tokio::time::timeout(CALL_TIMEOUT, identity.notify(&notification)).await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) if error.status.is_some() => {
            eprintln!("notification_rejected: {}", error.code);
        }
        _ => {
            let _ = tokio::time::timeout(
                CALL_TIMEOUT,
                identity.notification_by_request(&notification.request_id),
            )
            .await;
        }
    }
}

async fn notify_new(
    pool: &PgPool,
    identity: &Identity,
    doc: &access::NativeDoc,
    page: &NativePage,
    comment: &Comment,
    parent: Option<&Comment>,
) {
    let mentions = mentions_of(comment);
    let mut participants: Vec<String> = vec![doc.owner_id.clone()];
    if let Some(parent) = parent {
        participants.push(parent.author_id.clone());
        let others: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT author_id FROM docs_page_comments WHERE parent_id = $1 AND deleted_at IS NULL",
        )
        .bind(&parent.id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        participants.extend(others);
    }
    let participants: Vec<String> = participants
        .into_iter()
        .filter(|id| !mentions.contains(id))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let comment_readers = readers_among(pool, identity, &doc.id, &page.id, &participants).await;
    publish(
        pool,
        identity,
        doc,
        page,
        &comment.id,
        "comment",
        comment_readers,
    )
    .await;
    let mention_readers = readers_among(pool, identity, &doc.id, &page.id, &mentions).await;
    publish(
        pool,
        identity,
        doc,
        page,
        &comment.id,
        "mention",
        mention_readers,
    )
    .await;
}

/// Group keys and recipients to retract after a commit.
pub type Taken = BTreeMap<String, Vec<String>>;

/// Remove the notification records of comments on these pages inside the
/// caller's transaction (pages moving to the trash); retract after commit.
pub async fn take_notifications_for_pages(
    tx: &mut Transaction<'_, Postgres>,
    page_ids: &[String],
) -> ApiResult<Taken> {
    if page_ids.is_empty() {
        return Ok(Taken::new());
    }
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "DELETE FROM docs_comment_notifications n USING docs_page_comments c \
         WHERE c.id = n.comment_id AND c.page_id = ANY($1) RETURNING n.kind, n.comment_id, n.recipient_id",
    )
    .bind(page_ids)
    .fetch_all(&mut **tx)
    .await?;
    let mut taken = Taken::new();
    for (kind, comment_id, recipient) in rows {
        taken
            .entry(format!("{kind}:{comment_id}"))
            .or_default()
            .push(recipient);
    }
    Ok(taken)
}

pub async fn retract_taken(identity: &Identity, taken: &Taken) {
    for (group_key, recipients) in taken {
        notify::retract_for(identity, group_key, recipients).await;
    }
}

/// After an access change on a doc: retract the comment notifications of
/// people who can no longer read the page they point at.
pub async fn retract_lost_access(pool: &PgPool, identity: &Identity, doc_id: &str) {
    let Ok(rows) = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT n.kind, n.comment_id, n.recipient_id, c.page_id FROM docs_comment_notifications n \
         JOIN docs_page_comments c ON c.id = n.comment_id \
         JOIN docs_native_doc_pages p ON p.id = c.page_id WHERE p.doc_id = $1 LIMIT 1000",
    )
    .bind(doc_id)
    .fetch_all(pool)
    .await
    else {
        return;
    };
    let mut decided: HashMap<(String, String), bool> = HashMap::new();
    for (kind, comment_id, recipient, page_id) in rows {
        let key = (recipient.clone(), page_id.clone());
        let readable = match decided.get(&key) {
            Some(value) => *value,
            None => {
                let value =
                    notify::user_can_read_page(pool, identity, &recipient, doc_id, &page_id)
                        .await
                        .unwrap_or(true);
                decided.insert(key, value);
                value
            }
        };
        if readable {
            continue;
        }
        let _ = sqlx::query(
            "DELETE FROM docs_comment_notifications WHERE comment_id = $1 AND recipient_id = $2 AND kind = $3",
        )
        .bind(&comment_id)
        .bind(&recipient)
        .bind(&kind)
        .execute(pool)
        .await;
        notify::retract_for(
            identity,
            &format!("{kind}:{comment_id}"),
            std::slice::from_ref(&recipient),
        )
        .await;
    }
}

/// After space members changed: the docs of that space with comment
/// notifications (at most 200).
pub async fn retract_lost_access_in_space(pool: &PgPool, identity: &Identity, space_id: &str) {
    let Ok(doc_ids) = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT p.doc_id FROM docs_comment_notifications n \
         JOIN docs_page_comments c ON c.id = n.comment_id \
         JOIN docs_native_doc_pages p ON p.id = c.page_id \
         JOIN docs_native_docs d ON d.id = p.doc_id WHERE d.workspace_id = $1 LIMIT 200",
    )
    .bind(space_id)
    .fetch_all(pool)
    .await
    else {
        return;
    };
    for doc_id in doc_ids {
        retract_lost_access(pool, identity, &doc_id).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_keys_name_the_comment() {
        assert_eq!(comment_group_key("c1"), "comment:c1");
        assert_eq!(mention_group_key("c1"), "mention:c1");
    }

    #[test]
    fn bodies_are_trimmed_and_bounded() {
        assert_eq!(validate_body("  hi  ").unwrap(), "hi");
        assert!(validate_body("   ").is_err());
        assert!(validate_body(&"x".repeat(MAX_BODY_CHARS + 1)).is_err());
    }
}
