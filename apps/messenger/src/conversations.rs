//! Conversations: AI-DO domains/dm/conversation_application.py,
//! conversation_membership.py, conversation_lifecycle.py,
//! conversation_settings.py, conversation_read_receipts.py and the
//! `/dm/users` search (user_directory.py).
use crate::{
    error::{ApiError, ApiJson, ApiResult, check_length, is_unique_violation},
    live, notify,
    request::{
        DM_CONVERSATION_TITLE_MAX_LENGTH, DM_PARTICIPANT_IDS_MAX_LENGTH,
        normalize_optional_route_id, normalize_optional_title, normalize_route_id_list, route_id,
        unique_ids,
    },
    safety, safety_admin,
    schemas::{
        DmAddParticipantsRequest, DmConversationItem, DmConversationListResponse,
        DmCreateConversationRequest, DmMuteRequest, DmSafetyCapability,
        DmUpdateConversationRequest, DmUserItem,
    },
    store::{self, Conversation, new_id},
    time::now_utc,
    ugc, users, views,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use serde::Deserialize;
use sqlx::{PgConnection, PgPool};

fn recipient_not_found() -> ApiError {
    ApiError::not_found("dm.recipient_not_found", "DM recipient not found.")
}

fn recipients_not_found() -> ApiError {
    ApiError::not_found(
        "dm.recipients_not_found",
        "Some DM recipients were not found.",
    )
}

#[derive(Debug, Deserialize)]
pub struct UserSearchQuery {
    #[serde(default)]
    include_current: Option<String>,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    limit: Option<String>,
}

fn query_bool(name: &str, value: Option<&str>) -> ApiResult<bool> {
    match value.map(|v| v.to_ascii_lowercase()) {
        None => Ok(false),
        Some(v) if matches!(v.as_str(), "true" | "1" | "yes" | "on" | "t" | "y") => Ok(true),
        Some(v) if matches!(v.as_str(), "false" | "0" | "no" | "off" | "f" | "n") => Ok(false),
        Some(_) => Err(ApiError::validation(format!(
            "{name}: Input should be a valid boolean"
        ))),
    }
}

pub fn query_limit(value: Option<&str>, default: u32) -> ApiResult<u32> {
    match value {
        None => Ok(default),
        Some(raw) => match raw.trim().parse::<i64>() {
            Ok(limit) if (1..=100).contains(&limit) => Ok(limit as u32),
            _ => Err(ApiError::validation(
                "limit: Input should be an integer from 1 to 100",
            )),
        },
    }
}

/// `GET /v1/dm/users`.
#[utoipa::path(
    get, path = "/v1/dm/users", operation_id = "searchDmUsers",
    params(
        ("q" = Option<String>, Query, description = "Directory name search"),
        ("limit" = Option<i64>, Query, minimum = 1, maximum = 100, description = "Maximum user count (default 30)"),
        ("include_current" = Option<bool>, Query, description = "Include the signed caller (default false)"),
    ),
    responses(
        (status = 200, description = "Directory matches", body = Vec<DmUserItem>),
        (status = 400, description = "Malformed query parameters", body = String, content_type = "text/plain"),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 422, description = "Invalid search parameters", body = crate::error::ErrorResponse),
        (status = 502, description = "Directory request failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn search_dm_users(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Query(query): Query<UserSearchQuery>,
) -> ApiResult<Json<Vec<DmUserItem>>> {
    let include_current = query_bool("include_current", query.include_current.as_deref())?;
    let limit = query_limit(query.limit.as_deref(), 30)?;
    Ok(Json(
        users::search(
            &pool,
            &identity,
            query.q.as_deref().unwrap_or(""),
            limit,
            include_current,
        )
        .await?,
    ))
}

/// Messenger profile of the verified caller and their app manager status.
#[utoipa::path(
    get, path = "/v1/dm/me", operation_id = "getMessengerMe",
    responses(
        (status = 200, description = "Success", body = DmMe),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn get_dm_me(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<DmMe>> {
    let me = identity.user_id.clone();
    let names = users::names(&pool, &identity, [me.clone()]).await;
    let mut conn = pool.acquire().await?;
    let manager = safety_admin::is_manager(&mut conn, &me).await?;
    Ok(Json(DmMe {
        user: names.item(&me),
        manager,
    }))
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct DmMe {
    user: DmUserItem,
    manager: bool,
}

/// `GET /v1/dm/conversations`.
#[utoipa::path(
    get, path = "/v1/dm/conversations", operation_id = "listDmConversations",
    responses(
        (status = 200, description = "Success", body = DmConversationListResponse),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn list_dm_conversations(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<DmConversationListResponse>> {
    let me = identity.user_id.as_str();
    let mut conn = pool.acquire().await?;
    let conversations = store::list_user_conversations(&mut conn, me).await?;
    let blocked = safety::blocked_user_ids(&mut conn, me).await?;
    let terms = ugc::terms_status(&mut conn, me).await?;
    let manager_status = safety_admin::load_manager_status(&mut conn, me).await?;
    drop(conn);
    let items =
        views::render_conversations(&pool, &identity, &conversations, Some(&blocked)).await?;
    Ok(Json(DmConversationListResponse {
        items,
        safety: Some(DmSafetyCapability {
            version: safety::SAFETY_VERSION,
            report: true,
            block: true,
            report_categories: safety::REPORT_CATEGORIES.to_vec(),
            report_note_max_length: safety::REPORT_NOTE_MAX_LENGTH,
            terms: Some(terms),
            manager: manager_status.manager,
            manager_claimable: manager_status.claimable,
        }),
    }))
}

fn validate_create(
    request: DmCreateConversationRequest,
) -> ApiResult<(Option<String>, Vec<String>, Option<String>)> {
    let recipient = normalize_optional_route_id(request.recipient_user_id)?;
    let participants = normalize_route_id_list(request.participant_user_ids, true)?;
    if participants.len() > DM_PARTICIPANT_IDS_MAX_LENGTH {
        return Err(ApiError::validation(
            "participant_user_ids: List should have at most 50 items",
        ));
    }
    let title = normalize_optional_title(request.title);
    if let Some(title) = &title {
        check_length("title", title, 0, DM_CONVERSATION_TITLE_MAX_LENGTH)?;
    }
    if recipient.is_some() && !participants.is_empty() {
        return Err(ApiError::validation(
            "recipient_user_id and participant_user_ids are mutually exclusive",
        ));
    }
    if recipient.is_none() && participants.is_empty() {
        return Err(ApiError::validation(
            "recipient_user_id or participant_user_ids is required",
        ));
    }
    Ok((recipient, participants, title))
}

async fn insert_participant(
    conn: &mut PgConnection,
    conversation_id: &str,
    user_id: &str,
    role: &str,
    joined_at: NaiveDateTime,
    last_read_message_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO dm_conversation_participants (id, conversation_id, user_id, role, joined_at, \
           last_read_message_id, created_at) VALUES ($1, $2, $3, $4, $5, $6, $5)",
    )
    .bind(new_id())
    .bind(conversation_id)
    .bind(user_id)
    .bind(role)
    .bind(joined_at)
    .bind(last_read_message_id)
    .execute(conn)
    .await?;
    Ok(())
}

pub fn direct_conversation_key(left: &str, right: &str) -> String {
    let mut ids = [left, right];
    ids.sort();
    ids.join(":")
}

/// `conversation_membership.get_or_create_direct_conversation`.
async fn get_or_create_direct(
    pool: &PgPool,
    identity: &Identity,
    recipient_id: &str,
) -> ApiResult<String> {
    let me = identity.user_id.as_str();
    if recipient_id == me {
        return Err(recipient_not_found());
    }
    let active = users::active_users(pool, identity, &[recipient_id.to_string()]).await?;
    if !active.contains_key(recipient_id) {
        return Err(recipient_not_found());
    }
    let direct_key = direct_conversation_key(me, recipient_id);
    let mut tx = pool.begin().await?;
    let existing: Option<String> =
        sqlx::query_scalar("SELECT id FROM dm_conversations WHERE direct_key = $1 FOR UPDATE")
            .bind(&direct_key)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(id) = existing {
        // `_restore_direct_conversation`: both present stays as is even when
        // blocked; restoring a missing participant checks the block.
        let conversation = store::load_conversation(&mut tx, &id, false)
            .await?
            .ok_or_else(ApiError::thread_not_found)?;
        let missing: Vec<&str> = [me, recipient_id]
            .into_iter()
            .filter(|user| conversation.active_participant(user).is_none())
            .collect();
        if !missing.is_empty() {
            safety::ensure_not_blocked(&mut tx, me, recipient_id).await?;
            let latest = store::latest_message_id(&mut tx, &id).await?;
            let now = now_utc();
            for user in missing {
                insert_participant(&mut tx, &id, user, "member", now, latest.as_deref()).await?;
            }
        }
        tx.commit().await?;
        return Ok(id);
    }
    safety::ensure_not_blocked(&mut tx, me, recipient_id).await?;
    let id = new_id();
    let now = now_utc();
    let inserted = sqlx::query(
        "INSERT INTO dm_conversations (id, conversation_type, direct_key, title, created_by_id, message_seq, \
           created_at, updated_at) VALUES ($1, 'direct', $2, NULL, $3, 0, $4, $4)",
    )
    .bind(&id)
    .bind(&direct_key)
    .bind(me)
    .bind(now)
    .execute(&mut *tx)
    .await;
    match inserted {
        Ok(_) => {}
        Err(error) if is_unique_violation(&error) => {
            // The same 1:1 was created concurrently: answer the first one.
            drop(tx);
            let existing: Option<String> =
                sqlx::query_scalar("SELECT id FROM dm_conversations WHERE direct_key = $1")
                    .bind(&direct_key)
                    .fetch_optional(pool)
                    .await?;
            return existing.ok_or_else(ApiError::internal);
        }
        Err(error) => return Err(error.into()),
    }
    insert_participant(&mut tx, &id, me, "member", now, None).await?;
    insert_participant(&mut tx, &id, recipient_id, "member", now, None).await?;
    tx.commit().await?;
    Ok(id)
}

/// `conversation_membership.create_group_conversation`.
async fn create_group(
    pool: &PgPool,
    identity: &Identity,
    participant_ids: &[String],
    title: Option<String>,
) -> ApiResult<String> {
    let me = identity.user_id.as_str();
    let mut conn = pool.acquire().await?;
    if let Some(title) = &title {
        ugc::ensure_user_content_allowed(&mut conn, me, &[title]).await?;
    }
    let recipients = unique_ids(participant_ids.iter().filter(|id| *id != me));
    if recipients.len() < 2 {
        return Err(ApiError::unprocessable(
            "dm.group_participants_required",
            "A group conversation needs at least two other participants.",
        ));
    }
    drop(conn);
    let active = users::active_users(pool, identity, &recipients).await?;
    if active.len() != recipients.len() {
        return Err(recipients_not_found());
    }
    let mut tx = pool.begin().await?;
    safety::ensure_no_blocked_invitees(&mut tx, me, &recipients).await?;
    let id = new_id();
    let now = now_utc();
    sqlx::query(
        "INSERT INTO dm_conversations (id, conversation_type, direct_key, title, created_by_id, message_seq, \
           created_at, updated_at) VALUES ($1, 'group', NULL, $2, $3, 0, $4, $4)",
    )
    .bind(&id)
    .bind(&title)
    .bind(me)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    insert_participant(&mut tx, &id, me, "owner", now, None).await?;
    for user in &recipients {
        insert_participant(&mut tx, &id, user, "member", now, None).await?;
    }
    tx.commit().await?;
    Ok(id)
}

async fn render_for_me(
    pool: &PgPool,
    identity: &Identity,
    id: &str,
) -> ApiResult<DmConversationItem> {
    let mut conn = pool.acquire().await?;
    let conversation =
        store::require_user_conversation(&mut conn, &identity.user_id, id, false).await?;
    drop(conn);
    views::render_conversation(pool, identity, &conversation).await
}

/// `POST /v1/dm/conversations`.
#[utoipa::path(
    post, path = "/v1/dm/conversations", operation_id = "createDmConversation",
    request_body = DmCreateConversationRequest,
    responses(
        (status = 201, description = "Created", body = DmConversationItem),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation or domain policy rejected the operation", body = crate::error::ErrorResponse),
        (status = 502, description = "A required upstream service failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn create_dm_conversation(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    ApiJson(request): ApiJson<DmCreateConversationRequest>,
) -> ApiResult<(StatusCode, Json<DmConversationItem>)> {
    let (recipient, participants, title) = validate_create(request)?;
    let id = if !participants.is_empty() {
        create_group(&pool, &identity, &participants, title).await?
    } else if let Some(recipient) = recipient {
        get_or_create_direct(&pool, &identity, &recipient).await?
    } else {
        return Err(recipient_not_found());
    };
    live::conversation_changed(&pool, &id, &[]).await;
    Ok((
        StatusCode::CREATED,
        Json(render_for_me(&pool, &identity, &id).await?),
    ))
}

/// `PATCH /v1/dm/conversations/{id}`: `conversation_settings.update_conversation`.
#[utoipa::path(
    patch, path = "/v1/dm/conversations/{conversation_id}", operation_id = "updateDmConversation",
    params(
        ("conversation_id" = String, Path),
    ),
    request_body = DmUpdateConversationRequest,
    responses(
        (status = 200, description = "Success", body = DmConversationItem),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation or domain policy rejected the operation", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn update_dm_conversation(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(conversation_id): Path<String>,
    ApiJson(request): ApiJson<DmUpdateConversationRequest>,
) -> ApiResult<Json<DmConversationItem>> {
    route_id(&conversation_id)?;
    let title = normalize_optional_title(request.title);
    if let Some(title) = &title {
        check_length("title", title, 0, DM_CONVERSATION_TITLE_MAX_LENGTH)?;
    }
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    let conversation =
        store::require_user_conversation(&mut tx, me, &conversation_id, true).await?;
    conversation.ensure_group()?;
    conversation.ensure_manager(me)?;
    if let Some(title) = &title {
        ugc::ensure_user_content_allowed(&mut tx, me, &[title]).await?;
    }
    sqlx::query("UPDATE dm_conversations SET title = $2, updated_at = $3 WHERE id = $1")
        .bind(&conversation_id)
        .bind(&title)
        .bind(now_utc())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    live::conversation_changed(&pool, &conversation_id, &[]).await;
    Ok(Json(
        render_for_me(&pool, &identity, &conversation_id).await?,
    ))
}

/// `POST /v1/dm/conversations/{id}/participants`: any participant of a group
/// may invite (AI-DO `add_participants`).
#[utoipa::path(
    post, path = "/v1/dm/conversations/{conversation_id}/participants", operation_id = "addDmConversationParticipants",
    params(
        ("conversation_id" = String, Path),
    ),
    request_body = DmAddParticipantsRequest,
    responses(
        (status = 200, description = "Success", body = DmConversationItem),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation or domain policy rejected the operation", body = crate::error::ErrorResponse),
        (status = 502, description = "A required upstream service failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn add_dm_conversation_participants(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(conversation_id): Path<String>,
    ApiJson(request): ApiJson<DmAddParticipantsRequest>,
) -> ApiResult<Json<DmConversationItem>> {
    route_id(&conversation_id)?;
    let user_ids = normalize_route_id_list(request.user_ids, false)?;
    if user_ids.is_empty() {
        return Err(ApiError::validation(
            "user_ids: List should have at least 1 item",
        ));
    }
    if user_ids.len() > DM_PARTICIPANT_IDS_MAX_LENGTH {
        return Err(ApiError::validation(
            "user_ids: List should have at most 50 items",
        ));
    }
    let me = identity.user_id.as_str();
    let mut conn = pool.acquire().await?;
    let conversation =
        store::require_user_conversation(&mut conn, me, &conversation_id, false).await?;
    conversation.ensure_group()?;
    drop(conn);
    let active_ids: Vec<&str> = conversation.active().map(|p| p.user_id.as_str()).collect();
    let next_ids = unique_ids(
        user_ids
            .iter()
            .filter(|id| !active_ids.contains(&id.as_str())),
    );
    if next_ids.is_empty() {
        return Ok(Json(
            views::render_conversation(&pool, &identity, &conversation).await?,
        ));
    }
    let active = users::active_users(&pool, &identity, &next_ids).await?;
    if active.len() != next_ids.len() {
        return Err(recipients_not_found());
    }
    let mut tx = pool.begin().await?;
    let conversation =
        store::require_user_conversation(&mut tx, me, &conversation_id, true).await?;
    safety::ensure_no_blocked_invitees(&mut tx, me, &next_ids).await?;
    let latest = store::latest_message_id(&mut tx, &conversation_id).await?;
    let now = now_utc();
    for user in &next_ids {
        if conversation.active_participant(user).is_some() {
            continue;
        }
        insert_participant(
            &mut tx,
            &conversation_id,
            user,
            "member",
            now,
            latest.as_deref(),
        )
        .await?;
    }
    sqlx::query("UPDATE dm_conversations SET updated_at = $2 WHERE id = $1")
        .bind(&conversation_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    live::conversation_changed(&pool, &conversation_id, &[]).await;
    Ok(Json(
        render_for_me(&pool, &identity, &conversation_id).await?,
    ))
}

/// `DmParticipantDeparture.apply` with `promote_owner_if_needed`.
async fn depart(
    conn: &mut PgConnection,
    conversation: &Conversation,
    user_id: &str,
) -> ApiResult<()> {
    let participant = conversation
        .active_participant(user_id)
        .ok_or_else(ApiError::thread_not_found)?;
    let now = now_utc();
    sqlx::query("UPDATE dm_conversation_participants SET left_at = $2 WHERE id = $1")
        .bind(&participant.id)
        .bind(now)
        .execute(&mut *conn)
        .await?;
    sqlx::query("UPDATE dm_conversations SET updated_at = $2 WHERE id = $1")
        .bind(conversation.id())
        .bind(now)
        .execute(&mut *conn)
        .await?;
    if participant.role == "owner" {
        let next_owner = conversation
            .active()
            .filter(|p| p.user_id != user_id)
            .min_by_key(|p| p.joined_at);
        if let Some(next_owner) = next_owner {
            sqlx::query("UPDATE dm_conversation_participants SET role = 'owner' WHERE id = $1")
                .bind(&next_owner.id)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}

async fn leave(pool: &PgPool, identity: &Identity, conversation_id: &str) -> ApiResult<()> {
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    let conversation = store::require_user_conversation(&mut tx, me, conversation_id, true).await?;
    conversation.ensure_group()?;
    depart(&mut tx, &conversation, me).await?;
    tx.commit().await?;
    live::conversation_changed(pool, conversation_id, &[me.to_string()]).await;
    notify::retract(identity, conversation_id, Some(&[me.to_string()])).await;
    notify::mentions_of_departed(pool, identity, conversation_id, me).await;
    Ok(())
}

/// `DELETE /v1/dm/conversations/{id}/participants/me`.
#[utoipa::path(
    delete, path = "/v1/dm/conversations/{conversation_id}/participants/me", operation_id = "leaveDmConversation",
    params(
        ("conversation_id" = String, Path),
    ),
    responses(
        (status = 204, description = "No content"),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation or domain policy rejected the operation", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn leave_dm_conversation(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(conversation_id): Path<String>,
) -> ApiResult<StatusCode> {
    route_id(&conversation_id)?;
    leave(&pool, &identity, &conversation_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /v1/dm/conversations/{id}/participants/{user_id}`.
#[utoipa::path(
    delete, path = "/v1/dm/conversations/{conversation_id}/participants/{user_id}", operation_id = "removeDmConversationParticipant",
    params(
        ("conversation_id" = String, Path),
        ("user_id" = String, Path),
    ),
    responses(
        (status = 200, description = "Success", body = DmConversationItem),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation or domain policy rejected the operation", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn remove_dm_conversation_participant(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path((conversation_id, user_id)): Path<(String, String)>,
) -> ApiResult<Json<DmConversationItem>> {
    route_id(&conversation_id)?;
    route_id(&user_id)?;
    let me = identity.user_id.as_str();
    if user_id == me {
        // AI-DO answers the conversation as loaded before leaving.
        let mut conn = pool.acquire().await?;
        let conversation =
            store::require_user_conversation(&mut conn, me, &conversation_id, false).await?;
        conversation.ensure_group()?;
        drop(conn);
        let item = views::render_conversation(&pool, &identity, &conversation).await?;
        leave(&pool, &identity, &conversation_id).await?;
        return Ok(Json(item));
    }
    let mut tx = pool.begin().await?;
    let conversation =
        store::require_user_conversation(&mut tx, me, &conversation_id, true).await?;
    conversation.ensure_group()?;
    conversation.ensure_manager(me)?;
    if conversation.active_participant(&user_id).is_none() {
        return Err(ApiError::not_found(
            "dm.participant_not_found",
            "DM participant not found.",
        ));
    }
    depart(&mut tx, &conversation, &user_id).await?;
    tx.commit().await?;
    live::conversation_changed(&pool, &conversation_id, std::slice::from_ref(&user_id)).await;
    notify::retract(&identity, &conversation_id, Some(&[user_id.clone()])).await;
    notify::mentions_of_departed(&pool, &identity, &conversation_id, &user_id).await;
    Ok(Json(
        render_for_me(&pool, &identity, &conversation_id).await?,
    ))
}

/// `PATCH /v1/dm/conversations/{id}/read`: read up to the latest visible
/// message, then mark this conversation's platform notification read.
#[utoipa::path(
    patch, path = "/v1/dm/conversations/{conversation_id}/read", operation_id = "markDmConversationRead",
    params(
        ("conversation_id" = String, Path),
    ),
    responses(
        (status = 200, description = "Success", body = DmConversationItem),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation or domain policy rejected the operation", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn mark_dm_conversation_read(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(conversation_id): Path<String>,
) -> ApiResult<Json<DmConversationItem>> {
    route_id(&conversation_id)?;
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    let conversation =
        store::require_user_conversation(&mut tx, me, &conversation_id, true).await?;
    let participant = conversation.require_participant(me)?.clone();
    let latest = store::latest_visible_message(&mut tx, &conversation_id, &participant).await?;
    let latest_id = latest.as_ref().map(|m| m.id.clone());
    // miy: the screen marks read on every poll while visible; only a moved
    // read position is written and passed on to the platform inbox.
    let moved = latest.is_some_and(|message| {
        participant
            .last_read_sequence
            .is_none_or(|last_read| message.sequence > last_read)
    });
    if moved {
        sqlx::query(
            "UPDATE dm_conversation_participants SET last_read_message_id = $2 WHERE id = $1",
        )
        .bind(&participant.id)
        .bind(&latest_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    if moved {
        // Senders' read markers change too.
        live::conversation_changed(&pool, &conversation_id, &[]).await;
        notify::mark_read(&identity, &conversation_id).await;
    }
    Ok(Json(
        render_for_me(&pool, &identity, &conversation_id).await?,
    ))
}

/// miy: `PUT /v1/dm/conversations/{id}/mute` sets the caller's `muted_at`
/// (AI-DO kept the column for its commute-app relay). Muted conversations
/// publish no platform notifications to that person.
#[utoipa::path(
    put, path = "/v1/dm/conversations/{conversation_id}/mute", operation_id = "muteDmConversation",
    params(
        ("conversation_id" = String, Path),
    ),
    request_body = DmMuteRequest,
    responses(
        (status = 200, description = "Success", body = DmConversationItem),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation or domain policy rejected the operation", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
pub async fn mute_dm_conversation(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(conversation_id): Path<String>,
    ApiJson(request): ApiJson<DmMuteRequest>,
) -> ApiResult<Json<DmConversationItem>> {
    route_id(&conversation_id)?;
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    let conversation =
        store::require_user_conversation(&mut tx, me, &conversation_id, true).await?;
    let participant = conversation.require_participant(me)?;
    let changed = participant.muted_at.is_some() != request.muted;
    if changed {
        sqlx::query("UPDATE dm_conversation_participants SET muted_at = $2 WHERE id = $1")
            .bind(&participant.id)
            .bind(request.muted.then(now_utc))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    if changed {
        live::user_changed(me, &conversation_id);
    }
    if changed && request.muted {
        notify::retract(&identity, &conversation_id, Some(&[me.to_string()])).await;
    }
    Ok(Json(
        render_for_me(&pool, &identity, &conversation_id).await?,
    ))
}

#[utoipa::path(
    get, path = "/v1/dm/threads", operation_id = "legacyListDmThreads",
    description = "Deprecated alias for GET /v1/dm/conversations.",
    responses(
        (status = 200, description = "Success", body = DmConversationListResponse),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
#[deprecated(note = "Use the /v1/dm/conversations operation")]
pub async fn legacy_list_dm_threads(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<DmConversationListResponse>> {
    list_dm_conversations(State(pool), Extension(identity)).await
}

#[utoipa::path(
    post, path = "/v1/dm/threads", operation_id = "legacyCreateDmThread",
    description = "Deprecated alias for POST /v1/dm/conversations.",
    request_body = DmCreateConversationRequest,
    responses(
        (status = 201, description = "Created", body = DmConversationItem),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation or domain policy rejected the operation", body = crate::error::ErrorResponse),
        (status = 502, description = "A required upstream service failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
#[deprecated(note = "Use the /v1/dm/conversations operation")]
pub async fn legacy_create_dm_thread(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    ApiJson(request): ApiJson<DmCreateConversationRequest>,
) -> ApiResult<(StatusCode, Json<DmConversationItem>)> {
    create_dm_conversation(State(pool), Extension(identity), ApiJson(request)).await
}

#[utoipa::path(
    patch, path = "/v1/dm/threads/{conversation_id}/read", operation_id = "legacyMarkDmThreadRead",
    description = "Deprecated alias for PATCH /v1/dm/conversations/{conversation_id}/read.",
    params(
        ("conversation_id" = String, Path),
    ),
    responses(
        (status = 200, description = "Success", body = DmConversationItem),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation or domain policy rejected the operation", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse),
    )
)]
#[deprecated(note = "Use the /v1/dm/conversations operation")]
pub async fn legacy_mark_dm_thread_read(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(conversation_id): Path<String>,
) -> ApiResult<Json<DmConversationItem>> {
    mark_dm_conversation_read(State(pool), Extension(identity), Path(conversation_id)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_keys_are_order_independent() {
        assert_eq!(direct_conversation_key("b", "a"), "a:b");
        assert_eq!(direct_conversation_key("a", "b"), "a:b");
    }

    // test_dm_schemas.py / test_dm.py request validation
    #[test]
    fn create_requests_need_exactly_one_recipient_shape() {
        let both = DmCreateConversationRequest {
            recipient_user_id: Some("u1".into()),
            participant_user_ids: Some(vec!["u2".into()]),
            title: None,
        };
        assert!(validate_create(both).is_err());
        let none = DmCreateConversationRequest {
            recipient_user_id: None,
            participant_user_ids: None,
            title: Some("  ".into()),
        };
        assert!(validate_create(none).is_err());
        let group = DmCreateConversationRequest {
            recipient_user_id: None,
            participant_user_ids: Some(vec![" u1 ".into(), "u2".into()]),
            title: Some("  팀  ".into()),
        };
        let (_, ids, title) = validate_create(group).unwrap();
        assert_eq!(ids, vec!["u1", "u2"]);
        assert_eq!(title.as_deref(), Some("팀"));
        let long = DmCreateConversationRequest {
            recipient_user_id: None,
            participant_user_ids: Some(vec!["u1".into(), "u2".into()]),
            title: Some("가".repeat(141)),
        };
        assert!(validate_create(long).is_err());
    }

    #[test]
    fn query_values_follow_fastapi() {
        assert!(query_bool("x", Some("true")).unwrap());
        assert!(!query_bool("x", None).unwrap());
        assert!(query_bool("x", Some("maybe")).is_err());
        assert_eq!(query_limit(None, 30).unwrap(), 30);
        assert!(query_limit(Some("0"), 30).is_err());
        assert!(query_limit(Some("101"), 30).is_err());
    }
}
