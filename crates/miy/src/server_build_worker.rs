//! Source builds use a worker-owned recipe and never invoke project tools on the
//! host. A build receipt becomes successful only after immutable image and
//! packaged artifact verification; deployment remains a separate operation.
use crate::{bundle, contract, domain, server_build, source_archive};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sqlx::{Row, postgres::PgRow};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};
use uuid::Uuid;

const RECIPE: &str = include_str!("../server-build.Dockerfile");
const BUILD_SECONDS: u64 = 1800;
const OUTPUT_LIMIT: usize = 1024 * 1024;
const LOG_LIMIT: usize = 2 * 1024 * 1024;
const ARTIFACT_LIMIT: u64 = 128 * 1024 * 1024;
const BUILD_LOCK: i64 = 73490125;

// Docker may dispatch buildx as a subprocess. Cancel the complete owned client
// group on timeout/task abort so its BuildKit connection cannot outlive the job.
struct ClientGroup(Option<u32>);
impl Drop for ClientGroup {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.0.take() {
            unsafe extern "C" {
                fn kill(pid: i32, signal: i32) -> i32;
            }
            // This PID is a private group created before spawn; no host group
            // or daemon process is ever targeted.
            unsafe {
                kill(-(pid as i32), 9);
            }
        }
    }
}

fn work(id: Uuid) -> Result<PathBuf> {
    Ok(server_build::root()?.join("work").join(id.to_string()))
}

fn tag(id: Uuid) -> String {
    format!("miy-server-build:{id}")
}

fn docker_command(control: &Path, args: &[String]) -> Command {
    let mut command = Command::new("docker");
    command
        .env_clear()
        .env(
            "PATH",
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        )
        .env("HOME", control)
        .env("DOCKER_CONFIG", control.join("docker"))
        .env("DOCKER_BUILDKIT", "1")
        .env("LANG", "C.UTF-8")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    command
}

enum Output {
    Capture,
    Log,
    Archive(PathBuf),
}

/// Drain both pipes concurrently. Captured output, exported artifacts and saved
/// logs all have independent limits; excessive logs are drained, never buffered.
async fn docker(
    pool: &sqlx::PgPool,
    id: Uuid,
    control: &Path,
    args: &[String],
    seconds: u64,
    output: Output,
) -> Result<Vec<u8>> {
    let mut command = docker_command(control, args);
    let mut child = command.spawn().context("build_docker_unavailable")?;
    let mut group = ClientGroup(child.id());
    let mut stdout = child.stdout.take().context("build_stdout_unavailable")?;
    let mut stderr = child.stderr.take().context("build_stderr_unavailable")?;
    let mut export = match &output {
        Output::Archive(path) => Some(
            tokio::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(path)
                .await?,
        ),
        _ => None,
    };
    let result = tokio::time::timeout(Duration::from_secs(seconds), async {
        let (mut out_open, mut err_open) = (true, true);
        let (mut out_buf, mut err_buf) = ([0u8; 8192], [0u8; 8192]);
        let mut captured = Vec::new();
        let mut exported = 0u64;
        let mut logged = 0usize;
        while out_open || err_open {
            let (bytes, is_stdout) = tokio::select! {
                result = stdout.read(&mut out_buf), if out_open => {
                    let n = result?; if n == 0 { out_open = false; continue; }
                    (&out_buf[..n], true)
                }
                result = stderr.read(&mut err_buf), if err_open => {
                    let n = result?; if n == 0 { err_open = false; continue; }
                    (&err_buf[..n], false)
                }
            };
            if is_stdout {
                match &output {
                    Output::Capture => {
                        ensure!(
                            captured.len() + bytes.len() <= OUTPUT_LIMIT,
                            "build_output_limit"
                        );
                        captured.extend_from_slice(bytes);
                    }
                    Output::Archive(_) => {
                        exported += bytes.len() as u64;
                        ensure!(exported <= ARTIFACT_LIMIT, "build_artifact_size_limit");
                        export
                            .as_mut()
                            .context("missing_artifact_file")?
                            .write_all(bytes)
                            .await?;
                    }
                    Output::Log => {}
                }
            }
            if matches!(output, Output::Log) && logged < LOG_LIMIT {
                let text = String::from_utf8_lossy(bytes);
                server_build::append_log(pool, id, &text).await?;
                logged += text.len();
            }
        }
        ensure!(child.wait().await?.success(), "build_command_failed");
        group.0 = None;
        if let Some(file) = export.as_mut() {
            file.sync_all().await?;
        }
        Ok::<_, anyhow::Error>(captured)
    })
    .await;
    match result {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => {
            drop(group);
            let _ = child.kill().await;
            Err(error)
        }
        Err(_) => {
            drop(group);
            let _ = child.kill().await;
            anyhow::bail!("build_timeout")
        }
    }
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).to_owned()).collect()
}

