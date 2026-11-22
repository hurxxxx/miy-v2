//! Page templates of a space (보강, not in AI-DO 0002af062).
//!
//! AI-DO had no templates at the base commit: an earlier version showed three
//! cards (project overview, meeting notes, wiki) that only prefilled the new
//! doc's title, and commit 77a44b5cf removed them before 0002af062. So there
//! are no built-in templates.
//!
//! A space member (role member or admin) saves a page they can read as a
//! template of that page's space; everyone with a role in the space lists the
//! templates and starts pages and docs from them. The creator or a space
//! admin deletes a template. Images inside a template keep pointing at the
//! source page's images (visible to readers of that page only).
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::NaiveDateTime;
use miy_app_sdk::Identity;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{FromRow, Postgres, Transaction};

use crate::{
    AppState,
    access::{self, Require, Viewer},
    error::{ApiError, ApiJson, ApiResult, check_length},
    items::SpaceQuery,
    people,
    spaces::{self, SpaceRole},
    time::{naive_iso, now_utc},
};

#[derive(Clone, Debug, FromRow)]
pub struct Template {
    pub id: String,
    pub space_id: String,
    pub title: String,
    pub description: String,
    pub content_blocks: Value,
    pub source_page_id: Option<String>,
    pub created_by_id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

const COLUMNS: &str = "id, space_id, title, description, content_blocks, source_page_id, \
     created_by_id, created_at, updated_at";

fn template_not_found() -> ApiError {
    ApiError::not_found("docs.template_not_found", "Template not found.")
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct TemplateItem {
    id: String,
    space_id: String,
    title: String,
    description: String,
    #[schema(required)]
    source_page_id: Option<String>,
    created_by_id: String,
    created_by_name: String,
    created_at: String,
    updated_at: String,
    can_delete: bool,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct TemplateDetail {
    #[serde(flatten)]
    template: TemplateItem,
    #[schema(value_type = Vec<std::collections::BTreeMap<String, Value>>)]
    content_blocks: Value,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct TemplateList {
    items: Vec<TemplateItem>,
    can_create: bool,
}

fn item(template: &Template, name: &str, can_delete: bool) -> TemplateItem {
    TemplateItem {
        id: template.id.clone(),
        space_id: template.space_id.clone(),
        title: template.title.clone(),
        description: template.description.clone(),
        source_page_id: template.source_page_id.clone(),
        created_by_id: template.created_by_id.clone(),
        created_by_name: name.to_owned(),
        created_at: naive_iso(template.created_at),
        updated_at: naive_iso(template.updated_at),
        can_delete,
    }
}

/// The content of a template for a new page or doc in `space_id`, decided
/// under the role locks of the caller's write transaction.
pub async fn content_for(
    tx: &mut Transaction<'_, Postgres>,
    state: &AppState,
    identity: &Identity,
    template_id: &str,
    space_id: &str,
) -> ApiResult<Value> {
    let sql = format!("SELECT {COLUMNS} FROM docs_templates WHERE id = $1 FOR SHARE");
    let template = sqlx::query_as::<_, Template>(&sql)
        .bind(template_id)
        .fetch_optional(&mut **tx)
        .await?
        .filter(|template| template.space_id == space_id)
        .ok_or_else(template_not_found)?;
    let space = spaces::load_space(&state.pool, space_id)
        .await?
        .ok_or_else(spaces::space_not_found)?;
    if spaces::space_role_in(tx, state, identity, &space)
        .await?
        .is_none()
    {
        return Err(template_not_found());
    }
    Ok(template.content_blocks)
}

/// `GET /v1/docs/templates?space=`: the space's templates, by title.
#[utoipa::path(
    get, path = "/v1/docs/templates", operation_id = "listDocsTemplates", tag = "Templates",
    params(SpaceQuery),
    responses(
        (status = 200, description = "Success", body = TemplateList),
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
) -> ApiResult<Json<TemplateList>> {
    let (space, role) = spaces::require_space(&state, &identity, query.space()).await?;
    let sql = format!(
        "SELECT {COLUMNS} FROM docs_templates WHERE space_id = $1 ORDER BY lower(title), created_at"
    );
    let rows = sqlx::query_as::<_, Template>(&sql)
        .bind(&space.id)
        .fetch_all(&state.pool)
        .await?;
    let names =
        people::display_names(&identity, rows.iter().map(|row| row.created_by_id.clone())).await;
    Ok(Json(TemplateList {
        items: rows
            .iter()
            .map(|row| {
                item(
                    row,
                    names
                        .get(&row.created_by_id)
                        .map(String::as_str)
                        .unwrap_or(""),
                    row.created_by_id == identity.user_id || role == SpaceRole::Admin,
                )
            })
            .collect(),
        can_create: role >= SpaceRole::Member,
    }))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct TemplateCreateRequest {
    page_id: String,
    #[serde(default)]
    #[schema(min_length = 1, max_length = 200)]
    title: Option<String>,
    #[serde(default)]
    #[schema(max_length = 1000)]
    description: Option<String>,
}

/// `POST /v1/docs/templates`: save a page the caller can read as a template
/// of the page's space. A member role in that space is required.
#[utoipa::path(
    post, path = "/v1/docs/templates", operation_id = "createDocsTemplate", tag = "Templates",
    request_body = TemplateCreateRequest,
    responses(
        (status = 201, description = "Created", body = TemplateItem),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn create(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    ApiJson(payload): ApiJson<TemplateCreateRequest>,
) -> ApiResult<impl IntoResponse> {
    if let Some(title) = &payload.title {
        check_length("title", title, 1, 200)?;
    }
    let description = payload.description.clone().unwrap_or_default();
    check_length("description", &description, 0, 1000)?;
    let mut viewer = Viewer::new(&state, &identity);
    let context = access::page_or_403(&mut viewer, &payload.page_id, Require::View).await?;
    let doc = access::load_doc(&state.pool, &context.page.doc_id)
        .await?
        .ok_or_else(access::page_not_found)?;
    let space = spaces::load_space(&state.pool, &doc.workspace_id)
        .await?
        .ok_or_else(spaces::space_not_found)?;
    let title = payload
        .title
        .as_deref()
        .unwrap_or(&context.page.title)
        .trim()
        .to_string();
    if title.is_empty() {
        return Err(ApiError::validation(
            "title: Value error, must not be blank",
        ));
    }
    let mut tx = state.pool.begin().await?;
    let role = spaces::space_role_in(&mut tx, &state, &identity, &space).await?;
    if role.is_none_or(|role| role < SpaceRole::Member) {
        return Err(ApiError::forbidden(
            "docs.template_manage_access_required",
            "Template manage access required.",
        ));
    }
    // The page may have been saved or trashed since the access check: copy
    // the content under the doc lock.
    let locked = access::lock_doc_for_write(&mut tx, &state, &identity, &doc.id).await?;
    let content: Option<(Option<Value>, Option<NaiveDateTime>)> = sqlx::query_as(
        "SELECT content_blocks, trashed_at FROM docs_native_doc_pages WHERE id = $1 FOR SHARE",
    )
    .bind(&context.page.id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((blocks, None)) = content else {
        return Err(access::page_not_found());
    };
    if !locked.access.can_view || locked.doc.trashed_at.is_some() {
        return Err(access::page_not_found());
    }
    let now = now_utc();
    let sql = format!(
        "INSERT INTO docs_templates (id, space_id, title, description, content_blocks, source_page_id, \
         created_by_id, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$8) RETURNING {COLUMNS}"
    );
    let template = sqlx::query_as::<_, Template>(&sql)
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&space.id)
        .bind(&title)
        .bind(description.trim())
        .bind(blocks.unwrap_or(json!([])))
        .bind(&context.page.id)
        .bind(&identity.user_id)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    let names = people::display_names(&identity, [identity.user_id.clone()]).await;
    Ok((
        StatusCode::CREATED,
        Json(item(
            &template,
            names
                .get(&identity.user_id)
                .map(String::as_str)
                .unwrap_or(""),
            true,
        )),
    ))
}

/// `DELETE /v1/docs/templates/{id}`: the creator or a space admin.
#[utoipa::path(
    delete, path = "/v1/docs/templates/{template_id}", operation_id = "deleteDocsTemplate", tag = "Templates",
    params(("template_id" = String, Path)),
    responses(
        (status = 204, description = "Completed with no response body"),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Permission denied", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn delete(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(template_id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = state.pool.begin().await?;
    let sql = format!("SELECT {COLUMNS} FROM docs_templates WHERE id = $1 FOR UPDATE");
    let template = sqlx::query_as::<_, Template>(&sql)
        .bind(&template_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(template_not_found)?;
    let space = spaces::load_space(&state.pool, &template.space_id)
        .await?
        .ok_or_else(template_not_found)?;
    let role = spaces::space_role_in(&mut tx, &state, &identity, &space)
        .await?
        .ok_or_else(template_not_found)?;
    if template.created_by_id != identity.user_id && role != SpaceRole::Admin {
        return Err(ApiError::forbidden(
            "docs.template_manage_access_required",
            "Template manage access required.",
        ));
    }
    sqlx::query("DELETE FROM docs_templates WHERE id = $1")
        .bind(&template.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The template itself with its content (preview before starting a page).
#[utoipa::path(
    get, path = "/v1/docs/templates/{template_id}", operation_id = "getDocsTemplate", tag = "Templates",
    params(("template_id" = String, Path)),
    responses(
        (status = 200, description = "Success", body = TemplateDetail),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse),
        (status = 503, description = "A required platform service is unavailable", body = crate::error::ErrorResponse)
    )
)]
pub async fn get(
    State(state): State<AppState>,
    Extension(identity): Extension<Identity>,
    Path(template_id): Path<String>,
) -> ApiResult<Json<TemplateDetail>> {
    let sql = format!("SELECT {COLUMNS} FROM docs_templates WHERE id = $1");
    let template = sqlx::query_as::<_, Template>(&sql)
        .bind(&template_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(template_not_found)?;
    let space = spaces::load_space(&state.pool, &template.space_id)
        .await?
        .ok_or_else(template_not_found)?;
    let role = spaces::space_role(&state, &identity, &space)
        .await?
        .ok_or_else(template_not_found)?;
    let names = people::display_names(&identity, [template.created_by_id.clone()]).await;
    let value = item(
        &template,
        names
            .get(&template.created_by_id)
            .map(String::as_str)
            .unwrap_or(""),
        template.created_by_id == identity.user_id || role == SpaceRole::Admin,
    );
    Ok(Json(TemplateDetail {
        template: value,
        content_blocks: template.content_blocks,
    }))
}
