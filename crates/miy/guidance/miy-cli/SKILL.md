---
name: miy-cli
description: Discover and run MIY CLI commands for contexts, authentication, application development, deployment, jobs, components and troubleshooting. Use miy-app for business code and SDK contracts.
---

Use this project's `.miy-tools/miy` (`.miy-tools/miy.exe` on Windows).

1. Discover with `miy describe` or `miy describe --search WORD`.
2. Inspect only the needed contract: `miy describe --command "app deploy"`. Read its arguments, input schema, permissions and effects; use `--help` when useful. The installed Clap tree is authoritative.
3. Execute the user's requested operation. Identify context, app and environment first. Use `--output json` for one structured response; read `data` or `error`, never infer success from submission alone.
4. On failure, inspect `error.code`, `details` and the recovery executable/argument vector. Resume a saved request before retrying. Only `job retry` creates a retry for a typed retryable runtime failure.

Use `auth login` and `auth wait LOGIN_ID`; never inspect or print credential storage. Noninteractive login returns public approval instructions. Production operations and app removal require explicit `--context`, `--app`, `--environment` and confirmation (`--yes` for automation). Development permission alone does not authorize production work.

Read [workflow.md](references/workflow.md) for the requested workflow; [commands.md](references/commands.md) is the generated complete reference. MCP `get_cli_help` provides the same scoped discovery. Do not load the complete manual for every task.

Report actual checks, release/image IDs and unresolved failures. Acceptance records attest only tests actually performed; follow `docs/ACCEPTANCE.md`. Update compatible project tooling with the intended global `miy app setup`, or use explicit `app upgrade` for a different SDK bundle. Do not edit managed guidance, locks, protected SDK files or global agent settings; reopen the agent session after updates.
