#!/usr/bin/env python3
"""Report the shared PostgreSQL connection budget of a miy host (V1-O2). Read-only.

Worst case = core pools + identity (Keycloak) pool + every installation's runtime and
migration role limit + active development session role limits. It must stay within
max_connections minus the reserved slots. Current use is grouped by application_name.
FAIL when a managed login role has no connection limit or the worst case exceeds the
usable connections. Only setting names and counts are printed, never secrets.
"""
import argparse
import json
import subprocess
import sys

CORE_POOLS = 2 * 10  # serve + worker PgPoolOptions::max_connections(10)
CORE_TRANSIENT_SOURCES = {
    # Both owned connections live until prepare_resource returns, including migrations.
    "provisioning": 2,
    "developmentLeaseMonitor": 1,
    "serverBuildHistory": 1,
}
# Control, build and development reconciliation run independently. Migration-role
# connections are already counted in the per-role limits below, not again here.
CORE_TRANSIENT = sum(CORE_TRANSIENT_SOURCES.values())
DEV_SESSION_LIMIT = 16  # development.rs CREATE ROLE ... CONNECTION LIMIT 16
KEYCLOAK_DEFAULT_POOL = 100  # Keycloak db-pool-max-size default


def psql(container: str, database: str, sql: str) -> str:
    return subprocess.run(["docker", "exec", container, "psql", "-X", "-q", "-At", "-U", "postgres",
                           "-d", database, "-c", sql], check=True, capture_output=True, text=True).stdout.strip()


def identity_pool(container: str):
    probe = subprocess.run(["docker", "inspect", "-f", "{{range .Config.Env}}{{println .}}{{end}}", container],
                           capture_output=True, text=True)
    if probe.returncode != 0:
        return None, "identity container not found"
    for line in probe.stdout.splitlines():
        if line.startswith("KC_DB_POOL_MAX_SIZE="):
            try:
                size = int(line.split("=", 1)[1])
                if size > 0:
                    return size, "KC_DB_POOL_MAX_SIZE"
            except ValueError:
                pass
            return None, "invalid KC_DB_POOL_MAX_SIZE"
    return KEYCLOAK_DEFAULT_POOL, "Keycloak default (KC_DB_POOL_MAX_SIZE unset)"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--db-container", default="miy-v2-validation-postgres-1")
    parser.add_argument("--identity-container", default="miy-v2-validation-keycloak-1")
    args = parser.parse_args()
    settings = dict(line.split("|", 1) for line in psql(args.db_container, "postgres",
        "SELECT name || '|' || setting FROM pg_settings WHERE name IN "
        "('max_connections','superuser_reserved_connections','reserved_connections')").splitlines())
    usable = int(settings["max_connections"]) - int(settings.get("superuser_reserved_connections", 0)) \
        - int(settings.get("reserved_connections", 0))
    roles = json.loads(psql(args.db_container, "miy_core",
        "SELECT coalesce(json_agg(json_build_object('kind', k.kind, 'role', k.role, 'limit', r.rolconnlimit)), '[]') "
        "FROM (SELECT 'runtime' AS kind, runtime_role AS role FROM db_resources "
        "UNION ALL SELECT 'migration', migration_role FROM db_resources "
        "UNION ALL SELECT 'development_session', role_name FROM app_dev_sessions "
        "WHERE status IN ('queued','preparing','ready')) k JOIN pg_roles r ON r.rolname = k.role"))
    pool, pool_source = identity_pool(args.identity_container)
    usage = json.loads(psql(args.db_container, "postgres",
        "SELECT coalesce(json_object_agg(name, n), '{}') FROM (SELECT CASE "
        "WHEN application_name LIKE 'miy-%' THEN regexp_replace(application_name, '^(miy-app)-.*$', '\\1') "
        "WHEN usename LIKE 'miy\\_r\\_%' OR usename LIKE 'miy\\_m\\_%' THEN 'miy-app (no application_name)' "
        "WHEN datname = 'miy_core' THEN 'miy-core (no application_name)' "
        "WHEN datname = 'miy_identity' THEN 'identity' ELSE 'other' END AS name, count(*) AS n "
        "FROM pg_stat_activity WHERE backend_type = 'client backend' GROUP BY 1) s"))
    unbounded = sorted({r["kind"] for r in roles if r["limit"] < 0})
    by_kind = {}
    for row in roles:
        entry = by_kind.setdefault(row["kind"], {"roles": 0, "limitSum": 0, "unbounded": 0})
        entry["roles"] += 1
        if row["limit"] < 0:
            entry["unbounded"] += 1
        else:
            entry["limitSum"] += row["limit"]
    worst = CORE_POOLS + CORE_TRANSIENT + (pool or 0) + sum(v["limitSum"] for v in by_kind.values())
    problems = []
    if unbounded:
        problems.append("login roles without a connection limit: " + ", ".join(unbounded) +
                        " (check worker startup alignment or the next deployment)")
    if pool is None:
        problems.append("identity pool unknown")
    if worst > usable:
        problems.append(f"worst case {worst} exceeds usable {usable}")
    report = {
        "status": "FAIL" if problems else "PASS",
        "usableConnections": usable,
        "settings": settings,
        "worstCase": worst,
        "reserved": {"corePools": CORE_POOLS, "coreTransient": CORE_TRANSIENT,
                     "coreTransientSources": CORE_TRANSIENT_SOURCES,
                     "identityPool": pool, "identityPoolSource": pool_source, "roles": by_kind},
        "currentUse": usage,
        "problems": problems,
    }
    print(json.dumps(report, indent=2))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
