#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
# --profile server requires an explicit origin on its first configuration.
bootstrap_args=("$@")
while [ "$#" -gt 0 ]; do
  case "$1" in
    --state-dir) export MIY_STATE_DIR="${2:?--state-dir requires a path}"; shift 2 ;;
    --state-dir=*) export MIY_STATE_DIR="${1#--state-dir=}"; shift ;;
    *) shift ;;
  esac
done
python3 scripts/bootstrap.py "${bootstrap_args[@]}"
export MIY_STATE_DIR="${MIY_STATE_DIR:-$PWD/.local}"
readarray -t settings < <(python3 - "$MIY_STATE_DIR/server.json" <<'CONFIG'
import json,sys
v=json.load(open(sys.argv[1]))
for key in ['origin','caFile','identityAdminUrl','proxyMode','developmentOrigin','developmentLogin']:
 print(v.get(key) or '')
CONFIG
)
origin="${settings[0]}"
ca="${settings[1]}"
identity_admin="${settings[2]}"
proxy_mode="${settings[3]}"
compose=(docker compose --env-file "$MIY_STATE_DIR/compose.env")
if [ "$proxy_mode" = local ]; then
  "${compose[@]}" up -d
else
  "${compose[@]}" up -d postgres keycloak
fi
ready=false
for attempt in {1..90}; do
  if curl -fsS "$identity_admin/realms/miy/.well-known/openid-configuration" >/dev/null 2>&1; then ready=true; break; fi
  sleep 1
done
if [ "$ready" != true ]; then echo 'Identity service did not become ready' >&2;exit 1;fi
python3 scripts/configure-identity.py
cargo build --locked
bash scripts/run-service.sh migrate
bash scripts/run-service.sh seed
pnpm install --frozen-lockfile
pnpm build
python3 scripts/services.py start
curl_options=()
if [ -n "$ca" ]; then curl_options+=(--cacert "$ca"); fi
if [ "$proxy_mode" = local ]; then
  docker build -f ops/browser.Dockerfile -t miy-v2-validation-browser:1.64.1 .
  if ! docker inspect miy-v2-validation-browser >/dev/null 2>&1; then
    browser_options=()
    if [ -n "$ca" ]; then browser_options+=(-v "$ca:/miy-ca.crt:ro"); fi
    docker run -d --name miy-v2-validation-browser -p 127.0.0.1:19222:9222 --add-host=host.docker.internal:host-gateway "${browser_options[@]}" miy-v2-validation-browser:1.64.1
  else
    docker start miy-v2-validation-browser >/dev/null
  fi
fi
ready=false
for attempt in {1..60}; do
  if curl "${curl_options[@]}" -fsS "$origin/api/health" >/dev/null 2>&1; then
    if [ "$proxy_mode" != local ] || curl -fsS http://127.0.0.1:19222/json/version >/dev/null 2>&1; then ready=true;break;fi
  fi
  sleep 1
done
if [ "$ready" != true ];then echo 'Core or browser did not become ready; verify the configured origin and reverse proxy' >&2;exit 1;fi
printf 'Platform ready: %s; private configuration: %s\n' "$origin" "$MIY_STATE_DIR"
if [ "${settings[5]}" = True ]; then
  printf 'Local account shortcuts: %s\n' "${settings[4]}"
fi
