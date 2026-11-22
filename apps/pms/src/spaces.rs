//! Spaces, their members, the caller's sidebar and view preferences: AI-DO
//! service.py (list_spaces … remove_space_member), view_preferences.py and
//! auth/app_spaces_service.py (create/update/archive space, member rules).
//!
//! AI-DO kept PMS spaces as app-owned workspaces; here they are pms_spaces and
//! pms_space_members with the same rules: any PMS user may create a space and
//! becomes its owner; owners and admins manage it; only an owner grants or
//! changes admin/owner; the last owner must remain; anyone may leave.
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use miy_app_sdk::Identity;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};

use crate::access::{self, MANAGER_ROLES, ensure_space_access, normalize_role};
use crate::error::{ApiError, ApiResult, check_length};
use crate::input::{Body, Params, check_list_length, check_opt_length};
use crate::models::{MemberRow, SpaceRow, iso_datetime, new_id, paginate, utcnow};
use crate::notify;
use crate::statuses::create_default_space_statuses;
use crate::users;

const ROLE_CHOICES: &[&str] = &["owner", "admin", "viewer", "member"];

fn serialize_space(space: &SpaceRow, member_count: i64, role: Option<&str>) -> Value {
    json!({
        "id": space.id,
        "workspace_id": space.id,
        "workspace_key": space.key,
        "key": space.key,
        "name": space.name,
        "description": space.description,
        "member_count": member_count,
        "current_user_role": role,
        "created_at": iso_datetime(&space.created_at),
        "updated_at": iso_datetime(&space.updated_at),
    })
}

async fn member_count(conn: &mut PgConnection, space_id: &str) -> ApiResult<i64> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM pms_space_members WHERE space_id = $1")
            .bind(space_id)
            .fetch_one(conn)
            .await?,
    )
}

/// `GET /v1/pms/spaces`: active spaces the caller is a member of, by name.
pub async fn list_spaces(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<Value>> {
    let rows: Vec<(
        String,
        String,
        String,
        String,
        bool,
        chrono::NaiveDateTime,
        chrono::NaiveDateTime,
        String,
        i64,
    )> = sqlx::query_as(
        "SELECT s.id, s.key, s.name, s.description, s.active, s.created_at, s.updated_at, m.role, \
             (SELECT count(*) FROM pms_space_members c WHERE c.space_id = s.id) \
             FROM pms_spaces s JOIN pms_space_members m ON m.space_id = s.id \
             WHERE s.active AND m.user_id = $1 AND lower(trim(m.role)) = ANY($2) \
             ORDER BY s.name ASC, s.id",
    )
    .bind(&identity.user_id)
    .bind(access::ROLES)
    .fetch_all(&pool)
    .await?;
    Ok(Json(Value::Array(
        rows.into_iter()
            .map(
                |(id, key, name, description, active, created_at, updated_at, role, count)| {
                    let space = SpaceRow {
                        id,
                        key,
                        name,
                        description,
                        active,
                        created_at,
                        updated_at,
                    };
                    serialize_space(&space, count, normalize_role(&role).as_deref())
                },
            )
            .collect(),
    )))
}

/// `GET /v1/pms/space-preferences`: the caller's pinned/hidden spaces among
/// those they can still open.
pub async fn list_space_preferences(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<Value>> {
    let mut conn = pool.acquire().await?;
    let accessible = access::accessible_space_ids(&mut conn, &identity.user_id).await?;
    let rows: Vec<(String, bool, bool)> = sqlx::query_as(
        "SELECT team_id, pinned, hidden FROM pms_space_preferences \
         WHERE user_id = $1 AND team_id = ANY($2) ORDER BY team_id",
    )
    .bind(&identity.user_id)
    .bind(&accessible)
    .fetch_all(&mut *conn)
    .await?;
    Ok(Json(json!({
        "items": rows.into_iter().map(|(space_id, pinned, hidden)| json!({
            "space_id": space_id, "pinned": pinned, "hidden": hidden,
        })).collect::<Vec<_>>(),
    })))
}

/// `PATCH /v1/pms/spaces/{space_id}/preference`: pinning unhides, hiding unpins.
pub async fn update_space_preference(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    body: Body,
) -> ApiResult<Json<Value>> {
    body.forbid_extra(&["pinned", "hidden"])?;
    let pinned = body.opt_bool("pinned")?;
    let hidden = body.opt_bool("hidden")?;
    if pinned == Some(true) && hidden == Some(true) {
        return Err(ApiError::validation(
            "Value error, A space cannot be pinned and hidden at once.",
        ));
    }
    let mut tx = pool.begin().await?;
    ensure_space_access(&mut tx, &identity.user_id, &space_id).await?;
    let current: Option<(bool, bool)> = sqlx::query_as(
        "SELECT pinned, hidden FROM pms_space_preferences WHERE team_id = $1 AND user_id = $2 FOR UPDATE",
    )
    .bind(&space_id)
    .bind(&identity.user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (mut next_pinned, mut next_hidden) = current.unwrap_or((false, false));
    if let Some(pinned) = pinned {
        next_pinned = pinned;
        next_hidden = next_hidden && !pinned;
    }
    if let Some(hidden) = hidden {
        next_hidden = hidden;
        next_pinned = next_pinned && !hidden;
    }
    if !next_pinned && !next_hidden {
        sqlx::query("DELETE FROM pms_space_preferences WHERE team_id = $1 AND user_id = $2")
            .bind(&space_id)
            .bind(&identity.user_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(Json(
            json!({"space_id": space_id, "pinned": false, "hidden": false}),
        ));
    }
    let now = utcnow();
    sqlx::query(
        "INSERT INTO pms_space_preferences (team_id, user_id, pinned, hidden, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $5) ON CONFLICT (team_id, user_id) \
         DO UPDATE SET pinned = EXCLUDED.pinned, hidden = EXCLUDED.hidden, updated_at = EXCLUDED.updated_at",
    )
    .bind(&space_id)
    .bind(&identity.user_id)
    .bind(next_pinned)
    .bind(next_hidden)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"space_id": space_id, "pinned": next_pinned, "hidden": next_hidden}),
    ))
}

