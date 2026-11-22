//! Text of a block page for the integrated search: AI-DO
//! `search/service.py::_extract_blocks_text` (every `text` string found
//! anywhere in the BlockNote JSON, depth first) joined like AI-DO's
//! `content_text.extract_page_text` callers did. HTML pages are excluded.
use serde_json::Value;

/// Upper bound kept per page; longer text is cut at a character boundary.
const MAX_SEARCH_TEXT_CHARS: usize = 200_000;

pub fn extract_blocks_text(blocks: Option<&Value>) -> String {
    let mut parts: Vec<&str> = Vec::new();
    fn visit<'a>(value: &'a Value, parts: &mut Vec<&'a str>) {
        match value {
            Value::Object(map) => {
                if let Some(Value::String(text)) = map.get("text") {
                    parts.push(text);
                }
                for child in map.values() {
                    visit(child, parts);
                }
            }
            Value::Array(items) => {
                for child in items {
                    visit(child, parts);
                }
            }
            _ => {}
        }
    }
    if let Some(blocks) = blocks {
        visit(blocks, &mut parts);
    }
    let joined = parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if joined.chars().count() <= MAX_SEARCH_TEXT_CHARS {
        joined
    } else {
        joined.chars().take(MAX_SEARCH_TEXT_CHARS).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn block_text_collects_every_text_value() {
        // AI-DO test_content_text_extracts_indexable_html_and_block_content (block half)
        let blocks = json!([
            {"type": "heading", "props": {"level": 1}, "content": [{"type": "text", "text": "Roadmap"}]},
            {"type": "paragraph", "content": [{"type": "text", "text": "Ship", "styles": {}},
                                              {"type": "text", "text": " docs"}],
             "children": [{"type": "paragraph", "content": [{"type": "text", "text": "nested"}]}]},
            {"type": "table", "content": {"type": "tableContent", "rows": [
                {"cells": [[{"type": "text", "text": "cell"}]]}]}}
        ]);
        // serde_json visits object keys in sorted order ("children" before
        // "content"); Python kept insertion order. Search only needs the words.
        assert_eq!(
            extract_blocks_text(Some(&blocks)),
            "Roadmap nested Ship docs cell"
        );
        assert_eq!(extract_blocks_text(None), "");
        assert_eq!(extract_blocks_text(Some(&json!([]))), "");
    }
}
