use crate::{
    State,
    auth::{BrowserPurpose, Principal},
    contract::{self, Manifest},
    http::{ApiError, ApiResult},
    policy::{self, AppAction},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

pub async fn seed(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    for (i, n) in ["admin", "developer", "alice", "bob"].iter().enumerate() {
        sqlx::query("INSERT INTO users(id,name,admin,can_develop) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
            .bind(format!("00000000-0000-4000-8000-{:012}", i + 1))
            .bind(n)
            .bind(i == 0)
            .bind(i == 1)
            .execute(pool)
            .await?;
    }
    Ok(())
}
pub async fn admin(state: &State, p: &Principal) -> ApiResult<()> {
    if p.browser_purpose.is_some_and(BrowserPurpose::portal) {
        return Err(ApiError::forbidden());
    }
    crate::delegations::unrestricted(p)?;
    let yes: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT admin AND enabled FROM users WHERE id=$1),false)",
    )
    .bind(&p.id)
    .fetch_one(&state.pool)
    .await?;
    if yes {
        Ok(())
    } else {
        Err(ApiError::forbidden())
    }
}
pub async fn me(state: &State, p: &Principal) -> ApiResult<Value> {
    crate::auth_sessions::ensure_active(&state.pool, &p.id).await?;
    let row = sqlx::query("SELECT name,admin,can_develop FROM users WHERE id=$1")
        .bind(&p.id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::forbidden)?;
    let delegation = if let Some(id) = p.delegation_id {
        Some(crate::delegations::active(&state.pool, &p.id, id).await?)
    } else {
        None
    };
    let grants:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('appId',app_id,'environment',environment,'permission',permission) FROM effective_grants WHERE user_id=$1 ORDER BY app_id,environment,permission")
        .bind(&p.id).fetch_all(&state.pool).await?;
    let grants: Vec<Value> = grants
        .into_iter()
        .filter(|g| {
            delegation
                .as_ref()
                .is_none_or(|d| g["appId"] == d.app_id && g["environment"] == d.environment)
        })
        .collect();
    let portal = p.browser_purpose.is_some_and(BrowserPurpose::portal);
    let administrator = row.get::<bool, _>("admin") && p.delegation_id.is_none();
    // This is a navigation hint, never authority. Each console operation still
    // checks the current account and app/environment permissions on the server.
    let can_manage_console = p.delegation_id.is_none()
        && p.scopes.iter().any(|scope| scope == "miy.dev")
        && (administrator
            || row.get::<bool, _>("can_develop")
            || grants
                .iter()
                .any(|grant| matches!(grant["permission"].as_str(), Some("develop" | "deploy"))));
    let grants: Vec<Value> = grants
        .into_iter()
        .filter(|grant| !portal || grant["permission"] == "use")
        .collect();
    Ok(json!({
        "id":p.id,"name":row.get::<String,_>("name"),"admin":administrator && !portal,
        "grants":grants,"csrf":p.csrf,"development":p.development,"delegation":delegation,
        "browserPurpose":p.browser_purpose.map(|purpose| purpose.as_str()),
        "portalOrigin":state.auth.origin,"consoleOrigin":state.auth.console_origin,"devPortalOrigin":state.auth.dev_portal_origin,
        "canManageConsole":can_manage_console,
    }))
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
    let mut tx = crate::policy::Write::begin(&state.pool).await?;
    sqlx::query("INSERT INTO apps VALUES($1,$2)")
        .bind(&v.id)
        .bind(&v.name)
        .execute(&mut *tx)
        .await?;
    // The insert may wait for a concurrent one with the same ID; decide after it.
    tx.decide_platform(p, crate::policy::PlatformAction::Administer)
        .await?
        .commit()
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
    policy::require(state, p, app, "dev", AppAction::Develop).await?;
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
    let mut tx = policy::Write::begin(&state.pool).await?;
    sqlx::query(
        "INSERT INTO releases(id,app_id,version,artifact_hash,manifest) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(id)
    .bind(app)
    .bind(&m.version)
    .bind(&v.artifact_hash)
    .bind(serde_json::to_value(&m)?)
    .execute(&mut *tx)
    .await?;
    tx.decide(p, app, "dev", AppAction::Develop)
        .await?
        .commit()
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
    policy::require(state, p, &v.app_id, &v.environment, AppAction::Develop).await?;
    if v.environment == "prod" {
        return Err(ApiError::bad("use_checked_release_promotion_for_prod"));
    }
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
    let mut tx = policy::Write::begin(&state.pool).await?;
    sqlx::query("INSERT INTO installations(id,app_id,environment,release_id) VALUES($1,$2,$3,$4)")
        .bind(id)
        .bind(&v.app_id)
        .bind(&v.environment)
        .bind(v.release_id)
        .execute(&mut *tx)
        .await?;
    tx.decide(p, &v.app_id, &v.environment, AppAction::Develop)
        .await?
        .commit()
        .await?;
    Ok(
        json!({"id":id,"appId":v.app_id,"environment":v.environment,"runtimeStatus":"not_deployed","desiredState":"stopped"}),
    )
}
/// Installations the user may open now (rail, launcher, app detail).
pub async fn installations(state: &State, p: &Principal) -> ApiResult<Value> {
    installation_list(state, p, false).await
}
/// Per-app runtime management, including non-administrator developers/deployers.
/// The company switch blocks use, not runtime control, so disabled apps stay listed;
/// `canOpen` describes the user's portal admission, not a console app execution route.
pub async fn managed_installations(state: &State, p: &Principal) -> ApiResult<Value> {
    if p.browser_purpose.is_some_and(BrowserPurpose::portal) {
        return Err(ApiError::forbidden());
    }
    crate::auth_sessions::ensure_active(&state.pool, &p.id).await?;
    installation_list(state, p, true).await
}
async fn installation_list(state: &State, p: &Principal, managed: bool) -> ApiResult<Value> {
    let rows: Vec<Value> = sqlx::query_scalar(&format!("SELECT {INSTALLATION_ROW} FROM installations t JOIN releases r ON r.id=t.release_id JOIN apps a ON a.id=t.app_id ORDER BY a.name,t.environment"))
        .fetch_all(&state.pool)
        .await?;
    let facts = policy::Facts::load(&state.pool, p, None).await?;
    let mut out = Vec::new();
    for row in rows {
        let item = installation_view(row, &facts)?;
        if (managed && item.can_manage) || (!managed && item.can_open) {
            out.push(item);
        }
    }
    Ok(serde_json::to_value(out)?)
}
/// One installation for [`installation_view`]; every installation list selects it
/// over `installations t JOIN releases r JOIN apps a`. The display values are set
/// from the policy afterwards.
pub(crate) const INSTALLATION_ROW: &str = "jsonb_build_object('id',t.id,'appId',t.app_id,'name',a.name,'manifestName',r.manifest->>'name','version',r.version,'environment',t.environment,'releaseId',t.release_id,'kind',CASE WHEN r.image_id IS NULL THEN 'validation_fixture' ELSE 'docker_app' END,'imageId',r.image_id,'sourceCommit',r.source_commit,'dbStatus',t.db_status,'runtimeStatus',t.runtime_status,'desiredState',t.desired_state,'generation',t.control_generation,'health',jsonb_build_object('status',t.health_status,'consecutiveFailures',t.health_failures,'checkedAt',t.health_checked_at,'errorCode',t.health_error),'ui',jsonb_build_object('entry','/artifacts/'||r.artifact_hash||'/ui/'||(r.manifest->'ui'->>'moduleEntry'),'export',r.manifest->'ui'->>'moduleExport','contractVersion',r.manifest->'ui'->>'contractVersion'),'lastOperation',(SELECT jsonb_build_object('id',j.id,'action',j.action,'status',j.status,'step',j.step,'errorCode',j.error_code,'retryable',j.retryable) FROM runtime_jobs j WHERE j.app_id=t.app_id AND j.environment=t.environment ORDER BY j.created_at DESC,j.id DESC LIMIT 1),'acceptance',(SELECT jsonb_build_object('recordedBy',c.actor,'note',c.note,'checks',c.checks,'structured',jsonb_array_length(c.checks)>0,'checkedAt',c.checked_at) FROM release_checks c WHERE c.release_id=r.id),'appDisabled',EXISTS(SELECT 1 FROM app_controls c WHERE c.app_id=t.app_id AND c.disabled),'canOpen',false,'canManage',false,'canPromote',false)";
/// The installation in `row` ([`INSTALLATION_ROW`]) with the decisions `facts` give:
/// the display values, and operational details only with management read. Preserve
/// the published field names/types for the SDK and older CLI clients.
pub(crate) fn installation_view(
    row: Value,
    facts: &policy::Facts,
) -> ApiResult<crate::api_contract::Installation> {
    let mut item: crate::api_contract::Installation = serde_json::from_value(row)?;
    let flags = facts.flags(&item.app_id, item.environment.as_str());
    item.can_open = flags.can_open;
    item.can_manage = flags.can_manage;
    item.can_promote = flags.can_promote;
    if !flags.can_manage {
        item.acceptance = None;
        item.last_operation = None;
        item.image_id = None;
        item.source_commit = None;
        item.health.error_code = None;
    }
    Ok(item)
}
/// Load an installation the principal may use ([`AppAction::Use`]).
pub async fn installation(
    state: &State,
    p: &Principal,
    id: Uuid,
) -> ApiResult<sqlx::postgres::PgRow> {
    let row = sqlx::query("SELECT * FROM installations WHERE id=$1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let (app, env): (String, String) = (row.get("app_id"), row.get("environment"));
    policy::require(state, p, &app, &env, AppAction::Use).await?;
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
    let inst = sqlx::query("SELECT * FROM installations WHERE id=$1")
        .bind(v.installation_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let (app, env): (String, String) = (inst.get("app_id"), inst.get("environment"));
    policy::require(
        state,
        p,
        &app,
        &env,
        // The receipt is the stored job, so admission also needs read (like runtime
        // requests): a read-less delegation is refused before anything is inserted.
        AppAction::Request(crate::delegations::Action::PrepareDatabase),
    )
    .await?;
    if inst.get::<String, _>("environment") != "dev" {
        return Err(ApiError::forbidden());
    }
    if inst.get::<Option<String>, _>("runtime_image").is_some() {
        return Err(ApiError::conflict("deploy_image_for_database_changes"));
    }
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
    let mut tx = policy::Write::begin(&state.pool).await?;
    sqlx::query("INSERT INTO jobs(id,installation_id,release_id,actor,idempotency_key,input_hash,delegation_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(actor,idempotency_key) DO NOTHING")
        .bind(id).bind(v.installation_id).bind(v.release_id).bind(&p.id).bind(&v.idempotency_key).bind(&input).bind(p.delegation_id).execute(&mut *tx).await?;
    let row = sqlx::query("SELECT * FROM jobs WHERE actor=$1 AND idempotency_key=$2")
        .bind(&p.id)
        .bind(&v.idempotency_key)
        .fetch_one(&mut *tx)
        .await?;
    if row.get::<String, _>("input_hash") != input
        || row.get::<Option<Uuid>, _>("delegation_id") != p.delegation_id
    {
        return Err(ApiError::conflict("idempotency_conflict"));
    }
    // Decide after the insert waited on any competing request with this key; the
    // receipt is the row read here, not a second authorization after commit.
    let receipt = job_view(&row);
    tx.decide(
        p,
        &app,
        &env,
        AppAction::Request(crate::delegations::Action::PrepareDatabase),
    )
    .await?
    .commit()
    .await?;
    Ok(receipt)
}
pub async fn get_job(state: &State, p: &Principal, id: Uuid) -> ApiResult<Value> {
    let row = sqlx::query("SELECT j.*,t.app_id,t.environment FROM jobs j JOIN installations t ON t.id=j.installation_id WHERE j.id=$1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::not_found)?;
    policy::require(
        state,
        p,
        row.get("app_id"),
        row.get("environment"),
        AppAction::Observe,
    )
    .await?;
    Ok(job_view(&row))
}
fn job_view(row: &sqlx::postgres::PgRow) -> Value {
    json!({"id":row.get::<Uuid,_>("id"),"installationId":row.get::<Uuid,_>("installation_id"),"releaseId":row.get::<Uuid,_>("release_id"),"status":row.get::<String,_>("status"),"step":row.get::<String,_>("step"),"errorCode":row.get::<Option<String>,_>("error_code")})
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
        || !["use", "develop", "deploy"].contains(&v.permission.as_str())
    {
        return Err(ApiError::bad("invalid_grant"));
    }
    let mut tx = crate::policy::Write::begin(&state.pool).await?;
    let changed = if v.enabled {
        sqlx::query("INSERT INTO grants VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
            .bind(&v.user_id)
            .bind(&v.app_id)
            .bind(&v.environment)
            .bind(&v.permission)
            .execute(&mut *tx)
            .await?
            .rows_affected()
    } else {
        sqlx::query("DELETE FROM grants WHERE user_id=$1 AND app_id=$2 AND environment=$3 AND permission=$4").bind(&v.user_id).bind(&v.app_id).bind(&v.environment).bind(&v.permission).execute(&mut *tx).await?.rows_affected()
    };
    let mut tx = tx
        .decide_platform(p, crate::policy::PlatformAction::Administer)
        .await?;
    let before = if v.enabled {
        changed == 0
    } else {
        changed != 0
    };
    let action = if v.enabled {
        "grant.enabled"
    } else {
        "grant.disabled"
    };
    let target = grant_audit_target(
        &v.user_id,
        &v.app_id,
        &v.environment,
        &v.permission,
        before,
        v.enabled,
    );
    crate::audit::record(&mut tx, &p.id, action, &target).await?;
    tx.commit().await?;
    Ok(json!({"updated":true}))
}
/// Audit target of a personal grant change; registration approval uses it too.
pub(crate) fn grant_audit_target(
    user: &str,
    app: &str,
    environment: &str,
    permission: &str,
    before: bool,
    after: bool,
) -> String {
    format!("user:{user}:app:{app}:{environment}:{permission}:from:{before}:to:{after}")
}
pub use crate::contract::get_contract;
pub fn catalog() -> Value {
    let mut catalog = json!([
        {"id":"development_session_create","method":"POST","path":"/api/v1/apps/{app}/dev-sessions","permission":"CLI session + develop in dev","description":"CLI app dev creates one leased server development DB session per app, with immutable SQL migration history; preparation is worker-owned","input":schemars::schema_for!(crate::development::NewSession)},
        {"id":"development_session_get","method":"GET","path":"/api/v1/dev-sessions/{id}","permission":"own CLI session + develop","description":"Inspect queued/preparing/ready/failed database preparation; no connection secrets returned"},
        {"id":"development_session_heartbeat","method":"POST","path":"/api/v1/dev-sessions/{id}/heartbeat","permission":"own CLI session + develop","description":"Renew a 120-second development lease every 30 seconds; revoked parent sessions or grants cannot renew"},
        {"id":"development_session_close","method":"DELETE","path":"/api/v1/dev-sessions/{id}","permission":"own CLI session + develop","description":"CLI app dev-stop closes its development tunnel; persistent development database and data are preserved"},
        {"id":"runtime_preflight","method":"POST","path":"/api/v1/apps/{app}/runtime/preflight","permission":"develop in dev or deploy in prod","description":"Read exact runtime and database/schema revision before an operation; does not mutate resources","input":schemars::schema_for!(crate::runtime_contract::PreflightInput)},
        {"id":"runtime_job_events","method":"GET","path":"/api/v1/runtime/jobs/{id}/events","permission":"authorized job reader","description":"Read ordered cursor-based persistent job events; use CLI job watch"},
        {"id":"runtime_job_retry","method":"POST","path":"/api/v1/runtime/jobs/{id}/retry","permission":"develop in dev or deploy in prod","description":"Explicit retry of a retryable failed job after fresh preflight; use CLI job retry"},
        {"id":"runtime_logs","method":"POST","path":"/api/v1/apps/{app}/logs","permission":"develop in dev or deploy in prod","description":"Request bounded application log collection; use CLI app logs"},
        {"id":"cli_sessions","method":"GET","path":"/api/v1/cli/sessions","permission":"authenticated owner","description":"List your CLI sessions; credentials are never returned"},
        {"id":"cli_session_revoke","method":"DELETE","path":"/api/v1/cli/sessions/{id}","permission":"authenticated owner","description":"Revoke a CLI session and its development database connections"},
        {"id":"list_components","method":"GET","path":"/api/v1/components","permission":"authenticated","description":"Live versioned component catalog, independent of CLI releases; compatibility and sha256 included"},
        {"id":"get_component","method":"GET","path":"/api/v1/components/{name}/{version}","permission":"authenticated","description":"Exact immutable component package, usage README and SHA-256; used by CLI and MCP"},
        {"id":"publish_component","method":"POST","path":"/api/v1/components","permission":"admin","description":"Publish immutable component source (maximum 2 MiB). No install scripts, no implicit app updates, no platform restart.","input":schemars::schema_for!(crate::component_registry::Package)},
        {"id":"cli_approve","method":"POST","path":"/api/v1/cli-authorizations/{id}/approve","permission":"none (retired)","description":"Retired pre-0.5 CLI browser approval; always 410 cli_upgrade_required. Use the device login of a supported CLI"},
        {"id":"cli_logout","method":"POST","path":"/api/v1/cli/logout","permission":"CLI bearer","description":"Revoke the current CLI credential"},
        {"id":"accept_release","method":"POST","path":"/api/v1/apps/{app}/test-acceptance","permission":"develop in dev","description":"Record actual test acceptance for the currently healthy dev release; required for production promotion","input":schemars::schema_for!(crate::runtime::ReleaseCheck)},
        {"id":"upload_image","method":"POST","path":"/api/v1/apps/{app}/images","permission":"develop in dev","description":"Stream a Docker save archive (maximum 1 GiB). Required headers: Content-Length, X-Miy-Image (immutable sha256 ID), X-Miy-Archive-Sha256. Worker imports and verifies the image; poll image_upload_status.","binary":true},
        {"id":"image_upload_status","method":"GET","path":"/api/v1/image-uploads/{id}","permission":"develop in app/dev","description":"Read bounded archive import state: pending/loading/ready/failed"},
        {"id":"shared_directory","method":"GET","path":"/api/v1/installations/{id}/platform/directory","permission":"source use and declared platform:directory:read","description":"Shared user directory (id/name only); SDK runtime.platformApi('directory')"},
        {"id":"connected_app_read","method":"GET","path":"/api/v1/installations/{id}/connections/{target}/api/{path}","permission":"use on source and target; declared app:TARGET:read","description":"SDK runtime.appApi: access another app in the same environment through its authenticated API"},
        {"id":"connected_app_create","method":"POST","path":"/api/v1/installations/{id}/connections/{target}/api/{path}","permission":"use on source and target; declared app:TARGET:write + CSRF + source release","description":"Create through another app's API; target business authorization still applies"},
        {"id":"connected_app_update","method":"PATCH","path":"/api/v1/installations/{id}/connections/{target}/api/{path}","permission":"use on source and target; declared app:TARGET:write + CSRF + source release","description":"Update through another app's API in the same environment"},
        {"id":"connected_app_delete","method":"DELETE","path":"/api/v1/installations/{id}/connections/{target}/api/{path}","permission":"use on source and target; declared app:TARGET:write + CSRF + source release","description":"Delete through another app's API in the same environment"},
        {"id":"preview_directory","method":"GET","path":"/api/v1/preview/apps/{app}/platform/directory","permission":"develop on source app/dev","description":"Local preview shared directory; browser credentials stay in local CLI gateway"},
        {"id":"preview_app_read","method":"GET","path":"/api/v1/preview/apps/{app}/connections/{target}/api/{path}","permission":"develop on source/dev and use on target/dev","description":"Authenticated local-preview connection to a dev app API"},
        {"id":"preview_app_create","method":"POST","path":"/api/v1/preview/apps/{app}/connections/{target}/api/{path}","permission":"develop on source/dev and use on target/dev","description":"Local-preview dev connection; target business authorization applies"},
        {"id":"preview_app_update","method":"PATCH","path":"/api/v1/preview/apps/{app}/connections/{target}/api/{path}","permission":"develop on source/dev and use on target/dev","description":"Local-preview dev connection; target business authorization applies"},
        {"id":"preview_app_delete","method":"DELETE","path":"/api/v1/preview/apps/{app}/connections/{target}/api/{path}","permission":"develop on source/dev and use on target/dev","description":"Local-preview dev connection; target business authorization applies"},
        {"id":"rename_app","method":"POST","path":"/api/v1/apps/{app}/name","permission":"admin","description":"Update platform display name explicitly; deployment never renames apps or changes manifest names","input":schemars::schema_for!(crate::registration::DisplayName)},
        {"id":"request_registration","method":"POST","path":"/api/v1/app-registrations","permission":"provisioned developer or admin","description":"Request an app registration; no access until administrator approval","input":schemars::schema_for!(NewApp)},
        {"id":"list_registrations","method":"GET","path":"/api/v1/app-registrations","permission":"requester sees own, admin sees all","description":"List registration requests"},
        {"id":"decide_registration","method":"POST","path":"/api/v1/app-registrations/{id}/decision","permission":"admin","description":"Approve and grant requester develop access in dev, or reject","input":schemars::schema_for!(crate::registration::Decision)},
        {"id":"list_users","method":"GET","path":"/api/v1/users","permission":"admin","description":"List user IDs for permission management"},
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
        {"id":"get_job","method":"GET","path":"/api/v1/jobs/{id}","permission":"management read on installation","description":"Read persistent database job state; app use alone cannot read operating records"},
        {"id":"runtime_request","method":"POST","path":"/api/v1/apps/{app}/runtime","permission":"dev/develop or prod/deploy; administrator implicit","description":"Deploy uploaded immutable image to dev; promote accepted release unchanged to prod; start/stop/restart/remove without restarting the platform. Remove preserves database.","input":schemars::schema_for!(crate::runtime::RuntimeRequest)},
        {"id":"runtime_job","method":"GET","path":"/api/v1/runtime/jobs/{id}","permission":"management read on app/environment","description":"Read deployment or lifecycle job state, including company-disabled apps; app use alone is insufficient"},
        {"id":"runtime_installations","method":"GET","path":"/api/v1/runtime/installations","permission":"per-app management read with miy.dev","description":"Installations this developer, deployer or administrator may manage, including company-disabled apps. canOpen is portal admission; console credentials cannot run app UI/API."},
        {"id":"app_api_get","method":"GET","path":"/api/v1/installations/{id}/api/{path}","permission":"use or develop on installation","description":"Forward app-relative API path with verified signed identity. App enforces record ownership."},
        {"id":"app_api_post","method":"POST","path":"/api/v1/installations/{id}/api/{path}","permission":"use or develop on installation","description":"Forward mutation with CSRF and x-miy-release header matching active release"},
        {"id":"app_api_patch","method":"PATCH","path":"/api/v1/installations/{id}/api/{path}","permission":"use or develop on installation","description":"Forward app record update with CSRF and current release"},
        {"id":"app_api_delete","method":"DELETE","path":"/api/v1/installations/{id}/api/{path}","permission":"use or develop on installation","description":"Forward app record deletion with CSRF and current release"}
    ]);
    catalog.as_array_mut().unwrap().extend([
        json!({"id":"app_frame_launch","method":"POST","path":"/api/v1/app-frames","permission":"portal session; use on the installation in this portal's environment","description":"One-time code (60 s) that opens one cross-origin frame of a running installation. The portal posts it as a form to launchUrl inside a new iframe; the frame origin belongs to that installation only.","input":schemars::schema_for!(crate::app_frames::LaunchRequest)}),
        json!({"id":"app_frame_renew","method":"POST","path":"/api/v1/app-frames/{id}/renew","permission":"the portal session that opened the frame; current use","description":"One-time code that renews this frame's short API token on its own origin (POST /_miy/renew). Installation, environment, release and frame end never change."}),
        json!({"id":"get_installation_permissions","method":"GET","path":"/api/v1/installations/{id}/permissions","permission":"management read on installation and token read scope; app use is insufficient","description":"Read requested and approved external permissions, revision and last 100 history entries; optional releaseId query."}),
        json!({"id":"set_installation_permissions","method":"PUT","path":"/api/v1/installations/{id}/permissions","permission":"non-delegated admin with miy.dev","description":"Replace explicit external approvals for exactly this installation/release; compare expectedRevision. Empty permissions revokes all. No automatic carryover to new releases or prod.","input":schemars::schema_for!(crate::permissions::PermissionUpdate)}),
        json!({"id":"create_delegation","method":"POST","path":"/api/v1/delegations","permission":"non-delegated user; current user grants intersect all requested actions","description":"Issue an app/environment/action-scoped user credential with fixed expiry and revocation. Credential returned once; use auth delegations create --credential-file.","input":schemars::schema_for!(crate::delegations::Create)}),
        json!({"id":"list_delegations","method":"GET","path":"/api/v1/delegations","permission":"non-delegated owner","description":"Read own delegation metadata without credentials."}),
        json!({"id":"revoke_delegation","method":"DELETE","path":"/api/v1/delegations/{id}","permission":"non-delegated owner","description":"Revoke a delegation, including queued work and app continuation proofs."}),
        json!({"id":"source_build","method":"POST","path":"/api/v1/apps/{app}/builds","permission":"develop in dev","description":"Queue a server Docker build from a bounded committed-source tar (64 MiB maximum). Headers: Content-Length, X-Miy-Source-Commit, X-Miy-Archive-Sha256, X-Miy-Platform, X-Miy-Request-Id. Use CLI app build --remote.","binary":true}),
        json!({"id":"source_build_status","method":"GET","path":"/api/v1/builds/{id}","permission":"build owner with develop in dev","description":"Read durable build status and immutable image/source receipt; use CLI job get/watch/resume."}),
        json!({"id":"source_build_logs","method":"GET","path":"/api/v1/builds/{id}/logs","permission":"build owner with develop in dev","description":"Read bounded persistent build logs after a sequence; use CLI job logs."}),
        json!({"id":"source_build_request","method":"GET","path":"/api/v1/apps/{app}/build-requests/{request}","permission":"build owner with develop in dev","description":"Recover an original build request or deduplicated alias after a lost upload acknowledgement."}),
    ]);
    catalog
        .as_array_mut()
        .unwrap()
        .extend(crate::groups::catalog());
    catalog
        .as_array_mut()
        .unwrap()
        .push(crate::audit::catalog());
    catalog
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn portal_installations_preserve_the_sdk_shape_without_operating_details() {
        let row = json!({
            "id":Uuid::nil(),"appId":"app","name":"App","manifestName":"App","version":"1.0.0",
            "environment":"dev","releaseId":Uuid::nil(),"kind":"docker_app",
            "imageId":"internal-image","sourceCommit":"internal-source",
            "dbStatus":"ready","runtimeStatus":"running","desiredState":"running","generation":3,
            "health":{"status":"degraded","checkedAt":null,"consecutiveFailures":1,"errorCode":"internal-error"},
            "ui":{"entry":"/artifacts/hash/ui/mf-manifest.json","export":"./AppRoot","contractVersion":"miy.ui/v1alpha1"},
            "lastOperation":{"id":Uuid::nil(),"action":"deploy","status":"succeeded","step":"completed","errorCode":null,"retryable":false},
            "acceptance":{"recordedBy":"operator","note":"internal-note","checks":[],"structured":false,"checkedAt":"2026-11-03T00:00:00Z"},
            "appDisabled":false,"canOpen":false,"canManage":false,"canPromote":false,
        });
        let mut facts = policy::Facts {
            user: Some(policy::User {
                enabled: true,
                admin: true,
            }),
            browser_purpose: Some(BrowserPurpose::Portal),
            dev_scope: true,
            delegation: policy::Delegation::None,
            grants: HashMap::new(),
            disabled: HashSet::new(),
        };
        // The prod portal never opens this dev installation; the dev portal does.
        assert!(!installation_view(row.clone(), &facts).unwrap().can_open);
        facts.browser_purpose = Some(BrowserPurpose::DevPortal);
        let portal = installation_view(row.clone(), &facts).unwrap();
        assert!(portal.can_open);
        assert!(!portal.can_manage && !portal.can_promote);
        assert_eq!(portal.ui.entry, "/artifacts/hash/ui/mf-manifest.json");
        assert_eq!(portal.runtime_status, "running");
        assert!(portal.image_id.is_none() && portal.source_commit.is_none());
        assert!(portal.last_operation.is_none() && portal.acceptance.is_none());
        assert!(portal.health.error_code.is_none());
        let serialized = serde_json::to_value(&portal).unwrap();
        assert_eq!(
            serialized.as_object().unwrap().keys().collect::<Vec<_>>(),
            row.as_object().unwrap().keys().collect::<Vec<_>>()
        );
        facts.browser_purpose = Some(BrowserPurpose::Console);
        let managed = installation_view(row.clone(), &facts).unwrap();
        assert!(managed.can_manage && managed.can_promote);
        assert_eq!(managed.image_id.as_deref(), Some("internal-image"));
        assert!(managed.last_operation.is_some() && managed.acceptance.is_some());
        facts.browser_purpose = None;
        facts.user.as_mut().unwrap().admin = false;
        facts.grants.insert(
            ("app".into(), "dev".into()),
            policy::Grants {
                use_: true,
                ..Default::default()
            },
        );
        let use_only_cli = installation_view(row, &facts).unwrap();
        assert!(use_only_cli.can_open && !use_only_cli.can_manage);
        assert!(use_only_cli.last_operation.is_none() && use_only_cli.source_commit.is_none());
    }
}
