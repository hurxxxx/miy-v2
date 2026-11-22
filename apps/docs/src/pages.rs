//! Doc pages: AI-DO domains/docs/page_mutations.py (create, update, delete)
//! and the page routes of router.py/service.py (list, read).
//!
//! Real-time co-editing is excluded. AI-DO kept a Yjs document per page
//! (`docs_collab_documents`) and saved snapshots from the collaborative
//! editor; here one person edits at a time and every content save carries
//! `expected_revision`. A save against an older revision answers 409
//! `docs_page_revision_conflict` and changes nothing; a saved content change
//! increments `revision`. Titles, parents and order are page metadata and do
//! not take part in the revision check.
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use miy_app_sdk::Identity;
use serde::Deserialize;
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use std::collections::{HashMap, HashSet};

use crate::{
    AppState,
    access::{self, NativePage, PAGE_COLUMNS, Require, Viewer},
    comments,
    content_text::extract_blocks_text,
    error::{ApiError, ApiJson, ApiResult, check_length},
    items::{self, doc_edit_required, present, touch_doc},
    media, people, templates,
    time::{naive_iso, now_utc},
    versions,
};

#[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
enum PageSource {
    NativeDocPage,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct PageItem {
    id: String,
    doc_id: String,
    source_type: PageSource,
    source_page_id: String,
    #[schema(required)]
    parent_id: Option<String>,
    title: String,
    #[schema(required, value_type = Option<Vec<std::collections::BTreeMap<String, Value>>>)]
    content_blocks: Option<Value>,
    #[schema(required)]
    content_text: Option<String>,
    #[schema(value_type = items::ContentFormat)]
    content_format: String,
    sort_order: i32,
    created_by_id: String,
    created_by_name: String,
    created_at: String,
    updated_at: String,
    #[schema(required)]
    trashed_at: Option<String>,
    can_edit: bool,
    realtime_collab: bool,
    revision: i64,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct PageListResponse {
    items: Vec<PageItem>,
}

/// Includes the saved revision used by page updates and version restores.
fn page_item(page: &NativePage, can_edit: bool, creator_name: &str) -> PageItem {
    PageItem {
        id: page.id.clone(),
        doc_id: page.doc_id.clone(),
        source_type: PageSource::NativeDocPage,
        source_page_id: page.id.clone(),
        parent_id: page.parent_id.clone(),
        title: page.title.clone(),
        content_blocks: page.content_blocks.clone(),
        content_text: page.content_text.clone(),
        content_format: page.content_format.clone(),
        sort_order: page.sort_order,
        created_by_id: page.created_by_id.clone(),
        created_by_name: creator_name.to_owned(),
        created_at: naive_iso(page.created_at),
        updated_at: naive_iso(page.updated_at),
        trashed_at: page.trashed_at.map(naive_iso),
        can_edit,
        realtime_collab: false,
        revision: page.revision,
    }
}

async fn page_items(identity: &Identity, pages: &[NativePage], can_edit: bool) -> Vec<PageItem> {
    let names = people::display_names(
        identity,
        pages.iter().map(|page| page.created_by_id.clone()),
    )
    .await;
    pages
        .iter()
        .map(|page| {
            page_item(
                page,
                can_edit,
                names
                    .get(&page.created_by_id)
                    .map(String::as_str)
                    .unwrap_or(""),
            )
        })
        .collect()
}

#[utoipa::path(
    get, path = "/v1/docs/items/{item_id}/pages", operation_id = "listDocsPages", tag = "Pages",
    params(("item_id" = String, Path)),
    responses(
        (status = 200, description = "Success", body = PageListResponse),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn list_pages(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(item_id): Path<String>,
) -> ApiResult<Json<PageListResponse>> {
    let mut viewer = Viewer::new(&state, &identity);
    let context = access::doc_or_404(&mut viewer, &item_id).await?;
    let sql = format!(
        "SELECT {PAGE_COLUMNS} FROM docs_native_doc_pages WHERE doc_id = $1 AND trashed_at IS NULL \
         ORDER BY COALESCE(parent_id, ''), sort_order, created_at"
    );
    let pages = sqlx::query_as::<_, NativePage>(&sql)
        .bind(&context.doc.id)
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(PageListResponse {
        items: page_items(&identity, &pages, context.access.can_edit).await,
    }))
}

#[utoipa::path(
    get, path = "/v1/docs/pages/{page_id}", operation_id = "getDocsPage", tag = "Pages",
    params(("page_id" = String, Path)),
    responses(
        (status = 200, description = "Success", body = PageItem),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn get_page(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(page_id): Path<String>,
) -> ApiResult<Json<PageItem>> {
    let mut viewer = Viewer::new(&state, &identity);
    let context = access::page_or_403(&mut viewer, &page_id, Require::View).await?;
    let mut items = page_items(
        &identity,
        std::slice::from_ref(&context.page),
        context.access.can_edit,
    )
    .await;
    Ok(Json(items.remove(0)))
}

async fn active_pages(
    tx: &mut Transaction<'_, Postgres>,
    doc_id: &str,
) -> ApiResult<Vec<NativePage>> {
    let sql = format!(
        "SELECT {PAGE_COLUMNS} FROM docs_native_doc_pages WHERE doc_id = $1 AND trashed_at IS NULL FOR UPDATE"
    );
    Ok(sqlx::query_as::<_, NativePage>(&sql)
        .bind(doc_id)
        .fetch_all(&mut **tx)
        .await?)
}

/// AI-DO access_context.py `validate_native_parent`.
pub fn validate_parent(
    pages: &[NativePage],
    parent_id: Option<&str>,
    page_id: Option<&str>,
) -> ApiResult<()> {
    let Some(parent_id) = parent_id else {
        return Ok(());
    };
    let by_id: HashMap<&str, &NativePage> =
        pages.iter().map(|page| (page.id.as_str(), page)).collect();
    let parent = by_id.get(parent_id).ok_or_else(|| {
        ApiError::not_found("docs.parent_page_not_found", "Parent page not found.")
    })?;
    if page_id == Some(parent.id.as_str()) {
        return Err(ApiError::conflict(
            "docs.page_cannot_be_own_parent",
            "Page cannot be its own parent.",
        ));
    }
    let cycle = || {
        ApiError::conflict(
            "docs.page_parent_cycle",
            "Page parent relationship cannot contain a cycle.",
        )
    };
    let mut ancestor = Some(*parent);
    let mut visited = HashSet::new();
    while let Some(current) = ancestor {
        if !visited.insert(current.id.as_str()) {
            return Err(cycle());
        }
        if page_id.is_some() && current.parent_id.as_deref() == page_id {
            return Err(cycle());
        }
        ancestor = current
            .parent_id
            .as_deref()
            .and_then(|id| by_id.get(id).copied());
    }
    Ok(())
}

/// Pydantic `list[dict] | None`.
fn validate_blocks(value: &Option<Value>) -> ApiResult<()> {
    match value {
        None => Ok(()),
        Some(Value::Array(items)) if items.iter().all(Value::is_object) => Ok(()),
        Some(_) => Err(ApiError::validation(
            "content_blocks: Input should be a valid list of objects",
        )),
    }
}

fn format_mismatch() -> ApiError {
    ApiError::bad_request(
        "docs.content_format_mismatch",
        "The request body does not match the doc content format.",
    )
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreatePageRequest {
    #[schema(min_length = 1, max_length = 200)]
    title: String,
    #[serde(default)]
    parent_id: Option<String>,
    #[serde(default)]
    #[schema(value_type = Option<items::ContentFormat>)]
    content_format: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Vec<std::collections::BTreeMap<String, Value>>>)]
    content_blocks: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    /// A block page rejects any request that includes this field.
    #[schema(ignore)]
    content_text: Option<Option<String>>,
    #[serde(default)]
    sort_order: Option<i32>,
    /// miy (보강): start the page from a template of the doc's space.
    #[serde(default)]
    template_id: Option<String>,
}

/// AI-DO `create_native_page`.
#[utoipa::path(
    post, path = "/v1/docs/items/{item_id}/pages", operation_id = "createDocsPage", tag = "Pages",
    params(("item_id" = String, Path)),
    request_body = CreatePageRequest,
    responses(
        (status = 201, description = "Created", body = PageItem),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn create_page(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(item_id): Path<String>,
    ApiJson(payload): ApiJson<CreatePageRequest>,
) -> ApiResult<impl IntoResponse> {
    check_length("title", &payload.title, 1, 200)?;
    items::validate_content_format(payload.content_format.as_deref())?;
    if let Some(blocks) = &payload.content_blocks {
        validate_blocks(blocks)?;
    }
    if payload.content_text.is_some() {
        return Err(format_mismatch());
    }
    let mut tx = state.pool.begin().await?;
    let context = items::lock_item(&mut tx, &state, &identity, &item_id).await?;
    if !context.access.can_edit {
        return Err(doc_edit_required());
    }
    let parent_id = payload
        .parent_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .map(access::normalize_page_id)
        .transpose()?;
    let pages = active_pages(&mut tx, &context.doc.id).await?;
    validate_parent(&pages, parent_id.as_deref(), None)?;
    let siblings = pages
        .iter()
        .filter(|page| page.parent_id == parent_id)
        .count() as i32;
    let mut blocks: Option<Value> = payload.content_blocks.clone().flatten();
    if let Some(template_id) = payload.template_id.as_deref().filter(|id| !id.is_empty()) {
        if payload.content_blocks.is_some() {
            return Err(format_mismatch());
        }
        blocks = Some(
            templates::content_for(
                &mut tx,
                &state,
                &identity,
                template_id,
                &context.doc.workspace_id,
            )
            .await?,
        );
    }
    let search_text = extract_blocks_text(blocks.as_ref());
    let page_id = uuid::Uuid::new_v4().to_string();
    let now = now_utc();
    let sql = format!(
        "INSERT INTO docs_native_doc_pages (id, doc_id, parent_id, title, content_format, content_blocks, \
         sort_order, created_by_id, created_at, updated_at, search_text) \
         VALUES ($1,$2,$3,$4,'block',$5,$6,$7,$8,$8,$9) RETURNING {PAGE_COLUMNS}"
    );
    let page = sqlx::query_as::<_, NativePage>(&sql)
        .bind(&page_id)
        .bind(&context.doc.id)
        .bind(&parent_id)
        .bind(payload.title.trim())
        .bind(&blocks)
        .bind(payload.sort_order.unwrap_or(siblings))
        .bind(&identity.user_id)
        .bind(now)
        .bind(&search_text)
        .fetch_one(&mut *tx)
        .await?;
    let linked = match &blocks {
        Some(blocks) => {
            media::sync_embedded_media(&mut tx, &blocks.to_string(), &page.id, &identity.user_id)
                .await?
        }
        None => Vec::new(),
    };
    versions::record(&mut tx, &page, &identity.user_id, None).await?;
    touch_doc(&mut tx, &context.doc).await?;
    tx.commit().await?;
    media::attach_linked(&state.pool, &identity, &linked).await;
    Ok((
        StatusCode::CREATED,
        Json(page_json(&identity, &page, context.access.can_edit).await),
    ))
}

/// One page as the API answers it (with the creator's directory name).
pub async fn page_json(identity: &Identity, page: &NativePage, can_edit: bool) -> PageItem {
    let mut items = page_items(identity, std::slice::from_ref(page), can_edit).await;
    items.remove(0)
}

/// Write new body content as the next revision: search text, embedded media,
/// a version row and the doc's `updated_at`. The caller holds the doc and
/// page locks and has checked the revision.
pub async fn write_content(
    tx: &mut Transaction<'_, Postgres>,
    doc: &access::NativeDoc,
    page: &NativePage,
    blocks: &Option<Value>,
    user_id: &str,
    restored_from: Option<i64>,
) -> ApiResult<(NativePage, Vec<media::Linked>)> {
    let sql = format!(
        "UPDATE docs_native_doc_pages SET updated_at = $2, content_blocks = $3, content_text = NULL, \
         search_text = $4, revision = revision + 1 WHERE id = $1 RETURNING {PAGE_COLUMNS}"
    );
    let updated = sqlx::query_as::<_, NativePage>(&sql)
        .bind(&page.id)
        .bind(now_utc().max(page.updated_at))
        .bind(blocks)
        .bind(extract_blocks_text(blocks.as_ref()))
        .fetch_one(&mut **tx)
        .await?;
    let body = blocks.as_ref().map(Value::to_string).unwrap_or_default();
    let linked = media::sync_embedded_media(tx, &body, &updated.id, user_id).await?;
    versions::record(tx, &updated, user_id, restored_from).await?;
    touch_doc(tx, doc).await?;
    Ok((updated, linked))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdatePageRequest {
    #[serde(default)]
    #[schema(min_length = 1, max_length = 200)]
    title: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<String>)]
    /// Omit to keep the parent, or send null to move to the root.
    parent_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Vec<std::collections::BTreeMap<String, Value>>>)]
    /// Omit to keep the body, or send null to clear it.
    content_blocks: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    /// A block page rejects any request that includes this field.
    #[schema(ignore)]
    content_text: Option<Option<String>>,
    #[serde(default)]
    sort_order: Option<i32>,
    /// Use the last received revision when saving content to detect a conflicting edit.
    /// Omission is accepted for legacy callers.
    #[serde(default)]
    expected_revision: Option<i64>,
}

