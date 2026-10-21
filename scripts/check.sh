#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
export NODE_EXTRA_CA_CERTS="$PWD/.local/caddy/caddy/pki/authorities/local/root.crt"
mkdir -p .local/evidence
run_archive=".local/evidence/archive/$(date -u +%Y%m%dT%H%M%S)-$$"
mkdir -p "$run_archive"
for artifact in .local/evidence/*; do
  if [ -f "$artifact" ]; then mv "$artifact" "$run_archive/"; fi
done
curl --cacert "$NODE_EXTRA_CA_CERTS" -fsS https://localhost:18443/api/health >/dev/null
cargo test --workspace --locked
pnpm typecheck
pnpm test:e2e
python3 scripts/services.py stop worker
trap 'python3 scripts/services.py start worker' EXIT
set -a
source .local/runtime.env
set +a
cargo test --workspace --locked real_database_isolation_migrations_and_crash_recovery -- --ignored
python3 scripts/services.py start worker
trap - EXIT
node scripts/check-codex.mjs
node scripts/check-mcp.mjs
python3 scripts/report.py
