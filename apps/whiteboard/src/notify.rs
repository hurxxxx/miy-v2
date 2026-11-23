//! Platform notifications (miy) and the platform → app read-only endpoints.
//!
//! AI-DO sent no notification when a whiteboard was shared. miy publishes one
//! to the platform inbox (`platform:notifications:write`):
//!
//! | kind | when | recipient | group key |
//! | --- | --- | --- | --- |
//! | `share` | a person is newly given a board (direct share) | that person | `whiteboard:<id>` |
//!
//! Changing the access level does not notify again. Removing the share
//! retracts it for that person; moving the board to the trash or deleting it
//! retracts the group for everyone; opening the board marks it read.
//! Publishing happens after the app commit and never undoes it; an uncertain
//! result is looked up by request ID, not sent again.
use axum::{Extension, Json, extract::State};
use miy_app_sdk::{Identity, Notification};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, time::Duration};

use crate::{
    AppState,
    error::{ApiError, ApiJson, ApiResult, ErrorResponse, IdentityErrorResponse},
    people,
    spaces::{self, GLOBAL},
    whiteboard::{Board, TARGET_APP, TARGET_TYPE_WORKSPACE, board_path},
};

const CALL_TIMEOUT: Duration = Duration::from_secs(5);
const TITLE_MAX: usize = 200;

pub fn board_group_key(board_id: &str) -> String {
    format!("whiteboard:{board_id}")
}

pub fn shorten(text: &str, max: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max {
        return compact;
    }
    let mut cut: String = compact.chars().take(max - 1).collect();
    cut = cut.trim_end().to_string();
    cut.push('…');
    cut
}

/// The share line. The app does not know the recipient's language, so the
/// title is Korean (the platform's default language).
pub async fn board_shared(identity: &Identity, board: &Board, recipient: &str) {
    if identity.context.platform || recipient == identity.user_id {
        return;
    }
    let actor = match people::display_names(identity, [identity.user_id.clone()])
        .await
        .remove(&identity.user_id)
    {
        Some(name) => format!("{name}님이"),
        None => "다른 사용자가".to_string(),
    };
    let title = format!(
        "{actor} 화이트보드 \"{}\"을(를) 공유했습니다",
        shorten(&board.title, 120)
    );
    let notification = Notification {
        request_id: uuid::Uuid::new_v4().to_string(),
        recipients: vec![recipient.to_string()],
        kind: "share".to_string(),
        group_key: board_group_key(&board.id),
        title: shorten(&title, TITLE_MAX),
        path: board_path(&board.id, &board.workspace_id),
        resource_id: Some(board.id.clone()),
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

pub async fn retract_board(identity: &Identity, board_id: &str) {
    if identity.context.platform {
        return;
    }
    let _ = tokio::time::timeout(
        CALL_TIMEOUT,
        identity.retract_notifications(&board_group_key(board_id), None),
    )
    .await;
}

pub async fn retract_board_for(identity: &Identity, board_id: &str, recipient: &str) {
    if identity.context.platform || recipient.is_empty() {
        return;
    }
    let recipients = [recipient.to_string()];
    let _ = tokio::time::timeout(
        CALL_TIMEOUT,
        identity.retract_notifications(&board_group_key(board_id), Some(&recipients)),
    )
    .await;
}

pub async fn mark_board_read(identity: &Identity, board_id: &str) {
    if identity.context.platform {
        return;
    }
    let _ = tokio::time::timeout(
        CALL_TIMEOUT,
        identity.mark_notifications_read(&board_group_key(board_id)),
    )
    .await;
}

// ---------------------------------------------------------------- platform endpoints

pub fn platform_only(identity: &Identity) -> ApiResult<()> {
    if identity.context.platform {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "whiteboard.platform_only",
            "Only the platform calls this endpoint.",
        ))
    }
}

