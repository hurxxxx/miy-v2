# Native Windows Todo validation source

These are the business files created by an independent Codex CLI session on
Windows 11 on 2026-10-29. They contain no CLI binaries, credentials, local session
state, SDK copies, dependency installs or database connection strings.

Use the CLI built from this branch to scaffold a fresh independent project with
ID `todo-win-20261029` and name `Windows Todo 검증`. Copy this directory's `src/`,
`web/src/`, `migrations/0002_todos.sql` and `manifest.json` over the fresh project's
business files. Keep every generated SDK, lock, initial migration and CLI-managed
file intact. Run component sync and the project CLI's app check, then commit clean
source before building a release. This is an existing registered test app; do not
create another app ID or remove its database.

The application uses PostgreSQL, verified Identity ownership and runtime.api.
Its endpoints are GET/POST `/todos`, PATCH `/todos/{id}` with `{completed:boolean}`,
and DELETE `/todos/{id}`. Titles are trimmed and limited to 200 Unicode characters;
unknown ownership fields and invalid IDs are rejected. Six Rust unit tests cover
input and error handling. The UI has add, complete/undo, delete confirmation,
filters, pending/error states and a navigation guard.

The Windows native preview used the actual server at `https://v2.dwdcc.kr` with
an admin session selected by the user. Its server development database UUID is
`129f8abd-62c2-40d1-9175-d73d85196940`. Preserve todo UUID
`8dcfffe2-3854-4ce5-b878-2da9f915c151`, title
`네이티브 실행 후에도 남는 할 일 20261029`, completed `false`.
Windows browser create, complete/undo, refresh and native stop/restart retained
that row and the database UUID. API CRUD and rejection of blank title, forged
ownership, missing CSRF and unsigned direct backend calls also passed.

Linux handoff: build the CLI from this branch, use the explicitly selected server
context and a real login, scaffold and overlay the business files above, sync
components, check and commit the project, then use app build and dev deployment.
Use the Linux server's existing Docker Engine for the Linux release image. No
Docker Desktop or WSL is required on the Windows developer PC. Do not redeploy
the platform, change its core database or promote this app to production.
After the dev job completes, verify the exact release/image and the same database
and todo UUID through the actual dev app UI. Record any unavailable browser or
permissions check as NOT_RUN. Full developer least-privilege and production DB
isolation remain separate acceptance scopes.
