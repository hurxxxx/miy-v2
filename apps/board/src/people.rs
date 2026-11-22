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

/// IDs of people whose name matches `q` (AI-DO matched display or full name
/// with ILIKE). Up to five directory pages.
pub async fn search_user_ids(identity: &Identity, q: &str) -> Vec<String> {
    let mut ids = Vec::new();
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

/// Every person the directory lists (for company-wide notices), at most
/// `max_pages` pages of 100. The platform leaves out recipients who may not
/// use this app.
pub async fn all_user_ids(identity: &Identity, max_pages: usize) -> Vec<String> {
    let mut ids = Vec::new();
    let mut cursor = None;
    for _ in 0..max_pages {
        let query = DirectoryQuery {
            kind: DirectoryKind::Users,
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
