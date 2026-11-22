//! Keep the exact build inputs associated with an immutable release's build receipt.
use crate::{
    State,
    auth::Principal,
    http::{ApiError, ApiResult},
    policy::{self, AppAction},
    source_archive::MAX_SOURCE_BYTES,
};
use anyhow::{Context, Result, ensure};
use axum::{
    Extension, Json,
    body::{Body, Bytes},
    extract::{Path, Request, State as S},
    http::{StatusCode, header},
    response::Response,
};
use futures_util::StreamExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row, postgres::PgRow};
use std::{
    fs::{self, File},
    io::{self, Read, Seek},
    os::unix::fs::DirBuilderExt,
    path::{Path as FsPath, PathBuf},
    time::Duration,
};
use tempfile::NamedTempFile;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Semaphore,
};
use uuid::Uuid;

const TRANSFER_TIMEOUT: Duration = Duration::from_secs(300);
static UPLOADS: Semaphore = Semaphore::const_new(8);

struct Source {
    commit: String,
    hash: String,
    size: u64,
}

async fn receipt(pool: &PgPool, app: &str, release: Uuid) -> ApiResult<Source> {
    // Multiple build requests can refer to the same image. Accept only a single
    // byte identity for the immutable release, including receipts from older workers.
    let mut rows = sqlx::query(
        "SELECT DISTINCT b.source_commit,b.archive_hash,b.size_bytes
         FROM releases r JOIN app_builds b ON b.app_id=r.app_id AND b.image_id=r.image_id
              AND b.source_commit=r.source_commit AND b.version=r.version AND b.status='succeeded'
         WHERE r.id=$1 AND r.app_id=$2 LIMIT 2",
    )
    .bind(release)
    .bind(app)
    .fetch_all(pool)
    .await?;
    if rows.len() != 1 {
        return Err(missing("release_source_receipt_unavailable"));
    }
    let row = rows.pop().expect("one receipt row");
    Ok(Source {
        commit: row.get("source_commit"),
        hash: row.get("archive_hash"),
        size: row.get::<i64, _>("size_bytes") as u64,
    })
}

fn missing(code: &str) -> ApiError {
    ApiError {
        status: StatusCode::NOT_FOUND,
        code: code.into(),
    }
}

fn path(root: &FsPath, app: &str, hash: &str) -> Result<PathBuf> {
    ensure!(crate::contract::valid_id(app), "invalid_app_id");
    ensure!(
        crate::source_archive::valid_digest(hash),
        "invalid_archive_hash"
    );
    Ok(root.join("sources").join(app).join(format!("{hash}.tar")))
}

fn prepare(destination: &FsPath) -> Result<&FsPath> {
    let parent = destination.parent().context("invalid_source_path")?;
    // Each directory is created privately. Validate the parts added beneath the
    // already-private build root, including pre-existing paths.
    let sources = parent.parent().context("invalid_source_path")?;
    fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(parent)?;
    for directory in [sources, parent] {
        ensure!(
            fs::symlink_metadata(directory)?.is_dir(),
            "invalid_source_directory"
        );
    }
    Ok(parent)
}

/// `Ok(None)` means a regular file whose bytes fail the receipt check.
fn open_verified(path: &FsPath, size: u64, hash: &str) -> io::Result<Option<File>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid_source_path",
        ));
    }
    if metadata.len() != size || !(1..=MAX_SOURCE_BYTES).contains(&size) {
        return Ok(None);
    }
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let read = io::copy(&mut (&mut file).take(size + 1), &mut digest)?;
    if read != size || hex::encode(digest.finalize()) != hash {
        return Ok(None);
    }
    file.rewind()?;
    Ok(Some(file))
}

enum ArchiveInput<'a> {
    Build(&'a FsPath),
    Recovery(NamedTempFile),
}

