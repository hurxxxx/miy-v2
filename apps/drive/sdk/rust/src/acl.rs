//! Application-owned resource ACLs. Apply [`MIGRATION`] in an explicit app
//! migration. This library neither creates platform Use grants nor trusts the
//! administrator flag. Each check uses the original caller and fresh Core group
//! membership. Business creation, invitation and object-state rules remain the
//! application's responsibility.
use crate::{DirectoryKind, DirectoryQuery, Identity};
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool, Postgres, Row, Transaction};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

pub const MIGRATION: &str = include_str!("acl.sql");
pub const MANAGE: &str = "acl.manage";
const MAX_ENTRIES: usize = 100;
const MAX_MEMBERSHIPS: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AclError {
    pub code: &'static str,
    pub status: u16,
}
impl std::fmt::Display for AclError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code)
    }
}
impl std::error::Error for AclError {}
impl IntoResponse for AclError {
    fn into_response(self) -> Response {
        (
            StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(json!({"error":{"code":self.code}})),
        )
            .into_response()
    }
}
fn error(code: &'static str, status: u16) -> AclError {
    AclError { code, status }
}
fn database(_: sqlx::Error) -> AclError {
    error("acl_database_unavailable", 503)
}
fn denied() -> AclError {
    error("acl_denied", 403)
}
fn directory_error(error: crate::CallError) -> AclError {
    if matches!(error.status, Some(401 | 403)) {
        denied()
    } else {
        self::error("acl_directory_unavailable", 503)
    }
}
fn invalid() -> AclError {
    error("acl_invalid_request", 400)
}

