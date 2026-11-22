//! Docs items: AI-DO domains/docs/router.py (items, hub, targets, favorites,
//! recent pages, view records, shareable users, user sharing), service.py
//! (`list_hub` and its filters) and hub_projection.py (serializers).
//!
//! Paths stay AI-DO's under the app API (`/v1/docs/...` reaches
//! `/api/v1/docs/...`). AI-DO bound the workspace through the URL
//! (`/api/v1/workspaces/{slug}/docs/...`); the space-scoped routes here take
//! `?space=<id>` (the global space when absent), see spaces.rs.
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use std::collections::HashMap;

use crate::{
    AppState,
    access::{
        self, DOC_COLUMNS, DocContext, DocTarget, NativeAccess, NativeDoc, PAGE_COLUMNS,
        SOURCE_NATIVE_DOC, TARGET_APP, TARGET_SIDEBAR, Viewer,
    },
    error::{ApiError, ApiJson, ApiResult, check_length},
    files, notify, people,
    spaces::{self, Space, SpaceRole},
    time::{naive_iso, now_utc},
};

pub const DOC_TYPE_VALUES: [&str; 7] = [
    "general",
    "meeting_notes",
    "project_brief",
    "spec",
    "policy",
    "guide",
    "memo",
];

/// `Option<Option<T>>` for fields whose presence matters (Pydantic
/// `model_fields_set`): absent → `None`, `null` → `Some(None)`.
pub fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

pub fn doc_manage_required() -> ApiError {
    ApiError::forbidden(
        "docs.doc_manage_access_required",
        "Doc manage access required.",
    )
}

pub fn doc_edit_required() -> ApiError {
    ApiError::forbidden("docs.doc_edit_access_required", "Doc edit access required.")
}

/// AI-DO `touch_native_doc`: strictly later than the previous value.
pub async fn touch_doc(tx: &mut Transaction<'_, Postgres>, doc: &NativeDoc) -> ApiResult<()> {
    let mut next = now_utc();
    if next <= doc.updated_at {
        next = doc.updated_at + chrono::Duration::microseconds(1);
    }
    sqlx::query("UPDATE docs_native_docs SET updated_at = $2 WHERE id = $1")
        .bind(&doc.id)
        .bind(next)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// The doc ID as its app route (AI-DO `app_resource_path` for docs).
pub fn doc_path(doc_id: &str) -> String {
    format!("/{}", urlencode_segment(doc_id))
}

pub fn urlencode_segment(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// AI-DO registry.py `describe_source(...).badge`.
fn source_badge(source_app: &str) -> String {
    match source_app {
        "docs" => "Docs".into(),
        "meeting" => "Meeting".into(),
        "pms" => "PMS".into(),
        other => other
            .replace('_', " ")
            .split(' ')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => {
                        first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                    }
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// AI-DO `resolve_target_label` for the docs target adapter.
pub fn location_label(doc: &NativeDoc, targets: &[DocTarget]) -> &'static str {
    match access::primary_target(targets) {
        Some(target) if access::is_space_sidebar(doc, target) => "Workspace Docs",
        _ => "Unfiled",
    }
}

#[derive(Clone, Debug, Default, FromRow)]
pub struct Pref {
    pub source_doc_id: String,
    pub is_favorite: bool,
    pub last_viewed_at: Option<NaiveDateTime>,
    pub last_viewed_page_source_id: Option<String>,
}

/// Counts and names a hub item needs besides the doc and its access.
pub struct ItemExtras<'a> {
    pub targets: &'a [DocTarget],
    pub pref: Option<&'a Pref>,
    pub page_count: i64,
    pub user_share_count: i64,
    pub owner_name: String,
}

/// AI-DO hub_projection.py `serialize_native_hub_item` (without RAG scope and
/// collections, which are not ported; `collection` stays `null`).
pub fn hub_item(doc: &NativeDoc, access: NativeAccess, extras: &ItemExtras<'_>) -> Value {
    let primary = access::primary_target(extras.targets);
    let is_meeting_note = doc.source_app == "meeting" && doc.source_kind == "meeting_notes";
    let visibility = if extras.user_share_count > 0 || primary.is_some() || is_meeting_note {
        "shared"
    } else {
        "private"
    };
    let label = location_label(doc, extras.targets);
    json!({
        "id": doc.id,
        // miy: the space that holds the doc (AI-DO knew it from the route).
        "space_id": doc.workspace_id,
        "source_app": doc.source_app,
        "source_type": SOURCE_NATIVE_DOC,
        "source_id": doc.id,
        "source_kind": doc.source_kind,
        "source_ref": doc.source_ref,
        "generation_kind": doc.generation_kind,
        "doc_type": doc.doc_type,
        "content_format": "block",
        "structure_kind": "page_tree",
        "location_label": label,
        "target_label": label,
        "collection": Value::Null,
        "primary_target": primary.map(|target| json!({
            "app": target.target_app,
            "type": target.target_type,
            "id": target.target_id,
            "sort_order": target.sort_order,
        })),
        "source_badge": source_badge(&doc.source_app),
        "source_deeplink": doc_path(&doc.id),
        "title": doc.title,
        "page_count": extras.page_count,
        "created_by_id": doc.owner_id,
        "created_by_name": extras.owner_name,
        "created_at": naive_iso(doc.created_at),
        "updated_at": naive_iso(doc.updated_at),
        "trashed_at": doc.trashed_at.map(naive_iso),
        "is_favorite": extras.pref.is_some_and(|pref| pref.is_favorite),
        "is_private": visibility == "private",
        "last_viewed_at": extras.pref.and_then(|pref| pref.last_viewed_at).map(naive_iso),
        "can_view": access.can_view,
        "can_edit": access.can_edit,
        "can_share": access.can_share,
        "can_manage": access.can_manage,
        "sharing_summary": {
            "visibility": visibility,
            "user_share_count": extras.user_share_count,
            "link_active": false,
            "link_access_level": Value::Null,
        },
    })
}

/// A doc with what the hub shows about it.
pub struct Listed {
    pub doc: NativeDoc,
    pub item: Value,
    pub pref: Option<Pref>,
}

/// Serialize docs for the caller, keeping only those they may view (AI-DO
/// `load_accessible_native_docs` + `_serialize_native_item`), with batched
/// reads of targets, counts, direct shares, preferences and owner names.
pub async fn list_visible(viewer: &mut Viewer<'_>, docs: Vec<NativeDoc>) -> ApiResult<Vec<Listed>> {
    if docs.is_empty() {
        return Ok(Vec::new());
    }
    let pool = viewer.state.pool.clone();
    let user_id = viewer.user_id().to_string();
    let ids: Vec<String> = docs.iter().map(|doc| doc.id.clone()).collect();
    let mut targets = access::load_targets_for(&pool, &ids).await?;
    let page_counts: HashMap<String, i64> = sqlx::query_as::<_, (String, i64)>(
        "SELECT doc_id, count(*) FROM docs_native_doc_pages WHERE doc_id = ANY($1) AND trashed_at IS NULL \
         GROUP BY doc_id",
    )
    .bind(&ids)
    .fetch_all(&pool)
    .await?
    .into_iter()
    .collect();
    let share_counts: HashMap<String, i64> = sqlx::query_as::<_, (String, i64)>(
        "SELECT doc_id, count(*) FROM docs_native_doc_user_shares WHERE doc_id = ANY($1) GROUP BY doc_id",
    )
    .bind(&ids)
    .fetch_all(&pool)
    .await?
    .into_iter()
    .collect();
    let direct: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT doc_id, access_level FROM docs_native_doc_user_shares WHERE doc_id = ANY($1) AND user_id = $2",
    )
    .bind(&ids)
    .bind(&user_id)
    .fetch_all(&pool)
    .await?
    .into_iter()
    .collect();
    let mut prefs = load_prefs(&pool, &user_id, &ids).await?;
    let mut visible = Vec::new();
    for doc in docs {
        let doc_targets = targets.remove(&doc.id).unwrap_or_default();
        let level = direct
            .get(&doc.id)
            .and_then(|level| access::share_level(level));
        let access = viewer.access_with(&doc, &doc_targets, level).await?;
        if !access.can_view {
            continue;
        }
        visible.push((doc, doc_targets, access));
    }
    let names = people::display_names(
        viewer.identity,
        visible.iter().map(|(doc, _, _)| doc.owner_id.clone()),
    )
    .await;
    Ok(visible
        .into_iter()
        .map(|(doc, doc_targets, access)| {
            let pref = prefs.remove(&doc.id);
            let item = hub_item(
                &doc,
                access,
                &ItemExtras {
                    targets: &doc_targets,
                    pref: pref.as_ref(),
                    page_count: page_counts.get(&doc.id).copied().unwrap_or(0),
                    user_share_count: share_counts.get(&doc.id).copied().unwrap_or(0),
                    owner_name: names.get(&doc.owner_id).cloned().unwrap_or_default(),
                },
            );
            Listed { doc, item, pref }
        })
        .collect())
}

async fn load_prefs(
    pool: &PgPool,
    user_id: &str,
    ids: &[String],
) -> ApiResult<HashMap<String, Pref>> {
    Ok(sqlx::query_as::<_, Pref>(
        "SELECT source_doc_id, is_favorite, last_viewed_at, last_viewed_page_source_id \
         FROM docs_user_item_prefs WHERE user_id = $1 AND source_type = $2 AND source_doc_id = ANY($3)",
    )
    .bind(user_id)
    .bind(SOURCE_NATIVE_DOC)
    .bind(ids)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|pref| (pref.source_doc_id.clone(), pref))
    .collect())
}