fn publish(input: ArchiveInput<'_>, destination: &FsPath, hash: &str, size: u64) -> Result<()> {
    let parent = prepare(destination)?;
    let path = match &input {
        ArchiveInput::Build(path) => path,
        ArchiveInput::Recovery(temp) => temp.path(),
    };
    match fs::hard_link(path, destination) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            match (input, open_verified(destination, size, hash)?) {
                (_, Some(_)) => {}
                (ArchiveInput::Recovery(temp), None) => {
                    // The recovery handler already verified and synced these exact
                    // receipt bytes in the destination directory. Publish atomically.
                    temp.persist(destination).map_err(|error| error.error)?;
                }
                (ArchiveInput::Build(_), None) => anyhow::bail!("release_source_bytes_invalid"),
            }
        }
        Err(error) => return Err(error.into()),
    }
    // Persist the app, sources and build directories up to the artifacts root.
    for directory in parent.ancestors().take(4) {
        File::open(directory)?.sync_all()?;
    }
    Ok(())
}

pub async fn retain_build(archive: &FsPath, row: &PgRow) -> Result<()> {
    let hash: String = row.get("archive_hash");
    let size = row.get::<i64, _>("size_bytes") as u64;
    let root = archive.parent().context("invalid_build_root")?;
    let destination = path(root, row.get("app_id"), &hash)?;
    let archive = archive.to_owned();
    tokio::task::spawn_blocking(move || {
        publish(ArchiveInput::Build(&archive), &destination, &hash, size)
    })
    .await?
}

async fn access(s: &State, p: &Principal, app: &str) -> ApiResult<()> {
    crate::auth_sessions::ensure_active(&s.pool, &p.id).await?;
    policy::require(s, p, app, "dev", AppAction::Develop).await
}

pub async fn download(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path((app, release)): Path<(String, Uuid)>,
) -> ApiResult<Response> {
    access(&s, &p, &app).await?;
    let Source { hash, size, .. } = receipt(&s.pool, &app, release).await?;
    let archive = path(&s.artifacts.join(".builds"), &app, &hash)?;
    let expected = hash.clone();
    let file = tokio::task::spawn_blocking(move || open_verified(&archive, size, &expected))
        .await
        .context("release_source_open_failed")?
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => missing("release_source_missing"),
            _ => ApiError::unavailable("release_source_bytes_invalid"),
        })?
        .ok_or_else(|| ApiError::unavailable("release_source_bytes_invalid"))?;
    access(&s, &p, &app).await?;
    let stream = futures_util::stream::try_unfold(
        (tokio::fs::File::from_std(file), size),
        |(mut file, remaining)| async move {
            if remaining == 0 {
                return Ok::<_, io::Error>(None);
            }
            let mut bytes = vec![0; remaining.min(64 * 1024) as usize];
            file.read_exact(&mut bytes).await?;
            let remaining = remaining - bytes.len() as u64;
            Ok(Some((Bytes::from(bytes), (file, remaining))))
        },
    );
    let mut response = Response::new(Body::from_stream(stream));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/x-tar"),
    );
    headers.insert(header::CONTENT_LENGTH, header::HeaderValue::from(size));
    headers.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{app}-{release}.tar\"")
            .parse()
            .expect("validated app and UUID"),
    );
    headers.insert(
        "x-miy-archive-sha256",
        hash.parse().expect("validated SHA-256"),
    );
    Ok(response)
}

