//! Upgrade rehearsal on a fresh, disposable regression PostgreSQL instance.
//! This checks migrations 19 -> 20 -> 21 and an independent app database fixture; it
//! is not an operational backup/restore test. Private credential snapshots stay in RAM.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::borrow::Cow;
use uuid::Uuid;

struct Fixture {
    user: String,
    app: String,
    release: Uuid,
    installation: Uuid,
    group: Uuid,
    family: Uuid,
    resource: Uuid,
    legacy_hash: String,
    flow_hash: String,
    database: String,
}

async fn snapshot(pool: &PgPool, f: &Fixture) -> Result<Value> {
    // Never return this value in evidence or interpolate it in assertion errors.
    Ok(sqlx::query_scalar(
        "SELECT jsonb_build_object(\
         'user',(SELECT to_jsonb(t) FROM users t WHERE id=$1),\
         'app',(SELECT to_jsonb(t) FROM apps t WHERE id=$2),\
         'installation',(SELECT to_jsonb(t) FROM installations t WHERE id=$3),\
         'release',(SELECT to_jsonb(t) FROM releases t WHERE id=$4),\
         'group',(SELECT to_jsonb(t) FROM groups t WHERE id=$5),\
         'members',(SELECT jsonb_agg(to_jsonb(t)) FROM group_members t WHERE group_id=$5),\
         'groupGrants',(SELECT jsonb_agg(to_jsonb(t)) FROM group_grants t WHERE group_id=$5),\
         'grants',(SELECT jsonb_agg(to_jsonb(t)) FROM grants t WHERE user_id=$1),\
         'admission',(SELECT jsonb_agg(to_jsonb(t)) FROM app_admission t WHERE app_id=$2),\
         'database',(SELECT to_jsonb(t) FROM db_resources t WHERE installation_id=$3),\
         'legacy',(SELECT to_jsonb(t)-'id' FROM cli_tokens t WHERE token_hash=$7),\
         'family',(SELECT to_jsonb(t) FROM cli_sessions t WHERE id=$6),\
         'access',(SELECT jsonb_agg(to_jsonb(t)) FROM cli_access_tokens t WHERE session_id=$6),\
         'refresh',(SELECT jsonb_agg(to_jsonb(t) ORDER BY token_hash) FROM cli_refresh_tokens t WHERE session_id=$6),\
         'completedDevices',(SELECT jsonb_agg(to_jsonb(t) ORDER BY id) FROM cli_device_authorizations t WHERE user_id=$1 AND status IN ('consumed','denied')))"
    ).bind(&f.user).bind(&f.app).bind(f.installation).bind(f.release)
        .bind(f.group).bind(f.family).bind(&f.legacy_hash).fetch_one(pool).await?)
}

