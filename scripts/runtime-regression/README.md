# Isolated runtime regression

Run from the repository root with Rust 1.99.1, Python 3, OpenSSL, Docker, and cached `postgres:18.3` / `python:3.14-slim` images:

```sh
python3 scripts/runtime-regression/run.py
```

The runner creates a fresh temporary evidence directory, a PostgreSQL container with a random loopback port and disposable storage, and synthetic app images. It builds the actual CLI and runs the HTTP router, authentication, runtime and worker code against this isolated database. It never reads `.local/runtime.env` or stops the ordinary platform runner. The test uses fixture identity metadata and the explicit loopback development-account route; it verifies device/session issuance, refresh, revoke and Core user disable but does not verify a real OIDC issuer or business UI integration.

The Docker shim injects an error for container lookup and kills only the test worker immediately after successful removal of its exact test container ID. Tests cover ownership refusal, confirmed absence, retry after interruption, service failure/recovery, manual stop, audit transitions, and existing deployment/data-preservation boundaries. Twelve gated HTTP fixtures also verify that the real worker can start a different app before their responses arrive, uses bounded concurrency, and discards observations from before a same-release lifecycle change. These additional service fixtures use synthetic installation metadata, not twelve deployed app containers.

Any failed observation or missing results file makes the command fail. Cleanup runs even when setup fails. It uses test labels and the private Docker journal when database evidence is unavailable, checks ownership before removing resources, and verifies original running containers retained their IDs, images and start times. Repeated cleanup is safe. Keep the printed evidence directory; check `cleanup.log` on failure.

Run failure-injection checks without Docker or a platform server:

```sh
python3 scripts/runtime-regression/test_cleanup.py
```

The Docker stub exercises the actual runner and cleanup scripts with failure before database creation, an unavailable database, and an image build that fails after creating an unrecorded image. It also checks an empty initial Docker inventory and repeated cleanup.

The process/service UI can be checked without using the running platform:

```sh
pnpm exec playwright test --config scripts/runtime-regression/ui.config.ts
```

This builds the platform and a real Federation test app into `.local/evidence/runtime-health-ui/` and starts its own preview on `127.0.0.1:18147` (fails if occupied). It does not replace the running platform's build. API requests are intercepted with fixtures. Tests cover health displays, unsaved input and SDK callback stability, release-change API blocking, and late responses across app and environment navigation. It does not send mutations to a platform API.

The platform shell can also be checked against the real core server of this checkout:

```sh
cargo build
pnpm exec playwright test --config scripts/runtime-regression/server-ui.config.ts
```

`server-ui.mjs` starts a disposable `postgres:18.3` container (random loopback port, tmpfs, unique execution label `miy.server-ui`), static OIDC metadata, `miy migrate`/`seed`/`serve` on 127.0.0.1:18149 and the built platform with `/api` and `/auth` proxied on 127.0.0.1:18148. It uses only the loopback development login and never reads `.local/runtime.env`. Cleanup removes only the container created by this execution after checking its owner label. A `.local/evidence/server-ui.lock` directory prevents concurrent runs from overwriting evidence. It does not cover Keycloak login, deployed Docker apps or remote app screens served by the core. A lock or container left by SIGKILL requires explicit inspection; a new run never removes all containers sharing the harness label.

The full stack (Keycloak, Caddy, core serve/worker and CLI-deployed Docker apps) runs disposably:

```sh
scripts/runtime-regression/stack-ui.sh run   # up → tests/stack-ui.spec.ts → down → host diff
scripts/runtime-regression/stack-ui.sh up    # or start/stop by hand
scripts/runtime-regression/stack-ui.sh down
```

It uses compose project `miy-stack-ui` with `stack-ui.compose.yaml` (own app network, own platform build), ports 18300–18304, state in `.local/stack-ui` and `MIY_DATABASE_CONTAINER=miy-stack-ui-postgres-1`; the core listens on the Docker bridge address only. `down` removes the app containers, `miy-runtime-*` networks and images belonging to installations in the stack's own database, then the compose project with its volume and the state directory. `run` compares containers, networks, volumes and images before and after and fails on any difference (`.local/evidence/stack-ui/host-diff.txt`). It never reads `.local/runtime.env` or touches `miy-v2-validation-*`. Docker BuildKit cache from the app build is shared and kept.

The atomic state-directory creation and `.stack-ui-owner` marker establish ownership before `run` enables cleanup. Existing state, partial compose resources, or a port conflict cause refusal without deleting those resources or replacing existing evidence. `down` also verifies the marker and database compose label; older state without a marker must be inspected manually rather than adopted automatically.

Run UI harness ownership checks with mocked Docker/service commands, without a Docker daemon or platform server:

```sh
python3 scripts/runtime-regression/test_ui_cleanup.py
```

Remote app rendering before and after a shell change:

```sh
node scripts/runtime-regression/app-css-baseline.mjs <base-commit>
```

It builds the base platform in a temporary git worktree, loads the reference fixture app in both shells with mocked APIs and writes computed-style differences and screenshots to `.local/evidence/app-css-baseline/`.

The CLI 0.5 suites additionally check permission-scoped pagination, atomic event cursors, preflight/expected-state and confirmation rejection, typed retry, idempotent upload receipts, bounded worker logs, concurrent real CLI refresh, typed JSON input and request-ID resume. Two blocked log diagnostics must allow a production start to complete; the owned app prints 2 MiB so collection must truncate to 1 MiB. Neither test reads an unrelated container's logs.

Client transport failures can also be checked without Docker/PostgreSQL:

```sh
python3 scripts/check-cli-recovery.py --cli target/debug/miy
```

That separate check uses a synthetic Docker export and loopback HTTP fault fixture; it proves CLI journal/ACK recovery and request counts, not a real image import. Browser fixtures also cover device approval/CSRF and production confirmation/state conflicts.
