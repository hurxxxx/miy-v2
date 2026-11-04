//! Actual HTTP/PostgreSQL delegation boundaries inside the owned runtime fixture.
use crate::{State, auth, contract};
use anyhow::{Result, ensure};
use reqwest::{Client, Method, StatusCode};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

struct Harness<'a> {
    client: &'a Client,
    base: &'a str,
    user: String,
    parent: Uuid,
    token: String,
}
impl Harness<'_> {
    async fn request(
        &self,
        token: &str,
        method: Method,
        path: &str,
        body: Option<Value>,
        expected: StatusCode,
    ) -> Result<Value> {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await?;
        let status = response.status();
        let value: Value = response.json().await?;
        ensure!(
            status == expected,
            "delegation HTTP {path}: expected {expected}, got {status}; code={}",
            value["error"]["code"].as_str().unwrap_or("none")
        );
        Ok(value)
    }
    async fn create(&self, environment: &str, actions: &[&str]) -> Result<Value> {
        self.request(&self.token,Method::POST,"/api/v1/delegations",Some(json!({"name":"Isolated agent","appId":"audit-runtime","environment":environment,"actions":actions,"expiresIn":3600})),StatusCode::OK).await
    }
    async fn denied(&self, token: &str) -> Result<()> {
        let response = self
            .client
            .get(format!("{}/api/v1/me", self.base))
            .bearer_auth(token)
            .send()
            .await?;
        ensure!(
            matches!(response.status().as_u16(), 401 | 403),
            "inactive delegation retained access"
        );
        Ok(())
    }
}
fn id(value: &Value, path: &str) -> Uuid {
    value[path].as_str().unwrap().parse().unwrap()
}
fn access(value: &Value) -> &str {
    value["credential"]["accessToken"].as_str().unwrap()
}

async fn cli_import(base: &str, issued: &Value) -> Result<()> {
    let root = tempfile::tempdir()?;
    let credential = root.path().join("delegation.json");
    let tokens = &issued["credential"];
    let expiry =
        chrono::DateTime::parse_from_rfc3339(tokens["refreshExpiresAt"].as_str().unwrap())?
            .timestamp();
    crate::platform::write_private(
        &credential,
        &serde_json::to_vec(&json!({
            "schemaVersion":1,"delegationId":issued["delegation"]["id"],
            "credential":{"schemaVersion":1,"origin":base,"userId":tokens["userId"],"sessionId":tokens["sessionId"],
            "accessToken":tokens["accessToken"],"refreshToken":tokens["refreshToken"],
            "expiresAt":chrono::Utc::now().timestamp()+tokens["expiresIn"].as_i64().unwrap(),
            "refreshExpiresAt":expiry,"reauthRequired":false}
        }))?,
    )?;
    let commands: Vec<(Vec<&str>, i32)> = vec![
        (vec!["context", "add", "delegation", "--url", base], 0),
        (
            vec![
                "auth",
                "delegations",
                "import",
                "--credential-file",
                credential.to_str().unwrap(),
            ],
            0,
        ),
        (vec!["auth", "status"], 0),
        (vec!["--app", "audit-runtime", "app", "status"], 0),
        (vec!["user", "list"], 4),
        (
            vec![
                "auth",
                "delegations",
                "import",
                "--credential-file",
                credential.to_str().unwrap(),
            ],
            6,
        ),
    ];
    for (args, expected) in commands {
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            tokio::process::Command::new(std::env::var("MIY_CLI_TEST_BINARY")?)
                .args(["--output", "json", "--no-input"])
                .args(&args)
                .current_dir(root.path())
                .env("MIY_CONFIG_DIR", root.path().join("config"))
                .env("MIY_CREDENTIAL_BACKEND", "file")
                .env("RUST_LOG", "off")
                .env_remove("MIY_CONTEXT")
                .kill_on_drop(true)
                .output(),
        )
        .await??;
        for bytes in [&output.stdout, &output.stderr] {
            let text = String::from_utf8_lossy(bytes);
            ensure!(
                !text.contains("miy_access_") && !text.contains("miy_refresh_"),
                "delegation CLI printed a credential"
            );
        }
        let value: Value = serde_json::from_slice(&output.stdout)?;
        ensure!(
            output.status.code() == Some(expected),
            "delegation CLI expected exit {expected}, got {:?}, code={}",
            output.status.code(),
            value["error"]["code"].as_str().unwrap_or("none")
        );
    }
    Ok(())
}

