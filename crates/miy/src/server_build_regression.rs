//! Opt-in HTTP/PostgreSQL build admission regression. No Docker builds run here.
//! The harness owns a fresh database/container; no platform credentials are used.
use crate::{State, auth, contract::hash, server_build};
use anyhow::{Context, Result, ensure};
use axum::Json;
use reqwest::{Client, Method, RequestBuilder, Response};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use uuid::Uuid;

const APP: &str = "build-audit";
struct Actor {
    id: String,
    token: String,
}
struct Harness {
    state: State,
    client: Client,
    origin: String,
}
fn passed(name: &str) {
    println!("PASS server-build: {name}");
}
fn id(value: &Value) -> Result<Uuid> {
    Ok(value["id"].as_str().context("missing build ID")?.parse()?)
}
async fn response(value: Response, expected: u16) -> Result<Value> {
    ensure!(
        value.status().as_u16() == expected,
        "expected HTTP {expected}, received {}",
        value.status().as_u16()
    );
    Ok(value.json().await?)
}
async fn error(value: Response, expected: u16, code: &str) -> Result<()> {
    ensure!(
        response(value, expected).await?["error"]["code"] == code,
        "unexpected public error code"
    );
    Ok(())
}
impl Harness {
    fn request(&self, actor: &Actor, method: Method, path: &str) -> RequestBuilder {
        self.client
            .request(method, format!("{}{path}", self.origin))
            .bearer_auth(&actor.token)
    }
    fn upload(&self, actor: &Actor, key: Uuid, payload: &[u8]) -> RequestBuilder {
        self.request(actor, Method::POST, &format!("/api/v1/apps/{APP}/builds"))
            .header("content-length", payload.len())
            .header("x-miy-request-id", key.to_string())
            .header("x-miy-source-commit", "a".repeat(40))
            .header("x-miy-platform", "linux/amd64")
            .header("x-miy-archive-sha256", hash(payload))
    }
    async fn post(&self, actor: &Actor, key: Uuid, payload: &[u8]) -> Result<Value> {
        response(
            self.upload(actor, key, payload)
                .body(payload.to_vec())
                .send()
                .await?,
            200,
        )
        .await
    }
    async fn by_request(&self, actor: &Actor, key: Uuid) -> Result<Value> {
        response(
            self.request(
                actor,
                Method::GET,
                &format!("/api/v1/apps/{APP}/build-requests/{key}"),
            )
            .send()
            .await?,
            200,
        )
        .await
    }
    async fn actor(&self, permission: &str) -> Result<Actor> {
        let id = format!("build-audit-{}", Uuid::new_v4());
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
        sqlx::query(
            "INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,$2,'dev',$3)",
        )
        .bind(&id)
        .bind(APP)
        .bind(permission)
        .execute(&self.state.pool)
        .await?;
        Ok(Actor { id, token })
    }
    async fn finish_fixtures(&self) -> Result<()> {
        sqlx::query("UPDATE app_builds SET status='failed',step='fixture_complete',error_code='fixture_complete',finished_at=now() WHERE status IN ('queued','running','receiving')").execute(&self.state.pool).await?;
        Ok(())
    }
    async fn wait_partial(&self, actor: &Actor, key: Uuid) -> Result<Uuid> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let row = sqlx::query_scalar::<_, Uuid>(
                    "SELECT id FROM app_builds WHERE actor=$1 AND request_id=$2",
                )
                .bind(&actor.id)
                .bind(key)
                .fetch_optional(&self.state.pool)
                .await?;
                if let Some(id) = row
                    && std::fs::metadata(server_build::root()?.join(format!("{id}.part")))
                        .is_ok_and(|m| m.len() > 0)
                {
                    return Ok::<_, anyhow::Error>(id);
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await?
    }
}

