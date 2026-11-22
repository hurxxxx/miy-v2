//! Report review and messenger settings for managers: AI-DO
//! domains/dm/safety_admin.py and domains/admin/dm_safety_router.py
//! (`/admin/dm-safety/*` → `/v1/dm-safety/*`), plus the user-side report,
//! block and terms endpoints of domains/dm/router.py.
//!
//! miy: AI-DO allowed its platform administrators. Messenger declares its own
//! manager role (`dm_safety_managers`), granted inside the app. The first
//! manager claims the empty role (`POST /v1/dm-safety/managers/claim`, only
//! while nobody holds it); afterwards managers grant and revoke it. The
//! platform administrator flag is not used. See docs/PORTING.md §4.
use crate::{
    attachments, audit,
    conversations::query_limit,
    error::{ApiError, ApiJson, ApiResult},
    live, messages, notify,
    request::{is_uuid, normalize_optional_route_id, route_id},
    safety::{self, RESOLUTION_NOTE_MAX_LENGTH},
    schemas::{
        DmBlockItem, DmBlockListResponse, DmBlockResult, DmReportRequest, DmReportResult,
        DmUserItem, UgcTermsAcceptRequest, UgcTermsAcceptResult, UgcTermsStatus,
    },
    store::{self, Conversation},
    time::{iso, iso_opt, now_utc, parse_query_datetime},
    ugc, users,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Response,
};
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};

// --- user side: reports, blocks, terms ------------------------------------------

/// `POST /v1/dm/reports`.
pub async fn report_dm(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<DmReportRequest>,
) -> ApiResult<Json<DmReportResult>> {
    if !is_uuid(&payload.client_report_id)
        && uuid::Uuid::parse_str(&payload.client_report_id).is_err()
    {
        return Err(ApiError::validation(
            "client_report_id: String should match pattern",
        ));
    }
    if payload.client_report_id.len() != 36 {
        return Err(ApiError::validation(
            "client_report_id: String should match pattern",
        ));
    }
    let conversation_id = payload.conversation_id.trim().to_string();
    crate::error::check_length(
        "conversation_id",
        &conversation_id,
        1,
        crate::request::DM_ROUTE_ID_MAX_LENGTH,
    )?;
    let message_id = normalize_optional_route_id(payload.message_id)?;
    let reported_user_id = normalize_optional_route_id(payload.reported_user_id)?;
    if !safety::REPORT_CATEGORIES.contains(&payload.category.as_str()) {
        return Err(ApiError::validation(
            "category: Input should be a report category",
        ));
    }
    let note = payload
        .note
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    if let Some(note) = &note {
        crate::error::check_length("note", note, 0, safety::REPORT_NOTE_MAX_LENGTH)?;
    }
    let request = safety::ReportRequest {
        client_report_id: payload.client_report_id.to_lowercase(),
        conversation_id,
        category: payload.category,
        message_id,
        reported_user_id,
        note,
        block: payload.block,
    };
    let mut tx = pool.begin().await?;
    let outcome = safety::create_report(&mut tx, &pool, &identity, &request).await?;
    tx.commit().await?;
    if let Some(created) = &outcome.created {
        notify::report_created(&pool, &identity, &outcome.report_id, created).await;
    }
    Ok(Json(DmReportResult {
        report_id: outcome.report_id,
        status: "received",
        duplicate: outcome.duplicate,
        blocked: outcome.blocked,
    }))
}

