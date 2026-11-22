//! Tasks: AI-DO service.py (list_tasks, list_assigned_tasks,
//! list_today_overdue_tasks, list_personal_widget_assigned_tasks,
//! get_task_detail, create_task, update_task, delete, reorder, bulk) and the
//! router parts that live there.
use std::collections::{HashMap, HashSet};

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::NaiveDate;
use miy_app_sdk::Identity;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder};

use crate::access::{
    self, ensure_list_editor, ensure_list_member, ensure_task_readable, task_for_editor,
};
use crate::attachments;
use crate::custom_fields;
use crate::error::{ApiError, ApiResult, check_choice, check_length, check_range};
use crate::input::{Body, Params, check_list_length, check_opt_length};
use crate::models::{
    ATTACHMENT_COLUMNS, AttachmentRow, ChecklistRow, CommentRow, TASK_COLUMNS, TaskListRow,
    TaskRow, iso_date, iso_datetime, new_id, paginate, task_columns, task_reference, utcnow,
};
use crate::notify::{self, Outgoing};
use crate::summary::{self, TaskContext, serialize_task, serialize_tasks};
use crate::templates;
use crate::users;
use crate::workflow::{
    ListWorkflow, PRIORITIES, TASK_UPDATE_NULLABLE_FIELDS, TASK_UPDATE_SCALAR_FIELDS,
    is_completion_status, is_overdue_exempt_status, normalize_task_status,
};

// ── Shared helpers ─────────────────────────────────────────────────────

/// The caller's display name for activity messages (AI-DO `user.full_name`).
pub async fn actor_name(identity: &Identity) -> String {
    users::names(identity, std::slice::from_ref(&identity.user_id))
        .await
        .remove(&identity.user_id)
        .unwrap_or_else(|| identity.user_id.clone())
}

#[allow(clippy::too_many_arguments)]
pub async fn log_activity(
    conn: &mut PgConnection,
    task_id: &str,
    actor_id: Option<&str>,
    action: &str,
    message: &str,
    field_name: Option<&str>,
    from_value: Option<String>,
    to_value: Option<String>,
) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO pms_task_activity_logs (id, task_id, actor_id, action, field_name, from_value, to_value, message, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(new_id())
    .bind(task_id)
    .bind(actor_id)
    .bind(action)
    .bind(field_name)
    .bind(from_value)
    .bind(to_value)
    .bind(message)
    .bind(utcnow())
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn list_reference(conn: &mut PgConnection, task: &TaskRow) -> ApiResult<String> {
    let key: String = sqlx::query_scalar("SELECT key FROM pms_task_lists WHERE id = $1")
        .bind(&task.list_id)
        .fetch_one(conn)
        .await?;
    Ok(task_reference(&key, task.task_number))
}

pub async fn list_workflow(conn: &mut PgConnection, list_id: &str) -> ApiResult<ListWorkflow> {
    Ok(summary::load_lists(conn, &[list_id.to_string()])
        .await?
        .remove(list_id)
        .map(|info| info.workflow)
        .unwrap_or_default())
}

/// Acquire before task rows when a write can change a list's hierarchy or order.
pub async fn lock_task_list_order(conn: &mut PgConnection, list_id: &str) -> ApiResult<()> {
    sqlx::query("SELECT id FROM pms_task_lists WHERE id = $1 FOR UPDATE")
        .bind(list_id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn next_task_number(conn: &mut PgConnection, list_id: &str) -> ApiResult<i32> {
    let current: Option<i32> =
        sqlx::query_scalar("SELECT max(task_number) FROM pms_tasks WHERE list_id = $1")
            .bind(list_id)
            .fetch_one(conn)
            .await?;
    Ok(current.unwrap_or(0) + 1)
}

pub async fn next_board_position(
    conn: &mut PgConnection,
    list_id: &str,
    parent_id: Option<&str>,
) -> ApiResult<i32> {
    let current: Option<i32> = sqlx::query_scalar(
        "SELECT max(board_position) FROM pms_tasks WHERE list_id = $1 \
         AND (($2::text IS NULL AND parent_id IS NULL) OR parent_id = $2)",
    )
    .bind(list_id)
    .bind(parent_id)
    .fetch_one(conn)
    .await?;
    Ok(current.unwrap_or(0) + 1)
}

async fn task_assignee_ids(conn: &mut PgConnection, task: &TaskRow) -> ApiResult<Vec<String>> {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT user_id FROM pms_task_assignees WHERE task_id = $1 ORDER BY position, id",
    )
    .bind(&task.id)
    .fetch_all(conn)
    .await?;
    if ids.is_empty()
        && let Some(primary) = &task.assignee_id
    {
        return Ok(vec![primary.clone()]);
    }
    Ok(ids)
}

async fn task_link_ids(
    conn: &mut PgConnection,
    table: &str,
    task_id: &str,
) -> ApiResult<Vec<String>> {
    Ok(sqlx::query_scalar(&format!(
        "SELECT user_id FROM {table} WHERE task_id = $1 ORDER BY position, id"
    ))
    .bind(task_id)
    .fetch_all(conn)
    .await?)
}

/// `_task_notification_user_ids`: assignees, followers and the reporter.
async fn notification_user_ids(conn: &mut PgConnection, task: &TaskRow) -> ApiResult<Vec<String>> {
    let mut ids = task_assignee_ids(conn, task).await?;
    ids.extend(task_link_ids(conn, "pms_task_followers", &task.id).await?);
    ids.push(task.reporter_id.clone());
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// `_validate_task_assignees` (also used for followers): members of the
/// list's space, de-duplicated in order.
pub async fn validate_member_users(
    conn: &mut PgConnection,
    list: &TaskListRow,
    user_ids: &[String],
    invalid_member_code: &str,
) -> ApiResult<Vec<String>> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    let Some(team_id) = &list.team_id else {
        return Err(ApiError::conflict("pms.task_list_space_missing"));
    };
    let members: HashSet<String> = access::space_member_ids(conn, team_id)
        .await?
        .into_iter()
        .collect();
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for user_id in user_ids {
        if !seen.insert(user_id.clone()) {
            continue;
        }
        if !members.contains(user_id) {
            return Err(ApiError::bad_request(invalid_member_code));
        }
        result.push(user_id.clone());
    }
    Ok(result)
}

async fn validate_task_assignee(
    conn: &mut PgConnection,
    list: &TaskListRow,
    assignee_id: Option<&str>,
) -> ApiResult<()> {
    let Some(assignee_id) = assignee_id else {
        return Ok(());
    };
    let is_member = match &list.team_id {
        Some(team_id) => access::space_member_ids(conn, team_id)
            .await?
            .iter()
            .any(|id| id == assignee_id),
        None => false,
    };
    if !is_member {
        return Err(ApiError::bad_request(
            "pms.assignee_task_list_member_required",
        ));
    }
    Ok(())
}

async fn validate_milestone(
    conn: &mut PgConnection,
    list_id: &str,
    milestone_id: Option<&str>,
) -> ApiResult<()> {
    let Some(milestone_id) = milestone_id else {
        return Ok(());
    };
    // The list's own milestones and the space-wide milestones of its space.
    let ok: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pms_milestones m WHERE m.id = $1 AND (m.list_id = $2 \
         OR (m.list_id IS NULL AND m.team_id = (SELECT team_id FROM pms_task_lists WHERE id = $2))))",
    )
    .bind(milestone_id)
    .bind(list_id)
    .fetch_one(conn)
    .await?;
    if !ok {
        return Err(ApiError::bad_request("pms.milestone_wrong_list"));
    }
    Ok(())
}

/// `_validate_parent_task`.
async fn validate_parent_task(
    conn: &mut PgConnection,
    list_id: &str,
    parent_id: Option<&str>,
    task_id: Option<&str>,
) -> ApiResult<()> {
    let Some(parent_id) = parent_id else {
        return Ok(());
    };
    let parent: Option<(String, String, Option<String>)> =
        sqlx::query_as("SELECT id, list_id, parent_id FROM pms_tasks WHERE id = $1")
            .bind(parent_id)
            .fetch_optional(&mut *conn)
            .await?;
    let Some((id, parent_list, mut ancestor_parent)) = parent else {
        return Err(ApiError::not_found("pms.parent_task_not_found"));
    };
    if parent_list != list_id {
        return Err(ApiError::bad_request("pms.parent_task_same_list_required"));
    }
    if task_id == Some(id.as_str()) {
        return Err(ApiError::conflict("pms.task_cannot_be_own_parent"));
    }
    let mut visited: HashSet<String> = HashSet::from([id]);
    loop {
        if task_id.is_some() && ancestor_parent.as_deref() == task_id {
            return Err(ApiError::conflict("pms.task_parent_cycle"));
        }
        let Some(next) = ancestor_parent else {
            break;
        };
        if !visited.insert(next.clone()) {
            return Err(ApiError::conflict("pms.task_parent_cycle"));
        }
        ancestor_parent = sqlx::query_scalar::<_, Option<String>>(
            "SELECT parent_id FROM pms_tasks WHERE id = $1",
        )
        .bind(&next)
        .fetch_optional(&mut *conn)
        .await?
        .flatten();
    }
    Ok(())
}

fn validate_date_range(start: Option<NaiveDate>, due: Option<NaiveDate>) -> ApiResult<()> {
    if let (Some(start), Some(due)) = (start, due)
        && start > due
    {
        return Err(ApiError::code(
            StatusCode::UNPROCESSABLE_ENTITY,
            "pms.task_date_range_invalid",
        ));
    }
    Ok(())
}

