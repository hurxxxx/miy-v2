//! PMS associations keep only resource IDs. The target app checks the current
//! caller for every lookup, so a saved link itself grants no access.
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{Method, StatusCode},
};
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::{
    access,
    error::{ApiError, ApiResult, ErrorResponse},
};

const MAX_LINKS: i64 = 12;

#[derive(Clone, Copy, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkScope {
    Task,
    Space,
    List,
}

impl LinkScope {
    fn table(self) -> (&'static str, &'static str) {
        match self {
            Self::Task => ("pms_task_doc_refs", "task_id"),
            Self::Space => ("pms_space_whiteboard_refs", "space_id"),
            Self::List => ("pms_list_whiteboard_refs", "list_id"),
        }
    }

    fn target(self, id: Uuid) -> (&'static str, String) {
        match self {
            Self::Task => ("docs", format!("/v1/docs/items/{id}/reference")),
            Self::Space | Self::List => {
                ("whiteboard", format!("/v1/whiteboard/items/{id}/reference"))
            }
        }
    }

    async fn require_read(self, conn: &mut PgConnection, user: &str, owner: &str) -> ApiResult<()> {
        match self {
            Self::Task => {
                if access::ensure_task_readable(conn, user, owner)
                    .await?
                    .archived
                {
                    return Err(ApiError::not_found("pms.task_not_found"));
                }
            }
            Self::Space => {
                access::ensure_space_access(conn, user, owner).await?;
            }
            Self::List => {
                access::ensure_list_member(conn, user, owner).await?;
            }
        }
        Ok(())
    }

    async fn lock_for_write(
        self,
        conn: &mut PgConnection,
        user: &str,
        owner: &str,
    ) -> ApiResult<()> {
        match self {
            Self::Task => {
                let task = access::load_task(conn, owner).await?;
                access::lock_list_for_write(conn, &task.list_id).await?;
                if access::task_for_editor(conn, user, owner).await?.0.archived {
                    return Err(ApiError::not_found("pms.task_not_found"));
                }
            }
            Self::Space => {
                let exists = sqlx::query_scalar::<_, String>(
                    "SELECT id FROM pms_spaces WHERE id=$1 AND active FOR UPDATE",
                )
                .bind(owner)
                .fetch_optional(&mut *conn)
                .await?;
                if exists.is_none() {
                    return Err(ApiError::not_found("pms.space_not_found"));
                }
                access::ensure_space_editor(conn, user, owner).await?;
            }
            Self::List => {
                access::lock_list_for_write(conn, owner).await?;
                access::ensure_list_editor(conn, user, owner).await?;
            }
        }
        Ok(())
    }

    async fn associations(
        self,
        conn: &mut PgConnection,
        owner: &str,
    ) -> ApiResult<Vec<Association>> {
        let (table, column) = self.table();
        Ok(sqlx::query_as(&format!(
            "SELECT id, resource_id FROM {table} WHERE {column}=$1"
        ))
        .bind(owner)
        .fetch_all(conn)
        .await?)
    }

