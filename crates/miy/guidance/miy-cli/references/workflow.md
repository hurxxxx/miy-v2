# MIY CLI workflows

This is the CLI 0.7 command contract. Use the project's `.miy-tools/miy` on Unix or
`.miy-tools/miy.exe` on Windows. Use the intended global `miy` to initialize or
explicitly update a project. Exact parser definitions are in the generated
[command reference](commands.md); start with `describe --command "GROUP ACTION"`.
Implementation availability and successful validation are different: consult the
repository validation records for actual OS, server and end-to-end evidence.

## Context and authentication

```sh
# Fresh interactive setup asks for the target URL; there is no default address.
miy context init
miy auth login
# When login returned pending, approve its URL/code, then:
miy auth wait LOGIN_UUID
miy auth status
miy doctor --for operate
```

The platform install command includes its configured server URL in `--server` or
`-Server`; this is an explicit argument, not an address built into the CLI. Without
it, interactive installation asks for the URL; noninteractive installation or a
cancelled prompt installs the binary and reports that server setup is required.
Run `context init --url URL` for unattended fresh setup, or `context add NAME --url
URL` followed by `context use NAME` for additional connections. Existing settings,
including intentionally empty configurations, are preserved; a different supplied
address is reported with explicit add/use recovery arguments. `auth login` asks
for a URL on a fresh interactive client; noninteractive login never guesses one.
Use a complete `https://domain[:port]` or `http://IP:port` origin. HTTP requires an
explicit internal-server HTTP configuration and carries unencrypted traffic.

Add `--ca-file` only when a private CA is required. TLS verification stays enabled. Context selection
is explicit `--context`, then `MIY_CONTEXT`, project binding, then active context.
`--project PATH` selects a project directory; operational commands can instead use
`--app APP_ID` without a local source tree. Never silently fall back to another
server when explicit context selection fails. `context remove NAME` removes the
local context; it does not revoke remote sessions or delete app data.

`auth login` creates a five-minute device approval request. Only the verification
URL, user code and login ID are public. Non-TTY, `--no-input` or `--no-wait` returns
pending immediately. Interactive login may open the browser and wait; `--device-code`
or `--no-open` suppresses automatic browser opening. `auth wait ID` polls the saved
flow, respects slow-down and preserves it on Ctrl-C. The explicitly requested
`auth login --account developer` shortcut only works on an enabled HTTP loopback
development server; it approves the same device flow and waits even without a TTY.

Access tokens last up to 15 minutes and refresh sessions up to 30 days. Credentials
are bound to server origin and user, stored through the platform credential store
with its protected fallback, and never placed in project sources or normal output.
Renewal is serialized across CLI processes. A lost renewal response requires fresh
login rather than replaying the old refresh token. `auth sessions list` lists this
user's sessions; `auth sessions revoke ID` explicitly revokes one. `auth logout`
revokes remotely before local removal; `--local-only` explicitly skips revocation.
A local deletion failure and uncertain remote revocation are reported separately.
Core `users.enabled` is checked by the server; upstream identity-provider disable
propagation is not implied by that check.

## Scoped user delegation

Use `auth delegations create --app APP --environment dev --action read --action build
--action deploy --name AGENT --expires-in 3600 --credential-file PRIVATE_PATH` to
issue a separate user credential with exactly those actions. Repeat `--action`
for each needed operation; actions never imply another action. The lifetime is
explicit, at most 24 hours and bounded by the issuing CLI session. This is user
delegation, not a service account, and a delegated credential cannot delegate again.

The file contains secrets and belongs outside source control. Transfer it only to
the intended agent, create a matching server context in a separate `MIY_CONFIG_DIR`,
and run `auth delegations import --credential-file PRIVATE_PATH`. Import verifies
the server and delegation, refuses existing credentials and retains the input file;
remove that private copy after the recipient has stored it. `auth delegations list`
and `auth delegations revoke ID` inspect or revoke the owner's grants. Session
refresh never increases scope or extends the fixed delegation expiry. Revocation,
expiry and current user permissions are rechecked before queued work and before
publishing a long-running build result. A running irreversible action is not undone.

Read metadata requires `read`; app requests additionally require `app_read` or
`app_write`, and directory access requires `directory_read`. Other actions are
`build`, `dev_session`, `prepare_database`, `accept`, `deploy`, `promote`,
`start`, `stop`, `restart`, `remove` and `logs`. Production operations still require
explicit target confirmation. Local stdio MCP uses the selected CLI credential;
the HTTP MCP endpoint accepts the same scoped Core access credential as well as
its existing OIDC authentication. Neither transport changes authorization.

