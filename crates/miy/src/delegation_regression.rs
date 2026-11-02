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
