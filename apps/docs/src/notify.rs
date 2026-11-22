//! Platform notifications and the platform → app read-only endpoints.
//!
//! AI-DO docs published no notifications and had no mentions (the BlockNote
//! schema carries a mention inline type, but the docs editor never offers it).
//! miy adds one kind (`platform:notifications:write`):
//!
//! | kind | when | recipient | group key |
//! | --- | --- | --- | --- |
//! | `share` | a person is given access to a doc (new user share) | that person | `doc:<id>` |
//!
//! Removing the share retracts the group for that person; trashing the doc
//! retracts it for everyone; opening the doc (view record) marks it read.
//! Publishing happens after the commit and never undoes it; an uncertain
//! result is looked up by request ID, not sent again.
use axum::{Extension, Json, extract::State};
use miy_app_sdk::{Identity, Notification};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{collections::BTreeSet, time::Duration};

use crate::{
    AppState,
    access::{DOC_COLUMNS, NativeDoc},
    error::{ApiError, ApiJson, ApiResult},
    items, people,
    time::utc_iso,
};

const CALL_TIMEOUT: Duration = Duration::from_secs(5);
const TITLE_MAX: usize = 200;

pub fn doc_group_key(doc_id: &str) -> String {
    format!("doc:{doc_id}")
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

async fn publish(identity: &Identity, notification: Notification) {
    if identity.context.platform || notification.recipients.is_empty() {
        return;
    }
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

/// The share line names the sharer and the doc title, never page content.
/// The app does not know the recipient's language, so it is Korean.
pub async fn doc_shared(identity: &Identity, doc: &NativeDoc, recipient: &str) {
    if recipient == identity.user_id {
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
        "{actor} \"{}\" 문서를 공유했습니다",
        shorten(&doc.title, 120)
    );
    publish(
        identity,
        Notification {
            request_id: uuid::Uuid::new_v4().to_string(),
            recipients: vec![recipient.to_string()],
            kind: "share".into(),
            group_key: doc_group_key(&doc.id),
            title: shorten(&title, TITLE_MAX),
            path: items::doc_path(&doc.id),
            resource_id: Some(doc.id.clone()),
        },
    )
    .await;
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

// ---------------------------------------------------------------- platform endpoints

pub fn platform_only(identity: &Identity) -> ApiResult<()> {
    if identity.context.platform {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "docs.platform_only",
            "Only the platform calls this endpoint.",
        ))
    }
}

/// The readability rule of `access::resolve_access` for a platform call, in
/// one SQL predicate over `d` (a live doc): the owner, a direct user share,
/// or the doc's own space sidebar target with a space role. The global space
/// admits everyone; other spaces count the person's user grants and the
/// grants of the groups in `$4`. A platform call carries no token for the
/// directory; the core signs the person's current groups into it instead
/// (`signed_groups`). `$1` user, `$2` installation, `$3` environment, `$4`
/// group IDs (text[]; empty when unknown, which counts user grants only).
pub const READABLE_BY_CALLER: &str = "d.trashed_at IS NULL AND (\
    d.owner_id = $1 \
    OR EXISTS (SELECT 1 FROM docs_native_doc_user_shares s WHERE s.doc_id = d.id AND s.user_id = $1) \
    OR EXISTS (SELECT 1 FROM docs_doc_targets t WHERE t.doc_id = d.id AND t.target_app = 'docs' \
        AND t.target_type = 'workspace_sidebar' AND t.target_id = d.workspace_id \
        AND (EXISTS (SELECT 1 FROM docs_spaces sp WHERE sp.id = d.workspace_id AND sp.kind = 'app_global') \
          OR EXISTS (SELECT 1 FROM app.miy_acl_grants g WHERE g.installation_id = $2::uuid \
            AND g.environment = $3 AND g.kind = 'space' AND g.resource_id = d.workspace_id \
            AND ((g.subject_kind = 'user' AND g.subject_id = $1) \
              OR (g.subject_kind = 'group' AND g.subject_id = ANY($4::text[]))) \
            AND g.role IN ('viewer', 'member', 'admin')))))";

/// The caller's current group IDs signed into a platform call (the app is
/// approved for `platform:directory:read`); empty otherwise, so only user
/// grants count. Frame requests ask the directory through the SDK ACL.
pub fn signed_groups(identity: &Identity) -> Vec<String> {
    if !identity.context.platform {
        return Vec::new();
    }
    identity
        .context
        .groups
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|group| group.to_ascii_lowercase())
        .collect()
}

