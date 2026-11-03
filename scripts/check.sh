#!/usr/bin/env bash
# Full local acceptance: static checks, then the running local development stack
# (scripts/up.sh). It reads .local/runtime.env and stops/restarts the local worker,
# so it refuses a .local whose origin is not loopback (a shared or production
# platform). Shared hosts and CI use scripts/check-static.sh instead.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
origin=$(sed -n 's/^MIY_ORIGIN=//p' .local/runtime.env 2>/dev/null | tail -n 1)
case "$origin" in
  https://localhost:*|https://127.0.0.1:*|http://localhost:*|http://127.0.0.1:*) ;;
  *)
    if [ -z "$origin" ] || [ "${MIY_CHECK_ALLOW_NON_LOOPBACK:-}" != "$origin" ]; then
      echo "check.sh: .local origin '${origin:-missing}' is not a loopback development stack; refusing." >&2
      echo "Run scripts/check-static.sh here, or on a dedicated test host set MIY_CHECK_ALLOW_NON_LOOPBACK=<that origin>." >&2
      exit 1
    fi
    ;;
esac
export NODE_EXTRA_CA_CERTS="$PWD/.local/caddy/caddy/pki/authorities/local/root.crt"
mkdir -p .local/evidence
run_archive=".local/evidence/archive/$(date -u +%Y%m%dT%H%M%S)-$$"
mkdir -p "$run_archive"
for artifact in .local/evidence/*; do
  if [ -f "$artifact" ]; then mv "$artifact" "$run_archive/"; fi
done
curl --cacert "$NODE_EXTRA_CA_CERTS" -fsS https://localhost:18443/api/health >/dev/null
bash scripts/check-static.sh all
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
