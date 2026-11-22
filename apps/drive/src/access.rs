//! Space access: the miy stand-in for AI-DO workspace roles.
//!
//! AI-DO files belonged to a workspace whose members had a role (`viewer`,
//! `member`, `admin`, `owner`; domains/auth/app_space_roles.py). Drive keeps
//! that in its own spaces with the SDK resource ACL (kind `space`), so a space
//! can be shared with people and directory groups:
//!
//! | AI-DO workspace role | drive space role | ACL actions |
//! | --- | --- | --- |
//! | viewer | `viewer` | `space.read` |
//! | member | `member` | `space.read`, `files.write` |
//! | admin, owner | `admin` | `space.read`, `files.write`, `space.admin`, `acl.manage` |
//!
//! The creator of a space is its protected owner (an `admin` grant). The
//! platform administrator flag grants nothing.
//!
//! A frame request reads the caller's current directory groups once and
//! evaluates the role from the grants in SQL (the same predicate the SDK ACL
//! uses). Writes lock the space row and their own rows first and then check
//! again with the SDK (`require_in`, which holds the ACL share lock) before
//! changing anything.
use chrono::NaiveDateTime;
use miy_app_sdk::{
    Identity,
    acl::{MANAGE, ResourceKey, RoleDefinition},
};
use sqlx::{FromRow, PgExecutor, PgPool, Postgres, Transaction};

use crate::{
    AppState,
    error::{ApiError, ApiResult},
    people,
};

pub const SPACE_KIND: &str = "space";
pub const ACTION_READ: &str = "space.read";
pub const ACTION_WRITE: &str = "files.write";
pub const ACTION_ADMIN: &str = "space.admin";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Viewer,
    Member,
    Admin,
}

impl Role {
    pub fn from_id(value: &str) -> Option<Self> {
        match value {
            "viewer" => Some(Self::Viewer),
            "member" => Some(Self::Member),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Member => "member",
            Self::Admin => "admin",
        }
    }
}

pub fn acl_roles() -> Vec<RoleDefinition> {
    vec![
        RoleDefinition {
            id: "viewer".into(),
            label: "보기".into(),
            actions: vec![ACTION_READ.into()],
        },
        RoleDefinition {
            id: "member".into(),
            label: "멤버".into(),
            actions: vec![ACTION_READ.into(), ACTION_WRITE.into()],
        },
        RoleDefinition {
            id: "admin".into(),
            label: "관리자".into(),
            actions: vec![
                ACTION_READ.into(),
                ACTION_WRITE.into(),
                ACTION_ADMIN.into(),
                MANAGE.into(),
            ],
        },
    ]
}

pub fn space_key(space_id: &str) -> ResourceKey {
    ResourceKey {
        kind: SPACE_KIND.into(),
        id: space_id.into(),
    }
}

#[derive(Clone, Debug, FromRow)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub created_by: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

pub const SPACE_COLUMNS: &str = "id, name, created_by, created_at, updated_at";

/// The caller's standing in one space.
#[derive(Clone, Debug)]
pub struct SpaceAccess {
    pub space: Space,
    pub role: Role,
    pub user_id: String,
}

impl SpaceAccess {
    /// AI-DO `is_workspace_admin`.
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }

    /// AI-DO `app_space_role_allows(role, "member")`: may add files and folders.
    pub fn is_member(&self) -> bool {
        self.role >= Role::Member
    }
}

pub fn space_not_found() -> ApiError {
    ApiError::not_found("files.space_not_found", "Space not found.")
}

pub fn denied() -> ApiError {
    ApiError::forbidden("acl_denied", "acl_denied")
}

fn directory_unavailable() -> ApiError {
    ApiError::unavailable("acl_directory_unavailable", "acl_directory_unavailable")
}

pub async fn load_space<'e, E: PgExecutor<'e>>(
    executor: E,
    space_id: &str,
) -> ApiResult<Option<Space>> {
    let sql = format!("SELECT {SPACE_COLUMNS} FROM drive_spaces WHERE id = $1");
    Ok(sqlx::query_as::<_, Space>(&sql)
        .bind(space_id)
        .fetch_optional(executor)
        .await?)
}

/// The caller's current groups for a frame request (one directory read).
/// Fails closed when the directory cannot answer.
pub async fn frame_groups(identity: &Identity) -> ApiResult<Vec<String>> {
    people::current_groups(identity)
        .await
        .ok_or_else(directory_unavailable)
}

/// The groups a platform call carries: the core signs the person's current
/// group IDs into `identity.context.groups` when this app is approved for
/// `platform:directory:read` (none otherwise: direct grants only).
pub fn platform_groups(identity: &Identity) -> Vec<String> {
    identity
        .context
        .groups
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|group| group.to_ascii_lowercase())
        .collect()
}

