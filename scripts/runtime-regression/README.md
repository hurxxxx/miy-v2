# Isolated runtime regression

Run from the repository root with Rust 1.99.1, Python 3, OpenSSL, Docker, and cached `postgres:18.3` / `python:3.14-slim` images:

```sh
python3 scripts/runtime-regression/run.py
```

The runner creates a fresh temporary evidence directory, a PostgreSQL container with a random loopback port and disposable storage, and synthetic app images. It runs the actual runtime/worker code against this isolated database. It never reads `.local/runtime.env` or stops the ordinary platform runner. The test uses fixture identity metadata; it does not verify OAuth or business UI integration.

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
