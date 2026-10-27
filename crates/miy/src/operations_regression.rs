//! Owned PostgreSQL/HTTP/Docker audit coverage for operations contracts.
//! Docker runs only in the existing isolated worker subprocess. The HTTP core
//! queues diagnostics; fixture barriers delay real Docker logs, not controls.
use crate::{State, auth, contract::hash, runtime_contract::ExpectedState};
use anyhow::{Context, Result, ensure};
use axum::http::Method;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::{Client, RequestBuilder, Response};
use serde_json::{Value, json};
use sqlx::Row;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

struct Actor {
    id: String,
    token: String,
}
struct Harness<'a> {
    state: &'a State,
    client: &'a Client,
    base: &'a str,
    root: PathBuf,
}
fn id(value: &Value) -> Result<Uuid> {
    value["id"]
        .as_str()
        .context("missing public ID")?
        .parse()
        .context("invalid public ID")
}
async fn response(value: Response, status: u16) -> Result<Value> {
    ensure!(
        value.status().as_u16() == status,
        "expected HTTP {status}, received {}",
        value.status().as_u16()
    );
    Ok(value.json().await?)
}
async fn error(value: Response, status: u16, code: &str) -> Result<()> {
    let value = response(value, status).await?;
    ensure!(
        value["error"]["code"] == code,
        "expected stable code {code}"
    );
    Ok(())
}
impl Harness<'_> {
    fn request(&self, actor: &Actor, method: Method, path: &str) -> RequestBuilder {
        self.client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&actor.token)
    }
    async fn actor(&self) -> Result<Actor> {
        let id = format!("ops-audit-{}", Uuid::new_v4());
        let token = format!("miy_cli_{}", auth::random());
        sqlx::query("INSERT INTO users(id,name,can_develop) VALUES($1,$1,true)")
            .bind(&id)
            .execute(&self.state.pool)
            .await?;
        sqlx::query("INSERT INTO cli_tokens(token_hash,user_id) VALUES($1,$2)")
            .bind(hash(&token))
            .bind(&id)
            .execute(&self.state.pool)
            .await?;
        Ok(Actor { id, token })
    }
    async fn grant(&self, actor: &Actor, app: &str, env: &str, permission: &str) -> Result<()> {
        sqlx::query("INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING").bind(&actor.id).bind(app).bind(env).bind(permission).execute(&self.state.pool).await?;
        Ok(())
    }
    async fn snapshot(&self, env: &str) -> Result<Value> {
        Ok(serde_json::to_value(
            crate::runtime_preflight::snapshot(&self.state.pool, "audit-runtime", env)
                .await
                .map_err(|e| anyhow::anyhow!(e.code))?,
        )?)
    }
    async fn submit(
        &self,
        actor: &Actor,
        action: &str,
        env: &str,
        confirmed: bool,
        key: &str,
    ) -> Result<Value> {
        let expected = self.snapshot(env).await?;
        response(self.request(actor,Method::POST,"/api/v1/apps/audit-runtime/runtime").json(&json!({"action":action,"environment":env,"idempotencyKey":key,"expectedState":expected,"confirmed":confirmed})).send().await?,200).await
    }
    async fn job(&self, actor: &Actor, id: Uuid) -> Result<Value> {
        response(
            self.request(actor, Method::GET, &format!("/api/v1/runtime/jobs/{id}"))
                .send()
                .await?,
            200,
        )
        .await
    }
    async fn terminal(&self, actor: &Actor, id: Uuid) -> Result<Value> {
        tokio::time::timeout(Duration::from_secs(6), async {
            loop {
                let value = self.job(actor, id).await?;
                if matches!(value["status"].as_str(), Some("succeeded" | "failed")) {
                    return Ok::<_, anyhow::Error>(value);
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await?
    }
    async fn seed_job(
        &self,
        actor: &Actor,
        app: &str,
        env: &str,
        action: &str,
        state: (&str, Option<&str>, bool),
    ) -> Result<Uuid> {
        let (status, code, retryable) = state;
        let job = Uuid::new_v4();
        let input = json!({"action":action,"environment":env,"imageId":null,"releaseId":null,"idempotencyKey":format!("original-{job}"),"confirmed":false});
        sqlx::query("INSERT INTO runtime_jobs(id,app_id,environment,actor,action,idempotency_key,input_hash,status,step,error_code,retryable,failure_class,request_data) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'fixture',$9,$10,$11,$12)").bind(job).bind(app).bind(env).bind(&actor.id).bind(action).bind(format!("original-{job}")).bind(hash(job.to_string())).bind(status).bind(code).bind(retryable).bind(if retryable{"after_preflight"}else{"never"}).bind(input).execute(&self.state.pool).await?;
        Ok(job)
    }
    async fn log_request(&self, actor: &Actor, app: &str) -> Result<Uuid> {
        id(&response(
            self.request(actor, Method::POST, &format!("/api/v1/apps/{app}/logs"))
                .json(&json!({"environment":"dev","tail":1000}))
                .send()
                .await?,
            200,
        )
        .await?)
    }
    async fn log(&self, actor: &Actor, id: Uuid) -> Result<Value> {
        response(
            self.request(actor, Method::GET, &format!("/api/v1/runtime/logs/{id}"))
                .send()
                .await?,
            200,
        )
        .await
    }
}

async fn pages_and_events(
    h: &Harness<'_>,
    manager: &Actor,
    viewer: &Actor,
    other: &Actor,
    passed: &mut Vec<String>,
) -> Result<()> {
    let prefix = format!("aaa-ops-{}", &Uuid::new_v4().simple().to_string()[..10]);
    let a = format!("{prefix}-a");
    let hidden = format!("{prefix}-b");
    let c = format!("{prefix}-c");
    for app in [&a, &hidden, &c] {
        sqlx::query("INSERT INTO apps(id,name) VALUES($1,$1)")
            .bind(app)
            .execute(&h.state.pool)
            .await?;
    }
    for app in [&a, &c] {
        h.grant(manager, app, "dev", "develop").await?;
    }
    h.grant(other, &hidden, "dev", "develop").await?;
    h.grant(viewer, &a, "dev", "use").await?;
    let first = response(
        h.request(manager, Method::GET, "/api/v1/apps")
            .query(&[("limit", "1")])
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        first["apps"].as_array().is_some_and(|v| v.len() == 1) && first["apps"][0]["id"] == a,
        "list limit was applied before permissions"
    );
    let cursor = first["nextCursor"]
        .as_str()
        .context("missing app page cursor")?;
    let second = response(
        h.request(manager, Method::GET, "/api/v1/apps")
            .query(&[("limit", "1"), ("cursor", cursor)])
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        second["apps"][0]["id"] == c,
        "page revealed or stopped at an unauthorized app"
    );
    error(
        h.request(other, Method::GET, "/api/v1/apps")
            .query(&[("cursor", cursor)])
            .send()
            .await?,
        400,
        "invalid_cursor",
    )
    .await?;
    error(
        h.request(manager, Method::GET, "/api/v1/apps")
            .query(&[("cursor", cursor), ("appId", a.as_str())])
            .send()
            .await?,
        400,
        "invalid_cursor",
    )
    .await?;
    error(
        h.request(manager, Method::GET, "/api/v1/users")
            .send()
            .await?,
        403,
        "forbidden",
    )
    .await?;
    let direct = crate::read_api::page(
        h.state,
        &auth::Principal {
            cli_session_id: None,
            id: manager.id.clone(),
            scopes: vec!["miy.dev".into()],
            csrf: None,
            development: false,
        },
        "apps",
        crate::read_api::Page {
            limit: Some(1),
            ..Default::default()
        },
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.code))?;
    ensure!(
        direct["apps"] == first["apps"],
        "HTTP and domain page permissions differ"
    );
    let installed = response(
        h.request(viewer, Method::GET, "/api/v1/installations")
            .query(&[("appId", "audit-runtime"), ("environment", "prod")])
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        installed["installations"]
            .as_array()
            .is_some_and(|v| v.len() == 1)
            && installed["installations"][0]["canManage"] == false
            && installed["installations"][0]["acceptance"].is_null(),
        "viewer received management-only evidence"
    );
    let managed = response(
        h.request(manager, Method::GET, "/api/v1/installations")
            .query(&[("appId", "audit-runtime"), ("environment", "prod")])
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        managed["installations"][0]["canManage"] == true
            && !managed["installations"][0]["acceptance"].is_null(),
        "manager could not observe accepted production release"
    );
    sqlx::query("DELETE FROM grants WHERE user_id=$1 AND app_id=$2")
        .bind(&manager.id)
        .bind(&c)
        .execute(&h.state.pool)
        .await?;
    error(
        h.request(manager, Method::GET, &format!("/api/v1/apps/{c}"))
            .send()
            .await?,
        404,
        "not_found",
    )
    .await?;
    passed.push("permission-filtered keyset pages, actor/filter cursor scope and management-only acceptance evidence".into());

    let rolled = Uuid::new_v4();
    let mut tx = h.state.pool.begin().await?;
    sqlx::query("INSERT INTO runtime_jobs(id,app_id,environment,actor,action,idempotency_key,input_hash) VALUES($1,$2,'dev',$3,'start',$4,'fixture')").bind(rolled).bind(&a).bind(&manager.id).bind(format!("rollback-{rolled}")).execute(&mut *tx).await?;
    let inside: i64 = sqlx::query_scalar("SELECT count(*) FROM runtime_job_events WHERE job_id=$1")
        .bind(rolled)
        .fetch_one(&mut *tx)
        .await?;
    ensure!(
        inside == 1,
        "job insert did not record an event in its transaction"
    );
    tx.rollback().await?;
    ensure!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM runtime_job_events WHERE job_id=$1")
            .bind(rolled)
            .fetch_one(&h.state.pool)
            .await?
            == 0,
        "rollback left an event receipt"
    );
    let job = h
        .seed_job(manager, &a, "dev", "start", ("queued", None, false))
        .await?;
    let path = format!("/api/v1/runtime/jobs/{job}/events");
    let initial = response(
        h.request(manager, Method::GET, &path)
            .query(&[("limit", "1")])
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        initial["events"][0]["sequence"] == 1 && initial["terminal"] == false,
        "initial queued event is invalid"
    );
    let after = initial["nextCursor"]
        .as_str()
        .context("missing event cursor")?
        .to_owned();
    let mut tx = h.state.pool.begin().await?;
    sqlx::query("UPDATE runtime_jobs SET status='running',step='started' WHERE id=$1")
        .bind(job)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE runtime_jobs SET status='failed',step='complete',error_code='docker_unavailable',failure_class='after_preflight',retryable=true WHERE id=$1").bind(job).execute(&mut *tx).await?;
    let before_commit = response(h.request(manager, Method::GET, &path).send().await?, 200).await?;
    ensure!(
        before_commit["status"] == "queued"
            && before_commit["events"]
                .as_array()
                .is_some_and(|v| v.len() == 1)
            && before_commit["terminal"] == false,
        "uncommitted terminal state/event leaked"
    );
    tx.commit().await?;
    let middle = response(
        h.request(manager, Method::GET, &path)
            .query(&[("after", after.as_str()), ("limit", "1")])
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        middle["events"][0]["sequence"] == 2 && middle["terminal"] == false,
        "terminal was reported before all events were delivered"
    );
    let final_page = response(
        h.request(manager, Method::GET, &path)
            .query(&[
                ("after", middle["nextCursor"].as_str().unwrap()),
                ("limit", "1"),
            ])
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        final_page["events"][0]["sequence"] == 3
            && final_page["events"][0]["job"]["errorCode"] == "docker_unavailable"
            && final_page["terminal"] == true,
        "terminal page lost the failure event"
    );
    let empty = response(
        h.request(manager, Method::GET, &path)
            .query(&[("after", final_page["nextCursor"].as_str().unwrap())])
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        empty["events"].as_array().is_some_and(Vec::is_empty) && empty["terminal"] == true,
        "resume repeated or lost terminal events"
    );
    error(
        h.request(viewer, Method::GET, &path)
            .query(&[("after", after.as_str())])
            .send()
            .await?,
        400,
        "invalid_event_cursor",
    )
    .await?;
    let another = h
        .seed_job(
            manager,
            &a,
            "dev",
            "start",
            ("failed", Some("runtime_operation_failed"), false),
        )
        .await?;
    error(
        h.request(
            manager,
            Method::GET,
            &format!("/api/v1/runtime/jobs/{another}/events"),
        )
        .query(&[("after", after.as_str())])
        .send()
        .await?,
        400,
        "invalid_event_cursor",
    )
    .await?;
    let mut future: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(&after)?)?;
    future["seq"] = json!(999999);
    let future = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&future)?);
    error(
        h.request(manager, Method::GET, &path)
            .query(&[("after", future.as_str())])
            .send()
            .await?,
        400,
        "invalid_event_cursor",
    )
    .await?;
    sqlx::query("DELETE FROM grants WHERE user_id=$1 AND app_id=$2")
        .bind(&viewer.id)
        .bind(&a)
        .execute(&h.state.pool)
        .await?;
    error(
        h.request(viewer, Method::GET, &path).send().await?,
        403,
        "forbidden",
    )
    .await?;
    passed.push("transactional ordered job events, rollback, terminal pagination/resume and authority/cursor revocation".into());
    Ok(())
}

