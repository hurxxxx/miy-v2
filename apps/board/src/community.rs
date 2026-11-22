//! Community board: channels, posts, comments and read marks.
//!
//! Port of AI-DO domains/community/{router,service,schemas,models}.py at
//! 0002af062 (docs/PORTING.md). Rules, status codes and response shapes are
//! AI-DO's. What changed for miy:
//! - the platform administrator is the board manager (managers.rs);
//! - names come from the platform directory (people.rs);
//! - embedded images are core files behind the app (media.rs);
//! - comment notifications go to the platform notification inbox (notify.rs);
//! - posts and comments pass the banned-word filter (ugc.rs, 보강).
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, QueryBuilder, Transaction};
use std::collections::{HashMap, HashSet};

use crate::{
    AppState,
    error::{ApiError, ApiJson, ApiResult, check_length},
    managers::{is_manager, require_manager},
    media, notify, password, people,
    time::now_utc,
    ugc,
};

pub const DEFAULT_CHANNEL_KEY: &str = "suggestions";
const DEFAULT_CHANNEL_NAME: &str = "Suggestions";
const DEFAULT_CHANNEL_DESCRIPTION: &str = "Ideas, requests, and improvement threads.";
const DEFAULT_PAGE_SIZE: i64 = 10;
const MAX_PAGE_SIZE: i64 = 100;
pub const LOCKED_REASON_PASSWORD: &str = "password";
pub const LOCKED_REASON_ADMIN_ONLY: &str = "admin_only";
const LOCK_NOT_AVAILABLE_SQLSTATE: &str = "55P03";

// ---------------------------------------------------------------- models

#[derive(Clone, Debug, FromRow)]
pub struct Channel {
    pub id: String,
    pub key: String,
    pub name: String,
    pub description: String,
    pub position: i32,
    pub active: bool,
    pub read_only: bool,
    pub force_anonymous: bool,
    pub admin_only_content: bool,
    pub template_title: String,
    pub template_body: String,
}

const CHANNEL_COLUMNS: &str = "id, key, name, description, position, active, read_only, \
    force_anonymous, admin_only_content, template_title, template_body";

