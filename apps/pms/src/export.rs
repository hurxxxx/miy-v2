//! Excel export of a task list: AI-DO task_export.py (openpyxl) with
//! rust_xlsxwriter. Same sheet, columns, Korean labels, tree order, formats,
//! widths, frozen header and filter. Without a filter the file holds the whole
//! list (archived tasks too); with one it holds the rows the list shows.
use std::collections::{HashMap, HashSet};

use axum::{
    Extension,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};
use miy_app_sdk::Identity;
use rust_xlsxwriter::{Format, FormatAlign, Workbook};
use sqlx::{PgPool, Postgres, QueryBuilder};
use unicode_width::UnicodeWidthChar;

use crate::access::ensure_list_member;
use crate::error::{ApiError, ApiResult};
use crate::input::Params;
use crate::models::{TaskRow, task_columns, task_reference, today_utc};
use crate::summary;
use crate::tasks::{TaskFilters, matches_task_query};
use crate::users;
use crate::workflow::{
    DEFAULT_TASK_LIST_STATUSES, ListWorkflow, is_overdue_exempt_status, status_category,
    status_label,
};

pub const COLUMNS: &[&str] = &[
    "스페이스",
    "폴더",
    "리스트",
    "태스크번호",
    "상위태스크",
    "제목",
    "상태",
    "상태분류",
    "보관",
    "우선순위",
    "담당자",
    "등록자",
    "마일스톤",
    "시작일",
    "마감일",
    "지연일수",
    "완료일",
    "라벨",
    "체크리스트완료",
    "체크리스트전체",
    "댓글수",
    "등록일시",
    "수정일시",
];

fn priority_label(priority: &str) -> String {
    match priority {
        "critical" => "긴급",
        "high" => "높음",
        "medium" => "보통",
        "low" => "낮음",
        other => other,
    }
    .to_string()
}

fn category_label(category: Option<&str>) -> &'static str {
    match category {
        Some("not_started") => "미착수",
        Some("active") => "진행중",
        Some("done") => "완료",
        Some("closed") => "종료",
        _ => "",
    }
}

const WIDE_COLUMNS: &[&str] = &["제목", "상위태스크"];
const MIN_WIDTH: f64 = 9.0;
const MAX_WIDTH: f64 = 30.0;
const MAX_WIDE_WIDTH: f64 = 52.0;
const DATE_FORMAT: &str = "yyyy-mm-dd";
const DATETIME_FORMAT: &str = "yyyy-mm-dd hh:mm";