/// `_set_task_labels`: replace the task's labels (all must belong to the list).
async fn set_task_labels(
    conn: &mut PgConnection,
    task_id: &str,
    list_id: &str,
    label_ids: &[String],
) -> ApiResult<()> {
    if !label_ids.is_empty() {
        let allowed: HashSet<String> =
            sqlx::query_scalar::<_, String>("SELECT id FROM pms_labels WHERE list_id = $1")
                .bind(list_id)
                .fetch_all(&mut *conn)
                .await?
                .into_iter()
                .collect();
        if label_ids.iter().any(|id| !allowed.contains(id)) {
            return Err(ApiError::bad_request("pms.labels_invalid_for_list"));
        }
    }
    let existing: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT label_id, id FROM pms_task_labels WHERE task_id = $1",
    )
    .bind(task_id)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .collect();
    sqlx::query("DELETE FROM pms_task_labels WHERE task_id = $1")
        .bind(task_id)
        .execute(&mut *conn)
        .await?;
    let mut seen = HashSet::new();
    for (position, label_id) in label_ids.iter().enumerate() {
        if !seen.insert(label_id) {
            continue;
        }
        sqlx::query(
            "INSERT INTO pms_task_labels (id, task_id, label_id, position) VALUES ($1, $2, $3, $4)",
        )
        .bind(existing.get(label_id).cloned().unwrap_or_else(new_id))
        .bind(task_id)
        .bind(label_id)
        .bind(position as i32)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// `_set_task_assignees`: links in order; the first is the primary assignee.
/// Returns whether the task row's `assignee_id` changed.
pub async fn set_task_assignees(
    conn: &mut PgConnection,
    task: &mut TaskRow,
    assignees: &[String],
) -> ApiResult<bool> {
    let existing: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT user_id, id FROM pms_task_assignees WHERE task_id = $1",
    )
    .bind(&task.id)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .collect();
    sqlx::query("DELETE FROM pms_task_assignees WHERE task_id = $1")
        .bind(&task.id)
        .execute(&mut *conn)
        .await?;
    for (position, user_id) in assignees.iter().enumerate() {
        sqlx::query(
            "INSERT INTO pms_task_assignees (id, task_id, user_id, position) VALUES ($1, $2, $3, $4)",
        )
        .bind(existing.get(user_id).cloned().unwrap_or_else(new_id))
        .bind(&task.id)
        .bind(user_id)
        .bind(position as i32)
        .execute(&mut *conn)
        .await?;
    }
    let primary = assignees.first().cloned();
    let changed = task.assignee_id != primary;
    task.assignee_id = primary;
    Ok(changed)
}

