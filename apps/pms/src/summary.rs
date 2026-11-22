//! Response shapes shared by the routes: AI-DO projections.py
//! (`serialize_task_summary`) and service.py `_serialize_task_list`, with the
//! related rows loaded in batches instead of ORM relationships.
use std::collections::HashMap;

use chrono::NaiveDate;
use miy_app_sdk::Identity;
use serde_json::{Map, Value, json};
use sqlx::PgConnection;

use crate::error::ApiResult;
use crate::models::{TaskListRow, TaskRow, iso_date, iso_datetime, task_reference, today_utc};
use crate::users;
use crate::workflow::{
    ListWorkflow, StatusDef, calculate_progress, is_overdue_exempt_status, priority_label,
    status_label, task_progress,
};

/// A task list's key and workflow, for labels and categories of its tasks.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct ListInfo {
    pub key: String,
    pub name: String,
    pub team_id: Option<String>,
    pub archived: bool,
    pub workflow: ListWorkflow,
}

/// Workflows (and keys) of the given lists.
pub async fn load_lists(
    conn: &mut PgConnection,
    list_ids: &[String],
) -> ApiResult<HashMap<String, ListInfo>> {
    if list_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(String, String, String, String, Option<String>, bool)> = sqlx::query_as(
        "SELECT id, key, name, status_mode, team_id, archived FROM pms_task_lists WHERE id = ANY($1)",
    )
    .bind(list_ids)
    .fetch_all(&mut *conn)
    .await?;
    let team_ids: Vec<String> = rows.iter().filter_map(|row| row.4.clone()).collect();
    let list_statuses: Vec<(String, StatusDef)> = sqlx::query_as::<_, ListStatusRow>(
        "SELECT list_id AS owner, id, slug, name, color, category, sort_order \
         FROM pms_task_list_statuses WHERE list_id = ANY($1) ORDER BY sort_order, created_at",
    )
    .bind(list_ids)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(ListStatusRow::split)
    .collect();
    let space_statuses: Vec<(String, StatusDef)> = sqlx::query_as::<_, ListStatusRow>(
        "SELECT team_id AS owner, id, slug, name, color, category, sort_order \
         FROM pms_space_statuses WHERE team_id = ANY($1) ORDER BY sort_order, created_at",
    )
    .bind(&team_ids)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(ListStatusRow::split)
    .collect();
    Ok(rows
        .into_iter()
        .map(|(id, key, name, status_mode, team_id, archived)| {
            let workflow = ListWorkflow {
                status_mode,
                list_statuses: list_statuses
                    .iter()
                    .filter(|(owner, _)| *owner == id)
                    .map(|(_, status)| status.clone())
                    .collect(),
                space_statuses: space_statuses
                    .iter()
                    .filter(|(owner, _)| Some(owner) == team_id.as_ref())
                    .map(|(_, status)| status.clone())
                    .collect(),
                team_id: team_id.clone(),
            };
            (
                id,
                ListInfo {
                    key,
                    name,
                    team_id,
                    archived,
                    workflow,
                },
            )
        })
        .collect())
}

#[derive(sqlx::FromRow)]
struct ListStatusRow {
    owner: String,
    id: String,
    slug: String,
    name: String,
    color: String,
    category: String,
    sort_order: i32,
}

impl ListStatusRow {
    fn split(self) -> (String, StatusDef) {
        (
            self.owner,
            StatusDef {
                id: self.id,
                slug: self.slug,
                name: self.name,
                color: self.color,
                category: self.category,
                sort_order: self.sort_order,
            },
        )
    }
}

/// The related rows `serialize_task_summary` reads.
#[derive(Debug, Default)]
pub struct TaskContext {
    pub lists: HashMap<String, ListInfo>,
    pub milestones: HashMap<String, String>,
    pub labels: HashMap<String, Vec<Value>>,
    pub assignees: HashMap<String, Vec<String>>,
    pub followers: HashMap<String, Vec<String>>,
    pub comments: HashMap<String, i64>,
    pub subtasks: HashMap<String, i64>,
    pub checklist: HashMap<String, (i64, i64)>,
    pub names: HashMap<String, String>,
    /// miy: custom field values by task, then field id.
    pub custom_fields: HashMap<String, Map<String, Value>>,
}

