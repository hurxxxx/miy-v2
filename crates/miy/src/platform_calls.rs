//! Platform → app calls (docs/CORE-PERMISSIONS.md "플랫폼 → 앱 호출"). For the
//! signed-in person's portal request, the Core calls a read-only endpoint the
//! app declared in its manifest (`platformEndpoints`). The app receives that
//! person's signed identity with `admin=false` and `platform=true` and no call
//! token, so it cannot act further on the person's behalf. Calls have their own
//! small concurrency budget, a short timeout and a size limit; a failure only
//! affects the item that needed the answer.
use crate::{
    State,
    auth::Principal,
    identity::IdentityContext,
    policy::{self, AppAction},
};
use serde_json::Value;
use sqlx::Row;
use std::time::Duration;
use tokio::sync::Semaphore;
use uuid::Uuid;

/// Shared by every platform call of this process, apart from app API slots.
static BUDGET: Semaphore = Semaphore::const_new(16);
const TIMEOUT: Duration = Duration::from_secs(3);
const WAIT_FOR_SLOT: Duration = Duration::from_secs(1);
const MAX_RESPONSE: usize = 64 * 1024;

/// Which declared endpoint to call.
#[derive(Clone, Copy, Debug)]
pub enum Endpoint {
    NotificationCheck,
    HomeCard,
    Search,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    /// The release declares no such endpoint.
    NotDeclared,
    /// The person may not use the installation (or it is gone).
    Denied,
    /// Not running, busy, too slow, too large or an error answer.
    Unavailable,
}

/// Call `endpoint` of installation `id` for `p` with a JSON body.
pub async fn call(
    state: &State,
    p: &Principal,
    id: Uuid,
    endpoint: Endpoint,
    body: &Value,
) -> Result<Value, Failure> {
    let row = sqlx::query(
        "SELECT i.app_id,i.environment,i.release_id,i.runtime_status,i.desired_state,i.runtime_port,\
         i.runtime_secret,r.manifest FROM installations i JOIN releases r ON r.id=i.release_id WHERE i.id=$1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| Failure::Unavailable)?
    .ok_or(Failure::Denied)?;
    let (app, env): (String, String) = (row.get("app_id"), row.get("environment"));
    policy::require(state, p, &app, &env, AppAction::Use)
        .await
        .map_err(|_| Failure::Denied)?;
    let manifest: Value = row.get("manifest");
    let path = match endpoint {
        Endpoint::NotificationCheck => manifest["platformEndpoints"]["notificationCheck"].as_str(),
        Endpoint::HomeCard => manifest["platformEndpoints"]["homeCard"].as_str(),
        Endpoint::Search => manifest["platformEndpoints"]["search"].as_str(),
    }
    .filter(|path| crate::contract::valid_endpoint_path(path))
    .ok_or(Failure::NotDeclared)?
    .to_owned();
    if row.get::<String, _>("runtime_status") != "running"
        || row.get::<String, _>("desired_state") != "running"
    {
        return Err(Failure::Unavailable);
    }
    let port: i32 = row
        .get::<Option<i32>, _>("runtime_port")
        .ok_or(Failure::Unavailable)?;
    let secret = crate::auth::decrypt(
        row.get::<Option<&str>, _>("runtime_secret")
            .ok_or(Failure::Unavailable)?,
    )
    .map_err(|_| Failure::Unavailable)?;
    let token = crate::identity::sign_identity_context(
        &p.id,
        false,
        id,
        row.get("release_id"),
        &secret,
        &IdentityContext {
            environment: env,
            callers: vec![],
            effective_permissions: vec![],
            delegation_token: None,
            platform: true,
        },
    )
    .map_err(|_| Failure::Unavailable)?;
    let _slot = tokio::time::timeout(WAIT_FOR_SLOT, BUDGET.acquire())
        .await
        .map_err(|_| Failure::Unavailable)?
        .map_err(|_| Failure::Unavailable)?;
    let client = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| Failure::Unavailable)?;
    let mut response = client
        .post(format!("http://127.0.0.1:{port}/api{path}"))
        .header("x-miy-identity", token)
        .json(body)
        .send()
        .await
        .map_err(|_| Failure::Unavailable)?;
    if !response.status().is_success() {
        return Err(Failure::Unavailable);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Failure::Unavailable)? {
        if bytes.len() + chunk.len() > MAX_RESPONSE {
            return Err(Failure::Unavailable);
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| Failure::Unavailable)
}
