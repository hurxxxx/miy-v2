//! Explicit SDK upgrade; business code and published migrations are preserved.
use crate::cli_error::{Category, CliError};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
pub async fn run() -> Result<Value> {
    let root = Path::new(".").canonicalize()?;
    // Recover an interrupted update before old-bundle and clean-tree checks.
    let transaction = crate::file_transaction::begin(&root)?;
    let status = crate::owned_process::output(
        std::process::Command::new("git").args(["status", "--porcelain"]),
    )?;
    ensure!(
        status.status.success() && status.stdout.is_empty(),
        CliError::new(
            Category::Conflict,
            "commit_source_before_bundle_upgrade",
            "commit_source_before_bundle_upgrade"
        )
    );
    let mut expected_files = BTreeMap::new();
    let old: Value = serde_json::from_slice(&read_input("miy.bundle.json", &mut expected_files)?)?;
    if old["bundleVersion"] == crate::bundle::VERSION {
        crate::cli_error::validation(crate::bundle::verify(Path::new(".")), "bundle_invalid")?;
        let receipt = transaction.commit(crate::agent_tools::setup_changes(&root)?)?;
        let setup = crate::agent_tools::setup_result(&root)?;
        return Ok(
            json!({"alreadyCurrent":true,"bundleVersion":crate::bundle::VERSION,"cliVersion":crate::cli_guidance::VERSION,"setup":setup,"transaction":receipt}),
        );
    }
    let expected: Value = serde_json::from_str(match old["bundleVersion"].as_str() {
        Some("0.3.0") => include_str!("../bundles/0.3.0.json"),
        Some("0.4.0") => include_str!("../bundles/0.4.0.json"),
        Some("0.5.0") => include_str!("../bundles/0.5.0.json"),
        Some("0.6.0") => include_str!("../bundles/0.6.0.json"),
        Some("0.6.1") => include_str!("../bundles/0.6.1.json"),
        Some("0.6.2") => include_str!("../bundles/0.6.2.json"),
        Some("0.7.0") => include_str!("../bundles/0.7.0.json"),
        Some("0.7.1") => include_str!("../bundles/0.7.1.json"),
        Some("0.8.0") => include_str!("../bundles/0.8.0.json"),
        _ => anyhow::bail!(CliError::new(
            Category::Environment,
            "unsupported_old_bundle",
            "unsupported_old_bundle"
        )),
    })?;
    ensure!(
        old == expected,
        CliError::new(
            Category::Environment,
            "unsupported_or_modified_old_bundle",
            "unsupported_or_modified_old_bundle"
        )
    );
    let hashes = old["protectedFiles"]
        .as_object()
        .context("invalid_old_bundle")?;
    for (path, hash) in hashes {
        crate::cli_guidance::safe_target(&root, path)?;
        ensure!(
            std::fs::symlink_metadata(path)?.is_file()
                && json!(crate::contract::hash(std::fs::read(path)?)) == *hash,
            CliError::new(
                Category::Environment,
                "protected_bundle_modified",
                format!("protected_bundle_modified: {path}")
            )
        );
        expected_files.insert(
            path.clone(),
            Some(hash.as_str().context("invalid_old_file_hash")?.into()),
        );
    }
    crate::cli_error::validation(
        crate::cli_guidance::preflight(Path::new(".")),
        "cli_guidance_invalid",
    )?;
    let protected = crate::bundle::protected_hashes();
    // Browser components remain compatible. The explicit Rust SDK migration
    // resolves only the SDK's new dependencies, before any project file changes.
    // 0.9.0 runs the UI in its own frame: the browser SDK replaces the callback
    // navigation guard with setNavigationState and adds apiBlob. App code that
    // used registerNavigationGuard must change; `app check` reports it.
    if matches!(
        old["bundleVersion"].as_str(),
        Some("0.6.0" | "0.6.1" | "0.6.2" | "0.7.0" | "0.7.1" | "0.8.0")
    ) {
        let mut changes = crate::agent_tools::setup_changes(&root)?;
        for (path, content) in crate::bundle::template() {
            if !protected.contains_key(path) {
                continue;
            }
            let unchanged = hashes.get(*path).and_then(Value::as_str)
                == Some(crate::contract::hash(content).as_str());
            validate_guidance_change(path, unchanged)?;
            if unchanged {
                continue;
            }
            if !hashes.contains_key(*path) && root.join(path).exists() {
                ensure!(
                    read_input(path, &mut expected_files)? == content.as_bytes(),
                    CliError::new(
                        Category::Conflict,
                        "new_bundle_file_conflicts_with_user_file",
                        "A new managed guidance file conflicts with an existing project file."
                    )
                );
            }
            changes.push(
                crate::file_transaction::Change::new(*path, content.as_bytes())
                    .expecting(expected_files.get(*path).cloned().flatten()),
            );
        }
        changes.push(
            crate::file_transaction::Change::new(
                "miy.bundle.json",
                serde_json::to_vec_pretty(
                    &json!({"bundleVersion":crate::bundle::VERSION,"protectedFiles":protected}),
                )?,
            )
            .expecting(expected_files.get("miy.bundle.json").cloned().flatten()),
        );
        changes.extend(crate::cli_error::validation(
            resolve_sdk_lock(
                &root,
                &transaction.workspace()?.join("rust-sdk-resolution"),
                &mut expected_files,
            ),
            "sdk_upgrade_invalid",
        )?);
        let receipt = transaction.commit(changes)?;
        let setup = crate::agent_tools::setup_result(&root)?;
        return Ok(
            json!({"upgradedFrom":old["bundleVersion"],"bundleVersion":crate::bundle::VERSION,"cliVersion":crate::cli_guidance::VERSION,"backup":receipt.backup,"changedFiles":receipt.changed_files,"setup":setup,"next":["review SDK and Cargo.lock changes","miy app check","increment manifest version and commit before deployment","reopen the agent session"]}),
        );
    }
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (path, content) in crate::bundle::template() {
        if !protected.contains_key(path) {
            continue;
        }
        if Path::new(path).exists() && !hashes.contains_key(*path) {
            let actual = read_input(path, &mut expected_files)?;
            ensure!(
                actual == content.as_bytes(),
                CliError::new(
                    Category::Environment,
                    "new_bundle_file_conflicts_with_user_file",
                    format!("new_bundle_file_conflicts_with_user_file: {path}")
                )
            );
        }
        expected_files.entry((*path).into()).or_insert(None);
        files.insert((*path).into(), content.as_bytes().to_vec());
    }
    let mut package: Value =
        serde_json::from_slice(&read_input("package.json", &mut expected_files)?)?;
    let script = "vite build --config sdk/preview/vite.config.ts";
    ensure!(
        package["scripts"]["build:preview"].is_null()
            || package["scripts"]["build:preview"] == script,
        CliError::new(
            Category::Environment,
            "build_preview_script_conflict",
            "build_preview_script_conflict"
        )
    );
    package["scripts"]["build:preview"] = json!(script);
    ensure!(
        package["dependencies"]["@tanstack/history"].is_null()
            || package["dependencies"]["@tanstack/history"] == "1.162.5",
        CliError::new(
            Category::Environment,
            "history_dependency_conflict",
            "history_dependency_conflict"
        )
    );
    package["dependencies"]["@tanstack/history"] = json!("1.162.5");
    files.insert("package.json".into(), serde_json::to_vec_pretty(&package)?);
    let mut ts: Value = serde_json::from_slice(&read_input("tsconfig.json", &mut expected_files)?)?;
    let alias = &mut ts["compilerOptions"]["paths"]["@miy/components/*"];
    ensure!(
        alias.is_null() || *alias == json!(["./platform-components/*"]),
        CliError::new(
            Category::Environment,
            "component_alias_conflict",
            "component_alias_conflict"
        )
    );
    *alias = json!(["./platform-components/*"]);
    let include = ts["include"]
        .as_array_mut()
        .context("tsconfig_include_required")?;
    if !include.contains(&json!("sdk/preview/main.tsx")) {
        include.push(json!("sdk/preview/main.tsx"));
    }
    if !include.contains(&json!("platform-components")) {
        include.push(json!("platform-components"));
    }
    files.insert("tsconfig.json".into(), serde_json::to_vec_pretty(&ts)?);
    let vite = String::from_utf8(read_input("vite.config.ts", &mut expected_files)?)?;
    let anchor = "\"@miy/ui\": resolve(\"sdk/ui/index.tsx\")";
    if old["bundleVersion"] != "0.5.0" {
        ensure!(
            !vite.contains("@miy/components") && vite.matches(anchor).count() == 1,
            CliError::new(
                Category::Environment,
                "custom_vite_config_requires_component_alias_migration",
                "custom_vite_config_requires_component_alias_migration"
            )
        );
        files.insert(
            "vite.config.ts".into(),
            vite.replace(
                anchor,
                &format!("{anchor}, \"@miy/components\": resolve(\"platform-components\")"),
            )
            .into_bytes(),
        );
    }
    for (path, extra) in [
        (".gitignore", "\n/.codex/config.toml\n/.mcp.json\n"),
        (".dockerignore", "\n.codex\n.claude\n.agents\n.mcp.json\n"),
    ] {
        files.insert(
            path.into(),
            format!(
                "{}{}",
                String::from_utf8(read_input(path, &mut expected_files)?)?,
                extra
            )
            .into_bytes(),
        );
    }
    files.insert(
        "miy.bundle.json".into(),
        serde_json::to_vec_pretty(
            &json!({"bundleVersion":crate::bundle::VERSION,"protectedFiles":protected}),
        )?,
    );
    let existing_components = old["bundleVersion"] == "0.5.0";
    if existing_components {
        crate::component_cli::verify(Path::new(".")).context("before_upgrade: use the old bundled CLI to update ui to 1.0.1 (and other components to releases supporting 0.6.0), then commit")?;
    } else {
        ensure!(
            !Path::new("platform-components").exists(),
            CliError::new(
                Category::Environment,
                "component_directory_conflicts_with_user_files",
                "component_directory_conflicts_with_user_files"
            )
        );
    }
    // Fetch and verify before changing any protected SDK or application configuration.
    let session = crate::app_cli::session()?;
    let default_ui = if existing_components {
        None
    } else {
        Some(crate::component_cli::fetch(&session, "ui", "1.0.1").await?)
    };
    // Resolve the lock before touching any app files. Network/tool failures leave the old bundle usable.
    let stage = transaction.workspace()?.join("dependency-resolution");
    std::fs::create_dir_all(&stage)?;
    std::fs::write(
        stage.join("package.json"),
        files.get("package.json").unwrap(),
    )?;
    if Path::new("pnpm-lock.yaml").exists() {
        std::fs::write(
            stage.join("pnpm-lock.yaml"),
            read_input("pnpm-lock.yaml", &mut expected_files)?,
        )?;
    } else {
        expected_files.insert("pnpm-lock.yaml".into(), None);
    }
    let resolved = crate::owned_process::status(
        crate::platform::command(
            "pnpm",
            &[
                "install",
                "--lockfile-only",
                "--ignore-scripts",
                "--ignore-workspace",
            ],
        )?
        .current_dir(&stage)
        .env("PATH", crate::platform::tool_path()?)
        .env("CI", "true")
        .stdout(std::process::Stdio::from(std::io::stderr())),
    )?;
    ensure!(
        resolved.success(),
        CliError::new(
            Category::Environment,
            "upgrade_dependency_resolution_failed",
            "upgrade_dependency_resolution_failed: application files unchanged"
        )
    );
    files.insert(
        "pnpm-lock.yaml".into(),
        std::fs::read(stage.join("pnpm-lock.yaml"))?,
    );
    if let Some(default_ui) = default_ui {
        let component_stage = transaction.workspace()?.join("component-initialization");
        std::fs::create_dir_all(&component_stage)?;
        std::fs::write(
            component_stage.join("miy.components.json"),
            files
                .get("miy.components.json")
                .context("component_defaults_missing_from_upgrade")?,
        )?;
        crate::component_cli::initialize(&component_stage, &session, default_ui)?;
        collect_component_files(
            &component_stage,
            &component_stage.join("platform-components"),
            &mut files,
        )?;
    }
    let mut changes = crate::agent_tools::setup_changes(&root)?;
    changes.extend(crate::cli_error::validation(
        resolve_sdk_lock(
            &root,
            &transaction.workspace()?.join("rust-sdk-resolution"),
            &mut expected_files,
        ),
        "sdk_upgrade_invalid",
    )?);
    changes.extend(files.into_iter().map(|(path, bytes)| {
        let expected = expected_files.get(&path).cloned().flatten();
        crate::file_transaction::Change::new(path, bytes).expecting(expected)
    }));
    let receipt = transaction.commit(changes)?;
    crate::agent_tools::setup_result(&root)?;
    Ok(
        json!({"upgradedFrom":old["bundleVersion"],"cliVersion":crate::cli_guidance::VERSION,"bundleVersion":crate::bundle::VERSION,"backup":receipt.backup,"changedFiles":receipt.changed_files,"components":"compatible installed components preserved; older bundles initialize ui 1.0.1","next":["review git diff","miy app check","increment manifest version and commit before deployment"]}),
    )
}

