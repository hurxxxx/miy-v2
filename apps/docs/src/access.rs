//! AI-DO domains/docs/access_context.py: rows, loaders and the native doc
//! access rule.
//!
//! `resolve_native_doc_access`: the owner may do everything; anyone else gets
//! the best of a direct user share (`read`/`edit`) and the projection of the
//! space role through the doc's targets (viewer reads, member edits, admin
//! manages and shares). Link shares and meeting grants are excluded features.
//! AI-DO also bound each request to a workspace and loaded only docs of that
//! workspace; here item and page routes resolve the doc's own space, so a
//! person reaches a doc shared with them without entering its space (AI-DO's
//! share-token/resource route behaved the same way).
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use serde_json::Value;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use std::collections::HashMap;

use crate::{
    AppState,
    error::{ApiError, ApiResult},
    spaces::{self, SpaceRole},
};

pub const SOURCE_NATIVE_DOC: &str = "native_doc";
pub const PAGE_SOURCE_NATIVE_DOC: &str = "native_doc_page";
pub const TARGET_APP: &str = "docs";
pub const TARGET_SIDEBAR: &str = "workspace_sidebar";

#[derive(Clone, Debug, FromRow)]
pub struct NativeDoc {
    pub id: String,
    pub workspace_id: String,
    pub owner_id: String,
    pub title: String,
    pub doc_type: String,
    pub source_app: String,
    pub source_kind: String,
    pub source_ref: Option<String>,
    pub generation_kind: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub trashed_at: Option<NaiveDateTime>,
}

pub const DOC_COLUMNS: &str = "id, workspace_id, owner_id, title, doc_type, source_app, source_kind, \
     source_ref, generation_kind, created_at, updated_at, trashed_at";

#[derive(Clone, Debug, FromRow)]
#[allow(dead_code)] // `id` is read by the tests and kept for parity with AI-DO.
pub struct DocTarget {
    pub id: String,
    pub doc_id: String,
    pub target_app: String,
    pub target_type: String,
    pub target_id: String,
    pub is_primary: bool,
    pub sort_order: i32,
    pub created_at: NaiveDateTime,
}

pub const TARGET_COLUMNS: &str =
    "id, doc_id, target_app, target_type, target_id, is_primary, sort_order, created_at";

#[derive(Clone, Debug, FromRow)]
pub struct NativePage {
    pub id: String,
    pub doc_id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub content_format: String,
    pub content_blocks: Option<Value>,
    pub content_text: Option<String>,
    pub sort_order: i32,
    pub created_by_id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub trashed_at: Option<NaiveDateTime>,
    pub revision: i64,
}

pub const PAGE_COLUMNS: &str = "id, doc_id, parent_id, title, content_format, content_blocks, \
     content_text, sort_order, created_by_id, created_at, updated_at, trashed_at, revision";

/// AI-DO `NativeAccess` (without the matched link share).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeAccess {
    pub access_level: Option<&'static str>,
    pub can_view: bool,
    pub can_edit: bool,
    pub can_share: bool,
    pub can_manage: bool,
}

const TEAM_ACCESS_LEVEL_RANK: [(&str, u8); 2] = [("read", 10), ("edit", 20)];

fn rank(level: Option<&str>) -> Option<u8> {
    let level = level?;
    TEAM_ACCESS_LEVEL_RANK
        .iter()
        .find(|(name, _)| *name == level)
        .map(|(_, rank)| *rank)
}

