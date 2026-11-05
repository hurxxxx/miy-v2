//! Revocable user-to-agent authority. Scope never replaces current user grants.
use crate::policy;
use crate::{
    State,
    auth::Principal,
    http::{ApiError, ApiResult},
    policy::AppAction,
};
use axum::{
    Extension, Json,
    extract::{Path, Query, State as S},
    http::header,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use uuid::Uuid;

fn denied(code: &str) -> ApiError {
    ApiError {
        status: axum::http::StatusCode::FORBIDDEN,
        code: code.into(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Read,
    AppRead,
    AppWrite,
    DirectoryRead,
    Build,
    Upload,
    DevSession,
    PrepareDatabase,
    Accept,
    Deploy,
    Promote,
    Start,
    Stop,
    Restart,
    Remove,
    Logs,
}
impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::AppRead => "app_read",
            Self::AppWrite => "app_write",
            Self::DirectoryRead => "directory_read",
            Self::Build => "build",
            Self::Upload => "upload",
            Self::DevSession => "dev_session",
            Self::PrepareDatabase => "prepare_database",
            Self::Accept => "accept",
            Self::Deploy => "deploy",
            Self::Promote => "promote",
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Remove => "remove",
            Self::Logs => "logs",
        }
    }
    pub(crate) fn management(self) -> bool {
        !matches!(
            self,
            Self::Read | Self::AppRead | Self::AppWrite | Self::DirectoryRead
        )
    }
}
impl std::str::FromStr for Action {
    type Err = ApiError;
    fn from_str(value: &str) -> ApiResult<Self> {
        serde_json::from_value(json!(value)).map_err(|_| ApiError::bad("invalid_delegation_action"))
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    pub id: Uuid,
    pub user_id: String,
    pub name: String,
    pub app_id: String,
    pub environment: String,
    pub actions: Vec<String>,
    pub expires_at: DateTime<Utc>,
}
impl Scope {
    pub fn allows(&self, app: &str, env: &str, action: Action) -> bool {
        self.app_id == app
            && self.environment == env
            && self.actions.iter().any(|v| v == action.as_str())
    }
}
const ACTIVE: &str = "SELECT d.* FROM delegations d JOIN users u ON u.id=d.user_id WHERE d.id=$1 AND d.user_id=$2 AND u.enabled AND d.revoked_at IS NULL AND d.expires_at>clock_timestamp() AND EXISTS(SELECT 1 FROM cli_sessions issued WHERE issued.delegation_id=d.id AND issued.revoked_at IS NULL AND issued.expires_at>clock_timestamp()) AND (d.parent_session_id IS NULL OR EXISTS(SELECT 1 FROM cli_sessions c WHERE c.id=d.parent_session_id AND c.user_id=d.user_id AND c.delegation_id IS NULL AND c.revoked_at IS NULL AND c.expires_at>clock_timestamp()))";
pub async fn active(pool: &PgPool, user: &str, id: Uuid) -> ApiResult<Scope> {
    active_in(&mut *pool.acquire().await?, user, id, false).await
}
/// [`active`] through `connection`; with `lock` the delegation row stays share-locked
/// until the transaction ends, so revoking it waits for the write being decided.
pub(crate) async fn active_in(
    connection: &mut sqlx::PgConnection,
    user: &str,
    id: Uuid,
    lock: bool,
) -> ApiResult<Scope> {
    let locked;
    let query = if lock {
        locked = format!("{ACTIVE} FOR SHARE OF d");
        locked.as_str()
    } else {
        ACTIVE
    };
    let row = sqlx::query(query)
        .bind(id)
        .bind(user)
        .fetch_optional(connection)
        .await?
        .ok_or_else(|| denied("delegation_expired_or_revoked"))?;
    Ok(Scope {
        id: row.get("id"),
        user_id: row.get("user_id"),
        name: row.get("name"),
        app_id: row.get("app_id"),
        environment: row.get("environment"),
        actions: row.get("actions"),
        expires_at: row.get("expires_at"),
    })
}
pub async fn check(
    state: &State,
    p: &Principal,
    app: &str,
    env: &str,
    action: Action,
) -> ApiResult<()> {
    if let Some(id) = p.delegation_id {
        check_id(&state.pool, &p.id, id, app, env, action).await?;
    }
    Ok(())
}
pub async fn check_id(
    pool: &PgPool,
    user: &str,
    id: Uuid,
    app: &str,
    env: &str,
    action: Action,
) -> ApiResult<()> {
    let scope = active(pool, user, id).await?;
    let mut facts = policy::Facts::actor(pool, user, Some(app)).await?;
    facts.delegation = policy::Delegation::Active(scope);
    Ok(facts.delegated(app, env, action)?)
}
/// Generic legacy mutators do not acquire new delegated authority implicitly.
pub fn unrestricted(p: &Principal) -> ApiResult<()> {
    if p.delegation_id.is_some() {
        Err(denied("delegation_scope_denied"))
    } else {
        Ok(())
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Create {
    pub app_id: String,
    pub environment: String,
    pub actions: Vec<Action>,
    pub expires_in: i64,
    pub name: String,
}
fn validate(input: &Create) -> ApiResult<()> {
    if !crate::contract::valid_id(&input.app_id)
        || !["dev", "prod"].contains(&input.environment.as_str())
        || !(60..=86400).contains(&input.expires_in)
        || input.actions.is_empty()
        || input.actions.len() > 16
        || input.name.trim().is_empty()
        || input.name.chars().count() > 80
        || input.name.chars().any(|c| {
            c.is_control()
                || matches!(c,'\u{200e}'|'\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}')
        })
    {
        return Err(ApiError::bad("invalid_delegation"));
    }
    let mut actions: Vec<_> = input.actions.iter().map(|a| a.as_str()).collect();
    actions.sort();
    actions.dedup();
    if actions.len() != input.actions.len() {
        return Err(ApiError::bad("duplicate_delegation_action"));
    }
    Ok(())
}
fn public(row: &sqlx::postgres::PgRow) -> Value {
    json!({"id":row.get::<Uuid,_>("id"),"name":row.get::<String,_>("name"),"appId":row.get::<String,_>("app_id"),"environment":row.get::<String,_>("environment"),"actions":row.get::<Vec<String>,_>("actions"),"createdAt":row.get::<DateTime<Utc>,_>("created_at"),"expiresAt":row.get::<DateTime<Utc>,_>("expires_at"),"revokedAt":row.get::<Option<DateTime<Utc>>,_>("revoked_at"),"subjectKind":"delegated_user"})
}
pub async fn create(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Json(input): Json<Create>,
) -> ApiResult<Response> {
    unrestricted(&p)?;
    validate(&input)?;
    crate::auth_sessions::ensure_active(&s.pool, &p.id).await?;
    let delegated = || {
        input
            .actions
            .iter()
            .map(|action| AppAction::Delegate(*action))
    };
    let facts = policy::Facts::load(&s.pool, &p, Some(&input.app_id)).await?;
    for action in delegated() {
        facts.decide(&input.app_id, &input.environment, action)?;
    }
    let mut tx = policy::Write::begin(&s.pool).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("delegations:{}", p.id))
        .execute(&mut *tx)
        .await?;
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM delegations WHERE user_id=$1 AND revoked_at IS NULL AND expires_at>now()").bind(&p.id).fetch_one(&mut *tx).await?;
    if count >= 32 {
        return Err(ApiError::conflict("too_many_active_delegations"));
    }
    let issued: DateTime<Utc> = sqlx::query_scalar("SELECT now()")
        .fetch_one(&mut *tx)
        .await?;
    let mut expires = issued + chrono::Duration::seconds(input.expires_in);
    if let Some(parent) = p.cli_session_id {
        let end:DateTime<Utc>=sqlx::query_scalar("SELECT expires_at FROM cli_sessions WHERE id=$1 AND user_id=$2 AND delegation_id IS NULL AND revoked_at IS NULL AND expires_at>now() FOR SHARE").bind(parent).bind(&p.id).fetch_optional(&mut *tx).await?.ok_or_else(ApiError::unauthorized)?;
        expires = expires.min(end);
    }
    let id = Uuid::new_v4();
    let session = Uuid::new_v4();
    let actions: Vec<_> = input.actions.iter().map(|a| a.as_str()).collect();
    let row=sqlx::query("INSERT INTO delegations(id,user_id,parent_session_id,name,app_id,environment,actions,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING *")
        .bind(id).bind(&p.id).bind(p.cli_session_id).bind(input.name.trim()).bind(&input.app_id).bind(&input.environment).bind(actions).bind(expires).fetch_one(&mut *tx).await?;
    sqlx::query("INSERT INTO cli_sessions(id,user_id,client_id,client_name,expires_at,delegation_id) VALUES($1,$2,$3,$4,$5,$6)").bind(session).bind(&p.id).bind(Uuid::new_v4()).bind(input.name.trim()).bind(expires).bind(id).execute(&mut *tx).await?;
    let credential = crate::auth_sessions::issue(&mut tx, session, &p.id, expires).await?;
    sqlx::query(
        "INSERT INTO audit(actor,action,target,request_id) VALUES($1,'delegation.created',$2,$2)",
    )
    .bind(&p.id)
    .bind(id.to_string())
    .execute(&mut *tx)
    .await?;
    // Decide after the per-user lock and the count, holding the account until commit.
    tx.decide_all(&p, &input.app_id, &input.environment, delegated())
        .await?
        .commit()
        .await?;
    Ok((
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::PRAGMA, "no-cache"),
        ],
        Json(json!({"delegation":public(&row),"credential":credential})),
    )
        .into_response())
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Page {
    pub limit: Option<i64>,
    pub cursor: Option<Uuid>,
}
pub async fn list(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Query(q): Query<Page>,
) -> ApiResult<Json<Value>> {
    unrestricted(&p)?;
    crate::auth_sessions::ensure_active(&s.pool, &p.id).await?;
    let limit = q.limit.unwrap_or(50);
    if !(1..=200).contains(&limit) {
        return Err(ApiError::bad("invalid_page_limit"));
    }
    let mut rows=sqlx::query("SELECT * FROM delegations WHERE user_id=$1 AND ($2::uuid IS NULL OR id>$2) ORDER BY id LIMIT $3").bind(&p.id).bind(q.cursor).bind(limit+1).fetch_all(&s.pool).await?;
    let more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let next = if more {
        rows.last().map(|r| r.get::<Uuid, _>("id"))
    } else {
        None
    };
    Ok(Json(
        json!({"delegations":rows.iter().map(public).collect::<Vec<_>>(),"nextCursor":next}),
    ))
}
pub async fn revoke(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    unrestricted(&p)?;
    crate::auth_sessions::ensure_active(&s.pool, &p.id).await?;
    let mut tx = s.pool.begin().await?;
    let changed = sqlx::query(
        "UPDATE delegations SET revoked_at=COALESCE(revoked_at,now()) WHERE id=$1 AND user_id=$2",
    )
    .bind(id)
    .bind(&p.id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if changed != 1 {
        return Err(ApiError::not_found());
    }
    sqlx::query(
        "INSERT INTO audit(actor,action,target,request_id) VALUES($1,'delegation.revoked',$2,$2)",
    )
    .bind(&p.id)
    .bind(id.to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id,"revoked":true})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scope_has_no_action_or_target_wildcards() {
        let scope = Scope {
            id: Uuid::nil(),
            user_id: "actor".into(),
            name: "agent".into(),
            app_id: "one".into(),
            environment: "dev".into(),
            actions: vec!["start".into()],
            expires_at: Utc::now(),
        };
        assert!(scope.allows("one", "dev", Action::Start));
        assert!(!scope.allows("one", "dev", Action::Stop));
        assert!(!scope.allows("one", "prod", Action::Start));
        assert!(!scope.allows("two", "dev", Action::Start));
        assert!("*".parse::<Action>().is_err());
    }
    #[test]
    fn expiry_and_actions_require_explicit_bounded_values() {
        let mut value = Create {
            app_id: "one".into(),
            environment: "dev".into(),
            actions: vec![Action::Read],
            expires_in: 60,
            name: "agent".into(),
        };
        assert!(validate(&value).is_ok());
        value.actions.push(Action::Read);
        assert!(validate(&value).is_err());
        value.actions.clear();
        assert!(validate(&value).is_err());
        value.actions.push(Action::Read);
        value.expires_in = 86401;
        assert!(validate(&value).is_err());
    }
}
