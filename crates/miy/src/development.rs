//! Leased local development against the application's persistent server dev DB.
//! HTTP handlers only enqueue work; privileged database changes run in the worker.
use crate::{
    State,
    api_error::{ApiError, ApiResult},
    auth::{self, Principal},
    contract, domain, worker,
};
use anyhow::{Context, Result};
use axum::{
    Extension, Json,
    extract::{
        Path, State as S, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
};
use futures_util::SinkExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Connection, Executor, Row};
use std::{collections::BTreeSet, path::PathBuf, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

pub const HEARTBEAT_SECONDS: u64 = 30;
pub const LEASE_SECONDS: u64 = 120;
pub const MAX_BODY: usize = 2 * 1024 * 1024;

#[derive(Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationFile {
    pub name: String,
    pub sql: String,
}
#[derive(Deserialize, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewSession {
    pub idempotency_key: String,
    pub manifest: contract::Manifest,
    pub migrations: Vec<MigrationFile>,
}

fn validate_request(app: &str, input: &NewSession) -> ApiResult<()> {
    contract::validate(&input.manifest).map_err(ApiError::bad_owned)?;
    if input.manifest.id != app
        || input.idempotency_key.is_empty()
        || input.idempotency_key.len() > 128
        || input.migrations.is_empty()
        || input.migrations.len() > 128
    {
        return Err(ApiError::bad("invalid_development_request"));
    }
    let mut versions = BTreeSet::new();
    let mut bytes = 0usize;
    for file in &input.migrations {
        let Some((number, description)) = file.name.split_once('_') else {
            return Err(ApiError::bad("invalid_migration_name"));
        };
        let version = number.parse::<i64>().ok().filter(|v| *v > 0);
        if file.name.len() > 200
            || file
                .name
                .bytes()
                .any(|b| !b.is_ascii_alphanumeric() && !b"_.-".contains(&b))
            || !number.bytes().all(|b| b.is_ascii_digit())
            || version.is_none()
            || !versions.insert(version)
            || !description.ends_with(".sql")
            || description.len() <= 4
            || description.ends_with(".up.sql")
            || description.ends_with(".down.sql")
        {
            return Err(ApiError::bad("invalid_migration_name"));
        }
        bytes = bytes.saturating_add(file.sql.len());
        if file.sql.len() > 512 * 1024 || bytes > 1024 * 1024 {
            return Err(ApiError::bad("development_migrations_too_large"));
        }
        crate::bundle::validate_sql(&file.sql)
            .map_err(|_| ApiError::bad("invalid_migration_sql"))?;
    }
    Ok(())
}