async fn inspect(pool: &sqlx::PgPool, id: Uuid, control: &Path, image: &str) -> Result<Value> {
    let bytes = docker(
        pool,
        id,
        control,
        &args(&["image", "inspect", image]),
        90,
        Output::Capture,
    )
    .await?;
    let value: Value = serde_json::from_slice(&bytes)?;
    value
        .as_array()
        .and_then(|v| v.first())
        .cloned()
        .context("build_image_missing")
}

fn extraction_owned(meta: &Value, id: Uuid) -> bool {
    meta["Config"]["Labels"]["org.miy.build.id"] == id.to_string()
}

async fn remove_extraction(pool: &sqlx::PgPool, id: Uuid, control: &Path) -> Result<()> {
    let name = format!("miy-build-extract-{id}");
    let Ok(bytes) = docker(
        pool,
        id,
        control,
        &args(&["container", "inspect", &name]),
        90,
        Output::Capture,
    )
    .await
    else {
        return Ok(());
    };
    let inspected: Value = serde_json::from_slice(&bytes)?;
    ensure!(
        extraction_owned(&inspected[0], id),
        "build_container_ownership_mismatch"
    );
    docker(
        pool,
        id,
        control,
        &args(&["rm", "--force", &name]),
        90,
        Output::Capture,
    )
    .await?;
    Ok(())
}

fn checked_image(meta: &Value, row: &PgRow) -> Result<String> {
    let image = meta["Id"]
        .as_str()
        .filter(|id| crate::runtime::image_id(id))
        .context("build_image_id_invalid")?;
    let labels = &meta["Config"]["Labels"];
    ensure!(
        labels["org.miy.app.id"] == row.get::<String, _>("app_id")
            && labels["org.opencontainers.image.revision"] == row.get::<String, _>("source_commit")
            && labels["org.miy.source.archive-sha256"] == row.get::<String, _>("archive_hash")
            && labels["org.miy.build.id"] == row.get::<Uuid, _>("id").to_string()
            && crate::runtime_platform::image(meta).as_deref()
                == Some(row.get::<String, _>("platform").as_str()),
        "build_image_identity_mismatch"
    );
    Ok(image.to_owned())
}

