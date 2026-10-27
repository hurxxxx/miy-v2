# miy application bundle v0.6.2

This describes bundle 0.6.2 and its CLI 0.5 workflow; application runtime protocol versions remain unchanged. Existing apps must follow their installed protected SDK and use an explicit bundle upgrade before adopting new SDK methods. Project stdio MCP reports the installed bundle; the server HTTP contract describes the latest bundle.

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
The evaluator runs one CLI build and deploys its immutable image ID. The server extracts the same image's UI and migrations and checks the manifest before preparing the DB and starting the container. The CLI transfers the immutable image archive to the platform; the local and server Docker engines may be separate.
The bundle and its protected SDK/Dockerfile/toolchain/guidance/initial migration are pinned in miy.bundle.json and verified against the embedded CLI. Use the CLI shipped with this app. Existing bundles are not silently upgraded. This is a reproducibility check, not a security boundary against malicious source code.
Format app code with `cargo fmt --package miy-app`; `miy app check` also verifies that the shipped SDK already passes rustfmt. Do not fix SDK formatting yourself.
Registration: developer runs `miy app register`; administrator runs `miy registration list`, `miy registration approve ID`, and optionally `miy grant set --user USER_ID --permission use`. Approval creates the app and grants only its requester develop permission in dev. A request alone grants no access. Use `miy user list` to find IDs as administrator.
Build: commit source, then `miy app build` (which includes the full check). Deploy: `miy app deploy --environment dev`. A stale build is rejected. The CLI uploads the image archive over the authenticated platform API. The built Linux image platform must match the selected server; choose an explicit build --platform and a working builder when host/engine and server CPUs differ.
Long jobs: use `miy app deploy --no-wait`, then `miy job watch JOB_ID`. Request and upload receipt IDs are journaled before submission. Use `job resume REQUEST_OR_JOB_ID` to recover the same original input after a lost acknowledgement; it may explicitly retransmit after lookup. No byte-range archive continuation is provided. Use `job retry FAILED_JOB_ID` only when the typed failure permits it, after fresh preflight and confirmation. A running old release is not evidence that an update succeeded.
CLI login uses a selected context and browser/device approval. `auth login` returns pending instructions outside a TTY; approve the URL/user code and run `auth wait LOGIN_ID`. Access tokens last up to 15 minutes; rotating refresh sessions up to 30 days. Credentials are stored by server/user in the protected credential store. Project stdio MCP uses the same selected session. Explicit HTTP loopback development login is a separate testing shortcut, not evidence of enterprise identity-provider integration.
Discover commands with `miy describe`, then `miy describe --command "app deploy"`. `doctor --for operate` checks remote access; `doctor --for develop` also checks the app bundle, tools and Docker.

## Local development and data connections

`miy app dev` builds the current working tree and runs a local Docker backend and PostgreSQL database, with a browser preview on loopback. It does not write into the platform's app database. The preview gateway holds your CLI credential, signs your actual user identity, and proxies authorized calls to the platform's dev environment. Stop with Ctrl-C or `miy app dev-stop`; local database volume is retained.

Declare `platform:directory:read` in requestedPermissions and call `runtime.platformApi("directory")` for the shared user directory (id/name only). Declare `app:TARGET:read` or `app:TARGET:write` and use `runtime.appApi(TARGET,"/path",options)` to call another app in the same environment. The platform also requires the caller's access to that target; declarations do not grant user access. Target backend business authorization still applies. No cross-database credentials are exposed.

The UI executes trusted React code inside the platform's JavaScript context. CSS Modules and runtime helpers avoid accidental interference, but this is not a sandbox for hostile JavaScript. Use local styles, clean up global event listeners/timers in effects, and do not change document/body or platform navigation directly.

## Delivery

`app build` requires committed clean source. `app deploy` uploads the immutable Docker image archive and deploys to dev. Client and server may use separate Docker engines. The built Linux image must match the server platform. Uploaded archives are capped at 1 GiB. The platform is not restarted. Use `app status` and `job get ID` to check installation, release, image and terminal job state; a running old version is not evidence that an update succeeded.

After browser/API testing, `app accept --note "actual checks"` records acceptance for the exact dev release. `miy --context CONTEXT --app APP --yes app promote --environment prod --release UUID` deploys that same image to a separate prod installation/database; it does not copy dev data. Only administrators or users granted prod/deploy may promote or control prod. Runtime routes explicitly include environments: `/environments/dev/apps/ID` and `/environments/prod/apps/ID`.

## Component distribution

The SDK protocol remains in the versioned foundation bundle. UI implementations
come from the platform component registry using protocol `miy.component/v1`.
The template declares initial defaults in `miy.components.json`; the CLI installs
them after login with `component sync`. New component releases do not change
the CLI binary or any installed app. `platform-components/` is a managed source
directory with its own version/hash lock. Import `@miy/components/NAME` and keep
business extensions outside that directory. App check/build verifies both locks.

## Navigation and application lifecycle

Register unsaved input or in-flight work with `runtime.registerNavigationGuard`.
Return a message while leaving would discard work; otherwise return undefined.
Register in an effect and return the disposer. Include every state used by the
callback in effect dependencies, or read it through a current ref. Example:

```tsx
useEffect(() => runtime.registerNavigationGuard(() =>
  dirty || saving ? "저장되지 않은 작업이 있습니다. 이동하시겠습니까?" : undefined
), [runtime.registerNavigationGuard, dirty, saving]);
```

The platform and local preview apply this to runtime.navigate, host menu links,
back/forward, and document unload. Browsers use their own unload message and may
suppress prompts without user interaction. Do not add a second confirm around
runtime.navigate or manipulate browser history directly. Remove the guard when
work is saved/discarded; clean it up on unmount. Registration is required: hosts
cannot infer application draft state. Old apps continue to run, without automatic
draft protection. When a deployed release changes, the host retains the mounted
view and blocks stale API calls until the user reloads.

`app dev-stop` requests shutdown of this project's runner, waits for the port and
lock to be released, and removes its containers. The local DB volume survives.
During startup it waits for the current build/migration to finish. A timeout is
reported as pending, never successful. Old CLI runners must first be stopped with
Ctrl-C in their terminal. Deployment does not stop a local preview automatically.

## Names and acceptance records

The platform display name is administrative metadata. Deployment never renames
it from manifest.json. Administrators use `app rename --name "Display name"`;
`app show` reports platform app and installation metadata; manifest.json remains the local source name. This changes no source or release metadata.

`app accept --note "summary" --report acceptance.json` records user-attested tests
for the report's exact releaseId. See docs/ACCEPTANCE.md for the format and
platform-boundary checklist. The authenticated recorder and test actors are
separate. Failed tests cannot be submitted as approval; use not_run with a reason
for explicitly untested scope. Legacy note-only acceptance remains supported and
is marked unstructured. Neither format automatically proves business correctness.

## CLI input and runtime control

`app accept --input @FILE` accepts strict `{releaseId,note,checks}` JSON; `grant set --input @FILE` accepts `{userId,appId?,environment,permission}`. Use `-` for explicit stdin. Unknown fields and conflicting field sources are rejected. Discover exact schemas in `describe --command`.

`app start|stop|restart|remove` replaces app control. Prod changes and every app removal require explicit `--context`, `--app`, `--environment` and confirmation (`--yes` for automation). Dry-run uses the same target and returns planned changes/expected state without upload or job creation. Admission and execution recheck state and permissions; app removal preserves the app database. Context removal only affects local configuration.

`app logs --environment dev --tail 100` returns a bounded snapshot with truncation/rotation limits. Resume a known log request with `--request ID`. Do not treat it as retained historical logging.
