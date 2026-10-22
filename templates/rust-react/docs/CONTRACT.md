# miy application bundle v0.3.0

Backend: Rust 1.99.1, Axum 0.8.9, SQLx 0.8.6. UI: React 19.3.1, Vite 8.3.5, Module Federation.
Keep manifest.json `miy.app/v1alpha1`, runtime `rust-web-v1`, UI `miy.ui/v1alpha1`, export `./AppRoot`.
Set ui.moduleEntry to `mf-manifest.json`. The Federation manifest carries the ESM entry type and CSS asset metadata; remoteEntry.js alone is not the host registration document.
The host provides AppProps.runtime with installationId, releaseId, basePath, location, api, navigate, theme.
API paths passed to runtime.api are app relative (for example `/leaves`); the gateway forwards `/api/leaves` to Rust.
The Rust SDK wraps your router under `/api`, so route definitions use `/leaves`, not `/api/leaves`.
The SDK requires a valid short-lived platform identity signed for the installation/release. Extract `Extension<Identity>` in handlers. The identity has `user_id: String` and `admin: bool`.
Connect using `miy_app_sdk::database()`. Handlers can use `State<PgPool>`. Build your router with `.with_state(pool)` and call `miy_app_sdk::serve(router)`.
The runtime role has CRUD privileges in schema `app` and no DDL, migration-history, or other database access. The default search_path is app. SQL migrations run as a separate migration role.
SQLx owns each migration transaction. Do not put explicit BEGIN, COMMIT or ROLLBACK wrappers or other transaction-control statements in migration files. Use unique positive numbered names such as 0002_records.sql. No up/down pairs or no-transaction directives. Preserve every published migration, including 0001_initial.sql; add a new file for schema changes. The CLI and server enforce the same transaction/file rules; SQL syntax and execution are verified when the platform applies the migration to PostgreSQL.
Return JSON; business errors should have `{error:{code,message}}` and the appropriate 4xx status. Do not include DB connection strings in errors.
Use local CSS modules to avoid host style collisions. UI navigation stays under the app basePath. Server-side permission checks are required even if UI buttons are hidden.
The image contains `/miy/manifest.json`, `/miy/ui/`, `/miy/migrations/`, `/miy/build.json`, and Rust executable `/usr/local/bin/miy-app`. It listens on 8080 and serves GET /health.
The evaluator runs one CLI build and deploys its immutable image ID. The server extracts the same image's UI and migrations and checks the manifest before preparing the DB and starting the container. Local validation currently uses the shared Docker Desktop image store, not a remote registry.
The bundle and its protected SDK/Dockerfile/toolchain/guidance/initial migration are pinned in miy.bundle.json and verified against the embedded CLI. Use the CLI shipped with this app. Existing bundles are not silently upgraded. This is a reproducibility check, not a security boundary against malicious source code.
Format app code with `cargo fmt --package miy-app`; `miy app check` also verifies that the shipped SDK already passes rustfmt. Do not fix SDK formatting yourself.
Registration: developer runs `miy app register`; administrator runs `miy app registrations`, `miy app approve ID`, and optionally `miy app grant --user USER_ID --permission use`. Approval creates the app and grants only its requester develop permission in dev. A request alone grants no access. Use `miy app users` to find IDs as administrator.
Build: commit source, then `miy app build` (which includes the full check). Deploy: `miy app deploy --environment dev`. A stale build is rejected. The immutable image ID currently must exist in the same Docker Engine as the platform runner; remote transfer/registry is a separate future transport.
Long jobs: `miy app deploy --no-wait`, then `miy app resume`. Request keys are saved before submission; rerunning an interrupted submission reuses the key. A failed attempt requires explicit `--retry`. Resume never starts a different job. After success, another deploy/control invocation is a new operation.
CLI login currently uses explicitly enabled localhost development accounts. Real MCP/web OAuth remains separate; CLI OAuth is not implemented by this bundle.
Useful commands: `.miy-tools/miy app check`, `.miy-tools/miy app build`, `.miy-tools/miy app deploy`, `.miy-tools/miy app status`, `.miy-tools/miy app resume`.
