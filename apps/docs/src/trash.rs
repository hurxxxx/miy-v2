//! Trash (보강, not in AI-DO 0002af062: AI-DO trashed docs and pages but had
//! no restore, and released their files at once).
//!
//! - `GET /v1/docs/trash?space=` lists what was moved to the trash in a space:
//!   trashed docs the caller may manage, and pages of live docs the caller may
//!   edit, one entry per deletion (a page and the subpages trashed with it).
//! - Restoring a doc restores the pages trashed with it (same moment); pages
//!   trashed earlier stay in the trash as page entries. Restoring a page
//!   restores the subpages trashed with it. If its parent is still in the
//!   trash, the page goes to the top of the doc's page tree (the parent is
//!   not restored with it).
//! - Permanent deletion (the doc's owner or a space admin, i.e. who may
//!   manage the doc) removes the rows; the attachments and images are marked
//!   for release in the same transaction and detached from the core after the
//!   commit (files.rs). There is no automatic retention: the trash empties
//!   only by hand.
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use sqlx::{FromRow, Postgres, Transaction};
use std::collections::{HashMap, HashSet};

use crate::{
    AppState,
    access::{self, DocContext, NativePage, PAGE_COLUMNS},
    error::{ApiError, ApiResult},
    files, items,
    items::SpaceQuery,
    people, spaces,
    time::{naive_iso, now_utc},
};

fn not_in_trash() -> ApiError {
    ApiError::conflict("docs.not_in_trash", "The item is not in the trash.")
}

#[derive(FromRow)]
struct TrashedDoc {
    id: String,
    title: String,
    trashed_at: NaiveDateTime,
    trashed_by_id: Option<String>,
}

