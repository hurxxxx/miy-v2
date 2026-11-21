---
name: miy-cli
description: Discover and run MIY CLI commands for contexts, authentication, application development, deployment, jobs, components and troubleshooting. Use miy-app for business code and SDK contracts.
---

Use this project's `.miy-tools/miy` (`.miy-tools/miy.exe` on Windows).

1. Discover with `miy describe` or `miy describe --search WORD`.
2. Inspect only the needed contract: `miy describe --command "app deploy"`. Read its arguments, input schema, permissions and effects; use `--help` when useful. The installed Clap tree is authoritative.
3. Execute the user's requested operation. Identify context, app and environment first. Use `--output json` for one structured response; read `data` or `error`, never infer success from submission alone.
4. On failure, inspect `error.code`, `details` and the recovery executable/argument vector. Resume a saved request before retrying. Only `job retry` creates a retry for a typed retryable runtime failure.

Use `context init` to register an explicit server URL when none is configured; there is no default IP/domain. Use `auth login` and `auth wait LOGIN_ID`; never inspect or print credential storage. Noninteractive login requires a configured context, then returns public approval instructions. Existing connections are preserved; follow the reported add/use arguments for a different server. Production operations and app removal require explicit `--context`, `--app`, `--environment` and confirmation (`--yes` for automation). Development permission alone does not authorize production work.

Register the app and obtain administrator approval before `app dev`. Its default local preview uses the server development DB through the CLI-managed tunnel. Dev deployment reuses that same DB; `app dev --runtime docker --database local` is the explicit separate-DB option. Stop preview with `app dev-stop` before build/deploy. Schema changes may stop the deployed development app; preserve all applied SQL. Production uses a separate DB and does not copy development rows.

Read [workflow.md](references/workflow.md) for the requested workflow; [commands.md](references/commands.md) is the generated complete reference. MCP `get_cli_help` provides the same scoped discovery. Do not load the complete manual for every task.

For changes to app UI, sharing or files, read [harness.md](references/harness.md), `miy describe --topic harness`, or MCP `get_cli_help({"topic":"harness"})` before implementing. Choose components from the synced UI and run `miy app check --harness-only` before the full validation.

For CLI updates, read [updating.md](references/updating.md), `miy describe --topic updates`, or MCP `get_cli_help({"topic":"updates"})`; these work before login. Re-run the selected portal's installer, use the newly installed global `miy app setup` in each compatible project, then restart Codex/Claude Code. An old project's CLI cannot update itself to a newer global executable. Updates preserve authentication; do not repeat browser approval for a credential-storage error. Inspect `auth status` and send a manual unauthenticated report when needed.

Report actual checks, release/image IDs and unresolved failures. Acceptance records attest only tests actually performed; follow `docs/ACCEPTANCE.md`. Update compatible project tooling with the intended global `miy app setup`, or use explicit `app upgrade` for a different SDK bundle. Do not edit managed guidance, locks, protected SDK files or global agent settings; reopen the agent session after updates.

Local development defaults to native Rust plus the built React preview and an in-process tunnel to the server dev DB. Docker Desktop, WSL and local PostgreSQL are not needed for `app dev` or `doctor --for develop`. Explicit `--runtime docker` selects the container workflow for local testing only. `app build` always builds on the selected server's Linux Docker engine with the existing login; no client needs local Docker or SSH, and only a server build can be deployed. A local preview, native or Docker, is not a releasable image.