// All supported guidance-era bundles use the explicit browser/preview and Rust
// SDK migration. Other SDK files, component defaults and SQL stay immutable.
fn validate_guidance_change(path: &str, unchanged: bool) -> Result<()> {
    if unchanged {
        return Ok(());
    }
    ensure!(
        !path.ends_with(".sql")
            && (matches!(
                path,
                "AGENTS.md"
                    | "CLAUDE.md"
                    | "sdk/browser/index.ts"
                    | "sdk/preview/main.tsx"
                    | "sdk/rust/src/lib.rs"
                    | "sdk/rust/Cargo.toml"
            ) || path.starts_with("docs/")
                || path.starts_with(".agents/skills/")
                || path.starts_with(".claude/skills/")),
        CliError::new(
            Category::Environment,
            "guidance_upgrade_runtime_changed",
            "This upgrade requires an explicit migration for changed runtime files."
        )
    );
    Ok(())
}

fn sdk_upgrade_manifest(manifest: &toml::Value) -> Result<()> {
    ensure!(
        matches!(
            manifest
                .get("dependencies")
                .and_then(|value| value.get("miy-app-sdk"))
                .and_then(|value| value.get("path"))
                .and_then(toml::Value::as_str),
            Some("sdk/rust" | "./sdk/rust")
        ),
        "unsupported_sdk_dependency: the SDK must be the project's sdk/rust path dependency"
    );
    ensure!(
        manifest
            .get("package")
            .and_then(|value| value.get("workspace"))
            .is_none(),
        "custom_workspace_requires_sdk_migration"
    );
    if let Some(members) = manifest
        .get("workspace")
        .and_then(|value| value.get("members"))
    {
        ensure!(
            members.as_array().is_some_and(|members| members
                .iter()
                .all(|member| matches!(member.as_str(), Some("sdk/rust" | "./sdk/rust")))),
            "custom_workspace_requires_sdk_migration"
        );
    }
    fn dependency_paths(value: &toml::Value) -> bool {
        let Some(table) = value.as_table() else {
            return true;
        };
        for (key, value) in table {
            if matches!(
                key.as_str(),
                "dependencies" | "dev-dependencies" | "build-dependencies"
            ) {
                let Some(dependencies) = value.as_table() else {
                    return false;
                };
                for (name, dependency) in dependencies {
                    if let Some(path) = dependency.get("path")
                        && !(name == "miy-app-sdk"
                            && matches!(path.as_str(), Some("sdk/rust" | "./sdk/rust")))
                    {
                        return false;
                    }
                }
            } else if !dependency_paths(value) {
                return false;
            }
        }
        true
    }
    ensure!(
        dependency_paths(manifest),
        "custom_path_dependency_requires_sdk_migration"
    );
    // A package override can point outside the staged project or replace the
    // SDK's new HTTP transport. Leave such custom resolution to an explicit review.
    ensure!(
        manifest.get("patch").is_none() && manifest.get("replace").is_none(),
        "custom_dependency_override_requires_sdk_migration"
    );
    Ok(())
}

