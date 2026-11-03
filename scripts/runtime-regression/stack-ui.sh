#!/usr/bin/env bash
# Disposable full stack for UI-5: PostgreSQL, Keycloak, Caddy, core serve/worker and
# CLI-deployed Docker apps, all under project "miy-stack-ui" and .local/stack-ui.
#   up    start the stack          down  remove everything this stack created
#   run   up → browser checks → down (always), then verify the running platform is unchanged
# Never reads .local/runtime.env and never touches miy-v2-validation-* resources.
set -euo pipefail
cd "$(dirname "$0")/../.."
export PATH="$HOME/.cargo/bin:$PATH"
ROOT=$PWD
STATE=$ROOT/.local/stack-ui
PROJECT=miy-stack-ui
ORIGIN=http://localhost:18300
DB=$PROJECT-postgres-1
EVIDENCE=$ROOT/.local/evidence/stack-ui
OWNER_FILE=$STATE/.stack-ui-owner
OWNER_TOKEN=
OWNED_STATE=0
CAPTURE_SNAPSHOT=0
export MIY_STATE_DIR=$STATE
export MIY_DATABASE_CONTAINER=$DB
compose=(docker compose -p "$PROJECT" -f compose.yaml -f scripts/runtime-regression/stack-ui.compose.yaml --env-file "$STATE/compose.env")

project_resources() {
  docker ps -aq --filter "label=com.docker.compose.project=$PROJECT" || return
  docker volume ls -q --filter "label=com.docker.compose.project=$PROJECT" || return
  docker network ls -q --filter "label=com.docker.compose.project=$PROJECT" || return
  docker network ls -q --filter "name=^${PROJECT}-app-runtime$"
}

up() {
  if docker inspect "$DB" >/dev/null 2>&1 || [ -e "$STATE" ] || [ -L "$STATE" ]; then
    echo "stack-ui already exists; run '$0 down' first" >&2; exit 1
  fi
  resources=$(project_resources) || { echo "cannot inspect stack-ui ownership" >&2; exit 1; }
  if [ -n "$resources" ]; then
    echo "stack-ui resources already exist; refusing to adopt or remove them" >&2; exit 1
  fi
  for port in 18300 18301 18302 18303 18304; do
    if ss -ltn "sport = :$port" | grep -q LISTEN; then echo "port $port is in use" >&2; exit 1; fi
  done
  # The atomic mkdir is also the single-run lock. A rejected/concurrent run must
  # never remove a previous invocation's state or Docker resources in its trap.
  mkdir -p "$ROOT/.local"
  mkdir -m 700 "$STATE"
  OWNER_TOKEN="$$-$RANDOM-$RANDOM"
  printf '%s\n%s\n' "$ROOT" "$OWNER_TOKEN" >"$OWNER_FILE"
  OWNED_STATE=1
  if [ "$CAPTURE_SNAPSHOT" -eq 1 ]; then
    mkdir -p "$EVIDENCE"
    snapshot >"$EVIDENCE/host-before.txt"
  fi
  # The core listens on the Docker bridge address so only the proxy container can reach it.
  python3 scripts/bootstrap.py --state-dir "$STATE" --origin "$ORIGIN" --bind 172.17.0.1:18304 \
    --postgres-port 18301 --identity-port 18302 --http-port 18300 --https-port 18303 --no-shared-development-login
  pnpm --filter @miy/platform exec vite build --mode production --outDir "$STATE/site" --emptyOutDir
  "${compose[@]}" up -d
  for _ in $(seq 1 120); do
    curl -fsS http://127.0.0.1:18302/identity/realms/miy/.well-known/openid-configuration >/dev/null 2>&1 && break
    sleep 1
  done
  python3 scripts/configure-identity.py
  cargo build --locked
  bash scripts/run-service.sh migrate
  bash scripts/run-service.sh seed
  python3 scripts/services.py start
  for _ in $(seq 1 60); do
    curl -fsS "$ORIGIN/api/health" >/dev/null 2>&1 && { echo "stack-ui ready: $ORIGIN"; return; }
    sleep 1
  done
  echo "stack-ui core did not become ready; see $STATE/serve.log" >&2; exit 1
}

