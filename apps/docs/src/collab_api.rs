use crate::{
    AppState,
    error::{ApiError, ApiJson, ApiResult},
};
use axum::{
    Extension, Json,
    extract::{Path, State, WebSocketUpgrade},
    response::Response,
};
use miy_app_sdk::Identity;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SessionRequest {
    session: Option<String>,
}
#[derive(Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    room_id: String,
    generation: String,
    session_id: String,
    revision: i64,
    name: String,
}
#[utoipa::path(post,path="/v1/collaboration/{resource}/session",operation_id="openDocsEditingSession",tag="Collaboration",
 params(("resource"=String,Path)),request_body=SessionRequest,
 responses((status=200,body=Session),(status=403,description="Edit access required",body=crate::error::ErrorResponse),
 (status=409,description="Room ownership or generation changed",body=crate::error::ErrorResponse)))]
pub async fn session(
    State(state): State<AppState>,
    Extension(who): Extension<Identity>,
    Path(key): Path<String>,
    ApiJson(req): ApiJson<SessionRequest>,
) -> ApiResult<Json<Session>> {
    let resume = req
        .session
        .as_deref()
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| ApiError::validation("invalid session"))?;
    let session = state
        .rooms
        .session(state.clone(), key, who, resume)
        .await
        .map_err(|e| {
            if let Some(api) = e.downcast_ref::<ApiError>() {
                return api.clone();
            }
            match e.to_string().as_str() {
                "access_revoked" => {
                    ApiError::forbidden("resource_access_revoked", "Edit access required")
                }
                "room_busy" | "generation_changed" | "session_expired" => {
                    ApiError::conflict("room_changed", "Reopen this editing session")
                }
                _ => ApiError::unavailable(
                    "collaboration_unavailable",
                    "Unable to open the editing session",
                ),
            }
        })?;
    Ok(Json(Session {
        room_id: session.room_id,
        generation: session.generation.to_string(),
        session_id: session.session_id.to_string(),
        revision: session.revision,
        name: session.name,
    }))
}
#[utoipa::path(get,path="/live",operation_id="docsLive",tag="Collaboration",responses((status=101,description="Authenticated binary editing connection"),(status=401,description="Invalid identity")))]
pub async fn live(
    State(state): State<AppState>,
    Extension(who): Extension<Identity>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.max_frame_size(65536)
        .max_message_size(65536)
        .on_upgrade(move |socket| async move {
            state.rooms.serve(state.clone(), who, socket).await;
        })
}
