//! Explicit SDK upgrade; business code and published migrations are preserved.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path};
pub async fn run() -> Result<Value> {
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()?;
    ensure!(
        status.status.success() && status.stdout.is_empty(),
        "commit_source_before_bundle_upgrade"
    );
    let old: Value = serde_json::from_slice(&std::fs::read("miy.bundle.json")?)?;
    if old["bundleVersion"] == crate::bundle::VERSION {
        crate::bundle::verify(Path::new("."))?;
        let setup = crate::agent_tools::setup(Path::new("."))?;
        return Ok(
            json!({"alreadyCurrent":true,"bundleVersion":crate::bundle::VERSION,"cliVersion":crate::cli_guidance::VERSION,"setup":setup}),
        );
    }
    let expected: Value = serde_json::from_str(match old["bundleVersion"].as_str() {
        Some("0.3.0") => include_str!("../bundles/0.3.0.json"),
        Some("0.4.0") => include_str!("../bundles/0.4.0.json"),
        Some("0.5.0") => include_str!("../bundles/0.5.0.json"),
        Some("0.6.0") => include_str!("../bundles/0.6.0.json"),
        _ => anyhow::bail!("unsupported_old_bundle"),
    })?;
    ensure!(old == expected, "unsupported_or_modified_old_bundle");
    let hashes = old["protectedFiles"]
        .as_object()
        .context("invalid_old_bundle")?;
    for (path, hash) in hashes {
        ensure!(
            std::fs::symlink_metadata(path)?.is_file()
                && json!(crate::contract::hash(std::fs::read(path)?)) == *hash,
            "protected_bundle_modified: {path}"
        );
    }
    crate::cli_guidance::preflight(Path::new("."))?;
    let protected = crate::bundle::protected_hashes();
    // This patch only changes AGENTS.md/CLAUDE.md. No component download, login,
    // dependency resolution or application configuration change is needed.
    if old["bundleVersion"] == "0.6.0" {
        let backup = Path::new(".miy").join(format!("upgrade-backup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&backup)?;
        for path in ["AGENTS.md", "CLAUDE.md", "miy.bundle.json"] {
            std::fs::copy(path, backup.join(path))?;
        }
        if crate::platform::project_cli(Path::new(".")).is_file() {
            std::fs::copy(
                crate::platform::project_cli(Path::new(".")),
                backup.join("miy-previous"),
            )?;
        }
        for (path, content) in crate::bundle::template() {
            if matches!(*path, "AGENTS.md" | "CLAUDE.md") {
                std::fs::write(path, content)?;
            }
        }
        std::fs::write(
            "miy.bundle.json",
            serde_json::to_vec_pretty(
                &json!({"bundleVersion":crate::bundle::VERSION,"protectedFiles":protected}),
            )?,
        )?;
        let setup = crate::agent_tools::setup(Path::new("."))?;
        return Ok(
            json!({"upgradedFrom":"0.6.0","bundleVersion":crate::bundle::VERSION,"cliVersion":crate::cli_guidance::VERSION,"backup":backup,"changedFiles":["AGENTS.md","CLAUDE.md","miy.bundle.json"],"setup":setup,"next":["review git diff and commit updated guidance","reopen the agent session"]}),
        );
    }
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (path, content) in crate::bundle::template() {
        if !protected.contains_key(path) {
            continue;
        }
        if Path::new(path).exists() && !hashes.contains_key(*path) {
            ensure!(
                std::fs::read(path)? == content.as_bytes(),
                "new_bundle_file_conflicts_with_user_file: {path}"
            );
        }
        files.insert((*path).into(), content.as_bytes().to_vec());
    }
    let mut package: Value = serde_json::from_slice(&std::fs::read("package.json")?)?;
    let script = "vite build --config sdk/preview/vite.config.ts";
    ensure!(
        package["scripts"]["build:preview"].is_null()
            || package["scripts"]["build:preview"] == script,
        "build_preview_script_conflict"
    );
    package["scripts"]["build:preview"] = json!(script);
    ensure!(
        package["dependencies"]["@tanstack/history"].is_null()
            || package["dependencies"]["@tanstack/history"] == "1.162.5",
        "history_dependency_conflict"
    );
    package["dependencies"]["@tanstack/history"] = json!("1.162.5");
    files.insert("package.json".into(), serde_json::to_vec_pretty(&package)?);
    let mut ts: Value = serde_json::from_slice(&std::fs::read("tsconfig.json")?)?;
    let alias = &mut ts["compilerOptions"]["paths"]["@miy/components/*"];
    ensure!(
        alias.is_null() || *alias == json!(["./platform-components/*"]),
        "component_alias_conflict"
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
    let vite = std::fs::read_to_string("vite.config.ts")?;
    let anchor = "\"@miy/ui\": resolve(\"sdk/ui/index.tsx\")";
    if old["bundleVersion"] != "0.5.0" {
        ensure!(
            !vite.contains("@miy/components") && vite.matches(anchor).count() == 1,
            "custom_vite_config_requires_component_alias_migration"
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
            format!("{}{}", std::fs::read_to_string(path)?, extra).into_bytes(),
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
            "component_directory_conflicts_with_user_files"
        );
    }
    // Fetch and verify before changing any protected SDK or application configuration.
    let session = crate::app_cli::session()?;
    let default_ui = if existing_components {
        None
    } else {
        Some(crate::component_cli::fetch(&session, "ui", "1.0.1").await?)
    };
    let backup =
        std::path::PathBuf::from(".miy").join(format!("upgrade-backup-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&backup)?;
    // Resolve the lock before touching any app files. Network/tool failures leave the old bundle usable.
    let stage = backup.join("dependency-resolution");
    std::fs::create_dir_all(&stage)?;
    std::fs::write(
        stage.join("package.json"),
        files.get("package.json").unwrap(),
    )?;
    if Path::new("pnpm-lock.yaml").exists() {
        std::fs::copy("pnpm-lock.yaml", stage.join("pnpm-lock.yaml"))?;
    }
    let resolved = crate::platform::command(
        "pnpm",
        &[
            "install",
            "--lockfile-only",
            "--ignore-scripts",
            "--ignore-workspace",
        ],
    )?
    .current_dir(&stage)
    .env("PATH", crate::dev_tools::tool_path()?)
    .env("CI", "true")
    .stdout(std::process::Stdio::from(std::io::stderr()))
    .status()?;
    ensure!(
        resolved.success(),
        "upgrade_dependency_resolution_failed: application files unchanged"
    );
    files.insert(
        "pnpm-lock.yaml".into(),
        std::fs::read(stage.join("pnpm-lock.yaml"))?,
    );
    for (path, content) in &files {
        let target = Path::new(path);
        let saved = backup.join(path);
        if target.exists() {
            std::fs::create_dir_all(saved.parent().unwrap())?;
            std::fs::copy(target, &saved)?;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, content)?;
    }
    if let Some(default_ui) = default_ui {
        crate::component_cli::initialize(Path::new("."), &session, default_ui)?;
    }
    std::fs::create_dir_all(".miy-tools")?;
    if crate::platform::project_cli(Path::new(".")).is_file() {
        std::fs::copy(
            crate::platform::project_cli(Path::new(".")),
            backup.join("miy-previous"),
        )?;
    }
    crate::agent_tools::setup(Path::new("."))?;
    Ok(
        json!({"upgradedFrom":old["bundleVersion"],"cliVersion":crate::cli_guidance::VERSION,"bundleVersion":crate::bundle::VERSION,"backup":backup,"changedFiles":files.keys().collect::<Vec<_>>(),"components":"compatible installed components preserved; older bundles initialize ui 1.0.1","next":["review git diff","miy app check","increment manifest version and commit before deployment"]}),
    )
}