/// AI-DO `_lookup_item`: one doc the caller may see, serialized.
pub async fn lookup_item(viewer: &mut Viewer<'_>, item_id: &str) -> ApiResult<Value> {
    let context = access::doc_or_404(viewer, item_id).await?;
    let mut listed = list_visible(viewer, vec![context.doc]).await?;
    listed
        .pop()
        .map(|listed| listed.item)
        .ok_or_else(access::doc_not_found)
}

// ---------------------------------------------------------------- hub

#[derive(Deserialize, Default)]
pub struct HubQuery {
    #[serde(default)]
    view: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    sort_by: Option<String>,
    #[serde(default)]
    sort_dir: Option<String>,
    #[serde(default)]
    page: Option<i64>,
    #[serde(default)]
    page_size: Option<i64>,
    #[serde(default)]
    source_app: Option<String>,
    #[serde(default)]
    source_kind: Option<String>,
    #[serde(default)]
    collection_id: Option<String>,
    #[serde(default)]
    doc_type: Option<String>,
    /// AI-DO's PMS space filter (docs linked to that PMS space's tasks). PMS
    /// links are not ported, so a value matches no doc.
    #[serde(default)]
    space_id: Option<String>,
    /// miy: the docs space (AI-DO's workspace slug in the path).
    #[serde(default)]
    space: Option<String>,
}

const VIEWS: [&str; 7] = [
    "all",
    "mine",
    "shared",
    "private",
    "meeting_notes",
    "recent",
    "archived",
];

/// AI-DO `_matches_doc_view`.
fn matches_view(listed: &Listed, view: &str, user_id: &str) -> bool {
    let doc = &listed.doc;
    let live = doc.trashed_at.is_none();
    let private = listed.item["is_private"].as_bool().unwrap_or(false);
    match view {
        "mine" => live && doc.owner_id == user_id,
        "shared" => live && doc.owner_id != user_id,
        "private" => live && doc.owner_id == user_id && private,
        "meeting_notes" => {
            live && doc.source_app == "meeting" && doc.source_kind == "meeting_notes"
        }
        "recent" => {
            live && listed
                .pref
                .as_ref()
                .is_some_and(|p| p.last_viewed_at.is_some())
        }
        "archived" => !live,
        _ => live,
    }
}

