# MIY CLI workflows

Use `.miy-tools/miy` inside an initialized app. Use the installed global `miy` for
creating a project or explicitly updating its tooling. Exact syntax and options
are in the generated [command reference](commands.md). `docs/CLI-REFERENCE.md`
contains this workflow and the same generated reference for people.

## Initialize and connect

Create an empty destination with `miy app init PATH --id APP_ID --name 'Name'
--url PLATFORM_URL`. Enter it, run `.miy-tools/miy app doctor`, then `app login`.
Browser login requires approving the matching request ID. `--account developer`
is an explicitly enabled loopback development shortcut. Use `--ca-file` for a
private CA, not disabled TLS verification.

Run `app components sync` to install initial registry components. `app register`
requests registration; deployment waits for administrator approval. Administrators
use `app registrations`, `app approve ID`, `app users` and `app grant`. Approval
grants the requester develop permission in dev, not production deployment rights.

`app setup` configures project Codex/Claude Code MCP and synchronizes CLI guidance.
Accept the agent client's project trust and MCP prompts normally. The standard
stdio MCP uses the saved CLI login. `setup-codex` and the `codex` launcher remain
available for legacy direct HTTP MCP configurations; they are not needed for the
standard project setup.

## Local development

`app fmt` formats the application's Rust code. `app check` validates the SDK,
CLI guidance, component locks, migration contract, Rust tests, TypeScript and UI.
It does not prove live browser or database behavior.

`app dev` builds the working tree and runs a local backend/database and browser
preview. The local database is distinct from the server's dev database. Platform
and other-app calls use the CLI user's authorized server dev connections.
Use another `--port` for another project. This is build-and-run, not HMR.

Stop with Ctrl-C or `app dev-stop`. The updated runner releases its gateway, port,
project lock and containers while retaining the local DB volume. Startup shutdown
waits for the current build/migration; a timeout means pending, not success. Old
runners must first be stopped with Ctrl-C in their terminal. Deployment does not
automatically close local preview.

## Components

`app components list` / `show` and MCP `list_components` / `get_component` discover
current releases and usage. `sync` restores locked versions; `add` installs a new
component; `update NAME --version X.Y.Z` changes it explicitly. Commit the managed
sources and lock. Components can be published independently of the CLI. Publishing
requires administrator authority; it does not update running applications.

## Build and deploy to dev

For an update, increment manifest version and preserve published migrations.
Commit clean source, run `app build`, then `app deploy --environment dev`.
Build includes checks and produces one Rust/React Docker image. Deploy uploads its
immutable archive through the authenticated platform API. Different Docker engines
are supported; image/server CPU architectures must match. Archive limit is 1 GiB.

Check `app status`, version/image match and the deployed browser URL. For a long
job, use `--no-wait` and `app resume`; resume follows the existing submission.
Correct a failed attempt before explicit `--retry`. Inspect errors instead of
repeatedly creating new jobs or treating the old running release as success.

## Acceptance and production

After browser/API testing, use `app accept --note 'actual summary' --report FILE`.
The report identifies the exact dev release and actual scopes, test actors,
passed/not_run results and evidence. See `docs/ACCEPTANCE.md`. Failed results and
stale release reports are rejected. Legacy note-only acceptance remains available
and is marked unstructured; acceptance is a user's attestation plus runtime health.

For requested production deployment, use `app promote --release UUID`. Only an
administrator or a user with prod/deploy permission can promote. Promotion uses
that accepted image with a separate prod installation/database, without copying
dev data or rebuilding. Use explicit environment URLs from `app status`.

## Manage and troubleshoot

`app control start|stop|restart|remove --environment dev|prod` manages only this
app/environment. Removal preserves its database. `app rename --name 'Display name'`
is administrator-only platform metadata; it changes no source or manifest name.
`app logout` revokes the CLI credential.

Use `app doctor`, command `--help`, `app describe`, `app status` and `app resume`
for diagnosis. Keep failed/unverified checks visible in the completion report.
Do not bypass bundle or component integrity failures by editing managed files.

## Update CLI, skills and manuals

Installing a global CLI does not silently change every project's pinned tools.
With a compatible SDK bundle, run the new global `miy app setup` in the project.
It synchronizes `.miy-tools/miy`, Codex/Claude `miy-cli` skills, their references,
`docs/CLI-REFERENCE.md` and `miy.cli.json`. Local business edits are preserved.
`app check` detects stale or edited managed guidance. Commit changed guidance and
its lock; keep credentials and the ignored executable out of Git.

For an SDK bundle change, commit clean source and use `miy app upgrade`. For old
0.5 projects, first update ui to 1.0.1 and other components to releases compatible
with bundle 0.6 using the old project CLI, then commit. Review the upgrade, run
checks and redeploy only when requested. The upgrade also synchronizes CLI skills
and manuals. Neither global installation nor guidance sync deploys the app.

The foundation bundle's `docs/CLI.md` is an older quick reference. Use this skill
and the generated `docs/CLI-REFERENCE.md` for the installed CLI's complete usage.
SDK contracts remain in `docs/CONTRACT.md`; tooling updates do not rewrite them.