const GROUP_BY_CHOICES: &[&str] = &["none", "status", "assignee", "label"];

/// `GET /v1/pms/view-preferences`.
pub async fn get_view_preferences(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
) -> ApiResult<Json<Value>> {
    let value: Option<String> = sqlx::query_scalar(
        "SELECT task_list_group_by FROM pms_view_preferences WHERE user_id = $1",
    )
    .bind(&identity.user_id)
    .fetch_optional(&pool)
    .await?;
    Ok(Json(
        json!({"task_list_group_by": value.unwrap_or_else(|| "status".into())}),
    ))
}

/// `PATCH /v1/pms/view-preferences`.
pub async fn update_view_preferences(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    body: Body,
) -> ApiResult<Json<Value>> {
    let group_by = body.req_str("task_list_group_by")?;
    crate::error::check_choice("task_list_group_by", &group_by, GROUP_BY_CHOICES)?;
    let now = utcnow();
    sqlx::query(
        "INSERT INTO pms_view_preferences (user_id, task_list_group_by, created_at, updated_at) \
         VALUES ($1, $2, $3, $3) ON CONFLICT (user_id) \
         DO UPDATE SET task_list_group_by = EXCLUDED.task_list_group_by, updated_at = EXCLUDED.updated_at",
    )
    .bind(&identity.user_id)
    .bind(&group_by)
    .bind(now)
    .execute(&pool)
    .await?;
    Ok(Json(json!({"task_list_group_by": group_by})))
}

/// `POST /v1/pms/spaces`: the creator becomes the only owner; the default
/// statuses are seeded with the space (AI-DO `_initialize_pms_space`).
pub async fn create_space(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    let name = body.req_str("name")?.trim().to_string();
    let description = body.str_or("description", "")?.trim().to_string();
    check_length("name", &name, 2, 120)?;
    check_length("description", &description, 0, 1000)?;
    let id = new_id();
    let now = utcnow();
    let space = SpaceRow {
        key: format!("s-{id}"),
        id,
        name,
        description,
        active: true,
        created_at: now,
        updated_at: now,
    };
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO pms_spaces (id, key, name, description, active, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, true, $5, $5)",
    )
    .bind(&space.id)
    .bind(&space.key)
    .bind(&space.name)
    .bind(&space.description)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO pms_space_members (id, space_id, user_id, role, created_at) VALUES ($1, $2, $3, 'owner', $4)",
    )
    .bind(new_id())
    .bind(&space.id)
    .bind(&identity.user_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    create_default_space_statuses(&mut tx, &space.id).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(serialize_space(&space, 1, Some("owner"))),
    ))
}