/// AI-DO `_sort_docs` (a stable sort, reversed unless ascending).
pub fn sort_items(items: &mut [Listed], sort_by: &str, sort_dir: &str) {
    let key = |listed: &Listed| -> (String, NaiveDateTime, i64) {
        let doc = &listed.doc;
        match sort_by {
            "title" => (doc.title.to_lowercase(), NaiveDateTime::MIN, 0),
            "created_at" => (String::new(), doc.created_at, 0),
            "last_viewed_at" => (
                String::new(),
                listed
                    .pref
                    .as_ref()
                    .and_then(|pref| pref.last_viewed_at)
                    .unwrap_or(NaiveDateTime::MIN),
                0,
            ),
            "target_sort_order" => (
                String::new(),
                NaiveDateTime::MIN,
                listed.item["primary_target"]["sort_order"]
                    .as_i64()
                    .unwrap_or(0),
            ),
            _ => (String::new(), doc.updated_at, 0),
        }
    };
    if sort_dir == "asc" {
        items.sort_by_key(|a| key(a));
    } else {
        // Python `sorted(reverse=True)` keeps equal keys in their original order.
        items.sort_by(|a, b| key(b).cmp(&key(a)));
    }
}

pub async fn list_hub(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Query(query): Query<HubQuery>,
) -> ApiResult<Json<Value>> {
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(50);
    if page < 1 {
        return Err(ApiError::validation(
            "page: Input should be greater than or equal to 1",
        ));
    }
    if !(1..=200).contains(&page_size) {
        return Err(ApiError::validation(
            "page_size: Input should be between 1 and 200",
        ));
    }
    let sort_dir = query.sort_dir.as_deref().unwrap_or("desc");
    if !matches!(sort_dir, "asc" | "desc") {
        return Err(ApiError::validation(
            "sort_dir: Input should be 'asc' or 'desc'",
        ));
    }
    if let Some(view) = query.view.as_deref() {
        if !VIEWS.contains(&view) {
            return Err(ApiError::validation("view: Input should be a docs view"));
        }
    }
    if let Some(doc_type) = query.doc_type.as_deref() {
        if !DOC_TYPE_VALUES.contains(&doc_type) {
            return Err(ApiError::validation(
                "doc_type: Input should be a docs type",
            ));
        }
    }
    let view = query.view.clone().unwrap_or_else(|| {
        match query.category.as_deref().unwrap_or("all") {
            "my" => "mine",
            "shared" => "shared",
            "private" => "private",
            "recent" => "recent",
            "archived" => "archived",
            "notes" | "meeting_notes" => "meeting_notes",
            _ => "all",
        }
        .to_string()
    });
    let sort_by = query.sort_by.as_deref().unwrap_or("updated_at");
    let (space, role) = spaces::require_space(&state, &identity, query.space.as_deref()).await?;
    let sql = format!("SELECT {DOC_COLUMNS} FROM docs_native_docs WHERE workspace_id = $1");
    let docs = sqlx::query_as::<_, NativeDoc>(&sql)
        .bind(&space.id)
        .fetch_all(&state.pool)
        .await?;
    let mut viewer = Viewer::new(&state, &identity);
    viewer.remember(&space.id, Some(role));
    let listed = list_visible(&mut viewer, docs).await?;
    let search = query.q.as_deref().unwrap_or("").trim().to_lowercase();
    let mut items: Vec<Listed> = listed
        .into_iter()
        .filter(|listed| {
            matches_view(listed, &view, &identity.user_id)
                && query
                    .source_app
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .is_none_or(|value| listed.doc.source_app == value)
                && query
                    .source_kind
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .is_none_or(|value| listed.doc.source_kind == value)
                // Collections are not ported: no doc belongs to one.
                && query.collection_id.as_deref().filter(|v| !v.is_empty()).is_none()
                && query
                    .doc_type
                    .as_deref()
                    .is_none_or(|value| listed.doc.doc_type == value)
                && query.space_id.as_deref().filter(|v| !v.is_empty()).is_none()
                && (search.is_empty()
                    || listed.doc.title.to_lowercase().contains(&search)
                    || listed.item["location_label"]
                        .as_str()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&search)
                    || listed.item["source_badge"]
                        .as_str()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&search))
        })
        .collect();
    sort_items(&mut items, sort_by, sort_dir);
    let total = items.len();
    let start = ((page - 1) * page_size) as usize;
    let page_items: Vec<Value> = items
        .into_iter()
        .skip(start)
        .take(page_size as usize)
        .map(|listed| listed.item)
        .collect();
    Ok(Json(json!({
        "items": page_items,
        "total": total,
        "page": page,
        "page_size": page_size,
    })))
}

// ---------------------------------------------------------------- items

#[derive(Deserialize)]
pub struct TargetPayload {
    app: String,
    #[serde(rename = "type")]
    kind: String,
    id: String,
    #[serde(default)]
    sort_order: i32,
}

fn validate_target(payload: &TargetPayload) -> ApiResult<()> {
    check_length("app", &payload.app, 1, 64)?;
    check_length("type", &payload.kind, 1, 64)?;
    check_length("id", &payload.id, 1, 128)?;
    Ok(())
}

/// AI-DO `_target_write_allowed`: the doc's own space sidebar is always
/// allowed; other apps' targets (PMS spaces, meetings) are not ported.
fn target_allowed(space_id: &str, payload: &TargetPayload) -> ApiResult<()> {
    if payload.app == TARGET_APP && payload.kind == TARGET_SIDEBAR && payload.id == space_id {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "docs.target_edit_access_required",
            "Target edit access required.",
        ))
    }
}

#[derive(Deserialize)]
pub struct CreateItemRequest {
    title: String,
    #[serde(default)]
    first_page_title: Option<String>,
    #[serde(default)]
    source_app: Option<String>,
    #[serde(default)]
    source_kind: Option<String>,
    #[serde(default)]
    source_ref: Option<String>,
    #[serde(default)]
    generation_kind: Option<String>,
    #[serde(default)]
    doc_type: Option<String>,
    #[serde(default)]
    content_format: Option<String>,
    #[serde(default)]
    first_page_content_text: Option<String>,
    #[serde(default)]
    collection_id: Option<String>,
    #[serde(default)]
    primary_target: Option<TargetPayload>,
}

