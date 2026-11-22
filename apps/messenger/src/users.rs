//! People. AI-DO joined its `users` table (and org units) for names and
//! active status and searched it for recipients (domains/dm/user_directory.py).
//! miy apps read people from the platform directory (`platform:directory:read`).
//!
//! - Write paths that need an *active* person (new conversation, invitations,
//!   block, report) ask the directory each time and fail closed.
//! - Names for display come from `dm_user_directory_cache`, refreshed from the
//!   directory when older than [`REFRESH_AFTER`]. A person the directory no
//!   longer lists keeps the last known name (AI-DO kept the user row). The
//!   cache is display data, never an access input.
use crate::{
    error::{ApiError, ApiResult},
    schemas::DmUserItem,
    time::now_utc,
};
use chrono::Duration;
use miy_app_sdk::{DirectoryKind, DirectoryQuery, DirectoryUser, Identity};
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};

const REFRESH_AFTER_SECONDS: i64 = 300;
const DIRECTORY_IDS_PER_CALL: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub primary_group_name: Option<String>,
    pub active: bool,
}

impl Profile {
    pub fn unknown(id: &str) -> Self {
        Self {
            id: id.to_string(),
            name: id.to_string(),
            primary_group_name: None,
            active: false,
        }
    }

    pub fn item(&self) -> DmUserItem {
        DmUserItem {
            id: self.id.clone(),
            email: String::new(),
            full_name: self.name.clone(),
            display_name: None,
            job_title: None,
            primary_org_unit_name: self.primary_group_name.clone(),
        }
    }
}

/// Names resolved for one response.
#[derive(Debug, Default, Clone)]
pub struct Names(HashMap<String, Profile>);

impl Names {
    pub fn get(&self, id: &str) -> Profile {
        self.0
            .get(id)
            .cloned()
            .unwrap_or_else(|| Profile::unknown(id))
    }

    pub fn name(&self, id: &str) -> String {
        self.0
            .get(id)
            .map(|profile| profile.name.clone())
            .unwrap_or_else(|| id.to_string())
    }

    pub fn item(&self, id: &str) -> DmUserItem {
        self.get(id).item()
    }

    #[cfg(test)]
    pub fn from_profiles(profiles: Vec<Profile>) -> Self {
        Self(profiles.into_iter().map(|p| (p.id.clone(), p)).collect())
    }
}

fn from_directory(user: &DirectoryUser) -> Profile {
    Profile {
        id: user.id.clone(),
        name: user.name.clone(),
        primary_group_name: user.primary_group.as_ref().map(|group| group.name.clone()),
        active: true,
    }
}

async fn remember(pool: &PgPool, profiles: &[Profile]) {
    if profiles.is_empty() {
        return;
    }
    let ids: Vec<&str> = profiles.iter().map(|p| p.id.as_str()).collect();
    let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
    let groups: Vec<Option<&str>> = profiles
        .iter()
        .map(|p| p.primary_group_name.as_deref())
        .collect();
    let active: Vec<bool> = profiles.iter().map(|p| p.active).collect();
    let result = sqlx::query(
        "INSERT INTO dm_user_directory_cache (user_id, name, primary_group_name, active, refreshed_at) \
         SELECT u.id, u.name, u.grp, u.active, $5 FROM unnest($1::text[], $2::text[], $3::text[], $4::bool[]) \
           AS u(id, name, grp, active) \
         ON CONFLICT (user_id) DO UPDATE SET name = EXCLUDED.name, \
           primary_group_name = EXCLUDED.primary_group_name, active = EXCLUDED.active, \
           refreshed_at = EXCLUDED.refreshed_at",
    )
    .bind(&ids)
    .bind(&names)
    .bind(&groups)
    .bind(&active)
    .bind(now_utc())
    .execute(pool)
    .await;
    if result.is_err() {
        eprintln!("directory_cache_write_failed");
    }
}

async fn mark_inactive(pool: &PgPool, ids: &[String]) {
    if ids.is_empty() {
        return;
    }
    let _ = sqlx::query(
        "UPDATE dm_user_directory_cache SET active = false, refreshed_at = $2 WHERE user_id = ANY($1)",
    )
    .bind(ids)
    .bind(now_utc())
    .execute(pool)
    .await;
}

/// Ask the directory about `ids` (in pages of 100). People it does not list
/// are disabled or unknown.
async fn lookup(
    identity: &Identity,
    ids: &[String],
) -> Result<Vec<Profile>, miy_app_sdk::CallError> {
    let mut found = Vec::new();
    for chunk in ids.chunks(DIRECTORY_IDS_PER_CALL) {
        let page = identity
            .directory(&DirectoryQuery {
                kind: DirectoryKind::Users,
                ids: Some(chunk.to_vec()),
                limit: Some(DIRECTORY_IDS_PER_CALL as u32),
                ..Default::default()
            })
            .await?;
        found.extend(page.users.iter().map(from_directory));
    }
    Ok(found)
}