#[derive(FromRow)]
struct TrashedPage {
    id: String,
    doc_id: String,
    doc_title: String,
    parent_id: Option<String>,
    parent_trashed_at: Option<NaiveDateTime>,
    title: String,
    trashed_at: NaiveDateTime,
    trashed_by_id: Option<String>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
enum TrashKind {
    Doc,
    Page,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
struct TrashEntry {
    kind: TrashKind,
    id: String,
    doc_id: String,
    title: String,
    doc_title: String,
    trashed_at: String,
    #[schema(required)]
    trashed_by_id: Option<String>,
    #[schema(required)]
    trashed_by_name: Option<String>,
    can_restore: bool,
    can_delete: bool,
    /// Present for pages only.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = i64)]
    subpage_count: Option<i64>,
    /// Present for pages only.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = bool)]
    parent_in_trash: Option<bool>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct TrashList {
    items: Vec<TrashEntry>,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct RestoredPage {
    page: crate::pages::PageItem,
    restored_count: usize,
    moved_to_root: bool,
}

/// `GET /v1/docs/trash?space=`: newest first.
#[utoipa::path(
    get, path = "/v1/docs/trash", operation_id = "listDocsTrash", tag = "Trash",
    params(SpaceQuery),
    responses(
        (status = 200, description = "Success", body = TrashList),
        (status = 400, description = "Invalid request", body = String, content_type = "text/plain"),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn list(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Query(query): Query<SpaceQuery>,
) -> ApiResult<Json<TrashList>> {
    let (space, role) = spaces::require_space(&state, &identity, query.space()).await?;
    let docs: Vec<TrashedDoc> = sqlx::query_as(
        "SELECT id, title, trashed_at, trashed_by_id FROM docs_native_docs \
         WHERE workspace_id = $1 AND trashed_at IS NOT NULL ORDER BY trashed_at DESC LIMIT 500",
    )
    .bind(&space.id)
    .fetch_all(&state.pool)
    .await?;
    // One entry per deletion: a trashed page whose parent is live or was
    // trashed at another moment.
    let pages: Vec<TrashedPage> = sqlx::query_as(
        "SELECT p.id, p.doc_id, d.title AS doc_title, p.parent_id, pp.trashed_at AS parent_trashed_at, \
                p.title, p.trashed_at, p.trashed_by_id \
         FROM docs_native_doc_pages p JOIN docs_native_docs d ON d.id = p.doc_id \
         LEFT JOIN docs_native_doc_pages pp ON pp.id = p.parent_id \
         WHERE d.workspace_id = $1 AND d.trashed_at IS NULL AND p.trashed_at IS NOT NULL \
           AND (pp.id IS NULL OR pp.trashed_at IS NULL OR pp.trashed_at <> p.trashed_at) \
         ORDER BY p.trashed_at DESC LIMIT 500",
    )
    .bind(&space.id)
    .fetch_all(&state.pool)
    .await?;
    let mut viewer = access::Viewer::new(&state, &identity);
    viewer.remember(&space.id, Some(role));
    let mut doc_access: HashMap<String, access::NativeAccess> = HashMap::new();
    let mut needed: HashSet<String> = docs.iter().map(|doc| doc.id.clone()).collect();
    needed.extend(pages.iter().map(|page| page.doc_id.clone()));
    for doc_id in needed {
        if let Some(doc) = access::load_doc(&state.pool, &doc_id).await? {
            let targets = access::load_targets(&state.pool, &doc.id).await?;
            doc_access.insert(doc_id, viewer.access(&doc, &targets).await?);
        }
    }
    let counts: HashMap<String, i64> = if pages.is_empty() {
        HashMap::new()
    } else {
        let ids: Vec<String> = pages.iter().map(|page| page.id.clone()).collect();
        sqlx::query_as::<_, (String, i64)>(
            "WITH RECURSIVE batch(root, id, trashed_at) AS ( \
               SELECT id, id, trashed_at FROM docs_native_doc_pages WHERE id = ANY($1) \
               UNION ALL SELECT b.root, c.id, c.trashed_at FROM docs_native_doc_pages c \
               JOIN batch b ON c.parent_id = b.id AND c.trashed_at = b.trashed_at) \
             SELECT root, count(*) - 1 FROM batch GROUP BY root",
        )
        .bind(&ids)
        .fetch_all(&state.pool)
        .await?
        .into_iter()
        .collect()
    };
    let mut people_ids: Vec<String> = docs
        .iter()
        .filter_map(|d| d.trashed_by_id.clone())
        .collect();
    people_ids.extend(pages.iter().filter_map(|p| p.trashed_by_id.clone()));
    let names = people::display_names(&identity, people_ids).await;
    let name_of = |id: &Option<String>| id.as_ref().and_then(|id| names.get(id).cloned());
    let mut entries: Vec<(NaiveDateTime, TrashEntry)> = Vec::new();
    for doc in &docs {
        let Some(access) = doc_access.get(&doc.id) else {
            continue;
        };
        if !access.can_manage {
            continue;
        }
        entries.push((
            doc.trashed_at,
            TrashEntry {
                kind: TrashKind::Doc,
                id: doc.id.clone(),
                doc_id: doc.id.clone(),
                title: doc.title.clone(),
                doc_title: doc.title.clone(),
                trashed_at: naive_iso(doc.trashed_at),
                trashed_by_id: doc.trashed_by_id.clone(),
                trashed_by_name: name_of(&doc.trashed_by_id),
                can_restore: true,
                can_delete: true,
                subpage_count: None,
                parent_in_trash: None,
            },
        ));
    }
    for page in &pages {
        let Some(access) = doc_access.get(&page.doc_id) else {
            continue;
        };
        if !access.can_edit {
            continue;
        }
        entries.push((
            page.trashed_at,
            TrashEntry {
                kind: TrashKind::Page,
                id: page.id.clone(),
                doc_id: page.doc_id.clone(),
                title: page.title.clone(),
                doc_title: page.doc_title.clone(),
                subpage_count: Some(counts.get(&page.id).copied().unwrap_or(0)),
                parent_in_trash: Some(page.parent_id.is_some() && page.parent_trashed_at.is_some()),
                trashed_at: naive_iso(page.trashed_at),
                trashed_by_id: page.trashed_by_id.clone(),
                trashed_by_name: name_of(&page.trashed_by_id),
                can_restore: true,
                can_delete: access.can_manage,
            },
        ));
    }
    entries.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(Json(TrashList {
        items: entries.into_iter().map(|(_, item)| item).collect(),
    }))
}

/// Pages trashed together with `root` (its subtree with the same moment).
async fn batch_of(tx: &mut Transaction<'_, Postgres>, root: &NativePage) -> ApiResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        "WITH RECURSIVE batch(id) AS ( \
           SELECT id FROM docs_native_doc_pages WHERE id = $1 \
           UNION ALL SELECT c.id FROM docs_native_doc_pages c JOIN batch b ON c.parent_id = b.id \
           WHERE c.trashed_at = $2) SELECT id FROM batch",
    )
    .bind(&root.id)
    .bind(root.trashed_at)
    .fetch_all(&mut **tx)
    .await?)
}

/// Every page under `root`, live or trashed (all go with a permanent delete).
async fn all_descendants(
    tx: &mut Transaction<'_, Postgres>,
    root_id: &str,
) -> ApiResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        "WITH RECURSIVE tree(id) AS ( \
           SELECT id FROM docs_native_doc_pages WHERE id = $1 \
           UNION ALL SELECT c.id FROM docs_native_doc_pages c JOIN tree t ON c.parent_id = t.id) \
         SELECT id FROM tree",
    )
    .bind(root_id)
    .fetch_all(&mut **tx)
    .await?)
}

