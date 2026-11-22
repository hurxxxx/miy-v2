//! PMS: spaces, folders, task lists and tasks, ported from AI-DO 0002af062
//! (see docs/PORTING.md). The SDK serves this router under `/api`, so
//! `/v1/pms/lists` is reached as `/api/v1/pms/lists`, the path AI-DO used.
use axum::{
    Router,
    routing::{get, patch, post, put},
};
use sqlx::PgPool;

mod access;
mod attachments;
mod copy;
mod custom_fields;
mod error;
mod error_catalog;
mod export;
mod home_card;
mod input;
mod lists;
mod milestones;
mod models;
mod notify;
mod saved_filters;
mod search;
mod spaces;
mod statuses;
mod summary;
mod task_items;
mod tasks;
mod templates;
mod users;
mod workflow;

#[cfg(test)]
mod api_tests;

pub fn router(pool: PgPool) -> Router {
    Router::new()
        // People (miy: the platform directory).
        .route("/v1/pms/me", get(users::me))
        .route("/v1/pms/users", get(users::list_users))
        // Spaces, members and the caller's preferences.
        .route(
            "/v1/pms/spaces",
            get(spaces::list_spaces).post(spaces::create_space),
        )
        .route(
            "/v1/pms/spaces/{space_id}",
            patch(spaces::update_space).delete(spaces::delete_space),
        )
        .route(
            "/v1/pms/space-preferences",
            get(spaces::list_space_preferences),
        )
        .route(
            "/v1/pms/spaces/{space_id}/preference",
            patch(spaces::update_space_preference),
        )
        .route(
            "/v1/pms/view-preferences",
            get(spaces::get_view_preferences).patch(spaces::update_view_preferences),
        )
        .route(
            "/v1/pms/spaces/{space_id}/members",
            get(spaces::list_space_members).post(spaces::add_space_member),
        )
        .route(
            "/v1/pms/spaces/{space_id}/members/bulk",
            post(spaces::add_space_members),
        )
        .route(
            "/v1/pms/spaces/{space_id}/members/{user_id}",
            patch(spaces::update_space_member).delete(spaces::remove_space_member),
        )
        // Workflow statuses.
        .route(
            "/v1/pms/spaces/{space_id}/statuses",
            get(statuses::list_space_statuses).post(statuses::create_space_status),
        )
        .route(
            "/v1/pms/space-statuses/{status_id}",
            patch(statuses::update_space_status).delete(statuses::delete_space_status),
        )
        .route(
            "/v1/pms/lists/{list_id}/statuses",
            get(statuses::list_task_list_statuses).post(statuses::create_task_list_status),
        )
        .route(
            "/v1/pms/lists/{list_id}/status-mode",
            patch(statuses::update_task_list_status_mode),
        )
        .route(
            "/v1/pms/task-list-statuses/{status_id}",
            patch(statuses::update_task_list_status).delete(statuses::delete_task_list_status),
        )
        // Folders and task lists.
        .route(
            "/v1/pms/folders",
            get(lists::list_folders).post(lists::create_folder),
        )
        .route(
            "/v1/pms/folders/{folder_id}",
            patch(lists::update_folder).delete(lists::delete_folder),
        )
        .route(
            "/v1/pms/lists",
            get(lists::list_task_lists).post(lists::create_task_list),
        )
        .route(
            "/v1/pms/spaces/{space_id}/lists",
            get(lists::list_space_lists),
        )
        .route(
            "/v1/pms/spaces/{space_id}/lists/reorder",
            patch(lists::reorder_space_lists),
        )
        .route(
            "/v1/pms/lists/{list_id}",
            get(lists::get_task_list)
                .patch(lists::update_task_list)
                .delete(lists::delete_task_list),
        )
        .route(
            "/v1/pms/lists/{list_id}/milestones",
            get(milestones::list_milestones).post(milestones::create_milestone),
        )
        .route(
            "/v1/pms/spaces/{space_id}/milestones",
            get(milestones::list_space_milestones).post(milestones::create_space_milestone),
        )
        .route(
            "/v1/pms/milestones/{milestone_id}",
            patch(milestones::update_milestone).delete(milestones::delete_milestone),
        )
        .route(
            "/v1/pms/lists/{list_id}/labels",
            get(lists::list_labels).post(lists::create_label),
        )
        .route(
            "/v1/pms/labels/{label_id}",
            patch(lists::update_label).delete(lists::delete_label),
        )
        .route("/v1/pms/dashboard/summary", get(lists::dashboard_summary))
        .route(
            "/v1/pms/lists/{list_id}/templates",
            get(templates::list_list_templates),
        )
        .route(
            "/v1/pms/spaces/{space_id}/templates",
            get(templates::list_space_templates).post(templates::create_template),
        )
        .route(
            "/v1/pms/templates/{template_id}",
            get(templates::get_template)
                .patch(templates::update_template)
                .delete(templates::delete_template),
        )
        .route(
            "/v1/pms/tasks/{task_id}/template",
            post(templates::save_task_as_template),
        )
        .route(
            "/v1/pms/lists/{list_id}/custom-fields",
            get(custom_fields::list_custom_fields).post(custom_fields::create_custom_field),
        )
        .route(
            "/v1/pms/lists/{list_id}/custom-fields/reorder",
            patch(custom_fields::reorder_custom_fields),
        )
        .route(
            "/v1/pms/custom-fields/{field_id}",
            patch(custom_fields::update_custom_field).delete(custom_fields::delete_custom_field),
        )
        // Saved filters (miy: kept on the server, per person, shareable).
        .route(
            "/v1/pms/saved-filters",
            get(saved_filters::list_saved_filters).post(saved_filters::create_saved_filter),
        )
        .route(
            "/v1/pms/saved-filters/default",
            put(saved_filters::set_default_saved_filter),
        )
        .route(
            "/v1/pms/saved-filters/{filter_id}",
            patch(saved_filters::update_saved_filter).delete(saved_filters::delete_saved_filter),
        )
        .route(
            "/v1/pms/lists/{list_id}/export",
            get(export::export_task_list),
        )
        // Tasks.
        .route(
            "/v1/pms/lists/{list_id}/tasks",
            get(tasks::list_tasks).post(tasks::create_task),
        )
        .route(
            "/v1/pms/lists/{list_id}/tasks/reorder",
            patch(tasks::reorder_tasks),
        )
        .route(
            "/v1/pms/lists/{list_id}/tasks/reorder-by-title",
            patch(tasks::reorder_tasks_by_title),
        )
        .route(
            "/v1/pms/lists/{list_id}/tasks/bulk",
            patch(tasks::bulk_update_tasks),
        )
        .route("/v1/pms/lists/{list_id}/tasks/copy", post(copy::copy_tasks))
        .route("/v1/pms/tasks/assigned", get(tasks::list_assigned_tasks))
        .route(
            "/v1/pms/tasks/today-overdue",
            get(tasks::list_today_overdue_tasks),
        )
        .route(
            "/v1/personal-widgets/pms/tasks/assigned",
            get(tasks::list_personal_widget_assigned_tasks),
        )
        .route(
            "/v1/pms/tasks/{task_id}",
            get(tasks::get_task)
                .patch(tasks::update_task)
                .delete(tasks::delete_task),
        )
        .route(
            "/v1/pms/tasks/{task_id}/comments",
            post(tasks::create_comment),
        )
        .route(
            "/v1/pms/tasks/{task_id}/activity-logs",
            get(task_items::list_activity_logs),
        )
        .route(
            "/v1/pms/tasks/{task_id}/checklist",
            post(task_items::create_checklist_item),
        )
        .route(
            "/v1/pms/tasks/{task_id}/checklist/reorder",
            patch(task_items::reorder_checklist),
        )
        .route(
            "/v1/pms/checklist/{item_id}",
            patch(task_items::update_checklist_item).delete(task_items::delete_checklist_item),
        )
        .route(
            "/v1/pms/tasks/{task_id}/assignees",
            put(task_items::set_assignees),
        )
        .route(
            "/v1/pms/tasks/{task_id}/followers",
            put(task_items::set_followers),
        )
        .route(
            "/v1/pms/tasks/{task_id}/custom-field-values",
            get(custom_fields::list_custom_field_values).put(custom_fields::set_custom_field_value),
        )
        // Attachments (miy small attachments).
        .route(
            "/v1/pms/tasks/{task_id}/attachments",
            post(attachments::create_attachment),
        )
        .route(
            "/v1/pms/attachments/{attachment_id}",
            axum::routing::delete(attachments::delete_attachment),
        )
        .route(
            "/files/pms/attachments/{attachment_id}/content",
            get(attachments::download_content).put(attachments::upload_content),
        )
        // Platform calls (manifest `platformEndpoints`): read only.
        .route(
            "/platform/notification-check",
            post(notify::notification_check),
        )
        .route("/v1/home-card", post(home_card::home_card))
        .route("/v1/search", post(search::search))
        .with_state(pool)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = miy_app_sdk::database().await?;
    miy_app_sdk::serve(router(pool)).await
}