`permissions get --installation ID [--release ID]` compares manifest requests with
installation approvals. `permissions set --installation ID --input @approval.json`
uses `{ "releaseId": "UUID", "expectedRevision": NUMBER, "permissions": [] }` to
approve exactly the selected permissions. New requests are not automatically granted.

## Output, input and discovery

`--output auto|text|json|jsonl` is global. Auto uses text on stdout TTY and JSON
otherwise. JSON is one UTF-8 response with `schemaVersion: 1`, `command`, `ok`,
`data`, `error` and `meta`. Errors contain `code`, `message`, `retryable`, `details`
and optional `recovery` with executable plus argument array. Diagnostics and
child-tool output go to stderr. Help/version, completion scripts and stdio MCP
keep their native formats. JSONL is supported by `job watch` only.

Exit codes: 0 success/accepted `--no-wait`; 1 internal; 2 arguments/input;
3 authentication; 4 forbidden; 5 absent target; 6 conflict; 7 environment/validation;
8 network/service/observation timeout; 9 terminal remote failure; 130 local interruption.
A network error's retryable flag does not authorize a new runtime request.

```sh
miy describe
miy describe --search deploy
miy describe --command "app deploy"
miy --output json --fields apps.id,apps.name,nextCursor app list --limit 20
miy app list --cursor NEXT_CURSOR
miy app accept --input @acceptance.json
miy grant set --input @grant.json
miy completion powershell
```

`describe --all` is the explicit full reference. Input schemas distinguish parsed
CLI arguments from structured `--input` documents. Input accepts `@file` or `-`,
rejects unknown fields and conflicting field sources. `--fields` is for supported
read commands; projections do not bypass server authorization. Cursors are scoped
to the server's filters/user. Never decode or manufacture one. Completion supports
Bash, Zsh, Fish and PowerShell (plus shells advertised by the installed parser).

## Initialize, register and develop

```sh
miy app init ./my-app --id my-app --name 'My app'
cd my-app
miy component sync
miy doctor --for develop
miy app register
# Administrator in the intended context:
miy registration list
miy registration approve REGISTRATION_UUID
miy user list
miy --app my-app grant set --user USER_ID --permission use --environment dev
```

Registration creates no immediate access. Approval creates the app and grants its
requester develop permission in dev; production deploy permission is separate.
A person can sign in only after an administrator registered their identity-provider
account in the console (Platform management, user accounts); `user list` shows
registered people. Registration alone grants no app permission.
The operate doctor checks authentication/server capabilities without requiring
Rust, Node or Docker. Develop additionally checks project integrity, components,
Rust/Node/pnpm/Git versions, rustfmt, an actual tiny Rust link (MSVC/Windows
SDK on Windows), exact Codex/Claude MCP configuration, a bounded local stdio MCP
initialize/tools/list probe and server architecture. `--runtime docker` additionally checks the Linux Docker engine.
The local MCP probe does not prove that a real agent loaded the project or that
live API tool calls work. Develop requires server capability development.sessions.v1.
A failed diagnostic returns exit 7 and its check results.

`app setup` synchronizes the compatible project executable, CLI skills/manual and
Codex/Claude MCP settings. Accept each agent client's project trust/MCP prompt
normally. Start plain `codex` or `claude`; the configured root `miy mcp` stdio
server uses the selected CLI context/session. Its catalog/contract tools are read
operations; deployment remains an explicit CLI operation.

`app fmt` rewrites application Rust formatting. `app check` runs Rust tests and
frontend tooling and writes dependencies/artifacts; project code executes. It
checks managed files, migration rules and build assets, but does not prove actual
browser behavior or database migration execution.

`app init` with no selected context or explicit URL scaffolds an offline unbound
project; it does not invent a localhost address. Register/select the intended
server before remote work. Registration and administrator approval are required
before local preview can use an app's development permissions.

With a matching 0.21.x CLI and core, start local preview before the first dev
deployment. Starting with 0.21.1, preview checks project integrity and builds the preview runtime. Full tests, typecheck and API drift checks run in `miy app check` before publication. A startup 403 on `/preview/apps/{app}/browser/context` means to check the
selected CLI identity, target app and dev grant. Directory access for an app
feature requires the installed dev release to request and receive
`platform:directory:read`. Request that permission only for a feature that uses
the directory. The browser opens the ordinary dev portal sign-in and returns to loopback.
An existing dev session skips sign-in. The SDK Identity belongs to that browser.
Use separate browser profiles for multi-user checks. Before the first dev
installation the browser user needs develop access, and afterwards normal dev
app access applies. Directory and storage still require release approval.
Refresh the global CLI and run `miy app setup` in existing compatible projects.