type Result<T> = std::result::Result<T, AclError>;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubjectKind {
    User,
    Group,
}
impl SubjectKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Group => "group",
        }
    }
    fn parse(value: &str) -> Result<Self> {
        match value {
            "user" => Ok(Self::User),
            "group" => Ok(Self::Group),
            _ => Err(error("acl_database_unavailable", 503)),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubjectRef {
    pub kind: SubjectKind,
    pub id: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub subject: SubjectRef,
    pub role: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceKey {
    pub kind: String,
    pub id: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleDefinition {
    pub id: String,
    pub label: String,
    pub actions: Vec<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ShareSubject {
    pub kind: SubjectKind,
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ShareEntry {
    pub subject: ShareSubject,
    pub role: String,
    pub protected: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ShareRole {
    pub id: String,
    pub label: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareSnapshot {
    pub revision: i64,
    pub entries: Vec<ShareEntry>,
    /// In the order given to [`Acl::new`]. The order grants nothing.
    pub roles: Vec<ShareRole>,
    /// Role to preselect for a new subject: the one role whose actions every
    /// other role also has and that cannot manage sharing. `None` when no single
    /// least role exists; the person then chooses explicitly.
    #[serde(default)]
    pub default_role: Option<String>,
    pub can_manage: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePage {
    pub resources: Vec<ResourceKey>,
    pub next_cursor: Option<String>,
}

#[derive(Clone)]
pub struct Acl {
    pool: PgPool,
    roles: BTreeMap<String, RoleDefinition>,
    /// Role IDs as the app declared them, for display.
    order: Vec<String>,
    default_role: Option<String>,
}
struct Scope<'a> {
    installation: &'a str,
    environment: &'a str,
}
struct Stored {
    owner: String,
    revision: i64,
    grants: Vec<Grant>,
}
struct Actor {
    user: String,
    groups: Vec<String>,
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b':'))
}
fn reference(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 256
        && !value.chars().any(char::is_control)
        && !value.contains(',')
}
fn scope(identity: &Identity) -> Result<Scope<'_>> {
    let environment = identity
        .context
        .environment
        .as_deref()
        .filter(|v| matches!(*v, "dev" | "prod"))
        .ok_or_else(denied)?;
    if !crate::valid_uuid(&identity.context.installation_id) || !reference(&identity.user_id) {
        return Err(denied());
    }
    Ok(Scope {
        installation: &identity.context.installation_id,
        environment,
    })
}
fn resource(key: &ResourceKey) -> Result<()> {
    if !identifier(&key.kind) || !reference(&key.id) {
        return Err(invalid());
    }
    Ok(())
}

impl Acl {
    pub fn new(pool: PgPool, roles: Vec<RoleDefinition>) -> Result<Self> {
        if roles.is_empty() || roles.len() > 32 {
            return Err(error("acl_invalid_role", 400));
        }
        let order: Vec<String> = roles.iter().map(|role| role.id.clone()).collect();
        let default_role = least_role(&roles);
        let mut map = BTreeMap::new();
        for role in roles {
            if !identifier(&role.id)
                || role.label.trim().is_empty()
                || role.label.chars().count() > 120
                || role.actions.is_empty()
                || role.actions.len() > 64
                || role.actions.iter().any(|a| !identifier(a))
                || map.insert(role.id.clone(), role).is_some()
            {
                return Err(error("acl_invalid_role", 400));
            }
        }
        if !map.values().any(|r| r.actions.iter().any(|a| a == MANAGE)) {
            return Err(error("acl_invalid_role", 400));
        }
        Ok(Self {
            pool,
            roles: map,
            order,
            default_role,
        })
    }
    fn role_allows(&self, role: &str, action: &str) -> bool {
        self.roles
            .get(role)
            .is_some_and(|r| r.actions.iter().any(|a| a == action))
    }
    fn permits(&self, actor: &Actor, grants: &[Grant], action: &str) -> bool {
        grants.iter().any(|g| {
            self.role_allows(&g.role, action)
                && match g.subject.kind {
                    SubjectKind::User => g.subject.id == actor.user,
                    SubjectKind::Group => actor.groups.binary_search(&g.subject.id).is_ok(),
                }
        })
    }
    fn normalize(&self, grants: &[Grant]) -> Result<Vec<Grant>> {
        if grants.is_empty() || grants.len() > MAX_ENTRIES {
            return Err(invalid());
        }
        let mut result = grants.to_vec();
        for grant in &mut result {
            // Core reports group IDs in lowercase. Store the same form, or an
            // uppercase ID would never match the caller's current memberships.
            if grant.subject.kind == SubjectKind::Group {
                grant.subject.id.make_ascii_lowercase();
            }
        }
        result.sort_by(|a, b| a.subject.cmp(&b.subject));
        for (index, grant) in result.iter().enumerate() {
            if !reference(&grant.subject.id)
                || (grant.subject.kind == SubjectKind::Group
                    && !crate::valid_uuid(&grant.subject.id))
            {
                return Err(error("acl_invalid_subject", 400));
            }
            if !self.roles.contains_key(&grant.role) {
                return Err(error("acl_invalid_role", 400));
            }
            if index > 0 && result[index - 1].subject == grant.subject {
                return Err(error("acl_invalid_subject", 400));
            }
        }
        Ok(result)
    }
    /// Creation is never inferred from an existing object. Authorize business
    /// creation before calling this method, in the same transaction as the row.
    pub async fn create_resource_in(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        identity: &Identity,
        key: &ResourceKey,
        owner_role: &str,
    ) -> Result<ShareSnapshot> {
        resource(key)?;
        let s = scope(identity)?;
        if !self.role_allows(owner_role, MANAGE) {
            return Err(error("acl_invalid_role", 400));
        }
        // Even a direct-user grant verifies that the caller remains active and
        // that the app still has permission to use the platform directory.
        let _actor = actor(identity, Some(&[])).await?;
        let grants = self.normalize(&[Grant {
            subject: SubjectRef {
                kind: SubjectKind::User,
                id: identity.user_id.clone(),
            },
            role: owner_role.into(),
        }])?;
        let names = names(identity, &grants).await?;
        if !names.contains_key(&grants[0].subject) {
            return Err(error("acl_invalid_subject", 400));
        }
        let inserted=sqlx::query("INSERT INTO app.miy_acl_resources(installation_id,environment,kind,resource_id,owner_id) VALUES($1::uuid,$2,$3,$4,$5) ON CONFLICT DO NOTHING")
            .bind(s.installation).bind(s.environment).bind(&key.kind).bind(&key.id).bind(&identity.user_id)
            .execute(&mut **tx).await.map_err(database)?.rows_affected();
        if inserted != 1 {
            return Err(error("acl_resource_exists", 409));
        }
        insert_grants(&mut **tx, &s, key, &grants).await?;
        let stored = Stored {
            owner: identity.user_id.clone(),
            revision: 1,
            grants,
        };
        audit(
            &mut **tx,
            &s,
            key,
            &identity.user_id,
            "create",
            None,
            &[],
            &stored.grants,
            1,
        )
        .await?;
        Ok(self.snapshot(&stored, &names))
    }
    /// A check with no write to protect. The grants are read and the pooled
    /// connection returned before the directory call, so a slow directory never
    /// pins a connection or the resource lock. Use [`Acl::require_in`] when a
    /// business write must follow under the same lock.
    pub async fn require(
        &self,
        identity: &Identity,
        key: &ResourceKey,
        action: &str,
    ) -> Result<()> {
        if !identifier(action) {
            return Err(invalid());
        }
        let stored = self.read(identity, key).await?;
        let actor = actor(identity, Some(&group_ids(&stored.grants))).await?;
        if !self.permits(&actor, &stored.grants, action) {
            return Err(denied());
        }
        Ok(())
    }
    /// One consistent read of a resource and its grants in a short transaction.
    async fn read(&self, identity: &Identity, key: &ResourceKey) -> Result<Stored> {
        let mut tx = self.pool.begin().await.map_err(database)?;
        let stored = load(&mut tx, &scope(identity)?, key, false).await?;
        tx.commit().await.map_err(database)?;
        Ok(stored)
    }
    /// Keep this transaction open through the business mutation. The resource
    /// share lock prevents a concurrent ACL replacement before that write.
    pub async fn require_in(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        identity: &Identity,
        key: &ResourceKey,
        action: &str,
    ) -> Result<()> {
        if !identifier(action) {
            return Err(invalid());
        }
        let stored = load(&mut **tx, &scope(identity)?, key, false).await?;
        let actor = actor(identity, Some(&group_ids(&stored.grants))).await?;
        if !self.permits(&actor, &stored.grants, action) {
            return Err(denied());
        }
        Ok(())
    }
    /// Filtering occurs in SQL before LIMIT/cursor. The cursor is just a stable
    /// resource ID; changing it cannot bypass the current permission predicate.
    pub async fn list_resources(
        &self,
        identity: &Identity,
        kind: &str,
        action: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<ResourcePage> {
        if !identifier(kind)
            || !identifier(action)
            || !(1..=100).contains(&limit)
            || cursor.is_some_and(|c| !reference(c))
        {
            return Err(invalid());
        }
        let s = scope(identity)?;
        let actor = actor(identity, None).await?;
        let roles: Vec<&str> = self
            .roles
            .values()
            .filter(|r| r.actions.iter().any(|a| a == action))
            .map(|r| r.id.as_str())
            .collect();
        let rows=sqlx::query("SELECT r.resource_id FROM app.miy_acl_resources r WHERE r.installation_id=$1::uuid AND r.environment=$2 AND r.kind=$3 AND ($4::text IS NULL OR r.resource_id>$4) AND EXISTS(SELECT 1 FROM app.miy_acl_grants g WHERE g.installation_id=r.installation_id AND g.environment=r.environment AND g.kind=r.kind AND g.resource_id=r.resource_id AND g.role=ANY($7::text[]) AND ((g.subject_kind='user' AND g.subject_id=$5) OR (g.subject_kind='group' AND g.subject_id=ANY($6::text[])))) ORDER BY r.resource_id LIMIT $8")
            .bind(s.installation).bind(s.environment).bind(kind).bind(cursor).bind(&actor.user).bind(&actor.groups).bind(roles).bind(i64::from(limit)+1)
            .fetch_all(&self.pool).await.map_err(database)?;
        let mut resources = rows
            .iter()
            .map(|row| {
                Ok(ResourceKey {
                    kind: kind.into(),
                    id: row.try_get("resource_id").map_err(database)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let more = resources.len() > limit as usize;
        resources.truncate(limit as usize);
        let next_cursor = more.then(|| resources.last().unwrap().id.clone());
        Ok(ResourcePage {
            resources,
            next_cursor,
        })
    }
    pub async fn sharing(&self, identity: &Identity, key: &ResourceKey) -> Result<ShareSnapshot> {
        let stored = self.read(identity, key).await?;
        let actor = actor(identity, Some(&group_ids(&stored.grants))).await?;
        if !self.permits(&actor, &stored.grants, MANAGE) {
            return Err(denied());
        }
        let labels = names(identity, &stored.grants).await?;
        Ok(self.snapshot(&stored, &labels))
    }
    /// Replace all grants atomically with revision and request-ID checks. A lost
    /// response is resolved by replacement_result, never a blind write retry.
    pub async fn replace_in(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        identity: &Identity,
        key: &ResourceKey,
        expected_revision: i64,
        request_id: &str,
        entries: &[Grant],
    ) -> Result<ShareSnapshot> {
        if expected_revision < 1 || !crate::valid_uuid(request_id) {
            return Err(invalid());
        }
        let entries = self.normalize(entries)?;
        let s = scope(identity)?;
        let stored = load(&mut **tx, &s, key, true).await?;
        let actor = actor(identity, Some(&group_ids(&stored.grants))).await?;
        if !self.permits(&actor, &stored.grants, MANAGE) {
            return Err(denied());
        }
        let payload = json!({"expectedRevision":expected_revision,"entries":entries});
        if let Some((previous_actor, previous_payload, result)) =
            change(&mut **tx, &s, key, request_id).await?
        {
            if previous_actor != identity.user_id || previous_payload != payload {
                return Err(error("acl_request_conflict", 409));
            }
            return Ok(result);
        }
        if stored.revision != expected_revision {
            return Err(error("acl_revision_conflict", 409));
        }
        let labels = names(identity, &entries).await?;
        self.validate_replacement(&stored, &entries, &labels)?;
        let revision = stored
            .revision
            .checked_add(1)
            .ok_or_else(|| error("acl_revision_conflict", 409))?;
        sqlx::query("DELETE FROM app.miy_acl_grants WHERE installation_id=$1::uuid AND environment=$2 AND kind=$3 AND resource_id=$4")
            .bind(s.installation).bind(s.environment).bind(&key.kind).bind(&key.id).execute(&mut **tx).await.map_err(database)?;
        insert_grants(&mut **tx, &s, key, &entries).await?;
        sqlx::query("UPDATE app.miy_acl_resources SET revision=$5 WHERE installation_id=$1::uuid AND environment=$2 AND kind=$3 AND resource_id=$4")
            .bind(s.installation).bind(s.environment).bind(&key.kind).bind(&key.id).bind(revision).execute(&mut **tx).await.map_err(database)?;
        let result = self.snapshot(
            &Stored {
                owner: stored.owner.clone(),
                revision,
                grants: entries.clone(),
            },
            &labels,
        );
        let result_json = serde_json::to_string(&result).map_err(|_| invalid())?;
        sqlx::query("INSERT INTO app.miy_acl_changes(installation_id,environment,kind,resource_id,request_id,actor_id,payload,result) VALUES($1::uuid,$2,$3,$4,$5::uuid,$6,$7::jsonb,$8::jsonb)")
            .bind(s.installation).bind(s.environment).bind(&key.kind).bind(&key.id).bind(request_id).bind(&identity.user_id).bind(payload.to_string()).bind(result_json)
            .execute(&mut **tx).await.map_err(database)?;
        audit(
            &mut **tx,
            &s,
            key,
            &identity.user_id,
            "replace",
            Some(request_id),
            &stored.grants,
            &entries,
            revision,
        )
        .await?;
        Ok(result)
    }
    fn validate_replacement(
        &self,
        stored: &Stored,
        entries: &[Grant],
        labels: &BTreeMap<SubjectRef, String>,
    ) -> Result<()> {
        if !entries.iter().any(|g| {
            g.subject.kind == SubjectKind::User
                && g.subject.id == stored.owner
                && self.role_allows(&g.role, MANAGE)
        }) {
            return Err(error("acl_owner_protected", 409));
        }
        // Existing disabled entries may remain unchanged or be removed. New or
        // changed recipients must exist in the current active directory.
        if entries
            .iter()
            .any(|g| !labels.contains_key(&g.subject) && !stored.grants.contains(g))
        {
            return Err(error("acl_invalid_subject", 400));
        }
        if !entries.iter().any(|g| {
            g.subject.kind == SubjectKind::User
                && labels.contains_key(&g.subject)
                && self.role_allows(&g.role, MANAGE)
        }) {
            return Err(error("acl_last_manager", 409));
        }
        Ok(())
    }
    pub async fn replacement_result(
        &self,
        identity: &Identity,
        key: &ResourceKey,
        request_id: &str,
    ) -> Result<Option<ShareSnapshot>> {
        if !crate::valid_uuid(request_id) {
            return Err(invalid());
        }
        let s = scope(identity)?;
        let stored = self.read(identity, key).await?;
        let actor = actor(identity, Some(&group_ids(&stored.grants))).await?;
        if !self.permits(&actor, &stored.grants, MANAGE) {
            return Err(denied());
        }
        let mut connection = self.pool.acquire().await.map_err(database)?;
        change(&mut connection, &s, key, request_id)
            .await?
            .map(|(owner, _, result)| {
                if owner == identity.user_id {
                    Ok(result)
                } else {
                    Err(denied())
                }
            })
            .transpose()
    }
    fn snapshot(&self, stored: &Stored, labels: &BTreeMap<SubjectRef, String>) -> ShareSnapshot {
        ShareSnapshot {
            revision: stored.revision,
            can_manage: true,
            entries: stored
                .grants
                .iter()
                .map(|g| ShareEntry {
                    subject: ShareSubject {
                        kind: g.subject.kind.clone(),
                        id: g.subject.id.clone(),
                        name: labels
                            .get(&g.subject)
                            .cloned()
                            .unwrap_or_else(|| g.subject.id.clone()),
                    },
                    role: g.role.clone(),
                    protected: g.subject.kind == SubjectKind::User && g.subject.id == stored.owner,
                })
                .collect(),
            roles: self
                .order
                .iter()
                .filter_map(|id| self.roles.get(id))
                .map(|r| ShareRole {
                    id: r.id.clone(),
                    label: r.label.clone(),
                })
                .collect(),
            default_role: self.default_role.clone(),
        }
    }
}
/// The single role whose actions are a subset of every role's actions and that
/// cannot manage sharing. Action sets are only partially ordered, so there may
/// be none; a guess would hand out more than the person meant to give.
fn least_role(roles: &[RoleDefinition]) -> Option<String> {
    let mut least = roles.iter().filter(|role| {
        !role.actions.iter().any(|a| a == MANAGE)
            && roles
                .iter()
                .all(|other| role.actions.iter().all(|a| other.actions.contains(a)))
    });
    let role = least.next()?;
    least.next().is_none().then(|| role.id.clone())
}
fn group_ids(grants: &[Grant]) -> Vec<String> {
    grants
        .iter()
        .filter(|g| g.subject.kind == SubjectKind::Group)
        .map(|g| g.subject.id.clone())
        .collect()
}

async fn actor(identity: &Identity, candidates: Option<&[String]>) -> Result<Actor> {
    scope(identity)?;
    // A platform call (search, home card, notification check) cannot ask the
    // directory; Core signed the person's current groups into it instead
    // (only for an app approved to read the directory; none means none).
    if identity.context.platform {
        let signed = identity.context.groups.clone().unwrap_or_default();
        if signed.len() > MAX_MEMBERSHIPS || signed.iter().any(|g| !crate::valid_uuid(g)) {
            return Err(error("acl_directory_unavailable", 503));
        }
        let groups = match candidates {
            Some(ids) => signed.into_iter().filter(|g| ids.contains(g)).collect(),
            None => signed,
        };
        let mut groups: Vec<String> = groups;
        groups.sort();
        groups.dedup();
        return Ok(Actor {
            user: identity.user_id.clone(),
            groups,
        });
    }
    tokio::time::timeout(Duration::from_secs(15), async {
        if candidates.is_some_and(|v| v.len() > MAX_ENTRIES) {
            return Err(error("acl_membership_limit", 503));
        }
        let ids = candidates.filter(|v| !v.is_empty()).map(<[String]>::to_vec);
        let mut cursor = None;
        let mut seen = BTreeSet::new();
        let mut groups = BTreeSet::new();
        loop {
            let page = identity
                .directory(&DirectoryQuery {
                    kind: DirectoryKind::Memberships,
                    ids: ids.clone(),
                    limit: Some(100),
                    cursor: cursor.clone(),
                    ..Default::default()
                })
                .await
                .map_err(directory_error)?;
            if page.groups.iter().any(|g| !crate::valid_uuid(&g.id)) {
                return Err(error("acl_directory_unavailable", 503));
            }
            groups.extend(page.groups.into_iter().map(|g| g.id));
            if groups.len() > MAX_MEMBERSHIPS {
                return Err(error("acl_membership_limit", 503));
            }
            // No candidate groups: the first response rechecks actor/activity.
            if candidates.is_some_and(<[String]>::is_empty) {
                groups.clear();
                break;
            }
            match page.next_cursor {
                None => break,
                Some(next) if seen.insert(next.clone()) => {
                    cursor = Some(next);
                }
                _ => return Err(error("acl_directory_unavailable", 503)),
            }
        }
        Ok(Actor {
            user: identity.user_id.clone(),
            groups: groups.into_iter().collect(),
        })
    })
    .await
    .map_err(|_| error("acl_directory_unavailable", 503))?
}
async fn names(identity: &Identity, grants: &[Grant]) -> Result<BTreeMap<SubjectRef, String>> {
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut result = BTreeMap::new();
        for kind in [SubjectKind::User, SubjectKind::Group] {
            let ids: Vec<String> = grants
                .iter()
                .filter(|g| g.subject.kind == kind)
                .map(|g| g.subject.id.clone())
                .collect();
            if ids.is_empty() {
                continue;
            }
            let page = identity
                .directory(&DirectoryQuery {
                    kind: if kind == SubjectKind::User {
                        DirectoryKind::Users
                    } else {
                        DirectoryKind::Groups
                    },
                    ids: Some(ids.clone()),
                    limit: Some(100),
                    ..Default::default()
                })
                .await
                .map_err(directory_error)?;
            if page.next_cursor.is_some() {
                return Err(error("acl_directory_unavailable", 503));
            }
            for (id, name) in page
                .users
                .into_iter()
                .map(|u| (u.id, u.name))
                .chain(page.groups.into_iter().map(|g| (g.id, g.name)))
            {
                if !ids.contains(&id) {
                    return Err(error("acl_directory_unavailable", 503));
                }
                result.insert(
                    SubjectRef {
                        kind: kind.clone(),
                        id,
                    },
                    name,
                );
            }
        }
        Ok(result)
    })
    .await
    .map_err(|_| error("acl_directory_unavailable", 503))?
}
async fn load(
    connection: &mut PgConnection,
    scope: &Scope<'_>,
    key: &ResourceKey,
    write: bool,
) -> Result<Stored> {
    resource(key)?;
    let sql = if write {
        "SELECT owner_id,revision FROM app.miy_acl_resources WHERE installation_id=$1::uuid AND environment=$2 AND kind=$3 AND resource_id=$4 FOR UPDATE"
    } else {
        "SELECT owner_id,revision FROM app.miy_acl_resources WHERE installation_id=$1::uuid AND environment=$2 AND kind=$3 AND resource_id=$4 FOR SHARE"
    };
    let row = sqlx::query(sql)
        .bind(scope.installation)
        .bind(scope.environment)
        .bind(&key.kind)
        .bind(&key.id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(database)?
        .ok_or_else(denied)?;
    let rows=sqlx::query("SELECT subject_kind,subject_id,role FROM app.miy_acl_grants WHERE installation_id=$1::uuid AND environment=$2 AND kind=$3 AND resource_id=$4 ORDER BY subject_kind,subject_id LIMIT 101")
        .bind(scope.installation).bind(scope.environment).bind(&key.kind).bind(&key.id).fetch_all(&mut *connection).await.map_err(database)?;
    if rows.len() > MAX_ENTRIES {
        return Err(error("acl_database_unavailable", 503));
    }
    let grants = rows
        .iter()
        .map(|r| {
            Ok(Grant {
                subject: SubjectRef {
                    kind: SubjectKind::parse(r.try_get("subject_kind").map_err(database)?)?,
                    id: r.try_get("subject_id").map_err(database)?,
                },
                role: r.try_get("role").map_err(database)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Stored {
        owner: row.try_get("owner_id").map_err(database)?,
        revision: row.try_get("revision").map_err(database)?,
        grants,
    })
}
async fn insert_grants(
    connection: &mut PgConnection,
    s: &Scope<'_>,
    key: &ResourceKey,
    grants: &[Grant],
) -> Result<()> {
    for g in grants {
        sqlx::query("INSERT INTO app.miy_acl_grants(installation_id,environment,kind,resource_id,subject_kind,subject_id,role) VALUES($1::uuid,$2,$3,$4,$5,$6,$7)")
            .bind(s.installation).bind(s.environment).bind(&key.kind).bind(&key.id).bind(g.subject.kind.as_str()).bind(&g.subject.id).bind(&g.role)
            .execute(&mut *connection).await.map_err(database)?;
    }
    Ok(())
}
async fn audit(
    connection: &mut PgConnection,
    s: &Scope<'_>,
    key: &ResourceKey,
    actor: &str,
    action: &str,
    request_id: Option<&str>,
    before: &[Grant],
    after: &[Grant],
    revision: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO app.miy_acl_audit(installation_id,environment,kind,resource_id,actor_id,action,request_id,before_grants,after_grants,revision) VALUES($1::uuid,$2,$3,$4,$5,$6,$7::uuid,$8::jsonb,$9::jsonb,$10)")
        .bind(s.installation).bind(s.environment).bind(&key.kind).bind(&key.id).bind(actor).bind(action).bind(request_id)
        .bind(serde_json::to_string(before).map_err(|_| invalid())?).bind(serde_json::to_string(after).map_err(|_| invalid())?).bind(revision)
        .execute(connection).await.map_err(database)?;
    Ok(())
}
async fn change(
    connection: &mut PgConnection,
    s: &Scope<'_>,
    key: &ResourceKey,
    request_id: &str,
) -> Result<Option<(String, Value, ShareSnapshot)>> {
    sqlx::query("SELECT actor_id,payload::text,result::text FROM app.miy_acl_changes WHERE installation_id=$1::uuid AND environment=$2 AND kind=$3 AND resource_id=$4 AND request_id=$5::uuid")
        .bind(s.installation).bind(s.environment).bind(&key.kind).bind(&key.id).bind(request_id).fetch_optional(connection).await.map_err(database)?
        .map(|row| Ok((row.try_get("actor_id").map_err(database)?,serde_json::from_str(&row.try_get::<String,_>("payload").map_err(database)?).map_err(|_| error("acl_database_unavailable",503))?,serde_json::from_str(&row.try_get::<String,_>("result").map_err(database)?).map_err(|_| error("acl_database_unavailable",503))?))).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn platform_calls_use_the_signed_groups_without_the_directory() {
        let group = "00000000-0000-4000-8000-000000000001";
        let other = "00000000-0000-4000-8000-000000000002";
        let identity = |platform: bool, groups: Option<Vec<String>>| Identity {
            user_id: "alice".into(),
            admin: false,
            context: crate::InvocationContext {
                installation_id: "00000000-0000-4000-8000-0000000000aa".into(),
                release_id: "00000000-0000-4000-8000-0000000000bb".into(),
                environment: Some("prod".into()),
                callers: vec![],
                effective_permissions: vec![],
                platform,
                groups,
            },
            delegation_token: None,
        };
        let signed = Some(vec![group.to_owned(), other.to_owned()]);
        let all = actor(&identity(true, signed.clone()), None).await.unwrap();
        assert_eq!(all.groups, vec![group.to_owned(), other.to_owned()]);
        let narrowed = actor(&identity(true, signed), Some(&[group.to_owned()]))
            .await
            .unwrap();
        assert_eq!(narrowed.groups, vec![group.to_owned()]);
        // Not approved to read the directory: no groups, user grants only.
        assert!(
            actor(&identity(true, None), None)
                .await
                .unwrap()
                .groups
                .is_empty()
        );
        let bad = Some(vec!["../x".to_owned()]);
        assert!(actor(&identity(true, bad), None).await.is_err());
    }

    fn acl() -> Acl {
        Acl::new(
            sqlx::postgres::PgPoolOptions::new()
                .connect_lazy("postgres://unused:unused@127.0.0.1/unused")
                .unwrap(),
            vec![
                RoleDefinition {
                    id: "owner".into(),
                    label: "Owner".into(),
                    actions: vec![MANAGE.into(), "read".into(), "write".into()],
                },
                RoleDefinition {
                    id: "reader".into(),
                    label: "Reader".into(),
                    actions: vec!["read".into()],
                },
            ],
        )
        .unwrap()
    }
    fn grant(kind: SubjectKind, id: &str, role: &str) -> Grant {
        Grant {
            subject: SubjectRef {
                kind,
                id: id.into(),
            },
            role: role.into(),
        }
    }
    #[tokio::test]
    async fn permission_predicate_has_no_implicit_admin_or_role_hierarchy() {
        let acl = acl();
        let group = "00000000-0000-4000-8000-000000000001";
        let grants = vec![grant(SubjectKind::Group, group, "reader")];
        let current = Actor {
            user: "caller".into(),
            groups: vec![group.into()],
        };
        assert!(acl.permits(&current, &grants, "read"));
        assert!(!acl.permits(&current, &grants, "write"));
        assert!(!acl.permits(&current, &grants, MANAGE));
        assert!(!acl.permits(
            &Actor {
                user: "caller".into(),
                groups: vec![]
            },
            &grants,
            "read"
        ));
        assert!(!acl.permits(&current, &[], "read"));
        assert!(!acl.permits(
            &current,
            &[grant(SubjectKind::User, "other", "owner")],
            "read"
        ));
    }
    #[tokio::test]
    async fn replacement_protects_creator_and_last_active_manager_and_revalidates_targets() {
        let acl = acl();
        let owner = grant(SubjectKind::User, "owner", "owner");
        let reader = grant(SubjectKind::User, "reader", "reader");
        let stored = Stored {
            owner: "owner".into(),
            revision: 1,
            grants: vec![owner.clone(), reader.clone()],
        };
        let active = BTreeMap::from([(owner.subject.clone(), "Owner".into())]);
        assert_eq!(
            acl.validate_replacement(&stored, &[reader.clone()], &active)
                .unwrap_err()
                .code,
            "acl_owner_protected"
        );
        assert_eq!(
            acl.validate_replacement(&stored, &[owner.clone()], &BTreeMap::new())
                .unwrap_err()
                .code,
            "acl_last_manager"
        );
        assert!(
            acl.validate_replacement(&stored, &[owner.clone(), reader.clone()], &active)
                .is_ok()
        );
        let changed = grant(SubjectKind::User, "reader", "owner");
        assert_eq!(
            acl.validate_replacement(&stored, &[owner.clone(), changed], &active)
                .unwrap_err()
                .code,
            "acl_invalid_subject"
        );
        assert!(acl.validate_replacement(&stored, &[owner], &active).is_ok());
    }
    #[tokio::test]
    async fn grants_are_bounded_canonical_and_use_stable_subject_kinds() {
        let acl = acl();
        assert_eq!(acl.normalize(&[]).unwrap_err().code, "acl_invalid_request");
        let owner = grant(SubjectKind::User, "owner", "owner");
        assert_eq!(
            acl.normalize(&[owner.clone(), owner.clone()])
                .unwrap_err()
                .code,
            "acl_invalid_subject"
        );
        assert!(
            acl.normalize(&[grant(SubjectKind::Group, "not-uuid", "reader")])
                .is_err()
        );
        assert!(
            acl.normalize(&[grant(SubjectKind::User, "user", "unknown")])
                .is_err()
        );
        let reverse = vec![grant(SubjectKind::User, "z", "reader"), owner];
        let normal = acl.normalize(&reverse).unwrap();
        assert_eq!(normal[0].subject.id, "owner");
        assert_eq!(acl.normalize(&normal).unwrap(), normal);
        // Core reports lowercase group IDs; a stored uppercase one would never match.
        let upper = grant(
            SubjectKind::Group,
            "00000000-0000-4000-8000-00000000000A",
            "reader",
        );
        assert_eq!(
            acl.normalize(&[upper]).unwrap()[0].subject.id,
            "00000000-0000-4000-8000-00000000000a"
        );
    }
    fn role(id: &str, actions: &[&str]) -> RoleDefinition {
        RoleDefinition {
            id: id.into(),
            label: id.into(),
            actions: actions.iter().map(|a| (*a).to_owned()).collect(),
        }
    }
    #[tokio::test]
    async fn roles_keep_declared_order_and_only_an_unambiguous_least_role_is_preselected() {
        let stored = Stored {
            owner: "owner".into(),
            revision: 1,
            grants: vec![],
        };
        let snapshot = acl().snapshot(&stored, &BTreeMap::new());
        let ids: Vec<&str> = snapshot.roles.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["owner", "reader"]);
        assert_eq!(snapshot.default_role.as_deref(), Some("reader"));
        // Alphabetical order would have offered `editor`, or `owner` above.
        assert_eq!(
            least_role(&[
                role("owner", &[MANAGE, "read", "write"]),
                role("editor", &["read", "write"]),
                role("viewer", &["read"]),
            ])
            .as_deref(),
            Some("viewer")
        );
        // Neither of two unrelated roles is "less"; nothing is preselected.
        assert_eq!(
            least_role(&[
                role("owner", &[MANAGE, "read"]),
                role("commenter", &["read", "comment"]),
                role("auditor", &["read", "export"]),
            ]),
            None
        );
        assert_eq!(least_role(&[role("owner", &[MANAGE, "read"])]), None);
        assert_eq!(
            least_role(&[
                role("owner", &[MANAGE, "read"]),
                role("a", &["read"]),
                role("b", &["read"]),
            ]),
            None
        );
    }
}

#[cfg(test)]
#[path = "acl_postgres_tests.rs"]
mod postgres_tests;