async fn lock_trashed_doc(
    tx: &mut Transaction<'_, Postgres>,
    state: &AppState,
    identity: &Identity,
    item_id: &str,
) -> ApiResult<DocContext> {
    let context = items::lock_item(tx, state, identity, item_id).await?;
    if !context.access.can_manage {
        return Err(items::doc_manage_required());
    }
    if context.doc.trashed_at.is_none() {
        return Err(not_in_trash());
    }
    Ok(context)
}

/// `POST /v1/docs/trash/items/{id}/restore`.
#[utoipa::path(
    post, path = "/v1/docs/trash/items/{item_id}/restore", operation_id = "restoreDocsItem", tag = "Trash",
    params(("item_id" = String, Path)),
    responses(
        (status = 200, description = "Success", body = items::HubItem),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 409, description = "Current resource state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn restore_doc(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(item_id): Path<String>,
) -> ApiResult<Json<items::HubItem>> {
    let mut tx = state.pool.begin().await?;
    let context = lock_trashed_doc(&mut tx, &state, &identity, &item_id).await?;
    let now = now_utc();
    sqlx::query(
        "UPDATE docs_native_doc_pages SET trashed_at = NULL, trashed_by_id = NULL, updated_at = $3 \
         WHERE doc_id = $1 AND trashed_at = $2",
    )
    .bind(&context.doc.id)
    .bind(context.doc.trashed_at)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE docs_native_docs SET trashed_at = NULL, trashed_by_id = NULL, updated_at = $2 WHERE id = $1",
    )
    .bind(&context.doc.id)
    .bind(now.max(context.doc.updated_at + chrono::Duration::microseconds(1)))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let mut viewer = access::Viewer::new(&state, &identity);
    Ok(Json(
        items::lookup_item(&mut viewer, &context.doc.id).await?,
    ))
}

