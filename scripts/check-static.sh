#!/usr/bin/env bash
# Checks that need no running platform, Docker daemon, service manager or live state.
# Safe on a host whose .local serves a platform and in CI:
# - web builds go to .local/evidence/check-static (never web/platform/dist, which Nginx may serve);
# - browser suites use their own preview ports with mocked APIs;
# - script tests use stubs and temporary directories.
# Usage: scripts/check-static.sh [all|rust|web|scripts]   (default: all)
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
OUT=$PWD/.local/evidence/check-static

rust_checks() {
  python3 scripts/check-bundle-portability.py source
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets --locked -- -D warnings
  cargo clippy --workspace --all-targets --locked --no-default-features -- -D warnings
  cargo test --workspace --locked
  cargo test --workspace --locked --no-default-features
}

web_checks() {
  pnpm typecheck
  pnpm --filter @miy/platform exec vite build --mode production --outDir "$OUT/platform" --emptyOutDir
  for mode in reference diagnosis; do
    pnpm --filter @miy/fixtures exec vite build --mode "$mode" --outDir "$OUT/fixtures-$mode" --emptyOutDir
  done
  pnpm exec playwright test --config scripts/runtime-regression/ui.config.ts
  pnpm exec playwright test --config tests/cli-experience.config.ts
  # Needs a non-loopback IPv4 address (or MIY_HTTP_TEST_HOST).
  pnpm exec playwright test --config scripts/runtime-regression/http-ui.config.ts
}

script_checks() {
  python3 scripts/check-doc-links.py
  for script in scripts/*.sh scripts/runtime-regression/*.sh; do bash -n "$script"; done
  for script in scripts/*.mjs scripts/lib/*.mjs scripts/runtime-regression/*.mjs; do node --check "$script"; done
  python3 -c 'import ast, sys; [ast.parse(open(f, encoding="utf-8").read(), f) for f in sys.argv[1:]]' \
    scripts/*.py scripts/runtime-regression/*.py
  python3 scripts/runtime-regression/test_cleanup.py
  python3 scripts/runtime-regression/test_ui_cleanup.py
  python3 scripts/test-cli-release.py
  python3 scripts/test-cli-downloads.py
  python3 scripts/test-windows-installer-package.py
  python3 scripts/test-agent-acceptance-proxy.py
  python3 scripts/test-core-ops.py
  python3 scripts/check-server-config.py
}

case "${1:-all}" in
  rust) rust_checks ;;
  web) web_checks ;;
  scripts) script_checks ;;
  all) script_checks; rust_checks; web_checks ;;
  *) echo "usage: $0 [all|rust|web|scripts]" >&2; exit 2 ;;
esac
echo "check-static ${1:-all}: PASS"