async fn owner(state: &State, p: &Principal, id: Uuid) -> ApiResult<sqlx::postgres::PgRow> {
    let row = sqlx::query("SELECT * FROM app_dev_sessions WHERE id=$1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if row.get::<String, _>("actor") != p.id
        || Some(row.get::<Uuid, _>("parent_session_id")) != p.cli_session_id
    {
        return Err(ApiError::forbidden());
    }
    domain::manage(state, p, row.get("app_id"), "dev").await?;
    Ok(row)
}

async fn public(pool: &sqlx::PgPool, row: &sqlx::postgres::PgRow) -> ApiResult<Value> {
    let db = sqlx::query("SELECT schema_fingerprint FROM db_resources WHERE id=$1")
        .bind(row.get::<Uuid, _>("database_id"))
        .fetch_one(pool)
        .await?;
    Ok(
        json!({"id":row.get::<Uuid,_>("id"),"appId":row.get::<String,_>("app_id"),"status":row.get::<String,_>("status"),"databaseId":row.get::<Uuid,_>("database_id"),"schemaFingerprint":db.get::<Option<String>,_>("schema_fingerprint"),"expiresAt":row.get::<chrono::DateTime<chrono::Utc>,_>("expires_at"),"errorCode":row.get::<Option<String>,_>("error_code"),"heartbeatSeconds":HEARTBEAT_SECONDS,"leaseSeconds":LEASE_SECONDS}),
    )
}

async fn creation_reply(pool: &sqlx::PgPool, row: &sqlx::postgres::PgRow) -> ApiResult<Value> {
    let mut result = public(pool, row).await?;
    if matches!(
        row.get::<String, _>("status").as_str(),
        "queued" | "preparing" | "ready"
    ) && session_valid(pool, row.get("id")).await?
    {
        result["connection"] = serde_json::from_str(&auth::decrypt(row.get("connection_secret"))?)?;
    }
    Ok(result)
}

pub async fn create(
    S(state): S<State>,
    Extension(p): Extension<Principal>,
    Path(app): Path<String>,
    Json(input): Json<NewSession>,
) -> ApiResult<Json<Value>> {
    domain::manage(&state, &p, &app, "dev").await?;
    let parent = p
        .cli_session_id
        .ok_or_else(|| ApiError::bad("cli_session_required"))?;
    validate_request(&app, &input)?;
    let input_data = serde_json::to_value(&input)?;
    let input_hash = contract::hash(serde_json::to_vec(&input_data)?);
    let mut tx = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!(
            "dev-request:{}:{parent}:{}",
            p.id, input.idempotency_key
        ))
        .execute(&mut *tx)
        .await?;
    if let Some(row)=sqlx::query("SELECT * FROM app_dev_sessions WHERE actor=$1 AND parent_session_id=$2 AND idempotency_key=$3").bind(&p.id).bind(parent).bind(&input.idempotency_key).fetch_optional(&mut *tx).await? {
        if row.get::<String,_>("input_hash")!=input_hash {return Err(ApiError::conflict("idempotency_conflict"));}
        tx.commit().await?;
        return Ok(Json(creation_reply(&state.pool,&row).await?));
    }
    let locked: bool =
        sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("runtime:{app}:dev"))
            .fetch_one(&mut *tx)
            .await?;
    if !locked {
        return Err(ApiError::conflict("operation_in_progress"));
    }
    // Mark stale leases before enforcing the unique active-session index. Physical
    // role revocation is worker-owned; tunnel checks close stale connections too.
    sqlx::query("UPDATE app_dev_sessions SET status='expired',updated_at=now() WHERE app_id=$1 AND status IN ('queued','preparing','ready') AND expires_at<=now()") .bind(&app).execute(&mut *tx).await?;
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app_dev_sessions WHERE app_id=$1 AND status IN ('queued','preparing','ready'))").bind(&app).fetch_one(&mut *tx).await?;
    if active {
        return Err(ApiError::conflict("active_dev_session"));
    }
    let busy:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runtime_jobs WHERE app_id=$1 AND environment='dev' AND status IN ('queued','running')) OR EXISTS(SELECT 1 FROM jobs j JOIN installations i ON i.id=j.installation_id WHERE i.app_id=$1 AND i.environment='dev' AND j.status IN ('queued','running'))").bind(&app).fetch_one(&mut *tx).await?;
    if busy {
        return Err(ApiError::conflict("operation_in_progress"));
    }
    // Metadata allocation is unprivileged and idempotent. No CREATE DATABASE or
    // migration is run by this HTTP process.
    let resource = worker::ensure_resource(&state.pool, &app, "dev", None).await?;
    let id = Uuid::new_v4();
    let role = format!("miy_d_{}", id.simple());
    let token = format!("miy_dev_{}", auth::random());
    let connection = json!({"database":resource.database,"user":role,"password":auth::random(),"tunnelToken":token});
    sqlx::query("INSERT INTO app_dev_sessions(id,app_id,database_id,actor,parent_session_id,idempotency_key,input_hash,request_data,role_name,connection_secret,tunnel_token_hash) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
        .bind(id).bind(&app).bind(resource.id).bind(&p.id).bind(parent).bind(&input.idempotency_key).bind(input_hash).bind(input_data).bind(role).bind(auth::encrypt(&connection.to_string())?).bind(contract::hash(&token)).execute(&mut *tx).await?;
    tx.commit().await?;
    let row = sqlx::query("SELECT * FROM app_dev_sessions WHERE id=$1")
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(creation_reply(&state.pool, &row).await?))
}