impl TaskContext {
    pub fn workflow(&self, list_id: &str) -> Option<&ListWorkflow> {
        self.lists.get(list_id).map(|list| &list.workflow)
    }

    pub fn reference(&self, task: &TaskRow) -> String {
        let key = self
            .lists
            .get(&task.list_id)
            .map(|list| list.key.as_str())
            .unwrap_or("");
        task_reference(key, task.task_number)
    }

    pub fn name(&self, user_id: &str) -> String {
        self.names
            .get(user_id)
            .cloned()
            .unwrap_or_else(|| user_id.to_string())
    }

    /// `task_assignee_ids`: the links, or the primary assignee of older tasks.
    pub fn assignee_ids(&self, task: &TaskRow) -> Vec<String> {
        let ids = self.assignees.get(&task.id).cloned().unwrap_or_default();
        if ids.is_empty()
            && let Some(primary) = &task.assignee_id
        {
            return vec![primary.clone()];
        }
        ids
    }

    pub fn follower_ids(&self, task: &TaskRow) -> Vec<String> {
        self.followers.get(&task.id).cloned().unwrap_or_default()
    }
}

/// Load what [`serialize_task`] needs for `tasks`, names included.
pub async fn load_context(
    conn: &mut PgConnection,
    identity: &Identity,
    tasks: &[TaskRow],
) -> ApiResult<TaskContext> {
    let mut context = load_context_without_names(conn, tasks).await?;
    let mut people: Vec<String> = Vec::new();
    for task in tasks {
        people.push(task.reporter_id.clone());
        people.extend(task.assignee_id.clone());
    }
    for ids in context.assignees.values().chain(context.followers.values()) {
        people.extend(ids.iter().cloned());
    }
    people.sort();
    people.dedup();
    context.names = users::names(identity, &people).await;
    Ok(context)
}

pub async fn load_context_without_names(
    conn: &mut PgConnection,
    tasks: &[TaskRow],
) -> ApiResult<TaskContext> {
    let task_ids: Vec<String> = tasks.iter().map(|task| task.id.clone()).collect();
    let mut list_ids: Vec<String> = tasks.iter().map(|task| task.list_id.clone()).collect();
    list_ids.sort();
    list_ids.dedup();
    let milestone_ids: Vec<String> = tasks
        .iter()
        .filter_map(|task| task.milestone_id.clone())
        .collect();
    let mut context = TaskContext {
        lists: load_lists(conn, &list_ids).await?,
        ..Default::default()
    };
    if task_ids.is_empty() {
        return Ok(context);
    }
    context.milestones = sqlx::query_as::<_, (String, String)>(
        "SELECT id, title FROM pms_milestones WHERE id = ANY($1)",
    )
    .bind(&milestone_ids)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .collect();
    for (task_id, id, name, color) in sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT tl.task_id, l.id, l.name, l.color FROM pms_task_labels tl \
         JOIN pms_labels l ON l.id = tl.label_id WHERE tl.task_id = ANY($1) \
         ORDER BY tl.task_id, tl.position, l.name",
    )
    .bind(&task_ids)
    .fetch_all(&mut *conn)
    .await?
    {
        context
            .labels
            .entry(task_id)
            .or_default()
            .push(json!({"id": id, "name": name, "color": color}));
    }
    for (task_id, user_id) in sqlx::query_as::<_, (String, String)>(
        "SELECT task_id, user_id FROM pms_task_assignees WHERE task_id = ANY($1) \
         ORDER BY task_id, position, id",
    )
    .bind(&task_ids)
    .fetch_all(&mut *conn)
    .await?
    {
        context.assignees.entry(task_id).or_default().push(user_id);
    }
    for (task_id, user_id) in sqlx::query_as::<_, (String, String)>(
        "SELECT task_id, user_id FROM pms_task_followers WHERE task_id = ANY($1) \
         ORDER BY task_id, position, id",
    )
    .bind(&task_ids)
    .fetch_all(&mut *conn)
    .await?
    {
        context.followers.entry(task_id).or_default().push(user_id);
    }
    context.comments = sqlx::query_as::<_, (String, i64)>(
        "SELECT task_id, count(*) FROM pms_task_comments WHERE task_id = ANY($1) GROUP BY task_id",
    )
    .bind(&task_ids)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .collect();
    context.subtasks = sqlx::query_as::<_, (String, i64)>(
        "SELECT parent_id, count(*) FROM pms_tasks WHERE parent_id = ANY($1) GROUP BY parent_id",
    )
    .bind(&task_ids)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .collect();
    context.checklist = sqlx::query_as::<_, (String, i64, i64)>(
        "SELECT task_id, count(*), count(*) FILTER (WHERE completed) FROM pms_checklist_items \
         WHERE task_id = ANY($1) GROUP BY task_id",
    )
    .bind(&task_ids)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|(task_id, total, done)| (task_id, (total, done)))
    .collect();
    context.custom_fields = crate::custom_fields::values_by_task(conn, &task_ids).await?;
    Ok(context)
}