/// Persist every column of the task row (the ORM's UPDATE after changes).
pub async fn save_task(conn: &mut PgConnection, task: &TaskRow) -> ApiResult<()> {
    sqlx::query(
        "UPDATE pms_tasks SET title = $2, description = $3, description_blocks = $4, status = $5, \
         priority = $6, assignee_id = $7, parent_id = $8, milestone_id = $9, start_date = $10, \
         due_date = $11, completed_date = $12, board_position = $13, recurrence_rule = $14, \
         archived = $15, updated_at = $16 WHERE id = $1",
    )
    .bind(&task.id)
    .bind(&task.title)
    .bind(&task.description)
    .bind(&task.description_blocks)
    .bind(&task.status)
    .bind(&task.priority)
    .bind(&task.assignee_id)
    .bind(&task.parent_id)
    .bind(&task.milestone_id)
    .bind(task.start_date)
    .bind(task.due_date)
    .bind(task.completed_date)
    .bind(task.board_position)
    .bind(&task.recurrence_rule)
    .bind(task.archived)
    .bind(task.updated_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// `_set_missing_task_completed_date`.
async fn set_missing_completed_date(
    conn: &mut PgConnection,
    task: &mut TaskRow,
    actor_id: &str,
    workflow: &ListWorkflow,
    message: &str,
) -> ApiResult<bool> {
    if task.completed_date.is_some() || !is_completion_status(&task.status, Some(workflow)) {
        return Ok(false);
    }
    let today = crate::models::today_utc();
    task.completed_date = Some(today);
    log_activity(
        conn,
        &task.id,
        Some(actor_id),
        "updated",
        message,
        Some("completed_date"),
        None,
        Some(iso_date(&today)),
    )
    .await?;
    Ok(true)
}

async fn serialize_one(
    conn: &mut PgConnection,
    identity: &Identity,
    task_id: &str,
) -> ApiResult<Value> {
    let task = access::load_task(conn, task_id).await?;
    Ok(serialize_tasks(conn, identity, std::slice::from_ref(&task))
        .await?
        .into_iter()
        .next()
        .unwrap_or_default())
}

// ── Filters and order ──────────────────────────────────────────────────

/// AI-DO `TaskFilters` (the export reads the same ones).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct TaskFilters {
    pub status_filter: Vec<String>,
    pub assignee_id: Option<String>,
    pub priority: Option<String>,
    pub label_id: Option<String>,
    pub milestone_id: Option<String>,
    pub archived: Option<bool>,
    pub due_date_from: Option<NaiveDate>,
    pub due_date_to: Option<NaiveDate>,
    pub start_date_from: Option<NaiveDate>,
    pub start_date_to: Option<NaiveDate>,
    /// miy: `cf_<field id>=value` as sent, then resolved against the list's
    /// fields by [`TaskFilters::resolve_custom`].
    pub custom_raw: Vec<(String, String)>,
    pub custom: Vec<custom_fields::FieldFilter>,
}

impl TaskFilters {
    pub fn from_params(params: &Params) -> ApiResult<Self> {
        Ok(Self {
            status_filter: params.all("status"),
            assignee_id: params.opt_string("assignee_id"),
            priority: params.opt_string("priority"),
            label_id: params.opt_string("label_id"),
            milestone_id: params.opt_string("milestone_id"),
            archived: params.opt_bool("archived")?,
            due_date_from: params.opt_date("due_date_from")?,
            due_date_to: params.opt_date("due_date_to")?,
            start_date_from: params.opt_date("start_date_from")?,
            start_date_to: params.opt_date("start_date_to")?,
            custom_raw: custom_fields::raw_filters(&params.0),
            custom: Vec::new(),
        })
    }

    /// Resolve the custom-field conditions against the list's fields (the
    /// fields are returned for a custom-field sort).
    pub async fn resolve_custom(
        &mut self,
        conn: &mut PgConnection,
        list_id: &str,
    ) -> ApiResult<Vec<custom_fields::FieldRow>> {
        let fields = custom_fields::list_fields(conn, list_id).await?;
        self.custom = custom_fields::resolve_filters(&fields, &self.custom_raw)?;
        Ok(fields)
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// `filter_task_statement` (alias `t`); empty strings count as unset like
    /// Python's truthiness.
    pub fn push(&self, query: &mut QueryBuilder<'_, Postgres>) {
        if let Some(archived) = self.archived {
            query.push(" AND t.archived = ").push_bind(archived);
        }
        if !self.status_filter.is_empty() {
            query
                .push(" AND t.status = ANY(")
                .push_bind(self.status_filter.clone())
                .push(")");
        }
        if let Some(assignee) = self.assignee_id.clone().filter(|v| !v.is_empty()) {
            query
                .push(" AND (t.assignee_id = ")
                .push_bind(assignee.clone())
                .push(" OR EXISTS (SELECT 1 FROM pms_task_assignees a WHERE a.task_id = t.id AND a.user_id = ")
                .push_bind(assignee)
                .push("))");
        }
        if let Some(priority) = self.priority.clone().filter(|v| !v.is_empty()) {
            query.push(" AND t.priority = ").push_bind(priority);
        }
        if let Some(label) = self.label_id.clone().filter(|v| !v.is_empty()) {
            query
                .push(" AND EXISTS (SELECT 1 FROM pms_task_labels tl WHERE tl.task_id = t.id AND tl.label_id = ")
                .push_bind(label)
                .push(")");
        }
        if let Some(milestone) = self.milestone_id.clone().filter(|v| !v.is_empty()) {
            query.push(" AND t.milestone_id = ").push_bind(milestone);
        }
        if let Some(date) = self.due_date_from {
            query.push(" AND t.due_date >= ").push_bind(date);
        }
        if let Some(date) = self.due_date_to {
            query.push(" AND t.due_date <= ").push_bind(date);
        }
        if let Some(date) = self.start_date_from {
            query.push(" AND t.start_date >= ").push_bind(date);
        }
        if let Some(date) = self.start_date_to {
            query.push(" AND t.start_date <= ").push_bind(date);
        }
        custom_fields::push_filters(query, &self.custom);
    }
}

/// `matches_task_query`: plain substring over title, description and reference.
pub fn matches_task_query(task: &TaskRow, reference: &str, q: &str) -> bool {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    task.title.to_lowercase().contains(&q)
        || task.description.to_lowercase().contains(&q)
        || reference.to_lowercase().contains(&q)
}

/// AI-DO `_task_title_order_key`: lower-cased title where each digit run
/// becomes its length (three digits) and the digits without leading zeros,
/// compared by code point (`COLLATE "C"`), so "2. 설계" sorts before "10. 시공".
pub fn natural_title_key(title: &str) -> String {
    let lower = title.to_lowercase();
    let mut out = String::with_capacity(lower.len() + 8);
    let mut run = String::new();
    let mut run_is_digit = false;
    let flush = |run: &mut String, is_digit: bool, out: &mut String| {
        if run.is_empty() {
            return;
        }
        if is_digit {
            let digits = run.trim_start_matches('0');
            let length = format!("{:0>3}", digits.chars().count());
            out.push_str(&length.chars().take(3).collect::<String>());
            out.push_str(digits);
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for ch in lower.chars() {
        let is_digit = ch.is_ascii_digit();
        if !run.is_empty() && is_digit != run_is_digit {
            flush(&mut run, run_is_digit, &mut out);
        }
        run_is_digit = is_digit;
        run.push(ch);
    }
    flush(&mut run, run_is_digit, &mut out);
    out
}

fn priority_rank(priority: &str) -> i32 {
    match priority {
        "critical" => 3,
        "high" => 2,
        "medium" => 1,
        "low" => 0,
        _ => 1,
    }
}

/// The SQL ORDER BY of `_ordered_task_statement` (alias `t`), title excluded.
fn order_clause(sort_by: &str, descending: bool) -> String {
    let dir = if descending { "DESC" } else { "ASC" };
    match sort_by {
        "priority" => format!(
            "CASE t.priority WHEN 'critical' THEN 3 WHEN 'high' THEN 2 WHEN 'medium' THEN 1 \
             WHEN 'low' THEN 0 ELSE 1 END {dir}, t.task_number {dir}"
        ),
        "completed_date" | "due_date" | "start_date" => {
            format!("t.{sort_by} {dir} NULLS LAST, t.task_number {dir}")
        }
        "created_at" => format!("t.created_at {dir}, t.task_number {dir}"),
        "updated_at" => format!("t.updated_at {dir}, t.task_number {dir}"),
        _ => format!("t.board_position {dir}, t.task_number {dir}"),
    }
}

/// Title order (`_ordered_task_statement(..., "title", ...)`).
pub fn sort_by_title(tasks: &mut [TaskRow], descending: bool) {
    tasks.sort_by(|a, b| {
        let ordering = natural_title_key(&a.title)
            .cmp(&natural_title_key(&b.title))
            .then(a.task_number.cmp(&b.task_number));
        if descending {
            ordering.reverse()
        } else {
            ordering
        }
    });
}

/// The Python sorts `list_tasks` applies after a text search.
fn sort_searched(tasks: &mut Vec<TaskRow>, sort_by: &str, descending: bool) {
    let cmp_dir = |ordering: std::cmp::Ordering| {
        if descending {
            ordering.reverse()
        } else {
            ordering
        }
    };
    match sort_by {
        "title" => {}
        "priority" => tasks
            .sort_by(|a, b| cmp_dir(priority_rank(&a.priority).cmp(&priority_rank(&b.priority)))),
        "completed_date" | "due_date" | "start_date" => {
            let date = |task: &TaskRow| match sort_by {
                "completed_date" => task.completed_date,
                "due_date" => task.due_date,
                _ => task.start_date,
            };
            let (mut with, mut without): (Vec<TaskRow>, Vec<TaskRow>) =
                tasks.drain(..).partition(|task| date(task).is_some());
            with.sort_by(|a, b| cmp_dir((date(a), a.task_number).cmp(&(date(b), b.task_number))));
            without.sort_by(|a, b| cmp_dir(a.task_number.cmp(&b.task_number)));
            tasks.extend(with);
            tasks.extend(without);
        }
        "created_at" => tasks.sort_by(|a, b| {
            cmp_dir((a.created_at, a.task_number).cmp(&(b.created_at, b.task_number)))
        }),
        "updated_at" => tasks.sort_by(|a, b| cmp_dir(a.updated_at.cmp(&b.updated_at))),
        _ => tasks.sort_by(|a, b| {
            cmp_dir((a.board_position, a.task_number).cmp(&(b.board_position, b.task_number)))
        }),
    }
}

/// `GET /v1/pms/lists/{list_id}/tasks`.
pub async fn list_tasks(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let page = params.int("page", 1, 1, None)?;
    let page_size = params.int("page_size", 50, 1, Some(100))?;
    let sort_by = params.string("sort_by", "board_position");
    let sort_dir = params.choice("sort_dir", "asc", &["asc", "desc"])?;
    let q = params.string("q", "");
    let mut filters = TaskFilters::from_params(&params)?;
    let descending = sort_dir == "desc";
    let mut conn = pool.acquire().await?;
    let (list, _) = ensure_list_member(&mut conn, &identity.user_id, &list_id).await?;
    let fields = filters.resolve_custom(&mut conn, &list.id).await?;
    let field_sort = custom_fields::sort_field(&fields, &sort_by).cloned();
    let mut base = QueryBuilder::<Postgres>::new(format!(
        "SELECT {} FROM pms_tasks t WHERE t.list_id = ",
        task_columns("t")
    ));
    base.push_bind(list.id.clone());
    filters.push(&mut base);
    if q.trim().is_empty() && sort_by != "title" && field_sort.is_none() {
        let mut count =
            QueryBuilder::<Postgres>::new("SELECT count(*) FROM pms_tasks t WHERE t.list_id = ");
        count.push_bind(list.id.clone());
        filters.push(&mut count);
        let total: i64 = count.build_query_scalar().fetch_one(&mut *conn).await?;
        base.push(format!(" ORDER BY {}", order_clause(&sort_by, descending)));
        base.push(" OFFSET ").push_bind((page - 1) * page_size);
        base.push(" LIMIT ").push_bind(page_size);
        let tasks: Vec<TaskRow> = base.build_query_as().fetch_all(&mut *conn).await?;
        let items = serialize_tasks(&mut conn, &identity, &tasks).await?;
        return Ok(Json(json!({
            "items": items, "total": total, "page": page, "page_size": page_size,
        })));
    }
    let mut tasks: Vec<TaskRow> = base.build_query_as().fetch_all(&mut *conn).await?;
    if sort_by == "title" {
        sort_by_title(&mut tasks, descending);
    }
    tasks.retain(|task| matches_task_query(task, &task_reference(&list.key, task.task_number), &q));
    match &field_sort {
        Some(field) => {
            custom_fields::sort_tasks(&mut conn, &identity, field, &mut tasks, descending).await?
        }
        None => sort_searched(&mut tasks, &sort_by, descending),
    }
    let (page_tasks, total) = paginate(&tasks, page, page_size);
    let items = serialize_tasks(&mut conn, &identity, &page_tasks).await?;
    Ok(Json(json!({
        "items": items, "total": total, "page": page, "page_size": page_size,
    })))
}

/// Open tasks assigned to the caller in their active lists, sorted by due
/// date (none last) then most recently updated (AI-DO list_assigned_tasks).
pub async fn assigned_open_tasks(
    conn: &mut PgConnection,
    user_id: &str,
    due_by: Option<NaiveDate>,
) -> ApiResult<(Vec<TaskRow>, TaskContext)> {
    // Match assignments and current membership in the same SQL snapshot.
    let mut tasks: Vec<TaskRow> = sqlx::query_as(&format!(
        "SELECT {} FROM pms_tasks t JOIN pms_task_lists l ON l.id = t.list_id \
         JOIN pms_spaces s ON s.id = l.team_id AND s.active \
         JOIN pms_space_members m ON m.space_id = s.id AND m.user_id = $1 \
         WHERE NOT t.archived AND NOT l.archived AND lower(trim(m.role)) = ANY($2) \
         AND (t.assignee_id = $1 OR EXISTS (SELECT 1 FROM pms_task_assignees a \
            WHERE a.task_id = t.id AND a.user_id = $1)) \
         AND ($3::date IS NULL OR t.due_date <= $3)",
        task_columns("t")
    ))
    .bind(user_id)
    .bind(access::ROLES)
    .bind(due_by)
    .fetch_all(&mut *conn)
    .await?;
    let context = summary::load_context_without_names(conn, &tasks).await?;
    tasks.retain(|task| !is_overdue_exempt_status(&task.status, context.workflow(&task.list_id)));
    tasks.sort_by(|a, b| {
        (
            a.due_date.unwrap_or(NaiveDate::MAX),
            std::cmp::Reverse(a.updated_at),
        )
            .cmp(&(
                b.due_date.unwrap_or(NaiveDate::MAX),
                std::cmp::Reverse(b.updated_at),
            ))
    });
    Ok((tasks, context))
}

async fn with_names(
    identity: &Identity,
    tasks: &[TaskRow],
    mut context: TaskContext,
) -> TaskContext {
    let mut people: Vec<String> = Vec::new();
    for task in tasks {
        people.push(task.reporter_id.clone());
        people.extend(task.assignee_id.clone());
        people.extend(context.assignees.get(&task.id).cloned().unwrap_or_default());
        people.extend(context.followers.get(&task.id).cloned().unwrap_or_default());
    }
    people.sort();
    people.dedup();
    context.names = users::names(identity, &people).await;
    context
}

/// `GET /v1/pms/tasks/assigned`.
pub async fn list_assigned_tasks(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let limit = params.opt_int("limit", 1, Some(50))?.or(Some(10));
    let page = params.opt_int("page", 1, None)?;
    let page_size = params.int("page_size", 50, 1, Some(100))?;
    let mut conn = pool.acquire().await?;
    let (tasks, context) = assigned_open_tasks(&mut conn, &identity.user_id, None).await?;
    drop(conn);
    let total = tasks.len();
    let (selected, page, page_size) = match page {
        None => {
            let effective = limit.unwrap_or(page_size);
            (
                tasks
                    .iter()
                    .take(effective as usize)
                    .cloned()
                    .collect::<Vec<_>>(),
                1,
                effective,
            )
        }
        Some(page) => (paginate(&tasks, page, page_size).0, page, page_size),
    };
    let context = with_names(&identity, &selected, context).await;
    let items: Vec<Value> = selected
        .iter()
        .map(|task| serialize_task(task, &context))
        .collect();
    Ok(Json(json!({
        "items": items, "total": total, "page": page, "page_size": page_size,
    })))
}

/// `GET /v1/pms/tasks/today-overdue?today=YYYY-MM-DD`.
pub async fn list_today_overdue_tasks(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let today = params
        .opt_date("today")?
        .ok_or_else(|| ApiError::validation("today: Field required"))?;
    let page = params.int("page", 1, 1, None)?;
    let page_size = params.int("page_size", 50, 1, Some(100))?;
    let mut conn = pool.acquire().await?;
    let (tasks, context) = assigned_open_tasks(&mut conn, &identity.user_id, Some(today)).await?;
    drop(conn);
    let (selected, total) = paginate(&tasks, page, page_size);
    let context = with_names(&identity, &selected, context).await;
    let items: Vec<Value> = selected
        .iter()
        .map(|task| serialize_task(task, &context))
        .collect();
    Ok(Json(json!({
        "items": items, "total": total, "page": page, "page_size": page_size,
    })))
}

/// `GET /v1/personal-widgets/pms/tasks/assigned`: the floating PMS widget's
/// list across every space, each task with its space.
pub async fn list_personal_widget_assigned_tasks(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    params: Params,
) -> ApiResult<Json<Value>> {
    let page = params.int("page", 1, 1, None)?;
    let page_size = params.int("page_size", 50, 1, Some(100))?;
    let mut conn = pool.acquire().await?;
    let spaces: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT s.id, s.key, s.name, m.role FROM pms_spaces s JOIN pms_space_members m ON m.space_id = s.id \
         WHERE s.active AND m.user_id = $1 AND lower(trim(m.role)) = ANY($2) ORDER BY s.name ASC, s.key ASC",
    )
    .bind(&identity.user_id)
    .bind(access::ROLES)
    .fetch_all(&mut *conn)
    .await?;
    if spaces.is_empty() {
        return Ok(Json(json!({
            "items": [], "total": 0, "page": page, "page_size": page_size, "workspaces": [],
        })));
    }
    let (tasks, context) = assigned_open_tasks(&mut conn, &identity.user_id, None).await?;
    drop(conn);
    let (selected, total) = paginate(&tasks, page, page_size);
    let context = with_names(&identity, &selected, context).await;
    let by_id: HashMap<&str, &(String, String, String, String)> = spaces
        .iter()
        .map(|space| (space.0.as_str(), space))
        .collect();
    let items: Vec<Value> = selected
        .iter()
        .map(|task| {
            let mut value = serialize_task(task, &context);
            let space = context
                .lists
                .get(&task.list_id)
                .and_then(|list| list.team_id.as_deref())
                .and_then(|team| by_id.get(team));
            if let (Some(space), Value::Object(map)) = (space, &mut value) {
                map.insert(
                    "workspace".into(),
                    json!({"id": space.0, "slug": space.1, "name": space.2}),
                );
            }
            value
        })
        .collect();
    Ok(Json(json!({
        "items": items,
        "total": total,
        "page": page,
        "page_size": page_size,
        "workspaces": spaces.iter().map(|(id, key, name, role)| json!({
            "id": id, "slug": key, "name": name,
            "role": access::normalize_role(role).unwrap_or_else(|| role.clone()),
        })).collect::<Vec<_>>(),
    })))
}

// ── Detail ─────────────────────────────────────────────────────────────

pub fn serialize_comment(comment: &CommentRow, names: &HashMap<String, String>) -> Value {
    json!({
        "id": comment.id,
        "task_id": comment.task_id,
        "author_id": comment.author_id,
        "author_name": names.get(&comment.author_id).cloned().unwrap_or_else(|| comment.author_id.clone()),
        "body": comment.body,
        "body_blocks": comment.body_blocks,
        "created_at": iso_datetime(&comment.created_at),
    })
}

pub fn serialize_checklist_item(item: &ChecklistRow) -> Value {
    json!({
        "id": item.id,
        "task_id": item.task_id,
        "text": item.text,
        "completed": item.completed,
        "sort_order": item.sort_order,
        "created_at": iso_datetime(&item.created_at),
    })
}

/// `GET /v1/pms/tasks/{task_id}`: the caller's notification line for this
/// task is marked read (miy).
pub async fn get_task(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(task_id): Path<String>,
) -> ApiResult<Json<Value>> {
    let mut conn = pool.acquire().await?;
    let mut task = ensure_task_readable(&mut conn, &identity.user_id, &task_id).await?;
    if let Some(parent_id) = task.parent_id.as_deref()
        && !access::can_read_task(&mut conn, &identity.user_id, parent_id).await?
    {
        task.parent_id = None;
    }
    let comments: Vec<CommentRow> = sqlx::query_as::<_, CommentRow>(
        "SELECT id, task_id, author_id, body, body_blocks, created_at FROM pms_task_comments \
         WHERE task_id = $1 ORDER BY created_at, id",
    )
    .bind(&task.id)
    .fetch_all(&mut *conn)
    .await?;
    let subtasks = access::readable_subtasks(&mut conn, &identity.user_id, &task.id).await?;
    let attachment_rows: Vec<AttachmentRow> = sqlx::query_as::<_, AttachmentRow>(&format!(
        "SELECT {ATTACHMENT_COLUMNS} FROM pms_attachments WHERE task_id = $1 AND status = 'attached' \
         ORDER BY created_at, id"
    ))
    .bind(&task.id)
    .fetch_all(&mut *conn)
    .await?;
    let checklist: Vec<ChecklistRow> = sqlx::query_as::<_, ChecklistRow>(
        "SELECT id, task_id, text, completed, sort_order, created_at FROM pms_checklist_items \
         WHERE task_id = $1 ORDER BY sort_order, created_at",
    )
    .bind(&task.id)
    .fetch_all(&mut *conn)
    .await?;
    let mut all = vec![task.clone()];
    all.extend(subtasks.iter().cloned());
    let mut context = summary::load_context_without_names(&mut conn, &all).await?;
    let task_ids: Vec<&str> = all.iter().map(|row| row.id.as_str()).collect();
    context.subtasks =
        access::readable_subtask_counts(&mut conn, &identity.user_id, &task_ids).await?;
    drop(conn);
    let mut people: Vec<String> = comments.iter().map(|c| c.author_id.clone()).collect();
    people.extend(attachment_rows.iter().map(|a| a.uploaded_by_id.clone()));
    for task in &all {
        people.push(task.reporter_id.clone());
        people.extend(task.assignee_id.clone());
        people.extend(context.assignees.get(&task.id).cloned().unwrap_or_default());
        people.extend(context.followers.get(&task.id).cloned().unwrap_or_default());
    }
    people.sort();
    people.dedup();
    context.names = users::names(&identity, &people).await;
    let response = json!({
        "task": serialize_task(&task, &context),
        "comments": comments.iter().map(|c| serialize_comment(c, &context.names)).collect::<Vec<_>>(),
        // Docs links are not ported (the Docs app comes later).
        "linked_docs": [],
        "subtasks": subtasks.iter().map(|s| serialize_task(s, &context)).collect::<Vec<_>>(),
        "attachments": attachment_rows.iter().map(|a| attachments::serialize(a, &context.names)).collect::<Vec<_>>(),
        "checklist_items": checklist.iter().map(serialize_checklist_item).collect::<Vec<_>>(),
    });
    notify::mark_read(&pool, &identity, &task.id).await;
    Ok(Json(response))
}

// ── Create and update ──────────────────────────────────────────────────

const CREATE_FIELDS: &[&str] = &[
    "title",
    "description",
    "description_blocks",
    "status",
    "priority",
    "assignee_id",
    "assignee_ids",
    "milestone_id",
    "parent_id",
    "start_date",
    "due_date",
    "completed_date",
    "recurrence_rule",
    "label_ids",
    "template_id",
];

/// `POST /v1/pms/lists/{list_id}/tasks`. miy: with `template_id` (a template
/// of the list's space) the task also gets the template's checklist, labels and
/// subtasks; the other fields come from the request as usual.
pub async fn create_task(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    body.forbid_extra(CREATE_FIELDS)?;
    let title = body.req_str("title")?;
    check_length("title", &title, 2, 180)?;
    let description = body.str_or("description", "")?;
    check_length("description", &description, 0, 4000)?;
    let description_blocks = body.opt_dict_list("description_blocks")?;
    let status = body.str_or("status", "todo")?;
    let priority = body.str_or("priority", "medium")?;
    check_choice("priority", &priority, PRIORITIES)?;
    let mut assignee_id = body.opt_str("assignee_id")?;
    let assignee_ids = body.opt_str_list("assignee_ids")?;
    if let Some(ids) = &assignee_ids {
        check_list_length("assignee_ids", ids.len(), 0, 20)?;
    }
    let milestone_id = body.opt_str("milestone_id")?;
    let parent_id = body.opt_str("parent_id")?;
    let start_date = body.opt_date("start_date")?;
    let due_date = body.opt_date("due_date")?;
    let mut completed_date = body.opt_date("completed_date")?;
    let recurrence_rule = body.opt_str("recurrence_rule")?;
    check_opt_length("recurrence_rule", &recurrence_rule, 0, 120)?;
    let mut label_ids = body.opt_str_list("label_ids")?.unwrap_or_default();
    let template_id = body.opt_str("template_id")?;
    let name = actor_name(&identity).await;

    let mut tx = pool.begin().await?;
    lock_task_list_order(&mut tx, &list_id).await?;
    let (list, _) = ensure_list_editor(&mut tx, &identity.user_id, &list_id).await?;
    let template = match &template_id {
        Some(template_id) => Some(templates::template_for_list(&mut tx, template_id, &list).await?),
        None => None,
    };
    if let Some(template) = &template {
        for label_id in templates::label_ids_for(&mut tx, &list.id, &template.labels).await? {
            if !label_ids.contains(&label_id) {
                label_ids.push(label_id);
            }
        }
    }
    let validated: Option<Vec<String>> = match &assignee_ids {
        Some(ids) => {
            let users = validate_member_users(
                &mut tx,
                &list,
                ids,
                "pms.assignees_task_list_members_required",
            )
            .await?;
            assignee_id = users.first().cloned();
            Some(users)
        }
        None => {
            validate_task_assignee(&mut tx, &list, assignee_id.as_deref()).await?;
            assignee_id.clone().map(|id| vec![id])
        }
    };
    validate_milestone(&mut tx, &list.id, milestone_id.as_deref()).await?;
    validate_parent_task(&mut tx, &list.id, parent_id.as_deref(), None).await?;
    validate_date_range(start_date, due_date)?;
    let status = normalize_task_status(&status);
    check_length("status", &status, 0, 40)?;
    let workflow = list_workflow(&mut tx, &list.id).await?;
    if completed_date.is_none() && is_completion_status(&status, Some(&workflow)) {
        completed_date = Some(crate::models::today_utc());
    }
    let position = next_board_position(&mut tx, &list.id, parent_id.as_deref()).await?;
    let number = next_task_number(&mut tx, &list.id).await?;
    let now = utcnow();
    let mut task = TaskRow {
        id: new_id(),
        list_id: list.id.clone(),
        task_number: number,
        title: title.trim().to_string(),
        description: description.trim().to_string(),
        description_blocks,
        status,
        priority,
        assignee_id,
        reporter_id: identity.user_id.clone(),
        parent_id,
        milestone_id,
        start_date,
        due_date,
        completed_date,
        board_position: position,
        recurrence_rule,
        archived: false,
        created_at: now,
        updated_at: now,
    };
    sqlx::query(&format!(
        "INSERT INTO pms_tasks ({TASK_COLUMNS}) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, \
         $12, $13, $14, $15, $16, $17, $18, $19, $19)"
    ))
    .bind(&task.id)
    .bind(&task.list_id)
    .bind(task.task_number)
    .bind(&task.title)
    .bind(&task.description)
    .bind(&task.description_blocks)
    .bind(&task.status)
    .bind(&task.priority)
    .bind(&task.assignee_id)
    .bind(&task.reporter_id)
    .bind(&task.parent_id)
    .bind(&task.milestone_id)
    .bind(task.start_date)
    .bind(task.due_date)
    .bind(task.completed_date)
    .bind(task.board_position)
    .bind(&task.recurrence_rule)
    .bind(task.archived)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    if let Some(users) = &validated {
        set_task_assignees(&mut tx, &mut task, users).await?;
    }
    set_task_labels(&mut tx, &task.id, &list.id, &label_ids).await?;
    let reference = task_reference(&list.key, task.task_number);
    log_activity(
        &mut tx,
        &task.id,
        Some(&identity.user_id),
        "created",
        &format!("{name} created {reference}."),
        None,
        None,
        None,
    )
    .await?;
    if let Some(template) = &template {
        templates::apply_template_extras(&mut tx, &identity, &name, &list, &task, template).await?;
    }
    let item = serialize_one(&mut tx, &identity, &task.id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(item)))
}

const UPDATE_FIELDS: &[&str] = &[
    "title",
    "description",
    "description_blocks",
    "parent_id",
    "status",
    "priority",
    "assignee_id",
    "assignee_ids",
    "milestone_id",
    "start_date",
    "due_date",
    "completed_date",
    "board_position",
    "archived",
    "recurrence_rule",
    "label_ids",
];

/// A field value in a task update, as Python would `str()` it for the log.
#[derive(Debug, Clone, PartialEq)]
enum FieldValue {
    Null,
    Text(String),
    Date(NaiveDate),
    Int(i64),
    Bool(bool),
}

impl FieldValue {
    fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    fn python_str(&self) -> Option<String> {
        match self {
            Self::Null => None,
            Self::Text(value) => Some(value.clone()),
            Self::Date(value) => Some(iso_date(value)),
            Self::Int(value) => Some(value.to_string()),
            Self::Bool(value) => Some(if *value { "True" } else { "False" }.into()),
        }
    }
}

fn text(value: &Option<String>) -> FieldValue {
    value.clone().map_or(FieldValue::Null, FieldValue::Text)
}

fn date(value: Option<NaiveDate>) -> FieldValue {
    value.map_or(FieldValue::Null, FieldValue::Date)
}

fn current_value(task: &TaskRow, field: &str) -> FieldValue {
    match field {
        "title" => FieldValue::Text(task.title.clone()),
        "description" => FieldValue::Text(task.description.clone()),
        "status" => FieldValue::Text(task.status.clone()),
        "priority" => FieldValue::Text(task.priority.clone()),
        "assignee_id" => text(&task.assignee_id),
        "milestone_id" => text(&task.milestone_id),
        "start_date" => date(task.start_date),
        "due_date" => date(task.due_date),
        "completed_date" => date(task.completed_date),
        "board_position" => FieldValue::Int(task.board_position as i64),
        "archived" => FieldValue::Bool(task.archived),
        "recurrence_rule" => text(&task.recurrence_rule),
        _ => FieldValue::Null,
    }
}

fn apply_value(task: &mut TaskRow, field: &str, value: &FieldValue) {
    let as_text = || match value {
        FieldValue::Text(text) => Some(text.clone()),
        _ => None,
    };
    let as_date = || match value {
        FieldValue::Date(date) => Some(*date),
        _ => None,
    };
    match field {
        "title" => task.title = as_text().unwrap_or_default(),
        "description" => task.description = as_text().unwrap_or_default(),
        "status" => task.status = as_text().unwrap_or_default(),
        "priority" => task.priority = as_text().unwrap_or_default(),
        "assignee_id" => task.assignee_id = as_text(),
        "milestone_id" => task.milestone_id = as_text(),
        "start_date" => task.start_date = as_date(),
        "due_date" => task.due_date = as_date(),
        "completed_date" => task.completed_date = as_date(),
        "board_position" => {
            if let FieldValue::Int(position) = value {
                task.board_position = *position as i32;
            }
        }
        "archived" => {
            if let FieldValue::Bool(archived) = value {
                task.archived = *archived;
            }
        }
        "recurrence_rule" => task.recurrence_rule = as_text(),
        _ => {}
    }
}

/// The parsed fields of `TaskUpdateRequest`, with `provided` the fields sent.
pub struct TaskUpdate {
    pub provided: HashSet<String>,
    title: Option<String>,
    description: Option<String>,
    description_blocks: Option<Value>,
    parent_id: Option<String>,
    status: Option<String>,
    priority: Option<String>,
    assignee_id: Option<String>,
    pub assignee_ids: Option<Vec<String>>,
    milestone_id: Option<String>,
    start_date: Option<NaiveDate>,
    due_date: Option<NaiveDate>,
    completed_date: Option<NaiveDate>,
    board_position: Option<i64>,
    archived: Option<bool>,
    recurrence_rule: Option<String>,
    label_ids: Option<Vec<String>>,
}

impl TaskUpdate {
    pub fn parse(body: &Body) -> ApiResult<Self> {
        body.forbid_extra(UPDATE_FIELDS)?;
        let title = body.opt_str("title")?;
        check_opt_length("title", &title, 2, 180)?;
        let description = body.opt_str("description")?;
        check_opt_length("description", &description, 0, 4000)?;
        let assignee_ids = body.opt_str_list("assignee_ids")?;
        if let Some(ids) = &assignee_ids {
            check_list_length("assignee_ids", ids.len(), 0, 20)?;
        }
        let recurrence_rule = body.opt_str("recurrence_rule")?;
        check_opt_length("recurrence_rule", &recurrence_rule, 0, 120)?;
        Ok(Self {
            provided: body.0.keys().cloned().collect(),
            title,
            description,
            description_blocks: body.opt_dict_list("description_blocks")?,
            parent_id: body.opt_str("parent_id")?,
            status: body.opt_str("status")?,
            priority: body.opt_choice("priority", PRIORITIES)?,
            assignee_id: body.opt_str("assignee_id")?,
            assignee_ids,
            milestone_id: body.opt_str("milestone_id")?,
            start_date: body.opt_date("start_date")?,
            due_date: body.opt_date("due_date")?,
            completed_date: body.opt_date("completed_date")?,
            board_position: body.opt_int("board_position")?,
            archived: body.opt_bool("archived")?,
            recurrence_rule,
            label_ids: body.opt_str_list("label_ids")?,
        })
    }

    pub fn assignees_only(user_ids: Vec<String>) -> Self {
        Self {
            provided: HashSet::from(["assignee_ids".to_string()]),
            title: None,
            description: None,
            description_blocks: None,
            parent_id: None,
            status: None,
            priority: None,
            assignee_id: None,
            assignee_ids: Some(user_ids),
            milestone_id: None,
            start_date: None,
            due_date: None,
            completed_date: None,
            board_position: None,
            archived: None,
            recurrence_rule: None,
            label_ids: None,
        }
    }
}

/// `place_unlinked_task_after_former_parent`: a subtask moved to the top
/// level lands right after its former parent; root positions are renumbered.
async fn place_after_former_parent(
    conn: &mut PgConnection,
    task: &mut TaskRow,
    former_parent_id: &str,
) -> ApiResult<()> {
    let siblings: Vec<(String, i32)> = sqlx::query_as(
        "SELECT id, board_position FROM pms_tasks WHERE list_id = $1 AND parent_id IS NULL AND id <> $2 \
         ORDER BY board_position, task_number FOR UPDATE",
    )
    .bind(&task.list_id)
    .bind(&task.id)
    .fetch_all(&mut *conn)
    .await?;
    let index = siblings
        .iter()
        .position(|(id, _)| id == former_parent_id)
        .map(|index| index + 1)
        .unwrap_or(siblings.len());
    let mut ordered: Vec<(String, i32)> = siblings[..index].to_vec();
    ordered.push((task.id.clone(), task.board_position));
    ordered.extend(siblings[index..].iter().cloned());
    let now = utcnow();
    for (position, (id, current)) in ordered.iter().enumerate() {
        let next = (position as i32 + 1) * 1000;
        if *current == next {
            continue;
        }
        if id == &task.id {
            task.board_position = next;
        } else {
            sqlx::query("UPDATE pms_tasks SET board_position = $2, updated_at = $3 WHERE id = $1")
                .bind(id)
                .bind(next)
                .bind(now)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}

/// AI-DO `update_task`. Returns the serialized task; notifications are
/// published after the commit.
pub async fn apply_task_update(
    pool: &PgPool,
    identity: &Identity,
    task_id: &str,
    update: TaskUpdate,
) -> ApiResult<Value> {
    let mut effective = update.provided.clone();
    if effective.contains("assignee_ids") {
        effective.insert("assignee_id".into());
    }
    let name = actor_name(identity).await;
    let mut tx = pool.begin().await?;
    if effective.contains("board_position")
        || effective.contains("parent_id")
        || effective.contains("status")
    {
        let list_id: String = sqlx::query_scalar("SELECT list_id FROM pms_tasks WHERE id = $1")
            .bind(task_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ApiError::not_found("pms.task_not_found"))?;
        lock_task_list_order(&mut tx, &list_id).await?;
    }
    let (mut task, list) = task_for_editor(&mut tx, &identity.user_id, task_id).await?;
    let workflow = list_workflow(&mut tx, &list.id).await?;
    let mut assignee_id = update.assignee_id.clone();
    let validated: Option<Vec<String>> = if effective.contains("assignee_ids") {
        let users = validate_member_users(
            &mut tx,
            &list,
            update.assignee_ids.as_deref().unwrap_or_default(),
            "pms.assignees_task_list_members_required",
        )
        .await?;
        assignee_id = users.first().cloned();
        Some(users)
    } else if effective.contains("assignee_id") {
        validate_task_assignee(&mut tx, &list, assignee_id.as_deref()).await?;
        Some(assignee_id.clone().map(|id| vec![id]).unwrap_or_default())
    } else {
        None
    };
    let previous_assignee_ids = if validated.is_some() {
        task_assignee_ids(&mut tx, &task).await?
    } else {
        Vec::new()
    };
    if effective.contains("milestone_id") {
        validate_milestone(&mut tx, &list.id, update.milestone_id.as_deref()).await?;
    }
    if effective.contains("parent_id") {
        validate_parent_task(
            &mut tx,
            &list.id,
            update.parent_id.as_deref(),
            Some(&task.id),
        )
        .await?;
    }
    let status = if effective.contains("status") {
        update.status.as_deref().map(normalize_task_status)
    } else {
        None
    };
    check_opt_length("status", &status, 0, 40)?;
    validate_date_range(
        if effective.contains("start_date") {
            update.start_date
        } else {
            task.start_date
        },
        if effective.contains("due_date") {
            update.due_date
        } else {
            task.due_date
        },
    )?;
    let reference = task_reference(&list.key, task.task_number);
    let old_status = task.status.clone();
    let mut row_changed = false;
    let proposed = |field: &str| -> FieldValue {
        match field {
            "title" => text(&update.title),
            "description" => text(&update.description),
            "status" => text(&status),
            "priority" => text(&update.priority),
            "assignee_id" => text(&assignee_id),
            "milestone_id" => text(&update.milestone_id),
            "start_date" => date(update.start_date),
            "due_date" => date(update.due_date),
            "completed_date" => date(update.completed_date),
            "board_position" => update
                .board_position
                .map_or(FieldValue::Null, FieldValue::Int),
            "archived" => update.archived.map_or(FieldValue::Null, FieldValue::Bool),
            "recurrence_rule" => text(&update.recurrence_rule),
            _ => FieldValue::Null,
        }
    };
    for (field, message) in TASK_UPDATE_SCALAR_FIELDS {
        if !effective.contains(*field) {
            continue;
        }
        let mut value = proposed(field);
        if value.is_null() && !TASK_UPDATE_NULLABLE_FIELDS.contains(field) {
            continue;
        }
        if let FieldValue::Text(text) = &value {
            value = FieldValue::Text(text.trim().to_string());
        }
        let previous = current_value(&task, field);
        if previous == value {
            continue;
        }
        apply_value(&mut task, field, &value);
        row_changed = true;
        log_activity(
            &mut tx,
            &task.id,
            Some(&identity.user_id),
            "updated",
            &format!("{name} {message} for {reference}."),
            Some(field),
            previous.python_str(),
            value.python_str(),
        )
        .await?;
    }
    if effective.contains("parent_id") {
        let previous = task.parent_id.clone();
        if previous.is_some() && update.parent_id.is_none() && !effective.contains("board_position")
        {
            place_after_former_parent(&mut tx, &mut task, previous.as_deref().unwrap_or_default())
                .await?;
            row_changed = true;
        }
        task.parent_id = update.parent_id.clone();
        if previous != update.parent_id {
            row_changed = true;
            log_activity(
                &mut tx,
                &task.id,
                Some(&identity.user_id),
                "updated",
                &format!(
                    "{name} {} for {reference}.",
                    if update.parent_id.is_none() {
                        "removed parent"
                    } else {
                        "changed parent"
                    }
                ),
                Some("parent_id"),
                previous,
                update.parent_id.clone(),
            )
            .await?;
        }
    }
    if effective.contains("description_blocks")
        && task.description_blocks != update.description_blocks
    {
        task.description_blocks = update.description_blocks.clone();
        row_changed = true;
    }
    if effective.contains("label_ids")
        && let Some(label_ids) = &update.label_ids
    {
        set_task_labels(&mut tx, &task.id, &list.id, label_ids).await?;
        log_activity(
            &mut tx,
            &task.id,
            Some(&identity.user_id),
            "updated",
            &format!("{name} updated labels for {reference}."),
            Some("label_ids"),
            None,
            None,
        )
        .await?;
    }
    if let Some(users) = &validated {
        let current_links = task_link_ids(&mut tx, "pms_task_assignees", &task.id).await?;
        if current_links != *users {
            let primary_before = task.assignee_id.clone();
            set_task_assignees(&mut tx, &mut task, users).await?;
            row_changed |= primary_before != task.assignee_id;
        }
        if effective.contains("assignee_ids") && previous_assignee_ids != *users {
            log_activity(
                &mut tx,
                &task.id,
                Some(&identity.user_id),
                "updated",
                &format!("{name} updated assignees for {reference}."),
                Some("assignee_ids"),
                (!previous_assignee_ids.is_empty()).then(|| previous_assignee_ids.join(",")),
                (!users.is_empty()).then(|| users.join(",")),
            )
            .await?;
        }
    }
    if status.as_ref().is_some_and(|status| *status != old_status) {
        row_changed |= set_missing_completed_date(
            &mut tx,
            &mut task,
            &identity.user_id,
            &workflow,
            &format!("{name} updated completion date for {reference}."),
        )
        .await?;
    }
    if row_changed {
        task.updated_at = utcnow();
        save_task(&mut tx, &task).await?;
    }
    let label = notify::task_label(&task.title, &reference);
    let mut outgoing = Vec::new();
    if effective.contains("assignee_id") {
        outgoing.push(Outgoing {
            task_id: task.id.clone(),
            kind: notify::KIND_ASSIGNED,
            recipients: task_assignee_ids(&mut tx, &task).await?,
            title: notify::title(&name, notify::KIND_ASSIGNED, &label, None),
        });
    }
    if let Some(status) = status.as_ref().filter(|status| **status != old_status) {
        outgoing.push(Outgoing {
            task_id: task.id.clone(),
            kind: notify::KIND_STATUS,
            recipients: notification_user_ids(&mut tx, &task).await?,
            title: notify::title(
                &name,
                notify::KIND_STATUS,
                &label,
                Some(&notify::status_label_ko(status, Some(&workflow))),
            ),
        });
    }
    let item = serialize_one(&mut tx, identity, &task.id).await?;
    tx.commit().await?;
    notify::publish(pool, identity, outgoing).await;
    Ok(item)
}

/// `PATCH /v1/pms/tasks/{task_id}`.
pub async fn update_task(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(task_id): Path<String>,
    body: Body,
) -> ApiResult<Json<Value>> {
    let update = TaskUpdate::parse(&body)?;
    Ok(Json(
        apply_task_update(&pool, &identity, &task_id, update).await?,
    ))
}

// ── Reorder ────────────────────────────────────────────────────────────

/// `PATCH /v1/pms/lists/{list_id}/tasks/reorder`.
pub async fn reorder_tasks(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
    body: Body,
) -> ApiResult<Json<Value>> {
    body.forbid_extra(&["items"])?;
    let items = match body.0.get("items") {
        Some(Value::Array(items)) => items.clone(),
        Some(_) => return Err(ApiError::validation("items: Input should be a valid list")),
        None => return Err(ApiError::validation("items: Field required")),
    };
    check_list_length("items", items.len(), 1, 500)?;
    struct Move {
        task_id: String,
        board_position: i32,
        parent_id: Option<String>,
        parent_present: bool,
    }
    let mut moves = Vec::new();
    for item in items {
        let Value::Object(map) = item else {
            return Err(ApiError::validation(
                "items: Input should be a valid dictionary",
            ));
        };
        let item = Body(map);
        item.forbid_extra(&["task_id", "board_position", "parent_id"])?;
        let position = item
            .opt_int("board_position")?
            .ok_or_else(|| ApiError::validation("board_position: Field required"))?;
        check_range("board_position", position, 0, None)?;
        moves.push(Move {
            task_id: item.req_str("task_id")?,
            board_position: position as i32,
            parent_id: item.opt_str("parent_id")?,
            parent_present: item.has("parent_id"),
        });
    }
    let name = actor_name(&identity).await;
    let mut tx = pool.begin().await?;
    lock_task_list_order(&mut tx, &list_id).await?;
    let (list, _) = ensure_list_editor(&mut tx, &identity.user_id, &list_id).await?;
    let ids: Vec<String> = moves.iter().map(|m| m.task_id.clone()).collect();
    let unique: HashSet<&String> = ids.iter().collect();
    if unique.len() != ids.len() {
        return Err(ApiError::bad_request("pms.duplicate_task_ids"));
    }
    let mut tasks: HashMap<String, TaskRow> = sqlx::query_as::<_, TaskRow>(&format!(
        "SELECT {TASK_COLUMNS} FROM pms_tasks WHERE id = ANY($1) AND list_id = $2 ORDER BY id FOR UPDATE"
    ))
    .bind(&ids)
    .bind(&list.id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|task| (task.id.clone(), task))
    .collect();
    ensure_list_editor(&mut tx, &identity.user_id, &list_id).await?;
    if tasks.len() != ids.len() {
        return Err(ApiError::not_found("pms.no_matching_tasks"));
    }
    let mut parents: HashMap<String, Option<String>> =
        sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT id, parent_id FROM pms_tasks WHERE list_id = $1",
        )
        .bind(&list.id)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .collect();
    for m in &moves {
        if !m.parent_present {
            continue;
        }
        if m.parent_id.as_deref() == Some(m.task_id.as_str()) {
            return Err(ApiError::conflict("pms.task_cannot_be_own_parent"));
        }
        if let Some(parent) = &m.parent_id
            && !parents.contains_key(parent)
        {
            return Err(ApiError::not_found("pms.parent_task_not_found"));
        }
        parents.insert(m.task_id.clone(), m.parent_id.clone());
    }
    for m in &moves {
        let mut visited = HashSet::new();
        let mut parent = parents.get(&m.task_id).cloned().flatten();
        while let Some(current) = parent {
            if current == m.task_id || !visited.insert(current.clone()) {
                return Err(ApiError::conflict("pms.task_parent_cycle"));
            }
            parent = parents.get(&current).cloned().flatten();
        }
    }
    let mut updated: Vec<String> = Vec::new();
    let now = utcnow();
    for m in &moves {
        let task = tasks.get_mut(&m.task_id).expect("loaded");
        let mut changed = false;
        if task.board_position != m.board_position {
            task.board_position = m.board_position;
            changed = true;
        }
        if m.parent_present && task.parent_id != m.parent_id {
            let previous = task.parent_id.clone();
            task.parent_id = m.parent_id.clone();
            log_activity(
                &mut tx,
                &task.id,
                Some(&identity.user_id),
                "updated",
                &format!(
                    "{name} {} for {}.",
                    if m.parent_id.is_none() {
                        "removed parent"
                    } else {
                        "changed parent"
                    },
                    task_reference(&list.key, task.task_number)
                ),
                Some("parent_id"),
                previous,
                m.parent_id.clone(),
            )
            .await?;
            changed = true;
        }
        if changed {
            task.updated_at = now;
            sqlx::query(
                "UPDATE pms_tasks SET board_position = $2, parent_id = $3, updated_at = $4 WHERE id = $1",
            )
            .bind(&task.id)
            .bind(task.board_position)
            .bind(&task.parent_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            updated.push(task.id.clone());
        }
    }
    let rows: Vec<TaskRow> = ids
        .iter()
        .filter(|id| updated.contains(id))
        .filter_map(|id| tasks.get(id).cloned())
        .collect();
    let items = serialize_tasks(&mut tx, &identity, &rows).await?;
    tx.commit().await?;
    Ok(Json(json!({"updated_count": items.len(), "items": items})))
}

/// `PATCH /v1/pms/lists/{list_id}/tasks/reorder-by-title`: saves name order
/// as the custom order (parents before their subtasks); `updated_at` kept.
pub async fn reorder_tasks_by_title(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
    body: Body,
) -> ApiResult<Json<Value>> {
    body.forbid_extra(&["sort_dir"])?;
    let sort_dir = body.str_or("sort_dir", "asc")?;
    check_choice("sort_dir", &sort_dir, &["asc", "desc"])?;
    let mut tx = pool.begin().await?;
    lock_task_list_order(&mut tx, &list_id).await?;
    let (list, _) = ensure_list_editor(&mut tx, &identity.user_id, &list_id).await?;
    if list.team_id.is_none() {
        return Err(ApiError::conflict("pms.task_list_space_missing"));
    }
    sqlx::query("SELECT id FROM pms_tasks WHERE list_id = $1 ORDER BY id FOR UPDATE")
        .bind(&list.id)
        .execute(&mut *tx)
        .await?;
    ensure_list_editor(&mut tx, &identity.user_id, &list_id).await?;
    let mut rows: Vec<TaskRow> = sqlx::query_as::<_, TaskRow>(&format!(
        "SELECT {TASK_COLUMNS} FROM pms_tasks WHERE list_id = $1"
    ))
    .bind(&list.id)
    .fetch_all(&mut *tx)
    .await?;
    sort_by_title(&mut rows, sort_dir == "desc");
    let ordered = tree_order(&rows);
    let mut moved = 0;
    for (index, row) in ordered.iter().enumerate() {
        let position = (index as i32 + 1) * 1000;
        if row.board_position != position {
            sqlx::query("UPDATE pms_tasks SET board_position = $2 WHERE id = $1")
                .bind(&row.id)
                .bind(position)
                .execute(&mut *tx)
                .await?;
            moved += 1;
        }
    }
    tx.commit().await?;
    Ok(Json(json!({"updated_count": moved})))
}

/// Parents first, each followed by its subtasks, keeping the given order
/// among siblings; rows not reachable from a root come last.
pub fn tree_order(rows: &[TaskRow]) -> Vec<TaskRow> {
    let ids: HashSet<&str> = rows.iter().map(|row| row.id.as_str()).collect();
    let mut children: HashMap<Option<&str>, Vec<&TaskRow>> = HashMap::new();
    for row in rows {
        let parent = row
            .parent_id
            .as_deref()
            .filter(|parent| ids.contains(parent));
        children.entry(parent).or_default().push(row);
    }
    let mut ordered: Vec<TaskRow> = Vec::new();
    let mut pending: Vec<&TaskRow> = children.get(&None).cloned().unwrap_or_default();
    pending.reverse();
    let mut placed: HashSet<&str> = HashSet::new();
    while let Some(row) = pending.pop() {
        if !placed.insert(row.id.as_str()) {
            continue;
        }
        ordered.push(row.clone());
        if let Some(kids) = children.get(&Some(row.id.as_str())) {
            pending.extend(kids.iter().rev());
        }
    }
    ordered.extend(
        rows.iter()
            .filter(|row| !placed.contains(row.id.as_str()))
            .cloned(),
    );
    ordered
}

// ── Bulk and delete ────────────────────────────────────────────────────

pub const BULK_UPDATE_MAX_TASKS: usize = 200;

/// `PATCH /v1/pms/lists/{list_id}/tasks/bulk`.
pub async fn bulk_update_tasks(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
    body: Body,
) -> ApiResult<Json<Value>> {
    let task_ids = body
        .opt_str_list("task_ids")?
        .ok_or_else(|| ApiError::validation("task_ids: Field required"))?;
    check_list_length("task_ids", task_ids.len(), 1, BULK_UPDATE_MAX_TASKS)?;
    let status = body.opt_str("status")?;
    let priority = body.opt_choice("priority", PRIORITIES)?;
    let assignee_present = body.has("assignee_id");
    let assignee_id = body.opt_str("assignee_id")?;
    let add_label_ids = body.opt_str_list("add_label_ids")?.unwrap_or_default();
    let remove_label_ids = body.opt_str_list("remove_label_ids")?.unwrap_or_default();
    let archived = body.opt_bool("archived")?;
    let start_present = body.has("start_date");
    let start_date = body.opt_date("start_date")?;
    let due_present = body.has("due_date");
    let due_date = body.opt_date("due_date")?;
    let delete = body.opt_bool("delete")?.unwrap_or(false);
    let name = actor_name(&identity).await;

    let mut tx = pool.begin().await?;
    lock_task_list_order(&mut tx, &list_id).await?;
    let (list, _) = ensure_list_editor(&mut tx, &identity.user_id, &list_id).await?;
    let loaded: HashMap<String, TaskRow> = sqlx::query_as::<_, TaskRow>(&format!(
        "SELECT {TASK_COLUMNS} FROM pms_tasks WHERE id = ANY($1) AND list_id = $2 ORDER BY id FOR UPDATE"
    ))
    .bind(&task_ids)
    .bind(&list.id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|task| (task.id.clone(), task))
    .collect();
    ensure_list_editor(&mut tx, &identity.user_id, &list_id).await?;
    let mut seen = HashSet::new();
    let mut tasks: Vec<TaskRow> = task_ids
        .iter()
        .filter(|id| seen.insert((*id).clone()))
        .filter_map(|id| loaded.get(id).cloned())
        .collect();
    if tasks.is_empty() {
        return Err(ApiError::not_found("pms.no_matching_tasks"));
    }
    if delete {
        let ids: Vec<String> = tasks.iter().map(|task| task.id.clone()).collect();
        let deleted = lock_task_tree(&mut tx, &ids).await?;
        ensure_list_editor(&mut tx, &identity.user_id, &list_id).await?;
        delete_task_rows(&mut tx, &deleted).await?;
        tx.commit().await?;
        after_tasks_deleted(&pool, &identity, &deleted).await;
        return Ok(Json(
            json!({"updated_count": 0, "deleted_count": deleted.len()}),
        ));
    }
    let workflow = list_workflow(&mut tx, &list.id).await?;
    let labels: HashSet<String> =
        sqlx::query_scalar::<_, String>("SELECT id FROM pms_labels WHERE list_id = $1")
            .bind(&list.id)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect();
    let target_status = status.as_deref().map(normalize_task_status);
    check_opt_length("status", &target_status, 0, 40)?;
    let mut date_updates: Vec<(&str, Option<NaiveDate>, &str)> = Vec::new();
    if start_present {
        date_updates.push(("start_date", start_date, "updated start date"));
    }
    if due_present {
        date_updates.push(("due_date", due_date, "updated due date"));
    }
    if !date_updates.is_empty() {
        for task in &tasks {
            validate_date_range(
                if start_present {
                    start_date
                } else {
                    task.start_date
                },
                if due_present { due_date } else { task.due_date },
            )?;
        }
    }
    if assignee_present {
        validate_task_assignee(&mut tx, &list, assignee_id.as_deref()).await?;
    }
    let assignee_names = if assignee_present {
        let mut people: Vec<String> = tasks.iter().filter_map(|t| t.assignee_id.clone()).collect();
        people.sort();
        people.dedup();
        users::names(&identity, &people).await
    } else {
        HashMap::new()
    };
    let mut next_positions: HashMap<Option<String>, i32> = HashMap::new();
    let mut updated = 0;
    let me = identity.user_id.clone();
    for task in tasks.iter_mut() {
        let mut changed = false;
        let mut row_changed = false;
        if let Some(target) = target_status
            .as_ref()
            .filter(|target| **target != task.status)
        {
            log_activity(
                &mut tx,
                &task.id,
                Some(&me),
                "updated",
                &format!("{name} updated status."),
                Some("status"),
                Some(task.status.clone()),
                Some(target.clone()),
            )
            .await?;
            task.status = target.clone();
            let key = task.parent_id.clone();
            let next = match next_positions.get(&key) {
                Some(next) => *next,
                None => next_board_position(&mut tx, &list.id, key.as_deref()).await?,
            };
            next_positions.insert(key, next + 1);
            task.board_position = next;
            set_missing_completed_date(
                &mut tx,
                task,
                &me,
                &workflow,
                &format!("{name} updated completion date."),
            )
            .await?;
            changed = true;
            row_changed = true;
        }
        if let Some(priority) = priority.as_ref().filter(|p| **p != task.priority) {
            log_activity(
                &mut tx,
                &task.id,
                Some(&me),
                "updated",
                &format!("{name} updated priority."),
                Some("priority"),
                Some(task.priority.clone()),
                Some(priority.clone()),
            )
            .await?;
            task.priority = priority.clone();
            changed = true;
            row_changed = true;
        }
        for (field, value, message) in &date_updates {
            let previous = if *field == "start_date" {
                task.start_date
            } else {
                task.due_date
            };
            if previous == *value {
                continue;
            }
            log_activity(
                &mut tx,
                &task.id,
                Some(&me),
                "updated",
                &format!("{name} {message}."),
                Some(field),
                previous.as_ref().map(iso_date),
                value.as_ref().map(iso_date),
            )
            .await?;
            if *field == "start_date" {
                task.start_date = *value;
            } else {
                task.due_date = *value;
            }
            changed = true;
            row_changed = true;
        }
        if assignee_present {
            let links = task_link_ids(&mut tx, "pms_task_assignees", &task.id).await?;
            let current: Vec<String> = if links.is_empty() {
                task.assignee_id.clone().into_iter().collect()
            } else {
                links
            };
            let target: Vec<String> = assignee_id.clone().into_iter().collect();
            if task.assignee_id != assignee_id || current != target {
                let old_name = task
                    .assignee_id
                    .as_ref()
                    .map(|id| {
                        assignee_names
                            .get(id)
                            .cloned()
                            .unwrap_or_else(|| id.clone())
                    })
                    .unwrap_or_else(|| "Unassigned".into());
                row_changed |= set_task_assignees(&mut tx, task, &target).await?;
                log_activity(
                    &mut tx,
                    &task.id,
                    Some(&me),
                    "updated",
                    &format!("{name} updated assignee."),
                    Some("assignee"),
                    Some(old_name),
                    Some(assignee_id.clone().unwrap_or_else(|| "Unassigned".into())),
                )
                .await?;
                changed = true;
            }
        }
        if let Some(archived) = archived.filter(|archived| *archived != task.archived) {
            task.archived = archived;
            log_activity(
                &mut tx,
                &task.id,
                Some(&me),
                "updated",
                &format!(
                    "{name} {} task.",
                    if archived { "archived" } else { "unarchived" }
                ),
                Some("archived"),
                Some(if archived { "False" } else { "True" }.into()),
                Some(if archived { "True" } else { "False" }.into()),
            )
            .await?;
            changed = true;
            row_changed = true;
        }
        if !add_label_ids.is_empty() {
            let existing: HashSet<String> = sqlx::query_scalar::<_, String>(
                "SELECT label_id FROM pms_task_labels WHERE task_id = $1",
            )
            .bind(&task.id)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect();
            let mut position: i32 = sqlx::query_scalar::<_, Option<i32>>(
                "SELECT max(position) FROM pms_task_labels WHERE task_id = $1",
            )
            .bind(&task.id)
            .fetch_one(&mut *tx)
            .await?
            .unwrap_or(-1);
            let mut added = HashSet::new();
            for label_id in &add_label_ids {
                if !existing.contains(label_id)
                    && labels.contains(label_id)
                    && added.insert(label_id)
                {
                    position += 1;
                    sqlx::query(
                        "INSERT INTO pms_task_labels (id, task_id, label_id, position) VALUES ($1, $2, $3, $4)",
                    )
                    .bind(new_id())
                    .bind(&task.id)
                    .bind(label_id)
                    .bind(position)
                    .execute(&mut *tx)
                    .await?;
                    changed = true;
                }
            }
        }
        if !remove_label_ids.is_empty() {
            sqlx::query("DELETE FROM pms_task_labels WHERE task_id = $1 AND label_id = ANY($2)")
                .bind(&task.id)
                .bind(&remove_label_ids)
                .execute(&mut *tx)
                .await?;
            changed = true;
        }
        if row_changed {
            task.updated_at = utcnow();
            save_task(&mut tx, task).await?;
        }
        if changed {
            updated += 1;
        }
    }
    tx.commit().await?;
    Ok(Json(json!({"updated_count": updated, "deleted_count": 0})))
}

/// Find and lock the current descendants while the caller holds the list lock.
pub async fn lock_task_tree(
    conn: &mut PgConnection,
    task_ids: &[String],
) -> ApiResult<Vec<String>> {
    if task_ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = sqlx::query_scalar(
        "WITH RECURSIVE tree(id) AS (SELECT id FROM pms_tasks WHERE id = ANY($1) \
         UNION SELECT t.id FROM pms_tasks t JOIN tree ON t.parent_id = tree.id) \
         SELECT t.id FROM pms_tasks t JOIN tree ON tree.id = t.id ORDER BY t.id FOR UPDATE OF t",
    )
    .bind(task_ids)
    .fetch_all(conn)
    .await?;
    Ok(ids)
}

/// Delete the locked tasks and queue their platform attachments for release.
pub async fn delete_task_rows(conn: &mut PgConnection, ids: &[String]) -> ApiResult<()> {
    attachments::queue_release_for_tasks(conn, ids).await?;
    sqlx::query("DELETE FROM pms_custom_field_values WHERE task_id = ANY($1)")
        .bind(ids)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM pms_tasks WHERE id = ANY($1)")
        .bind(ids)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// After a delete committed: retract the tasks' notifications and release
/// their platform files.
pub async fn after_tasks_deleted(pool: &PgPool, identity: &Identity, task_ids: &[String]) {
    notify::retract_tasks(pool, identity, task_ids).await;
    attachments::process_releases(pool, identity).await;
}

/// `DELETE /v1/pms/tasks/{task_id}`.
pub async fn delete_task(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(task_id): Path<String>,
) -> ApiResult<StatusCode> {
    let mut tx = pool.begin().await?;
    let list_id = access::load_task(&mut tx, &task_id).await?.list_id;
    lock_task_list_order(&mut tx, &list_id).await?;
    let (task, _) = task_for_editor(&mut tx, &identity.user_id, &task_id).await?;
    let deleted = lock_task_tree(&mut tx, std::slice::from_ref(&task.id)).await?;
    ensure_list_editor(&mut tx, &identity.user_id, &list_id).await?;
    delete_task_rows(&mut tx, &deleted).await?;
    tx.commit().await?;
    after_tasks_deleted(&pool, &identity, &deleted).await;
    Ok(StatusCode::NO_CONTENT)
}

// ── Comments ───────────────────────────────────────────────────────────

/// `_extract_mentions_from_blocks`.
pub fn extract_mentions_from_blocks(blocks: &Value, out: &mut HashSet<String>) {
    let Value::Array(blocks) = blocks else {
        return;
    };
    for block in blocks {
        let Value::Object(block) = block else {
            continue;
        };
        if let Some(Value::Array(content)) = block.get("content") {
            for item in content {
                let Value::Object(item) = item else {
                    continue;
                };
                if item.get("type").and_then(Value::as_str) == Some("mention") {
                    let props = item.get("props").and_then(Value::as_object);
                    let attrs = item.get("attrs").and_then(Value::as_object);
                    let user_id = props
                        .and_then(|p| p.get("userId").or(p.get("user_id")))
                        .or_else(|| {
                            attrs.and_then(|a| a.get("id").or(a.get("userId")).or(a.get("user_id")))
                        })
                        .and_then(Value::as_str)
                        .filter(|id| !id.is_empty());
                    if let Some(user_id) = user_id {
                        out.insert(user_id.to_string());
                    }
                }
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    out.extend(mentions_in_text(text));
                }
            }
        }
        if let Some(Value::Array(children)) = block.get("children") {
            for child in children {
                if child.is_object() {
                    extract_mentions_from_blocks(&Value::Array(vec![child.clone()]), out);
                }
            }
        }
    }
}

/// `re.findall(r"@([0-9a-f-]{36})", text)`.
pub fn mentions_in_text(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'@' && index + 37 <= bytes.len() {
            let candidate = &bytes[index + 1..index + 37];
            if candidate
                .iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b) || *b == b'-')
            {
                found.push(String::from_utf8_lossy(candidate).into_owned());
                index += 37;
                continue;
            }
        }
        index += 1;
    }
    found
}

/// `POST /v1/pms/tasks/{task_id}/comments`.
pub async fn create_comment(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(task_id): Path<String>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    let text = body.str_or("body", "")?;
    check_length("body", &text, 0, 4000)?;
    let blocks = body.opt_dict_list("body_blocks")?;
    let name = actor_name(&identity).await;
    let mut tx = pool.begin().await?;
    let (task, list) = task_for_editor(&mut tx, &identity.user_id, &task_id).await?;
    let comment = CommentRow {
        id: new_id(),
        task_id: task.id.clone(),
        author_id: identity.user_id.clone(),
        body: text.trim().to_string(),
        body_blocks: blocks.clone(),
        created_at: utcnow(),
    };
    sqlx::query(
        "INSERT INTO pms_task_comments (id, task_id, author_id, body, body_blocks, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(&comment.id)
    .bind(&comment.task_id)
    .bind(&comment.author_id)
    .bind(&comment.body)
    .bind(&comment.body_blocks)
    .bind(comment.created_at)
    .execute(&mut *tx)
    .await?;
    let reference = task_reference(&list.key, task.task_number);
    log_activity(
        &mut tx,
        &task.id,
        Some(&identity.user_id),
        "commented",
        &format!("{name} added a comment to {reference}."),
        None,
        None,
        None,
    )
    .await?;
    let label = notify::task_label(&task.title, &reference);
    let mut notify_ids = notification_user_ids(&mut tx, &task).await?;
    notify_ids.retain(|id| *id != identity.user_id);
    let mut mentioned: HashSet<String> = mentions_in_text(&text).into_iter().collect();
    if let Some(blocks) = &blocks {
        extract_mentions_from_blocks(blocks, &mut mentioned);
    }
    mentioned.retain(|id| !notify_ids.contains(id) && *id != identity.user_id);
    let mut mentioned: Vec<String> = mentioned.into_iter().collect();
    mentioned.sort();
    let outgoing = vec![
        Outgoing {
            task_id: task.id.clone(),
            kind: notify::KIND_COMMENT,
            recipients: notify_ids,
            title: notify::title(&name, notify::KIND_COMMENT, &label, None),
        },
        Outgoing {
            task_id: task.id.clone(),
            kind: notify::KIND_MENTION,
            recipients: mentioned,
            title: notify::title(&name, notify::KIND_MENTION, &label, None),
        },
    ];
    tx.commit().await?;
    notify::publish(&pool, &identity, outgoing).await;
    let names = HashMap::from([(identity.user_id.clone(), name)]);
    Ok((
        StatusCode::CREATED,
        Json(serialize_comment(&comment, &names)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_title_order_reads_numbers() {
        let mut titles = vec!["10. 시공", "2. 설계", "1. 기획", "B", "a", "002 x"];
        titles.sort_by_key(|title| natural_title_key(title));
        // "002 x" sorts before "2. 설계": a space is below a full stop in code point order.
        assert_eq!(
            titles,
            vec!["1. 기획", "002 x", "2. 설계", "10. 시공", "a", "B"]
        );
        assert_eq!(natural_title_key("Task 007"), "task 0017");
    }

    #[test]
    fn mention_parsing_matches_ai_do() {
        let id = "0f9e8d7c-6b5a-4321-8fed-cba987654321";
        assert_eq!(
            mentions_in_text(&format!("hi @{id} there")),
            vec![id.to_string()]
        );
        assert!(mentions_in_text("@short").is_empty());
        let mut out = HashSet::new();
        extract_mentions_from_blocks(
            &json!([{
                "content": [
                    {"type": "mention", "props": {"userId": "u-1"}},
                    {"type": "text", "text": format!("cc @{id}")},
                ],
                "children": [{"content": [{"type": "mention", "attrs": {"id": "u-2"}}]}],
            }]),
            &mut out,
        );
        assert_eq!(
            out,
            HashSet::from(["u-1".to_string(), "u-2".to_string(), id.to_string()])
        );
    }

    fn row(id: &str, parent: Option<&str>) -> TaskRow {
        let at = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        TaskRow {
            id: id.into(),
            list_id: "l".into(),
            task_number: 1,
            title: id.into(),
            description: String::new(),
            description_blocks: None,
            status: "todo".into(),
            priority: "medium".into(),
            assignee_id: None,
            reporter_id: "u".into(),
            parent_id: parent.map(str::to_string),
            milestone_id: None,
            start_date: None,
            due_date: None,
            completed_date: None,
            board_position: 0,
            recurrence_rule: None,
            archived: false,
            created_at: at,
            updated_at: at,
        }
    }

    #[test]
    fn tree_order_puts_subtasks_under_parents() {
        let rows = vec![
            row("b", None),
            row("a1", Some("a")),
            row("a", None),
            row("b1", Some("b")),
        ];
        let ids: Vec<String> = tree_order(&rows).into_iter().map(|row| row.id).collect();
        assert_eq!(ids, vec!["b", "b1", "a", "a1"]);
    }
}
