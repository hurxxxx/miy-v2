use crate::{
    AppState, collaboration,
    error::{ApiError, ApiJson, ApiResult},
    scene,
    whiteboard::{self, Board},
};
use axum::{
    Extension, Json,
    extract::{Path, State},
};
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Compare the visible drawing in order. Editor clocks, tombstones and
/// unreferenced image records remain available for synchronization and undo.
fn visible(scene: &Value) -> Value {
    let mut out = scene::for_compare(scene);
    let mut used = std::collections::BTreeSet::new();
    if let Some(elements) = out["elements"].as_array_mut() {
        elements.retain(|e| e["isDeleted"] != true);
        for e in elements {
            if let Some(id) = e["fileId"].as_str() {
                used.insert(id.to_owned());
            }
            if let Some(obj) = e.as_object_mut() {
                for k in ["updated", "version", "versionNonce", "index", "isDeleted"] {
                    obj.remove(k);
                }
            }
        }
    }
    if let Some(files) = out["files"].as_object_mut() {
        files.retain(|k, _| used.contains(k));
    }
    out
}
pub async fn baseline(
    tx: &mut Transaction<'_, Postgres>,
    board: &Board,
    scene: &Value,
    user: &str,
) -> ApiResult<()> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM whiteboard_versions WHERE board_id=$1)")
            .bind(&board.id)
            .fetch_one(&mut **tx)
            .await?;
    if !exists {
        record(tx, board, scene, user).await?;
    }
    Ok(())
}
pub async fn checkpoint(
    tx: &mut Transaction<'_, Postgres>,
    board: &Board,
    scene: &Value,
    user: &str,
) -> ApiResult<()> {
    let last: Option<Value> = sqlx::query_scalar(
        "SELECT scene FROM whiteboard_versions WHERE board_id=$1 ORDER BY revision DESC LIMIT 1",
    )
    .bind(&board.id)
    .fetch_optional(&mut **tx)
    .await?;
    if last.as_ref().is_some_and(|v| visible(v) != visible(scene)) {
        record(tx, board, scene, user).await?;
    }
    Ok(())
}
pub async fn record(
    tx: &mut Transaction<'_, Postgres>,
    board: &Board,
    scene: &Value,
    user: &str,
) -> ApiResult<()> {
    let last: Option<Value> = sqlx::query_scalar(
        "SELECT scene FROM whiteboard_versions WHERE board_id=$1 ORDER BY revision DESC LIMIT 1",
    )
    .bind(&board.id)
    .fetch_optional(&mut **tx)
    .await?;
    if last.as_ref().is_some_and(|v| visible(v) == visible(scene)) {
        return Ok(());
    }
    sqlx::query("INSERT INTO whiteboard_versions(id, board_id,revision,scene,author) VALUES($1,$2,$3,$4,$5) ON CONFLICT(board_id,revision) DO NOTHING")
        .bind(Uuid::new_v4()).bind(&board.id).bind(board.revision).bind(scene).bind(user).execute(&mut **tx).await?;
    sqlx::query("DELETE FROM whiteboard_versions WHERE id IN (SELECT id FROM (SELECT id,row_number() OVER(ORDER BY revision DESC) AS recent,row_number() OVER(PARTITION BY (created_at AT TIME ZONE 'UTC')::date ORDER BY revision DESC) AS daily FROM whiteboard_versions WHERE board_id=$1) q WHERE recent>50 AND daily>1)")
        .bind(&board.id).execute(&mut **tx).await?;
    Ok(())
}

