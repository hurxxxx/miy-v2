#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
python3 scripts/bootstrap.py
docker compose --env-file .local/compose.env up -d
ca="$PWD/.local/caddy/caddy/pki/authorities/local/root.crt"
ready=false
for attempt in {1..90}; do
  if [ -f "$ca" ] && curl --cacert "$ca" -fsS https://localhost:18443/identity/realms/miy/.well-known/openid-configuration >/dev/null 2>&1; then ready=true; break; fi
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
docker build -f ops/browser.Dockerfile -t miy-v2-validation-browser:1.64.1 .
if ! docker inspect miy-v2-validation-browser >/dev/null 2>&1; then
  docker run -d --name miy-v2-validation-browser -p 127.0.0.1:19222:9222 --add-host=host.docker.internal:host-gateway -v "$ca:/miy-ca.crt:ro" miy-v2-validation-browser:1.64.1
else
  docker start miy-v2-validation-browser >/dev/null
fi
ready=false
for attempt in {1..60}; do
  if curl --cacert "$ca" -fsS https://localhost:18443/api/health >/dev/null 2>&1 && curl -fsS http://127.0.0.1:19222/json/version >/dev/null 2>&1; then ready=true;break;fi
  sleep 1
done
if [ "$ready" != true ];then echo 'Core or browser did not become ready' >&2;exit 1;fi
echo 'Validation services ready: https://localhost:18443'
echo 'Local CA: .local/caddy/caddy/pki/authorities/local/root.crt; credentials: .local/credentials.json (private).'
