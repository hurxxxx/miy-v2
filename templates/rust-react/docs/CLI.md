# CLI 0.5 quick reference

Use `.miy-tools/miy` in this app (`.miy-tools/miy.exe` on Windows). Start with
`describe`, then `describe --command "GROUP ACTION"`; `--search` finds candidates
and `--all` explicitly loads the full reference. Every command supports `--help`.
The generated [CLI-REFERENCE.md](CLI-REFERENCE.md) contains the installed workflow.

| Task | Commands | Contract |
| --- | --- | --- |
| Connect | `context add NAME --url https://PLATFORM`, `context use NAME` | Origin, optional CA and user binding; no credentials in project sources |
| Sign in | `auth login`, `auth wait LOGIN_ID`, `auth status` | Approve the public URL/user code; noninteractive login returns pending |
| Diagnose | `doctor --for operate`, `doctor --for develop` | Operations need remote access; development also needs project/tool/Docker checks |
| Configure agents | `app setup`, then ordinary `codex` or `claude` | Project stdio `mcp`; accept normal trust/MCP prompts |
| Register | `app register`, `registration list`, `registration approve ID` | Administrator approval grants requester develop access in dev |
| Permissions | `user list`, `grant list`, `grant set`, `grant revoke` | Administrator grants are scoped to app/environment |
| Develop | `app fmt`, `app check`, `app dev`, `app dev-stop` | Check executes project tooling; preview has its own local database |
| Components | `component list/show/add/update/sync/check/publish` | Lock exact sources; publishing needs administrator authority |
| Build | `app build [--platform linux/amd64]` | Clean committed source; Linux image must match the server platform |
| Deploy | `app deploy --environment dev --dry-run`, `app deploy --no-wait` | Preflight then image receipt/job journal; no prod deployment shortcut |
| Observe | `job list/get/watch`, `job resume ID` | Resume the same request; Ctrl-C does not cancel remote work |
| Retry | `job retry ID` | Only typed retryable terminal failures, with fresh checks/confirmation |
| Accept | `app accept --note 'observed checks' --report FILE`, `app accept --input @FILE` | Exact release and actual evidence; see [ACCEPTANCE.md](ACCEPTANCE.md) |
| Promote | `--context NAME --app APP --yes app promote --environment prod --release ID` | Exact accepted image; separate prod DB, no dev-data copy |
| Lifecycle | `app start/stop/restart/remove --environment dev` | Prod and all removals require explicit target and confirmation; DB is preserved |
| Logs | `app logs --tail 100 --since RFC3339`, `app logs --request ID` | Bounded snapshot; truncation and rotation may omit data |
| Sign out | `auth logout`, `auth sessions list/revoke` | Distinguish server revocation from local removal; `--local-only` skips revocation |

`--context` overrides `MIY_CONTEXT`, project binding and the active context in that
order. `--project PATH` selects a project; `--app APP` allows operation without
local source. `context remove` changes local configuration, not remote sessions or
app data. `--account developer` is an explicit enabled HTTP loopback login shortcut.
Access tokens last up to 15 minutes and rotating refresh sessions up to 30 days;
a lost refresh response requires fresh login. Never inspect credential storage.

Use `--output json` for one UTF-8 envelope. Successful values are under `data`;
errors include stable code, details and optional recovery executable/argument array.
`--output jsonl job watch ID` streams event envelopes and supports `--after CURSOR`.
Read commands support field selection and list pagination, for example
`--fields apps.id,apps.name,nextCursor app list --limit 20 --cursor CURSOR`.
Strict `--input @file`/`--input -` documents reject unknown and duplicated fields.

Prod changes and every app removal require explicit `--context`, `--app` and
`--environment`, including dry-run and retry. Dry-run skips confirmation and does
not upload or create work. Other executions confirm interactively or require
`--yes`. Permission, release acceptance and expected-state checks still apply.

Upload response recovery covers the receipt before the runtime job exists. Only
explicit resume may retransmit a missing receipt's whole original archive with
the same request key. It does not continue a partial byte range. Preserve the
reported request/job ID; inspect a failed receipt before explicitly deploying a
new archive. Runtime retry uses `job retry`, never deploy `--retry`.

Import managed UI components from `@miy/components/NAME`; basic UI remains at
`@miy/ui`. Commit `platform-components/` and its lock, and keep custom code in
`web/src`. Sync restores pinned releases; update is explicit. Component publication
does not rebuild or redeploy an app. Current archive size limit is 1 GiB.

Installing a global CLI does not change a pinned project. Use the intended global
`miy app setup` for compatible tooling, or commit clean source and use explicit
`miy app upgrade` for a different supported bundle. Review and check changes; do
not hand-edit protected SDK files, managed guidance or applied SQL. Old CLI names
such as `app login`, `app resume` and `app control` return migration guidance and
are not aliases. Use the old bundled CLI only when preparing an older bundle for
its documented upgrade path.
