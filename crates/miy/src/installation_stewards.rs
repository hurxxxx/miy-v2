//! One-time installation nominations. The app owns its live steward roster.
use crate::{
    State, app_calls, audit,
    auth::{BrowserPurpose, Principal},
    http::{ApiError, ApiResult},
    policy::{AppAction, Decided, PlatformAction, Write},
};
use axum::{
    Extension, Json,
    extract::{Path, State as S},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

const PERMISSION: &str = "platform:stewards:claim";

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Nomination {
    subject_id: String,
    expected_assignment_id: Option<Uuid>,
    reason: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Claim {
    assignment_id: Uuid,
    /// Complete roster read by the app backend while holding its local write lock.
    stewards: Vec<String>,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Receipt {
    assignment_id: Uuid,
}

fn console(p: &Principal) -> ApiResult<()> {
    if p.browser_purpose != Some(BrowserPurpose::Console) || p.delegation_id.is_some() {
        return Err(ApiError::forbidden());
    }
    Ok(())
}

async fn approval(conn: &mut PgConnection, id: Uuid) -> ApiResult<bool> {
    sqlx::query_scalar(
        "SELECT COALESCE((r.manifest->'requestedPermissions') ? $2 AND ip.permissions ? $2, false)
         FROM installations i JOIN releases r ON r.id=i.release_id
         LEFT JOIN installation_permissions ip ON ip.installation_id=i.id AND ip.release_id=i.release_id
         WHERE i.id=$1")
        .bind(id).bind(PERMISSION).fetch_optional(conn).await?.ok_or_else(ApiError::not_found)
}

pub async fn view(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    console(&p)?;
    crate::domain::admin(&s, &p).await?;
    let mut conn = s.pool.acquire().await?;
    let approved = approval(&mut conn, id).await?;
    let assignment: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id',a.id,'subjectName',u.name,'subjectId',u.id,
          'status',a.status,'reason',a.recovery_reason) FROM installation_steward_assignments a
          JOIN users u ON u.id=a.subject_id WHERE a.installation_id=$1 AND a.superseded_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(Json(json!({"approved":approved,"assignment":assignment})))
}

pub async fn nominate(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    Path(id): Path<Uuid>,
    Json(input): Json<Nomination>,
) -> ApiResult<Json<Value>> {
    console(&p)?;
    let subject = input.subject_id;
    if subject.is_empty() || subject.len() > 256 || subject.chars().any(char::is_control) {
        return Err(ApiError::bad("invalid_user_id"));
    }
    let reason = input.reason.as_deref().map(str::trim);
    if input.expected_assignment_id.is_none() && reason.is_some() {
        return Err(ApiError::bad("steward_initial_reason_unexpected"));
    }
    if reason
        .is_some_and(|r| r.is_empty() || r.chars().count() > 500 || r.chars().any(char::is_control))
        || (input.expected_assignment_id.is_some() && reason.is_none())
    {
        return Err(ApiError::bad("steward_reason_required"));
    }
    let tx = Write::begin(&s.pool).await?;
    let mut tx = tx.decide_platform(&p, PlatformAction::Administer).await?;
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM installations WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(ApiError::not_found)?;
    if !approval(&mut tx, id).await? {
        return Err(crate::permissions::denied());
    }
    let previous = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM installation_steward_assignments
        WHERE installation_id=$1 AND superseded_at IS NULL FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    if previous != input.expected_assignment_id {
        return Err(ApiError::conflict("steward_assignment_changed"));
    }
    if sqlx::query_scalar::<_, bool>("SELECT enabled FROM users WHERE id=$1 FOR SHARE")
        .bind(&subject)
        .fetch_optional(&mut *tx)
        .await?
        != Some(true)
    {
        return Err(ApiError::bad("steward_user_inactive"));
    }
    let next = Uuid::new_v4();
    sqlx::query(
        "UPDATE installation_steward_assignments SET superseded_at=clock_timestamp() WHERE id=$1",
    )
    .bind(input.expected_assignment_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO installation_steward_assignments(id,installation_id,subject_id,assigned_by,recovery_reason,status)
        VALUES ($1,$2,$3,$4,$5,'pending')")
        .bind(next).bind(id).bind(&subject).bind(&p.id).bind(reason).execute(&mut *tx).await?;
    let mut target = format!("installation:{id}:assignment:{next}:user:{subject}");
    let action = if let Some(reason) = reason {
        target.push_str(" reason=");
        target.push_str(reason);
        "steward.recovery_requested"
    } else {
        "steward.nominated"
    };
    audit::record(&mut tx, &p.id, action, &target).await?;
    tx.commit().await?;
    Ok(Json(json!({"id":next})))
}

// Only the deployed app serving the designated user's own direct frame can
// participate. Recheck consent after locking the installation against release or approval changes.
async fn app_transaction(
    mut tx: Write,
    s: &State,
    p: &Principal,
    c: &app_calls::Context,
    id: Uuid,
) -> ApiResult<Decided> {
    let frame = c
        .origin
        .as_ref()
        .filter(|o| o.kind == "frame")
        .ok_or_else(ApiError::forbidden)?;
    if c.installation_id != id
        || !c.callers.is_empty()
        || p.delegation_id.is_some()
        || !c.effective_permissions.iter().any(|v| v == PERMISSION)
    {
        return Err(ApiError::forbidden());
    }
    let row = sqlx::query(
        "SELECT app_id,environment,release_id FROM installations WHERE id=$1 FOR SHARE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(ApiError::not_found)?;
    if row.get::<Uuid, _>("release_id") != c.release_id
        || row.get::<&str, _>("environment") != c.environment
        || !approval(&mut tx, id).await?
    {
        return Err(crate::permissions::denied());
    }
    app_calls::current_origin(&mut tx, s.auth.development_login, p, c).await?;
    let switched: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app_frame_sessions f JOIN sessions s
        ON s.id_hash=f.parent_session_hash WHERE f.id::text=$1 AND s.impersonator_user_id IS NOT NULL)")
        .bind(&frame.digest).fetch_one(&mut *tx).await?;
    if switched {
        return Err(ApiError::forbidden());
    }
    tx.decide(p, row.get("app_id"), &c.environment, AppAction::Use)
        .await
}

pub async fn pending(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    context: Option<Extension<app_calls::Context>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let Extension(c) = context.ok_or_else(ApiError::forbidden)?;
    let mut tx = app_transaction(Write::begin(&s.pool).await?, &s, &p, &c, id).await?;
    let assignment: Option<Uuid> = sqlx::query_scalar("SELECT id FROM installation_steward_assignments WHERE
        installation_id=$1 AND subject_id=$2 AND superseded_at IS NULL AND status IN ('pending','issued')")
        .bind(id).bind(&p.id).fetch_optional(&mut *tx).await?;
    Ok(Json(json!({"assignmentId":assignment})))
}

pub async fn claim(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    context: Option<Extension<app_calls::Context>>,
    Path(id): Path<Uuid>,
    Json(input): Json<Claim>,
) -> ApiResult<Json<Value>> {
    let Extension(c) = context.ok_or_else(ApiError::forbidden)?;
    let mut tx = app_transaction(Write::begin(&s.pool).await?, &s, &p, &c, id).await?;
    let row = sqlx::query("SELECT status,recovery_reason IS NOT NULL AS recovery FROM installation_steward_assignments
        WHERE installation_id=$1 AND id=$2 AND subject_id=$3 AND superseded_at IS NULL FOR UPDATE")
        .bind(id).bind(input.assignment_id).bind(&p.id).fetch_optional(&mut *tx).await?.ok_or_else(ApiError::forbidden)?;
    if !matches!(row.get::<&str, _>("status"), "pending" | "issued") {
        return Err(ApiError::conflict("steward_assignment_changed"));
    }
    if input.stewards.len() > 4096 || input.stewards.iter().any(|u| u.is_empty() || u.len() > 256) {
        return Err(ApiError::bad("invalid_steward_roster"));
    }
    let active = sqlx::query_scalar::<_, String>(
        "SELECT id FROM users WHERE id=ANY($1) AND enabled ORDER BY id FOR SHARE",
    )
    .bind(&input.stewards)
    .fetch_all(&mut *tx)
    .await?;
    let blocked =
        !active.is_empty() || (!row.get::<bool, _>("recovery") && !input.stewards.is_empty());
    let (status, action) = if blocked {
        ("blocked", "steward.blocked")
    } else {
        ("issued", "steward.issued")
    };
    if row.get::<&str, _>("status") != status {
        sqlx::query("UPDATE installation_steward_assignments SET status=$2 WHERE id=$1")
            .bind(input.assignment_id)
            .bind(status)
            .execute(&mut *tx)
            .await?;
        audit::record(
            &mut tx,
            &p.id,
            action,
            &format!("installation:{id}:assignment:{}", input.assignment_id),
        )
        .await?;
    }
    tx.commit().await?;
    if blocked {
        return Err(ApiError::conflict("steward_roster_occupied"));
    }
    Ok(Json(json!({"assignmentId":input.assignment_id})))
}

pub async fn applied(
    S(s): S<State>,
    Extension(p): Extension<Principal>,
    context: Option<Extension<app_calls::Context>>,
    Path(id): Path<Uuid>,
    Json(input): Json<Receipt>,
) -> ApiResult<Json<Value>> {
    let Extension(c) = context.ok_or_else(ApiError::forbidden)?;
    let mut tx = app_transaction(Write::begin(&s.pool).await?, &s, &p, &c, id).await?;
    let status: String = sqlx::query_scalar(
        // A replacement can be requested after issuance and before the local commit is acknowledged.
        // Acknowledging that historical receipt changes no roster or later nomination.
        "SELECT status FROM installation_steward_assignments
        WHERE installation_id=$1 AND id=$2 AND subject_id=$3 FOR UPDATE",
    )
    .bind(id)
    .bind(input.assignment_id)
    .bind(&p.id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(ApiError::forbidden)?;
    match status.as_str() {
        "issued" => {
            sqlx::query("UPDATE installation_steward_assignments SET status='applied' WHERE id=$1")
                .bind(input.assignment_id)
                .execute(&mut *tx)
                .await?;
            audit::record(
                &mut tx,
                &p.id,
                "steward.applied",
                &format!("installation:{id}:assignment:{}", input.assignment_id),
            )
            .await?;
        }
        "applied" => (),
        _ => return Err(ApiError::conflict("steward_assignment_changed")),
    }
    tx.commit().await?;
    Ok(Json(json!({"assignmentId":input.assignment_id})))
}
