//! Whiteboards: AI-DO 0002af062 domains/whiteboard (router.py, access.py,
//! hub.py, item_mutations.py, sharing.py, scene_state.py, registry.py).
//!
//! Every route is mounted twice, as AI-DO's router was: under
//! `/v1/whiteboard` (the app-global space, AI-DO's request without a
//! workspace in app-owned mode) and under `/v1/workspaces/{space_id}/whiteboard`
//! (AI-DO `rewriteWorkspaceApiPath`). An item route answers only for a board
//! of that space (AI-DO `load_whiteboard_for_access` filters by the current
//! workspace).
//!
//! Differences forced by miy (docs/PORTING.md §3):
//! - no Yjs collaboration: one editor saves at a time with the board
//!   `revision` (a scene save names the revision it started from; an older
//!   one answers 409 `whiteboard_revision_conflict`);
//! - no anonymous link shares, PMS/meeting targets or context slots;
//! - image bytes live in the core storage (files.rs);
//! - every write locks the board row and decides access again under the
//!   lock (the space's ACL share lock too) before changing anything.
use axum::{
    Extension, Json,
    extract::{FromRequest, Path, Query, Request, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use std::collections::{HashMap, HashSet};

use crate::{
    AppState,
    api_contract::SCENE_BODY_LIMIT,
    error::{ApiError, ApiJson, ApiResult, ErrorResponse, IdentityErrorResponse, check_length},
    files, notify, people, scene,
    spaces::{self, GLOBAL, SpaceRole},
    time::now_utc,
};

pub const TARGET_APP: &str = "whiteboard";
pub const TARGET_TYPE_WORKSPACE: &str = "workspace_sidebar";

// ---------------------------------------------------------------- rows

#[derive(Clone, Debug, FromRow)]
pub struct Board {
    pub id: String,
    pub workspace_id: String,
    pub owner_id: String,
    pub title: String,
    pub source_app: String,
    pub source_kind: String,
    pub source_ref: Option<String>,
    pub generation_kind: String,
    pub revision: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub trashed_at: Option<NaiveDateTime>,
}

pub const BOARD_COLUMNS: &str = "id, workspace_id, owner_id, title, source_app, source_kind, source_ref, \
     generation_kind, revision, created_at, updated_at, trashed_at";

#[derive(Clone, Debug, FromRow)]
pub struct Target {
    pub whiteboard_id: String,
    pub target_app: String,
    pub target_type: String,
    pub target_id: String,
    pub is_primary: bool,
    pub sort_order: i32,
    pub created_at: NaiveDateTime,
}

const TARGET_COLUMNS: &str =
    "whiteboard_id, target_app, target_type, target_id, is_primary, sort_order, created_at";

impl Target {
    fn is_workspace_of(&self, board: &Board) -> bool {
        self.target_app == TARGET_APP
            && self.target_type == TARGET_TYPE_WORKSPACE
            && self.target_id == board.workspace_id
    }
}

// ---------------------------------------------------------------- access

/// AI-DO `WhiteboardAccess`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Access {
    pub access_level: Option<&'static str>,
    pub can_view: bool,
    pub can_edit: bool,
    pub can_share: bool,
    pub can_manage: bool,
}

fn level_rank(level: Option<&str>) -> u8 {
    match level {
        Some("read") => 10,
        Some("edit") => 20,
        _ => 0,
    }
}

fn level(value: Option<&str>) -> Option<&'static str> {
    match value {
        Some("read") => Some("read"),
        Some("edit") => Some("edit"),
        _ => None,
    }
}

/// AI-DO `max_access_level`.
fn max_level(left: Option<&'static str>, right: Option<&'static str>) -> Option<&'static str> {
    if level_rank(right) > level_rank(left) {
        right
    } else {
        left
    }
}

/// AI-DO `resolve_whiteboard_access` with `target_access_level`. The current
/// space membership is required for ownership and direct shares to grant
/// access. The owner has every right. Others take the higher of their direct share and the
/// targets: the board's space target (AI-DO `workspace_sidebar`) gives space
/// viewers read, members edit, and admins manage (AI-DO `project_app_workspace_target_access`).
/// Other targets (PMS spaces and task lists, meetings) project nothing in
/// miy, as unknown targets did in AI-DO.
pub fn resolve_access(
    board: &Board,
    user_id: &str,
    direct_share: Option<&str>,
    targets: &[Target],
    space_role: SpaceRole,
) -> Access {
    if space_role == SpaceRole::None {
        return Access::default();
    }
    if board.owner_id == user_id {
        return Access {
            access_level: Some("edit"),
            can_view: true,
            can_edit: true,
            can_share: true,
            can_manage: true,
        };
    }
    let mut target_level = None;
    let mut target_can_manage = false;
    for target in targets {
        let (can_view, can_edit, can_manage) = if target.is_workspace_of(board) {
            match space_role {
                SpaceRole::Admin => (true, true, true),
                SpaceRole::Member => (true, true, false),
                SpaceRole::Viewer => (true, false, false),
                SpaceRole::None => (false, false, false),
            }
        } else {
            (false, false, false)
        };
        if can_manage {
            target_can_manage = true;
        }
        let candidate = if can_edit || can_manage {
            Some("edit")
        } else if can_view {
            Some("read")
        } else {
            None
        };
        target_level = max_level(target_level, candidate);
    }
    let access_level = max_level(level(direct_share), target_level);
    Access {
        access_level,
        can_view: access_level.is_some(),
        can_edit: access_level == Some("edit"),
        can_share: target_can_manage,
        can_manage: target_can_manage,
    }
}

pub fn not_found() -> ApiError {
    ApiError::not_found("whiteboard.not_found", "Whiteboard not found.")
}

fn edit_required() -> ApiError {
    ApiError::forbidden(
        "whiteboard.edit_access_required",
        "Whiteboard edit access required.",
    )
}

fn manage_required() -> ApiError {
    ApiError::forbidden(
        "whiteboard.manage_access_required",
        "Whiteboard manage access required.",
    )
}

fn share_required() -> ApiError {
    ApiError::forbidden(
        "whiteboard.share_access_required",
        "Whiteboard share access required.",
    )
}

fn target_edit_required() -> ApiError {
    ApiError::forbidden(
        "whiteboard.target_edit_access_required",
        "Target edit access required.",
    )
}

/// A board with what the caller may do with it.
pub struct Loaded {
    pub board: Board,
    pub targets: Vec<Target>,
    pub access: Access,
}

async fn targets_of<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    board_ids: &[String],
) -> ApiResult<Vec<Target>> {
    let sql =
        format!("SELECT {TARGET_COLUMNS} FROM whiteboard_targets WHERE whiteboard_id = ANY($1)");
    Ok(sqlx::query_as::<_, Target>(&sql)
        .bind(board_ids)
        .fetch_all(executor)
        .await?)
}

async fn direct_share<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    board_id: &str,
    user_id: &str,
) -> ApiResult<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT access_level FROM whiteboard_user_shares WHERE whiteboard_id = $1 AND user_id = $2",
    )
    .bind(board_id)
    .bind(user_id)
    .fetch_optional(executor)
    .await?)
}

