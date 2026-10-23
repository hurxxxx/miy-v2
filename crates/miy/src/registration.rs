//! Registration grants no access until an administrator approves the request.
use crate::{
    State,
    auth::Principal,
    contract, domain,
    http::{ApiError, ApiResult},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

fn view(row: &sqlx::postgres::PgRow) -> Value {
    json!({"id":row.get::<Uuid,_>("id"),"appId":row.get::<String,_>("app_id"),"name":row.get::<String,_>("name"),"requestedBy":row.get::<String,_>("actor"),"status":row.get::<String,_>("status"),"decidedBy":row.get::<Option<String>,_>("decided_by")})
}
pub async fn request(s: &State, p: &Principal, v: domain::NewApp) -> ApiResult<Value> {
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id=$1 AND (admin OR can_develop))",
    )
    .bind(&p.id)
    .fetch_one(&s.pool)
    .await?;
    if !allowed || !p.scopes.iter().any(|s| s == "miy.dev") {
        return Err(ApiError::forbidden());
    }
    if !contract::valid_id(&v.id) || v.name.trim().is_empty() || v.name.len() > 120 {
        return Err(ApiError::bad("invalid_app"));
    }
    let mut tx = s.pool.begin().await?;
    // The unique app ID serializes competing requests without giving the loser access.
    sqlx::query("INSERT INTO app_registrations(id,app_id,name,actor) VALUES($1,$2,$3,$4) ON CONFLICT(app_id) DO NOTHING").bind(Uuid::new_v4()).bind(&v.id).bind(&v.name).bind(&p.id).execute(&mut *tx).await?;
    let row = sqlx::query("SELECT * FROM app_registrations WHERE app_id=$1 FOR UPDATE")
        .bind(&v.id)
        .fetch_one(&mut *tx)
        .await?;
    if row.get::<String, _>("actor") != p.id || row.get::<String, _>("name") != v.name {
        return Err(ApiError::conflict("registration_conflict"));
    }
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM apps WHERE id=$1)")
        .bind(&v.id)
        .fetch_one(&mut *tx)
        .await?;
    if exists && row.get::<String, _>("status") != "approved" {
        return Err(ApiError::conflict("app_already_registered"));
    }
    // A rejection is terminal; the requester cannot undo an administrator's decision.
    let result = view(&row);
    tx.commit().await?;
    Ok(result)
}
pub async fn list(s: &State, p: &Principal) -> ApiResult<Value> {
    let admin = domain::admin(s, p).await.is_ok();
    let rows =
        sqlx::query("SELECT * FROM app_registrations WHERE actor=$1 OR $2 ORDER BY created_at,id")
            .bind(&p.id)
            .bind(admin)
            .fetch_all(&s.pool)
            .await?;
    Ok(json!(rows.iter().map(view).collect::<Vec<_>>()))
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub approve: bool,
}
pub async fn decide(s: &State, p: &Principal, id: Uuid, decision: Decision) -> ApiResult<Value> {
    domain::admin(s, p).await?;
    let mut tx = s.pool.begin().await?;
    let row = sqlx::query("SELECT * FROM app_registrations WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(ApiError::not_found)?;
    let status = if decision.approve {
        "approved"
    } else {
        "rejected"
    };
    let prior: String = row.get("status");
    if prior != "pending" {
        if prior != status {
            return Err(ApiError::conflict("registration_already_decided"));
        }
        return Ok(view(&row));
    }
    if decision.approve {
        sqlx::query("INSERT INTO apps(id,name) VALUES($1,$2)")
            .bind(row.get::<String, _>("app_id"))
            .bind(row.get::<String, _>("name"))
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO grants(user_id,app_id,environment,permission) VALUES($1,$2,'dev','develop')").bind(row.get::<String,_>("actor")).bind(row.get::<String,_>("app_id")).execute(&mut *tx).await?;
    }
    let row = sqlx::query("UPDATE app_registrations SET status=$2,decided_by=$3,updated_at=now() WHERE id=$1 RETURNING *").bind(id).bind(status).bind(&p.id).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(view(&row))
}
pub async fn users(s: &State, p: &Principal) -> ApiResult<Value> {
    domain::admin(s, p).await?;
    let rows: Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',name,'admin',admin,'canDevelop',can_develop) FROM users ORDER BY name").fetch_all(&s.pool).await?;
    Ok(json!(rows))
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DisplayName {
    pub name: String,
}
pub async fn rename(s: &State, p: &Principal, app: &str, v: DisplayName) -> ApiResult<Value> {
    domain::admin(s, p).await?;
    let name = v.name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(ApiError::bad("invalid_app_name"));
    }
    let changed = sqlx::query("UPDATE apps SET name=$2 WHERE id=$1")
        .bind(app)
        .bind(name)
        .execute(&s.pool)
        .await?
        .rows_affected();
    if changed == 0 {
        return Err(ApiError::not_found());
    }
    Ok(
        json!({"appId":app,"name":name,"policy":"Platform display name; release manifest and application source are unchanged"}),
    )
}