async fn admission_and_retry(
    h: &Harness<'_>,
    manager: &Actor,
    viewer: &Actor,
    passed: &mut Vec<String>,
) -> Result<()> {
    let dev = h.snapshot("dev").await?;
    let prod = h.snapshot("prod").await?;
    let before:Value=sqlx::query_scalar("SELECT jsonb_build_object('jobs',(SELECT count(*) FROM runtime_jobs),'uploads',(SELECT count(*) FROM image_uploads),'logs',(SELECT count(*) FROM runtime_log_requests))").fetch_one(&h.state.pool).await?;
    for (action, env) in [("stop", "dev"), ("remove", "dev"), ("stop", "prod")] {
        let plan = response(
            h.request(
                manager,
                Method::POST,
                "/api/v1/apps/audit-runtime/runtime/preflight",
            )
            .json(&json!({"target":{"appId":"audit-runtime","environment":env},"action":action}))
            .send()
            .await?,
            200,
        )
        .await?;
        ensure!(
            plan["canSubmit"] == true && plan["preservesDatabase"] == true,
            "read-only preflight did not describe valid operation"
        );
    }
    error(
        h.request(manager, Method::POST, "/api/v1/apps/audit-runtime/runtime")
            .json(&json!({"action":"stop","environment":"dev","idempotencyKey":Uuid::new_v4()}))
            .send()
            .await?,
        400,
        "expected_state_required",
    )
    .await?;
    for (action, env, expected) in [("remove", "dev", &dev), ("stop", "prod", &prod)] {
        error(h.request(manager,Method::POST,"/api/v1/apps/audit-runtime/runtime").json(&json!({"action":action,"environment":env,"idempotencyKey":Uuid::new_v4(),"expectedState":expected})).send().await?,400,"confirmation_required").await?;
    }
    let mut stale = dev.clone();
    stale["installation"]["generation"] =
        json!(stale["installation"]["generation"].as_u64().unwrap() + 1);
    error(h.request(manager,Method::POST,"/api/v1/apps/audit-runtime/runtime").json(&json!({"action":"stop","environment":"dev","idempotencyKey":Uuid::new_v4(),"expectedState":stale})).send().await?,409,"state_changed").await?;
    let after:Value=sqlx::query_scalar("SELECT jsonb_build_object('jobs',(SELECT count(*) FROM runtime_jobs),'uploads',(SELECT count(*) FROM image_uploads),'logs',(SELECT count(*) FROM runtime_log_requests))").fetch_one(&h.state.pool).await?;
    ensure!(
        before == after && dev == h.snapshot("dev").await? && prod == h.snapshot("prod").await?,
        "preflight or rejected requests created a side effect"
    );
    let _: ExpectedState = serde_json::from_value(dev.clone())?;
    passed.push("dry-run has zero remote mutations; expected-state, prod/remove confirmation and stale-state admission fail closed".into());

    let original = h
        .seed_job(
            manager,
            "audit-runtime",
            "dev",
            "start",
            ("failed", Some("docker_unavailable"), true),
        )
        .await?;
    let path = format!("/api/v1/runtime/jobs/{original}/retry");
    let key = Uuid::new_v4().to_string();
    let retry = json!({"appId":"audit-runtime","environment":"dev","idempotencyKey":key,"expectedState":dev,"confirmed":false});
    let mut missing = retry.clone();
    missing.as_object_mut().unwrap().remove("expectedState");
    ensure!(
        h.request(manager, Method::POST, &path)
            .json(&missing)
            .send()
            .await?
            .status()
            .as_u16()
            == 422,
        "retry accepted missing expected state"
    );
    error(
        h.request(viewer, Method::POST, &path)
            .json(&retry)
            .send()
            .await?,
        403,
        "forbidden",
    )
    .await?;
    let mut mismatch = retry.clone();
    mismatch["environment"] = json!("prod");
    error(
        h.request(manager, Method::POST, &path)
            .json(&mismatch)
            .send()
            .await?,
        400,
        "retry_target_mismatch",
    )
    .await?;
    let mut changed = retry.clone();
    changed["expectedState"] = stale.clone();
    error(
        h.request(manager, Method::POST, &path)
            .json(&changed)
            .send()
            .await?,
        409,
        "state_changed",
    )
    .await?;
    let accepted = response(
        h.request(manager, Method::POST, &path)
            .json(&retry)
            .send()
            .await?,
        200,
    )
    .await?;
    let child = id(&accepted)?;
    let duplicate = response(
        h.request(manager, Method::POST, &path)
            .json(&retry)
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        id(&duplicate)? == child && accepted["retryOf"] == original.to_string(),
        "retry did not preserve one receipt and original lineage"
    );
    error(
        h.request(manager, Method::POST, &path)
            .json(&changed)
            .send()
            .await?,
        409,
        "idempotency_conflict",
    )
    .await?;
    let mut busy = retry.clone();
    busy["idempotencyKey"] = json!(Uuid::new_v4());
    error(
        h.request(manager, Method::POST, &path)
            .json(&busy)
            .send()
            .await?,
        409,
        "operation_in_progress",
    )
    .await?;
    // This admission test never executes its queued start; the worker is started
    // later only after all synthetic jobs have been made terminal.
    sqlx::query("UPDATE runtime_jobs SET status='failed',error_code='runtime_operation_failed',retryable=false,failure_class='never' WHERE id=$1").bind(child).execute(&h.state.pool).await?;
    let recovered = response(
        h.request(manager, Method::GET, "/api/v1/runtime/jobs")
            .query(&[
                ("appId", "audit-runtime"),
                ("environment", "dev"),
                ("requestId", key.as_str()),
            ])
            .send()
            .await?,
        200,
    )
    .await?;
    ensure!(
        recovered["jobs"].as_array().is_some_and(|v| v.len() == 1)
            && recovered["jobs"][0]["id"] == child.to_string(),
        "request-ID recovery did not find the one accepted job"
    );
    for code in [
        "permission_revoked",
        "migration_failed",
        "immutable_release_conflict",
        "runtime_operation_failed",
        "prefix_docker_unavailable",
    ] {
        sqlx::query("UPDATE runtime_jobs SET error_code=$2,retryable=true WHERE id=$1")
            .bind(original)
            .bind(code)
            .execute(&h.state.pool)
            .await?;
        error(
            h.request(manager, Method::POST, &path)
                .json(&busy)
                .send()
                .await?,
            409,
            "job_not_retryable",
        )
        .await?;
    }
    for (env, action, expected) in [("prod", "stop", prod), ("dev", "remove", dev)] {
        let denied = h
            .seed_job(
                manager,
                "audit-runtime",
                env,
                action,
                ("failed", Some("docker_unavailable"), true),
            )
            .await?;
        error(h.request(manager,Method::POST,&format!("/api/v1/runtime/jobs/{denied}/retry")).json(&json!({"appId":"audit-runtime","environment":env,"idempotencyKey":Uuid::new_v4(),"expectedState":expected,"confirmed":false})).send().await?,400,"confirmation_required").await?;
    }
    passed.push("typed retry eligibility, fresh confirmation/state, request-ID recovery, idempotency and active-operation exclusion".into());
    Ok(())
}

