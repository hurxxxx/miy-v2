//! Response projection: AI-DO domains/dm/serialization.py,
//! thread_projection.py and read_state.message_read_state.
use crate::{
    attachments::is_previewable_image_content_type,
    error::{ApiError, ApiResult},
    safety,
    schemas::{
        DmConversationItem, DmConversationParticipantItem, DmMessageAttachmentItem, DmMessageItem,
        DmMessageReadStateItem, DmMessageReplyToItem,
    },
    store::{self, AttachmentRow, Conversation, MessageRow, ParticipantRow},
    time::{iso, iso_opt},
    users::{self, Names},
};
use miy_app_sdk::Identity;
use sqlx::{PgConnection, PgPool};
use std::collections::{HashMap, HashSet};

pub const DM_REPLY_BODY_PREVIEW_LENGTH: usize = 120;
pub const HIDDEN_MESSAGE_PLACEHOLDER: &str = "관리자가 가린 메시지입니다.";

/// `safety.visible_body`. miy: a deleted message has no body.
pub fn visible_body(message: &MessageRow) -> String {
    if message.hidden() {
        HIDDEN_MESSAGE_PLACEHOLDER.to_string()
    } else if message.deleted() {
        String::new()
    } else {
        message.body.clone()
    }
}

/// `safety.visible_attachments`. miy: a deleted message has none.
pub fn attachments_visible(message: &MessageRow) -> bool {
    !message.hidden() && !message.deleted()
}

pub fn attachment_content_path(attachment_id: &str) -> String {
    format!("/dm/attachments/{attachment_id}/content")
}

/// `serialization.serialize_attachment` with miy file paths.
pub fn attachment_item(row: &AttachmentRow) -> DmMessageAttachmentItem {
    let image =
        row.storage_status != "reserved" && is_previewable_image_content_type(&row.content_type);
    let download_url = attachment_content_path(&row.id);
    DmMessageAttachmentItem {
        id: row.id.clone(),
        conversation_id: row.conversation_id.clone(),
        message_id: row.message_id.clone(),
        filename: row.filename.clone(),
        content_type: row.content_type.clone(),
        size_bytes: i64::from(row.size_bytes),
        is_image: image,
        preview_url: image.then(|| format!("{download_url}?disposition=inline")),
        download_url,
        created_at: iso(row.created_at),
    }
}

/// `read_state.message_read_state`.
pub fn message_read_state(
    message: &MessageRow,
    conversation: &Conversation,
) -> DmMessageReadStateItem {
    let unread_count = conversation
        .active()
        .filter(|p| p.user_id != message.sender_id && p.joined_at <= message.created_at)
        .filter(|p| {
            p.last_read_sequence
                .is_none_or(|seq| seq < message.sequence)
        })
        .count() as i64;
    DmMessageReadStateItem {
        unread_count,
        read_by_all: unread_count == 0,
    }
}

/// `thread_projection.conversation_display_name`.
pub fn conversation_display_name(conversation: &Conversation, me: &str, names: &Names) -> String {
    if conversation.is_direct() {
        return conversation
            .other_active(me)
            .map(|p| names.name(&p.user_id))
            .unwrap_or_else(|| "DM".to_string());
    }
    if let Some(title) = conversation.row.title.as_deref().filter(|t| !t.is_empty()) {
        return title.to_string();
    }
    let others: Vec<String> = conversation
        .active()
        .filter(|p| p.user_id != me)
        .map(|p| names.name(&p.user_id))
        .take(3)
        .collect();
    if others.is_empty() {
        "DM".to_string()
    } else {
        others.join(", ")
    }
}

/// Attachments and quoted messages a set of messages needs.
#[derive(Default)]
pub struct MessageExtras {
    attachments: HashMap<String, Vec<AttachmentRow>>,
    replies: HashMap<String, MessageRow>,
    mentions: HashMap<String, Vec<crate::mentions::Mention>>,
    reactions: HashMap<String, Vec<crate::reactions::ReactionRow>>,
}

