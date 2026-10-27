//! Bounded, expiring diagnostics. The core queues requests; only the worker runs Docker.
use crate::{
    State,
    auth::Principal,
    domain,
    http::{ApiError, ApiResult},
};
use axum::{
    Extension, Json,
    extract::{Path, State as S},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncReadExt};
use uuid::Uuid;
const MAX_BYTES: usize = 1024 * 1024;
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    environment: String,
    #[serde(default = "default_tail")]
    tail: i32,
    since: Option<chrono::DateTime<chrono::Utc>>,
}
fn default_tail() -> i32 {
    100
}
pub async fn request(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(app): Path<String>,
    Json(input): Json<Request>,
) -> ApiResult<Json<Value>> {
    if !["dev", "prod"].contains(&input.environment.as_str())
        || !(1..=1000).contains(&input.tail)
        || input.since.is_some_and(|v| v > chrono::Utc::now())
    {
        return Err(ApiError::bad("invalid_log_request"));
    }
    domain::manage(&s, &p, &app, &input.environment).await?;
    let mut tx = s.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("logs:{}", p.id))
        .execute(&mut *tx)
        .await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM runtime_log_requests WHERE actor=$1 AND expires_at>clock_timestamp()",
    )
    .bind(&p.id)
    .fetch_one(&mut *tx)
    .await?;
    if count >= 20 {
        return Err(ApiError::conflict("log_request_limit"));
    }
    let row=sqlx::query("SELECT id,control_generation,runtime_container FROM installations WHERE app_id=$1 AND environment=$2 AND runtime_status='running' AND runtime_container IS NOT NULL").bind(&app).bind(&input.environment).fetch_optional(&mut *tx).await?.ok_or_else(||ApiError::conflict("runtime_not_running"))?;
    let id = Uuid::new_v4();
    let expires:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("INSERT INTO runtime_log_requests(id,actor,app_id,environment,installation_id,generation,expected_container,tail,since) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING expires_at").bind(id).bind(&p.id).bind(app).bind(input.environment).bind(row.get::<Uuid,_>("id")).bind(row.get::<i64,_>("control_generation")).bind(row.get::<String,_>("runtime_container")).bind(input.tail).bind(input.since).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id,"status":"queued","expiresAt":expires})))
}
pub async fn get(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let row = sqlx::query(
        "SELECT * FROM runtime_log_requests WHERE id=$1 AND expires_at>clock_timestamp()",
    )
    .bind(id)
    .fetch_optional(&s.pool)
    .await?
    .ok_or_else(ApiError::not_found)?;
    if row.get::<String, _>("actor") != p.id {
        return Err(ApiError::forbidden());
    }
    domain::manage(&s, &p, row.get("app_id"), row.get("environment")).await?;
    Ok(Json(
        json!({"id":id,"status":row.get::<String,_>("status"),"appId":row.get::<String,_>("app_id"),"environment":row.get::<String,_>("environment"),"installationId":row.get::<Uuid,_>("installation_id"),"generation":row.get::<i64,_>("generation"),"container":row.get::<Option<String>,_>("container_id"),"tail":row.get::<i32,_>("tail"),"since":row.get::<Option<chrono::DateTime<chrono::Utc>>,_>("since"),"text":row.get::<Option<String>,_>("text"),"truncated":row.get::<bool,_>("truncated"),"rotationMayOmit":true,"maxBytes":MAX_BYTES,"expiresAt":row.get::<chrono::DateTime<chrono::Utc>,_>("expires_at"),"errorCode":row.get::<Option<String>,_>("error_code")}),
    ))
}
pub struct Monitor(tokio::task::JoinHandle<()>);
impl Drop for Monitor {
    fn drop(&mut self) {
        self.0.abort();
    }
}
pub fn start(pool: sqlx::PgPool) -> Monitor {
    Monitor(tokio::spawn(async move {
        let mut pending = tokio::task::JoinSet::new();
        // Only the globally exclusive worker starts this monitor.
        if sqlx::query("UPDATE runtime_log_requests SET status='failed',error_code='worker_restarted',text=NULL WHERE status='running'").execute(&pool).await.is_err(){return;}
        loop {
            while pending.try_join_next().is_some() {}
            if sqlx::query("DELETE FROM runtime_log_requests WHERE expires_at<=clock_timestamp()")
                .execute(&pool)
                .await
                .is_err()
            {
                tracing::warn!(event = "log_retention_failed");
            }
            if pending.len() < 2 {
                let next=sqlx::query("UPDATE runtime_log_requests SET status='running' WHERE id=(SELECT id FROM runtime_log_requests WHERE status='queued' AND expires_at>clock_timestamp() ORDER BY created_at,id LIMIT 1) RETURNING *").fetch_optional(&pool).await;
                match next {
                    Ok(Some(row)) => {
                        let pool = pool.clone();
                        pending.spawn(async move {
                            collect(&pool, row).await;
                        });
                    }
                    Ok(None) => {}
                    Err(_) => {
                        tracing::warn!(event = "log_queue_unavailable");
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }))
}
async fn current(pool: &sqlx::PgPool, row: &sqlx::postgres::PgRow) -> Result<(), &'static str> {
    if !domain::can_manage(
        pool,
        row.get("actor"),
        row.get("app_id"),
        row.get("environment"),
    )
    .await
    .map_err(|_| "diagnostic_unavailable")?
    {
        return Err("permission_revoked");
    }
    let same:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM installations WHERE id=$1 AND app_id=$2 AND environment=$3 AND control_generation=$4 AND runtime_container=$5 AND runtime_status='running')").bind(row.get::<Uuid,_>("installation_id")).bind(row.get::<String,_>("app_id")).bind(row.get::<String,_>("environment")).bind(row.get::<i64,_>("generation")).bind(row.get::<String,_>("expected_container")).fetch_one(pool).await.map_err(|_|"diagnostic_unavailable")?;
    if !same {
        return Err("state_changed");
    }
    Ok(())
}
async fn bounded_read(
    mut input: impl AsyncRead + Unpin,
    budget: Arc<AtomicUsize>,
) -> std::io::Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = input.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        let prior = budget.fetch_add(n, Ordering::Relaxed);
        let keep = n.min(MAX_BYTES.saturating_sub(prior));
        result.extend_from_slice(&buffer[..keep]);
    }
    Ok(result)
}
async fn logs(
    container: &str,
    tail: i32,
    since: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<(String, bool), &'static str> {
    let mut command = tokio::process::Command::new("docker");
    command.args(["logs", "--timestamps", "--tail", &tail.to_string()]);
    if let Some(since) = since {
        command.args(["--since", &since.to_rfc3339()]);
    }
    command
        .arg(container)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|_| "docker_unavailable")?;
    let stdout = child.stdout.take().ok_or("diagnostic_failed")?;
    let stderr = child.stderr.take().ok_or("diagnostic_failed")?;
    let count = Arc::new(AtomicUsize::new(0));
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::try_join!(
            bounded_read(stdout, count.clone()),
            bounded_read(stderr, count.clone()),
            child.wait()
        )
    })
    .await
    .map_err(|_| "log_timeout")?
    .map_err(|_| "log_read_failed")?;
    if !result.2.success() {
        return Err("log_read_failed");
    }
    let mut bytes = result.0;
    bytes.extend(result.1);
    // Lossy decoding can expand invalid UTF-8; cap the final UTF-8 representation too.
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    let mut truncated = count.load(Ordering::Relaxed) > MAX_BYTES;
    if text.len() > MAX_BYTES {
        let mut end = MAX_BYTES;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        truncated = true;
    }
    Ok((text, truncated))
}
async fn collect(pool: &sqlx::PgPool, row: sqlx::postgres::PgRow) {
    let id: Uuid = row.get("id");
    let result = async {
        current(pool, &row).await?;
        let metadata = crate::runtime::inspect(row.get("expected_container"))
            .await
            .map_err(|_| "container_unavailable")?;
        if metadata["Config"]["Labels"]["org.miy.installation"].as_str()
            != Some(row.get::<Uuid, _>("installation_id").to_string().as_str())
        {
            return Err("container_ownership_mismatch");
        }
        let container = metadata["Id"]
            .as_str()
            .filter(|v| v.len() == 64 && v.bytes().all(|c| c.is_ascii_hexdigit()))
            .ok_or("invalid_container_identity")?
            .to_owned();
        let (text, truncated) = logs(&container, row.get("tail"), row.get("since")).await?;
        current(pool, &row).await?;
        // The immutable ID prevents same-name replacements from changing the captured target.
        Ok::<_, &'static str>((container, text, truncated))
    }
    .await;
    let saved=match result {
        Ok((container,text,truncated))=>sqlx::query("UPDATE runtime_log_requests SET status='ready',container_id=$2,text=$3,truncated=$4 WHERE id=$1 AND expires_at>clock_timestamp()").bind(id).bind(container).bind(text).bind(truncated).execute(pool).await,
        Err(code)=>sqlx::query("UPDATE runtime_log_requests SET status='failed',error_code=$2,text=NULL WHERE id=$1").bind(id).bind(code).execute(pool).await,
    };
    if saved.is_err() {
        tracing::warn!(event="log_result_unavailable",%id);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn combined_stream_limit() {
        let budget = Arc::new(AtomicUsize::new(0));
        let data = vec![b'x'; MAX_BYTES + 17];
        let a = bounded_read(data.as_slice(), budget.clone()).await.unwrap();
        let b = bounded_read(b"second".as_slice(), budget.clone())
            .await
            .unwrap();
        assert_eq!(a.len() + b.len(), MAX_BYTES);
        assert!(budget.load(Ordering::Relaxed) > MAX_BYTES);
    }
}