async fn seed(pool: &PgPool, f: &Fixture) -> Result<()> {
    sqlx::query(
        "INSERT INTO users(id,name,admin,can_develop) VALUES($1,'Upgrade fixture',true,true)",
    )
    .bind(&f.user)
    .execute(pool)
    .await?;
    sqlx::query("INSERT INTO apps(id,name) VALUES($1,'Upgrade fixture app')")
        .bind(&f.app)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO releases(id,app_id,version,artifact_hash,manifest) VALUES($1,$2,'1.0.0',$3,$4)")
        .bind(f.release).bind(&f.app).bind(crate::contract::hash(f.release.as_bytes()))
        .bind(json!({"id":f.app,"name":"Upgrade fixture","version":"1.0.0"}))
        .execute(pool).await?;
    sqlx::query("INSERT INTO installations(id,app_id,environment,release_id,db_status) VALUES($1,$2,'prod',$3,'ready')")
        .bind(f.installation).bind(&f.app).bind(f.release).execute(pool).await?;
    sqlx::query("INSERT INTO groups(id,name,source) VALUES($1,$2,'local')")
        .bind(f.group)
        .bind(format!("Upgrade {}", f.group))
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO group_members(group_id,user_id) VALUES($1,$2)")
        .bind(f.group)
        .bind(&f.user)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO group_grants(group_id,app_id,environment,permission) VALUES($1,$2,'prod','use')")
        .bind(f.group).bind(&f.app).execute(pool).await?;
    sqlx::query(
        "INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,$2,'prod','deploy')",
    )
    .bind(&f.user)
    .bind(&f.app)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO app_admission(app_id,environment,mode,changed_by) VALUES($1,'dev','all',$2)",
    )
    .bind(&f.app)
    .bind(&f.user)
    .execute(pool)
    .await?;
    sqlx::query("INSERT INTO db_resources(id,installation_id,app_id,environment,database_name,owner_role,migration_role,runtime_role,migration_secret,runtime_secret) VALUES($1,$2,$3,'prod',$4,$5,$6,$7,$8,$9)")
        .bind(f.resource).bind(f.installation).bind(&f.app).bind(&f.database)
        .bind(format!("owner_{}", f.resource.simple()))
        .bind(format!("migrate_{}", f.resource.simple()))
        .bind(format!("runtime_{}", f.resource.simple()))
        .bind(crate::auth::random()).bind(crate::auth::random()).execute(pool).await?;
    sqlx::query("INSERT INTO sessions(id_hash,user_id,access_token,refresh_token,csrf,expires_at) VALUES($1,$2,$3,$4,$5,now()+interval '15 minutes')")
        .bind(crate::contract::hash(crate::auth::random())).bind(&f.user)
        .bind(crate::auth::random()).bind(crate::auth::random()).bind(crate::auth::random())
        .execute(pool).await?;
    sqlx::query("INSERT INTO development_sessions(id_hash,user_id,csrf,expires_at) VALUES($1,$2,$3,now()+interval '8 hours')")
        .bind(crate::contract::hash(crate::auth::random())).bind(&f.user)
        .bind(crate::auth::random()).execute(pool).await?;
    sqlx::query("INSERT INTO login_flows(state_hash,verifier,nonce,expires_at) VALUES($1,$2,$3,now()+interval '5 minutes')")
        .bind(&f.flow_hash).bind(crate::auth::random()).bind(crate::auth::random())
        .execute(pool).await?;
    for approved in [false, true] {
        sqlx::query("INSERT INTO cli_authorizations(id,challenge,user_id) VALUES($1,$2,$3)")
            .bind(Uuid::new_v4())
            .bind(crate::contract::hash(crate::auth::random()))
            .bind(approved.then_some(&f.user))
            .execute(pool)
            .await?;
    }
    for status in ["pending", "approved", "consumed", "denied"] {
        sqlx::query("INSERT INTO cli_device_authorizations(id,client_id,client_name,device_secret_hash,user_code_hash,user_id,status) VALUES($1,$2,'Upgrade fixture',$3,$4,$5,$6)")
            .bind(Uuid::new_v4()).bind(Uuid::new_v4())
            .bind(crate::contract::hash(crate::auth::random()))
            .bind(crate::contract::hash(crate::auth::random())).bind(&f.user).bind(status)
            .execute(pool).await?;
    }
    sqlx::query("INSERT INTO cli_tokens(token_hash,user_id) VALUES($1,$2)")
        .bind(&f.legacy_hash)
        .bind(&f.user)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO cli_sessions(id,user_id,client_id,client_name) VALUES($1,$2,$3,'Upgrade fixture')")
        .bind(f.family).bind(&f.user).bind(Uuid::new_v4()).execute(pool).await?;
    sqlx::query("INSERT INTO cli_access_tokens(token_hash,session_id,expires_at) VALUES($1,$2,now()+interval '15 minutes')")
        .bind(crate::contract::hash(crate::auth::random())).bind(f.family).execute(pool).await?;
    for used in [false, true] {
        sqlx::query("INSERT INTO cli_refresh_tokens(token_hash,session_id,used_at) VALUES($1,$2,CASE WHEN $3 THEN now() ELSE NULL END)")
            .bind(crate::contract::hash(crate::auth::random())).bind(f.family).bind(used)
            .execute(pool).await?;
    }
    Ok(())
}

