//! A small app-owned project/task example. Platform Use is never a project ACL.
use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use miy_app_sdk::{
    FileCreate, FileMetadata, FileStatus, Identity,
    acl::{Acl, AclError, Grant, ResourceKey, RoleDefinition, ShareSnapshot},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    acl: Acl,
}

pub fn router(pool: PgPool) -> anyhow::Result<Router> {
    let roles = [
        (
            "owner",
            "소유자",
            vec!["read", "write", "files.read", "files.write", "acl.manage"],
        ),
        (
            "editor",
            "편집자",
            vec!["read", "write", "files.read", "files.write"],
        ),
        ("viewer", "조회자", vec!["read", "files.read"]),
    ]
    .into_iter()
    .map(|(id, label, actions)| RoleDefinition {
        id: id.into(),
        label: label.into(),
        actions: actions.into_iter().map(str::to_owned).collect(),
    })
    .collect();
    let state = AppState {
        acl: Acl::new(pool.clone(), roles)?,
        pool,
    };
    Ok(Router::new()
        .route("/projects", get(projects).post(create_project))
        .route("/projects/{id}", get(project))
        .route("/projects/{id}/tasks", get(tasks).post(create_task))
        .route("/projects/{id}/tasks/{task}", get(task).patch(update_task))
        .route("/projects/{id}/sharing", get(sharing).put(replace_sharing))
        .route(
            "/projects/{id}/sharing/results/{request}",
            get(sharing_result),
        )
        .route("/projects/{id}/files", get(files).post(reserve_file))
        .route(
            "/projects/{id}/files/by-request/{request}",
            get(file_by_request),
        )
        .route(
            "/projects/{id}/files/{file}",
            get(file_get).delete(file_cancel),
        )
        .route(
            "/projects/{id}/files/{file}/attach",
            axum::routing::post(file_attach),
        )
        // The separate platform gateway streams only this app's /api/files routes.
        .route(
            "/files/projects/{id}/{file}/content",
            get(file_download).put(file_upload),
        )
        .with_state(state))
}

struct Error {
    status: u16,
    code: &'static str,
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (
            StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(json!({"error":{"code":self.code}})),
        )
            .into_response()
    }
}
impl From<AclError> for Error {
    fn from(e: AclError) -> Self {
        Self {
            status: e.status,
            code: e.code,
        }
    }
}
impl From<sqlx::Error> for Error {
    fn from(_: sqlx::Error) -> Self {
        Self {
            status: 503,
            code: "app_database_unavailable",
        }
    }
}
impl From<miy_app_sdk::CallError> for Error {
    fn from(e: miy_app_sdk::CallError) -> Self {
        Self {
            status: e.status.unwrap_or(503),
            code: e.code,
        }
    }
}
type Result<T> = std::result::Result<T, Error>;
fn fail(status: u16, code: &'static str) -> Error {
    Error { status, code }
}
fn key(id: Uuid) -> ResourceKey {
    ResourceKey {
        kind: "project".into(),
        id: id.to_string(),
    }
}
fn title(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 200 || value.chars().any(char::is_control) {
        return Err(fail(400, "invalid_title"));
    }
    Ok(value.into())
}

