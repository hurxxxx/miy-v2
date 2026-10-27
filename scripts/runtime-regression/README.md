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

The CLI 0.5 suites additionally check permission-scoped pagination, atomic event cursors, preflight/expected-state and confirmation rejection, typed retry, idempotent upload receipts, bounded worker logs, concurrent real CLI refresh, typed JSON input and request-ID resume. Two blocked log diagnostics must allow a production start to complete; the owned app prints 2 MiB so collection must truncate to 1 MiB. Neither test reads an unrelated container's logs.

Client transport failures can also be checked without Docker/PostgreSQL:

```sh
python3 scripts/check-cli-recovery.py --cli target/debug/miy
```

That separate check uses a synthetic Docker export and loopback HTTP fault fixture; it proves CLI journal/ACK recovery and request counts, not a real image import. Browser fixtures also cover device approval/CSRF and production confirmation/state conflicts.
