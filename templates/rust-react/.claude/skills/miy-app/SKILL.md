---
name: miy-app
description: Develop, test, and deploy this miy Rust and React application using its bundled CLI, SDK contract, and platform MCP. Use for app implementation, local preview, or requested miy deployments.
---

Read `AGENTS.md` and `docs/CONTRACT.md` at the app root. Discover the required CLI command with `describe --command "GROUP ACTION"`. Query `miy_app.get_contract` and relevant `search_api` entries for SDK/API details. If MCP login is missing, select the intended context, run `.miy-tools/miy auth login`, approve the public URL/code and use `auth wait LOGIN_ID` if pending; do not inspect credentials or silently replace the live contract with assumptions.

Work in `src/`, `web/src/`, and new numbered SQL migrations. Preserve protected SDK and published migrations. Use verified backend `Identity`, `runtime.api`, and declared `runtime.appApi` / `runtime.platformApi` connections. Use CSS Modules and effect cleanup to coexist with the platform.

Use `.miy-tools/miy app check` to verify the development contract. Register an explicit server URL with `context init` if needed; no default address is assumed. Register the app and obtain administrator approval before `app dev`. It tests the working tree locally against the server development DB through the managed tunnel, with real authorized API connections. Development deployment reuses that same DB and rows. Use `--runtime docker --database local` only for an explicitly separate local DB. Schema changes may stop the deployed dev app. Stop preview with `app dev-stop` before build/deploy; production uses a separate DB without copying development rows.

When deployment is requested, increment `manifest.json` version for changes to an existing release, commit source, run `app build`, then `app deploy`. Inspect `app status`, the completed `job get ID` and their release/image/version, and test the deployed URL. Do not report success on submission alone. For requested production deployment, record completed checks using `app accept --note ...` and promote the returned exact release with `miy --context CONTEXT --app APP --yes app promote --environment prod --release UUID`; server-side prod/deploy permission is required.

Report exact checks, deployed environment/version, and unresolved failures. Do not bypass a failed contract or change global agent settings to get past a tool error.

Public UI components are distributed independently of the CLI. After login run `component sync` to install defaults or restore exact locked releases. Query MCP `list_components` / `get_component` for current usage; use `component add NAME` or `component update NAME --version X.Y.Z` explicitly. Import from `@miy/components/NAME`; `@miy/ui` remains the basic UI alias. Never hand-edit `platform-components/` or its lock. Commit the entire directory. Publishing a new registry version does not update an app; review, test and redeploy explicitly.

For forms or in-flight actions, register `runtime.registerNavigationGuard` in an effect with cleanup (see docs/CONTRACT.md); test app links, platform menus, browser history and refresh. Do not infer dirty state automatically or add duplicate confirms. Use docs/ACCEPTANCE.md to record actual integration boundaries and not_run items with `app accept --report`. Stop a completed local preview with app dev-stop and verify it can restart. Platform display names are administrator-managed via app rename, independently of manifest names.

For CLI failures, use the structured recovery executable/argument vector. `job resume ID` reconnects to the original request; only `job retry ID` can retry an eligible terminal failure. Use explicit production/removal targets and dry-run before authorized changes. The selected server platform must match the built Linux image; cross-CPU builds require an explicit supported `--platform` and a working Docker builder.

Local development defaults to native Rust plus the built React preview and an in-process tunnel to the server dev DB. Docker Desktop, WSL and local PostgreSQL are not needed for `app dev` or `doctor --for develop`. Explicit `--runtime docker` selects the container workflow. Image build/deployment still require a Linux Docker engine; native preview is not a releasable Linux image.
