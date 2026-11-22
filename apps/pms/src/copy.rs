//! Copy selected tasks, with their subtasks, into a task list the caller can
//! edit: AI-DO task_copy.py. Assignees, labels, milestones and custom fields
//! carry over only when the target has a match; `unmatched` counts the rest.
//! With `reset_progress` the copies start in the target's first status with
//! their checklists unchecked and no dates or assignees. Description images
//! are not cloned (AI-DO media is not ported).
use std::collections::{HashMap, HashSet};

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::NaiveDate;
use miy_app_sdk::Identity;
use serde_json::json;
use sqlx::{PgConnection, PgPool};

use crate::access::{self, ensure_list_member, ensure_task_list_active};
use crate::error::{ApiError, ApiResult};
use crate::input::{Body, check_list_length};
use crate::models::{
    TASK_COLUMNS, TaskListRow, TaskRow, new_id, task_reference, today_utc, utcnow,
};
use crate::tasks::{
    actor_name, list_workflow, lock_task_list_order, log_activity, next_board_position,
    next_task_number,
};
use crate::workflow::{
    ListWorkflow, StatusDef, is_completion_status, normalize_status_category, status_category,
};

pub const MAX_TASK_COPY_SELECTION: usize = 500;
const DEFAULT_INITIAL_STATUS: &str = "todo";

fn name_key(value: &str) -> String {
    value.trim().to_lowercase()
}

/// `_copy_set`: the selection and their subtasks in list order (parents before
/// subtasks); archived subtasks stay behind unless selected themselves.
pub fn copy_set(
    rows: &[(String, Option<String>, i32, i32, bool)],
    selected: &HashSet<String>,
) -> Vec<String> {
    let ids: HashSet<&str> = rows.iter().map(|row| row.0.as_str()).collect();
    let mut children: HashMap<Option<&str>, Vec<&(String, Option<String>, i32, i32, bool)>> =
        HashMap::new();
    for row in rows {
        let parent = row.1.as_deref().filter(|parent| ids.contains(parent));
        children.entry(parent).or_default().push(row);
    }
    for siblings in children.values_mut() {
        siblings.sort_by_key(|row| (row.2, row.3));
    }
    let mut included: HashSet<&str> = HashSet::new();
    let mut stack: Vec<&(String, Option<String>, i32, i32, bool)> = rows
        .iter()
        .filter(|row| selected.contains(&row.0))
        .collect();
    while let Some(row) = stack.pop() {
        if !included.insert(row.0.as_str()) {
            continue;
        }
        if let Some(kids) = children.get(&Some(row.0.as_str())) {
            stack.extend(
                kids.iter()
                    .filter(|child| selected.contains(&child.0) || !child.4),
            );
        }
    }
    let mut ordered = Vec::new();
    let mut walk: Vec<&(String, Option<String>, i32, i32, bool)> =
        children.get(&None).cloned().unwrap_or_default();
    walk.reverse();
    while let Some(row) = walk.pop() {
        if included.contains(row.0.as_str()) {
            ordered.push(row.0.clone());
        }
        if let Some(kids) = children.get(&Some(row.0.as_str())) {
            walk.extend(kids.iter().rev());
        }
    }
    ordered
}

fn initial_status(definitions: &[StatusDef]) -> String {
    definitions
        .iter()
        .find(|definition| normalize_status_category(&definition.category) == "not_started")
        .or(definitions.first())
        .map(|definition| definition.slug.clone())
        .unwrap_or_else(|| DEFAULT_INITIAL_STATUS.into())
}

/// `_copied_status`.
pub fn copied_status(
    status: &str,
    source: &ListWorkflow,
    target_definitions: &[StatusDef],
    reset_progress: bool,
) -> String {
    if reset_progress {
        return initial_status(target_definitions);
    }
    if target_definitions.is_empty() || target_definitions.iter().any(|d| d.slug == status) {
        return status.to_string();
    }
    let category = status_category(status, Some(source));
    target_definitions
        .iter()
        .find(|definition| Some(normalize_status_category(&definition.category)) == category)
        .map(|definition| definition.slug.clone())
        .unwrap_or_else(|| initial_status(target_definitions))
}

