//! Bounded typed JSON input and authorized-result projection.
use crate::cli_error::{Category, CliError};
use anyhow::Result;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use std::io::Read;
const MAX_INPUT: usize = 1024 * 1024;
fn invalid(code: &'static str, message: &'static str) -> CliError {
    CliError::new(Category::Usage, code, message)
}
pub fn read<T: DeserializeOwned>(source: &str) -> Result<T> {
    let reader: Box<dyn Read> = if source == "-" {
        Box::new(std::io::stdin())
    } else if let Some(path) = source.strip_prefix('@').filter(|p| !p.is_empty()) {
        Box::new(
            std::fs::File::open(path)
                .map_err(|_| invalid("input_unavailable", "Cannot read the input file."))?,
        )
    } else {
        return Err(invalid(
            "invalid_input_source",
            "Use --input @file or --input - for a JSON document.",
        )
        .into());
    };
    let mut bytes = Vec::new();
    reader
        .take((MAX_INPUT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("input_unavailable", "Cannot read JSON input."))?;
    if bytes.len() > MAX_INPUT {
        return Err(invalid("input_too_large", "JSON input exceeds 1 MiB.").into());
    }
    // UTF-8 BOM from Windows PowerShell file writers is accepted explicitly.
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    serde_json::from_slice(bytes).map_err(|_|invalid("invalid_input_schema","Input does not match the command schema; unknown fields and duplicate keys are rejected.").into())
}
fn paths(fields: &str) -> Result<Vec<Vec<&str>>> {
    if fields.len() > 2048 {
        return Err(invalid("invalid_fields", "Field selection is too long.").into());
    }
    let paths: Vec<Vec<_>> = fields
        .split(',')
        .map(|field| field.trim().split('.').collect())
        .collect();
    if paths.is_empty()
        || paths.len() > 64
        || paths.iter().any(|p| {
            p.len() > 16
                || p.iter().any(|s| {
                    s.is_empty()
                        || !s
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
                })
        })
    {
        return Err(invalid(
            "invalid_fields",
            "Select comma-separated field paths, for example apps.id,apps.name,nextCursor.",
        )
        .into());
    }
    Ok(paths)
}
pub fn validate_fields(command: &str, fields: &str) -> Result<()> {
    let _ = paths(fields)?;
    let read_only = matches!(
        command,
        "describe"
            | "doctor"
            | "context list"
            | "context show"
            | "auth status"
            | "auth sessions list"
            | "app list"
            | "app show"
            | "app status"
            | "job list"
            | "job get"
            | "registration list"
            | "user list"
            | "grant list"
            | "component list"
            | "component show"
            | "component check"
    );
    if !read_only {
        return Err(invalid(
            "fields_not_supported",
            "--fields is available only for structured read commands.",
        )
        .into());
    }
    Ok(())
}
pub fn project(value: &Value, fields: &str) -> Result<Value> {
    let paths = paths(fields)?;
    let borrowed: Vec<&[&str]> = paths.iter().map(Vec::as_slice).collect();
    select(value, &borrowed)
}
fn select(value: &Value, paths: &[&[&str]]) -> Result<Value> {
    if paths.iter().any(|p| p.is_empty()) {
        return Ok(value.clone());
    }
    if let Value::Array(items) = value {
        return Ok(Value::Array(
            items
                .iter()
                .map(|item| select(item, paths))
                .collect::<Result<_>>()?,
        ));
    }
    let object = value.as_object().ok_or_else(|| {
        invalid(
            "unknown_field",
            "The selected field is absent or is not an object.",
        )
    })?;
    let mut selected = Map::new();
    let mut groups = std::collections::BTreeMap::<&str, Vec<&[&str]>>::new();
    for path in paths {
        groups.entry(path[0]).or_default().push(&path[1..]);
    }
    for (key, rest) in groups {
        let field = object.get(key).ok_or_else(|| {
            invalid(
                "unknown_field",
                "A selected field is not present in this result.",
            )
        })?;
        selected.insert(
            key.into(),
            if field.is_null() {
                Value::Null
            } else {
                select(field, &rest)?
            },
        );
    }
    Ok(Value::Object(selected))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn nested_array_projection() {
        let v = json!({"apps":[{"id":"a","name":"A","secret":"excluded"}],"nextCursor":null});
        assert_eq!(
            project(&v, "apps.id,nextCursor").unwrap(),
            json!({"apps":[{"id":"a"}],"nextCursor":null})
        );
        assert!(project(&v, "apps.missing").is_err());
        assert!(validate_fields("app remove", "id").is_err());
        assert!(project(&v, "apps..id").is_err());
        assert_eq!(project(&json!({"contexts":{"work-prod":{"origin":"https://example.test","userId":"hidden"}}}),"contexts.work-prod.origin").unwrap(),json!({"contexts":{"work-prod":{"origin":"https://example.test"}}}));
    }
}
