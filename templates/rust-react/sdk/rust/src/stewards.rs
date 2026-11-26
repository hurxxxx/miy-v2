//! Installation steward bootstrap and recovery. Ordinary handover belongs to the app.
use crate::{CallError, Identity, Method, call_error, valid_uuid};
use serde::Deserialize;
use serde_json::json;
use sqlx::{PgConnection, PgPool};

pub const MIGRATION: &str = include_str!("stewards.sql");
const PATH: &str = "platform/steward-assignment";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Assignment {
    assignment_id: Option<String>,
}

fn db(_: sqlx::Error) -> CallError {
    call_error("steward_database_unavailable")
}

async fn no_audit(_: &mut PgConnection, _: &str) -> Result<(), sqlx::Error> {
    Ok(())
}

impl Identity {
    /// Check the verified user inside the transaction that changes app administrators.
    pub async fn is_installation_steward_in(
        &self,
        conn: &mut PgConnection,
    ) -> Result<bool, CallError> {
        let id: Option<String> = sqlx::query_scalar(
            "SELECT user_id FROM app.miy_installation_stewards
            WHERE installation_id=$1::uuid AND environment=$2 AND user_id=$3 FOR SHARE",
        )
        .bind(&self.context.installation_id)
        .bind(&self.context.environment)
        .bind(&self.user_id)
        .fetch_optional(conn)
        .await
        .map_err(db)?;
        Ok(id.is_some())
    }

    /// Backend-only nomination lookup. A UI only needs whether to offer acceptance.
    pub async fn pending_steward_assignment(&self) -> Result<Option<String>, CallError> {
        if self.context.platform
            || !self.context.callers.is_empty()
            || self.delegation_token().is_none()
            || !self
                .context
                .effective_permissions
                .iter()
                .any(|permission| permission == "platform:stewards:claim")
        {
            return Ok(None);
        }
        let value = self.call(PATH, Method::GET, None).await?;
        let offer: Assignment =
            serde_json::from_value(value).map_err(|_| call_error("invalid_steward_response"))?;
        if offer
            .assignment_id
            .as_deref()
            .is_some_and(|id| !valid_uuid(id))
        {
            return Err(call_error("invalid_steward_response"));
        }
        Ok(offer.assignment_id)
    }

    /// Call on a user-initiated app write. True means this assignment was applied or
    /// acknowledged on retry. Check the current local role to authorize subsequent work.
    pub async fn apply_steward_assignment(&self, pool: &PgPool) -> Result<bool, CallError> {
        self.apply_steward_assignment_with(pool, no_audit).await
    }

    /// Run app audit SQL on the same connection as the new role and receipt.
    /// The callback receives the assignment UUID, runs once, and an error rolls back all local changes.
    pub async fn apply_steward_assignment_with<F>(
        &self,
        pool: &PgPool,
        audit: F,
    ) -> Result<bool, CallError>
    where
        F: for<'conn, 'id> AsyncFnOnce(
            &'conn mut PgConnection,
            &'id str,
        ) -> Result<(), sqlx::Error>,
    {
        let installation = &self.context.installation_id;
        let environment = self
            .context
            .environment
            .as_deref()
            .ok_or_else(|| call_error("invalid_gateway_context"))?;
        let Some(id) = self.pending_steward_assignment().await? else {
            return Ok(false);
        };

        let mut tx = pool.begin().await.map_err(db)?;
        // Also serializes ordinary app SQL that adds/removes stewards. A complete roster
        // must stay stable between Core's active-account check and the local commit.
        sqlx::query("LOCK TABLE app.miy_installation_stewards IN SHARE ROW EXCLUSIVE MODE")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let applied: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.miy_steward_receipts
            WHERE assignment_id=$1::uuid AND installation_id=$2::uuid AND environment=$3 AND user_id=$4)")
            .bind(&id).bind(installation).bind(environment).bind(&self.user_id)
            .fetch_one(&mut *tx).await.map_err(db)?;
        if !applied {
            let roster: Vec<String> = sqlx::query_scalar(
                "SELECT user_id FROM app.miy_installation_stewards
                WHERE installation_id=$1::uuid AND environment=$2 LIMIT 4097",
            )
            .bind(installation)
            .bind(environment)
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?;
            if roster.len() > 4096 {
                return Err(call_error("invalid_steward_roster"));
            }
            let result = self
                .call(
                    PATH,
                    Method::POST,
                    Some(&json!({"assignmentId":id,"stewards":roster})),
                )
                .await?;
            if result["assignmentId"].as_str() != Some(id.as_str()) {
                return Err(call_error("invalid_steward_response"));
            }
            sqlx::query("DELETE FROM app.miy_installation_stewards WHERE installation_id=$1::uuid AND environment=$2")
                .bind(installation).bind(environment).execute(&mut *tx).await.map_err(db)?;
            sqlx::query("INSERT INTO app.miy_installation_stewards(installation_id,environment,user_id) VALUES ($1::uuid,$2,$3)")
                .bind(installation).bind(environment).bind(&self.user_id).execute(&mut *tx).await.map_err(db)?;
            sqlx::query("INSERT INTO app.miy_steward_receipts(assignment_id,installation_id,environment,user_id)
                VALUES ($1::uuid,$2::uuid,$3,$4)")
                .bind(&id).bind(installation).bind(environment).bind(&self.user_id)
                .execute(&mut *tx).await.map_err(db)?;
            audit(&mut tx, &id).await.map_err(db)?;
        }
        tx.commit().await.map_err(db)?;
        self.call(
            "platform/steward-assignment/applied",
            Method::POST,
            Some(&json!({"assignmentId":id})),
        )
        .await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Extension, Router, extract::State, routing::post};

    // Router registration also checks the handler's Send future, as an app does.
    #[test]
    fn both_steward_apis_work_in_axum_handlers() {
        async fn accept(State(pool): State<PgPool>, Extension(user): Extension<Identity>) {
            let _ = user.apply_steward_assignment(&pool).await;
        }
        async fn accept_with_audit(
            State(pool): State<PgPool>,
            Extension(user): Extension<Identity>,
        ) {
            let _ = user
                .apply_steward_assignment_with(&pool, async |conn, id| {
                    sqlx::query("SELECT $1::text")
                        .bind(id)
                        .execute(conn)
                        .await?;
                    Ok(())
                })
                .await;
        }
        let _: Router<PgPool> = Router::new()
            .route("/accept", post(accept))
            .route("/accept-audit", post(accept_with_audit));
    }
}