/// Read-only recovery of a POST whose acknowledgement was lost. This never
/// creates a session or reveals its connection secrets.
pub async fn by_request(
    S(state): S<State>,
    Extension(p): Extension<Principal>,
    Path((app, request)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let parent = p
        .cli_session_id
        .ok_or_else(|| ApiError::bad("cli_session_required"))?;
    if request.is_empty() || request.len() > 128 {
        return Err(ApiError::bad("invalid_idempotency_key"));
    }
    let row=sqlx::query("SELECT * FROM app_dev_sessions WHERE actor=$1 AND parent_session_id=$2 AND app_id=$3 AND idempotency_key=$4").bind(&p.id).bind(parent).bind(app).bind(request).fetch_optional(&state.pool).await?.ok_or_else(ApiError::not_found)?;
    Ok(Json(public(&state.pool, &row).await?))
}

pub async fn get(
    S(state): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let row = owner(&state, &p, id).await?;
    Ok(Json(public(&state.pool, &row).await?))
}
pub async fn heartbeat(
    S(state): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    owner(&state, &p, id).await?;
    if !session_valid(&state.pool, id).await? {
        return Err(ApiError::conflict("development_session_expired"));
    }
    let row=sqlx::query("UPDATE app_dev_sessions SET expires_at=now()+interval '120 seconds',updated_at=now() WHERE id=$1 AND status IN ('queued','preparing','ready') AND expires_at>now() RETURNING *").bind(id).fetch_optional(&state.pool).await?.ok_or_else(||ApiError::conflict("development_session_expired"))?;
    Ok(Json(public(&state.pool, &row).await?))
}
pub async fn close(
    S(state): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let parent = p
        .cli_session_id
        .ok_or_else(|| ApiError::bad("cli_session_required"))?;
    let authenticated:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cli_sessions c JOIN users u ON u.id=c.user_id WHERE c.id=$1 AND c.user_id=$2 AND c.revoked_at IS NULL AND c.expires_at>now() AND u.enabled)").bind(parent).bind(&p.id).fetch_one(&state.pool).await?;
    if !authenticated {
        return Err(ApiError::unauthorized());
    }
    let row = sqlx::query("SELECT * FROM app_dev_sessions WHERE id=$1 AND actor=$2")
        .bind(id)
        .bind(&p.id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(ApiError::not_found)?;
    // Releasing one's own lease cannot grant DB access. Permit a fresh login and
    // permission loss to clean up an older owned session without its secrets.

    let app: String = row.get("app_id");
    let mut tx = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("runtime:{app}:dev"))
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE app_dev_sessions SET status='closed',updated_at=now() WHERE id=$1 AND status IN ('queued','preparing','ready')").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    let row = sqlx::query("SELECT * FROM app_dev_sessions WHERE id=$1")
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(public(&state.pool, &row).await?))
}

pub(crate) async fn session_valid(pool: &sqlx::PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app_dev_sessions d JOIN cli_sessions c ON c.id=d.parent_session_id JOIN users u ON u.id=d.actor WHERE d.id=$1 AND d.status IN ('queued','preparing','ready') AND d.expires_at>now() AND c.user_id=d.actor AND c.revoked_at IS NULL AND c.expires_at>now() AND u.enabled AND (u.admin OR EXISTS(SELECT 1 FROM grants g WHERE g.user_id=d.actor AND g.app_id=d.app_id AND g.environment='dev' AND g.permission='develop')))").bind(id).fetch_one(pool).await
}
pub(crate) async fn active_for_app(pool: &sqlx::PgPool, app: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app_dev_sessions WHERE app_id=$1 AND status IN ('queued','preparing','ready') AND expires_at>now())").bind(app).fetch_one(pool).await
}

