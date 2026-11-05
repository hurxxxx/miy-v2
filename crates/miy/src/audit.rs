//! Administrator audit queries (V1-A1). Rows are written by the request middleware
//! (`METHOD:status` with the request path) and by domain changes in the same
//! transaction (for example `group.member_added`). Newest first, paged by id.
//! Retention is not decided yet (PRD open item); nothing is deleted.
use crate::{
    State,
    api_contract::{AuditEntry, AuditPage},
    auth::Principal,
    domain,
    http::{ApiError, ApiResult},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuditQuery {
    /// Inclusive lower bound (RFC 3339).
    pub from: Option<DateTime<Utc>>,
    /// Exclusive upper bound (RFC 3339).
    pub to: Option<DateTime<Utc>>,
    /// Exact actor user ID.
    pub actor: Option<String>,
    /// Action prefix, for example `group.` or `app.`.
    pub action: Option<String>,
    /// `nextCursor` of the previous page.
    pub cursor: Option<i64>,
    /// 1-200, default 50.
    pub limit: Option<i64>,
}

/// Record a domain change in the caller's transaction, so a rolled-back change
/// leaves no audit row and a committed one always has its row.
pub(crate) async fn record(
    tx: &mut sqlx::PgConnection,
    actor: &str,
    action: &str,
    target: &str,
) -> ApiResult<()> {
    sqlx::query("INSERT INTO audit(actor,action,target,request_id) VALUES($1,$2,$3,$4)")
        .bind(actor)
        .bind(action)
        .bind(target)
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&mut *tx)
        .await?;
    Ok(())
}

fn action_prefix(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}

pub async fn list(state: &State, p: &Principal, q: AuditQuery) -> ApiResult<Value> {
    domain::admin(state, p).await?;
    let limit = q.limit.unwrap_or(50);
    if !(1..=200).contains(&limit) {
        return Err(ApiError::bad("invalid_page_limit"));
    }
    if q.action.as_deref().is_some_and(|a| !action_prefix(a)) {
        return Err(ApiError::bad("invalid_audit_action"));
    }
    let rows = sqlx::query(
        "SELECT a.id,a.created_at,a.actor,u.name AS actor_name,a.action,a.target,a.request_id FROM audit a \
         LEFT JOIN users u ON u.id=a.actor \
         WHERE ($1::timestamptz IS NULL OR a.created_at>=$1) AND ($2::timestamptz IS NULL OR a.created_at<$2) \
         AND ($3::text IS NULL OR a.actor=$3) AND ($4::text IS NULL OR starts_with(a.action,$4)) \
         AND ($5::bigint IS NULL OR a.id<$5) ORDER BY a.id DESC LIMIT $6",
    )
    .bind(q.from)
    .bind(q.to)
    .bind(&q.actor)
    .bind(&q.action)
    .bind(q.cursor)
    .bind(limit + 1)
    .fetch_all(&state.pool)
    .await?;
    let more = rows.len() as i64 > limit;
    let entries: Vec<AuditEntry> = rows
        .iter()
        .take(limit as usize)
        .map(|r| AuditEntry {
            id: r.get("id"),
            at: r.get("created_at"),
            actor: r.get("actor"),
            actor_name: r.get("actor_name"),
            action: r.get("action"),
            target: r.get("target"),
            request_id: r.get("request_id"),
        })
        .collect();
    let next_cursor = if more {
        entries.last().map(|e| e.id)
    } else {
        None
    };
    Ok(serde_json::to_value(AuditPage {
        entries,
        next_cursor,
    })?)
}

pub fn catalog() -> Value {
    json!({"id":"list_audit","method":"GET","path":"/api/v1/audit","permission":"admin","description":"Newest-first audit entries filtered by period, actor and action prefix, paged by nextCursor; request rows are METHOD:status with the path, domain rows name the change","input":schemars::schema_for!(AuditQuery)})
}

#[cfg(test)]
mod tests {
    #[test]
    fn action_prefixes_are_plain_identifiers() {
        assert!(super::action_prefix("group."));
        assert!(super::action_prefix("POST:403"));
        assert!(!super::action_prefix(""));
        assert!(!super::action_prefix("group%"));
        assert!(!super::action_prefix(&"a".repeat(65)));
    }
}