/// Docs among `ids` the caller may read now, decided in one statement.
pub async fn readable_docs(
    pool: &PgPool,
    identity: &Identity,
    ids: &[String],
) -> ApiResult<Vec<NativeDoc>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT {DOC_COLUMNS} FROM docs_native_docs d WHERE d.id = ANY($5) AND {READABLE_BY_CALLER}"
    );
    Ok(sqlx::query_as::<_, NativeDoc>(&sql)
        .bind(&identity.user_id)
        .bind(&identity.context.installation_id)
        .bind(identity.context.environment.as_deref())
        .bind(signed_groups(identity))
        .bind(ids)
        .fetch_all(pool)
        .await?)
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
    #[serde(default)]
    items: Vec<CheckItem>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CheckResponse {
    allowed: Vec<String>,
}

/// Can `user_id` read the live page `page_id` of doc `doc_id` now (the
/// platform-call rule, decided in one statement)? Used for comment
/// notification recipients and their retraction. Another person's groups are
/// not known to the app (the directory answers only the caller's own), so
/// only their user grants count here: someone who reads the page only through
/// a group is left out of comment notifications (fail closed).
pub async fn user_can_read_page(
    pool: &PgPool,
    identity: &Identity,
    user_id: &str,
    doc_id: &str,
    page_id: &str,
) -> ApiResult<bool> {
    let sql = format!(
        "SELECT EXISTS(SELECT 1 FROM docs_native_docs d JOIN docs_native_doc_pages pg ON pg.doc_id = d.id \
         WHERE d.id = $5 AND pg.id = $6 AND pg.trashed_at IS NULL AND {READABLE_BY_CALLER})"
    );
    let groups = if user_id == identity.user_id {
        signed_groups(identity)
    } else {
        Vec::new()
    };
    Ok(sqlx::query_scalar(&sql)
        .bind(user_id)
        .bind(&identity.context.installation_id)
        .bind(identity.context.environment.as_deref())
        .bind(groups)
        .bind(doc_id)
        .bind(page_id)
        .fetch_one(pool)
        .await?)
}