async fn uploads(
    h: &Harness<'_>,
    manager: &Actor,
    peer: &Actor,
    passed: &mut Vec<String>,
) -> Result<Uuid> {
    let image:String=sqlx::query_scalar("SELECT runtime_image FROM installations WHERE app_id='audit-runtime' AND environment='dev'").fetch_one(&h.state.pool).await?;
    let bytes = b"isolated transport fixture; intentionally not an image archive";
    let digest = hash(bytes);
    let key = Uuid::new_v4();
    let upload = |actor: &Actor, digest: &str| {
        h.request(actor, Method::POST, "/api/v1/apps/audit-runtime/images")
            .header("x-miy-image", &image)
            .header("x-miy-archive-sha256", digest)
            .header("x-miy-request-id", key.to_string())
            .body(bytes.to_vec())
    };
    let lost = upload(manager, &digest).send().await?;
    ensure!(
        lost.status().is_success(),
        "initial upload was not accepted"
    );
    drop(lost);
    let lookup = format!("/api/v1/apps/audit-runtime/image-uploads/by-request/{key}");
    let recovered = response(h.request(manager, Method::GET, &lookup).send().await?, 200).await?;
    let original = id(&recovered)?;
    ensure!(
        recovered["archiveHash"] == digest
            && recovered["sizeBytes"] == bytes.len()
            && recovered["status"] == "pending",
        "lost-receipt lookup changed archive identity"
    );
    let duplicate = response(upload(manager, &digest).send().await?, 200).await?;
    ensure!(
        id(&duplicate)? == original,
        "same upload key created a duplicate receipt"
    );
    error(
        upload(manager, &hash(b"changed identity")).send().await?,
        409,
        "idempotency_conflict",
    )
    .await?;
    error(
        h.request(peer, Method::GET, &lookup).send().await?,
        404,
        "not_found",
    )
    .await?;
    error(
        h.request(
            peer,
            Method::GET,
            &format!("/api/v1/image-uploads/{original}"),
        )
        .send()
        .await?,
        403,
        "forbidden",
    )
    .await?;
    let other = response(upload(peer, &digest).send().await?, 200).await?;
    let other = id(&other)?;
    ensure!(
        other != original,
        "upload request IDs were globally shared across actors"
    );
    ensure!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM image_uploads WHERE actor=$1 AND request_id=$2"
        )
        .bind(&manager.id)
        .bind(key)
        .fetch_one(&h.state.pool)
        .await?
            == 1,
        "duplicate archive reception was persisted"
    );
    sqlx::query(
        "UPDATE image_uploads SET status='failed',error_code='fixture_complete' WHERE id=$1",
    )
    .bind(other)
    .execute(&h.state.pool)
    .await?;
    tokio::fs::remove_file(
        h.state
            .artifacts
            .join(".uploads")
            .join(format!("{other}.tar")),
    )
    .await?;
    sqlx::query(
        "UPDATE image_uploads SET expires_at=clock_timestamp()-interval '1 second' WHERE id=$1",
    )
    .bind(original)
    .execute(&h.state.pool)
    .await?;
    error(
        h.request(manager, Method::GET, &lookup).send().await?,
        409,
        "upload_expired",
    )
    .await?;
    error(
        upload(manager, &digest).send().await?,
        409,
        "upload_expired",
    )
    .await?;
    passed.push("lost upload receipt recovery, exact immutable identity, per-actor privacy and expired-key rejection".into());
    Ok(original)
}

