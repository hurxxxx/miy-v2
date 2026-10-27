//! Bounded service probes. Observations never start or stop a container.
pub use crate::app_health::{client, probe};
use anyhow::{Result, ensure};
use futures_util::StreamExt;
use sqlx::Row;
use std::time::Duration;
use uuid::Uuid;

pub enum Observation<'a> {
    Healthy,
    Failed(&'a str),
    Unknown(&'a str),
    Inactive,
}
fn transition(previous_failures: i32, observation: &Observation<'_>) -> (&'static str, i32) {
    match observation {
        Observation::Healthy => ("healthy", 0),
        Observation::Failed(_) => {
            let failures = previous_failures.saturating_add(1).min(3);
            (
                if failures >= 3 {
                    "unhealthy"
                } else {
                    "degraded"
                },
                failures,
            )
        }
        Observation::Unknown(_) => ("unknown", 0),
        Observation::Inactive => ("not_applicable", 0),
    }
}
pub async fn record(pool: &sqlx::PgPool, id: Uuid, observation: Observation<'_>) -> Result<()> {
    ensure!(
        persist(pool, id, None, observation).await?,
        "missing_installation"
    );
    Ok(())
}
struct Snapshot {
    id: Uuid,
    generation: i64,
    release: Uuid,
    port: i32,
}
async fn persist(
    pool: &sqlx::PgPool,
    id: Uuid,
    snapshot: Option<&Snapshot>,
    observation: Observation<'_>,
) -> Result<bool> {
    let mut tx = pool.begin().await?;
    // Lifecycle records invalidate in-flight observations, including same-release
    // restarts. A pending control job also takes precedence over monitoring.
    let row = sqlx::query("SELECT i.health_status,i.health_failures FROM installations i WHERE i.id=$1 AND ($2::bigint IS NULL OR (i.health_generation=$2 AND i.release_id=$3 AND i.runtime_port=$4 AND i.runtime_status='running' AND i.desired_state='running' AND NOT EXISTS(SELECT 1 FROM runtime_jobs j WHERE j.app_id=i.app_id AND j.environment=i.environment AND j.status IN ('queued','running')))) FOR UPDATE OF i")
    .bind(id)
    .bind(snapshot.map(|s|s.generation))
    .bind(snapshot.map(|s|s.release))
    .bind(snapshot.map(|s|s.port))
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Ok(false);
    };
    let previous: String = row.get("health_status");
    let (status, failures) = transition(row.get("health_failures"), &observation);
    let error = match observation {
        Observation::Failed(code) | Observation::Unknown(code) => Some(code),
        _ => None,
    };
    let checked = matches!(observation, Observation::Healthy | Observation::Failed(_));
    sqlx::query("UPDATE installations SET health_status=$2,health_failures=$3,health_error=$4,health_checked_at=CASE WHEN $5 THEN now() ELSE NULL END,health_generation=health_generation+$6 WHERE id=$1")
        .bind(id).bind(status).bind(failures).bind(error).bind(checked).bind(i64::from(snapshot.is_none())).execute(&mut *tx).await?;
    if previous != status {
        sqlx::query("INSERT INTO audit(action,target,request_id) VALUES($1,$2,$3)")
            .bind(format!("runtime_health.{status}"))
            .bind(id.to_string())
            .bind(Uuid::new_v4().to_string())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    if previous != status {
        tracing::info!(event="runtime_health_changed",installation_id=%id,previous,status,error_code=error);
    }
    Ok(true)
}

pub struct Monitor(tokio::task::JoinHandle<()>);
impl Drop for Monitor {
    fn drop(&mut self) {
        self.0.abort();
    }
}
pub fn start(pool: sqlx::PgPool) -> Result<Monitor> {
    let client = client()?;
    Ok(Monitor(tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            if let Err(error) = scan(&pool, &client).await {
                tracing::warn!(event="runtime_health_scan_failed",error=%error);
            }
        }
    })))
}
// A scan does not overlap the previous scan. At most four requests are active,
// and lifecycle changes are rechecked under a row lock before accepting results.
pub async fn scan(pool: &sqlx::PgPool, client: &reqwest::Client) -> Result<()> {
    let rows = sqlx::query("SELECT i.id,i.health_generation,i.release_id,i.runtime_port FROM installations i WHERE i.runtime_status='running' AND i.desired_state='running' AND i.runtime_port IS NOT NULL AND NOT EXISTS(SELECT 1 FROM runtime_jobs j WHERE j.app_id=i.app_id AND j.environment=i.environment AND j.status IN ('queued','running')) ORDER BY i.health_checked_at NULLS FIRST,i.id").fetch_all(pool).await?;
    let mut work = futures_util::stream::iter(rows)
        .map(|row| async move {
            let snapshot = Snapshot {
                id: row.get("id"),
                generation: row.get("health_generation"),
                release: row.get("release_id"),
                port: row.get("runtime_port"),
            };
            let observation = match probe(client, snapshot.port, snapshot.release).await {
                Ok(()) => Observation::Healthy,
                Err(code) => Observation::Failed(code),
            };
            persist(pool, snapshot.id, Some(&snapshot), observation).await
        })
        .buffer_unordered(4);
    // One observation's failure must not cancel the other apps' checks.
    while let Some(result) = work.next().await {
        if let Err(error) = result {
            tracing::warn!(event="runtime_health_observation_failed",error=%error);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failures_recover_and_inactive_is_not_a_service_failure() {
        assert_eq!(transition(0, &Observation::Failed("503")), ("degraded", 1));
        assert_eq!(transition(1, &Observation::Failed("503")), ("degraded", 2));
        assert_eq!(transition(2, &Observation::Failed("503")), ("unhealthy", 3));
        assert_eq!(
            transition(i32::MAX, &Observation::Failed("503")),
            ("unhealthy", 3)
        );
        assert_eq!(transition(3, &Observation::Healthy), ("healthy", 0));
        assert_eq!(transition(3, &Observation::Inactive), ("not_applicable", 0));
        assert_eq!(
            transition(3, &Observation::Unknown("docker")),
            ("unknown", 0)
        );
    }
}