/// AI-DO `max_access_level`.
pub fn max_access_level(levels: &[Option<&'static str>]) -> Option<&'static str> {
    levels
        .iter()
        .copied()
        .filter(|level| rank(*level).is_some())
        .max_by_key(|level| rank(*level))
        .flatten()
}

/// AI-DO `select_primary_target`: primary first, then sort order, then age.
pub fn primary_target(targets: &[DocTarget]) -> Option<&DocTarget> {
    targets.iter().min_by(|a, b| {
        (!a.is_primary, a.sort_order, a.created_at).cmp(&(
            !b.is_primary,
            b.sort_order,
            b.created_at,
        ))
    })
}

/// Is this target the sidebar of the doc's own space (AI-DO
/// `_is_workspace_sidebar_ref` for the docs adapter)?
pub fn is_space_sidebar(doc: &NativeDoc, target: &DocTarget) -> bool {
    target.target_app == TARGET_APP
        && target.target_type == TARGET_SIDEBAR
        && target.target_id == doc.workspace_id
}

/// AI-DO `project_app_workspace_target_access` (view, edit, manage).
fn project(role: Option<SpaceRole>) -> (bool, bool, bool) {
    match role {
        None => (false, false, false),
        Some(role) => (true, role >= SpaceRole::Member, role >= SpaceRole::Admin),
    }
}

/// AI-DO `resolve_native_doc_access` with the space role and the direct share
/// level already looked up.
pub fn resolve_access(
    doc: &NativeDoc,
    targets: &[DocTarget],
    user_id: &str,
    direct_share: Option<&'static str>,
    space_role: Option<SpaceRole>,
) -> NativeAccess {
    if doc.owner_id == user_id {
        return NativeAccess {
            access_level: Some("edit"),
            can_view: true,
            can_edit: true,
            can_share: true,
            can_manage: true,
        };
    }
    let mut target_level: Option<&'static str> = None;
    let mut target_can_manage = false;
    for target in targets {
        let (view, edit, manage) = if is_space_sidebar(doc, target) {
            project(space_role)
        } else {
            // Other apps' targets (PMS spaces, meetings) are not ported.
            (false, false, false)
        };
        if manage {
            target_can_manage = true;
        }
        let candidate = if edit || manage {
            Some("edit")
        } else if view {
            Some("read")
        } else {
            None
        };
        target_level = max_access_level(&[target_level, candidate]);
    }
    let access_level = max_access_level(&[direct_share, target_level]);
    NativeAccess {
        access_level,
        can_view: rank(access_level).is_some(),
        can_edit: access_level == Some("edit"),
        can_share: target_can_manage,
        can_manage: target_can_manage,
    }
}

pub fn share_level(value: &str) -> Option<&'static str> {
    match value {
        "read" => Some("read"),
        "edit" => Some("edit"),
        _ => None,
    }
}

/// Per-request cache of the caller's role in each space.
pub struct Viewer<'a> {
    pub state: &'a AppState,
    pub identity: &'a Identity,
    roles: HashMap<String, Option<SpaceRole>>,
}

impl<'a> Viewer<'a> {
    pub fn new(state: &'a AppState, identity: &'a Identity) -> Self {
        Self {
            state,
            identity,
            roles: HashMap::new(),
        }
    }

    pub fn user_id(&self) -> &str {
        &self.identity.user_id
    }

    pub fn remember(&mut self, space_id: &str, role: Option<SpaceRole>) {
        self.roles.insert(space_id.to_string(), role);
    }

    pub async fn space_role(&mut self, space_id: &str) -> ApiResult<Option<SpaceRole>> {
        if let Some(role) = self.roles.get(space_id) {
            return Ok(*role);
        }
        let role = match spaces::load_space(&self.state.pool, space_id).await? {
            Some(space) => spaces::space_role(self.state, self.identity, &space).await?,
            None => None,
        };
        self.roles.insert(space_id.to_string(), role);
        Ok(role)
    }

    /// The caller's access to one doc.
    pub async fn access(
        &mut self,
        doc: &NativeDoc,
        targets: &[DocTarget],
    ) -> ApiResult<NativeAccess> {
        let direct = direct_share_level(&self.state.pool, &doc.id, self.user_id()).await?;
        self.access_with(doc, targets, direct).await
    }

    pub async fn access_with(
        &mut self,
        doc: &NativeDoc,
        targets: &[DocTarget],
        direct: Option<&'static str>,
    ) -> ApiResult<NativeAccess> {
        if doc.owner_id == self.identity.user_id {
            return Ok(resolve_access(doc, targets, self.user_id(), direct, None));
        }
        let needs_space = targets.iter().any(|target| is_space_sidebar(doc, target));
        let role = if needs_space {
            self.space_role(&doc.workspace_id).await?
        } else {
            None
        };
        Ok(resolve_access(doc, targets, self.user_id(), direct, role))
    }
}

