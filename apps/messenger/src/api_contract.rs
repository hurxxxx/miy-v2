use crate::{attachments, conversations, live, messages, notify, reactions, safety_admin, search};
use axum::Router;
use std::{fs, path::Path};
use utoipa::openapi::{ComponentsBuilder, Info, OpenApi};
use utoipa_axum::{router::OpenApiRouter, routes};

// The deprecated thread aliases remain registered for existing callers.
#[allow(deprecated)]
pub(crate) fn router() -> (Router<sqlx::PgPool>, OpenApi) {
    let (router, mut spec) = OpenApiRouter::new()
        .routes(routes!(conversations::search_dm_users))
        .routes(routes!(live::live))
        .routes(routes!(
            conversations::list_dm_conversations,
            conversations::create_dm_conversation
        ))
        .routes(routes!(
            conversations::legacy_list_dm_threads,
            conversations::legacy_create_dm_thread
        ))
        .routes(routes!(conversations::update_dm_conversation))
        .routes(routes!(conversations::add_dm_conversation_participants))
        .routes(routes!(conversations::leave_dm_conversation))
        .routes(routes!(conversations::remove_dm_conversation_participant))
        .routes(routes!(
            messages::list_dm_messages,
            messages::send_dm_message
        ))
        .routes(routes!(
            messages::legacy_list_dm_thread_messages,
            messages::legacy_send_dm_thread_message
        ))
        .routes(routes!(
            messages::edit_dm_message,
            messages::delete_dm_message
        ))
        .routes(routes!(reactions::add_reaction, reactions::remove_reaction))
        .routes(routes!(search::search_messages))
        .routes(routes!(attachments::reserve_dm_attachment))
        .routes(routes!(attachments::cancel_dm_attachment))
        .routes(routes!(attachments::get_dm_attachment_download_url))
        .routes(routes!(attachments::get_dm_attachment_preview_url))
        .routes(routes!(
            attachments::download_dm_attachment_content,
            attachments::upload_dm_attachment_content
        ))
        .routes(routes!(conversations::mark_dm_conversation_read))
        .routes(routes!(conversations::legacy_mark_dm_thread_read))
        .routes(routes!(conversations::mute_dm_conversation))
        .routes(routes!(safety_admin::report_dm))
        .routes(routes!(safety_admin::list_dm_blocks))
        .routes(routes!(
            safety_admin::block_dm_user,
            safety_admin::unblock_dm_user
        ))
        .routes(routes!(safety_admin::get_dm_terms))
        .routes(routes!(safety_admin::accept_dm_terms))
        .routes(routes!(safety_admin::manager_status))
        .routes(routes!(
            safety_admin::list_managers,
            safety_admin::grant_manager
        ))
        .routes(routes!(safety_admin::claim_manager))
        .routes(routes!(safety_admin::revoke_manager))
        .routes(routes!(
            safety_admin::get_terms_settings,
            safety_admin::put_terms_settings
        ))
        .routes(routes!(safety_admin::list_reports))
        .routes(routes!(safety_admin::get_report))
        .routes(routes!(safety_admin::get_evidence))
        .routes(routes!(safety_admin::evidence_attachment_content))
        .routes(routes!(safety_admin::resolve_report))
        .routes(routes!(safety_admin::set_message_visibility))
        .routes(routes!(
            safety_admin::list_filter_terms,
            safety_admin::add_filter_term
        ))
        .routes(routes!(safety_admin::remove_filter_term))
        .routes(routes!(notify::notification_check))
        .routes(routes!(notify::home_card))
        .routes(routes!(search::platform_search))
        .routes(routes!(conversations::get_dm_me))
        .split_for_parts();
    spec.info = Info::new("메신저 API", env!("CARGO_PKG_VERSION"));
    spec.components = Some(
        ComponentsBuilder::from(spec.components.take().unwrap_or_default())
            .schema_from::<live::ClientMessage>()
            .schema_from::<live::ServerHint<'static>>()
            .build(),
    );
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

#[cfg(test)]
mod tests {
    use super::router;
    use serde_json::json;
    use std::collections::BTreeSet;

    #[test]
    fn thread_aliases_keep_deprecated_operations_with_distinct_ids() {
        let spec = serde_json::to_value(router().1).unwrap();
        let paths = spec["paths"].as_object().unwrap();
        let mut ids = BTreeSet::new();
        let mut aliases = 0;
        for (path, methods) in paths {
            for (method, operation) in methods.as_object().unwrap() {
                assert!(ids.insert(operation["operationId"].as_str().unwrap()));
                if path.starts_with("/v1/dm/threads") {
                    aliases += 1;
                    assert_eq!(operation["deprecated"], true);
                    let canonical = path.replacen("/threads", "/conversations", 1);
                    assert_eq!(
                        operation["responses"],
                        paths[&canonical][method]["responses"]
                    );
                }
            }
        }
        assert_eq!(aliases, 5);
    }

    #[test]
    fn websocket_and_file_transfers_keep_their_protocol_shapes() {
        let spec = serde_json::to_value(router().1).unwrap();
        let live = &spec["paths"]["/live"]["get"];
        let upgrade = &live["responses"]["101"];
        assert!(upgrade["content"].is_null());
        assert!(upgrade["headers"]["Sec-WebSocket-Accept"].is_object());
        assert_eq!(live["x-miy-websocket"]["maxClientMessageBytes"], 4096);
        for (key, name) in [
            ("clientMessage", "MessengerLiveClientMessage"),
            ("serverMessage", "MessengerLiveServerHint"),
        ] {
            assert_eq!(
                live["x-miy-websocket"][key]["$ref"],
                format!("#/components/schemas/{name}")
            );
            assert!(spec["components"]["schemas"][name]["oneOf"].is_array());
        }

        let attachment = &spec["paths"]["/files/dm/attachments/{attachment_id}/content"];
        let evidence = &spec["paths"]["/files/dm-safety/reports/{report_id}/evidence/attachments/{attachment_id}/content"];
        let binary = json!({"type": "string", "format": "binary"});
        assert_eq!(
            attachment["put"]["requestBody"]["content"]["*/*"]["schema"],
            binary
        );
        for download in [attachment, evidence] {
            let response = &download["get"]["responses"]["200"];
            assert_eq!(response["content"]["*/*"]["schema"], binary);
            assert!(response["headers"]["Content-Disposition"].is_object());
        }
    }
}