async fn identities(h: &Harness, actor: &Actor) -> Result<()> {
    let payload = b"synthetic source archive admission bytes";
    let key = Uuid::new_v4();
    let (first, second) = tokio::join!(h.post(actor, key, payload), h.post(actor, key, payload));
    let first = first?;
    let second = second?;
    let build = id(&first)?;
    ensure!(id(&second)? == build, "parallel replay duplicated a build");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app_builds WHERE actor=$1")
        .bind(&actor.id)
        .fetch_one(&h.state.pool)
        .await?;
    ensure!(count == 1, "parallel request created multiple receipts");
    ensure!(
        h.by_request(actor, key).await?["status"] == "queued",
        "complete source did not queue"
    );
    ensure!(
        crate::source_archive::digest(&server_build::archive_path(build)?)? == hash(payload),
        "published source hash differs"
    );
    let alias = Uuid::new_v4();
    let again = h.post(actor, alias, payload).await?;
    ensure!(
        id(&again)? == build && again["requestId"] == alias.to_string(),
        "content alias did not retain one canonical build"
    );
    ensure!(
        id(&h.by_request(actor, alias).await?)? == build,
        "alias recovery returned another job"
    );
    error(
        h.upload(actor, key, b"changed source")
            .body(b"changed source".to_vec())
            .send()
            .await?,
        409,
        "idempotency_conflict",
    )
    .await?;
    sqlx::query("UPDATE app_builds SET status='failed',error_code='fixture_failure' WHERE id=$1")
        .bind(build)
        .execute(&h.state.pool)
        .await?;
    ensure!(
        id(&h.post(actor, key, payload).await?)? == build,
        "same failed request started new work"
    );
    let new = h.post(actor, Uuid::new_v4(), payload).await?;
    ensure!(
        id(&new)? != build,
        "new explicit request could not retry a failed build"
    );
    passed(
        "concurrent same-request dedup, content aliases, input conflicts and failed-request identity",
    );
    Ok(())
}
async fn interrupted(h: &Harness, actor: &Actor) -> Result<()> {
    h.finish_fixtures().await?;
    let payload = b"correct complete upload";
    let key = Uuid::new_v4();
    error(
        h.upload(actor, key, payload)
            .body(vec![b'x'; payload.len()])
            .send()
            .await?,
        400,
        "invalid_source_archive",
    )
    .await?;
    let incomplete = h.by_request(actor, key).await?;
    let build = id(&incomplete)?;
    ensure!(
        incomplete["status"] == "receiving" && incomplete["step"] == "upload_incomplete",
        "bad hash lost resumable identity"
    );
    let part = server_build::root()?.join(format!("{build}.part"));
    ensure!(
        !part.exists() && !server_build::archive_path(build)?.exists(),
        "failed transfer retained bytes"
    );
    let mut lock = h.state.pool.acquire().await?;
    lock.close_on_drop();
    sqlx::query("SELECT pg_advisory_lock(hashtextextended($1,0))")
        .bind(format!("build-upload:{build}"))
        .execute(&mut *lock)
        .await?;
    std::fs::write(&part, b"another active receiver")?;
    let blocked = h.post(actor, key, payload).await?;
    ensure!(
        id(&blocked)? == build
            && blocked["status"] == "receiving"
            && std::fs::read(&part)? == b"another active receiver",
        "reconnect disturbed active transfer"
    );
    sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1,0))")
        .bind(format!("build-upload:{build}"))
        .execute(&mut *lock)
        .await?;
    drop(lock);
    let resumed = h.post(actor, key, payload).await?;
    ensure!(
        id(&resumed)? == build && resumed["status"] == "queued" && !part.exists(),
        "incomplete upload did not reuse original receipt"
    );
    // The source was atomically published but the queue update was lost.
    sqlx::query("UPDATE app_builds SET status='receiving',step='receiving' WHERE id=$1")
        .bind(build)
        .execute(&h.state.pool)
        .await?;
    let recovered = h.post(actor, key, payload).await?;
    ensure!(
        id(&recovered)? == build && recovered["status"] == "queued",
        "published source receipt did not recover"
    );
    passed(
        "hash failure cleanup, active transfer exclusion and reconnect after publication-before-queue",
    );
    Ok(())
}
async fn lost_upload_lock(h: &Harness, actor: &Actor) -> Result<()> {
    h.finish_fixtures().await?;
    let key = Uuid::new_v4();
    let payload = b"source survives an interrupted database upload lock";
    let (send, recv) =
        tokio::sync::mpsc::channel::<std::result::Result<Vec<u8>, std::io::Error>>(2);
    let body = futures_util::stream::unfold(recv, |mut recv| async {
        recv.recv().await.map(|chunk| (chunk, recv))
    });
    let request = h
        .upload(actor, key, payload)
        .body(reqwest::Body::wrap_stream(body));
    let pending = tokio::spawn(async move { request.send().await });
    send.send(Ok(payload[..8].to_vec())).await?;
    let build = h.wait_partial(actor, key).await?;
    // Terminate only this disposable database's connection holding our exact
    // build-upload lock. A normal replay can now own the source transfer.
    let pid: i32 = sqlx::query_scalar("SELECT pid FROM pg_locks WHERE locktype='advisory' AND granted AND objsubid=1 AND classid=((hashtextextended($1,0)>>32)&4294967295)::oid AND objid=(hashtextextended($1,0)&4294967295)::oid")
        .bind(format!("build-upload:{build}"))
        .fetch_one(&h.state.pool)
        .await?;
    let stopped: bool = sqlx::query_scalar("SELECT pg_terminate_backend($1)")
        .bind(pid)
        .fetch_one(&h.state.pool)
        .await?;
    ensure!(stopped, "fixture upload connection was not stopped");
    let resumed = h.post(actor, key, payload).await?;
    ensure!(
        id(&resumed)? == build && resumed["status"] == "queued",
        "lock-loss replay did not recover original job"
    );
    send.send(Ok(payload[8..].to_vec())).await?;
    drop(send);
    let abandoned = pending.await??;
    ensure!(
        abandoned.status().is_server_error(),
        "abandoned upload retained its lost lock"
    );
    ensure!(
        crate::source_archive::digest(&server_build::archive_path(build)?)? == hash(payload),
        "abandoned upload removed or replaced resumed bytes"
    );
    ensure!(
        h.by_request(actor, key).await?["status"] == "queued",
        "abandoned upload changed resumed state"
    );
    passed("lost upload lock cannot remove a completed reconnect's source");
    Ok(())
}
async fn permissions(h: &Harness, actor: &Actor) -> Result<()> {
    h.finish_fixtures().await?;
    let payload = b"authorization fixture";
    let key = Uuid::new_v4();
    let unauth = h
        .client
        .post(format!("{}/api/v1/apps/{APP}/builds", h.origin))
        .body(payload.to_vec())
        .send()
        .await?;
    ensure!(
        unauth.status().as_u16() == 401,
        "anonymous source upload was accepted"
    );
    let viewer = h.actor("use").await?;
    error(
        h.upload(&viewer, Uuid::new_v4(), payload)
            .body(payload.to_vec())
            .send()
            .await?,
        403,
        "forbidden",
    )
    .await?;
    let job = h.post(actor, key, payload).await?;
    let build = id(&job)?;
    let other = h.actor("develop").await?;
    for suffix in [
        format!("/api/v1/builds/{build}"),
        format!("/api/v1/builds/{build}/logs"),
    ] {
        error(
            h.request(&other, Method::GET, &suffix).send().await?,
            403,
            "forbidden",
        )
        .await?;
    }
    sqlx::query("DELETE FROM grants WHERE user_id=$1 AND app_id=$2")
        .bind(&actor.id)
        .bind(APP)
        .execute(&h.state.pool)
        .await?;
    error(
        h.request(actor, Method::GET, &format!("/api/v1/builds/{build}"))
            .send()
            .await?,
        403,
        "forbidden",
    )
    .await?;
    error(
        h.upload(actor, key, payload)
            .body(payload.to_vec())
            .send()
            .await?,
        403,
        "forbidden",
    )
    .await?;
    sqlx::query("INSERT INTO grants VALUES($1,$2,'dev','develop')")
        .bind(&actor.id)
        .bind(APP)
        .execute(&h.state.pool)
        .await?;
    // Revoke while the actual HTTP body is suspended between two chunks.
    let key = Uuid::new_v4();
    let payload = b"permission changes midway through this upload";
    let (send, recv) =
        tokio::sync::mpsc::channel::<std::result::Result<Vec<u8>, std::io::Error>>(2);
    let body = futures_util::stream::unfold(recv, |mut recv| async {
        recv.recv().await.map(|chunk| (chunk, recv))
    });
    let request = h
        .upload(actor, key, payload)
        .body(reqwest::Body::wrap_stream(body));
    let pending = tokio::spawn(async move { request.send().await });
    send.send(Ok(payload[..8].to_vec())).await?;
    let interrupted_id = h.wait_partial(actor, key).await?;
    sqlx::query("DELETE FROM grants WHERE user_id=$1 AND app_id=$2")
        .bind(&actor.id)
        .bind(APP)
        .execute(&h.state.pool)
        .await?;
    send.send(Ok(payload[8..].to_vec())).await?;
    drop(send);
    error(pending.await??, 403, "forbidden").await?;
    let status: String = sqlx::query_scalar("SELECT status FROM app_builds WHERE id=$1")
        .bind(interrupted_id)
        .fetch_one(&h.state.pool)
        .await?;
    ensure!(
        status == "receiving",
        "revoked actor queued a completed upload"
    );
    sqlx::query("INSERT INTO grants VALUES($1,$2,'dev','develop')")
        .bind(&actor.id)
        .bind(APP)
        .execute(&h.state.pool)
        .await?;
    ensure!(
        id(&h.post(actor, key, payload).await?)? == interrupted_id,
        "grant restoration lost resumable identity"
    );
    sqlx::query("UPDATE users SET enabled=false WHERE id=$1")
        .bind(&actor.id)
        .execute(&h.state.pool)
        .await?;
    let denied = h
        .request(actor, Method::GET, &format!("/api/v1/builds/{build}"))
        .send()
        .await?;
    ensure!(
        matches!(denied.status().as_u16(), 401 | 403),
        "disabled user retained build access"
    );
    sqlx::query("UPDATE users SET enabled=true WHERE id=$1")
        .bind(&actor.id)
        .execute(&h.state.pool)
        .await?;
    // Disabling legitimately revokes the old CLI token; this fixture issues a new one.
    sqlx::query("INSERT INTO cli_tokens(token_hash,user_id) VALUES($1,$2) ON CONFLICT(token_hash) DO NOTHING").bind(hash(&actor.token)).bind(&actor.id).execute(&h.state.pool).await?;
    passed(
        "anonymous/use-only/other-actor denial, read revocation, mid-upload revocation and disabled user",
    );
    Ok(())
}
async fn quotas(h: &Harness, actor: &Actor) -> Result<()> {
    h.finish_fixtures().await?;
    h.post(actor, Uuid::new_v4(), b"actor quota one").await?;
    h.post(actor, Uuid::new_v4(), b"actor quota two").await?;
    error(
        h.upload(actor, Uuid::new_v4(), b"actor quota three")
            .body(b"actor quota three".to_vec())
            .send()
            .await?,
        409,
        "build_queue_full",
    )
    .await?;
    h.finish_fixtures().await?;
    let mut actors = Vec::new();
    for _ in 0..12 {
        actors.push(h.actor("develop").await?);
    }
    let requests = actors.iter().enumerate().map(|(n, a)| {
        h.upload(a, Uuid::new_v4(), format!("global quota {n}").as_bytes())
            .body(format!("global quota {n}"))
            .send()
    });
    let outcomes = futures_util::future::join_all(requests).await;
    let mut accepted = 0;
    let mut denied = 0;
    for result in outcomes {
        let result = result?;
        match result.status().as_u16() {
            200 => {
                accepted += 1;
            }
            409 => {
                error(result, 409, "build_queue_full").await?;
                denied += 1;
            }
            _ => anyhow::bail!("unexpected quota response"),
        }
    }
    ensure!(
        accepted == 8 && denied == 4,
        "parallel global admission exceeded bounded queue"
    );
    let total: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app_builds WHERE status IN ('queued','running')")
            .fetch_one(&h.state.pool)
            .await?;
    ensure!(total == 8, "global cap differs from committed queue");
    passed("actor and concurrent global queue limits");
    Ok(())
}
async fn log_pages(h: &Harness, actor: &Actor) -> Result<()> {
    h.finish_fixtures().await?;
    let build = id(&h
        .post(actor, Uuid::new_v4(), b"bounded log fixture")
        .await?)?;
    let chunks: Vec<_> = (0..12).map(|n| format!("검사 {n}\0\x1b\t완료\n")).collect();
    let results = futures_util::future::join_all(
        chunks
            .iter()
            .map(|text| server_build::append_log(&h.state.pool, build, text)),
    )
    .await;
    for result in results {
        result?;
    }
    let first = response(
        h.request(
            actor,
            Method::GET,
            &format!("/api/v1/builds/{build}/logs?after=0&limit=2"),
        )
        .send()
        .await?,
        200,
    )
    .await?;
    ensure!(
        first["logs"].as_array().is_some_and(|v| v.len() == 2)
            && first["nextSequence"] == 2
            && first["terminal"] == false,
        "initial log page is invalid"
    );
    for row in first["logs"].as_array().unwrap() {
        ensure!(
            !row["text"].as_str().unwrap().contains(['\0', '\x1b']),
            "terminal control survived log persistence"
        );
    }
    server_build::append_log(
        &h.state.pool,
        build,
        &"x".repeat(server_build::MAX_LOG_BYTES + 100),
    )
    .await?;
    server_build::append_log(&h.state.pool, build, "discard after cap").await?;
    let bytes: i64 =
        sqlx::query_scalar("SELECT sum(octet_length(text)) FROM app_build_logs WHERE build_id=$1")
            .bind(build)
            .fetch_one(&h.state.pool)
            .await?;
    ensure!(
        bytes == server_build::MAX_LOG_BYTES as i64,
        "durable logs exceeded or miscounted byte cap"
    );
    sqlx::query("UPDATE app_builds SET status='failed',error_code='fixture_terminal' WHERE id=$1")
        .bind(build)
        .execute(&h.state.pool)
        .await?;
    let last = response(
        h.request(
            actor,
            Method::GET,
            &format!("/api/v1/builds/{build}/logs?after=2"),
        )
        .send()
        .await?,
        200,
    )
    .await?;
    ensure!(
        last["terminal"] == true && last["truncated"] == true,
        "terminal pagination lost truncation/terminal state"
    );
    let logs = last["logs"].as_array().context("logs array absent")?;
    for (n, line) in logs.iter().enumerate() {
        ensure!(
            line["sequence"] == (n + 3) as u64,
            "concurrent log sequence gap"
        );
    }
    let maximum = last["nextSequence"]
        .as_i64()
        .context("last sequence absent")?;
    let empty = response(
        h.request(
            actor,
            Method::GET,
            &format!("/api/v1/builds/{build}/logs?after={maximum}"),
        )
        .send()
        .await?,
        200,
    )
    .await?;
    ensure!(
        empty["logs"].as_array().unwrap().is_empty() && empty["terminal"] == true,
        "resume repeated final logs"
    );
    for after in [-1, maximum + 1] {
        error(
            h.request(
                actor,
                Method::GET,
                &format!("/api/v1/builds/{build}/logs?after={after}"),
            )
            .send()
            .await?,
            400,
            "invalid_build_log_cursor",
        )
        .await?;
    }
    passed(
        "concurrent ordered logs, UTF-8/control sanitation, exact2MiB cap and reconnect cursors",
    );
    Ok(())
}