/// `ensure_list_editor_from_resource_context`: the target's own space decides.
async fn ensure_target_editor(
    conn: &mut PgConnection,
    user_id: &str,
    list_id: &str,
) -> ApiResult<TaskListRow> {
    let Some(list) = access::load_list(conn, list_id).await? else {
        return Err(ApiError::not_found("pms.task_list_not_found"));
    };
    let Some(team_id) = list.team_id.clone() else {
        return Err(ApiError::not_found("pms.task_list_not_found"));
    };
    let Some(role) = access::space_role(conn, user_id, &team_id).await? else {
        return Err(ApiError::forbidden("pms.task_list_access_required"));
    };
    if role == "viewer" {
        return Err(ApiError::forbidden("pms.task_list_viewer_modify_denied"));
    }
    ensure_task_list_active(&list)?;
    Ok(list)
}

/// `POST /v1/pms/lists/{list_id}/tasks/copy`.
pub async fn copy_tasks(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(source_list_id): Path<String>,
    body: Body,
) -> ApiResult<impl IntoResponse> {
    body.forbid_extra(&["task_ids", "target_list_id", "reset_progress"])?;
    let task_ids = body
        .opt_str_list("task_ids")?
        .ok_or_else(|| ApiError::validation("task_ids: Field required"))?;
    check_list_length("task_ids", task_ids.len(), 1, MAX_TASK_COPY_SELECTION)?;
    let target_list_id = body.req_str("target_list_id")?;
    crate::error::check_length("target_list_id", &target_list_id, 1, usize::MAX)?;
    let reset_progress = body.opt_bool("reset_progress")?.unwrap_or(false);
    let unique: HashSet<String> = task_ids.iter().cloned().collect();
    if unique.len() != task_ids.len() {
        return Err(ApiError::bad_request("pms.duplicate_task_ids"));
    }
    let name = actor_name(&identity).await;
    let mut tx = pool.begin().await?;
    let (source_list, _) = ensure_list_member(&mut tx, &identity.user_id, &source_list_id).await?;
    let target_list = ensure_target_editor(&mut tx, &identity.user_id, &target_list_id).await?;
    lock_task_list_order(&mut tx, &target_list.id).await?;
    let rows: Vec<(String, Option<String>, i32, i32, bool)> = sqlx::query_as(
        "SELECT id, parent_id, board_position, task_number, archived FROM pms_tasks WHERE list_id = $1",
    )
    .bind(&source_list.id)
    .fetch_all(&mut *tx)
    .await?;
    let copy_ids = copy_set(&rows, &unique);
    if copy_ids.is_empty() {
        return Err(ApiError::not_found("pms.no_matching_tasks"));
    }
    let loaded: HashMap<String, TaskRow> = sqlx::query_as::<_, TaskRow>(&format!(
        "SELECT {TASK_COLUMNS} FROM pms_tasks WHERE id = ANY($1)"
    ))
    .bind(&copy_ids)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|task| (task.id.clone(), task))
    .collect();
    let sources: Vec<&TaskRow> = copy_ids.iter().filter_map(|id| loaded.get(id)).collect();
    let same_list = source_list.id == target_list.id;
    let source_workflow = list_workflow(&mut tx, &source_list.id).await?;
    let target_workflow = list_workflow(&mut tx, &target_list.id).await?;
    let mut target_definitions = target_workflow.definitions().to_vec();
    target_definitions.sort_by_key(|definition| definition.sort_order);
    let member_ids: HashSet<String> = match &target_list.team_id {
        Some(team) => access::space_member_ids(&mut tx, team)
            .await?
            .into_iter()
            .collect(),
        None => HashSet::new(),
    };
    let mut target_labels: HashMap<String, String> = HashMap::new();
    for (id, label_name) in sqlx::query_as::<_, (String, String)>(
        "SELECT id, name FROM pms_labels WHERE list_id = $1 ORDER BY created_at",
    )
    .bind(&target_list.id)
    .fetch_all(&mut *tx)
    .await?
    {
        target_labels.entry(name_key(&label_name)).or_insert(id);
    }
    let mut target_milestones: HashMap<String, String> = HashMap::new();
    for (id, title) in sqlx::query_as::<_, (String, String)>(
        "SELECT id, title FROM pms_milestones WHERE list_id = $1 ORDER BY sort_order",
    )
    .bind(&target_list.id)
    .fetch_all(&mut *tx)
    .await?
    {
        target_milestones.entry(name_key(&title)).or_insert(id);
    }
    let milestone_titles: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT id, title FROM pms_milestones WHERE list_id = $1",
    )
    .bind(&source_list.id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .collect();
    // Custom fields by (name, type).
    let source_fields: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, name, field_type FROM pms_custom_fields WHERE list_id = $1")
            .bind(&source_list.id)
            .fetch_all(&mut *tx)
            .await?;
    #[allow(clippy::type_complexity)]
    let mut field_targets: HashMap<String, (String, String, Option<serde_json::Value>)> =
        HashMap::new();
    if same_list {
        for (id, _, field_type) in &source_fields {
            let options: Option<serde_json::Value> =
                sqlx::query_scalar("SELECT options FROM pms_custom_fields WHERE id = $1")
                    .bind(id)
                    .fetch_one(&mut *tx)
                    .await?;
            field_targets.insert(id.clone(), (id.clone(), field_type.clone(), options));
        }
    } else {
        let mut by_key: HashMap<(String, String), (String, Option<serde_json::Value>)> =
            HashMap::new();
        for (id, field_name, field_type, options) in
            sqlx::query_as::<_, (String, String, String, Option<serde_json::Value>)>(
                "SELECT id, name, field_type, options FROM pms_custom_fields WHERE list_id = $1 \
                 ORDER BY sort_order, created_at",
            )
            .bind(&target_list.id)
            .fetch_all(&mut *tx)
            .await?
        {
            by_key
                .entry((name_key(&field_name), field_type))
                .or_insert((id, options));
        }
        for (id, field_name, field_type) in &source_fields {
            if let Some((target_id, options)) =
                by_key.get(&(name_key(field_name), field_type.clone()))
            {
                field_targets.insert(
                    id.clone(),
                    (target_id.clone(), field_type.clone(), options.clone()),
                );
            }
        }
    }
    let mut unmatched_assignees = 0;
    let mut unmatched_labels = 0;
    let mut unmatched_milestones = 0;
    let mut unmatched_fields = 0;
    let mut copy_by_source: HashMap<String, String> = HashMap::new();
    let mut next_child: HashMap<String, i32> = HashMap::new();
    let mut next_root = next_board_position(&mut tx, &target_list.id, None).await?;
    let mut next_number = next_task_number(&mut tx, &target_list.id).await?;
    let now = utcnow();
    let today: NaiveDate = today_utc();
    let mut copies: Vec<(String, String, i32)> = Vec::new();
    for source in &sources {
        let copy_id = new_id();
        let parent_id = source
            .parent_id
            .as_ref()
            .and_then(|parent| copy_by_source.get(parent).cloned());
        let board_position = match &parent_id {
            None => {
                let position = next_root;
                next_root += 1;
                position
            }
            Some(parent) => {
                let position = *next_child.get(parent).unwrap_or(&1);
                next_child.insert(parent.clone(), position + 1);
                position
            }
        };
        let status = copied_status(
            &source.status,
            &source_workflow,
            &target_definitions,
            reset_progress,
        );
        let mut milestone_id = None;
        if let Some(source_milestone) = &source.milestone_id {
            milestone_id = if same_list {
                Some(source_milestone.clone())
            } else {
                milestone_titles
                    .get(source_milestone)
                    .and_then(|title| target_milestones.get(&name_key(title)).cloned())
            };
            if milestone_id.is_none() {
                unmatched_milestones += 1;
            }
        }
        let completed_date =
            if reset_progress || !is_completion_status(&status, Some(&target_workflow)) {
                None
            } else {
                Some(source.completed_date.unwrap_or(today))
            };
        let assignees: Vec<String> = if reset_progress {
            Vec::new()
        } else {
            let mut source_assignees: Vec<String> = sqlx::query_scalar(
                "SELECT user_id FROM pms_task_assignees WHERE task_id = $1 ORDER BY position, id",
            )
            .bind(&source.id)
            .fetch_all(&mut *tx)
            .await?;
            if source_assignees.is_empty()
                && let Some(primary) = &source.assignee_id
            {
                source_assignees.push(primary.clone());
            }
            let kept: Vec<String> = source_assignees
                .iter()
                .filter(|id| member_ids.contains(*id))
                .cloned()
                .collect();
            unmatched_assignees += source_assignees.len() - kept.len();
            kept
        };
        sqlx::query(&format!(
            "INSERT INTO pms_tasks ({TASK_COLUMNS}) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, \
             $12, $13, $14, $15, $16, $17, false, $18, $18)"
        ))
        .bind(&copy_id)
        .bind(&target_list.id)
        .bind(next_number)
        .bind(&source.title)
        .bind(&source.description)
        .bind(&source.description_blocks)
        .bind(&status)
        .bind(&source.priority)
        .bind(assignees.first())
        .bind(&identity.user_id)
        .bind(&parent_id)
        .bind(&milestone_id)
        .bind(if reset_progress { None } else { source.start_date })
        .bind(if reset_progress { None } else { source.due_date })
        .bind(completed_date)
        .bind(board_position)
        .bind(&source.recurrence_rule)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        for (position, user_id) in assignees.iter().enumerate() {
            sqlx::query(
                "INSERT INTO pms_task_assignees (id, task_id, user_id, position) VALUES ($1, $2, $3, $4)",
            )
            .bind(new_id())
            .bind(&copy_id)
            .bind(user_id)
            .bind(position as i32)
            .execute(&mut *tx)
            .await?;
        }
        let source_labels: Vec<(String, String)> = sqlx::query_as(
            "SELECT l.id, l.name FROM pms_task_labels tl JOIN pms_labels l ON l.id = tl.label_id \
             WHERE tl.task_id = $1 ORDER BY tl.position, l.name",
        )
        .bind(&source.id)
        .fetch_all(&mut *tx)
        .await?;
        let mut position = 0;
        for (label_id, label_name) in source_labels {
            let target = if same_list {
                Some(label_id)
            } else {
                target_labels.get(&name_key(&label_name)).cloned()
            };
            let Some(target) = target else {
                unmatched_labels += 1;
                continue;
            };
            sqlx::query(
                "INSERT INTO pms_task_labels (id, task_id, label_id, position) VALUES ($1, $2, $3, $4) \
                 ON CONFLICT ON CONSTRAINT uq_pms_task_label DO NOTHING",
            )
            .bind(new_id())
            .bind(&copy_id)
            .bind(&target)
            .bind(position)
            .execute(&mut *tx)
            .await?;
            position += 1;
        }
        sqlx::query(
            "INSERT INTO pms_checklist_items (id, task_id, text, completed, sort_order, created_at) \
             SELECT gen_random_uuid()::text, $2, text, CASE WHEN $3 THEN false ELSE completed END, sort_order, $4 \
             FROM pms_checklist_items WHERE task_id = $1 ORDER BY sort_order",
        )
        .bind(&source.id)
        .bind(&copy_id)
        .bind(reset_progress)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        copy_by_source.insert(source.id.clone(), copy_id.clone());
        copies.push((source.id.clone(), copy_id, next_number));
        next_number += 1;
    }
    let source_ids: Vec<String> = copy_by_source.keys().cloned().collect();
    for (task_id, field_id, value) in sqlx::query_as::<_, (String, String, String)>(
        "SELECT task_id, field_id, value FROM pms_custom_field_values WHERE task_id = ANY($1)",
    )
    .bind(&source_ids)
    .fetch_all(&mut *tx)
    .await?
    {
        if value.is_empty() {
            continue;
        }
        let fits = field_targets
            .get(&field_id)
            .filter(|(_, field_type, options)| {
                field_type != "select"
                    || options
                        .as_ref()
                        .and_then(|options| options.as_array())
                        .is_some_and(|options| {
                            options.iter().any(|option| option.as_str() == Some(&value))
                        })
            });
        let Some((target_field, _, _)) = fits else {
            unmatched_fields += 1;
            continue;
        };
        sqlx::query(
            "INSERT INTO pms_custom_field_values (id, task_id, field_id, value) VALUES ($1, $2, $3, $4)",
        )
        .bind(new_id())
        .bind(&copy_by_source[&task_id])
        .bind(target_field)
        .bind(&value)
        .execute(&mut *tx)
        .await?;
    }
    let source_numbers: HashMap<&str, i32> = sources
        .iter()
        .map(|task| (task.id.as_str(), task.task_number))
        .collect();
    for (source_id, copy_id, number) in &copies {
        log_activity(
            &mut tx,
            copy_id,
            Some(&identity.user_id),
            "created",
            &format!(
                "{name} copied {} to {}.",
                task_reference(&source_list.key, source_numbers[source_id.as_str()]),
                task_reference(&target_list.key, *number)
            ),
            Some("copied_from"),
            None,
            Some(source_id.clone()),
        )
        .await?;
    }
    tx.commit().await?;
    let copied: Vec<String> = copies
        .iter()
        .map(|(_, copy_id, _)| copy_id.clone())
        .collect();
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "target_list_id": target_list.id,
            "copied_count": copied.len(),
            "copied_task_ids": copied,
            "unmatched": {
                "assignees": unmatched_assignees,
                "labels": unmatched_labels,
                "milestones": unmatched_milestones,
                "custom_fields": unmatched_fields,
            },
        })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(slug: &str, category: &str, order: i32) -> StatusDef {
        StatusDef {
            id: slug.into(),
            slug: slug.into(),
            name: slug.into(),
            color: "#000".into(),
            category: category.into(),
            sort_order: order,
        }
    }

    #[test]
    fn copy_set_walks_the_tree_and_skips_archived_subtasks() {
        let rows = vec![
            ("a".to_string(), None, 2, 1, false),
            ("a1".to_string(), Some("a".to_string()), 1, 2, false),
            ("a2".to_string(), Some("a".to_string()), 2, 3, true),
            ("b".to_string(), None, 1, 4, false),
        ];
        let selected = HashSet::from(["a".to_string()]);
        assert_eq!(copy_set(&rows, &selected), vec!["a", "a1"]);
        let selected = HashSet::from(["a".to_string(), "a2".to_string(), "b".to_string()]);
        assert_eq!(copy_set(&rows, &selected), vec!["b", "a", "a1", "a2"]);
    }

    #[test]
    fn copied_status_maps_by_slug_then_category() {
        let source = ListWorkflow {
            status_mode: "custom".into(),
            team_id: None,
            list_statuses: vec![status("qa", "active", 0)],
            space_statuses: vec![],
        };
        let target = vec![
            status("open", "not_started", 0),
            status("doing", "active", 1),
        ];
        assert_eq!(copied_status("qa", &source, &target, false), "doing");
        assert_eq!(copied_status("doing", &source, &target, false), "doing");
        assert_eq!(copied_status("qa", &source, &target, true), "open");
        assert_eq!(copied_status("mystery", &source, &target, false), "open");
        assert_eq!(copied_status("qa", &source, &[], false), "qa");
    }
}
