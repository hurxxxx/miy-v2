//! Names and the caller's groups from the platform directory
//! (`platform:directory:read`).
//!
//! AI-DO joined its `users` table for owner names (`display_name or
//! full_name`). In miy the people live in the core: names are read per
//! request with stable IDs (at most 100 per call), and a failed directory call
//! only leaves names out (AI-DO's own fallback for a missing owner), never
//! fails the request.
use miy_app_sdk::{DirectoryKind, DirectoryQuery, Identity};
use std::{
    collections::{BTreeSet, HashMap},
    time::Duration,
};

const CALL_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_MEMBERSHIP_PAGES: usize = 10;

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

/// IDs of people whose name matches `q` (owner search). Up to five pages.
pub async fn search_user_ids(identity: &Identity, q: &str) -> Vec<String> {
    let mut ids = Vec::new();
    if identity.context.platform {
        return ids;
    }
    let mut cursor = None;
    for _ in 0..5 {
        let query = DirectoryQuery {
            kind: DirectoryKind::Users,
            q: Some(q.to_string()),
            limit: Some(100),
            cursor: cursor.clone(),
            ..Default::default()
        };
        match tokio::time::timeout(CALL_TIMEOUT, identity.directory(&query)).await {
            Ok(Ok(page)) => {
                ids.extend(page.users.into_iter().map(|user| user.id));
                match page.next_cursor {
                    Some(next) if Some(&next) != cursor.as_ref() => cursor = Some(next),
                    _ => break,
                }
            }
            _ => break,
        }
    }
    ids
}

/// The caller's current directory groups (lowercase IDs, as grants store
/// them). `None` when the directory cannot answer: callers fail closed.
pub async fn current_groups(identity: &Identity) -> Option<Vec<String>> {
    if identity.context.platform {
        return None;
    }
    let mut groups = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_MEMBERSHIP_PAGES {
        let query = DirectoryQuery {
            kind: DirectoryKind::Memberships,
            limit: Some(100),
            cursor: cursor.clone(),
            ..Default::default()
        };
        let page = tokio::time::timeout(CALL_TIMEOUT, identity.directory(&query))
            .await
            .ok()?
            .ok()?;
        groups.extend(page.groups.into_iter().map(|g| g.id.to_ascii_lowercase()));
        match page.next_cursor {
            None => return Some(groups.into_iter().collect()),
            Some(next) if seen.insert(next.clone()) => cursor = Some(next),
            Some(_) => return None,
        }
    }
    None
}