fn date_value(value: &Option<NaiveDate>) -> Value {
    value
        .as_ref()
        .map_or(Value::Null, |date| json!(iso_date(date)))
}

/// AI-DO `serialize_task_summary` (`TaskItem`).
pub fn serialize_task(task: &TaskRow, context: &TaskContext) -> Value {
    let workflow = context.workflow(&task.list_id);
    let assignee_ids = context.assignee_ids(task);
    let linked: Vec<String> = context.assignees.get(&task.id).cloned().unwrap_or_default();
    let mut assignee_names: Vec<String> = linked.iter().map(|id| context.name(id)).collect();
    if assignee_names.is_empty()
        && let Some(primary) = &task.assignee_id
    {
        assignee_names = vec![context.name(primary)];
    }
    let follower_ids = context.follower_ids(task);
    let follower_names: Vec<String> = follower_ids.iter().map(|id| context.name(id)).collect();
    let (checklist_total, checklist_done) =
        context.checklist.get(&task.id).copied().unwrap_or((0, 0));
    json!({
        "id": task.id,
        "list_id": task.list_id,
        "reference": context.reference(task),
        "title": task.title,
        "description": task.description,
        "description_blocks": task.description_blocks,
        "parent_id": task.parent_id,
        "subtask_count": context.subtasks.get(&task.id).copied().unwrap_or(0),
        "status": task.status,
        "status_label": status_label(&task.status, workflow),
        "priority": task.priority,
        "priority_label": priority_label(&task.priority),
        "assignee_id": task.assignee_id,
        "assignee_name": task.assignee_id.as_ref().map(|id| context.name(id)),
        "assignee_ids": assignee_ids,
        "assignee_names": assignee_names,
        "follower_ids": follower_ids,
        "follower_names": follower_names,
        "reporter_id": task.reporter_id,
        "reporter_name": context.name(&task.reporter_id),
        "milestone_id": task.milestone_id,
        "milestone_title": task.milestone_id.as_ref().and_then(|id| context.milestones.get(id)),
        "start_date": date_value(&task.start_date),
        "due_date": date_value(&task.due_date),
        "completed_date": date_value(&task.completed_date),
        "board_position": task.board_position,
        "archived": task.archived,
        "progress": task_progress(&task.status, workflow),
        "comments_count": context.comments.get(&task.id).copied().unwrap_or(0),
        "checklist_total": checklist_total,
        "checklist_done": checklist_done,
        "recurrence_rule": task.recurrence_rule,
        "labels": context.labels.get(&task.id).cloned().unwrap_or_default(),
        "custom_fields": context.custom_fields.get(&task.id).cloned().unwrap_or_default(),
        "updated_at": iso_datetime(&task.updated_at),
    })
}

/// Serialize tasks in order with one batch of related reads.
pub async fn serialize_tasks(
    conn: &mut PgConnection,
    identity: &Identity,
    tasks: &[TaskRow],
) -> ApiResult<Vec<Value>> {
    let context = load_context(conn, identity, tasks).await?;
    Ok(tasks
        .iter()
        .map(|task| serialize_task(task, &context))
        .collect())
}

/// Per-list task facts `_serialize_task_list` counts.
#[derive(Debug, Default, Clone)]
pub struct ListCounts {
    pub tasks: Vec<(String, bool, Option<NaiveDate>)>,
    pub milestone_count: i64,
}

