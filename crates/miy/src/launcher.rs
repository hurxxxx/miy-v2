//! Launcher categories an administrator arranges, ported from AI-DO's admin
//! app bar categories (`/admin/app-bar-categories`: list, create, update,
//! delete, layout). Categories only group the apps a person can already open;
//! the launcher still shows apps in no category, and nothing here grants use.
use crate::{
    State,
    api_contract::{LauncherCategory, LauncherCategoryPage},
    auth::Principal,
    http::{ApiError, ApiResult},
    policy::{Decided, PlatformAction, Write},
};
use serde::Deserialize;
use sqlx::{PgConnection, Row};
use std::collections::HashSet;
use uuid::Uuid;

const MAX_CATEGORIES: usize = 100;
const MAX_APPS_PER_CATEGORY: usize = 200;

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewCategory {
    /// 1-120 characters.
    pub title: String,
    /// A launcher icon name: lowercase letters, digits and hyphens.
    pub icon_key: String,
    /// Display position; omitted puts the category last.
    pub position: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CategoryUpdate {
    pub title: Option<String>,
    pub icon_key: Option<String>,
    pub position: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutCategory {
    pub id: Uuid,
    pub title: String,
    pub icon_key: String,
    /// The category's apps in display order.
    #[serde(default)]
    pub app_ids: Vec<String>,
}

/// The whole arrangement at once, in display order. Categories left out are
/// deleted; an app may appear in one category only.
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Layout {
    pub categories: Vec<LayoutCategory>,
    pub expected_revision: String,
}

fn title(value: &str) -> ApiResult<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 120 || value.chars().any(char::is_control) {
        return Err(ApiError::bad("invalid_category_title"));
    }
    Ok(value.to_owned())
}

fn icon_key(value: &str) -> ApiResult<String> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.len() <= 64
        && value.starts_with(|c: char| c.is_ascii_lowercase())
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if !valid {
        return Err(ApiError::bad("invalid_category_icon"));
    }
    Ok(value.to_owned())
}

async fn read(connection: &mut PgConnection) -> ApiResult<LauncherCategoryPage> {
    let rows = sqlx::query(
        "SELECT c.id,c.title,c.icon_key,c.position,c.updated_at,\
         COALESCE(array_agg(a.app_id ORDER BY a.position,a.app_id) FILTER (WHERE a.app_id IS NOT NULL),'{}') AS app_ids \
         FROM launcher_categories c LEFT JOIN launcher_category_apps a ON a.category_id=c.id \
         GROUP BY c.id ORDER BY c.position,c.created_at,c.id",
    )
    .fetch_all(&mut *connection)
    .await?;
    let categories: Vec<_> = rows
        .iter()
        .map(|row| LauncherCategory {
            id: row.get("id"),
            title: row.get("title"),
            icon_key: row.get("icon_key"),
            position: row.get::<i32, _>("position").max(0) as u32,
            app_ids: row.get("app_ids"),
            updated_at: row.get("updated_at"),
        })
        .collect();
    let revision = crate::contract::hash(serde_json::to_vec(&categories)?);
    Ok(LauncherCategoryPage {
        categories,
        revision,
    })
}

/// Every signed-in person reads the arrangement: it names app IDs and titles
/// only, and the portal shows just the apps that person can open.
pub async fn list(state: &State, p: &Principal) -> ApiResult<LauncherCategoryPage> {
    crate::delegations::unrestricted(p)?;
    let mut connection = state.pool.acquire().await?;
    read(&mut connection).await
}

/// Category changes are serialized so positions and app assignments stay whole.
async fn lock(tx: &mut PgConnection) -> sqlx::Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('launcher-categories',0))")
        .execute(tx)
        .await?;
    Ok(())
}