#[derive(Deserialize, Default)]
pub struct SpaceQuery {
    #[serde(default)]
    space: Option<String>,
}

pub fn validate_content_format(value: Option<&str>) -> ApiResult<()> {
    match value {
        None | Some("block") => Ok(()),
        // AI-DO also accepted "html"; HTML pages are excluded.
        Some(_) => Err(ApiError::validation(
            "content_format: Input should be 'block'",
        )),
    }
}

pub async fn create_item(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Query(query): Query<SpaceQuery>,
    ApiJson(payload): ApiJson<CreateItemRequest>,
) -> ApiResult<impl IntoResponse> {
    check_length("title", &payload.title, 1, 200)?;
    if let Some(value) = &payload.first_page_title {
        check_length("first_page_title", value, 1, 200)?;
    }
    let source_app = payload.source_app.clone().unwrap_or_else(|| "docs".into());
    let source_kind = payload
        .source_kind
        .clone()
        .unwrap_or_else(|| "manual".into());
    let generation_kind = payload
        .generation_kind
        .clone()
        .unwrap_or_else(|| "human".into());
    check_length("source_app", &source_app, 1, 64)?;
    check_length("source_kind", &source_kind, 1, 64)?;
    check_length("generation_kind", &generation_kind, 1, 32)?;
    if let Some(value) = &payload.source_ref {
        check_length("source_ref", value, 0, 128)?;
    }
    if let Some(doc_type) = payload.doc_type.as_deref() {
        if !DOC_TYPE_VALUES.contains(&doc_type) {
            return Err(ApiError::validation(
                "doc_type: Input should be a docs type",
            ));
        }
    }
    validate_content_format(payload.content_format.as_deref())?;
    if let Some(target) = &payload.primary_target {
        validate_target(target)?;
    }
    if payload.first_page_content_text.is_some() {
        return Err(ApiError::bad_request(
            "docs.content_format_mismatch",
            "The request body does not match the doc content format.",
        ));
    }
    if payload
        .collection_id
        .as_deref()
        .is_some_and(|v| !v.is_empty())
    {
        return Err(ApiError::not_found(
            "docs.collection_not_found",
            "Collection not found.",
        ));
    }
    let space_id = query
        .space
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or(spaces::GLOBAL_SPACE_ID)
        .to_string();
    let space = spaces::load_space(&state.pool, &space_id)
        .await?
        .ok_or_else(spaces::space_not_found)?;
    if let Some(target) = &payload.primary_target {
        target_allowed(&space.id, target)?;
    }
    let doc_type = payload.doc_type.clone().unwrap_or_else(|| {
        if source_app == "meeting" || source_kind == "meeting_notes" {
            "meeting_notes".into()
        } else {
            "general".into()
        }
    });
    let title = payload.title.trim().to_string();
    let first_title = payload
        .first_page_title
        .as_deref()
        .unwrap_or(&payload.title)
        .trim()
        .to_string();
    let doc_id = uuid::Uuid::new_v4().to_string();
    let now = now_utc();
    let mut tx = state.pool.begin().await?;
    // Entering the space is required, decided under the role locks.
    spaces::space_role_in(&mut tx, &state, &identity, &space)
        .await?
        .ok_or_else(spaces::space_access_required)?;
    sqlx::query(
        "INSERT INTO docs_native_docs (id, workspace_id, owner_id, title, doc_type, source_app, source_kind, \
         source_ref, generation_kind, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10)",
    )
    .bind(&doc_id)
    .bind(&space.id)
    .bind(&identity.user_id)
    .bind(&title)
    .bind(&doc_type)
    .bind(&source_app)
    .bind(&source_kind)
    .bind(&payload.source_ref)
    .bind(&generation_kind)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO docs_native_doc_pages (id, doc_id, parent_id, title, content_format, content_blocks, \
         sort_order, created_by_id, created_at, updated_at) VALUES ($1,$2,NULL,$3,'block','[]'::json,0,$4,$5,$5)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&doc_id)
    .bind(&first_title)
    .bind(&identity.user_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    if let Some(target) = &payload.primary_target {
        insert_primary_target(&mut tx, &doc_id, target).await?;
    }
    tx.commit().await?;
    let mut viewer = Viewer::new(&state, &identity);
    let item = lookup_item(&mut viewer, &doc_id).await?;
    Ok((StatusCode::CREATED, Json(item)))
}