/// Apply the source migrations up to `version` (all when `None`).
async fn migrate_to(pool: &PgPool, version: Option<i64>) -> Result<()> {
    let mut migrator = sqlx::migrate!("../../migrations/core");
    if let Some(version) = version {
        migrator.migrations = Cow::Owned(
            migrator
                .iter()
                .filter(|migration| migration.version <= version)
                .cloned()
                .collect(),
        );
    }
    migrator.run(pool).await?;
    Ok(())
}

fn sql_state(result: Result<sqlx::postgres::PgQueryResult, sqlx::Error>) -> Option<String> {
    result.err().and_then(|error| {
        error
            .as_database_error()
            .and_then(|db| db.code().map(|code| code.into_owned()))
    })
}

async fn check(pool: &PgPool, app_pool: &PgPool, f: &Fixture) -> Result<Vec<String>> {
    let before = snapshot(pool, f).await?;
    let records: Vec<Value> = sqlx::query_scalar("SELECT payload FROM records ORDER BY id")
        .fetch_all(app_pool)
        .await?;
    migrate_to(pool, Some(20)).await?;
    ensure!(
        snapshot(pool, f).await? == before,
        "browser boundary migration changed durable user/app/ACL/resource or existing CLI credential data"
    );
    let after_records: Vec<Value> = sqlx::query_scalar("SELECT payload FROM records ORDER BY id")
        .fetch_all(app_pool)
        .await?;
    ensure!(
        after_records == records && !records.is_empty(),
        "app database fixture data changed during core migration"
    );
    let invalidated: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS(SELECT 1 FROM sessions) AND NOT EXISTS(SELECT 1 FROM development_sessions)\
         AND NOT EXISTS(SELECT 1 FROM login_flows) AND NOT EXISTS(SELECT 1 FROM cli_authorizations)\
         AND NOT EXISTS(SELECT 1 FROM cli_device_authorizations WHERE status IN ('pending','approved'))"
    ).fetch_one(pool).await?;
    ensure!(
        invalidated,
        "pre-boundary browser credentials or pending CLI approval survived migration"
    );
    let legacy_id: Uuid = sqlx::query_scalar("SELECT id FROM cli_tokens WHERE token_hash=$1")
        .bind(&f.legacy_hash)
        .fetch_one(pool)
        .await?;
    ensure!(
        !legacy_id.is_nil(),
        "existing legacy credential did not gain a public management ID"
    );
    let valid_credentials: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM cli_tokens WHERE token_hash=$1 AND expires_at>now())\
         AND EXISTS(SELECT 1 FROM cli_sessions WHERE id=$2 AND revoked_at IS NULL AND expires_at>now())"
    ).bind(&f.legacy_hash).bind(f.family).fetch_one(pool).await?;
    ensure!(
        valid_credentials,
        "migration revoked or shortened an existing CLI credential"
    );
    // Run it again to prove migration tracking does not invalidate newly issued
    // purpose-bound browser sessions on a normal subsequent server migration check.
    let new_browser = crate::contract::hash(crate::auth::random());
    sqlx::query("INSERT INTO development_sessions(id_hash,user_id,csrf,expires_at,browser_purpose) VALUES($1,$2,$3,now()+interval '8 hours','console')")
        .bind(&new_browser).bind(&f.user).bind(crate::auth::random()).execute(pool).await?;
    migrate_to(pool, Some(20)).await?;
    ensure!(sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM development_sessions WHERE id_hash=$1 AND browser_purpose='console')")
        .bind(&new_browser).fetch_one(pool).await?, "repeated migration invalidated a current browser session");
    let missing_purpose = sqlx::query("INSERT INTO sessions(id_hash,user_id,access_token,refresh_token,csrf,expires_at) VALUES('boundary-missing-purpose',$1,'fixture','fixture','fixture',now()+interval '1 minute')")
        .bind(&f.user).execute(pool).await;
    ensure!(
        missing_purpose
            .err()
            .and_then(|error| error
                .as_database_error()
                .and_then(|db| db.code().map(|code| code.into_owned())))
            .as_deref()
            == Some("23502"),
        "sessions accepted a missing browser purpose"
    );
    let bad_purpose = sqlx::query("INSERT INTO development_sessions(id_hash,user_id,csrf,expires_at,browser_purpose) VALUES('boundary-invalid-purpose',$1,'fixture',now()+interval '1 minute','invalid')")
        .bind(&f.user).execute(pool).await;
    ensure!(
        bad_purpose
            .err()
            .and_then(|error| error
                .as_database_error()
                .and_then(|db| db.code().map(|code| code.into_owned())))
            .as_deref()
            == Some("23514"),
        "development sessions accepted an unknown browser purpose"
    );
    let missing_callback = sqlx::query("INSERT INTO login_flows(state_hash,verifier,nonce,expires_at,browser_purpose) VALUES($1,'fixture','fixture',now()+interval '1 minute','portal')")
        .bind(&f.flow_hash).execute(pool).await;
    ensure!(
        missing_callback
            .err()
            .and_then(|error| error
                .as_database_error()
                .and_then(|db| db.code().map(|code| code.into_owned())))
            .as_deref()
            == Some("23502"),
        "login flow accepted a missing fixed callback URI"
    );
    ensure!(
        snapshot(pool, f).await? == before,
        "constraint checks or repeated migration changed durable data"
    );

    // 20 -> 21: the dev app portal purpose. Schema 20 rejects it; 21 accepts it in
    // every purpose-bound table without touching existing sessions or durable data.
    let dev_portal_session = crate::contract::hash(crate::auth::random());
    let insert_dev_portal = || {
        sqlx::query("INSERT INTO development_sessions(id_hash,user_id,csrf,expires_at,browser_purpose) VALUES($1,$2,'fixture',now()+interval '1 minute','dev_portal')")
            .bind(&dev_portal_session)
            .bind(&f.user)
            .execute(pool)
    };
    ensure!(
        sql_state(insert_dev_portal().await).as_deref() == Some("23514"),
        "schema 20 accepted the dev portal purpose before its migration"
    );
    migrate_to(pool, Some(21)).await?;
    let version: i64 =
        sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations WHERE success")
            .fetch_one(pool)
            .await?;
    ensure!(version == 21, "migration rehearsal did not reach schema 21");
    insert_dev_portal().await?;
    let dev_flow = sqlx::query("INSERT INTO login_flows(state_hash,verifier,nonce,expires_at,browser_purpose,callback_uri) VALUES($1,'fixture','fixture',now()+interval '1 minute','dev_portal','http://127.0.0.1:1/auth/callback')")
        .bind(&f.flow_hash).execute(pool).await;
    ensure!(
        dev_flow.is_ok(),
        "schema 21 rejected a dev portal login flow"
    );
    sqlx::query("DELETE FROM login_flows WHERE state_hash=$1")
        .bind(&f.flow_hash)
        .execute(pool)
        .await?;
    ensure!(
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM development_sessions WHERE id_hash=$1 AND browser_purpose='console')")
            .bind(&new_browser).fetch_one(pool).await?,
        "migration 21 invalidated an existing browser session"
    );
    let still_invalid = sqlx::query("INSERT INTO sessions(id_hash,user_id,access_token,refresh_token,csrf,expires_at,browser_purpose) VALUES('boundary-invalid-21',$1,'fixture','fixture','fixture',now()+interval '1 minute','invalid')")
        .bind(&f.user).execute(pool).await;
    ensure!(
        sql_state(still_invalid).as_deref() == Some("23514"),
        "schema 21 accepted an unknown browser purpose"
    );
    ensure!(
        snapshot(pool, f).await? == before,
        "migration 21 changed durable data"
    );
    let after_records: Vec<Value> = sqlx::query_scalar("SELECT payload FROM records ORDER BY id")
        .fetch_all(app_pool)
        .await?;
    ensure!(
        after_records == records,
        "app database fixture data changed during migration 21"
    );
    Ok(vec![
        "schema 19 to 20 preserves user/group/app/ACL/resource identities, existing legacy/device CLI credential rows and lifetimes, and independent app database fixture records while invalidating old browser sessions and pending login approvals".into(),
        "migration adds public legacy credential IDs, requires browser purpose and fixed callback data, rejects invalid purpose, and does not re-invalidate current sessions when migration runs again; owned fixtures are cleaned after checks".into(),
        "schema 20 to 21 admits the dev portal purpose in sessions, development sessions and login flows only after its migration, keeps rejecting unknown purposes, and preserves existing sessions, durable records and app database data".into(),
    ])
}