async fn lease_cleanup_scope(state: &State, h: &Harness<'_>, other_app_token: &str) -> Result<()> {
    let app = format!(
        "delegate-cleanup-{}",
        &Uuid::new_v4().simple().to_string()[..12]
    );
    sqlx::query("INSERT INTO apps(id,name) VALUES($1,'Delegation cleanup fixture')")
        .bind(&app)
        .execute(&state.pool)
        .await?;
    let resource = crate::worker::ensure_resource(&state.pool, &app, "dev", None).await?;
    let lease = Uuid::new_v4();
    // No database role/tunnel is created: these are durable lease rows whose
    // close authorization is exercised through the actual HTTP endpoint.
    sqlx::query("INSERT INTO app_dev_sessions(id,app_id,database_id,actor,parent_session_id,idempotency_key,input_hash,request_data,role_name,connection_secret,tunnel_token_hash) VALUES($1,$2,$3,$4,$5,$6,$6,'{}',$7,$8,$6)")
        .bind(lease).bind(&app).bind(resource.id).bind(&h.user).bind(h.parent)
        .bind(contract::hash(lease.as_bytes())).bind(format!("miy_d_{}",lease.simple()))
        .bind(auth::encrypt("{}")?).execute(&state.pool).await?;
    let path = format!("/api/v1/dev-sessions/{lease}");
    h.request(
        other_app_token,
        Method::DELETE,
        &path,
        None,
        StatusCode::FORBIDDEN,
    )
    .await?;
    let read=h.request(&h.token,Method::POST,"/api/v1/delegations",Some(json!({"name":"Read cleanup fixture","appId":app,"environment":"dev","actions":["read"],"expiresIn":3600})),StatusCode::OK).await?;
    h.request(
        access(&read),
        Method::DELETE,
        &path,
        None,
        StatusCode::FORBIDDEN,
    )
    .await?;
    let developer=h.request(&h.token,Method::POST,"/api/v1/delegations",Some(json!({"name":"Developer cleanup fixture","appId":app,"environment":"dev","actions":["dev_session"],"expiresIn":3600})),StatusCode::OK).await?;
    h.request(
        access(&developer),
        Method::DELETE,
        &path,
        None,
        StatusCode::FORBIDDEN,
    )
    .await?;
    let status: String = sqlx::query_scalar("SELECT status FROM app_dev_sessions WHERE id=$1")
        .bind(lease)
        .fetch_one(&state.pool)
        .await?;
    ensure!(
        status == "queued",
        "scoped agent closed another credential family's lease"
    );
    // Preserve the existing undelegated cleanup policy even without app grants.
    sqlx::query("UPDATE users SET admin=false WHERE id=$1")
        .bind(&h.user)
        .execute(&state.pool)
        .await?;
    let closed = h
        .request(&h.token, Method::DELETE, &path, None, StatusCode::OK)
        .await?;
    ensure!(closed["status"] == "closed");
    sqlx::query("UPDATE users SET admin=true WHERE id=$1")
        .bind(&h.user)
        .execute(&state.pool)
        .await?;
    let own = Uuid::new_v4();
    sqlx::query("INSERT INTO app_dev_sessions(id,app_id,database_id,actor,parent_session_id,idempotency_key,input_hash,request_data,role_name,connection_secret,tunnel_token_hash) VALUES($1,$2,$3,$4,$5,$6,$6,'{}',$7,$8,$6)")
        .bind(own).bind(&app).bind(resource.id).bind(&h.user).bind(id(&developer["credential"],"sessionId"))
        .bind(contract::hash(own.as_bytes())).bind(format!("miy_d_{}",own.simple()))
        .bind(auth::encrypt("{}")?).execute(&state.pool).await?;
    let closed = h
        .request(
            access(&developer),
            Method::DELETE,
            &format!("/api/v1/dev-sessions/{own}"),
            None,
            StatusCode::OK,
        )
        .await?;
    ensure!(closed["status"] == "closed");
    sqlx::query("DELETE FROM app_dev_sessions WHERE id=ANY($1)")
        .bind(vec![lease, own])
        .execute(&state.pool)
        .await?;
    Ok(())
}