/// `POST /platform/notification-check`: which groups still point at
/// something the person may read now. `doc:<id>` (share) needs the doc;
/// `comment:<id>` and `mention:<id>` need the comment (not deleted) on a live
/// page of a doc the person may read. The resource ID must match the key.
/// Read-only, app DB only.
#[utoipa::path(
    post, path = "/platform/notification-check", operation_id = "checkDocsNotifications", tag = "Platform callbacks",
    request_body = CheckRequest,
    responses(
        (status = 200, description = "Success", body = CheckResponse),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn notification_check(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<CheckRequest>,
) -> ApiResult<Json<CheckResponse>> {
    platform_only(&identity)?;
    let mut docs: Vec<(String, String)> = Vec::new();
    let mut comments: Vec<(String, String)> = Vec::new();
    for item in payload.items.iter().take(500) {
        let Some((kind, id)) = item.group_key.split_once(':') else {
            continue;
        };
        if item.resource_id.as_deref() != Some(id) {
            continue;
        }
        match kind {
            "doc" => docs.push((item.group_key.clone(), id.to_string())),
            "comment" | "mention" => comments.push((item.group_key.clone(), id.to_string())),
            _ => {}
        }
    }
    let doc_ids: Vec<String> = docs.iter().map(|(_, id)| id.clone()).collect();
    let readable: BTreeSet<String> = readable_docs(&state.pool, &identity, &doc_ids)
        .await?
        .into_iter()
        .map(|doc| doc.id)
        .collect();
    let comment_ids: Vec<String> = comments.iter().map(|(_, id)| id.clone()).collect();
    let readable_comments: BTreeSet<String> = if comment_ids.is_empty() {
        BTreeSet::new()
    } else {
        let sql = format!(
            "SELECT c.id FROM docs_page_comments c \
             JOIN docs_native_doc_pages pg ON pg.id = c.page_id \
             JOIN docs_native_docs d ON d.id = pg.doc_id \
             WHERE c.id = ANY($5) AND c.deleted_at IS NULL AND pg.trashed_at IS NULL \
               AND {READABLE_BY_CALLER}"
        );
        sqlx::query_scalar(&sql)
            .bind(&identity.user_id)
            .bind(&identity.context.installation_id)
            .bind(identity.context.environment.as_deref())
            .bind(signed_groups(&identity))
            .bind(&comment_ids)
            .fetch_all(&state.pool)
            .await?
            .into_iter()
            .collect()
    };
    let mut allowed: Vec<String> = Vec::new();
    for (group_key, id) in docs {
        if readable.contains(&id) && !allowed.contains(&group_key) {
            allowed.push(group_key);
        }
    }
    for (group_key, id) in comments {
        if readable_comments.contains(&id) && !allowed.contains(&group_key) {
            allowed.push(group_key);
        }
    }
    Ok(Json(CheckResponse { allowed }))
}

#[derive(Deserialize, Default, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DisplayRequest {
    #[serde(default)]
    locale: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct HomeCard {
    title: &'static str,
    #[schema(max_items = 5)]
    items: Vec<HomeCardItem>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct HomeCardItem {
    title: String,
    #[schema(required)]
    subtitle: Option<String>,
    path: String,
    #[schema(format = DateTime)]
    at: String,
}

/// `POST /v1/home-card`: "최근 문서" — the pages the person opened last whose
/// doc they may still read now (at most five, newest first). Read-only.
#[utoipa::path(
    post, path = "/v1/home-card", operation_id = "getDocsHomeCard", tag = "Platform callbacks",
    request_body = Option<DisplayRequest>,
    responses(
        (status = 200, description = "Success", body = HomeCard),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 413, description = "Request exceeds the size limit", body = String, content_type = "text/plain"),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse)
    )
)]
pub async fn home_card(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    body: axum::body::Bytes,
) -> ApiResult<Json<HomeCard>> {
    platform_only(&identity)?;
    let request: DisplayRequest = serde_json::from_slice(&body).unwrap_or_default();
    let english = request.locale.as_deref() == Some("en-US");
    let sql = format!(
        "SELECT d.id, d.title, p.last_viewed_at, pg.id AS page_id, pg.title AS page_title \
         FROM docs_user_item_prefs p JOIN docs_native_docs d ON d.id = p.source_doc_id \
         LEFT JOIN docs_native_doc_pages pg ON pg.id = p.last_viewed_page_source_id \
           AND pg.doc_id = d.id AND pg.trashed_at IS NULL \
         WHERE p.user_id = $1 AND p.source_type = 'native_doc' AND p.last_viewed_at IS NOT NULL \
           AND {READABLE_BY_CALLER} \
         ORDER BY p.last_viewed_at DESC LIMIT 5"
    );
    let rows: Vec<(
        String,
        String,
        chrono::NaiveDateTime,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(&sql)
        .bind(&identity.user_id)
        .bind(&identity.context.installation_id)
        .bind(identity.context.environment.as_deref())
        .bind(signed_groups(&identity))
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(HomeCard {
        title: if english {
            "Recent docs"
        } else {
            "최근 문서"
        },
        items: rows
            .into_iter()
            .map(|(doc_id, doc_title, at, page_id, page_title)| {
                let (title, subtitle) = match &page_title {
                    Some(page_title) if page_title != &doc_title => {
                        (page_title.clone(), Some(doc_title.clone()))
                    }
                    _ => (doc_title.clone(), None),
                };
                let mut path = items::doc_path(&doc_id);
                if let Some(page_id) = page_id {
                    path.push_str(&format!("?page={}", items::urlencode_segment(&page_id)));
                }
                HomeCardItem {
                    title: shorten(&title, 160),
                    subtitle: subtitle.map(|s| shorten(&s, 160)),
                    path,
                    at: utc_iso(at),
                }
            })
            .collect(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_compacted_and_bounded() {
        assert_eq!(shorten("  a \n b  ", 10), "a b");
        let short = shorten(&"가".repeat(250), 200);
        assert_eq!(short.chars().count(), 200);
        assert!(short.ends_with('…'));
        assert_eq!(doc_group_key("d1"), "doc:d1");
    }
}
