# MIY CLI command reference

Generated from MIY CLI 0.17.0, SDK bundle 0.12.0. Do not edit this generated file.

## miy

```text
Usage: miy [OPTIONS] <COMMAND>

Commands:
  context       Manage named server contexts and the active server
  auth          Sign in and manage user sessions for the selected context
  doctor        Diagnose the requirements for remote operation or local development
  report        Report a problem without authentication, or inspect the private local outbox
  describe      Discover command inputs, outputs, permissions and effects
  completion    Generate completion for a supported shell
  component     Discover and synchronize reusable components
  job           Inspect and recover remote jobs
  registration  
  user          
  grant         
  permissions   Inspect and explicitly approve installation/release external access
  mcp           Serve the project read-only JSON-RPC MCP interface over stdio
  app           
  help          Print this message or the help of the given subcommand(s)

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help

  -V, --version
          Print version

CLI updates: miy describe --topic updates
Re-run your server's installer, run the new global miy app setup in each project, then restart Codex/Claude Code.
```

## miy context

```text
Manage named server contexts and the active server

Usage: miy context [OPTIONS] <COMMAND>

Commands:
  init    Register an explicitly supplied server, or ask for its URL on a terminal
  add     Add a named server origin without storing credentials in the project
  list    
  show    
  use     
  remove  Remove a local context binding; remote sessions remain until explicitly revoked
  help    Print this message or the help of the given subcommand(s)

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy context init

```text
Register an explicitly supplied server, or ask for its URL on a terminal

Usage: miy context init [OPTIONS]

Options:
      --context <CONTEXT>
          

      --url <URL>
          

      --app <APP>
          

      --ca-file <CA_FILE>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy context add

