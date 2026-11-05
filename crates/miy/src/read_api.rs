//! Permission-filtered keyset pages. Cursors select a boundary, never authority.
use crate::{
    State,
    auth::Principal,
    domain,
    http::{ApiError, ApiResult},
    policy::{self, AppAction},
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State as S},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;

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
    fn boundary(&self, scope: &str) -> ApiResult<String> {
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
fn encode(scope: &str, boundary: String) -> String {
    URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&Cursor {
            version: 1,
            scope: scope.into(),
            boundary,
        })
        .unwrap(),
    )
}

pub async fn page(s: &State, p: &Principal, kind: &str, mut q: Page) -> ApiResult<Value> {
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
    let scope = q.scope(kind, &format!("{}:{:?}", p.id, p.delegation_id));
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
            "apps t",
            "t.id",
            "jsonb_build_object('id',t.id,'name',t.name)",
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
            "jsonb_build_object('id',t.id,'appId',t.app_id,'environment',t.environment,'action',t.action,'status',t.status,'step',t.step,'errorCode',t.error_code,'failureClass',t.failure_class,'retryable',t.retryable,'retryOf',t.retry_of,'requestId',t.idempotency_key,'imageId',t.image_id,'releaseId',t.release_id,'createdAt',t.created_at)",
            // The (app, environment) pairs where runtime::job would answer (FollowJob).
            ALLOWED_PAIRS,
            "t.app_id",
            "t.environment",
            "t.idempotency_key",
        ),
        "installations" => (
            "installations t JOIN releases r ON r.id=t.release_id JOIN apps a ON a.id=t.app_id",
            "t.app_id||':'||t.environment||':'||t.id::text",
            INSTALLATION_VIEW,
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
                    let action = if kind == "jobs" {
                        AppAction::FollowJob
                    } else {
                        AppAction::Use
                    };
                    let (apps, envs) = facts.allowed_pairs(&apps, action);
                    query.bind(apps).bind(envs)
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
    let values: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            let mut value: Value = row.get("value");
            if let Some(facts) = &facts
                && kind == "installations"
            {
                // Display values are the decisions this credential would get.
                let flags = facts.flags(
                    value["appId"].as_str().unwrap_or_default(),
                    value["environment"].as_str().unwrap_or_default(),
                );
                let record = value
                    .as_object_mut()
                    .and_then(|item| item.remove("acceptanceRecord"))
                    .unwrap_or(Value::Null);
                value["canPromote"] = json!(flags.can_promote);
                value["canManage"] = json!(flags.can_manage);
                value["acceptance"] = if flags.can_manage {
                    record
                } else {
                    Value::Null
                };
            }
            value
        })
        .collect();
    Ok(json!({kind:values,"nextCursor":cursor}))
}
const INSTALLATION_VIEW: &str = "jsonb_build_object('id',t.id,'appId',t.app_id,'name',a.name,'environment',t.environment,'releaseId',t.release_id,'imageId',r.image_id,'sourceCommit',r.source_commit,'version',r.version,'manifestName',r.manifest->>'name','kind',CASE WHEN r.image_id IS NULL THEN 'validation_fixture' ELSE 'docker_app' END,'lastOperation',(SELECT jsonb_build_object('id',j.id,'status',j.status,'action',j.action,'errorCode',j.error_code,'step',j.step,'retryable',j.retryable) FROM runtime_jobs j WHERE j.app_id=t.app_id AND j.environment=t.environment ORDER BY j.created_at DESC,j.id DESC LIMIT 1),'generation',t.control_generation,'dbStatus',t.db_status,'runtimeStatus',t.runtime_status,'desiredState',t.desired_state,'health',jsonb_build_object('status',t.health_status,'checkedAt',t.health_checked_at,'consecutiveFailures',t.health_failures),'acceptanceRecord',(SELECT jsonb_build_object('recordedBy',c.actor,'note',c.note,'checks',c.checks,'checkedAt',c.checked_at) FROM release_checks c WHERE c.release_id=r.id))";
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
    let app: Value =
        sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',name) FROM apps WHERE id=$1")
            .bind(&id)
            .fetch_optional(&s.pool)
            .await?
            .ok_or_else(ApiError::not_found)?;
    let installs = page(
        &s,
        &p,
        "installations",
        Page {
            app_id: Some(id),
            ..Default::default()
        },
    )
    .await?;
    Ok(Json(
        json!({"app":app,"installations":installs["installations"]}),
    ))
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