fn preserved_lock_packages(before: &str, after: &str) -> Result<()> {
    fn packages(
        input: &str,
    ) -> Result<std::collections::BTreeSet<(String, String, String, String)>> {
        let document: toml::Value =
            toml::from_str(input).map_err(|_| anyhow::anyhow!("invalid_cargo_lock"))?;
        document
            .get("package")
            .and_then(toml::Value::as_array)
            .context("invalid_cargo_lock")?
            .iter()
            .filter(|package| {
                package.get("name").and_then(toml::Value::as_str) != Some("miy-app-sdk")
                    || package.get("source").is_some()
            })
            .map(|package| {
                Ok((
                    package
                        .get("name")
                        .and_then(toml::Value::as_str)
                        .context("invalid_cargo_lock")?
                        .into(),
                    package
                        .get("version")
                        .and_then(toml::Value::as_str)
                        .context("invalid_cargo_lock")?
                        .into(),
                    package
                        .get("source")
                        .and_then(toml::Value::as_str)
                        .unwrap_or_default()
                        .into(),
                    package
                        .get("checksum")
                        .and_then(toml::Value::as_str)
                        .unwrap_or_default()
                        .into(),
                ))
            })
            .collect()
    }
    ensure!(
        packages(before)?.is_subset(&packages(after)?),
        "sdk_upgrade_would_change_existing_dependency: project files unchanged"
    );
    Ok(())
}