#[tokio::test]
#[ignore = "disposable PostgreSQL only; run scripts/check-server-builds.py"]
async fn durable_server_build_api() -> Result<()> {
    let root = PathBuf::from(std::env::var("MIY_SERVER_BUILD_AUDIT")?);
    let config: Value = serde_json::from_slice(&std::fs::read(root.join("fixture.json"))?)?;
    ensure!(
        config["kind"] == "server-build-api" && config["database"] == "miy_build_audit",
        "not an owned build fixture"
    );
    let url = url::Url::parse(&std::env::var("DATABASE_URL")?)?;
    ensure!(
        url.host_str() == Some("127.0.0.1")
            && url.path() == "/miy_build_audit"
            && url.port().is_some(),
        "refusing non-fixture database"
    );
    ensure!(
        std::env::var("MIY_ARTIFACTS")? == root.join("artifacts").to_string_lossy(),
        "fixture artifacts differ"
    );
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(24)
        .connect(url.as_str())
        .await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.tables WHERE table_schema='public'",
    )
    .fetch_one(&pool)
    .await?;
    ensure!(count == 0, "fixture database is not fresh");
    let migrations = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../migrations/core");
    let old = tempfile::tempdir()?;
    for file in std::fs::read_dir(&migrations)? {
        let file = file?;
        if file.file_name().to_string_lossy().as_ref() < "0015" {
            std::fs::copy(file.path(), old.path().join(file.file_name()))?;
        }
    }
    sqlx::migrate::Migrator::new(old.path())
        .await?
        .run(&pool)
        .await?;
    sqlx::query("INSERT INTO apps(id,name) VALUES('build-preserved','Preserved app')")
        .execute(&pool)
        .await?;
    sqlx::migrate::Migrator::new(migrations.as_path())
        .await?
        .run(&pool)
        .await?;
    let retained: String = sqlx::query_scalar("SELECT name FROM apps WHERE id='build-preserved'")
        .fetch_one(&pool)
        .await?;
    ensure!(
        retained == "Preserved app",
        "additive build migration changed existing apps"
    );
    passed("0014-to-current migration preserves unrelated app");
    sqlx::query("INSERT INTO apps(id,name) VALUES($1,'Build API fixture')")
        .bind(APP)
        .execute(&pool)
        .await?;
    let identity = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let issuer = format!("http://{}", identity.local_addr()?);
    let api = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let origin = format!("http://{}", api.local_addr()?);
    let metadata = json!({"issuer":issuer,"authorization_endpoint":format!("{issuer}/authorize"),"token_endpoint":format!("{issuer}/token"),"jwks_uri":format!("{issuer}/jwks"),"introspection_endpoint":format!("{issuer}/introspect"),"end_session_endpoint":format!("{issuer}/logout")});
    let router = axum::Router::new()
        .route(
            "/.well-known/openid-configuration",
            axum::routing::get(move || {
                let value = metadata.clone();
                async move { Json(value) }
            }),
        )
        .route(
            "/jwks",
            axum::routing::get(|| async { Json(json!({"keys":[]})) }),
        );
    let oidc = tokio::spawn(async move { axum::serve(identity, router).await });
    unsafe {
        std::env::set_var("MIY_ORIGIN", &origin);
        std::env::set_var("MIY_ISSUER", &issuer);
        std::env::set_var("MIY_DEV_LOGIN", "0");
    }
    let state = State {
        pool: pool.clone(),
        auth: Arc::new(auth::Auth::new().await?),
        artifacts: root.join("artifacts"),
    };
    let router = crate::http::router(state.clone());
    let server = tokio::spawn(async move { axum::serve(api, router).await });
    let h = Harness {
        state,
        client: Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()?,
        origin,
    };
    let actor = h.actor("develop").await?;
    identities(&h, &actor).await.context("request identities")?;
    interrupted(&h, &actor).await.context("upload recovery")?;
    lost_upload_lock(&h, &actor)
        .await
        .context("upload connection loss")?;
    permissions(&h, &actor)
        .await
        .context("permission boundaries")?;
    quotas(&h, &actor).await.context("queue bounds")?;
    log_pages(&h, &actor)
        .await
        .context("bounded log observations")?;
    let untouched: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM runtime_jobs),(SELECT count(*) FROM installations)",
    )
    .fetch_one(&pool)
    .await?;
    ensure!(
        untouched == (0, 0),
        "build admission mutated runtime lifecycle state"
    );
    let summary = json!({"passed":true,"suite":"server-build-api","storage":"owned disposable PostgreSQL","transport":"real loopback HTTP","groups":8,"dockerBuildExecuted":false,"runtimeJobsUnchanged":true});
    std::fs::write(
        root.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    server.abort();
    oidc.abort();
    pool.close().await;
    Ok(())
}
