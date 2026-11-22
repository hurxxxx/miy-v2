//! People. AI-DO joined its `users` table (full_name, primary org unit) into
//! every response; miy keeps users in the core, so names come from the
//! platform directory (`platform:directory:read`), read per request with the
//! caller's identity and kept for a few minutes in memory. A directory failure
//! never fails a read: the user id stands in for a missing name, as AI-DO's own
//! fallback did. Checks that need a user to exist (adding a space member)
//! answer 503 instead.
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::{Extension, Json};
use miy_app_sdk::{DirectoryKind, DirectoryQuery, Identity};
use serde::Serialize;
use serde_json::{Value, json};

use crate::error::{ApiError, ApiResult};

const CACHE_TTL: Duration = Duration::from_secs(300);
const CACHE_LIMIT: usize = 20_000;
/// The most users `GET /v1/pms/users` pages through (AI-DO returned all).
const LIST_LIMIT: usize = 5_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub name: String,
    pub primary_group_id: Option<String>,
    pub primary_group_name: Option<String>,
}

type CacheKey = (String, String, String);
static CACHE: Mutex<Option<HashMap<CacheKey, (UserInfo, Instant)>>> = Mutex::new(None);

fn cache_key(identity: &Identity, user_id: &str) -> CacheKey {
    (
        identity.context.installation_id.clone(),
        identity.context.environment.clone().unwrap_or_default(),
        user_id.to_string(),
    )
}

fn cached(identity: &Identity, ids: &[String]) -> HashMap<String, UserInfo> {
    let guard = CACHE.lock().unwrap_or_else(|poison| poison.into_inner());
    let Some(cache) = guard.as_ref() else {
        return HashMap::new();
    };
    ids.iter()
        .filter_map(|id| {
            cache
                .get(&cache_key(identity, id))
                .filter(|(_, at)| at.elapsed() < CACHE_TTL)
                .map(|(user, _)| (id.clone(), user.clone()))
        })
        .collect()
}

fn remember(identity: &Identity, users: &[UserInfo]) {
    let mut guard = CACHE.lock().unwrap_or_else(|poison| poison.into_inner());
    let cache = guard.get_or_insert_with(HashMap::new);
    if cache.len() > CACHE_LIMIT {
        cache.retain(|_, (_, at)| at.elapsed() < CACHE_TTL);
        if cache.len() > CACHE_LIMIT {
            cache.clear();
        }
    }
    let now = Instant::now();
    for user in users {
        cache.insert(cache_key(identity, &user.id), (user.clone(), now));
    }
}

fn info(user: miy_app_sdk::DirectoryUser) -> UserInfo {
    UserInfo {
        id: user.id,
        primary_group_id: user.primary_group.as_ref().map(|group| group.id.clone()),
        primary_group_name: user.primary_group.map(|group| group.name),
        name: user.name,
    }
}

/// Active directory users among `ids`. Users the directory does not return
/// (disabled, removed) are absent from the map.
pub async fn lookup(identity: &Identity, ids: &[String]) -> Result<HashMap<String, UserInfo>, ()> {
    let mut wanted: Vec<String> = ids.to_vec();
    wanted.sort();
    wanted.dedup();
    wanted.retain(|id| !id.is_empty() && id.chars().count() <= 256 && !id.contains(','));
    let mut found = cached(identity, &wanted);
    let missing: Vec<String> = wanted
        .into_iter()
        .filter(|id| !found.contains_key(id))
        .collect();
    for chunk in missing.chunks(100) {
        let page = identity
            .directory(&DirectoryQuery {
                kind: DirectoryKind::Users,
                ids: Some(chunk.to_vec()),
                limit: Some(100),
                ..Default::default()
            })
            .await
            .map_err(|_| ())?;
        let users: Vec<UserInfo> = page
            .users
            .into_iter()
            .filter(|user| chunk.contains(&user.id))
            .map(info)
            .collect();
        remember(identity, &users);
        found.extend(users.into_iter().map(|user| (user.id.clone(), user)));
    }
    Ok(found)
}

/// Display names for `ids`; the id stands in when the directory has no name.
pub async fn names(identity: &Identity, ids: &[String]) -> HashMap<String, String> {
    let found = lookup(identity, ids).await.unwrap_or_default();
    ids.iter()
        .map(|id| {
            (
                id.clone(),
                found
                    .get(id)
                    .map(|user| user.name.clone())
                    .unwrap_or_else(|| id.clone()),
            )
        })
        .collect()
}

/// The users that must exist and be active, or an error: 503 when the
/// directory cannot answer, `auth.user_not_found` when one is missing.
pub async fn require_active(
    identity: &Identity,
    ids: &[String],
) -> ApiResult<HashMap<String, UserInfo>> {
    let found = lookup(identity, ids)
        .await
        .map_err(|_| ApiError::directory_unavailable())?;
    if ids.iter().any(|id| !found.contains_key(id)) {
        return Err(ApiError::not_found("auth.user_not_found"));
    }
    Ok(found)
}

/// Every active directory user, following cursors (bounded).
pub async fn list_all(identity: &Identity) -> ApiResult<Vec<UserInfo>> {
    let mut users = Vec::new();
    let mut cursor = None;
    let mut seen = std::collections::HashSet::new();
    loop {
        let page = identity
            .directory(&DirectoryQuery {
                kind: DirectoryKind::Users,
                limit: Some(100),
                cursor: cursor.clone(),
                ..Default::default()
            })
            .await
            .map_err(|_| ApiError::directory_unavailable())?;
        let batch: Vec<UserInfo> = page.users.into_iter().map(info).collect();
        remember(identity, &batch);
        users.extend(batch);
        match page.next_cursor {
            Some(next) if users.len() < LIST_LIMIT && seen.insert(next.clone()) => {
                cursor = Some(next)
            }
            _ => break,
        }
    }
    Ok(users)
}

/// `GET /v1/pms/me` (miy addition): AI-DO's frontend read the signed-in user
/// from its auth context; the frame gets no user from the platform.
pub async fn me(Extension(identity): Extension<Identity>) -> ApiResult<Json<Value>> {
    let found = lookup(&identity, std::slice::from_ref(&identity.user_id))
        .await
        .unwrap_or_default();
    let user = found.get(&identity.user_id);
    Ok(Json(json!({
        "id": identity.user_id,
        "name": user.map(|u| u.name.clone()).unwrap_or_else(|| identity.user_id.clone()),
        "primary_org_unit_id": user.and_then(|u| u.primary_group_id.clone()),
        "primary_org_unit_name": user.and_then(|u| u.primary_group_name.clone()),
    })))
}

/// `GET /v1/pms/users`: AI-DO listed active users with PMS access
/// (`SpaceUserItem`). The directory lists active users of the platform; who
/// may use this app is checked by the platform when they open it. The
/// directory has no e-mail, so `email` is empty.
pub async fn list_users(Extension(identity): Extension<Identity>) -> ApiResult<Json<Value>> {
    let mut users = list_all(&identity).await?;
    users.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    Ok(Json(Value::Array(
        users
            .into_iter()
            .map(|user| {
                json!({
                    "id": user.id,
                    "email": "",
                    "full_name": user.name,
                    "primary_org_unit_id": user.primary_group_id,
                    "primary_org_unit_name": user.primary_group_name,
                })
            })
            .collect(),
    )))
}