down() {
  if [ ! -e "$STATE" ] && [ ! -L "$STATE" ]; then
    resources=$(project_resources) || return 1
    if docker inspect "$DB" >/dev/null 2>&1 || [ -n "$resources" ]; then
      echo "stack-ui has resources without owned state; refusing cleanup" >&2; return 1
    fi
    echo "stack-ui already absent"
    return 0
  fi
  if [ -L "$STATE" ] || [ ! -f "$OWNER_FILE" ] || [ -L "$OWNER_FILE" ] ||
      [ "$(sed -n '1p' "$OWNER_FILE")" != "$ROOT" ] ||
      [ -z "$(sed -n '2p' "$OWNER_FILE")" ] ||
      { [ "$OWNED_STATE" -eq 1 ] && [ "$(sed -n '2p' "$OWNER_FILE")" != "$OWNER_TOKEN" ]; }; then
    echo "stack-ui state is not owned by this harness; refusing cleanup" >&2; return 1
  fi
  if docker inspect "$DB" >/dev/null 2>&1 &&
      [ "$(docker inspect -f '{{ index .Config.Labels "com.docker.compose.project" }}' "$DB")" != "$PROJECT" ]; then
    echo "stack-ui database name belongs to another owner; refusing cleanup" >&2; return 1
  fi
  set +e
  [ -d "$STATE" ] && python3 scripts/services.py stop
  if docker inspect "$DB" >/dev/null 2>&1; then
    sql() { docker exec "$DB" psql -U postgres -d miy_core -At -c "$1" 2>/dev/null; }
    installations=$(sql "SELECT replace(id::text,'-','') FROM installations")
    apps=$(sql "SELECT id FROM apps")
    images=$(sql "SELECT DISTINCT runtime_local_image FROM installations WHERE runtime_local_image IS NOT NULL")
    for id in $installations; do
      [[ $id =~ ^[0-9a-f]{32}$ ]] || continue
      docker rm -f "miy-app-$id" >/dev/null 2>&1
      docker network disconnect -f "miy-runtime-$id" "$DB" >/dev/null 2>&1
      docker network rm "miy-runtime-$id" >/dev/null 2>&1
    done
    for image in $images; do docker image rm "$image" >/dev/null 2>&1; done
    for app in $apps; do
      [[ $app =~ ^ui5-[a-z0-9]+$ ]] || continue
      for image in $(docker images -q --filter "label=org.miy.app.id=$app" | sort -u) $(docker images -q "miy-app/$app"); do
        docker image rm -f "$image" >/dev/null 2>&1
      done
    done
  fi
  if [ -f "$STATE/compose.env" ]; then
    # Caddy writes its data directory as root; clear it from inside the container.
    "${compose[@]}" exec -T proxy sh -c 'rm -rf /data/* /data/.[!.]*' >/dev/null 2>&1
    "${compose[@]}" down -v --remove-orphans
  fi
  rm -rf "$STATE"
  left=$( { docker ps -aq --filter "label=com.docker.compose.project=$PROJECT"
            docker volume ls -q --filter "label=com.docker.compose.project=$PROJECT"
            docker network ls -q --filter "name=$PROJECT"
            [ -e "$STATE" ] && echo "$STATE"; } | wc -l)
  echo "stack-ui removed (leftover resources: $left)"
  set -e
  [ "$left" -eq 0 ]
}

# Identity of the running platform's containers, networks and app images.
snapshot() {
  { docker ps -a --format '{{.ID}} {{.Names}} {{.State}}' | awk -v prefix="$PROJECT-" 'index($2, prefix) != 1' | sort
    docker network ls --format '{{.ID}} {{.Name}}' | awk -v prefix="$PROJECT" 'index($2, prefix) != 1' | sort
    docker volume ls -q | awk -v prefix="${PROJECT}_" 'index($0, prefix) != 1' | sort
    docker images --format '{{.ID}} {{.Repository}}:{{.Tag}}' | sort; }
}

case "${1:-}" in
  up) up ;;
  down) down ;;
  run)
    CAPTURE_SNAPSHOT=1
    status=0
    trap 'if [ "$OWNED_STATE" -eq 1 ]; then down; snapshot >"$EVIDENCE/host-after.txt"; diff -u "$EVIDENCE/host-before.txt" "$EVIDENCE/host-after.txt" >"$EVIDENCE/host-diff.txt" && echo "running platform unchanged" || { echo "host resources changed: $EVIDENCE/host-diff.txt" >&2; exit 1; }; fi' EXIT
    up
    pnpm exec playwright test -c scripts/runtime-regression/stack-ui.config.ts || status=$?
    exit $status
    ;;
  *) echo "usage: $0 up|down|run" >&2; exit 2 ;;
esac
