#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
set -a
source "${MIY_STATE_DIR:-$PWD/.local}/runtime.env"
set +a
if [ "$#" -eq 0 ]; then set -- serve; fi
# Only serve administers sign-in accounts (account links).
case "$1" in
  serve) unset PROVISION_DATABASE_URL ;;
  migrate|seed) unset PROVISION_DATABASE_URL MIY_IDENTITY_ADMIN_URL MIY_ACCOUNTS_SECRET ;;
  catalog-sync) unset PROVISION_DATABASE_URL MIY_WEB_SECRET MIY_RESOURCE_SECRET MIY_IDENTITY_ADMIN_URL MIY_ACCOUNTS_SECRET ;;
  worker) unset MIY_WEB_SECRET MIY_RESOURCE_SECRET MIY_IDENTITY_ADMIN_URL MIY_ACCOUNTS_SECRET ;;
esac
export RUST_LOG="${RUST_LOG:-miy=info}"
exec "${MIY_BINARY:-$PWD/target/debug/miy}" "$@"
