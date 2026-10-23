//! Bounded authenticated archive transfer. Only the worker can load Docker images.
use crate::{
    State,
    auth::Principal,
    domain,
    http::{ApiError, ApiResult},
    runtime,
};
use axum::{
    Json,
    extract::{Path, Request, State as S},
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;
pub const MAX_ARCHIVE: u64 = 1024 * 1024 * 1024;
fn root() -> anyhow::Result<std::path::PathBuf> {
    Ok(std::path::PathBuf::from(std::env::var("MIY_ARTIFACTS")?).join(".uploads"))
}
pub async fn upload(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path(app): Path<String>,
    req: Request,
) -> ApiResult<Json<Value>> {
    domain::manage(&s, &p, &app, "dev").await?;
    let image = req
        .headers()
        .get("x-miy-image")
        .and_then(|v| v.to_str().ok())
        .filter(|v| runtime::image_id(v))
        .ok_or_else(|| ApiError::bad("invalid_image_id"))?
        .to_string();
    let expected_hash = req
        .headers()
        .get("x-miy-archive-sha256")
        .and_then(|v| v.to_str().ok())
        .filter(|v| v.len() == 64 && v.bytes().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| ApiError::bad("invalid_archive_hash"))?
        .to_string();
    let size = req
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0 && *v <= MAX_ARCHIVE)
        .ok_or_else(|| ApiError::bad("archive_size_limit_1GiB"))?;
    let id = Uuid::new_v4();
    let mut tx = s.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("upload:{}", p.id))
        .execute(&mut *tx)
        .await?;
    let pending:i64=sqlx::query_scalar("SELECT count(*) FROM image_uploads WHERE actor=$1 AND status IN ('receiving','pending','loading')").bind(&p.id).fetch_one(&mut *tx).await?;
    if pending >= 2 {
        return Err(ApiError::conflict("finish_pending_uploads_first"));
    }
    sqlx::query("INSERT INTO image_uploads(id,app_id,actor,image_id,size_bytes,archive_hash,status) VALUES($1,$2,$3,$4,$5,$6,'receiving')")
        .bind(id).bind(&app).bind(&p.id).bind(&image).bind(size as i64).bind(&expected_hash).execute(&mut *tx).await?;
    tx.commit().await?;
    let root = root()?;
    tokio::fs::create_dir_all(&root).await?;
    let path = root.join(format!("{id}.tar"));
    let result = async {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .await?;
        let mut stream = req.into_body().into_data_stream();
        let mut count = 0u64;
        let mut hash = Sha256::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            count += chunk.len() as u64;
            anyhow::ensure!(count <= size, "archive_size_mismatch");
            hash.update(&chunk);
            file.write_all(&chunk).await?;
        }
        anyhow::ensure!(
            count == size && hex::encode(hash.finalize()) == expected_hash,
            "archive_hash_mismatch"
        );
        file.sync_all().await?;
        Ok::<_, anyhow::Error>(())
    };
    if !matches!(
        tokio::time::timeout(std::time::Duration::from_secs(300), result).await,
        Ok(Ok(()))
    ) {
        let _ = tokio::fs::remove_file(path).await;
        sqlx::query(
            "UPDATE image_uploads SET status='failed',error_code='invalid_archive' WHERE id=$1",
        )
        .bind(id)
        .execute(&s.pool)
        .await?;
        return Err(ApiError::bad("invalid_archive"));
    }
    sqlx::query("UPDATE image_uploads SET status='pending' WHERE id=$1")
        .bind(id)
        .execute(&s.pool)
        .await?;
    Ok(Json(json!({"id":id,"status":"pending","imageId":image})))
}
pub async fn status(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let row = sqlx::query("SELECT * FROM image_uploads WHERE id=$1")
        .bind(id)
        .fetch_optional(&s.pool)
        .await?
        .ok_or_else(ApiError::not_found)?;
    domain::manage(&s, &p, row.get("app_id"), "dev").await?;
    Ok(Json(
        json!({"id":id,"imageId":row.get::<String,_>("image_id"),"status":row.get::<String,_>("status"),"errorCode":row.get::<Option<String>,_>("error_code")}),
    ))
}
pub async fn tick(pool: &sqlx::PgPool) -> anyhow::Result<bool> {
    let expired:Vec<Uuid>=sqlx::query_scalar("UPDATE image_uploads SET status='failed',error_code='upload_expired' WHERE status='receiving' AND created_at<now()-interval '6 minutes' RETURNING id").fetch_all(pool).await?;
    for id in expired {
        let _ = tokio::fs::remove_file(root()?.join(format!("{id}.tar"))).await;
    }

    let Some(row) = sqlx::query(
        "SELECT * FROM image_uploads WHERE status='pending' ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(false);
    };
    let id: Uuid = row.get("id");
    let app: String = row.get("app_id");
    let image: String = row.get("image_id");
    let path = root()?.join(format!("{id}.tar"));
    sqlx::query("UPDATE image_uploads SET status='loading' WHERE id=$1")
        .bind(id)
        .execute(pool)
        .await?;
    let result = async {
        anyhow::ensure!(
            domain::can_manage(pool, row.get("actor"), &app, "dev").await?,
            "permission_revoked"
        );
        let mut file = tokio::fs::File::open(&path).await?;
        use tokio::io::AsyncReadExt;
        let mut hash = Sha256::new();
        let mut buffer = vec![0u8; 65536];
        let mut count = 0i64;
        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            count += n as i64;
            hash.update(&buffer[..n]);
        }
        anyhow::ensure!(
            count == row.get::<i64, _>("size_bytes")
                && hex::encode(hash.finalize()) == row.get::<String, _>("archive_hash"),
            "archive_hash_mismatch"
        );
        let checked_path = path.clone();
        let checked_app = app.clone();
        let checked_image = image.clone();
        tokio::task::spawn_blocking(move || {
            validate_archive(&checked_path, &checked_app, &checked_image)
        })
        .await??;
        runtime::docker(&["load", "--input", path.to_str().unwrap()]).await?;
        let meta = runtime::inspect(&image).await?;
        anyhow::ensure!(
            meta["Id"] == image && meta["Config"]["Labels"]["org.miy.app.id"] == app,
            "uploaded_image_mismatch"
        );
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let _ = tokio::fs::remove_file(path).await;
    sqlx::query("UPDATE image_uploads SET status=$2,error_code=$3 WHERE id=$1")
        .bind(id)
        .bind(if result.is_ok() { "ready" } else { "failed" })
        .bind(if result.is_ok() {
            None
        } else {
            Some("image_import_failed")
        })
        .execute(pool)
        .await?;
    Ok(true)
}