/// Lock a doc row for a write (AI-DO serialized page writes through the doc's
/// ORM session; here concurrent page creates and moves of one doc wait for
/// each other) and decide the caller's access again under that lock.
pub async fn lock_doc_for_write(
    tx: &mut Transaction<'_, Postgres>,
    state: &AppState,
    identity: &Identity,
    doc_id: &str,
) -> ApiResult<DocContext> {
    let sql = format!("SELECT {DOC_COLUMNS} FROM docs_native_docs WHERE id = $1 FOR UPDATE");
    let doc = sqlx::query_as::<_, NativeDoc>(&sql)
        .bind(doc_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(doc_not_found)?;
    let sql = format!("SELECT {TARGET_COLUMNS} FROM docs_doc_targets WHERE doc_id = $1");
    let targets = sqlx::query_as::<_, DocTarget>(&sql)
        .bind(&doc.id)
        .fetch_all(&mut **tx)
        .await?;
    let direct: Option<String> = sqlx::query_scalar(
        "SELECT access_level FROM docs_native_doc_user_shares WHERE doc_id = $1 AND user_id = $2",
    )
    .bind(&doc.id)
    .bind(&identity.user_id)
    .fetch_optional(&mut **tx)
    .await?;
    let direct = direct.as_deref().and_then(share_level);
    let role = if doc.owner_id != identity.user_id
        && targets.iter().any(|target| is_space_sidebar(&doc, target))
    {
        match spaces::load_space(&state.pool, &doc.workspace_id).await? {
            Some(space) => spaces::space_role_in(tx, state, identity, &space).await?,
            None => None,
        }
    } else {
        None
    };
    let access = resolve_access(&doc, &targets, &identity.user_id, direct, role);
    Ok(DocContext {
        doc,
        targets,
        access,
    })
}

pub async fn direct_share_level(
    pool: &PgPool,
    doc_id: &str,
    user_id: &str,
) -> ApiResult<Option<&'static str>> {
    let level: Option<String> = sqlx::query_scalar(
        "SELECT access_level FROM docs_native_doc_user_shares WHERE doc_id = $1 AND user_id = $2",
    )
    .bind(doc_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(level.as_deref().and_then(share_level))
}

pub async fn load_doc(pool: &PgPool, doc_id: &str) -> ApiResult<Option<NativeDoc>> {
    let sql = format!("SELECT {DOC_COLUMNS} FROM docs_native_docs WHERE id = $1");
    Ok(sqlx::query_as::<_, NativeDoc>(&sql)
        .bind(doc_id)
        .fetch_optional(pool)
        .await?)
}

pub async fn load_targets(pool: &PgPool, doc_id: &str) -> ApiResult<Vec<DocTarget>> {
    let sql = format!("SELECT {TARGET_COLUMNS} FROM docs_doc_targets WHERE doc_id = $1");
    Ok(sqlx::query_as::<_, DocTarget>(&sql)
        .bind(doc_id)
        .fetch_all(pool)
        .await?)
}

pub async fn load_targets_for(
    pool: &PgPool,
    doc_ids: &[String],
) -> ApiResult<HashMap<String, Vec<DocTarget>>> {
    let sql = format!("SELECT {TARGET_COLUMNS} FROM docs_doc_targets WHERE doc_id = ANY($1)");
    let rows = sqlx::query_as::<_, DocTarget>(&sql)
        .bind(doc_ids)
        .fetch_all(pool)
        .await?;
    let mut map: HashMap<String, Vec<DocTarget>> = HashMap::new();
    for row in rows {
        map.entry(row.doc_id.clone()).or_default().push(row);
    }
    Ok(map)
}

pub async fn load_page(pool: &PgPool, page_id: &str) -> ApiResult<Option<NativePage>> {
    let sql = format!("SELECT {PAGE_COLUMNS} FROM docs_native_doc_pages WHERE id = $1");
    Ok(sqlx::query_as::<_, NativePage>(&sql)
        .bind(page_id)
        .fetch_optional(pool)
        .await?)
}

/// AI-DO `split_prefixed_id`.
fn split_prefixed_id(value: &str) -> (Option<&str>, &str) {
    match value.split_once("__") {
        Some((prefix, raw)) => (Some(prefix), raw),
        None => (None, value),
    }
}

pub fn doc_not_found() -> ApiError {
    ApiError::not_found("docs.doc_not_found", "Doc not found.")
}

pub fn page_not_found() -> ApiError {
    ApiError::not_found("docs.page_not_found", "Page not found.")
}

/// AI-DO `normalize_doc_id`: raw or `native_doc__<id>`.
pub fn normalize_doc_id(value: &str) -> ApiResult<String> {
    let (prefix, raw) = split_prefixed_id(value);
    if prefix.is_some_and(|prefix| prefix != SOURCE_NATIVE_DOC) {
        return Err(doc_not_found());
    }
    Ok(raw.to_string())
}

/// AI-DO `normalize_page_id`: raw or `native_doc_page__<id>`.
pub fn normalize_page_id(value: &str) -> ApiResult<String> {
    let (prefix, raw) = split_prefixed_id(value);
    if prefix.is_some_and(|prefix| prefix != PAGE_SOURCE_NATIVE_DOC) {
        return Err(page_not_found());
    }
    Ok(raw.to_string())
}

pub struct DocContext {
    pub doc: NativeDoc,
    pub targets: Vec<DocTarget>,
    pub access: NativeAccess,
}

/// AI-DO `native_doc_from_item_or_404`: a doc the caller cannot see, and a
/// trashed doc they cannot manage, are both 404.
pub async fn doc_or_404(viewer: &mut Viewer<'_>, item_id: &str) -> ApiResult<DocContext> {
    let doc_id = normalize_doc_id(item_id)?;
    let pool = viewer.state.pool.clone();
    let doc = load_doc(&pool, &doc_id).await?.ok_or_else(doc_not_found)?;
    let targets = load_targets(&pool, &doc.id).await?;
    let access = viewer.access(&doc, &targets).await?;
    if !access.can_view || (doc.trashed_at.is_some() && !access.can_manage) {
        return Err(doc_not_found());
    }
    Ok(DocContext {
        doc,
        targets,
        access,
    })
}

pub struct PageContext {
    pub page: NativePage,
    pub access: NativeAccess,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Require {
    View,
    Edit,
}

/// AI-DO `native_page_context_from_page_or_404`.
pub async fn page_or_403(
    viewer: &mut Viewer<'_>,
    page_id: &str,
    require: Require,
) -> ApiResult<PageContext> {
    let page_id = normalize_page_id(page_id)?;
    let pool = viewer.state.pool.clone();
    let page = load_page(&pool, &page_id)
        .await?
        .ok_or_else(page_not_found)?;
    let doc = load_doc(&pool, &page.doc_id)
        .await?
        .ok_or_else(page_not_found)?;
    let targets = load_targets(&pool, &doc.id).await?;
    let access = viewer.access(&doc, &targets).await?;
    let live = page.trashed_at.is_none() && doc.trashed_at.is_none();
    match require {
        Require::View if !access.can_view || !live => Err(ApiError::forbidden(
            "docs.page_access_required",
            "Page access required.",
        )),
        Require::Edit if !access.can_edit || !live => Err(ApiError::forbidden(
            "docs.doc_edit_access_required",
            "Doc edit access required.",
        )),
        _ => Ok(PageContext { page, access }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> NaiveDateTime {
        chrono::DateTime::from_timestamp(seconds, 0)
            .unwrap()
            .naive_utc()
    }

    fn doc(owner: &str) -> NativeDoc {
        NativeDoc {
            id: "d1".into(),
            workspace_id: "global".into(),
            owner_id: owner.into(),
            title: "Plan".into(),
            doc_type: "general".into(),
            source_app: "docs".into(),
            source_kind: "manual".into(),
            source_ref: None,
            generation_kind: "human".into(),
            created_at: at(0),
            updated_at: at(0),
            trashed_at: None,
        }
    }

    fn sidebar(space: &str) -> DocTarget {
        DocTarget {
            id: "t1".into(),
            doc_id: "d1".into(),
            target_app: TARGET_APP.into(),
            target_type: TARGET_SIDEBAR.into(),
            target_id: space.into(),
            is_primary: true,
            sort_order: 0,
            created_at: at(0),
        }
    }

    #[test]
    fn ids_normalize_like_ai_do_access_context() {
        // AI-DO test_native_doc_and_page_refs_normalize_raw_and_prefixed_ids
        assert_eq!(normalize_doc_id("doc-1").unwrap(), "doc-1");
        assert_eq!(normalize_doc_id("native_doc__doc-1").unwrap(), "doc-1");
        assert_eq!(normalize_page_id("page-1").unwrap(), "page-1");
        assert_eq!(
            normalize_page_id("native_doc_page__page-1").unwrap(),
            "page-1"
        );
        // test_native_ref_normalization_rejects_wrong_prefix_with_not_found_code
        assert_eq!(
            normalize_doc_id("pms_doc__doc-1").unwrap_err().code,
            "docs_doc_not_found"
        );
        assert_eq!(
            normalize_page_id("native_doc__page-1").unwrap_err().code,
            "docs_page_not_found"
        );
    }

    #[test]
    fn owner_gets_full_access_and_others_get_the_best_grant() {
        let full = resolve_access(&doc("owner"), &[], "owner", None, None);
        assert!(full.can_view && full.can_edit && full.can_share && full.can_manage);

        let private = resolve_access(&doc("owner"), &[], "other", None, Some(SpaceRole::Admin));
        assert_eq!(private, NativeAccess::default());

        let shared = resolve_access(&doc("owner"), &[], "other", Some("read"), None);
        assert!(shared.can_view && !shared.can_edit && !shared.can_share);

        let targets = [sidebar("global")];
        let viewer = resolve_access(&doc("owner"), &targets, "u", None, Some(SpaceRole::Viewer));
        assert_eq!(viewer.access_level, Some("read"));
        let member = resolve_access(
            &doc("owner"),
            &targets,
            "u",
            Some("read"),
            Some(SpaceRole::Member),
        );
        assert!(member.can_edit && !member.can_manage);
        let admin = resolve_access(&doc("owner"), &targets, "u", None, Some(SpaceRole::Admin));
        assert!(admin.can_edit && admin.can_manage && admin.can_share);
        let edit_share = resolve_access(
            &doc("owner"),
            &targets,
            "u",
            Some("edit"),
            Some(SpaceRole::Viewer),
        );
        assert!(edit_share.can_edit && !edit_share.can_manage);
    }

    #[test]
    fn only_the_sidebar_of_the_docs_own_space_projects_a_role() {
        let other_space = [sidebar("another")];
        let access = resolve_access(
            &doc("owner"),
            &other_space,
            "u",
            None,
            Some(SpaceRole::Admin),
        );
        assert!(!access.can_view);
        let mut pms = sidebar("global");
        pms.target_app = "pms".into();
        pms.target_type = "space".into();
        let access = resolve_access(&doc("owner"), &[pms], "u", None, Some(SpaceRole::Admin));
        assert!(!access.can_view);
    }

    #[test]
    fn primary_target_prefers_primary_then_order() {
        let mut a = sidebar("global");
        a.is_primary = false;
        a.id = "a".into();
        let mut b = sidebar("global");
        b.id = "b".into();
        b.sort_order = 5;
        let targets = [a, b];
        assert_eq!(primary_target(&targets).unwrap().id, "b");
        assert!(primary_target(&[]).is_none());
        assert_eq!(
            max_access_level(&[Some("read"), Some("edit"), None]),
            Some("edit")
        );
        assert_eq!(max_access_level(&[None, Some("bogus")]), None);
    }
}
