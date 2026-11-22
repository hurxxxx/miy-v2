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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_contract: Option<ApiContract>,
    #[serde(default)]
    pub requested_permissions: Vec<String>,
    /// Read-only endpoints the platform calls for the signed-in person
    /// (docs/CORE-PERMISSIONS.md "플랫폼 → 앱 호출"). Paths are under the app's
    /// API, like the UI's `runtime.api` paths.
    #[serde(default, skip_serializing_if = "PlatformEndpoints::is_empty")]
    pub platform_endpoints: PlatformEndpoints,
    /// Narrow screens the portal can open in a side panel next to another app
    /// (docs/APP-FRAMES.md "보조 프레임"). At most four.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub panels: Vec<Panel>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
pub enum ApiContract {
    #[serde(rename = "openapi-v1")]
    OpenApiV1,
}
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Panel {
    /// Lowercase slug, unique in the manifest.
    pub id: String,
    /// Shown on the panel's header and in the panel menu, 1-40 characters.
    pub title: String,
    /// App-relative path of the panel screen (no query).
    pub path: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlatformEndpoints {
    /// POST `{ "items": [{ "groupKey", "resourceId" }] }` answering
    /// `{ "allowed": ["groupKey", ...] }`: which of the person's notifications
    /// still point at something they may open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notification_check: Option<String>,
    /// POST `{ "timeZone", "locale" }` answering `{ "title", "count"?, "items": [{ "title",
    /// "subtitle"?, "path"?, "at"? }] }` (at most five items): the person's
    /// summary card on the portal home.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_card: Option<String>,
    /// POST `{ "query", "limit", "timeZone", "locale" }` answering `{ "items":
    /// [{ "title", "snippet"?, "path"?, "at"?, "kind"? }] }`: what this person
    /// may open that matches, for the portal's integrated search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    /// POST `{ "fileId", "reference" }` answering `{ "allowed": bool }`: right
    /// before a download link sends bytes, whether this person may still read
    /// that file through the app's own ACL. Required for download links.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_access: Option<String>,
}
impl PlatformEndpoints {
    pub fn is_empty(&self) -> bool {
        self.notification_check.is_none()
            && self.home_card.is_none()
            && self.search.is_none()
            && self.file_access.is_none()
    }
}
/// An app API path for a platform endpoint: absolute, no query, no dot segments.
pub fn valid_endpoint_path(path: &str) -> bool {
    path.starts_with('/')
        && !path.starts_with("//")
        && path.len() <= 200
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-_".contains(&b))
        && !path.split('/').any(|part| part == "." || part == "..")
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
    if [
        &m.platform_endpoints.notification_check,
        &m.platform_endpoints.home_card,
        &m.platform_endpoints.search,
        &m.platform_endpoints.file_access,
    ]
    .into_iter()
    .flatten()
    .any(|path| !valid_endpoint_path(path))
    {
        return Err("invalid_platform_endpoint".into());
    }
    let mut panel_ids = std::collections::HashSet::new();
    if m.panels.len() > 4
        || !m.panels.iter().all(|panel| {
            valid_id(&panel.id)
                && panel_ids.insert(panel.id.as_str())
                && !panel.title.trim().is_empty()
                && panel.title.chars().count() <= 40
                && !panel.title.chars().any(char::is_control)
                && valid_endpoint_path(&panel.path)
        })
    {
        return Err("invalid_panel".into());
    }
    Ok(())
}
pub fn valid_permission(p: &str) -> bool {
    if matches!(
        p,
        "database"
            | "platform:directory:read"
            | "platform:files:read"
            | "platform:files:write"
            | "platform:notifications:write"
            | "frame:clipboard-write"
            | "frame:fullscreen"
    ) {
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
/// Limits of the platform contract directory (`/miy`) a release carries:
/// manifest, build record, migrations and the UI module.
#[cfg(any(feature = "server", test))]
pub const MAX_ARTIFACT_BYTES: u64 = 256 * 1024 * 1024;
#[cfg(any(feature = "server", test))]
pub const MAX_ARTIFACT_FILE_BYTES: u64 = 64 * 1024 * 1024;
#[cfg(any(feature = "server", test))]
pub const MAX_ARTIFACT_ENTRIES: usize = 10_000;

#[cfg(any(feature = "server", test))]
fn hash_file(path: &Path) -> anyhow::Result<String> {
    let mut hasher = Sha256::new();
    std::io::copy(&mut std::fs::File::open(path)?, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

/// Only directories and regular files within the limits are an artifact. A
/// link, pipe, socket or device is refused instead of skipped: later readers
/// open files by name and must never meet one.
#[cfg(any(feature = "server", test))]
pub fn artifact_hash(root: &Path) -> anyhow::Result<String> {
    fn visit(
        root: &Path,
        path: &Path,
        out: &mut Vec<(String, String)>,
        seen: &mut (usize, u64),
    ) -> anyhow::Result<()> {
        for ent in std::fs::read_dir(path)? {
            let ent = ent?;
            let p = ent.path();
            crate::platform::reject_reparse(&p)?;
            let meta = std::fs::symlink_metadata(&p)?;
            anyhow::ensure!(!meta.file_type().is_symlink(), "symlink_in_artifact");
            seen.0 += 1;
            anyhow::ensure!(seen.0 <= MAX_ARTIFACT_ENTRIES, "artifact_entry_limit");
            if meta.is_dir() {
                visit(root, &p, out, seen)?;
            } else {
                anyhow::ensure!(meta.is_file(), "unsupported_artifact_entry");
                anyhow::ensure!(meta.len() <= MAX_ARTIFACT_FILE_BYTES, "artifact_size_limit");
                seen.1 += meta.len();
                anyhow::ensure!(seen.1 <= MAX_ARTIFACT_BYTES, "artifact_size_limit");
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
                out.push((rel, hash_file(&p)?));
            }
        }
        Ok(())
    }
    let mut entries = vec![];
    visit(root, root, &mut entries, &mut (0, 0))?;
    entries.sort();
    Ok(hash(
        entries
            .into_iter()
            .map(|(p, h)| format!("{p}\0{h}\n"))
            .collect::<String>(),
    ))
}

/// Unpack the tar stream of an image's `/miy` directory into a new directory.
/// The image is developer supplied: only directories and regular files with
/// safe relative names are written, within the artifact limits, so a link,
/// device, pipe or oversized member never reaches the platform's disk.
#[cfg(any(feature = "server", test))]
pub fn unpack_artifact(archive: impl std::io::Read, destination: &Path) -> anyhow::Result<()> {
    use std::io::Read;
    let mut tar = tar::Archive::new(archive);
    let (mut entries, mut total) = (0usize, 0u64);
    for entry in tar.entries()? {
        let mut entry = entry?;
        entries += 1;
        anyhow::ensure!(entries <= MAX_ARTIFACT_ENTRIES, "artifact_entry_limit");
        let raw = std::str::from_utf8(&entry.path_bytes())
            .map_err(|_| anyhow::anyhow!("non_utf8_path"))?
            .to_owned();
        let name = raw.strip_prefix("./").unwrap_or(&raw).trim_end_matches('/');
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            if !name.is_empty() {
                anyhow::ensure!(safe_relative(name), "invalid_artifact_path");
                std::fs::create_dir_all(destination.join(name))?;
            }
            continue;
        }
        anyhow::ensure!(kind.is_file(), "unsupported_artifact_entry");
        anyhow::ensure!(safe_relative(name), "invalid_artifact_path");
        let size = entry.size();
        anyhow::ensure!(size <= MAX_ARTIFACT_FILE_BYTES, "artifact_size_limit");
        total += size;
        anyhow::ensure!(total <= MAX_ARTIFACT_BYTES, "artifact_size_limit");
        let path = destination.join(name);
        std::fs::create_dir_all(
            path.parent()
                .ok_or_else(|| anyhow::anyhow!("invalid_artifact_path"))?,
        )?;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        let written = std::io::copy(&mut (&mut entry).take(size), &mut file)?;
        anyhow::ensure!(written == size, "truncated_artifact_member");
    }
    Ok(())
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
    json!({"version":VERSION,"bundleVersion":crate::bundle::VERSION,"templateVersion":crate::bundle::template_version(),"guidanceVersion":crate::cli_guidance::GUIDANCE_VERSION,"uiVersion":"miy.ui/v1alpha1","manifestSchema":schemars::schema_for!(Manifest),"guidance":crate::bundle::GUIDANCE,"agentGuidance":crate::bundle::AGENT_GUIDANCE,"cliVersion":crate::cli_guidance::VERSION,"cliGuidance":crate::cli_guidance::manual(),"protectedFiles":crate::bundle::protected_hashes(),"sdkExample":"export default function AppRoot({runtime}) { return <button onClick={() => runtime.navigate('/detail/1')}>Open</button>; }"})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn archive(members: &[(&str, tar::EntryType, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, kind, bytes) in members {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(*kind);
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            if kind.is_symlink() || kind.is_hard_link() {
                header.set_link_name("manifest.json").unwrap();
            }
            // Raw name bytes: the builder's own path checks must not hide a bad member.
            let field = &mut header.as_old_mut().name;
            field[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            builder.append(&header, *bytes).unwrap();
        }
        builder.into_inner().unwrap()
    }

    #[test]
    fn image_contract_unpacks_only_bounded_regular_files() {
        use tar::EntryType as Kind;
        let root = tempfile::tempdir().unwrap();
        let good = archive(&[
            ("./", Kind::Directory, b""),
            ("./manifest.json", Kind::Regular, b"{}"),
            ("./ui/", Kind::Directory, b""),
            ("./ui/assets/entry.js", Kind::Regular, b"export{}"),
        ]);
        let target = root.path().join("good");
        std::fs::create_dir(&target).unwrap();
        unpack_artifact(good.as_slice(), &target).unwrap();
        assert_eq!(
            std::fs::read(target.join("ui/assets/entry.js")).unwrap(),
            b"export{}"
        );
        artifact_hash(&target).unwrap();
        for (label, members, code) in [
            (
                "symlink",
                vec![("./manifest.json", Kind::Symlink, &b""[..])],
                "unsupported_artifact_entry",
            ),
            (
                "hard link",
                vec![("./build.json", Kind::Link, &b""[..])],
                "unsupported_artifact_entry",
            ),
            (
                "pipe",
                vec![("./manifest.json", Kind::Fifo, &b""[..])],
                "unsupported_artifact_entry",
            ),
            (
                "device",
                vec![("./manifest.json", Kind::Char, &b""[..])],
                "unsupported_artifact_entry",
            ),
            (
                "parent",
                vec![("./../escape", Kind::Regular, &b"x"[..])],
                "invalid_artifact_path",
            ),
            (
                "absolute",
                vec![("/etc/escape", Kind::Regular, &b"x"[..])],
                "invalid_artifact_path",
            ),
            (
                "duplicate",
                vec![
                    ("./a", Kind::Regular, &b"x"[..]),
                    ("./a", Kind::Regular, &b"y"[..]),
                ],
                "",
            ),
        ] {
            let target = root.path().join(label);
            std::fs::create_dir(&target).unwrap();
            let error = unpack_artifact(archive(&members).as_slice(), &target).unwrap_err();
            if !code.is_empty() {
                assert_eq!(error.to_string(), code, "{label}");
            }
            assert!(!root.path().join("escape").exists(), "{label}");
        }
        // A declared size above the per-file limit is refused before any byte is read.
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(Kind::Regular);
        header.set_size(MAX_ARTIFACT_FILE_BYTES + 1);
        header.set_mode(0o644);
        header.as_old_mut().name[..5].copy_from_slice(b"./big");
        header.set_cksum();
        let target = root.path().join("large");
        std::fs::create_dir(&target).unwrap();
        let error = unpack_artifact(header.as_bytes().as_slice(), &target).unwrap_err();
        assert_eq!(error.to_string(), "artifact_size_limit");
        assert!(!target.join("big").exists());
    }

    #[cfg(unix)]
    #[test]
    fn artifact_hash_refuses_entries_that_are_not_files_or_directories() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("manifest.json"), "{}").unwrap();
        artifact_hash(root.path()).unwrap();
        let socket = root.path().join("build.json");
        let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        assert_eq!(
            artifact_hash(root.path()).unwrap_err().to_string(),
            "unsupported_artifact_entry"
        );
    }

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