/// These are synthetic metadata and HTTP admission tests. The harness advances
/// workers explicitly; no Docker action runs for this app, and its one admitted
/// job is made terminal before the next worker tick.
async fn runtime_job_boundaries(state: &State, h: &Harness<'_>) -> Result<Vec<String>> {
    let app = format!(
        "delegated-jobs-{}",
        &Uuid::new_v4().simple().to_string()[..12]
    );
    let release = Uuid::new_v4();
    sqlx::query("INSERT INTO apps(id,name) VALUES($1,'Runtime job permission fixture')")
        .bind(&app)
        .execute(&state.pool)
        .await?;
    sqlx::query("INSERT INTO releases(id,app_id,version,artifact_hash,manifest) SELECT $1,$2,'1.0.0',artifact_hash,jsonb_set(manifest,'{id}',to_jsonb($2::text)) FROM releases WHERE app_id='audit-runtime' LIMIT 1")
        .bind(release).bind(&app).execute(&state.pool).await?;
    let mut historical_jobs = Vec::new();
    for environment in ["dev", "prod"] {
        sqlx::query("INSERT INTO installations(id,app_id,environment,release_id,db_status,runtime_status,desired_state,runtime_image) VALUES($1,$2,$3,$4,'ready','running','running',$5)")
            .bind(Uuid::new_v4()).bind(&app).bind(environment).bind(release)
            .bind(format!("sha256:{}", "a".repeat(64))).execute(&state.pool).await?;
        for _ in 0..2 {
            let job = Uuid::new_v4();
            sqlx::query("INSERT INTO runtime_jobs(id,app_id,environment,actor,action,idempotency_key,input_hash,status,step) VALUES($1,$2,$3,$4,'stop',$5,$5,'succeeded','done')")
                .bind(job).bind(&app).bind(environment).bind(&h.user).bind(job.to_string())
                .execute(&state.pool).await?;
            historical_jobs.push((environment, job));
        }
    }
    let viewer = Harness {
        client: h.client,
        base: h.base,
        user: format!("job-reader-{}", Uuid::new_v4()),
        parent: Uuid::new_v4(),
        token: format!("miy_access_{}", auth::random()),
    };
    sqlx::query("INSERT INTO users(id,name) VALUES($1,'Job reader fixture')")
        .bind(&viewer.user)
        .execute(&state.pool)
        .await?;
    sqlx::query("INSERT INTO cli_sessions(id,user_id,client_id,client_name) VALUES($1,$2,$3,'Job reader fixture')")
        .bind(viewer.parent).bind(&viewer.user).bind(Uuid::new_v4()).execute(&state.pool).await?;
    sqlx::query("INSERT INTO cli_access_tokens(token_hash,session_id,expires_at) VALUES($1,$2,now()+interval '15 minutes')")
        .bind(contract::hash(&viewer.token)).bind(viewer.parent).execute(&state.pool).await?;
    for environment in ["dev", "prod"] {
        sqlx::query(
            "INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,$2,$3,'use')",
        )
        .bind(&viewer.user)
        .bind(&app)
        .bind(environment)
        .execute(&state.pool)
        .await?;
    }
    let list = format!("/api/v1/runtime/jobs?appId={app}&environment=prod&limit=1");
    let first_page = h
        .request(&viewer.token, Method::GET, &list, None, StatusCode::OK)
        .await?;
    let cursor = first_page["nextCursor"]
        .as_str()
        .expect("job fixture needs a second page");
    let next = format!("{list}&cursor={cursor}");
    let viewer_delegation = viewer.request(&viewer.token, Method::POST, "/api/v1/delegations",
        Some(json!({"name":"Scoped reader","appId":app,"environment":"prod","actions":["read"],"expiresIn":3600})), StatusCode::OK).await?;
    let delegated_page = h
        .request(
            access(&viewer_delegation),
            Method::GET,
            &list,
            None,
            StatusCode::OK,
        )
        .await?;
    let delegated_next = format!(
        "{list}&cursor={}",
        delegated_page["nextCursor"].as_str().unwrap()
    );
    h.request(
        &h.token,
        Method::PUT,
        &format!("/api/v1/apps/{app}/control"),
        Some(json!({"disabled":true})),
        StatusCode::OK,
    )
    .await?;

    let mut issued = Vec::new();
    for actions in [vec!["read", "stop"], vec!["stop"]] {
        issued.push(h.request(&h.token, Method::POST, "/api/v1/delegations",
            Some(json!({"name":"Scoped runtime fixture","appId":app,"environment":"prod","actions":actions,"expiresIn":3600})), StatusCode::OK).await?);
    }
    let delegated = access(&issued[0]);
    let stop_only = access(&issued[1]);
    let preflight = format!("/api/v1/apps/{app}/runtime/preflight");
    let submit = format!("/api/v1/apps/{app}/runtime");
    let plan_input = json!({"target":{"appId":app,"environment":"prod"},"action":"stop"});
    let plan = h
        .request(
            delegated,
            Method::POST,
            &preflight,
            Some(plan_input.clone()),
            StatusCode::OK,
        )
        .await?;
    ensure!(plan["canSubmit"] == true);
    h.request(
        stop_only,
        Method::POST,
        &preflight,
        Some(plan_input),
        StatusCode::FORBIDDEN,
    )
    .await?;
    let no_read_key = Uuid::new_v4().to_string();
    let mut request = json!({"action":"stop","environment":"prod","idempotencyKey":no_read_key,"confirmed":true,"expectedState":plan["expectedState"]});
    h.request(
        stop_only,
        Method::POST,
        &submit,
        Some(request.clone()),
        StatusCode::FORBIDDEN,
    )
    .await?;
    let denied_rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM runtime_jobs WHERE actor=$1 AND idempotency_key=$2",
    )
    .bind(&h.user)
    .bind(&no_read_key)
    .fetch_one(&state.pool)
    .await?;
    ensure!(
        denied_rows == 0,
        "read-less delegation left a committed job"
    );
    request["idempotencyKey"] = json!(Uuid::new_v4().to_string());
    let accepted = h
        .request(
            delegated,
            Method::POST,
            &submit,
            Some(request.clone()),
            StatusCode::OK,
        )
        .await?;
    let job = id(&accepted, "id");
    ensure!(accepted["status"] == "queued");
    let row = sqlx::query("SELECT status,delegation_id FROM runtime_jobs WHERE id=$1")
        .bind(job)
        .fetch_one(&state.pool)
        .await?;
    ensure!(
        row.get::<String, _>("status") == "queued"
            && row.get::<Uuid, _>("delegation_id") == id(&issued[0]["delegation"], "id")
    );
    let replay = h
        .request(
            delegated,
            Method::POST,
            &submit,
            Some(request),
            StatusCode::OK,
        )
        .await?;
    ensure!(id(&replay, "id") == job);
    let accepted_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM runtime_jobs WHERE delegation_id=$1")
            .bind(id(&issued[0]["delegation"], "id"))
            .fetch_one(&state.pool)
            .await?;
    ensure!(
        accepted_rows == 1,
        "idempotent runtime request inserted twice"
    );
    for suffix in ["", "/events"] {
        let path = format!("/api/v1/runtime/jobs/{job}{suffix}");
        h.request(delegated, Method::GET, &path, None, StatusCode::OK)
            .await?;
        h.request(stop_only, Method::GET, &path, None, StatusCode::FORBIDDEN)
            .await?;
        let denied = h
            .request(
                &viewer.token,
                Method::GET,
                &path,
                None,
                StatusCode::FORBIDDEN,
            )
            .await?;
        ensure!(denied["error"]["code"] == "app_disabled");
    }
    // Cross-environment detail and list checks must retain the delegation boundary.
    h.request(
        delegated,
        Method::GET,
        &format!("/api/v1/runtime/jobs/{}", historical_jobs[0].1),
        None,
        StatusCode::FORBIDDEN,
    )
    .await?;
    h.request(
        delegated,
        Method::GET,
        &format!("/api/v1/runtime/jobs?appId={app}&environment=dev"),
        None,
        StatusCode::FORBIDDEN,
    )
    .await?;
    h.request(
        delegated,
        Method::GET,
        "/api/v1/runtime/jobs?appId=audit-runtime&environment=prod",
        None,
        StatusCode::FORBIDDEN,
    )
    .await?;
    let visible = h
        .request(
            delegated,
            Method::GET,
            &format!("/api/v1/runtime/jobs?appId={app}&environment=prod&limit=200"),
            None,
            StatusCode::OK,
        )
        .await?;
    ensure!(
        visible["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| id(value, "id") == job)
    );
    // No worker action is claimed for these metadata-only fixtures.
    sqlx::query("UPDATE runtime_jobs SET status='failed',step='fixture_complete' WHERE id=$1")
        .bind(job)
        .execute(&state.pool)
        .await?;

    for token in [viewer.token.as_str(), access(&viewer_delegation)] {
        let page = h
            .request(token, Method::GET, &list, None, StatusCode::OK)
            .await?;
        ensure!(page["jobs"].as_array().unwrap().is_empty() && page["nextCursor"].is_null());
    }
    let disabled_next = h
        .request(&viewer.token, Method::GET, &next, None, StatusCode::OK)
        .await?;
    ensure!(disabled_next["jobs"].as_array().unwrap().is_empty());
    let disabled_delegated = h
        .request(
            access(&viewer_delegation),
            Method::GET,
            &delegated_next,
            None,
            StatusCode::OK,
        )
        .await?;
    ensure!(disabled_delegated["jobs"].as_array().unwrap().is_empty());
    h.request(
        &h.token,
        Method::PUT,
        &format!("/api/v1/apps/{app}/control"),
        Some(json!({"disabled":false})),
        StatusCode::OK,
    )
    .await?;
    let enabled_next = h
        .request(&viewer.token, Method::GET, &next, None, StatusCode::OK)
        .await?;
    ensure!(!enabled_next["jobs"].as_array().unwrap().is_empty());
    h.request(
        &viewer.token,
        Method::GET,
        &format!("/api/v1/runtime/jobs/{job}"),
        None,
        StatusCode::OK,
    )
    .await?;
    sqlx::query("DELETE FROM grants WHERE user_id=$1 AND app_id=$2")
        .bind(&viewer.user)
        .bind(&app)
        .execute(&state.pool)
        .await?;
    let revoked_next = h
        .request(&viewer.token, Method::GET, &next, None, StatusCode::OK)
        .await?;
    ensure!(revoked_next["jobs"].as_array().unwrap().is_empty());
    h.request(
        &viewer.token,
        Method::GET,
        &format!("/api/v1/runtime/jobs/{job}"),
        None,
        StatusCode::FORBIDDEN,
    )
    .await?;
    h.request(
        access(&viewer_delegation),
        Method::GET,
        &delegated_next,
        None,
        StatusCode::FORBIDDEN,
    )
    .await?;

    // CLI access tokens carry miy.dev. Exercise the same SQL with read-only OIDC
    // Principal scopes directly, including the deliberate dev repair exception.
    h.request(
        &h.token,
        Method::PUT,
        &format!("/api/v1/apps/{app}/control"),
        Some(json!({"disabled":true})),
        StatusCode::OK,
    )
    .await?;
    for (admin, permission, env, has_dev, allowed) in [
        (true, "use", "prod", true, true),
        (true, "use", "prod", false, false),
        (true, "use", "dev", false, true),
        (false, "use", "prod", true, false),
        (false, "use", "dev", true, false),
        (false, "deploy", "prod", true, true),
        (false, "deploy", "prod", false, false),
        (false, "develop", "dev", true, true),
        (false, "develop", "dev", false, true),
        (false, "develop", "prod", true, false),
    ] {
        sqlx::query("UPDATE users SET admin=$2 WHERE id=$1")
            .bind(&viewer.user)
            .bind(admin)
            .execute(&state.pool)
            .await?;
        sqlx::query("DELETE FROM grants WHERE user_id=$1 AND app_id=$2")
            .bind(&viewer.user)
            .bind(&app)
            .execute(&state.pool)
            .await?;
        sqlx::query(
            "INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,$2,$3,$4)",
        )
        .bind(&viewer.user)
        .bind(&app)
        .bind(env)
        .bind(permission)
        .execute(&state.pool)
        .await?;
        let mut principal = auth::Principal {
            id: viewer.user.clone(),
            cli_session_id: Some(viewer.parent),
            delegation_id: None,
            scopes: vec!["miy.read".into()],
            csrf: None,
            development: false,
        };
        if has_dev {
            principal.scopes.push("miy.dev".into());
        }
        let page = crate::read_api::page(
            state,
            &principal,
            "jobs",
            crate::read_api::Page {
                app_id: Some(app.clone()),
                environment: Some(env.into()),
                ..Default::default()
            },
        )
        .await
        .map_err(|error| anyhow::anyhow!(error.code))?;
        let detail = crate::runtime::job(
            state,
            &principal,
            historical_jobs
                .iter()
                .find(|(environment, _)| *environment == env)
                .unwrap()
                .1,
        )
        .await;
        ensure!(
            detail.is_ok() == allowed && page["jobs"].as_array().unwrap().is_empty() != allowed,
            "runtime job scope mismatch: admin={admin}, permission={permission}, env={env}, miy.dev={has_dev}"
        );
    }
    sqlx::query(
        "UPDATE installations SET desired_state='stopped',runtime_status='stopped' WHERE app_id=$1",
    )
    .bind(&app)
    .execute(&state.pool)
    .await?;
    Ok(vec![
        "delegated disabled runtime read+stop returns one queued receipt and idempotent replay; missing read is denied by planning/submission before insertion".into(),
        "runtime job detail/events and cursor pages enforce company disable, current grants and delegated app/environment/read scope".into(),
        "runtime job list/detail agree for admin, deploy, develop and use with actual miy.dev scope, including read-only dev repair".into(),
    ])
}

