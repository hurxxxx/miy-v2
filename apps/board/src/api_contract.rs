use crate::{announcements, community, managers, media, notify, spaces, ugc};
use axum::{Router, extract::DefaultBodyLimit};
use std::{fs, path::Path};
use utoipa::{
    PartialSchema, ToSchema,
    openapi::{Info, OpenApi},
};
use utoipa_axum::{
    router::{OpenApiRouter, UtoipaMethodRouterExt},
    routes,
};

pub(crate) fn router() -> (Router<crate::AppState>, OpenApi) {
    let (router, mut spec) = OpenApiRouter::new()
        .routes(routes!(community::list_channels))
        .routes(routes!(
            community::list_admin_channels,
            community::create_channel
        ))
        .routes(routes!(
            community::update_channel,
            community::delete_channel
        ))
        .routes(routes!(community::list_posts, community::create_post))
        .routes(routes!(
            community::get_post,
            community::update_post,
            community::delete_post
        ))
        .routes(routes!(community::mark_read))
        .routes(routes!(community::unlock_post))
        .routes(routes!(community::create_comment))
        .routes(routes!(
            community::update_comment,
            community::delete_comment
        ))
        .routes(routes!(ugc::list_filter_terms, ugc::add_filter_term))
        .routes(routes!(ugc::remove_filter_term))
        .routes(routes!(
            announcements::list_company,
            announcements::create_company
        ))
        .routes(routes!(
            announcements::get_company,
            announcements::update_company,
            announcements::delete_company
        ))
        .routes(routes!(
            announcements::list_in_space,
            announcements::create_in_space
        ))
        .routes(routes!(
            announcements::get_in_space,
            announcements::update_in_space,
            announcements::delete_in_space
        ))
        .routes(routes!(announcements::recent))
        .routes(routes!(spaces::list_spaces, spaces::create_space))
        .routes(routes!(spaces::update_space, spaces::delete_space))
        .routes(routes!(spaces::get_sharing, spaces::replace_sharing))
        .routes(routes!(spaces::sharing_result))
        .routes(routes!(managers::me))
        .routes(routes!(managers::bootstrap))
        .routes(routes!(managers::list_managers, managers::add_manager))
        .routes(routes!(managers::remove_manager))
        .routes(routes!(media::create_upload))
        .routes(
            routes!(media::upload_content, media::download)
                .layer(DefaultBodyLimit::max(11 * 1024 * 1024)),
        )
        .routes(routes!(media::download_post_media))
        .routes(routes!(notify::notification_check))
        .routes(routes!(notify::home_card))
        .split_for_parts();
    spec.info = Info::new("게시판 API", env!("CARGO_PKG_VERSION"));
    // The post-image JSON body may be omitted, but JSON null is rejected.
    // Utoipa's Option<T> would document both omission and null.
    spec.components
        .get_or_insert_with(Default::default)
        .schemas
        .insert(
            media::PostMediaRequest::name().into_owned(),
            media::PostMediaRequest::schema(),
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

    #[test]
    fn media_contract_preserves_binary_transfers_and_optional_password_body() {
        let spec = serde_json::to_value(router().1).unwrap();
        let media = &spec["paths"]["/files/media/{media_id}/content"];
        let post_media =
            &spec["paths"]["/files/community/posts/{post_id}/media/{media_id}"]["post"];
        let binary = json!({"type": "string", "format": "binary"});
        assert_eq!(
            media["put"]["requestBody"]["content"]["application/octet-stream"]["schema"],
            binary
        );

        for download in [&media["get"], post_media] {
            let response = &download["responses"]["200"];
            let content = response["content"].as_object().unwrap();
            assert_eq!(content.len(), 4);
            for mime in ["image/gif", "image/jpeg", "image/png", "image/webp"] {
                assert_eq!(content[mime]["schema"], binary);
            }
            assert!(response["headers"]["Content-Disposition"].is_object());
            assert!(response["headers"]["X-Miy-File-Sha256"].is_object());
        }

        let password_body = &post_media["requestBody"];
        assert_ne!(password_body["required"], true);
        assert_eq!(
            password_body["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/PostMediaRequest"
        );
    }

    #[test]
    fn company_announcements_require_company_scope() {
        let spec = serde_json::to_value(router().1).unwrap();
        let company = &spec["paths"]["/v1/announcements"];
        let parameters = company["get"]["parameters"].as_array().unwrap();
        let scope = parameters
            .iter()
            .find(|parameter| parameter["name"] == "scope")
            .unwrap();
        assert_eq!(scope["required"], true);
        let schemas = &spec["components"]["schemas"];
        assert_eq!(
            schemas["CompanyAnnouncementScope"]["enum"],
            json!(["company"])
        );
        assert_eq!(
            company["post"]["requestBody"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/CreateCompanyAnnouncementRequest"
        );
        assert!(
            schemas["CreateCompanyAnnouncementRequest"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("scope"))
        );
    }
}