fn worker(root: &Path, gate: &Path) -> Result<tokio::process::Child> {
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("evidence/operations-worker.log"))?;
    let mut command = tokio::process::Command::new(std::env::current_exe()?);
    command
        .args([
            "--exact",
            "runtime_regression::worker_child",
            "--ignored",
            "--nocapture",
        ])
        .env("AUDIT_LOG_GATE", gate)
        .env_remove("MIY_FAILPOINT")
        .env_remove("AUDIT_CRASH_REMOVE")
        .env_remove("AUDIT_FAIL_LIST")
        .kill_on_drop(true)
        .stdout(log.try_clone()?)
        .stderr(log);
    Ok(command.spawn()?)
}
async fn wait_log(h: &Harness<'_>, actor: &Actor, id: Uuid) -> Result<Value> {
    tokio::time::timeout(Duration::from_secs(12), async {
        loop {
            let result = h.log(actor, id).await?;
            if matches!(result["status"].as_str(), Some("ready" | "failed")) {
                return Ok::<_, anyhow::Error>(result);
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    })
    .await?
}

async fn diagnostics(
    h: &Harness<'_>,
    manager: &Actor,
    peer: &Actor,
    viewer: &Actor,
    expired_upload: Uuid,
    passed: &mut Vec<String>,
) -> Result<()> {
    let dev = sqlx::query(
        "SELECT * FROM installations WHERE app_id='audit-runtime' AND environment='dev'",
    )
    .fetch_one(&h.state.pool)
    .await?;
    let dev_id: Uuid = dev.get("id");
    response(
        h.request(
            manager,
            Method::GET,
            &format!("/api/v1/installations/{dev_id}/api/audit-log-burst"),
        )
        .send()
        .await?,
        200,
    )
    .await?;
    error(
        h.request(viewer, Method::POST, "/api/v1/apps/audit-runtime/logs")
            .json(&json!({"environment":"dev"}))
            .send()
            .await?,
        403,
        "forbidden",
    )
    .await?;
    for tail in [0, 1001] {
        error(
            h.request(manager, Method::POST, "/api/v1/apps/audit-runtime/logs")
                .json(&json!({"environment":"dev","tail":tail}))
                .send()
                .await?,
            400,
            "invalid_log_request",
        )
        .await?;
    }
    let foreign_app = format!("ops-log-{}", &Uuid::new_v4().simple().to_string()[..10]);
    let foreign_install = Uuid::new_v4();
    let foreign_release = Uuid::new_v4();
    sqlx::query("INSERT INTO apps(id,name) VALUES($1,'Ownership log fixture')")
        .bind(&foreign_app)
        .execute(&h.state.pool)
        .await?;
    sqlx::query("INSERT INTO releases(id,app_id,version,artifact_hash,manifest,image_id) SELECT $1,$2,'1.0.0',artifact_hash,jsonb_set(manifest,'{id}',to_jsonb($2::text)),image_id FROM releases WHERE id=$3").bind(foreign_release).bind(&foreign_app).bind(dev.get::<Uuid,_>("release_id")).execute(&h.state.pool).await?;
    sqlx::query("INSERT INTO installations(id,app_id,environment,release_id,db_status,runtime_status,desired_state,runtime_container,runtime_image) VALUES($1,$2,'dev',$3,'ready','running','running',$4,$5)").bind(foreign_install).bind(&foreign_app).bind(foreign_release).bind(dev.get::<String,_>("runtime_container")).bind(dev.get::<String,_>("runtime_image")).execute(&h.state.pool).await?;
    h.grant(manager, &foreign_app, "dev", "develop").await?;
    let foreign = h.log_request(manager, &foreign_app).await?;
    let stale = h.log_request(manager, "audit-runtime").await?;
    let mut tx = h.state.pool.begin().await?;
    sqlx::query("UPDATE installations SET desired_state='stopped' WHERE id=$1")
        .bind(dev_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE installations SET desired_state='running' WHERE id=$1")
        .bind(dev_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let revoked = h.log_request(peer, "audit-runtime").await?;
    sqlx::query(
        "DELETE FROM grants WHERE user_id=$1 AND app_id='audit-runtime' AND environment='dev'",
    )
    .bind(&peer.id)
    .execute(&h.state.pool)
    .await?;
    let expired = h.log_request(manager, "audit-runtime").await?;
    sqlx::query("UPDATE runtime_log_requests SET expires_at=clock_timestamp()-interval '1 second' WHERE id=$1").bind(expired).execute(&h.state.pool).await?;
    error(
        h.request(
            manager,
            Method::GET,
            &format!("/api/v1/runtime/logs/{expired}"),
        )
        .send()
        .await?,
        404,
        "not_found",
    )
    .await?;
    // Stop the fixture before starting the bounded diagnostics. PID 1 in this
    // synthetic Python container may consume Docker's full ten-second stop
    // grace; the independence assertion below measures the fast start command.
    let stopped = h
        .submit(
            manager,
            "stop",
            "prod",
            true,
            &format!("prepare-log-independence-{}", Uuid::new_v4()),
        )
        .await?;
    crate::runtime::tick(&h.state.pool).await?;
    ensure!(
        h.job(manager, id(&stopped)?).await?["status"] == "succeeded",
        "diagnostic control fixture could not stop"
    );
    let first = h.log_request(manager, "audit-runtime").await?;
    let second = h.log_request(manager, "audit-runtime").await?;
    error(
        h.request(peer, Method::GET, &format!("/api/v1/runtime/logs/{first}"))
            .send()
            .await?,
        403,
        "forbidden",
    )
    .await?;
    let gate = h.root.join(format!("evidence/log-gate-{}", Uuid::new_v4()));
    let mut runner = worker(&h.root, &gate)?;
    let result = async {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let markers = tokio::fs::read_to_string(format!("{}.started", gate.display()))
                    .await
                    .unwrap_or_default();
                if markers.lines().count() >= 2 {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .context("two diagnostics did not reach the owned worker barrier")?;
        for action in ["start"] {
            let accepted = h
                .submit(
                    manager,
                    action,
                    "prod",
                    true,
                    &format!("log-independent-{action}-{}", Uuid::new_v4()),
                )
                .await?;
            let job = h.terminal(manager, id(&accepted)?).await?;
            ensure!(
                job["status"] == "succeeded",
                "a gated diagnostic blocked production control"
            );
        }
        ensure!(
            !gate.exists(),
            "diagnostic barrier opened before the control assertions"
        );
        let running: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM runtime_log_requests WHERE id=ANY($1) AND status='running'",
        )
        .bind(&[first, second][..])
        .fetch_one(&h.state.pool)
        .await?;
        ensure!(
            running == 2,
            "diagnostics timed out before independent controls completed"
        );
        tokio::fs::write(&gate, b"release").await?;
        for request in [first, second] {
            let log = wait_log(h, manager, request).await?;
            let text = log["text"]
                .as_str()
                .context("log collection did not return text")?;
            ensure!(
                log["status"] == "ready"
                    && log["truncated"] == true
                    && text.len() <= 1024 * 1024
                    && text.contains("OPERATIONS-FIXTURE-")
                    && log["rotationMayOmit"] == true
                    && log["tail"] == 1000,
                "finite log snapshot violated contract: status={} truncated={} bytes={} marker={} rotation={} tail={}", log["status"],log["truncated"],text.len(),text.contains("OPERATIONS-FIXTURE-"),log["rotationMayOmit"],log["tail"]
            );
            ensure!(
                log["container"].as_str().is_some_and(|id| id.len() == 64)
                    && log["generation"].as_i64().is_some(),
                "log target identity/generation absent"
            );
        }
        let foreign = wait_log(h, manager, foreign).await?;
        ensure!(
            foreign["errorCode"] == "container_ownership_mismatch" && foreign["text"].is_null(),
            "foreign installation container logs were disclosed"
        );
        let stale = wait_log(h, manager, stale).await?;
        ensure!(
            stale["errorCode"] == "state_changed" && stale["text"].is_null(),
            "previous control-generation logs were disclosed"
        );
        let code: String =
            sqlx::query_scalar("SELECT error_code FROM runtime_log_requests WHERE id=$1")
                .bind(revoked)
                .fetch_one(&h.state.pool)
                .await?;
        ensure!(
            code == "permission_revoked",
            "worker did not recheck diagnostic authority"
        );
        error(
            h.request(
                peer,
                Method::GET,
                &format!("/api/v1/runtime/logs/{revoked}"),
            )
            .send()
            .await?,
            403,
            "forbidden",
        )
        .await?;
        sqlx::query(
            "DELETE FROM grants WHERE user_id=$1 AND app_id='audit-runtime' AND environment='dev'",
        )
        .bind(&manager.id)
        .execute(&h.state.pool)
        .await?;
        error(
            h.request(
                manager,
                Method::GET,
                &format!("/api/v1/runtime/logs/{first}"),
            )
            .send()
            .await?,
            403,
            "forbidden",
        )
        .await?;
        h.grant(manager, "audit-runtime", "dev", "develop").await?;
        let upload = sqlx::query("SELECT status,error_code FROM image_uploads WHERE id=$1")
            .bind(expired_upload)
            .fetch_one(&h.state.pool)
            .await?;
        ensure!(
            upload.get::<String, _>("status") == "failed"
                && upload.get::<Option<String>, _>("error_code").as_deref()
                    == Some("upload_expired")
                && !h
                    .state
                    .artifacts
                    .join(".uploads")
                    .join(format!("{expired_upload}.tar"))
                    .exists(),
            "worker imported or retained an expired pending archive"
        );
        ensure!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM runtime_log_requests WHERE id=$1")
                .bind(expired)
                .fetch_one(&h.state.pool)
                .await?
                == 0,
            "worker did not delete expired diagnostic content"
        );
        Ok::<_, anyhow::Error>(())
    }
    .await;
    // Always release the fixture barrier and reap only our own worker process.
    let _ = tokio::fs::write(&gate, b"release").await;
    let _ = runner.kill().await;
    let _ = runner.wait().await;
    sqlx::query("DELETE FROM runtime_log_requests WHERE installation_id=$1")
        .bind(foreign_install)
        .execute(&h.state.pool)
        .await?;
    sqlx::query("DELETE FROM installations WHERE id=$1")
        .bind(foreign_install)
        .execute(&h.state.pool)
        .await?;
    sqlx::query("DELETE FROM releases WHERE id=$1")
        .bind(foreign_release)
        .execute(&h.state.pool)
        .await?;
    result?;
    passed.push("real worker logs are capped at 1MiB, ownership/generation/permission/TTL checked; two gated diagnostics do not block prod start".into());
    Ok(())
}

pub async fn run(state: &State, client: &Client, base: &str) -> Result<Vec<String>> {
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&state.pool)
        .await?;
    ensure!(
        database == "miy_audit_core",
        "owned operations audit database required"
    );
    let root = PathBuf::from(std::env::var("MIY_AUDIT_ROOT")?);
    ensure!(
        root.join("private-env.json").is_file(),
        "owned audit environment required"
    );
    let config: Value = serde_json::from_slice(&std::fs::read(root.join("config.json"))?)?;
    ensure!(
        std::env::var("MIY_DATABASE_CONTAINER")?
            == config["databaseContainer"]
                .as_str()
                .context("missing owned database")?,
        "audit database ownership mismatch"
    );
    let url = url::Url::parse(base)?;
    ensure!(
        matches!(url.host(),Some(url::Host::Ipv4(ip)) if ip.is_loopback())
            || matches!(url.host(),Some(url::Host::Ipv6(ip)) if ip.is_loopback())
            || url.host_str() == Some("localhost"),
        "loopback API required"
    );
    let h = Harness {
        state,
        client,
        base: base.trim_end_matches('/'),
        root,
    };
    let manager = h.actor().await?;
    let peer = h.actor().await?;
    let viewer = h.actor().await?;
    let other = h.actor().await?;
    h.grant(&manager, "audit-runtime", "dev", "develop").await?;
    h.grant(&manager, "audit-runtime", "prod", "deploy").await?;
    h.grant(&peer, "audit-runtime", "dev", "develop").await?;
    for env in ["dev", "prod"] {
        h.grant(&viewer, "audit-runtime", env, "use").await?;
    }
    let mut passed = vec![];
    pages_and_events(&h, &manager, &viewer, &other, &mut passed)
        .await
        .context("operation pages/events")?;
    admission_and_retry(&h, &manager, &viewer, &mut passed)
        .await
        .context("operation admission/retry")?;
    let expired = uploads(&h, &manager, &peer, &mut passed)
        .await
        .context("operation upload recovery")?;
    diagnostics(&h, &manager, &peer, &viewer, expired, &mut passed)
        .await
        .context("worker diagnostics")?;
    Ok(passed)
}
