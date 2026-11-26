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
    let preferences = crate::preferences::read(&state.pool, &p.id).await?;
    Ok(json!({
        "id":p.id,"name":row.get::<String,_>("name"),"admin":administrator && !portal,
        "preferences":preferences,
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

/// Serialize a new organization ID with host-managed catalog imports. Reservations
/// in the shipped catalog also apply before the first catalog import. A pending
/// organization registration from an older installation can still be retried.
pub(crate) async fn reserve_organization_id(
    tx: &mut sqlx::PgConnection,
    app: &str,
    registration_retry: bool,
) -> ApiResult<()> {
    crate::official_catalog::lock_app(tx, app).await?;
    let reserved: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM official_apps WHERE app_id=$1)
             OR ($2 AND NOT ($3 AND EXISTS(SELECT 1 FROM app_registrations WHERE app_id=$1)))",
    )
    .bind(app)
    .bind(crate::official_catalog::bundled_id(app))
    .bind(registration_retry)
    .fetch_one(&mut *tx)
    .await?;
    if reserved {
        return Err(ApiError::conflict("official_app_id_reserved"));
    }
    Ok(())
}

/// Fixture routes have no server build receipt and must not create a release,
/// installation or database preparation job for an official product.
async fn require_fixture_app(pool: &sqlx::PgPool, app: &str) -> ApiResult<()> {
    let official: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM official_apps WHERE app_id=a.id) FROM apps a WHERE a.id=$1",
    )
    .bind(app)
    .fetch_optional(pool)
    .await?
    .ok_or_else(ApiError::not_found)?;
    if official {
        return Err(ApiError::conflict("official_app_requires_server_build"));
    }
    Ok(())
}

pub async fn create_app(state: &State, p: &Principal, v: NewApp) -> ApiResult<Value> {
    admin(state, p).await?;
    if !contract::valid_id(&v.id) || v.name.trim().is_empty() || v.name.len() > 120 {
        return Err(ApiError::bad("invalid_app"));
    }
    let mut tx = crate::policy::Write::begin(&state.pool).await?;
    reserve_organization_id(&mut tx, &v.id, false).await?;
    sqlx::query("INSERT INTO apps(id,name) VALUES($1,$2)")
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
    require_fixture_app(&state.pool, app).await?;
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
    require_fixture_app(&state.pool, &v.app_id).await?;
    let matches: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM releases WHERE id=$1 AND app_id=$2)")
            .bind(v.release_id)
            .bind(&v.app_id)
            .fetch_one(&state.pool)
            .await?;
    if !matches {
        return Err(ApiError::bad("release_app_mismatch"));
    }
    let mut tx = policy::Write::begin(&state.pool).await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO installations(id,app_id,environment,release_id)
         SELECT initial_dev_installation_id,id,'dev',$2 FROM apps WHERE id=$1 RETURNING id",
    )
    .bind(&v.app_id)
    .bind(v.release_id)
    .fetch_one(&mut *tx)
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
pub(crate) const INSTALLATION_ROW: &str = "jsonb_build_object('id',t.id,'appId',t.app_id,'name',a.name,'manifestName',r.manifest->>'name','version',r.version,'environment',t.environment,'releaseId',t.release_id,'distribution',
    COALESCE((SELECT CASE WHEN o.default_install THEN 'default_official' ELSE 'optional_official' END
                FROM official_apps o WHERE o.app_id=t.app_id AND o.app_id=r.app_id AND r.official_version=r.version), 'organization'),
    'kind',CASE WHEN r.image_id IS NULL THEN 'validation_fixture' ELSE 'docker_app' END,'imageId',r.image_id,'sourceCommit',r.source_commit,'dbStatus',t.db_status,'runtimeStatus',t.runtime_status,'desiredState',t.desired_state,'generation',t.control_generation,'health',jsonb_build_object('status',t.health_status,'consecutiveFailures',t.health_failures,'checkedAt',t.health_checked_at,'errorCode',t.health_error),'ui',jsonb_build_object('entry','/artifacts/'||r.artifact_hash||'/ui/'||(r.manifest->'ui'->>'moduleEntry'),'export',r.manifest->'ui'->>'moduleExport','contractVersion',r.manifest->'ui'->>'contractVersion'),'lastOperation',(SELECT jsonb_build_object('id',j.id,'releaseId',j.release_id,'action',j.action,'status',j.status,'step',j.step,'errorCode',j.error_code,'retryable',j.retryable) FROM runtime_jobs j WHERE j.app_id=t.app_id AND j.environment=t.environment ORDER BY j.created_at DESC,j.id DESC LIMIT 1),'acceptance',(SELECT jsonb_build_object('recordedBy',c.actor,'note',c.note,'checks',c.checks,'structured',jsonb_array_length(c.checks)>0,'checkedAt',c.checked_at) FROM release_checks c WHERE c.release_id=r.id),'appDisabled',EXISTS(SELECT 1 FROM app_controls c WHERE c.app_id=t.app_id AND c.disabled),'canOpen',false,'canManage',false,'canPromote',false,'panels',CASE WHEN jsonb_typeof(r.manifest->'panels')='array' THEN r.manifest->'panels' ELSE '[]'::jsonb END)";
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
    require_fixture_app(&state.pool, &app).await?;
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
    // Preassigned grants can exist before the app. Serialize them with the
    // catalog's first claim so an import never inherits an earlier grant.
    crate::official_catalog::lock_app(&mut tx, &v.app_id).await?;
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