pub fn revision_conflict() -> ApiError {
    ApiError::conflict(
        "docs.page_revision_conflict",
        "The page was changed by someone else after you opened it.",
    )
}

/// Lock the page's doc, then the page, and decide edit access again.
pub async fn lock_page_for_edit(
    tx: &mut Transaction<'_, Postgres>,
    state: &AppState,
    identity: &Identity,
    page_id: &str,
) -> ApiResult<(access::DocContext, NativePage)> {
    let page_id = access::normalize_page_id(page_id)?;
    let page = access::load_page(&state.pool, &page_id)
        .await?
        .ok_or_else(access::page_not_found)?;
    let context = access::lock_doc_for_write(tx, state, identity, &page.doc_id).await?;
    let sql = format!("SELECT {PAGE_COLUMNS} FROM docs_native_doc_pages WHERE id = $1 FOR UPDATE");
    let page = sqlx::query_as::<_, NativePage>(&sql)
        .bind(&page_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(access::page_not_found)?;
    if !context.access.can_edit || page.trashed_at.is_some() || context.doc.trashed_at.is_some() {
        return Err(doc_edit_required());
    }
    Ok((context, page))
}

/// AI-DO `update_native_page` with the revision check for content saves.
#[utoipa::path(
    patch, path = "/v1/docs/pages/{page_id}", operation_id = "updateDocsPage", tag = "Pages",
    params(("page_id" = String, Path)),
    request_body = UpdatePageRequest,
    responses(
        (status = 200, description = "Success", body = PageItem),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 409, description = "Current resource state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn update_page(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(page_id): Path<String>,
    ApiJson(payload): ApiJson<UpdatePageRequest>,
) -> ApiResult<Json<PageItem>> {
    if let Some(title) = &payload.title {
        check_length("title", title, 1, 200)?;
    }
    if let Some(blocks) = &payload.content_blocks {
        validate_blocks(blocks)?;
    }
    let mut tx = state.pool.begin().await?;
    let (context, page) = lock_page_for_edit(&mut tx, &state, &identity, &page_id).await?;
    let mut parent_id = page.parent_id.clone();
    let mut title = page.title.clone();
    let mut sort_order = page.sort_order;
    let mut changed = false;
    if let Some(next) = &payload.parent_id {
        let next = next
            .as_deref()
            .filter(|id| !id.is_empty())
            .map(access::normalize_page_id)
            .transpose()?;
        let pages = active_pages(&mut tx, &context.doc.id).await?;
        validate_parent(&pages, next.as_deref(), Some(&page.id))?;
        if parent_id != next {
            parent_id = next;
            changed = true;
        }
    }
    if let Some(next) = &payload.title {
        let next = next.trim().to_string();
        if title != next {
            title = next;
            changed = true;
        }
    }
    let mut content: Option<Option<Value>> = None;
    if let Some(blocks) = &payload.content_blocks {
        if page.content_format != "block" {
            return Err(format_mismatch());
        }
        if page.content_blocks != *blocks || page.content_text.is_some() {
            if payload
                .expected_revision
                .is_some_and(|expected| expected != page.revision)
            {
                return Err(revision_conflict());
            }
            content = Some(blocks.clone());
            changed = true;
        }
    }
    if payload.content_text.is_some() {
        // Only block pages exist (AI-DO: content_text belongs to HTML pages).
        return Err(format_mismatch());
    }
    if let Some(next) = payload.sort_order {
        if sort_order != next {
            sort_order = next;
            changed = true;
        }
    }
    let mut linked = Vec::new();
    let mut page = page;
    if changed {
        if parent_id != page.parent_id || title != page.title || sort_order != page.sort_order {
            let sql = format!(
                "UPDATE docs_native_doc_pages SET parent_id = $2, title = $3, sort_order = $4, updated_at = $5 \
                 WHERE id = $1 RETURNING {PAGE_COLUMNS}"
            );
            page = sqlx::query_as::<_, NativePage>(&sql)
                .bind(&page.id)
                .bind(&parent_id)
                .bind(&title)
                .bind(sort_order)
                .bind(now_utc())
                .fetch_one(&mut *tx)
                .await?;
            touch_doc(&mut tx, &context.doc).await?;
        }
        if let Some(blocks) = &content {
            let (updated, newly_linked) = write_content(
                &mut tx,
                &context.doc,
                &page,
                blocks,
                &identity.user_id,
                None,
            )
            .await?;
            page = updated;
            linked = newly_linked;
        }
    }
    tx.commit().await?;
    media::attach_linked(&state.pool, &identity, &linked).await;
    Ok(Json(
        page_json(&identity, &page, context.access.can_edit).await,
    ))
}

/// AI-DO `_collect_native_page_subtree`.
pub fn subtree(pages: &[NativePage], root: &str) -> Vec<String> {
    let mut by_parent: HashMap<&str, Vec<&str>> = HashMap::new();
    for page in pages {
        if let Some(parent) = page.parent_id.as_deref() {
            by_parent.entry(parent).or_default().push(page.id.as_str());
        }
    }
    if !pages.iter().any(|page| page.id == root) {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let mut stack = vec![root];
    while let Some(current) = stack.pop() {
        if !seen.insert(current) {
            continue;
        }
        result.push(current.to_string());
        if let Some(children) = by_parent.get(current) {
            stack.extend(children.iter().copied());
        }
    }
    result
}

/// AI-DO `delete_native_page`: move the page and its live subtree to the
/// trash. miy keeps their attachments and images until the trash is emptied
/// (AI-DO released them at once; the trash restore needs them, §8 of
/// docs/PORTING.md). Comment notifications of those pages are retracted.
#[utoipa::path(
    delete, path = "/v1/docs/pages/{page_id}", operation_id = "deleteDocsPage", tag = "Pages",
    params(("page_id" = String, Path)),
    responses(
        (status = 204, description = "Completed with no response body"),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn delete_page(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(page_id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.pool.begin().await?;
    let (context, page) = lock_page_for_edit(&mut tx, &state, &identity, &page_id).await?;
    let pages = active_pages(&mut tx, &context.doc.id).await?;
    let ids = subtree(&pages, &page.id);
    let now = now_utc();
    sqlx::query(
        "UPDATE docs_native_doc_pages SET trashed_at = $2, trashed_by_id = $3, updated_at = $2 \
         WHERE id = ANY($1)",
    )
    .bind(&ids)
    .bind(now)
    .bind(&identity.user_id)
    .execute(&mut *tx)
    .await?;
    let retract = comments::take_notifications_for_pages(&mut tx, &ids).await?;
    touch_doc(&mut tx, &context.doc).await?;
    tx.commit().await?;
    comments::retract_taken(&identity, &retract).await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(id: &str, parent: Option<&str>) -> NativePage {
        let at = chrono::DateTime::from_timestamp(0, 0).unwrap().naive_utc();
        NativePage {
            id: id.into(),
            doc_id: "d".into(),
            parent_id: parent.map(str::to_string),
            title: id.into(),
            content_format: "block".into(),
            content_blocks: None,
            content_text: None,
            sort_order: 0,
            created_by_id: "u".into(),
            created_at: at,
            updated_at: at,
            trashed_at: None,
            revision: 1,
        }
    }

    #[test]
    fn parents_reject_missing_self_and_cycles() {
        let pages = [page("a", None), page("b", Some("a")), page("c", Some("b"))];
        assert!(validate_parent(&pages, None, Some("a")).is_ok());
        assert!(validate_parent(&pages, Some("b"), None).is_ok());
        assert_eq!(
            validate_parent(&pages, Some("missing"), None)
                .unwrap_err()
                .code,
            "docs_parent_page_not_found"
        );
        assert_eq!(
            validate_parent(&pages, Some("a"), Some("a"))
                .unwrap_err()
                .code,
            "docs_page_cannot_be_own_parent"
        );
        // AI-DO test_native_doc_page_patch_rejects_cycle: Parent under Child.
        assert_eq!(
            validate_parent(&pages, Some("c"), Some("a"))
                .unwrap_err()
                .code,
            "docs_page_parent_cycle"
        );
    }

    #[test]
    fn subtree_collects_descendants() {
        let pages = [
            page("a", None),
            page("b", Some("a")),
            page("c", Some("b")),
            page("d", None),
        ];
        let mut ids = subtree(&pages, "a");
        ids.sort();
        assert_eq!(ids, vec!["a", "b", "c"]);
        assert!(subtree(&pages, "missing").is_empty());
    }

    #[test]
    fn page_items_keep_ai_do_shape_with_revision() {
        let value = serde_json::to_value(page_item(&page("p", None), true, "Kim")).unwrap();
        assert_eq!(value["source_type"], "native_doc_page");
        assert_eq!(value["source_page_id"], "p");
        assert_eq!(value["realtime_collab"], false);
        assert_eq!(value["revision"], 1);
        assert_eq!(value["created_by_name"], "Kim");
    }
}
