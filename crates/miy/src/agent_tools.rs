//! A standard stdio MCP adapter shared by Codex and Claude Code, backed by live API contracts.
use crate::app_cli::{api, session};
use anyhow::{Result, ensure};
use clap::CommandFactory;
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;

pub fn cli_help() -> Value {
    fn walk(c: clap::Command, prefix: &str, rows: &mut Vec<Value>) {
        let mut c = c.bin_name(prefix);
        rows.push(json!({"command":prefix,"help":c.render_long_help().to_string()}));
        for child in c.get_subcommands() {
            if child.is_hide_set() {
                continue;
            }
            walk(
                child.clone(),
                &format!("{prefix} {}", child.get_name()),
                rows,
            );
        }
    }
    let mut c = crate::Args::command();
    c.build();
    let mut rows = vec![];
    walk(c, "miy", &mut rows);
    json!({"cliVersion":crate::cli_guidance::VERSION,"bundleVersion":crate::bundle::VERSION,"workflow":crate::cli_guidance::WORKFLOW,"commands":rows})
}
pub fn setup(root: &Path) -> Result<Value> {
    crate::cli_error::validation(setup_local(root), "setup_failed")
}
fn setup_local(root: &Path) -> Result<Value> {
    let root = root.canonicalize()?;
    let transaction = crate::file_transaction::begin(&root)?;
    ensure!(
        root.join("manifest.json").is_file(),
        "run_inside_an_app_directory"
    );
    crate::bundle::verify(&root)?;
    let receipt = transaction.commit(setup_changes(&root)?)?;
    let mut result = setup_result(&root)?;
    result["transaction"] = serde_json::to_value(receipt)?;
    Ok(result)
}

pub(crate) fn setup_changes(root: &Path) -> Result<Vec<crate::file_transaction::Change>> {
    use crate::file_transaction::Change;
    let root = root.canonicalize()?;
    let binary = crate::platform::project_cli(&root);
    crate::cli_guidance::preflight(&root)?;
    crate::cli_guidance::safe_target(&root, ".codex/config.toml")?;
    crate::cli_guidance::safe_target(&root, ".mcp.json")?;
    let path = root.join(".codex/config.toml");
    let original = if path.exists() {
        std::fs::read_to_string(&path)?
    } else {
        String::new()
    };
    let start = "# BEGIN MIY MANAGED MCP";
    let end = "# END MIY MANAGED MCP";
    let block = format!(
        "{start}\n[mcp_servers.miy_app]\ncommand = {}\nargs = [\"mcp\"]\ncwd = {}\nrequired = true\n{end}",
        json!(binary),
        json!(root)
    );
    let changed = if let Some(a) = original.find(start) {
        let b = original[a..]
            .find(end)
            .ok_or_else(|| anyhow::anyhow!("incomplete_miy_config_block"))?
            + a
            + end.len();
        format!("{}{}{}", &original[..a], block, &original[b..])
    } else {
        ensure!(
            !original.contains("mcp_servers.miy_app"),
            "existing_miy_app_config: preserve or remove the conflicting entry manually"
        );
        format!("{original}\n{block}\n")
    };
    let claude_path = root.join(".mcp.json");
    let mut claude: Value = if claude_path.exists() {
        serde_json::from_slice(&std::fs::read(&claude_path)?)?
    } else {
        json!({"mcpServers":{}})
    };
    let servers = claude
        .get_mut("mcpServers")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| anyhow::anyhow!("invalid_claude_mcp_config"))?;
    if let Some(existing) = servers.get("miy_app") {
        ensure!(
            existing["command"].as_str().is_some_and(|v| {
                let path = v.replace('\\', "/");
                path.ends_with("/.miy-tools/miy") || path.ends_with("/.miy-tools/miy.exe")
            }) && (existing["args"] == json!(["app", "mcp"]) || existing["args"] == json!(["mcp"])),
            "existing_claude_miy_app_config"
        );
    }
    servers.insert(
        "miy_app".into(),
        json!({"type":"stdio","command":binary,"args":["mcp"],"env":{"MIY_APP_DIRECTORY":root}}),
    );
    let mut changes: Vec<_> = crate::cli_guidance::planned_files(&root)?
        .into_iter()
        .map(|(path, bytes)| Change::new(path, bytes))
        .collect();
    let mut executable = Change::new(
        crate::platform::project_cli_relative(),
        std::fs::read(std::env::current_exe()?)?,
    );
    executable.executable = true;
    changes.push(executable);
    // These files can also contain credentials for the user's other MCP servers.
    let mut codex = Change::new(".codex/config.toml", changed.into_bytes());
    codex.private = true;
    changes.push(codex);
    let mut claude = Change::new(".mcp.json", serde_json::to_vec_pretty(&claude)?);
    claude.private = true;
    changes.push(claude);
    changes
        .into_iter()
        .map(|change| change.observe(&root))
        .collect()
}

