use crate::{AppState, files, scene, versions, whiteboard};
use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use miy_app_collab::{
    Loaded, Save, Store, db, document, snapshot,
    yrs::{Doc, Map, Transact},
};
use miy_app_sdk::Identity;
use serde_json::{Map as JsonMap, Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

pub async fn space(state: &AppState, key: &str) -> crate::error::ApiResult<String> {
    sqlx::query_scalar("SELECT workspace_id FROM whiteboards WHERE id=$1")
        .bind(key)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| {
            crate::error::ApiError::not_found("whiteboard_not_found", "Board unavailable")
        })
}
pub fn seed(scene: &Value) -> Result<Vec<u8>> {
    let doc = Doc::new();
    let elements = doc.get_or_insert_map("elements");
    let files = doc.get_or_insert_map("files");
    let settings = doc.get_or_insert_map("settings");
    {
        let mut tx = doc.transact_mut();
        for (pos, element) in scene["elements"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let id = element["id"].as_str().context("missing element id")?;
            elements.insert(
                &mut tx,
                format!("seed:{id}"),
                json!({"pos":pos,"element":element}).to_string(),
            );
        }
        for (key, value) in scene["files"].as_object().into_iter().flatten() {
            files.insert(&mut tx, key.as_str(), value.to_string());
        }
        for key in scene::PERSISTED_APP_STATE_KEYS {
            if let Some(v) = scene["appState"].get(key) {
                settings.insert(&mut tx, key, v.to_string());
            }
        }
    }
    Ok(snapshot(&doc))
}
fn rank(item: &Value) -> (i64, i64, i64) {
    let e = &item["element"];
    (
        e["version"].as_i64().unwrap_or(0),
        e["updated"].as_i64().unwrap_or(0),
        e["versionNonce"].as_i64().unwrap_or(0),
    )
}
pub fn project(bytes: &[u8]) -> Result<Value> {
    let doc = document(bytes)?;
    let elements = doc.get_or_insert_map("elements");
    let files = doc.get_or_insert_map("files");
    let settings = doc.get_or_insert_map("settings");
    let tx = doc.transact();
    let mut winners = BTreeMap::<String, (String, Value)>::new();
    for (key, v) in elements.iter(&tx) {
        let item: Value = serde_json::from_str(&v.to_string(&tx))?;
        let id = item["element"]["id"]
            .as_str()
            .context("missing element id")?
            .to_string();
        if winners.get(&id).is_none_or(|(old_key, existing)| {
            (rank(&item), key) > (rank(existing), old_key.as_str())
        }) {
            winners.insert(id, (key.to_owned(), item));
        }
    }
    let mut elements: Vec<_> = winners.into_values().map(|(_, v)| v).collect();
    elements.sort_by(|a, b| {
        let order = |item: &Value| {
            (
                item["element"]["index"].as_str().unwrap_or("").to_owned(),
                item["pos"].as_u64().unwrap_or(0),
                item["element"]["id"].to_string(),
            )
        };
        order(a).cmp(&order(b))
    });
    let mut refs = JsonMap::new();
    let mut app_state = JsonMap::new();
    for (k, v) in files.iter(&tx) {
        refs.insert(k.into(), serde_json::from_str(&v.to_string(&tx))?);
    }
    for (k, v) in settings.iter(&tx) {
        if scene::PERSISTED_APP_STATE_KEYS.contains(&k) {
            app_state.insert(k.into(), serde_json::from_str(&v.to_string(&tx))?);
        }
    }
    Ok(
        json!({"elements":elements.iter().map(|e|&e["element"]).collect::<Vec<_>>(),"appState":app_state,"files":refs}),
    )
}
#[async_trait]
impl Store for AppState {
    async fn authorize(&self, key: &str, who: &Identity) -> Result<()> {
        let space = space(self, key).await.map_err(|e| {
            if matches!(e.status.as_u16(), 403 | 404) {
                anyhow::anyhow!("access_revoked")
            } else {
                e.into()
            }
        })?;
        let board = whiteboard::load_for_user(self, who, &space, key)
            .await
            .map_err(|e| {
                if matches!(e.status.as_u16(), 403 | 404) {
                    anyhow::anyhow!("access_revoked")
                } else {
                    e.into()
                }
            })?;
        if !board.access.can_edit || board.board.trashed_at.is_some() {
            anyhow::bail!("access_revoked");
        }
        Ok(())
    }
    async fn validate(&self, _key: &str, candidate: &[u8]) -> Result<()> {
        let scene = project(candidate).map_err(|_| anyhow::anyhow!("invalid_update"))?;
        whiteboard::check_scene_files(&self.pool, Some(_key), &scene)
            .await
            .map_err(|e| {
                if e.status.is_client_error() {
                    anyhow::anyhow!("invalid_update")
                } else {
                    e.into()
                }
            })?;
        Ok(())
    }
    async fn generation(&self, key: &str) -> Result<Option<Uuid>> {
        Ok(
            sqlx::query_scalar("SELECT generation FROM collab_rooms WHERE resource_id=$1")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?,
        )
    }
    async fn load(&self, key: &str, who: &Identity, owner: Uuid) -> Result<Loaded> {
        let space = space(self, key).await?;
        let mut tx = self.pool.begin().await?;
        let b = whiteboard::lock_for_user(&mut tx, self, who, &space, key).await?;
        ensure!(
            b.access.can_edit && b.board.trashed_at.is_none(),
            "access_revoked"
        );
        let scene: Value = sqlx::query_scalar("SELECT scene FROM whiteboards WHERE id=$1")
            .bind(key)
            .fetch_one(&mut *tx)
            .await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM collab_rooms WHERE resource_id=$1)")
                .bind(key)
                .fetch_one(&mut *tx)
                .await?;
        let initial = if exists { vec![] } else { seed(&scene)? };
        let loaded = db::acquire(&mut tx, key, owner, &initial, b.board.revision).await?;
        tx.commit().await?;
        Ok(loaded)
    }
    async fn persist(&self, save: Save<'_>) -> Result<i64> {
        let mut tx = self.pool.begin().await?;
        let mut board: whiteboard::Board = sqlx::query_as(&format!(
            "SELECT {} FROM whiteboards WHERE id=$1 FOR UPDATE",
            whiteboard::BOARD_COLUMNS
        ))
        .bind(save.resource)
        .fetch_one(&mut *tx)
        .await?;
        db::claim_save(
            &mut tx,
            save.resource,
            save.owner,
            save.generation,
            save.state,
            save.release,
        )
        .await?;
        let mut refs = vec![];
        if save.state.is_some() || save.checkpoint {
            let mut stored: Value = sqlx::query_scalar("SELECT scene FROM whiteboards WHERE id=$1")
                .bind(save.resource)
                .fetch_one(&mut *tx)
                .await?;
            if let Some(bytes) = save.state {
                let next = project(bytes)?;
                refs = whiteboard::check_scene_files(&mut *tx, Some(save.resource), &next).await?;
                if !scene::matches(&stored, &next) {
                    versions::baseline(&mut tx, &board, &stored, &save.author.user_id).await?;
                    board.revision=sqlx::query_scalar("UPDATE whiteboards SET scene=$2,revision=revision+1,updated_at=timezone('UTC',now()) WHERE id=$1 RETURNING revision")
                        .bind(save.resource).bind(&next).fetch_one(&mut *tx).await?;
                    stored = next;
                }
            }
            if save.checkpoint {
                versions::checkpoint(&mut tx, &board, &stored, &save.author.user_id).await?;
            }
        }
        tx.commit().await?;
        if !refs.is_empty() {
            files::attach_referenced(&self.pool, save.author, save.resource, &refs).await;
        }
        Ok(board.revision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miy_app_collab::yrs::{ReadTxn, StateVector, Update, updates::decoder::Decode};

    #[test]
    fn concurrent_element_edits_converge_in_either_delivery_order() {
        let initial=seed(&json!({"elements":[{"id":"base","index":"a0","version":1}],"files":{},"appState":{"gridModeEnabled":true}})).unwrap();
        let branch = |actor: &str, nonce: i64, delete: bool| {
            let doc = document(&initial).unwrap();
            let vector = doc.transact().state_vector();
            let elements = doc.get_or_insert_map("elements");
            let mut tx = doc.transact_mut();
            for (id, index) in [(actor, actor), ("base", "a0")] {
                elements.insert(
                    &mut tx,
                    format!("{actor}:{id}"),
                    json!({"pos":0,"element":{
                        "id":id,"index":index,"version":2,"updated":123,"versionNonce":nonce,
                        "isDeleted":id=="base" && delete
                    }})
                    .to_string(),
                );
            }
            tx.encode_state_as_update_v1(&vector)
        };
        let a = branch("a1", 11, false);
        let b = branch("a2", 12, true);
        let merge = |parts: [&[u8]; 3]| {
            let doc = document(&initial).unwrap();
            for part in parts {
                doc.transact_mut()
                    .apply_update(Update::decode_v1(part).unwrap())
                    .unwrap();
            }
            project(
                &doc.transact()
                    .encode_state_as_update_v1(&StateVector::default()),
            )
            .unwrap()
        };
        let scene = merge([&a, &b, &a]);
        assert_eq!(scene, merge([&b, &a, &b]));
        let elements = scene["elements"].as_array().unwrap();
        assert_eq!(
            elements
                .iter()
                .map(|e| e["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["base", "a1", "a2"]
        );
        assert_eq!(elements[0]["isDeleted"], true);
        assert_eq!(scene["appState"]["gridModeEnabled"], true);
    }
}