fn migration_directory(id: Uuid) -> Result<PathBuf> {
    Ok(PathBuf::from(std::env::var("MIY_ARTIFACTS")?)
        .join("development")
        .join(id.to_string())
        .join("migrations"))
}
async fn prepare(pool: &sqlx::PgPool, row: &sqlx::postgres::PgRow) -> Result<()> {
    let id: Uuid = row.get("id");
    anyhow::ensure!(
        session_valid(pool, id).await?,
        "development_session_expired"
    );
    let input: NewSession = serde_json::from_value(row.get("request_data"))?;
    let app: String = row.get("app_id");
    let directory = migration_directory(id)?;
    std::fs::create_dir_all(&directory)?;
    for file in &input.migrations {
        std::fs::write(directory.join(&file.name), &file.sql)?;
    }
    crate::bundle::migrations(&directory)?;
    let migrator = sqlx::migrate::Migrator::new(directory.as_path()).await?;
    let wanted = worker::desired_fingerprint(&migrator)?;
    let resource = worker::resource_by_id(pool, row.get("database_id")).await?;
    let previous: Option<String> =
        sqlx::query_scalar("SELECT schema_fingerprint FROM db_resources WHERE id=$1")
            .bind(resource.id)
            .fetch_one(pool)
            .await?;
    if previous.as_deref() != Some(&wanted) {
        crate::runtime::stop_for_development_schema(pool, &app).await?;
    }
    worker::prepare_resource(pool, &resource, &directory, None).await?;
    anyhow::ensure!(
        session_valid(pool, id).await?,
        "development_session_expired"
    );
    let connection: Value = serde_json::from_str(&auth::decrypt(row.get("connection_secret"))?)?;
    let role: String = row.get("role_name");
    let password = connection["password"]
        .as_str()
        .context("invalid_development_secret")?;
    let mut admin = sqlx::PgConnection::connect(&std::env::var("PROVISION_DATABASE_URL")?).await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_roles WHERE rolname=$1)")
        .bind(&role)
        .fetch_one(&mut admin)
        .await?;
    if !exists {
        admin.execute(format!("CREATE ROLE {role} LOGIN PASSWORD '{password}' NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION CONNECTION LIMIT 16").as_str()).await?;
    }
    admin
        .execute(
            format!(
                "GRANT {} TO {role} WITH INHERIT TRUE, SET FALSE",
                resource.runtime
            )
            .as_str(),
        )
        .await?;
    admin
        .execute(
            format!(
                "ALTER ROLE {role} IN DATABASE {} SET search_path=app",
                resource.database
            )
            .as_str(),
        )
        .await?;
    let expiry: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT expires_at FROM app_dev_sessions WHERE id=$1")
            .bind(id)
            .fetch_one(pool)
            .await?;
    admin
        .execute(format!("ALTER ROLE {role} VALID UNTIL '{}'", expiry.to_rfc3339()).as_str())
        .await?;
    anyhow::ensure!(
        session_valid(pool, id).await?,
        "development_session_expired"
    );
    sqlx::query("UPDATE app_dev_sessions SET status='ready',error_code=NULL,updated_at=now() WHERE id=$1 AND status='preparing' AND expires_at>now()").bind(id).execute(pool).await?;
    Ok(())
}