/// Docker cp produces tar on stdout. Never let it extract image-owned symlinks
/// or unlimited files onto the worker filesystem.
fn extract_artifact(archive: &Path, root: &Path) -> Result<()> {
    ensure!(
        std::fs::metadata(archive)?.len() <= ARTIFACT_LIMIT,
        "build_artifact_size_limit"
    );
    std::fs::create_dir(root)?;
    let mut archive = tar::Archive::new(std::fs::File::open(archive)?);
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let name = std::str::from_utf8(&entry.path_bytes())?.to_owned();
        let name = name
            .strip_prefix("./")
            .unwrap_or(&name)
            .trim_end_matches('/');
        if name.is_empty() || name == "." {
            ensure!(
                entry.header().entry_type().is_dir(),
                "invalid_build_artifact"
            );
            continue;
        }
        ensure!(
            contract::safe_relative(name)
                && name.len() <= 240
                && name.split('/').count() <= 16
                && (matches!(name, "manifest.json" | "build.json" | "migrations" | "ui")
                    || name.starts_with("migrations/")
                    || name.starts_with("ui/")),
            "invalid_build_artifact_path"
        );
        ensure!(
            seen.insert(name.to_owned()) && seen.len() <= 8192,
            "build_artifact_file_limit"
        );
        let path = root.join(name);
        if entry.header().entry_type().is_dir() {
            ensure!(entry.size() == 0, "invalid_build_artifact_directory");
            std::fs::create_dir_all(path)?;
            continue;
        }
        ensure!(
            entry.header().entry_type().is_file() && entry.size() <= 16 * 1024 * 1024,
            "invalid_build_artifact_member"
        );
        total += entry.size();
        ensure!(total <= ARTIFACT_LIMIT, "build_artifact_size_limit");
        std::fs::create_dir_all(path.parent().context("invalid_build_artifact_path")?)?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        ensure!(
            std::io::copy(&mut entry, &mut file)? == entry.size(),
            "truncated_build_artifact"
        );
    }
    contract::artifact_hash(root)?;
    Ok(())
}

async fn step(pool: &sqlx::PgPool, id: Uuid, name: &str) -> Result<()> {
    ensure!(
        sqlx::query(
            "UPDATE app_builds SET step=$2,updated_at=now() WHERE id=$1 AND status='running'"
        )
        .bind(id)
        .bind(name)
        .execute(pool)
        .await?
        .rows_affected()
            == 1,
        "build_ownership_lost"
    );
    Ok(())
}

