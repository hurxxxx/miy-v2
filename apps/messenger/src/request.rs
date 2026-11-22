//! AI-DO domains/dm/request_normalization.py.
//!
//! miy: route IDs may be up to 256 characters (AI-DO 36). Conversation,
//! message and attachment IDs are still the app's UUIDs; user IDs are the
//! platform's subjects, which may be longer than a UUID.
use crate::error::{ApiError, ApiResult};

pub const DM_ROUTE_ID_MAX_LENGTH: usize = 256;
pub const DM_MESSAGE_BODY_MAX_LENGTH: usize = 120_000;
pub const DM_MESSAGE_ATTACHMENT_IDS_MAX_LENGTH: usize = 10;
pub const DM_PARTICIPANT_IDS_MAX_LENGTH: usize = 50;
pub const DM_CONVERSATION_TITLE_MAX_LENGTH: usize = 140;

pub fn is_dm_route_id(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= DM_ROUTE_ID_MAX_LENGTH
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Path parameters (FastAPI `Path(min_length, max_length, pattern)`).
pub fn route_id(value: &str) -> ApiResult<&str> {
    if is_dm_route_id(value) {
        Ok(value)
    } else {
        Err(ApiError::validation("path: DM route ID is invalid"))
    }
}

fn invalid_id() -> ApiError {
    ApiError::validation("DM route ID is invalid")
}

pub fn normalize_optional_route_id(value: Option<String>) -> ApiResult<Option<String>> {
    match value {
        None => Ok(None),
        Some(value) => {
            let normalized = value.trim().to_string();
            if is_dm_route_id(&normalized) {
                Ok(Some(normalized))
            } else {
                Err(invalid_id())
            }
        }
    }
}

pub fn normalize_route_id_list(
    value: Option<Vec<String>>,
    allow_empty: bool,
) -> ApiResult<Vec<String>> {
    let Some(value) = value else {
        return if allow_empty {
            Ok(Vec::new())
        } else {
            Err(ApiError::validation("Field required"))
        };
    };
    let mut ids = Vec::with_capacity(value.len());
    for item in value {
        let normalized = item.trim().to_string();
        if !is_dm_route_id(&normalized) {
            return Err(invalid_id());
        }
        ids.push(normalized);
    }
    if !allow_empty && ids.is_empty() {
        return Ok(ids);
    }
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    if unique.len() != ids.len() {
        return Err(ApiError::validation("DM route IDs must be unique"));
    }
    Ok(ids)
}

pub fn normalize_optional_title(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim().to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

pub fn normalize_message_body(value: Option<String>) -> String {
    value
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

/// `conversation_lifecycle.unique_user_ids`: strip, skip empty, keep order.
pub fn unique_ids<'a>(ids: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for id in ids {
        let normalized = id.trim();
        if normalized.is_empty() || !seen.insert(normalized.to_string()) {
            continue;
        }
        result.push(normalized.to_string());
    }
    result
}

pub fn is_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value)
        .is_ok_and(|parsed| parsed.hyphenated().to_string() == value.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_ids_follow_ai_do_pattern() {
        assert!(is_dm_route_id("abc-DEF_123"));
        assert!(is_dm_route_id(&"a".repeat(256)));
        assert!(!is_dm_route_id(&"a".repeat(257)));
        for bad in ["", "a/b", "a.b", "../x", "가"] {
            assert!(!is_dm_route_id(bad), "{bad}");
        }
    }

    #[test]
    fn id_lists_are_stripped_and_unique() {
        assert_eq!(
            normalize_route_id_list(Some(vec![" a ".into(), "b".into()]), false).unwrap(),
            vec!["a", "b"]
        );
        assert!(normalize_route_id_list(Some(vec!["a".into(), " a".into()]), true).is_err());
        assert!(normalize_route_id_list(Some(vec!["a b".into()]), true).is_err());
        assert_eq!(
            normalize_route_id_list(None, true).unwrap(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn titles_and_bodies_are_stripped() {
        assert_eq!(normalize_optional_title(Some("  ".into())), None);
        assert_eq!(
            normalize_optional_title(Some(" 팀 ".into())),
            Some("팀".into())
        );
        assert_eq!(normalize_message_body(Some("  hi \n".into())), "hi");
        assert_eq!(normalize_message_body(None), "");
    }
}
