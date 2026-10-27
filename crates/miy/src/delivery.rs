//! Bounded authenticated archive transfer. Only the worker can load Docker images.
pub use crate::image_contract::MAX_ARCHIVE;
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
    let request_id = req
        .headers()
        .get("x-miy-request-id")
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|v| Uuid::parse_str(v).ok())
                .ok_or_else(|| ApiError::bad("invalid_upload_request_id"))
        })
        .transpose()?;
    let root = root()?;
    tokio::fs::create_dir_all(&root).await?;
    let id = Uuid::new_v4();
    let mut tx = s.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("upload:{}", p.id))
        .execute(&mut *tx)
        .await?;
    if let Some(request_id) = request_id
        && let Some(row) =
            sqlx::query("SELECT * FROM image_uploads WHERE actor=$1 AND request_id=$2")
                .bind(&p.id)
                .bind(request_id)
                .fetch_optional(&mut *tx)
                .await?
    {
        if row.get::<String, _>("app_id") != app
            || row.get::<String, _>("image_id") != image
            || row.get::<String, _>("archive_hash") != expected_hash
            || row.get::<i64, _>("size_bytes") != size as i64
        {
            return Err(ApiError::conflict("idempotency_conflict"));
        }
        if row.get::<chrono::DateTime<chrono::Utc>, _>("expires_at") <= chrono::Utc::now() {
            return Err(ApiError::conflict("upload_expired"));
        }
        let receipt = view(&row);
        tx.commit().await?;
        return Ok(Json(receipt));
    }
    let pending:i64=sqlx::query_scalar("SELECT count(*) FROM image_uploads WHERE actor=$1 AND status IN ('receiving','pending','loading')").bind(&p.id).fetch_one(&mut *tx).await?;
    if pending >= 2 {
        return Err(ApiError::conflict("finish_pending_uploads_first"));
    }
    sqlx::query("INSERT INTO image_uploads(id,app_id,actor,image_id,size_bytes,archive_hash,status,request_id) VALUES($1,$2,$3,$4,$5,$6,'receiving',$7)")
        .bind(id).bind(&app).bind(&p.id).bind(&image).bind(size as i64).bind(&expected_hash).bind(request_id).execute(&mut *tx).await?;
    tx.commit().await?;
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
    status(S(s), axum::Extension(p), Path(id)).await
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
    if row.get::<String, _>("actor") != p.id {
        return Err(ApiError::forbidden());
    }
    if row.get::<chrono::DateTime<chrono::Utc>, _>("expires_at") <= chrono::Utc::now() {
        return Err(ApiError::conflict("upload_expired"));
    }
    Ok(Json(view(&row)))
}
fn view(row: &sqlx::postgres::PgRow) -> Value {
    json!({"id":row.get::<Uuid,_>("id"),"requestId":row.get::<Option<Uuid>,_>("request_id"),"appId":row.get::<String,_>("app_id"),"imageId":row.get::<String,_>("image_id"),"archiveHash":row.get::<String,_>("archive_hash"),"sizeBytes":row.get::<i64,_>("size_bytes"),"status":row.get::<String,_>("status"),"errorCode":row.get::<Option<String>,_>("error_code"),"expiresAt":row.get::<chrono::DateTime<chrono::Utc>,_>("expires_at")})
}
pub async fn by_request(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path((app, request)): Path<(String, Uuid)>,
) -> ApiResult<Json<Value>> {
    domain::manage(&s, &p, &app, "dev").await?;
    let id: Uuid = sqlx::query_scalar(
        "SELECT id FROM image_uploads WHERE actor=$1 AND request_id=$2 AND app_id=$3",
    )
    .bind(&p.id)
    .bind(request)
    .bind(app)
    .fetch_optional(&s.pool)
    .await?
    .ok_or_else(ApiError::not_found)?;
    status(S(s), axum::Extension(p), Path(id)).await
}

pub async fn tick(pool: &sqlx::PgPool) -> anyhow::Result<bool> {
    let expired:Vec<Uuid>=sqlx::query_scalar("UPDATE image_uploads SET status='failed',error_code='upload_expired' WHERE (status='receiving' AND created_at<now()-interval '6 minutes') OR (status IN ('receiving','pending') AND expires_at<=clock_timestamp()) RETURNING id").fetch_all(pool).await?;
    for id in expired {
        let _ = tokio::fs::remove_file(root()?.join(format!("{id}.tar"))).await;
    }

    let Some(row) = sqlx::query(
        "SELECT * FROM image_uploads WHERE status='pending' AND expires_at>clock_timestamp() ORDER BY created_at,id LIMIT 1",
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
        let identity = tokio::task::spawn_blocking(move || {
            crate::image_import::archive(&checked_path, &checked_app, Some(&checked_image))
        })
        .await??;
        crate::image_import::load(
            pool,
            &app,
            &image,
            &path,
            &identity,
            row.get("archive_hash"),
        )
        .await?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
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
    // Keep the archive until the durable receipt commits. A worker crash before
    // this point can repeat verification; after it the ready receipt is final.
    let _ = tokio::fs::remove_file(path).await;
    Ok(true)
}
