use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
#[cfg(any(feature = "server", test))]
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
        && !s.contains(':')
        && s.split('/')
            .all(|p| !p.is_empty() && !matches!(p, "." | ".."))
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
        || m.ui.module_entry != "mf-manifest.json"
        || m.resources.database != "postgresql"
        || m.migrations.directory != "migrations"
    {
        return Err("invalid_manifest".into());
    }
    if m.requested_permissions.iter().any(|p| !valid_permission(p)) {
        return Err("unsupported_permission".into());
    }
    Ok(())
}
pub fn valid_permission(p: &str) -> bool {
    if matches!(p, "database" | "platform:directory:read") {
        return true;
    }
    let parts: Vec<_> = p.split(':').collect();
    parts.len() == 3
        && parts[0] == "app"
        && valid_id(parts[1])
        && matches!(parts[2], "read" | "write")
}
pub fn hash(s: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(s.as_ref()))
}
#[cfg(any(feature = "server", test))]
pub fn artifact_hash(root: &Path) -> anyhow::Result<String> {
    fn visit(root: &Path, path: &Path, out: &mut Vec<(String, String)>) -> anyhow::Result<()> {
        for ent in std::fs::read_dir(path)? {
            let ent = ent?;
            let p = ent.path();
            crate::platform::reject_reparse(&p)?;
            let meta = std::fs::symlink_metadata(&p)?;
            anyhow::ensure!(!meta.file_type().is_symlink(), "symlink_in_artifact");
            if meta.is_dir() {
                visit(root, &p, out)?;
            } else if meta.is_file() {
                let rel = p
                    .strip_prefix(root)?
                    .components()
                    .map(|part| match part {
                        Component::Normal(name) => name
                            .to_str()
                            .ok_or_else(|| anyhow::anyhow!("non_utf8_path")),
                        _ => anyhow::bail!("invalid_artifact_path"),
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?
                    .join("/");
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
#[cfg(feature = "server")]
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

pub fn get_contract() -> Value {
    json!({"version":VERSION,"bundleVersion":crate::bundle::VERSION,"uiVersion":"miy.ui/v1alpha1","manifestSchema":schemars::schema_for!(Manifest),"guidance":crate::bundle::GUIDANCE,"agentGuidance":crate::bundle::AGENT_GUIDANCE,"cliVersion":crate::cli_guidance::VERSION,"cliGuidance":crate::cli_guidance::manual(),"protectedFiles":crate::bundle::protected_hashes(),"sdkExample":"export default function AppRoot({runtime}) { return <button onClick={() => runtime.navigate('/detail/1')}>Open</button>; }"})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_are_bounded() {
        for s in [
            "../x", "/tmp/x", "a/../b", "a\\b", "", "C:drive", "a:b", "a//b", "a/./b",
        ] {
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
    #[test]
    fn nested_artifact_hash_uses_logical_paths_and_preserves_sql_bytes() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("migrations")).unwrap();
        let sql = b"SELECT 1;\r\n";
        std::fs::write(root.path().join("migrations/0001_initial.sql"), sql).unwrap();
        assert_eq!(
            artifact_hash(root.path()).unwrap(),
            hash(format!("migrations/0001_initial.sql\0{}\n", hash(sql)))
        );
    }
    #[test]
    fn ui_registration_requires_federation_manifest() {
        let mut value: serde_json::Value =
            serde_json::from_str(include_str!("../../../templates/rust-react/manifest.json"))
                .unwrap();
        value["id"] = serde_json::json!("example-app");
        let mut manifest: Manifest = serde_json::from_value(value).unwrap();
        assert!(validate(&manifest).is_ok());
        manifest.ui.module_entry = "remoteEntry.js".into();
        assert_eq!(validate(&manifest).unwrap_err(), "invalid_manifest");
    }
}