/// Audit and answer in the same transaction as the change.
async fn finish(
    p: &Principal,
    mut tx: Decided,
    action: &str,
    target: &str,
) -> ApiResult<LauncherCategoryPage> {
    crate::audit::record(&mut tx, &p.id, action, target).await?;
    let page = read(&mut tx).await?;
    tx.commit().await?;
    Ok(page)
}

pub async fn create(
    state: &State,
    p: &Principal,
    input: NewCategory,
) -> ApiResult<LauncherCategoryPage> {
    let (title, icon_key) = (title(&input.title)?, icon_key(&input.icon_key)?);
    let mut tx = Write::begin(&state.pool).await?;
    lock(&mut tx).await?;
    // Decided before any read that could tell a non-administrator which apps
    // or categories exist; the lock is already held.
    let mut tx = tx.decide_platform(p, PlatformAction::Administer).await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM launcher_categories")
        .fetch_one(&mut *tx)
        .await?;
    if count as usize >= MAX_CATEGORIES {
        return Err(ApiError::conflict("category_limit"));
    }
    let position = input.position.map_or(count, i64::from).clamp(0, count);
    sqlx::query("UPDATE launcher_categories SET position=position+1 WHERE position>=$1")
        .bind(position as i32)
        .execute(&mut *tx)
        .await?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO launcher_categories(id,title,icon_key,position) VALUES($1,$2,$3,$4)")
        .bind(id)
        .bind(&title)
        .bind(&icon_key)
        .bind(position as i32)
        .execute(&mut *tx)
        .await?;
    finish(
        p,
        tx,
        "launcher.category_created",
        &format!("launcher-category:{id}"),
    )
    .await
}

pub async fn update(
    state: &State,
    p: &Principal,
    id: Uuid,
    input: CategoryUpdate,
) -> ApiResult<LauncherCategoryPage> {
    if input.title.is_none() && input.icon_key.is_none() && input.position.is_none() {
        return Err(ApiError::bad("no_category_change"));
    }
    let title = input.title.as_deref().map(title).transpose()?;
    let icon_key = input.icon_key.as_deref().map(icon_key).transpose()?;
    let mut tx = Write::begin(&state.pool).await?;
    lock(&mut tx).await?;
    // Decided before any read that could tell a non-administrator which apps
    // or categories exist; the lock is already held.
    let mut tx = tx.decide_platform(p, PlatformAction::Administer).await?;
    let ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM launcher_categories ORDER BY position,created_at,id")
            .fetch_all(&mut *tx)
            .await?;
    let Some(from) = ids.iter().position(|value| *value == id) else {
        return Err(ApiError::not_found());
    };
    if let Some(to) = input.position {
        let mut order = ids.clone();
        let moved = order.remove(from);
        order.insert((to as usize).min(order.len()), moved);
        reorder(&mut tx, &order).await?;
    }
    sqlx::query(
        "UPDATE launcher_categories SET title=COALESCE($2,title),icon_key=COALESCE($3,icon_key),\
         updated_at=clock_timestamp() WHERE id=$1",
    )
    .bind(id)
    .bind(&title)
    .bind(&icon_key)
    .execute(&mut *tx)
    .await?;
    finish(
        p,
        tx,
        "launcher.category_updated",
        &format!("launcher-category:{id}"),
    )
    .await
}

async fn reorder(tx: &mut PgConnection, order: &[Uuid]) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE launcher_categories c SET position=o.position-1 \
         FROM unnest($1::uuid[]) WITH ORDINALITY AS o(id,position) WHERE c.id=o.id",
    )
    .bind(order)
    .execute(tx)
    .await?;
    Ok(())
}

pub async fn delete(state: &State, p: &Principal, id: Uuid) -> ApiResult<LauncherCategoryPage> {
    let mut tx = Write::begin(&state.pool).await?;
    lock(&mut tx).await?;
    // Decided before any read that could tell a non-administrator which apps
    // or categories exist; the lock is already held.
    let mut tx = tx.decide_platform(p, PlatformAction::Administer).await?;
    let deleted = sqlx::query("DELETE FROM launcher_categories WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if deleted.rows_affected() == 0 {
        return Err(ApiError::not_found());
    }
    let ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM launcher_categories ORDER BY position,created_at,id")
            .fetch_all(&mut *tx)
            .await?;
    reorder(&mut tx, &ids).await?;
    finish(
        p,
        tx,
        "launcher.category_deleted",
        &format!("launcher-category:{id}"),
    )
    .await
}

