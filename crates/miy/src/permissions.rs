//! Explicit external access approvals, scoped to one installation and release.
use crate::{
    State,
    api_error::{ApiError, ApiResult},
    auth::Principal,
    domain,
    policy::{self, AppAction},
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State as S},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionUpdate {
    pub release_id: Uuid,
    pub expected_revision: u64,
    pub permissions: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseQuery {
    pub release_id: Option<Uuid>,
}

pub fn external(permission: &str) -> bool {
    permission != "database" && crate::contract::valid_permission(permission)
}

fn approved_subset(requested: &[String], permissions: &[String]) -> ApiResult<Vec<String>> {
    if permissions.len() > 128
        || permissions
            .iter()
            .any(|p| !external(p) || !requested.contains(p))
    {
        return Err(ApiError::bad("invalid_installation_permissions"));
    }
    let mut values = permissions.to_vec();
    values.sort();
    values.dedup();
    if values.len() != permissions.len() {
        return Err(ApiError::bad("duplicate_permission"));
    }
    Ok(values)
}

pub async fn requested(
    pool: &sqlx::PgPool,
    installation: Uuid,
    release: Uuid,
) -> ApiResult<Vec<String>> {
    let manifest: Option<Value> = sqlx::query_scalar("SELECT r.manifest FROM installations i JOIN releases r ON r.app_id=i.app_id WHERE i.id=$1 AND r.id=$2")
        .bind(installation).bind(release).fetch_optional(pool).await?;
    let manifest: crate::contract::Manifest =
        serde_json::from_value(manifest.ok_or_else(|| ApiError::bad("release_app_mismatch"))?)?;
    Ok(manifest
        .requested_permissions
        .into_iter()
        .filter(|p| external(p))
        .collect())
}

/// Every use re-intersects the immutable manifest with the current approval.
pub async fn effective(
    pool: &sqlx::PgPool,
    installation: Uuid,
    release: Uuid,
) -> ApiResult<Vec<String>> {
    let requested = requested(pool, installation, release).await?;
    let value:Option<Value>=sqlx::query_scalar("SELECT permissions FROM installation_permissions WHERE installation_id=$1 AND release_id=$2")
        .bind(installation).bind(release).fetch_optional(pool).await?;
    let approved: Vec<String> = serde_json::from_value(value.unwrap_or_else(|| json!([])))?;
    Ok(approved
        .into_iter()
        .filter(|p| requested.contains(p))
        .collect())
}

pub async fn require(
    pool: &sqlx::PgPool,
    installation: Uuid,
    release: Uuid,
    permission: &str,
) -> ApiResult<()> {
    if effective(pool, installation, release)
        .await?
        .iter()
        .any(|p| p == permission)
    {
        Ok(())
    } else {
        Err(ApiError {
            status: axum::http::StatusCode::FORBIDDEN,
            code: "installation_permission_denied".into(),
        })
    }
}

/// Approvals are an installation's operating record ([`AppAction::Observe`]): readable
/// with management read that keeps a company-disabled prod app in the
/// administrator's runtime management list. Returns the installation's current release.
pub async fn observable(s: &State, p: &Principal, installation: Uuid) -> ApiResult<Uuid> {
    let row = sqlx::query("SELECT app_id,environment,release_id FROM installations WHERE id=$1")
        .bind(installation)
        .fetch_optional(&s.pool)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let (app, env): (String, String) = (row.get("app_id"), row.get("environment"));
    policy::require(s, p, &app, &env, AppAction::Observe).await?;
    Ok(row.get("release_id"))
}

pub async fn read(s: &State, p: &Principal, installation: Uuid, release: Uuid) -> ApiResult<Value> {
    observable(s, p, installation).await?;
    let requested = requested(&s.pool, installation, release).await?;
    let mut connection = s.pool.acquire().await?;
    record(&mut connection, installation, release, requested).await
}

/// The approval record. Callers decide first; a change builds its answer in its own
/// transaction, so the answer never depends on a decision made after the commit.
async fn record(
    connection: &mut sqlx::PgConnection,
    installation: Uuid,
    release: Uuid,
    requested: Vec<String>,
) -> ApiResult<Value> {
    let current=sqlx::query("SELECT revision,permissions,approved_by,approved_at FROM installation_permissions WHERE installation_id=$1 AND release_id=$2")
        .bind(installation).bind(release).fetch_optional(&mut *connection).await?;
    let history:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('revision',revision,'permissions',permissions,'actor',actor,'createdAt',created_at) FROM installation_permission_history WHERE installation_id=$1 AND release_id=$2 ORDER BY revision DESC LIMIT 100")
        .bind(installation).bind(release).fetch_all(&mut *connection).await?;
    Ok(
        json!({"installationId":installation,"releaseId":release,"requestedPermissions":requested,
        "approvedPermissions":current.as_ref().map(|r|r.get::<Value,_>("permissions")).unwrap_or_else(||json!([])),
        "revision":current.as_ref().map(|r|r.get::<i64,_>("revision")).unwrap_or(0),
        "approvedBy":current.as_ref().map(|r|r.get::<String,_>("approved_by")),
        "approvedAt":current.as_ref().map(|r|r.get::<chrono::DateTime<chrono::Utc>,_>("approved_at")),"history":history}),
    )
}

pub async fn update(
    s: &State,
    p: &Principal,
    installation: Uuid,
    input: PermissionUpdate,
) -> ApiResult<Value> {
    // A delegated administrator is still limited; it cannot mint app grants.
    if p.delegation_id.is_some() || !p.scopes.iter().any(|v| v == "miy.dev") {
        return Err(ApiError::forbidden());
    }
    domain::admin(s, p).await?;
    let requested = requested(&s.pool, installation, input.release_id).await?;
    let approved = approved_subset(&requested, &input.permissions)?;
    let revision = i64::try_from(input.expected_revision)
        .ok()
        .and_then(|v| v.checked_add(1))
        .ok_or_else(|| ApiError::bad("invalid_revision"))?;
    let mut tx = policy::Write::begin(&s.pool).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "installation-permissions:{installation}:{}",
            input.release_id
        ))
        .execute(&mut *tx)
        .await?;
    // Decide after the approval lock, in the transaction that records the approval.
    let mut tx = tx
        .decide_platform(p, policy::PlatformAction::Administer)
        .await?;
    let previous: Option<i64> = sqlx::query_scalar(
        "SELECT revision FROM installation_permissions WHERE installation_id=$1 AND release_id=$2",
    )
    .bind(installation)
    .bind(input.release_id)
    .fetch_optional(&mut *tx)
    .await?;
    if previous.unwrap_or(0) != revision - 1 {
        return Err(ApiError::conflict("permission_revision_changed"));
    }
    sqlx::query("INSERT INTO installation_permissions(installation_id,release_id,revision,permissions,approved_by) VALUES($1,$2,$3,$4,$5) ON CONFLICT(installation_id,release_id) DO UPDATE SET revision=excluded.revision,permissions=excluded.permissions,approved_by=excluded.approved_by,approved_at=clock_timestamp()")
        .bind(installation).bind(input.release_id).bind(revision).bind(json!(approved)).bind(&p.id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO installation_permission_history(installation_id,release_id,revision,permissions,actor) VALUES($1,$2,$3,$4,$5)")
        .bind(installation).bind(input.release_id).bind(revision).bind(json!(approved)).bind(&p.id).execute(&mut *tx).await?;
    let answer = record(&mut tx, installation, input.release_id, requested).await?;
    tx.commit().await?;
    Ok(answer)
}
pub async fn get(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
    Query(q): Query<ReleaseQuery>,
) -> ApiResult<Json<Value>> {
    let release = match q.release_id {
        Some(r) => r,
        None => observable(&s, &p, id).await?,
    };
    Ok(Json(read(&s, &p, id, release).await?))
}
pub async fn put(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
    Json(v): Json<PermissionUpdate>,
) -> ApiResult<Json<Value>> {
    Ok(Json(update(&s, &p, id, v).await?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn approvals_are_explicit_external_subsets() {
        let requested = vec!["platform:directory:read".into(), "app:other:read".into()];
        assert_eq!(
            approved_subset(&requested, &[]).unwrap(),
            Vec::<String>::new()
        );
        for p in ["database", "app:other:write", "platform:admin"] {
            assert!(approved_subset(&requested, &[p.into()]).is_err());
        }
        assert!(
            approved_subset(&requested, &[requested[0].clone(), requested[0].clone()]).is_err()
        );
        assert_eq!(
            approved_subset(&requested, &[requested[1].clone()]).unwrap(),
            vec![requested[1].clone()]
        );
    }
}
