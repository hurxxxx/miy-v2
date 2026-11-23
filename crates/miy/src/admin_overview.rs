//! Console landing page. Read existing operating records without loading
//! per-installation approval history or account and application directories.
use crate::{State, api_error::ApiResult, auth::Principal, domain};
use axum::{Extension, Json, extract::State as S};
use serde_json::Value;

pub async fn read(
    S(state): S<State>,
    Extension(actor): Extension<Principal>,
) -> ApiResult<Json<Value>> {
    domain::admin(&state, &actor).await?;
    let summary = sqlx::query_scalar(
        "WITH current_installations AS (
            SELECT i.app_id, a.name, i.environment,
                EXISTS (
                    SELECT 1
                    FROM jsonb_array_elements_text(r.manifest->'requestedPermissions') AS wanted(permission)
                    WHERE wanted.permission <> 'database'
                      AND NOT COALESCE(ap.permissions ? wanted.permission, false)
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
                    ORDER BY name, app_id, environment)
                FROM (SELECT * FROM current_installations WHERE awaiting_approval ORDER BY name, app_id, environment LIMIT 5) approvals
            ), '[]'::jsonb),
            'runtimeExamples', COALESCE((
                SELECT jsonb_agg(jsonb_build_object('appId', app_id, 'name', name, 'environment', environment, 'issue', issue)
                    ORDER BY name, app_id, environment)
                FROM (SELECT * FROM current_installations WHERE issue IS NOT NULL ORDER BY name, app_id, environment LIMIT 5) issues
            ), '[]'::jsonb)
        )",
    )
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(summary))
}