async fn cleanup(pool: &PgPool, f: &Fixture) -> Result<()> {
    let mut tx = pool.begin().await?;
    for table in [
        "sessions",
        "development_sessions",
        "cli_tokens",
        "cli_device_authorizations",
        "cli_authorizations",
        "cli_sessions",
        "grants",
    ] {
        sqlx::query(&format!("DELETE FROM {table} WHERE user_id=$1"))
            .bind(&f.user)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("DELETE FROM login_flows WHERE state_hash=$1")
        .bind(&f.flow_hash)
        .execute(&mut *tx)
        .await?;
    for table in ["group_members", "group_grants"] {
        sqlx::query(&format!("DELETE FROM {table} WHERE group_id=$1"))
            .bind(f.group)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("DELETE FROM app_admission WHERE app_id=$1")
        .bind(&f.app)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM db_resources WHERE id=$1")
        .bind(f.resource)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM installations WHERE id=$1")
        .bind(f.installation)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM releases WHERE id=$1")
        .bind(f.release)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM apps WHERE id=$1")
        .bind(&f.app)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM groups WHERE id=$1")
        .bind(f.group)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM users WHERE id=$1")
        .bind(&f.user)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn run(pool: &PgPool) -> Result<Vec<String>> {
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool)
        .await?;
    let pristine: bool = sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations') IS NULL AND to_regclass('public.users') IS NULL")
        .fetch_one(pool).await?;
    ensure!(
        database == "miy_audit_core" && pristine,
        "boundary migration rehearsal requires a fresh disposable regression database"
    );
    let mut prior = sqlx::migrate!("../../migrations/core");
    prior.migrations = Cow::Owned(
        prior
            .iter()
            .filter(|migration| migration.version <= 19)
            .cloned()
            .collect(),
    );
    prior.run(pool).await?;
    let version: i64 =
        sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations WHERE success")
            .fetch_one(pool)
            .await?;
    ensure!(
        version == 19,
        "migration rehearsal did not start from schema 19"
    );
    let suffix = Uuid::new_v4().simple().to_string();
    let fixture = Fixture {
        user: format!("boundary-migration-{suffix}"),
        app: format!("migration-{suffix}"),
        release: Uuid::new_v4(),
        installation: Uuid::new_v4(),
        group: Uuid::new_v4(),
        family: Uuid::new_v4(),
        resource: Uuid::new_v4(),
        legacy_hash: crate::contract::hash(crate::auth::random()),
        flow_hash: crate::contract::hash(crate::auth::random()),
        database: format!("miy_boundary_data_{suffix}"),
    };
    // Identifier contains only our fixed prefix and a fresh UUID, never caller input.
    sqlx::query(&format!("CREATE DATABASE {}", fixture.database))
        .execute(pool)
        .await?;
    let app_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect_with(
            pool.connect_options()
                .as_ref()
                .clone()
                .database(&fixture.database),
        )
        .await?;
    let outcome = async {
        sqlx::query("CREATE TABLE records(id integer PRIMARY KEY,payload jsonb NOT NULL)")
            .execute(&app_pool)
            .await?;
        sqlx::query("INSERT INTO records VALUES(1,$1)")
            .bind(json!({"part":"fixture-part","revision":"A","retained":true}))
            .execute(&app_pool)
            .await?;
        seed(pool, &fixture).await?;
        check(pool, &app_pool, &fixture).await
    }
    .await;
    let cleaned = cleanup(pool, &fixture).await;
    app_pool.close().await;
    let dropped = sqlx::query(&format!("DROP DATABASE {}", fixture.database))
        .execute(pool)
        .await;
    let observations = outcome?;
    cleaned.context("boundary migration fixture cleanup")?;
    dropped.context("boundary migration app database cleanup")?;
    Ok(observations)
}
