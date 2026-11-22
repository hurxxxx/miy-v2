//! Platform notifications (보강) and the platform → app read-only endpoints.
//!
//! AI-DO created an in-app notification plus a bot DM for a comment on one's
//! post (domains/community/notifications.py) and had no notice notifications.
//! miy publishes to the platform inbox (`platform:notifications:write`):
//!
//! | kind | when | recipients | group key |
//! | --- | --- | --- | --- |
//! | `comment` | someone comments on a post | the post author (not self; for an admin-only channel only a board manager) | `post:<id>` |
//! | `notice` | a notice is published | company: every directory user; space: the space's direct user members (not the author) | `notice:<id>` |
//!
//! Deleting the post or notice retracts its group; removing someone from a
//! space retracts that space's notices for them; reading the post (author) or
//! notice marks the group read. Publishing happens after the app commit and
//! never undoes it; an uncertain result is looked up by request ID, not sent
//! again.
use axum::{Extension, Json, extract::State};
use miy_app_sdk::{Identity, Notification};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{collections::BTreeSet, time::Duration};

use crate::{
    AppState,
    announcements::{self, Announcement, NoticeWithSpace},
    community::{self, LoadedPost},
    error::{ApiError, ApiJson, ApiResult},
    managers::is_manager,
    people,
    spaces::{self, Space},
    time::utc_iso,
};

const CALL_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RECIPIENTS_PER_CALL: usize = 500;
/// Directory pages read for a company-wide notice (100 people per page).
const COMPANY_DIRECTORY_PAGES: usize = 100;
const TITLE_MAX: usize = 200;

pub fn post_group_key(post_id: &str) -> String {
    format!("post:{post_id}")
}

pub fn notice_group_key(notice_id: &str) -> String {
    format!("notice:{notice_id}")
}

fn shorten(text: &str, max: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max {
        return compact;
    }
    let mut cut: String = compact.chars().take(max - 1).collect();
    cut = cut.trim_end().to_string();
    cut.push('…');
    cut
}

