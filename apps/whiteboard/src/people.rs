//! Names and user lists from the platform directory (`platform:directory:read`).
//!
//! AI-DO joined its `users` table for display names and author search. In miy
//! the people live in the core: names are read per request with stable IDs
//! (at most 100 per call), and a failed directory call only leaves names out
//! (AI-DO's own fallback for a missing name), never fails the request.
//! AI-DO's `users` lookups for sharing (`shareable-users`, the share target
//! check) read the directory too.
use miy_app_sdk::{DirectoryKind, DirectoryQuery, Identity};
use std::{
    collections::{BTreeSet, HashMap},
    time::Duration,
};

const CALL_TIMEOUT: Duration = Duration::from_secs(5);

/// Display names by user ID; unknown or unavailable people are absent.
pub async fn display_names<I>(identity: &Identity, ids: I) -> HashMap<String, String>
where
    I: IntoIterator<Item = String>,
{
    let ids: BTreeSet<String> = ids.into_iter().filter(|id| !id.is_empty()).collect();
    let ids: Vec<String> = ids.into_iter().collect();
    let mut names = HashMap::new();
    if identity.context.platform {
        // Platform calls carry no token for further platform calls.
        return names;
    }
    for chunk in ids.chunks(100) {
        let query = DirectoryQuery {
            kind: DirectoryKind::Users,
            ids: Some(chunk.to_vec()),
            limit: Some(100),
            ..Default::default()
        };
        match tokio::time::timeout(CALL_TIMEOUT, identity.directory(&query)).await {
            Ok(Ok(page)) => {
                for user in page.users {
                    if chunk.contains(&user.id) && !user.name.trim().is_empty() {
                        names.insert(user.id, user.name);
                    }
                }
            }
            _ => break,
        }
    }
    names
}

/// One person by stable ID when the directory still lists them (AI-DO
/// `select(User).where(User.id == user_id, User.status == "active")`).
/// `Err` when the directory could not be read.
pub async fn find_user(identity: &Identity, user_id: &str) -> Result<Option<(String, String)>, ()> {
    let query = DirectoryQuery {
        kind: DirectoryKind::Users,
        ids: Some(vec![user_id.to_string()]),
        limit: Some(1),
        ..Default::default()
    };
    match tokio::time::timeout(CALL_TIMEOUT, identity.directory(&query)).await {
        Ok(Ok(page)) => Ok(page
            .users
            .into_iter()
            .find(|user| user.id == user_id)
            .map(|user| (user.id, user.name))),
        _ => Err(()),
    }
}

/// A person offered for sharing: AI-DO `ShareableUserItem` (`id`, `email`,
/// `full_name`). The directory exposes no e-mail, so `primary_group` carries
/// the person's group name for the second line instead.
pub struct DirectoryPerson {
    pub id: String,
    pub name: String,
    pub primary_group: Option<String>,
}

/// AI-DO `list_whiteboard_shareable_users`: active people whose name matches
/// `q` (all people when empty), name order, at most `limit`. One directory
/// page; `Err` when the directory could not be read.
pub async fn search_people(
    identity: &Identity,
    q: &str,
    limit: u32,
) -> Result<Vec<DirectoryPerson>, ()> {
    let query = DirectoryQuery {
        kind: DirectoryKind::Users,
        q: (!q.is_empty()).then(|| q.to_string()),
        limit: Some(limit.clamp(1, 100)),
        ..Default::default()
    };
    match tokio::time::timeout(CALL_TIMEOUT, identity.directory(&query)).await {
        Ok(Ok(page)) => Ok(page
            .users
            .into_iter()
            .map(|user| DirectoryPerson {
                id: user.id,
                name: user.name,
                primary_group: user.primary_group.map(|group| group.name),
            })
            .collect()),
        _ => Err(()),
    }
}
