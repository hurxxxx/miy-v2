//! Core-owned continuation proof. An app's HS256 identity key never authenticates
//! it to the core. Each hop retains the original user and only narrows authority.
use crate::{
    State,
    api_error::{ApiError, ApiResult},
    auth::Principal,
    contract, domain,
    identity::{Caller, IdentityContext},
    permissions,
};
use axum::http::Method;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

pub const PREFIX: &str = "miy_call_";
const MAX_HOPS: usize = 8;

pub async fn audit_rejected(s: &State, token: &str, path: &str) {
    // Unknown random tokens do not create unbounded audit rows. Never store the
    // capability or its hash in the public audit record.
    if let Ok(Some(user)) =
        sqlx::query_scalar::<_, String>("SELECT user_id FROM app_call_tokens WHERE token_hash=$1")
            .bind(contract::hash(token))
            .fetch_optional(&s.pool)
            .await
        && sqlx::query("INSERT INTO audit(actor,action,target,request_id) VALUES($1,'app_call.rejected',$2,$3)")
            .bind(user).bind(path.chars().take(1024).collect::<String>()).bind(Uuid::new_v4().to_string()).execute(&s.pool).await.is_err(){
            tracing::error!(event="audit_write_failed");
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Context {
    pub installation_id: Uuid,
    pub release_id: Uuid,
    pub environment: String,
    pub callers: Vec<Caller>,
    pub permissions_used: Vec<String>,
    pub effective_permissions: Vec<String>,
    pub expires_at: DateTime<Utc>,
    pub origin: Option<Origin>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Origin {
    pub kind: String,
    pub digest: String,
}

pub fn origin(p: &Principal, headers: &axum::http::HeaderMap) -> Option<Origin> {
    if p.cli_session_id.is_some() {
        return None;
    }
    if p.csrf.is_some() {
        let (kind, cookie) = if p.development {
            ("development", "miy_development")
        } else {
            ("browser", "miy_session")
        };
        crate::auth::cookie(headers, cookie).map(|value| Origin {
            kind: kind.into(),
            digest: contract::hash(value),
        })
    } else {
        headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .filter(|v| !v.starts_with(PREFIX))
            .map(|value| Origin {
                kind: "legacy_cli".into(),
                digest: contract::hash(value),
            })
    }
}
/// A trusted request extension, constructed only by core routing.
#[derive(Clone)]
pub struct ForwardContext(pub Context);

fn intersection(left: &[String], right: &[String]) -> Vec<String> {
    left.iter().filter(|p| right.contains(p)).cloned().collect()
}

pub fn allowed_route(context: &Context, path: &str, method: &Method) -> bool {
    let prefix = format!("/api/v1/installations/{}/", context.installation_id);
    let Some(suffix) = path.strip_prefix(&prefix) else {
        return false;
    };
    if suffix == "platform/directory" {
        return *method == Method::GET;
    }
    suffix.strip_prefix("connections/").is_some_and(|p| {
        p.split_once("/api/")
            .is_some_and(|(target, tail)| contract::valid_id(target) && !tail.is_empty())
    }) && matches!(
        *method,
        Method::GET | Method::HEAD | Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

pub async fn authenticate(s: &State, token: &str) -> ApiResult<(Principal, Context)> {
    let Some(secret) = token.strip_prefix(PREFIX) else {
        return Err(ApiError::unauthorized());
    };
    if secret.len() != 43
        || !secret
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return Err(ApiError::unauthorized());
    }
    let row = sqlx::query(
        "SELECT * FROM app_call_tokens WHERE token_hash=$1 AND expires_at>clock_timestamp()",
    )
    .bind(contract::hash(token))
    .fetch_optional(&s.pool)
    .await?
    .ok_or_else(ApiError::unauthorized)?;
    let p = Principal {
        id: row.get("user_id"),
        cli_session_id: row.get("cli_session_id"),
        delegation_id: row.get("delegation_id"),
        scopes: vec!["miy.read".into()],
        csrf: None,
        development: false,
    };
    crate::auth_sessions::ensure_active(&s.pool, &p.id).await?;
    if let Some(session) = p.cli_session_id {
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cli_sessions WHERE id=$1 AND user_id=$2 AND revoked_at IS NULL AND expires_at>clock_timestamp())")
            .bind(session).bind(&p.id).fetch_one(&s.pool).await?;
        if !valid {
            return Err(ApiError::unauthorized());
        }
    }
    let context = Context {
        installation_id: row.get("installation_id"),
        release_id: row.get("release_id"),
        environment: row.get("environment"),
        callers: serde_json::from_value(row.get("callers"))?,
        permissions_used: serde_json::from_value(row.get("permissions_used"))?,
        effective_permissions: serde_json::from_value(row.get("effective_permissions"))?,
        expires_at: row.get("expires_at"),
        origin: row
            .get::<Option<serde_json::Value>, _>("origin")
            .map(serde_json::from_value)
            .transpose()?,
    };
    current(s, &p, &context).await?;
    Ok((p, context))
}

/// Revalidate all installations, releases and grants at each continuation. A
/// revoked permission cannot return through an already issued child capability.
async fn current(s: &State, p: &Principal, c: &Context) -> ApiResult<Vec<String>> {
    if c.callers.len() > MAX_HOPS
        || c.callers.len() != c.permissions_used.len()
        || c.expires_at <= Utc::now()
    {
        return Err(ApiError::forbidden());
    }
    if let Some(session) = p.cli_session_id {
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cli_sessions WHERE id=$1 AND user_id=$2 AND revoked_at IS NULL AND expires_at>clock_timestamp())")
            .bind(session).bind(&p.id).fetch_one(&s.pool).await?;
        if !valid {
            return Err(ApiError::unauthorized());
        }
    }
    if let Some(origin) = &c.origin {
        let sql = match origin.kind.as_str() {
            "browser" => {
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE id_hash=$1 AND user_id=$2 AND expires_at>clock_timestamp() AND created_at>clock_timestamp()-interval '8 hours')"
            }
            "development" => {
                "SELECT EXISTS(SELECT 1 FROM development_sessions WHERE id_hash=$1 AND user_id=$2 AND expires_at>clock_timestamp())"
            }
            "legacy_cli" => {
                "SELECT EXISTS(SELECT 1 FROM cli_tokens WHERE token_hash=$1 AND user_id=$2 AND expires_at>clock_timestamp())"
            }
            _ => return Err(ApiError::unauthorized()),
        };
        if !sqlx::query_scalar::<_, bool>(sql)
            .bind(&origin.digest)
            .bind(&p.id)
            .fetch_one(&s.pool)
            .await?
        {
            return Err(ApiError::unauthorized());
        }
    }
    for (caller, permission) in c.callers.iter().zip(&c.permissions_used) {
        permissions::require(
            &s.pool,
            caller.installation_id,
            caller.release_id,
            permission,
        )
        .await?;
    }
    let mut effective = c.effective_permissions.clone();
    let mut ids = std::collections::HashSet::new();
    for (id, release, app) in c
        .callers
        .iter()
        .map(|v| (v.installation_id, v.release_id, Some(v.app_id.as_str())))
        .chain(std::iter::once((c.installation_id, c.release_id, None)))
    {
        if !ids.insert(id) {
            return Err(ApiError::forbidden());
        }
        let inst = domain::installation(s, p, id).await?;
        if inst.get::<Uuid, _>("release_id") != release
            || inst.get::<String, _>("environment") != c.environment
            || app.is_some_and(|app| inst.get::<String, _>("app_id") != app)
        {
            return Err(ApiError::forbidden());
        }
        effective = intersection(
            &effective,
            &permissions::effective(&s.pool, id, release).await?,
        );
    }
    Ok(effective)
}

pub async fn require(
    s: &State,
    p: &Principal,
    c: &Context,
    id: Uuid,
    permission: &str,
) -> ApiResult<()> {
    if c.installation_id != id || !current(s, p, c).await?.iter().any(|v| v == permission) {
        return Err(ApiError::forbidden());
    }
    Ok(())
}

/// Append the source only after its requested+approved permission was checked.
pub async fn forward(
    s: &State,
    p: &Principal,
    source: &sqlx::postgres::PgRow,
    target: &sqlx::postgres::PgRow,
    prior: Option<&Context>,
    origin: Option<&Origin>,
    permission: &str,
) -> ApiResult<ForwardContext> {
    let source_id: Uuid = source.get("id");
    let source_release: Uuid = source.get("release_id");
    let target_id: Uuid = target.get("id");
    let mut callers = prior.map(|v| v.callers.clone()).unwrap_or_default();
    let mut permissions_used = prior
        .map(|v| v.permissions_used.clone())
        .unwrap_or_default();
    if callers.len() >= MAX_HOPS
        || source_id == target_id
        || callers.iter().any(|v| v.installation_id == target_id)
    {
        return Err(ApiError::forbidden());
    }
    let effective = match prior {
        Some(c) => current(s, p, c).await?,
        None => permissions::effective(&s.pool, source_id, source_release).await?,
    };
    callers.push(Caller {
        app_id: source.get("app_id"),
        installation_id: source_id,
        release_id: source_release,
    });
    permissions_used.push(permission.to_owned());
    let target_permissions =
        permissions::effective(&s.pool, target_id, target.get("release_id")).await?;
    Ok(ForwardContext(Context {
        installation_id: target_id,
        release_id: target.get("release_id"),
        environment: source.get("environment"),
        callers,
        permissions_used,
        effective_permissions: intersection(&effective, &target_permissions),
        expires_at: prior
            .map(|v| v.expires_at)
            .unwrap_or_else(|| Utc::now() + Duration::seconds(30)),
        origin: prior
            .and_then(|v| v.origin.clone())
            .or_else(|| origin.cloned()),
    }))
}

pub async fn identity(
    s: &State,
    p: &Principal,
    inst: &sqlx::postgres::PgRow,
    forward: Option<&ForwardContext>,
    origin: Option<&Origin>,
    issue: bool,
) -> ApiResult<IdentityContext> {
    let id: Uuid = inst.get("id");
    let release: Uuid = inst.get("release_id");
    let context = match forward {
        Some(c) => c.0.clone(),
        None => Context {
            installation_id: id,
            release_id: release,
            environment: inst.get("environment"),
            callers: vec![],
            permissions_used: vec![],
            effective_permissions: permissions::effective(&s.pool, id, release).await?,
            expires_at: Utc::now() + Duration::seconds(30),
            origin: origin.cloned(),
        },
    };
    if context.installation_id != id || context.release_id != release {
        return Err(ApiError::forbidden());
    }
    let effective = current(s, p, &context).await?;
    let token = if issue
        && (p.cli_session_id.is_some() || context.origin.is_some())
        && !effective.is_empty()
        && context.callers.len() < MAX_HOPS
    {
        let mut tx = crate::policy::Write::begin(&s.pool).await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("app-call:{}", p.id))
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM app_call_tokens WHERE token_hash IN (SELECT token_hash FROM app_call_tokens WHERE expires_at<=clock_timestamp() LIMIT 2000)").execute(&mut *tx).await?;
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM app_call_tokens WHERE user_id=$1 AND expires_at>clock_timestamp()")
                .bind(&p.id)
                .fetch_one(&mut *tx)
                .await?;
        if count >= 1000 {
            return Err(ApiError::unavailable("app_call_capacity"));
        }
        let token = format!("{PREFIX}{}", crate::auth::random());
        sqlx::query("INSERT INTO app_call_tokens(token_hash,user_id,cli_session_id,delegation_id,installation_id,release_id,environment,callers,effective_permissions,expires_at,permissions_used,origin) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
            .bind(contract::hash(&token)).bind(&p.id).bind(p.cli_session_id).bind(p.delegation_id).bind(id).bind(release).bind(&context.environment).bind(serde_json::to_value(&context.callers)?).bind(serde_json::to_value(&effective)?).bind(context.expires_at).bind(serde_json::to_value(&context.permissions_used)?).bind(context.origin.as_ref().map(serde_json::to_value).transpose()?).execute(&mut *tx).await?;
        // The token carries this use of the installation; decide it before it exists.
        let app: String = inst.get("app_id");
        let environment: String = inst.get("environment");
        tx.decide(p, &app, &environment, crate::policy::AppAction::Use)
            .await?
            .commit()
            .await?;
        Some(token)
    } else {
        None
    };
    Ok(IdentityContext {
        environment: context.environment,
        callers: context.callers,
        effective_permissions: effective,
        delegation_token: token,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capability_routes_are_bound_to_one_installation_and_data_calls() {
        let c = Context {
            installation_id: Uuid::new_v4(),
            release_id: Uuid::new_v4(),
            environment: "dev".into(),
            callers: vec![],
            permissions_used: vec![],
            effective_permissions: vec![],
            expires_at: Utc::now(),
            origin: None,
        };
        let prefix = format!("/api/v1/installations/{}", c.installation_id);
        assert!(allowed_route(
            &c,
            &format!("{prefix}/connections/target/api/items"),
            &Method::POST
        ));
        assert!(allowed_route(
            &c,
            &format!("{prefix}/platform/directory"),
            &Method::GET
        ));
        for path in [
            "/api/v1/grants".to_string(),
            format!("{prefix}/api/data"),
            format!("{prefix}/permissions"),
            format!(
                "/api/v1/installations/{}/connections/target/api/items",
                Uuid::new_v4()
            ),
        ] {
            assert!(!allowed_route(&c, &path, &Method::GET));
        }
        assert!(!allowed_route(
            &c,
            &format!("{prefix}/platform/directory"),
            &Method::POST
        ));
    }
}
