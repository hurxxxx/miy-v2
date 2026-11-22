//! Rows of the PMS tables (migrations/0002_pms.sql) and small shared helpers.
use chrono::{NaiveDate, NaiveDateTime, Utc};
use serde_json::Value;
use sqlx::FromRow;

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Naive UTC "now", like AI-DO's `utcnow_naive`. Microsecond precision, as
/// PostgreSQL stores it, so a value read back compares equal.
pub fn utcnow() -> NaiveDateTime {
    use chrono::Timelike;
    let now = Utc::now().naive_utc();
    now.with_nanosecond(now.nanosecond() / 1000 * 1000)
        .unwrap_or(now)
}

pub fn today_utc() -> NaiveDate {
    Utc::now().date_naive()
}

/// FastAPI serializes naive datetimes without an offset (`2026-11-22T09:00:00`).
pub fn iso_datetime(value: &NaiveDateTime) -> String {
    value.format("%Y-%m-%dT%H:%M:%S%.f").to_string()
}

pub fn iso_date(value: &NaiveDate) -> String {
    value.format("%Y-%m-%d").to_string()
}

#[derive(Debug, Clone, FromRow)]
#[allow(dead_code)]
pub struct SpaceRow {
    pub id: String,
    pub key: String,
    pub name: String,
    pub description: String,
    pub active: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, FromRow)]
#[allow(dead_code)]
pub struct MemberRow {
    pub id: String,
    pub space_id: String,
    pub user_id: String,
    pub role: String,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, FromRow)]
pub struct TaskListRow {
    pub id: String,
    pub key: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub status_mode: String,
    pub archived: bool,
    pub team_id: Option<String>,
    pub folder_id: Option<String>,
    pub sort_order: i32,
    pub created_by_id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

pub const TASK_LIST_COLUMNS: &str = "id, key, name, description, status, status_mode, archived, \
     team_id, folder_id, sort_order, created_by_id, created_at, updated_at";

#[derive(Debug, Clone, FromRow)]
pub struct TaskRow {
    pub id: String,
    pub list_id: String,
    pub task_number: i32,
    pub title: String,
    pub description: String,
    pub description_blocks: Option<Value>,
    pub status: String,
    pub priority: String,
    pub assignee_id: Option<String>,
    pub reporter_id: String,
    pub parent_id: Option<String>,
    pub milestone_id: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub completed_date: Option<NaiveDate>,
    pub board_position: i32,
    pub recurrence_rule: Option<String>,
    pub archived: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

pub const TASK_COLUMNS: &str = "id, list_id, task_number, title, description, description_blocks, \
     status, priority, assignee_id, reporter_id, parent_id, milestone_id, start_date, due_date, \
     completed_date, board_position, recurrence_rule, archived, created_at, updated_at";

/// TASK_COLUMNS qualified with a table alias.
pub fn task_columns(alias: &str) -> String {
    TASK_COLUMNS
        .split(", ")
        .map(|column| format!("{alias}.{}", column.trim()))
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Debug, Clone, FromRow)]
#[allow(dead_code)]
pub struct MilestoneRow {
    pub id: String,
    pub list_id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub sort_order: i32,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, FromRow)]
pub struct CommentRow {
    pub id: String,
    pub task_id: String,
    pub author_id: String,
    pub body: String,
    pub body_blocks: Option<Value>,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, FromRow)]
pub struct ActivityRow {
    pub id: String,
    pub task_id: String,
    pub actor_id: Option<String>,
    pub action: String,
    pub field_name: Option<String>,
    pub from_value: Option<String>,
    pub to_value: Option<String>,
    pub message: String,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, FromRow)]
pub struct ChecklistRow {
    pub id: String,
    pub task_id: String,
    pub text: String,
    pub completed: bool,
    pub sort_order: i32,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, FromRow)]
pub struct AttachmentRow {
    pub id: String,
    pub task_id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i32,
    pub file_id: Option<uuid::Uuid>,
    pub request_id: uuid::Uuid,
    pub attach_request_id: uuid::Uuid,
    pub status: String,
    pub uploaded_by_id: String,
    pub created_at: NaiveDateTime,
}

pub const ATTACHMENT_COLUMNS: &str = "id, task_id, filename, content_type, size_bytes, file_id, \
     request_id, attach_request_id, status, uploaded_by_id, created_at";

/// Paginate an in-memory list like AI-DO's `_paginate`.
pub fn paginate<T: Clone>(items: &[T], page: i64, page_size: i64) -> (Vec<T>, usize) {
    let total = items.len();
    let start = ((page - 1).max(0) * page_size) as usize;
    let end = (start + page_size.max(0) as usize).min(total);
    if start >= total {
        return (Vec::new(), total);
    }
    (items[start..end].to_vec(), total)
}

/// AI-DO's `task_reference`: `KEY-12`.
pub fn task_reference(list_key: &str, task_number: i32) -> String {
    format!("{list_key}-{task_number}")
}