pub(crate) async fn recover(pool: &sqlx::PgPool) -> Result<()> {
    // Replaying preparation uses the same database/role/SQLx history, never a new DB.
    sqlx::query(
        "UPDATE app_dev_sessions SET status='queued',updated_at=now() WHERE status='preparing'",
    )
    .execute(pool)
    .await?;
    Ok(())
}
pub(crate) async fn tick(pool: &sqlx::PgPool) -> Result<bool> {
    let Some(row) = sqlx::query(
        "SELECT * FROM app_dev_sessions WHERE status='queued' ORDER BY created_at,id LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(false);
    };
    let id: Uuid = row.get("id");
    let app: String = row.get("app_id");
    let mut lock = pool.acquire().await?;
    lock.close_on_drop();
    sqlx::query("SELECT pg_advisory_lock(hashtextextended($1,0))")
        .bind(format!("runtime:{app}:dev"))
        .execute(&mut *lock)
        .await?;
    if !session_valid(pool, id).await? {
        sqlx::query("UPDATE app_dev_sessions SET status='expired',updated_at=now() WHERE id=$1 AND status='queued'").bind(id).execute(pool).await?;
        return Ok(true);
    }
    sqlx::query("UPDATE app_dev_sessions SET status='preparing',updated_at=now() WHERE id=$1 AND status='queued'").bind(id).execute(pool).await?;
    if let Err(error) = prepare(pool, &row).await {
        let code = if matches!(
            error.downcast_ref::<sqlx::migrate::MigrateError>(),
            Some(
                sqlx::migrate::MigrateError::VersionMissing(_)
                    | sqlx::migrate::MigrateError::VersionMismatch(_)
            )
        ) {
            "migration_history_mismatch"
        } else {
            "development_database_prepare_failed"
        };
        sqlx::query("UPDATE app_dev_sessions SET status='failed',error_code=$2,updated_at=now() WHERE id=$1 AND status='preparing'").bind(id).bind(code).execute(pool).await?;
        tracing::error!(event="development_prepare_failed",session_id=%id,code);
    }
    Ok(true)
}

pub(crate) struct LeaseMonitor(tokio::task::JoinHandle<()>);
impl Drop for LeaseMonitor {
    fn drop(&mut self) {
        self.0.abort();
    }
}
pub(crate) fn start(pool: sqlx::PgPool) -> LeaseMonitor {
    LeaseMonitor(tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if reconcile(&pool).await.is_err() {
                tracing::error!(event = "development_lease_reconcile_failed");
            }
        }
    }))
}

pub(crate) async fn reconcile(pool: &sqlx::PgPool) -> Result<()> {
    let rows = sqlx::query(
        "SELECT * FROM app_dev_sessions WHERE cleaned_at IS NULL ORDER BY created_at,id",
    )
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(());
    }
    let mut admin = sqlx::PgConnection::connect(&std::env::var("PROVISION_DATABASE_URL")?).await?;
    for row in rows {
        let id: Uuid = row.get("id");
        let role: String = row.get("role_name");
        // Never clean a queued/preparing session while its worker can still
        // create a role. Other apps' long migrations do not delay lease renewal.
        let mut guard = pool.acquire().await?;
        guard.close_on_drop();
        let acquired: bool =
            sqlx::query_scalar("SELECT pg_try_advisory_lock(hashtextextended($1,0))")
                .bind(format!("runtime:{}:dev", row.get::<String, _>("app_id")))
                .fetch_one(&mut *guard)
                .await?;
        if !acquired {
            continue;
        }
        let valid = session_valid(pool, id).await?;
        if !valid {
            sqlx::query("UPDATE app_dev_sessions SET status=CASE WHEN expires_at<=now() THEN 'expired' ELSE 'revoked' END,updated_at=now() WHERE id=$1 AND status IN ('queued','preparing','ready')").bind(id).execute(pool).await?;
        }
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_roles WHERE rolname=$1)")
                .bind(&role)
                .fetch_one(&mut admin)
                .await?;
        if !valid {
            if exists {
                admin
                    .execute(format!("ALTER ROLE {role} NOLOGIN").as_str())
                    .await?;
                sqlx::query("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE usename=$1 AND pid<>pg_backend_pid()").bind(&role).execute(&mut admin).await?;
                admin.execute(format!("DROP ROLE {role}").as_str()).await?;
            }
            sqlx::query("UPDATE app_dev_sessions SET cleaned_at=now() WHERE id=$1")
                .bind(id)
                .execute(pool)
                .await?;
        } else if exists {
            let expiry: chrono::DateTime<chrono::Utc> =
                sqlx::query_scalar("SELECT expires_at FROM app_dev_sessions WHERE id=$1")
                    .bind(id)
                    .fetch_one(pool)
                    .await?;
            admin
                .execute(
                    format!("ALTER ROLE {role} VALID UNTIL '{}'", expiry.to_rfc3339()).as_str(),
                )
                .await?;
        }
    }
    Ok(())
}

