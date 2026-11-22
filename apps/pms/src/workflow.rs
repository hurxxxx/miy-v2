//! Status workflow rules: AI-DO domains/pms/workflow.py, the defaults of
//! status_lifecycle.py and task_update_plan.py, unchanged.
use serde::Serialize;

pub const TASK_STATUS_LABELS: &[(&str, &str)] = &[
    ("todo", "Todo"),
    ("in_progress", "In Progress"),
    ("review", "Review"),
    ("done", "Done"),
    ("canceled", "Canceled"),
    ("complete", "Complete"),
];

/// `TASK_STATUS_PROGRESS`; `None` inside means "no progress value" (canceled, complete).
const TASK_STATUS_PROGRESS: &[(&str, Option<f64>)] = &[
    ("todo", Some(0.0)),
    ("in_progress", Some(0.5)),
    ("review", Some(0.75)),
    ("done", Some(1.0)),
    ("canceled", None),
    ("complete", None),
];

pub const PRIORITY_LABELS: &[(&str, &str)] = &[
    ("low", "Low"),
    ("medium", "Medium"),
    ("high", "High"),
    ("critical", "Critical"),
];

pub const PRIORITIES: &[&str] = &["low", "medium", "high", "critical"];
pub const STATUS_CATEGORIES: &[&str] = &["not_started", "active", "done", "closed"];

/// `DEFAULT_TASK_LIST_STATUSES`: slug, name, color, category, sort order.
pub const DEFAULT_TASK_LIST_STATUSES: &[(&str, &str, &str, &str, i32)] = &[
    ("todo", "To Do", "#9ca3af", "not_started", 0),
    ("in_progress", "In Progress", "#3b82f6", "active", 1),
    ("review", "Review", "#8b5cf6", "active", 2),
    ("done", "Done", "#22c55e", "done", 3),
    ("complete", "Complete", "#16a34a", "closed", 4),
];

/// A configured status (pms_space_statuses or pms_task_list_statuses).
#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct StatusDef {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub color: String,
    pub category: String,
    pub sort_order: i32,
}

/// What the workflow functions read from a task list.
#[derive(Debug, Clone, Default)]
pub struct ListWorkflow {
    pub status_mode: String,
    pub team_id: Option<String>,
    /// Ordered by sort_order.
    pub list_statuses: Vec<StatusDef>,
    /// Ordered by sort_order.
    pub space_statuses: Vec<StatusDef>,
}

impl ListWorkflow {
    /// `status_definitions(task_list)`.
    pub fn definitions(&self) -> &[StatusDef] {
        if self.status_mode == "inherit" && self.team_id.is_some() {
            &self.space_statuses
        } else {
            &self.list_statuses
        }
    }
}