/// AI-DO `load_whiteboard_for_user_or_404`: the board of this space when the
/// caller may see it; a trashed board only for those who may manage it.
pub async fn load_for_user(
    state: &AppState,
    identity: &Identity,
    space_id: &str,
    item_id: &str,
) -> ApiResult<Loaded> {
    let sql =
        format!("SELECT {BOARD_COLUMNS} FROM whiteboards WHERE id = $1 AND workspace_id = $2");
    let board = sqlx::query_as::<_, Board>(&sql)
        .bind(item_id)
        .bind(space_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(not_found)?;
    load_visible_board(state, identity, board).await
}

async fn load_visible_board(
    state: &AppState,
    identity: &Identity,
    board: Board,
) -> ApiResult<Loaded> {
    let targets = targets_of(&state.pool, std::slice::from_ref(&board.id)).await?;
    let share = direct_share(&state.pool, &board.id, &identity.user_id).await?;
    let role = spaces::role(&state.acl, identity, &board.workspace_id).await?;
    let access = resolve_access(&board, &identity.user_id, share.as_deref(), &targets, role);
    if !access.can_view || (board.trashed_at.is_some() && !access.can_manage) {
        return Err(not_found());
    }
    Ok(Loaded {
        board,
        targets,
        access,
    })
}

/// [`load_for_user`] for a write: the board row is locked and the caller's
/// access (direct share, targets and space role under the ACL share lock) is
/// read again inside the transaction that will change the board.
pub async fn lock_for_user(
    tx: &mut Transaction<'_, Postgres>,
    state: &AppState,
    identity: &Identity,
    space_id: &str,
    item_id: &str,
) -> ApiResult<Loaded> {
    let sql = format!(
        "SELECT {BOARD_COLUMNS} FROM whiteboards WHERE id = $1 AND workspace_id = $2 FOR UPDATE"
    );
    let board = sqlx::query_as::<_, Board>(&sql)
        .bind(item_id)
        .bind(space_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(not_found)?;
    let targets = targets_of(&mut **tx, std::slice::from_ref(&board.id)).await?;
    let share = direct_share(&mut **tx, &board.id, &identity.user_id).await?;
    let role = spaces::role_in(tx, &state.acl, identity, &board.workspace_id).await?;
    let access = resolve_access(&board, &identity.user_id, share.as_deref(), &targets, role);
    if !access.can_view || (board.trashed_at.is_some() && !access.can_manage) {
        return Err(not_found());
    }
    Ok(Loaded {
        board,
        targets,
        access,
    })
}

/// AI-DO `ensure_whiteboard_workspace_access`: listing, creating and the
/// sharing people list need the current space. The global space is everyone's;
/// an app space needs membership.
pub async fn require_workspace(
    state: &AppState,
    identity: &Identity,
    space_id: &str,
) -> ApiResult<SpaceRole> {
    spaces::ensure_exists(&state.pool, space_id).await?;
    let role = spaces::role(&state.acl, identity, space_id).await?;
    if role == SpaceRole::None {
        return Err(ApiError::forbidden(
            "whiteboard.workspace_context_required",
            "Whiteboard requests require a workspace context.",
        ));
    }
    Ok(role)
}

/// AI-DO `target_write_allowed`. Only the board's own space target is known;
/// any other target (PMS, meetings) is refused like an unknown AI-DO target.
fn target_write_allowed(
    app: &str,
    target_type: &str,
    id: &str,
    space_id: &str,
    role: SpaceRole,
) -> bool {
    app == TARGET_APP
        && target_type == TARGET_TYPE_WORKSPACE
        && id == space_id
        && role >= SpaceRole::Member
}

// ---------------------------------------------------------------- output

#[derive(Serialize, Clone, utoipa::ToSchema)]
pub struct PrimaryTarget {
    app: String,
    #[serde(rename = "type")]
    target_type: String,
    id: String,
    sort_order: i32,
}

#[derive(Serialize, Clone, utoipa::ToSchema)]
pub struct TargetItem {
    app: String,
    #[serde(rename = "type")]
    target_type: String,
    id: String,
    sort_order: i32,
    is_primary: bool,
}

/// AI-DO `WhiteboardHubItem` plus `revision` (miy).
#[derive(Serialize, Clone, utoipa::ToSchema)]
pub struct HubItem {
    pub id: String,
    #[schema(required = true)]
    workspace_id: Option<String>,
    #[schema(required = true)]
    workspace_key: Option<String>,
    source_app: String,
    #[schema(value_type = WhiteboardSourceType)]
    source_type: &'static str,
    source_id: String,
    source_kind: String,
    #[schema(required = true)]
    source_ref: Option<String>,
    generation_kind: String,
    pub location_label: String,
    target_label: String,
    #[schema(required = true)]
    primary_target: Option<PrimaryTarget>,
    targets: Vec<TargetItem>,
    pub source_badge: String,
    #[schema(required = true)]
    source_deeplink: Option<String>,
    pub title: String,
    pub created_by_id: String,
    created_by_name: String,
    #[serde(serialize_with = "crate::time::serialize")]
    #[schema(value_type = String)]
    pub created_at: NaiveDateTime,
    #[serde(serialize_with = "crate::time::serialize")]
    #[schema(value_type = String)]
    pub updated_at: NaiveDateTime,
    #[serde(serialize_with = "crate::time::serialize_opt")]
    #[schema(value_type = Option<String>, required = true)]
    pub trashed_at: Option<NaiveDateTime>,
    pub is_favorite: bool,
    is_private: bool,
    #[serde(serialize_with = "crate::time::serialize_opt")]
    #[schema(value_type = Option<String>, required = true)]
    pub last_viewed_at: Option<NaiveDateTime>,
    can_view: bool,
    can_edit: bool,
    can_share: bool,
    can_manage: bool,
    /// Stored scene revision used for concurrent saves.
    revision: i64,
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[schema(rename_all = "snake_case")]
enum WhiteboardSourceType {
    Whiteboard,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct Detail {
    #[serde(flatten)]
    item: HubItem,
    /// Excalidraw scene object. `files` references stored images by row ID.
    #[schema(value_type = HashMap<String, Value>)]
    scene: Value,
}

#[derive(Default, Clone, Copy)]
pub struct Pref {
    is_favorite: bool,
    last_viewed_at: Option<NaiveDateTime>,
}

/// App-relative path of a board in this app's frame (AI-DO
/// `app_resource_path(workspace, "whiteboard", id)`).
pub fn board_path(board_id: &str, space_id: &str) -> String {
    if space_id == GLOBAL {
        format!("/{board_id}")
    } else {
        format!("/{board_id}?space={space_id}")
    }
}

/// Python `str.title()` for ASCII app IDs.
fn title_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut start = true;
    for c in value.chars() {
        if c.is_alphabetic() {
            if start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            start = false;
        } else {
            out.push(c);
            start = true;
        }
    }
    out
}

/// AI-DO `primary_target`: primary first, then sort order, then age.
fn primary_target(targets: &[Target]) -> Option<&Target> {
    targets
        .iter()
        .min_by_key(|target| (!target.is_primary, target.sort_order, target.created_at))
}

/// AI-DO `resolve_target_label`.
fn target_label(board: &Board, target: Option<&Target>) -> String {
    match target {
        None => "Unfiled".to_string(),
        Some(target) if target.is_workspace_of(board) => "Workspace Whiteboards".to_string(),
        Some(target) => format!("{}:{}", target.target_app, target.target_type),
    }
}

/// AI-DO `describe_source` without the PMS and meeting deep links.
fn source_badge(board: &Board) -> String {
    match board.source_app.as_str() {
        "whiteboard" => "Whiteboard".to_string(),
        "pms" => "PMS".to_string(),
        "meeting" => "Meeting".to_string(),
        other => title_case(&other.replace('_', " ")),
    }
}

/// AI-DO `_serialize_whiteboard_item`.
pub fn hub_item(
    board: &Board,
    targets: &[Target],
    access: Access,
    pref: Pref,
    owner_name: Option<&str>,
) -> HubItem {
    let mine: Vec<&Target> = targets
        .iter()
        .filter(|target| target.whiteboard_id == board.id)
        .collect();
    let owned: Vec<Target> = mine.iter().map(|target| (*target).clone()).collect();
    let primary = primary_target(&owned);
    let mut sorted = owned.clone();
    sorted.sort_by(|a, b| {
        (
            !a.is_primary,
            &a.target_app,
            &a.target_type,
            a.sort_order,
            a.created_at,
        )
            .cmp(&(
                !b.is_primary,
                &b.target_app,
                &b.target_type,
                b.sort_order,
                b.created_at,
            ))
    });
    let label = target_label(board, primary);
    HubItem {
        id: board.id.clone(),
        workspace_id: Some(board.workspace_id.clone()),
        workspace_key: Some(board.workspace_id.clone()),
        source_app: board.source_app.clone(),
        source_type: "whiteboard",
        source_id: board.id.clone(),
        source_kind: board.source_kind.clone(),
        source_ref: board.source_ref.clone(),
        generation_kind: board.generation_kind.clone(),
        location_label: label.clone(),
        target_label: label,
        primary_target: primary.map(|target| PrimaryTarget {
            app: target.target_app.clone(),
            target_type: target.target_type.clone(),
            id: target.target_id.clone(),
            sort_order: target.sort_order,
        }),
        targets: sorted
            .iter()
            .map(|target| TargetItem {
                app: target.target_app.clone(),
                target_type: target.target_type.clone(),
                id: target.target_id.clone(),
                sort_order: target.sort_order,
                is_primary: target.is_primary,
            })
            .collect(),
        source_badge: source_badge(board),
        source_deeplink: Some(board_path(&board.id, &board.workspace_id)),
        title: board.title.clone(),
        created_by_id: board.owner_id.clone(),
        created_by_name: owner_name.unwrap_or_default().to_string(),
        created_at: board.created_at,
        updated_at: board.updated_at,
        trashed_at: board.trashed_at,
        is_favorite: pref.is_favorite,
        is_private: primary.is_none(),
        last_viewed_at: pref.last_viewed_at,
        can_view: access.can_view,
        can_edit: access.can_edit,
        can_share: access.can_share,
        can_manage: access.can_manage,
        revision: board.revision,
    }
}

async fn pref_of(pool: &PgPool, user_id: &str, board_id: &str) -> ApiResult<Pref> {
    let row: Option<(bool, Option<NaiveDateTime>)> = sqlx::query_as(
        "SELECT is_favorite, last_viewed_at FROM whiteboard_user_item_prefs WHERE user_id = $1 AND whiteboard_id = $2",
    )
    .bind(user_id)
    .bind(board_id)
    .fetch_optional(pool)
    .await?;
    Ok(row
        .map(|(is_favorite, last_viewed_at)| Pref {
            is_favorite,
            last_viewed_at,
        })
        .unwrap_or_default())
}

/// AI-DO `_serialize_whiteboard_detail` for a loaded board.
async fn detail(state: &AppState, identity: &Identity, loaded: &Loaded) -> ApiResult<Detail> {
    let scene: Value = sqlx::query_scalar("SELECT scene FROM whiteboards WHERE id = $1")
        .bind(&loaded.board.id)
        .fetch_optional(&state.pool)
        .await?
        .unwrap_or_else(scene::empty_scene);
    let pref = pref_of(&state.pool, &identity.user_id, &loaded.board.id).await?;
    let names = people::display_names(identity, [loaded.board.owner_id.clone()]).await;
    Ok(Detail {
        item: hub_item(
            &loaded.board,
            &loaded.targets,
            loaded.access,
            pref,
            names.get(&loaded.board.owner_id).map(String::as_str),
        ),
        scene,
    })
}

/// Re-read after a committed write, with the access decided under the lock.
async fn reload_detail(
    state: &AppState,
    identity: &Identity,
    board_id: &str,
    access: Access,
) -> ApiResult<Detail> {
    let sql = format!("SELECT {BOARD_COLUMNS} FROM whiteboards WHERE id = $1");
    let board = sqlx::query_as::<_, Board>(&sql)
        .bind(board_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(not_found)?;
    let targets = targets_of(&state.pool, std::slice::from_ref(&board.id)).await?;
    detail(
        state,
        identity,
        &Loaded {
            board,
            targets,
            access,
        },
    )
    .await
}

// ---------------------------------------------------------------- params

fn space_param(params: &HashMap<String, String>) -> String {
    params
        .get("space_id")
        .cloned()
        .unwrap_or_else(|| GLOBAL.to_string())
}

fn item_param(params: &HashMap<String, String>) -> ApiResult<String> {
    params.get("item_id").cloned().ok_or_else(not_found)
}

// ---------------------------------------------------------------- hub

/// AI-DO `WhiteboardHubView`.
const HUB_VIEWS: [&str; 5] = ["all", "mine", "recent", "favorites", "archived"];

/// AI-DO `resolve_whiteboard_hub_view`.
fn resolve_view(view: Option<&str>, category: Option<&str>) -> ApiResult<&'static str> {
    if let Some(view) = view {
        return HUB_VIEWS
            .iter()
            .copied()
            .find(|candidate| *candidate == view)
            .ok_or_else(|| {
                ApiError::validation(
                    "view: Input should be 'all', 'mine', 'recent', 'favorites' or 'archived'",
                )
            });
    }
    Ok(match category.unwrap_or("all") {
        "my" | "mine" => "mine",
        "recent" => "recent",
        "favorite" | "favorites" => "favorites",
        "archived" => "archived",
        _ => "all",
    })
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct HubResponse {
    items: Vec<HubItem>,
    total: usize,
    page: i64,
    page_size: i64,
    can_publish: bool,
}

fn int_param(
    query: &HashMap<String, String>,
    name: &str,
    default: i64,
    min: i64,
    max: i64,
) -> ApiResult<i64> {
    let Some(raw) = query.get(name) else {
        return Ok(default);
    };
    let value: i64 = raw.trim().parse().map_err(|_| {
        ApiError::validation(format!(
            "{name}: Input should be a valid integer, unable to parse string as an integer"
        ))
    })?;
    if value < min {
        return Err(ApiError::validation(format!(
            "{name}: Input should be greater than or equal to {min}"
        )));
    }
    if value > max {
        return Err(ApiError::validation(format!(
            "{name}: Input should be less than or equal to {max}"
        )));
    }
    Ok(value)
}

fn non_empty(query: &HashMap<String, String>, name: &str) -> Option<String> {
    query.get(name).filter(|value| !value.is_empty()).cloned()
}

/// `GET …/hub`: AI-DO `build_whiteboard_hub_response`. Boards of the current
/// space the caller may see, filtered and sorted like AI-DO.
#[utoipa::path(
    get, path = "/hub", operation_id = "listWhiteboardHub", tag = "Whiteboards",
    params(
        ("view" = Option<String>, Query, description = "all, mine, recent, favorites, or archived. Takes precedence over category."),
        ("category" = Option<String>, Query, description = "Legacy filter: my, mine, recent, favorite, favorites, archived, or all."),
        ("page" = Option<i64>, Query, minimum = 1, maximum = 9007199254740991i64, description = "One-based page. Defaults to 1."),
        ("page_size" = Option<i64>, Query, minimum = 1, maximum = 200, description = "Defaults to 50."),
        ("sort_by" = Option<String>, Query, description = "title, created_at, last_viewed_at, or target_sort_order. Defaults to updated_at for omitted or unrecognized values."),
        ("sort_dir" = Option<String>, Query, description = "asc or desc. Defaults to desc."),
        ("q" = Option<String>, Query),
        ("source_app" = Option<String>, Query),
        ("source_kind" = Option<String>, Query),
        ("space_id" = Option<String>, Query, description = "Filter by workspace ID."),
        ("target_app" = Option<String>, Query),
        ("target_type" = Option<String>, Query),
        ("target_id" = Option<String>, Query),
    ),
    responses(
        (status = 200, description = "Page of visible boards", body = HubResponse),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = ErrorResponse),
        (status = 404, description = "The workspace or board is missing or is not visible", body = ErrorResponse),
        (status = 422, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn hub(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Json<HubResponse>> {
    let space_id = space_param(&params);
    let view = resolve_view(
        query.get("view").map(String::as_str),
        query.get("category").map(String::as_str),
    )?;
    let page = int_param(&query, "page", 1, 1, i64::MAX / 1024)?;
    let page_size = int_param(&query, "page_size", 50, 1, 200)?;
    let sort_by = query
        .get("sort_by")
        .cloned()
        .unwrap_or_else(|| "updated_at".into());
    let sort_dir = query.get("sort_dir").map(String::as_str).unwrap_or("desc");
    if sort_dir != "asc" && sort_dir != "desc" {
        return Err(ApiError::validation(
            "sort_dir: Input should be 'asc' or 'desc'",
        ));
    }
    let q = query.get("q").cloned().unwrap_or_default();
    let source_app = non_empty(&query, "source_app");
    let source_kind = non_empty(&query, "source_kind");
    let space_filter = non_empty(&query, "space_id");
    let target_app = non_empty(&query, "target_app");
    let target_type = non_empty(&query, "target_type");
    let target_id = non_empty(&query, "target_id");

    let role = require_workspace(&state, &identity, &space_id).await?;
    // AI-DO `load_accessible_whiteboards`, with the access predicate in SQL.
    let sql = format!(
        "SELECT {BOARD_COLUMNS} FROM whiteboards w WHERE w.workspace_id = $1 AND (w.owner_id = $2 \
           OR EXISTS (SELECT 1 FROM whiteboard_user_shares s WHERE s.whiteboard_id = w.id AND s.user_id = $2) \
           OR EXISTS (SELECT 1 FROM whiteboard_targets t WHERE t.whiteboard_id = w.id \
                AND t.target_app = '{TARGET_APP}' AND t.target_type = '{TARGET_TYPE_WORKSPACE}' AND t.target_id = w.workspace_id))"
    );
    let boards = sqlx::query_as::<_, Board>(&sql)
        .bind(&space_id)
        .bind(&identity.user_id)
        .fetch_all(&state.pool)
        .await?;
    let ids: Vec<String> = boards.iter().map(|board| board.id.clone()).collect();
    let targets = targets_of(&state.pool, &ids).await?;
    let shares: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT whiteboard_id, access_level FROM whiteboard_user_shares WHERE whiteboard_id = ANY($1) AND user_id = $2",
    )
    .bind(&ids)
    .bind(&identity.user_id)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .collect();
    let prefs: HashMap<String, Pref> = sqlx::query_as::<_, (String, bool, Option<NaiveDateTime>)>(
        "SELECT whiteboard_id, is_favorite, last_viewed_at FROM whiteboard_user_item_prefs \
         WHERE user_id = $1 AND whiteboard_id = ANY($2)",
    )
    .bind(&identity.user_id)
    .bind(&ids)
    .fetch_all(&state.pool)
    .await?
    .into_iter()
    .map(|(id, is_favorite, last_viewed_at)| {
        (
            id,
            Pref {
                is_favorite,
                last_viewed_at,
            },
        )
    })
    .collect();
    let mut items = Vec::new();
    let mut owners = HashSet::new();
    let mut visible = Vec::new();
    for board in &boards {
        let mine: Vec<Target> = targets
            .iter()
            .filter(|target| target.whiteboard_id == board.id)
            .cloned()
            .collect();
        let access = resolve_access(
            board,
            &identity.user_id,
            shares.get(&board.id).map(String::as_str),
            &mine,
            role,
        );
        if !access.can_view || (board.trashed_at.is_some() && !access.can_manage) {
            continue;
        }
        owners.insert(board.owner_id.clone());
        visible.push((board, mine, access));
    }
    let names = people::display_names(&identity, owners).await;
    for (board, mine, access) in visible {
        items.push(hub_item(
            board,
            &mine,
            access,
            prefs.get(&board.id).copied().unwrap_or_default(),
            names.get(&board.owner_id).map(String::as_str),
        ));
    }

    // AI-DO `_filter_whiteboards`.
    let me = identity.user_id.as_str();
    items.retain(|item| match view {
        "mine" => item.trashed_at.is_none() && item.created_by_id == me,
        "recent" => item.trashed_at.is_none() && item.last_viewed_at.is_some(),
        "favorites" => item.trashed_at.is_none() && item.is_favorite,
        "archived" => item.trashed_at.is_some(),
        _ => item.trashed_at.is_none(),
    });
    let (filter_app, filter_type, filter_id) = if space_filter.is_some() {
        (
            Some("pms".to_string()),
            Some("space".to_string()),
            space_filter,
        )
    } else {
        (target_app, target_type, target_id)
    };
    if let Some(source_app) = &source_app {
        items.retain(|item| &item.source_app == source_app);
    }
    if let Some(source_kind) = &source_kind {
        items.retain(|item| &item.source_kind == source_kind);
    }
    if filter_app.is_some() || filter_type.is_some() || filter_id.is_some() {
        items.retain(|item| {
            item.targets.iter().any(|target| {
                filter_app.as_ref().is_none_or(|app| &target.app == app)
                    && filter_type
                        .as_ref()
                        .is_none_or(|t| &target.target_type == t)
                    && filter_id.as_ref().is_none_or(|id| &target.id == id)
            })
        });
    }
    let search = q.trim().to_lowercase();
    if !search.is_empty() {
        items.retain(|item| {
            item.title.to_lowercase().contains(&search)
                || item.location_label.to_lowercase().contains(&search)
                || item.source_badge.to_lowercase().contains(&search)
        });
    }

    // AI-DO `_sort_whiteboards`. Python's `sorted(reverse=True)` keeps equal
    // keys in input order; so does a stable sort with the comparison flipped.
    let reverse = sort_dir != "asc";
    let sort_by = sort_by.as_str();
    items.sort_by(|a, b| {
        let ordering = compare_items(a, b, sort_by);
        if reverse {
            ordering.reverse()
        } else {
            ordering
        }
    });
    let total = items.len();
    let start = ((page - 1) * page_size).min(total as i64) as usize;
    let end = (start + page_size as usize).min(total);
    Ok(Json(HubResponse {
        items: items[start..end].to_vec(),
        total,
        page,
        page_size,
        can_publish: role >= SpaceRole::Member,
    }))
}

fn compare_items(a: &HubItem, b: &HubItem, sort_by: &str) -> std::cmp::Ordering {
    match sort_by {
        "title" => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
        "created_at" => a.created_at.cmp(&b.created_at),
        "last_viewed_at" => a
            .last_viewed_at
            .unwrap_or(NaiveDateTime::MIN)
            .cmp(&b.last_viewed_at.unwrap_or(NaiveDateTime::MIN)),
        "target_sort_order" => a
            .targets
            .first()
            .map_or(0, |target| target.sort_order)
            .cmp(&b.targets.first().map_or(0, |target| target.sort_order)),
        _ => a.updated_at.cmp(&b.updated_at),
    }
}

// ---------------------------------------------------------------- items

#[derive(Deserialize, utoipa::ToSchema)]
pub struct TargetPayload {
    /// Use `whiteboard` for a board shared in its own workspace.
    #[schema(min_length = 1, max_length = 64)]
    app: String,
    /// Use `workspace_sidebar` for a board shared in its own workspace.
    #[serde(rename = "type")]
    #[schema(min_length = 1, max_length = 64)]
    target_type: String,
    /// The board's space ID, or `global` for the app-wide space.
    #[schema(min_length = 1, max_length = 128)]
    id: String,
    /// Defaults to 0.
    #[serde(default)]
    sort_order: i32,
}

fn check_target(payload: &TargetPayload) -> ApiResult<()> {
    check_length("app", &payload.app, 1, 64)?;
    check_length("type", &payload.target_type, 1, 64)?;
    check_length("id", &payload.id, 1, 128)
}

fn default_source_app() -> String {
    "whiteboard".into()
}

fn default_source_kind() -> String {
    "manual".into()
}

fn default_generation_kind() -> String {
    "human".into()
}

/// AI-DO `CreateWhiteboardRequest`.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateRequest {
    #[schema(min_length = 1, max_length = 200)]
    title: String,
    /// Excalidraw scene object. Omit or send null to start with an empty scene.
    #[serde(default)]
    #[schema(value_type = Option<HashMap<String, Value>>)]
    scene: Option<Value>,
    /// Defaults to `whiteboard`.
    #[serde(default = "default_source_app")]
    #[schema(min_length = 1, max_length = 64)]
    source_app: String,
    /// Defaults to `manual`.
    #[serde(default = "default_source_kind")]
    #[schema(min_length = 1, max_length = 64)]
    source_kind: String,
    #[serde(default)]
    #[schema(max_length = 128)]
    source_ref: Option<String>,
    /// Defaults to `human`.
    #[serde(default = "default_generation_kind")]
    #[schema(min_length = 1, max_length = 32)]
    generation_kind: String,
    #[serde(default)]
    primary_target: Option<TargetPayload>,
}

fn scene_file_invalid() -> ApiError {
    ApiError::unprocessable(
        "whiteboard.scene_file_invalid",
        "Scene images must be stored files of this whiteboard.",
    )
}

/// The scene's files name stored images of this board (scene.rs).
pub(crate) async fn check_scene_files<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    board_id: Option<&str>,
    value: &Value,
) -> ApiResult<Vec<String>> {
    // Leave space for the board detail and its sharing data in the response.
    const MAX_SCENE_BYTES: usize = SCENE_BODY_LIMIT - 64 * 1024;
    if value.to_string().len() > MAX_SCENE_BYTES {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "whiteboard.scene_too_large",
            "This board is too large to save. Move some elements to another board.",
        ));
    }
    let ids = scene::storage_ids(value).map_err(|_| scene_file_invalid())?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let Some(board_id) = board_id else {
        return Err(scene_file_invalid());
    };
    let ids: Vec<String> = ids.into_iter().collect();
    let found: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM whiteboard_files WHERE id = ANY($1) AND whiteboard_id = $2 \
         AND status IN ('ready', 'attached')",
    )
    .bind(&ids)
    .bind(board_id)
    .fetch_one(executor)
    .await?;
    if found as usize != ids.len() {
        return Err(scene_file_invalid());
    }
    Ok(ids)
}

