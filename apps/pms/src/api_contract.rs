use crate::{
    attachments, copy, custom_fields, export, home_card, lists, milestones, notify, resource_links,
    saved_filters, search, spaces, statuses, task_items, tasks, templates, users,
};
use axum::Router;
use std::{fs, path::Path};
use utoipa::{
    PartialSchema, ToSchema,
    openapi::{Info, OpenApi},
};
use utoipa_axum::{router::OpenApiRouter, routes};

pub(crate) fn router() -> (Router<sqlx::PgPool>, OpenApi) {
    let (router, mut spec) = OpenApiRouter::new()
        .routes(routes!(resource_links::list))
        .routes(routes!(resource_links::attach))
        .routes(routes!(resource_links::detach))
        .routes(routes!(users::me))
        .routes(routes!(users::list_users))
        .routes(routes!(spaces::list_spaces, spaces::create_space))
        .routes(routes!(spaces::update_space, spaces::delete_space))
        .routes(routes!(spaces::list_space_preferences))
        .routes(routes!(spaces::update_space_preference))
        .routes(routes!(
            spaces::get_view_preferences,
            spaces::update_view_preferences
        ))
        .routes(routes!(
            spaces::list_space_members,
            spaces::add_space_member
        ))
        .routes(routes!(spaces::add_space_members))
        .routes(routes!(
            spaces::update_space_member,
            spaces::remove_space_member
        ))
        .routes(routes!(
            statuses::list_space_statuses,
            statuses::create_space_status
        ))
        .routes(routes!(
            statuses::update_space_status,
            statuses::delete_space_status
        ))
        .routes(routes!(
            statuses::list_task_list_statuses,
            statuses::create_task_list_status
        ))
        .routes(routes!(statuses::update_task_list_status_mode))
        .routes(routes!(
            statuses::update_task_list_status,
            statuses::delete_task_list_status
        ))
        .routes(routes!(lists::list_folders, lists::create_folder))
        .routes(routes!(lists::update_folder, lists::delete_folder))
        .routes(routes!(lists::list_task_lists, lists::create_task_list))
        .routes(routes!(lists::list_space_lists))
        .routes(routes!(lists::reorder_space_lists))
        .routes(routes!(
            lists::get_task_list,
            lists::update_task_list,
            lists::delete_task_list
        ))
        .routes(routes!(
            milestones::list_milestones,
            milestones::create_milestone
        ))
        .routes(routes!(
            milestones::list_space_milestones,
            milestones::create_space_milestone
        ))
        .routes(routes!(
            milestones::update_milestone,
            milestones::delete_milestone
        ))
        .routes(routes!(lists::list_labels, lists::create_label))
        .routes(routes!(lists::update_label, lists::delete_label))
        .routes(routes!(lists::dashboard_summary))
        .routes(routes!(templates::list_list_templates))
        .routes(routes!(
            templates::list_space_templates,
            templates::create_template
        ))
        .routes(routes!(
            templates::get_template,
            templates::update_template,
            templates::delete_template
        ))
        .routes(routes!(templates::save_task_as_template))
        .routes(routes!(
            custom_fields::list_custom_fields,
            custom_fields::create_custom_field
        ))
        .routes(routes!(custom_fields::reorder_custom_fields))
        .routes(routes!(
            custom_fields::update_custom_field,
            custom_fields::delete_custom_field
        ))
        .routes(routes!(
            saved_filters::list_saved_filters,
            saved_filters::create_saved_filter
        ))
        .routes(routes!(saved_filters::set_default_saved_filter))
        .routes(routes!(
            saved_filters::update_saved_filter,
            saved_filters::delete_saved_filter
        ))
        .routes(routes!(export::export_task_list))
        .routes(routes!(tasks::list_tasks, tasks::create_task))
        .routes(routes!(tasks::reorder_tasks))
        .routes(routes!(tasks::reorder_tasks_by_title))
        .routes(routes!(tasks::bulk_update_tasks))
        .routes(routes!(copy::copy_tasks))
        .routes(routes!(tasks::list_assigned_tasks))
        .routes(routes!(tasks::list_today_overdue_tasks))
        .routes(routes!(tasks::list_personal_widget_assigned_tasks))
        .routes(routes!(
            tasks::get_task,
            tasks::update_task,
            tasks::delete_task
        ))
        .routes(routes!(tasks::create_comment))
        .routes(routes!(task_items::list_activity_logs))
        .routes(routes!(task_items::create_checklist_item))
        .routes(routes!(task_items::reorder_checklist))
        .routes(routes!(
            task_items::update_checklist_item,
            task_items::delete_checklist_item
        ))
        .routes(routes!(task_items::set_assignees))
        .routes(routes!(task_items::set_followers))
        .routes(routes!(
            custom_fields::list_custom_field_values,
            custom_fields::set_custom_field_value
        ))
        .routes(routes!(attachments::create_attachment))
        .routes(routes!(attachments::delete_attachment))
        .routes(routes!(
            attachments::download_content,
            attachments::upload_content
        ))
        .routes(routes!(notify::notification_check))
        .routes(routes!(home_card::home_card))
        .routes(routes!(search::search))
        .split_for_parts();
    spec.info = Info::new("PMS API", env!("CARGO_PKG_VERSION"));
    // These requests may be omitted, but a supplied JSON body must be an object.
    spec.components
        .get_or_insert_with(Default::default)
        .schemas
        .extend([
            (
                home_card::HomeCardRequest::name().into_owned(),
                home_card::HomeCardRequest::schema(),
            ),
            (
                notify::CheckRequest::name().into_owned(),
                notify::CheckRequest::schema(),
            ),
            (
                notify::CheckItem::name().into_owned(),
                notify::CheckItem::schema(),
            ),
            (
                resource_links::LinkScope::name().into_owned(),
                resource_links::LinkScope::schema(),
            ),
        ]);
    (router, spec)
}

/// Export or check the local API contract without a database connection.
pub(crate) fn handle_cli() -> anyhow::Result<bool> {
    let path = Path::new("docs/openapi.json");
    match std::env::args().nth(1).as_deref() {
        Some("--write-openapi") => {
            fs::write(path, serde_json::to_string_pretty(&router().1)? + "\n")?;
        }
        Some("--check-openapi") => {
            let current: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
            anyhow::ensure!(
                current == serde_json::to_value(router().1)?,
                "docs/openapi.json is out of date. Run pnpm api:generate."
            );
        }
        _ => return Ok(false),
    }
    Ok(true)
}
