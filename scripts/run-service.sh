#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
set -a
source "${MIY_STATE_DIR:-$PWD/.local}/runtime.env"
set +a
case "${1:-serve}" in
  serve|migrate|seed) unset PROVISION_DATABASE_URL ;;
  worker) unset MIY_WEB_SECRET MIY_RESOURCE_SECRET ;;
esac
export RUST_LOG="${RUST_LOG:-miy=info}"
exec "${MIY_BINARY:-$PWD/target/debug/miy}" "${1:-serve}"