`app dev` builds the working tree and opens a single local development session for
the app. The server prepares its development DB and applies the validated SQL;
the local backend connects through a managed WS/WSS tunnel. Local preview and the
later dev deployment use the same DB and rows. Schema changes stop an already
deployed dev app; applied SQL files must not be edited or deleted. The previous
release is not automatically restarted against a changed schema.

Use `app dev --runtime docker --database local` only when a separate local DB is explicitly wanted.
Remote failures never trigger that mode automatically. Connected platform calls
still require authorization. Stop with Ctrl-C or `app dev-stop` before building
and deploying; an active session blocks dev deployment. Stop closes the tunnel
and releases the session, preserving server data and any explicit local volume.
Do not claim success while shutdown is pending. Preview is build-and-run, not HMR.

## Components and immutable deployment

`component list` / `show` and MCP `list_components` / `get_component` discover live
registry metadata. `component sync` restores locked releases; `add` and `update`
change them explicitly. Commit `platform-components/` and its lock; custom code
belongs outside managed files. Publishing requires administrator authority and
never updates an installed or running app automatically.

For an update, stop local preview, increment manifest version, preserve applied
SQL bytes, commit clean source and run `app build` (add `--no-wait` to return at
once). It builds one immutable Rust/React Linux image on the selected server, for
that server's platform (server capability `app.builds.v1`). The server runs only
images it built itself: there is no local release build and no image upload, so
test locally with `app dev` and deploy what `app build` produced. The build
uploads up to 64 MiB of clean committed
source, excluding credentials, `.git`, `.miy`, dependencies and build output.
Other committed files outside the application source set are not sent either;
the CLI names them on stderr before uploading, so move anything the build needs
under `src/`, `web/`, `assets/` or another listed source directory.
The worker verifies source, SDK, published components and applied SQL history;
Rust tests and React checks/build execute inside its trusted Docker recipe.
The build never applies SQL or changes an installation. Use `job get BUILD_ID`,
`job watch BUILD_ID --timeout 1800`, `job logs BUILD_ID --after 0` and
`job resume BUILD_OR_REQUEST_ID --environment dev` for durable observations.
JSONL watch emits log sequence cursors. Duplicate input recovers existing work.
After terminal failure, resume observes the original result; a new explicit
`app build` starts a new attempt. A successful result writes a receipt
bound to context, actor, app, source hash/commit and immutable image. Dev deploy
and deployment resume validate it and use that server image directly; without it
`app deploy` fails with `server_build_required`. `job list` remains the
lifecycle job list; save build/request IDs returned by the build.

A server offering `app.release-source.v1` keeps the exact archive of successful
builds. Use `app source download --release RELEASE_ID --out source.tar` to save a
release's verified archive. Restore a historical release from its clean committed
project with `--project PATH app source upload --release RELEASE_ID`, or provide
the exact original tar with `--archive PATH` and a global `--app`. The original
build receipt must match the uploaded bytes. Run `app build` for edited sources.

```sh
miy app deploy --environment dev --dry-run
miy app deploy --environment dev --no-wait
miy job get JOB_UUID
miy job watch JOB_UUID
miy --output jsonl job watch JOB_UUID --after EVENT_CURSOR
miy job resume JOB_OR_REQUEST_UUID
```

Dry-run returns checked state, planned changes and checks still deferred to the
worker. It creates no runtime job. Actual submission persists the
original request key, actor/context, target, payload and expected state first.
The server refuses an image, or a release for promotion, that is not the output of
this app's succeeded server build (`server_build_required`). Runtime request lookup
recovers a lost acknowledgement. Only explicit `job resume` may resubmit an
unacknowledged request after lookup, using its original key and payload.

Watch cursors reconnect to events. Ctrl-C/timeout stops local observation, not the
remote job. Inspect terminal failure before `job retry JOB_UUID`; only the server's
typed transient classes permit it. Retry performs fresh preflight and preserves
its link to the failed job. A failed build has no runtime job to
retry: inspect `job logs BUILD_ID`, fix and commit the source, then run `app build` again.
Use `app status`/`show`, release/image/version and the real deployed URL as evidence;
an old running installation or an accepted submission is not proof of success.

## Acceptance, production and lifecycle

