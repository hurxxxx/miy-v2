//! Console landing page. Read existing operating records without loading
//! per-installation approval history or account and application directories.
use crate::{
    State,
    api_contract::AdminOverviewResponse,
    api_error::{ApiError, ApiResult},
    auth::Principal,
    domain,
    read_api::{Page, encode},
};
use axum::{
    Extension, Json,
    extract::{Query, State as S, rejection::QueryRejection},
};
use serde::Deserialize;
use serde_json::Value;

const PAGE_SIZE: usize = 5;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OverviewQuery {
    approval_cursor: Option<String>,
    runtime_cursor: Option<String>,
}

fn page_scope(actor: &Principal, list: &str) -> String {
    crate::contract::hash(
        serde_json::to_vec(&serde_json::json!([
            "admin-overview",
            list,
            actor.id,
            actor.browser_purpose.map(|purpose| purpose.as_str()),
        ]))
        .expect("overview cursor scope is serializable"),
    )
}

fn boundary(cursor: Option<String>, scope: &str) -> ApiResult<String> {
    Page {
        cursor,
        ..Page::default()
    }
    .boundary(scope)
}

fn next_cursor<T>(rows: &mut Vec<T>, scope: &str, key: impl Fn(&T) -> String) -> Option<String> {
    if rows.len() <= PAGE_SIZE {
        return None;
    }
    rows.truncate(PAGE_SIZE);
    rows.last().map(|last| encode(scope, key(last)))
}

pub async fn read(
    S(state): S<State>,
    Extension(actor): Extension<Principal>,
    query: Result<Query<OverviewQuery>, QueryRejection>,
) -> ApiResult<Json<AdminOverviewResponse>> {
    domain::admin(&state, &actor).await?;
    let Query(query) = query.map_err(|_| ApiError::bad("invalid_admin_overview_query"))?;
    let approval_scope = page_scope(&actor, "approvals");
    let runtime_scope = page_scope(&actor, "runtime");
    let approval_boundary = boundary(query.approval_cursor, &approval_scope)?;
    let runtime_boundary = boundary(query.runtime_cursor, &runtime_scope)?;
    let summary: Value = sqlx::query_scalar(
        "WITH current_installations AS (
            SELECT i.app_id, a.name, i.environment, i.app_id || ':' || i.environment AS key,
                EXISTS (
                    SELECT 1
                    FROM jsonb_array_elements_text(r.manifest->'requestedPermissions') AS wanted(permission)
                    WHERE wanted.permission <> 'database' AND (ap.release_id IS NULL
                        OR (ap.reviewed_permissions IS NOT NULL AND NOT ap.reviewed_permissions ? wanted.permission))
                ) AS awaiting_approval,
                CASE
                    WHEN i.runtime_status IN ('failed', 'recovery_required', 'unreachable') THEN i.runtime_status
                    WHEN i.runtime_status = 'stopped' AND i.desired_state = 'running' THEN 'unexpected_stop'
                    WHEN i.runtime_status = 'running' AND i.health_status IN ('degraded', 'unhealthy')
                        THEN i.health_status
                END AS issue
            FROM installations i
            JOIN apps a ON a.id = i.app_id
            JOIN releases r ON r.id = i.release_id
            LEFT JOIN installation_permissions ap ON ap.installation_id = i.id AND ap.release_id = r.id
            WHERE r.image_id IS NOT NULL AND i.runtime_status <> 'removed'
        )
        SELECT jsonb_build_object(
            'pendingRegistrations', (SELECT count(*) FROM app_registrations WHERE status = 'pending'),
            'pendingApprovals', (SELECT count(*) FROM current_installations WHERE awaiting_approval),
            'runtimeIssues', (SELECT count(*) FROM current_installations WHERE issue IS NOT NULL),
            'approvalExamples', COALESCE((
                SELECT jsonb_agg(jsonb_build_object('appId', app_id, 'name', name, 'environment', environment)
                    ORDER BY key COLLATE \"C\")
                FROM (SELECT * FROM current_installations WHERE awaiting_approval AND key COLLATE \"C\" > $1
                    ORDER BY key COLLATE \"C\" LIMIT $3) approvals
            ), '[]'::jsonb),
            'runtimeExamples', COALESCE((
                SELECT jsonb_agg(jsonb_build_object('appId', app_id, 'name', name, 'environment', environment, 'issue', issue)
                    ORDER BY key COLLATE \"C\")
                FROM (SELECT * FROM current_installations WHERE issue IS NOT NULL AND key COLLATE \"C\" > $2
                    ORDER BY key COLLATE \"C\" LIMIT $3) issues
            ), '[]'::jsonb)
        )",
    )
    .bind(approval_boundary)
    .bind(runtime_boundary)
    .bind(PAGE_SIZE as i64 + 1)
    .fetch_one(&state.pool)
    .await?;
    let mut summary: AdminOverviewResponse = serde_json::from_value(summary)?;
    summary.approval_next_cursor =
        next_cursor(&mut summary.approval_examples, &approval_scope, |row| {
            format!("{}:{}", row.app_id, row.environment.as_str())
        });
    summary.runtime_next_cursor =
        next_cursor(&mut summary.runtime_examples, &runtime_scope, |row| {
            format!("{}:{}", row.app_id, row.environment.as_str())
        });
    Ok(Json(summary))
}
