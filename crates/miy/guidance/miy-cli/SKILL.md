---
name: miy-cli
description: Use and troubleshoot the MIY CLI for project setup, login, local preview, components, checks, builds, deployment, acceptance, promotion, and app lifecycle management. Use for MIY command usage or development/deployment operations; use miy-app for application business code and SDK contracts.
---

Use the project's `.miy-tools/miy`. Read [workflow.md](references/workflow.md) for the operation requested and [commands.md](references/commands.md) for exact commands, arguments, defaults and version. The command reference is generated from this CLI's actual parser. `.miy-tools/miy app describe` and MCP `miy_app.get_cli_help` expose the same information without requiring a separate documentation service.

Before changing a remote app, identify the configured platform, app and environment from CLI output. Local preview, server dev deployment and production promotion are separate operations. Execute the workflow the user requested; development access alone does not imply production deployment authorization.

Use `app status` and the completed job's release/image/version to assess deployment. Report failed or untested checks explicitly; neither job submission nor a running previous release proves a new deployment succeeded. Record acceptance only for tests actually performed, following `docs/ACCEPTANCE.md`.

For authentication, use `app login`; never read or print `.miy` credentials. API and component details come from the platform MCP catalog. CLI commands and options come from the installed CLI, which can be older than the platform server.

After installing a compatible global CLI update, run `miy app setup` inside the project to update the project CLI, this skill, its references and the generated manual together. If the SDK bundle differs, use the explicit `miy app upgrade` workflow. Do not hand-edit managed guidance, its lock, protected SDK files or agent global settings. Keep project-specific operational notes in separate files. Refresh the agent session after updating skills so it reads the installed version.