/// `POST …/items`: AI-DO `create_whiteboard_item` / `create_whiteboard_for_user`.
#[utoipa::path(
    post, path = "/items", operation_id = "createWhiteboardItem", tag = "Whiteboards",
    description = "The JSON request is limited to 4 MiB. Images can be reserved after the board is created.",
    request_body = CreateRequest,
    responses(
        (status = 201, description = "Created board", body = Detail),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = ErrorResponse),
        (status = 404, description = "The workspace or board is missing or is not visible", body = ErrorResponse),
        (status = 413, description = "The Excalidraw scene exceeds the supported size", body = ErrorResponse),
        (status = 422, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn create_item(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(payload): ApiJson<CreateRequest>,
) -> ApiResult<impl IntoResponse> {
    let space_id = space_param(&params);
    check_length("title", &payload.title, 1, 200)?;
    check_length("source_app", &payload.source_app, 1, 64)?;
    check_length("source_kind", &payload.source_kind, 1, 64)?;
    if let Some(source_ref) = &payload.source_ref {
        check_length("source_ref", source_ref, 0, 128)?;
    }
    check_length("generation_kind", &payload.generation_kind, 1, 32)?;
    if let Some(target) = &payload.primary_target {
        check_target(target)?;
    }
    let scene_value = match payload.scene {
        None | Some(Value::Null) => scene::empty_scene(),
        Some(value @ Value::Object(_)) => value,
        Some(_) => {
            return Err(ApiError::validation(
                "scene: Input should be a valid dictionary",
            ));
        }
    };
    // A new board has no stored images yet; they are uploaded to it after.
    let pool = state.pool.clone();
    check_scene_files(&pool, None, &scene_value).await?;

    let mut tx = state.pool.begin().await?;
    spaces::ensure_exists(&state.pool, &space_id).await?;
    let role = spaces::role_in(&mut tx, &state.acl, &identity, &space_id).await?;
    if role == SpaceRole::None {
        return Err(ApiError::forbidden(
            "whiteboard.workspace_context_required",
            "Whiteboard requests require a workspace context.",
        ));
    }
    if let Some(target) = &payload.primary_target {
        if !target_write_allowed(
            &target.app,
            &target.target_type,
            &target.id,
            &space_id,
            role,
        ) {
            return Err(target_edit_required());
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_utc();
    sqlx::query(
        "INSERT INTO whiteboards (id, workspace_id, owner_id, title, source_app, source_kind, source_ref, \
           generation_kind, scene, revision, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 1, $10, $10)",
    )
    .bind(&id)
    .bind(&space_id)
    .bind(&identity.user_id)
    .bind(payload.title.trim())
    .bind(&payload.source_app)
    .bind(&payload.source_kind)
    .bind(&payload.source_ref)
    .bind(&payload.generation_kind)
    .bind(&scene_value)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    if let Some(target) = &payload.primary_target {
        sqlx::query(
            "INSERT INTO whiteboard_targets (id, whiteboard_id, target_app, target_type, target_id, is_primary, \
               sort_order, created_by_id, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, true, $6, $7, $8, $8)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&id)
        .bind(&target.app)
        .bind(&target.target_type)
        .bind(&target.id)
        .bind(target.sort_order)
        .bind(&identity.user_id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    let loaded = load_for_user(&state, &identity, &space_id, &id).await?;
    Ok((
        StatusCode::CREATED,
        Json(detail(&state, &identity, &loaded).await?),
    ))
}

/// `GET …/items/{item_id}`.
#[utoipa::path(
    get, path = "/items/{item_id}", operation_id = "getWhiteboardItem", tag = "Whiteboards",
    params(("item_id" = String, Path)),
    responses(
        (status = 200, description = "Board detail and Excalidraw scene", body = Detail),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 404, description = "The workspace or board is missing or is not visible", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn get_item(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Detail>> {
    let space_id = space_param(&params);
    let item_id = item_param(&params)?;
    spaces::ensure_exists(&state.pool, &space_id).await?;
    let loaded = load_for_user(&state, &identity, &space_id, &item_id).await?;
    Ok(Json(detail(&state, &identity, &loaded).await?))
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct BoardReference {
    id: String,
    title: String,
    space_id: String,
}

#[utoipa::path(
    get, path = "/items/{item_id}/reference", operation_id = "getWhiteboardReference", tag = "Whiteboards",
    params(("item_id" = String, Path)),
    responses(
        (status = 200, description = "A readable, active board's ID and title", body = BoardReference),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 404, description = "The workspace or board is missing or is not visible", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn get_reference(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<BoardReference>> {
    let item_id = item_param(&params)?;
    let sql = format!(
        "SELECT {BOARD_COLUMNS} FROM whiteboards WHERE id = $1 AND trashed_at IS NULL \
         AND ($2::text IS NULL OR workspace_id = $2)"
    );
    let board = sqlx::query_as::<_, Board>(&sql)
        .bind(item_id)
        .bind(params.get("space_id"))
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(not_found)?;
    spaces::ensure_exists(&state.pool, &board.workspace_id).await?;
    let loaded = load_visible_board(&state, &identity, board).await?;
    Ok(Json(BoardReference {
        id: loaded.board.id,
        title: loaded.board.title,
        space_id: loaded.board.workspace_id,
    }))
}

fn revision_conflict() -> ApiError {
    ApiError::conflict(
        "whiteboard.revision_conflict",
        "The whiteboard was changed by someone else. Reload it or keep your version.",
    )
}

#[derive(utoipa::ToSchema)]
pub struct UpdateRequest {
    /// Omit or send null to leave the title unchanged.
    #[schema(min_length = 1, max_length = 200)]
    title: Option<String>,
    /// Omit to leave the scene unchanged. An explicit null clears it.
    #[schema(value_type = Option<HashMap<String, Value>>)]
    scene: Option<Value>,
    /// Integer scene revision. Required when `scene` is present, including null.
    base_revision: Option<i64>,
}

impl FromRequest<AppState> for UpdateRequest {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &AppState) -> ApiResult<Self> {
        let ApiJson(body) = ApiJson::<Value>::from_request(request, state).await?;
        let Some(fields) = body.as_object() else {
            return Err(ApiError::validation("Input should be a valid dictionary"));
        };
        let title = match fields.get("title") {
            None | Some(Value::Null) => None,
            Some(Value::String(title)) => {
                check_length("title", title, 1, 200)?;
                Some(title.trim().to_string())
            }
            Some(_) => {
                return Err(ApiError::validation(
                    "title: Input should be a valid string",
                ));
            }
        };
        let scene = match fields.get("scene") {
            None => None,
            Some(Value::Null) => Some(scene::empty_scene()),
            Some(value @ Value::Object(_)) => Some(value.clone()),
            Some(_) => {
                return Err(ApiError::validation(
                    "scene: Input should be a valid dictionary",
                ));
            }
        };
        let base_revision = match (&scene, fields.get("base_revision")) {
            (None, _) => None,
            (Some(_), Some(value)) => Some(value.as_i64().ok_or_else(|| {
                ApiError::validation("base_revision: Input should be a valid integer")
            })?),
            (Some(_), None) => return Err(ApiError::validation("base_revision: Field required")),
        };
        Ok(Self {
            title,
            scene,
            base_revision,
        })
    }
}

/// `PATCH …/items/{item_id}`: AI-DO `update_whiteboard_item`
/// (item_mutations.py). A scene save carries `base_revision`, the revision
/// the editor loaded or last saved. A changed scene from an older revision is
/// refused with 409 instead of overwriting (AI-DO merged concurrent editors
/// with Yjs). A scene equal to the stored one (ignoring per-viewer editor
/// state) changes nothing, whatever its revision.
#[utoipa::path(
    patch, path = "/items/{item_id}", operation_id = "updateWhiteboardItem", tag = "Whiteboards",
    description = "The JSON request is limited to 4 MiB. Scene saves require base_revision and use 409 for conflicting newer changes.",
    request_body = UpdateRequest,
    params(("item_id" = String, Path)),
    responses(
        (status = 200, description = "Board detail after saving", body = Detail),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = ErrorResponse),
        (status = 404, description = "The workspace or board is missing or is not visible", body = ErrorResponse),
        (status = 409, description = "A different scene was saved since base_revision (whiteboard_revision_conflict)", body = ErrorResponse),
        (status = 413, description = "The Excalidraw scene exceeds the supported size", body = ErrorResponse),
        (status = 422, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn update_item(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(params): Path<HashMap<String, String>>,
    UpdateRequest {
        title,
        scene: new_scene,
        base_revision,
    }: UpdateRequest,
) -> ApiResult<Json<Detail>> {
    let space_id = space_param(&params);
    let item_id = item_param(&params)?;
    spaces::ensure_exists(&state.pool, &space_id).await?;

    let mut tx = state.pool.begin().await?;
    let loaded = lock_for_user(&mut tx, &state, &identity, &space_id, &item_id).await?;
    if !loaded.access.can_edit {
        return Err(edit_required());
    }
    if title.is_some() && !loaded.access.can_manage {
        return Err(manage_required());
    }
    let mut changed = false;
    let mut scene_changed = false;
    let mut referenced = Vec::new();
    if let Some(title) = &title {
        if title != &loaded.board.title {
            sqlx::query("UPDATE whiteboards SET title = $2 WHERE id = $1")
                .bind(&loaded.board.id)
                .bind(title)
                .execute(&mut *tx)
                .await?;
            changed = true;
        }
    }
    if let Some(new_scene) = &new_scene {
        referenced = check_scene_files(&mut *tx, Some(&loaded.board.id), new_scene).await?;
        let stored: Value = sqlx::query_scalar("SELECT scene FROM whiteboards WHERE id = $1")
            .bind(&loaded.board.id)
            .fetch_one(&mut *tx)
            .await?;
        if !scene::matches(&stored, new_scene) {
            if base_revision != Some(loaded.board.revision) {
                return Err(revision_conflict());
            }
            crate::versions::record(&mut tx, &loaded.board, &stored, &identity.user_id).await?;
            miy_app_collab::db::invalidate(&mut tx, &loaded.board.id).await?;
            sqlx::query(
                "UPDATE whiteboards SET scene = $2, revision = revision + 1, updated_at = $3 WHERE id = $1",
            )
            .bind(&loaded.board.id)
            .bind(new_scene)
            .bind(now_utc())
            .execute(&mut *tx)
            .await?;
            changed = true;
            scene_changed = true;
        } else if base_revision == Some(loaded.board.revision) {
            miy_app_collab::db::invalidate(&mut tx, &loaded.board.id).await?;
        }
    }
    if changed && !scene_changed {
        sqlx::query("UPDATE whiteboards SET updated_at = $2 WHERE id = $1")
            .bind(&loaded.board.id)
            .bind(now_utc())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    if scene_changed {
        files::attach_referenced(&state.pool, &identity, &loaded.board.id, &referenced).await;
    }
    Ok(Json(
        reload_detail(&state, &identity, &loaded.board.id, loaded.access).await?,
    ))
}

/// `DELETE …/items/{item_id}`: move to the trash (AI-DO sets `trashed_at`).
/// The people it was shared with lose its notification.
#[utoipa::path(
    delete, path = "/items/{item_id}", operation_id = "trashWhiteboardItem", tag = "Whiteboards",
    params(("item_id" = String, Path)),
    responses(
        (status = 204, description = "Board moved to the trash"),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = ErrorResponse),
        (status = 404, description = "The workspace or board is missing or is not visible", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn trash_item(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<StatusCode> {
    let space_id = space_param(&params);
    let item_id = item_param(&params)?;
    spaces::ensure_exists(&state.pool, &space_id).await?;
    let mut tx = state.pool.begin().await?;
    let loaded = lock_for_user(&mut tx, &state, &identity, &space_id, &item_id).await?;
    if !loaded.access.can_manage {
        return Err(manage_required());
    }
    sqlx::query("UPDATE whiteboards SET trashed_at = $2 WHERE id = $1")
        .bind(&loaded.board.id)
        .bind(now_utc())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    notify::retract_board(&identity, &loaded.board.id).await;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST …/items/{item_id}/restore`.
#[utoipa::path(
    post, path = "/items/{item_id}/restore", operation_id = "restoreWhiteboardItem", tag = "Whiteboards",
    params(("item_id" = String, Path)),
    responses(
        (status = 200, description = "Restored board detail", body = Detail),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = ErrorResponse),
        (status = 404, description = "The workspace or board is missing or is not visible", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn restore_item(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Detail>> {
    let space_id = space_param(&params);
    let item_id = item_param(&params)?;
    spaces::ensure_exists(&state.pool, &space_id).await?;
    let mut tx = state.pool.begin().await?;
    let loaded = lock_for_user(&mut tx, &state, &identity, &space_id, &item_id).await?;
    if !loaded.access.can_manage {
        return Err(manage_required());
    }
    if loaded.board.trashed_at.is_some() {
        sqlx::query("UPDATE whiteboards SET trashed_at = NULL, updated_at = $2 WHERE id = $1")
            .bind(&loaded.board.id)
            .bind(now_utc())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Json(
        reload_detail(&state, &identity, &loaded.board.id, loaded.access).await?,
    ))
}

/// `DELETE …/items/{item_id}/permanent`: only from the trash. Its stored
/// images are released after the commit (files.rs).
#[utoipa::path(
    delete, path = "/items/{item_id}/permanent", operation_id = "deleteWhiteboardItemPermanently", tag = "Whiteboards",
    params(("item_id" = String, Path)),
    responses(
        (status = 204, description = "Board permanently deleted"),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = ErrorResponse),
        (status = 404, description = "The workspace or board is missing or is not visible", body = ErrorResponse),
        (status = 409, description = "Move the board to the trash before permanently deleting it", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn delete_item_permanently(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<StatusCode> {
    let space_id = space_param(&params);
    let item_id = item_param(&params)?;
    spaces::ensure_exists(&state.pool, &space_id).await?;
    let mut tx = state.pool.begin().await?;
    let loaded = lock_for_user(&mut tx, &state, &identity, &space_id, &item_id).await?;
    if !loaded.access.can_manage {
        return Err(manage_required());
    }
    if loaded.board.trashed_at.is_none() {
        return Err(ApiError::conflict(
            "whiteboard.archive_before_permanent_delete",
            "Archive the whiteboard before permanent deletion.",
        ));
    }
    let released = files::release_for_board(&mut tx, &loaded.board.id).await?;
    // Targets, shares and preferences go with it (ON DELETE CASCADE).
    sqlx::query("DELETE FROM whiteboards WHERE id = $1")
        .bind(&loaded.board.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    files::detach_released(&state.pool, &identity, &released).await;
    notify::retract_board(&identity, &loaded.board.id).await;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT …/items/{item_id}/target`: AI-DO `_upsert_primary_target`. The only
/// target miy knows is the board's space ("워크스페이스 공개").
#[utoipa::path(
    put, path = "/items/{item_id}/target", operation_id = "updateWhiteboardTarget", tag = "Whiteboards",
    request_body = TargetPayload,
    params(("item_id" = String, Path)),
    responses(
        (status = 200, description = "Board detail after sharing with its workspace", body = Detail),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = ErrorResponse),
        (status = 404, description = "The workspace or board is missing or is not visible", body = ErrorResponse),
        (status = 422, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn update_target(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(payload): ApiJson<TargetPayload>,
) -> ApiResult<Json<Detail>> {
    let space_id = space_param(&params);
    let item_id = item_param(&params)?;
    check_target(&payload)?;
    spaces::ensure_exists(&state.pool, &space_id).await?;
    let mut tx = state.pool.begin().await?;
    let loaded = lock_for_user(&mut tx, &state, &identity, &space_id, &item_id).await?;
    if !loaded.access.can_manage {
        return Err(manage_required());
    }
    let role = spaces::role_in(&mut tx, &state.acl, &identity, &loaded.board.workspace_id).await?;
    if !target_write_allowed(
        &payload.app,
        &payload.target_type,
        &payload.id,
        &loaded.board.workspace_id,
        role,
    ) {
        return Err(target_edit_required());
    }
    let now = now_utc();
    sqlx::query("UPDATE whiteboard_targets SET is_primary = false, updated_at = $2 WHERE whiteboard_id = $1 AND is_primary")
        .bind(&loaded.board.id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO whiteboard_targets (id, whiteboard_id, target_app, target_type, target_id, is_primary, \
           sort_order, created_by_id, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, true, $6, $7, $8, $8) \
         ON CONFLICT (whiteboard_id, target_app, target_type, target_id) \
         DO UPDATE SET is_primary = true, sort_order = EXCLUDED.sort_order, updated_at = EXCLUDED.updated_at",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&loaded.board.id)
    .bind(&payload.app)
    .bind(&payload.target_type)
    .bind(&payload.id)
    .bind(payload.sort_order)
    .bind(&identity.user_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE whiteboards SET updated_at = $2 WHERE id = $1")
        .bind(&loaded.board.id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    // The caller's access after the change (the target may widen it).
    let access = access_in(&mut tx, &state, &identity, &loaded.board.id).await?;
    tx.commit().await?;
    Ok(Json(
        reload_detail(&state, &identity, &loaded.board.id, access).await?,
    ))
}

/// `DELETE …/items/{item_id}/target`: AI-DO `_delete_primary_target` (back to
/// "비공개").
#[utoipa::path(
    delete, path = "/items/{item_id}/target", operation_id = "deleteWhiteboardTarget", tag = "Whiteboards",
    params(("item_id" = String, Path)),
    responses(
        (status = 200, description = "Board detail after removing the workspace target", body = Detail),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = ErrorResponse),
        (status = 404, description = "The workspace or board is missing or is not visible", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn delete_target(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<Detail>> {
    let space_id = space_param(&params);
    let item_id = item_param(&params)?;
    spaces::ensure_exists(&state.pool, &space_id).await?;
    let mut tx = state.pool.begin().await?;
    let loaded = lock_for_user(&mut tx, &state, &identity, &space_id, &item_id).await?;
    if !loaded.access.can_manage {
        return Err(manage_required());
    }
    let now = now_utc();
    sqlx::query("DELETE FROM whiteboard_targets WHERE whiteboard_id = $1 AND is_primary")
        .bind(&loaded.board.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE whiteboards SET updated_at = $2 WHERE id = $1")
        .bind(&loaded.board.id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    let access = access_in(&mut tx, &state, &identity, &loaded.board.id).await?;
    tx.commit().await?;
    if !access.can_view {
        // AI-DO answers through `_lookup_item`, which no longer finds a board
        // the caller cannot see.
        return Err(not_found());
    }
    Ok(Json(
        reload_detail(&state, &identity, &loaded.board.id, access).await?,
    ))
}

/// The caller's access to a board already locked in `tx`.
async fn access_in(
    tx: &mut Transaction<'_, Postgres>,
    state: &AppState,
    identity: &Identity,
    board_id: &str,
) -> ApiResult<Access> {
    let sql = format!("SELECT {BOARD_COLUMNS} FROM whiteboards WHERE id = $1");
    let board = sqlx::query_as::<_, Board>(&sql)
        .bind(board_id)
        .fetch_one(&mut **tx)
        .await?;
    let targets = targets_of(&mut **tx, std::slice::from_ref(&board.id)).await?;
    let share = direct_share(&mut **tx, &board.id, &identity.user_id).await?;
    let role = spaces::role_in(tx, &state.acl, identity, &board.workspace_id).await?;
    Ok(resolve_access(
        &board,
        &identity.user_id,
        share.as_deref(),
        &targets,
        role,
    ))
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct FavoriteResponse {
    is_favorite: bool,
}

/// `PATCH …/items/{item_id}/favorite`.
#[utoipa::path(
    patch, path = "/items/{item_id}/favorite", operation_id = "toggleWhiteboardFavorite", tag = "Whiteboards",
    params(("item_id" = String, Path)),
    responses(
        (status = 200, description = "Current favorite state", body = FavoriteResponse),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 404, description = "The workspace or board is missing or is not visible", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn toggle_favorite(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<FavoriteResponse>> {
    let space_id = space_param(&params);
    let item_id = item_param(&params)?;
    spaces::ensure_exists(&state.pool, &space_id).await?;
    let mut tx = state.pool.begin().await?;
    let loaded = lock_for_user(&mut tx, &state, &identity, &space_id, &item_id).await?;
    let is_favorite: bool = sqlx::query_scalar(
        "INSERT INTO whiteboard_user_item_prefs (id, user_id, whiteboard_id, is_favorite) VALUES ($1, $2, $3, true) \
         ON CONFLICT (user_id, whiteboard_id) DO UPDATE SET is_favorite = NOT whiteboard_user_item_prefs.is_favorite \
         RETURNING is_favorite",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&identity.user_id)
    .bind(&loaded.board.id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(FavoriteResponse { is_favorite }))
}

/// `POST …/items/{item_id}/view`: last viewed (AI-DO), and the board's share
/// notification for the caller is read (miy).
#[utoipa::path(
    post, path = "/items/{item_id}/view", operation_id = "recordWhiteboardView", tag = "Whiteboards",
    params(("item_id" = String, Path)),
    responses(
        (status = 204, description = "View recorded"),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 404, description = "The workspace or board is missing or is not visible", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn record_view(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<StatusCode> {
    let space_id = space_param(&params);
    let item_id = item_param(&params)?;
    spaces::ensure_exists(&state.pool, &space_id).await?;
    let mut tx = state.pool.begin().await?;
    let loaded = lock_for_user(&mut tx, &state, &identity, &space_id, &item_id).await?;
    sqlx::query(
        "INSERT INTO whiteboard_user_item_prefs (id, user_id, whiteboard_id, is_favorite, last_viewed_at) \
         VALUES ($1, $2, $3, false, $4) \
         ON CONFLICT (user_id, whiteboard_id) DO UPDATE SET last_viewed_at = EXCLUDED.last_viewed_at",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&identity.user_id)
    .bind(&loaded.board.id)
    .bind(now_utc())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    notify::mark_board_read(&identity, &loaded.board.id).await;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- sharing

/// AI-DO `ShareableUserItem`; `email` is empty (the directory has none) and
/// `primary_group` names the person's group.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ShareableUser {
    id: String,
    email: String,
    full_name: String,
    #[schema(required = true)]
    primary_group: Option<String>,
}

fn directory_unavailable() -> ApiError {
    ApiError::unavailable(
        "whiteboard.directory_unavailable",
        "The people directory could not be read.",
    )
}

/// `GET …/shareable-users?q=`: AI-DO `list_whiteboard_shareable_users`.
/// People of the current space (an app space: its direct members), not the
/// caller, at most 30.
#[utoipa::path(
    get, path = "/shareable-users", operation_id = "listWhiteboardShareableUsers", tag = "Whiteboards",
    params(("q" = Option<String>, Query, description = "Filter workspace members by name. The trimmed query may contain up to 200 characters.")),
    responses(
        (status = 200, description = "Up to 30 eligible people", body = [ShareableUser]),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = ErrorResponse),
        (status = 404, description = "Space not found", body = ErrorResponse),
        (status = 422, description = "The query exceeds 200 characters after trimming", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn shareable_users(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(params): Path<HashMap<String, String>>,
    Query(query): Query<HashMap<String, String>>,
) -> ApiResult<Json<Vec<ShareableUser>>> {
    let space_id = space_param(&params);
    require_workspace(&state, &identity, &space_id).await?;
    let q = query
        .get("q")
        .map(|q| q.trim().to_string())
        .unwrap_or_default();
    check_length("q", &q, 0, 200)?;
    let people = people::search_people(&identity, &q, 30)
        .await
        .map_err(|_| directory_unavailable())?;
    let mut users = Vec::new();
    for person in people {
        if person.id == identity.user_id {
            continue;
        }
        if !spaces::is_direct_member(&state.pool, &identity, &space_id, &person.id).await? {
            continue;
        }
        users.push(ShareableUser {
            id: person.id,
            email: String::new(),
            full_name: person.name,
            primary_group: person.primary_group,
        });
    }
    Ok(Json(users))
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct UserShareItem {
    user_id: String,
    email: String,
    full_name: String,
    #[schema(value_type = UserShareAccess)]
    access_level: String,
}

/// AI-DO `WhiteboardSharingResponse`. Link shares are not offered in miy, so
/// `link_share` is always null.
#[derive(Serialize, utoipa::ToSchema)]
pub struct SharingResponse {
    whiteboard_id: String,
    owner_id: String,
    users: Vec<UserShareItem>,
    #[schema(schema_with = null_link_share)]
    link_share: (),
}

fn null_link_share() -> utoipa::openapi::schema::Object {
    utoipa::openapi::schema::Object::with_type(utoipa::openapi::schema::Type::Null)
}

async fn sharing_response(
    state: &AppState,
    identity: &Identity,
    board: &Board,
) -> ApiResult<SharingResponse> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT user_id, access_level FROM whiteboard_user_shares WHERE whiteboard_id = $1",
    )
    .bind(&board.id)
    .fetch_all(&state.pool)
    .await?;
    let names = people::display_names(identity, rows.iter().map(|(id, _)| id.clone())).await;
    let mut users: Vec<UserShareItem> = rows
        .into_iter()
        .map(|(user_id, access_level)| UserShareItem {
            full_name: names.get(&user_id).cloned().unwrap_or_default(),
            email: String::new(),
            user_id,
            access_level,
        })
        .collect();
    users.sort_by(|a, b| {
        (
            a.full_name.to_lowercase(),
            a.email.to_lowercase(),
            &a.user_id,
        )
            .cmp(&(
                b.full_name.to_lowercase(),
                b.email.to_lowercase(),
                &b.user_id,
            ))
    });
    Ok(SharingResponse {
        whiteboard_id: board.id.clone(),
        owner_id: board.owner_id.clone(),
        users,
        link_share: (),
    })
}

/// `GET …/items/{item_id}/sharing`: AI-DO `load_whiteboard_for_share_or_403`.
#[utoipa::path(
    get, path = "/items/{item_id}/sharing", operation_id = "getWhiteboardSharing", tag = "Whiteboards",
    params(("item_id" = String, Path)),
    responses(
        (status = 200, description = "Current board shares", body = SharingResponse),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = ErrorResponse),
        (status = 404, description = "The workspace or board is missing or is not visible", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn get_sharing(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<SharingResponse>> {
    let space_id = space_param(&params);
    let item_id = item_param(&params)?;
    spaces::ensure_exists(&state.pool, &space_id).await?;
    let loaded = load_for_user(&state, &identity, &space_id, &item_id).await?;
    if !loaded.access.can_share {
        return Err(share_required());
    }
    Ok(Json(
        sharing_response(&state, &identity, &loaded.board).await?,
    ))
}

#[allow(dead_code)]
#[derive(utoipa::ToSchema)]
#[schema(rename_all = "snake_case")]
enum UserShareAccess {
    Read,
    Edit,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UserShareRequest {
    #[schema(value_type = UserShareAccess)]
    access_level: String,
}

/// `PUT …/items/{item_id}/sharing/users/{user_id}`: AI-DO
/// `upsert_whiteboard_user_share`, plus a `share` notification to a person
/// newly given the board (miy).
#[utoipa::path(
    put, path = "/items/{item_id}/sharing/users/{user_id}", operation_id = "upsertWhiteboardUserShare", tag = "Whiteboards",
    request_body = UserShareRequest,
    params(("item_id" = String, Path), ("user_id" = String, Path)),
    responses(
        (status = 200, description = "Current board shares", body = SharingResponse),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = ErrorResponse),
        (status = 404, description = "The workspace or board is missing or is not visible", body = ErrorResponse),
        (status = 409, description = "The user already owns the board or is not a member of its workspace", body = ErrorResponse),
        (status = 422, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn upsert_user_share(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(params): Path<HashMap<String, String>>,
    ApiJson(payload): ApiJson<UserShareRequest>,
) -> ApiResult<Json<SharingResponse>> {
    let space_id = space_param(&params);
    let item_id = item_param(&params)?;
    let user_id = params.get("user_id").cloned().unwrap_or_default();
    let level = match payload.access_level.as_str() {
        "read" => "read",
        "edit" => "edit",
        _ => {
            return Err(ApiError::validation(
                "access_level: Input should be 'read' or 'edit'",
            ));
        }
    };
    spaces::ensure_exists(&state.pool, &space_id).await?;
    // The directory is read before the board lock is taken; its answer is
    // used only after the access checks below, in AI-DO's order.
    let person = if user_id == identity.user_id {
        Ok(None)
    } else {
        people::find_user(&identity, &user_id).await
    };

    let mut tx = state.pool.begin().await?;
    let loaded = lock_for_user(&mut tx, &state, &identity, &space_id, &item_id).await?;
    if !loaded.access.can_share {
        return Err(share_required());
    }
    if user_id == identity.user_id {
        return Err(ApiError::conflict(
            "whiteboard.owner_already_has_full_access",
            "Owner already has full access.",
        ));
    }
    let Some((target_id, _name)) = person.map_err(|_| directory_unavailable())? else {
        return Err(ApiError::not_found(
            "auth.user_not_found",
            "User not found.",
        ));
    };
    if !spaces::is_direct_member(&mut *tx, &identity, &loaded.board.workspace_id, &target_id)
        .await?
    {
        return Err(ApiError::conflict(
            "whiteboard.shared_users_workspace_required",
            "Shared users must be members of the same workspace.",
        ));
    }
    let existing = direct_share(&mut *tx, &loaded.board.id, &target_id).await?;
    if existing.is_some() {
        sqlx::query(
            "UPDATE whiteboard_user_shares SET access_level = $3 WHERE whiteboard_id = $1 AND user_id = $2",
        )
        .bind(&loaded.board.id)
        .bind(&target_id)
        .bind(level)
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO whiteboard_user_shares (id, whiteboard_id, user_id, access_level, created_by_id, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&loaded.board.id)
        .bind(&target_id)
        .bind(level)
        .bind(&identity.user_id)
        .bind(now_utc())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    if existing.is_none() {
        notify::board_shared(&identity, &loaded.board, &target_id).await;
    }
    Ok(Json(
        sharing_response(&state, &identity, &loaded.board).await?,
    ))
}

/// `DELETE …/items/{item_id}/sharing/users/{user_id}`: the person also loses
/// the board's notification (miy).
#[utoipa::path(
    delete, path = "/items/{item_id}/sharing/users/{user_id}", operation_id = "deleteWhiteboardUserShare", tag = "Whiteboards",
    params(("item_id" = String, Path), ("user_id" = String, Path)),
    responses(
        (status = 200, description = "Current board shares after removal", body = SharingResponse),
        (status = 401, description = "Platform identity could not be verified", body = IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = ErrorResponse),
        (status = 404, description = "The workspace or board is missing or is not visible", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = ErrorResponse),
    )
)]
pub async fn delete_user_share(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(params): Path<HashMap<String, String>>,
) -> ApiResult<Json<SharingResponse>> {
    let space_id = space_param(&params);
    let item_id = item_param(&params)?;
    let user_id = params.get("user_id").cloned().unwrap_or_default();
    spaces::ensure_exists(&state.pool, &space_id).await?;
    let mut tx = state.pool.begin().await?;
    let loaded = lock_for_user(&mut tx, &state, &identity, &space_id, &item_id).await?;
    if !loaded.access.can_share {
        return Err(share_required());
    }
    let removed =
        sqlx::query("DELETE FROM whiteboard_user_shares WHERE whiteboard_id = $1 AND user_id = $2")
            .bind(&loaded.board.id)
            .bind(&user_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    tx.commit().await?;
    if removed > 0 {
        notify::retract_board_for(&identity, &loaded.board.id, &user_id).await;
    }
    Ok(Json(
        sharing_response(&state, &identity, &loaded.board).await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(owner: &str) -> Board {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        Board {
            id: "b1".into(),
            workspace_id: "space-1".into(),
            owner_id: owner.into(),
            title: "Board".into(),
            source_app: "whiteboard".into(),
            source_kind: "manual".into(),
            source_ref: None,
            generation_kind: "human".into(),
            revision: 1,
            created_at: now,
            updated_at: now,
            trashed_at: None,
        }
    }

    fn workspace_target(board: &Board) -> Target {
        Target {
            whiteboard_id: board.id.clone(),
            target_app: TARGET_APP.into(),
            target_type: TARGET_TYPE_WORKSPACE.into(),
            target_id: board.workspace_id.clone(),
            is_primary: true,
            sort_order: 0,
            created_at: board.created_at,
        }
    }

    #[test]
    fn owner_has_every_right() {
        let b = board("owner");
        let access = resolve_access(&b, "owner", None, &[], SpaceRole::Member);
        assert!(access.can_view && access.can_edit && access.can_share && access.can_manage);
    }

    #[test]
    fn private_board_is_owner_only() {
        // AI-DO test_private_whiteboard_is_owner_only.
        let b = board("owner");
        let access = resolve_access(&b, "other", None, &[], SpaceRole::Admin);
        assert!(!access.can_view);
    }

    #[test]
    fn direct_shares_and_space_roles_combine_like_ai_do() {
        let b = board("owner");
        let read = resolve_access(&b, "other", Some("read"), &[], SpaceRole::Member);
        assert_eq!(read.access_level, Some("read"));
        assert!(read.can_view && !read.can_edit && !read.can_share);
        let target = vec![workspace_target(&b)];
        let member = resolve_access(&b, "other", Some("read"), &target, SpaceRole::Member);
        assert!(member.can_edit && !member.can_manage && !member.can_share);
        let admin = resolve_access(&b, "other", None, &target, SpaceRole::Admin);
        assert!(admin.can_edit && admin.can_manage && admin.can_share);
        let outsider = resolve_access(&b, "other", None, &target, SpaceRole::None);
        assert!(!outsider.can_view);
        let removed_viewer = resolve_access(&b, "other", Some("read"), &[], SpaceRole::None);
        assert_eq!(removed_viewer, Access::default());
        let removed_owner = resolve_access(&b, "owner", None, &target, SpaceRole::None);
        assert_eq!(removed_owner, Access::default());
        let mut foreign = workspace_target(&b);
        foreign.target_app = "pms".into();
        foreign.target_type = "space".into();
        let pms = resolve_access(&b, "other", None, &[foreign], SpaceRole::Admin);
        assert!(!pms.can_view);
    }

    #[test]
    fn hub_views_and_labels_follow_ai_do() {
        assert_eq!(resolve_view(None, Some("my")).unwrap(), "mine");
        assert_eq!(resolve_view(None, Some("favorite")).unwrap(), "favorites");
        assert_eq!(resolve_view(None, None).unwrap(), "all");
        assert!(resolve_view(Some("trash"), None).is_err());
        let b = board("owner");
        assert_eq!(target_label(&b, None), "Unfiled");
        assert_eq!(
            target_label(&b, Some(&workspace_target(&b))),
            "Workspace Whiteboards"
        );
        assert_eq!(title_case("source app"), "Source App");
        assert_eq!(board_path("b1", GLOBAL), "/b1");
        assert_eq!(board_path("b1", "s1"), "/b1?space=s1");
    }

    #[test]
    fn target_writes_accept_only_the_boards_space() {
        assert!(target_write_allowed(
            TARGET_APP,
            TARGET_TYPE_WORKSPACE,
            "s1",
            "s1",
            SpaceRole::Member
        ));
        assert!(!target_write_allowed(
            TARGET_APP,
            TARGET_TYPE_WORKSPACE,
            "s2",
            "s1",
            SpaceRole::Admin
        ));
        assert!(!target_write_allowed(
            "pms",
            "space",
            "s1",
            "s1",
            SpaceRole::Admin
        ));
        assert!(!target_write_allowed(
            TARGET_APP,
            TARGET_TYPE_WORKSPACE,
            "s1",
            "s1",
            SpaceRole::None
        ));
    }
}
