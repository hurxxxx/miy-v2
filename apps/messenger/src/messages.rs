//! Messages: AI-DO domains/dm/message_application.py, message_flow.py,
//! message_delivery.py and message_history.py, plus the miy author edit and
//! delete (COMMON-FEATURES-PLAN §2.2 "메신저의 메시지 수정·삭제 기준").
use crate::{
    attachments, audit,
    conversations::query_limit,
    error::{ApiError, ApiJson, ApiResult},
    live, notify,
    request::{
        DM_MESSAGE_ATTACHMENT_IDS_MAX_LENGTH, DM_MESSAGE_BODY_MAX_LENGTH, normalize_message_body,
        normalize_optional_route_id, normalize_route_id_list, route_id, unique_ids,
    },
    safety,
    schemas::{DmEditMessageRequest, DmMessageItem, DmMessageListResponse, DmSendMessageRequest},
    store::{self, AttachmentRow, Conversation, MessageRow, new_id},
    time::{now_utc, parse_query_datetime},
    ugc, views,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
};
use miy_app_sdk::Identity;
use serde::Deserialize;
use serde_json::json;
use sqlx::{PgConnection, PgPool};

#[derive(Debug, Deserialize)]
pub struct MessageListQuery {
    #[serde(default)]
    limit: Option<String>,
    #[serde(default)]
    before: Option<String>,
    #[serde(default)]
    before_sequence: Option<i32>,
    #[serde(default)]
    ids: Option<String>,
}

/// `GET /v1/dm/conversations/{id}/messages`: the newest `limit` messages the
/// caller may see (joined before), older than `before` when given.
pub async fn list_dm_messages(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(conversation_id): Path<String>,
    Query(query): Query<MessageListQuery>,
) -> ApiResult<Json<DmMessageListResponse>> {
    route_id(&conversation_id)?;
    let limit = query_limit(query.limit.as_deref(), 50)?;
    let ids = if let Some(ids) = query.ids.as_deref() {
        let ids: Vec<&str> = ids.split(',').collect();
        if ids.len() > 50 || query.before.is_some() || query.before_sequence.is_some() {
            return Err(ApiError::validation("Invalid message refresh list"));
        }
        for id in &ids {
            route_id(id)?;
        }
        Some(ids)
    } else {
        None
    };
    if query.before_sequence.is_some_and(|sequence| sequence < 1)
        || (query.before_sequence.is_some() && query.before.is_some())
    {
        return Err(ApiError::validation("Invalid message cursor"));
    }
    let before = match query.before.as_deref() {
        None => None,
        Some(value) => Some(
            parse_query_datetime(value)
                .ok_or_else(|| ApiError::validation("before: Input should be a valid datetime"))?,
        ),
    };
    let me = identity.user_id.as_str();
    let mut conn = pool.acquire().await?;
    let conversation =
        store::require_user_conversation(&mut conn, me, &conversation_id, false).await?;
    let participant = conversation.require_participant(me)?.clone();
    let mut rows: Vec<MessageRow> = sqlx::query_as(&format!(
        "SELECT {} FROM dm_messages WHERE conversation_id = $1 AND created_at >= $2 \
           AND ($3::timestamp IS NULL OR created_at < $3) \
           AND ($4::integer IS NULL OR sequence < $4) \
           AND ($5::text[] IS NULL OR id = ANY($5)) ORDER BY sequence DESC LIMIT $6",
        store::MESSAGE_COLUMNS
    ))
    .bind(&conversation_id)
    .bind(participant.joined_at)
    .bind(before)
    .bind(query.before_sequence)
    .bind(ids)
    .bind(i64::from(limit))
    .fetch_all(&mut *conn)
    .await?;
    drop(conn);
    rows.reverse();
    let items =
        views::render_messages(&pool, &identity, &conversation, Some(&participant), &rows).await?;
    Ok(Json(DmMessageListResponse { items }))
}

fn body_required() -> ApiError {
    ApiError::unprocessable(
        "dm.message_body_required",
        "A message needs text or an attachment.",
    )
}

