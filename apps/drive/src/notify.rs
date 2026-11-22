//! Platform notifications (`platform:notifications:write`).
//!
//! AI-DO's files module sent none. Drive tells people when a space is shared
//! with them, directly or through a directory group (`notify_with_groups`;
//! kind `share`, group `space:<id>`, path `/spaces/<id>`), retracts it from
//! people taken off the space and from everyone when the space is deleted,
//! and marks it read when they open the space. Members of a group taken off
//! a space lose the notification through the platform recheck
//! (notificationCheck), since the app cannot list a group's members. Publishing
//! happens after the app commit and never undoes it; an uncertain result is
//! looked up by its request ID, not sent again.
use miy_app_sdk::{Identity, Notification};
use std::{collections::BTreeSet, time::Duration};

const CALL_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RECIPIENTS_PER_CALL: usize = 500;
const MAX_GROUPS_PER_CALL: usize = 50;
const TITLE_MAX: usize = 200;

pub const KIND_SHARE: &str = "share";

pub fn space_group_key(space_id: &str) -> String {
    format!("space:{space_id}")
}

pub fn space_path(space_id: &str) -> String {
    format!("/spaces/{space_id}")
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

/// "<name>님이 "<space>" 공간을 공유했습니다" (Korean only: the app does not
/// know the recipients' languages).
pub fn share_title(sharer: Option<&str>, space_name: &str) -> String {
    match sharer.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => format!("{name}님이 \"{space_name}\" 공간을 공유했습니다"),
        None => format!("\"{space_name}\" 공간이 공유되었습니다"),
    }
}

/// Publish to `recipients` (people) and the current members of `groups` in
/// batches (500 people, 50 groups per call). Best effort.
#[allow(clippy::too_many_arguments)]
pub async fn publish(
    identity: &Identity,
    recipients: Vec<String>,
    groups: Vec<String>,
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
        .filter(|id| !id.is_empty() && *id != identity.user_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let groups: Vec<String> = groups
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let user_chunks: Vec<&[String]> = recipients.chunks(MAX_RECIPIENTS_PER_CALL).collect();
    let group_chunks: Vec<&[String]> = groups.chunks(MAX_GROUPS_PER_CALL).collect();
    for index in 0..user_chunks.len().max(group_chunks.len()) {
        let people = user_chunks.get(index).copied().unwrap_or(&[]);
        let teams = group_chunks.get(index).copied().unwrap_or(&[]);
        let notification = Notification {
            request_id: uuid::Uuid::new_v4().to_string(),
            recipients: people.to_vec(),
            kind: kind.to_string(),
            group_key: group_key.to_string(),
            title: shorten(title, TITLE_MAX),
            path: path.to_string(),
            resource_id: Some(resource_id.to_string()),
        };
        let call = async {
            if teams.is_empty() {
                identity.notify(&notification).await
            } else {
                identity.notify_with_groups(&notification, teams).await
            }
        };
        match tokio::time::timeout(CALL_TIMEOUT, call).await {
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
}

/// Retract a group for everyone.
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

/// Retract a group for some people.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_titles_carry_no_file_content() {
        assert_eq!(
            share_title(Some("김민지"), "설계팀"),
            "김민지님이 \"설계팀\" 공간을 공유했습니다"
        );
        assert_eq!(
            share_title(None, "설계팀"),
            "\"설계팀\" 공간이 공유되었습니다"
        );
        assert_eq!(shorten(&"가".repeat(300), 200).chars().count(), 200);
    }
}