```text
Add a named server origin without storing credentials in the project

Usage: miy context add [OPTIONS] --url <URL> <NAME>

Arguments:
  <NAME>
          

Options:
      --context <CONTEXT>
          

      --url <URL>
          

      --app <APP>
          

      --ca-file <CA_FILE>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy context list

```text
Usage: miy context list [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy context show

```text
Usage: miy context show [OPTIONS] [NAME]

Arguments:
  [NAME]
          

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy context use

```text
Usage: miy context use [OPTIONS] <NAME>

Arguments:
  <NAME>
          

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy context remove

```text
Remove a local context binding; remote sessions remain until explicitly revoked

Usage: miy context remove [OPTIONS] <NAME>

Arguments:
  <NAME>
          

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy context help

```text
Print this message or the help of the given subcommand(s)

Usage: miy context help [COMMAND]

Commands:
  init    Register an explicitly supplied server, or ask for its URL on a terminal
  add     Add a named server origin without storing credentials in the project
  list    
  show    
  use     
  remove  Remove a local context binding; remote sessions remain until explicitly revoked
  help    Print this message or the help of the given subcommand(s)
```

## miy context help init

```text
Register an explicitly supplied server, or ask for its URL on a terminal

Usage: miy context help init
```

## miy context help add

```text
Add a named server origin without storing credentials in the project

Usage: miy context help add
```

## miy context help list

```text
Usage: miy context help list
```

## miy context help show

```text
Usage: miy context help show
```

## miy context help use

```text
Usage: miy context help use
```

## miy context help remove

```text
Remove a local context binding; remote sessions remain until explicitly revoked

Usage: miy context help remove
```

## miy context help help

```text
Print this message or the help of the given subcommand(s)

Usage: miy context help help
```

## miy auth

```text
Sign in and manage user sessions for the selected context

Usage: miy auth [OPTIONS] <COMMAND>

Commands:
  login        Start device approval; noninteractive mode returns public instructions for auth wait
  wait         Wait for a saved device login without printing its private device code
  status       Verify the saved identity and show public session metadata
  logout       Revoke the server session, then remove local credentials
  sessions     List or explicitly revoke this user's CLI sessions
  delegations  Issue, inspect, revoke or import an explicitly scoped user delegation
  help         Print this message or the help of the given subcommand(s)

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy auth login

```text
Start device approval; noninteractive mode returns public instructions for auth wait

Usage: miy auth login [OPTIONS]

Options:
      --context <CONTEXT>
          

      --device-code
          Display the device code without automatically opening a browser

      --app <APP>
          

      --no-wait
          Return the pending login immediately

      --no-open
          Do not open a browser

      --project <PROJECT>
          

      --account <ACCOUNT>
          Explicit HTTP loopback development shortcut; approves and waits even without a TTY

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy auth wait

```text
Wait for a saved device login without printing its private device code

Usage: miy auth wait [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy auth status

```text
Verify the saved identity and show public session metadata

Usage: miy auth status [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy auth logout

```text
Revoke the server session, then remove local credentials

Usage: miy auth logout [OPTIONS]

Options:
      --context <CONTEXT>
          

      --local-only
          Delete local credentials without attempting server revocation

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy auth sessions

```text
List or explicitly revoke this user's CLI sessions

Usage: miy auth sessions [OPTIONS] <COMMAND>

Commands:
  list    
  revoke  
  help    Print this message or the help of the given subcommand(s)

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy auth sessions list

```text
Usage: miy auth sessions list [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy auth sessions revoke

```text
Usage: miy auth sessions revoke [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy auth sessions help

```text
Print this message or the help of the given subcommand(s)

Usage: miy auth sessions help [COMMAND]

Commands:
  list    
  revoke  
  help    Print this message or the help of the given subcommand(s)
```

## miy auth sessions help list

```text
Usage: miy auth sessions help list
```

## miy auth sessions help revoke

```text
Usage: miy auth sessions help revoke
```

## miy auth sessions help help

```text
Print this message or the help of the given subcommand(s)

Usage: miy auth sessions help help
```

## miy auth delegations

```text
Issue, inspect, revoke or import an explicitly scoped user delegation

Usage: miy auth delegations [OPTIONS] <COMMAND>

Commands:
  create  Create a fixed, revocable app/environment grant; secrets go only to a private file
  list    
  revoke  
  import  Import into an isolated CLI config with a matching server context and no existing credential
  help    Print this message or the help of the given subcommand(s)

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy auth delegations create

```text
Create a fixed, revocable app/environment grant; secrets go only to a private file

Usage: miy auth delegations create [OPTIONS] --app <APP> --environment <ENVIRONMENT> --action <ACTIONS> --name <NAME> --expires-in <EXPIRES_IN> --credential-file <CREDENTIAL_FILE>

Options:
      --app <APP>
          

      --context <CONTEXT>
          

      --environment <ENVIRONMENT>
          [possible values: dev, prod]

      --action <ACTIONS>
          [possible values: read, app_read, app_write, directory_read, build, dev_session, prepare_database, accept, deploy, promote, start, stop, restart, remove, logs]

      --project <PROJECT>
          

      --name <NAME>
          

      --no-input
          

      --expires-in <EXPIRES_IN>
          

      --yes
          

      --credential-file <CREDENTIAL_FILE>
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy auth delegations list

```text
Usage: miy auth delegations list [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy auth delegations revoke

```text
Usage: miy auth delegations revoke [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy auth delegations import

```text
Import into an isolated CLI config with a matching server context and no existing credential

Usage: miy auth delegations import [OPTIONS] --credential-file <CREDENTIAL_FILE>

Options:
      --context <CONTEXT>
          

      --credential-file <CREDENTIAL_FILE>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy auth delegations help

```text
Print this message or the help of the given subcommand(s)

Usage: miy auth delegations help [COMMAND]

Commands:
  create  Create a fixed, revocable app/environment grant; secrets go only to a private file
  list    
  revoke  
  import  Import into an isolated CLI config with a matching server context and no existing credential
  help    Print this message or the help of the given subcommand(s)
```

## miy auth delegations help create

```text
Create a fixed, revocable app/environment grant; secrets go only to a private file

Usage: miy auth delegations help create
```

## miy auth delegations help list

```text
Usage: miy auth delegations help list
```

## miy auth delegations help revoke

```text
Usage: miy auth delegations help revoke
```

## miy auth delegations help import

```text
Import into an isolated CLI config with a matching server context and no existing credential

Usage: miy auth delegations help import
```

## miy auth delegations help help

```text
Print this message or the help of the given subcommand(s)

Usage: miy auth delegations help help
```

## miy auth help

```text
Print this message or the help of the given subcommand(s)

Usage: miy auth help [COMMAND]

Commands:
  login        Start device approval; noninteractive mode returns public instructions for auth wait
  wait         Wait for a saved device login without printing its private device code
  status       Verify the saved identity and show public session metadata
  logout       Revoke the server session, then remove local credentials
  sessions     List or explicitly revoke this user's CLI sessions
  delegations  Issue, inspect, revoke or import an explicitly scoped user delegation
  help         Print this message or the help of the given subcommand(s)
```

## miy auth help login

```text
Start device approval; noninteractive mode returns public instructions for auth wait

Usage: miy auth help login
```

## miy auth help wait

```text
Wait for a saved device login without printing its private device code

Usage: miy auth help wait
```

## miy auth help status

```text
Verify the saved identity and show public session metadata

Usage: miy auth help status
```

## miy auth help logout

```text
Revoke the server session, then remove local credentials

Usage: miy auth help logout
```

## miy auth help sessions

```text
List or explicitly revoke this user's CLI sessions

Usage: miy auth help sessions [COMMAND]

Commands:
  list    
  revoke
```

## miy auth help sessions list

```text
Usage: miy auth help sessions list
```

## miy auth help sessions revoke

```text
Usage: miy auth help sessions revoke
```

## miy auth help delegations

```text
Issue, inspect, revoke or import an explicitly scoped user delegation

Usage: miy auth help delegations [COMMAND]

Commands:
  create  Create a fixed, revocable app/environment grant; secrets go only to a private file
  list    
  revoke  
  import  Import into an isolated CLI config with a matching server context and no existing credential
```

## miy auth help delegations create

```text
Create a fixed, revocable app/environment grant; secrets go only to a private file

Usage: miy auth help delegations create
```

## miy auth help delegations list

```text
Usage: miy auth help delegations list
```

## miy auth help delegations revoke

```text
Usage: miy auth help delegations revoke
```

## miy auth help delegations import

```text
Import into an isolated CLI config with a matching server context and no existing credential

Usage: miy auth help delegations import
```

## miy auth help help

```text
Print this message or the help of the given subcommand(s)

Usage: miy auth help help
```

## miy doctor

```text
Diagnose the requirements for remote operation or local development

Usage: miy doctor [OPTIONS]

Options:
      --context <CONTEXT>
          

      --for <FOR_MODE>
          Diagnose operation access, or the full local application development environment
          
          [default: operate]
          [possible values: develop, operate]

      --app <APP>
          

      --runtime <RUNTIME>
          Native development needs Rust/Node; Docker is an explicit alternative
          
          [default: native]
          [possible values: native, docker]

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy report

```text
Report a problem without authentication, or inspect the private local outbox

Usage: miy report [OPTIONS] <COMMAND>

Commands:
  send   Send a manual report without login. Does not read the credential store
  retry  Resend one saved report to its original server, without authentication
  list   List locally saved reports and submission state
  show   Inspect a locally saved redacted report
  help   Print this message or the help of the given subcommand(s)

Options:
      --context <CONTEXT>
          

      --outbox <OUTBOX>
          Separate private outbox if the normal configuration directory is broken

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy report send

```text
Send a manual report without login. Does not read the credential store

Usage: miy report send [OPTIONS] --message <MESSAGE>

Options:
      --context <CONTEXT>
          

      --message <MESSAGE>
          Describe the problem without passwords, tokens, personal data or logs

      --outbox <OUTBOX>
          Separate private outbox if the normal configuration directory is broken

      --app <APP>
          

      --server <SERVER>
          Explicit origin, also usable when the context configuration is broken

      --ca-file <CA_FILE>
          

      --project <PROJECT>
          

      --no-input
          

      --save-only
          Save a private report for later submission without contacting the server

      --dry-run
          Preview the redacted payload without saving or sending it

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy report retry

```text
Resend one saved report to its original server, without authentication

Usage: miy report retry [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --context <CONTEXT>
          

      --outbox <OUTBOX>
          Separate private outbox if the normal configuration directory is broken

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy report list

```text
List locally saved reports and submission state

Usage: miy report list [OPTIONS]

Options:
      --context <CONTEXT>
          

      --outbox <OUTBOX>
          Separate private outbox if the normal configuration directory is broken

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy report show

```text
Inspect a locally saved redacted report

Usage: miy report show [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --context <CONTEXT>
          

      --outbox <OUTBOX>
          Separate private outbox if the normal configuration directory is broken

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy report help

```text
Print this message or the help of the given subcommand(s)

Usage: miy report help [COMMAND]

Commands:
  send   Send a manual report without login. Does not read the credential store
  retry  Resend one saved report to its original server, without authentication
  list   List locally saved reports and submission state
  show   Inspect a locally saved redacted report
  help   Print this message or the help of the given subcommand(s)
```

## miy report help send

```text
Send a manual report without login. Does not read the credential store

Usage: miy report help send
```

## miy report help retry

```text
Resend one saved report to its original server, without authentication

Usage: miy report help retry
```

## miy report help list

```text
List locally saved reports and submission state

Usage: miy report help list
```

## miy report help show

```text
Inspect a locally saved redacted report

Usage: miy report help show
```

## miy report help help

```text
Print this message or the help of the given subcommand(s)

Usage: miy report help help
```

## miy describe

```text
Discover command inputs, outputs, permissions and effects

Usage: miy describe [OPTIONS]

Options:
      --context <CONTEXT>
          

      --topic <TOPIC>
          Read an offline workflow guide without authentication

          Possible values:
          - updates: Update the global CLI, each project's executable and agent MCP sessions
          - harness: Review component reuse, file recovery and the app integration checks
          - api:     Read the app OpenAPI rules and generation commands

      --app <APP>
          

      --command <COMMAND>
          Inspect one command, for example --command "app deploy"

      --project <PROJECT>
          

      --search <SEARCH>
          Search command names and descriptions without loading the full reference

      --all
          Return the complete command reference explicitly

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help (see a summary with '-h')
```

## miy completion

```text
Generate completion for a supported shell

Usage: miy completion [OPTIONS] <SHELL>

Arguments:
  <SHELL>
          [possible values: bash, elvish, fish, powershell, zsh]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy component

```text
Discover and synchronize reusable components

Usage: miy component [OPTIONS] <COMMAND>

Commands:
  list     Read live component releases from the configured platform
  show     Read usage, source and compatibility of a release (latest compatible by default)
  add      Install a new component; does not execute package hooks or change business code
  update   Explicitly update an installed component (latest compatible by default)
  sync     Fetch template defaults initially; otherwise restore exact locked releases and missing files
  check    Verify installed files and compatibility without contacting the platform
  publish  Publish a component source directory; requires platform administrator authority
  help     Print this message or the help of the given subcommand(s)

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy component list

```text
Read live component releases from the configured platform

Usage: miy component list [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy component show

```text
Read usage, source and compatibility of a release (latest compatible by default)

Usage: miy component show [OPTIONS] <NAME>

Arguments:
  <NAME>
          

Options:
      --context <CONTEXT>
          

      --version <VERSION>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy component add

```text
Install a new component; does not execute package hooks or change business code

Usage: miy component add [OPTIONS] <NAME>

Arguments:
  <NAME>
          

Options:
      --context <CONTEXT>
          

      --version <VERSION>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy component update

```text
Explicitly update an installed component (latest compatible by default)

Usage: miy component update [OPTIONS] <NAME>

Arguments:
  <NAME>
          

Options:
      --context <CONTEXT>
          

      --version <VERSION>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy component sync

```text
Fetch template defaults initially; otherwise restore exact locked releases and missing files

Usage: miy component sync [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy component check

```text
Verify installed files and compatibility without contacting the platform

Usage: miy component check [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy component publish

```text
Publish a component source directory; requires platform administrator authority

Usage: miy component publish [OPTIONS] <PATH>

Arguments:
  <PATH>
          

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy component help

```text
Print this message or the help of the given subcommand(s)

Usage: miy component help [COMMAND]

Commands:
  list     Read live component releases from the configured platform
  show     Read usage, source and compatibility of a release (latest compatible by default)
  add      Install a new component; does not execute package hooks or change business code
  update   Explicitly update an installed component (latest compatible by default)
  sync     Fetch template defaults initially; otherwise restore exact locked releases and missing files
  check    Verify installed files and compatibility without contacting the platform
  publish  Publish a component source directory; requires platform administrator authority
  help     Print this message or the help of the given subcommand(s)
```

## miy component help list

```text
Read live component releases from the configured platform

Usage: miy component help list
```

## miy component help show

```text
Read usage, source and compatibility of a release (latest compatible by default)

Usage: miy component help show
```

## miy component help add

```text
Install a new component; does not execute package hooks or change business code

Usage: miy component help add
```

## miy component help update

```text
Explicitly update an installed component (latest compatible by default)

Usage: miy component help update
```

## miy component help sync

```text
Fetch template defaults initially; otherwise restore exact locked releases and missing files

Usage: miy component help sync
```

## miy component help check

```text
Verify installed files and compatibility without contacting the platform

Usage: miy component help check
```

## miy component help publish

```text
Publish a component source directory; requires platform administrator authority

Usage: miy component help publish
```

## miy component help help

```text
Print this message or the help of the given subcommand(s)

Usage: miy component help help
```

## miy job

```text
Inspect and recover remote jobs

Usage: miy job [OPTIONS] <COMMAND>

Commands:
  list    
  get     
  watch   Observe events after the saved/explicit cursor; stopping observation does not cancel work
  logs    Read bounded server-build log entries after a sequence
  resume  Recover the original request or reconnect to an acknowledged runtime or build job
  retry   Retry a typed transient failure after fresh preflight and target confirmation
  help    Print this message or the help of the given subcommand(s)

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy job list

```text
Usage: miy job list [OPTIONS]

Options:
      --context <CONTEXT>
          

      --environment <ENVIRONMENT>
          [possible values: dev, prod]

      --app <APP>
          

      --request-id <REQUEST_ID>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy job get

```text
Usage: miy job get [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy job watch

```text
Observe events after the saved/explicit cursor; stopping observation does not cancel work

Usage: miy job watch [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --after <AFTER>
          

      --context <CONTEXT>
          

      --app <APP>
          

      --timeout <TIMEOUT>
          [default: 600]

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy job logs

```text
Read bounded server-build log entries after a sequence

Usage: miy job logs [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --after <AFTER>
          [default: 0]

      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy job resume

```text
Recover the original request or reconnect to an acknowledged runtime or build job

Usage: miy job resume [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --context <CONTEXT>
          

      --no-wait
          

      --app <APP>
          

      --environment <ENVIRONMENT>
          [possible values: dev, prod]

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy job retry

```text
Retry a typed transient failure after fresh preflight and target confirmation

Usage: miy job retry [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --context <CONTEXT>
          

      --environment <ENVIRONMENT>
          [possible values: dev, prod]

      --app <APP>
          

      --no-wait
          

      --dry-run
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy job help

```text
Print this message or the help of the given subcommand(s)

Usage: miy job help [COMMAND]

Commands:
  list    
  get     
  watch   Observe events after the saved/explicit cursor; stopping observation does not cancel work
  logs    Read bounded server-build log entries after a sequence
  resume  Recover the original request or reconnect to an acknowledged runtime or build job
  retry   Retry a typed transient failure after fresh preflight and target confirmation
  help    Print this message or the help of the given subcommand(s)
```

## miy job help list

```text
Usage: miy job help list
```

## miy job help get

```text
Usage: miy job help get
```

## miy job help watch

```text
Observe events after the saved/explicit cursor; stopping observation does not cancel work

Usage: miy job help watch
```

## miy job help logs

```text
Read bounded server-build log entries after a sequence

Usage: miy job help logs
```

## miy job help resume

```text
Recover the original request or reconnect to an acknowledged runtime or build job

Usage: miy job help resume
```

## miy job help retry

```text
Retry a typed transient failure after fresh preflight and target confirmation

Usage: miy job help retry
```

## miy job help help

```text
Print this message or the help of the given subcommand(s)

Usage: miy job help help
```

## miy registration

```text
Usage: miy registration [OPTIONS] <COMMAND>

Commands:
  list     
  approve  
  reject   
  help     Print this message or the help of the given subcommand(s)

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy registration list

```text
Usage: miy registration list [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy registration approve

```text
Usage: miy registration approve [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy registration reject

```text
Usage: miy registration reject [OPTIONS] <ID>

Arguments:
  <ID>
          

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy registration help

```text
Print this message or the help of the given subcommand(s)

Usage: miy registration help [COMMAND]

Commands:
  list     
  approve  
  reject   
  help     Print this message or the help of the given subcommand(s)
```

## miy registration help list

```text
Usage: miy registration help list
```

## miy registration help approve

```text
Usage: miy registration help approve
```

## miy registration help reject

```text
Usage: miy registration help reject
```

## miy registration help help

```text
Print this message or the help of the given subcommand(s)

Usage: miy registration help help
```

## miy user

```text
Usage: miy user [OPTIONS] <COMMAND>

Commands:
  list  
  help  Print this message or the help of the given subcommand(s)

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy user list

```text
Usage: miy user list [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy user help

```text
Print this message or the help of the given subcommand(s)

Usage: miy user help [COMMAND]

Commands:
  list  
  help  Print this message or the help of the given subcommand(s)
```

## miy user help list

```text
Usage: miy user help list
```

## miy user help help

```text
Print this message or the help of the given subcommand(s)

Usage: miy user help help
```

## miy grant

```text
Usage: miy grant [OPTIONS] <COMMAND>

Commands:
  list    
  set     
  revoke  
  help    Print this message or the help of the given subcommand(s)

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy grant list

```text
Usage: miy grant list [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy grant set

```text
Usage: miy grant set [OPTIONS]

Options:
      --context <CONTEXT>
          

      --user <USER>
          

      --app <APP>
          

      --permission <PERMISSION>
          [possible values: use, develop, deploy]

      --environment <ENVIRONMENT>
          [possible values: dev, prod]

      --project <PROJECT>
          

      --input <INPUT>
          Strict JSON {userId,appId?,environment,permission} from @file or stdin (-)

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy grant revoke

```text
Usage: miy grant revoke [OPTIONS]

Options:
      --context <CONTEXT>
          

      --user <USER>
          

      --app <APP>
          

      --permission <PERMISSION>
          [possible values: use, develop, deploy]

      --environment <ENVIRONMENT>
          [possible values: dev, prod]

      --project <PROJECT>
          

      --input <INPUT>
          Strict JSON {userId,appId?,environment,permission} from @file or stdin (-)

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy grant help

```text
Print this message or the help of the given subcommand(s)

Usage: miy grant help [COMMAND]

Commands:
  list    
  set     
  revoke  
  help    Print this message or the help of the given subcommand(s)
```

## miy grant help list

```text
Usage: miy grant help list
```

## miy grant help set

```text
Usage: miy grant help set
```

## miy grant help revoke

```text
Usage: miy grant help revoke
```

## miy grant help help

```text
Print this message or the help of the given subcommand(s)

Usage: miy grant help help
```

## miy permissions

```text
Inspect and explicitly approve installation/release external access

Usage: miy permissions [OPTIONS] <COMMAND>

Commands:
  get   Read requested/approved external permissions and the revision before changing them
  set   Replace approvals for exactly one release, using a reviewed JSON input and revision
  help  Print this message or the help of the given subcommand(s)

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy permissions get

```text
Read requested/approved external permissions and the revision before changing them

Usage: miy permissions get [OPTIONS] --installation <INSTALLATION>

Options:
      --context <CONTEXT>
          

      --installation <INSTALLATION>
          

      --app <APP>
          

      --release <RELEASE>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy permissions set

```text
Replace approvals for exactly one release, using a reviewed JSON input and revision

Usage: miy permissions set [OPTIONS] --installation <INSTALLATION> --input <INPUT>

Options:
      --context <CONTEXT>
          

      --installation <INSTALLATION>
          

      --app <APP>
          

      --input <INPUT>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy permissions help

```text
Print this message or the help of the given subcommand(s)

Usage: miy permissions help [COMMAND]

Commands:
  get   Read requested/approved external permissions and the revision before changing them
  set   Replace approvals for exactly one release, using a reviewed JSON input and revision
  help  Print this message or the help of the given subcommand(s)
```

## miy permissions help get

```text
Read requested/approved external permissions and the revision before changing them

Usage: miy permissions help get
```

## miy permissions help set

```text
Replace approvals for exactly one release, using a reviewed JSON input and revision

Usage: miy permissions help set
```

## miy permissions help help

```text
Print this message or the help of the given subcommand(s)

Usage: miy permissions help help
```

## miy mcp

```text
Serve the project read-only JSON-RPC MCP interface over stdio

Usage: miy mcp [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app

```text
Usage: miy app [OPTIONS] <COMMAND>

Commands:
  list        List apps visible to the authenticated user
  show        Show the selected app and visible installations
  status      Show server deployment state for --app, or the current project app
  deploy      Deploy this project's server build or an existing official release to dev
  promote     Promote the exact accepted release to the explicitly selected prod target
  start       
  stop        
  restart     
  remove      Remove runtime containers after confirmation, preserving the application database
  logs        Retrieve a bounded container log snapshot; rotation may omit older lines
  init        Create an independent Rust + React application
  setup       Transactionally update the compatible project CLI, guidance and MCP settings
  upgrade     Upgrade a supported SDK bundle while preserving business code and applied SQL
  check       Check app integration, managed files, components and both language toolchains
  fmt         
  dev         Run local development against the server's development database
  dev-stop    Stop this project's owned development processes; preserve its database volume
  source-pin  Print this committed app's ID, version, source commit and server archive SHA-256 offline
  source      Download or restore the exact source of an existing app release
  build       Build clean committed source on the selected server. The server runs the format check, tests and integrity checks; only its builds can be deployed. For local testing use `app dev`
  register    Request administrator approval for this project's application
  rename      Change only the platform display name (administrator)
  accept      Record evidence for a dev release before promotion
  help        Print this message or the help of the given subcommand(s)

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app list

```text
List apps visible to the authenticated user

Usage: miy app list [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app show

```text
Show the selected app and visible installations

Usage: miy app show [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app status

```text
Show server deployment state for --app, or the current project app

Usage: miy app status [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app deploy

```text
Deploy this project's server build or an existing official release to dev

Usage: miy app deploy [OPTIONS]

Options:
      --context <CONTEXT>
          

      --release <RELEASE>
          Reuse the exact official release on this server, including after a change of developer

      --app <APP>
          

      --environment <ENVIRONMENT>
          [possible values: dev]

      --no-wait
          

      --project <PROJECT>
          

      --dry-run
          

      --no-input
          

      --if-current <IF_CURRENT>
          Submit only if the target still matches the dry-run data read from @file or stdin (-)

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app promote

```text
Promote the exact accepted release to the explicitly selected prod target

Usage: miy app promote [OPTIONS] --release <RELEASE>

Options:
      --context <CONTEXT>
          

      --release <RELEASE>
          

      --app <APP>
          

      --environment <ENVIRONMENT>
          [possible values: prod]

      --no-wait
          

      --project <PROJECT>
          

      --dry-run
          

      --no-input
          

      --if-current <IF_CURRENT>
          Submit only if the target still matches the dry-run data read from @file or stdin (-)

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app start

```text
Usage: miy app start [OPTIONS]

Options:
      --context <CONTEXT>
          

      --environment <ENVIRONMENT>
          Defaults to dev; prod and app removal require an explicitly supplied target
          
          [possible values: dev, prod]

      --app <APP>
          

      --no-wait
          

      --dry-run
          Inspect admission and expected state without uploading or submitting work

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app stop

```text
Usage: miy app stop [OPTIONS]

Options:
      --context <CONTEXT>
          

      --environment <ENVIRONMENT>
          Defaults to dev; prod and app removal require an explicitly supplied target
          
          [possible values: dev, prod]

      --app <APP>
          

      --no-wait
          

      --dry-run
          Inspect admission and expected state without uploading or submitting work

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app restart

```text
Usage: miy app restart [OPTIONS]

Options:
      --context <CONTEXT>
          

      --environment <ENVIRONMENT>
          Defaults to dev; prod and app removal require an explicitly supplied target
          
          [possible values: dev, prod]

      --app <APP>
          

      --no-wait
          

      --dry-run
          Inspect admission and expected state without uploading or submitting work

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app remove

```text
Remove runtime containers after confirmation, preserving the application database

Usage: miy app remove [OPTIONS]

Options:
      --context <CONTEXT>
          

      --environment <ENVIRONMENT>
          Defaults to dev; prod and app removal require an explicitly supplied target
          
          [possible values: dev, prod]

      --app <APP>
          

      --no-wait
          

      --dry-run
          Inspect admission and expected state without uploading or submitting work

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app logs

```text
Retrieve a bounded container log snapshot; rotation may omit older lines

Usage: miy app logs [OPTIONS]

Options:
      --context <CONTEXT>
          

      --request <REQUEST>
          Resume an existing bounded log request without creating another one

      --app <APP>
          

      --environment <ENVIRONMENT>
          [default: dev]
          [possible values: dev, prod]

      --project <PROJECT>
          

      --tail <TAIL>
          [default: 100]

      --no-input
          

      --since <SINCE>
          

      --timeout <TIMEOUT>
          [default: 60]

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app init

```text
Create an independent Rust + React application

Usage: miy app init [OPTIONS] --id <ID> --name <NAME> <PATH>

Arguments:
  <PATH>
          

Options:
      --context <CONTEXT>
          

      --id <ID>
          

      --app <APP>
          

      --name <NAME>
          

      --project <PROJECT>
          

      --url <URL>
          

      --ca-file <CA_FILE>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app setup

```text
Copy this executable and its CLI guidance into the current compatible project, and configure Codex/Claude MCP. First update the global CLI using your server's installer, then run the new global miy app setup here. This command does not download a newer CLI or reset authentication. Restart the agent afterward. See miy describe --topic updates; a different SDK bundle requires an explicit app upgrade.

Usage: miy app setup [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help (see a summary with '-h')
```

## miy app upgrade

```text
Upgrade a supported SDK bundle while preserving business code and applied SQL

Usage: miy app upgrade [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app check

```text
Check app integration, managed files, components and both language toolchains

Usage: miy app check [OPTIONS]

Options:
      --context <CONTEXT>
          

      --harness-only
          Review app source integration offline without compiling or running the app

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app fmt

```text
Usage: miy app fmt [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app dev

```text
Run local development against the server's development database

Usage: miy app dev [OPTIONS]

Options:
      --context <CONTEXT>
          

      --port <PORT>
          [default: 18100]

      --app <APP>
          

      --no-open
          

      --database <DATABASE>
          Use the server development DB, or explicitly select an isolated local DB
          
          [default: server]
          [possible values: server, local]

      --project <PROJECT>
          

      --no-input
          

      --runtime <RUNTIME>
          Run Rust directly, or explicitly use a Linux Docker engine
          
          [default: native]
          [possible values: native, docker]

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app dev-stop

```text
Stop this project's owned development processes; preserve its database volume

Usage: miy app dev-stop [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app source-pin

```text
Print this committed app's ID, version, source commit and server archive SHA-256 offline

Usage: miy app source-pin [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app source

```text
Download or restore the exact source of an existing app release

Usage: miy app source [OPTIONS] <COMMAND>

Commands:
  download  Save the verified build archive of a release to a new local file
  upload    Restore a release from its original committed project or exact build tar. Use the global --project to select a different local project directory
  help      Print this message or the help of the given subcommand(s)

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app source download

```text
Save the verified build archive of a release to a new local file

Usage: miy app source download [OPTIONS] --release <RELEASE> --out <OUT>

Options:
      --context <CONTEXT>
          

      --release <RELEASE>
          

      --app <APP>
          

      --out <OUT>
          Path to a new tar file. The parent directory must exist

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app source upload

```text
Restore a release from its original committed project or exact build tar. Use the global --project to select a different local project directory

Usage: miy app source upload [OPTIONS] --release <RELEASE>

Options:
      --context <CONTEXT>
          

      --release <RELEASE>
          

      --app <APP>
          

      --archive <ARCHIVE>
          Use an existing build tar. Select the app explicitly with --app

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app source help

```text
Print this message or the help of the given subcommand(s)

Usage: miy app source help [COMMAND]

Commands:
  download  Save the verified build archive of a release to a new local file
  upload    Restore a release from its original committed project or exact build tar. Use the global --project to select a different local project directory
  help      Print this message or the help of the given subcommand(s)
```

## miy app source help download

```text
Save the verified build archive of a release to a new local file

Usage: miy app source help download
```

## miy app source help upload

```text
Restore a release from its original committed project or exact build tar. Use the global --project to select a different local project directory

Usage: miy app source help upload
```

## miy app source help help

```text
Print this message or the help of the given subcommand(s)

Usage: miy app source help help
```

## miy app build

```text
Build clean committed source on the selected server. The server runs the format check, tests and integrity checks; only its builds can be deployed. For local testing use `app dev`

Usage: miy app build [OPTIONS]

Options:
      --context <CONTEXT>
          

      --no-wait
          

      --app <APP>
          

      --timeout <TIMEOUT>
          [default: 1800]

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app register

```text
Request administrator approval for this project's application

Usage: miy app register [OPTIONS]

Options:
      --context <CONTEXT>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app rename

```text
Change only the platform display name (administrator)

Usage: miy app rename [OPTIONS] --name <NAME>

Options:
      --context <CONTEXT>
          

      --name <NAME>
          

      --app <APP>
          

      --project <PROJECT>
          

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app accept

```text
Record evidence for a dev release before promotion

Usage: miy app accept [OPTIONS]

Options:
      --context <CONTEXT>
          

      --note <NOTE>
          

      --app <APP>
          

      --release <RELEASE>
          

      --project <PROJECT>
          

      --report <REPORT>
          Strict JSON {releaseId,checks:[{scope,actor,result,evidence}]}

      --input <INPUT>
          Strict JSON {releaseId,note,checks} from @file or stdin (-)

      --no-input
          

      --yes
          

      --fields <FIELDS>
          

      --limit <LIMIT>
          [default: 50]

      --cursor <CURSOR>
          

      --output <OUTPUT>
          Output for application commands: auto uses text on a terminal and JSON otherwise
          
          [default: auto]
          [possible values: auto, text, json, jsonl]

  -h, --help
          Print help
```

## miy app help

```text
Print this message or the help of the given subcommand(s)

Usage: miy app help [COMMAND]

Commands:
  list        List apps visible to the authenticated user
  show        Show the selected app and visible installations
  status      Show server deployment state for --app, or the current project app
  deploy      Deploy this project's server build or an existing official release to dev
  promote     Promote the exact accepted release to the explicitly selected prod target
  start       
  stop        
  restart     
  remove      Remove runtime containers after confirmation, preserving the application database
  logs        Retrieve a bounded container log snapshot; rotation may omit older lines
  init        Create an independent Rust + React application
  setup       Transactionally update the compatible project CLI, guidance and MCP settings
  upgrade     Upgrade a supported SDK bundle while preserving business code and applied SQL
  check       Check app integration, managed files, components and both language toolchains
  fmt         
  dev         Run local development against the server's development database
  dev-stop    Stop this project's owned development processes; preserve its database volume
  source-pin  Print this committed app's ID, version, source commit and server archive SHA-256 offline
  source      Download or restore the exact source of an existing app release
  build       Build clean committed source on the selected server. The server runs the format check, tests and integrity checks; only its builds can be deployed. For local testing use `app dev`
  register    Request administrator approval for this project's application
  rename      Change only the platform display name (administrator)
  accept      Record evidence for a dev release before promotion
  help        Print this message or the help of the given subcommand(s)
```

## miy app help list

```text
List apps visible to the authenticated user

Usage: miy app help list
```

## miy app help show

```text
Show the selected app and visible installations

Usage: miy app help show
```

## miy app help status

```text
Show server deployment state for --app, or the current project app

Usage: miy app help status
```

## miy app help deploy

```text
Deploy this project's server build or an existing official release to dev

Usage: miy app help deploy
```

## miy app help promote

```text
Promote the exact accepted release to the explicitly selected prod target

Usage: miy app help promote
```

## miy app help start

```text
Usage: miy app help start
```

## miy app help stop

```text
Usage: miy app help stop
```

## miy app help restart

```text
Usage: miy app help restart
```

## miy app help remove

```text
Remove runtime containers after confirmation, preserving the application database

Usage: miy app help remove
```

## miy app help logs

```text
Retrieve a bounded container log snapshot; rotation may omit older lines

Usage: miy app help logs
```

## miy app help init

```text
Create an independent Rust + React application

Usage: miy app help init
```

## miy app help setup

```text
Transactionally update the compatible project CLI, guidance and MCP settings

Usage: miy app help setup
```

## miy app help upgrade

```text
Upgrade a supported SDK bundle while preserving business code and applied SQL

Usage: miy app help upgrade
```

## miy app help check

```text
Check app integration, managed files, components and both language toolchains

Usage: miy app help check
```

## miy app help fmt

```text
Usage: miy app help fmt
```

## miy app help dev

```text
Run local development against the server's development database

Usage: miy app help dev
```

## miy app help dev-stop

```text
Stop this project's owned development processes; preserve its database volume

Usage: miy app help dev-stop
```

## miy app help source-pin

```text
Print this committed app's ID, version, source commit and server archive SHA-256 offline

Usage: miy app help source-pin
```

## miy app help source

```text
Download or restore the exact source of an existing app release

Usage: miy app help source [COMMAND]

Commands:
  download  Save the verified build archive of a release to a new local file
  upload    Restore a release from its original committed project or exact build tar. Use the global --project to select a different local project directory
```

## miy app help source download

```text
Save the verified build archive of a release to a new local file

Usage: miy app help source download
```

## miy app help source upload

```text
Restore a release from its original committed project or exact build tar. Use the global --project to select a different local project directory

Usage: miy app help source upload
```

## miy app help build

```text
Build clean committed source on the selected server. The server runs the format check, tests and integrity checks; only its builds can be deployed. For local testing use `app dev`

Usage: miy app help build
```

## miy app help register

```text
Request administrator approval for this project's application

Usage: miy app help register
```

## miy app help rename

```text
Change only the platform display name (administrator)

Usage: miy app help rename
```

## miy app help accept

```text
Record evidence for a dev release before promotion

Usage: miy app help accept
```

## miy app help help

```text
Print this message or the help of the given subcommand(s)

Usage: miy app help help
```

## miy help

```text
Print this message or the help of the given subcommand(s)

Usage: miy help [COMMAND]

Commands:
  context       Manage named server contexts and the active server
  auth          Sign in and manage user sessions for the selected context
  doctor        Diagnose the requirements for remote operation or local development
  report        Report a problem without authentication, or inspect the private local outbox
  describe      Discover command inputs, outputs, permissions and effects
  completion    Generate completion for a supported shell
  component     Discover and synchronize reusable components
  job           Inspect and recover remote jobs
  registration  
  user          
  grant         
  permissions   Inspect and explicitly approve installation/release external access
  mcp           Serve the project read-only JSON-RPC MCP interface over stdio
  app           
  help          Print this message or the help of the given subcommand(s)
```

## miy help context

```text
Manage named server contexts and the active server

Usage: miy help context [COMMAND]

Commands:
  init    Register an explicitly supplied server, or ask for its URL on a terminal
  add     Add a named server origin without storing credentials in the project
  list    
  show    
  use     
  remove  Remove a local context binding; remote sessions remain until explicitly revoked
```

## miy help context init

```text
Register an explicitly supplied server, or ask for its URL on a terminal

Usage: miy help context init
```

## miy help context add

```text
Add a named server origin without storing credentials in the project

Usage: miy help context add
```

## miy help context list

```text
Usage: miy help context list
```

## miy help context show

```text
Usage: miy help context show
```

## miy help context use

```text
Usage: miy help context use
```

## miy help context remove

```text
Remove a local context binding; remote sessions remain until explicitly revoked

Usage: miy help context remove
```

## miy help auth

```text
Sign in and manage user sessions for the selected context

Usage: miy help auth [COMMAND]

Commands:
  login        Start device approval; noninteractive mode returns public instructions for auth wait
  wait         Wait for a saved device login without printing its private device code
  status       Verify the saved identity and show public session metadata
  logout       Revoke the server session, then remove local credentials
  sessions     List or explicitly revoke this user's CLI sessions
  delegations  Issue, inspect, revoke or import an explicitly scoped user delegation
```

## miy help auth login

```text
Start device approval; noninteractive mode returns public instructions for auth wait

Usage: miy help auth login
```

## miy help auth wait

```text
Wait for a saved device login without printing its private device code

Usage: miy help auth wait
```

## miy help auth status

```text
Verify the saved identity and show public session metadata

Usage: miy help auth status
```

## miy help auth logout

```text
Revoke the server session, then remove local credentials

Usage: miy help auth logout
```

## miy help auth sessions

```text
List or explicitly revoke this user's CLI sessions

Usage: miy help auth sessions [COMMAND]

Commands:
  list    
  revoke
```

## miy help auth sessions list

```text
Usage: miy help auth sessions list
```

## miy help auth sessions revoke

```text
Usage: miy help auth sessions revoke
```

## miy help auth delegations

```text
Issue, inspect, revoke or import an explicitly scoped user delegation

Usage: miy help auth delegations [COMMAND]

Commands:
  create  Create a fixed, revocable app/environment grant; secrets go only to a private file
  list    
  revoke  
  import  Import into an isolated CLI config with a matching server context and no existing credential
```

## miy help auth delegations create

```text
Create a fixed, revocable app/environment grant; secrets go only to a private file

Usage: miy help auth delegations create
```

## miy help auth delegations list

```text
Usage: miy help auth delegations list
```

## miy help auth delegations revoke

```text
Usage: miy help auth delegations revoke
```

## miy help auth delegations import

```text
Import into an isolated CLI config with a matching server context and no existing credential

Usage: miy help auth delegations import
```

## miy help doctor

```text
Diagnose the requirements for remote operation or local development

Usage: miy help doctor
```

## miy help report

```text
Report a problem without authentication, or inspect the private local outbox

Usage: miy help report [COMMAND]

Commands:
  send   Send a manual report without login. Does not read the credential store
  retry  Resend one saved report to its original server, without authentication
  list   List locally saved reports and submission state
  show   Inspect a locally saved redacted report
```

## miy help report send

```text
Send a manual report without login. Does not read the credential store

Usage: miy help report send
```

## miy help report retry

```text
Resend one saved report to its original server, without authentication

Usage: miy help report retry
```

## miy help report list

```text
List locally saved reports and submission state

Usage: miy help report list
```

## miy help report show

```text
Inspect a locally saved redacted report

Usage: miy help report show
```

## miy help describe

```text
Discover command inputs, outputs, permissions and effects

Usage: miy help describe
```

## miy help completion

```text
Generate completion for a supported shell

Usage: miy help completion
```

## miy help component

```text
Discover and synchronize reusable components

Usage: miy help component [COMMAND]

Commands:
  list     Read live component releases from the configured platform
  show     Read usage, source and compatibility of a release (latest compatible by default)
  add      Install a new component; does not execute package hooks or change business code
  update   Explicitly update an installed component (latest compatible by default)
  sync     Fetch template defaults initially; otherwise restore exact locked releases and missing files
  check    Verify installed files and compatibility without contacting the platform
  publish  Publish a component source directory; requires platform administrator authority
```

## miy help component list

```text
Read live component releases from the configured platform

Usage: miy help component list
```

## miy help component show

```text
Read usage, source and compatibility of a release (latest compatible by default)

Usage: miy help component show
```

## miy help component add

```text
Install a new component; does not execute package hooks or change business code

Usage: miy help component add
```

## miy help component update

```text
Explicitly update an installed component (latest compatible by default)

Usage: miy help component update
```

## miy help component sync

```text
Fetch template defaults initially; otherwise restore exact locked releases and missing files

Usage: miy help component sync
```

## miy help component check

```text
Verify installed files and compatibility without contacting the platform

Usage: miy help component check
```

## miy help component publish

```text
Publish a component source directory; requires platform administrator authority

Usage: miy help component publish
```

## miy help job

```text
Inspect and recover remote jobs

Usage: miy help job [COMMAND]

Commands:
  list    
  get     
  watch   Observe events after the saved/explicit cursor; stopping observation does not cancel work
  logs    Read bounded server-build log entries after a sequence
  resume  Recover the original request or reconnect to an acknowledged runtime or build job
  retry   Retry a typed transient failure after fresh preflight and target confirmation
```

## miy help job list

```text
Usage: miy help job list
```

## miy help job get

```text
Usage: miy help job get
```

## miy help job watch

```text
Observe events after the saved/explicit cursor; stopping observation does not cancel work

Usage: miy help job watch
```

## miy help job logs

```text
Read bounded server-build log entries after a sequence

Usage: miy help job logs
```

## miy help job resume

```text
Recover the original request or reconnect to an acknowledged runtime or build job

Usage: miy help job resume
```

## miy help job retry

```text
Retry a typed transient failure after fresh preflight and target confirmation

Usage: miy help job retry
```

## miy help registration

```text
Usage: miy help registration [COMMAND]

Commands:
  list     
  approve  
  reject
```

## miy help registration list

```text
Usage: miy help registration list
```

## miy help registration approve

```text
Usage: miy help registration approve
```

## miy help registration reject

```text
Usage: miy help registration reject
```

## miy help user

```text
Usage: miy help user [COMMAND]

Commands:
  list
```

## miy help user list

```text
Usage: miy help user list
```

## miy help grant

```text
Usage: miy help grant [COMMAND]

Commands:
  list    
  set     
  revoke
```

## miy help grant list

```text
Usage: miy help grant list
```

## miy help grant set

```text
Usage: miy help grant set
```

## miy help grant revoke

```text
Usage: miy help grant revoke
```

## miy help permissions

```text
Inspect and explicitly approve installation/release external access

Usage: miy help permissions [COMMAND]

Commands:
  get  Read requested/approved external permissions and the revision before changing them
  set  Replace approvals for exactly one release, using a reviewed JSON input and revision
```

## miy help permissions get

```text
Read requested/approved external permissions and the revision before changing them

Usage: miy help permissions get
```

## miy help permissions set

```text
Replace approvals for exactly one release, using a reviewed JSON input and revision

Usage: miy help permissions set
```

## miy help mcp

```text
Serve the project read-only JSON-RPC MCP interface over stdio

Usage: miy help mcp
```

## miy help app

```text
Usage: miy help app [COMMAND]

Commands:
  list        List apps visible to the authenticated user
  show        Show the selected app and visible installations
  status      Show server deployment state for --app, or the current project app
  deploy      Deploy this project's server build or an existing official release to dev
  promote     Promote the exact accepted release to the explicitly selected prod target
  start       
  stop        
  restart     
  remove      Remove runtime containers after confirmation, preserving the application database
  logs        Retrieve a bounded container log snapshot; rotation may omit older lines
  init        Create an independent Rust + React application
  setup       Transactionally update the compatible project CLI, guidance and MCP settings
  upgrade     Upgrade a supported SDK bundle while preserving business code and applied SQL
  check       Check app integration, managed files, components and both language toolchains
  fmt         
  dev         Run local development against the server's development database
  dev-stop    Stop this project's owned development processes; preserve its database volume
  source-pin  Print this committed app's ID, version, source commit and server archive SHA-256 offline
  source      Download or restore the exact source of an existing app release
  build       Build clean committed source on the selected server. The server runs the format check, tests and integrity checks; only its builds can be deployed. For local testing use `app dev`
  register    Request administrator approval for this project's application
  rename      Change only the platform display name (administrator)
  accept      Record evidence for a dev release before promotion
```

## miy help app list

```text
List apps visible to the authenticated user

Usage: miy help app list
```

## miy help app show

```text
Show the selected app and visible installations

Usage: miy help app show
```

## miy help app status

```text
Show server deployment state for --app, or the current project app

Usage: miy help app status
```

## miy help app deploy

```text
Deploy this project's server build or an existing official release to dev

Usage: miy help app deploy
```

## miy help app promote

```text
Promote the exact accepted release to the explicitly selected prod target

Usage: miy help app promote
```

## miy help app start

```text
Usage: miy help app start
```

## miy help app stop

```text
Usage: miy help app stop
```

## miy help app restart

```text
Usage: miy help app restart
```

## miy help app remove

```text
Remove runtime containers after confirmation, preserving the application database

Usage: miy help app remove
```

## miy help app logs

```text
Retrieve a bounded container log snapshot; rotation may omit older lines

Usage: miy help app logs
```

## miy help app init

```text
Create an independent Rust + React application

Usage: miy help app init
```

## miy help app setup

```text
Transactionally update the compatible project CLI, guidance and MCP settings

Usage: miy help app setup
```

## miy help app upgrade

```text
Upgrade a supported SDK bundle while preserving business code and applied SQL

Usage: miy help app upgrade
```

## miy help app check

```text
Check app integration, managed files, components and both language toolchains

Usage: miy help app check
```

## miy help app fmt

```text
Usage: miy help app fmt
```

## miy help app dev

```text
Run local development against the server's development database

Usage: miy help app dev
```

## miy help app dev-stop

```text
Stop this project's owned development processes; preserve its database volume

Usage: miy help app dev-stop
```

## miy help app source-pin

```text
Print this committed app's ID, version, source commit and server archive SHA-256 offline

Usage: miy help app source-pin
```

## miy help app source

```text
Download or restore the exact source of an existing app release

Usage: miy help app source [COMMAND]

Commands:
  download  Save the verified build archive of a release to a new local file
  upload    Restore a release from its original committed project or exact build tar. Use the global --project to select a different local project directory
```

## miy help app source download

```text
Save the verified build archive of a release to a new local file

Usage: miy help app source download
```

## miy help app source upload

```text
Restore a release from its original committed project or exact build tar. Use the global --project to select a different local project directory

Usage: miy help app source upload
```

## miy help app build

```text
Build clean committed source on the selected server. The server runs the format check, tests and integrity checks; only its builds can be deployed. For local testing use `app dev`

Usage: miy help app build
```

## miy help app register

```text
Request administrator approval for this project's application

Usage: miy help app register
```

## miy help app rename

```text
Change only the platform display name (administrator)

Usage: miy help app rename
```

## miy help app accept

```text
Record evidence for a dev release before promotion

Usage: miy help app accept
```

## miy help help

```text
Print this message or the help of the given subcommand(s)

Usage: miy help help
```

