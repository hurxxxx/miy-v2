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
    // The 0.6.x transition updates guidance only. Prove runtime/default bytes
    // stay identical before taking this path; no login/download/lock resolution.
    if matches!(old["bundleVersion"].as_str(), Some("0.6.0" | "0.6.1")) {
        let mut changes = crate::agent_tools::setup_changes(&root)?;
        for (path, content) in crate::bundle::template() {
            if !protected.contains_key(path) {
                continue;
            }
            let unchanged = hashes.get(*path).and_then(Value::as_str)
                == Some(crate::contract::hash(content).as_str());
            if path.starts_with("sdk/") || path.ends_with(".sql") || *path == "miy.components.json"
            {
                ensure!(
                    unchanged,
                    CliError::new(
                        Category::Environment,
                        "guidance_upgrade_runtime_changed",
                        "This guidance-only upgrade cannot change SDK, SQL or component defaults."
                    )
                );
            }
            if unchanged {
                continue;
            }
            ensure!(
                matches!(*path, "AGENTS.md" | "CLAUDE.md")
                    || path.starts_with("docs/")
                    || path.starts_with(".agents/skills/")
                    || path.starts_with(".claude/skills/"),
                CliError::new(
                    Category::Environment,
                    "guidance_upgrade_runtime_changed",
                    "This upgrade requires an explicit migration for changed runtime files."
                )
            );
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
        let receipt = transaction.commit(changes)?;
        let setup = crate::agent_tools::setup_result(&root)?;
        return Ok(
            json!({"upgradedFrom":old["bundleVersion"],"bundleVersion":crate::bundle::VERSION,"cliVersion":crate::cli_guidance::VERSION,"backup":receipt.backup,"changedFiles":receipt.changed_files,"setup":setup,"next":["review git diff and commit updated guidance","reopen the agent session"]}),
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
