//! Administrator registration of people and their platform roles.
//!
//! The identity provider only authenticates. A person can sign in once an
//! administrator has registered the provider's subject ID here; nothing is
//! created by a sign-in attempt, and a registered person holds no app
//! permission until one is granted.
use crate::{
    State,
    auth::Principal,
    domain,
    http::{ApiError, ApiResult},
    policy::{PlatformAction, Write},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

/// Shared with user state changes: one account change is decided at a time.
const ACCOUNT_LOCK: i64 = 73490130;

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Registration {
    /// The identity provider's subject ID for this person (lowercase UUID).
    pub id: String,
    /// Display name, 1-120 characters.
    pub name: String,
    /// May request app registrations. No app permission is granted by this.
    #[serde(default)]
    pub can_develop: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Update {
    pub name: Option<String>,
    pub admin: Option<bool>,
    pub can_develop: Option<bool>,
}

fn subject(id: &str) -> ApiResult<String> {
    match Uuid::parse_str(id) {
        Ok(value) if value.hyphenated().to_string() == id => Ok(id.to_owned()),
        _ => Err(ApiError::bad("invalid_user_id")),
    }
}

fn display_name(name: &str) -> ApiResult<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
        return Err(ApiError::bad("invalid_user_name"));
    }
    Ok(name.to_owned())
}

fn view(row: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id":row.get::<String,_>("id"),"name":row.get::<String,_>("name"),
        "admin":row.get::<bool,_>("admin"),"canDevelop":row.get::<bool,_>("can_develop"),
        "enabled":row.get::<bool,_>("enabled"),
    })
}

pub async fn register(s: &State, p: &Principal, input: Registration) -> ApiResult<Value> {
    domain::admin(s, p).await?;
    let id = subject(&input.id)?;
    let name = display_name(&input.name)?;
    let mut tx = Write::begin(&s.pool).await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(ACCOUNT_LOCK)
        .execute(&mut *tx)
        .await?;
    let row = sqlx::query(
        "INSERT INTO users(id,name,admin,can_develop,enabled) VALUES($1,$2,false,$3,true) \
         ON CONFLICT(id) DO NOTHING RETURNING id,name,admin,can_develop,enabled",
    )
    .bind(&id)
    .bind(&name)
    .bind(input.can_develop)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| ApiError::conflict("user_exists"))?;
    // Decided after the insert: a registration whose administrator was revoked
    // meanwhile rolls back together with its audit row.
    let mut tx = tx.decide_platform(p, PlatformAction::Administer).await?;
    let target = format!("user:{id}:developer:{}", input.can_develop);
    crate::audit::record(&mut tx, &p.id, "user.registered", &target).await?;
    tx.commit().await?;
    Ok(view(&row))
}

pub async fn update(s: &State, p: &Principal, id: &str, input: Update) -> ApiResult<Value> {
    domain::admin(s, p).await?;
    if input.name.is_none() && input.admin.is_none() && input.can_develop.is_none() {
        return Err(ApiError::bad("no_user_change"));
    }
    let name = input.name.as_deref().map(display_name).transpose()?;
    let mut tx = Write::begin(&s.pool).await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(ACCOUNT_LOCK)
        .execute(&mut *tx)
        .await?;
    // Actor and target in one ID order, as state and primary-group changes do.
    let users = sqlx::query(
        "SELECT id,name,admin,can_develop,enabled FROM users WHERE id=ANY($1) ORDER BY id FOR NO KEY UPDATE",
    )
    .bind(vec![p.id.as_str(), id])
    .fetch_all(&mut *tx)
    .await?;
    let before = users
        .iter()
        .find(|row| row.get::<String, _>("id") == id)
        .ok_or_else(ApiError::not_found)?;
    let (was_admin, was_developer, enabled) = (
        before.get::<bool, _>("admin"),
        before.get::<bool, _>("can_develop"),
        before.get::<bool, _>("enabled"),
    );
    let old_name: String = before.get("name");
    // Before the update: an administrator may still give up their own role.
    let mut tx = tx.decide_platform(p, PlatformAction::Administer).await?;
    if input.admin == Some(false) && was_admin && enabled {
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE admin AND enabled")
            .fetch_one(&mut *tx)
            .await?;
        if count <= 1 {
            return Err(ApiError::conflict("last_enabled_administrator"));
        }
    }
    let row = sqlx::query(
        "UPDATE users SET name=COALESCE($2,name),admin=COALESCE($3,admin),can_develop=COALESCE($4,can_develop) \
         WHERE id=$1 RETURNING id,name,admin,can_develop,enabled",
    )
    .bind(id)
    .bind(&name)
    .bind(input.admin)
    .bind(input.can_develop)
    .fetch_one(&mut *tx)
    .await?;
    for (role, before, after) in [
        ("admin", was_admin, input.admin),
        ("developer", was_developer, input.can_develop),
    ] {
        if let Some(after) = after.filter(|after| *after != before) {
            let action = format!("user.{role}.{}", if after { "enabled" } else { "disabled" });
            crate::audit::record(&mut tx, &p.id, &action, &format!("user:{id}")).await?;
        }
    }
    if name.as_ref().is_some_and(|name| *name != old_name) {
        crate::audit::record(&mut tx, &p.id, "user.renamed", &format!("user:{id}")).await?;
    }
    tx.commit().await?;
    Ok(view(&row))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_canonical_subject_ids_and_bounded_names_are_accepted() {
        assert!(subject("3f0e2c5a-9d1b-4c7e-8a55-0123456789ab").is_ok());
        for id in [
            "",
            "admin",
            "3F0E2C5A-9D1B-4C7E-8A55-0123456789AB",
            "3f0e2c5a9d1b4c7e8a550123456789ab",
            " 3f0e2c5a-9d1b-4c7e-8a55-0123456789ab",
            "{3f0e2c5a-9d1b-4c7e-8a55-0123456789ab}",
        ] {
            assert_eq!(subject(id).unwrap_err().code, "invalid_user_id", "{id}");
        }
        assert_eq!(display_name("  홍길동 ").unwrap(), "홍길동");
        for name in ["", "   ", "a\nb", &"가".repeat(121)] {
            assert_eq!(display_name(name).unwrap_err().code, "invalid_user_name");
        }
        assert!(display_name(&"가".repeat(120)).is_ok());
    }
}