/// `GET /v1/dm/blocks`.
pub async fn list_dm_blocks(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<DmBlockListResponse>> {
    let mut conn = pool.acquire().await?;
    let rows = safety::list_blocks(&mut conn, &identity.user_id).await?;
    drop(conn);
    let names = users::names(&pool, &identity, rows.iter().map(|(id, _)| id.clone())).await;
    Ok(Json(DmBlockListResponse {
        items: rows
            .into_iter()
            .map(|(id, created_at)| DmBlockItem {
                user: names.item(&id),
                blocked_at: iso(created_at),
            })
            .collect(),
    }))
}

async fn set_dm_block(
    pool: &PgPool,
    identity: &Identity,
    user_id: &str,
    blocked: bool,
) -> ApiResult<DmBlockResult> {
    route_id(user_id)?;
    let mut tx = pool.begin().await?;
    let change = safety::set_block(&mut tx, pool, identity, user_id, blocked).await?;
    tx.commit().await?;
    Ok(DmBlockResult {
        user_id: change.user_id,
        blocked: change.blocked,
        changed: change.changed,
    })
}

/// `PUT /v1/dm/blocks/{user_id}`.
pub async fn block_dm_user(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(user_id): Path<String>,
) -> ApiResult<Json<DmBlockResult>> {
    Ok(Json(set_dm_block(&pool, &identity, &user_id, true).await?))
}

/// `DELETE /v1/dm/blocks/{user_id}`.
pub async fn unblock_dm_user(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(user_id): Path<String>,
) -> ApiResult<Json<DmBlockResult>> {
    Ok(Json(set_dm_block(&pool, &identity, &user_id, false).await?))
}

/// `GET /v1/dm/terms`.
pub async fn get_dm_terms(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<UgcTermsStatus>> {
    let mut conn = pool.acquire().await?;
    Ok(Json(ugc::terms_status(&mut conn, &identity.user_id).await?))
}

/// `POST /v1/dm/terms/accept`.
pub async fn accept_dm_terms(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<UgcTermsAcceptRequest>,
) -> ApiResult<Json<UgcTermsAcceptResult>> {
    let version = payload.version.trim().to_string();
    crate::error::check_length("version", &version, 1, 32)?;
    let mut tx = pool.begin().await?;
    let (accepted_at, changed) = ugc::accept_terms(&mut tx, &identity.user_id, &version).await?;
    tx.commit().await?;
    Ok(Json(UgcTermsAcceptResult {
        version,
        accepted_at: iso(accepted_at),
        changed,
    }))
}

// --- manager role -----------------------------------------------------------------

pub async fn is_manager(conn: &mut PgConnection, user_id: &str) -> ApiResult<bool> {
    let found: Option<String> =
        sqlx::query_scalar("SELECT user_id FROM dm_safety_managers WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(conn)
            .await?;
    Ok(found.is_some())
}

async fn require_manager(conn: &mut PgConnection, user_id: &str) -> ApiResult<()> {
    if is_manager(conn, user_id).await? {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "dm_safety.manager_required",
            "Only messenger managers can do this.",
        ))
    }
}

#[derive(Debug, Serialize)]
pub struct ManagerStatus {
    pub manager: bool,
    /// Nobody holds the role yet: the caller may claim it.
    pub claimable: bool,
}

pub async fn load_manager_status(
    conn: &mut PgConnection,
    user_id: &str,
) -> ApiResult<ManagerStatus> {
    let (manager, claimable) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM dm_safety_managers WHERE user_id = $1), \
                NOT EXISTS(SELECT 1 FROM dm_safety_managers)",
    )
    .bind(user_id)
    .fetch_one(conn)
    .await?;
    Ok(ManagerStatus { manager, claimable })
}