#[derive(Serialize, sqlx::FromRow)]
struct Project {
    id: Uuid,
    title: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewProject {
    id: Uuid,
    title: String,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Page {
    cursor: Option<String>,
}

async fn projects(
    State(s): State<AppState>,
    Extension(i): Extension<Identity>,
    Query(q): Query<Page>,
) -> Result<Json<serde_json::Value>> {
    // Filter in the ACL SQL before pagination; never load all titles then hide them in the browser.
    let page = s
        .acl
        .list_resources(&i, "project", "read", 50, q.cursor.as_deref())
        .await?;
    let ids: Vec<Uuid> = page
        .resources
        .iter()
        .filter_map(|r| Uuid::parse_str(&r.id).ok())
        .collect();
    let rows = sqlx::query_as::<_, Project>(
        "SELECT id,title FROM app.projects WHERE id=ANY($1) ORDER BY id::text",
    )
    .bind(ids)
    .fetch_all(&s.pool)
    .await?;
    Ok(Json(json!({"projects":rows,"nextCursor":page.next_cursor})))
}
async fn create_project(
    State(s): State<AppState>,
    Extension(i): Extension<Identity>,
    Json(input): Json<NewProject>,
) -> Result<Json<Project>> {
    let name = title(&input.title)?;
    let mut tx = s.pool.begin().await?;
    // Fixed client-generated ID makes a network-result lookup possible; no blind POST retry.
    let inserted =
        sqlx::query("INSERT INTO app.projects(id,title) VALUES($1,$2) ON CONFLICT DO NOTHING")
            .bind(input.id)
            .bind(&name)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    if inserted == 0 {
        return Err(fail(409, "project_exists"));
    }
    s.acl
        .create_resource_in(&mut tx, &i, &key(input.id), "owner")
        .await?;
    tx.commit().await?;
    Ok(Json(Project {
        id: input.id,
        title: name,
    }))
}
async fn project(
    State(s): State<AppState>,
    Extension(i): Extension<Identity>,
    Path(id): Path<Uuid>,
) -> Result<Json<Project>> {
    s.acl.require(&i, &key(id), "read").await?;
    let row = sqlx::query_as("SELECT id,title FROM app.projects WHERE id=$1")
        .bind(id)
        .fetch_optional(&s.pool)
        .await?
        .ok_or_else(|| fail(404, "not_found"))?;
    Ok(Json(row))
}
#[derive(Serialize, sqlx::FromRow)]
struct Task {
    id: Uuid,
    title: String,
    completed: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewTask {
    id: Uuid,
    title: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateTask {
    completed: bool,
}
async fn tasks(
    State(s): State<AppState>,
    Extension(i): Extension<Identity>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Task>>> {
    s.acl.require(&i, &key(id), "read").await?;
    Ok(Json(
        sqlx::query_as(
            "SELECT id,title,completed FROM app.tasks WHERE project_id=$1 ORDER BY id LIMIT 200",
        )
        .bind(id)
        .fetch_all(&s.pool)
        .await?,
    ))
}
async fn create_task(
    State(s): State<AppState>,
    Extension(i): Extension<Identity>,
    Path(id): Path<Uuid>,
    Json(input): Json<NewTask>,
) -> Result<Json<Task>> {
    let name = title(&input.title)?;
    let mut tx = s.pool.begin().await?;
    s.acl.require_in(&mut tx, &i, &key(id), "write").await?;
    let row = sqlx::query_as(
        "INSERT INTO app.tasks(id,project_id,title) VALUES($1,$2,$3) RETURNING id,title,completed",
    )
    .bind(input.id)
    .bind(id)
    .bind(name)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(row))
}
async fn task(
    State(s): State<AppState>,
    Extension(i): Extension<Identity>,
    Path((id, task)): Path<(Uuid, Uuid)>,
) -> Result<Json<Task>> {
    s.acl.require(&i, &key(id), "read").await?;
    let row =
        sqlx::query_as("SELECT id,title,completed FROM app.tasks WHERE project_id=$1 AND id=$2")
            .bind(id)
            .bind(task)
            .fetch_optional(&s.pool)
            .await?
            .ok_or_else(|| fail(404, "not_found"))?;
    Ok(Json(row))
}
async fn update_task(
    State(s): State<AppState>,
    Extension(i): Extension<Identity>,
    Path((id, task)): Path<(Uuid, Uuid)>,
    Json(input): Json<UpdateTask>,
) -> Result<Json<Task>> {
    let mut tx = s.pool.begin().await?;
    s.acl.require_in(&mut tx, &i, &key(id), "write").await?;
    let row=sqlx::query_as("UPDATE app.tasks SET completed=$3 WHERE project_id=$1 AND id=$2 RETURNING id,title,completed")
        .bind(id).bind(task).bind(input.completed).fetch_optional(&mut *tx).await?.ok_or_else(||fail(404,"not_found"))?;
    tx.commit().await?;
    Ok(Json(row))
}
async fn sharing(
    State(s): State<AppState>,
    Extension(i): Extension<Identity>,
    Path(id): Path<Uuid>,
) -> Result<Json<ShareSnapshot>> {
    Ok(Json(s.acl.sharing(&i, &key(id)).await?))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ShareChange {
    expected_revision: i64,
    request_id: Uuid,
    entries: Vec<Grant>,
}
async fn replace_sharing(
    State(s): State<AppState>,
    Extension(i): Extension<Identity>,
    Path(id): Path<Uuid>,
    Json(input): Json<ShareChange>,
) -> Result<Json<ShareSnapshot>> {
    let mut tx = s.pool.begin().await?;
    let result = s
        .acl
        .replace_in(
            &mut tx,
            &i,
            &key(id),
            input.expected_revision,
            &input.request_id.to_string(),
            &input.entries,
        )
        .await?;
    tx.commit().await?;
    Ok(Json(result))
}
async fn sharing_result(
    State(s): State<AppState>,
    Extension(i): Extension<Identity>,
    Path((id, request)): Path<(Uuid, Uuid)>,
) -> Result<Json<Option<ShareSnapshot>>> {
    Ok(Json(
        s.acl
            .replacement_result(&i, &key(id), &request.to_string())
            .await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NewFile {
    request_id: Uuid,
    name: String,
    mime: String,
    size: u64,
}
#[derive(sqlx::FromRow)]
struct FileRow {
    request_id: Uuid,
    actor_id: String,
    attached: bool,
    attach_request: Option<Uuid>,
}
async fn file_row(s: &AppState, id: Uuid, file: Uuid) -> Result<FileRow> {
    sqlx::query_as("SELECT request_id,file_id,actor_id,attached,attach_request FROM app.attachments WHERE project_id=$1 AND file_id=$2")
        .bind(id).bind(file).fetch_optional(&s.pool).await?.ok_or_else(||fail(404,"not_found"))
}
fn pending_owner(row: &FileRow, i: &Identity) -> Result<()> {
    if row.actor_id != i.user_id {
        return Err(fail(403, "forbidden"));
    }
    Ok(())
}
async fn reserve_file(
    State(s): State<AppState>,
    Extension(i): Extension<Identity>,
    Path(id): Path<Uuid>,
    Json(input): Json<NewFile>,
) -> Result<Json<FileMetadata>> {
    if input.size > miy_app_sdk::storage::MAX_FILE_BYTES {
        return Err(fail(413, "too_large"));
    }
    if input.name.trim().is_empty()
        || input.name.chars().count() > 255
        || input
            .name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\'))
        || input.mime.is_empty()
        || input.mime.len() > 128
    {
        return Err(fail(400, "invalid_input"));
    }
    let mut tx = s.pool.begin().await?;
    s.acl
        .require_in(&mut tx, &i, &key(id), "files.write")
        .await?;
    sqlx::query("INSERT INTO app.attachments(request_id,project_id,actor_id,name,mime,size) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING")
        .bind(input.request_id).bind(id).bind(&i.user_id).bind(&input.name).bind(&input.mime)
        .bind(i64::try_from(input.size).map_err(|_|fail(400,"invalid_input"))?).execute(&mut *tx).await?;
    let row=sqlx::query("SELECT project_id,actor_id,name,mime,size FROM app.attachments WHERE request_id=$1 FOR UPDATE")
        .bind(input.request_id).fetch_one(&mut *tx).await?;
    if row.get::<Uuid, _>("project_id") != id
        || row.get::<String, _>("actor_id") != i.user_id
        || row.get::<String, _>("name") != input.name
        || row.get::<String, _>("mime") != input.mime
        || row.get::<i64, _>("size") != input.size as i64
    {
        return Err(fail(409, "conflict"));
    }
    // Persist intent before Core reservation: an uncertain response is recoverable by request ID.
    tx.commit().await?;
    let file = i
        .files_create(&FileCreate {
            request_id: input.request_id.to_string(),
            name: input.name,
            mime: input.mime,
            size: input.size,
        })
        .await?;
    s.acl.require(&i, &key(id), "files.write").await?;
    sqlx::query("UPDATE app.attachments SET file_id=$2 WHERE request_id=$1")
        .bind(input.request_id)
        .bind(Uuid::parse_str(&file.id).map_err(|_| fail(502, "invalid_file_response"))?)
        .execute(&s.pool)
        .await?;
    Ok(Json(file))
}
async fn file_by_request(
    State(s): State<AppState>,
    Extension(i): Extension<Identity>,
    Path((id, request)): Path<(Uuid, Uuid)>,
) -> Result<Json<Option<FileMetadata>>> {
    s.acl.require(&i, &key(id), "files.write").await?;
    let row:Option<FileRow>=sqlx::query_as("SELECT request_id,file_id,actor_id,attached,attach_request FROM app.attachments WHERE project_id=$1 AND request_id=$2")
        .bind(id).bind(request).fetch_optional(&s.pool).await?;
    let Some(row) = row else {
        return Ok(Json(None));
    };
    pending_owner(&row, &i)?;
    let file = match i.files_by_request(&request.to_string()).await {
        Ok(file) => file,
        Err(e) if e.status == Some(404) => return Ok(Json(None)),
        Err(e) => return Err(e.into()),
    };
    s.acl.require(&i, &key(id), "files.write").await?;
    sqlx::query("UPDATE app.attachments SET file_id=$2 WHERE request_id=$1")
        .bind(request)
        .bind(Uuid::parse_str(&file.id).map_err(|_| fail(502, "invalid_file_response"))?)
        .execute(&s.pool)
        .await?;
    Ok(Json(Some(file)))
}
async fn file_get(
    State(s): State<AppState>,
    Extension(i): Extension<Identity>,
    Path((id, file)): Path<(Uuid, Uuid)>,
) -> Result<Json<FileMetadata>> {
    s.acl.require(&i, &key(id), "files.read").await?;
    let row = file_row(&s, id, file).await?;
    if !row.attached {
        pending_owner(&row, &i)?;
    }
    Ok(Json(i.files_get(&file.to_string()).await?))
}
async fn files(
    State(s): State<AppState>,
    Extension(i): Extension<Identity>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>> {
    s.acl.require(&i, &key(id), "files.read").await?;
    let rows=sqlx::query("SELECT file_id,name,mime,size FROM app.attachments WHERE project_id=$1 AND attached ORDER BY request_id LIMIT 100")
        .bind(id).fetch_all(&s.pool).await?;
    Ok(Json(json!(rows.into_iter().map(|r|json!({"id":r.get::<Uuid,_>("file_id"),"name":r.get::<String,_>("name"),"mime":r.get::<String,_>("mime"),"size":r.get::<i64,_>("size")})).collect::<Vec<_>>())))
}
async fn file_upload(
    State(s): State<AppState>,
    Extension(i): Extension<Identity>,
    Path((id, file)): Path<(Uuid, Uuid)>,
    body: Body,
) -> Result<Json<FileMetadata>> {
    s.acl.require(&i, &key(id), "files.write").await?;
    let row = file_row(&s, id, file).await?;
    pending_owner(&row, &i)?;
    if row.attached {
        return Err(fail(409, "conflict"));
    }
    let meta = i.files_get(&file.to_string()).await?;
    let result = i.files_upload(&meta, body).await?;
    s.acl.require(&i, &key(id), "files.write").await?;
    Ok(Json(result))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Attach {
    reference: String,
    request_id: Uuid,
}
async fn file_attach(
    State(s): State<AppState>,
    Extension(i): Extension<Identity>,
    Path((id, file)): Path<(Uuid, Uuid)>,
    Json(input): Json<Attach>,
) -> Result<Json<FileMetadata>> {
    if input.reference != id.to_string() {
        return Err(fail(400, "invalid_input"));
    }
    let mut tx = s.pool.begin().await?;
    s.acl
        .require_in(&mut tx, &i, &key(id), "files.write")
        .await?;
    let row:FileRow=sqlx::query_as("SELECT request_id,file_id,actor_id,attached,attach_request FROM app.attachments WHERE project_id=$1 AND file_id=$2 FOR UPDATE")
        .bind(id).bind(file).fetch_optional(&mut *tx).await?.ok_or_else(||fail(404,"not_found"))?;
    pending_owner(&row, &i)?;
    if row.attach_request.is_some_and(|r| r != input.request_id) {
        return Err(fail(409, "conflict"));
    }
    let meta = i.files_get(&file.to_string()).await?;
    if !matches!(meta.status, FileStatus::Ready | FileStatus::Attached) {
        return Err(fail(409, "conflict"));
    }
    // The application reference commits first. Never delete bytes to compensate for a lost reply.
    sqlx::query("UPDATE app.attachments SET attached=true,attach_request=$2 WHERE request_id=$1")
        .bind(row.request_id)
        .bind(input.request_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(
        i.files_attach(
            &file.to_string(),
            &input.reference,
            &input.request_id.to_string(),
        )
        .await?,
    ))
}
async fn file_cancel(
    State(s): State<AppState>,
    Extension(i): Extension<Identity>,
    Path((id, file)): Path<(Uuid, Uuid)>,
) -> Result<Json<FileMetadata>> {
    let mut tx = s.pool.begin().await?;
    s.acl
        .require_in(&mut tx, &i, &key(id), "files.write")
        .await?;
    let row:FileRow=sqlx::query_as("SELECT request_id,file_id,actor_id,attached,attach_request FROM app.attachments WHERE project_id=$1 AND file_id=$2 FOR UPDATE")
        .bind(id).bind(file).fetch_optional(&mut *tx).await?.ok_or_else(||fail(404,"not_found"))?;
    pending_owner(&row, &i)?;
    if row.attached {
        return Err(fail(409, "attached_file_preserved"));
    }
    let result = i.files_cancel(&file.to_string()).await?;
    tx.commit().await?;
    Ok(Json(result))
}
async fn file_download(
    State(s): State<AppState>,
    Extension(i): Extension<Identity>,
    Path((id, file)): Path<(Uuid, Uuid)>,
) -> Result<Response> {
    s.acl.require(&i, &key(id), "files.read").await?;
    let row = file_row(&s, id, file).await?;
    if !row.attached {
        return Err(fail(404, "not_found"));
    }
    Ok(i.files_download(&file.to_string()).await?)
}