/// The highest role granted to the user or one of `groups` on a space.
pub async fn role_in<'e, E: PgExecutor<'e>>(
    executor: E,
    identity: &Identity,
    space_id: &str,
    groups: &[String],
) -> ApiResult<Option<Role>> {
    let Some(environment) = identity.context.environment.as_deref() else {
        return Ok(None);
    };
    let roles: Vec<String> = sqlx::query_scalar(
        "SELECT role FROM app.miy_acl_grants WHERE installation_id = $1::uuid AND environment = $2 \
         AND kind = $3 AND resource_id = $4 AND ((subject_kind = 'user' AND subject_id = $5) \
         OR (subject_kind = 'group' AND subject_id = ANY($6)))",
    )
    .bind(&identity.context.installation_id)
    .bind(environment)
    .bind(SPACE_KIND)
    .bind(space_id)
    .bind(&identity.user_id)
    .bind(groups)
    .fetch_all(executor)
    .await?;
    Ok(roles.iter().filter_map(|role| Role::from_id(role)).max())
}

/// Every space the user (or one of `groups`) holds a role in, with that role.
pub async fn roles_by_space(
    pool: &PgPool,
    identity: &Identity,
    groups: &[String],
) -> ApiResult<Vec<(String, Role)>> {
    let Some(environment) = identity.context.environment.as_deref() else {
        return Ok(Vec::new());
    };
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT resource_id, role FROM app.miy_acl_grants WHERE installation_id = $1::uuid \
         AND environment = $2 AND kind = $3 AND ((subject_kind = 'user' AND subject_id = $4) \
         OR (subject_kind = 'group' AND subject_id = ANY($5)))",
    )
    .bind(&identity.context.installation_id)
    .bind(environment)
    .bind(SPACE_KIND)
    .bind(&identity.user_id)
    .bind(groups)
    .fetch_all(pool)
    .await?;
    let mut best: std::collections::BTreeMap<String, Role> = std::collections::BTreeMap::new();
    for (space, role) in rows {
        if let Some(role) = Role::from_id(&role) {
            let entry = best.entry(space).or_insert(role);
            if role > *entry {
                *entry = role;
            }
        }
    }
    Ok(best.into_iter().collect())
}

/// A read in a frame request: the space and the caller's current role.
pub async fn read_access(
    state: &AppState,
    identity: &Identity,
    space_id: &str,
) -> ApiResult<SpaceAccess> {
    let space = load_space(&state.pool, space_id)
        .await?
        .ok_or_else(space_not_found)?;
    let groups = frame_groups(identity).await?;
    let role = role_in(&state.pool, identity, space_id, &groups)
        .await?
        .ok_or_else(denied)?;
    Ok(SpaceAccess {
        space,
        role,
        user_id: identity.user_id.clone(),
    })
}

/// First step of a write: hold the space row (a concurrent space deletion
/// waits) before the write locks its own rows.
pub async fn lock_space(tx: &mut Transaction<'_, Postgres>, space_id: &str) -> ApiResult<Space> {
    let sql = format!("SELECT {SPACE_COLUMNS} FROM drive_spaces WHERE id = $1 FOR SHARE");
    sqlx::query_as::<_, Space>(&sql)
        .bind(space_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(space_not_found)
}

/// Last step before a write changes rows it has locked: the SDK check holds
/// the ACL share lock to the commit, then the role decides what is allowed.
pub async fn recheck(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    identity: &Identity,
    space: Space,
    groups: &[String],
) -> ApiResult<SpaceAccess> {
    state
        .acl
        .require_in(tx, identity, &space_key(&space.id), ACTION_READ)
        .await?;
    let role = role_in(&mut **tx, identity, &space.id, groups)
        .await?
        .ok_or_else(denied)?;
    Ok(SpaceAccess {
        space,
        role,
        user_id: identity.user_id.clone(),
    })
}

/// A platform call (no token for a directory call): the role from the
/// person's direct grants and the current groups the call carries.
pub async fn platform_access(
    pool: &PgPool,
    identity: &Identity,
    space_id: &str,
) -> ApiResult<Option<SpaceAccess>> {
    let Some(space) = load_space(pool, space_id).await? else {
        return Ok(None);
    };
    let groups = platform_groups(identity);
    Ok(role_in(pool, identity, space_id, &groups)
        .await?
        .map(|role| SpaceAccess {
            space,
            role,
            user_id: identity.user_id.clone(),
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use miy_app_sdk::acl::Acl;

    #[tokio::test]
    async fn roles_keep_ai_do_workspace_ranks_and_viewer_is_the_default() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://localhost/unused")
            .unwrap();
        assert!(Acl::new(pool, acl_roles()).is_ok());
        let roles = acl_roles();
        for pair in roles.windows(2) {
            assert!(pair[0].actions.iter().all(|a| pair[1].actions.contains(a)));
        }
        assert!(!roles[0].actions.iter().any(|a| a == MANAGE));
        assert!(Role::Admin > Role::Member && Role::Member > Role::Viewer);
        assert_eq!(Role::from_id("owner"), None);
    }
}