/// Korean glyphs take two cells (`east_asian_width` F/W).
pub fn display_width(text: &str) -> usize {
    text.lines()
        .map(|line| {
            line.chars()
                .map(|ch| if ch.width().unwrap_or(1) == 2 { 2 } else { 1 })
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0)
}

/// `_status_label`: the list's name for the status, then the seeded defaults.
fn export_status_label(status: &str, workflow: &ListWorkflow) -> String {
    if let Some(configured) = workflow.definitions().iter().find(|d| d.slug == status) {
        return configured.name.clone();
    }
    if let Some((_, name, ..)) = DEFAULT_TASK_LIST_STATUSES
        .iter()
        .find(|(slug, ..)| *slug == status)
    {
        return name.to_string();
    }
    status_label(status, Some(workflow))
}

/// `order_tasks`: depth-tagged rows with each subtask under its parent.
pub fn order_tasks(tasks: &[TaskRow]) -> Vec<(usize, TaskRow)> {
    let ids: HashSet<&str> = tasks.iter().map(|task| task.id.as_str()).collect();
    let mut children: HashMap<Option<&str>, Vec<&TaskRow>> = HashMap::new();
    for task in tasks {
        let owner = task
            .parent_id
            .as_deref()
            .filter(|parent| ids.contains(parent));
        children.entry(owner).or_default().push(task);
    }
    let mut ordered: Vec<(usize, TaskRow)> = Vec::new();
    let mut stack: Vec<(usize, &TaskRow)> = children
        .get(&None)
        .map(|roots| roots.iter().rev().map(|task| (0, *task)).collect())
        .unwrap_or_default();
    let mut emitted: HashSet<&str> = HashSet::new();
    while let Some((depth, task)) = stack.pop() {
        if !emitted.insert(task.id.as_str()) {
            continue;
        }
        ordered.push((depth, task.clone()));
        if let Some(kids) = children.get(&Some(task.id.as_str())) {
            stack.extend(kids.iter().rev().map(|child| (depth + 1, *child)));
        }
    }
    ordered.extend(
        tasks
            .iter()
            .filter(|task| !emitted.contains(task.id.as_str()))
            .map(|task| (0, task.clone())),
    );
    ordered
}

/// The file name the screen uses (AI-DO `content_disposition`).
pub fn export_filename(list_name: &str, list_key: &str, filtered: bool) -> String {
    let unsafe_chars = "\\/:*?\"<>|";
    let name: String = list_name
        .chars()
        .map(|ch| if unsafe_chars.contains(ch) { ' ' } else { ch })
        .collect::<String>()
        .trim()
        .to_string();
    let suffix = if filtered { " (필터 적용)" } else { "" };
    format!(
        "{}{suffix}.xlsx",
        if name.is_empty() { list_key } else { &name }
    )
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

enum Cell {
    Text(String),
    Number(f64),
    Date(NaiveDate),
    DateTime(NaiveDateTime),
    Empty,
}

fn local(value: &NaiveDateTime) -> NaiveDateTime {
    Utc.from_utc_datetime(value)
        .with_timezone(&chrono_tz::Asia::Seoul)
        .naive_local()
}

#[derive(utoipa::ToSchema)]
#[schema(format = Binary)]
struct XlsxWorkbook(#[allow(dead_code)] String);

/// `GET /v1/pms/lists/{list_id}/export`.
/// The same task filters as the list apply, including cf_<field ID> keys.
#[utoipa::path(
    get, path = "/v1/pms/lists/{list_id}/export", operation_id = "exportPmsTaskList", tag = "Task lists, folders and labels",
    params(
        ("list_id" = String, Path),
        ("q" = Option<String>, Query, description = "Match task title, description or reference"),
        ("status" = Option<Vec<String>>, Query, style = Form, explode, description = "Repeat status for multiple values"),
        ("assignee_id" = Option<String>, Query),
        ("priority" = Option<String>, Query),
        ("label_id" = Option<String>, Query),
        ("milestone_id" = Option<String>, Query),
        ("archived" = Option<bool>, Query),
        ("due_date_from" = Option<String>, Query, format = Date),
        ("due_date_to" = Option<String>, Query, format = Date),
        ("start_date_from" = Option<String>, Query, format = Date),
        ("start_date_to" = Option<String>, Query, format = Date)
    ),
    responses(
        (status = 200, description = "Excel workbook", body = XlsxWorkbook, content_type = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", headers(("Content-Disposition" = String, description = "Attachment filename, including UTF-8 filename*"), ("Cache-Control" = String, description = "private, no-store"), ("X-Content-Type-Options" = String, description = "nosniff"))),
        (status = 401, description = "Valid platform identity required", body = crate::error::IdentityErrorResponse),
        (status = 403, description = "Current permissions do not allow this operation", body = crate::error::ErrorResponse),
        (status = 404, description = "Resource is unavailable or does not exist", body = crate::error::ErrorResponse),
        (status = 409, description = "Current resource state conflicts with the request", body = crate::error::ErrorResponse),
        (status = 422, description = "Request validation failed", body = crate::error::ErrorResponse),
        (status = 500, description = "Could not complete the request", body = crate::error::ErrorResponse)
    )
)]
pub async fn export_task_list(
    State(pool): State<PgPool>,
    Extension(identity): Extension<Identity>,
    Path(list_id): Path<String>,
    params: Params,
) -> ApiResult<Response> {
    let q = params.string("q", "");
    let mut filters = TaskFilters::from_params(&params)?;
    let mut conn = pool.acquire().await?;
    let (list, _) = ensure_list_member(&mut conn, &identity.user_id, &list_id).await?;
    filters.resolve_custom(&mut conn, &list.id).await?;
    let mut query = QueryBuilder::<Postgres>::new(format!(
        "SELECT {} FROM pms_tasks t WHERE t.list_id = ",
        task_columns("t")
    ));
    query.push_bind(list.id.clone());
    filters.push(&mut query);
    query.push(" ORDER BY t.task_number");
    let mut tasks: Vec<TaskRow> = query.build_query_as().fetch_all(&mut *conn).await?;
    tasks.retain(|task| matches_task_query(task, &task_reference(&list.key, task.task_number), &q));
    let exported: HashSet<String> = tasks.iter().map(|task| task.id.clone()).collect();
    let missing: Vec<String> = tasks
        .iter()
        .filter_map(|task| task.parent_id.clone())
        .filter(|parent| !exported.contains(parent))
        .collect();
    let mut parent_titles: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
        "SELECT id, title FROM pms_tasks WHERE list_id = $1 AND id = ANY($2)",
    )
    .bind(&list.id)
    .bind(&missing)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .collect();
    parent_titles.extend(
        tasks
            .iter()
            .map(|task| (task.id.clone(), task.title.clone())),
    );
    let space_name: String = match &list.team_id {
        Some(team) => sqlx::query_scalar("SELECT name FROM pms_spaces WHERE id = $1")
            .bind(team)
            .fetch_optional(&mut *conn)
            .await?
            .unwrap_or_default(),
        None => String::new(),
    };
    let folder_name: String = match &list.folder_id {
        Some(folder) => sqlx::query_scalar("SELECT name FROM pms_folders WHERE id = $1")
            .bind(folder)
            .fetch_optional(&mut *conn)
            .await?
            .unwrap_or_default(),
        None => String::new(),
    };
    let context = summary::load_context_without_names(&mut conn, &tasks).await?;
    drop(conn);
    let mut people: Vec<String> = Vec::new();
    for task in &tasks {
        people.push(task.reporter_id.clone());
        people.extend(context.assignee_ids(task));
    }
    people.sort();
    people.dedup();
    let names = users::names(&identity, &people).await;
    let name = |id: &str| names.get(id).cloned().unwrap_or_else(|| id.to_string());
    let workflow = context.workflow(&list.id).cloned().unwrap_or_default();
    let today = today_utc();
    let filtered = !filters.is_empty() || !q.trim().is_empty();

    let ordered = order_tasks(&tasks);
    let rows: Vec<(usize, bool, Vec<Cell>)> = ordered
        .iter()
        .map(|(depth, task)| {
            let overdue = if is_overdue_exempt_status(&task.status, Some(&workflow)) {
                None
            } else {
                task.due_date
                    .filter(|due| *due < today)
                    .map(|due| (today - due).num_days())
            };
            let (checklist_total, checklist_done) =
                context.checklist.get(&task.id).copied().unwrap_or((0, 0));
            let labels: Vec<String> = context
                .labels
                .get(&task.id)
                .map(|labels| {
                    labels
                        .iter()
                        .filter_map(|label| label["name"].as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let text = |value: String| {
                if value.is_empty() {
                    Cell::Empty
                } else {
                    Cell::Text(value)
                }
            };
            let cells = vec![
                text(space_name.clone()),
                text(folder_name.clone()),
                text(list.name.clone()),
                Cell::Number(task.task_number as f64),
                text(
                    task.parent_id
                        .as_ref()
                        .and_then(|parent| parent_titles.get(parent).cloned())
                        .unwrap_or_default(),
                ),
                text(task.title.clone()),
                text(export_status_label(&task.status, &workflow)),
                text(
                    category_label(status_category(&task.status, Some(&workflow)).as_deref())
                        .to_string(),
                ),
                Cell::Text(if task.archived { "보관됨" } else { "활성" }.to_string()),
                text(priority_label(&task.priority)),
                text(
                    context
                        .assignee_ids(task)
                        .iter()
                        .map(|id| name(id))
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
                text(name(&task.reporter_id)),
                text(
                    task.milestone_id
                        .as_ref()
                        .and_then(|id| context.milestones.get(id).cloned())
                        .unwrap_or_default(),
                ),
                task.start_date.map_or(Cell::Empty, Cell::Date),
                task.due_date.map_or(Cell::Empty, Cell::Date),
                overdue.map_or(Cell::Empty, |days| Cell::Number(days as f64)),
                task.completed_date.map_or(Cell::Empty, Cell::Date),
                text(labels.join(", ")),
                Cell::Number(checklist_done as f64),
                Cell::Number(checklist_total as f64),
                Cell::Number(context.comments.get(&task.id).copied().unwrap_or(0) as f64),
                Cell::DateTime(local(&task.created_at)),
                Cell::DateTime(local(&task.updated_at)),
            ];
            (*depth, task.title.contains('\n'), cells)
        })
        .collect();
    let bytes = render_workbook(&rows).map_err(|error| {
        eprintln!("export_render_failed: {error}");
        ApiError::internal()
    })?;
    let filename = export_filename(&list.name, &list.key, filtered);
    let mut response = (StatusCode::OK, bytes).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    if let Ok(value) = HeaderValue::from_str(&format!(
        "attachment; filename=\"tasks_export.xlsx\"; filename*=UTF-8''{}",
        percent_encode(&filename)
    )) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    Ok(response)
}

fn render_workbook(
    rows: &[(usize, bool, Vec<Cell>)],
) -> Result<Vec<u8>, rust_xlsxwriter::XlsxError> {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.set_name("태스크")?;
    let header = Format::new()
        .set_bold()
        .set_align(FormatAlign::VerticalCenter);
    let date = Format::new().set_num_format(DATE_FORMAT);
    let datetime = Format::new().set_num_format(DATETIME_FORMAT);
    let title_column = COLUMNS.iter().position(|c| *c == "제목").unwrap_or(5) as u16;
    let mut widths: Vec<usize> = COLUMNS.iter().map(|c| display_width(c)).collect();
    for (index, column) in COLUMNS.iter().enumerate() {
        sheet.write_string_with_format(0, index as u16, *column, &header)?;
    }
    for (row_index, (depth, wrapped, cells)) in rows.iter().enumerate() {
        let row = row_index as u32 + 1;
        for (column, cell) in cells.iter().enumerate() {
            let col = column as u16;
            match cell {
                Cell::Text(value) => {
                    // A name like "=HYPERLINK(...)" stays text, never a formula.
                    if col == title_column {
                        let mut format = Format::new()
                            .set_indent(*depth as u8)
                            .set_align(FormatAlign::VerticalCenter);
                        if *wrapped {
                            format = format.set_text_wrap();
                        }
                        sheet.write_string_with_format(row, col, value, &format)?;
                    } else {
                        sheet.write_string(row, col, value)?;
                    }
                    widths[column] = widths[column].max(display_width(value));
                }
                Cell::Number(value) => {
                    sheet.write_number(row, col, *value)?;
                    widths[column] = widths[column].max(format_number(*value).len());
                }
                Cell::Date(value) => {
                    sheet.write_date_with_format(row, col, value, &date)?;
                }
                Cell::DateTime(value) => {
                    sheet.write_datetime_with_format(row, col, value, &datetime)?;
                }
                Cell::Empty => {}
            }
        }
    }
    for (index, column) in COLUMNS.iter().enumerate() {
        let ceiling = if WIDE_COLUMNS.contains(column) {
            MAX_WIDE_WIDTH
        } else {
            MAX_WIDTH
        };
        let mut widest = widths[index];
        if column.ends_with("일시") {
            widest = widest.max(DATETIME_FORMAT.len());
        } else if column.ends_with('일') {
            widest = widest.max(DATE_FORMAT.len());
        }
        let width = (widest as f64 + 2.5).max(MIN_WIDTH).min(ceiling);
        sheet.set_column_width(index as u16, width)?;
    }
    sheet.set_freeze_panes(1, 0)?;
    sheet.autofilter(0, 0, rows.len() as u32, COLUMNS.len() as u16 - 1)?;
    workbook.save_to_buffer()
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn korean_takes_two_cells() {
        assert_eq!(display_width("제목"), 4);
        assert_eq!(display_width("ab\n가나다"), 6);
    }

    #[test]
    fn filename_uses_the_list_name() {
        assert_eq!(
            export_filename("주간: 계획", "LS", false),
            "주간  계획.xlsx"
        );
        assert_eq!(export_filename("  ", "LS1", true), "LS1 (필터 적용).xlsx");
    }

    #[test]
    fn workbook_renders() {
        let rows = vec![(
            0,
            false,
            vec![Cell::Text("=HYPERLINK(\"x\")".into()), Cell::Number(1.0)],
        )];
        let bytes = render_workbook(&rows).unwrap();
        assert!(bytes.starts_with(b"PK"));
    }
}