fn resolve_sdk_lock(
    root: &Path,
    stage: &Path,
    expected: &mut BTreeMap<String, Option<String>>,
) -> Result<Vec<crate::file_transaction::Change>> {
    let manifest = read_input("Cargo.toml", expected)?;
    let mut staged_manifest: toml::Value = toml::from_str(std::str::from_utf8(&manifest)?)
        .map_err(|_| anyhow::anyhow!("invalid_cargo_manifest"))?;
    sdk_upgrade_manifest(&staged_manifest)?;
    for configuration in [".cargo/config", ".cargo/config.toml"] {
        ensure!(
            !root.join(configuration).exists(),
            "custom_cargo_configuration_requires_sdk_migration"
        );
    }
    let lock =
        read_input("Cargo.lock", expected).context("sdk_upgrade_requires_committed_cargo_lock")?;
    std::fs::create_dir_all(stage.join("src"))?;
    staged_manifest
        .as_table_mut()
        .context("invalid_cargo_manifest")?
        .entry("workspace")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    std::fs::write(stage.join("Cargo.toml"), toml::to_string(&staged_manifest)?)?;
    std::fs::write(stage.join("Cargo.lock"), &lock)?;
    // Cargo lock resolution never builds or executes project code. Stub targets
    // avoid copying application source, configuration or credentials to the stage.
    std::fs::write(stage.join("src/main.rs"), b"fn main() {}\n")?;
    std::fs::write(
        stage.join("src/lib.rs"),
        b"// Dependency resolution only.\n",
    )?;
    for (path, content) in crate::bundle::template() {
        if path.starts_with("sdk/rust/") {
            let destination = stage.join(path);
            std::fs::create_dir_all(destination.parent().context("invalid_sdk_stage")?)?;
            std::fs::write(destination, content)?;
        }
    }
    let resolved = crate::owned_process::output(
        crate::platform::command("cargo", &["update", "--package", "miy-app-sdk"])?
            .current_dir(stage)
            .env("PATH", crate::platform::tool_path()?),
    )?;
    ensure!(
        resolved.status.success(),
        CliError::new(
            Category::Environment,
            "sdk_dependency_resolution_failed",
            "The staged SDK dependency lock could not be resolved; application files are unchanged. Check Cargo registry access and custom target configuration."
        )
    );
    let updated = std::fs::read(stage.join("Cargo.lock"))?;
    preserved_lock_packages(std::str::from_utf8(&lock)?, std::str::from_utf8(&updated)?)?;
    Ok(vec![
        // Include the unchanged manifest in the transaction's expectation check:
        // a concurrent dependency edit must invalidate this resolved lock.
        crate::file_transaction::Change::new("Cargo.toml", manifest)
            .expecting(expected.get("Cargo.toml").cloned().flatten()),
        crate::file_transaction::Change::new("Cargo.lock", updated)
            .expecting(expected.get("Cargo.lock").cloned().flatten()),
    ])
}

