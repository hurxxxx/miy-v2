//! Reactions (COMMON-FEATURES-PLAN §2.2 "보강"; AI-DO 0002af062 showed an
//! empty reaction button only). A small fixed set; one row per (message,
//! person, reaction); only current active participants who see the message.
use crate::{
    error::{ApiError, ApiResult},
    live,
    messages::render_one,
    request::route_id,
    schemas::{DmMessageItem, DmReactionItem},
    store,
    time::now_utc,
    users::Names,
};
use axum::{
    Extension, Json,
    extract::{Path, State},
};
use miy_app_sdk::Identity;
use sqlx::{PgConnection, PgPool};
use std::collections::HashMap;

/// The fixed set, in display order. The UI maps each key to an emoji.
pub const REACTIONS: [&str; 7] = [
    "thumbs_up",
    "heart",
    "laugh",
    "surprised",
    "sad",
    "pray",
    "check",
];

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReactionKind {
    ThumbsUp,
    Heart,
    Laugh,
    Surprised,
    Sad,
    Pray,
    Check,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReactionRow {
    pub message_id: String,
    pub user_id: String,
    pub reaction: String,
}

pub async fn of_messages(conn: &mut PgConnection, ids: &[String]) -> ApiResult<Vec<ReactionRow>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_as::<_, ReactionRow>(
        "SELECT message_id, user_id, reaction FROM dm_message_reactions \
         WHERE message_id = ANY($1) ORDER BY created_at, user_id",
    )
    .bind(ids)
    .fetch_all(conn)
    .await?)
}

/// Grouped in the fixed order with who reacted.
pub fn items(rows: &[&ReactionRow], me: &str, names: &Names) -> Vec<DmReactionItem> {
    let mut by_reaction: HashMap<&str, Vec<&str>> = HashMap::new();
    for row in rows {
        by_reaction
            .entry(row.reaction.as_str())
            .or_default()
            .push(row.user_id.as_str());
    }
    REACTIONS
        .iter()
        .filter_map(|reaction| {
            let users = by_reaction.get(reaction)?;
            Some(DmReactionItem {
                reaction: reaction.to_string(),
                count: users.len() as i64,
                reacted: users.contains(&me),
                users: users.iter().map(|id| names.item(id)).collect(),
            })
        })
        .collect()
}

async fn change(
    pool: &PgPool,
    identity: &Identity,
    conversation_id: &str,
    message_id: &str,
    reaction: &str,
    add: bool,
) -> ApiResult<DmMessageItem> {
    route_id(conversation_id)?;
    route_id(message_id)?;
    if !REACTIONS.contains(&reaction) {
        return Err(ApiError::validation("reaction: unknown reaction"));
    }
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    // Lock first, then recheck current participation and the message.
    let conversation = store::require_user_conversation(&mut tx, me, conversation_id, true).await?;
    let participant = conversation.require_participant(me)?;
    let message = store::message_by_id(&mut tx, conversation_id, message_id, false)
        .await?
        .filter(|message| message.created_at >= participant.joined_at)
        .ok_or_else(ApiError::message_not_found)?;
    if message.hidden() || message.deleted() {
        return Err(ApiError::unprocessable(
            "dm.reaction_unavailable",
            "This message cannot take reactions.",
        ));
    }
    let changed = if add {
        sqlx::query(
            "INSERT INTO dm_message_reactions (message_id, user_id, reaction, created_at) \
             VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING",
        )
        .bind(message_id)
        .bind(me)
        .bind(reaction)
        .bind(now_utc())
        .execute(&mut *tx)
        .await?
        .rows_affected()
    } else {
        sqlx::query(
            "DELETE FROM dm_message_reactions WHERE message_id = $1 AND user_id = $2 AND reaction = $3",
        )
        .bind(message_id)
        .bind(me)
        .bind(reaction)
        .execute(&mut *tx)
        .await?
        .rows_affected()
    } > 0;
    tx.commit().await?;
    if changed {
        live::conversation_changed(pool, conversation_id, &[]).await;
    }
    render_one(pool, identity, &conversation, &message).await
}

/// `PUT /v1/dm/conversations/{id}/messages/{message_id}/reactions/{reaction}`.
#[utoipa::path(
    put, path = "/v1/dm/conversations/{conversation_id}/messages/{message_id}/reactions/{reaction}", operation_id = "addReaction",
    params(
        ("conversation_id" = String, Path),
        ("message_id" = String, Path),
        ("reaction" = ReactionKind, Path),
    ),
    responses(
        (status = 200, description = "Success", body = DmMessageItem),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation or domain policy rejected the operation", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn add_reaction(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path((conversation_id, message_id, reaction)): Path<(String, String, String)>,
) -> ApiResult<Json<DmMessageItem>> {
    Ok(Json(
        change(
            &pool,
            &identity,
            &conversation_id,
            &message_id,
            &reaction,
            true,
        )
        .await?,
    ))
}

/// `DELETE /v1/dm/conversations/{id}/messages/{message_id}/reactions/{reaction}`.
#[utoipa::path(
    delete, path = "/v1/dm/conversations/{conversation_id}/messages/{message_id}/reactions/{reaction}", operation_id = "removeReaction",
    params(
        ("conversation_id" = String, Path),
        ("message_id" = String, Path),
        ("reaction" = ReactionKind, Path),
    ),
    responses(
        (status = 200, description = "Success", body = DmMessageItem),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation or domain policy rejected the operation", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn remove_reaction(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path((conversation_id, message_id, reaction)): Path<(String, String, String)>,
) -> ApiResult<Json<DmMessageItem>> {
    Ok(Json(
        change(
            &pool,
            &identity,
            &conversation_id,
            &message_id,
            &reaction,
            false,
        )
        .await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::users::Profile;

    #[test]
    fn reactions_group_in_fixed_order_with_people() {
        let row = |user: &str, reaction: &str| ReactionRow {
            message_id: "m1".into(),
            user_id: user.into(),
            reaction: reaction.into(),
        };
        let rows = [row("b", "heart"), row("a", "thumbs_up"), row("c", "heart")];
        let refs: Vec<&ReactionRow> = rows.iter().collect();
        let names = Names::from_profiles(vec![Profile {
            id: "b".into(),
            name: "비".into(),
            primary_group_name: None,
            active: true,
        }]);
        let items = items(&refs, "c", &names);
        assert_eq!(items.len(), 2);
        assert_eq!(
            (items[0].reaction.as_str(), items[0].count, items[0].reacted),
            ("thumbs_up", 1, false)
        );
        assert_eq!(
            (items[1].reaction.as_str(), items[1].count, items[1].reacted),
            ("heart", 2, true)
        );
        assert_eq!(items[1].users[0].full_name, "비");
        assert_eq!(items[1].users[1].full_name, "c");
    }
}