// Validate the single-image envelope without extracting any archive path on the host.
fn validate_archive(path: &std::path::Path, app: &str, image: &str) -> anyhow::Result<()> {
    use std::io::Read;
    let mut archive = tar::Archive::new(std::fs::File::open(path)?);
    let mut json_files = std::collections::BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let name = entry.path()?.to_string_lossy().into_owned();
        anyhow::ensure!(
            seen.insert(name.clone()) && seen.len() <= 4096,
            "invalid_archive_entries"
        );
        anyhow::ensure!(
            crate::contract::safe_relative(name.trim_end_matches('/')),
            "invalid_archive_path"
        );
        anyhow::ensure!(
            entry.header().entry_type().is_file() || entry.header().entry_type().is_dir(),
            "invalid_archive_entry_type"
        );
        if entry.header().entry_type().is_file() && entry.size() <= 1024 * 1024 {
            let mut data = Vec::new();
            entry.read_to_end(&mut data)?;
            if let Ok(value) = serde_json::from_slice::<Value>(&data) {
                json_files.insert(name, (value, crate::contract::hash(&data)));
            }
        }
    }
    let manifest = &json_files
        .get("manifest.json")
        .ok_or_else(|| anyhow::anyhow!("missing_image_manifest"))?
        .0;
    anyhow::ensure!(
        manifest.as_array().is_some_and(|v| v.len() == 1),
        "upload_must_contain_one_image"
    );
    let config = manifest[0]["Config"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing_image_config"))?;
    let (config_json, config_hash) = json_files
        .get(config)
        .ok_or_else(|| anyhow::anyhow!("missing_image_config"))?;
    anyhow::ensure!(
        config_json["config"]["Labels"]["org.miy.app.id"] == app,
        "image_app_mismatch"
    );
    if let Some((index, _)) = json_files.get("index.json") {
        anyhow::ensure!(
            index["manifests"].as_array().is_some_and(|v| v.len() == 1)
                && index["manifests"][0]["digest"] == image,
            "image_digest_mismatch"
        );
        let blob = format!(
            "blobs/sha256/{}",
            image.strip_prefix("sha256:").unwrap_or_default()
        );
        let (_, digest) = json_files
            .get(&blob)
            .ok_or_else(|| anyhow::anyhow!("missing_image_index"))?;
        anyhow::ensure!(format!("sha256:{digest}") == image, "image_digest_mismatch");
    } else {
        anyhow::ensure!(
            format!("sha256:{config_hash}") == image,
            "image_digest_mismatch"
        );
    }
    Ok(())
}
