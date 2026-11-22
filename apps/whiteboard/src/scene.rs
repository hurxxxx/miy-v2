//! Excalidraw scenes as AI-DO stored them (domains/whiteboard/scene_state.py),
//! plus the miy rule that image bytes live in the core storage.
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

/// AI-DO `PERSISTED_SCENE_APP_STATE_KEYS`: editor state that belongs to the
/// board. Everything else in `appState` (selection, scroll, zoom, name) is
/// per viewer and never makes a save count as a change.
pub const PERSISTED_APP_STATE_KEYS: [&str; 3] =
    ["gridModeEnabled", "gridSize", "viewBackgroundColor"];

/// AI-DO `empty_scene()`.
pub fn empty_scene() -> Value {
    json!({"elements": [], "appState": {}, "files": {}})
}

/// AI-DO `scene_for_compare`.
pub fn for_compare(scene: &Value) -> Value {
    let Some(object) = scene.as_object() else {
        return for_compare(&empty_scene());
    };
    let elements = match object.get("elements") {
        Some(Value::Array(items)) => Value::Array(items.clone()),
        _ => json!([]),
    };
    let app_state = match object.get("appState") {
        Some(Value::Object(state)) => {
            let mut kept = Map::new();
            for key in PERSISTED_APP_STATE_KEYS {
                if let Some(value) = state.get(key) {
                    kept.insert(key.to_string(), value.clone());
                }
            }
            Value::Object(kept)
        }
        _ => json!({}),
    };
    let files = match object.get("files") {
        Some(Value::Object(files)) => Value::Object(files.clone()),
        _ => json!({}),
    };
    json!({"elements": elements, "appState": app_state, "files": files})
}

/// AI-DO `scene_matches` (sorted-key JSON equality of the comparable parts).
pub fn matches(left: &Value, right: &Value) -> bool {
    for_compare(left) == for_compare(right)
}

/// Why a scene's `files` entry is refused.
#[derive(Debug, PartialEq, Eq)]
pub enum FileProblem {
    /// Not an object, or an inline `dataURL` (bytes belong in the storage).
    Inline(String),
    /// The entry names no stored image of this board.
    Missing(String),
}

/// The storage row IDs named by `scene.files` (`{"<excalidraw id>": {"id",
/// "mimeType", "storage": "<row id>", …}}`). An entry with image bytes
/// (`dataURL`) or without `storage` is refused: AI-DO kept data URLs in the
/// scene; miy keeps the bytes in the core storage (10 MiB per file).
pub fn storage_ids(scene: &Value) -> Result<BTreeSet<String>, FileProblem> {
    let mut ids = BTreeSet::new();
    let Some(files) = scene.get("files").and_then(Value::as_object) else {
        return Ok(ids);
    };
    for (key, entry) in files {
        let Some(entry) = entry.as_object() else {
            return Err(FileProblem::Inline(key.clone()));
        };
        if entry.contains_key("dataURL") {
            return Err(FileProblem::Inline(key.clone()));
        }
        match entry.get("storage").and_then(Value::as_str) {
            Some(id) if !id.is_empty() && id.len() <= 36 => {
                ids.insert(id.to_string());
            }
            _ => return Err(FileProblem::Missing(key.clone())),
        }
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_editor_state_is_not_a_change() {
        // AI-DO test_whiteboard_hub / test_update_command_preserves_updated_at_for_noop_scene.
        let stored = json!({
            "elements": [{"id": "text-1", "type": "text", "text": "hello"}],
            "appState": {"viewBackgroundColor": "#ffffff"},
            "files": {},
        });
        let incoming = json!({
            "elements": [{"id": "text-1", "type": "text", "text": "hello"}],
            "appState": {
                "viewBackgroundColor": "#ffffff",
                "name": "Launch Map v2",
                "selectedElementIds": {"text-1": true},
                "scrollX": 12,
                "zoom": {"value": 1},
            },
            "files": {},
            "type": "excalidraw",
            "version": 2,
        });
        assert!(matches(&stored, &incoming));
        assert!(matches(
            &empty_scene(),
            &json!({"elements": [], "appState": {"name": "x", "scrollX": 32}, "files": {}, "type": "excalidraw"})
        ));
        assert!(!matches(
            &stored,
            &json!({"elements": [], "appState": {"viewBackgroundColor": "#ffffff"}, "files": {}})
        ));
        assert!(!matches(
            &empty_scene(),
            &json!({"elements": [], "appState": {"gridModeEnabled": true}})
        ));
    }

    #[test]
    fn malformed_parts_compare_as_empty() {
        assert!(matches(&json!(null), &empty_scene()));
        assert!(matches(
            &json!({"elements": "x", "appState": [], "files": 3}),
            &empty_scene()
        ));
    }

    #[test]
    fn files_must_name_stored_images() {
        let ok =
            json!({"files": {"f1": {"id": "f1", "mimeType": "image/png", "storage": "row-1"}}});
        assert_eq!(
            storage_ids(&ok).unwrap().into_iter().collect::<Vec<_>>(),
            vec!["row-1"]
        );
        let inline = json!({"files": {"f1": {"id": "f1", "dataURL": "data:image/png;base64,AA", "storage": "row-1"}}});
        assert_eq!(storage_ids(&inline), Err(FileProblem::Inline("f1".into())));
        let missing = json!({"files": {"f1": {"id": "f1", "mimeType": "image/png"}}});
        assert_eq!(
            storage_ids(&missing),
            Err(FileProblem::Missing("f1".into()))
        );
        assert!(storage_ids(&empty_scene()).unwrap().is_empty());
    }
}
