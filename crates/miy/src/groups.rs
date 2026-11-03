//! Platform groups and app access (V1-G1, V1-G2, PRD "앱 사용 권한과 앱 내부 데이터 권한").
//!
//! The core database is the source of truth. Users and groups receive app permissions;
//! apps keep their internal data access. Group membership is computed by the
//! `effective_grants` view on every check, so a change applies to the next request and
//! queued work. HR groups match the user's primary group exactly and local groups use
//! manual members; nothing is inherited through the hierarchy. Groups may hold `use`
//! and `develop` only: production deployment stays per user. Every change writes an
//! audit row in the same transaction.
use crate::{
    State,
    auth::Principal,
    domain,
    http::{ApiError, ApiResult},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

/// Serializes hierarchy changes so two concurrent moves cannot form a cycle.
const HIERARCHY_LOCK: i64 = 73490140;

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewGroup {
    pub name: String,
    /// `hr` (organisation unit, members by exact primary group) or `local` (manual members).
    pub source: String,
    pub parent_id: Option<Uuid>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupChange {
    pub name: Option<String>,
    pub active: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupParent {
    pub parent_id: Option<Uuid>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Membership {
    pub user_id: String,
    pub member: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrimaryGroup {
    pub group_id: Option<Uuid>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupGrant {
    pub group_id: Uuid,
    pub app_id: String,
    pub environment: String,
    /// `use` or `develop`; `deploy` is granted per user only.
    pub permission: String,
    pub enabled: bool,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Admission {
    pub environment: String,
    /// `selected` (explicit user and group grants) or `all` (every enabled user).
    pub mode: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppControl {
    pub disabled: bool,
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    actor: &str,
    action: &str,
    target: &str,
) -> ApiResult<()> {
    sqlx::query("INSERT INTO audit(actor,action,target,request_id) VALUES($1,$2,$3,$4)")
        .bind(actor)
        .bind(action)
        .bind(target)
        .bind(Uuid::new_v4().to_string())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn name(value: &str) -> ApiResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 120 || trimmed.chars().any(char::is_control)
    {
        return Err(ApiError::bad("invalid_group_name"));
    }
    Ok(trimmed.to_owned())
}

fn environment(value: &str) -> ApiResult<&str> {
    match value {
        "dev" | "prod" => Ok(value),
        _ => Err(ApiError::bad("invalid_environment")),
    }
}

async fn source(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> ApiResult<String> {
    sqlx::query_scalar("SELECT source FROM groups WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(ApiError::not_found)
}

pub async fn list(state: &State, p: &Principal) -> ApiResult<Value> {
    domain::admin(state, p).await?;
    let rows: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id',g.id,'name',g.name,'source',g.source,'parentId',g.parent_id,'active',g.active,\
         'members',CASE WHEN g.source='hr' THEN (SELECT count(*) FROM users u WHERE u.primary_group_id=g.id) \
         ELSE (SELECT count(*) FROM group_members m WHERE m.group_id=g.id) END) \
         FROM groups g ORDER BY lower(g.name),g.id",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(json!(rows))
}

pub async fn create(state: &State, p: &Principal, v: NewGroup) -> ApiResult<Value> {
    domain::admin(state, p).await?;
    let name = name(&v.name)?;
    if !["hr", "local"].contains(&v.source.as_str()) {
        return Err(ApiError::bad("invalid_group_source"));
    }
    let mut tx = state.pool.begin().await?;
    if let Some(parent) = v.parent_id
        && (v.source != "hr" || source(&mut tx, parent).await? != "hr")
    {
        return Err(ApiError::bad("hierarchy_is_hr_only"));
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO groups(id,name,source,parent_id) VALUES($1,$2,$3,$4)")
        .bind(id)
        .bind(&name)
        .bind(&v.source)
        .bind(v.parent_id)
        .execute(&mut *tx)
        .await?;
    domain::admin_locked(&mut tx, p).await?;
    audit(&mut tx, &p.id, "group.created", &format!("group:{id}")).await?;
    tx.commit().await?;
    Ok(json!({"id":id,"name":name,"source":v.source,"parentId":v.parent_id,"active":true}))
}

pub async fn update(state: &State, p: &Principal, id: Uuid, v: GroupChange) -> ApiResult<Value> {
    domain::admin(state, p).await?;
    let new_name = v.name.as_deref().map(name).transpose()?;
    let mut tx = state.pool.begin().await?;
    source(&mut tx, id).await?;
    sqlx::query("UPDATE groups SET name=COALESCE($2,name),active=COALESCE($3,active),updated_at=now() WHERE id=$1")
        .bind(id)
        .bind(&new_name)
        .bind(v.active)
        .execute(&mut *tx)
        .await?;
    let action = match v.active {
        Some(false) => "group.deactivated",
        Some(true) => "group.activated",
        None => "group.renamed",
    };
    domain::admin_locked(&mut tx, p).await?;
    audit(&mut tx, &p.id, action, &format!("group:{id}")).await?;
    tx.commit().await?;
    Ok(json!({"id":id,"updated":true}))
}

pub async fn set_parent(
    state: &State,
    p: &Principal,
    id: Uuid,
    v: GroupParent,
) -> ApiResult<Value> {
    domain::admin(state, p).await?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(HIERARCHY_LOCK)
        .execute(&mut *tx)
        .await?;
    if source(&mut tx, id).await? != "hr" {
        return Err(ApiError::bad("hierarchy_is_hr_only"));
    }
    if let Some(parent) = v.parent_id {
        if source(&mut tx, parent).await? != "hr" {
            return Err(ApiError::bad("hierarchy_is_hr_only"));
        }
        // Re-read the latest relations under the lock: the new parent must not be a
        // descendant of this group (or the group itself).
        let cycle: bool = sqlx::query_scalar(
            "WITH RECURSIVE up(id) AS (SELECT $1::uuid UNION SELECT g.parent_id FROM groups g JOIN up ON g.id=up.id WHERE g.parent_id IS NOT NULL) \
             SELECT EXISTS(SELECT 1 FROM up WHERE id=$2)",
        )
        .bind(parent)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if cycle {
            return Err(ApiError::conflict("group_hierarchy_cycle"));
        }
    }
    sqlx::query("UPDATE groups SET parent_id=$2,updated_at=now() WHERE id=$1")
        .bind(id)
        .bind(v.parent_id)
        .execute(&mut *tx)
        .await?;
    domain::admin_locked(&mut tx, p).await?;
    audit(&mut tx, &p.id, "group.parent_set", &format!("group:{id}")).await?;
    tx.commit().await?;
    Ok(json!({"id":id,"parentId":v.parent_id}))
}

pub async fn members(state: &State, p: &Principal, id: Uuid) -> ApiResult<Value> {
    domain::admin(state, p).await?;
    let rows: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id',u.id,'name',u.name,'enabled',u.enabled) FROM groups g JOIN users u ON \
         (g.source='hr' AND u.primary_group_id=g.id) OR (g.source='local' AND EXISTS(SELECT 1 FROM group_members m WHERE m.group_id=g.id AND m.user_id=u.id)) \
         WHERE g.id=$1 ORDER BY lower(u.name),u.id",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    Ok(json!(rows))
}

pub async fn set_member(state: &State, p: &Principal, id: Uuid, v: Membership) -> ApiResult<Value> {
    domain::admin(state, p).await?;
    let mut tx = state.pool.begin().await?;
    if source(&mut tx, id).await? != "local" {
        // HR membership follows the user's primary group exactly.
        return Err(ApiError::bad("hr_membership_is_primary_group"));
    }
    if v.member {
        sqlx::query(
            "INSERT INTO group_members(group_id,user_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(&v.user_id)
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query("DELETE FROM group_members WHERE group_id=$1 AND user_id=$2")
            .bind(id)
            .bind(&v.user_id)
            .execute(&mut *tx)
            .await?;
    }
    let action = if v.member {
        "group.member_added"
    } else {
        "group.member_removed"
    };
    domain::admin_locked(&mut tx, p).await?;
    audit(
        &mut tx,
        &p.id,
        action,
        &format!("group:{id}:user:{}", v.user_id),
    )
    .await?;
    tx.commit().await?;
    Ok(json!({"groupId":id,"userId":v.user_id,"member":v.member}))
}

pub async fn set_primary_group(
    state: &State,
    p: &Principal,
    user: &str,
    v: PrimaryGroup,
) -> ApiResult<Value> {
    domain::admin(state, p).await?;
    let mut tx = state.pool.begin().await?;
    if let Some(group) = v.group_id
        && source(&mut tx, group).await? != "hr"
    {
        return Err(ApiError::bad("primary_group_must_be_hr"));
    }
    // Both rows may be updated by another administrator assigning a primary group.
    // Lock them in one order rather than target first and actor at final approval;
    // NO KEY UPDATE remains compatible with ordinary user foreign-key checks.
    let users = sqlx::query(
        "SELECT id,primary_group_id FROM users WHERE id=ANY($1) ORDER BY id FOR NO KEY UPDATE",
    )
    .bind(vec![p.id.as_str(), user])
    .fetch_all(&mut *tx)
    .await?;
    let before = users
        .iter()
        .find(|row| row.get::<String, _>("id") == user)
        .ok_or_else(ApiError::not_found)?
        .get::<Option<Uuid>, _>("primary_group_id");
    sqlx::query("UPDATE users SET primary_group_id=$2 WHERE id=$1")
        .bind(user)
        .bind(v.group_id)
        .execute(&mut *tx)
        .await?;
    domain::admin_locked(&mut tx, p).await?;
    audit(
        &mut tx,
        &p.id,
        "user.primary_group_set",
        &format!(
            "user:{user}:from:{}:to:{}",
            before.map_or_else(|| "none".into(), |id| id.to_string()),
            v.group_id
                .map_or_else(|| "none".into(), |id| id.to_string()),
        ),
    )
    .await?;
    tx.commit().await?;
    Ok(json!({"userId":user,"groupId":v.group_id}))
}

pub async fn grants(state: &State, p: &Principal) -> ApiResult<Value> {
    domain::admin(state, p).await?;
    let rows: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('groupId',group_id,'appId',app_id,'environment',environment,'permission',permission) \
         FROM group_grants ORDER BY app_id,environment,group_id,permission",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(json!(rows))
}

pub async fn set_grant(state: &State, p: &Principal, v: GroupGrant) -> ApiResult<Value> {
    domain::admin(state, p).await?;
    environment(&v.environment)?;
    if !["use", "develop"].contains(&v.permission.as_str()) {
        return Err(ApiError::bad("group_permission_not_allowed"));
    }
    let mut tx = state.pool.begin().await?;
    source(&mut tx, v.group_id).await?;
    if v.enabled {
        sqlx::query("INSERT INTO group_grants(group_id,app_id,environment,permission) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
            .bind(v.group_id).bind(&v.app_id).bind(&v.environment).bind(&v.permission)
            .execute(&mut *tx).await?;
    } else {
        sqlx::query("DELETE FROM group_grants WHERE group_id=$1 AND app_id=$2 AND environment=$3 AND permission=$4")
            .bind(v.group_id).bind(&v.app_id).bind(&v.environment).bind(&v.permission)
            .execute(&mut *tx).await?;
    }
    let action = if v.enabled {
        "group_grant.enabled"
    } else {
        "group_grant.disabled"
    };
    let target = format!(
        "group:{}:app:{}:{}:{}",
        v.group_id, v.app_id, v.environment, v.permission
    );
    domain::admin_locked(&mut tx, p).await?;
    audit(&mut tx, &p.id, action, &target).await?;
    tx.commit().await?;
    Ok(json!({"updated":true}))
}

pub async fn access(state: &State, p: &Principal, app: &str) -> ApiResult<Value> {
    domain::admin(state, p).await?;
    let row = sqlx::query(
        "SELECT a.id, COALESCE((SELECT disabled FROM app_controls c WHERE c.app_id=a.id),false) AS disabled, \
         COALESCE((SELECT mode FROM app_admission x WHERE x.app_id=a.id AND x.environment='dev'),'selected') AS dev, \
         COALESCE((SELECT mode FROM app_admission x WHERE x.app_id=a.id AND x.environment='prod'),'selected') AS prod, \
         COALESCE((SELECT jsonb_agg(jsonb_build_object('userId',g.user_id,'environment',g.environment,'permission',g.permission) ORDER BY g.environment,g.user_id,g.permission) FROM grants g WHERE g.app_id=a.id),'[]') AS users, \
         COALESCE((SELECT jsonb_agg(jsonb_build_object('groupId',g.group_id,'environment',g.environment,'permission',g.permission) ORDER BY g.environment,g.group_id,g.permission) FROM group_grants g WHERE g.app_id=a.id),'[]') AS groups \
         FROM apps a WHERE a.id=$1",
    )
    .bind(app)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(ApiError::not_found)?;
    Ok(json!({
        "appId": app,
        "disabled": row.get::<bool, _>("disabled"),
        "admission": {"dev": row.get::<String, _>("dev"), "prod": row.get::<String, _>("prod")},
        "userGrants": row.get::<Value, _>("users"),
        "groupGrants": row.get::<Value, _>("groups"),
    }))
}

pub async fn set_admission(
    state: &State,
    p: &Principal,
    app: &str,
    v: Admission,
) -> ApiResult<Value> {
    domain::admin(state, p).await?;
    environment(&v.environment)?;
    if !["selected", "all"].contains(&v.mode.as_str()) {
        return Err(ApiError::bad("invalid_admission_mode"));
    }
    let mut tx = state.pool.begin().await?;
    sqlx::query("INSERT INTO app_admission(app_id,environment,mode,changed_by) VALUES($1,$2,$3,$4) ON CONFLICT(app_id,environment) DO UPDATE SET mode=excluded.mode,changed_by=excluded.changed_by,changed_at=now()")
        .bind(app).bind(&v.environment).bind(&v.mode).bind(&p.id)
        .execute(&mut *tx).await?;
    let target = format!("app:{app}:{}:{}", v.environment, v.mode);
    domain::admin_locked(&mut tx, p).await?;
    audit(&mut tx, &p.id, "app.admission_set", &target).await?;
    tx.commit().await?;
    Ok(json!({"appId":app,"environment":v.environment,"mode":v.mode}))
}

pub async fn set_control(
    state: &State,
    p: &Principal,
    app: &str,
    v: AppControl,
) -> ApiResult<Value> {
    domain::admin(state, p).await?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("INSERT INTO app_controls(app_id,disabled,changed_by) VALUES($1,$2,$3) ON CONFLICT(app_id) DO UPDATE SET disabled=excluded.disabled,changed_by=excluded.changed_by,changed_at=now()")
        .bind(app).bind(v.disabled).bind(&p.id)
        .execute(&mut *tx).await?;
    let action = if v.disabled {
        "app.disabled"
    } else {
        "app.enabled"
    };
    domain::admin_locked(&mut tx, p).await?;
    audit(&mut tx, &p.id, action, &format!("app:{app}")).await?;
    tx.commit().await?;
    Ok(json!({"appId":app,"disabled":v.disabled}))
}

/// API catalog entries for MCP and CLI discovery.
pub fn catalog() -> Vec<Value> {
    vec![
        json!({"id":"list_groups","method":"GET","path":"/api/v1/groups","permission":"admin","description":"List HR and local groups with member counts; the core database is the group source of truth"}),
        json!({"id":"create_group","method":"POST","path":"/api/v1/groups","permission":"admin","description":"Create an HR group (members by exact primary group, optional HR parent) or a local group (manual members); hierarchy never grants permissions","input":schemars::schema_for!(NewGroup)}),
        json!({"id":"update_group","method":"PATCH","path":"/api/v1/groups/{id}","permission":"admin","description":"Rename or deactivate a group; deactivated groups grant nothing","input":schemars::schema_for!(GroupChange)}),
        json!({"id":"set_group_parent","method":"PUT","path":"/api/v1/groups/{id}/parent","permission":"admin","description":"Move an HR group under another HR group or to the top; cycles are rejected under a hierarchy lock","input":schemars::schema_for!(GroupParent)}),
        json!({"id":"list_group_members","method":"GET","path":"/api/v1/groups/{id}/members","permission":"admin","description":"Current members: primary-group users for HR groups, assigned users for local groups"}),
        json!({"id":"set_group_member","method":"PUT","path":"/api/v1/groups/{id}/members","permission":"admin","description":"Add or remove a user of a local group; effective on the next request and for queued work","input":schemars::schema_for!(Membership)}),
        json!({"id":"set_primary_group","method":"PUT","path":"/api/v1/users/{id}/primary-group","permission":"admin","description":"Set or clear a user's primary HR group","input":schemars::schema_for!(PrimaryGroup)}),
        json!({"id":"list_group_grants","method":"GET","path":"/api/v1/group-grants","permission":"admin","description":"List group app permissions"}),
        json!({"id":"set_group_grant","method":"PUT","path":"/api/v1/group-grants","permission":"admin","description":"Grant or revoke use or develop to a group for an app environment; deploy is per user only","input":schemars::schema_for!(GroupGrant)}),
        json!({"id":"get_app_access","method":"GET","path":"/api/v1/apps/{app}/access","permission":"admin","description":"Company switch, admission mode per environment and the user and group grants of one app"}),
        json!({"id":"set_app_admission","method":"PUT","path":"/api/v1/apps/{app}/admission","permission":"admin","description":"Set an environment to selected (explicit user and group grants) or all (every enabled user, chosen explicitly)","input":schemars::schema_for!(Admission)}),
        json!({"id":"set_app_control","method":"PUT","path":"/api/v1/apps/{app}/control","permission":"admin","description":"Company switch: a disabled app cannot be used by anyone, administrators included; developers keep dev to repair it","input":schemars::schema_for!(AppControl)}),
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_are_trimmed_bounded_and_printable() {
        assert_eq!(super::name("  영업 1팀 ").unwrap(), "영업 1팀");
        assert!(super::name("   ").is_err());
        assert!(super::name(&"가".repeat(121)).is_err());
        assert!(super::name("a\nb").is_err());
        assert!(super::environment("staging").is_err());
    }
}
