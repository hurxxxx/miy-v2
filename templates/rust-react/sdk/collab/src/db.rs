//! The caller locks the business resource first. A room's owner lease fences native
//! previews sharing a dev DB and crashed app processes. Generation fences replacements.
use crate::Loaded;
use anyhow::{Result, ensure};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

pub async fn acquire(
    tx: &mut Transaction<'_, Postgres>,
    key: &str,
    owner: Uuid,
    initial: &[u8],
    revision: i64,
) -> Result<Loaded> {
    let row=sqlx::query("INSERT INTO collab_rooms(resource_id,generation,owner,lease_until,state) VALUES($1,$2,$3,now()+interval '30 seconds',$4)
        ON CONFLICT(resource_id) DO UPDATE SET owner=excluded.owner,lease_until=excluded.lease_until
        WHERE collab_rooms.owner IS NULL OR collab_rooms.owner=$3 OR collab_rooms.lease_until<=now()
        RETURNING generation,state").bind(key).bind(Uuid::new_v4()).bind(owner).bind(initial)
        .fetch_optional(&mut **tx).await?.ok_or_else(||anyhow::anyhow!("room_busy"))?;
    Ok(Loaded {
        generation: row.get("generation"),
        bytes: row.get("state"),
        revision,
    })
}

pub async fn claim_save(
    tx: &mut Transaction<'_, Postgres>,
    key: &str,
    owner: Uuid,
    generation: Uuid,
    bytes: Option<&[u8]>,
    release: bool,
) -> Result<()> {
    let updated=sqlx::query("UPDATE collab_rooms SET state=coalesce($4,state), saved_at=CASE WHEN $4 IS NULL THEN saved_at ELSE now() END,
       owner=CASE WHEN $5 THEN NULL ELSE owner END,lease_until=now()+interval '30 seconds'
       WHERE resource_id=$1 AND owner=$2 AND generation=$3")
       .bind(key).bind(owner).bind(generation).bind(bytes).bind(release).execute(&mut **tx).await?;
    ensure!(updated.rows_affected() == 1, "generation_changed");
    Ok(())
}

/// Called under the business resource row lock in the same transaction as a replacement.
pub async fn invalidate(tx: &mut Transaction<'_, Postgres>, key: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM collab_rooms WHERE resource_id=$1")
        .bind(key)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
