//! One embedded source of truth for CLI scaffolding, checks and MCP guidance.
use crate::contract;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
pub const VERSION: &str = "0.6.1";
// 0.6.1 changes agent entry-point documentation only; its component API is identical to 0.6.0.
pub fn component_compatible(version: &str) -> bool {
    version == VERSION || version == "0.6.0"
}
pub const GUIDANCE: &str = include_str!("../../../templates/rust-react/docs/CONTRACT.md");
pub const AGENT_GUIDANCE: &str = include_str!("../../../templates/rust-react/AGENTS.md");
include!(concat!(env!("OUT_DIR"), "/template.rs"));
pub fn template() -> &'static [(&'static str, &'static str)] {
    TEMPLATE
}
fn protected(path: &str) -> bool {
    path.starts_with("sdk/")
        || path.starts_with(".agents/skills/")
        || path.starts_with(".claude/skills/")
        || matches!(
            path,
            "Dockerfile"
                | "miy.components.json"
                | "rust-toolchain.toml"
                | "AGENTS.md"
                | "CLAUDE.md"
                | "docs/CONTRACT.md"
                | "docs/CLI.md"
                | "docs/ACCEPTANCE.md"
                | "migrations/0001_initial.sql"
        )
}
pub fn protected_hashes() -> BTreeMap<&'static str, String> {
    TEMPLATE
        .iter()
        .filter(|(p, _)| protected(p))
        .map(|(p, c)| (*p, contract::hash(c)))
        .collect()
}
pub fn verify(root: &Path) -> Result<()> {
    let lock: Value = serde_json::from_slice(
        &std::fs::read(root.join("miy.bundle.json"))
            .context("missing_bundle_lock: regenerate with miy app init")?,
    )?;
    ensure!(
        lock["bundleVersion"] == VERSION,
        "bundle_version_mismatch: use the CLI shipped with this app"
    );
    ensure!(
        lock["protectedFiles"] == serde_json::to_value(protected_hashes())?,
        "bundle_lock_modified"
    );
    for (path, expected) in protected_hashes() {
        let actual = root.join(path);
        ensure!(
            std::fs::symlink_metadata(&actual)?.is_file(),
            "invalid_bundle_file: {path}"
        );
        ensure!(
            contract::hash(std::fs::read(actual)?) == expected,
            "protected_bundle_modified: {path}"
        );
    }
    Ok(())
}
// Lex top-level statement tokens, skipping SQL comments, strings and dollar-quoted
// procedural bodies. This checks the supported migration contract, not SQL semantics.
fn statements(sql: &str) -> Result<Vec<Vec<String>>> {
    let bytes = sql.as_bytes();
    let mut i = 0;
    let mut result = vec![vec![]];
    while i < bytes.len() {
        if bytes[i..].starts_with(b"--") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if bytes[i..].starts_with(b"/*") {
            i += 2;
            let mut depth = 1;
            while i < bytes.len() && depth > 0 {
                if bytes[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if bytes[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            ensure!(depth == 0, "unterminated_migration_comment");
        } else if matches!(bytes[i], b'\'' | b'"') {
            let quote = bytes[i];
            i += 1;
            let mut closed = false;
            while i < bytes.len() {
                if bytes[i] == quote {
                    i += 1;
                    if i < bytes.len() && bytes[i] == quote {
                        i += 1;
                    } else {
                        closed = true;
                        break;
                    }
                } else if bytes[i] == b'\\' && quote == b'\'' {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            ensure!(closed, "unterminated_migration_string");
            result.last_mut().unwrap().push("<quoted>".into());
        } else if bytes[i] == b'$' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'$' {
                i += 1;
                let delimiter = &sql[start..i];
                let end = sql[i..]
                    .find(delimiter)
                    .context("unterminated_migration_body")?;
                i += end + delimiter.len();
                result.last_mut().unwrap().push("<body>".into());
            }
        } else if bytes[i] == b';' {
            result.push(vec![]);
            i += 1;
        } else if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            result
                .last_mut()
                .unwrap()
                .push(sql[start..i].to_ascii_uppercase());
        } else {
            i += 1;
        }
    }
    Ok(result)
}
pub fn validate_sql(sql: &str) -> Result<()> {
    ensure!(
        !sql.contains("-- no-transaction"),
        "nontransactional_migration_unsupported"
    );
    for tokens in statements(sql)? {
        let first = tokens.first().map(String::as_str).unwrap_or("");
        ensure!(
            !matches!(
                first,
                "BEGIN" | "COMMIT" | "END" | "ROLLBACK" | "ABORT" | "SAVEPOINT" | "RELEASE"
            ) && !(matches!(first, "START" | "PREPARE" | "SET")
                && tokens.iter().any(|t| t == "TRANSACTION")),
            "explicit_migration_transaction: SQLx owns transactions; remove BEGIN/COMMIT/ROLLBACK and transaction-control statements"
        );
    }
    Ok(())
}
pub fn migrations(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut hashes = BTreeMap::new();
    let mut versions = BTreeSet::new();
    for file in std::fs::read_dir(root)? {
        let file = file?;
        ensure!(file.file_type()?.is_file(), "invalid_migration_file");
        let name = file
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("invalid_migration_name"))?;
        let (number, description) = name.split_once('_').context("invalid_migration_name")?;
        let version: i64 = number.parse().context("invalid_migration_version")?;
        ensure!(
            number.bytes().all(|c| c.is_ascii_digit())
                && version > 0
                && description.ends_with(".sql")
                && description.len() > 4
                && !description.ends_with(".down.sql")
                && !description.ends_with(".up.sql"),
            "invalid_migration_name: {name}"
        );
        ensure!(
            versions.insert(version),
            "duplicate_migration_version: {version}"
        );
        let sql = std::fs::read_to_string(file.path())?;
        validate_sql(&sql).with_context(|| format!("migration_contract: {name}"))?;
        hashes.insert(name, contract::hash(sql));
    }
    ensure!(!hashes.is_empty(), "missing_migrations");
    Ok(hashes)
}
pub fn ui(root: &Path) -> Result<()> {
    let v: Value = serde_json::from_slice(&std::fs::read(root.join("mf-manifest.json"))?)?;
    ensure!(
        v["metaData"]["remoteEntry"]["type"] == "module",
        "invalid_ui_module_type"
    );
    let entry = v["metaData"]["remoteEntry"]["name"]
        .as_str()
        .context("missing_ui_remote_entry")?;
    ensure!(
        contract::safe_relative(entry) && root.join(entry).is_file(),
        "missing_ui_remote_entry"
    );
    let exposes = v["exposes"].as_array().context("missing_ui_exposes")?;
    ensure!(
        exposes.iter().any(|e| e["path"] == "./AppRoot"),
        "missing_ui_AppRoot"
    );
    for item in exposes
        .iter()
        .chain(v["shared"].as_array().into_iter().flatten())
    {
        for kind in ["js", "css"] {
            for loading in ["sync", "async"] {
                if let Some(files) = item["assets"][kind][loading].as_array() {
                    for file in files {
                        let path = file.as_str().context("invalid_ui_asset")?;
                        ensure!(
                            contract::safe_relative(path) && root.join(path).is_file(),
                            "missing_ui_asset: {path}"
                        );
                    }
                }
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migration_transaction_control_is_rejected_without_rejecting_function_bodies() {
        for sql in [
            "-- comment\n begin; SELECT 1; commit;",
            "/* nested /* x */ x */ COMMIT;",
            "SELECT 1; START /*x*/ TRANSACTION;",
            "ROLLBACK TO SAVEPOINT x;",
            "SET SESSION CHARACTERISTICS AS TRANSACTION READ ONLY;",
            "-- no-transaction\nCREATE TABLE x(id int);",
        ] {
            assert!(validate_sql(sql).is_err(), "{sql}");
        }
        for sql in [
            "CREATE TABLE x(message text DEFAULT 'BEGIN;COMMIT');",
            "DO $body$ BEGIN RAISE NOTICE 'COMMIT'; END $body$;",
            "-- BEGIN\n/* COMMIT */ SELECT 'x'';BEGIN';",
            "CREATE FUNCTION x() RETURNS void AS $$ BEGIN RETURN; END $$ LANGUAGE plpgsql;",
        ] {
            assert!(validate_sql(sql).is_ok(), "{sql}");
        }
    }
    #[test]
    fn duplicate_migration_versions_are_rejected() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("0001_a.sql"), "SELECT 1;").unwrap();
        std::fs::write(d.path().join("1_b.sql"), "SELECT 2;").unwrap();
        assert!(
            migrations(d.path())
                .unwrap_err()
                .to_string()
                .contains("duplicate_migration_version")
        );
    }
}
