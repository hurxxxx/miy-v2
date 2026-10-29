//! One bounded source format shared by the portable client and Linux worker.
//! Only committed regular files are selected; extraction never follows links.
use crate::{
    bundle,
    contract::{self, Manifest},
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    path::Path,
    process::Command,
};

pub const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_FILES: usize = 4096;

pub struct Packed {
    pub source_commit: String,
    pub archive_hash: String,
    pub size_bytes: u64,
    pub manifest: Manifest,
}

pub fn valid_commit(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// An allowlist avoids uploading unrelated project-local settings and credentials.
pub fn allowed(path: &str) -> bool {
    if !contract::safe_relative(path)
        || path.chars().any(char::is_control)
        || path.contains('\\')
        || path.contains(':')
        || path.len() > 240
    {
        return false;
    }
    let lower = path.to_ascii_lowercase();
    for part in lower.split('/') {
        if matches!(
            part,
            ".git"
                | ".miy"
                | ".miy-tools"
                | ".ssh"
                | ".aws"
                | ".azure"
                | ".config"
                | ".cargo"
                | "node_modules"
                | "target"
                | "dist"
                | ".pnpm-store"
                | "credentials"
                | "credentials.json"
                | "secrets"
                | "secrets.json"
                | ".npmrc"
                | ".netrc"
        ) || part.starts_with(".env")
            || part.starts_with("id_rsa")
            || part.starts_with("id_ed25519")
            || [".pem", ".key", ".p12", ".pfx", ".log"]
                .iter()
                .any(|ext| part.ends_with(ext))
        {
            return false;
        }
    }
    matches!(
        path,
        "Cargo.toml"
            | "Cargo.lock"
            | "build.rs"
            | "package.json"
            | "pnpm-lock.yaml"
            | "pnpm-workspace.yaml"
            | "tsconfig.json"
            | "vite.config.ts"
            | "rust-toolchain.toml"
            | "manifest.json"
            | "Dockerfile"
            | "miy.bundle.json"
            | "miy.components.json"
            | "miy.components.lock.json"
            | "AGENTS.md"
            | "CLAUDE.md"
    ) || [
        "src/",
        "web/",
        "tests/",
        "examples/",
        "benches/",
        "public/",
        "assets/",
        "sdk/",
        "migrations/",
        "platform-components/",
        "docs/",
        ".agents/skills/",
        ".claude/skills/",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
}

fn no_private_key(bytes: &[u8]) -> Result<()> {
    // Reject high-confidence credentials even when accidentally committed under
    // a business filename. Arbitrary secrets cannot be identified by filename.
    for marker in [
        b"-----BEGIN PRIVATE KEY-----".as_slice(),
        b"-----BEGIN RSA PRIVATE KEY-----",
        b"-----BEGIN OPENSSH PRIVATE KEY-----",
        b"-----BEGIN EC PRIVATE KEY-----",
    ] {
        ensure!(
            !bytes.windows(marker.len()).any(|part| part == marker),
            "source_contains_private_key"
        );
    }
    Ok(())
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .context("git_unavailable")?;
    ensure!(output.status.success(), "source_git_command_failed");
    Ok(output.stdout)
}

/// Capture HEAD blobs, not the working tree. The second HEAD/dirty check catches
/// an editor or concurrent commit changing source while the archive is prepared.
pub fn pack(root: &Path, destination: &Path) -> Result<Packed> {
    ensure!(
        git(root, &["status", "--porcelain"])?.is_empty(),
        "commit_source_before_building"
    );
    let commit = String::from_utf8(git(root, &["rev-parse", "HEAD"])?)?
        .trim()
        .to_owned();
    ensure!(valid_commit(&commit), "invalid_source_commit");
    let prefix = String::from_utf8(git(root, &["rev-parse", "--show-prefix"])?)?;
    let prefix = prefix.trim_end_matches(['\r', '\n']).trim_end_matches('/');
    ensure!(
        prefix.is_empty()
            || (contract::safe_relative(prefix) && !prefix.chars().any(char::is_control)),
        "invalid_source_project_prefix"
    );
    let tree_id = if prefix.is_empty() {
        commit.clone()
    } else {
        format!("{commit}:{prefix}")
    };
    let tree = git(root, &["ls-tree", "-r", "-z", "--full-tree", &tree_id])?;
    let mut selected = Vec::new();
    for entry in tree.split(|b| *b == 0).filter(|e| !e.is_empty()) {
        let entry = std::str::from_utf8(entry).context("source_path_not_utf8")?;
        let (meta, path) = entry.split_once('\t').context("invalid_source_tree")?;
        if !allowed(path) {
            continue;
        }
        let parts: Vec<_> = meta.split(' ').collect();
        ensure!(
            parts.len() == 3
                && matches!(parts[0], "100644" | "100755")
                && parts[1] == "blob"
                && valid_commit(parts[2]),
            "source_links_and_submodules_unsupported"
        );
        selected.push((path.to_owned(), parts[2].to_owned()));
    }
    ensure!(
        !selected.is_empty() && selected.len() <= MAX_FILES,
        "source_file_count_limit"
    );
    // The caller uses a new private path. Refuse overwriting any existing file.
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let result = (|| {
        let mut tar = tar::Builder::new(file);
        let mut total = 0u64;
        for (path, blob) in selected {
            let size = String::from_utf8(git(root, &["cat-file", "-s", &blob])?)?
                .trim()
                .parse::<u64>()?;
            ensure!(
                size <= MAX_FILE_BYTES && total + size <= MAX_SOURCE_BYTES - 4 * 1024 * 1024,
                "source_size_limit"
            );
            let bytes = git(root, &["cat-file", "blob", &blob])?;
            ensure!(bytes.len() as u64 == size, "source_blob_size_mismatch");
            no_private_key(&bytes)?;
            let mut header = tar::Header::new_ustar();
            header.set_size(size);
            header.set_mode(0o644);
            header.set_uid(0);
            header.set_gid(0);
            header.set_mtime(0);
            header.set_cksum();
            tar.append_data(&mut header, path, bytes.as_slice())?;
            total += size;
        }
        tar.finish()?;
        tar.into_inner()?.sync_all()?;
        ensure!(
            git(root, &["status", "--porcelain"])?.is_empty()
                && String::from_utf8(git(root, &["rev-parse", "HEAD"])?)?.trim() == commit,
            "source_changed_during_archive"
        );
        let size_bytes = std::fs::metadata(destination)?.len();
        ensure!(size_bytes <= MAX_SOURCE_BYTES, "source_size_limit");
        // Validation uses a separate scratch directory and the actual uploaded
        // snapshot, preventing ignored local files from satisfying integrity checks.
        let scratch = destination.with_extension(format!("check-{}", uuid::Uuid::new_v4()));
        let validation = (|| {
            extract(destination, &scratch)?;
            let m: Manifest =
                serde_json::from_slice(&std::fs::read(scratch.join("manifest.json"))?)?;
            validate(&scratch, &m.id)
        })();
        let _ = std::fs::remove_dir_all(&scratch);
        let manifest = validation?;
        Ok(Packed {
            source_commit: commit,
            archive_hash: digest(destination)?,
            size_bytes,
            manifest,
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(destination);
    }
    result
}

pub fn digest(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hex::encode(hash.finalize()))
}

pub fn extract(archive: &Path, destination: &Path) -> Result<()> {
    ensure!(
        std::fs::metadata(archive)?.len() <= MAX_SOURCE_BYTES,
        "source_size_limit"
    );
    ensure!(!destination.exists(), "source_destination_exists");
    std::fs::create_dir(destination)?;
    let mut tar = tar::Archive::new(std::fs::File::open(archive)?);
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    for entry in tar.entries()? {
        let mut entry = entry?;
        let name = std::str::from_utf8(&entry.path_bytes())?.to_owned();
        ensure!(
            allowed(&name) && entry.header().entry_type().is_file(),
            "invalid_source_member"
        );
        ensure!(
            seen.insert(name.clone()) && seen.len() <= MAX_FILES,
            "duplicate_or_excess_source_members"
        );
        ensure!(entry.size() <= MAX_FILE_BYTES, "source_member_size_limit");
        total += entry.size();
        ensure!(total <= MAX_SOURCE_BYTES, "source_size_limit");
        let path = destination.join(&name);
        std::fs::create_dir_all(path.parent().context("invalid_source_path")?)?;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == entry.size(),
            "truncated_source_member"
        );
        no_private_key(&bytes)?;
        file.write_all(&bytes)?;
    }
    ensure!(!seen.is_empty(), "empty_source_archive");
    Ok(())
}

fn inventory(root: &Path, prefix: &str, output: &mut BTreeSet<String>) -> Result<()> {
    for file in std::fs::read_dir(root.join(prefix))? {
        let file = file?;
        let name = file
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("source_path_not_utf8"))?;
        let path = format!("{prefix}/{name}");
        if file.file_type()?.is_dir() {
            inventory(root, &path, output)?;
        } else {
            ensure!(file.file_type()?.is_file(), "source_link_unsupported");
            output.insert(path);
        }
    }
    Ok(())
}

/// Pure checks only. Compilers/package scripts execute inside Docker, never here.
pub fn validate(root: &Path, app: &str) -> Result<Manifest> {
    let m: Manifest = serde_json::from_slice(&std::fs::read(root.join("manifest.json"))?)?;
    contract::validate(&m).map_err(anyhow::Error::msg)?;
    ensure!(m.id == app, "source_app_mismatch");
    bundle::verify(root)?;
    let mut actual = BTreeSet::new();
    inventory(root, "sdk", &mut actual)?;
    let expected: BTreeSet<_> = bundle::protected_hashes()
        .keys()
        .filter(|p| p.starts_with("sdk/"))
        .map(|p| p.to_string())
        .collect();
    ensure!(actual == expected, "source_sdk_inventory_mismatch");
    crate::component_cli::verify(root)?;
    bundle::migrations(&root.join("migrations"))?;
    let cargo: toml::Value = toml::from_str(&std::fs::read_to_string(root.join("Cargo.toml"))?)?;
    ensure!(
        cargo
            .get("package")
            .and_then(|v| v.get("name"))
            .and_then(toml::Value::as_str)
            == Some("miy-app"),
        "source_package_mismatch"
    );
    let sdk = cargo
        .get("dependencies")
        .and_then(|v| v.get("miy-app-sdk"))
        .and_then(toml::Value::as_table)
        .context("source_sdk_dependency_required")?;
    ensure!(
        sdk.get("path").and_then(toml::Value::as_str) == Some("sdk/rust")
            && !sdk.contains_key("git")
            && !sdk.contains_key("registry")
            && !sdk.contains_key("package"),
        "source_sdk_dependency_modified"
    );
    let package: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("package.json"))?)?;
    ensure!(
        package["packageManager"] == "pnpm@10.34.5",
        "source_package_manager_modified"
    );
    for path in [
        "Cargo.lock",
        "pnpm-lock.yaml",
        "package.json",
        "src/main.rs",
        "web/src/AppRoot.tsx",
    ] {
        ensure!(root.join(path).is_file(), "missing_source_build_input");
    }
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn archive(path: &Path, entries: &[(&str, &[u8], u8)]) {
        let mut writer = tar::Builder::new(std::fs::File::create(path).unwrap());
        for (name, bytes, kind) in entries {
            let mut header = tar::Header::new_ustar();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::new(*kind));
            header.set_cksum();
            writer.append_data(&mut header, name, *bytes).unwrap();
        }
        writer.finish().unwrap();
    }
    #[test]
    fn source_allowlist_excludes_credentials_and_generated_files() {
        for path in [
            ".git/config",
            ".miy/session.json",
            "src/.env",
            "web/node_modules/a.js",
            "target/app",
            "src/secret.pem",
            "web/.npmrc",
            "docs/credentials.json",
            "../src/a",
            "src/../a",
            "src\\a",
            "src/a:b",
            ".codex/config.toml",
        ] {
            assert!(!allowed(path), "{path}");
        }
        for path in [
            "src/main.rs",
            "sdk/browser/index.ts",
            ".agents/skills/miy-app/SKILL.md",
            "migrations/0002_todos.sql",
            "miy.bundle.json",
        ] {
            assert!(allowed(path), "{path}");
        }
    }
    #[test]
    fn extract_rejects_duplicates_links_secrets_and_existing_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let tar = tmp.path().join("source.tar");
        for entries in [
            vec![
                ("src/a", b"x".as_slice(), b'0'),
                ("src/a", b"x".as_slice(), b'0'),
            ],
            vec![("src/a", b"".as_slice(), b'2')],
            vec![("src/.env", b"secret".as_slice(), b'0')],
            vec![("src/a", b"-----BEGIN PRIVATE KEY-----".as_slice(), b'0')],
        ] {
            archive(&tar, &entries);
            let out = tmp.path().join(uuid::Uuid::new_v4().to_string());
            assert!(extract(&tar, &out).is_err());
        }
        archive(&tar, &[("src/a", b"content", b'0')]);
        let out = tmp.path().join("ok");
        extract(&tar, &out).unwrap();
        assert_eq!(std::fs::read(out.join("src/a")).unwrap(), b"content");
        assert!(extract(&tar, &out).is_err());
    }
    #[test]
    fn commit_format_is_strict() {
        assert!(valid_commit(&"a".repeat(40)));
        assert!(!valid_commit(&"A".repeat(40)));
        assert!(!valid_commit("HEAD"));
    }

    fn fixture() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        for (path, content) in bundle::template() {
            let file = tmp.path().join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(
                file,
                content
                    .replace("{{APP_ID}}", "sample")
                    .replace("{{APP_MODULE}}", "sample")
                    .replace("{{APP_NAME}}", "Sample"),
            )
            .unwrap();
        }
        std::fs::write(tmp.path().join("miy.bundle.json"), serde_json::to_vec(&serde_json::json!({"bundleVersion":bundle::VERSION,"protectedFiles":bundle::protected_hashes()})).unwrap()).unwrap();
        let package = crate::component_contract::read_source(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../registry/components/ui/1.0.1"),
        )
        .unwrap();
        let release = crate::component_contract::release(package).unwrap();
        let mut hashes = std::collections::BTreeMap::new();
        for (path, text) in &release.package.files {
            let file = tmp.path().join("platform-components/ui").join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
            hashes.insert(path, contract::hash(text));
        }
        std::fs::write(tmp.path().join("platform-components/miy.components.lock.json"), serde_json::to_vec(&serde_json::json!({"protocol":crate::component_contract::PROTOCOL,"registry":"http://localhost:18000","components":{"ui":{"version":release.package.version,"sha256":release.sha256,"description":release.package.description,"uiContract":release.package.ui_contract,"bundleVersions":release.package.bundle_versions,"files":hashes}}})).unwrap()).unwrap();
        tmp
    }
    #[test]
    fn validates_frozen_sdk_inventory_and_dependency() {
        let tmp = fixture();
        validate(tmp.path(), "sample").unwrap();
        assert!(validate(tmp.path(), "other").is_err());
        std::fs::write(tmp.path().join("sdk/rust/build.rs"), "fn main() {} ").unwrap();
        assert!(
            validate(tmp.path(), "sample")
                .unwrap_err()
                .to_string()
                .contains("inventory")
        );
        std::fs::remove_file(tmp.path().join("sdk/rust/build.rs")).unwrap();
        let cargo = tmp.path().join("Cargo.toml");
        let text = std::fs::read_to_string(&cargo)
            .unwrap()
            .replace("path = \"sdk/rust\"", "path = \"src/fake-sdk\"");
        std::fs::write(cargo, text).unwrap();
        assert!(
            validate(tmp.path(), "sample")
                .unwrap_err()
                .to_string()
                .contains("dependency")
        );
    }
    #[test]
    fn pack_is_deterministic_committed_filtered_and_rejects_dirty_source() {
        let tmp = fixture();
        std::fs::write(tmp.path().join(".env"), "secret=never-upload").unwrap();
        git(tmp.path(), &["init", "--quiet"]).unwrap();
        git(
            tmp.path(),
            &["config", "user.email", "test@example.invalid"],
        )
        .unwrap();
        git(tmp.path(), &["config", "user.name", "Test"]).unwrap();
        git(tmp.path(), &["add", "-A"]).unwrap();
        git(tmp.path(), &["add", "-f", ".env"]).unwrap();
        git(tmp.path(), &["commit", "--quiet", "-m", "fixture"]).unwrap();
        let out = tempfile::tempdir().unwrap();
        let first = pack(tmp.path(), &out.path().join("one.tar")).unwrap();
        let second = pack(tmp.path(), &out.path().join("two.tar")).unwrap();
        assert_eq!(first.archive_hash, second.archive_hash);
        extract(&out.path().join("one.tar"), &out.path().join("source")).unwrap();
        assert!(!out.path().join("source/.env").exists());
        assert!(!out.path().join("source/.git").exists());
        std::fs::write(tmp.path().join("src/new.rs"), "// uncommitted").unwrap();
        assert!(pack(tmp.path(), &out.path().join("dirty.tar")).is_err());
    }
    #[test]
    fn nested_project_uses_its_committed_tree_without_sibling_sources() {
        let fixture = fixture();
        let repo = tempfile::tempdir().unwrap();
        let app = repo.path().join("nested app");
        std::fs::rename(fixture.path(), &app).unwrap();
        std::fs::write(repo.path().join("Cargo.toml"), "not the application").unwrap();
        git(repo.path(), &["init", "--quiet"]).unwrap();
        git(
            repo.path(),
            &["config", "user.email", "test@example.invalid"],
        )
        .unwrap();
        git(repo.path(), &["config", "user.name", "Test"]).unwrap();
        git(repo.path(), &["add", "-A"]).unwrap();
        git(repo.path(), &["commit", "--quiet", "-m", "nested fixture"]).unwrap();
        let out = tempfile::tempdir().unwrap();
        let packed = pack(&app, &out.path().join("source.tar")).unwrap();
        assert_eq!(packed.manifest.id, "sample");
        extract(&out.path().join("source.tar"), &out.path().join("source")).unwrap();
        assert!(!out.path().join("source/nested app").exists());
        assert!(
            std::fs::read_to_string(out.path().join("source/Cargo.toml"))
                .unwrap()
                .contains("miy-app")
        );
    }
}
