@AGENTS.md

Before CLI work, read [.claude/skills/miy-cli/SKILL.md](.claude/skills/miy-cli/SKILL.md) and [docs/CLI-REFERENCE.md](docs/CLI-REFERENCE.md), available locally before MCP login or connection. Read the miy-app skill for implementation and SDK contracts, and use miy_app MCP get_contract and get_cli_help before development. Follow AGENTS.md for CLI updates and reopen the agent session afterward.

No server address is assumed. Register one with `miy context init` if needed. Obtain app registration approval before local preview. `app dev` uses the server development DB; development deployment retains that same DB and data. Use `app dev-stop` before build/deploy. Schema changes may stop the deployed dev app. Only explicit `--runtime docker --database local` creates a separate local DB; production always uses a separate server DB.

Public UI components are distributed independently of the CLI. After login run `component sync` to install defaults or restore exact locked releases. Query MCP `list_components` / `get_component` for current usage; use `component add NAME` or `component update NAME --version X.Y.Z` explicitly. Import from `@miy/components/NAME`; `@miy/ui` remains the basic UI alias. Never hand-edit `platform-components/` or its lock. Commit the entire directory. Publishing a new registry version does not update an app; review, test and redeploy explicitly.

Local development defaults to native Rust plus the built React preview and an in-process tunnel to the server dev DB. Docker Desktop, WSL and local PostgreSQL are not needed for `app dev` or `doctor --for develop`. Explicit `--runtime docker` selects the container workflow. Image build/deployment still require a Linux Docker engine; native preview is not a releasable Linux image.