    async fn can_edit(self, conn: &mut PgConnection, user: &str, owner: &str) -> ApiResult<bool> {
        let result = match self {
            Self::Task => {
                let task = access::load_task(conn, owner).await?;
                if task.archived {
                    return Ok(false);
                }
                access::ensure_list_editor(conn, user, &task.list_id)
                    .await
                    .map(|_| ())
            }
            Self::Space => access::ensure_space_editor(conn, user, owner)
                .await
                .map(|_| ()),
            Self::List => access::ensure_list_editor(conn, user, owner)
                .await
                .map(|_| ()),
        };
        match result {
            Ok(()) => Ok(true),
            Err(error)
                if matches!(
                    error.status,
                    StatusCode::FORBIDDEN | StatusCode::NOT_FOUND | StatusCode::CONFLICT
                ) =>
            {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    async fn resolve(self, identity: &Identity, resource_id: Uuid) -> ApiResult<Reference> {
        let (app, path) = self.target(resource_id);
        let value = identity
            .app_api(app, &path, Method::GET, None)
            .await
            .map_err(|error| {
                if error.code != "installation_permission_denied"
                    && matches!(error.status, Some(403 | 404 | 410))
                {
                    ApiError::not_found("pms.linked_resource_not_found")
                } else {
                    unavailable()
                }
            })?;
        let item: Reference = serde_json::from_value(value).map_err(|_| unavailable())?;
        if item.id != resource_id {
            return Err(unavailable());
        }
        Ok(item)
    }
}

fn unavailable() -> ApiError {
    ApiError::new(
        StatusCode::FAILED_DEPENDENCY,
        "pms.linked_resource_unavailable",
        "Could not read the linked app.",
    )
}

#[derive(Deserialize, Serialize, utoipa::ToSchema)]
pub struct Reference {
    #[schema(value_type = String)]
    id: Uuid,
    title: String,
    space_id: String,
}

#[derive(sqlx::FromRow)]
struct Association {
    id: Uuid,
    resource_id: Uuid,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct LinkedReference {
    #[schema(value_type = String)]
    link_id: Uuid,
    #[serde(flatten)]
    resource: Reference,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct References {
    items: Vec<LinkedReference>,
    /// Opaque IDs for unavailable links, only returned to PMS editors.
    #[schema(value_type = Vec<String>)]
    unavailable_links: Vec<Uuid>,
}

#[utoipa::path(
    get, path = "/v1/pms/resource-links/{scope}/{owner_id}", operation_id = "listPmsResourceLinks",
    tag = "Linked resources",
    params(("scope" = LinkScope, Path), ("owner_id" = String, Path)),
    responses((status = 200, body = References), (status = 403, body = ErrorResponse),
              (status = 404, body = ErrorResponse), (status = 424, body = ErrorResponse)),
)]
pub async fn list(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path((scope, owner)): Path<(LinkScope, String)>,
) -> ApiResult<Json<References>> {
    let associations = {
        let mut conn = pool.acquire().await?;
        scope
            .require_read(&mut conn, &identity.user_id, &owner)
            .await?;
        scope.associations(&mut conn, &owner).await?
    };
    let mut reads = tokio::task::JoinSet::new();
    for link in associations {
        let identity = identity.clone();
        reads.spawn(async move { (link.id, scope.resolve(&identity, link.resource_id).await) });
    }
    let mut items = Vec::new();
    let mut hidden = Vec::new();
    while let Some(result) = reads.join_next().await {
        let (link_id, resource) = result.map_err(|_| unavailable())?;
        match resource {
            Ok(resource) => items.push(LinkedReference { link_id, resource }),
            Err(error) if error.status == StatusCode::NOT_FOUND => hidden.push(link_id),
            Err(error) => return Err(error),
        }
    }
    if items.is_empty() && hidden.is_empty() {
        return Ok(Json(References {
            items,
            unavailable_links: Vec::new(),
        }));
    }
    // A task, membership or link could have changed while the remote reads ran.
    let mut conn = pool.acquire().await?;
    scope
        .require_read(&mut conn, &identity.user_id, &owner)
        .await?;
    let current = scope.associations(&mut conn, &owner).await?;
    items.retain(|item| current.iter().any(|link| link.id == item.link_id));
    items.sort_by(|a, b| {
        a.resource
            .title
            .cmp(&b.resource.title)
            .then(a.resource.id.cmp(&b.resource.id))
    });
    let mut unavailable_links = Vec::new();
    if !hidden.is_empty() && scope.can_edit(&mut conn, &identity.user_id, &owner).await? {
        hidden.sort_unstable();
        unavailable_links.extend(
            hidden
                .into_iter()
                .filter(|id| current.iter().any(|link| link.id == *id)),
        );
    }
    Ok(Json(References {
        items,
        unavailable_links,
    }))
}

#[utoipa::path(
    put, path = "/v1/pms/resource-links/{scope}/{owner_id}/{resource_id}", operation_id = "attachPmsResourceLink",
    tag = "Linked resources",
    params(("scope" = LinkScope, Path), ("owner_id" = String, Path), ("resource_id" = String, Path, description = "Application resource UUID")),
    responses((status = 200, body = Reference), (status = 403, body = ErrorResponse),
              (status = 404, body = ErrorResponse), (status = 409, body = ErrorResponse),
              (status = 424, body = ErrorResponse)),
)]
pub async fn attach(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path((scope, owner, resource_id)): Path<(LinkScope, String, Uuid)>,
) -> ApiResult<Json<Reference>> {
    // Check the owner first and release locks before making a network request.
    {
        let mut tx = pool.begin().await?;
        scope
            .lock_for_write(&mut tx, &identity.user_id, &owner)
            .await?;
        tx.commit().await?;
    }
    let resource = scope.resolve(&identity, resource_id).await?;
    let mut tx = pool.begin().await?;
    scope
        .lock_for_write(&mut tx, &identity.user_id, &owner)
        .await?;
    if matches!(scope, LinkScope::List) {
        sqlx::query(
            "INSERT INTO pms_list_whiteboard_refs (list_id, resource_id) VALUES ($1,$2) \
             ON CONFLICT (list_id) DO UPDATE SET id=EXCLUDED.id, resource_id=EXCLUDED.resource_id \
             WHERE pms_list_whiteboard_refs.resource_id <> EXCLUDED.resource_id",
        )
        .bind(owner)
        .bind(resource_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(Json(resource));
    }
    let (table, column) = scope.table();
    let (count, already_linked): (i64, bool) = sqlx::query_as(&format!(
        "SELECT count(*), coalesce(bool_or(resource_id=$2),false) FROM {table} WHERE {column}=$1",
    ))
    .bind(&owner)
    .bind(resource_id)
    .fetch_one(&mut *tx)
    .await?;
    if count >= MAX_LINKS && !already_linked {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "pms.linked_resource_limit",
            "This item has reached its linked resource limit.",
        ));
    }
    sqlx::query(&format!(
        "INSERT INTO {table} ({column}, resource_id) VALUES ($1,$2) ON CONFLICT DO NOTHING"
    ))
    .bind(owner)
    .bind(resource_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(resource))
}

#[utoipa::path(
    delete, path = "/v1/pms/resource-links/{scope}/{owner_id}/links/{link_id}", operation_id = "detachPmsResourceLink",
    tag = "Linked resources",
    params(("scope" = LinkScope, Path), ("owner_id" = String, Path), ("link_id" = String, Path, description = "PMS association UUID")),
    responses((status = 204), (status = 403, body = ErrorResponse), (status = 404, body = ErrorResponse)),
)]
pub async fn detach(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path((scope, owner, link_id)): Path<(LinkScope, String, Uuid)>,
) -> ApiResult<StatusCode> {
    let mut tx = pool.begin().await?;
    scope
        .lock_for_write(&mut tx, &identity.user_id, &owner)
        .await?;
    let (table, column) = scope.table();
    sqlx::query(&format!("DELETE FROM {table} WHERE {column}=$1 AND id=$2"))
        .bind(owner)
        .bind(link_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