async fn execute(pool: &sqlx::PgPool, row: &PgRow, recovering: bool) -> Result<(String, String)> {
    let id: Uuid = row.get("id");
    let app: String = row.get("app_id");
    ensure!(
        domain::can_manage(pool, row.get("actor"), &app, "dev").await?,
        "permission_revoked"
    );
    ensure!(
        source_archive::valid_commit(row.get("source_commit"))
            && crate::runtime_platform::server().as_deref()
                == Some(row.get::<String, _>("platform").as_str()),
        "build_platform_mismatch"
    );
    let directory = work(id)?;
    let control = directory.join("control");
    tokio::fs::create_dir_all(control.join("docker")).await?;
    tokio::fs::write(control.join("docker/config.json"), b"{}\n").await?;
    let context = directory.join("source");
    let archive = server_build::archive_path(id)?;
    step(pool, id, "validating_source").await?;
    let check_archive = archive.clone();
    let check_context = context.clone();
    let check_app = app.clone();
    let expected_hash: String = row.get("archive_hash");
    let expected_size: i64 = row.get("size_bytes");
    let manifest = tokio::task::spawn_blocking(move || {
        ensure!(
            std::fs::metadata(&check_archive)?.len() == expected_size as u64
                && source_archive::digest(&check_archive)? == expected_hash,
            "archive_hash_mismatch"
        );
        if check_context.exists() {
            std::fs::remove_dir_all(&check_context)?;
        }
        source_archive::extract(&check_archive, &check_context)?;
        source_archive::validate(&check_context, &check_app)
    })
    .await
    .context("source_validation_failed")?
    .context("source_validation_failed")?;
    crate::worker::verify_build_components(pool, &context)
        .await
        .context("component_validation_failed")?;
    crate::worker::verify_development_migrations(pool, &app, &context.join("migrations"))
        .await
        .context("migration_history_validation_failed")?;
    sqlx::query("UPDATE app_builds SET version=$2,bundle_version=$3,updated_at=now() WHERE id=$1 AND status='running'")
        .bind(id).bind(&manifest.version).bind(bundle::VERSION).execute(pool).await?;
    let build = json!({"bundleVersion":bundle::VERSION,"sourceCommit":row.get::<String,_>("source_commit"),"sourceArchiveHash":row.get::<String,_>("archive_hash"),"appId":app,"version":manifest.version});
    let iid = control.join("image.iid");
    let unique_tag = tag(id);
    let image = if recovering {
        // A disconnected BuildKit request is never restarted automatically.
        // A finished image can be recovered using immutable ID or job-only tag.
        let candidate = tokio::fs::read_to_string(&iid)
            .await
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| crate::runtime::image_id(v))
            .unwrap_or_else(|| unique_tag.clone());
        let meta = inspect(pool, id, &control, &candidate)
            .await
            .context("build_interrupted")?;
        checked_image(&meta, row)?
    } else {
        tokio::fs::write(
            context.join(".miy-server-build.json"),
            serde_json::to_vec(&build)?,
        )
        .await?;
        tokio::fs::write(control.join("Dockerfile"), RECIPE).await?;
        let _ = tokio::fs::remove_file(&iid).await;
        step(pool, id, "building").await?;
        server_build::append_log(pool,id,"Building with the verified worker recipe; application checks run inside offline Docker stages.\n").await?;
        let command = vec![
            "build".into(),
            "--progress=plain".into(),
            "--pull".into(),
            "--platform".into(),
            row.get("platform"),
            "--iidfile".into(),
            iid.to_string_lossy().into_owned(),
            "--tag".into(),
            unique_tag.clone(),
            "--file".into(),
            control.join("Dockerfile").to_string_lossy().into_owned(),
            "--build-arg".into(),
            format!("MIY_APP_ID={app}"),
            "--build-arg".into(),
            format!(
                "MIY_SOURCE_COMMIT={}",
                row.get::<String, _>("source_commit")
            ),
            "--build-arg".into(),
            format!("MIY_SOURCE_HASH={}", row.get::<String, _>("archive_hash")),
            "--build-arg".into(),
            format!("MIY_BUILD_ID={id}"),
            context.to_string_lossy().into_owned(),
        ];
        docker(pool, id, &control, &command, BUILD_SECONDS, Output::Log)
            .await
            .context("build_compilation_failed")?;
        let value = tokio::fs::read_to_string(&iid).await?;
        let image = value.trim();
        ensure!(crate::runtime::image_id(image), "build_image_id_invalid");
        let meta = inspect(pool, id, &control, image).await?;
        let actual = checked_image(&meta, row)?;
        ensure!(actual == image, "build_image_identity_mismatch");
        actual
    };
    step(pool, id, "verifying_image").await?;
    let container = format!("miy-build-extract-{id}");
    remove_extraction(pool, id, &control).await?;
    docker(
        pool,
        id,
        &control,
        &args(&[
            "create",
            "--name",
            &container,
            "--network",
            "none",
            "--label",
            &format!("org.miy.build.id={id}"),
            &image,
        ]),
        90,
        Output::Capture,
    )
    .await
    .context("build_artifact_invalid")?;
    let artifact_archive = directory.join("artifact.tar");
    let _ = tokio::fs::remove_file(&artifact_archive).await;
    let copied = docker(
        pool,
        id,
        &control,
        &args(&["cp", &format!("{container}:/miy/."), "-"]),
        90,
        Output::Archive(artifact_archive.clone()),
    )
    .await;
    let removed = remove_extraction(pool, id, &control).await;
    copied.context("build_artifact_invalid")?;
    removed.context("build_artifact_invalid")?;
    let artifact = directory.join("artifact");
    let checked_artifact = artifact.clone();
    let checked_context = context.clone();
    tokio::task::spawn_blocking(move || {
        if checked_artifact.exists() {
            std::fs::remove_dir_all(&checked_artifact)?;
        }
        extract_artifact(&artifact_archive, &checked_artifact)?;
        let packaged: contract::Manifest =
            serde_json::from_slice(&std::fs::read(checked_artifact.join("manifest.json"))?)?;
        contract::validate(&packaged).map_err(anyhow::Error::msg)?;
        ensure!(
            serde_json::to_value(&packaged)? == serde_json::to_value(&manifest)?,
            "build_manifest_mismatch"
        );
        let actual: Value =
            serde_json::from_slice(&std::fs::read(checked_artifact.join("build.json"))?)?;
        ensure!(actual == build, "build_metadata_mismatch");
        ensure!(
            bundle::migrations(&checked_artifact.join("migrations"))?
                == bundle::migrations(&checked_context.join("migrations"))?,
            "build_migrations_mismatch"
        );
        bundle::ui(&checked_artifact.join("ui"))?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("build_artifact_invalid")?
    .context("build_artifact_invalid")?;
    // Authorization and history can change during a long compilation. A build
    // never applies migrations, provisions databases, or changes a release.
    ensure!(
        domain::can_manage(pool, row.get("actor"), &app, "dev").await?,
        "permission_revoked"
    );
    crate::worker::verify_development_migrations(pool, &app, &artifact.join("migrations"))
        .await
        .context("migration_history_validation_failed")?;
    let version: String = serde_json::from_slice::<Value>(
        &tokio::fs::read(artifact.join("manifest.json")).await?,
    )?["version"]
        .as_str()
        .context("missing_build_version")?
        .to_owned();
    Ok((image, version))
}

fn failure_code(error: &anyhow::Error, recovering: bool) -> &'static str {
    // Emit only fixed public codes. Parser messages and paths can include
    // source-controlled data, so never include an arbitrary error string.
    for cause in error.chain().collect::<Vec<_>>().into_iter().rev() {
        let message = cause.to_string();
        let code = message.split(':').next().unwrap_or("");
        match code {
            "permission_revoked" => return "permission_revoked",
            "migration_history_mismatch" => return "migration_history_mismatch",
            "build_database_history_unavailable" => return "build_database_history_unavailable",
            "build_timeout" => return "build_timeout",
            "archive_hash_mismatch" => return "archive_hash_mismatch",
            "unpublished_source_component" => return "unpublished_source_component",
            "build_platform_mismatch" => return "build_platform_mismatch",
            "build_docker_unavailable" => return "build_docker_unavailable",
            "bundle_version_mismatch" => return "bundle_version_mismatch",
            "missing_bundle_lock"
            | "bundle_lock_modified"
            | "invalid_bundle_file"
            | "protected_bundle_modified" => return "protected_bundle_modified",
            "source_sdk_inventory_mismatch"
            | "source_sdk_dependency_required"
            | "source_sdk_dependency_modified" => return "source_sdk_modified",
            "source_package_manager_mismatch" | "source_package_manager_modified" => {
                return "source_package_manager_modified";
            }
            "build_image_identity_mismatch" | "build_image_id_invalid" => {
                return "build_image_identity_mismatch";
            }
            "build_container_ownership_mismatch" => return "build_container_ownership_mismatch",
            "source_validation_failed" => return "source_validation_failed",
            "component_validation_failed" => return "component_validation_failed",
            "migration_history_validation_failed" => return "migration_history_validation_failed",
            "build_compilation_failed" => return "build_compilation_failed",
            "build_artifact_invalid" => return "build_artifact_invalid",
            "build_interrupted" => return "build_interrupted",
            _ => {}
        }
    }
    if recovering {
        "build_interrupted"
    } else {
        "build_failed"
    }
}