pub async fn load_list_counts(
    conn: &mut PgConnection,
    list_ids: &[String],
) -> ApiResult<HashMap<String, ListCounts>> {
    let mut counts: HashMap<String, ListCounts> = HashMap::new();
    if list_ids.is_empty() {
        return Ok(counts);
    }
    for (list_id, status, archived, due_date) in
        sqlx::query_as::<_, (String, String, bool, Option<NaiveDate>)>(
            "SELECT list_id, status, archived, due_date FROM pms_tasks WHERE list_id = ANY($1)",
        )
        .bind(list_ids)
        .fetch_all(&mut *conn)
        .await?
    {
        counts
            .entry(list_id)
            .or_default()
            .tasks
            .push((status, archived, due_date));
    }
    for (list_id, count) in sqlx::query_as::<_, (String, i64)>(
        "SELECT list_id, count(*) FROM pms_milestones WHERE list_id = ANY($1) GROUP BY list_id",
    )
    .bind(list_ids)
    .fetch_all(&mut *conn)
    .await?
    {
        counts.entry(list_id).or_default().milestone_count = count;
    }
    Ok(counts)
}

/// AI-DO `_serialize_task_list` (`TaskListItem`).
#[allow(clippy::too_many_arguments)]
pub fn serialize_task_list(
    list: &TaskListRow,
    role: &str,
    team_name: Option<&str>,
    space_key: Option<&str>,
    folder_name: Option<&str>,
    member_count: i64,
    workflow: &ListWorkflow,
    counts: &ListCounts,
) -> Value {
    let today = today_utc();
    let open: Vec<&(String, bool, Option<NaiveDate>)> = counts
        .tasks
        .iter()
        .filter(|(status, archived, _)| {
            !archived && !is_overdue_exempt_status(status, Some(workflow))
        })
        .collect();
    let overdue = open
        .iter()
        .filter(|(_, _, due)| due.is_some_and(|due| due < today))
        .count();
    let active = counts
        .tasks
        .iter()
        .filter(|(_, archived, _)| !archived)
        .count();
    json!({
        "id": list.id,
        "key": list.key,
        "name": list.name,
        "description": list.description,
        "status": list.status,
        "status_mode": list.status_mode,
        "archived": list.archived,
        "team_id": list.team_id,
        "team_name": team_name,
        "workspace_id": list.team_id,
        "workspace_key": space_key,
        "folder_id": list.folder_id,
        "folder_name": if list.folder_id.is_some() { folder_name } else { None },
        "sort_order": list.sort_order,
        "role": role,
        "progress": calculate_progress(
            counts.tasks.iter().map(|(status, archived, _)| (status.as_str(), *archived)),
            Some(workflow),
        ),
        "member_count": member_count,
        "milestone_count": counts.milestone_count,
        "task_count": counts.tasks.len(),
        "active_task_count": active,
        "open_task_count": open.len(),
        "overdue_task_count": overdue,
        "created_at": iso_datetime(&list.created_at),
        "updated_at": iso_datetime(&list.updated_at),
    })
}

/// One list in the `TaskListItem` shape, loading what it needs.
pub async fn task_list_item(
    conn: &mut PgConnection,
    list: &TaskListRow,
    role: &str,
) -> ApiResult<Value> {
    let ids = vec![list.id.clone()];
    let lists = load_lists(conn, &ids).await?;
    let counts = load_list_counts(conn, &ids).await?;
    let (team_name, space_key, member_count): (Option<String>, Option<String>, i64) =
        match &list.team_id {
            Some(team_id) => sqlx::query_as(
                "SELECT s.name, s.key, (SELECT count(*) FROM pms_space_members m WHERE m.space_id = s.id) \
                 FROM pms_spaces s WHERE s.id = $1",
            )
            .bind(team_id)
            .fetch_optional(&mut *conn)
            .await?
            .map(|(name, key, count): (String, String, i64)| (Some(name), Some(key), count))
            .unwrap_or((None, None, 0)),
            None => (None, None, 0),
        };
    let folder_name: Option<String> = match &list.folder_id {
        Some(folder_id) => {
            sqlx::query_scalar("SELECT name FROM pms_folders WHERE id = $1")
                .bind(folder_id)
                .fetch_optional(&mut *conn)
                .await?
        }
        None => None,
    };
    let workflow = lists
        .get(&list.id)
        .map(|info| info.workflow.clone())
        .unwrap_or_default();
    Ok(serialize_task_list(
        list,
        role,
        team_name.as_deref(),
        space_key.as_deref(),
        folder_name.as_deref(),
        member_count,
        &workflow,
        counts.get(&list.id).unwrap_or(&ListCounts::default()),
    ))
}
