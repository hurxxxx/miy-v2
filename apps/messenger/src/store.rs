//! Rows and loaders. AI-DO hydrated conversations through SQLAlchemy
//! relationships (conversation_queries.hydrated_conversation_query); here the
//! same data is read with explicit queries.
use crate::error::{ApiError, ApiResult};
use chrono::NaiveDateTime;
use sqlx::PgConnection;

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)] // full row shape
pub struct ConversationRow {
    pub id: String,
    pub conversation_type: String,
    pub direct_key: Option<String>,
    pub title: Option<String>,
    pub created_by_id: String,
    pub message_seq: i32,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)] // full row shape
pub struct ParticipantRow {
    pub id: String,
    pub conversation_id: String,
    pub user_id: String,
    pub role: String,
    pub joined_at: NaiveDateTime,
    pub left_at: Option<NaiveDateTime>,
    pub muted_at: Option<NaiveDateTime>,
    pub last_read_message_id: Option<String>,
    /// Sequence of `last_read_message_id` (AI-DO `last_read_message.sequence`).
    pub last_read_sequence: Option<i32>,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)] // full row shape
pub struct MessageRow {
    pub id: String,
    pub conversation_id: String,
    pub sequence: i32,
    pub sender_id: String,
    pub reply_to_message_id: Option<String>,
    pub body: String,
    pub created_at: NaiveDateTime,
    pub moderation_hidden_at: Option<NaiveDateTime>,
    pub moderation_hidden_by_id: Option<String>,
    pub edited_at: Option<NaiveDateTime>,
    pub deleted_at: Option<NaiveDateTime>,
}

impl MessageRow {
    pub fn hidden(&self) -> bool {
        self.moderation_hidden_at.is_some()
    }
    pub fn deleted(&self) -> bool {
        self.deleted_at.is_some()
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)] // full row shape
pub struct AttachmentRow {
    pub id: String,
    pub conversation_id: String,
    pub message_id: Option<String>,
    pub uploader_id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i32,
    pub created_at: NaiveDateTime,
    pub storage_request_id: uuid::Uuid,
    pub storage_file_id: Option<uuid::Uuid>,
    pub storage_status: String,
    pub attach_request_id: Option<uuid::Uuid>,
    pub released_at: Option<NaiveDateTime>,
}

impl AttachmentRow {
    /// The Storage reference given at attach and detach.
    pub fn reference(&self) -> String {
        format!("dm-attachment:{}", self.id)
    }
}

pub const CONVERSATION_COLUMNS: &str =
    "id, conversation_type, direct_key, title, created_by_id, message_seq, created_at, updated_at";
pub const MESSAGE_COLUMNS: &str = "id, conversation_id, sequence, sender_id, reply_to_message_id, body, \
     created_at, moderation_hidden_at, moderation_hidden_by_id, edited_at, deleted_at";
pub const ATTACHMENT_COLUMNS: &str = "id, conversation_id, message_id, uploader_id, filename, content_type, \
     size_bytes, created_at, storage_request_id, storage_file_id, storage_status, attach_request_id, released_at";
const PARTICIPANT_SELECT: &str = "SELECT p.id, p.conversation_id, p.user_id, p.role, p.joined_at, p.left_at, \
     p.muted_at, p.last_read_message_id, lr.sequence AS last_read_sequence, p.created_at \
     FROM dm_conversation_participants p LEFT JOIN dm_messages lr ON lr.id = p.last_read_message_id";

/// A conversation with all its participant rows (active and departed), as
/// AI-DO's `conversation.participants`.
#[derive(Debug, Clone)]
pub struct Conversation {
    pub row: ConversationRow,
    pub participants: Vec<ParticipantRow>,
}

impl Conversation {
    pub fn id(&self) -> &str {
        &self.row.id
    }

    pub fn is_group(&self) -> bool {
        self.row.conversation_type == "group"
    }

    pub fn is_direct(&self) -> bool {
        self.row.conversation_type == "direct"
    }

    /// `participants.active_participants`.
    pub fn active(&self) -> impl Iterator<Item = &ParticipantRow> {
        self.participants.iter().filter(|p| p.left_at.is_none())
    }

    /// `participants.active_participant`.
    pub fn active_participant(&self, user_id: &str) -> Option<&ParticipantRow> {
        self.participants
            .iter()
            .find(|p| p.user_id == user_id && p.left_at.is_none())
    }

    /// `participants.other_active_participant`.
    pub fn other_active(&self, user_id: &str) -> Option<&ParticipantRow> {
        self.active().find(|p| p.user_id != user_id)
    }

    /// `safety.direct_peer_id`: the other person of a 1:1 (departed included).
    pub fn direct_peer_id(&self, user_id: &str) -> Option<String> {
        if !self.is_direct() {
            return None;
        }
        if let Some(other) = self.participants.iter().find(|p| p.user_id != user_id) {
            return Some(other.user_id.clone());
        }
        let key = self.row.direct_key.as_deref().unwrap_or_default();
        let ids: Vec<&str> = key.split(':').collect();
        if ids.len() == 2 && ids.contains(&user_id) {
            let other = if ids[0] == user_id { ids[1] } else { ids[0] };
            return (!other.is_empty()).then(|| other.to_string());
        }
        None
    }

    pub fn require_participant(&self, user_id: &str) -> ApiResult<&ParticipantRow> {
        self.active_participant(user_id)
            .ok_or_else(ApiError::thread_not_found)
    }

    pub fn ensure_group(&self) -> ApiResult<()> {
        if self.is_group() {
            Ok(())
        } else {
            Err(ApiError::unprocessable(
                "dm.group_only",
                "This action is only available in group conversations.",
            ))
        }
    }