Record actual testing with `app accept --note 'observed checks' --report FILE`, or
`app accept --input @FILE` with `{releaseId,note,checks}`. See `docs/ACCEPTANCE.md`.
Evidence is user attestation for an exact healthy dev release and database schema, not an automated
business-quality verdict. Unchecked scopes must remain `not_run` with a reason.

```sh
miy --context work --app my-app app promote --environment prod --release RELEASE_UUID --dry-run
miy --context work --app my-app --yes app promote --environment prod --release RELEASE_UUID
miy --context work --app my-app --yes app restart --environment prod
miy --context work --app my-app --yes app remove --environment dev
miy --context work --app my-app --yes job retry JOB_UUID --environment prod
```

Prod changes and every app removal require explicit context, app and environment,
including dry-run and retry. Dry-run skips confirmation; execution requires user
confirmation or `--yes`. Authentication and permission checks still apply. A known
job can always be observed without approving another mutation. Resubmission of an
unacknowledged prod/remove request requires the explicit target and confirmation
again, while keeping the original expected state. State changes are rejected.

Promotion uses the accepted immutable image with a separate prod database; it
neither rebuilds nor copies dev rows. `app start|stop|restart|remove` selects one
app/environment; remove preserves its database. `app rename` changes administrative
display-name metadata, never source or manifest identity.

`app logs --environment dev --tail 100 --since 2026-10-27T00:00:00Z` requests a bounded
snapshot. Output indicates truncation and possible omission due to log rotation.
It is not a retained logging service. On timeout/interruption preserve the returned
log request ID and use `app logs --environment dev --request ID` to observe it.

## Tooling updates and removed commands

Use `miy describe --topic updates` or MCP `get_cli_help({"topic":"updates"})`
for the complete offline update procedure. Re-run the selected portal's
`/cli/install` command to update the global CLI first; this preserves contexts
and credentials. Stop processes using the executable before replacing it.

Global installation does not update every project's pinned CLI. In a compatible
project run the intended new global `miy app setup`; for a different SDK bundle,
commit clean source and use explicit `miy app upgrade`. Review changes and check
again. Restart Codex/Claude Code to reload the new project MCP executable and
guidance, then verify the project CLI version and `auth status`. Reauthenticate
only when the authentication error requires it. Applied SQL and user business
files must be preserved. Keep managed files
LF, component/SQL byte hashes intact, and credentials/executables out of Git.

Old `app login/logout/doctor/describe/components/mcp/registrations/approve/reject/users/grant`
commands are replaced by the corresponding root groups. `app resume` is replaced
by `job resume ID`; `app control` by explicit lifecycle commands. `app setup-codex`
and `app codex` are removed; use `app setup` and your ordinary agent launcher.
Removed forms return usage errors with recovery guidance. Deployment `--retry`
and `deploy --environment prod` are invalid; use `job retry` and `app promote`.

`context list --limit/--cursor` pages the local context map and restarts when configuration changes. `component list` returns `{components,nextCursor}`; it retrieves the complete authenticated catalog before paging locally. Fields such as `components.name` and `contexts.work-prod.origin` select keys from the authorized result.

For server build steps `reclaiming_build_cache` and `waiting_for_storage`, keep the accepted job ID. The worker rechecks automatically. After a disconnect use `miy job resume BUILD_ID --environment dev`. For a terminal `docker_storage_low`, inspect `miy job logs BUILD_ID` and report the shortfall to the platform operator. Submit the same commit again once capacity is available.

Local development defaults to native Rust plus the built React preview and an in-process tunnel to the server dev DB. Docker Desktop, WSL and local PostgreSQL are not needed for `app dev` or `doctor --for develop`. Explicit `--runtime docker` selects the container workflow for local testing only. `app build` always builds on the selected server's Linux Docker engine with the existing login; no client needs local Docker or SSH, and only a server build can be deployed. A local preview, native or Docker, is not a releasable image.


Use `app status --environment dev` to inspect the running release and its relationship to local Git history. Review `app accept --note "observed checks" --dry-run`, then explicitly identify the release with `--release` or a report containing releaseId. A saved full successful JSON dry-run or its data object can be passed to deploy/promote with `--if-current @file`.

Platform administrators can inspect a nomination with `app status --environment dev --stewards`. The current roster is owned by the app. Use `--roster-path /roles` for an app that documents that GET endpoint and allows your CLI identity to read it. Use `permissions get --environment dev` for approval state. For an unsupported manifest permission, consult the portal release notes and update the global CLI before updating the project. The first CLI supporting platform:stewards:claim is 0.22.0.