/// The SQL condition "the caller may open board `w` now", answerable from the
/// app DB alone (a platform call carries no token for further platform calls).
/// A board in an app space requires current membership: a direct user grant,
/// or a group grant for one of the person's current groups that the platform
/// signed into the call (`identity.context.groups`, see [`signed_groups`]).
/// Binds: `$2` caller, `$3` installation, `$4` environment, `$5` groups.
pub fn readable_board_condition() -> String {
    format!(
        "w.trashed_at IS NULL AND (w.workspace_id = '{GLOBAL}' OR EXISTS (SELECT 1 FROM app.miy_acl_grants g \
                 WHERE g.installation_id = $3::uuid AND g.environment = $4 AND g.kind = '{kind}' \
                   AND g.resource_id = w.workspace_id AND ((g.subject_kind = 'user' AND g.subject_id = $2) \
                     OR (g.subject_kind = 'group' AND g.subject_id = ANY($5::text[]))))) \
         AND (w.owner_id = $2 \
           OR EXISTS (SELECT 1 FROM whiteboard_user_shares s WHERE s.whiteboard_id = w.id AND s.user_id = $2) \
           OR EXISTS (SELECT 1 FROM whiteboard_targets t WHERE t.whiteboard_id = w.id \
                 AND t.target_app = '{TARGET_APP}' AND t.target_type = '{TARGET_TYPE_WORKSPACE}' \
                 AND t.target_id = w.workspace_id))",
        kind = spaces::SPACE_KIND,
    )
}

/// The person's current group IDs signed into a platform call. The platform
/// adds them for an app approved to read the directory; without them only
/// direct grants count. A malformed list is refused like the SDK ACL does.
pub fn signed_groups(identity: &Identity) -> ApiResult<Vec<String>> {
    let groups = identity.context.groups.clone().unwrap_or_default();
    if groups.len() > 1000
        || groups
            .iter()
            .any(|group| uuid::Uuid::parse_str(group).is_err())
    {
        return Err(ApiError::unavailable(
            "acl_directory_unavailable",
            "acl_directory_unavailable",
        ));
    }
    let mut groups: Vec<String> = groups
        .into_iter()
        .map(|group| group.to_ascii_lowercase())
        .collect();
    groups.sort();
    groups.dedup();
    Ok(groups)
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckItem {
    group_key: String,
    #[serde(default)]
    resource_id: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CheckRequest {
    /// The first 500 items are checked.
    #[serde(default)]
    items: Vec<CheckItem>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CheckResponse {
    allowed: Vec<String>,
}

/// `POST /platform/notification-check`: which of the person's notification
/// groups still point at a board they may open now (also through a group's
/// space membership). Read-only.
#[utoipa::path(
    post, path = "/platform/notification-check", operation_id = "checkWhiteboardNotifications", tag = "Platform callbacks",
    request_body = CheckRequest,
    responses(
        (status = 200, description = "Notification group keys the caller may still open", body = CheckResponse),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Only the platform may call this endpoint", body = ErrorResponse),
        (status = 422, description = "Invalid notification check request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "Signed directory information is unavailable", body = ErrorResponse),
    )
)]
pub async fn notification_check(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<CheckRequest>,
) -> ApiResult<Json<CheckResponse>> {
    platform_only(&identity)?;
    let mut asked = Vec::new();
    for item in payload.items.iter().take(500) {
        let Some(board_id) = item.group_key.strip_prefix("whiteboard:") else {
            continue;
        };
        if board_id.is_empty() || item.resource_id.as_deref() != Some(board_id) {
            continue;
        }
        asked.push(board_id.to_string());
    }
    if asked.is_empty() {
        return Ok(Json(CheckResponse {
            allowed: Vec::new(),
        }));
    }
    let sql = format!(
        "SELECT w.id FROM whiteboards w WHERE w.id = ANY($1) AND {}",
        readable_board_condition()
    );
    let readable: BTreeSet<String> = sqlx::query_scalar::<_, String>(&sql)
        .bind(&asked)
        .bind(&identity.user_id)
        .bind(&identity.context.installation_id)
        .bind(identity.context.environment.as_deref())
        .bind(signed_groups(&identity)?)
        .fetch_all(&state.pool)
        .await?
        .into_iter()
        .collect();
    let mut allowed = Vec::new();
    for board_id in asked {
        let key = board_group_key(&board_id);
        if readable.contains(&board_id) && !allowed.contains(&key) {
            allowed.push(key);
        }
    }
    Ok(Json(CheckResponse { allowed }))
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
        assert_eq!(board_group_key("b1"), "whiteboard:b1");
    }
}