pub async fn run(state: &State, client: &Client, base: &str) -> Result<Vec<String>> {
    let user = format!("delegation-audit-{}", Uuid::new_v4());
    let parent = Uuid::new_v4();
    let token = format!("miy_access_{}", auth::random());
    sqlx::query("INSERT INTO users(id,name,admin) VALUES($1,'Delegation fixture',true)")
        .bind(&user)
        .execute(&state.pool)
        .await?;
    sqlx::query("INSERT INTO cli_sessions(id,user_id,client_id,client_name) VALUES($1,$2,$3,'Delegation fixture owner')").bind(parent).bind(&user).bind(Uuid::new_v4()).execute(&state.pool).await?;
    sqlx::query("INSERT INTO cli_access_tokens(token_hash,session_id,expires_at) VALUES($1,$2,now()+interval '15 minutes')").bind(contract::hash(&token)).bind(parent).execute(&state.pool).await?;
    let h = Harness {
        client,
        base,
        user,
        parent,
        token,
    };
    let mut passed = vec![];
    let first = h.create("prod", &["read", "start"]).await?;
    cli_import(base, &first).await?;
    let delegated = access(&first);
    lease_cleanup_scope(state, &h, delegated).await?;
    let me = h
        .request(delegated, Method::GET, "/api/v1/me", None, StatusCode::OK)
        .await?;
    ensure!(me["admin"] == false && me["delegation"]["environment"] == "prod");
    for (app, env, action) in [
        ("audit-runtime", "prod", "stop"),
        ("audit-runtime", "dev", "start"),
        ("another-app", "prod", "start"),
    ] {
        h.request(
            delegated,
            Method::POST,
            &format!("/api/v1/apps/{app}/runtime/preflight"),
            Some(json!({"target":{"appId":app,"environment":env},"action":action})),
            StatusCode::FORBIDDEN,
        )
        .await?;
    }
    h.request(
        delegated,
        Method::GET,
        "/api/v1/users",
        None,
        StatusCode::FORBIDDEN,
    )
    .await?;
    h.request(
        delegated,
        Method::GET,
        "/api/v1/cli/sessions",
        None,
        StatusCode::FORBIDDEN,
    )
    .await?;
    h.request(delegated,Method::POST,"/api/v1/delegations",Some(json!({"name":"Nested","appId":"audit-runtime","environment":"prod","actions":["start"],"expiresIn":3600})),StatusCode::FORBIDDEN).await?;
    let installs = h
        .request(
            delegated,
            Method::GET,
            "/api/v1/installations?limit=200",
            None,
            StatusCode::OK,
        )
        .await?;
    ensure!(
        installs["installations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["appId"] == "audit-runtime" && v["environment"] == "prod")
    );
    passed.push("actual CLI private delegation import and explicit app/environment/actions; scoped lease cleanup rejects other apps/families while undelegated grant-loss cleanup remains available".into());

    passed.extend(runtime_job_boundaries(state, &h).await?);

    let plan = h
        .request(
            delegated,
            Method::POST,
            "/api/v1/apps/audit-runtime/runtime/preflight",
            Some(json!({"target":{"appId":"audit-runtime","environment":"prod"},"action":"start"})),
            StatusCode::OK,
        )
        .await?;
    let request_id = Uuid::new_v4().to_string();
    let request = json!({"action":"start","environment":"prod","idempotencyKey":request_id,"expectedState":plan["expectedState"],"confirmed":true});
    let job = h
        .request(
            delegated,
            Method::POST,
            "/api/v1/apps/audit-runtime/runtime",
            Some(request.clone()),
            StatusCode::OK,
        )
        .await?;
    let second = h.create("prod", &["read", "start"]).await?;
    h.request(
        access(&second),
        Method::POST,
        "/api/v1/apps/audit-runtime/runtime",
        Some(request),
        StatusCode::CONFLICT,
    )
    .await?;
    h.request(
        &h.token,
        Method::DELETE,
        &format!(
            "/api/v1/delegations/{}",
            first["delegation"]["id"].as_str().unwrap()
        ),
        None,
        StatusCode::OK,
    )
    .await?;
    crate::runtime::tick(&state.pool).await?;
    let row = sqlx::query(
        "SELECT status,error_code,retryable,delegation_id FROM runtime_jobs WHERE id=$1",
    )
    .bind(id(&job, "id"))
    .fetch_one(&state.pool)
    .await?;
    ensure!(
        row.get::<String, _>("status") == "failed"
            && row.get::<String, _>("error_code") == "permission_revoked"
            && !row.get::<bool, _>("retryable")
    );
    ensure!(row.get::<Uuid, _>("delegation_id") == id(&first["delegation"], "id"));
    h.denied(delegated).await?;
    passed.push("queued runtime retains originating delegation, rejects idempotency rebinding and fails closed after revocation before Docker action".into());

    let child = id(&second["credential"], "sessionId");
    let before: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT expires_at FROM cli_sessions WHERE id=$1")
            .bind(child)
            .fetch_one(&state.pool)
            .await?;
    let refreshed=h.request("",Method::POST,"/auth/cli/token",Some(json!({"grantType":"refresh_token","refreshToken":second["credential"]["refreshToken"]})),StatusCode::OK).await?;
    let row = sqlx::query("SELECT expires_at,delegation_id FROM cli_sessions WHERE id=$1")
        .bind(child)
        .fetch_one(&state.pool)
        .await?;
    ensure!(
        row.get::<chrono::DateTime<chrono::Utc>, _>("expires_at") == before
            && row.get::<Uuid, _>("delegation_id") == id(&second["delegation"], "id")
    );
    h.request(
        &h.token,
        Method::DELETE,
        &format!("/api/v1/cli/sessions/{child}"),
        None,
        StatusCode::OK,
    )
    .await?;
    h.denied(refreshed["accessToken"].as_str().unwrap()).await?;
    ensure!(
        crate::delegations::check_queued(
            &state.pool,
            &h.user,
            Some(id(&second["delegation"], "id")),
            "audit-runtime",
            "prod",
            crate::delegations::Action::Start
        )
        .await
        .is_err()
    );
    let replay = h.create("dev", &["read"]).await?;
    let rotated=h.request("",Method::POST,"/auth/cli/token",Some(json!({"grantType":"refresh_token","refreshToken":replay["credential"]["refreshToken"]})),StatusCode::OK).await?;
    h.request("",Method::POST,"/auth/cli/token",Some(json!({"grantType":"refresh_token","refreshToken":replay["credential"]["refreshToken"]})),StatusCode::UNAUTHORIZED).await?;
    h.denied(rotated["accessToken"].as_str().unwrap()).await?;
    ensure!(
        crate::delegations::check_queued(
            &state.pool,
            &h.user,
            Some(id(&replay["delegation"], "id")),
            "audit-runtime",
            "dev",
            crate::delegations::Action::Read
        )
        .await
        .is_err()
    );
    passed.push("refresh preserves scope/expiry; delegated session revocation and refresh reuse remove queued authority independently of owner session".into());

    sqlx::query("UPDATE users SET admin=false WHERE id=$1")
        .bind(&h.user)
        .execute(&state.pool)
        .await?;
    sqlx::query("INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,'audit-runtime','dev','develop')").bind(&h.user).execute(&state.pool).await?;
    let grant = h.create("dev", &["read", "build"]).await?;
    ensure!(
        crate::delegations::check_queued(
            &state.pool,
            &h.user,
            Some(id(&grant["delegation"], "id")),
            "audit-runtime",
            "dev",
            crate::delegations::Action::Build
        )
        .await
        .is_ok()
    );
    sqlx::query("DELETE FROM grants WHERE user_id=$1")
        .bind(&h.user)
        .execute(&state.pool)
        .await?;
    h.request(
        access(&grant),
        Method::GET,
        "/api/v1/apps/audit-runtime",
        None,
        StatusCode::FORBIDDEN,
    )
    .await?;
    ensure!(
        crate::delegations::check_queued(
            &state.pool,
            &h.user,
            Some(id(&grant["delegation"], "id")),
            "audit-runtime",
            "dev",
            crate::delegations::Action::Build
        )
        .await
        .is_err()
    );
    sqlx::query("UPDATE users SET admin=true WHERE id=$1")
        .bind(&h.user)
        .execute(&state.pool)
        .await?;

    let expiring = h.create("dev", &["read"]).await?;
    sqlx::query("UPDATE delegations SET created_at=now()-interval '2 hours',expires_at=now()-interval '1 hour' WHERE id=$1").bind(id(&expiring["delegation"],"id")).execute(&state.pool).await?;
    h.denied(access(&expiring)).await?;
    let inherited = h.create("dev", &["read"]).await?;
    sqlx::query("UPDATE cli_sessions SET revoked_at=now() WHERE id=$1")
        .bind(h.parent)
        .execute(&state.pool)
        .await?;
    h.denied(access(&inherited)).await?;
    passed.push("current grant revocation, delegation expiry and issuing CLI family revocation invalidate delegated API and queued authority".into());
    Ok(passed)
}