/// miy: `GET /v1/dm-safety/me`.
pub async fn manager_status(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<ManagerStatus>> {
    let mut conn = pool.acquire().await?;
    Ok(Json(
        load_manager_status(&mut conn, &identity.user_id).await?,
    ))
}

#[derive(Debug, Serialize)]
pub struct ManagerItem {
    user: DmUserItem,
    granted_by_id: Option<String>,
    created_at: String,
}

async fn manager_items(pool: &PgPool, identity: &Identity) -> ApiResult<Vec<ManagerItem>> {
    let rows: Vec<(String, Option<String>, NaiveDateTime)> =
        sqlx::query_as("SELECT user_id, granted_by_id, created_at FROM dm_safety_managers ORDER BY created_at, user_id")
            .fetch_all(pool)
            .await?;
    let names = users::names(pool, identity, rows.iter().map(|row| row.0.clone())).await;
    Ok(rows
        .into_iter()
        .map(|(id, granted_by_id, created_at)| ManagerItem {
            user: names.item(&id),
            granted_by_id,
            created_at: iso(created_at),
        })
        .collect())
}

/// miy: `GET /v1/dm-safety/managers`.
pub async fn list_managers(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<Value>> {
    let mut conn = pool.acquire().await?;
    require_manager(&mut conn, &identity.user_id).await?;
    drop(conn);
    Ok(Json(
        json!({"items": manager_items(&pool, &identity).await?}),
    ))
}

/// miy: `POST /v1/dm-safety/managers/claim`: only while the role is empty.
pub async fn claim_manager(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<Value>> {
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    sqlx::query("LOCK TABLE dm_safety_managers IN EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM dm_safety_managers")
        .fetch_one(&mut *tx)
        .await?;
    if count > 0 {
        return Err(ApiError::conflict(
            "dm_safety.manager_already_assigned",
            "Messenger managers are already assigned.",
        ));
    }
    sqlx::query(
        "INSERT INTO dm_safety_managers (user_id, granted_by_id, created_at) VALUES ($1, NULL, $2)",
    )
    .bind(me)
    .bind(now_utc())
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut tx,
        "dm_safety.manager_claimed",
        "user",
        Some(me),
        "메신저 관리 역할을 처음 맡았다",
        me,
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"items": manager_items(&pool, &identity).await?}),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManagerGrantRequest {
    user_id: String,
}

/// miy: `POST /v1/dm-safety/managers`.
pub async fn grant_manager(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    ApiJson(request): ApiJson<ManagerGrantRequest>,
) -> ApiResult<Json<Value>> {
    let target = request.user_id.trim().to_string();
    route_id(&target)?;
    let me = identity.user_id.as_str();
    let mut conn = pool.acquire().await?;
    require_manager(&mut conn, me).await?;
    drop(conn);
    let active = users::active_users(&pool, &identity, std::slice::from_ref(&target)).await?;
    if !active.contains_key(&target) {
        return Err(ApiError::not_found(
            "dm_safety.user_not_found",
            "User not found.",
        ));
    }
    let mut tx = pool.begin().await?;
    sqlx::query("LOCK TABLE dm_safety_managers IN EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    require_manager(&mut tx, me).await?;
    let inserted = sqlx::query(
        "INSERT INTO dm_safety_managers (user_id, granted_by_id, created_at) VALUES ($1, $2, $3) \
         ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(&target)
    .bind(me)
    .bind(now_utc())
    .execute(&mut *tx)
    .await?;
    if inserted.rows_affected() > 0 {
        audit::record(
            &mut tx,
            "dm_safety.manager_granted",
            "user",
            Some(&target),
            "메신저 관리 역할을 주었다",
            me,
            json!({}),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(
        json!({"items": manager_items(&pool, &identity).await?}),
    ))
}

/// miy: `DELETE /v1/dm-safety/managers/{user_id}`. The last manager stays.
pub async fn revoke_manager(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(user_id): Path<String>,
) -> ApiResult<Json<Value>> {
    route_id(&user_id)?;
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    sqlx::query("LOCK TABLE dm_safety_managers IN EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    require_manager(&mut tx, me).await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM dm_safety_managers")
        .fetch_one(&mut *tx)
        .await?;
    if !is_manager(&mut tx, &user_id).await? {
        return Err(ApiError::not_found(
            "dm_safety.manager_not_found",
            "Manager not found.",
        ));
    }
    if count <= 1 {
        return Err(ApiError::unprocessable(
            "dm_safety.last_manager",
            "At least one messenger manager must remain.",
        ));
    }
    sqlx::query("DELETE FROM dm_safety_managers WHERE user_id = $1")
        .bind(&user_id)
        .execute(&mut *tx)
        .await?;
    audit::record(
        &mut tx,
        "dm_safety.manager_revoked",
        "user",
        Some(&user_id),
        "메신저 관리 역할을 거두었다",
        me,
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"items": manager_items(&pool, &identity).await?}),
    ))
}

// --- terms settings (miy) -----------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct TermsSettingsItem {
    version: String,
    required: bool,
    document_url: Option<String>,
    contact_email: Option<String>,
    contact_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermsSettingsRequest {
    version: String,
    required: bool,
    #[serde(default)]
    document_url: Option<String>,
    #[serde(default)]
    contact_email: Option<String>,
    #[serde(default)]
    contact_url: Option<String>,
}

fn settings_item(settings: ugc::TermsSettings) -> TermsSettingsItem {
    TermsSettingsItem {
        version: settings.version,
        required: settings.required,
        document_url: settings.document_url,
        contact_email: settings.contact_email,
        contact_url: settings.contact_url,
    }
}

/// miy: `GET /v1/dm-safety/terms-settings`.
pub async fn get_terms_settings(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<TermsSettingsItem>> {
    let mut conn = pool.acquire().await?;
    require_manager(&mut conn, &identity.user_id).await?;
    Ok(Json(settings_item(ugc::settings(&mut conn).await?)))
}

fn optional_url(field: &str, value: Option<String>) -> ApiResult<Option<String>> {
    let value = value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    if let Some(url) = &value
        && (url.chars().count() > 2048
            || !(url.starts_with("https://") || url.starts_with("http://")))
    {
        return Err(ApiError::validation(format!(
            "{field}: Input should be an http(s) URL"
        )));
    }
    Ok(value)
}

/// miy: `PUT /v1/dm-safety/terms-settings` (AI-DO `AI_DO_UGC_*` settings).
pub async fn put_terms_settings(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    ApiJson(request): ApiJson<TermsSettingsRequest>,
) -> ApiResult<Json<TermsSettingsItem>> {
    let version = request.version.trim().to_string();
    crate::error::check_length("version", &version, 1, 32)?;
    let document_url = optional_url("document_url", request.document_url)?;
    let contact_url = optional_url("contact_url", request.contact_url)?;
    let contact_email = request
        .contact_email
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    if let Some(email) = &contact_email {
        crate::error::check_length("contact_email", email, 3, 320)?;
    }
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    require_manager(&mut tx, me).await?;
    sqlx::query(
        "UPDATE dm_terms_settings SET version = $1, required = $2, document_url = $3, contact_email = $4, \
         contact_url = $5, updated_by_id = $6, updated_at = $7 WHERE id = 1",
    )
    .bind(&version)
    .bind(request.required)
    .bind(&document_url)
    .bind(&contact_email)
    .bind(&contact_url)
    .bind(me)
    .bind(now_utc())
    .execute(&mut *tx)
    .await?;
    audit::record(
        &mut tx,
        "ugc_terms.settings_updated",
        "dm_terms_settings",
        Some("1"),
        "이용규칙 설정을 바꿨다",
        me,
        json!({"version": version, "required": request.required}),
    )
    .await?;
    let settings = ugc::settings(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(settings_item(settings)))
}

// --- report review (safety_admin.py) -----------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
struct ReportRow {
    id: String,
    reporter_user_id: String,
    reported_user_id: String,
    conversation_id: String,
    message_id: Option<String>,
    category: String,
    note: Option<String>,
    source: String,
    status: String,
    action: Option<String>,
    resolution_note: Option<String>,
    resolved_by_id: Option<String>,
    resolved_at: Option<NaiveDateTime>,
    created_at: NaiveDateTime,
}

const REPORT_COLUMNS: &str = "id, reporter_user_id, reported_user_id, conversation_id, message_id, category, note, \
     source, status, action, resolution_note, resolved_by_id, resolved_at, created_at";

#[derive(Debug, Serialize)]
pub struct ReportSummary {
    id: String,
    status: String,
    category: String,
    source: String,
    has_message: bool,
    conversation_type: String,
    action: Option<String>,
    created_at: String,
    resolved_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ReportDetail {
    #[serde(flatten)]
    summary: ReportSummary,
    note: Option<String>,
    conversation_id: String,
    message_id: Option<String>,
    reporter: Option<DmUserItem>,
    reported_user: Option<DmUserItem>,
    resolution_note: Option<String>,
    resolved_by: Option<DmUserItem>,
    message_hidden: Option<bool>,
    /// miy: the author deleted the reported message.
    message_deleted: Option<bool>,
}

async fn conversation_type(conn: &mut PgConnection, conversation_id: &str) -> ApiResult<String> {
    let kind: Option<String> =
        sqlx::query_scalar("SELECT conversation_type FROM dm_conversations WHERE id = $1")
            .bind(conversation_id)
            .fetch_optional(conn)
            .await?;
    Ok(kind.unwrap_or_else(|| "unknown".to_string()))
}

fn summary(report: &ReportRow, conversation_type: String) -> ReportSummary {
    ReportSummary {
        id: report.id.clone(),
        status: report.status.clone(),
        category: report.category.clone(),
        source: report.source.clone(),
        has_message: report.message_id.is_some(),
        conversation_type,
        action: report.action.clone(),
        created_at: iso(report.created_at),
        resolved_at: iso_opt(report.resolved_at),
    }
}

async fn require_report(
    conn: &mut PgConnection,
    report_id: &str,
    lock: bool,
) -> ApiResult<ReportRow> {
    sqlx::query_as::<_, ReportRow>(&format!(
        "SELECT {REPORT_COLUMNS} FROM dm_abuse_reports WHERE id = $1{}",
        if lock { " FOR UPDATE" } else { "" }
    ))
    .bind(report_id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| ApiError::not_found("dm_safety.report_not_found", "Report not found."))
}

/// `ensure_reviewer_is_not_party`.
fn ensure_not_party(report: &ReportRow, reviewer: &str) -> ApiResult<()> {
    if reviewer == report.reporter_user_id || reviewer == report.reported_user_id {
        return Err(ApiError::forbidden(
            "dm_safety.review_conflict",
            "You cannot review a report you are part of.",
        ));
    }
    Ok(())
}

async fn review_audit(
    conn: &mut PgConnection,
    reviewer: &str,
    action: &str,
    summary: &str,
    report_id: Option<&str>,
    payload: Value,
) -> ApiResult<()> {
    audit::record(
        conn,
        action,
        "dm_abuse_report",
        report_id,
        summary,
        reviewer,
        payload,
    )
    .await
}

#[derive(Debug, Deserialize)]
pub struct ReportListQuery {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    before: Option<String>,
    #[serde(default)]
    limit: Option<String>,
}

/// `GET /v1/dm-safety/reports`.
pub async fn list_reports(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Query(query): Query<ReportListQuery>,
) -> ApiResult<Json<Value>> {
    let me = identity.user_id.as_str();
    let status = query.status.unwrap_or_else(|| "open".to_string());
    if !matches!(status.as_str(), "open" | "actioned" | "dismissed") {
        return Err(ApiError::validation(
            "status: Input should be 'open', 'actioned' or 'dismissed'",
        ));
    }
    let before = match query.before.as_deref() {
        None => None,
        Some(value) => {
            Some(parse_query_datetime(value).ok_or_else(|| ApiError::validation("before"))?)
        }
    };
    let limit = query_limit(query.limit.as_deref(), 50)?;
    let mut tx = pool.begin().await?;
    require_manager(&mut tx, me).await?;
    let mut rows: Vec<ReportRow> = sqlx::query_as(&format!(
        "SELECT {REPORT_COLUMNS} FROM dm_abuse_reports WHERE status = $1 \
           AND ($2::timestamp IS NULL OR created_at < $2) ORDER BY created_at DESC, id DESC LIMIT $3"
    ))
    .bind(&status)
    .bind(before)
    .bind(i64::from(limit) + 1)
    .fetch_all(&mut *tx)
    .await?;
    let has_more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let mut items = Vec::with_capacity(rows.len());
    for report in &rows {
        let kind = conversation_type(&mut tx, &report.conversation_id).await?;
        items.push(summary(report, kind));
    }
    review_audit(
        &mut tx,
        me,
        "dm_safety.reports_listed",
        "DM 신고 목록을 열었다",
        None,
        json!({"status": status, "count": rows.len()}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"items": items, "has_more": has_more})))
}

async fn detail(pool: &PgPool, identity: &Identity, report: &ReportRow) -> ApiResult<ReportDetail> {
    let mut conn = pool.acquire().await?;
    let kind = conversation_type(&mut conn, &report.conversation_id).await?;
    let (message_hidden, message_deleted) = match &report.message_id {
        None => (None, None),
        Some(message_id) => {
            let message =
                store::message_by_id(&mut conn, &report.conversation_id, message_id, false).await?;
            (
                Some(message.as_ref().is_some_and(|m| m.hidden())),
                Some(message.as_ref().is_some_and(|m| m.deleted())),
            )
        }
    };
    drop(conn);
    let mut ids = vec![
        report.reporter_user_id.clone(),
        report.reported_user_id.clone(),
    ];
    ids.extend(report.resolved_by_id.clone());
    let names = users::names(pool, identity, ids).await;
    Ok(ReportDetail {
        summary: summary(report, kind),
        note: report.note.clone(),
        conversation_id: report.conversation_id.clone(),
        message_id: report.message_id.clone(),
        reporter: Some(names.item(&report.reporter_user_id)),
        reported_user: Some(names.item(&report.reported_user_id)),
        resolution_note: report.resolution_note.clone(),
        resolved_by: report.resolved_by_id.as_deref().map(|id| names.item(id)),
        message_hidden,
        message_deleted,
    })
}

/// `GET /v1/dm-safety/reports/{id}`.
pub async fn get_report(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(report_id): Path<String>,
) -> ApiResult<Json<ReportDetail>> {
    route_id(&report_id)?;
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    require_manager(&mut tx, me).await?;
    let report = require_report(&mut tx, &report_id, false).await?;
    ensure_not_party(&report, me)?;
    review_audit(
        &mut tx,
        me,
        "dm_safety.report_viewed",
        "DM 신고를 열었다",
        Some(&report.id),
        json!({}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(detail(&pool, &identity, &report).await?))
}

#[derive(Debug, Serialize)]
pub struct EvidenceAttachment {
    id: String,
    filename: String,
    content_type: String,
    size_bytes: i64,
    is_image: bool,
}

#[derive(Debug, Serialize)]
pub struct EvidenceRevision {
    action: String,
    body_before: Option<String>,
    created_at: String,
}

#[derive(Debug, Serialize)]
pub struct EvidenceMessage {
    id: String,
    conversation_id: String,
    sequence: i32,
    sender: Option<DmUserItem>,
    body: String,
    created_at: String,
    hidden: bool,
    hidden_at: Option<String>,
    attachments: Vec<EvidenceAttachment>,
    /// miy: edits and deletion (with the texts kept while a report is open).
    edited_at: Option<String>,
    deleted_at: Option<String>,
    revisions: Vec<EvidenceRevision>,
}

/// `GET /v1/dm-safety/reports/{id}/evidence`: the reported message only.
pub async fn get_evidence(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(report_id): Path<String>,
) -> ApiResult<Json<Value>> {
    route_id(&report_id)?;
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    require_manager(&mut tx, me).await?;
    let report = require_report(&mut tx, &report_id, false).await?;
    ensure_not_party(&report, me)?;
    let message = match &report.message_id {
        Some(id) => store::message_by_id(&mut tx, &report.conversation_id, id, false).await?,
        None => None,
    };
    let mut item = None;
    if let Some(message) = &message {
        let attachments =
            store::attachments_of_messages(&mut tx, std::slice::from_ref(&message.id)).await?;
        let revisions: Vec<(String, Option<String>, NaiveDateTime)> = sqlx::query_as(
            "SELECT action, body_before, created_at FROM dm_message_revisions WHERE message_id = $1 ORDER BY id",
        )
        .bind(&message.id)
        .fetch_all(&mut *tx)
        .await?;
        let names = users::names(&pool, &identity, [message.sender_id.clone()]).await;
        // A message deleted while this report was open shows its kept copy.
        let body = if message.deleted() {
            revisions
                .iter()
                .rev()
                .find(|(action, _, _)| action == "delete")
                .and_then(|(_, body, _)| body.clone())
                .unwrap_or_default()
        } else {
            message.body.clone()
        };
        item = Some(EvidenceMessage {
            id: message.id.clone(),
            conversation_id: message.conversation_id.clone(),
            sequence: message.sequence,
            sender: Some(names.item(&message.sender_id)),
            body,
            created_at: iso(message.created_at),
            hidden: message.hidden(),
            hidden_at: iso_opt(message.moderation_hidden_at),
            attachments: attachments
                .iter()
                .filter(|a| matches!(a.storage_status.as_str(), "uploaded" | "attached"))
                .map(|a| EvidenceAttachment {
                    id: a.id.clone(),
                    filename: a.filename.clone(),
                    content_type: a.content_type.clone(),
                    size_bytes: i64::from(a.size_bytes),
                    is_image: attachments::is_previewable_image_content_type(&a.content_type),
                })
                .collect(),
            edited_at: iso_opt(message.edited_at),
            deleted_at: iso_opt(message.deleted_at),
            revisions: revisions
                .into_iter()
                .map(|(action, body_before, created_at)| EvidenceRevision {
                    action,
                    body_before,
                    created_at: iso(created_at),
                })
                .collect(),
        });
    }
    review_audit(
        &mut tx,
        me,
        "dm_safety.evidence_viewed",
        "DM 신고 증거(신고한 메시지)를 열었다",
        Some(&report.id),
        json!({"message_id": report.message_id}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"report_id": report.id, "message": item})))
}

/// `GET /files/dm-safety/reports/{id}/evidence/attachments/{attachment_id}/content`.
pub async fn evidence_attachment_content(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path((report_id, attachment_id)): Path<(String, String)>,
) -> ApiResult<Response> {
    route_id(&report_id)?;
    route_id(&attachment_id)?;
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    require_manager(&mut tx, me).await?;
    let report = require_report(&mut tx, &report_id, false).await?;
    ensure_not_party(&report, me)?;
    let attachment = store::attachment_by_id(&mut tx, &attachment_id, false)
        .await?
        .filter(|a| {
            report.message_id.is_some()
                && a.message_id == report.message_id
                && a.conversation_id == report.conversation_id
                && matches!(a.storage_status.as_str(), "uploaded" | "attached")
        })
        .ok_or_else(ApiError::attachment_not_found)?;
    review_audit(
        &mut tx,
        me,
        "dm_safety.evidence_attachment_opened",
        "DM 신고 증거 첨부를 열었다",
        Some(&report.id),
        json!({"attachment_id": attachment.id, "size_bytes": attachment.size_bytes}),
    )
    .await?;
    tx.commit().await?;
    attachments::stream_file(&identity, &attachment).await
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolveRequest {
    expected_status: String,
    outcome: String,
    action: String,
    #[serde(default)]
    resolution_note: Option<String>,
}

async fn set_hidden(
    conn: &mut PgConnection,
    report: &ReportRow,
    reviewer: &str,
    hidden: bool,
) -> ApiResult<Option<String>> {
    let Some(message_id) = &report.message_id else {
        return Err(ApiError::unprocessable(
            "dm_safety.report_target_invalid",
            "This report target is not valid.",
        ));
    };
    let message = store::message_by_id(conn, &report.conversation_id, message_id, true)
        .await?
        .ok_or_else(ApiError::message_not_found)?;
    if message.hidden() == hidden {
        return Ok(None);
    }
    let now = now_utc();
    sqlx::query("UPDATE dm_messages SET moderation_hidden_at = $2, moderation_hidden_by_id = $3 WHERE id = $1")
        .bind(message_id)
        .bind(hidden.then_some(now))
        .bind(hidden.then_some(reviewer))
        .execute(&mut *conn)
        .await?;
    review_audit(
        conn,
        reviewer,
        if hidden {
            "dm_safety.message_hidden"
        } else {
            "dm_safety.message_restored"
        },
        if hidden {
            "DM 메시지를 가렸다"
        } else {
            "DM 메시지 가림을 되돌렸다"
        },
        Some(&report.id),
        json!({"message_id": message_id, "conversation_id": report.conversation_id}),
    )
    .await?;
    Ok(Some(message_id.clone()))
}

/// Recipients' notification lines follow a moderation change.
async fn after_visibility_change(
    pool: &PgPool,
    identity: &Identity,
    report: &ReportRow,
    message_id: &str,
) {
    let loaded = async {
        let mut conn = pool.acquire().await?;
        let conversation: Option<Conversation> =
            store::load_conversation(&mut conn, &report.conversation_id, false).await?;
        let message =
            store::message_by_id(&mut conn, &report.conversation_id, message_id, false).await?;
        Ok::<_, ApiError>((conversation, message))
    }
    .await;
    live::conversation_changed(pool, &report.conversation_id, &[]).await;
    if let Ok((Some(conversation), Some(message))) = loaded {
        notify::message_changed(pool, identity, &conversation, &message).await;
        // A hidden message keeps no mention lines (they are not published again).
        if message.hidden() {
            notify::mentions_removed(identity, &report.conversation_id, message_id, None).await;
        }
    }
}

/// `POST /v1/dm-safety/reports/{id}/resolve`.
pub async fn resolve_report(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(report_id): Path<String>,
    ApiJson(request): ApiJson<ResolveRequest>,
) -> ApiResult<Json<ReportDetail>> {
    route_id(&report_id)?;
    if request.expected_status != "open"
        || !matches!(request.outcome.as_str(), "actioned" | "dismissed")
        || !matches!(request.action.as_str(), "none" | "hide_message")
    {
        return Err(ApiError::validation("Invalid resolve request"));
    }
    if request.outcome == "dismissed" && request.action != "none" {
        return Err(ApiError::validation("dismissed reports take action 'none'"));
    }
    let note = request
        .resolution_note
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    if let Some(note) = &note {
        crate::error::check_length("resolution_note", note, 0, RESOLUTION_NOTE_MAX_LENGTH)?;
    }
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    require_manager(&mut tx, me).await?;
    let report = require_report(&mut tx, &report_id, true).await?;
    ensure_not_party(&report, me)?;
    if report.status != request.expected_status {
        return Err(ApiError::conflict(
            "dm_safety.report_state_conflict",
            "This report was already handled.",
        ));
    }
    let changed = if request.action == "hide_message" {
        set_hidden(&mut tx, &report, me, true).await?
    } else {
        None
    };
    let now = now_utc();
    sqlx::query(
        "UPDATE dm_abuse_reports SET status = $2, action = $3, resolution_note = $4, resolved_by_id = $5, \
         resolved_at = $6, open_dedupe_key = NULL, updated_at = $6 WHERE id = $1",
    )
    .bind(&report.id)
    .bind(&request.outcome)
    .bind(&request.action)
    .bind(&note)
    .bind(me)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    review_audit(
        &mut tx,
        me,
        "dm_safety.report_resolved",
        "DM 신고를 처리했다",
        Some(&report.id),
        json!({"outcome": request.outcome, "action": request.action, "has_resolution_note": note.is_some()}),
    )
    .await?;
    // miy: a message deleted while reported loses its kept copy once no
    // report on it is open; its files are released after commit.
    let purge = match &report.message_id {
        Some(message_id) => messages::purge_deleted_copy(&mut tx, message_id).await?,
        None => false,
    };
    tx.commit().await?;
    if let Some(message_id) = &changed {
        after_visibility_change(&pool, &identity, &report, message_id).await;
    }
    if purge && let Some(message_id) = &report.message_id {
        attachments::release_deleted_message_files(&pool, &identity, message_id).await;
    }
    attachments::release_pending_files(&pool, &identity).await;
    notify::retract(&identity, &notify::report_group_key(&report.id), None).await;
    let mut conn = pool.acquire().await?;
    let report = require_report(&mut conn, &report_id, false).await?;
    drop(conn);
    Ok(Json(detail(&pool, &identity, &report).await?))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisibilityRequest {
    hidden: bool,
}

/// `POST /v1/dm-safety/reports/{id}/message-visibility`.
pub async fn set_message_visibility(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(report_id): Path<String>,
    ApiJson(request): ApiJson<VisibilityRequest>,
) -> ApiResult<Json<Value>> {
    route_id(&report_id)?;
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    require_manager(&mut tx, me).await?;
    let report = require_report(&mut tx, &report_id, false).await?;
    ensure_not_party(&report, me)?;
    let changed = set_hidden(&mut tx, &report, me, request.hidden).await?;
    tx.commit().await?;
    if let Some(message_id) = &changed {
        after_visibility_change(&pool, &identity, &report, message_id).await;
    }
    Ok(Json(json!({
        "report_id": report.id,
        "message_id": report.message_id,
        "hidden": request.hidden,
        "changed": changed.is_some(),
    })))
}

// --- banned words -------------------------------------------------------------------

/// `GET /v1/dm-safety/filter-terms`.
pub async fn list_filter_terms(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<Value>> {
    let mut conn = pool.acquire().await?;
    require_manager(&mut conn, &identity.user_id).await?;
    let terms = ugc::list_terms(&mut conn).await?;
    Ok(Json(json!({
        "items": terms.iter().map(|t| json!({"id": t.id, "term": t.term, "created_at": iso(t.created_at)})).collect::<Vec<_>>(),
        "limit": ugc::FILTER_TERMS_MAX,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilterTermRequest {
    term: String,
}

/// `POST /v1/dm-safety/filter-terms`.
pub async fn add_filter_term(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    ApiJson(request): ApiJson<FilterTermRequest>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    crate::error::check_length("term", &request.term, 1, ugc::FILTER_TERM_MAX_LENGTH)?;
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    require_manager(&mut tx, me).await?;
    let term = ugc::add_term(&mut tx, me, &request.term).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"id": term.id, "term": term.term, "created_at": iso(term.created_at)})),
    ))
}

/// `DELETE /v1/dm-safety/filter-terms/{id}`.
pub async fn remove_filter_term(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(term_id): Path<String>,
) -> ApiResult<StatusCode> {
    route_id(&term_id)?;
    let me = identity.user_id.as_str();
    let mut tx = pool.begin().await?;
    require_manager(&mut tx, me).await?;
    ugc::remove_term(&mut tx, me, &term_id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