fn check_body_length(body: &str) -> ApiResult<()> {
    if body.chars().count() > DM_MESSAGE_BODY_MAX_LENGTH {
        return Err(ApiError::validation(
            "body: String should have at most 120000 characters",
        ));
    }
    Ok(())
}

/// `message_attachments_to_link` (`_DmAttachmentLinkBuilder`).
async fn attachments_to_link(
    conn: &mut PgConnection,
    sender: &str,
    conversation_id: &str,
    attachment_ids: &[String],
) -> ApiResult<Vec<AttachmentRow>> {
    let ids = unique_ids(attachment_ids);
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<AttachmentRow> = sqlx::query_as(&format!(
        "SELECT {} FROM dm_message_attachments WHERE id = ANY($1) FOR UPDATE",
        store::ATTACHMENT_COLUMNS
    ))
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    if rows.len() != ids.len() {
        return Err(ApiError::attachment_not_found());
    }
    let mut ordered = Vec::with_capacity(ids.len());
    for id in &ids {
        let row = rows
            .iter()
            .find(|row| &row.id == id)
            .ok_or_else(ApiError::attachment_not_found)?;
        if row.conversation_id != conversation_id || row.uploader_id != sender {
            return Err(ApiError::attachment_not_found());
        }
        if row.message_id.is_some() {
            return Err(ApiError::unprocessable(
                "dm.attachment_already_sent",
                "This attachment was already sent.",
            ));
        }
        // miy: the bytes must have arrived (AI-DO stored them before the row).
        if row.storage_status != "uploaded" {
            return Err(ApiError::unprocessable(
                "dm.attachment_not_uploaded",
                "The attachment has not finished uploading.",
            ));
        }
        ordered.push(row.clone());
    }
    Ok(ordered)
}

