## Update the CLI and project MCP

This guide is available without login or network access through
`miy describe --topic updates` and local MCP `get_cli_help({"topic":"updates"})`.
Reading it does not download, install or authenticate anything.

1. **Choose the existing server.** `miy context show` displays its public origin.
   Open that origin's `/cli/install` page and use its published install command.
   If configuration cannot be read, obtain the portal origin from the operator.
   There is no built-in domain: domain names and IP addresses with ports are both
   supported. Use the portal origin, not the console or app-development portal.
2. **Update the global CLI from a normal terminal.** Stop local preview and close
   Codex/Claude Code sessions using the CLI before replacing executables;
   Windows refuses replacement while an executable is in use. Run the same
   installer again as the same OS user. Existing contexts and credentials are
   preserved. Keep any custom installation prefix; do not uninstall or delete
   credential storage to update.

Linux/macOS (replace `https://PLATFORM` with the selected origin):

```sh
curl -fsSL 'https://PLATFORM/api/v1/cli/install.sh' | bash
```

Windows PowerShell:

```powershell
& ([scriptblock]::Create((Invoke-RestMethod -Uri 'https://PLATFORM/api/v1/cli/install.ps1')))
```

Use the PC's configured CA trust for private HTTPS; do not disable TLS checks.
An offline installation uses the verified package's included installer instead.
Open a new terminal if PATH changed, or use the installer's printed executable
path. Check `miy --version` against the version on the install page. If it is
still old, inspect `command -v miy` (Unix) or `Get-Command miy` (PowerShell) and
select the newly installed global executable.

3. **Update each existing project.** Inside its application directory, run the
   newly installed global executable:

```sh
miy app setup
```

This copies that executable into `.miy-tools`, refreshes CLI manuals/skills,
`docs/API.md` and Codex/Claude MCP settings, and preserves authentication.
Running an old project's `.miy-tools/miy app setup` copies the
old executable again. With the same SDK bundle, `app setup` is sufficient. If
the installed SDK bundle differs, review and commit clean project source before
an explicit `miy app upgrade`; do not rewrite protected files manually.

Use `miy app check --harness-only --output json` to read the installed and
available guidance versions. The lock `miy.cli.json` records `guidanceVersion`
and, for projects created by an initializer that records it, `templateVersion`.
The latter identifies the original scaffold by its SHA-256. Setup and bundle
upgrades preserve this value. Earlier projects have an unknown scaffold
revision and receive the current guidance on setup.

4. **Restart the agent and verify the project executable.** A running stdio MCP
   process does not switch to the new executable or guidance automatically.
   Reopen Codex/Claude Code and accept its normal project trust/MCP prompts.

```sh
./.miy-tools/miy --version
./.miy-tools/miy auth status --output json
```

On Windows use `.\.miy-tools\miy.exe` for those commands. Reauthenticate only
when the reported error requires it (`login_required` or
`reauthentication_required`), not merely because the CLI was updated. A local
credential-storage failure is not repaired by repeating browser approval. Read
the safe diagnostic details and submit a report without login if it persists:

```sh
./.miy-tools/miy report send --message "Credential storage error after updating; describe the execution environment"
```

The project stdio MCP uses this local CLI. A direct HTTP MCP connection to the
portal's `/mcp` runs on the server and has its own authentication; updating a
local CLI neither replaces that server nor repairs its OAuth session. Reconnect
or authenticate the HTTP MCP client only when its server response requires it.