/// AI-DO `_upsert_primary_target`: clear the primary flag, then insert or
/// update the matching target as the primary one.
async fn insert_primary_target(
    tx: &mut Transaction<'_, Postgres>,
    doc_id: &str,
    target: &TargetPayload,
) -> ApiResult<()> {
    let now = now_utc();
    sqlx::query("UPDATE docs_doc_targets SET is_primary = false, updated_at = $2 WHERE doc_id = $1 AND is_primary")
        .bind(doc_id)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "INSERT INTO docs_doc_targets (id, doc_id, target_app, target_type, target_id, is_primary, sort_order, \
         created_at, updated_at) VALUES ($1,$2,$3,$4,$5,true,$6,$7,$7) \
         ON CONFLICT (doc_id, target_app, target_type, target_id) \
         DO UPDATE SET is_primary = true, sort_order = EXCLUDED.sort_order, updated_at = EXCLUDED.updated_at",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(doc_id)
    .bind(&target.app)
    .bind(&target.kind)
    .bind(&target.id)
    .bind(target.sort_order)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn get_item(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(item_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let mut viewer = Viewer::new(&state, &identity);
    Ok(Json(lookup_item(&mut viewer, &item_id).await?))
}

/// Lock a doc for a write and apply AI-DO `native_doc_from_item_or_404`.
pub async fn lock_item(
    tx: &mut Transaction<'_, Postgres>,
    state: &AppState,
    identity: &Identity,
    item_id: &str,
) -> ApiResult<DocContext> {
    let doc_id = access::normalize_doc_id(item_id)?;
    let context = access::lock_doc_for_write(tx, state, identity, &doc_id).await?;
    if !context.access.can_view || (context.doc.trashed_at.is_some() && !context.access.can_manage)
    {
        return Err(access::doc_not_found());
    }
    Ok(context)
}

#[derive(Deserialize)]
pub struct UpdateItemRequest {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    doc_type: Option<String>,
    #[serde(default, deserialize_with = "present")]
    collection_id: Option<Option<String>>,
}

pub async fn update_item(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(item_id): Path<String>,
    ApiJson(payload): ApiJson<UpdateItemRequest>,
) -> ApiResult<Json<Value>> {
    if let Some(title) = &payload.title {
        check_length("title", title, 1, 200)?;
    }
    if let Some(doc_type) = payload.doc_type.as_deref() {
        if !DOC_TYPE_VALUES.contains(&doc_type) {
            return Err(ApiError::validation(
                "doc_type: Input should be a docs type",
            ));
        }
    }
    let mut tx = state.pool.begin().await?;
    let context = lock_item(&mut tx, &state, &identity, &item_id).await?;
    if !context.access.can_manage {
        return Err(doc_manage_required());
    }
    if let Some(Some(collection)) = &payload.collection_id {
        if !collection.is_empty() {
            return Err(ApiError::not_found(
                "docs.collection_not_found",
                "Collection not found.",
            ));
        }
    }
    let title = payload
        .title
        .as_deref()
        .map(|title| title.trim().to_string())
        .unwrap_or_else(|| context.doc.title.clone());
    let doc_type = payload
        .doc_type
        .clone()
        .unwrap_or_else(|| context.doc.doc_type.clone());
    if title != context.doc.title || doc_type != context.doc.doc_type {
        sqlx::query(
            "UPDATE docs_native_docs SET title = $2, doc_type = $3, updated_at = $4 WHERE id = $1",
        )
        .bind(&context.doc.id)
        .bind(&title)
        .bind(&doc_type)
        .bind(now_utc().max(context.doc.updated_at + chrono::Duration::microseconds(1)))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    let mut viewer = Viewer::new(&state, &identity);
    Ok(Json(lookup_item(&mut viewer, &context.doc.id).await?))
}

/// AI-DO `_trash_doc_item`: trash the doc and its live pages, release their
/// body images and attachments, and retract its share notifications.
pub async fn delete_item(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(item_id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.pool.begin().await?;
    let context = lock_item(&mut tx, &state, &identity, &item_id).await?;
    if !context.access.can_manage {
        return Err(doc_manage_required());
    }
    let now = now_utc();
    sqlx::query("UPDATE docs_native_docs SET trashed_at = $2, updated_at = $3 WHERE id = $1")
        .bind(&context.doc.id)
        .bind(now)
        .bind(now.max(context.doc.updated_at + chrono::Duration::microseconds(1)))
        .execute(&mut *tx)
        .await?;
    let trashed: Vec<String> = sqlx::query_scalar(
        "UPDATE docs_native_doc_pages SET trashed_at = $2, updated_at = $2 WHERE doc_id = $1 AND trashed_at IS NULL \
         RETURNING id",
    )
    .bind(&context.doc.id)
    .bind(now)
    .fetch_all(&mut *tx)
    .await?;
    let released = files::release_for_pages(&mut tx, &trashed).await?;
    tx.commit().await?;
    files::detach_released(&state.pool, &identity, &released).await;
    notify::retract(&identity, &notify::doc_group_key(&context.doc.id)).await;
    Ok(StatusCode::NO_CONTENT)
}

/// AI-DO `duplicate_doc_item` + `_clone_page_tree`: a copy owned by the
/// caller in the same space, with the same targets and the live page tree.
pub async fn duplicate_item(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(item_id): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let mut tx = state.pool.begin().await?;
    let context = lock_item(&mut tx, &state, &identity, &item_id).await?;
    let source = &context.doc;
    let duplicate_id = uuid::Uuid::new_v4().to_string();
    let now = now_utc();
    let doc_type = if DOC_TYPE_VALUES.contains(&source.doc_type.as_str()) {
        source.doc_type.clone()
    } else {
        "general".to_string()
    };
    let title: String = format!("{} Copy", source.title).chars().take(200).collect();
    sqlx::query(
        "INSERT INTO docs_native_docs (id, workspace_id, owner_id, title, doc_type, source_app, source_kind, \
         source_ref, generation_kind, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10)",
    )
    .bind(&duplicate_id)
    .bind(&source.workspace_id)
    .bind(&identity.user_id)
    .bind(&title)
    .bind(&doc_type)
    .bind(&source.source_app)
    .bind(&source.source_kind)
    .bind(&source.source_ref)
    .bind(&source.generation_kind)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    for target in &context.targets {
        sqlx::query(
            "INSERT INTO docs_doc_targets (id, doc_id, target_app, target_type, target_id, is_primary, sort_order, \
             created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$8)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&duplicate_id)
        .bind(&target.target_app)
        .bind(&target.target_type)
        .bind(&target.target_id)
        .bind(target.is_primary)
        .bind(target.sort_order)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    let sql = format!(
        "SELECT {PAGE_COLUMNS} FROM docs_native_doc_pages WHERE doc_id = $1 AND trashed_at IS NULL \
         ORDER BY sort_order, created_at"
    );
    let pages = sqlx::query_as::<_, access::NativePage>(&sql)
        .bind(&source.id)
        .fetch_all(&mut *tx)
        .await?;
    let mut by_parent: HashMap<Option<String>, Vec<&access::NativePage>> = HashMap::new();
    for page in &pages {
        by_parent
            .entry(page.parent_id.clone())
            .or_default()
            .push(page);
    }
    let mut stack: Vec<(Option<String>, Option<String>)> = vec![(None, None)];
    while let Some((source_parent, destination_parent)) = stack.pop() {
        for page in by_parent.get(&source_parent).cloned().unwrap_or_default() {
            let cloned_id = uuid::Uuid::new_v4().to_string();
            let search_text: String =
                sqlx::query_scalar("SELECT search_text FROM docs_native_doc_pages WHERE id = $1")
                    .bind(&page.id)
                    .fetch_one(&mut *tx)
                    .await?;
            sqlx::query(
                "INSERT INTO docs_native_doc_pages (id, doc_id, parent_id, title, content_format, content_blocks, \
                 content_text, sort_order, created_by_id, created_at, updated_at, search_text) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10,$11)",
            )
            .bind(&cloned_id)
            .bind(&duplicate_id)
            .bind(&destination_parent)
            .bind(&page.title)
            .bind(&page.content_format)
            .bind(&page.content_blocks)
            .bind(&page.content_text)
            .bind(page.sort_order)
            .bind(&identity.user_id)
            .bind(now)
            .bind(&search_text)
            .execute(&mut *tx)
            .await?;
            stack.push((Some(page.id.clone()), Some(cloned_id)));
        }
    }
    tx.commit().await?;
    let mut viewer = Viewer::new(&state, &identity);
    let item = lookup_item(&mut viewer, &duplicate_id).await?;
    Ok((StatusCode::CREATED, Json(item)))
}

pub async fn update_target(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(item_id): Path<String>,
    ApiJson(payload): ApiJson<TargetPayload>,
) -> ApiResult<Json<Value>> {
    validate_target(&payload)?;
    let mut tx = state.pool.begin().await?;
    let context = lock_item(&mut tx, &state, &identity, &item_id).await?;
    if !context.access.can_edit {
        return Err(doc_edit_required());
    }
    target_allowed(&context.doc.workspace_id, &payload)?;
    insert_primary_target(&mut tx, &context.doc.id, &payload).await?;
    tx.commit().await?;
    let mut viewer = Viewer::new(&state, &identity);
    Ok(Json(lookup_item(&mut viewer, &context.doc.id).await?))
}

pub async fn delete_target(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(item_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    let context = lock_item(&mut tx, &state, &identity, &item_id).await?;
    if !context.access.can_edit {
        return Err(doc_edit_required());
    }
    sqlx::query("DELETE FROM docs_doc_targets WHERE doc_id = $1 AND is_primary")
        .bind(&context.doc.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let mut viewer = Viewer::new(&state, &identity);
    Ok(Json(lookup_item(&mut viewer, &context.doc.id).await?))
}

pub async fn toggle_favorite(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(item_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let mut viewer = Viewer::new(&state, &identity);
    let context = access::doc_or_404(&mut viewer, &item_id).await?;
    let is_favorite: bool = sqlx::query_scalar(
        "INSERT INTO docs_user_item_prefs (id, user_id, source_type, source_doc_id, is_favorite) \
         VALUES ($1, $2, $3, $4, true) ON CONFLICT (user_id, source_type, source_doc_id) \
         DO UPDATE SET is_favorite = NOT docs_user_item_prefs.is_favorite RETURNING is_favorite",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&identity.user_id)
    .bind(SOURCE_NATIVE_DOC)
    .bind(&context.doc.id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(json!({ "is_favorite": is_favorite })))
}

#[derive(Deserialize, Default)]
pub struct RecordViewRequest {
    #[serde(default)]
    page_id: Option<String>,
}

pub async fn record_view(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(item_id): Path<String>,
    ApiJson(payload): ApiJson<RecordViewRequest>,
) -> ApiResult<Json<Value>> {
    let mut viewer = Viewer::new(&state, &identity);
    let context = access::doc_or_404(&mut viewer, &item_id).await?;
    let mut page_title = context.doc.title.clone();
    let mut page_source_id = context.doc.id.clone();
    if let Some(raw) = payload.page_id.as_deref().filter(|id| !id.is_empty()) {
        let page_id = access::normalize_page_id(raw)?;
        let page: Option<(String, String)> = sqlx::query_as(
            "SELECT id, title FROM docs_native_doc_pages WHERE id = $1 AND doc_id = $2 AND trashed_at IS NULL",
        )
        .bind(&page_id)
        .bind(&context.doc.id)
        .fetch_optional(&state.pool)
        .await?;
        if let Some((id, title)) = page {
            page_source_id = id;
            page_title = title;
        }
    }
    sqlx::query(
        "INSERT INTO docs_user_item_prefs (id, user_id, source_type, source_doc_id, is_favorite, last_viewed_at, \
         last_viewed_page_source_id, last_viewed_page_title) VALUES ($1,$2,$3,$4,false,$5,$6,$7) \
         ON CONFLICT (user_id, source_type, source_doc_id) DO UPDATE SET last_viewed_at = EXCLUDED.last_viewed_at, \
         last_viewed_page_source_id = EXCLUDED.last_viewed_page_source_id, \
         last_viewed_page_title = EXCLUDED.last_viewed_page_title",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&identity.user_id)
    .bind(SOURCE_NATIVE_DOC)
    .bind(&context.doc.id)
    .bind(now_utc())
    .bind(&page_source_id)
    .bind(&page_title)
    .execute(&state.pool)
    .await?;
    // miy: opening the doc reads its share notification.
    notify::mark_read(&identity, &notify::doc_group_key(&context.doc.id)).await;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------- favorites, recent

#[derive(Deserialize, Default)]
pub struct PrefQuery {
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    space: Option<String>,
}

fn space_location(space: &Space) -> Value {
    json!({ "key": space.id, "name": space.name, "kind": space.kind })
}

/// The spaces a pref listing reads from, with the caller's roles remembered.
async fn pref_spaces(
    state: &AppState,
    identity: &Identity,
    viewer: &mut Viewer<'_>,
    query: &PrefQuery,
) -> ApiResult<(HashMap<String, Space>, bool)> {
    let scope = query.scope.as_deref().unwrap_or("space");
    if !matches!(scope, "space" | "docs_spaces") {
        return Err(ApiError::validation(
            "scope: Input should be 'space' or 'docs_spaces'",
        ));
    }
    let spaces: Vec<(Space, SpaceRole)> = if scope == "docs_spaces" {
        spaces::enterable_spaces(state, identity).await?
    } else {
        vec![spaces::require_space(state, identity, query.space.as_deref()).await?]
    };
    for (space, role) in &spaces {
        viewer.remember(&space.id, Some(*role));
    }
    Ok((
        spaces
            .into_iter()
            .map(|(space, _)| (space.id.clone(), space))
            .collect(),
        scope == "docs_spaces",
    ))
}

async fn docs_in(pool: &PgPool, ids: &[String], space_ids: &[String]) -> ApiResult<Vec<NativeDoc>> {
    let sql = format!(
        "SELECT {DOC_COLUMNS} FROM docs_native_docs WHERE id = ANY($1) AND workspace_id = ANY($2) \
         AND trashed_at IS NULL"
    );
    Ok(sqlx::query_as::<_, NativeDoc>(&sql)
        .bind(ids)
        .bind(space_ids)
        .fetch_all(pool)
        .await?)
}

pub async fn list_favorites(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Query(query): Query<PrefQuery>,
) -> ApiResult<Json<Vec<Value>>> {
    let mut viewer = Viewer::new(&state, &identity);
    let (spaces, with_space) = pref_spaces(&state, &identity, &mut viewer, &query).await?;
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT source_doc_id FROM docs_user_item_prefs WHERE user_id = $1 AND source_type = $2 AND is_favorite",
    )
    .bind(&identity.user_id)
    .bind(SOURCE_NATIVE_DOC)
    .fetch_all(&state.pool)
    .await?;
    let space_ids: Vec<String> = spaces.keys().cloned().collect();
    let docs = docs_in(&state.pool, &ids, &space_ids).await?;
    let mut listed = list_visible(&mut viewer, docs).await?;
    sort_items(&mut listed, "updated_at", "desc");
    Ok(Json(
        listed
            .into_iter()
            .map(|listed| {
                let mut item = json!({
                    "id": listed.doc.id,
                    "title": listed.doc.title,
                    "source_type": SOURCE_NATIVE_DOC,
                    "space": Value::Null,
                });
                if with_space {
                    item["space"] = spaces
                        .get(&listed.doc.workspace_id)
                        .map(space_location)
                        .unwrap_or(Value::Null);
                }
                item
            })
            .collect(),
    ))
}

pub async fn list_recent_pages(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Query(query): Query<PrefQuery>,
) -> ApiResult<Json<Vec<Value>>> {
    let limit = query.limit.unwrap_or(10);
    if !(1..=20).contains(&limit) {
        return Err(ApiError::validation(
            "limit: Input should be between 1 and 20",
        ));
    }
    let mut viewer = Viewer::new(&state, &identity);
    let (spaces, with_space) = pref_spaces(&state, &identity, &mut viewer, &query).await?;
    Ok(Json(
        recent_pages(&mut viewer, &spaces, with_space, limit as usize).await?,
    ))
}

/// AI-DO `_list_docs_space_recent_pages`: read the view history in batches and
/// stop once `limit` rows are still readable. The page title is the live
/// page's current title (AI-DO kept the title stored at view time).
pub async fn recent_pages(
    viewer: &mut Viewer<'_>,
    spaces: &HashMap<String, Space>,
    with_space: bool,
    limit: usize,
) -> ApiResult<Vec<Value>> {
    let pool = viewer.state.pool.clone();
    let user_id = viewer.user_id().to_string();
    let space_ids: Vec<String> = spaces.keys().cloned().collect();
    let mut items = Vec::new();
    let mut offset = 0i64;
    loop {
        let batch: Vec<Pref> = sqlx::query_as(
            "SELECT source_doc_id, is_favorite, last_viewed_at, last_viewed_page_source_id \
             FROM docs_user_item_prefs WHERE user_id = $1 AND source_type = $2 AND last_viewed_at IS NOT NULL \
             ORDER BY last_viewed_at DESC, id OFFSET $3 LIMIT 50",
        )
        .bind(&user_id)
        .bind(SOURCE_NATIVE_DOC)
        .bind(offset)
        .fetch_all(&pool)
        .await?;
        if batch.is_empty() {
            return Ok(items);
        }
        offset += batch.len() as i64;
        let ids: Vec<String> = batch
            .iter()
            .map(|pref| pref.source_doc_id.clone())
            .collect();
        let docs = docs_in(&pool, &ids, &space_ids).await?;
        let listed: HashMap<String, Listed> = list_visible(viewer, docs)
            .await?
            .into_iter()
            .map(|listed| (listed.doc.id.clone(), listed))
            .collect();
        let page_ids: Vec<String> = batch
            .iter()
            .filter_map(|pref| pref.last_viewed_page_source_id.clone())
            .collect();
        let pages: HashMap<String, (String, String)> = sqlx::query_as::<_, (String, String, String)>(
            "SELECT id, doc_id, title FROM docs_native_doc_pages WHERE id = ANY($1) AND trashed_at IS NULL",
        )
        .bind(&page_ids)
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(|(id, doc_id, title)| (id, (doc_id, title)))
        .collect();
        for pref in batch {
            let Some(listed) = listed.get(&pref.source_doc_id) else {
                continue;
            };
            let page = pref
                .last_viewed_page_source_id
                .as_ref()
                .and_then(|id| pages.get(id).map(|(doc_id, title)| (id, doc_id, title)))
                .filter(|(_, doc_id, _)| **doc_id == listed.doc.id);
            let (page_id, page_title) = match page {
                Some((id, _, title)) => (id.clone(), title.clone()),
                None => (listed.doc.id.clone(), listed.doc.title.clone()),
            };
            let mut item = json!({
                "page_id": page_id,
                "page_title": page_title,
                "doc_id": listed.doc.id,
                "doc_title": listed.doc.title,
                "source_type": SOURCE_NATIVE_DOC,
                "location_label": listed.item["location_label"],
                "last_viewed_at": pref.last_viewed_at.map(naive_iso),
                "space": Value::Null,
            });
            if with_space {
                item["space"] = spaces
                    .get(&listed.doc.workspace_id)
                    .map(space_location)
                    .unwrap_or(Value::Null);
            }
            items.push(item);
            if items.len() >= limit {
                return Ok(items);
            }
        }
    }
}

// ---------------------------------------------------------------- sharing

#[derive(Deserialize, Default)]
pub struct ShareableQuery {
    #[serde(default)]
    q: Option<String>,
}

/// AI-DO `list_shareable_users`: active people matching `q` (at most 30),
/// without the caller. miy reads them from the platform directory, which has
/// no e-mail; `email` stays empty. AI-DO also kept only workspace members; the
/// platform leaves out people who may not use the app when it delivers.
pub async fn list_shareable_users(
    State(_state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Query(query): Query<ShareableQuery>,
) -> ApiResult<Json<Vec<Value>>> {
    let q = query.q.unwrap_or_default();
    let users = people::search_users(&identity, q.trim(), 30).await;
    Ok(Json(
        users
            .into_iter()
            .filter(|(id, _)| id != &identity.user_id)
            .map(|(id, name)| json!({ "id": id, "email": "", "full_name": name }))
            .collect(),
    ))
}

/// AI-DO `_serialize_sharing_response` (no link share: excluded feature).
async fn sharing_response(pool: &PgPool, identity: &Identity, doc: &NativeDoc) -> ApiResult<Value> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT user_id, access_level FROM docs_native_doc_user_shares WHERE doc_id = $1",
    )
    .bind(&doc.id)
    .fetch_all(pool)
    .await?;
    let names = people::display_names(identity, rows.iter().map(|(id, _)| id.clone())).await;
    let mut users: Vec<(String, String, String)> = rows
        .into_iter()
        .map(|(user_id, level)| {
            let name = names
                .get(&user_id)
                .cloned()
                .unwrap_or_else(|| user_id.clone());
            (name, user_id, level)
        })
        .collect();
    users.sort_by(|a, b| {
        a.0.to_lowercase()
            .cmp(&b.0.to_lowercase())
            .then(a.1.cmp(&b.1))
    });
    Ok(json!({
        "doc_id": doc.id,
        "owner_id": doc.owner_id,
        "users": users.into_iter().map(|(name, user_id, level)| json!({
            "user_id": user_id,
            "email": "",
            "full_name": name,
            "access_level": level,
        })).collect::<Vec<_>>(),
        "link_share": Value::Null,
    }))
}

fn share_required() -> ApiError {
    ApiError::forbidden(
        "docs.doc_share_access_required",
        "Doc share access required.",
    )
}

pub async fn get_sharing(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(item_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let mut viewer = Viewer::new(&state, &identity);
    let context = access::doc_or_404(&mut viewer, &item_id).await?;
    if !context.access.can_share {
        return Err(share_required());
    }
    Ok(Json(
        sharing_response(&state.pool, &identity, &context.doc).await?,
    ))
}

#[derive(Deserialize)]
pub struct UpsertShareRequest {
    access_level: String,
}

pub async fn upsert_user_share(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((item_id, user_id)): Path<(String, String)>,
    ApiJson(payload): ApiJson<UpsertShareRequest>,
) -> ApiResult<Json<Value>> {
    let Some(level) = access::share_level(&payload.access_level) else {
        return Err(ApiError::validation(
            "access_level: Input should be 'read' or 'edit'",
        ));
    };
    // The person must be active in the directory now (AI-DO: an active user).
    let names = people::display_names(&identity, [user_id.clone()]).await;
    let mut tx = state.pool.begin().await?;
    let context = lock_item(&mut tx, &state, &identity, &item_id).await?;
    if !context.access.can_share {
        return Err(share_required());
    }
    if user_id == identity.user_id {
        return Err(ApiError::conflict(
            "docs.owner_already_has_full_access",
            "Owner already has full access.",
        ));
    }
    if !names.contains_key(&user_id) {
        return Err(ApiError::not_found(
            "auth.user_not_found",
            "User not found.",
        ));
    }
    let inserted: bool = sqlx::query_scalar(
        "INSERT INTO docs_native_doc_user_shares (id, doc_id, user_id, access_level, created_by_id, created_at) \
         VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (doc_id, user_id) DO UPDATE SET access_level = EXCLUDED.access_level \
         RETURNING (xmax = 0)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&context.doc.id)
    .bind(&user_id)
    .bind(level)
    .bind(&identity.user_id)
    .bind(now_utc())
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    if inserted && context.doc.trashed_at.is_none() {
        notify::doc_shared(&identity, &context.doc, &user_id).await;
    }
    Ok(Json(
        sharing_response(&state.pool, &identity, &context.doc).await?,
    ))
}

pub async fn delete_user_share(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((item_id, user_id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    let context = lock_item(&mut tx, &state, &identity, &item_id).await?;
    if !context.access.can_share {
        return Err(share_required());
    }
    let removed =
        sqlx::query("DELETE FROM docs_native_doc_user_shares WHERE doc_id = $1 AND user_id = $2")
            .bind(&context.doc.id)
            .bind(&user_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    tx.commit().await?;
    if removed > 0 {
        notify::retract_for(
            &identity,
            &notify::doc_group_key(&context.doc.id),
            std::slice::from_ref(&user_id),
        )
        .await;
    }
    Ok(Json(
        sharing_response(&state.pool, &identity, &context.doc).await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badges_follow_ai_do_source_adapters() {
        assert_eq!(source_badge("docs"), "Docs");
        assert_eq!(source_badge("pms"), "PMS");
        assert_eq!(source_badge("meeting"), "Meeting");
        assert_eq!(source_badge("image_wizard"), "Image Wizard");
    }

    #[test]
    fn doc_paths_encode_the_id() {
        assert_eq!(doc_path("abc-1"), "/abc-1");
        assert_eq!(doc_path("a/b c"), "/a%2Fb%20c");
    }
}
