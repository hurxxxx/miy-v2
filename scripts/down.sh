#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python3 scripts/services.py stop
docker rm -f miy-v2-validation-browser >/dev/null 2>&1 || true
docker compose --env-file .local/compose.env down
# Preserve database volumes, artifacts and credentials.