/// Display names for `ids`: cached rows, refreshed when stale. A directory
/// failure keeps the stale names; unknown people show their ID (AI-DO's
/// `message_sender_name` fallback).
pub async fn names(
    pool: &PgPool,
    identity: &Identity,
    ids: impl IntoIterator<Item = String>,
) -> Names {
    let ids: Vec<String> = ids
        .into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .filter(|id| !id.is_empty())
        .collect();
    if ids.is_empty() {
        return Names::default();
    }
    let rows: Vec<(String, String, Option<String>, bool, chrono::NaiveDateTime)> = sqlx::query_as(
        "SELECT user_id, name, primary_group_name, active, refreshed_at FROM dm_user_directory_cache \
         WHERE user_id = ANY($1)",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let fresh_after = now_utc() - Duration::seconds(REFRESH_AFTER_SECONDS);
    let mut profiles = HashMap::new();
    let mut stale = Vec::new();
    for (id, name, group, active, refreshed_at) in rows {
        if refreshed_at < fresh_after {
            stale.push(id.clone());
        }
        profiles.insert(
            id.clone(),
            Profile {
                id,
                name,
                primary_group_name: group,
                active,
            },
        );
    }
    let missing: Vec<String> = ids
        .iter()
        .filter(|id| !profiles.contains_key(*id))
        .cloned()
        .collect();
    let wanted: Vec<String> = stale.into_iter().chain(missing).collect();
    // The platform's read-only recheck call carries no token for further
    // platform calls; it never needs names.
    if !wanted.is_empty() && !identity.context.platform {
        match lookup(identity, &wanted).await {
            Ok(found) => {
                let found_ids: HashSet<String> = found.iter().map(|p| p.id.clone()).collect();
                remember(pool, &found).await;
                let gone: Vec<String> = wanted
                    .iter()
                    .filter(|id| !found_ids.contains(*id) && profiles.contains_key(*id))
                    .cloned()
                    .collect();
                mark_inactive(pool, &gone).await;
                for id in gone {
                    if let Some(profile) = profiles.get_mut(&id) {
                        profile.active = false;
                    }
                }
                for profile in found {
                    profiles.insert(profile.id.clone(), profile);
                }
            }
            Err(error) => eprintln!("directory_names_failed: {}", error.code),
        }
    }
    Names(profiles)
}

fn directory_unavailable(error: miy_app_sdk::CallError) -> ApiError {
    eprintln!("directory_lookup_failed: {}", error.code);
    ApiError::unavailable(
        "dm.directory_unavailable",
        "The people directory is not available. Try again.",
    )
}

/// Active people among `ids`, asked of the directory now (fail closed).
/// `conversation_lifecycle.active_users_by_id`.
pub async fn active_users(
    pool: &PgPool,
    identity: &Identity,
    ids: &[String],
) -> ApiResult<HashMap<String, Profile>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let found = lookup(identity, ids).await.map_err(directory_unavailable)?;
    remember(pool, &found).await;
    let wanted: HashSet<&String> = ids.iter().collect();
    Ok(found
        .into_iter()
        .filter(|profile| wanted.contains(&profile.id))
        .map(|profile| (profile.id.clone(), profile))
        .collect())
}

/// `GET /v1/dm/users`: `user_directory.search_users`. AI-DO searched name,
/// display name, e-mail, job title and org unit; the platform directory
/// searches the name and primary group.
pub async fn search(
    pool: &PgPool,
    identity: &Identity,
    q: &str,
    limit: u32,
    include_current: bool,
) -> ApiResult<Vec<DmUserItem>> {
    let ask = if include_current {
        limit
    } else {
        (limit + 1).min(100)
    };
    let query = q.trim();
    let page = identity
        .directory(&DirectoryQuery {
            kind: DirectoryKind::Users,
            q: (!query.is_empty()).then(|| query.chars().take(120).collect()),
            limit: Some(ask),
            ..Default::default()
        })
        .await
        .map_err(directory_unavailable)?;
    let profiles: Vec<Profile> = page.users.iter().map(from_directory).collect();
    remember(pool, &profiles).await;
    Ok(profiles
        .into_iter()
        .filter(|profile| include_current || profile.id != identity.user_id)
        .take(limit as usize)
        .map(|profile| profile.item())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_people_show_their_id() {
        let names = Names::default();
        assert_eq!(names.name("user-1"), "user-1");
        let item = names.item("user-1");
        assert_eq!(item.full_name, "user-1");
        assert_eq!(item.email, "");
    }

    #[test]
    fn directory_users_map_to_ai_do_user_items() {
        let profile = Profile {
            id: "u1".into(),
            name: "김하나".into(),
            primary_group_name: Some("개발팀".into()),
            active: true,
        };
        let item = profile.item();
        assert_eq!(item.full_name, "김하나");
        assert_eq!(item.primary_org_unit_name.as_deref(), Some("개발팀"));
        assert_eq!(item.display_name, None);
    }
}