// The database endpoint has its own scoped token; never route it through the
// ordinary user-token middleware or expose the user's refresh/access token.
pub async fn database(
    S(state): S<State>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> ApiResult<Response> {
    if headers.contains_key(header::ORIGIN) {
        return Err(ApiError::forbidden());
    }
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| v.starts_with("miy_dev_") && v.len() < 256)
        .ok_or_else(ApiError::unauthorized)?;
    let row=sqlx::query("SELECT d.role_name,r.database_name,r.endpoint_host,r.endpoint_port FROM app_dev_sessions d JOIN db_resources r ON r.id=d.database_id WHERE d.id=$1 AND d.tunnel_token_hash=$2 AND d.status='ready' AND r.environment='dev'").bind(id).bind(contract::hash(token)).fetch_optional(&state.pool).await?.ok_or_else(ApiError::unauthorized)?;
    if !session_valid(&state.pool, id).await? {
        return Err(ApiError::unauthorized());
    }
    // Only the provisioned cluster's network address is used. Neither the client
    // nor session input can choose a destination, role, or production database.
    let host = row
        .get::<Option<String>, _>("endpoint_host")
        .ok_or_else(|| ApiError::unavailable("database_unavailable"))?;
    let port = row
        .get::<Option<i32>, _>("endpoint_port")
        .and_then(|p| u16::try_from(p).ok())
        .ok_or_else(|| ApiError::unavailable("database_unavailable"))?;
    let database: String = row.get("database_name");
    let role: String = row.get("role_name");
    Ok(ws
        .max_message_size(65536)
        .max_frame_size(65536)
        .on_upgrade(move |socket| async move {
            if tunnel(socket, &state.pool, id, &host, port, &database, &role)
                .await
                .is_err()
            {
                tracing::debug!(event="development_tunnel_closed",session_id=%id);
            }
        })
        .into_response())
}

fn startup(packet: &[u8], database: &str, role: &str) -> Result<()> {
    anyhow::ensure!(
        packet.len() >= 9 && u32::from_be_bytes(packet[..4].try_into()?) as usize == packet.len(),
        "invalid_postgres_startup"
    );
    anyhow::ensure!(
        u32::from_be_bytes(packet[4..8].try_into()?) == 196608,
        "unsupported_postgres_startup"
    );
    anyhow::ensure!(packet.last() == Some(&0), "invalid_postgres_startup");
    let mut pieces = packet[8..packet.len() - 1].split(|b| *b == 0);
    let mut seen = BTreeSet::new();
    let mut found_database = false;
    let mut found_role = false;
    loop {
        let name = pieces.next().context("invalid_postgres_startup")?;
        if name.is_empty() {
            anyhow::ensure!(pieces.next().is_none(), "invalid_postgres_startup");
            break;
        }
        let name = std::str::from_utf8(name)?;
        anyhow::ensure!(seen.insert(name), "duplicate_postgres_parameter");
        let value = std::str::from_utf8(pieces.next().context("invalid_postgres_startup")?)?;
        match name {
            "database" => {
                anyhow::ensure!(value == database, "database_mismatch");
                found_database = true;
            }
            "user" => {
                anyhow::ensure!(value == role, "role_mismatch");
                found_role = true;
            }
            "application_name" | "client_encoding" | "DateStyle" | "TimeZone"
            | "extra_float_digits" => {}
            _ => anyhow::bail!("unsupported_postgres_parameter"),
        }
    }
    anyhow::ensure!(found_database && found_role, "missing_postgres_identity");
    Ok(())
}