/// `PATCH /v1/pms/spaces/{space_id}`: owners and admins.
pub async fn update_space(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    body: Body,
) -> ApiResult<Json<Value>> {
    let name = body.opt_str("name")?.map(|value| value.trim().to_string());
    let description = body
        .opt_str("description")?
        .map(|value| value.trim().to_string());
    check_opt_length("name", &name, 2, 120)?;
    check_opt_length("description", &description, 0, 1000)?;
    let mut tx = pool.begin().await?;
    let role = lock_space(&mut tx, &identity.user_id, &space_id).await?;
    require_manager(&role)?;
    if name.is_some() || description.is_some() {
        sqlx::query(
            "UPDATE pms_spaces SET name = coalesce($2, name), description = coalesce($3, description), \
             updated_at = CASE WHEN ($2 IS NOT NULL AND $2 <> name) OR ($3 IS NOT NULL AND $3 <> description) \
               THEN $4 ELSE updated_at END WHERE id = $1",
        )
        .bind(&space_id)
        .bind(&name)
        .bind(&description)
        .bind(utcnow())
        .execute(&mut *tx)
        .await?;
    }
    let space = access::load_active_space(&mut tx, &space_id)
        .await?
        .ok_or_else(|| ApiError::not_found("pms.space_not_found"))?;
    let count = member_count(&mut tx, &space_id).await?;
    tx.commit().await?;
    Ok(Json(serialize_space(&space, count, Some(&role))))
}

/// `DELETE /v1/pms/spaces/{space_id}`: archives the space (AI-DO
/// `archive_app_space` sets it inactive; the data stays).
pub async fn delete_space(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = pool.begin().await?;
    let role = lock_space(&mut tx, &identity.user_id, &space_id).await?;
    require_manager(&role)?;
    sqlx::query("UPDATE pms_spaces SET active = false, updated_at = $2 WHERE id = $1")
        .bind(&space_id)
        .bind(utcnow())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    notify::retract_space(&pool, &identity, &space_id).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn load_members(conn: &mut PgConnection, space_id: &str) -> ApiResult<Vec<MemberRow>> {
    Ok(sqlx::query_as::<_, MemberRow>(
        "SELECT id, space_id, user_id, role, created_at FROM pms_space_members WHERE space_id = $1",
    )
    .bind(space_id)
    .fetch_all(conn)
    .await?)
}

async fn load_membership(
    conn: &mut PgConnection,
    space_id: &str,
    user_id: &str,
) -> ApiResult<Option<MemberRow>> {
    Ok(sqlx::query_as::<_, MemberRow>(
        "SELECT id, space_id, user_id, role, created_at FROM pms_space_members \
         WHERE space_id = $1 AND user_id = $2",
    )
    .bind(space_id)
    .bind(user_id)
    .fetch_optional(conn)
    .await?)
}

fn serialize_member(member: &MemberRow, user: Option<&users::UserInfo>) -> Value {
    json!({
        "user_id": member.user_id,
        "email": "",
        "full_name": user.map(|u| u.name.clone()).unwrap_or_else(|| member.user_id.clone()),
        "primary_org_unit_name": user.and_then(|u| u.primary_group_name.clone()),
        // AI-DO showed the platform-admin flag; miy does not give apps other
        // people's admin flags, and it grants nothing here.
        "is_admin": false,
        "role": member.role,
        "joined_at": iso_datetime(&member.created_at),
    })
}

/// `GET /v1/pms/spaces/{space_id}/members`: owners and admins first, then by name.
pub async fn list_space_members(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let page = params.int("page", 1, 1, None)?;
    let page_size = params.int("page_size", 20, 1, Some(100))?;
    let mut conn = pool.acquire().await?;
    ensure_space_access(&mut conn, &identity.user_id, &space_id).await?;
    let members = load_members(&mut conn, &space_id).await?;
    drop(conn);
    let ids: Vec<String> = members
        .iter()
        .map(|member| member.user_id.clone())
        .collect();
    let found = users::lookup(&identity, &ids).await.unwrap_or_default();
    let mut members = members;
    members.sort_by_key(|member| {
        (
            !MANAGER_ROLES.contains(&member.role.as_str()),
            found
                .get(&member.user_id)
                .map(|user| user.name.to_lowercase())
                .unwrap_or_else(|| member.user_id.to_lowercase()),
        )
    });
    let items: Vec<Value> = members
        .iter()
        .map(|member| serialize_member(member, found.get(&member.user_id)))
        .collect();
    let (page_items, total) = paginate(&items, page, page_size);
    Ok(Json(json!({
        "items": page_items, "total": total, "page": page, "page_size": page_size,
    })))
}

async fn lock_space(conn: &mut PgConnection, user_id: &str, space_id: &str) -> ApiResult<String> {
    // Membership writers take the same space lock. Read the actor's current
    // role only after acquiring it, since a previous writer may have revoked it.
    sqlx::query_scalar::<_, String>(
        "SELECT id FROM pms_spaces WHERE id = $1 AND active FOR UPDATE",
    )
    .bind(space_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| ApiError::not_found("pms.space_not_found"))?;
    access::space_role(conn, user_id, space_id)
        .await?
        .ok_or_else(|| ApiError::forbidden("pms.space_access_required"))
}

async fn owner_count(conn: &mut PgConnection, space_id: &str) -> ApiResult<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM pms_space_members WHERE space_id = $1 AND lower(trim(role)) = 'owner'",
    )
    .bind(space_id)
    .fetch_one(conn)
    .await?)
}

