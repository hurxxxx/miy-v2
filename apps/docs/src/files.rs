//! Core Storage bookkeeping shared by page attachments and body images.
//!
//! AI-DO collected object-store keys while trashing a page or doc and removed
//! the objects after the commit (`cleanup_media_for_resource`,
//! `collect_page_attachment_storage_keys`, `_remove_docs_storage_objects`).
//! miy keeps the same order with the core file API (CONTRACT "Small
//! attachments"): the app reference is removed and the detach work recorded
//! (`status = 'detaching'`) in the business transaction, and the core file is
//! detached (or cancelled, when it never got attached) after the commit. Work
//! the core did not confirm is finished by the next release.
use miy_app_sdk::{FileStatus, Identity};
use sqlx::{PgPool, Postgres, Transaction};

/// Which table a released file belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Attachment,
    Media,
}

impl Kind {
    fn table(self) -> &'static str {
        match self {
            Self::Attachment => "docs_page_attachments",
            Self::Media => "media_files",
        }
    }

    pub fn reference(self, id: &str) -> String {
        match self {
            Self::Attachment => format!("docs_attachment:{id}"),
            Self::Media => format!("docs_media:{id}"),
        }
    }
}

/// A file released by a deleted page, doc or attachment (status `detaching`).
#[derive(Clone, Debug)]
pub struct Released {
    pub kind: Kind,
    pub id: String,
    pub file_id: String,
}

/// Body images and attachments of these pages, released with them.
pub async fn release_for_pages(
    tx: &mut Transaction<'_, Postgres>,
    page_ids: &[String],
) -> sqlx::Result<Vec<Released>> {
    if page_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut released = Vec::new();
    let media: Vec<(String, String)> = sqlx::query_as(
        "UPDATE media_files SET resource_type = NULL, resource_id = NULL, status = 'detaching' \
         WHERE resource_type = 'docs_native_page' AND resource_id = ANY($1) \
           AND file_id IS NOT NULL AND status IN ('ready', 'attached') \
         RETURNING id, file_id",
    )
    .bind(page_ids)
    .fetch_all(&mut **tx)
    .await?;
    released.extend(media.into_iter().map(|(id, file_id)| Released {
        kind: Kind::Media,
        id,
        file_id,
    }));
    released.extend(release_attachments_where(tx, "page_id = ANY($1)", page_ids).await?);
    // Attachments that never got bytes have nothing to detach; drop their rows
    // with the page (AI-DO deleted every attachment row of a trashed page).
    sqlx::query(
        "UPDATE docs_page_attachments SET status = 'detached' \
         WHERE page_id = ANY($1) AND status = 'pending'",
    )
    .bind(page_ids)
    .execute(&mut **tx)
    .await?;
    Ok(released)
}

async fn release_attachments_where<T>(
    tx: &mut Transaction<'_, Postgres>,
    condition: &str,
    value: T,
) -> sqlx::Result<Vec<Released>>
where
    T: for<'q> sqlx::Encode<'q, Postgres> + sqlx::Type<Postgres> + Send,
{
    let sql = format!(
        "UPDATE docs_page_attachments SET status = 'detaching' \
         WHERE {condition} AND file_id IS NOT NULL AND status IN ('ready', 'attached') \
         RETURNING id, file_id"
    );
    let rows: Vec<(String, String)> = sqlx::query_as(&sql)
        .bind(value)
        .fetch_all(&mut **tx)
        .await?;
    Ok(rows
        .into_iter()
        .map(|(id, file_id)| Released {
            kind: Kind::Attachment,
            id,
            file_id,
        })
        .collect())
}

/// One attachment removed by its own delete request.
pub async fn release_attachment(
    tx: &mut Transaction<'_, Postgres>,
    attachment_id: &str,
) -> sqlx::Result<Vec<Released>> {
    let released = release_attachments_where(tx, "id = $1", attachment_id).await?;
    sqlx::query(
        "UPDATE docs_page_attachments SET status = 'detached' WHERE id = $1 AND status = 'pending'",
    )
    .bind(attachment_id)
    .execute(&mut **tx)
    .await?;
    Ok(released)
}

/// Detach a file when attached, cancel it when it never was. Repeating is
/// safe; an uncertain result is looked up on the next attempt.
async fn release_in_core(identity: &Identity, item: &Released) -> bool {
    let Ok(file) = identity.files_get(&item.file_id).await else {
        return false;
    };
    let result = match file.status {
        FileStatus::Attached => identity
            .files_detach(&item.file_id, &item.kind.reference(&item.id))
            .await
            .map(|_| ()),
        FileStatus::Ready | FileStatus::Pending => {
            identity.files_cancel(&item.file_id).await.map(|_| ())
        }
        FileStatus::Cancelled | FileStatus::Expired => Ok(()),
    };
    result.is_ok()
}

async fn mark_detached(pool: &PgPool, item: &Released) {
    let sql = format!(
        "UPDATE {} SET status = 'detached' WHERE id = $1 AND status = 'detaching'",
        item.kind.table()
    );
    let _ = sqlx::query(&sql).bind(&item.id).execute(pool).await;
}

/// After the app commit: release the files, then clear their recorded work.
/// Platform calls never get here (they change no data).
pub async fn detach_released(pool: &PgPool, identity: &Identity, released: &[Released]) {
    if identity.context.platform {
        return;
    }
    for item in released {
        if release_in_core(identity, item).await {
            mark_detached(pool, item).await;
        }
    }
    retry_pending(pool, identity, released).await;
}

/// Finish detach work an earlier request left unconfirmed (at most ten files
/// of each kind per request; the core keeps their quota until then).
async fn retry_pending(pool: &PgPool, identity: &Identity, skip: &[Released]) {
    for kind in [Kind::Attachment, Kind::Media] {
        let skip_ids: Vec<String> = skip
            .iter()
            .filter(|item| item.kind == kind)
            .map(|item| item.id.clone())
            .collect();
        let sql = format!(
            "SELECT id, file_id FROM {} WHERE status = 'detaching' AND file_id IS NOT NULL \
             AND NOT (id = ANY($1)) ORDER BY created_at LIMIT 10",
            kind.table()
        );
        let Ok(rows) = sqlx::query_as::<_, (String, String)>(&sql)
            .bind(&skip_ids)
            .fetch_all(pool)
            .await
        else {
            continue;
        };
        for (id, file_id) in rows {
            let item = Released { kind, id, file_id };
            if release_in_core(identity, &item).await {
                mark_detached(pool, &item).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_references_name_the_app_row() {
        assert_eq!(Kind::Attachment.reference("a1"), "docs_attachment:a1");
        assert_eq!(Kind::Media.reference("m1"), "docs_media:m1");
    }
}
