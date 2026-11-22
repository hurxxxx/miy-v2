//! Page version history (보강, not in AI-DO 0002af062).
//!
//! Every saved body revision keeps a version row (the content as saved, the
//! page title at that moment, who saved it and when). Rows are pruned in the
//! same transaction to the latest `KEEP_RECENT` revisions plus the last
//! revision of each older UTC day. Restoring a version writes its content as
//! a new revision (and a new version row that names the restored revision);
//! history is never rewritten.
use axum::{
    Extension, Json,
    extract::{Path, State},
};
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{FromRow, Postgres, Transaction};

use crate::{
    AppState,
    access::{self, NativePage, Require, Viewer},
    error::{ApiError, ApiJson, ApiResult},
    pages, people,
    time::{naive_iso, now_utc},
};

/// The latest revisions always kept per page.
pub const KEEP_RECENT: i64 = 50;

/// Record the page's current content as a version, then prune.
pub async fn record(
    tx: &mut Transaction<'_, Postgres>,
    page: &NativePage,
    user_id: &str,
    restored_from: Option<i64>,
) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO docs_page_versions (id, page_id, revision, title, content_blocks, created_by_id, \
         created_at, restored_from_revision) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) \
         ON CONFLICT (page_id, revision) DO NOTHING",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&page.id)
    .bind(page.revision)
    .bind(&page.title)
    .bind(&page.content_blocks)
    .bind(user_id)
    .bind(now_utc())
    .bind(restored_from)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "DELETE FROM docs_page_versions WHERE id IN (\
           SELECT id FROM (\
             SELECT id, row_number() OVER (ORDER BY revision DESC) AS recent_rank, \
                    row_number() OVER (PARTITION BY (created_at::date) ORDER BY revision DESC) AS day_rank \
             FROM docs_page_versions WHERE page_id = $1) ranked \
           WHERE recent_rank > $2 AND day_rank > 1)",
    )
    .bind(&page.id)
    .bind(KEEP_RECENT)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[derive(Clone, Debug, FromRow)]
pub struct Version {
    pub id: String,
    pub page_id: String,
    pub revision: i64,
    pub title: String,
    pub content_blocks: Option<Value>,
    pub created_by_id: String,
    pub created_at: NaiveDateTime,
    pub restored_from_revision: Option<i64>,
}

const COLUMNS: &str = "id, page_id, revision, title, content_blocks, created_by_id, created_at, \
     restored_from_revision";

fn version_item(version: &Version, name: &str, current: i64, with_content: bool) -> Value {
    let mut item = json!({
        "id": version.id,
        "page_id": version.page_id,
        "revision": version.revision,
        "title": version.title,
        "created_by_id": version.created_by_id,
        "created_by_name": name,
        "created_at": naive_iso(version.created_at),
        "restored_from_revision": version.restored_from_revision,
        "is_current": version.revision == current,
    });
    if with_content {
        item["content_blocks"] = version.content_blocks.clone().unwrap_or(json!([]));
    }
    item
}

/// `GET /v1/docs/pages/{page_id}/versions`: newest first, without content.
pub async fn list(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(page_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let mut viewer = Viewer::new(&state, &identity);
    let context = access::page_or_403(&mut viewer, &page_id, Require::View).await?;
    let sql = format!(
        "SELECT {COLUMNS} FROM docs_page_versions WHERE page_id = $1 ORDER BY revision DESC"
    );
    let rows = sqlx::query_as::<_, Version>(&sql)
        .bind(&context.page.id)
        .fetch_all(&state.pool)
        .await?;
    let names =
        people::display_names(&identity, rows.iter().map(|row| row.created_by_id.clone())).await;
    Ok(Json(json!({
        "items": rows.iter().map(|row| version_item(
            row,
            names.get(&row.created_by_id).map(String::as_str).unwrap_or(""),
            context.page.revision,
            false,
        )).collect::<Vec<_>>(),
        "current_revision": context.page.revision,
        "can_restore": context.access.can_edit,
    })))
}

fn version_not_found() -> ApiError {
    ApiError::not_found("docs.version_not_found", "Version not found.")
}

/// `GET /v1/docs/pages/{page_id}/versions/{version_id}`: one version with its
/// content, for a read-only preview.
pub async fn get(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((page_id, version_id)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let mut viewer = Viewer::new(&state, &identity);
    let context = access::page_or_403(&mut viewer, &page_id, Require::View).await?;
    let sql = format!("SELECT {COLUMNS} FROM docs_page_versions WHERE id = $1 AND page_id = $2");
    let version = sqlx::query_as::<_, Version>(&sql)
        .bind(&version_id)
        .bind(&context.page.id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(version_not_found)?;
    let names = people::display_names(&identity, [version.created_by_id.clone()]).await;
    Ok(Json(version_item(
        &version,
        names
            .get(&version.created_by_id)
            .map(String::as_str)
            .unwrap_or(""),
        context.page.revision,
        true,
    )))
}

#[derive(Deserialize)]
pub struct RestoreRequest {
    expected_revision: i64,
}

/// `POST /v1/docs/pages/{page_id}/versions/{version_id}/restore`: the
/// version's content becomes a new revision of the page. The caller states
/// the revision they are looking at; a newer one answers 409 like a save.
pub async fn restore(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path((page_id, version_id)): Path<(String, String)>,
    ApiJson(payload): ApiJson<RestoreRequest>,
) -> ApiResult<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    let (context, page) = pages::lock_page_for_edit(&mut tx, &state, &identity, &page_id).await?;
    let sql = format!("SELECT {COLUMNS} FROM docs_page_versions WHERE id = $1 AND page_id = $2");
    let version = sqlx::query_as::<_, Version>(&sql)
        .bind(&version_id)
        .bind(&page.id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(version_not_found)?;
    if payload.expected_revision != page.revision {
        return Err(pages::revision_conflict());
    }
    let blocks = version.content_blocks.clone().or_else(|| Some(json!([])));
    let (updated, linked) = pages::write_content(
        &mut tx,
        &context.doc,
        &page,
        &blocks,
        &identity.user_id,
        Some(version.revision),
    )
    .await?;
    tx.commit().await?;
    crate::media::attach_linked(&state.pool, &identity, &linked).await;
    Ok(Json(
        pages::page_json(&identity, &updated, context.access.can_edit).await,
    ))
}