/// AI-DO `_check_member_change`.
async fn check_member_change(
    conn: &mut PgConnection,
    space_id: &str,
    actor_role: &str,
    current_role: Option<&str>,
    next_role: Option<&str>,
    self_removal: bool,
) -> ApiResult<()> {
    if !self_removal
        && actor_role != "owner"
        && (current_role.is_some_and(|role| MANAGER_ROLES.contains(&role))
            || next_role.is_some_and(|role| MANAGER_ROLES.contains(&role)))
    {
        return Err(ApiError::forbidden("pms.space_owner_admin_manage_required"));
    }
    if current_role == Some("owner")
        && next_role != Some("owner")
        && owner_count(conn, space_id).await? <= 1
    {
        return Err(ApiError::conflict("pms.space_owner_must_remain"));
    }
    Ok(())
}

fn require_manager(role: &str) -> ApiResult<()> {
    if MANAGER_ROLES.contains(&role) {
        return Ok(());
    }
    Err(ApiError::forbidden("pms.space_owner_admin_required"))
}

/// AI-DO `put_app_space_member` after the target was validated.
async fn put_member(
    conn: &mut PgConnection,
    space_id: &str,
    actor_role: &str,
    target_user_id: &str,
    role: &str,
) -> ApiResult<()> {
    require_manager(actor_role)?;
    let membership = load_membership(conn, space_id, target_user_id).await?;
    let current = membership.as_ref().and_then(|m| normalize_role(&m.role));
    check_member_change(
        conn,
        space_id,
        actor_role,
        current.as_deref(),
        Some(role),
        false,
    )
    .await?;
    match membership {
        None => {
            sqlx::query(
                "INSERT INTO pms_space_members (id, space_id, user_id, role, created_at) VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(new_id())
            .bind(space_id)
            .bind(target_user_id)
            .bind(role)
            .bind(utcnow())
            .execute(&mut *conn)
            .await?;
        }
        Some(member) => {
            sqlx::query("UPDATE pms_space_members SET role = $2 WHERE id = $1")
                .bind(&member.id)
                .bind(role)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}

fn role_input(body: &Body, required: bool) -> ApiResult<String> {
    let role = if required {
        body.req_str("role")?
    } else {
        body.str_or("role", "member")?
    };
    crate::error::check_choice("role", &role, ROLE_CHOICES)?;
    Ok(role)
}

/// `POST /v1/pms/spaces/{space_id}/members`.
pub async fn add_space_member(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    let target = body.req_str("user_id")?;
    let role = role_input(&body, false)?;
    let mut tx = pool.begin().await?;
    let actor_role = lock_space(&mut tx, &identity.user_id, &space_id).await?;
    require_manager(&actor_role)?;
    let found = users::require_active(&identity, std::slice::from_ref(&target)).await?;
    if load_membership(&mut tx, &space_id, &target)
        .await?
        .is_some()
    {
        return Err(ApiError::conflict("pms.user_already_space_member"));
    }
    put_member(&mut tx, &space_id, &actor_role, &target, &role).await?;
    let member = load_membership(&mut tx, &space_id, &target)
        .await?
        .ok_or_else(ApiError::internal)?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(serialize_member(&member, found.get(&target))),
    ))
}

/// `POST /v1/pms/spaces/{space_id}/members/bulk`: members already in the space
/// are skipped, not rejected, so a department can be added again.
pub async fn add_space_members(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(space_id): Path<String>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    let user_ids = body
        .opt_str_list("user_ids")?
        .ok_or_else(|| ApiError::validation("user_ids: Field required"))?;
    check_list_length("user_ids", user_ids.len(), 1, 100)?;
    let role = role_input(&body, false)?;
    let mut tx = pool.begin().await?;
    let actor_role = lock_space(&mut tx, &identity.user_id, &space_id).await?;
    require_manager(&actor_role)?;
    let mut requested: Vec<String> = Vec::new();
    for id in user_ids {
        if !requested.contains(&id) {
            requested.push(id);
        }
    }
    let existing = access::space_member_ids(&mut tx, &space_id).await?;
    let found = users::require_active(&identity, &requested).await?;
    let added: Vec<String> = requested
        .iter()
        .filter(|id| !existing.contains(id))
        .cloned()
        .collect();
    for user_id in &added {
        put_member(&mut tx, &space_id, &actor_role, user_id, &role).await?;
    }
    let mut members = Vec::new();
    for user_id in &added {
        if let Some(member) = load_membership(&mut tx, &space_id, user_id).await? {
            members.push(member);
        }
    }
    tx.commit().await?;
    let items: Vec<Value> = members
        .iter()
        .map(|member| serialize_member(member, found.get(&member.user_id)))
        .collect();
    Ok((
        StatusCode::CREATED,
        Json(json!({"items": items, "skipped_count": requested.len() - added.len()})),
    ))
}

/// `PATCH /v1/pms/spaces/{space_id}/members/{user_id}`.
pub async fn update_space_member(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path((space_id, target)): Path<(String, String)>,
    body: Body,
) -> ApiResult<Json<Value>> {
    let role = role_input(&body, true)?;
    let mut tx = pool.begin().await?;
    let actor_role = lock_space(&mut tx, &identity.user_id, &space_id).await?;
    if load_membership(&mut tx, &space_id, &target)
        .await?
        .is_none()
    {
        return Err(ApiError::not_found("pms.member_not_found"));
    }
    require_manager(&actor_role)?;
    let found = users::require_active(&identity, std::slice::from_ref(&target)).await?;
    put_member(&mut tx, &space_id, &actor_role, &target, &role).await?;
    let member = load_membership(&mut tx, &space_id, &target)
        .await?
        .ok_or_else(ApiError::internal)?;
    tx.commit().await?;
    Ok(Json(serialize_member(&member, found.get(&target))))
}

/// `DELETE /v1/pms/spaces/{space_id}/members/{user_id}`: managers remove
/// others; anyone may leave; the last owner must remain. The removed person's
/// notifications for this space are retracted.
pub async fn remove_space_member(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path((space_id, target)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let mut tx = pool.begin().await?;
    let actor_role = lock_space(&mut tx, &identity.user_id, &space_id).await?;
    let self_removal = target == identity.user_id;
    if !self_removal {
        require_manager(&actor_role)?;
    }
    let Some(member) = load_membership(&mut tx, &space_id, &target).await? else {
        return Err(ApiError::not_found("pms.member_not_found"));
    };
    let current = normalize_role(&member.role);
    check_member_change(
        &mut tx,
        &space_id,
        &actor_role,
        current.as_deref(),
        None,
        self_removal,
    )
    .await?;
    sqlx::query("DELETE FROM pms_space_members WHERE id = $1")
        .bind(&member.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    notify::retract_space_member(&pool, &identity, &space_id, &target).await;
    Ok(StatusCode::NO_CONTENT)
}