async fn tunnel(
    mut socket: WebSocket,
    pool: &sqlx::PgPool,
    id: Uuid,
    host: &str,
    port: u16,
    database: &str,
    role: &str,
) -> Result<()> {
    let mut initial = Vec::new();
    let remaining = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match socket.recv().await.context("tunnel_closed")?? {
                Message::Binary(bytes) => initial.extend_from_slice(&bytes),
                Message::Ping(bytes) => {
                    socket.send(Message::Pong(bytes)).await?;
                    continue;
                }
                _ => anyhow::bail!("binary_tunnel_required"),
            }
            if initial.len() < 4 {
                continue;
            }
            let size = u32::from_be_bytes(initial[..4].try_into()?) as usize;
            anyhow::ensure!(
                (8..=8192).contains(&size) && initial.len() <= 65536,
                "invalid_startup_size"
            );
            if initial.len() < size {
                continue;
            }
            let code = u32::from_be_bytes(initial[4..8].try_into()?);
            if size == 8 && matches!(code, 80877103 | 80877104) {
                initial.drain(..8);
                socket.send(Message::Binary(vec![b'N'].into())).await?;
                continue;
            }
            if code == 80877102 {
                // A PostgreSQL cancellation packet has no database/user. Do not
                // forward an unchecked backend PID/key to the shared cluster.
                socket
                    .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                        code: 1008,
                        reason: "development_cancellation_not_supported; close the connection"
                            .into(),
                    })))
                    .await?;
                anyhow::bail!("development_cancellation_not_supported");
            }
            startup(&initial[..size], database, role)?;
            return Ok::<_, anyhow::Error>(initial);
        }
    })
    .await??;
    anyhow::ensure!(
        session_valid(pool, id).await?,
        "development_session_expired"
    );
    let mut database = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::net::TcpStream::connect((host, port)),
    )
    .await??;
    tokio::time::timeout(Duration::from_secs(5), database.write_all(&remaining)).await??;
    let mut buffer = vec![0u8; 32768];
    let mut guard = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            _=guard.tick()=>{
                if !session_valid(pool,id).await? {break;}
                tokio::time::timeout(Duration::from_secs(5),socket.send(Message::Ping(Vec::new().into()))).await??;
            }
            input=socket.recv()=>match input {
                Some(Ok(Message::Binary(bytes)))=>{tokio::time::timeout(Duration::from_secs(5),database.write_all(&bytes)).await??;},
                Some(Ok(Message::Ping(bytes)))=>{tokio::time::timeout(Duration::from_secs(5),socket.send(Message::Pong(bytes))).await??;},
                Some(Ok(Message::Pong(_)))=>{},
                _=>break,
            },
            output=database.read(&mut buffer)=>{
                let size=output?;if size==0 {break;}
                tokio::time::timeout(Duration::from_secs(5),socket.send(Message::Binary(buffer[..size].to_vec().into()))).await??;
            }
        }
    }
    let _ = tokio::time::timeout(Duration::from_secs(1), socket.close()).await;
    let _ = database.shutdown().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet(fields: &[(&str, &str)]) -> Vec<u8> {
        let mut value = vec![0; 4];
        value.extend_from_slice(&196608u32.to_be_bytes());
        for (name, item) in fields {
            value.extend_from_slice(name.as_bytes());
            value.push(0);
            value.extend_from_slice(item.as_bytes());
            value.push(0);
        }
        value.push(0);
        let size = (value.len() as u32).to_be_bytes();
        value[..4].copy_from_slice(&size);
        value
    }
    #[test]
    fn tunnel_startup_is_pinned_and_cannot_forward_cancellation_or_options() {
        assert!(
            startup(
                &packet(&[
                    ("user", "session"),
                    ("database", "dev"),
                    ("client_encoding", "UTF8")
                ]),
                "dev",
                "session"
            )
            .is_ok()
        );
        for fields in [
            vec![("user", "owner"), ("database", "dev")],
            vec![("user", "session"), ("database", "prod")],
            vec![
                ("user", "session"),
                ("database", "dev"),
                ("options", "-c role=owner"),
            ],
            vec![
                ("user", "session"),
                ("database", "dev"),
                ("user", "session"),
            ],
        ] {
            assert!(startup(&packet(&fields), "dev", "session").is_err());
        }
        let mut cancel = vec![0, 0, 0, 16];
        cancel.extend_from_slice(&80877102u32.to_be_bytes());
        cancel.extend_from_slice(&[0; 8]);
        assert!(startup(&cancel, "dev", "session").is_err());
    }
}

#[cfg(test)]
#[path = "development_regression.rs"]
mod regression;
