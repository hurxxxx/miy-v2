//! AI-DO `record_audit_log` wrote to the platform audit table. Messenger keeps
//! these rows in its own database (`dm_audit_logs`); like AI-DO the payloads
//! carry no message body, note or file name.
use crate::{error::ApiResult, time::now_utc};
use serde_json::Value;
use sqlx::PgConnection;

pub async fn record(
    conn: &mut PgConnection,
    action: &str,
    entity_kind: &str,
    entity_id: Option<&str>,
    summary: &str,
    actor_user_id: &str,
    payload: Value,
) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO dm_audit_logs (action, entity_kind, entity_id, summary, actor_user_id, payload, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(action)
    .bind(entity_kind)
    .bind(entity_id)
    .bind(summary)
    .bind(actor_user_id)
    .bind(strip_nulls(payload))
    .bind(now_utc())
    .execute(conn)
    .await?;
    Ok(())
}

fn strip_nulls(payload: Value) -> Value {
    match payload {
        Value::Object(map) => {
            Value::Object(map.into_iter().filter(|(_, v)| !v.is_null()).collect())
        }
        other => other,
    }
}