/// `DELETE /v1/docs/trash/items/{id}`: remove the doc for good.
#[utoipa::path(
    delete, path = "/v1/docs/trash/items/{item_id}", operation_id = "purgeDocsItem", tag = "Trash",
    params(("item_id" = String, Path)),
    responses(
        (status = 204, description = "Completed with no response body"),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 409, description = "Current resource state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn purge_doc(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(item_id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.pool.begin().await?;
    let context = lock_trashed_doc(&mut tx, &state, &identity, &item_id).await?;
    let page_ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM docs_native_doc_pages WHERE doc_id = $1")
            .bind(&context.doc.id)
            .fetch_all(&mut *tx)
            .await?;
    let released = files::release_for_pages(&mut tx, &page_ids).await?;
    sqlx::query("DELETE FROM docs_native_docs WHERE id = $1")
        .bind(&context.doc.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM docs_user_item_prefs WHERE source_doc_id = $1")
        .bind(&context.doc.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    files::detach_released(&state.pool, &identity, &released).await;
    Ok(StatusCode::NO_CONTENT)
}

/// Lock the doc of a trashed page (the doc itself must be live).
async fn lock_trashed_page(
    tx: &mut Transaction<'_, Postgres>,
    state: &AppState,
    identity: &Identity,
    page_id: &str,
) -> ApiResult<(DocContext, NativePage)> {
    let page_id = access::normalize_page_id(page_id)?;
    let page = access::load_page(&state.pool, &page_id)
        .await?
        .ok_or_else(access::page_not_found)?;
    let context = access::lock_doc_for_write(tx, state, identity, &page.doc_id).await?;
    if !context.access.can_view || context.doc.trashed_at.is_some() {
        return Err(access::page_not_found());
    }
    let sql = format!("SELECT {PAGE_COLUMNS} FROM docs_native_doc_pages WHERE id = $1 FOR UPDATE");
    let page = sqlx::query_as::<_, NativePage>(&sql)
        .bind(&page_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(access::page_not_found)?;
    if page.trashed_at.is_none() {
        return Err(not_in_trash());
    }
    Ok((context, page))
}

/// `POST /v1/docs/trash/pages/{id}/restore`: the page and the subpages
/// trashed with it; to the top of the tree when its parent is still trashed.
#[utoipa::path(
    post, path = "/v1/docs/trash/pages/{page_id}/restore", operation_id = "restoreDocsPage", tag = "Trash",
    params(("page_id" = String, Path)),
    responses(
        (status = 200, description = "Success", body = RestoredPage),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 409, description = "Current resource state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn restore_page(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(page_id): Path<String>,
) -> ApiResult<Json<RestoredPage>> {
    let mut tx = state.pool.begin().await?;
    let (context, page) = lock_trashed_page(&mut tx, &state, &identity, &page_id).await?;
    if !context.access.can_edit {
        return Err(items::doc_edit_required());
    }
    let batch = batch_of(&mut tx, &page).await?;
    let parent_live: bool = match &page.parent_id {
        Some(parent_id) => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM docs_native_doc_pages WHERE id = $1 AND trashed_at IS NULL)",
        )
        .bind(parent_id)
        .fetch_one(&mut *tx)
        .await?,
        None => true,
    };
    let now = now_utc();
    sqlx::query(
        "UPDATE docs_native_doc_pages SET trashed_at = NULL, trashed_by_id = NULL, updated_at = $2 \
         WHERE id = ANY($1)",
    )
    .bind(&batch)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    if !parent_live {
        let siblings: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM docs_native_doc_pages WHERE doc_id = $1 AND parent_id IS NULL \
             AND trashed_at IS NULL",
        )
        .bind(&context.doc.id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE docs_native_doc_pages SET parent_id = NULL, sort_order = $2 WHERE id = $1",
        )
        .bind(&page.id)
        .bind(siblings as i32)
        .execute(&mut *tx)
        .await?;
    }
    items::touch_doc(&mut tx, &context.doc).await?;
    let restored = access::load_page_in(&mut tx, &page.id).await?;
    tx.commit().await?;
    Ok(Json(RestoredPage {
        page: crate::pages::page_json(&identity, &restored, context.access.can_edit).await,
        restored_count: batch.len(),
        moved_to_root: !parent_live,
    }))
}

/// `DELETE /v1/docs/trash/pages/{id}`: remove the page and everything under
/// it for good (who may manage the doc).
#[utoipa::path(
    delete, path = "/v1/docs/trash/pages/{page_id}", operation_id = "purgeDocsPage", tag = "Trash",
    params(("page_id" = String, Path)),
    responses(
        (status = 204, description = "Completed with no response body"),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 409, description = "Current resource state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn purge_page(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(page_id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.pool.begin().await?;
    let (context, page) = lock_trashed_page(&mut tx, &state, &identity, &page_id).await?;
    if !context.access.can_manage {
        return Err(items::doc_manage_required());
    }
    let ids = all_descendants(&mut tx, &page.id).await?;
    let live_below: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM docs_native_doc_pages WHERE id = ANY($1) AND trashed_at IS NULL)",
    )
    .bind(&ids)
    .fetch_one(&mut *tx)
    .await?;
    if live_below {
        return Err(not_in_trash());
    }
    let released = files::release_for_pages(&mut tx, &ids).await?;
    sqlx::query("DELETE FROM docs_native_doc_pages WHERE id = $1")
        .bind(&page.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    files::detach_released(&state.pool, &identity, &released).await;
    Ok(StatusCode::NO_CONTENT)
}
