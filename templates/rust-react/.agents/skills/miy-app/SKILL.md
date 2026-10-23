---
name: miy-app
description: Develop, test, and deploy this miy Rust and React application using its bundled CLI, SDK contract, and platform MCP. Use for app implementation, local preview, or requested miy deployments.
---

Read `AGENTS.md` and `docs/CONTRACT.md` at the app root. Query `miy_app.get_contract`, `get_cli_help`, and relevant `search_api` entries. If MCP login is missing, run `.miy-tools/miy app login`; do not inspect credentials or silently replace the live contract with assumptions.

Work in `src/`, `web/src/`, and new numbered SQL migrations. Preserve protected SDK and published migrations. Use verified backend `Identity`, `runtime.api`, and declared `runtime.appApi` / `runtime.platformApi` connections. Use CSS Modules and effect cleanup to coexist with the platform.

Use `.miy-tools/miy app check` to verify the development contract. Use `app dev` to test the working tree locally with its own database and real authorized server API connections. This is separate from the server's dev installation.

When deployment is requested, increment `manifest.json` version for changes to an existing release, commit source, run `app build`, then `app deploy`. Inspect `app status` and its image/version match, and test the deployed URL. Do not report success on submission alone. For requested production deployment, record completed checks using `app accept --note ...` and promote the returned exact release with `app promote --release UUID`; server-side prod/deploy permission is required.

Report exact checks, deployed environment/version, and unresolved failures. Do not bypass a failed contract or change global agent settings to get past a tool error.

Public UI components are distributed independently of the CLI. After login run `app components sync` to install defaults or restore exact locked releases. Query MCP `list_components` / `get_component` for current usage; use `app components add NAME` or `app components update NAME --version X.Y.Z` explicitly. Import from `@miy/components/NAME`; `@miy/ui` remains the basic UI alias. Never hand-edit `platform-components/` or its lock. Commit the entire directory. Publishing a new registry version does not update an app; review, test and redeploy explicitly.

For forms or in-flight actions, register `runtime.registerNavigationGuard` in an effect with cleanup (see docs/CONTRACT.md); test app links, platform menus, browser history and refresh. Do not infer dirty state automatically or add duplicate confirms. Use docs/ACCEPTANCE.md to record actual integration boundaries and not_run items with `app accept --report`. Stop a completed local preview with app dev-stop and verify it can restart. Platform display names are administrator-managed via app rename, independently of manifest names.