fn read_input(path: &str, expected: &mut BTreeMap<String, Option<String>>) -> Result<Vec<u8>> {
    crate::cli_guidance::safe_target(Path::new("."), path)?;
    let bytes = std::fs::read(path)?;
    expected.insert(path.into(), Some(crate::contract::hash(&bytes)));
    Ok(bytes)
}

fn collect_component_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    crate::platform::reject_reparse(directory)?;
    for item in std::fs::read_dir(directory)? {
        let item = item?;
        let path = item.path();
        crate::platform::reject_reparse(&path)?;
        if item.file_type()?.is_dir() {
            collect_component_files(root, &path, files)?;
        } else {
            ensure!(
                item.file_type()?.is_file(),
                "component_stage_file_type_invalid"
            );
            let logical = path
                .strip_prefix(root)?
                .components()
                .map(|component| {
                    component
                        .as_os_str()
                        .to_str()
                        .context("component_stage_path_invalid")
                })
                .collect::<Result<Vec<_>>>()?
                .join("/");
            ensure!(
                files.insert(logical, std::fs::read(path)?).is_none(),
                "component_stage_path_conflict"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_guidance_bundles_can_migrate_all_protected_changes() {
        let current = crate::bundle::protected_hashes();
        for previous in [
            include_str!("../bundles/0.6.0.json"),
            include_str!("../bundles/0.6.1.json"),
            include_str!("../bundles/0.6.2.json"),
            include_str!("../bundles/0.7.0.json"),
            include_str!("../bundles/0.7.1.json"),
            include_str!("../bundles/0.8.0.json"),
        ] {
            let old: Value = serde_json::from_str(previous).unwrap();
            for (path, hash) in &current {
                let unchanged = old["protectedFiles"][*path].as_str() == Some(hash.as_str());
                validate_guidance_change(path, unchanged).unwrap_or_else(|error| {
                    panic!(
                        "{} -> {}: {path}: {error}",
                        old["bundleVersion"],
                        crate::bundle::VERSION
                    )
                });
            }
        }
        // Extending the supported SDK migration must not silently rewrite SQL,
        // component defaults or files that have no explicit runtime migration.
        for path in [
            "migrations/0001_initial.sql",
            "miy.components.json",
            "sdk/ui/index.tsx",
            "sdk/browser/unrelated.ts",
            "package.json",
        ] {
            assert!(validate_guidance_change(path, false).is_err(), "{path}");
        }
    }

    #[test]
    fn sdk_lock_upgrade_preserves_every_existing_external_package() {
        let before = "version = 4\n[[package]]\nname='miy-app-sdk'\nversion='0.2.0'\n[[package]]\nname='business-dependency'\nversion='1.2.3'\nsource='registry+https://example.invalid/index'\nchecksum='original'\n";
        let added = format!(
            "{}\n[[package]]\nname='new-sdk-http-dependency'\nversion='0.1.0'\n",
            before.replace("0.2.0", "0.3.0")
        );
        preserved_lock_packages(before, &added).unwrap();
        for changed in [
            added.replace("1.2.3", "1.2.4"),
            added.replace("original", "changed"),
            added.replace("business-dependency", "replacement"),
        ] {
            assert!(preserved_lock_packages(before, &changed).is_err());
        }
    }

    #[test]
    fn staged_sdk_resolution_rejects_external_paths_and_custom_workspace_members() {
        let base = "[package]\nname='business-app'\nversion='1.0.0'\n[dependencies]\nmiy-app-sdk={path='sdk/rust'}\nserde='1'\n[workspace]\n";
        sdk_upgrade_manifest(&toml::from_str(base).unwrap()).unwrap();
        for changed in [
            base.replace("serde='1'", "private-library={path='../private-library'}"),
            format!("{base}members=['../other-app']\n"),
            format!("{base}\n[patch.crates-io]\nreqwest={{path='../private-http'}}\n"),
            base.replace("path='sdk/rust'", "path='../sdk/rust'"),
            format!(
                "{base}\n[target.'cfg(unix)'.build-dependencies]\nprivate-library={{path='../private-library'}}\n"
            ),
        ] {
            assert!(sdk_upgrade_manifest(&toml::from_str(&changed).unwrap()).is_err());
        }
    }
}
