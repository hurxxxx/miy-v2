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
            walk(
                child.clone(),
                &format!("{prefix} {}", child.get_name()),
                rows,
            );
        }
    }
    let c = crate::Args::command();
    let app = c.find_subcommand("app").unwrap().clone();
    let mut rows = vec![];
    walk(app, "miy app", &mut rows);
    json!({"cliVersion":crate::cli_guidance::VERSION,"bundleVersion":crate::bundle::VERSION,"workflow":crate::cli_guidance::WORKFLOW,"commands":rows})
}
pub fn setup(root: &Path) -> Result<Value> {
    let root = root.canonicalize()?;
    ensure!(
        root.join("manifest.json").is_file(),
        "run_inside_an_app_directory"
    );
    let binary = crate::platform::project_cli(&root);
    crate::bundle::verify(&root)?;
    let _operation = crate::cli_guidance::operation(&root)?;
    crate::cli_guidance::preflight(&root)?;
    crate::cli_guidance::safe_target(&root, ".codex/config.toml")?;
    crate::cli_guidance::safe_target(&root, ".mcp.json")?;
    let path = root.join(".codex/config.toml");
    std::fs::create_dir_all(path.parent().unwrap())?;
    let original = if path.exists() {
        std::fs::read_to_string(&path)?
    } else {
        String::new()
    };
    let start = "# BEGIN MIY MANAGED MCP";
    let end = "# END MIY MANAGED MCP";
    let block = format!(
        "{start}\n[mcp_servers.miy_app]\ncommand = {}\nargs = [\"app\", \"mcp\"]\ncwd = {}\nrequired = true\n{end}",
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
            }) && existing["args"] == json!(["app", "mcp"]),
            "existing_claude_miy_app_config"
        );
    }
    servers.insert("miy_app".into(),json!({"type":"stdio","command":binary,"args":["app","mcp"],"env":{"MIY_APP_DIRECTORY":root}}));
    crate::cli_guidance::install_binary(&root)?;
    let guidance = crate::cli_guidance::sync(&root)?;
    // These files can also contain credentials for the user's other MCP servers.
    crate::platform::write_private(&path, changed.as_bytes())?;
    crate::platform::write_private(&claude_path, &serde_json::to_vec_pretty(&claude)?)?;
    Ok(
        json!({"cliGuidance":guidance,"cliVersion":crate::cli_guidance::VERSION,"configured":["Codex","Claude Code"],"transport":"stdio MCP using CLI login","next":["miy app login","codex or claude; accept project trust and MCP prompts"],"globalSettingsChanged":false}),
    )
}
#[derive(Clone)]
struct Tools {
    tool_router: ToolRouter<Self>,
}
fn result(v: Result<Value>) -> Result<CallToolResult, McpError> {
    Ok(match v {
        Ok(v)=>CallToolResult::success(vec![ContentBlock::text(v.to_string())]),
        Err(e)=>CallToolResult::error(vec![ContentBlock::text(json!({"error":e.to_string(),"hint":"Run .miy-tools/miy app login in this app directory; inspect app doctor. Never read or print credential files."}).to_string())])
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
        description = "Read the installed CLI's command usage, options and deployment workflow.",
        annotations(read_only_hint = true)
    )]
    async fn get_cli_help(&self) -> Result<CallToolResult, McpError> {
        result(Ok(cli_help()))
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