/// `POST /v1/dm/conversations/{id}/messages` (`message_delivery.send_message`).
pub async fn send_dm_message(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(conversation_id): Path<String>,
    ApiJson(request): ApiJson<DmSendMessageRequest>,
) -> ApiResult<Json<DmMessageItem>> {
    route_id(&conversation_id)?;
    let body = normalize_message_body(request.body);
    check_body_length(&body)?;
    let attachment_ids = normalize_route_id_list(request.attachment_ids, true)?;
    if attachment_ids.len() > DM_MESSAGE_ATTACHMENT_IDS_MAX_LENGTH {
        return Err(ApiError::validation(
            "attachment_ids: List should have at most 10 items",
        ));
    }
    let reply_to_message_id = normalize_optional_route_id(request.reply_to_message_id)?;
    if body.is_empty() && attachment_ids.is_empty() {
        return Err(ApiError::validation("body or attachment_ids is required"));
    }
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    let conversation =
        store::require_user_conversation(&mut tx, me, &conversation_id, true).await?;
    // `_DmMessageComposePlanner.plan`.
    ugc::ensure_user_content_allowed(&mut tx, me, &[body.as_str()]).await?;
    let sender_participant = conversation.require_participant(me)?.clone();
    let linked = attachments_to_link(&mut tx, me, &conversation_id, &attachment_ids).await?;
    if !linked.is_empty() {
        let names: Vec<&str> = linked.iter().map(|row| row.filename.as_str()).collect();
        ugc::ensure_user_content_allowed(&mut tx, me, &names).await?;
    }
    if body.is_empty() && linked.is_empty() {
        return Err(body_required());
    }
    if let Some(reply_id) = reply_to_message_id.as_deref() {
        let reply = store::message_by_id(&mut tx, &conversation_id, reply_id, false).await?;
        if reply.is_none_or(|reply| reply.created_at < sender_participant.joined_at) {
            return Err(ApiError::message_not_found());
        }
    }
    if conversation.active().all(|p| p.user_id == me) {
        return Err(ApiError::thread_not_found());
    }
    // The conversation lock keeps membership, message order and the read marker current.
    let now = now_utc();
    let sequence: Option<i32> = sqlx::query_scalar(
        "UPDATE dm_conversations SET message_seq = message_seq + 1, updated_at = $2 WHERE id = $1 \
         RETURNING message_seq",
    )
    .bind(&conversation_id)
    .bind(now)
    .fetch_optional(&mut *tx)
    .await?;
    let sequence = sequence.ok_or_else(ApiError::thread_not_found)?;
    safety::ensure_direct_conversation_open(&mut tx, &conversation, me).await?;
    let message_id = new_id();
    sqlx::query(
        "INSERT INTO dm_messages (id, conversation_id, sequence, sender_id, reply_to_message_id, body, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&message_id)
    .bind(&conversation_id)
    .bind(sequence)
    .bind(me)
    .bind(&reply_to_message_id)
    .bind(&body)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE dm_conversation_participants SET last_read_message_id = $2 WHERE id = $1")
        .bind(&sender_participant.id)
        .bind(&message_id)
        .execute(&mut *tx)
        .await?;
    let linked_ids: Vec<String> = linked.iter().map(|row| row.id.clone()).collect();
    for id in &linked_ids {
        sqlx::query("UPDATE dm_message_attachments SET message_id = $2, attach_request_id = $3 WHERE id = $1")
            .bind(id)
            .bind(&message_id)
            .bind(uuid::Uuid::new_v4())
            .execute(&mut *tx)
            .await?;
    }
    let message = store::message_by_id(&mut tx, &conversation_id, &message_id, false)
        .await?
        .ok_or_else(ApiError::message_not_found)?;
    tx.commit().await?;

    live::conversation_changed(&pool, &conversation_id, &[]).await;
    attachments::attach_linked(&pool, &identity, &linked_ids).await;
    notify::message_created(&pool, &identity, &conversation, &message).await;
    Ok(Json(
        render_one(&pool, &identity, &conversation, &message).await?,
    ))
}

/// The author's own message in a conversation they are still in, locked.
async fn own_message(
    conn: &mut PgConnection,
    me: &str,
    conversation_id: &str,
    message_id: &str,
) -> ApiResult<(Conversation, MessageRow)> {
    let conversation = store::require_user_conversation(conn, me, conversation_id, true).await?;
    let participant = conversation.require_participant(me)?;
    let message = store::message_by_id(conn, conversation_id, message_id, true)
        .await?
        .filter(|message| message.created_at >= participant.joined_at)
        .ok_or_else(ApiError::message_not_found)?;
    if message.sender_id != me {
        return Err(ApiError::forbidden(
            "dm.message_edit_forbidden",
            "Only the author can change this message.",
        ));
    }
    Ok((conversation, message))
}

async fn render_one(
    pool: &PgPool,
    identity: &Identity,
    conversation: &Conversation,
    message: &MessageRow,
) -> ApiResult<DmMessageItem> {
    // Render the authorized write's outcome even if membership changes after commit.
    views::render_messages(
        pool,
        identity,
        conversation,
        conversation.active_participant(&identity.user_id),
        std::slice::from_ref(message),
    )
    .await?
    .pop()
    .ok_or_else(ApiError::internal)
}

/// miy: `PATCH /v1/dm/conversations/{id}/messages/{message_id}`. Only the
/// author; not a hidden or deleted message. The replaced text stays in
/// `dm_message_revisions`; the message shows "수정됨" (`edited_at`).
pub async fn edit_dm_message(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path((conversation_id, message_id)): Path<(String, String)>,
    ApiJson(request): ApiJson<DmEditMessageRequest>,
) -> ApiResult<Json<DmMessageItem>> {
    route_id(&conversation_id)?;
    route_id(&message_id)?;
    let body = normalize_message_body(request.body);
    check_body_length(&body)?;
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    let (conversation, mut message) =
        own_message(&mut tx, me, &conversation_id, &message_id).await?;
    if message.hidden() || message.deleted() {
        return Err(ApiError::unprocessable(
            "dm.message_not_editable",
            "This message can no longer be changed.",
        ));
    }
    ugc::ensure_user_content_allowed(&mut tx, me, &[body.as_str()]).await?;
    let attachment_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM dm_message_attachments WHERE message_id = $1 AND storage_status IN ('uploaded', 'attached')",
    )
    .bind(&message_id)
    .fetch_one(&mut *tx)
    .await?;
    if body.is_empty() && attachment_count == 0 {
        return Err(body_required());
    }
    safety::ensure_direct_conversation_open(&mut tx, &conversation, me).await?;
    if body != message.body {
        let now = now_utc();
        sqlx::query(
            "INSERT INTO dm_message_revisions (message_id, action, body_before, actor_user_id, created_at) \
             VALUES ($1, 'edit', $2, $3, $4)",
        )
        .bind(&message_id)
        .bind(&message.body)
        .bind(me)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE dm_messages SET body = $2, edited_at = $3 WHERE id = $1")
            .bind(&message_id)
            .bind(&body)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        audit::record(
            &mut tx,
            "dm.message_edited",
            "dm_message",
            Some(&message_id),
            "DM 메시지를 고쳤다",
            me,
            json!({"conversation_id": conversation_id}),
        )
        .await?;
        message.body = body;
        message.edited_at = Some(now);
        tx.commit().await?;
        live::conversation_changed(&pool, &conversation_id, &[]).await;
        notify::message_changed(&pool, &identity, &conversation, &message).await;
    } else {
        tx.commit().await?;
    }
    Ok(Json(
        render_one(&pool, &identity, &conversation, &message).await?,
    ))
}

