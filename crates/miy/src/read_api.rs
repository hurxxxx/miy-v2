//! Permission-filtered keyset pages. Cursors select a boundary, never authority.
use crate::{
    State,
    api_contract::{
        AdminDirectoryUser, AdminUserDirectoryPage, App, AppDetail, InstallationPage,
        RuntimeJobPage,
    },
    auth::{BrowserPurpose, Principal},
    domain,
    http::{ApiError, ApiResult},
    policy::{self, AppAction},
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State as S, rejection::QueryRejection},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;

const APP_SOURCE: &str =
    "apps t LEFT JOIN official_apps o ON o.app_id=t.id LEFT JOIN app_controls c ON c.app_id=t.id";
const APP_ROW: &str = "jsonb_build_object('id',t.id,'name',t.name,'disabled',COALESCE(c.disabled,false),'distribution',CASE o.default_install WHEN true THEN 'default_official' WHEN false THEN 'optional_official' ELSE 'organization' END)";

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Page {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
    pub app_id: Option<String>,
    pub environment: Option<String>,
    pub request_id: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Cursor {
    version: u8,
    scope: String,
    boundary: String,
}
impl Page {
    pub fn limit(&self) -> ApiResult<i64> {
        let value = self.limit.unwrap_or(50);
        if !(1..=200).contains(&value) {
            return Err(ApiError::bad("invalid_page_limit"));
        }
        if self
            .environment
            .as_ref()
            .is_some_and(|v| !["dev", "prod"].contains(&v.as_str()))
        {
            return Err(ApiError::bad("invalid_environment"));
        }
        if self
            .app_id
            .as_ref()
            .is_some_and(|v| !crate::contract::valid_id(v))
        {
            return Err(ApiError::bad("invalid_app_id"));
        }
        if self
            .request_id
            .as_ref()
            .is_some_and(|v| v.is_empty() || v.len() > 128)
        {
            return Err(ApiError::bad("invalid_request_id"));
        }
        Ok(value)
    }
    fn scope(&self, kind: &str, user: &str) -> String {
        crate::contract::hash(
            serde_json::to_vec(&json!([
                kind,
                user,
                self.app_id,
                self.environment,
                self.request_id
            ]))
            .unwrap(),
        )
    }
    pub(crate) fn boundary(&self, scope: &str) -> ApiResult<String> {
        let Some(raw) = &self.cursor else {
            return Ok(String::new());
        };
        if raw.len() > 4096 {
            return Err(ApiError::bad("invalid_cursor"));
        }
        let cursor: Cursor = URL_SAFE_NO_PAD
            .decode(raw)
            .ok()
            .and_then(|v| serde_json::from_slice(&v).ok())
            .ok_or_else(|| ApiError::bad("invalid_cursor"))?;
        if cursor.version != 1 || cursor.scope != scope || cursor.boundary.len() > 2048 {
            return Err(ApiError::bad("invalid_cursor"));
        }
        Ok(cursor.boundary)
    }
}
pub(crate) fn encode(scope: &str, boundary: String) -> String {
    URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&Cursor {
            version: 1,
            scope: scope.into(),
            boundary,
        })
        .unwrap(),
    )
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum UserRole {
    Admin,
    Developer,
    Member,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum UserStatus {
    Enabled,
    Disabled,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UserDirectoryQuery {
    q: Option<String>,
    role: Option<UserRole>,
    status: Option<UserStatus>,
    organization: Option<String>,
    #[serde(default)]
    descendants: bool,
    limit: Option<i64>,
    cursor: Option<String>,
}

pub(crate) async fn user_directory(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    query: Result<Query<UserDirectoryQuery>, QueryRejection>,
) -> ApiResult<Json<AdminUserDirectoryPage>> {
    domain::admin(&s, &p).await?;
    let Query(q) = query.map_err(|_| ApiError::bad("invalid_user_directory_query"))?;
    let page = Page {
        limit: q.limit,
        cursor: q.cursor,
        ..Page::default()
    };
    let limit = page.limit()?;
    let search = q.q.as_deref().unwrap_or_default().trim();
    if search.chars().count() > 120 || search.contains('\0') {
        return Err(ApiError::bad("invalid_user_search"));
    }
    let unassigned = q.organization.as_deref() == Some("unassigned");
    let organization = q
        .organization
        .as_deref()
        .filter(|_| !unassigned)
        .map(str::parse::<uuid::Uuid>)
        .transpose()
        .map_err(|_| ApiError::bad("invalid_organization"))?;
    let descendants = organization.is_some() && q.descendants;
    let scope = crate::contract::hash(serde_json::to_vec(&json!([
        "admin-user-directory",
        p.id,
        p.browser_purpose.map(BrowserPurpose::as_str),
        search,
        q.role,
        q.status,
        organization,
        unassigned,
        descendants,
    ]))?);
    let boundary = page.boundary(&scope)?;
    let after = if boundary.is_empty() {
        None
    } else {
        Some(
            serde_json::from_str::<(String, String)>(&boundary)
                .map_err(|_| ApiError::bad("invalid_cursor"))?,
        )
    };
    let (admins, developers) = match q.role {
        Some(UserRole::Admin) => (Some(true), None),
        Some(UserRole::Developer) => (None, Some(true)),
        Some(UserRole::Member) => (Some(false), Some(false)),
        None => (None, None),
    };
    let enabled = q.status.map(|status| matches!(status, UserStatus::Enabled));
    let mut rows = sqlx::query(
        "WITH RECURSIVE subtree(id) AS (
           SELECT id FROM groups WHERE id = $1 AND source = 'hr'
           UNION
           SELECT child.id FROM groups child JOIN subtree parent ON child.parent_id = parent.id
           WHERE $2 AND child.source = 'hr'
         )
         SELECT lower(u.name) AS sort_name, u.id, u.name, u.admin, u.can_develop, u.enabled,
           u.primary_group_id, g.name AS primary_group_name
         FROM users u LEFT JOIN groups g ON g.id = u.primary_group_id
         WHERE ($1 IS NULL OR u.primary_group_id IN (SELECT id FROM subtree))
           AND (NOT $3 OR u.primary_group_id IS NULL)
           AND ($4 = '' OR strpos(lower(u.id), lower($4)) > 0 OR strpos(lower(u.name), lower($4)) > 0)
           AND ($5::boolean IS NULL OR u.admin = $5)
           AND ($6::boolean IS NULL OR u.can_develop = $6)
           AND ($7::boolean IS NULL OR u.enabled = $7)
           AND ($8::text IS NULL OR (lower(u.name), u.id) > ($8, $9::text))
         ORDER BY lower(u.name), u.id LIMIT $10",
    )
    .bind(organization)
    .bind(descendants)
    .bind(unassigned)
    .bind(search)
    .bind(admins)
    .bind(developers)
    .bind(enabled)
    .bind(after.as_ref().map(|(name, _)| name))
    .bind(after.as_ref().map(|(_, id)| id))
    .bind(limit + 1)
    .fetch_all(&s.pool)
    .await?;
    let mut next_cursor = None;
    if rows.len() > limit as usize {
        rows.pop();
        if let Some(last) = rows.last() {
            let key: (&str, &str) = (last.try_get("sort_name")?, last.try_get("id")?);
            next_cursor = Some(encode(&scope, serde_json::to_string(&key)?));
        }
    }
    let items = rows
        .into_iter()
        .map(|row| {
            Ok(AdminDirectoryUser {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                admin: row.try_get("admin")?,
                can_develop: row.try_get("can_develop")?,
                enabled: row.try_get("enabled")?,
                primary_group_id: row.try_get("primary_group_id")?,
                primary_group_name: row.try_get("primary_group_name")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(Json(AdminUserDirectoryPage { items, next_cursor }))
}

pub async fn page(s: &State, p: &Principal, kind: &str, q: Page) -> ApiResult<Value> {
    page_for(s, p, kind, q, false).await
}

async fn page_for(
    s: &State,
    p: &Principal,
    kind: &str,
    mut q: Page,
    include_managed_installations: bool,
) -> ApiResult<Value> {
    crate::auth_sessions::ensure_active(&s.pool, &p.id).await?;
    let limit = q.limit()?;
    let delegation = if let Some(id) = p.delegation_id {
        if !matches!(kind, "apps" | "jobs" | "installations" | "grants") {
            return Err(ApiError::forbidden());
        }
        let d = crate::delegations::active(&s.pool, &p.id, id).await?;
        crate::delegations::check(
            s,
            p,
            &d.app_id,
            &d.environment,
            crate::delegations::Action::Read,
        )
        .await?;
        if q.app_id.as_ref().is_some_and(|a| a != &d.app_id)
            || q.environment.as_ref().is_some_and(|e| e != &d.environment)
        {
            return Err(ApiError::forbidden());
        }
        q.app_id = Some(d.app_id.clone());
        if kind != "apps" {
            q.environment = Some(d.environment.clone());
        } else {
            q.environment = None;
        }
        Some(d)
    } else {
        None
    };
    let mut actor_scope = format!("{}:{:?}", p.id, p.delegation_id);
    if let Some(purpose) = p.browser_purpose {
        actor_scope.push(':');
        actor_scope.push_str(purpose.as_str());
    }
    let scope = q.scope(kind, &actor_scope);
    let boundary = q.boundary(&scope)?;
    // A delegation never carries administrator authority, as in domain::admin and /me:
    // a delegated administrator lists only its own grants. App visibility in the
    // apps, installations and jobs pages comes from the policy decisions instead.
    let is_admin = delegation.is_none() && domain::admin(s, p).await.is_ok();
    if kind == "users" && !is_admin {
        return Err(ApiError::forbidden());
    }
    let (source, key, view, permission, app, env, request) = match kind {
        "apps" => (
            APP_SOURCE,
            "t.id",
            APP_ROW,
            "t.id=ANY($8::text[])",
            "t.id",
            "NULL::text",
            "NULL::text",
        ),
        "users" => (
            "users t",
            "t.id",
            "jsonb_build_object('id',t.id,'name',t.name,'admin',t.admin,'canDevelop',t.can_develop,'enabled',t.enabled)",
            "$2",
            "NULL::text",
            "NULL::text",
            "NULL::text",
        ),
        "grants" => (
            "grants t",
            "jsonb_build_array(t.app_id,t.environment,t.user_id,t.permission)::text",
            "jsonb_build_object('userId',t.user_id,'appId',t.app_id,'environment',t.environment,'permission',t.permission)",
            "($2 OR t.user_id=$1)",
            "t.app_id",
            "t.environment",
            "NULL::text",
        ),
        "registrations" => (
            "app_registrations t",
            "t.id::text",
            "jsonb_build_object('id',t.id,'appId',t.app_id,'name',t.name,'requestedBy',t.actor,'status',t.status,'decidedBy',t.decided_by,'createdAt',t.created_at)",
            "($2 OR t.actor=$1)",
            "t.app_id",
            "NULL::text",
            "NULL::text",
        ),
        "jobs" => (
            "runtime_jobs t",
            "t.created_at::text||':'||t.id::text",
            crate::runtime::JOB_ROW,
            // The (app, environment) pairs where runtime::job would answer (Observe).
            ALLOWED_PAIRS,
            "t.app_id",
            "t.environment",
            "t.idempotency_key",
        ),
        "installations" => (
            "installations t JOIN releases r ON r.id=t.release_id JOIN apps a ON a.id=t.app_id",
            "t.app_id||':'||t.environment||':'||t.id::text",
            domain::INSTALLATION_ROW,
            // The pairs where the installation may be used (domain::installation).
            ALLOWED_PAIRS,
            "t.app_id",
            "t.environment",
            "NULL::text",
        ),
        _ => return Err(ApiError::bad("invalid_list_kind")),
    };
    let sql = format!(
        "SELECT {key} AS key,{view} AS value FROM {source} WHERE {permission} AND {key}>$3 AND ($4::text IS NULL OR {app}=$4) AND ($5::text IS NULL OR {env}=$5) AND ($6::text IS NULL OR {request}=$6) ORDER BY {key} LIMIT $7"
    );
    let q_app = q.app_id.clone();
    let mut query = sqlx::query(&sql)
        .bind(&p.id)
        .bind(is_admin)
        .bind(boundary)
        .bind(q.app_id)
        .bind(q.environment)
        .bind(q.request_id)
        .bind(limit + 1);
    let facts = match kind {
        "apps" | "jobs" | "installations" => {
            let facts = policy::Facts::load(&s.pool, p, q_app.as_deref()).await?;
            let apps: Vec<String> =
                sqlx::query_scalar("SELECT id FROM apps WHERE ($1::text IS NULL OR id=$1)")
                    .bind(q_app.as_deref())
                    .fetch_all(&s.pool)
                    .await?;
            query = match kind {
                "apps" => query.bind(
                    apps.into_iter()
                        .filter(|app| facts.sees_app(app))
                        .collect::<Vec<_>>(),
                ),
                _ => {
                    // Console app details include installations the user manages,
                    // including disabled apps and prod without a use grant. Public
                    // portal/non-browser installation lists retain their Use scope.
                    let action =
                        if kind == "jobs" || p.browser_purpose == Some(BrowserPurpose::Console) {
                            AppAction::Observe
                        } else {
                            AppAction::Use
                        };
                    let (mut allowed_apps, mut allowed_envs) = facts.allowed_pairs(&apps, action);
                    if kind == "installations" && include_managed_installations {
                        let (managed_apps, managed_envs) =
                            facts.allowed_pairs(&apps, AppAction::Observe);
                        allowed_apps.extend(managed_apps);
                        allowed_envs.extend(managed_envs);
                    }
                    query.bind(allowed_apps).bind(allowed_envs)
                }
            };
            Some(facts)
        }
        _ => None,
    };
    let mut rows = query.fetch_all(&s.pool).await?;
    let more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let cursor = if more {
        rows.last().map(|row| encode(&scope, row.get("key")))
    } else {
        None
    };
    let values = rows.into_iter().map(|row| row.get::<Value, _>("value"));
    // Installations and jobs are answered through their contract types (api_contract).
    Ok(match (kind, &facts) {
        ("installations", Some(facts)) => serde_json::to_value(InstallationPage {
            installations: values
                .map(|value| domain::installation_view(value, facts))
                .collect::<ApiResult<_>>()?,
            next_cursor: cursor,
        })?,
        ("jobs", _) => serde_json::to_value(RuntimeJobPage {
            jobs: values
                .map(serde_json::from_value)
                .collect::<Result<_, _>>()?,
            next_cursor: cursor,
        })?,
        _ => json!({kind:values.collect::<Vec<_>>(),"nextCursor":cursor}),
    })
}
/// Rows whose (app, environment) is in the policy's allowed pairs ($8, $9).
const ALLOWED_PAIRS: &str =
    "(t.app_id,t.environment) IN (SELECT * FROM unnest($8::text[],$9::text[]))";
pub async fn apps(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Query(q): Query<Page>,
) -> ApiResult<Json<Value>> {
    Ok(Json(page(&s, &p, "apps", q).await?))
}
pub async fn grants(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Query(q): Query<Page>,
) -> ApiResult<Json<Value>> {
    Ok(Json(page(&s, &p, "grants", q).await?))
}
pub async fn jobs(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Query(q): Query<Page>,
) -> ApiResult<Json<Value>> {
    Ok(Json(page(&s, &p, "jobs", q).await?))
}
pub async fn app(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    crate::auth_sessions::ensure_active(&s.pool, &p.id).await?;
    if let Some(delegation) = p.delegation_id {
        let d = crate::delegations::active(&s.pool, &p.id, delegation).await?;
        crate::delegations::check(
            &s,
            &p,
            &id,
            &d.environment,
            crate::delegations::Action::Read,
        )
        .await?;
    }
    if !policy::Facts::load(&s.pool, &p, Some(&id))
        .await?
        .sees_app(&id)
    {
        return Err(ApiError::not_found());
    }
    let app = sqlx::query_scalar::<_, sqlx::types::Json<App>>(&format!(
        "SELECT {APP_ROW} FROM {APP_SOURCE} WHERE t.id=$1"
    ))
    .bind(&id)
    .fetch_optional(&s.pool)
    .await?
    .ok_or_else(ApiError::not_found)?
    .0;
    // Published CLI app show/status uses this detail. Preserve its management
    // visibility without granting prod Use; each returned row carries canOpen.
    let installs = page_for(
        &s,
        &p,
        "installations",
        Page {
            app_id: Some(id),
            ..Default::default()
        },
        p.browser_purpose.is_none(),
    )
    .await?;
    let installs: InstallationPage = serde_json::from_value(installs)?;
    Ok(Json(serde_json::to_value(AppDetail {
        app,
        installations: installs.installations,
    })?))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cursors_are_bound_to_actor_and_filters() {
        let mut page = Page::default();
        let scope = page.scope("jobs", "alice");
        page.cursor = Some(encode(&scope, "boundary".into()));
        assert_eq!(page.boundary(&scope).unwrap(), "boundary");
        assert!(page.boundary(&page.scope("jobs", "bob")).is_err());
        page.app_id = Some("another-app".into());
        assert!(page.boundary(&page.scope("jobs", "alice")).is_err());
        page.limit = Some(201);
        assert!(page.limit().is_err());
    }
}
