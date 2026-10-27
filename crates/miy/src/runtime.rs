//! Local single-host image store + persistent Docker deployment runner.
pub(crate) use crate::docker::{docker, healthy, inspect};
pub(crate) use crate::identity::sign_identity;
use crate::{
    State,
    auth::{self, Principal},
    contract::{self, Manifest},
    domain,
    http::{ApiError, ApiResult},
    runtime_health::{self, Observation},
};
use anyhow::{Context, Result};
use axum::{
    Extension,
    body::{Body, to_bytes},
    extract::{Path, Request, State as S},
    http::{Method, StatusCode, header},
    response::Response,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::{path::PathBuf, time::Duration};
use uuid::Uuid;

struct RemoveFileOnDrop(std::path::PathBuf);
impl Drop for RemoveFileOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
struct RemoveDirectoryOnDrop(std::path::PathBuf);
impl Drop for RemoveDirectoryOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRequest {
    pub action: String,
    pub environment: String,
    pub image_id: Option<String>,
    pub release_id: Option<Uuid>,
    pub idempotency_key: String,
}
pub(crate) fn image_id(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|v| {
        v.len() == 64
            && v.bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    })
}
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceCheck {
    pub scope: String,
    /// Account/role used in the test, not the authenticated acceptance recorder.
    pub actor: String,
    pub result: String,
    pub evidence: String,
}
fn validate_checks(checks: &[AcceptanceCheck]) -> ApiResult<()> {
    if checks.len() > 64
        || checks.iter().any(|c| {
            c.scope.trim().is_empty()
                || c.scope.len() > 120
                || c.actor.trim().is_empty()
                || c.actor.len() > 120
                || c.evidence.trim().is_empty()
                || c.evidence.len() > 2000
                || !["passed", "not_run"].contains(&c.result.as_str())
        })
        || (!checks.is_empty() && !checks.iter().any(|c| c.result == "passed"))
    {
        return Err(ApiError::bad(
            "invalid_acceptance_checks: use passed/not_run with scope, actor and evidence; unresolved failures cannot be accepted",
        ));
    }
    Ok(())
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseCheck {
    pub release_id: Uuid,
    pub note: String,
    #[serde(default)]
    pub checks: Vec<AcceptanceCheck>,
}
pub async fn check_release(
    state: &State,
    p: &Principal,
    app: &str,
    v: ReleaseCheck,
) -> ApiResult<Value> {
    domain::manage(state, p, app, "dev").await?;
    if v.note.trim().is_empty() || v.note.len() > 2000 {
        return Err(ApiError::bad("test_note_required"));
    }
    validate_checks(&v.checks)?;
    let row = sqlx::query("SELECT id,runtime_port FROM installations WHERE app_id=$1 AND environment='dev' AND release_id=$2 AND db_status='ready' AND runtime_status='running' AND desired_state='running'")
        .bind(app).bind(v.release_id).fetch_optional(&state.pool).await?.ok_or_else(||ApiError::conflict("release_not_running_in_dev"))?;
    healthy(
        row.get::<Option<i32>, _>("runtime_port")
            .ok_or_else(|| ApiError::conflict("not_a_docker_release"))?,
        v.release_id,
    )
    .await
    .map_err(|_| ApiError::conflict("app_health_failed"))?;
    sqlx::query("INSERT INTO release_checks(release_id,installation_id,actor,note,checks) VALUES($1,$2,$3,$4,$5) ON CONFLICT(release_id) DO UPDATE SET actor=excluded.actor,note=excluded.note,checks=excluded.checks,checked_at=now()")
        .bind(v.release_id).bind(row.get::<Uuid,_>("id")).bind(&p.id).bind(v.note.trim()).bind(json!(v.checks)).execute(&state.pool).await?;
    Ok(
        json!({"releaseId":v.release_id,"tested":true,"recordedBy":p.id,"checks":v.checks,"structured":!v.checks.is_empty(),"note":v.note.trim(),"meaning":"User-recorded test acceptance plus runtime health; not automatic business correctness verification"}),
    )
}
async fn checked_image(pool: &sqlx::PgPool, app: &str, release: Uuid) -> Result<String> {
    let image: Option<String> = sqlx::query_scalar("SELECT r.image_id FROM releases r JOIN release_checks c ON c.release_id=r.id WHERE r.id=$1 AND r.app_id=$2 AND EXISTS(SELECT 1 FROM runtime_jobs j WHERE j.release_id=r.id AND j.environment='dev' AND j.action='deploy' AND j.status='succeeded')")
        .bind(release).bind(app).fetch_optional(pool).await?.flatten();
    image.context("release_not_tested_in_dev")
}
pub async fn request(
    state: &State,
    p: &Principal,
    app: &str,
    v: RuntimeRequest,
) -> ApiResult<Value> {
    if !["dev", "prod"].contains(&v.environment.as_str())
        || !["deploy", "promote", "start", "stop", "restart", "remove"].contains(&v.action.as_str())
        || v.idempotency_key.is_empty()
        || v.idempotency_key.len() > 128
    {
        return Err(ApiError::bad("invalid_runtime_request"));
    }
    if (v.action == "deploy" && !v.image_id.as_deref().is_some_and(image_id))
        || (v.action != "deploy" && v.image_id.is_some())
    {
        return Err(ApiError::bad("invalid_image_id"));
    }
    domain::manage(state, p, app, &v.environment).await?;
    if (v.action == "deploy" && v.environment != "dev")
        || (v.action == "promote" && (v.environment != "prod" || v.release_id.is_none()))
        || (v.action != "promote" && v.release_id.is_some())
    {
        return Err(ApiError::bad("use_checked_release_promotion_for_prod"));
    }
    let image = if v.action == "promote" {
        Some(
            checked_image(&state.pool, app, v.release_id.unwrap())
                .await
                .map_err(|_| ApiError::conflict("release_not_tested_in_dev"))?,
        )
    } else {
        v.image_id.clone()
    };
    let input = contract::hash(format!("{app}:{}:{}:{:?}", v.environment, v.action, image));
    sqlx::query("INSERT INTO runtime_jobs(id,app_id,environment,actor,action,image_id,idempotency_key,input_hash) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(actor,idempotency_key) DO NOTHING")
        .bind(Uuid::new_v4()).bind(app).bind(&v.environment).bind(&p.id).bind(&v.action).bind(&image).bind(&v.idempotency_key).bind(&input).execute(&state.pool).await?;
    let row =
        sqlx::query("SELECT id,input_hash FROM runtime_jobs WHERE actor=$1 AND idempotency_key=$2")
            .bind(&p.id)
            .bind(&v.idempotency_key)
            .fetch_one(&state.pool)
            .await?;
    if row.get::<String, _>("input_hash") != input {
        return Err(ApiError::conflict("idempotency_conflict"));
    }
    job(state, p, row.get("id")).await
}
pub async fn job(state: &State, p: &Principal, id: Uuid) -> ApiResult<Value> {
    let row = sqlx::query("SELECT * FROM runtime_jobs WHERE id=$1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::not_found)?;
    domain::authorize(state, p, row.get("app_id"), row.get("environment"), false).await?;
    Ok(
        json!({"id":id,"appId":row.get::<String,_>("app_id"),"environment":row.get::<String,_>("environment"),"action":row.get::<String,_>("action"),"status":row.get::<String,_>("status"),"step":row.get::<String,_>("step"),"errorCode":row.get::<Option<String>,_>("error_code"),"installationId":row.get::<Option<Uuid>,_>("installation_id"),"releaseId":row.get::<Option<Uuid>,_>("release_id")}),
    )
}
async fn owned(name: &str, installation: Uuid) -> Result<Value> {
    let value = inspect(name).await?;
    anyhow::ensure!(
        value["Config"]["Labels"]["org.miy.installation"] == installation.to_string(),
        "unmanaged_container"
    );
    Ok(value)
}
// A successful, exact container listing is the only evidence that absence is safe.
// Inspect or daemon failures are never treated as "already removed". Use the immutable
// container ID after ownership validation so a same-name replacement cannot be removed.
async fn removable_container(name: &str, installation: Uuid) -> Result<Option<Value>> {
    anyhow::ensure!(
        name == format!("miy-app-{}", installation.simple()),
        "unexpected_container_name"
    );
    let filter = format!("name=^/{name}$");
    let id = docker(&[
        "container",
        "ls",
        "--all",
        "--quiet",
        "--no-trunc",
        "--filter",
        &filter,
    ])
    .await?;
    if id.is_empty() {
        return Ok(None);
    }
    anyhow::ensure!(
        id.len() == 64 && id.bytes().all(|c| c.is_ascii_hexdigit()),
        "ambiguous_container_lookup"
    );
    let value = owned(&id, installation).await?;
    anyhow::ensure!(
        value["Id"] == id && value["Name"] == format!("/{name}"),
        "container_lookup_mismatch"
    );
    Ok(Some(value))
}
async fn step(pool: &sqlx::PgPool, id: Uuid, name: &str) -> Result<()> {
    sqlx::query("UPDATE runtime_jobs SET step=$2,updated_at=now() WHERE id=$1")
        .bind(id)
        .bind(name)
        .execute(pool)
        .await?;
    Ok(())
}
pub async fn recover(pool: &sqlx::PgPool) -> Result<()> {
    sqlx::query("UPDATE runtime_jobs SET status='queued',updated_at=now() WHERE status='running'")
        .execute(pool)
        .await?;
    Ok(())
}
pub async fn tick(pool: &sqlx::PgPool) -> Result<bool> {
    let Some(job) = sqlx::query(
        "SELECT * FROM runtime_jobs WHERE status='queued' ORDER BY created_at,id LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(false);
    };
    let id: Uuid = job.get("id");
    let app: String = job.get("app_id");
    let env: String = job.get("environment");
    let mut lock = pool.acquire().await?;
    lock.close_on_drop();
    let lock_key = format!("runtime:{app}:{env}");
    sqlx::query("SELECT pg_advisory_lock(hashtextextended($1,0))")
        .bind(&lock_key)
        .execute(&mut *lock)
        .await?;
    sqlx::query(
        "UPDATE runtime_jobs SET status='running',error_code=NULL,updated_at=now() WHERE id=$1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    let result = execute(pool, &job).await;
    if let Err(error) = result {
        let detail = error.to_string();
        let code = if detail.contains("manifest") || detail.contains("contract") {
            "invalid_image_contract"
        } else if detail.contains("permission") {
            "permission_revoked"
        } else if detail.contains("migration") {
            "migration_failed"
        } else if detail.contains("immutable") {
            "immutable_release_conflict"
        } else if detail.contains("health") {
            "app_health_failed"
        } else {
            "runtime_operation_failed"
        };
        sqlx::query(
            "UPDATE runtime_jobs SET status='failed',error_code=$2,updated_at=now() WHERE id=$1",
        )
        .bind(id)
        .bind(code)
        .execute(pool)
        .await?;
        tracing::error!(event="runtime_job_failed",job_id=%id,code);
    } else {
        sqlx::query("UPDATE runtime_jobs SET status='succeeded',step='complete',updated_at=now() WHERE id=$1").bind(id).execute(pool).await?;
    }
    sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1,0))")
        .bind(lock_key)
        .execute(&mut *lock)
        .await?;
    Ok(true)
}
async fn execute(pool: &sqlx::PgPool, job: &sqlx::postgres::PgRow) -> Result<()> {
    let id: Uuid = job.get("id");
    let app: String = job.get("app_id");
    let env: String = job.get("environment");
    let actor: String = job.get("actor");
    let action: String = job.get("action");
    anyhow::ensure!(
        domain::can_manage(pool, &actor, &app, &env).await?,
        "permission_revoked"
    );
    if action == "promote" {
        let image: String = job.get("image_id");
        let release: Uuid =
            sqlx::query_scalar("SELECT id FROM releases WHERE app_id=$1 AND image_id=$2")
                .bind(&app)
                .bind(&image)
                .fetch_one(pool)
                .await?;
        checked_image(pool, &app, release).await?;
    }
    if action != "deploy" && action != "promote" {
        let inst = sqlx::query("SELECT * FROM installations WHERE app_id=$1 AND environment=$2")
            .bind(&app)
            .bind(&env)
            .fetch_one(pool)
            .await?;
        let installation: Uuid = inst.get("id");
        sqlx::query("UPDATE runtime_jobs SET installation_id=$2,release_id=$3 WHERE id=$1")
            .bind(id)
            .bind(installation)
            .bind(inst.get::<Uuid, _>("release_id"))
            .execute(pool)
            .await?;
        let name: String = inst
            .get::<Option<String>, _>("runtime_container")
            .context("not_deployed")?;
        if action == "remove" {
            let metadata = removable_container(&name, installation).await?;
            step(pool, id, "remove").await?;
            sqlx::query("UPDATE installations SET desired_state='stopped',runtime_status='stopping' WHERE id=$1").bind(installation).execute(pool).await?;
            if let Some(metadata) = metadata {
                let container = metadata["Id"].as_str().context("missing_container_id")?;
                docker(&["stop", "--time", "10", container]).await?;
                docker(&["rm", container]).await?;
            }
            sqlx::query(
                "UPDATE installations SET runtime_status='removed',runtime_port=NULL WHERE id=$1",
            )
            .bind(installation)
            .execute(pool)
            .await?;
            runtime_health::record(pool, installation, Observation::Inactive).await?;
            return Ok(());
        }
        owned(&name, installation).await?;
        step(pool, id, &action).await?;
        match action.as_str() {
            "stop" => {
                sqlx::query("UPDATE installations SET desired_state='stopped',runtime_status='stopping' WHERE id=$1").bind(installation).execute(pool).await?;
                docker(&["stop", "--time", "10", &name]).await?;
                sqlx::query("UPDATE installations SET runtime_status='stopped' WHERE id=$1")
                    .bind(installation)
                    .execute(pool)
                    .await?;
                runtime_health::record(pool, installation, Observation::Inactive).await?;
            }
            "start" | "restart" => {
                anyhow::ensure!(
                    inst.get::<String, _>("db_status") == "ready",
                    "database_not_ready"
                );
                let metadata = owned(&name, installation).await?;
                anyhow::ensure!(
                    metadata["Config"]["Labels"]["org.miy.release"]
                        == inst.get::<Uuid, _>("release_id").to_string(),
                    "container_release_mismatch_redeploy"
                );
                sqlx::query("UPDATE installations SET desired_state='running',runtime_status='starting' WHERE id=$1").bind(installation).execute(pool).await?;
                let started = async {
                    docker(&[
                        if action == "restart" {
                            "restart"
                        } else {
                            "start"
                        },
                        &name,
                    ])
                    .await?;
                    let meta = owned(&name, installation).await?;
                    anyhow::ensure!(
                        meta["Image"].as_str()
                            == inst.get::<Option<String>, _>("runtime_image").as_deref(),
                        "running_image_mismatch"
                    );
                    // Docker allocates a fresh random published port on every start.
                    let port = published_port(&meta)?;
                    sqlx::query("UPDATE installations SET runtime_port=$2 WHERE id=$1")
                        .bind(installation)
                        .bind(port)
                        .execute(pool)
                        .await?;
                    healthy(port, inst.get("release_id")).await?;
                    Ok::<_, anyhow::Error>(())
                }
                .await;
                if let Err(error) = started {
                    let _ = docker(&["stop", "--time", "5", &name]).await;
                    sqlx::query("UPDATE installations SET runtime_status='failed',desired_state='stopped' WHERE id=$1").bind(installation).execute(pool).await?;
                    runtime_health::record(pool, installation, Observation::Inactive).await?;
                    return Err(error);
                }
                sqlx::query("UPDATE installations SET runtime_status='running' WHERE id=$1")
                    .bind(installation)
                    .execute(pool)
                    .await?;
                runtime_health::record(pool, installation, Observation::Healthy).await?;
            }
            _ => anyhow::bail!("invalid_action"),
        }
        return Ok(());
    }
    let image: String = job
        .get::<Option<String>, _>("image_id")
        .context("missing_image")?;
    anyhow::ensure!(image_id(&image), "invalid_image");
    step(pool, id, "verifying_image").await?;
    let meta = inspect(&image).await?;
    anyhow::ensure!(
        meta["Id"] == image && meta["Config"]["Labels"]["org.miy.app.id"] == app,
        "image_manifest_mismatch"
    );
    let root = PathBuf::from(std::env::var("MIY_ARTIFACTS")?);
    let temporary = root.join(format!(".import-{id}"));
    if temporary.exists() {
        std::fs::remove_dir_all(&temporary)?;
    }
    std::fs::create_dir_all(&temporary)?;
    let _temporary_cleanup = RemoveDirectoryOnDrop(temporary.clone());
    let extraction = format!("miy-import-{id}");
    if inspect(&extraction).await.is_ok() {
        docker(&["rm", "-f", &extraction]).await?;
    }
    docker(&["create", "--name", &extraction, "--network", "none", &image]).await?;
    let copied = docker(&[
        "cp",
        &format!("{extraction}:/miy/."),
        temporary.to_str().context("invalid_path")?,
    ])
    .await;
    let removed = docker(&["rm", &extraction]).await;
    copied?;
    removed?;
    let hash = contract::artifact_hash(&temporary)?; // Reject symlinks before reading any imported file.
    let manifest: Manifest =
        serde_json::from_slice(&std::fs::read(temporary.join("manifest.json"))?)?;
    contract::validate(&manifest).map_err(anyhow::Error::msg)?;
    anyhow::ensure!(
        manifest.id == app
            && temporary
                .join("ui")
                .join(&manifest.ui.module_entry)
                .is_file()
            && temporary.join("migrations").is_dir(),
        "invalid_manifest_bundle"
    );
    let build: Value = serde_json::from_slice(&std::fs::read(temporary.join("build.json"))?)?;
    // Fail before stopping the previous release when the packaged contract is invalid.
    crate::bundle::migrations(&temporary.join("migrations"))?;
    crate::bundle::ui(&temporary.join("ui"))?;
    let commit = build["sourceCommit"]
        .as_str()
        .context("missing_source_commit")?;
    anyhow::ensure!(
        commit.len() == 40
            && commit.bytes().all(|c| c.is_ascii_hexdigit())
            && meta["Config"]["Labels"]["org.opencontainers.image.revision"] == commit
            && build["appId"] == app
            && build["version"] == manifest.version,
        "image_manifest_provenance_mismatch"
    );
    let dest = root.join(&hash);
    if dest.exists() {
        contract::verified_artifact(&root, &hash)?;
        std::fs::remove_dir_all(&temporary)?;
    } else {
        std::fs::rename(&temporary, &dest)?;
    }
    let existing = sqlx::query("SELECT id,image_id FROM releases WHERE app_id=$1 AND version=$2")
        .bind(&app)
        .bind(&manifest.version)
        .fetch_optional(pool)
        .await?;
    let release = if let Some(row) = existing {
        anyhow::ensure!(
            row.get::<Option<String>, _>("image_id").as_deref() == Some(&image),
            "immutable_release_conflict"
        );
        row.get::<Uuid, _>("id")
    } else {
        let release = Uuid::new_v4();
        sqlx::query("INSERT INTO releases(id,app_id,version,artifact_hash,manifest,image_id,source_commit) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(release).bind(&app).bind(&manifest.version).bind(&hash).bind(serde_json::to_value(&manifest)?).bind(&image).bind(commit).execute(pool).await?;
        release
    };
    sqlx::query("INSERT INTO installations(id,app_id,environment,release_id) VALUES($1,$2,$3,$4) ON CONFLICT(app_id,environment) DO NOTHING").bind(Uuid::new_v4()).bind(&app).bind(&env).bind(release).execute(pool).await?;
    let inst = sqlx::query("SELECT * FROM installations WHERE app_id=$1 AND environment=$2")
        .bind(&app)
        .bind(&env)
        .fetch_one(pool)
        .await?;
    let installation: Uuid = inst.get("id");
    sqlx::query("UPDATE runtime_jobs SET installation_id=$2,release_id=$3 WHERE id=$1")
        .bind(id)
        .bind(installation)
        .bind(release)
        .execute(pool)
        .await?;
    // An installation lock also excludes standalone DB jobs.
    let mut lock = pool.acquire().await?;
    lock.close_on_drop();
    sqlx::query("SELECT pg_advisory_lock(hashtextextended($1,0))")
        .bind(installation.to_string())
        .execute(&mut *lock)
        .await?;
    let result = deploy_installation(pool, id, installation, release, &image, &actor, &inst).await;
    sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1,0))")
        .bind(installation.to_string())
        .execute(&mut *lock)
        .await?;
    if result.is_err() {
        let name = format!("miy-app-{}", installation.simple());
        if owned(&name, installation).await.is_ok() {
            let _ = docker(&["stop", "--time", "5", &name]).await;
        }
        sqlx::query(
            "UPDATE installations SET runtime_status='failed',desired_state='stopped' WHERE id=$1",
        )
        .bind(installation)
        .execute(pool)
        .await?;
    }
    result
}
async fn deploy_installation(
    pool: &sqlx::PgPool,
    job: Uuid,
    installation: Uuid,
    release: Uuid,
    image: &str,
    actor: &str,
    inst: &sqlx::postgres::PgRow,
) -> Result<()> {
    runtime_health::record(pool, installation, Observation::Inactive).await?;
    step(pool, job, "stopping_previous").await?;
    sqlx::query(
        "UPDATE installations SET runtime_status='deploying',desired_state='stopped' WHERE id=$1",
    )
    .bind(installation)
    .execute(pool)
    .await?;
    let name = format!("miy-app-{}", installation.simple());
    if inspect(&name).await.is_ok() {
        owned(&name, installation).await?;
        docker(&["stop", "--time", "10", &name]).await?;
        docker(&["rm", &name]).await?;
    }
    step(pool, job, "preparing_database").await?;
    let db_job = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,installation_id,release_id,actor,idempotency_key,input_hash,status) VALUES($1,$2,$3,$4,$5,$6,'running')").bind(db_job).bind(installation).bind(release).bind(actor).bind(format!("runtime-{job}-{}",Uuid::new_v4())).bind(contract::hash(job.to_string())).execute(pool).await?;
    let db = crate::worker::prepare(pool, db_job, installation, release).await;
    sqlx::query("UPDATE jobs SET status=$2,step=$3,error_code=$4,updated_at=now() WHERE id=$1")
        .bind(db_job)
        .bind(if db.is_ok() { "succeeded" } else { "failed" })
        .bind(if db.is_ok() { "ready" } else { "failed" })
        .bind(if db.is_ok() {
            None
        } else {
            Some("database_prepare_failed")
        })
        .execute(pool)
        .await?;
    if db.is_err() {
        sqlx::query("UPDATE installations SET db_status='failed' WHERE id=$1")
            .bind(installation)
            .execute(pool)
            .await?;
    }
    db?;
    let resource = sqlx::query("SELECT * FROM db_resources WHERE installation_id=$1")
        .bind(installation)
        .fetch_one(pool)
        .await?;
    let password = auth::decrypt(resource.get("runtime_secret"))?;
    let db_url = format!(
        "postgres://{}:{}@postgres:5432/{}",
        resource.get::<String, _>("runtime_role"),
        password,
        resource.get::<String, _>("database_name")
    );
    let secret = match inst.get::<Option<String>, _>("runtime_secret") {
        Some(value) => auth::decrypt(&value)?,
        None => auth::random(),
    };
    // Secrets are passed through a private temporary env file, never Docker argv or job output.
    use std::os::unix::fs::OpenOptionsExt;
    let env_path = std::env::temp_dir().join(format!("miy-runtime-{}.env", Uuid::new_v4()));
    let _secret_cleanup = RemoveFileOnDrop(env_path.clone());
    let mut env = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&env_path)?;
    std::io::Write::write_all(&mut env,format!("DATABASE_URL={db_url}\nMIY_GATEWAY_SECRET={secret}\nMIY_INSTALLATION_ID={installation}\nMIY_RELEASE_ID={release}\n").as_bytes())?;
    drop(env);
    step(pool, job, "creating_container").await?;
    let network = installation_network(installation).await?;
    let created = docker(&[
        "create",
        "--name",
        &name,
        "--network",
        &network,
        "--publish",
        "127.0.0.1::8080",
        "--env-file",
        env_path.to_str().unwrap(),
        "--user",
        "10001:10001",
        "--read-only",
        "--tmpfs",
        "/tmp:rw,noexec,nosuid,size=16m",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges",
        "--memory",
        "256m",
        "--cpus",
        "1",
        "--pids-limit",
        "128",
        "--restart",
        "unless-stopped",
        "--label",
        &format!("org.miy.installation={installation}"),
        "--label",
        &format!("org.miy.release={release}"),
        "--log-opt",
        "max-size=5m",
        "--log-opt",
        "max-file=2",
        image,
    ])
    .await;
    std::fs::remove_file(&env_path)?;
    created?;
    sqlx::query("UPDATE installations SET runtime_container=$2,runtime_image=$3,runtime_secret=$4,desired_state='running',runtime_status='starting' WHERE id=$1").bind(installation).bind(&name).bind(image).bind(auth::encrypt(&secret)?).execute(pool).await?;
    step(pool, job, "starting_container").await?;
    docker(&["start", &name]).await?;
    let meta = owned(&name, installation).await?;
    anyhow::ensure!(meta["Image"] == image, "running_image_mismatch");
    let port = published_port(&meta)?;
    sqlx::query("UPDATE installations SET runtime_port=$2 WHERE id=$1")
        .bind(installation)
        .bind(port)
        .execute(pool)
        .await?;
    step(pool, job, "health_check").await?;
    if let Err(error) = healthy(port, release).await {
        let _ = docker(&["stop", "--time", "5", &name]).await;
        return Err(error);
    }
    sqlx::query(
        "UPDATE installations SET runtime_status='running',desired_state='running' WHERE id=$1",
    )
    .bind(installation)
    .execute(pool)
    .await?;
    runtime_health::record(pool, installation, Observation::Healthy).await?;
    Ok(())
}
async fn installation_network(installation: Uuid) -> Result<String> {
    let name = format!("miy-runtime-{}", installation.simple());
    if let Ok(raw) = docker(&["network", "inspect", &name]).await {
        let metadata: Value = serde_json::from_str(&raw)?;
        anyhow::ensure!(
            metadata[0]["Labels"]["org.miy.installation"] == installation.to_string(),
            "unmanaged_network"
        );
    } else {
        docker(&[
            "network",
            "create",
            "--label",
            &format!("org.miy.installation={installation}"),
            &name,
        ])
        .await?;
    }
    let database = std::env::var("MIY_DATABASE_CONTAINER")
        .unwrap_or_else(|_| "miy-v2-validation-postgres-1".into());
    let meta = inspect(&database).await?;
    if meta["NetworkSettings"]["Networks"].get(&name).is_none() {
        docker(&[
            "network", "connect", "--alias", "postgres", &name, &database,
        ])
        .await?;
    }
    Ok(name)
}
/// Reconcile externally stopped/restarted containers. Database state is not fabricated.
pub async fn reconcile(pool: &sqlx::PgPool) -> Result<()> {
    let rows=sqlx::query("SELECT i.* FROM installations i WHERE runtime_container IS NOT NULL AND runtime_status NOT IN ('removed','failed') AND NOT EXISTS(SELECT 1 FROM runtime_jobs j WHERE j.app_id=i.app_id AND j.environment=i.environment AND j.status IN ('queued','running'))").fetch_all(pool).await?;
    for row in rows {
        let id: Uuid = row.get("id");
        let name: String = row.get("runtime_container");
        match owned(&name, id).await {
            Ok(meta) if meta["State"]["Running"] == true => {
                let matching = meta["Image"].as_str()
                    == row.get::<Option<String>, _>("runtime_image").as_deref()
                    && meta["Config"]["Labels"]["org.miy.release"]
                        == row.get::<Uuid, _>("release_id").to_string();
                if !matching || row.get::<String, _>("desired_state") != "running" {
                    let _ = docker(&["stop", "--time", "5", &name]).await;
                    sqlx::query("UPDATE installations SET runtime_status='failed',desired_state='stopped' WHERE id=$1").bind(id).execute(pool).await?;
                    runtime_health::record(pool, id, Observation::Inactive).await?;
                } else {
                    let port = published_port(&meta)?;
                    sqlx::query("UPDATE installations SET runtime_port=$2,runtime_status='running' WHERE id=$1").bind(id).bind(port).execute(pool).await?;
                }
            }
            Ok(_) => {
                sqlx::query("UPDATE installations SET runtime_status='stopped' WHERE id=$1")
                    .bind(id)
                    .execute(pool)
                    .await?;
                runtime_health::record(pool, id, Observation::Inactive).await?;
            }
            Err(_) => {
                sqlx::query("UPDATE installations SET runtime_status='unreachable' WHERE id=$1")
                    .bind(id)
                    .execute(pool)
                    .await?;
                runtime_health::record(pool, id, Observation::Unknown("container_unavailable"))
                    .await?;
            }
        }
    }
    Ok(())
}
fn published_port(meta: &Value) -> Result<i32> {
    Ok(meta["NetworkSettings"]["Ports"]["8080/tcp"][0]["HostPort"]
        .as_str()
        .context("missing_runtime_port")?
        .parse()?)
}
pub async fn proxy(
    S(state): S<State>,
    Extension(p): Extension<Principal>,
    Path((id, path)): Path<(Uuid, String)>,
    request: Request,
) -> ApiResult<Response> {
    let inst = domain::installation(&state, &p, id, false).await?;
    if inst.get::<String, _>("runtime_status") != "running"
        || inst.get::<String, _>("desired_state") != "running"
    {
        return Err(ApiError::unavailable("app_runtime_not_running"));
    }
    let release: Uuid = inst.get("release_id");
    if !matches!(*request.method(), Method::GET | Method::HEAD)
        && request
            .headers()
            .get("x-miy-release")
            .and_then(|v| v.to_str().ok())
            != Some(release.to_string().as_str())
    {
        return Err(ApiError::conflict("release_changed_reload_required"));
    }
    if path.contains('\\') || path.split('/').any(|p| p == ".." || p == ".") {
        return Err(ApiError::bad("invalid_api_path"));
    }
    let port: i32 = inst
        .get::<Option<i32>, _>("runtime_port")
        .ok_or_else(|| ApiError::unavailable("app_runtime_not_running"))?;
    let secret = auth::decrypt(
        inst.get::<Option<&str>, _>("runtime_secret")
            .ok_or_else(ApiError::not_found)?,
    )?;
    let token = sign_identity(
        &p.id,
        domain::admin(&state, &p).await.is_ok(),
        id,
        release,
        &secret,
    )?;
    let query = request
        .uri()
        .query()
        .map(|q| format!("?{q}"))
        .unwrap_or_default();
    let method = request.method().clone();
    let content_type = request.headers().get(header::CONTENT_TYPE).cloned();
    let body = to_bytes(request.into_body(), 4 * 1024 * 1024)
        .await
        .map_err(|_| ApiError::bad("request_too_large"))?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| ApiError::unavailable("app_gateway_failed"))?;
    let mut target = client
        .request(method, format!("http://127.0.0.1:{port}/api/{path}{query}"))
        .header("x-miy-identity", token)
        .body(body);
    if let Some(content_type) = content_type {
        target = target.header(header::CONTENT_TYPE, content_type);
    }
    let response = target
        .send()
        .await
        .map_err(|_| ApiError::unavailable("app_runtime_unreachable"))?;
    let status = response.status();
    let content_type = response.headers().get(header::CONTENT_TYPE).cloned();
    let bytes = response
        .bytes()
        .await
        .map_err(|_| ApiError::unavailable("app_response_failed"))?;
    let mut out = Response::builder().status(StatusCode::from_u16(status.as_u16()).unwrap());
    if let Some(ct) = content_type {
        out = out.header(header::CONTENT_TYPE, ct);
    }
    Ok(out
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(bytes))
        .unwrap())
}

#[cfg(test)]
mod tests {
    use super::image_id;
    #[test]
    fn image_references_are_immutable() {
        assert!(image_id(&format!("sha256:{}", "a".repeat(64))));
        for value in [
            "latest",
            "--privileged",
            "sha256:abc",
            "registry/app:latest",
        ] {
            assert!(!image_id(value));
        }
    }
}

#[cfg(test)]
mod acceptance_tests {
    use super::*;
    #[test]
    fn acceptance_scope_is_explicit_and_failure_is_not_approval() {
        let mut checks = vec![AcceptanceCheck {
            scope: "browser navigation".into(),
            actor: "developer".into(),
            result: "passed".into(),
            evidence: "cancelled shell/back/forward navigation preserved draft".into(),
        }];
        assert!(validate_checks(&checks).is_ok());
        checks[0].result = "failed".into();
        assert!(validate_checks(&checks).is_err());
        checks[0].result = "not_run".into();
        assert!(validate_checks(&checks).is_err());
        assert!(validate_checks(&[]).is_ok()); // Legacy CLI remains supported, explicitly unstructured.
    }
}