fn lookup<T: Copy>(table: &[(&str, T)], key: &str) -> Option<T> {
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// Python `str.replace("_", " ").title()`.
fn title_case(value: &str) -> String {
    let replaced = value.replace('_', " ");
    let mut out = String::with_capacity(replaced.len());
    let mut previous_cased = false;
    for ch in replaced.chars() {
        if ch.is_alphabetic() {
            if previous_cased {
                out.extend(ch.to_lowercase());
            } else {
                out.extend(ch.to_uppercase());
            }
            previous_cased = true;
        } else {
            out.push(ch);
            previous_cased = false;
        }
    }
    out
}

pub fn normalize_task_status(status: &str) -> String {
    if status == "backlog" {
        "todo".into()
    } else {
        status.into()
    }
}

pub fn normalize_status_category(category: &str) -> String {
    if category == "canceled" {
        "closed".into()
    } else {
        category.into()
    }
}

/// `status_slug(name)`: lowercased, spaces to underscores, at most 40 characters.
pub fn status_slug(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .replace(' ', "_")
        .chars()
        .take(40)
        .collect()
}

pub fn status_label(status: &str, list: Option<&ListWorkflow>) -> String {
    if let Some(list) = list {
        for definition in list.definitions() {
            if definition.slug == status {
                return definition.name.clone();
            }
        }
    }
    lookup(TASK_STATUS_LABELS, status)
        .map(str::to_string)
        .unwrap_or_else(|| title_case(status))
}

pub fn status_category(status: &str, list: Option<&ListWorkflow>) -> Option<String> {
    match status {
        "todo" => return Some("not_started".into()),
        "in_progress" => return Some("active".into()),
        "done" => return Some("done".into()),
        "canceled" | "closed" | "complete" => return Some("closed".into()),
        _ => {}
    }
    list?
        .definitions()
        .iter()
        .find(|definition| definition.slug == status)
        .map(|definition| normalize_status_category(&definition.category))
}

pub fn task_progress(status: &str, list: Option<&ListWorkflow>) -> Option<f64> {
    if let Some(progress) = lookup(TASK_STATUS_PROGRESS, status) {
        return progress;
    }
    match status_category(status, list).as_deref() {
        Some("not_started") => Some(0.0),
        Some("active") => Some(0.5),
        Some("done") => Some(1.0),
        Some("closed") => None,
        Some(_) => Some(0.5),
        None => Some(0.5),
    }
}

#[allow(dead_code)]
pub fn is_closed_status(status: &str, list: Option<&ListWorkflow>) -> bool {
    status_category(status, list).as_deref() == Some("closed")
}

pub fn is_done_status(status: &str, list: Option<&ListWorkflow>) -> bool {
    status_category(status, list).as_deref() == Some("done")
}

pub fn is_completion_status(status: &str, list: Option<&ListWorkflow>) -> bool {
    matches!(
        status_category(status, list).as_deref(),
        Some("done" | "closed")
    )
}

pub fn is_overdue_exempt_status(status: &str, list: Option<&ListWorkflow>) -> bool {
    is_completion_status(status, list)
}

/// `violates_single_closed_status_rule`.
pub fn violates_single_closed_status_rule(
    statuses: &[StatusDef],
    next_category: &str,
    exclude_id: Option<&str>,
) -> bool {
    normalize_status_category(next_category) == "closed"
        && statuses.iter().any(|item| {
            Some(item.id.as_str()) != exclude_id
                && normalize_status_category(&item.category) == "closed"
        })
}

/// `incompatible_statuses_for_inherit`: sorted, unique.
pub fn incompatible_statuses_for_inherit(used: &[String], allowed: &[String]) -> Vec<String> {
    let mut result: Vec<String> = used
        .iter()
        .filter(|status| !allowed.contains(status))
        .cloned()
        .collect();
    result.sort();
    result.dedup();
    result
}

/// `calculate_progress(tasks, task_list)` over (status, archived) pairs.
pub fn calculate_progress<'a>(
    tasks: impl IntoIterator<Item = (&'a str, bool)>,
    list: Option<&ListWorkflow>,
) -> f64 {
    let values: Vec<f64> = tasks
        .into_iter()
        .filter(|(_, archived)| !archived)
        .filter_map(|(status, _)| task_progress(status, list))
        .collect();
    if values.is_empty() {
        return 0.0;
    }
    round2(values.iter().sum::<f64>() / values.len() as f64)
}

/// Python `round(x, 2)` (round half to even on the binary value).
pub fn round2(value: f64) -> f64 {
    let scaled = value * 100.0;
    let floor = scaled.floor();
    let diff = scaled - floor;
    let rounded = if (diff - 0.5).abs() < 1e-9 {
        if floor % 2.0 == 0.0 {
            floor
        } else {
            floor + 1.0
        }
    } else {
        scaled.round()
    };
    rounded / 100.0
}

pub fn priority_label(priority: &str) -> String {
    lookup(PRIORITY_LABELS, priority)
        .map(str::to_string)
        .unwrap_or_else(|| title_case(priority))
}

/// `_auto_key_from_name`: initials of the ASCII words (NFKD, non-ASCII
/// dropped), else the first six ASCII characters, else "LS".
pub fn auto_key_from_name(name: &str) -> String {
    // NFKD then `encode("ascii", "ignore")`: accents drop, other non-ASCII
    // characters vanish. AI-DO's class `[^A-Za-z0-9\\s]` (a raw string, so a
    // literal backslash and the letter s) then keeps letters, digits and
    // backslashes only: whitespace is removed too. Reproduced exactly.
    let cleaned: String = name
        .chars()
        .filter_map(strip_accent)
        .filter(|c| c.is_ascii_alphanumeric() || *c == '\\')
        .collect();
    let cleaned = cleaned.trim().to_string();
    if !cleaned.is_empty() {
        let upper = cleaned.to_uppercase();
        let key: String = upper
            .split_whitespace()
            .filter_map(|word| word.chars().next())
            .take(6)
            .collect();
        if key.chars().count() >= 2 {
            return key;
        }
        return cleaned.chars().take(6).collect::<String>().to_uppercase();
    }
    "LS".into()
}

/// The ASCII base letter of a character after NFKD, or the character itself
/// when it is ASCII; `None` for characters that have no ASCII decomposition.
fn strip_accent(ch: char) -> Option<char> {
    if ch.is_ascii() {
        return Some(ch);
    }
    const MAP: &[(&str, char)] = &[
        ("ÀÁÂÃÄÅĀĂĄǍ", 'A'),
        ("àáâãäåāăąǎ", 'a'),
        ("ÇĆĈĊČ", 'C'),
        ("çćĉċč", 'c'),
        ("ĎḌ", 'D'),
        ("ďḍ", 'd'),
        ("ÈÉÊËĒĔĖĘĚ", 'E'),
        ("èéêëēĕėęě", 'e'),
        ("ĜĞĠĢ", 'G'),
        ("ĝğġģ", 'g'),
        ("ĤḤ", 'H'),
        ("ĥḥ", 'h'),
        ("ÌÍÎÏĨĪĬĮİǏ", 'I'),
        ("ìíîïĩīĭįǐ", 'i'),
        ("Ĵ", 'J'),
        ("ĵ", 'j'),
        ("Ķ", 'K'),
        ("ķ", 'k'),
        ("ĹĻĽ", 'L'),
        ("ĺļľ", 'l'),
        ("ÑŃŅŇ", 'N'),
        ("ñńņň", 'n'),
        ("ÒÓÔÕÖŌŎŐǑ", 'O'),
        ("òóôõöōŏőǒ", 'o'),
        ("ŔŖŘ", 'R'),
        ("ŕŗř", 'r'),
        ("ŚŜŞŠ", 'S'),
        ("śŝşš", 's'),
        ("ŢŤ", 'T'),
        ("ţť", 't'),
        ("ÙÚÛÜŨŪŬŮŰŲǓ", 'U'),
        ("ùúûüũūŭůűųǔ", 'u'),
        ("Ŵ", 'W'),
        ("ŵ", 'w'),
        ("ÝŶŸ", 'Y'),
        ("ýÿŷ", 'y'),
        ("ŹŻŽ", 'Z'),
        ("źżž", 'z'),
    ];
    // Fullwidth ASCII (NFKD maps U+FF01..U+FF5E to ASCII).
    if ('\u{FF01}'..='\u{FF5E}').contains(&ch) {
        return char::from_u32(ch as u32 - 0xFF01 + 0x21);
    }
    MAP.iter()
        .find(|(chars, _)| chars.contains(ch))
        .map(|(_, base)| *base)
}

/// `TASK_UPDATE_SCALAR_FIELD_SPECS`: field and its activity message.
pub const TASK_UPDATE_SCALAR_FIELDS: &[(&str, &str)] = &[
    ("title", "updated title"),
    ("description", "updated description"),
    ("status", "changed status"),
    ("priority", "changed priority"),
    ("assignee_id", "changed assignee"),
    ("milestone_id", "changed milestone"),
    ("start_date", "updated start date"),
    ("due_date", "updated due date"),
    ("completed_date", "updated completion date"),
    ("board_position", "reordered board position"),
    ("archived", "changed archive state"),
    ("recurrence_rule", "updated recurrence"),
];

pub const TASK_UPDATE_NULLABLE_FIELDS: &[&str] = &[
    "assignee_id",
    "milestone_id",
    "start_date",
    "due_date",
    "completed_date",
    "recurrence_rule",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn status(id: &str, slug: &str, name: &str, category: &str, order: i32) -> StatusDef {
        StatusDef {
            id: id.into(),
            slug: slug.into(),
            name: name.into(),
            color: "#6b7280".into(),
            category: category.into(),
            sort_order: order,
        }
    }

    fn custom(statuses: Vec<StatusDef>) -> ListWorkflow {
        ListWorkflow {
            status_mode: "custom".into(),
            team_id: Some("space".into()),
            list_statuses: statuses,
            space_statuses: vec![],
        }
    }

    // test_pms_workflow.py
    #[test]
    fn builtin_status_categories_and_labels() {
        assert_eq!(
            status_category("todo", None).as_deref(),
            Some("not_started")
        );
        assert_eq!(
            status_category("in_progress", None).as_deref(),
            Some("active")
        );
        assert_eq!(status_category("done", None).as_deref(), Some("done"));
        assert_eq!(status_category("canceled", None).as_deref(), Some("closed"));
        assert_eq!(status_category("complete", None).as_deref(), Some("closed"));
        assert_eq!(status_category("unknown", None), None);
        assert_eq!(status_label("in_progress", None), "In Progress");
        assert_eq!(status_label("waiting_on_vendor", None), "Waiting On Vendor");
        assert_eq!(normalize_task_status("backlog"), "todo");
        assert_eq!(normalize_status_category("canceled"), "closed");
    }

    #[test]
    fn custom_statuses_define_label_category_and_progress() {
        let list = custom(vec![
            status("1", "qa", "QA", "active", 0),
            status("2", "shipped", "Shipped", "done", 1),
            status("3", "dropped", "Dropped", "canceled", 2),
        ]);
        assert_eq!(status_label("qa", Some(&list)), "QA");
        assert_eq!(
            status_category("shipped", Some(&list)).as_deref(),
            Some("done")
        );
        assert_eq!(
            status_category("dropped", Some(&list)).as_deref(),
            Some("closed")
        );
        assert_eq!(task_progress("qa", Some(&list)), Some(0.5));
        assert_eq!(task_progress("shipped", Some(&list)), Some(1.0));
        assert_eq!(task_progress("dropped", Some(&list)), None);
        assert_eq!(task_progress("nowhere", Some(&list)), Some(0.5));
        assert!(is_completion_status("shipped", Some(&list)));
        assert!(is_overdue_exempt_status("dropped", Some(&list)));
        assert!(!is_overdue_exempt_status("qa", Some(&list)));
    }

    #[test]
    fn inherited_lists_read_the_space_workflow() {
        let list = ListWorkflow {
            status_mode: "inherit".into(),
            team_id: Some("space".into()),
            list_statuses: vec![status("1", "qa", "List QA", "active", 0)],
            space_statuses: vec![status("2", "qa", "Space QA", "done", 0)],
        };
        assert_eq!(status_label("qa", Some(&list)), "Space QA");
        assert!(is_done_status("qa", Some(&list)));
    }

    #[test]
    fn progress_skips_archived_and_closed_tasks() {
        assert_eq!(calculate_progress(Vec::<(&str, bool)>::new(), None), 0.0);
        assert_eq!(
            calculate_progress(
                [
                    ("todo", false),
                    ("done", false),
                    ("done", true),
                    ("canceled", false)
                ],
                None
            ),
            0.5
        );
        assert_eq!(
            calculate_progress(
                [("review", false), ("in_progress", false), ("todo", false)],
                None
            ),
            0.42
        );
    }

    #[test]
    fn single_closed_status_rule() {
        let statuses = vec![
            status("1", "todo", "To Do", "not_started", 0),
            status("2", "complete", "Complete", "closed", 1),
        ];
        assert!(violates_single_closed_status_rule(
            &statuses, "closed", None
        ));
        assert!(violates_single_closed_status_rule(
            &statuses, "canceled", None
        ));
        assert!(!violates_single_closed_status_rule(
            &statuses,
            "closed",
            Some("2")
        ));
        assert!(!violates_single_closed_status_rule(
            &statuses, "active", None
        ));
    }

    #[test]
    fn inherit_compatibility_lists_unknown_statuses_sorted() {
        let used = vec![
            "qa".to_string(),
            "todo".into(),
            "blocked".into(),
            "qa".into(),
        ];
        let allowed = vec!["todo".to_string(), "done".into()];
        assert_eq!(
            incompatible_statuses_for_inherit(&used, &allowed),
            vec!["blocked".to_string(), "qa".into()]
        );
    }

    #[test]
    fn status_slug_matches_ai_do() {
        assert_eq!(status_slug("  Waiting On Vendor "), "waiting_on_vendor");
        assert_eq!(status_slug("검토 중"), "검토_중");
        assert_eq!(status_slug(&"x".repeat(50)).len(), 40);
    }

    #[test]
    fn task_list_keys_from_names() {
        assert_eq!(auto_key_from_name("List"), "LIST");
        assert_eq!(
            auto_key_from_name("Mobile App"),
            "MOBILEAPP".chars().take(6).collect::<String>()
        );
        assert_eq!(auto_key_from_name("한글 목록"), "LS");
        assert_eq!(auto_key_from_name("Café"), "CAFE");
        assert_eq!(auto_key_from_name("a"), "A");
    }

    #[test]
    fn python_rounding() {
        assert_eq!(round2(0.4166666), 0.42);
        assert_eq!(round2(0.125), 0.12);
        assert_eq!(round2(0.5), 0.5);
    }
}