    /// `participants.ensure_conversation_manager`.
    pub fn ensure_manager(&self, user_id: &str) -> ApiResult<()> {
        match self.active_participant(user_id) {
            Some(p) if p.role == "owner" || p.role == "admin" => Ok(()),
            _ => Err(ApiError::forbidden(
                "dm.manage_forbidden",
                "You cannot manage this conversation.",
            )),
        }
    }
}

pub async fn participants_of(
    conn: &mut PgConnection,
    conversation_ids: &[String],
) -> ApiResult<Vec<ParticipantRow>> {
    Ok(sqlx::query_as::<_, ParticipantRow>(&format!(
        "{PARTICIPANT_SELECT} WHERE p.conversation_id = ANY($1) ORDER BY p.joined_at, p.created_at, p.id"
    ))
    .bind(conversation_ids)
    .fetch_all(conn)
    .await?)
}

/// `conversation_queries.conversation_for_publish`. `lock` takes the
/// conversation row lock for a membership change.
pub async fn load_conversation(
    conn: &mut PgConnection,
    id: &str,
    lock: bool,
) -> ApiResult<Option<Conversation>> {
    let row = sqlx::query_as::<_, ConversationRow>(&format!(
        "SELECT {CONVERSATION_COLUMNS} FROM dm_conversations WHERE id = $1{}",
        if lock { " FOR UPDATE" } else { "" }
    ))
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let participants = participants_of(conn, std::slice::from_ref(&row.id)).await?;
    Ok(Some(Conversation { row, participants }))
}

/// `conversation_queries.require_user_conversation`: 404 unless the user is an
/// active participant.
pub async fn require_user_conversation(
    conn: &mut PgConnection,
    user_id: &str,
    id: &str,
    lock: bool,
) -> ApiResult<Conversation> {
    let conversation = load_conversation(conn, id, lock)
        .await?
        .ok_or_else(ApiError::thread_not_found)?;
    conversation.require_participant(user_id)?;
    Ok(conversation)
}

/// `conversation_queries.list_user_conversations`, newest first.
pub async fn list_user_conversations(
    conn: &mut PgConnection,
    user_id: &str,
) -> ApiResult<Vec<Conversation>> {
    let rows = sqlx::query_as::<_, ConversationRow>(&format!(
        "SELECT {CONVERSATION_COLUMNS} FROM dm_conversations WHERE id IN (\
           SELECT conversation_id FROM dm_conversation_participants WHERE user_id = $1 AND left_at IS NULL) \
         ORDER BY updated_at DESC, id"
    ))
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await?;
    let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
    let mut participants = participants_of(conn, &ids).await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let (mine, rest): (Vec<_>, Vec<_>) = participants
                .drain(..)
                .partition(|p| p.conversation_id == row.id);
            participants = rest;
            Conversation {
                row,
                participants: mine,
            }
        })
        .collect())
}

pub async fn message_by_id(
    conn: &mut PgConnection,
    conversation_id: &str,
    message_id: &str,
    lock: bool,
) -> ApiResult<Option<MessageRow>> {
    Ok(sqlx::query_as::<_, MessageRow>(&format!(
        "SELECT {MESSAGE_COLUMNS} FROM dm_messages WHERE id = $1 AND conversation_id = $2{}",
        if lock { " FOR UPDATE" } else { "" }
    ))
    .bind(message_id)
    .bind(conversation_id)
    .fetch_optional(conn)
    .await?)
}

pub async fn messages_by_ids(
    conn: &mut PgConnection,
    ids: &[String],
) -> ApiResult<Vec<MessageRow>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_as::<_, MessageRow>(&format!(
        "SELECT {MESSAGE_COLUMNS} FROM dm_messages WHERE id = ANY($1)"
    ))
    .bind(ids)
    .fetch_all(conn)
    .await?)
}

/// `read_state.latest_visible_message`.
pub async fn latest_visible_message(
    conn: &mut PgConnection,
    conversation_id: &str,
    participant: &ParticipantRow,
) -> ApiResult<Option<MessageRow>> {
    Ok(sqlx::query_as::<_, MessageRow>(&format!(
        "SELECT {MESSAGE_COLUMNS} FROM dm_messages WHERE conversation_id = $1 AND created_at >= $2 \
         ORDER BY sequence DESC LIMIT 1"
    ))
    .bind(conversation_id)
    .bind(participant.joined_at)
    .fetch_optional(conn)
    .await?)
}

/// `conversation_queries.latest_message` (any participant's view).
pub async fn latest_message_id(
    conn: &mut PgConnection,
    conversation_id: &str,
) -> ApiResult<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM dm_messages WHERE conversation_id = $1 ORDER BY sequence DESC LIMIT 1",
    )
    .bind(conversation_id)
    .fetch_optional(conn)
    .await?)
}

pub async fn attachments_of_messages(
    conn: &mut PgConnection,
    message_ids: &[String],
) -> ApiResult<Vec<AttachmentRow>> {
    if message_ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_as::<_, AttachmentRow>(&format!(
        "SELECT {ATTACHMENT_COLUMNS} FROM dm_message_attachments WHERE message_id = ANY($1) \
         ORDER BY created_at, id"
    ))
    .bind(message_ids)
    .fetch_all(conn)
    .await?)
}

pub async fn attachment_by_id(
    conn: &mut PgConnection,
    id: &str,
    lock: bool,
) -> ApiResult<Option<AttachmentRow>> {
    Ok(sqlx::query_as::<_, AttachmentRow>(&format!(
        "SELECT {ATTACHMENT_COLUMNS} FROM dm_message_attachments WHERE id = $1{}",
        if lock { " FOR UPDATE" } else { "" }
    ))
    .bind(id)
    .fetch_optional(conn)
    .await?)
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
