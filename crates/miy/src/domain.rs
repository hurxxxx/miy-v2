use crate::{
    State,
    auth::Principal,
    contract::{self, Manifest},
    http::{ApiError, ApiResult},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

pub async fn seed(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    for (i, n) in ["admin", "developer", "alice", "bob"].iter().enumerate() {
        sqlx::query("INSERT INTO users VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
            .bind(format!("00000000-0000-4000-8000-{:012}", i + 1))
            .bind(n)
            .bind(i == 0)
            .execute(pool)
            .await?;
    }
    Ok(())
}
pub async fn admin(state: &State, p: &Principal) -> ApiResult<()> {
    let yes: bool =
        sqlx::query_scalar("SELECT COALESCE((SELECT admin FROM users WHERE id=$1),false)")
            .bind(&p.id)
            .fetch_one(&state.pool)
            .await?;
    if yes {
        Ok(())
    } else {
        Err(ApiError::forbidden())
    }
}
pub async fn authorize(
    state: &State,
    p: &Principal,
    app: &str,
    env: &str,
    write: bool,
) -> ApiResult<()> {
    if write && !p.scopes.iter().any(|s| s == "miy.dev") {
        return Err(ApiError::forbidden());
    }
    if admin(state, p).await.is_ok() {
        return Ok(());
    }
    let yes:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM grants WHERE user_id=$1 AND app_id=$2 AND environment=$3 AND (permission='develop' OR ($4=false AND permission='use')))")
        .bind(&p.id).bind(app).bind(env).bind(write).fetch_one(&state.pool).await?;
    if yes {
        Ok(())
    } else {
        Err(ApiError::forbidden())
    }
}
pub async fn me(state: &State, p: &Principal) -> ApiResult<Value> {
    let row = sqlx::query("SELECT name,admin FROM users WHERE id=$1")
        .bind(&p.id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::forbidden)?;
    let grants:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('appId',app_id,'environment',environment,'permission',permission) FROM grants WHERE user_id=$1")
        .bind(&p.id).fetch_all(&state.pool).await?;
    Ok(
        json!({"id":p.id,"name":row.get::<String,_>("name"),"admin":row.get::<bool,_>("admin"),"grants":grants,"csrf":p.csrf}),
    )
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewApp {
    pub id: String,
    pub name: String,
}
pub async fn create_app(state: &State, p: &Principal, v: NewApp) -> ApiResult<Value> {
    admin(state, p).await?;
    if !contract::valid_id(&v.id) || v.name.trim().is_empty() || v.name.len() > 120 {
        return Err(ApiError::bad("invalid_app"));
    }
    sqlx::query("INSERT INTO apps VALUES($1,$2)")
        .bind(&v.id)
        .bind(&v.name)
        .execute(&state.pool)
        .await?;
    Ok(json!({"id":v.id,"name":v.name}))
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewRelease {
    pub artifact_hash: String,
}
pub async fn create_release(
    state: &State,
    p: &Principal,
    app: &str,
    v: NewRelease,
) -> ApiResult<Value> {
    authorize(state, p, app, "dev", true).await?;
    let path = contract::verified_artifact(&state.artifacts, &v.artifact_hash)
        .map_err(|_| ApiError::bad("invalid_artifact"))?;
    let m: Manifest = serde_json::from_slice(&std::fs::read(path.join("manifest.json"))?)
        .map_err(|_| ApiError::bad("invalid_manifest"))?;
    contract::validate(&m).map_err(ApiError::bad_owned)?;
    if m.id != app
        || !path.join("ui").join(&m.ui.module_entry).is_file()
        || !path.join("migrations").is_dir()
    {
        return Err(ApiError::bad("invalid_artifact"));
    }
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO releases(id,app_id,version,artifact_hash,manifest) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(id)
    .bind(app)
    .bind(&m.version)
    .bind(&v.artifact_hash)
    .bind(serde_json::to_value(&m)?)
    .execute(&state.pool)
    .await?;
    Ok(json!({"id":id,"artifactHash":v.artifact_hash,"manifest":m,"kind":"validation_fixture"}))
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewInstallation {
    pub app_id: String,
    pub environment: String,
    pub release_id: Uuid,
}
pub async fn create_installation(
    state: &State,
    p: &Principal,
    v: NewInstallation,
) -> ApiResult<Value> {
    if !["dev", "prod"].contains(&v.environment.as_str()) {
        return Err(ApiError::bad("invalid_environment"));
    }
    authorize(state, p, &v.app_id, &v.environment, true).await?;
    let matches: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM releases WHERE id=$1 AND app_id=$2)")
            .bind(v.release_id)
            .bind(&v.app_id)
            .fetch_one(&state.pool)
            .await?;
    if !matches {
        return Err(ApiError::bad("release_app_mismatch"));
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO installations(id,app_id,environment,release_id) VALUES($1,$2,$3,$4)")
        .bind(id)
        .bind(&v.app_id)
        .bind(&v.environment)
        .bind(v.release_id)
        .execute(&state.pool)
        .await?;
    Ok(
        json!({"id":id,"appId":v.app_id,"environment":v.environment,"runtimeStatus":"not_deployed","desiredState":"stopped"}),
    )
}
pub async fn installations(state: &State, p: &Principal) -> ApiResult<Value> {
    let rows=sqlx::query("SELECT i.*,a.name,r.artifact_hash,r.manifest FROM installations i JOIN apps a ON a.id=i.app_id JOIN releases r ON r.id=i.release_id ORDER BY a.name,i.environment")
        .fetch_all(&state.pool).await?;
    let mut out = vec![];
    for r in rows {
        let app: String = r.get("app_id");
        let env: String = r.get("environment");
        if authorize(state, p, &app, &env, false).await.is_err() {
            continue;
        }
        let m: Manifest = serde_json::from_value(r.get("manifest"))?;
        out.push(json!({"id":r.get::<Uuid,_>("id"),"appId":app,"name":r.get::<String,_>("name"),"environment":env,"releaseId":r.get::<Uuid,_>("release_id"),
            "dbStatus":r.get::<String,_>("db_status"),"runtimeStatus":r.get::<String,_>("runtime_status"),"desiredState":r.get::<String,_>("desired_state"),
            "ui":{"entry":format!("/artifacts/{}/ui/{}",r.get::<String,_>("artifact_hash"),m.ui.module_entry),"export":m.ui.module_export,"contractVersion":m.ui.contract_version},"kind":"validation_fixture"}));
    }
    Ok(json!(out))
}
pub async fn installation(
    state: &State,
    p: &Principal,
    id: Uuid,
    write: bool,
) -> ApiResult<sqlx::postgres::PgRow> {
    let row = sqlx::query("SELECT * FROM installations WHERE id=$1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::not_found)?;
    authorize(state, p, row.get("app_id"), row.get("environment"), write).await?;
    Ok(row)
}
#[derive(Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Prepare {
    pub installation_id: Uuid,
    pub release_id: Uuid,
    pub idempotency_key: String,
}
pub async fn prepare(state: &State, p: &Principal, v: Prepare) -> ApiResult<Value> {
    let inst = installation(state, p, v.installation_id, true).await?;
    if v.idempotency_key.is_empty() || v.idempotency_key.len() > 128 {
        return Err(ApiError::bad("invalid_idempotency_key"));
    }
    let yes: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM releases WHERE id=$1 AND app_id=$2)")
            .bind(v.release_id)
            .bind(inst.get::<String, _>("app_id"))
            .fetch_one(&state.pool)
            .await?;
    if !yes {
        return Err(ApiError::bad("release_app_mismatch"));
    }
    let input = contract::hash(format!("{}:{}", v.installation_id, v.release_id));
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,installation_id,release_id,actor,idempotency_key,input_hash) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(actor,idempotency_key) DO NOTHING")
        .bind(id).bind(v.installation_id).bind(v.release_id).bind(&p.id).bind(&v.idempotency_key).bind(&input).execute(&state.pool).await?;
    let row = sqlx::query("SELECT id,input_hash FROM jobs WHERE actor=$1 AND idempotency_key=$2")
        .bind(&p.id)
        .bind(&v.idempotency_key)
        .fetch_one(&state.pool)
        .await?;
    if row.get::<String, _>("input_hash") != input {
        return Err(ApiError::conflict("idempotency_conflict"));
    }
    get_job(state, p, row.get("id")).await
}
pub async fn get_job(state: &State, p: &Principal, id: Uuid) -> ApiResult<Value> {
    let row = sqlx::query("SELECT * FROM jobs WHERE id=$1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::not_found)?;
    installation(state, p, row.get("installation_id"), false).await?;
    Ok(
        json!({"id":id,"installationId":row.get::<Uuid,_>("installation_id"),"releaseId":row.get::<Uuid,_>("release_id"),"status":row.get::<String,_>("status"),"step":row.get::<String,_>("step"),"errorCode":row.get::<Option<String>,_>("error_code")}),
    )
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Grant {
    pub user_id: String,
    pub app_id: String,
    pub environment: String,
    pub permission: String,
    pub enabled: bool,
}
pub async fn grant(state: &State, p: &Principal, v: Grant) -> ApiResult<Value> {
    admin(state, p).await?;
    if !["dev", "prod"].contains(&v.environment.as_str())
        || !["use", "develop"].contains(&v.permission.as_str())
    {
        return Err(ApiError::bad("invalid_grant"));
    }
    if v.enabled {
        sqlx::query("INSERT INTO grants VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
            .bind(&v.user_id)
            .bind(&v.app_id)
            .bind(&v.environment)
            .bind(&v.permission)
            .execute(&state.pool)
            .await?;
    } else {
        sqlx::query("DELETE FROM grants WHERE user_id=$1 AND app_id=$2 AND environment=$3 AND permission=$4").bind(&v.user_id).bind(&v.app_id).bind(&v.environment).bind(&v.permission).execute(&state.pool).await?;
    }
    Ok(json!({"updated":true}))
}
pub fn get_contract() -> Value {
    json!({"version":contract::VERSION,"uiVersion":"miy.ui/v1alpha1","manifestSchema":schemars::schema_for!(Manifest),"guidance":"Expose ./AppRoot; use SDK navigation. Declare permissions; actual grants are checked on the server. DB migrations belong to one immutable release. No access to core or other app databases.","sdkExample":"export default function AppRoot({runtime}) { return <button onClick={() => runtime.navigate('/detail/1')}>Open</button>; }"})
}
pub fn catalog() -> Value {
    json!([
        {"id":"create_app","method":"POST","path":"/api/v1/apps","permission":"admin","description":"Register an application","input":schemars::schema_for!(NewApp)},
        {"id":"create_release","method":"POST","path":"/api/v1/apps/{app}/releases","permission":"develop on app/dev","description":"Register a hash-verified immutable validation artifact","input":schemars::schema_for!(NewRelease)},
        {"id":"create_installation","method":"POST","path":"/api/v1/installations","permission":"develop on app/environment","description":"Create an app/environment installation; runtime remains not_deployed","input":schemars::schema_for!(NewInstallation)},
        {"id":"set_grant","method":"PUT","path":"/api/v1/grants","permission":"admin","description":"Grant or revoke app/environment permissions","input":schemars::schema_for!(Grant)},
        {"id":"get_contract","method":"GET","path":"/api/v1/contracts","permission":"authenticated","description":"App/UI contract and standalone JSON Schema"},
        {"id":"catalog","method":"GET","path":"/api/v1/catalog","permission":"authenticated","description":"All implemented public V0-V1 JSON APIs"},
        {"id":"openapi","method":"GET","path":"/api/v1/openapi.json","permission":"authenticated","description":"OpenAPI specification for the implemented V0-V1 API scope"},
        {"id":"me","method":"GET","path":"/api/v1/me","permission":"authenticated","description":"Current user and effective app/environment grants"},
        {"id":"installations","method":"GET","path":"/api/v1/installations","permission":"use or develop","description":"Visible app installations and runtime UI metadata"},
        {"id":"validate_manifest","method":"POST","path":"/api/v1/contracts/validate","permission":"authenticated","description":"Validate manifest against pinned contract","input":schemars::schema_for!(Manifest)},
        {"id":"prepare_database","method":"POST","path":"/api/v1/database/prepare","permission":"develop on installation","description":"Prepare one installation DB and apply a registered release's migrations","input":schemars::schema_for!(Prepare)},
        {"id":"get_job","method":"GET","path":"/api/v1/jobs/{id}","permission":"use or develop on installation","description":"Read persistent database job state"}
    ])
}