#[derive(Clone, Debug, FromRow)]
pub struct Post {
    pub id: String,
    pub channel_id: String,
    pub number: i32,
    pub author_id: String,
    pub is_anonymous: bool,
    pub is_secret: bool,
    pub password_hash: Option<String>,
    pub title: String,
    pub body: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

const POST_COLUMNS: &str = "id, channel_id, number, author_id, is_anonymous, is_secret, \
    password_hash, title, body, created_at, updated_at";

/// A post with its channel (`selectinload(CommunityPost.channel)`).
#[derive(Clone, Debug)]
pub struct LoadedPost {
    pub post: Post,
    pub channel: Channel,
}

#[derive(Clone, Debug, FromRow)]
pub struct Comment {
    pub id: String,
    pub post_id: String,
    pub author_id: String,
    pub parent_comment_id: Option<String>,
    pub is_anonymous: bool,
    pub is_deleted: bool,
    pub body: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

const COMMENT_COLUMNS: &str = "id, post_id, author_id, parent_comment_id, is_anonymous, is_deleted, body, created_at, updated_at";

/// The caller as AI-DO's checks see it: `viewer_is_admin` is the board manager.
#[derive(Clone, Debug)]
pub struct Viewer {
    pub id: String,
    pub is_admin: bool,
}

pub async fn viewer<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    identity: &Identity,
) -> ApiResult<Viewer> {
    Ok(Viewer {
        id: identity.user_id.clone(),
        is_admin: is_manager(executor, &identity.user_id).await?,
    })
}

// ---------------------------------------------------------------- schemas

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelOut {
    id: String,
    key: String,
    name: String,
    description: String,
    position: i32,
    active: bool,
    read_only: bool,
    force_anonymous: bool,
    admin_only_content: bool,
    template_title: String,
    template_body: String,
}

impl From<&Channel> for ChannelOut {
    fn from(channel: &Channel) -> Self {
        Self {
            id: channel.id.clone(),
            key: channel.key.clone(),
            name: channel.name.clone(),
            description: channel.description.clone(),
            position: channel.position,
            active: channel.active,
            read_only: channel.read_only,
            force_anonymous: channel.force_anonymous,
            admin_only_content: channel.admin_only_content,
            template_title: channel.template_title.clone(),
            template_body: channel.template_body.clone(),
        }
    }
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ChannelsResponse {
    channels: Vec<ChannelOut>,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChannelRequest {
    key: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    position: i64,
    #[serde(default = "default_true")]
    active: bool,
    #[serde(default, alias = "read_only")]
    read_only: bool,
    #[serde(default, alias = "force_anonymous")]
    force_anonymous: bool,
    #[serde(default, alias = "admin_only_content")]
    admin_only_content: bool,
    #[serde(default, alias = "template_title")]
    template_title: String,
    #[serde(default, alias = "template_body")]
    template_body: String,
}

/// `CommunityChannelCreateRequest` after Pydantic validation.
struct ChannelInput {
    key: String,
    name: String,
    description: String,
    position: i32,
    active: bool,
    read_only: bool,
    force_anonymous: bool,
    admin_only_content: bool,
    template_title: String,
    template_body: String,
}

fn valid_channel_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn stripped_nonblank(field: &str, value: &str) -> ApiResult<String> {
    let stripped = value.trim();
    if stripped.is_empty() {
        return Err(ApiError::validation(format!(
            "{field}: Value error, must not be blank"
        )));
    }
    Ok(stripped.to_string())
}

impl ChannelRequest {
    fn validate(self) -> ApiResult<ChannelInput> {
        check_length("key", &self.key, 1, 64)?;
        if !valid_channel_key(&self.key) {
            return Err(ApiError::validation(
                "key: String should match pattern '^[a-z0-9][a-z0-9-]*$'",
            ));
        }
        check_length("name", &self.name, 1, 120)?;
        check_length("description", &self.description, 0, 5000)?;
        if self.position < 0 {
            return Err(ApiError::validation(
                "position: Input should be greater than or equal to 0",
            ));
        }
        let position = i32::try_from(self.position)
            .map_err(|_| ApiError::validation("position: Input is too large"))?;
        check_length("templateTitle", &self.template_title, 0, 240)?;
        check_length("templateBody", &self.template_body, 0, 20000)?;
        Ok(ChannelInput {
            key: stripped_nonblank("key", &self.key)?,
            name: stripped_nonblank("name", &self.name)?,
            description: self.description.trim().to_string(),
            position,
            active: self.active,
            read_only: self.read_only,
            force_anonymous: self.force_anonymous,
            admin_only_content: self.admin_only_content,
            template_title: self.template_title.trim().to_string(),
            template_body: self.template_body.trim().to_string(),
        })
    }
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
enum LockedReason {
    Password,
    AdminOnly,
}

#[derive(Serialize, Clone, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CommentOut {
    id: String,
    body: String,
    is_anonymous: bool,
    is_deleted: bool,
    #[schema(required = true)]
    parent_comment_id: Option<String>,
    #[schema(required = true)]
    author_name: Option<String>,
    #[schema(required = true)]
    anon_seq: Option<i64>,
    can_modify: bool,
    #[serde(serialize_with = "crate::time::serialize")]
    #[schema(value_type = String, example = "2026-05-04T01:02:03")]
    created_at: NaiveDateTime,
    #[serde(serialize_with = "crate::time::serialize")]
    #[schema(value_type = String, example = "2026-05-04T01:02:03")]
    updated_at: NaiveDateTime,
}

#[derive(Serialize, Clone, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PostOut {
    id: String,
    number: i32,
    channel_key: String,
    title: String,
    body: String,
    is_anonymous: bool,
    is_secret: bool,
    locked: bool,
    #[schema(required = true, value_type = Option<LockedReason>)]
    locked_reason: Option<&'static str>,
    is_read: bool,
    is_mine: bool,
    can_modify: bool,
    #[schema(required = true)]
    author_name: Option<String>,
    comment_count: i64,
    #[serde(serialize_with = "crate::time::serialize")]
    #[schema(value_type = String, example = "2026-05-04T01:02:03")]
    created_at: NaiveDateTime,
    #[serde(serialize_with = "crate::time::serialize")]
    #[schema(value_type = String, example = "2026-05-04T01:02:03")]
    updated_at: NaiveDateTime,
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PostDetail {
    #[serde(flatten)]
    post: PostOut,
    comments: Vec<CommentOut>,
}

#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PostListResponse {
    posts: Vec<PostOut>,
    total: i64,
    page: i64,
    page_size: i64,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PostCreateRequest {
    title: String,
    body: String,
    #[serde(default, alias = "is_anonymous")]
    is_anonymous: bool,
    #[serde(default, alias = "is_secret")]
    is_secret: bool,
    #[serde(default)]
    password: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PostUpdateRequest {
    title: String,
    body: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnlockRequest {
    password: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CommentCreateRequest {
    body: String,
    #[serde(default, alias = "is_anonymous")]
    is_anonymous: bool,
    #[serde(default)]
    password: Option<String>,
    #[serde(default, alias = "parent_comment_id")]
    parent_comment_id: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CommentUpdateRequest {
    body: String,
}

fn validate_title_body(title: &str, body: &str) -> ApiResult<(String, String)> {
    check_length("title", title, 1, 240)?;
    check_length("body", body, 1, 20000)?;
    Ok((
        stripped_nonblank("title", title)?,
        stripped_nonblank("body", body)?,
    ))
}

fn validate_password(password: &Option<String>) -> ApiResult<()> {
    if let Some(value) = password {
        check_length("password", value, 0, 200)?;
    }
    Ok(())
}

fn validate_comment_body(body: &str) -> ApiResult<String> {
    check_length("body", body, 1, 10000)?;
    stripped_nonblank("body", body)
}

// ---------------------------------------------------------------- errors

fn channel_not_found() -> ApiError {
    ApiError::not_found(
        "community.channel_not_found",
        "Community channel not found.",
    )
}

fn post_not_found() -> ApiError {
    ApiError::not_found("community.post_not_found", "Community post not found.")
}

fn comment_not_found() -> ApiError {
    ApiError::not_found(
        "community.comment_not_found",
        "Community comment not found.",
    )
}

fn channel_key_exists() -> ApiError {
    ApiError::conflict(
        "community.channel_key_exists",
        "Community channel key already exists.",
    )
}

fn channel_read_only() -> ApiError {
    ApiError::forbidden(
        "community.channel_read_only",
        "Read-only community channels do not allow posts or comments.",
    )
}

fn invalid_password() -> ApiError {
    ApiError::forbidden("community.invalid_password", "Password is invalid.")
}

fn admin_only_content() -> ApiError {
    ApiError::forbidden(
        "community.admin_only_content",
        "Only administrators can view this community post.",
    )
}

fn require_author_or_admin(author_id: &str, viewer: &Viewer) -> ApiResult<()> {
    if author_id == viewer.id || viewer.is_admin {
        return Ok(());
    }
    Err(ApiError::forbidden(
        "community.not_author",
        "Only the author or an administrator can change it.",
    ))
}

fn require_channel_writable(channel: &Channel, viewer: &Viewer) -> ApiResult<()> {
    if channel.read_only && !viewer.is_admin {
        return Err(channel_read_only());
    }
    Ok(())
}

// ---------------------------------------------------------------- rules

fn post_is_anonymous(post: &Post, channel: &Channel) -> bool {
    post.is_anonymous || channel.force_anonymous
}

fn post_author_name_visible(post: &Post, channel: &Channel, viewer_is_admin: bool) -> bool {
    if viewer_is_admin {
        return true;
    }
    if channel.admin_only_content {
        return false;
    }
    !post_is_anonymous(post, channel)
}

fn comment_is_anonymous(comment: &Comment, force_anonymous: bool) -> bool {
    comment.is_anonymous || force_anonymous
}

pub fn post_locked_reason(
    post: &Post,
    channel: &Channel,
    unlocked: bool,
    viewer_is_admin: bool,
) -> Option<&'static str> {
    if channel.admin_only_content && !viewer_is_admin {
        return Some(LOCKED_REASON_ADMIN_ONLY);
    }
    if post.is_secret && !unlocked {
        return Some(LOCKED_REASON_PASSWORD);
    }
    None
}

pub fn post_read_locked_reason(loaded: &LoadedPost, viewer: &Viewer) -> Option<&'static str> {
    post_locked_reason(
        &loaded.post,
        &loaded.channel,
        viewer.is_admin || loaded.post.author_id == viewer.id,
        viewer.is_admin,
    )
}

pub async fn verify_post_password(post: &Post, password: Option<&str>) -> bool {
    if !post.is_secret {
        return true;
    }
    match (password, post.password_hash.as_deref()) {
        (Some(password), Some(hash)) if !password.is_empty() => {
            password::verify_password_blocking(password.to_string(), hash.to_string()).await
        }
        _ => false,
    }
}

fn anon_seq_map(comments: &[Comment], force_anonymous: bool) -> HashMap<String, i64> {
    let mut seq = HashMap::new();
    for comment in comments {
        if comment_is_anonymous(comment, force_anonymous) && !seq.contains_key(&comment.author_id) {
            let next = seq.len() as i64 + 1;
            seq.insert(comment.author_id.clone(), next);
        }
    }
    seq
}

#[allow(clippy::too_many_arguments)]
fn post_out(
    loaded: &LoadedPost,
    comment_count: i64,
    names: &HashMap<String, String>,
    unlocked: bool,
    viewer_id: Option<&str>,
    viewer_is_admin: bool,
    read_at: Option<NaiveDateTime>,
) -> PostOut {
    let post = &loaded.post;
    let channel = &loaded.channel;
    let locked_reason = post_locked_reason(post, channel, unlocked, viewer_is_admin);
    let locked = locked_reason.is_some();
    let is_mine = viewer_id.is_some_and(|id| post.author_id == id);
    PostOut {
        id: post.id.clone(),
        number: post.number,
        channel_key: channel.key.clone(),
        title: if locked {
            String::new()
        } else {
            post.title.clone()
        },
        body: if locked {
            String::new()
        } else {
            post.body.clone()
        },
        is_anonymous: post_is_anonymous(post, channel),
        is_secret: post.is_secret,
        locked,
        locked_reason,
        is_read: locked_reason != Some(LOCKED_REASON_ADMIN_ONLY)
            && read_at.is_some_and(|read_at| read_at >= post.updated_at),
        is_mine,
        can_modify: (viewer_is_admin || (is_mine && !channel.read_only))
            && !(locked_reason == Some(LOCKED_REASON_ADMIN_ONLY) && !viewer_is_admin),
        author_name: if post_author_name_visible(post, channel, viewer_is_admin) {
            names.get(&post.author_id).cloned()
        } else {
            None
        },
        comment_count,
        created_at: post.created_at,
        updated_at: post.updated_at,
    }
}

fn comment_out(
    comment: &Comment,
    names: &HashMap<String, String>,
    viewer_id: Option<&str>,
    viewer_is_admin: bool,
    anon_seq: Option<i64>,
    force_anonymous: bool,
    channel_read_only: bool,
) -> CommentOut {
    let is_anonymous = comment_is_anonymous(comment, force_anonymous);
    if comment.is_deleted {
        return CommentOut {
            id: comment.id.clone(),
            body: String::new(),
            is_anonymous,
            is_deleted: true,
            parent_comment_id: comment.parent_comment_id.clone(),
            author_name: None,
            anon_seq: None,
            can_modify: false,
            created_at: comment.created_at,
            updated_at: comment.updated_at,
        };
    }
    let is_mine = viewer_id.is_some_and(|id| comment.author_id == id);
    CommentOut {
        id: comment.id.clone(),
        body: comment.body.clone(),
        is_anonymous,
        is_deleted: false,
        parent_comment_id: comment.parent_comment_id.clone(),
        author_name: if viewer_is_admin || !is_anonymous {
            names.get(&comment.author_id).cloned()
        } else {
            None
        },
        anon_seq: if is_anonymous { anon_seq } else { None },
        can_modify: viewer_is_admin || (is_mine && !channel_read_only),
        created_at: comment.created_at,
        updated_at: comment.updated_at,
    }
}

// ---------------------------------------------------------------- data access

pub async fn ensure_default_channels(pool: &PgPool) -> ApiResult<()> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM community_channels WHERE key = $1)")
            .bind(DEFAULT_CHANNEL_KEY)
            .fetch_one(pool)
            .await?;
    if exists {
        return Ok(());
    }
    let now = now_utc();
    sqlx::query(
        "INSERT INTO community_channels (id, key, name, description, position, active, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, 0, true, $5, $5) ON CONFLICT (key) DO NOTHING",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(DEFAULT_CHANNEL_KEY)
    .bind(DEFAULT_CHANNEL_NAME)
    .bind(DEFAULT_CHANNEL_DESCRIPTION)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_channel_by_key(
    pool: &PgPool,
    channel_key: &str,
    active_only: bool,
) -> ApiResult<Option<Channel>> {
    ensure_default_channels(pool).await?;
    load_channel_by_key(pool, channel_key, active_only).await
}

async fn load_channel_by_key<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    channel_key: &str,
    active_only: bool,
) -> ApiResult<Option<Channel>> {
    let sql = format!(
        "SELECT {CHANNEL_COLUMNS} FROM community_channels WHERE key = $1{}",
        if active_only { " AND active" } else { "" }
    );
    Ok(sqlx::query_as::<_, Channel>(&sql)
        .bind(channel_key)
        .fetch_optional(executor)
        .await?)
}

async fn get_channel_or_404(pool: &PgPool, channel_key: &str) -> ApiResult<Channel> {
    get_channel_by_key(pool, channel_key, true)
        .await?
        .ok_or_else(channel_not_found)
}

async fn get_admin_channel_or_404(pool: &PgPool, channel_key: &str) -> ApiResult<Channel> {
    get_channel_by_key(pool, channel_key, false)
        .await?
        .ok_or_else(channel_not_found)
}

async fn channel_by_id<'e, E>(executor: E, id: &str) -> ApiResult<Channel>
where
    E: sqlx::PgExecutor<'e>,
{
    let sql = format!("SELECT {CHANNEL_COLUMNS} FROM community_channels WHERE id = $1");
    Ok(sqlx::query_as::<_, Channel>(&sql)
        .bind(id)
        .fetch_one(executor)
        .await?)
}

pub async fn load_post(pool: &PgPool, post_id: &str) -> ApiResult<Option<LoadedPost>> {
    let sql = format!("SELECT {POST_COLUMNS} FROM community_posts WHERE id = $1");
    let Some(post) = sqlx::query_as::<_, Post>(&sql)
        .bind(post_id)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };
    let channel = channel_by_id(pool, &post.channel_id).await?;
    Ok(Some(LoadedPost { post, channel }))
}

async fn get_post_or_404(pool: &PgPool, post_id: &str) -> ApiResult<LoadedPost> {
    load_post(pool, post_id).await?.ok_or_else(post_not_found)
}

async fn load_comment(
    pool: &PgPool,
    post_id: &str,
    comment_id: &str,
) -> ApiResult<Option<Comment>> {
    let sql = format!("SELECT {COMMENT_COLUMNS} FROM community_comments WHERE id = $1");
    let comment = sqlx::query_as::<_, Comment>(&sql)
        .bind(comment_id)
        .fetch_optional(pool)
        .await?;
    Ok(comment.filter(|comment| comment.post_id == post_id))
}

async fn get_comment_or_404(pool: &PgPool, post_id: &str, comment_id: &str) -> ApiResult<Comment> {
    load_comment(pool, post_id, comment_id)
        .await?
        .filter(|comment| !comment.is_deleted)
        .ok_or_else(comment_not_found)
}

async fn post_comments(pool: &PgPool, post_id: &str) -> ApiResult<Vec<Comment>> {
    let sql = format!(
        "SELECT {COMMENT_COLUMNS} FROM community_comments WHERE post_id = $1 ORDER BY created_at ASC"
    );
    Ok(sqlx::query_as::<_, Comment>(&sql)
        .bind(post_id)
        .fetch_all(pool)
        .await?)
}

async fn upsert_post_read(
    tx: &mut Transaction<'_, Postgres>,
    post_id: &str,
    user_id: &str,
) -> ApiResult<NaiveDateTime> {
    let timestamp = now_utc();
    sqlx::query(
        "INSERT INTO community_post_reads (post_id, user_id, read_at) VALUES ($1, $2, $3) \
         ON CONFLICT (post_id, user_id) DO UPDATE SET read_at = EXCLUDED.read_at",
    )
    .bind(post_id)
    .bind(user_id)
    .bind(timestamp)
    .execute(&mut **tx)
    .await?;
    Ok(timestamp)
}

pub async fn mark_post_read(pool: &PgPool, post_id: &str, user_id: &str) -> ApiResult<()> {
    let mut tx = pool.begin().await?;
    upsert_post_read(&mut tx, post_id, user_id).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn get_post_detail(
    pool: &PgPool,
    identity: &Identity,
    post_id: &str,
    unlocked: bool,
    viewer_id: &str,
    viewer_is_admin: bool,
) -> ApiResult<Option<PostDetail>> {
    let Some(loaded) = load_post(pool, post_id).await? else {
        return Ok(None);
    };
    let post = &loaded.post;
    let channel = &loaded.channel;
    let locked = post_locked_reason(post, channel, unlocked, viewer_is_admin).is_some();
    let force_anonymous = channel.force_anonymous;
    let comments = if locked {
        Vec::new()
    } else {
        post_comments(pool, post_id).await?
    };
    let comment_count = if locked {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM community_comments WHERE post_id = $1 AND NOT is_deleted",
        )
        .bind(post_id)
        .fetch_one(pool)
        .await?
    } else {
        comments
            .iter()
            .filter(|comment| !comment.is_deleted)
            .count() as i64
    };
    let mut author_ids: HashSet<String> = HashSet::new();
    if post_author_name_visible(post, channel, viewer_is_admin) {
        author_ids.insert(post.author_id.clone());
    }
    author_ids.extend(
        comments
            .iter()
            .filter(|comment| viewer_is_admin || !comment_is_anonymous(comment, force_anonymous))
            .map(|comment| comment.author_id.clone()),
    );
    let names = people::display_names(identity, author_ids).await;
    let anon_seq = anon_seq_map(&comments, force_anonymous);
    let read_at: Option<NaiveDateTime> = sqlx::query_scalar(
        "SELECT read_at FROM community_post_reads WHERE post_id = $1 AND user_id = $2",
    )
    .bind(post_id)
    .bind(viewer_id)
    .fetch_optional(pool)
    .await?;
    let base = post_out(
        &loaded,
        comment_count,
        &names,
        unlocked,
        Some(viewer_id),
        viewer_is_admin,
        read_at,
    );
    Ok(Some(PostDetail {
        post: base,
        comments: comments
            .iter()
            .map(|comment| {
                comment_out(
                    comment,
                    &names,
                    Some(viewer_id),
                    viewer_is_admin,
                    anon_seq.get(&comment.author_id).copied(),
                    force_anonymous,
                    channel.read_only,
                )
            })
            .collect(),
    }))
}

// ---------------------------------------------------------------- channel routes

#[utoipa::path(
    get, path = "/v1/community/channels", operation_id = "listCommunityChannels",
    responses(
        (status = 200, description = "Success", body = ChannelsResponse),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_channels(State(state): State<AppState>) -> ApiResult<Json<ChannelsResponse>> {
    list_channels_response(&state.pool, false).await.map(Json)
}

async fn list_channels_response(
    pool: &PgPool,
    include_inactive: bool,
) -> ApiResult<ChannelsResponse> {
    ensure_default_channels(pool).await?;
    let sql = format!(
        "SELECT {CHANNEL_COLUMNS} FROM community_channels{} ORDER BY position ASC, name ASC",
        if include_inactive {
            ""
        } else {
            " WHERE active"
        }
    );
    let rows = sqlx::query_as::<_, Channel>(&sql).fetch_all(pool).await?;
    Ok(ChannelsResponse {
        channels: rows.iter().map(ChannelOut::from).collect(),
    })
}

#[utoipa::path(
    get, path = "/v1/community/admin/channels", operation_id = "listAdminCommunityChannels",
    responses(
        (status = 200, description = "Success", body = ChannelsResponse),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_admin_channels(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<ChannelsResponse>> {
    require_manager(&state.pool, &identity).await?;
    list_channels_response(&state.pool, true).await.map(Json)
}

#[utoipa::path(
    post, path = "/v1/community/admin/channels", operation_id = "createCommunityChannel",
    request_body = ChannelRequest,
    responses(
        (status = 201, description = "Created", body = ChannelOut),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 409, description = "Current state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn create_channel(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<ChannelRequest>,
) -> ApiResult<(StatusCode, Json<ChannelOut>)> {
    require_manager(&state.pool, &identity).await?;
    let input = payload.validate()?;
    ensure_default_channels(&state.pool).await?;
    let now = now_utc();
    let sql = format!(
        "INSERT INTO community_channels (id, key, name, description, position, active, read_only, \
         force_anonymous, admin_only_content, template_title, template_body, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12) \
         ON CONFLICT (key) DO NOTHING RETURNING {CHANNEL_COLUMNS}"
    );
    let channel = sqlx::query_as::<_, Channel>(&sql)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&input.key)
        .bind(&input.name)
        .bind(&input.description)
        .bind(input.position)
        .bind(input.active)
        .bind(input.read_only)
        .bind(input.force_anonymous)
        .bind(input.admin_only_content)
        .bind(&input.template_title)
        .bind(&input.template_body)
        .bind(now)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(channel_key_exists)?;
    Ok((StatusCode::CREATED, Json(ChannelOut::from(&channel))))
}

#[utoipa::path(
    patch, path = "/v1/community/admin/channels/{channel_key}", operation_id = "updateCommunityChannel",
    params(
        ("channel_key" = String, Path),
    ),
    request_body = ChannelRequest,
    responses(
        (status = 200, description = "Success", body = ChannelOut),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 409, description = "Current state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn update_channel(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(channel_key): Path<String>,
    ApiJson(payload): ApiJson<ChannelRequest>,
) -> ApiResult<Json<ChannelOut>> {
    require_manager(&state.pool, &identity).await?;
    let channel = get_admin_channel_or_404(&state.pool, &channel_key).await?;
    let input = payload.validate()?;
    let existing: Option<String> =
        sqlx::query_scalar("SELECT id FROM community_channels WHERE key = $1 AND id <> $2")
            .bind(&input.key)
            .bind(&channel.id)
            .fetch_optional(&state.pool)
            .await?;
    if existing.is_some() {
        return Err(channel_key_exists());
    }
    // updated_at changes only when a value does (SQLAlchemy onupdate).
    let sql = format!(
        "UPDATE community_channels SET key = $2, name = $3, description = $4, position = $5, \
         active = $6, read_only = $7, force_anonymous = $8, admin_only_content = $9, \
         template_title = $10, template_body = $11, \
         updated_at = CASE WHEN (key, name, description, position, active, read_only, force_anonymous, \
            admin_only_content, template_title, template_body) IS DISTINCT FROM \
            ($2, $3, $4, $5, $6, $7, $8, $9, $10, $11) THEN $12 ELSE updated_at END \
         WHERE id = $1 RETURNING {CHANNEL_COLUMNS}"
    );
    let updated = sqlx::query_as::<_, Channel>(&sql)
        .bind(&channel.id)
        .bind(&input.key)
        .bind(&input.name)
        .bind(&input.description)
        .bind(input.position)
        .bind(input.active)
        .bind(input.read_only)
        .bind(input.force_anonymous)
        .bind(input.admin_only_content)
        .bind(&input.template_title)
        .bind(&input.template_body)
        .bind(now_utc())
        .fetch_one(&state.pool)
        .await
        .map_err(|error| match &error {
            sqlx::Error::Database(db) if db.is_unique_violation() => channel_key_exists(),
            _ => ApiError::from(error),
        })?;
    Ok(Json(ChannelOut::from(&updated)))
}

#[utoipa::path(
    delete, path = "/v1/community/admin/channels/{channel_key}", operation_id = "deleteCommunityChannel",
    params(
        ("channel_key" = String, Path),
    ),
    responses(
        (status = 204, description = "No content"),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 409, description = "Current state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn delete_channel(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(channel_key): Path<String>,
) -> ApiResult<StatusCode> {
    require_manager(&state.pool, &identity).await?;
    let channel = get_admin_channel_or_404(&state.pool, &channel_key).await?;
    let post_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM community_posts WHERE channel_id = $1")
            .bind(&channel.id)
            .fetch_one(&state.pool)
            .await?;
    if channel.key == DEFAULT_CHANNEL_KEY || post_count > 0 {
        return Err(ApiError::conflict(
            "community.channel_delete_blocked",
            "Default channels or channels with posts cannot be deleted. Disable the channel instead.",
        ));
    }
    sqlx::query("DELETE FROM community_channels WHERE id = $1")
        .bind(&channel.id)
        .execute(&state.pool)
        .await
        .map_err(|error| match &error {
            // A post raced in between: the foreign key keeps it.
            sqlx::Error::Database(db) if db.is_foreign_key_violation() => ApiError::conflict(
                "community.channel_delete_blocked",
                "Default channels or channels with posts cannot be deleted. Disable the channel instead.",
            ),
            _ => ApiError::from(error),
        })?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- post routes

#[derive(Deserialize)]
pub struct PostListQuery {
    page: Option<String>,
    page_size: Option<String>,
    q: Option<String>,
}

fn int_query(
    name: &str,
    value: Option<&str>,
    default: i64,
    min: i64,
    max: Option<i64>,
) -> ApiResult<i64> {
    let Some(raw) = value else {
        return Ok(default);
    };
    let parsed: i64 = raw
        .trim()
        .parse()
        .map_err(|_| ApiError::validation(format!("{name}: Input should be a valid integer")))?;
    if parsed < min || max.is_some_and(|max| parsed > max) {
        return Err(ApiError::validation(format!("{name}: out of range")));
    }
    Ok(parsed)
}

/// "37" or "#37" also finds post number 37.
fn post_number_query(query: &str) -> Option<i32> {
    let digits = query.strip_prefix('#').unwrap_or(query);
    if digits.is_empty() || digits.len() > 9 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

#[utoipa::path(
    get, path = "/v1/community/channels/{channel_key}/posts", operation_id = "listCommunityPosts",
    params(
        ("channel_key" = String, Path),
        ("page" = Option<i64>, Query, minimum = 1, description = "Page number (default 1)"),
        ("page_size" = Option<i64>, Query, minimum = 1, maximum = 100, description = "Page size (default 10)"),
        ("q" = Option<String>, Query, max_length = 120, description = "Post title, author, or number"),
    ),
    responses(
        (status = 200, description = "Success", body = PostListResponse),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_posts(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(channel_key): Path<String>,
    Query(query): Query<PostListQuery>,
) -> ApiResult<Json<PostListResponse>> {
    let page = int_query("page", query.page.as_deref(), 1, 1, None)?;
    let page_size = int_query(
        "page_size",
        query.page_size.as_deref(),
        DEFAULT_PAGE_SIZE,
        1,
        Some(100),
    )?;
    if let Some(q) = &query.q {
        check_length("q", q, 0, 120)?;
    }
    let bounded_page = page.max(1);
    let bounded_page_size = page_size.clamp(1, MAX_PAGE_SIZE);
    let search = query.q.as_deref().unwrap_or_default().trim().to_string();

    ensure_default_channels(&state.pool).await?;
    let mut author_ids = Vec::new();
    if !search.is_empty() {
        let channel = load_channel_by_key(&state.pool, &channel_key, true)
            .await?
            .ok_or_else(channel_not_found)?;
        if (!channel.admin_only_content && !channel.force_anonymous)
            || is_manager(&state.pool, &identity.user_id).await?
        {
            author_ids = people::search_user_ids(&identity, &search).await;
        }
    }

    // Read channel rules, the current manager role, and the count and rows
    // from one snapshot after the directory request.
    let mut tx = state.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let channel = load_channel_by_key(&mut *tx, &channel_key, true)
        .await?
        .ok_or_else(channel_not_found)?;
    let viewer = viewer(&mut *tx, &identity).await?;

    fn push_filters(
        builder: &mut QueryBuilder<'_, Postgres>,
        channel: &Channel,
        viewer: &Viewer,
        search: &str,
        author_ids: &[String],
    ) {
        builder
            .push(" WHERE channel_id = ")
            .push_bind(channel.id.clone());
        if search.is_empty() {
            return;
        }
        if channel.admin_only_content && channel.force_anonymous && !viewer.is_admin {
            builder.push(" AND false");
            return;
        }
        builder.push(" AND (false");
        if viewer.is_admin || !channel.admin_only_content {
            builder
                .push(" OR (title ILIKE ")
                .push_bind(format!("%{search}%"));
            if !viewer.is_admin {
                builder
                    .push(" AND (NOT is_secret OR author_id = ")
                    .push_bind(viewer.id.clone())
                    .push(")");
            }
            builder.push(")");
        }
        if let Some(number) = post_number_query(search) {
            builder.push(" OR number = ").push_bind(number);
        }
        if !author_ids.is_empty()
            && (viewer.is_admin || (!channel.admin_only_content && !channel.force_anonymous))
        {
            builder
                .push(" OR (author_id = ANY(")
                .push_bind(author_ids.to_vec())
                .push(")");
            if !viewer.is_admin {
                builder.push(" AND NOT is_anonymous");
            }
            builder.push(")");
        }
        builder.push(")");
    }
    let mut count_query = QueryBuilder::<Postgres>::new("SELECT count(*) FROM community_posts");
    push_filters(&mut count_query, &channel, &viewer, &search, &author_ids);
    let total: i64 = count_query.build_query_scalar().fetch_one(&mut *tx).await?;
    let mut post_query =
        QueryBuilder::<Postgres>::new(format!("SELECT {POST_COLUMNS} FROM community_posts"));
    push_filters(&mut post_query, &channel, &viewer, &search, &author_ids);
    post_query
        .push(" ORDER BY created_at DESC LIMIT ")
        .push_bind(bounded_page_size)
        .push(" OFFSET ")
        .push_bind((bounded_page - 1).saturating_mul(bounded_page_size));
    let posts: Vec<Post> = post_query.build_query_as().fetch_all(&mut *tx).await?;
    if posts.is_empty() {
        tx.commit().await?;
        return Ok(Json(PostListResponse {
            posts: Vec::new(),
            total,
            page: bounded_page,
            page_size: bounded_page_size,
        }));
    }
    let post_ids: Vec<String> = posts.iter().map(|post| post.id.clone()).collect();
    let reads: HashMap<String, NaiveDateTime> = sqlx::query_as::<_, (String, NaiveDateTime)>(
        "SELECT post_id, read_at FROM community_post_reads WHERE user_id = $1 AND post_id = ANY($2)",
    )
    .bind(&viewer.id)
    .bind(&post_ids)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    let counts: HashMap<String, i64> = sqlx::query_as::<_, (String, i64)>(
        "SELECT post_id, count(id) FROM community_comments \
         WHERE post_id = ANY($1) AND NOT is_deleted GROUP BY post_id",
    )
    .bind(&post_ids)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    tx.commit().await?;
    let author_ids: HashSet<String> = posts
        .iter()
        .filter(|post| post_author_name_visible(post, &channel, viewer.is_admin))
        .map(|post| post.author_id.clone())
        .collect();
    let names = people::display_names(&identity, author_ids).await;
    let posts = posts
        .into_iter()
        .map(|post| {
            let comment_count = counts.get(&post.id).copied().unwrap_or(0);
            let read_at = reads.get(&post.id).copied();
            let unlocked = viewer.is_admin || post.author_id == viewer.id;
            post_out(
                &LoadedPost {
                    post,
                    channel: channel.clone(),
                },
                comment_count,
                &names,
                unlocked,
                Some(&viewer.id),
                viewer.is_admin,
                read_at,
            )
        })
        .collect();
    Ok(Json(PostListResponse {
        posts,
        total,
        page: bounded_page,
        page_size: bounded_page_size,
    }))
}

/// Take the channel's next post number without ever waiting for it (AI-DO
/// `_next_post_number`): the counter row is locked FOR NO KEY UPDATE NOWAIT
/// until commit, so a concurrent writer is refused at once with 409
/// `community.post_number_busy` and takes no number.
async fn next_post_number(tx: &mut Transaction<'_, Postgres>, channel_id: &str) -> ApiResult<i32> {
    let locked =
        sqlx::query("SELECT id FROM community_channels WHERE id = $1 FOR NO KEY UPDATE NOWAIT")
            .bind(channel_id)
            .execute(&mut **tx)
            .await;
    if let Err(error) = locked {
        if let sqlx::Error::Database(db) = &error {
            if db.code().as_deref() == Some(LOCK_NOT_AVAILABLE_SQLSTATE) {
                return Err(ApiError::conflict(
                    "community.post_number_busy",
                    "Another post is being added to this channel. Try again shortly.",
                ));
            }
        }
        return Err(error.into());
    }
    Ok(sqlx::query_scalar(
        "UPDATE community_channels SET last_post_number = last_post_number + 1 \
         WHERE id = $1 RETURNING last_post_number",
    )
    .bind(channel_id)
    .fetch_one(&mut **tx)
    .await?)
}

#[utoipa::path(
    post, path = "/v1/community/channels/{channel_key}/posts", operation_id = "createCommunityPost",
    params(
        ("channel_key" = String, Path),
    ),
    request_body = PostCreateRequest,
    responses(
        (status = 201, description = "Created", body = PostOut),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 400, description = "Invalid request for this resource", body = crate::error::ErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 409, description = "Current state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn create_post(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(channel_key): Path<String>,
    ApiJson(payload): ApiJson<PostCreateRequest>,
) -> ApiResult<(StatusCode, Json<PostOut>)> {
    let (title, body) = validate_title_body(&payload.title, &payload.body)?;
    validate_password(&payload.password)?;
    let channel = get_channel_or_404(&state.pool, &channel_key).await?;
    let viewer = viewer(&state.pool, &identity).await?;
    require_channel_writable(&channel, &viewer)?;
    if payload.is_secret
        && !channel.admin_only_content
        && payload
            .password
            .as_deref()
            .unwrap_or_default()
            .trim()
            .is_empty()
    {
        return Err(ApiError::bad_request(
            "community.password_required",
            "A password is required for secret posts.",
        ));
    }
    ugc::ensure_content_allowed(&state.pool, &[&title, &body]).await?;
    let effective_is_secret = payload.is_secret && !channel.admin_only_content;
    // Everything slow happens before the number: taking it locks the channel counter until commit.
    let password_hash = match (effective_is_secret, payload.password.as_deref()) {
        (true, Some(password)) if !password.is_empty() => {
            Some(password::hash_password_blocking(password.to_string()).await)
        }
        _ => None,
    };
    let post_id = uuid::Uuid::new_v4().to_string();
    let mut tx = state.pool.begin().await?;
    let linked = media::sync_embedded_media(
        &mut tx,
        &body,
        media::RESOURCE_COMMUNITY_POST,
        &post_id,
        &viewer.id,
    )
    .await?;
    let number = next_post_number(&mut tx, &channel.id).await?;
    let now = now_utc();
    sqlx::query(
        "INSERT INTO community_posts (id, channel_id, number, author_id, is_anonymous, is_secret, \
         password_hash, title, body, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)",
    )
    .bind(&post_id)
    .bind(&channel.id)
    .bind(number)
    .bind(&viewer.id)
    .bind(payload.is_anonymous || channel.force_anonymous)
    .bind(effective_is_secret)
    .bind(&password_hash)
    .bind(&title)
    .bind(&body)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    upsert_post_read(&mut tx, &post_id, &viewer.id).await?;
    tx.commit().await?;
    media::attach_linked(&state.pool, &identity, &linked).await;
    // AI-DO answers with the author's unlocked view and viewer_is_admin=False.
    let detail = get_post_detail(&state.pool, &identity, &post_id, true, &viewer.id, false)
        .await?
        .ok_or_else(post_not_found)?;
    Ok((StatusCode::CREATED, Json(detail.post)))
}

#[utoipa::path(
    get, path = "/v1/community/posts/{post_id}", operation_id = "getCommunityPost",
    params(
        ("post_id" = String, Path),
    ),
    responses(
        (status = 200, description = "Success", body = PostDetail),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn get_post(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(post_id): Path<String>,
) -> ApiResult<Json<PostDetail>> {
    let loaded = get_post_or_404(&state.pool, &post_id).await?;
    let viewer = viewer(&state.pool, &identity).await?;
    let is_author = loaded.post.author_id == viewer.id;
    let detail = get_post_detail(
        &state.pool,
        &identity,
        &loaded.post.id,
        viewer.is_admin || is_author,
        &viewer.id,
        viewer.is_admin,
    )
    .await?
    .ok_or_else(post_not_found)?;
    if is_author && !detail.post.locked {
        // The author reads the comments the inbox told them about.
        notify::mark_read(&identity, &notify::post_group_key(&loaded.post.id)).await;
    }
    Ok(Json(detail))
}

async fn lock_post(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, post_id: &str) -> ApiResult<()> {
    sqlx::query_scalar::<_, String>("SELECT id FROM community_posts WHERE id = $1 FOR UPDATE")
        .bind(post_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(post_not_found)?;
    Ok(())
}

#[utoipa::path(
    patch, path = "/v1/community/posts/{post_id}", operation_id = "updateCommunityPost",
    params(
        ("post_id" = String, Path),
    ),
    request_body = PostUpdateRequest,
    responses(
        (status = 200, description = "Success", body = PostOut),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn update_post(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(post_id): Path<String>,
    ApiJson(payload): ApiJson<PostUpdateRequest>,
) -> ApiResult<Json<PostOut>> {
    let (title, body) = validate_title_body(&payload.title, &payload.body)?;
    let loaded = get_post_or_404(&state.pool, &post_id).await?;
    let viewer = viewer(&state.pool, &identity).await?;
    require_author_or_admin(&loaded.post.author_id, &viewer)?;
    require_channel_writable(&loaded.channel, &viewer)?;
    ugc::ensure_content_allowed(&state.pool, &[&title, &body]).await?;
    let mut tx = state.pool.begin().await?;
    lock_post(&mut tx, &loaded.post.id).await?;
    let linked = media::sync_embedded_media(
        &mut tx,
        &body,
        media::RESOURCE_COMMUNITY_POST,
        &loaded.post.id,
        &viewer.id,
    )
    .await?;
    sqlx::query(
        "UPDATE community_posts SET title = $2, body = $3, \
         updated_at = CASE WHEN (title, body) IS DISTINCT FROM ($2, $3) THEN $4 ELSE updated_at END \
         WHERE id = $1",
    )
    .bind(&loaded.post.id)
    .bind(&title)
    .bind(&body)
    .bind(now_utc())
    .execute(&mut *tx)
    .await?;
    upsert_post_read(&mut tx, &loaded.post.id, &viewer.id).await?;
    tx.commit().await?;
    media::attach_linked(&state.pool, &identity, &linked).await;
    let detail = get_post_detail(
        &state.pool,
        &identity,
        &loaded.post.id,
        true,
        &viewer.id,
        viewer.is_admin,
    )
    .await?
    .ok_or_else(post_not_found)?;
    Ok(Json(detail.post))
}

#[utoipa::path(
    delete, path = "/v1/community/posts/{post_id}", operation_id = "deleteCommunityPost",
    params(
        ("post_id" = String, Path),
    ),
    responses(
        (status = 204, description = "No content"),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn delete_post(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(post_id): Path<String>,
) -> ApiResult<StatusCode> {
    let loaded = get_post_or_404(&state.pool, &post_id).await?;
    let viewer = viewer(&state.pool, &identity).await?;
    require_author_or_admin(&loaded.post.author_id, &viewer)?;
    require_channel_writable(&loaded.channel, &viewer)?;
    let mut tx = state.pool.begin().await?;
    lock_post(&mut tx, &loaded.post.id).await?;
    let released = media::release_for_post(&mut tx, &loaded.post.id).await?;
    sqlx::query("DELETE FROM community_posts WHERE id = $1")
        .bind(&loaded.post.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    media::detach_released(&state.pool, &identity, &released).await;
    notify::retract(&identity, &notify::post_group_key(&loaded.post.id)).await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post, path = "/v1/community/posts/{post_id}/read", operation_id = "markCommunityPostRead",
    params(
        ("post_id" = String, Path),
    ),
    responses(
        (status = 204, description = "No content"),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn mark_read(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(post_id): Path<String>,
) -> ApiResult<StatusCode> {
    let loaded = get_post_or_404(&state.pool, &post_id).await?;
    let viewer = viewer(&state.pool, &identity).await?;
    match post_read_locked_reason(&loaded, &viewer) {
        Some(LOCKED_REASON_ADMIN_ONLY) => return Err(admin_only_content()),
        Some(_) => return Err(invalid_password()),
        None => {}
    }
    mark_post_read(&state.pool, &loaded.post.id, &viewer.id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post, path = "/v1/community/posts/{post_id}/unlock", operation_id = "unlockCommunityPost",
    params(
        ("post_id" = String, Path),
    ),
    request_body = UnlockRequest,
    responses(
        (status = 200, description = "Success", body = PostDetail),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn unlock_post(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(post_id): Path<String>,
    ApiJson(payload): ApiJson<UnlockRequest>,
) -> ApiResult<Json<PostDetail>> {
    check_length("password", &payload.password, 1, 200)?;
    let loaded = get_post_or_404(&state.pool, &post_id).await?;
    let viewer = viewer(&state.pool, &identity).await?;
    if post_read_locked_reason(&loaded, &viewer) == Some(LOCKED_REASON_ADMIN_ONLY) {
        return Err(admin_only_content());
    }
    if !verify_post_password(&loaded.post, Some(&payload.password)).await {
        return Err(invalid_password());
    }
    mark_post_read(&state.pool, &loaded.post.id, &viewer.id).await?;
    let detail = get_post_detail(
        &state.pool,
        &identity,
        &loaded.post.id,
        true,
        &viewer.id,
        viewer.is_admin,
    )
    .await?
    .ok_or_else(post_not_found)?;
    Ok(Json(detail))
}

// ---------------------------------------------------------------- comment routes

#[utoipa::path(
    post, path = "/v1/community/posts/{post_id}/comments", operation_id = "createCommunityComment",
    params(
        ("post_id" = String, Path),
    ),
    request_body = CommentCreateRequest,
    responses(
        (status = 201, description = "Created", body = CommentOut),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn create_comment(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(post_id): Path<String>,
    ApiJson(payload): ApiJson<CommentCreateRequest>,
) -> ApiResult<(StatusCode, Json<CommentOut>)> {
    let body = validate_comment_body(&payload.body)?;
    validate_password(&payload.password)?;
    let loaded = get_post_or_404(&state.pool, &post_id).await?;
    let viewer = viewer(&state.pool, &identity).await?;
    require_channel_writable(&loaded.channel, &viewer)?;
    if loaded.channel.admin_only_content && !viewer.is_admin {
        return Err(admin_only_content());
    }
    if loaded.post.is_secret
        && loaded.post.author_id != viewer.id
        && !viewer.is_admin
        && !verify_post_password(&loaded.post, payload.password.as_deref()).await
    {
        return Err(invalid_password());
    }
    let mut parent_comment_id = None;
    if let Some(parent_id) = payload
        .parent_comment_id
        .as_deref()
        .filter(|id| !id.is_empty())
    {
        let parent = get_comment_or_404(&state.pool, &loaded.post.id, parent_id).await?;
        parent_comment_id = Some(parent.parent_comment_id.unwrap_or(parent.id));
    }
    ugc::ensure_content_allowed(&state.pool, &[&body]).await?;
    let force_anonymous = loaded.channel.force_anonymous;
    let comment_id = uuid::Uuid::new_v4().to_string();
    let now = now_utc();
    let is_anonymous = payload.is_anonymous || force_anonymous;
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO community_comments (id, post_id, author_id, parent_comment_id, is_anonymous, \
         is_deleted, body, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, false, $6, $7, $7)",
    )
    .bind(&comment_id)
    .bind(&loaded.post.id)
    .bind(&viewer.id)
    .bind(&parent_comment_id)
    .bind(is_anonymous)
    .bind(&body)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    let linked = media::sync_embedded_media(
        &mut tx,
        &body,
        media::RESOURCE_COMMUNITY_COMMENT,
        &comment_id,
        &viewer.id,
    )
    .await?;
    tx.commit().await?;
    media::attach_linked(&state.pool, &identity, &linked).await;
    notify::comment_created(&state.pool, &identity, &loaded, is_anonymous).await;
    let comments = post_comments(&state.pool, &loaded.post.id).await?;
    let comment = comments
        .iter()
        .find(|comment| comment.id == comment_id)
        .cloned()
        .ok_or_else(comment_not_found)?;
    let names = if viewer.is_admin || !comment_is_anonymous(&comment, force_anonymous) {
        people::display_names(&identity, [viewer.id.clone()]).await
    } else {
        HashMap::new()
    };
    let out = comment_out(
        &comment,
        &names,
        Some(&viewer.id),
        viewer.is_admin,
        anon_seq_map(&comments, force_anonymous)
            .get(&viewer.id)
            .copied(),
        force_anonymous,
        false,
    );
    Ok((StatusCode::CREATED, Json(out)))
}

async fn lock_comment_post(tx: &mut Transaction<'_, Postgres>, post_id: &str) -> ApiResult<()> {
    // Post deletion must wait before releasing comment media.
    sqlx::query_scalar::<_, String>("SELECT id FROM community_posts WHERE id = $1 FOR KEY SHARE")
        .bind(post_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(post_not_found)?;
    Ok(())
}

#[utoipa::path(
    patch, path = "/v1/community/posts/{post_id}/comments/{comment_id}", operation_id = "updateCommunityComment",
    params(
        ("post_id" = String, Path),
        ("comment_id" = String, Path),
    ),
    request_body = CommentUpdateRequest,
    responses(
        (status = 200, description = "Success", body = CommentOut),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn update_comment(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((post_id, comment_id)): Path<(String, String)>,
    ApiJson(payload): ApiJson<CommentUpdateRequest>,
) -> ApiResult<Json<CommentOut>> {
    let body = validate_comment_body(&payload.body)?;
    let loaded = get_post_or_404(&state.pool, &post_id).await?;
    let comment = get_comment_or_404(&state.pool, &post_id, &comment_id).await?;
    let viewer = viewer(&state.pool, &identity).await?;
    require_author_or_admin(&comment.author_id, &viewer)?;
    require_channel_writable(&loaded.channel, &viewer)?;
    ugc::ensure_content_allowed(&state.pool, &[&body]).await?;
    let mut tx = state.pool.begin().await?;
    lock_comment_post(&mut tx, &post_id).await?;
    let update = format!(
        "UPDATE community_comments SET body = $3, \
         updated_at = CASE WHEN body IS DISTINCT FROM $3 THEN $4 ELSE updated_at END \
         WHERE id = $1 AND post_id = $2 AND NOT is_deleted RETURNING {COMMENT_COLUMNS}",
    );
    let updated = sqlx::query_as::<_, Comment>(&update)
        .bind(&comment.id)
        .bind(&comment.post_id)
        .bind(&body)
        .bind(now_utc())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(comment_not_found)?;
    let linked = media::sync_embedded_media(
        &mut tx,
        &body,
        media::RESOURCE_COMMUNITY_COMMENT,
        &comment.id,
        &viewer.id,
    )
    .await?;
    tx.commit().await?;
    media::attach_linked(&state.pool, &identity, &linked).await;
    let comments = post_comments(&state.pool, &comment.post_id).await?;
    let force_anonymous = loaded.channel.force_anonymous;
    let names = if viewer.is_admin || !comment_is_anonymous(&updated, force_anonymous) {
        people::display_names(&identity, [updated.author_id.clone()]).await
    } else {
        HashMap::new()
    };
    Ok(Json(comment_out(
        &updated,
        &names,
        Some(&viewer.id),
        viewer.is_admin,
        anon_seq_map(&comments, force_anonymous)
            .get(&updated.author_id)
            .copied(),
        force_anonymous,
        false,
    )))
}

#[utoipa::path(
    delete, path = "/v1/community/posts/{post_id}/comments/{comment_id}", operation_id = "deleteCommunityComment",
    params(
        ("post_id" = String, Path),
        ("comment_id" = String, Path),
    ),
    responses(
        (status = 204, description = "No content"),
        (status = 401, description = "Valid gateway identity required", body = crate::error::GatewayIdentityError),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Requested resource was not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::ErrorResponse)
    )
)]
pub async fn delete_comment(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((post_id, comment_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let loaded = get_post_or_404(&state.pool, &post_id).await?;
    let comment = get_comment_or_404(&state.pool, &post_id, &comment_id).await?;
    let viewer = viewer(&state.pool, &identity).await?;
    require_author_or_admin(&comment.author_id, &viewer)?;
    require_channel_writable(&loaded.channel, &viewer)?;
    let mut tx = state.pool.begin().await?;
    lock_comment_post(&mut tx, &post_id).await?;
    // Match the update path's lock order: comment first, then its media.
    let deleted = sqlx::query(
        "UPDATE community_comments SET is_deleted = true, body = '', \
         updated_at = CASE WHEN (is_deleted, body) IS DISTINCT FROM (true, '') THEN $2 ELSE updated_at END \
         WHERE id = $1 AND NOT is_deleted",
    )
    .bind(&comment.id)
    .bind(now_utc())
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if deleted == 0 {
        return Err(comment_not_found());
    }
    let released = media::release_for_comment(&mut tx, &comment.id).await?;
    tx.commit().await?;
    media::detach_released(&state.pool, &identity, &released).await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel() -> Channel {
        Channel {
            id: "c".into(),
            key: "suggestions".into(),
            name: "Suggestions".into(),
            description: String::new(),
            position: 0,
            active: true,
            read_only: false,
            force_anonymous: false,
            admin_only_content: false,
            template_title: String::new(),
            template_body: String::new(),
        }
    }

    fn post(author: &str) -> Post {
        let now = now_utc();
        Post {
            id: "p".into(),
            channel_id: "c".into(),
            number: 1,
            author_id: author.into(),
            is_anonymous: false,
            is_secret: false,
            password_hash: None,
            title: "Title".into(),
            body: "Body".into(),
            created_at: now,
            updated_at: now,
        }
    }

    // AI-DO test_require_author_or_admin_allows_author_and_admin_only
    #[test]
    fn require_author_or_admin_allows_author_and_admin_only() {
        let author = Viewer {
            id: "author".into(),
            is_admin: false,
        };
        let admin = Viewer {
            id: "admin".into(),
            is_admin: true,
        };
        let other = Viewer {
            id: "other".into(),
            is_admin: false,
        };
        assert!(require_author_or_admin("author", &author).is_ok());
        assert!(require_author_or_admin("author", &admin).is_ok());
        let error = require_author_or_admin("author", &other).unwrap_err();
        assert_eq!(error.status, StatusCode::FORBIDDEN);
        assert_eq!(error.code, "community_not_author");
    }

    // AI-DO test_channel_admin_contract_rejects_blank_name
    #[test]
    fn channel_admin_contract_rejects_blank_name() {
        let request: ChannelRequest =
            serde_json::from_value(serde_json::json!({"key": "qna", "name": "   "})).unwrap();
        let error = request.validate().err().unwrap();
        assert_eq!(error.status, StatusCode::UNPROCESSABLE_ENTITY);
        let request: ChannelRequest =
            serde_json::from_value(serde_json::json!({"key": "Q&A", "name": "QnA"})).unwrap();
        assert!(request.validate().is_err());
        let request: ChannelRequest = serde_json::from_value(
            serde_json::json!({"key": "q-a", "name": " QnA ", "templateTitle": " T "}),
        )
        .unwrap();
        let input = request.validate().unwrap();
        assert_eq!(input.name, "QnA");
        assert_eq!(input.template_title, "T");
        assert!(input.active);
    }

    #[test]
    fn locked_reasons_follow_ai_do() {
        let mut secret = post("author");
        secret.is_secret = true;
        let open = channel();
        assert_eq!(
            post_locked_reason(&secret, &open, false, false),
            Some(LOCKED_REASON_PASSWORD)
        );
        assert_eq!(post_locked_reason(&secret, &open, true, false), None);
        let mut admin_only = channel();
        admin_only.admin_only_content = true;
        assert_eq!(
            post_locked_reason(&post("author"), &admin_only, true, false),
            Some(LOCKED_REASON_ADMIN_ONLY)
        );
        assert_eq!(
            post_locked_reason(&post("author"), &admin_only, false, true),
            None
        );
    }

    #[test]
    fn anonymous_names_are_hidden_except_for_admins() {
        let mut anonymous = post("author");
        anonymous.is_anonymous = true;
        let names: HashMap<String, String> = [("author".to_string(), "Author".to_string())].into();
        let loaded = LoadedPost {
            post: anonymous,
            channel: channel(),
        };
        let out = post_out(&loaded, 0, &names, false, Some("viewer"), false, None);
        assert_eq!(out.author_name, None);
        assert!(out.is_anonymous);
        let out = post_out(&loaded, 0, &names, false, Some("viewer"), true, None);
        assert_eq!(out.author_name.as_deref(), Some("Author"));
    }

    #[test]
    fn post_numbers_match_digits_with_an_optional_hash() {
        assert_eq!(post_number_query("37"), Some(37));
        assert_eq!(post_number_query("#37"), Some(37));
        assert_eq!(post_number_query("##37"), None);
        assert_eq!(post_number_query("1234567890"), None);
        assert_eq!(post_number_query("37a"), None);
        assert_eq!(post_number_query("#"), None);
    }

    #[test]
    fn anonymous_numbering_is_stable_per_author() {
        let now = now_utc();
        let comment = |author: &str, anonymous: bool| Comment {
            id: uuid::Uuid::new_v4().to_string(),
            post_id: "p".into(),
            author_id: author.into(),
            parent_comment_id: None,
            is_anonymous: anonymous,
            is_deleted: false,
            body: "b".into(),
            created_at: now,
            updated_at: now,
        };
        let comments = vec![
            comment("a", true),
            comment("b", false),
            comment("c", true),
            comment("a", true),
            comment("b", true),
        ];
        let seq = anon_seq_map(&comments, false);
        assert_eq!(seq.get("a"), Some(&1));
        assert_eq!(seq.get("c"), Some(&2));
        assert_eq!(seq.get("b"), Some(&3));
        let forced = anon_seq_map(&comments, true);
        assert_eq!(forced.get("b"), Some(&2));
    }
}