pub async fn upload(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path((app, release)): Path<(String, Uuid)>,
    req: Request,
) -> ApiResult<Json<Value>> {
    access(&s, &p, &app).await?;
    let Source { commit, hash, size } = receipt(&s.pool, &app, release).await?;
    let mut sizes = req.headers().get_all(header::CONTENT_LENGTH).iter();
    if sizes
        .next()
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        != Some(size)
        || sizes.next().is_some()
    {
        return Err(ApiError::conflict("release_source_size_mismatch"));
    }
    let _slot = UPLOADS
        .try_acquire()
        .map_err(|_| ApiError::unavailable("release_source_upload_busy"))?;
    let destination = path(&s.artifacts.join(".builds"), &app, &hash)?;
    let temp_path = destination.clone();
    let temp = tokio::task::spawn_blocking(move || -> Result<_> {
        Ok(NamedTempFile::new_in(prepare(&temp_path)?)?)
    })
    .await
    .context("release_source_temp_failed")??;
    let mut file = tokio::fs::File::from_std(temp.reopen()?);
    let mut body = req.into_body().into_data_stream();
    let mut digest = Sha256::new();
    let mut bytes = 0;
    tokio::time::timeout(TRANSFER_TIMEOUT, async {
        while let Some(chunk) = body.next().await {
            let chunk = chunk.map_err(|_| ApiError::bad("release_source_transfer_failed"))?;
            bytes += chunk.len() as u64;
            if bytes > size {
                return Err(ApiError::conflict("release_source_size_mismatch"));
            }
            digest.update(&chunk);
            file.write_all(&chunk).await?;
        }
        Ok(())
    })
    .await
    .map_err(|_| ApiError::bad("release_source_transfer_timeout"))??;
    if bytes != size || hex::encode(digest.finalize()) != hash {
        return Err(ApiError::conflict("release_source_digest_mismatch"));
    }
    file.sync_all().await?;
    drop(file);

    // Decide again after transfer. Only these verified receipt bytes may be
    // published, including when repairing an existing corrupt archive.
    let archived_hash = hash.clone();
    let archived_app = app.clone();
    tokio::spawn(async move {
        crate::auth_sessions::ensure_active(&s.pool, &p.id).await?;
        let auth = policy::Write::begin(&s.pool)
            .await?
            .decide(&p, &archived_app, "dev", AppAction::Develop)
            .await?;
        tokio::task::spawn_blocking(move || {
            publish(
                ArchiveInput::Recovery(temp),
                &destination,
                &archived_hash,
                size,
            )
        })
        .await
        .context("release_source_publish_failed")??;
        auth.commit().await
    })
    .await
    .context("release_source_publish_failed")??;
    Ok(Json(
        json!({"appId":app,"releaseId":release,"sourceCommit":commit,"archiveSha256":hash,"sizeBytes":size}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Write,
        os::unix::fs::{MetadataExt, symlink},
    };

    fn archive(destination: &FsPath, bytes: &[u8]) -> Result<NamedTempFile> {
        let mut input = NamedTempFile::new_in(prepare(destination)?)?;
        input.write_all(bytes)?;
        input.as_file().sync_all()?;
        Ok(input)
    }

    #[test]
    fn verified_recovery_repairs_only_damaged_archives() -> Result<()> {
        let root = tempfile::tempdir()?;
        let bytes = b"original archive";
        let size = bytes.len() as u64;
        let hash = hex::encode(Sha256::digest(bytes));
        let destination = path(root.path(), "files", &hash)?;
        let parent = prepare(&destination)?;

        for damaged in [b"truncated".as_slice(), b"modified archive".as_slice()] {
            fs::write(&destination, damaged)?;
            let input = archive(&destination, bytes)?;
            assert!(publish(ArchiveInput::Build(input.path()), &destination, &hash, size).is_err());
            assert_eq!(fs::read(&destination)?, damaged);
            let temp_path = input.path().to_owned();
            publish(ArchiveInput::Recovery(input), &destination, &hash, size)?;
            assert_eq!(fs::read(&destination)?, bytes);
            assert!(!temp_path.exists());
        }

        let inode = fs::metadata(&destination)?.ino();
        let replay = archive(&destination, bytes)?;
        publish(ArchiveInput::Recovery(replay), &destination, &hash, size)?;
        assert_eq!(fs::metadata(&destination)?.ino(), inode);
        assert_eq!(fs::read_dir(parent)?.count(), 1);
        Ok(())
    }

    #[test]
    fn verified_recovery_refuses_non_regular_paths() -> Result<()> {
        let root = tempfile::tempdir()?;
        let bytes = b"original archive";
        let size = bytes.len() as u64;
        let hash = hex::encode(Sha256::digest(bytes));
        let destination = path(root.path(), "files", &hash)?;
        let parent = prepare(&destination)?;

        let target = root.path().join("other-archive");
        fs::write(&target, bytes)?;
        symlink(&target, &destination)?;
        let input = archive(&destination, bytes)?;
        assert!(publish(ArchiveInput::Recovery(input), &destination, &hash, size).is_err());
        assert_eq!(fs::read_link(&destination)?, target);
        assert_eq!(fs::read(target)?, bytes);

        fs::remove_file(&destination)?;
        fs::create_dir(&destination)?;
        let input = archive(&destination, bytes)?;
        assert!(publish(ArchiveInput::Recovery(input), &destination, &hash, size).is_err());
        assert!(destination.is_dir());
        assert_eq!(fs::read_dir(parent)?.count(), 1);
        Ok(())
    }
}
