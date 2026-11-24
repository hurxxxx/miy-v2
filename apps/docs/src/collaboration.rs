use crate::{AppState, access, block_codec, media, pages, versions};
use anyhow::{Result, ensure};
use async_trait::async_trait;
use miy_app_collab::{Loaded, Save, Store, db};
use miy_app_sdk::Identity;
use serde_json::{Value, json};
use uuid::Uuid;

#[async_trait]
impl Store for AppState {
    async fn authorize(&self, key: &str, who: &Identity) -> Result<()> {
        let mut viewer = access::Viewer::new(self, who);
        let context = access::page_or_403(&mut viewer, key, access::Require::Edit)
            .await
            .map_err(|e| {
                if matches!(e.status.as_u16(), 403 | 404) {
                    anyhow::anyhow!("access_revoked")
                } else {
                    e.into()
                }
            })?;
        ensure!(context.page.content_format == "block", "access_revoked");
        Ok(())
    }
    async fn validate(&self, _key: &str, candidate: &[u8]) -> Result<()> {
        let blocks = block_codec::project(candidate).await?;
        ensure!(
            blocks.to_string().len() <= 4 * 1024 * 1024,
            "invalid_update"
        );
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
        let mut tx = self.pool.begin().await?;
        let (_, page) = pages::lock_page_for_edit(&mut tx, self, who, key).await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM collab_rooms WHERE resource_id=$1)")
                .bind(key)
                .fetch_one(&mut *tx)
                .await?;
        let initial = if exists {
            vec![]
        } else {
            block_codec::seed(&page.content_blocks.clone().unwrap_or_else(|| json!([]))).await?
        };
        let loaded = db::acquire(&mut tx, key, owner, &initial, page.revision).await?;
        tx.commit().await?;
        Ok(loaded)
    }
    async fn persist(&self, save: Save<'_>) -> Result<i64> {
        let blocks = match save.state {
            Some(bytes) => Some(block_codec::project(bytes).await?),
            None => None,
        };
        let mut tx = self.pool.begin().await?;
        // Accepted edits retain their verified author while the last connection
        // drains. An intervening replacement atomically invalidates the fence.
        let doc_id: String =
            sqlx::query_scalar("SELECT doc_id FROM docs_native_doc_pages WHERE id=$1")
                .bind(save.resource)
                .fetch_one(&mut *tx)
                .await?;
        let doc: access::NativeDoc = sqlx::query_as(&format!(
            "SELECT {} FROM docs_native_docs WHERE id=$1 FOR UPDATE",
            access::DOC_COLUMNS
        ))
        .bind(doc_id)
        .fetch_one(&mut *tx)
        .await?;
        let mut page: access::NativePage = sqlx::query_as(&format!(
            "SELECT {} FROM docs_native_doc_pages WHERE id=$1 FOR UPDATE",
            access::PAGE_COLUMNS
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
        let mut linked = Vec::new();
        if let Some(blocks) = blocks {
            if !same_blocks(page.content_blocks.as_ref(), Some(&blocks)) {
                (page, linked) = pages::write_projection(
                    &mut tx,
                    &doc,
                    &page,
                    &Some(blocks),
                    &save.author.user_id,
                )
                .await?;
            }
        }
        if save.checkpoint {
            checkpoint(&mut tx, &page, &save.author.user_id).await?;
        }
        tx.commit().await?;
        if !linked.is_empty() {
            media::attach_linked(&self.pool, save.author, &linked).await;
        }
        Ok(page.revision)
    }
}
fn same_blocks(a: Option<&Value>, b: Option<&Value>) -> bool {
    fn empty(value: Option<&Value>) -> bool {
        let Some(blocks) = value.and_then(Value::as_array) else {
            return value.is_none();
        };
        if blocks.is_empty() {
            return true;
        }
        if let [block] = blocks.as_slice() {
            return block["id"] == "initialBlockId"
                && block["type"] == "paragraph"
                && block["content"] == json!([])
                && block["children"] == json!([])
                && block["props"].as_object().is_some_and(|props| {
                    props.iter().all(|(key, value)| match key.as_str() {
                        "backgroundColor" | "textColor" => value == "default",
                        "textAlignment" => value == "left",
                        _ => false,
                    })
                });
        }
        false
    }
    a == b || (empty(a) && empty(b))
}

pub async fn checkpoint(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    page: &access::NativePage,
    author: &str,
) -> Result<(), crate::error::ApiError> {
    let last:Option<(Option<Value>,)>=sqlx::query_as("SELECT content_blocks FROM docs_page_versions WHERE page_id=$1 ORDER BY revision DESC LIMIT 1")
        .bind(&page.id).fetch_optional(&mut **tx).await?;
    if last
        .as_ref()
        .is_some_and(|p| same_blocks(p.0.as_ref(), page.content_blocks.as_ref()))
    {
        return Ok(());
    }
    versions::record(tx, page, author, None).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn undo_to_initial_paragraph_preserves_the_original_version() {
        let paragraph = json!([{"id":"initialBlockId","type":"paragraph","content":[],"children":[],"props":{
            "backgroundColor":"default","textColor":"default","textAlignment":"left"}}]);
        assert!(same_blocks(Some(&json!([])), Some(&paragraph)));
        let mut styled = paragraph;
        styled[0]["props"]["textAlignment"] = json!("right");
        assert!(!same_blocks(None, Some(&styled)));
    }
}