async fn finish(pool: &sqlx::PgPool, row: &PgRow, recovering: bool) -> Result<()> {
    let id: Uuid = row.get("id");
    let result = execute(pool, row, recovering).await;
    let succeeded = result.is_ok();
    match result {
        Ok((image, version)) => {
            ensure!(sqlx::query("UPDATE app_builds SET status='succeeded',step='ready',image_id=$2,version=$3,bundle_version=$4,error_code=NULL,finished_at=now(),updated_at=now() WHERE id=$1 AND status='running'")
                .bind(id).bind(image).bind(version).bind(bundle::VERSION).execute(pool).await?.rows_affected() == 1,"build_ownership_lost");
            server_build::append_log(pool,id,"Build succeeded; the immutable image is ready for a separate development deployment.\n").await?;
        }
        Err(error) => {
            let code = failure_code(&error, recovering);
            server_build::append_log(
                pool,
                id,
                &format!("Build failed: {code}. A new explicit request is required to retry.\n"),
            )
            .await?;
            sqlx::query("UPDATE app_builds SET status='failed',step='failed',error_code=$2,finished_at=now(),updated_at=now() WHERE id=$1 AND status='running'")
                .bind(id).bind(code).execute(pool).await?;
            tracing::warn!(event="server_build_failed",build_id=%id,code);
        }
    }
    // Keep source and iid until the terminal receipt is durable.
    let control = work(id)?.join("control");
    let _ = remove_extraction(pool, id, &control).await;
    // Keep the unique tag as the daemon's durable reference to the successful
    // image. Removing its last tag would also delete the immutable image.
    if !succeeded
        && let Ok(meta) = inspect(pool, id, &control, &tag(id)).await
        && checked_image(&meta, row).is_ok()
    {
        let _ = docker(
            pool,
            id,
            &control,
            &args(&["image", "rm", &tag(id)]),
            90,
            Output::Capture,
        )
        .await;
    }
    let _ = tokio::fs::remove_file(server_build::archive_path(id)?).await;
    let _ = tokio::fs::remove_dir_all(work(id)?).await;
    Ok(())
}

