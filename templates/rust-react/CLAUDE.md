@AGENTS.md

Before CLI work, read [.claude/skills/miy-cli/SKILL.md](.claude/skills/miy-cli/SKILL.md) and [docs/CLI-REFERENCE.md](docs/CLI-REFERENCE.md), available locally before MCP login or connection. Read the miy-app skill for implementation and SDK contracts, and use miy_app MCP get_contract and get_cli_help before development. Follow AGENTS.md for CLI updates and reopen the agent session afterward.

Public UI components are distributed independently of the CLI. After login run `app components sync` to install defaults or restore exact locked releases. Query MCP `list_components` / `get_component` for current usage; use `app components add NAME` or `app components update NAME --version X.Y.Z` explicitly. Import from `@miy/components/NAME`; `@miy/ui` remains the basic UI alias. Never hand-edit `platform-components/` or its lock. Commit the entire directory. Publishing a new registry version does not update an app; review, test and redeploy explicitly.
