//! A person's own settings: the display settings the portal applies and passes
//! to app frames, the job title shown with their name, and the launcher pins.
//! Presentation only; no access decision reads them.
//!
//! The fields and limits follow AI-DO's `PATCH /auth/preferences`
//! (display_name, job_title, theme_preference, locale, time_zone, date_format,
//! app_bar_layout.pinned_app_ids) in this API's camelCase form.
use crate::{
    State,
    api_contract::{DateFormat, Locale, Preferences, ThemePreference},
    auth::Principal,
    http::{ApiError, ApiResult},
    policy::{PlatformAction, Write},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use std::collections::HashSet;

/// Launcher pins kept per person (the column's check uses the same bound).
const MAX_PINNED_APPS: usize = 64;

/// Omitted fields keep their value.
#[derive(Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Update {
    /// Display name, 1-120 characters.
    pub name: Option<String>,
    /// Job title, up to 120 characters. An empty string clears it.
    pub job_title: Option<String>,
    pub theme: Option<ThemePreference>,
    pub locale: Option<Locale>,
    /// IANA time zone name.
    pub time_zone: Option<String>,
    pub date_format: Option<DateFormat>,
    /// App IDs pinned to the launcher, in display order. At most 64, no repeats.
    pub pinned_apps: Option<Vec<String>>,
    /// Side panels pinned to the header as `appId/panelId`, in order. At most 16.
    pub pinned_panels: Option<Vec<String>>,
    /// Set one pin against the current list without overwriting other tabs.
    pub panel_pin: Option<PanelPin>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PanelPin {
    id: String,
    pinned: bool,
}

/// One of the closed enums as its JSON text, the form the table stores.
fn text<T: serde::Serialize>(value: T) -> String {
    match serde_json::to_value(value) {
        Ok(Value::String(text)) => text,
        _ => unreachable!("preference enums serialize as strings"),
    }
}

/// A stored value the table's check constraint already limits; an unknown
/// value (only possible after a manual edit) reads as the default.
fn parse<T: serde::de::DeserializeOwned + Default>(value: &str) -> T {
    serde_json::from_value(Value::String(value.to_owned())).unwrap_or_default()
}

fn defaults() -> Preferences {
    let frame = crate::api_contract::FrameSettings::default();
    Preferences {
        theme: frame.theme,
        locale: frame.locale,
        time_zone: frame.time_zone,
        date_format: frame.date_format,
        job_title: None,
        pinned_apps: Vec::new(),
        pinned_panels: Vec::new(),
    }
}

pub(crate) async fn read(pool: &PgPool, user: &str) -> sqlx::Result<Preferences> {
    let row = sqlx::query(
        "SELECT theme,locale,time_zone,date_format,job_title,pinned_apps,pinned_panels \
         FROM user_preferences WHERE user_id=$1",
    )
    .bind(user)
    .fetch_optional(pool)
    .await?;
    Ok(row.map_or_else(defaults, |row| Preferences {
        theme: parse(row.get("theme")),
        locale: parse(row.get("locale")),
        time_zone: row.get("time_zone"),
        date_format: parse(row.get("date_format")),
        job_title: row.get("job_title"),
        pinned_apps: row.get("pinned_apps"),
        pinned_panels: row.get("pinned_panels"),
    }))
}

/// `Some(None)` clears the title; `None` leaves it.
fn job_title(value: Option<&str>) -> ApiResult<Option<Option<String>>> {
    match value.map(str::trim) {
        None => Ok(None),
        Some("") => Ok(Some(None)),
        Some(title) if title.chars().count() <= 120 && !title.chars().any(char::is_control) => {
            Ok(Some(Some(title.to_owned())))
        }
        Some(_) => Err(ApiError::bad("invalid_job_title")),
    }
}

fn pinned_panels(value: &[String]) -> bool {
    let mut seen = HashSet::new();
    value.len() <= 16
        && value
            .iter()
            .all(|entry| panel_key(entry) && seen.insert(entry))
}

fn panel_key(entry: &str) -> bool {
    entry.split_once('/').is_some_and(|(app, panel)| {
        crate::contract::valid_id(app) && crate::contract::valid_id(panel)
    })
}

fn pinned_apps(value: &[String]) -> bool {
    let mut seen = HashSet::new();
    value.len() <= MAX_PINNED_APPS
        && value
            .iter()
            .all(|app| crate::contract::valid_id(app) && seen.insert(app))
}

pub async fn update(s: &State, p: &Principal, input: Update) -> ApiResult<Value> {
    let name = input
        .name
        .as_deref()
        .map(crate::user_admin::display_name)
        .transpose()?;
    let title = job_title(input.job_title.as_deref())?;
    if input
        .time_zone
        .as_deref()
        .is_some_and(|zone| !crate::app_frames::time_zone_name(zone))
    {
        return Err(ApiError::bad("invalid_time_zone"));
    }
    if input
        .pinned_apps
        .as_deref()
        .is_some_and(|apps| !pinned_apps(apps))
    {
        return Err(ApiError::bad("invalid_pinned_apps"));
    }
    if input
        .pinned_panels
        .as_deref()
        .is_some_and(|panels| !pinned_panels(panels))
        || input
            .panel_pin
            .as_ref()
            .is_some_and(|pin| !panel_key(&pin.id) || input.pinned_panels.is_some())
    {
        return Err(ApiError::bad("invalid_pinned_panels"));
    }
    let mut tx = Write::begin(&s.pool).await?;
    let old_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM users WHERE id=$1 FOR NO KEY UPDATE")
            .bind(&p.id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(old_name) = old_name else {
        return Err(ApiError::unauthorized());
    };
    sqlx::query("INSERT INTO user_preferences(user_id) VALUES($1) ON CONFLICT DO NOTHING")
        .bind(&p.id)
        .execute(&mut *tx)
        .await?;
    let mut panels = input.pinned_panels;
    if let Some(pin) = input.panel_pin {
        let mut current: Vec<String> =
            sqlx::query_scalar("SELECT pinned_panels FROM user_preferences WHERE user_id=$1")
                .bind(&p.id)
                .fetch_one(&mut *tx)
                .await?;
        if pin.pinned {
            if !current.contains(&pin.id) {
                current.push(pin.id);
            }
        } else {
            current.retain(|id| id != &pin.id);
        }
        if !pinned_panels(&current) {
            return Err(ApiError::bad("invalid_pinned_panels"));
        }
        panels = Some(current);
    }
    sqlx::query(
        "UPDATE user_preferences SET theme=COALESCE($2,theme),locale=COALESCE($3,locale),\
         time_zone=COALESCE($4,time_zone),date_format=COALESCE($5,date_format),\
         job_title=CASE WHEN $6 THEN $7 ELSE job_title END,pinned_apps=COALESCE($8,pinned_apps),\
         pinned_panels=COALESCE($9,pinned_panels),\
         updated_at=clock_timestamp() WHERE user_id=$1",
    )
    .bind(&p.id)
    .bind(input.theme.map(text))
    .bind(input.locale.map(text))
    .bind(&input.time_zone)
    .bind(input.date_format.map(text))
    .bind(title.is_some())
    .bind(title.flatten())
    .bind(&input.pinned_apps)
    .bind(&panels)
    .execute(&mut *tx)
    .await?;
    let renamed = name.as_ref().is_some_and(|name| *name != old_name);
    if renamed {
        sqlx::query("UPDATE users SET name=$2 WHERE id=$1")
            .bind(&p.id)
            .bind(&name)
            .execute(&mut *tx)
            .await?;
    }
    let mut tx = tx.decide_platform(p, PlatformAction::Personal).await?;
    if renamed {
        // The name is what colleagues see in the directory and in app screens.
        crate::audit::record(&mut tx, &p.id, "user.profile.rename", &p.id).await?;
    }
    tx.commit().await?;
    Ok(json!({
        "name": name.unwrap_or(old_name),
        "preferences": read(&s.pool, &p.id).await?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_text_matches_the_table_checks() {
        assert_eq!(text(ThemePreference::System), "system");
        assert_eq!(text(Locale::EnUs), "en-US");
        assert_eq!(text(DateFormat::European), "european");
        assert_eq!(parse::<Locale>("ko-KR"), Locale::KoKr);
        assert_eq!(parse::<ThemePreference>("sepia"), ThemePreference::System);
    }

    #[test]
    fn job_title_trims_clears_and_bounds() {
        assert_eq!(job_title(None).unwrap(), None);
        assert_eq!(job_title(Some("  ")).unwrap(), Some(None));
        assert_eq!(
            job_title(Some(" 책임 ")).unwrap(),
            Some(Some("책임".to_owned()))
        );
        assert!(job_title(Some(&"가".repeat(121))).is_err());
        assert!(job_title(Some("a\nb")).is_err());
    }

    #[test]
    fn pins_are_app_ids_without_repeats() {
        let ids = |values: &[&str]| values.iter().map(|v| v.to_string()).collect::<Vec<_>>();
        assert!(pinned_apps(&ids(&["planner", "messenger"])));
        assert!(pinned_apps(&[]));
        assert!(!pinned_apps(&ids(&["planner", "planner"])));
        assert!(!pinned_apps(&ids(&["Planner"])));
        let many: Vec<String> = (0..65).map(|i| format!("app-{i}")).collect();
        assert!(pinned_apps(&many[..64]));
        assert!(!pinned_apps(&many));
    }
}