async fn lock(pool: &sqlx::PgPool) -> Result<Option<sqlx::pool::PoolConnection<sqlx::Postgres>>> {
    let mut guard = pool.acquire().await?;
    guard.close_on_drop();
    let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(BUILD_LOCK)
        .fetch_one(&mut *guard)
        .await?;
    Ok(acquired.then_some(guard))
}

pub async fn recover(pool: &sqlx::PgPool) -> Result<()> {
    let Some(_guard) = lock(pool).await? else {
        return Ok(());
    };
    let rows =
        sqlx::query("SELECT * FROM app_builds WHERE status='running' ORDER BY created_at,id")
            .fetch_all(pool)
            .await?;
    for row in rows {
        finish(pool, &row, true).await?;
    }
    Ok(())
}

pub async fn tick(pool: &sqlx::PgPool) -> Result<bool> {
    let Some(_guard) = lock(pool).await? else {
        return Ok(false);
    };
    let expired: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM app_builds WHERE status='receiving' AND upload_expires_at<now() AND updated_at<now()-interval '1 minute' ORDER BY updated_at,id LIMIT 32").fetch_all(pool).await?;
    for id in expired {
        let mut tx = pool.begin().await?;
        let acquired: bool =
            sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended($1,0))")
                .bind(format!("build-upload:{id}"))
                .fetch_one(&mut *tx)
                .await?;
        if acquired {
            let changed = sqlx::query("UPDATE app_builds SET step='upload_incomplete',error_code='source_upload_interrupted',updated_at=now() WHERE id=$1 AND status='receiving' AND upload_expires_at<now()")
                .bind(id).execute(&mut *tx).await?.rows_affected();
            if changed == 1 {
                let path = server_build::archive_path(id)?;
                let _ = tokio::fs::remove_file(path.with_extension("part")).await;
                let _ = tokio::fs::remove_file(path).await;
            }
            // Hold the upload lock through deletion: a retry must not open a
            // fresh .part between the status update and cleanup.
            tx.commit().await?;
        }
    }
    let Some(row) = sqlx::query("UPDATE app_builds SET status='running',step='validating_source',started_at=now(),updated_at=now() WHERE id=(SELECT id FROM app_builds WHERE status='queued' ORDER BY created_at,id LIMIT 1) AND status='queued' RETURNING *").fetch_optional(pool).await? else { return Ok(false); };
    finish(pool, &row, false).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recipe_uses_offline_checks_and_no_shared_compiled_cache() {
        assert!(RECIPE.contains("cargo fetch --locked"));
        assert!(RECIPE.contains(
            "RUN --network=none cargo fmt --all --check && cargo test --locked --offline"
        ));
        assert!(RECIPE.contains("pnpm fetch --frozen-lockfile --ignore-scripts"));
        assert!(RECIPE.contains("RUN --network=none pnpm install --offline --frozen-lockfile"));
        assert!(!RECIPE.contains("--mount"));
        // Both compilers receive the complete verified snapshot, including
        // integration tests/build.rs and frontend public assets. Narrow COPY
        // lists can silently turn a real cargo test suite into zero tests.
        assert_eq!(RECIPE.lines().filter(|line| *line == "COPY . .").count(), 2);
        assert!(
            RECIPE
                .lines()
                .filter(|l| l.starts_with("FROM ")
                    && !l.ends_with("AS backend")
                    && !l.ends_with("AS frontend"))
                .all(|l| l.contains("@sha256:"))
        );
    }
    #[test]
    fn docker_has_only_fixed_environment_and_no_secret_mounts() {
        let command = docker_command(
            Path::new("/tmp/build-control"),
            &args(&["build", "/tmp/source"]),
        );
        let env: std::collections::BTreeMap<_, _> = command
            .as_std()
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        assert_eq!(env.len(), 5);
        assert_eq!(env["HOME"].as_deref(), Some("/tmp/build-control"));
        assert_eq!(
            env["DOCKER_CONFIG"].as_deref(),
            Some("/tmp/build-control/docker")
        );
        assert!(!env.contains_key("DATABASE_URL"));
        assert!(!env.contains_key("PROVISION_DATABASE_URL"));
        assert!(!env.contains_key("MIY_MASTER_KEY"));
    }
    #[test]
    fn artifacts_reject_links_duplicates_and_unexpected_paths() {
        for (name, kind, duplicate) in [
            ("ui/link", b'2', false),
            ("manifest.json", b'0', true),
            ("secret", b'0', false),
        ] {
            let tmp = tempfile::tempdir().unwrap();
            let path = tmp.path().join("artifact.tar");
            let mut writer = tar::Builder::new(std::fs::File::create(&path).unwrap());
            for _ in 0..if duplicate { 2 } else { 1 } {
                let mut header = tar::Header::new_ustar();
                header.set_size(0);
                header.set_mode(0o644);
                header.set_entry_type(tar::EntryType::new(kind));
                header.set_cksum();
                writer
                    .append_data(&mut header, name, std::io::empty())
                    .unwrap();
            }
            writer.finish().unwrap();
            drop(writer);
            assert!(extract_artifact(&path, &tmp.path().join("out")).is_err());
        }
    }
    #[test]
    fn cleanup_requires_the_exact_job_label() {
        let id = Uuid::new_v4();
        assert!(!extraction_owned(&json!({}), id));
        assert!(!extraction_owned(
            &json!({"Config":{"Labels":{"org.miy.build.id":Uuid::new_v4().to_string()}}}),
            id
        ));
        assert!(extraction_owned(
            &json!({"Config":{"Labels":{"org.miy.build.id":id.to_string()}}}),
            id
        ));
    }
    #[test]
    fn validation_diagnostics_are_specific_and_do_not_expose_source_text() {
        assert_eq!(
            failure_code(
                &anyhow::anyhow!("protected_bundle_modified: sdk/private.txt")
                    .context("source_validation_failed"),
                false
            ),
            "protected_bundle_modified"
        );
        assert_eq!(
            failure_code(
                &anyhow::anyhow!("source_sdk_inventory_mismatch")
                    .context("source_validation_failed"),
                false
            ),
            "source_sdk_modified"
        );
        assert_eq!(
            failure_code(
                &anyhow::anyhow!("parser copied user secret").context("source_validation_failed"),
                false
            ),
            "source_validation_failed"
        );
        assert_eq!(
            failure_code(
                &anyhow::anyhow!("build_timeout").context("build_compilation_failed"),
                false
            ),
            "build_timeout"
        );
        assert_eq!(
            failure_code(
                &anyhow::anyhow!("image missing").context("build_interrupted"),
                true
            ),
            "build_interrupted"
        );
    }
}