fn response_schema<T: schemars::JsonSchema>() -> schemars::Schema {
    schemars::generate::SchemaSettings::draft2020_12()
        .for_serialize()
        .into_generator()
        .into_root_schema_for::<T>()
}

pub fn catalog() -> Value {
    const CONSOLE_ADMIN: &str = "Enabled platform administrator using a console browser session or a non-delegated CLI bearer. Bearer requests must omit Origin and can use the portal or console API origin.";
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
        {"id":"shared_directory","method":"GET","path":"/api/v1/installations/{id}/platform/directory","permission":"source use and current release approval for platform:directory:read","description":"Bounded active user/group directory. kind=users|groups|memberships, literal name q, limit 1..100, cursor, comma-separated ids (max100). Memberships always belong to the authenticated original user and reject q; no caller-supplied userId or hierarchy inheritance.","input":schemars::schema_for!(crate::directory::DirectoryQuery)},
        {"id":"connected_app_read","method":"GET","path":"/api/v1/installations/{id}/connections/{target}/api/{path}","permission":"use on source and target; declared app:TARGET:read","description":"SDK runtime.appApi: access another app in the same environment through its authenticated API"},
        {"id":"connected_app_create","method":"POST","path":"/api/v1/installations/{id}/connections/{target}/api/{path}","permission":"use on source and target; declared app:TARGET:write + CSRF + source release","description":"Create through another app's API; target business authorization still applies"},
        {"id":"connected_app_update","method":"PATCH","path":"/api/v1/installations/{id}/connections/{target}/api/{path}","permission":"use on source and target; declared app:TARGET:write + CSRF + source release","description":"Update through another app's API in the same environment"},
        {"id":"connected_app_delete","method":"DELETE","path":"/api/v1/installations/{id}/connections/{target}/api/{path}","permission":"use on source and target; declared app:TARGET:write + CSRF + source release","description":"Delete through another app's API in the same environment"},
        {"id":"preview_context","method":"GET","path":"/api/v1/preview/apps/{app}/browser/context","permission":"CLI developer or dev_session delegation on app/dev","description":"Local preview startup identity. Available before first dev deployment. Directory and other data calls check their own release permissions.","output":schemars::schema_for!(crate::api_contract::PreviewContext)},
        {"id":"preview_directory","method":"GET","path":"/api/v1/preview/apps/{app}/platform/directory","permission":"scoped preview browser grant, dev app use and current dev installation directory approval","description":"Same bounded directory query and safe DTO as installed apps; directory subject follows the dev portal browser session","input":schemars::schema_for!(crate::directory::DirectoryQuery)},
        {"id":"preview_app_read","method":"GET","path":"/api/v1/preview/apps/{app}/connections/{target}/api/{path}","permission":"scoped preview grant, use on source and target/dev, current release approval","description":"Authenticated local-preview connection to a dev app API"},
        {"id":"preview_app_create","method":"POST","path":"/api/v1/preview/apps/{app}/connections/{target}/api/{path}","permission":"scoped preview grant, use on source and target/dev, current release approval","description":"Local-preview dev connection; target business authorization applies"},
        {"id":"preview_app_update","method":"PATCH","path":"/api/v1/preview/apps/{app}/connections/{target}/api/{path}","permission":"scoped preview grant, use on source and target/dev, current release approval","description":"Local-preview dev connection; target business authorization applies"},
        {"id":"preview_app_delete","method":"DELETE","path":"/api/v1/preview/apps/{app}/connections/{target}/api/{path}","permission":"scoped preview grant, use on source and target/dev, current release approval","description":"Local-preview dev connection; target business authorization applies"},
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
        {"id":"me","method":"GET","path":"/api/v1/me","permission":"authenticated","description":"Current user, personal preferences and effective app/environment grants"},
        {"id":"installations","method":"GET","path":"/api/v1/installations","permission":"use or develop","description":"Visible app installations and runtime UI metadata"},
        {"id":"validate_manifest","method":"POST","path":"/api/v1/contracts/validate","permission":"authenticated","description":"Validate manifest against pinned contract","input":schemars::schema_for!(Manifest)},
        {"id":"prepare_database","method":"POST","path":"/api/v1/database/prepare","permission":"develop on installation","description":"Prepare one installation DB and apply a registered release's migrations","input":schemars::schema_for!(Prepare)},
        {"id":"get_job","method":"GET","path":"/api/v1/jobs/{id}","permission":"management read on installation","description":"Read persistent database job state; app use alone cannot read operating records"},
        {"id":"runtime_request","method":"POST","path":"/api/v1/apps/{app}/runtime","permission":"dev/develop or prod/deploy; administrator implicit","description":"Deploy this app's succeeded server build or reuse its registered official releaseId in dev. Promote a dev-accepted release unchanged to prod (otherwise 409 server_build_required); start/stop/restart/remove without restarting the platform. Remove preserves database.","input":schemars::schema_for!(crate::runtime::RuntimeRequest)},
        {"id":"runtime_job","method":"GET","path":"/api/v1/runtime/jobs/{id}","permission":"management read on app/environment","description":"Read deployment or lifecycle job state, including company-disabled apps; app use alone is insufficient"},
        {"id":"runtime_installations","method":"GET","path":"/api/v1/runtime/installations","permission":"per-app management read with miy.dev","description":"Installations this developer, deployer or administrator may manage, including company-disabled apps. canOpen is portal admission; console credentials cannot run app UI/API."},
        {"id":"app_api_get","method":"GET","path":"/api/v1/installations/{id}/api/{path}","permission":"use or develop on installation","description":"Forward app-relative API path with verified signed identity. App enforces record ownership."},
        {"id":"app_api_post","method":"POST","path":"/api/v1/installations/{id}/api/{path}","permission":"use or develop on installation","description":"Forward mutation with CSRF and x-miy-release header matching active release"},
        {"id":"app_api_patch","method":"PATCH","path":"/api/v1/installations/{id}/api/{path}","permission":"use or develop on installation","description":"Forward app record update with CSRF and current release"},
        {"id":"app_api_delete","method":"DELETE","path":"/api/v1/installations/{id}/api/{path}","permission":"use or develop on installation","description":"Forward app record deletion with CSRF and current release"}
    ]);
    // Personal settings and sessions (kept out of the literal above, which is at
    // the json! macro recursion limit).
    let personal = [
        json!({
            "id":"integrated_search","method":"GET","path":"/api/v1/search",
            "permission":"portal user (own apps in this environment)","cookieOnly":true,
            "description":"Integrated search: each usable app that declares a search endpoint answers for this person; an app that does not answer is marked unavailable",
            "parameters":[
                {"name":"q","in":"query","required":true,"schema":{"type":"string","minLength":1,"maxLength":200}},
                {"name":"limit","in":"query","required":false,"schema":{"type":"integer","minimum":1,"maximum":10,"default":5}}
            ]
        }),
        json!({
            "id":"list_notifications","method":"GET","path":"/api/v1/notifications",
            "permission":"portal session (own notifications in this environment)","cookieOnly":true,
            "description":"List notifications, optionally checking source access with the app. Send id and check=true before opening an entry.",
            "parameters":[
                {"name":"limit","in":"query","schema":{"type":"integer","minimum":1,"maximum":100,"default":30}},
                {"name":"cursor","in":"query","schema":{"type":"string"}},
                {"name":"check","in":"query","schema":{"type":"boolean"}},
                {"name":"id","in":"query","schema":{"type":"string","format":"uuid"}}
            ]
        }),
        json!({"id":"home_cards","method":"GET","path":"/api/v1/home/cards","permission":"portal user (own apps in this environment)","description":"Summary cards from the apps' declared homeCard endpoints; an app that does not answer is marked unavailable"}),
        json!({"id":"launcher_categories","method":"GET","path":"/api/v1/launcher/categories","permission":"undelegated user","description":"Launcher categories in display order with their app IDs; grouping only, never access"}),
        json!({"id":"create_launcher_category","method":"POST","path":"/api/v1/launcher/categories","permission":"admin","description":"Add a launcher category","input":schemars::schema_for!(crate::launcher::NewCategory)}),
        json!({"id":"update_launcher_category","method":"PATCH","path":"/api/v1/launcher/categories/{id}","permission":"admin","description":"Rename, re-icon or move a launcher category","input":schemars::schema_for!(crate::launcher::CategoryUpdate)}),
        json!({"id":"delete_launcher_category","method":"DELETE","path":"/api/v1/launcher/categories/{id}","permission":"admin","description":"Delete a launcher category; its apps stay in the launcher uncategorized"}),
        json!({"id":"set_launcher_layout","method":"PUT","path":"/api/v1/launcher/categories/layout","permission":"admin","description":"Replace the whole arrangement: categories in order and each app in at most one","input":schemars::schema_for!(crate::launcher::Layout)}),
        json!({"id":"browser_sessions","method":"GET","path":"/api/v1/me/sessions","permission":"undelegated user (own sessions only)","description":"Live browser sessions of the signed-in person on the portals and console; current marks this request's session"}),
        json!({"id":"revoke_browser_session","method":"DELETE","path":"/api/v1/me/sessions/{id}","permission":"undelegated user (own sessions only)","description":"End one of one's own browser sessions and the app frames launched from it; the current session ends by signing out"}),
        json!({"id":"update_preferences","method":"PATCH","path":"/api/v1/me/preferences","permission":"undelegated, enabled user (own record only)","description":"Change one's own display name, job title, display settings and launcher pins; omitted fields stay","input":schemars::schema_for!(crate::preferences::Update)}),
    ];
    catalog
        .as_array_mut()
        .expect("catalog array")
        .extend(personal);
    catalog.as_array_mut().unwrap().extend([
        json!({"id":"preview_browser_start","method":"POST","path":"/api/v1/preview/apps/{app}/browser/start","permission":"CLI DevSession on app/dev","description":"Register the preview instance, loopback port, state and S256 challenge."}),
        json!({"id":"preview_browser_authorize","method":"POST","path":"/api/v1/preview/browser/{id}/authorize","permission":"dev portal browser session with CSRF and current dev access","description":"Bind the login to the active portal session and return a one-time loopback code."}),
        json!({"id":"preview_browser_exchange","method":"POST","path":"/api/v1/preview/apps/{app}/browser/exchange","permission":"same CLI host credential, instance, code and PKCE verifier","description":"Redeem a single-use code for a scoped preview grant."}),
        json!({"id":"preview_browser_viewer","method":"GET","path":"/api/v1/preview/apps/{app}/browser/viewer","permission":"scoped preview grant and matching instance","description":"Resolve the current dev portal user after checking host and browser access."}),

        json!({"id":"admin_overview","method":"GET","path":"/api/v1/admin/overview","permission":CONSOLE_ADMIN,"consoleAdmin":true,"description":"Counts pending registrations, current installation approvals and runtime issues; returns an independent page of up to five installations for each queue.","output":response_schema::<crate::api_contract::AdminOverviewResponse>(),"parameters":[
            {"in":"query","name":"approvalCursor","schema":{"type":"string","maxLength":4096},"description":"Opaque approvalNextCursor from this actor's preceding approval page on the same credential surface."},
            {"in":"query","name":"runtimeCursor","schema":{"type":"string","maxLength":4096},"description":"Opaque runtimeNextCursor from this actor's preceding runtime page on the same credential surface."}
        ]}),
        json!({"id":"admin_user_directory","method":"GET","path":"/api/v1/users/directory","permission":CONSOLE_ADMIN,"consoleAdmin":true,"description":"User accounts ordered by case-insensitive name and ID and filtered by name or ID, role, status and primary organization. Descendant matching is a directory filter only.","output":response_schema::<crate::api_contract::AdminUserDirectoryPage>(),"parameters":[
            {"in":"query","name":"q","schema":{"type":"string"},"description":"Case-insensitive substring of name or ID. Trims surrounding whitespace and accepts up to 120 Unicode characters after trimming."},
            {"in":"query","name":"role","schema":{"type":"string","enum":["admin","developer","member"]},"description":"admin selects administrators, developer selects accounts with canDevelop enabled, and member selects accounts with neither flag."},
            {"in":"query","name":"status","schema":{"type":"string","enum":["enabled","disabled"]}},
            {"in":"query","name":"organization","schema":{"type":"string"},"description":"Organization UUID or unassigned"},
            {"in":"query","name":"descendants","schema":{"type":"boolean","default":false},"description":"Include descendants when organization names a UUID. Ignored for an omitted organization or unassigned."},
            {"in":"query","name":"limit","schema":{"type":"integer","minimum":1,"maximum":200,"default":50}},
            {"in":"query","name":"cursor","schema":{"type":"string","maxLength":4096},"description":"Opaque nextCursor from the previous page with the same filters, actor and credential surface."}
        ]}),
        json!({"id":"register_user","method":"POST","path":"/api/v1/users","permission":"admin","description":"Register a person by the identity provider's subject ID so they can sign in. Grants no app permission; 409 user_exists when already registered","input":schemars::schema_for!(crate::user_admin::Registration)}),
        json!({"id":"get_steward_assignment","method":"GET","path":"/api/v1/installations/{id}/steward-assignment","permission":"console administrator session","cookieOnly":true,"description":"Read installation consent and the latest one-time steward nomination, status and recovery reason."}),
        json!({"id":"nominate_installation_steward","method":"PUT","path":"/api/v1/installations/{id}/steward-assignment","permission":"console administrator session","cookieOnly":true,"description":"Nominate one active user for this installation. Send the expected assignment ID and a reason for recovery. Does not grant app admission or business permissions.","input":schemars::schema_for!(crate::installation_stewards::Nomination)}),
        json!({"id":"pending_steward_assignment","method":"GET","path":"/api/v1/installations/{id}/platform/steward-assignment","permission":"approved platform:stewards:claim, backend proof from the user's direct deployed frame","description":"For the signed-in nominee only, return assignmentId or null. No administrator identity is returned."}),
        json!({"id":"claim_steward_assignment","method":"POST","path":"/api/v1/installations/{id}/platform/steward-assignment","permission":"approved platform:stewards:claim, direct frame backend","description":"SDK step under the app roster lock. Refuses recovery while any current steward is active.","input":schemars::schema_for!(crate::installation_stewards::Claim)}),
        json!({"id":"confirm_steward_assignment","method":"POST","path":"/api/v1/installations/{id}/platform/steward-assignment/applied","permission":"approved platform:stewards:claim, direct frame backend","description":"Idempotently acknowledge the committed app receipt.","input":schemars::schema_for!(crate::installation_stewards::Receipt)}),
        json!({"id":"file_quota","method":"GET, PUT, DELETE","path":"/api/v1/installations/{id}/file-quota","permission":"admin (console)","description":"An installation's file storage limits and use. PUT sets maxBytes/maxFiles, DELETE returns to the defaults; existing files stay, new reservations above the limit are refused. Audited as file.quota_set","input":schemars::schema_for!(crate::storage::QuotaInput)}),
        json!({"id":"sign_in_as_user","method":"POST","path":"/api/v1/users/{id}/sign-in-as","permission":"console administrator browser session","cookieOnly":true,"description":"Start a user switch: a 60-second one-time link that signs this browser in to the prod or dev portal as that person. Audited as user.sign_in_as; the session ends when the starting administrator is disabled or loses administration","input":schemars::schema_for!(crate::user_switch::Start)}),
        json!({"id":"update_user","method":"PATCH","path":"/api/v1/users/{id}","permission":"admin","description":"Change a registered person's display name, administrator role or app-registration eligibility. The last enabled administrator keeps the role","input":schemars::schema_for!(crate::user_admin::Update)}),
        json!({"id":"app_frame_launch","method":"POST","path":"/api/v1/app-frames","permission":"portal session; use on the installation in this portal's environment","description":"One-time code (60 s) that opens one cross-origin frame of a running installation. The portal posts it as a form to launchUrl inside a new iframe; the frame origin belongs to that installation only.","input":schemars::schema_for!(crate::app_frames::LaunchRequest)}),
        json!({"id":"app_frame_renew","method":"POST","path":"/api/v1/app-frames/{id}/renew","permission":"the portal session that opened the frame; current use","description":"One-time code that renews this frame's short API token on its own origin (POST /_miy/renew). Installation, environment, release and frame end never change."}),
        json!({"id":"app_frame_close","method":"DELETE","path":"/api/v1/app-frames/{id}","permission":"the portal session that opened the frame","cookieOnly":true,"description":"Close a frame and release its slot. The same session may repeat the request.","successStatus":"204"}),
        json!({"id":"get_installation_permissions","method":"GET","path":"/api/v1/installations/{id}/permissions","permission":"management read on installation and token read scope; app use is insufficient","description":"Read requested and approved external permissions, revision and last 100 history entries; optional releaseId query."}),
        json!({"id":"set_installation_permissions","method":"PUT","path":"/api/v1/installations/{id}/permissions","permission":"non-delegated admin with miy.dev","description":"Review the active release and replace approvals for this installation using expectedRevision. Continuously requested permissions retain consent on update and rollback. New requests and other installations require approval. An empty list revokes all.","input":schemars::schema_for!(crate::permissions::PermissionUpdate)}),
        json!({"id":"create_delegation","method":"POST","path":"/api/v1/delegations","permission":"non-delegated user; current user grants intersect all requested actions","description":"Issue an app/environment/action-scoped user credential with fixed expiry and revocation. Credential returned once; use auth delegations create --credential-file.","input":schemars::schema_for!(crate::delegations::Create)}),
        json!({"id":"list_delegations","method":"GET","path":"/api/v1/delegations","permission":"non-delegated owner","description":"Read own delegation metadata without credentials."}),
        json!({"id":"revoke_delegation","method":"DELETE","path":"/api/v1/delegations/{id}","permission":"non-delegated owner","description":"Revoke a delegation, including queued work and app continuation proofs."}),
        json!({"id":"source_build","method":"POST","path":"/api/v1/apps/{app}/builds","permission":"develop in dev","description":"Queue a server Docker build from a bounded committed-source tar (64 MiB maximum). Headers: Content-Length, X-Miy-Source-Commit, X-Miy-Archive-Sha256, X-Miy-Platform, X-Miy-Request-Id. Use CLI app build. Only an image built here can be deployed.","binary":true}),
        json!({"id":"release_source_download","method":"GET","path":"/api/v1/apps/{app}/releases/{release}/source","permission":"undelegated develop in dev","description":"Download the exact build tar for this release; verify X-Miy-Archive-Sha256. Use CLI app source download."}),
        json!({"id":"release_source_upload","method":"PUT","path":"/api/v1/apps/{app}/releases/{release}/source","permission":"undelegated develop in dev","description":"Restore a missing release archive. Content-Length and SHA-256 must match the successful server build receipt. Use CLI app source upload.","binary":true}),
        json!({"id":"source_build_status","method":"GET","path":"/api/v1/builds/{id}","permission":"build owner with develop in dev","description":"Read durable build status and immutable image/source receipt; use CLI job get/watch/resume."}),
        json!({"id":"source_build_logs","method":"GET","path":"/api/v1/builds/{id}/logs","permission":"build owner with develop in dev","description":"Read bounded persistent build logs after a sequence; use CLI job logs."}),
        json!({"id":"source_build_request","method":"GET","path":"/api/v1/apps/{app}/build-requests/{request}","permission":"build owner with develop in dev","description":"Recover an original build request or deduplicated alias after a lost upload acknowledgement."}),
    ]);
    catalog
        .as_array_mut()
        .unwrap()
        .extend(crate::storage::catalog());
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
            "environment":"dev","releaseId":Uuid::nil(),"kind":"docker_app","distribution":"default_official",
            "imageId":"internal-image","sourceCommit":"internal-source",
            "dbStatus":"ready","runtimeStatus":"running","desiredState":"running","generation":3,
            "health":{"status":"degraded","checkedAt":null,"consecutiveFailures":1,"errorCode":"internal-error"},
            "ui":{"entry":"/artifacts/hash/ui/mf-manifest.json","export":"./AppRoot","contractVersion":"miy.ui/v1alpha1"},
            "lastOperation":{"id":Uuid::nil(),"action":"deploy","status":"succeeded","step":"completed","errorCode":null,"retryable":false},
            "acceptance":{"recordedBy":"operator","note":"internal-note","checks":[],"structured":false,"checkedAt":"2026-11-03T00:00:00Z"},
            "appDisabled":false,"canOpen":false,"canManage":false,"canPromote":false,
            "panels":[{"id":"todos","title":"할 일","path":"/widgets/todos"}],
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
        assert_eq!(
            portal.distribution,
            crate::api_contract::AppDistribution::DefaultOfficial
        );
        assert!(!portal.can_manage && !portal.can_promote);
        assert_eq!(portal.ui.entry, "/artifacts/hash/ui/mf-manifest.json");
        assert_eq!(portal.runtime_status, "running");
        assert!(portal.image_id.is_none() && portal.source_commit.is_none());
        assert!(portal.last_operation.is_none() && portal.acceptance.is_none());
        assert!(portal.health.error_code.is_none());
        assert_eq!(portal.panels.len(), 1);
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
