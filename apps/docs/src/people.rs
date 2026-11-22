//! Names and user lists from the platform directory (`platform:directory:read`).
//!
//! AI-DO joined its `users` table for display names and author search. In miy
//! the people live in the core: names are read per request with stable IDs
//! (at most 100 per call), and a failed directory call only leaves names out
//! (AI-DO's own fallback for a missing name), never fails the request.
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

/// One directory page of people matching `q` (all people when empty), as
/// `(id, name)`. Docs' shareable-user search (AI-DO matched name or e-mail
/// with ILIKE, at most 30 rows).
pub async fn search_users(identity: &Identity, q: &str, limit: u32) -> Vec<(String, String)> {
    let query = DirectoryQuery {
        kind: DirectoryKind::Users,
        q: (!q.is_empty()).then(|| q.to_string()),
        limit: Some(limit.clamp(1, 100)),
        ..Default::default()
    };
    match tokio::time::timeout(CALL_TIMEOUT, identity.directory(&query)).await {
        Ok(Ok(page)) => page
            .users
            .into_iter()
            .map(|user| (user.id, user.name))
            .collect(),
        _ => Vec::new(),
    }
}