pub async fn layout(
    state: &State,
    p: &Principal,
    input: Layout,
) -> ApiResult<LauncherCategoryPage> {
    if input.categories.len() > MAX_CATEGORIES {
        return Err(ApiError::bad("category_limit"));
    }
    let mut ids = HashSet::new();
    let mut apps = HashSet::new();
    let mut categories = Vec::with_capacity(input.categories.len());
    for category in &input.categories {
        if !ids.insert(category.id)
            || category.app_ids.len() > MAX_APPS_PER_CATEGORY
            || !category
                .app_ids
                .iter()
                .all(|app| crate::contract::valid_id(app) && apps.insert(app.as_str()))
        {
            return Err(ApiError::bad("invalid_category_layout"));
        }
        categories.push((
            category.id,
            title(&category.title)?,
            icon_key(&category.icon_key)?,
        ));
    }
    let mut tx = Write::begin(&state.pool).await?;
    lock(&mut tx).await?;
    // Decided before any read that could tell a non-administrator which apps
    // or categories exist; the lock is already held.
    let mut tx = tx.decide_platform(p, PlatformAction::Administer).await?;
    if read(&mut tx).await?.revision != input.expected_revision {
        return Err(ApiError::conflict("launcher_layout_changed"));
    }
    let app_list: Vec<&str> = apps.iter().copied().collect();
    let known: i64 = sqlx::query_scalar("SELECT count(*) FROM apps WHERE id=ANY($1)")
        .bind(&app_list)
        .fetch_one(&mut *tx)
        .await?;
    if known as usize != app_list.len() {
        return Err(ApiError::bad("unknown_app"));
    }
    let keep: Vec<Uuid> = categories.iter().map(|(id, _, _)| *id).collect();
    sqlx::query("DELETE FROM launcher_categories WHERE NOT (id=ANY($1))")
        .bind(&keep)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM launcher_category_apps")
        .execute(&mut *tx)
        .await?;
    for (position, (id, title, icon_key)) in categories.iter().enumerate() {
        sqlx::query(
            "INSERT INTO launcher_categories(id,title,icon_key,position) VALUES($1,$2,$3,$4) \
             ON CONFLICT(id) DO UPDATE SET title=excluded.title,icon_key=excluded.icon_key,\
             position=excluded.position,updated_at=clock_timestamp()",
        )
        .bind(id)
        .bind(title)
        .bind(icon_key)
        .bind(position as i32)
        .execute(&mut *tx)
        .await?;
    }
    for category in &input.categories {
        sqlx::query(
            "INSERT INTO launcher_category_apps(app_id,category_id,position) \
             SELECT app,$1,position-1 FROM unnest($2::text[]) WITH ORDINALITY AS a(app,position)",
        )
        .bind(category.id)
        .bind(&category.app_ids)
        .execute(&mut *tx)
        .await?;
    }
    finish(p, tx, "launcher.layout_set", "launcher-categories").await
}

#[cfg(test)]
mod tests {
    #[test]
    fn titles_and_icons_are_bounded() {
        assert_eq!(super::title("  업무 지원 ").unwrap(), "업무 지원");
        assert!(super::title("").is_err());
        assert!(super::title(&"가".repeat(121)).is_err());
        assert_eq!(super::icon_key("layout-grid").unwrap(), "layout-grid");
        assert!(super::icon_key("Layout").is_err());
        assert!(super::icon_key("1grid").is_err());
        assert!(super::icon_key("../x").is_err());
    }
}
