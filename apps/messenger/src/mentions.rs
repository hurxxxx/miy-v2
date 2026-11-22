//! Mentions (COMMON-FEATURES-PLAN §2.2 "보강"; AI-DO 0002af062 had none).
//!
//! A message carries structured mention ranges: the user ID and the range of
//! the stored (trimmed) body, in UTF-16 code units as the browser counts. A
//! range starts with "@" and names a current active participant of the
//! conversation when the message is written. The body text stays the source
//! of what people read; the ranges only mark it.
use crate::{
    error::{ApiError, ApiResult},
    request::is_dm_route_id,
    store::Conversation,
};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use std::collections::HashSet;

pub const MAX_MENTIONS: usize = 50;
const MAX_MENTION_LENGTH: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, sqlx::FromRow)]
pub struct Mention {
    pub user_id: String,
    #[sqlx(rename = "start_index")]
    pub start: i32,
    pub length: i32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MentionRow {
    pub message_id: String,
    pub user_id: String,
    pub start_index: i32,
    pub length: i32,
}

fn invalid(reason: &str) -> ApiError {
    ApiError::unprocessable(
        "dm.mention_invalid",
        format!("Mentions do not match the message: {reason}."),
    )
}

/// Check the ranges against `body` and the conversation's current active
/// participants; answer them sorted by position.
pub fn validate(
    body: &str,
    mentions: Vec<Mention>,
    conversation: &Conversation,
) -> ApiResult<Vec<Mention>> {
    if mentions.len() > MAX_MENTIONS {
        return Err(invalid("too many mentions"));
    }
    let units: Vec<u16> = body.encode_utf16().collect();
    let mut sorted = mentions;
    sorted.sort_by_key(|mention| mention.start);
    let mut end = 0usize;
    for mention in &sorted {
        if !is_dm_route_id(&mention.user_id) || mention.start < 0 || mention.length < 2 {
            return Err(invalid("bad range"));
        }
        let start = mention.start as usize;
        let length = mention.length as usize;
        if length > MAX_MENTION_LENGTH || start < end || start + length > units.len() {
            return Err(invalid("bad range"));
        }
        let text =
            String::from_utf16(&units[start..start + length]).map_err(|_| invalid("bad range"))?;
        if !text.starts_with('@') || text.contains('\n') {
            return Err(invalid("not a mention"));
        }
        if conversation.active_participant(&mention.user_id).is_none() {
            return Err(invalid("not a participant"));
        }
        end = start + length;
    }
    Ok(sorted)
}

/// People named by `mentions`, once each.
pub fn users(mentions: &[Mention]) -> Vec<String> {
    let mut seen = HashSet::new();
    mentions
        .iter()
        .filter(|mention| seen.insert(mention.user_id.clone()))
        .map(|mention| mention.user_id.clone())
        .collect()
}

pub async fn replace(
    conn: &mut PgConnection,
    message_id: &str,
    mentions: &[Mention],
) -> ApiResult<()> {
    sqlx::query("DELETE FROM dm_message_mentions WHERE message_id = $1")
        .bind(message_id)
        .execute(&mut *conn)
        .await?;
    if mentions.is_empty() {
        return Ok(());
    }
    let users: Vec<&str> = mentions.iter().map(|m| m.user_id.as_str()).collect();
    let starts: Vec<i32> = mentions.iter().map(|m| m.start).collect();
    let lengths: Vec<i32> = mentions.iter().map(|m| m.length).collect();
    sqlx::query(
        "INSERT INTO dm_message_mentions (message_id, user_id, start_index, length) \
         SELECT $1, u, s, l FROM unnest($2::text[], $3::int[], $4::int[]) AS t(u, s, l)",
    )
    .bind(message_id)
    .bind(&users)
    .bind(&starts)
    .bind(&lengths)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn of_message(conn: &mut PgConnection, message_id: &str) -> ApiResult<Vec<Mention>> {
    Ok(sqlx::query_as::<_, Mention>(
        "SELECT user_id, start_index, length FROM dm_message_mentions WHERE message_id = $1 \
         ORDER BY start_index",
    )
    .bind(message_id)
    .fetch_all(conn)
    .await?)
}

pub async fn of_messages(conn: &mut PgConnection, ids: &[String]) -> ApiResult<Vec<MentionRow>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_as::<_, MentionRow>(
        "SELECT message_id, user_id, start_index, length FROM dm_message_mentions \
         WHERE message_id = ANY($1) ORDER BY message_id, start_index",
    )
    .bind(ids)
    .fetch_all(conn)
    .await?)
}

/// Messages of `conversation_id` that mention `user_id` (newest first, bounded),
/// to retract their mention notifications when the person leaves.
pub async fn messages_mentioning(
    conn: &mut PgConnection,
    conversation_id: &str,
    user_id: &str,
) -> ApiResult<Vec<String>> {
    let rows: Vec<(String, i32)> = sqlx::query_as(
        "SELECT DISTINCT m.id, m.sequence FROM dm_message_mentions x \
         JOIN dm_messages m ON m.id = x.message_id \
         WHERE m.conversation_id = $1 AND x.user_id = $2 ORDER BY m.sequence DESC LIMIT 200",
    )
    .bind(conversation_id)
    .bind(user_id)
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(|(id, _)| id).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{ConversationRow, ParticipantRow};

    fn conversation() -> Conversation {
        let at = chrono::DateTime::from_timestamp(1_700_000_000, 0)
            .unwrap()
            .naive_utc();
        let participant = |user: &str, left: bool| ParticipantRow {
            id: format!("p-{user}"),
            conversation_id: "c1".into(),
            user_id: user.into(),
            role: "member".into(),
            joined_at: at,
            left_at: left.then_some(at),
            muted_at: None,
            last_read_message_id: None,
            last_read_sequence: None,
            created_at: at,
        };
        Conversation {
            row: ConversationRow {
                id: "c1".into(),
                conversation_type: "group".into(),
                direct_key: None,
                title: None,
                created_by_id: "a".into(),
                message_seq: 0,
                created_at: at,
                updated_at: at,
            },
            participants: vec![
                participant("a", false),
                participant("b", false),
                participant("gone", true),
            ],
        }
    }

    fn mention(user: &str, start: i32, length: i32) -> Mention {
        Mention {
            user_id: user.into(),
            start,
            length,
        }
    }

    #[test]
    fn ranges_count_utf16_units_and_must_name_participants() {
        let body = "😀 @김하나 와 @Bee 안녕";
        // "😀" is two UTF-16 units: "@김하나" starts at 3.
        let ok = validate(
            body,
            vec![mention("b", 10, 4), mention("a", 3, 4)],
            &conversation(),
        )
        .unwrap();
        assert_eq!(ok, vec![mention("a", 3, 4), mention("b", 10, 4)]);
        assert_eq!(users(&ok), vec!["a", "b"]);
        for bad in [
            vec![mention("a", 2, 4)],                     // not starting at "@"
            vec![mention("a", 3, 40)],                    // past the end
            vec![mention("gone", 3, 4)],                  // left the conversation
            vec![mention("a", 3, 4), mention("b", 5, 4)], // overlapping
            vec![mention("a", 0, 2)],                     // splits a surrogate pair
            vec![mention("a", -1, 4)],
        ] {
            assert!(validate(body, bad, &conversation()).is_err());
        }
        let many: Vec<Mention> = (0..51).map(|i| mention("a", i * 10, 2)).collect();
        assert!(validate(body, many, &conversation()).is_err());
    }
}