pub(crate) fn setup_result(root: &Path) -> Result<Value> {
    let guidance = crate::cli_guidance::verify(root)?;
    let mcp = verify_setup(root)?;
    Ok(
        json!({"cliGuidance":guidance,"mcpConfiguration":mcp,"cliVersion":crate::cli_guidance::VERSION,"configured":["Codex","Claude Code"],"transport":"stdio MCP using CLI login","next":["miy auth login","codex or claude; accept project trust and MCP prompts"],"globalSettingsChanged":false}),
    )
}

/// Validate the actual agent configuration, including TOML syntax/duplicate
/// tables, without returning other MCP servers' configuration or credentials.
pub(crate) fn verify_setup(root: &Path) -> Result<Value> {
    let root = root.canonicalize()?;
    for path in [
        ".codex/config.toml",
        ".mcp.json",
        crate::platform::project_cli_relative(),
    ] {
        crate::cli_guidance::safe_target(&root, path)?;
    }
    let binary = crate::platform::project_cli(&root);
    ensure!(binary.is_file(), "project_cli_unavailable");
    let codex_path = root.join(".codex/config.toml");
    let claude_path = root.join(".mcp.json");
    ensure!(
        std::fs::metadata(&codex_path)?.len() <= 1024 * 1024,
        "codex_config_too_large"
    );
    ensure!(
        std::fs::metadata(&claude_path)?.len() <= 1024 * 1024,
        "claude_config_too_large"
    );
    let codex: toml::Value = toml::from_str(&std::fs::read_to_string(codex_path)?)
        .map_err(|_| anyhow::anyhow!("invalid_codex_mcp_config"))?;
    let configured = codex
        .get("mcp_servers")
        .and_then(|servers| servers.get("miy_app"));
    ensure!(
        serde_json::to_value(configured)?
            == json!({"command":binary,"args":["mcp"],"cwd":root,"required":true}),
        "codex_mcp_config_mismatch"
    );
    let claude: Value = serde_json::from_slice(&std::fs::read(claude_path)?)
        .map_err(|_| anyhow::anyhow!("invalid_claude_mcp_config"))?;
    ensure!(
        claude["mcpServers"]["miy_app"]
            == json!({"type":"stdio","command":binary,"args":["mcp"],"env":{"MIY_APP_DIRECTORY":root}}),
        "claude_mcp_config_mismatch"
    );
    Ok(
        json!({"valid":true,"clients":["Codex","Claude Code"],"transport":"stdio","liveProbe":false}),
    )
}
#[derive(Clone)]
struct Tools {
    tool_router: ToolRouter<Self>,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct CliHelpInput {
    /// One command, for example "app deploy". Omit for a short root command list.
    command: Option<String>,
    /// Search command names and descriptions.
    search: Option<String>,
    /// Request the complete reference explicitly.
    #[serde(default)]
    all: bool,
}
fn result(v: Result<Value>) -> Result<CallToolResult, McpError> {
    Ok(match v {
        Ok(v)=>CallToolResult::success(vec![ContentBlock::text(v.to_string())]),
        Err(e)=>CallToolResult::error(vec![ContentBlock::text(json!({"error":e.to_string(),"hint":"Register an explicit server with miy context init if needed, then run .miy-tools/miy auth login and doctor --for develop in this app directory (.exe on Windows). Never read or print credential files."}).to_string())])
    })
}
#[derive(Deserialize, schemars::JsonSchema)]
struct Search {
    query: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct ApiId {
    id: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
struct ComponentId {
    name: String,
    version: Option<String>,
}
#[tool_router]
impl Tools {
    #[tool(
        description = "Search live platform component releases, compatibility and versions. New components appear without updating this CLI.",
        annotations(read_only_hint = true)
    )]
    async fn list_components(
        &self,
        Parameters(v): Parameters<Search>,
    ) -> Result<CallToolResult, McpError> {
        result(
            async {
                let all = api(&session()?, "/api/v1/components", None).await?;
                Ok(json!(
                    all.as_array()
                        .ok_or_else(|| anyhow::anyhow!("invalid_component_catalog"))?
                        .iter()
                        .filter(|e| e
                            .to_string()
                            .to_lowercase()
                            .contains(&v.query.to_lowercase()))
                        .collect::<Vec<_>>()
                ))
            }
            .await,
        )
    }
    #[tool(
        description = "Read a live component's usage, compatibility and exact install command. Omitting version selects the latest compatible release. Does not install or execute code.",
        annotations(read_only_hint = true)
    )]
    async fn get_component(
        &self,
        Parameters(v): Parameters<ComponentId>,
    ) -> Result<CallToolResult, McpError> {
        result(
            async {
                let r = crate::component_cli::select(&session()?, &v.name, v.version).await?;
                Ok(crate::component_contract::guidance(serde_json::to_value(
                    r,
                )?))
            }
            .await,
        )
    }
    #[tool(
        description = "Read this installed bundle's SDK contract and guidelines, plus the live platform version.",
        annotations(read_only_hint = true)
    )]
    async fn get_contract(&self) -> Result<CallToolResult, McpError> {
        result(
            async {
                crate::bundle::verify(Path::new("."))?;
                let remote = api(&session()?, "/api/v1/contracts", None).await?;
                let mut local = crate::contract::get_contract();
                local["platformBundleVersion"] = remote["bundleVersion"].clone();
                local["guidanceSource"] = json!(
                    "installed verified bundle; query search_api for live server capabilities"
                );
                Ok(local)
            }
            .await,
        )
    }
    #[tool(
        description = "Discover CLI commands, inspect one typed command contract, or search by keyword. The default returns a short command list; all explicitly requests the full reference.",
        annotations(read_only_hint = true)
    )]
    async fn get_cli_help(
        &self,
        Parameters(input): Parameters<CliHelpInput>,
    ) -> Result<CallToolResult, McpError> {
        result(crate::cli_discovery::inspect(
            crate::cli_discovery::DescribeArgs {
                command: input.command,
                search: input.search,
                all: input.all,
            },
        ))
    }
    #[tool(
        description = "Search all implemented platform APIs and permissions by keyword (empty query lists all).",
        annotations(read_only_hint = true)
    )]
    async fn search_api(
        &self,
        Parameters(v): Parameters<Search>,
    ) -> Result<CallToolResult, McpError> {
        result(
            async {
                let all = api(&session()?, "/api/v1/catalog", None).await?;
                Ok(json!(
                    all.as_array()
                        .ok_or_else(|| anyhow::anyhow!("invalid_catalog"))?
                        .iter()
                        .filter(|e| e
                            .to_string()
                            .to_lowercase()
                            .contains(&v.query.to_lowercase()))
                        .collect::<Vec<_>>()
                ))
            }
            .await,
        )
    }
    #[tool(
        description = "Get an API's input schema, permissions and route by operation ID.",
        annotations(read_only_hint = true)
    )]
    async fn get_api(&self, Parameters(v): Parameters<ApiId>) -> Result<CallToolResult, McpError> {
        result(
            async {
                let all = api(&session()?, "/api/v1/catalog", None).await?;
                all.as_array()
                    .and_then(|a| a.iter().find(|e| e["id"] == v.id))
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("unknown_api"))
            }
            .await,
        )
    }
    #[tool(
        description = "Validate an app manifest against the live platform contract. Does not grant permissions or deploy.",
        annotations(read_only_hint = true)
    )]
    async fn validate_manifest(
        &self,
        Parameters(v): Parameters<crate::contract::Manifest>,
    ) -> Result<CallToolResult, McpError> {
        result(
            async {
                api(
                    &session()?,
                    "/api/v1/contracts/validate",
                    Some(serde_json::to_value(v)?),
                )
                .await
            }
            .await,
        )
    }
}
#[tool_handler(router=self.tool_router)]
impl ServerHandler for Tools {
    fn get_info(&self) -> ServerConfig {
        let mut c = ServerConfig::default();
        c.capabilities = ServerCapabilities::builder().enable_tools().build();
        c.with_instructions("Read get_contract and get_cli_help before development. Use the project miy-cli skill for CLI workflows; docs/CLI-REFERENCE.md matches the installed executable. Use the installed CLI for checks, local preview and authorized deployment. Never read credential files or modify the protected SDK bundle. This MCP only supplies guidance; it does not deploy applications.")
    }
}
pub async fn serve() -> Result<()> {
    if let Some(root) = std::env::var_os("MIY_APP_DIRECTORY") {
        std::env::set_current_dir(root)?;
    }
    let service = Tools {
        tool_router: Tools::tool_router(),
    }
    .serve(rmcp::transport::stdio())
    .await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod configuration_tests {
    use super::*;

    #[test]
    fn actual_mcp_configuration_rejects_wrong_targets_duplicates_and_private_extra_fields() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        std::fs::create_dir(root.join(".codex")).unwrap();
        std::fs::create_dir(root.join(".miy-tools")).unwrap();
        let binary = crate::platform::project_cli(&root);
        std::fs::write(&binary, []).unwrap();
        let codex = format!(
            "[mcp_servers.other]\ncommand = \"unrelated\"\n\n[mcp_servers.miy_app]\ncommand = {}\nargs = [\"mcp\"]\ncwd = {}\nrequired = true\n",
            json!(binary),
            json!(root)
        );
        let claude = json!({"mcpServers":{"other":{"token":"PRIVATE-SENTINEL"},"miy_app":{"type":"stdio","command":binary,"args":["mcp"],"env":{"MIY_APP_DIRECTORY":root}}}});
        let codex_path = root.join(".codex/config.toml");
        let claude_path = root.join(".mcp.json");
        std::fs::write(&codex_path, &codex).unwrap();
        std::fs::write(&claude_path, claude.to_string()).unwrap();
        assert!(
            !verify_setup(&root)
                .unwrap()
                .to_string()
                .contains("PRIVATE-SENTINEL")
        );
        std::fs::write(
            &codex_path,
            format!("{codex}\n[mcp_servers.miy_app]\ncommand = \"PRIVATE-SENTINEL\"\n"),
        )
        .unwrap();
        let error = verify_setup(&root).unwrap_err().to_string();
        assert_eq!(error, "invalid_codex_mcp_config");
        assert!(!error.contains("PRIVATE-SENTINEL"));
        std::fs::write(&codex_path, &codex).unwrap();
        let mut invalid = claude;
        invalid["mcpServers"]["miy_app"]["env"]["MIY_APP_DIRECTORY"] = json!("wrong-directory");
        std::fs::write(&claude_path, invalid.to_string()).unwrap();
        assert_eq!(
            verify_setup(&root).unwrap_err().to_string(),
            "claude_mcp_config_mismatch"
        );
    }
}