impl MessageExtras {
    pub async fn load(conn: &mut PgConnection, rows: &[&MessageRow]) -> ApiResult<Self> {
        let reply_ids: Vec<String> = rows
            .iter()
            .filter_map(|row| row.reply_to_message_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let replies = store::messages_by_ids(conn, &reply_ids).await?;
        let mut ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        ids.extend(replies.iter().map(|row| row.id.clone()));
        let mut attachments: HashMap<String, Vec<AttachmentRow>> = HashMap::new();
        for attachment in store::attachments_of_messages(conn, &ids).await? {
            if let Some(message_id) = attachment.message_id.clone() {
                attachments.entry(message_id).or_default().push(attachment);
            }
        }
        let own_ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        let mut mentions: HashMap<String, Vec<crate::mentions::Mention>> = HashMap::new();
        for row in crate::mentions::of_messages(conn, &own_ids).await? {
            mentions
                .entry(row.message_id)
                .or_default()
                .push(crate::mentions::Mention {
                    user_id: row.user_id,
                    start: row.start_index,
                    length: row.length,
                });
        }
        let mut reactions: HashMap<String, Vec<crate::reactions::ReactionRow>> = HashMap::new();
        for row in crate::reactions::of_messages(conn, &own_ids).await? {
            reactions
                .entry(row.message_id.clone())
                .or_default()
                .push(row);
        }
        Ok(Self {
            attachments,
            mentions,
            reactions,
            replies: replies
                .into_iter()
                .map(|row| (row.id.clone(), row))
                .collect(),
        })
    }

    pub fn user_ids(&self, rows: &[&MessageRow]) -> Vec<String> {
        rows.iter()
            .map(|row| row.sender_id.clone())
            .chain(self.replies.values().map(|row| row.sender_id.clone()))
            .chain(
                self.reactions
                    .values()
                    .flatten()
                    .map(|row| row.user_id.clone()),
            )
            .collect()
    }

    fn visible_attachments(&self, message: &MessageRow) -> Vec<&AttachmentRow> {
        if !attachments_visible(message) {
            return Vec::new();
        }
        self.attachments
            .get(&message.id)
            .map(|rows| {
                rows.iter()
                    .filter(|row| matches!(row.storage_status.as_str(), "uploaded" | "attached"))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `serialization.serialize_message`.
    pub fn render(
        &self,
        message: &MessageRow,
        conversation: &Conversation,
        viewer: Option<&ParticipantRow>,
        names: &Names,
    ) -> DmMessageItem {
        let reply_to = message
            .reply_to_message_id
            .as_ref()
            .and_then(|id| self.replies.get(id))
            .filter(|reply| viewer.is_none_or(|viewer| reply.created_at >= viewer.joined_at))
            .map(|reply| self.reply_item(reply, names));
        DmMessageItem {
            id: message.id.clone(),
            conversation_id: message.conversation_id.clone(),
            thread_id: message.conversation_id.clone(),
            sequence: message.sequence,
            sender_id: message.sender_id.clone(),
            sender_name: names.name(&message.sender_id),
            read_state: message_read_state(message, conversation),
            reply_to,
            body: visible_body(message),
            attachments: self
                .visible_attachments(message)
                .into_iter()
                .map(attachment_item)
                .collect(),
            created_at: iso(message.created_at),
            hidden: message.hidden(),
            edited_at: iso_opt(message.edited_at),
            deleted: message.deleted(),
            mentions: if attachments_visible(message) {
                self.mentions.get(&message.id).cloned().unwrap_or_default()
            } else {
                Vec::new()
            },
            reactions: if attachments_visible(message) {
                let rows: Vec<&crate::reactions::ReactionRow> = self
                    .reactions
                    .get(&message.id)
                    .map(|rows| rows.iter().collect())
                    .unwrap_or_default();
                crate::reactions::items(&rows, viewer.map_or("", |v| v.user_id.as_str()), names)
            } else {
                Vec::new()
            },
        }
    }

    /// `thread_projection.build_message_reply_to_item`.
    fn reply_item(&self, reply: &MessageRow, names: &Names) -> DmMessageReplyToItem {
        let mut body_preview = visible_body(reply).trim().to_string();
        if body_preview.chars().count() > DM_REPLY_BODY_PREVIEW_LENGTH {
            body_preview = format!(
                "{}...",
                body_preview
                    .chars()
                    .take(DM_REPLY_BODY_PREVIEW_LENGTH - 1)
                    .collect::<String>()
            );
        }
        DmMessageReplyToItem {
            id: reply.id.clone(),
            sender_id: reply.sender_id.clone(),
            sender_name: names.name(&reply.sender_id),
            body_preview,
            attachment_count: self.visible_attachments(reply).len() as i64,
            created_at: iso(reply.created_at),
            hidden: reply.hidden(),
            deleted: reply.deleted(),
        }
    }
}

/// Messages of one conversation for `viewer`.
pub async fn render_messages(
    pool: &PgPool,
    identity: &Identity,
    conversation: &Conversation,
    viewer: Option<&ParticipantRow>,
    rows: &[MessageRow],
) -> ApiResult<Vec<DmMessageItem>> {
    let refs: Vec<&MessageRow> = rows.iter().collect();
    let mut conn = pool.acquire().await?;
    let extras = MessageExtras::load(&mut conn, &refs).await?;
    drop(conn);
    let names = users::names(pool, identity, extras.user_ids(&refs)).await;
    Ok(rows
        .iter()
        .map(|row| extras.render(row, conversation, viewer, &names))
        .collect())
}

pub fn participant_item(
    participant: &ParticipantRow,
    names: &Names,
) -> DmConversationParticipantItem {
    DmConversationParticipantItem {
        user: names.item(&participant.user_id),
        role: participant.role.clone(),
        joined_at: iso(participant.joined_at),
        left_at: iso_opt(participant.left_at),
        muted_at: iso_opt(participant.muted_at),
        last_read_message_id: participant.last_read_message_id.clone(),
    }
}

/// `serialization.serialize_conversation` for several conversations at once.
/// `blocked` is the caller's block list when already read.
pub async fn render_conversations(
    pool: &PgPool,
    identity: &Identity,
    conversations: &[Conversation],
    blocked: Option<&HashSet<String>>,
) -> ApiResult<Vec<DmConversationItem>> {
    let me = identity.user_id.as_str();
    if conversations.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = conversations.iter().map(|c| c.id().to_string()).collect();
    let mut conn = pool.acquire().await?;
    let last_messages: Vec<MessageRow> = sqlx::query_as(&format!(
        "SELECT DISTINCT ON (m.conversation_id) {} FROM dm_messages m \
         JOIN dm_conversation_participants p ON p.conversation_id = m.conversation_id \
           AND p.user_id = $1 AND p.left_at IS NULL \
         WHERE m.conversation_id = ANY($2) AND m.created_at >= p.joined_at \
         ORDER BY m.conversation_id, m.sequence DESC",
        store::MESSAGE_COLUMNS
            .split(", ")
            .map(|column| format!("m.{}", column.trim()))
            .collect::<Vec<_>>()
            .join(", ")
    ))
    .bind(me)
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await?;
    let unread = unread_counts(&mut conn, me, &ids).await?;
    let refs: Vec<&MessageRow> = last_messages.iter().collect();
    let extras = MessageExtras::load(&mut conn, &refs).await?;
    let blocked_owned;
    let blocked = match blocked {
        Some(blocked) => blocked,
        None => {
            let others: Vec<String> = conversations
                .iter()
                .filter(|c| c.is_direct())
                .filter_map(|c| c.other_active(me).map(|p| p.user_id.clone()))
                .collect();
            blocked_owned = safety::users_blocked_by(&mut conn, me, &others).await?;
            &blocked_owned
        }
    };
    drop(conn);
    let mut user_ids = extras.user_ids(&refs);
    for conversation in conversations {
        user_ids.extend(conversation.active().map(|p| p.user_id.clone()));
    }
    let names = users::names(pool, identity, user_ids).await;
    let last_by_conversation: HashMap<&str, &MessageRow> = last_messages
        .iter()
        .map(|row| (row.conversation_id.as_str(), row))
        .collect();
    let mut items = Vec::with_capacity(conversations.len());
    for conversation in conversations {
        let Some(current) = conversation.active_participant(me) else {
            continue;
        };
        let other_user = if conversation.is_direct() {
            match conversation.other_active(me) {
                Some(other) => Some(other.user_id.clone()),
                None => continue,
            }
        } else {
            None
        };
        let active: Vec<&ParticipantRow> = conversation.active().collect();
        items.push(DmConversationItem {
            id: conversation.id().to_string(),
            conversation_type: conversation.row.conversation_type.clone(),
            thread_type: conversation.row.conversation_type.clone(),
            title: conversation.row.title.clone(),
            display_name: conversation_display_name(conversation, me, &names),
            other_user: other_user.as_deref().map(|id| names.item(id)),
            participants: active.iter().map(|p| participant_item(p, &names)).collect(),
            participant_count: active.len() as i64,
            last_message: last_by_conversation
                .get(conversation.id())
                .map(|row| extras.render(row, conversation, Some(current), &names)),
            unread_count: unread.get(conversation.id()).copied().unwrap_or(0),
            last_read_message_id: current.last_read_message_id.clone(),
            muted_at: iso_opt(current.muted_at),
            created_by_id: conversation.row.created_by_id.clone(),
            created_at: iso(conversation.row.created_at),
            updated_at: iso(conversation.row.updated_at),
            peer_blocked: other_user.as_ref().is_some_and(|id| blocked.contains(id)),
        });
    }
    Ok(items)
}

/// One conversation (`serialize_conversation`); 404 when the caller is not in it.
pub async fn render_conversation(
    pool: &PgPool,
    identity: &Identity,
    conversation: &Conversation,
) -> ApiResult<DmConversationItem> {
    render_conversations(pool, identity, std::slice::from_ref(conversation), None)
        .await?
        .pop()
        .ok_or_else(ApiError::thread_not_found)
}

/// `read_state.unread_count` for the caller's conversations. miy: messages the
/// author deleted are not counted.
pub async fn unread_counts(
    conn: &mut PgConnection,
    user_id: &str,
    conversation_ids: &[String],
) -> ApiResult<HashMap<String, i64>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT p.conversation_id, count(m.id) FROM dm_conversation_participants p \
         LEFT JOIN dm_messages lr ON lr.id = p.last_read_message_id \
         JOIN dm_messages m ON m.conversation_id = p.conversation_id AND m.created_at >= p.joined_at \
           AND m.sender_id <> p.user_id AND m.sequence > coalesce(lr.sequence, 0) AND m.deleted_at IS NULL \
         WHERE p.user_id = $1 AND p.left_at IS NULL AND p.conversation_id = ANY($2) \
         GROUP BY p.conversation_id",
    )
    .bind(user_id)
    .bind(conversation_ids)
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::ConversationRow;
    use crate::users::Profile;
    use chrono::NaiveDateTime;

    fn at(seconds: i64) -> NaiveDateTime {
        chrono::DateTime::from_timestamp(1_700_000_000 + seconds, 0)
            .unwrap()
            .naive_utc()
    }

    fn participant(user: &str, joined: i64, last_read: Option<i32>, left: bool) -> ParticipantRow {
        ParticipantRow {
            id: format!("p-{user}"),
            conversation_id: "c1".into(),
            user_id: user.into(),
            role: "member".into(),
            joined_at: at(joined),
            left_at: left.then(|| at(100)),
            muted_at: None,
            last_read_message_id: last_read.map(|seq| format!("m{seq}")),
            last_read_sequence: last_read,
            created_at: at(joined),
        }
    }

    fn conversation(
        kind: &str,
        title: Option<&str>,
        participants: Vec<ParticipantRow>,
    ) -> Conversation {
        Conversation {
            row: ConversationRow {
                id: "c1".into(),
                conversation_type: kind.into(),
                direct_key: None,
                title: title.map(str::to_string),
                created_by_id: "a".into(),
                message_seq: 0,
                created_at: at(0),
                updated_at: at(0),
            },
            participants,
        }
    }

    fn message(sequence: i32, sender: &str, created: i64) -> MessageRow {
        MessageRow {
            id: format!("m{sequence}"),
            conversation_id: "c1".into(),
            sequence,
            sender_id: sender.into(),
            reply_to_message_id: None,
            body: "hello".into(),
            created_at: at(created),
            moderation_hidden_at: None,
            moderation_hidden_by_id: None,
            edited_at: None,
            deleted_at: None,
        }
    }

    fn names() -> Names {
        Names::from_profiles(
            ["a", "b", "c", "d", "e"]
                .iter()
                .map(|id| Profile {
                    id: id.to_string(),
                    name: id.to_uppercase(),
                    primary_group_name: None,
                    active: true,
                })
                .collect(),
        )
    }

    // test_dm_read_state.py: message_read_state counts active participants who
    // joined before the message and have not read it.
    #[test]
    fn read_state_counts_unread_active_participants() {
        let conv = conversation(
            "group",
            None,
            vec![
                participant("a", 0, Some(2), false),
                participant("b", 0, Some(1), false),
                participant("c", 50, None, false),
                participant("d", 0, None, true),
            ],
        );
        let state = message_read_state(&message(2, "a", 10), &conv);
        assert_eq!(
            state,
            DmMessageReadStateItem {
                unread_count: 1,
                read_by_all: false
            }
        );
        let state = message_read_state(&message(1, "b", 5), &conv);
        assert_eq!(
            state,
            DmMessageReadStateItem {
                unread_count: 0,
                read_by_all: true
            }
        );
    }

    // test_dm_conversation_display.py
    #[test]
    fn display_names_follow_ai_do_rules() {
        let names = names();
        let direct = conversation(
            "direct",
            None,
            vec![
                participant("a", 0, None, false),
                participant("b", 0, None, false),
            ],
        );
        assert_eq!(conversation_display_name(&direct, "a", &names), "B");
        let lonely = conversation("direct", None, vec![participant("a", 0, None, false)]);
        assert_eq!(conversation_display_name(&lonely, "a", &names), "DM");
        let titled = conversation("group", Some("팀"), vec![participant("a", 0, None, false)]);
        assert_eq!(conversation_display_name(&titled, "a", &names), "팀");
        let group = conversation(
            "group",
            None,
            ["a", "b", "c", "d", "e"]
                .iter()
                .map(|u| participant(u, 0, None, false))
                .collect(),
        );
        assert_eq!(conversation_display_name(&group, "a", &names), "B, C, D");
    }

    // test_dm_serialization.py: hidden messages show the placeholder and no
    // attachments; reply previews are clipped to 120 characters.
    #[test]
    fn hidden_and_deleted_messages_hide_content() {
        let conv = conversation(
            "direct",
            None,
            vec![
                participant("a", 0, None, false),
                participant("b", 0, None, false),
            ],
        );
        let mut hidden = message(1, "a", 1);
        hidden.moderation_hidden_at = Some(at(2));
        let extras = MessageExtras::default();
        let item = extras.render(&hidden, &conv, None, &names());
        assert_eq!(item.body, HIDDEN_MESSAGE_PLACEHOLDER);
        assert!(item.hidden && !item.deleted);
        let mut deleted = message(2, "a", 1);
        deleted.deleted_at = Some(at(3));
        let item = extras.render(&deleted, &conv, None, &names());
        assert_eq!(item.body, "");
        assert!(item.deleted);
    }

    #[test]
    fn reply_previews_are_clipped_and_hidden_for_late_joiners() {
        let conv = conversation(
            "group",
            None,
            vec![
                participant("a", 0, None, false),
                participant("b", 20, None, false),
            ],
        );
        let mut original = message(1, "a", 1);
        original.body = "가".repeat(200);
        let mut reply = message(2, "a", 30);
        reply.reply_to_message_id = Some("m1".into());
        let mut extras = MessageExtras::default();
        extras.replies.insert("m1".into(), original);
        let item = extras.render(&reply, &conv, conv.active_participant("a"), &names());
        let preview = item.reply_to.unwrap().body_preview;
        assert_eq!(preview.chars().count(), 122);
        assert!(preview.ends_with("..."));
        let late = extras.render(&reply, &conv, conv.active_participant("b"), &names());
        assert!(late.reply_to.is_none());
    }
}
