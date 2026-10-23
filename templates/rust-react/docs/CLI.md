# App CLI quick reference

Use `.miy-tools/miy` in this app or `miy` from your PATH. `app describe` returns the installed CLI's full command reference as JSON; MCP `get_cli_help` exposes the same reference. `--help` works on every command.

| Step | Command | Meaning |
| --- | --- | --- |
| Environment | `app doctor`, `app setup` | Check tools; configure project Codex and Claude Code MCP |
| Login | `app login --url https://PLATFORM` | Open browser, approve matching request ID, save expiring private CLI credential |
| Local shortcut | `app login --account developer` | Explicit loopback-only validation profile |
| Registration | `app register`, `app registrations` | Request registration; deployment waits for administrator approval |
| Administration | `app approve UUID`, `app reject UUID`, `app users`, `app grant --user UUID --permission deploy --environment prod` | Administrator approval and separate app/environment grants |
| Format | `app fmt` | Format app Rust code even when cargo is absent from the agent PATH |
| Check | `app check` | Bundle, migration contract, Rust tests/format, TypeScript, UI assets |
| Local preview | `app dev`, `app dev-stop` | Working tree with its own local DB; actual authenticated dev server API connections |
| Build | `app build` | Clean committed source to one Docker image; increment manifest version for updates |
| Test deployment | `app deploy`, `app status` | Upload archive to server, run DB migration and container, inspect actual version/image |
| Resume | `app deploy --no-wait`, `app resume` | Persisted deployment job; explicit `--retry` only after addressing failure |
| Test acceptance | `app accept --note 'checks performed'` | Record testing for exact healthy dev release; does not automatically test business behavior |
| Production | `app promote --release UUID` | Deploy accepted exact image to independent prod DB; requires prod/deploy permission |
| Lifecycle | `app control start`, `stop`, `restart`, `remove`, with `--environment dev|prod` | Control this app only; removal preserves DB |
| Sign out | `app logout` | Revoke browser-approved CLI credential |

`app login` and the standard project stdio MCP share one saved CLI credential. For legacy direct HTTP OAuth MCP configurations, the old `setup-codex` launcher remains separate. Prefer standard project MCP in new apps. Approve the agent client's project trust/MCP prompt normally; no global config or sandbox bypass is required.

Local preview and dev deployment are different: local changes must be rebuilt before server deployment. Production promotion never rebuilds or copies dev database rows. The local CLI and server image architecture must match; use an appropriate Docker builder for the server CPU. Current archive limit is 1 GiB.

## Components independent of CLI releases

After `app login`, run `app components sync` once before checks or local preview.
It installs the template's declared default UI from the authenticated platform registry.

```sh
.miy-tools/miy app components list
.miy-tools/miy app components show ui --version 1.0.1
.miy-tools/miy app components sync
.miy-tools/miy app components add COMPONENT --version X.Y.Z
.miy-tools/miy app components update COMPONENT --version X.Y.Z
.miy-tools/miy app components check
```

`list` and MCP `list_components` query the live registry. `get_component` supplies
usage and compatibility. The CLI never needs a component-specific command or rebuild.
`add`/`update` without a version select the latest compatible stable release; sync
restores pinned releases and never upgrades existing components.

Import new components from `@miy/components/NAME`. Basic Button/Panel remain at
`@miy/ui`. Commit `platform-components/` including `miy.components.lock.json`.
The lock records platform origin, exact versions, package digests and file hashes.
Checks/builds use installed files without contacting the registry. Missing files
can be restored with sync; modified managed files are refused, not overwritten.
Keep custom components in `web/src`. Component updates still require app version
increase, commit, tests, build and deployment to affect running applications.

Administrators may run `app components publish /path/to/component-directory`.
The source directory has component.json metadata and an explicit file list.
Only approved trusted source should be published. Packages contain source/docs,
not install hooks, arbitrary dependency installation or credential configuration.
Authentication components are not yet part of this registry.

`app dev-stop` releases the gateway, port, project lock and containers; DB data is retained.
If an older CLI runner is active, stop it with Ctrl-C before starting the updated CLI.
`app rename --name 'Display name'` is administrator-only platform metadata.
`app accept --note 'checks' --report acceptance.json` records structured test scope,
actors and results for the exact release. See [ACCEPTANCE.md](ACCEPTANCE.md).

For bundle 0.5 projects: first use the old bundled CLI to update ui to 1.0.1 and any
other components to releases supporting 0.6.0, then commit. Run the new global
`miy app upgrade`, review the diff and lockfile, run `app check`, commit and redeploy
when desired. Protected SDK files must not be edited by hand. Upgrade does not add
guard usage to business code; implement the documented SDK registration explicitly.
