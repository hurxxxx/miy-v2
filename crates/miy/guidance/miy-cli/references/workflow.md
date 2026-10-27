# MIY CLI workflows

This is the CLI 0.6 command contract. Use the project's `.miy-tools/miy` on Unix or
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
The operate doctor checks authentication/server capabilities without requiring
Rust, Node or Docker. Develop additionally checks project integrity, components,
Rust/Node/pnpm/Git/Docker versions, rustfmt, an actual tiny Rust link (MSVC/Windows
SDK on Windows), exact Codex/Claude MCP configuration, a bounded local stdio MCP
initialize/tools/list probe, Linux Docker mode and server/image architecture.
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

`app dev` builds the working tree and opens a single local development session for
the app. The server prepares its development DB and applies the validated SQL;
the local backend connects through a managed WS/WSS tunnel. Local preview and the
later dev deployment use the same DB and rows. Schema changes stop an already
deployed dev app; applied SQL files must not be edited or deleted. The previous
release is not automatically restarted against a changed schema.

Use `app dev --database local` only when a separate local DB is explicitly wanted.
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
SQL bytes, commit clean source and run `app build`. It builds one immutable Rust/React Linux image. The
image platform must match the selected server. When local Docker has another CPU,
choose `app build --platform linux/amd64` (or `linux/arm64`) explicitly and configure
a working Docker emulation/builder; the flag alone does not install emulation.

```sh
miy app deploy --environment dev --dry-run
miy app deploy --environment dev --no-wait
miy job get JOB_UUID
miy job watch JOB_UUID
miy --output jsonl job watch JOB_UUID --after EVENT_CURSOR
miy job resume JOB_OR_REQUEST_UUID
```

Dry-run returns checked state, planned changes and checks still deferred to the
worker. It creates no image upload or runtime job. Actual submission persists the
original request key, actor/context, target, payload and expected state first.
Image exports are limited to 1 GiB. Upload receipt lookup and runtime request lookup
recover a lost acknowledgement. Only explicit `job resume` may retransmit an
unacknowledged archive/request after lookup, using its original key and payload.
This is whole-archive recovery, not byte-range continuation.

Watch cursors reconnect to events. Ctrl-C/timeout stops local observation, not the
remote job. Inspect terminal failure before `job retry JOB_UUID`; only the server's
typed transient classes permit it. Retry performs fresh preflight and preserves
its link to the failed job. A terminal failed image receipt has no runtime job to
retry: inspect it, then explicitly start a new `app deploy` for a new upload.
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

Global installation does not update every project's pinned CLI. In a compatible
project run the intended new global `miy app setup`; for a different SDK bundle,
commit clean source and use explicit `miy app upgrade`. Review changes and check
again. Applied SQL and user business files must be preserved. Keep managed files
LF, component/SQL byte hashes intact, and credentials/executables out of Git.

Old `app login/logout/doctor/describe/components/mcp/registrations/approve/reject/users/grant`
commands are replaced by the corresponding root groups. `app resume` is replaced
by `job resume ID`; `app control` by explicit lifecycle commands. `app setup-codex`
and `app codex` are removed; use `app setup` and your ordinary agent launcher.
Removed forms return usage errors with recovery guidance. Deployment `--retry`
and `deploy --environment prod` are invalid; use `job retry` and `app promote`.

`context list --limit/--cursor` pages the local context map and restarts when configuration changes. `component list` returns `{components,nextCursor}`; it retrieves the complete authenticated catalog before paging locally. Fields such as `components.name` and `contexts.work-prod.origin` select keys from the authorized result.
