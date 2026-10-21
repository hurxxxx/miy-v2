use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Component, Path};

pub const VERSION: &str = "miy.app/v1alpha1";
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub contract_version: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub runtime: Runtime,
    pub ui: Ui,
    pub resources: Resources,
    pub migrations: Migrations,
    #[serde(default)]
    pub requested_permissions: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub profile: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ui {
    pub delivery: String,
    pub module_entry: String,
    pub module_export: String,
    pub contract_version: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    pub database: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Migrations {
    pub directory: String,
}
pub fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 48
        && s.as_bytes()[0].is_ascii_lowercase()
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
pub fn safe_relative(s: &str) -> bool {
    !s.is_empty()
        && !s.contains('\\')
        && Path::new(s)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}
pub fn validate(m: &Manifest) -> Result<(), String> {
    if m.contract_version != VERSION
        || m.runtime.profile != "rust-web-v1"
        || m.ui.contract_version != "miy.ui/v1alpha1"
    {
        return Err("unsupported_contract".into());
    }
    if !valid_id(&m.id)
        || m.name.trim().is_empty()
        || m.name.len() > 120
        || m.version.split('.').count() != 3
        || !m
            .version
            .split('.')
            .all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()))
    {
        return Err("invalid_manifest".into());
    }
    if m.ui.delivery != "module"
        || m.ui.module_export != "./AppRoot"
        || !safe_relative(&m.ui.module_entry)
        || m.resources.database != "postgresql"
        || m.migrations.directory != "migrations"
    {
        return Err("invalid_manifest".into());
    }
    if m.requested_permissions
        .iter()
        .any(|p| !["database"].contains(&p.as_str()))
    {
        return Err("unsupported_permission".into());
    }
    Ok(())
}
pub fn hash(s: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(s.as_ref()))
}
pub fn artifact_hash(root: &Path) -> anyhow::Result<String> {
    fn visit(root: &Path, path: &Path, out: &mut Vec<(String, String)>) -> anyhow::Result<()> {
        for ent in std::fs::read_dir(path)? {
            let ent = ent?;
            let p = ent.path();
            let meta = std::fs::symlink_metadata(&p)?;
            anyhow::ensure!(!meta.file_type().is_symlink(), "symlink_in_artifact");
            if meta.is_dir() {
                visit(root, &p, out)?;
            } else if meta.is_file() {
                let rel = p
                    .strip_prefix(root)?
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("non_utf8_path"))?
                    .to_string();
                anyhow::ensure!(safe_relative(&rel), "invalid_artifact_path");
                out.push((rel, hash(std::fs::read(p)?)));
            }
        }
        Ok(())
    }
    let mut entries = vec![];
    visit(root, root, &mut entries)?;
    entries.sort();
    Ok(hash(
        entries
            .into_iter()
            .map(|(p, h)| format!("{p}\0{h}\n"))
            .collect::<String>(),
    ))
}
pub fn verified_artifact(root: &Path, id: &str) -> anyhow::Result<std::path::PathBuf> {
    anyhow::ensure!(
        id.len() == 64
            && id
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "invalid_artifact_hash"
    );
    let path = root.join(id);
    anyhow::ensure!(artifact_hash(&path)? == id, "artifact_hash_mismatch");
    Ok(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_are_bounded() {
        for s in ["../x", "/tmp/x", "a/../b", "a\\b", ""] {
            assert!(!safe_relative(s), "{s}");
        }
        assert!(safe_relative("assets/entry.js"));
    }
    #[test]
    fn artifact_tampering_rejected() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a"), "one").unwrap();
        let h = artifact_hash(d.path()).unwrap();
        std::fs::write(d.path().join("a"), "two").unwrap();
        assert_ne!(h, artifact_hash(d.path()).unwrap());
    }
}
