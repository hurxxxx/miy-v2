//! Platform notifications (docs/CORE-PERMISSIONS.md "플랫폼 알림과 플랫폼 → 앱
//! 호출"). An app backend publishes, retracts and marks its caller's
//! notifications with the `miy_call_` context of the user request it is
//! serving and the approved `platform:notifications:write`. The portal of the
//! same environment lists them; counts and lists are filtered by what the Core
//! knows now (current use permission, account and app switches).
use crate::{
    State,
    api_contract::{
        NotificationItem, NotificationMute, NotificationPage, NotificationPublished,
        NotificationSettings,
    },
    auth::{BrowserPurpose, Principal},
    http::{ApiError, ApiResult},
    policy::{self, AppAction, Write},
};
use axum::{
    Json,
    extract::{Path, Query, Request, State as S},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::{PgPool, Row, postgres::PgRow};
use std::{collections::HashSet, time::Duration};
use uuid::Uuid;

pub const PERMISSION: &str = "platform:notifications:write";
/// Notifications older than this are deleted by the worker.
pub const RETENTION_DAYS: i32 = 90;
const MAX_RECIPIENTS: usize = 500;
const MAX_RECIPIENT_GROUPS: usize = 50;
/// People one notification reaches after group members are added.
const MAX_EXPANDED: i64 = 5000;
const MAX_MUTES: usize = 200;
const SWEEP_SECONDS: u64 = 3600;

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Publish {
    /// Accepted once per installation; the same ID returns the first result.
    pub request_id: Uuid,
    /// miy user IDs, up to 500. People who may not use the app in this
    /// environment, disabled accounts and people who turned this kind off are
    /// left out.
    #[serde(default)]
    pub recipients: Vec<String>,
    /// Platform group IDs (up to 50) whose current members also receive it,
    /// with the same filters. Needs the approved `platform:directory:read`.
    #[serde(default)]
    pub recipient_groups: Vec<Uuid>,
    /// For example `message` or `mention`: lowercase, digits, `_.-`, up to 40.
    pub kind: String,
    /// One line per recipient and key: a newer notification replaces the older.
    pub group_key: String,
    /// Short display text, 1-200 characters. No message body preview.
    pub title: String,
    /// App-relative destination path, opened in the app's frame.
    pub path: String,
    pub resource_id: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Retract {
    pub group_key: String,
    /// Only these people's notifications; omitted retracts the group for all.
    pub recipients: Option<Vec<String>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkGroupRead {
    /// Marks the calling person's notification of this group read.
    pub group_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListQuery {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
    /// Select the specific notification version displayed by the portal.
    pub id: Option<Uuid>,
    /// Ask the apps (their declared `notificationCheck`) whether each listed
    /// notification still points at something the person may open. The portal
    /// sets it when the panel opens.
    pub check: Option<bool>,
}

fn kind(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 40
        && value.starts_with(|c: char| c.is_ascii_lowercase())
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b))
}

fn text(value: &str, max: usize) -> bool {
    let count = value.chars().count();
    count > 0 && count <= max && !value.chars().any(char::is_control)
}

/// An app-relative path such as `/conversations/42?message=7`.
fn app_path(value: &str) -> bool {
    value.starts_with('/')
        && !value.starts_with("//")
        && value.len() <= 1024
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
}

fn users(value: &[String]) -> bool {
    let mut seen = HashSet::new();
    !value.is_empty()
        && value.len() <= MAX_RECIPIENTS
        && value
            .iter()
            .all(|user| text(user, 128) && seen.insert(user.as_str()))
}

impl Publish {
    fn validate(&mut self) -> ApiResult<()> {
        self.title = self.title.trim().to_owned();
        let mut groups = HashSet::new();
        let valid = (self.recipients.is_empty() || users(&self.recipients))
            && (!self.recipients.is_empty() || !self.recipient_groups.is_empty())
            && self.recipient_groups.len() <= MAX_RECIPIENT_GROUPS
            && self.recipient_groups.iter().all(|g| groups.insert(*g))
            && kind(&self.kind)
            && text(&self.group_key, 200)
            && text(&self.title, 200)
            && app_path(&self.path)
            && self.resource_id.as_deref().is_none_or(|id| text(id, 200));
        if valid {
            Ok(())
        } else {
            Err(ApiError::bad("invalid_notification"))
        }
    }
}

/// The app backend serving a user request: the installation the caller may
/// use, the release's approved permission and the call chain's current grant.
async fn app_access(
    s: &State,
    p: &Principal,
    id: Uuid,
    extensions: &axum::http::Extensions,
) -> ApiResult<PgRow> {
    let context = extensions
        .get::<crate::app_calls::Context>()
        .ok_or_else(ApiError::forbidden)?;
    let installation = crate::domain::installation(s, p, id).await?;
    crate::app_calls::require(s, p, context, id, PERMISSION).await?;
    crate::permissions::require(&s.pool, id, installation.get("release_id"), PERMISSION).await?;
    crate::delegations::check(
        s,
        p,
        installation.get("app_id"),
        installation.get("environment"),
        crate::delegations::Action::AppWrite,
    )
    .await?;
    Ok(installation)
}

async fn body<T: serde::de::DeserializeOwned>(request: Request) -> ApiResult<T> {
    let bytes = tokio::time::timeout(
        Duration::from_secs(5),
        axum::body::to_bytes(request.into_body(), 64 * 1024),
    )
    .await
    .map_err(|_| ApiError::bad("notification_body_timeout"))?
    .map_err(|_| ApiError::bad("notification_body_too_large"))?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// The decision for the acting person, inside the transaction after its writes.
async fn commit(tx: Write, p: &Principal, installation: &PgRow) -> ApiResult<()> {
    tx.decide(
        p,
        installation.get("app_id"),
        installation.get("environment"),
        AppAction::Use,
    )
    .await?
    .commit()
    .await
}

pub async fn publish(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path(id): Path<Uuid>,
    request: Request,
) -> ApiResult<Json<NotificationPublished>> {
    let installation = app_access(&s, &p, id, request.extensions()).await?;
    let mut input: Publish = body(request).await?;
    input.validate()?;
    // Without groups the request hash stays what it always was.
    let input_hash = if input.recipient_groups.is_empty() {
        crate::contract::hash(&serde_json::to_vec(&(
            &p.id,
            &input.recipients,
            &input.kind,
            &input.group_key,
            &input.title,
            &input.path,
            &input.resource_id,
        ))?)
    } else {
        crate::contract::hash(&serde_json::to_vec(&(
            &p.id,
            &input.recipients,
            &input.recipient_groups,
            &input.kind,
            &input.group_key,
            &input.title,
            &input.path,
            &input.resource_id,
        ))?)
    };
    if !input.recipient_groups.is_empty() {
        crate::permissions::require(
            &s.pool,
            id,
            installation.get("release_id"),
            "platform:directory:read",
        )
        .await?;
    }
    let (app, env): (String, String) =
        (installation.get("app_id"), installation.get("environment"));
    let mut tx = Write::begin(&s.pool).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("notification-request:{id}:{}", input.request_id))
        .execute(&mut *tx)
        .await?;
    if let Some(row) = sqlx::query(
        "SELECT input_hash,accepted FROM notification_requests WHERE installation_id=$1 AND request_id=$2",
    )
    .bind(id)
    .bind(input.request_id)
    .fetch_optional(&mut *tx)
    .await?
    {
        if row.get::<String, _>("input_hash") != input_hash {
            return Err(ApiError::conflict("notification_request_conflict"));
        }
        let accepted = row.get::<i32, _>("accepted");
        commit(tx, &p, &installation).await?;
        return Ok(Json(NotificationPublished {
            request_id: input.request_id,
            accepted: accepted as u32,
        }));
    }
    let eligible = policy::may_use_sql("r.id", "$2", "$3");
    let accepted: Vec<String> = sqlx::query_scalar(&format!(
        "INSERT INTO notifications(id,installation_id,app_id,environment,user_id,kind,group_key,title,path,resource_id,actor) \
         SELECT gen_random_uuid(),$1,$2,$3,r.id,$4,$5,$6,$7,$8,$9 FROM ( \
             SELECT DISTINCT x.id FROM (SELECT unnest($10::text[]) AS id UNION \
             SELECT u.id FROM users u JOIN groups g ON g.id=ANY($11::uuid[]) AND g.active \
             WHERE (g.source='hr' AND u.primary_group_id=g.id) \
                OR (g.source='local' AND EXISTS(SELECT 1 FROM group_members m WHERE m.group_id=g.id AND m.user_id=u.id))) x \
             ORDER BY x.id LIMIT $12) AS r(id) \
         WHERE {eligible} AND NOT EXISTS(SELECT 1 FROM notification_mutes m \
             WHERE m.user_id=r.id AND m.app_id=$2 AND (m.kind=$4 OR m.kind='*')) \
         ON CONFLICT(installation_id,user_id,group_key) DO UPDATE SET id=excluded.id,kind=excluded.kind,title=excluded.title,\
         path=excluded.path,resource_id=excluded.resource_id,actor=excluded.actor,\
         created_at=clock_timestamp(),read_at=NULL RETURNING user_id"
    ))
    .bind(id)
    .bind(&app)
    .bind(&env)
    .bind(&input.kind)
    .bind(&input.group_key)
    .bind(&input.title)
    .bind(&input.path)
    .bind(&input.resource_id)
    .bind(&p.id)
    .bind(&input.recipients)
    .bind(&input.recipient_groups)
    .bind(MAX_EXPANDED)
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO notification_requests(installation_id,request_id,actor,input_hash,accepted) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(id)
    .bind(input.request_id)
    .bind(&p.id)
    .bind(&input_hash)
    .bind(accepted.len() as i32)
    .execute(&mut *tx)
    .await?;
    changed(&mut tx, &accepted, &env).await?;
    commit(tx, &p, &installation).await?;
    Ok(Json(NotificationPublished {
        request_id: input.request_id,
        accepted: accepted.len() as u32,
    }))
}

pub async fn by_request(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path((id, request_id)): Path<(Uuid, Uuid)>,
    request: Request,
) -> ApiResult<Json<NotificationPublished>> {
    app_access(&s, &p, id, request.extensions()).await?;
    let accepted: Option<i32> = sqlx::query_scalar(
        "SELECT accepted FROM notification_requests WHERE installation_id=$1 AND request_id=$2 AND actor=$3",
    )
    .bind(id)
    .bind(request_id)
    .bind(&p.id)
    .fetch_optional(&s.pool)
    .await?;
    let accepted = accepted.ok_or_else(ApiError::not_found)?;
    Ok(Json(NotificationPublished {
        request_id,
        accepted: accepted as u32,
    }))
}

pub async fn retract(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path(id): Path<Uuid>,
    request: Request,
) -> ApiResult<Json<serde_json::Value>> {
    let installation = app_access(&s, &p, id, request.extensions()).await?;
    let input: Retract = body(request).await?;
    if !text(&input.group_key, 200)
        || input
            .recipients
            .as_deref()
            .is_some_and(|value| !users(value))
    {
        return Err(ApiError::bad("invalid_notification"));
    }
    let mut tx = Write::begin(&s.pool).await?;
    let removed: Vec<String> = sqlx::query_scalar(
        "DELETE FROM notifications WHERE installation_id=$1 AND group_key=$2 AND ($3::text[] IS NULL OR user_id=ANY($3)) RETURNING user_id",
    )
    .bind(id)
    .bind(&input.group_key)
    .bind(&input.recipients)
    .fetch_all(&mut *tx)
    .await?;
    changed(
        &mut tx,
        &removed,
        installation.get::<&str, _>("environment"),
    )
    .await?;
    commit(tx, &p, &installation).await?;
    Ok(Json(serde_json::json!({"retracted": removed.len()})))
}

pub async fn mark_group_read(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path(id): Path<Uuid>,
    request: Request,
) -> ApiResult<StatusCode> {
    let installation = app_access(&s, &p, id, request.extensions()).await?;
    let input: MarkGroupRead = body(request).await?;
    if !text(&input.group_key, 200) {
        return Err(ApiError::bad("invalid_notification"));
    }
    let mut tx = Write::begin(&s.pool).await?;
    sqlx::query(
        "UPDATE notifications SET read_at=clock_timestamp() WHERE installation_id=$1 AND group_key=$2 AND user_id=$3 AND read_at IS NULL",
    )
    .bind(id)
    .bind(&input.group_key)
    .bind(&p.id)
    .execute(&mut *tx)
    .await?;
    changed(
        &mut tx,
        std::slice::from_ref(&p.id),
        installation.get::<&str, _>("environment"),
    )
    .await?;
    commit(tx, &p, &installation).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The inbox environment of a portal session: prod on the work portal, dev on
/// the dev portal. Other credentials have no inbox.
fn environment(p: &Principal) -> ApiResult<&'static str> {
    crate::delegations::unrestricted(p)?;
    p.browser_purpose
        .and_then(|purpose| purpose.environment())
        .ok_or_else(ApiError::forbidden)
}

/// Rows the person may still see: their own, in this environment, for apps
/// they may use now.
fn visible() -> String {
    format!(
        "n.user_id=$1 AND n.environment=$2 AND {}",
        policy::may_use_sql("n.user_id", "n.app_id", "n.environment")
    )
}

async fn unread_count(pool: &PgPool, user: &str, env: &str) -> sqlx::Result<i64> {
    sqlx::query_scalar(&format!(
        "SELECT count(*) FROM notifications n WHERE {} AND n.read_at IS NULL",
        visible()
    ))
    .bind(user)
    .bind(env)
    .fetch_one(pool)
    .await
}

fn cursor(created_at: DateTime<Utc>, id: Uuid) -> String {
    format!("{}_{id}", created_at.timestamp_micros())
}

fn parse_cursor(value: &str) -> ApiResult<(DateTime<Utc>, Uuid)> {
    let (micros, id) = value
        .split_once('_')
        .ok_or_else(|| ApiError::bad("invalid_cursor"))?;
    let at = micros
        .parse::<i64>()
        .ok()
        .and_then(DateTime::from_timestamp_micros)
        .ok_or_else(|| ApiError::bad("invalid_cursor"))?;
    Ok((at, id.parse().map_err(|_| ApiError::bad("invalid_cursor"))?))
}

pub async fn list(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<NotificationPage>> {
    let env = environment(&p)?;
    let limit = query.limit.unwrap_or(30).clamp(1, 100);
    let after = query.cursor.as_deref().map(parse_cursor).transpose()?;
    let rows = sqlx::query(&format!(
        "SELECT n.id,n.installation_id,n.app_id,a.name AS app_name,n.kind,n.group_key,n.title,n.path,\
         n.resource_id,n.created_at,n.read_at FROM notifications n JOIN apps a ON a.id=n.app_id \
         WHERE {} AND ($3::timestamptz IS NULL OR (n.created_at,n.id)<($3,$4)) \
         AND ($6::uuid IS NULL OR n.id=$6) \
         ORDER BY n.created_at DESC,n.id DESC LIMIT $5",
        visible()
    ))
    .bind(&p.id)
    .bind(env)
    .bind(after.map(|(at, _)| at))
    .bind(after.map(|(_, id)| id))
    .bind(limit + 1)
    .bind(query.id)
    .fetch_all(&s.pool)
    .await?;
    let mut more = rows.len() as i64 > limit;
    let mut items: Vec<NotificationItem> = rows
        .iter()
        .take(limit as usize)
        .map(|row| NotificationItem {
            id: row.get("id"),
            installation_id: row.get("installation_id"),
            app_id: row.get("app_id"),
            app_name: row.get("app_name"),
            kind: row.get("kind"),
            group_key: row.get("group_key"),
            title: row.get("title"),
            path: row.get("path"),
            resource_id: row.get("resource_id"),
            created_at: row.get("created_at"),
            read_at: row.get("read_at"),
        })
        .collect();
    if query.check == Some(true) {
        let mut installations = HashSet::new();
        if let Some(end) = items.iter().position(|item| {
            installations.insert(item.installation_id)
                && installations.len() > MAX_RECHECKED_INSTALLATIONS
        }) {
            items.truncate(end);
            more = true;
        }
    }
    let next_cursor = more
        .then(|| items.last().map(|item| cursor(item.created_at, item.id)))
        .flatten();
    let (items, hidden_unread) = if query.check == Some(true) {
        recheck(&s, &p, items).await?
    } else {
        (items, 0)
    };
    let unread = (unread_count(&s.pool, &p.id, env).await? - hidden_unread).max(0);
    Ok(Json(NotificationPage {
        unread: unread as u64,
        items,
        next_cursor,
    }))
}

/// Installations whose answers one panel opening waits for.
const MAX_RECHECKED_INSTALLATIONS: usize = 8;

/// Keep what the apps still allow. A definite answer removes the person's
/// notifications the app no longer allows (they lost access or the source is
/// gone); no answer only hides them from this response. Apps without a
/// declared check keep their notifications. Returns the kept items and how many
/// hidden ones were unread.
async fn recheck(
    s: &State,
    p: &Principal,
    items: Vec<NotificationItem>,
) -> ApiResult<(Vec<NotificationItem>, i64)> {
    let mut installations: Vec<Uuid> = Vec::new();
    for item in &items {
        if !installations.contains(&item.installation_id) {
            installations.push(item.installation_id);
        }
    }
    let checked = futures_util::future::join_all(
        installations
            .iter()
            .map(|installation| {
                let body = serde_json::json!({
                    "items": items
                        .iter()
                        .filter(|item| item.installation_id == *installation)
                        .map(|item| serde_json::json!({"groupKey": item.group_key, "resourceId": item.resource_id}))
                        .collect::<Vec<_>>()
                });
                async move {
                    let answer = crate::platform_calls::call(
                        s,
                        p,
                        *installation,
                        crate::platform_calls::Endpoint::NotificationCheck,
                        &body,
                    )
                    .await;
                    (*installation, answer)
                }
            }),
    )
    .await;
    let mut removed = Vec::new();
    let mut unanswered: HashSet<Uuid> = HashSet::new();
    let mut allowed: std::collections::HashMap<Uuid, HashSet<String>> =
        std::collections::HashMap::new();
    for (installation, answer) in checked {
        match answer {
            Ok(value) => {
                match serde_json::from_value::<HashSet<String>>(value["allowed"].clone()) {
                    Ok(groups) => {
                        allowed.insert(installation, groups);
                    }
                    Err(_) => {
                        unanswered.insert(installation);
                    }
                }
            }
            Err(crate::platform_calls::Failure::NotDeclared) => {}
            Err(_) => {
                unanswered.insert(installation);
            }
        }
    }
    let mut kept = Vec::with_capacity(items.len());
    let mut hidden_unread = 0;
    for item in items {
        let denied = allowed
            .get(&item.installation_id)
            .is_some_and(|groups| !groups.contains(&item.group_key));
        if denied {
            removed.push(item.id);
        }
        if denied || unanswered.contains(&item.installation_id) {
            if item.read_at.is_none() && !denied {
                hidden_unread += 1;
            }
            continue;
        }
        kept.push(item);
    }
    if !removed.is_empty() {
        let mut tx = Write::begin(&s.pool).await?;
        let deleted =
            sqlx::query("DELETE FROM notifications WHERE user_id=$1 AND id=ANY($2::uuid[])")
                .bind(&p.id)
                .bind(&removed)
                .execute(&mut *tx)
                .await?;
        if deleted.rows_affected() > 0 {
            changed(&mut tx, std::slice::from_ref(&p.id), environment(p)?).await?;
        }
        tx.decide_platform(p, policy::PlatformAction::Personal)
            .await?
            .commit()
            .await?;
    }
    Ok((kept, hidden_unread))
}

pub async fn unread(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
) -> ApiResult<Json<serde_json::Value>> {
    let env = environment(&p)?;
    Ok(Json(
        serde_json::json!({"unread": unread_count(&s.pool, &p.id, env).await?}),
    ))
}

pub async fn mark_read(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let env = environment(&p)?;
    let mut tx = Write::begin(&s.pool).await?;
    let updated = sqlx::query(&format!(
        "UPDATE notifications n SET read_at=COALESCE(n.read_at,clock_timestamp()) WHERE n.id=$3 AND {}",
        visible()
    ))
    .bind(&p.id)
    .bind(env)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    changed(&mut tx, std::slice::from_ref(&p.id), env).await?;
    tx.decide_platform(&p, policy::PlatformAction::Personal)
        .await?
        .commit()
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn mark_all_read(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
) -> ApiResult<StatusCode> {
    let env = environment(&p)?;
    let mut tx = Write::begin(&s.pool).await?;
    sqlx::query(&format!(
        "UPDATE notifications n SET read_at=clock_timestamp() WHERE n.read_at IS NULL AND {}",
        visible()
    ))
    .bind(&p.id)
    .bind(env)
    .execute(&mut *tx)
    .await?;
    changed(&mut tx, std::slice::from_ref(&p.id), env).await?;
    tx.decide_platform(&p, policy::PlatformAction::Personal)
        .await?
        .commit()
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn settings(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
) -> ApiResult<Json<NotificationSettings>> {
    crate::delegations::unrestricted(&p)?;
    let rows = sqlx::query(
        "SELECT app_id,kind FROM notification_mutes WHERE user_id=$1 ORDER BY app_id,kind",
    )
    .bind(&p.id)
    .fetch_all(&s.pool)
    .await?;
    Ok(Json(NotificationSettings {
        mutes: rows
            .iter()
            .map(|row| NotificationMute {
                app_id: row.get("app_id"),
                kind: row.get("kind"),
            })
            .collect(),
    }))
}

pub async fn set_settings(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    Json(input): Json<NotificationSettings>,
) -> ApiResult<Json<NotificationSettings>> {
    crate::delegations::unrestricted(&p)?;
    let mut seen = HashSet::new();
    if input.mutes.len() > MAX_MUTES
        || !input.mutes.iter().all(|mute| {
            crate::contract::valid_id(&mute.app_id)
                && (mute.kind == "*" || kind(&mute.kind))
                && seen.insert((mute.app_id.as_str(), mute.kind.as_str()))
        })
    {
        return Err(ApiError::bad("invalid_notification_settings"));
    }
    let (apps, kinds): (Vec<&str>, Vec<&str>) = input
        .mutes
        .iter()
        .map(|mute| (mute.app_id.as_str(), mute.kind.as_str()))
        .unzip();
    let mut tx = Write::begin(&s.pool).await?;
    sqlx::query("DELETE FROM notification_mutes WHERE user_id=$1")
        .bind(&p.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO notification_mutes(user_id,app_id,kind) SELECT $1,a,k FROM unnest($2::text[],$3::text[]) AS m(a,k)",
    )
    .bind(&p.id)
    .bind(&apps)
    .bind(&kinds)
    .execute(&mut *tx)
    .await?;
    tx.decide_platform(&p, policy::PlatformAction::Personal)
        .await?
        .commit()
        .await?;
    Ok(Json(input))
}

/// Tell live portal channels (any Core process) that these people's inbox in
/// `env` changed; PostgreSQL delivers it when the transaction commits.
async fn changed(tx: &mut sqlx::PgConnection, users: &[String], env: &str) -> sqlx::Result<()> {
    if users.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "SELECT pg_notify('miy_notifications', u || ' ' || $2) FROM unnest($1::text[]) AS u",
    )
    .bind(users)
    .bind(env)
    .execute(tx)
    .await?;
    Ok(())
}

type Change = (String, String);
static HUB: std::sync::OnceLock<tokio::sync::broadcast::Sender<Change>> =
    std::sync::OnceLock::new();

/// Inbox changes of every person, from one PostgreSQL listener started on
/// first use. It reconnects by itself; a missed change only delays a badge.
fn hub(pool: &PgPool) -> tokio::sync::broadcast::Sender<Change> {
    HUB.get_or_init(|| {
        let (sender, _) = tokio::sync::broadcast::channel(1024);
        let (forward, pool) = (sender.clone(), pool.clone());
        tokio::spawn(async move {
            loop {
                if let Ok(mut listener) = sqlx::postgres::PgListener::connect_with(&pool).await
                    && listener.listen("miy_notifications").await.is_ok()
                {
                    while let Ok(notification) = listener.recv().await {
                        if let Some((user, env)) = notification.payload().split_once(' ') {
                            let _ = forward.send((user.to_owned(), env.to_owned()));
                        }
                    }
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
        sender
    })
    .clone()
}

async fn live_browser_session(
    pool: &PgPool,
    session: &str,
    p: &Principal,
    purpose: BrowserPurpose,
) -> bool {
    sqlx::query_scalar(concat!(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id=$2 AND enabled) AND CASE WHEN $4 THEN ",
        "EXISTS(SELECT 1 FROM development_sessions WHERE id_hash=$1 AND user_id=$2 ",
        "AND browser_purpose=$3 AND expires_at>now()) ELSE ",
        "EXISTS(SELECT 1 FROM sessions s WHERE id_hash=$1 AND user_id=$2 ",
        "AND browser_purpose=$3 AND created_at>now()-interval '8 hours' ",
        "AND NOT EXISTS(SELECT 1 FROM password_resets r WHERE r.user_id=s.user_id) AND ",
        crate::user_switch::issuer_current_sql!(),
        ") END"
    ))
    .bind(session)
    .bind(&p.id)
    .bind(purpose.as_str())
    .bind(p.development)
    .fetch_one(pool)
    .await
    .unwrap_or(false)
}

/// The portal's live channel (`GET /api/v1/live`, WebSocket): the unread count
/// at once and again whenever this person's inbox in this environment changes.
/// Closed when the browser session ends or the account is disabled.
pub async fn live(
    S(s): S<State>,
    axum::Extension(p): axum::Extension<Principal>,
    headers: axum::http::HeaderMap,
    upgrade: axum::extract::ws::WebSocketUpgrade,
) -> ApiResult<axum::response::Response> {
    let env = environment(&p)?.to_owned();
    let purpose = p.browser_purpose.ok_or_else(ApiError::forbidden)?;
    let session = s
        .auth
        .session_hash(&headers, purpose)
        .ok_or_else(ApiError::unauthorized)?;
    let mut events = hub(&s.pool).subscribe();
    Ok(upgrade
        .max_message_size(4 * 1024)
        .on_upgrade(move |mut socket| async move {
            use axum::extract::ws::Message;
            let current = || live_browser_session(&s.pool, &session, &p, purpose);
            let count_message = |count: i64| {
                Message::Text(
                    serde_json::json!({"type":"notifications.unread","unread":count})
                        .to_string()
                        .into(),
                )
            };
            if !current().await {
                crate::live::close_with(&mut socket, crate::live::close::SESSION_ENDED, "session_ended").await;
                return;
            }
            let Ok(count) = unread_count(&s.pool, &p.id, &env).await else {
                return;
            };
            if !crate::live::send_with_deadline(&mut socket, count_message(count)).await {
                return;
            }
            let mut recheck = tokio::time::interval(crate::live::RECHECK);
            recheck.tick().await;
            loop {
                tokio::select! {
                    event = events.recv() => {
                        let relevant = match event {
                            Ok((user, event_env)) => user == p.id && event_env == env,
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => true,
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                        };
                        if relevant {
                            if !current().await {
                                crate::live::close_with(&mut socket, crate::live::close::SESSION_ENDED, "session_ended").await;
                                return;
                            }
                            let Ok(count) = unread_count(&s.pool, &p.id, &env).await else { continue };
                            if !crate::live::send_with_deadline(&mut socket, count_message(count)).await {
                                return;
                            }
                        }
                    }
                    message = socket.recv() => match message {
                        Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                        _ => {}
                    },
                    _ = recheck.tick() => {
                        if !current().await {
                            crate::live::close_with(&mut socket, crate::live::close::SESSION_ENDED, "session_ended").await;
                            return;
                        }
                        if !crate::live::send_with_deadline(&mut socket, Message::Ping(Vec::new().into())).await {
                            return;
                        }
                    }
                }
            }
        }))
}

/// Delete notifications and request records past the retention period.
pub(crate) async fn sweep(pool: &PgPool) -> sqlx::Result<u64> {
    let notifications = sqlx::query(
        "DELETE FROM notifications WHERE created_at<clock_timestamp()-make_interval(days=>$1)",
    )
    .bind(RETENTION_DAYS)
    .execute(pool)
    .await?;
    sqlx::query(
        "DELETE FROM notification_requests WHERE created_at<clock_timestamp()-make_interval(days=>$1)",
    )
    .bind(RETENTION_DAYS)
    .execute(pool)
    .await?;
    Ok(notifications.rows_affected())
}

pub(crate) struct Sweeper(tokio::task::JoinHandle<()>);
impl Drop for Sweeper {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The worker deletes expired notifications hourly.
pub(crate) fn start(pool: PgPool) -> Sweeper {
    Sweeper(tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(SWEEP_SECONDS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if sweep(&pool).await.is_err() {
                tracing::error!(event = "notification_sweep_failed");
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_fields_are_bounded() {
        assert!(
            kind("message") && kind("dm.mention") && !kind("Message") && !kind("1x") && !kind("")
        );
        assert!(app_path("/conversations/42?message=7"));
        assert!(
            !app_path("//evil.example")
                && !app_path("https://x")
                && !app_path("/a\\b")
                && !app_path("/a\nb")
        );
        assert!(users(&["a".into(), "b".into()]));
        assert!(!users(&[]) && !users(&["a".into(), "a".into()]));
        assert!(
            text("보낸 사람 · 새 메시지 3개", 200)
                && !text("", 200)
                && !text(&"가".repeat(201), 200)
        );
    }

    #[test]
    fn cursors_round_trip() {
        let at = DateTime::from_timestamp_micros(1_790_000_000_123_456).unwrap();
        let id = Uuid::new_v4();
        assert_eq!(parse_cursor(&cursor(at, id)).unwrap(), (at, id));
        assert!(parse_cursor("x").is_err());
    }
}

#[cfg(test)]
mod publish_shape_tests {
    #[test]
    fn regression_body_parses_and_validates() {
        let mut input: super::Publish = serde_json::from_value(serde_json::json!({
            "requestId": uuid::Uuid::new_v4(),
            "recipients": ["permission-audit-0b8c2f5e-2a43-4bd5-8a60-0f4d2c2e9d11", "no-such-user"],
            "kind": "message", "groupKey": "conversation:1", "title": "앨리스 · 새 메시지 2개",
            "path": "/conversations/1", "resourceId": "1"
        }))
        .unwrap();
        input.validate().unwrap();
    }
}