/// Publish to `recipients` in batches. Best effort: failures are not retried
/// with a new request and never reach the caller.
async fn publish(
    identity: &Identity,
    recipients: Vec<String>,
    kind: &str,
    group_key: &str,
    title: &str,
    path: &str,
    resource_id: &str,
) {
    if identity.context.platform {
        return;
    }
    let recipients: Vec<String> = recipients
        .into_iter()
        .filter(|id| !id.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    for chunk in recipients.chunks(MAX_RECIPIENTS_PER_CALL) {
        let notification = Notification {
            request_id: uuid::Uuid::new_v4().to_string(),
            recipients: chunk.to_vec(),
            kind: kind.to_string(),
            group_key: group_key.to_string(),
            title: shorten(title, TITLE_MAX),
            path: path.to_string(),
            resource_id: Some(resource_id.to_string()),
        };
        match tokio::time::timeout(CALL_TIMEOUT, identity.notify(&notification)).await {
            Ok(Ok(_)) => {}
            Ok(Err(error)) if error.status.is_some() => {
                eprintln!("notification_rejected: {}", error.code);
            }
            _ => {
                // Uncertain: ask for the first result instead of sending again.
                let _ = tokio::time::timeout(
                    CALL_TIMEOUT,
                    identity.notification_by_request(&notification.request_id),
                )
                .await;
            }
        }
    }
}

pub async fn retract(identity: &Identity, group_key: &str) {
    if identity.context.platform {
        return;
    }
    let _ = tokio::time::timeout(
        CALL_TIMEOUT,
        identity.retract_notifications(group_key, None),
    )
    .await;
}

pub async fn retract_for(identity: &Identity, group_key: &str, recipients: &[String]) {
    if identity.context.platform || recipients.is_empty() {
        return;
    }
    let _ = tokio::time::timeout(
        CALL_TIMEOUT,
        identity.retract_notifications(group_key, Some(recipients)),
    )
    .await;
}

pub async fn mark_read(identity: &Identity, group_key: &str) {
    if identity.context.platform {
        return;
    }
    let _ = tokio::time::timeout(CALL_TIMEOUT, identity.mark_notifications_read(group_key)).await;
}

/// AI-DO `_community_post_url`: `/community/posts/<id>` plus `?channel=` off
/// the default channel.
pub fn post_path(loaded: &LoadedPost) -> String {
    let path = format!("/community/posts/{}", loaded.post.id);
    if loaded.channel.key == community::DEFAULT_CHANNEL_KEY {
        path
    } else {
        format!("{path}?channel={}", loaded.channel.key)
    }
}

/// AI-DO `create_comment_notification` rules with the Korean copy of
/// `_notification_copy` (the app does not know the recipient's language). The
/// line names no body text; an anonymous comment names no commenter.
pub async fn comment_created(
    pool: &PgPool,
    identity: &Identity,
    loaded: &LoadedPost,
    anonymous: bool,
) {
    let recipient = loaded.post.author_id.clone();
    if recipient == identity.user_id {
        return;
    }
    if loaded.channel.admin_only_content && !is_manager(pool, &recipient).await.unwrap_or(false) {
        return;
    }
    let actor = if anonymous {
        "익명 사용자가".to_string()
    } else {
        match people::display_names(identity, [identity.user_id.clone()])
            .await
            .remove(&identity.user_id)
        {
            Some(name) => format!("{name}님이"),
            None => "다른 사용자가".to_string(),
        }
    };
    let title = format!(
        "{actor} \"{}\"에 댓글을 남겼습니다",
        shorten(&loaded.post.title, 120)
    );
    publish(
        identity,
        vec![recipient],
        "comment",
        &post_group_key(&loaded.post.id),
        &title,
        &post_path(loaded),
        &loaded.post.id,
    )
    .await;
}

pub async fn notice_published(
    pool: &PgPool,
    identity: &Identity,
    notice: &Announcement,
    space: Option<&Space>,
) {
    let (recipients, label) = if notice.scope == "company" {
        (
            people::all_user_ids(identity, COMPANY_DIRECTORY_PAGES).await,
            "전사 공지".to_string(),
        )
    } else {
        let Some(space) = space else { return };
        (
            spaces::direct_user_members(pool, identity, &space.id)
                .await
                .unwrap_or_default(),
            format!("{} 공지", shorten(&space.name, 60)),
        )
    };
    let recipients: Vec<String> = recipients
        .into_iter()
        .filter(|id| id != &identity.user_id)
        .collect();
    if recipients.is_empty() {
        return;
    }
    let title = format!("[{label}] {}", notice.title);
    publish(
        identity,
        recipients,
        "notice",
        &notice_group_key(&notice.id),
        &title,
        &announcements::notice_path(notice),
        &notice.id,
    )
    .await;
}

// ---------------------------------------------------------------- platform endpoints

fn platform_only(identity: &Identity) -> ApiResult<()> {
    if identity.context.platform {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "board.platform_only",
            "Only the platform calls this endpoint.",
        ))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckItem {
    group_key: String,
    #[serde(default)]
    #[allow(dead_code)]
    resource_id: Option<String>,
}

#[derive(Deserialize)]
pub struct CheckRequest {
    #[serde(default)]
    items: Vec<CheckItem>,
}

#[derive(Serialize)]
pub struct CheckResponse {
    allowed: Vec<String>,
}

/// `POST /platform/notification-check`: which of the person's notification
/// groups still point at something they may open. Read-only and answered from
/// the app DB alone (a platform call carries no token for directory calls), so
/// a space notice counts only for direct members; group-only members are
/// hidden (fail closed).
pub async fn notification_check(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<CheckRequest>,
) -> ApiResult<Json<CheckResponse>> {
    platform_only(&identity)?;
    let viewer = community::viewer(&state.pool, &identity).await?;
    let direct_spaces: BTreeSet<String> = spaces::direct_spaces_of(&state.pool, &identity)
        .await?
        .into_iter()
        .collect();
    let mut allowed = Vec::new();
    for item in payload.items.iter().take(500) {
        let ok = if let Some(post_id) = item.group_key.strip_prefix("post:") {
            match community::load_post(&state.pool, post_id).await? {
                Some(loaded) => {
                    loaded.channel.active
                        && community::post_read_locked_reason(&loaded, &viewer).is_none()
                }
                None => false,
            }
        } else if let Some(notice_id) = item.group_key.strip_prefix("notice:") {
            let row: Option<(String, Option<String>)> =
                sqlx::query_as("SELECT scope, workspace_id FROM announcements WHERE id = $1")
                    .bind(notice_id)
                    .fetch_optional(&state.pool)
                    .await?;
            match row {
                Some((scope, _)) if scope == "company" => true,
                Some((_, Some(space))) => direct_spaces.contains(&space),
                _ => false,
            }
        } else {
            false
        };
        if ok && !allowed.contains(&item.group_key) {
            allowed.push(item.group_key.clone());
        }
    }
    Ok(Json(CheckResponse { allowed }))
}

#[derive(Serialize)]
pub struct HomeCardItem {
    title: String,
    subtitle: String,
    path: String,
    at: String,
}

#[derive(Serialize)]
pub struct HomeCard {
    title: String,
    items: Vec<HomeCardItem>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HomeCardRequest {
    #[serde(default)]
    locale: Option<String>,
}

/// `POST /platform/home-card`: the person's notice card on the portal home.
/// At most five notices they may read, pinned first then newest. Read-only,
/// app DB only (space notices of direct memberships, as above). The body
/// carries the person's display settings; only the language is used, for the
/// card and company labels (Korean unless `en-US`).
pub async fn home_card(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    body: axum::body::Bytes,
) -> ApiResult<Json<HomeCard>> {
    platform_only(&identity)?;
    let request: HomeCardRequest = serde_json::from_slice(&body).unwrap_or_default();
    let english = request.locale.as_deref() == Some("en-US");
    let rows = announcements::direct_notices(&state.pool, &identity, 5).await?;
    Ok(Json(HomeCard {
        title: if english { "Announcements" } else { "공지" }.into(),
        items: rows
            .into_iter()
            .map(
                |NoticeWithSpace {
                     notice: row,
                     space_name,
                 }| HomeCardItem {
                    title: shorten(&row.title, 200),
                    subtitle: if row.scope == "company" {
                        if english { "Company" } else { "전사" }.into()
                    } else {
                        space_name.unwrap_or_default()
                    },
                    path: announcements::notice_path(&row),
                    at: utc_iso(row.created_at),
                },
            )
            .collect(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_compacted_and_bounded() {
        assert_eq!(shorten("  a \n b  ", 10), "a b");
        let long = "가".repeat(250);
        let short = shorten(&long, 200);
        assert_eq!(short.chars().count(), 200);
        assert!(short.ends_with('…'));
    }
}