/// miy: `DELETE /v1/dm/conversations/{id}/messages/{message_id}`. Only the
/// author. The flow keeps a "삭제된 메시지" placeholder; the body is cleared
/// and the files are released after commit. While a report on the message
/// is open, the body copy stays in the revision and the files stay attached
/// until the report is resolved.
pub async fn delete_dm_message(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path((conversation_id, message_id)): Path<(String, String)>,
) -> ApiResult<Json<DmMessageItem>> {
    route_id(&conversation_id)?;
    route_id(&message_id)?;
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    let (conversation, mut message) =
        own_message(&mut tx, me, &conversation_id, &message_id).await?;
    if message.deleted() {
        tx.commit().await?;
        return Ok(Json(
            render_one(&pool, &identity, &conversation, &message).await?,
        ));
    }
    let reported = safety::message_has_open_report(&mut tx, &message_id).await?;
    let now = now_utc();
    sqlx::query(
        "INSERT INTO dm_message_revisions (message_id, action, body_before, actor_user_id, created_at) \
         VALUES ($1, 'delete', $2, $3, $4)",
    )
    .bind(&message_id)
    .bind(reported.then_some(&message.body))
    .bind(me)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    if !reported {
        // Earlier edit texts go with the body.
        sqlx::query("UPDATE dm_message_revisions SET body_before = NULL WHERE message_id = $1")
            .bind(&message_id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE dm_messages SET body = '', deleted_at = $2 WHERE id = $1")
        .bind(&message_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut tx,
        "dm.message_deleted",
        "dm_message",
        Some(&message_id),
        "DM 메시지를 지웠다",
        me,
        json!({"conversation_id": conversation_id, "report_open": reported}),
    )
    .await?;
    message.body.clear();
    message.deleted_at = Some(now);
    tx.commit().await?;
    live::conversation_changed(&pool, &conversation_id, &[]).await;
    if !reported {
        attachments::release_deleted_message_files(&pool, &identity, &message_id).await;
    }
    attachments::release_pending_files(&pool, &identity).await;
    notify::message_changed(&pool, &identity, &conversation, &message).await;
    Ok(Json(
        render_one(&pool, &identity, &conversation, &message).await?,
    ))
}

/// The status a deleted-then-reported message reaches once all its reports
/// are closed: drop the body copy (files are released by the caller).
pub async fn purge_deleted_copy(conn: &mut PgConnection, message_id: &str) -> ApiResult<bool> {
    let deleted: Option<Option<chrono::NaiveDateTime>> =
        sqlx::query_scalar("SELECT deleted_at FROM dm_messages WHERE id = $1")
            .bind(message_id)
            .fetch_optional(&mut *conn)
            .await?;
    if !matches!(deleted, Some(Some(_)))
        || safety::message_has_open_report(conn, message_id).await?
    {
        return Ok(false);
    }
    sqlx::query("UPDATE dm_message_revisions SET body_before = NULL WHERE message_id = $1")
        .bind(message_id)
        .execute(conn)
        .await?;
    Ok(true)
}