#[derive(Serialize, utoipa::ToSchema, sqlx::FromRow)]
pub struct Item {
    id: String,
    revision: i64,
    author: String,
    created_at: String,
}
#[utoipa::path(get,path="/v1/boards/{id}/versions",operation_id="listBoardVersions",tag="Versions",params(("id"=String,Path)),responses((status=200,body=Vec<Item>)))]
pub async fn list(
    State(s): State<AppState>,
    Extension(who): Extension<Identity>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<Item>>> {
    let space = collaboration::space(&s, &id).await?;
    whiteboard::load_for_user(&s, &who, &space, &id).await?;
    Ok(Json(sqlx::query_as("SELECT id::text,revision,author,created_at::text FROM whiteboard_versions WHERE board_id=$1 ORDER BY revision DESC").bind(id).fetch_all(&s.pool).await?))
}
#[utoipa::path(get,path="/v1/boards/{id}/versions/{version}",operation_id="getBoardVersion",tag="Versions",params(("id"=String,Path),("version"=String,Path)),responses((status=200,body=Object)))]
pub async fn get(
    State(s): State<AppState>,
    Extension(who): Extension<Identity>,
    Path((id, version)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    let space = collaboration::space(&s, &id).await?;
    whiteboard::load_for_user(&s, &who, &space, &id).await?;
    Ok(Json(
        sqlx::query_scalar(
            "SELECT scene FROM whiteboard_versions WHERE board_id=$1 AND id::text=$2",
        )
        .bind(id)
        .bind(version)
        .fetch_optional(&s.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("version_not_found", "Version unavailable"))?,
    ))
}
#[derive(Deserialize, utoipa::ToSchema)]
pub struct Restore {
    expected_revision: i64,
}
#[utoipa::path(post,path="/v1/boards/{id}/versions/{version}/restore",operation_id="restoreBoardVersion",tag="Versions",params(("id"=String,Path),("version"=String,Path)),request_body=Restore,responses((status=200,body=i64),(status=409,description="Board changed")))]
pub async fn restore(
    State(s): State<AppState>,
    Extension(who): Extension<Identity>,
    Path((id, version)): Path<(String, String)>,
    ApiJson(input): ApiJson<Restore>,
) -> ApiResult<Json<i64>> {
    let space = collaboration::space(&s, &id).await?;
    let mut tx = s.pool.begin().await?;
    let mut b = whiteboard::lock_for_user(&mut tx, &s, &who, &space, &id).await?;
    if !b.access.can_edit || b.board.trashed_at.is_some() {
        return Err(ApiError::forbidden("edit_required", "Edit access required"));
    }
    if b.board.revision != input.expected_revision {
        return Err(ApiError::conflict(
            "whiteboard_revision_conflict",
            "Board changed",
        ));
    }
    let next: Value = sqlx::query_scalar(
        "SELECT scene FROM whiteboard_versions WHERE board_id=$1 AND id::text=$2",
    )
    .bind(&id)
    .bind(version)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| ApiError::not_found("version_not_found", "Version unavailable"))?;
    let old: Value = sqlx::query_scalar("SELECT scene FROM whiteboards WHERE id=$1")
        .bind(&id)
        .fetch_one(&mut *tx)
        .await?;
    record(&mut tx, &b.board, &old, &who.user_id).await?;
    miy_app_collab::db::invalidate(&mut tx, &id).await?;
    if visible(&old) != visible(&next) {
        whiteboard::check_scene_files(&mut *tx, Some(&id), &next).await?;
        b.board.revision=sqlx::query_scalar("UPDATE whiteboards SET scene=$2,revision=revision+1,updated_at=timezone('UTC',now()) WHERE id=$1 RETURNING revision").bind(&id).bind(&next).fetch_one(&mut *tx).await?;
        record(&mut tx, &b.board, &next, &who.user_id).await?;
    }
    tx.commit().await?;
    Ok(Json(b.board.revision))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn undo_and_cursor_changes_preserve_the_visible_version() {
        let original = json!({"elements":[{"id":"a","x":2,"version":1}],"appState":{},"files":{}});
        let undone = json!({"elements":[{"id":"a","x":2,"version":4,"updated":42,"versionNonce":2},{"id":"b","isDeleted":true}],
            "appState":{"scrollX":45,"selectedElementIds":{"a":true}},"files":{"unused":{"storage":"s"}}});
        assert_eq!(visible(&original), visible(&undone));
    }
}
