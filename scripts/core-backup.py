#!/usr/bin/env python3
"""Minimal pre-change backup of a platform database and a restore check in a separate area.

create  Dump one database from the running PostgreSQL container without stopping it. The dump and
        the per-table row counts come from one exported REPEATABLE READ snapshot, so they describe
        the same moment. Output: <out>/<database>.dump and manifest.json (both 0600, dir 0700).
verify  Restore the dump into a new disposable PostgreSQL container (tmpfs, unique label, no
        published port unless --migrate-with), compare migration version and every row count with
        the manifest, optionally apply the given binary's core migrations to the restored copy as a
        rehearsal, and always remove only the container it created.

The source database is only read. Secrets are never printed; the dump file itself is private.
This is the minimal change-scope protection of PLAN step 1, not the full backup product (core
hardening phase 4): roles/globals, app databases, files and keys are outside its scope.
"""
import argparse
import hashlib
import json
import os
import re
import secrets
import socket
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LABEL = "miy.core-backup-verify"
IDENT = re.compile(r"^[a-z_][a-z0-9_]{0,62}$")
COUNTS_SQL = (
    "SELECT json_build_object("
    "'migrationVersion', (SELECT max(version) FROM _sqlx_migrations),"
    "'serverVersion', current_setting('server_version'),"
    "'tables', (SELECT coalesce(json_object_agg(t.table_name, "
    "(xpath('/row/c/text()', query_to_xml(format('SELECT count(*) AS c FROM %I.%I', t.table_schema, t.table_name), false, true, '')))[1]::text::bigint"
    " ORDER BY t.table_name), '{}'::json) FROM information_schema.tables t "
    "WHERE t.table_schema='public' AND t.table_type='BASE TABLE'))"
)


def compose_postgres_image() -> str:
    match = re.search(r"image:\s*(postgres:[^\s]+)", (ROOT / "compose.yaml").read_text())
    if not match:
        raise SystemExit("compose.yaml has no postgres image")
    return match.group(1)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def run(args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


class Session:
    """One psql session kept open so a snapshot stays exported while pg_dump reads it."""

    def __init__(self, container: str, database: str):
        self.proc = subprocess.Popen(
            ["docker", "exec", "-i", container, "psql", "-X", "-q", "-At", "-v", "ON_ERROR_STOP=1",
             "-U", "postgres", "-d", database],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

    def send(self, sql: str) -> None:
        assert self.proc.stdin
        self.proc.stdin.write(sql.rstrip(";") + ";\n")
        self.proc.stdin.flush()

    def one(self, sql: str) -> str:
        assert self.proc.stdout
        self.send(sql)
        line = self.proc.stdout.readline()
        if not line:
            raise SystemExit("psql session ended unexpectedly")
        return line.strip()

    def close(self):
        if self.proc.stdin:
            try:
                self.proc.stdin.write("COMMIT;\n")
                self.proc.stdin.close()
            except BrokenPipeError:
                pass
        self.proc.wait(timeout=30)


def create(args) -> int:
    if not IDENT.match(args.database):
        raise SystemExit("invalid database name")
    out = Path(args.out).resolve()
    out.mkdir(mode=0o700, parents=True, exist_ok=False)
    dump = out / f"{args.database}.dump"
    session = Session(args.db_container, args.database)
    session.send("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")  # prints nothing with -q
    snapshot = session.one("SELECT pg_export_snapshot()")
    if not re.fullmatch(r"[0-9A-F-]+", snapshot):
        raise SystemExit("could not export a snapshot")
    fd = os.open(dump, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "wb") as handle:
        run(["docker", "exec", args.db_container, "pg_dump", "-U", "postgres", "-Fc",
             f"--snapshot={snapshot}", args.database], stdout=handle)
    state = json.loads(session.one(COUNTS_SQL))
    session.close()
    with dump.open("rb") as handle:
        listing = subprocess.run(["docker", "exec", "-i", args.db_container, "pg_restore", "--list"],
                                 stdin=handle, capture_output=True)
    if listing.returncode != 0 or b"TABLE DATA" not in listing.stdout:
        raise SystemExit("dump is not readable by pg_restore")
    manifest = {
        "schemaVersion": 1,
        "createdAt": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "database": args.database,
        "sourceContainer": args.db_container,
        "consistency": "pg_dump and row counts share one exported REPEATABLE READ snapshot",
        "serverVersion": state["serverVersion"],
        "migrationVersion": state["migrationVersion"],
        "dump": {"file": dump.name, "bytes": dump.stat().st_size, "sha256": sha256(dump)},
        "tables": state["tables"],
    }
    fd = os.open(out / "manifest.json", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as handle:
        json.dump(manifest, handle, indent=2)
        handle.write("\n")
    print(json.dumps({"status": "CREATED", "out": str(out), "bytes": manifest["dump"]["bytes"],
                      "migrationVersion": manifest["migrationVersion"],
                      "tables": len(manifest["tables"]), "rows": sum(manifest["tables"].values())}))
    return 0


def free_port() -> int:
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def verify(args) -> int:
    source = Path(args.backup).resolve()
    manifest = json.loads((source / "manifest.json").read_text())
    dump = source / manifest["dump"]["file"]
    if sha256(dump) != manifest["dump"]["sha256"]:
        raise SystemExit("dump hash differs from manifest")
    token = secrets.token_hex(8)
    password = secrets.token_hex(16)
    port = free_port() if args.migrate_with else None
    command = ["docker", "run", "-d", "--label", f"{LABEL}={token}", "--tmpfs", "/var/lib/postgresql:rw",
               "-e", f"POSTGRES_PASSWORD={password}"]
    if port:
        command += ["-p", f"127.0.0.1:{port}:5432"]
    container = run(command + [args.image], capture_output=True, text=True).stdout.strip()
    result = {"status": "FAIL", "container": container[:12], "image": args.image}
    try:
        for _ in range(60):
            if subprocess.run(["docker", "exec", container, "pg_isready", "-U", "postgres", "-h", "127.0.0.1"],
                              capture_output=True).returncode == 0:
                break
            time.sleep(1)
        else:
            raise SystemExit("disposable PostgreSQL did not start")
        run(["docker", "exec", container, "createdb", "-U", "postgres", "verify"], capture_output=True)
        with dump.open("rb") as handle:
            run(["docker", "exec", "-i", container, "pg_restore", "-U", "postgres", "-d", "verify",
                 "--no-owner", "--no-acl", "--exit-on-error"], stdin=handle, capture_output=True)
        restored = json.loads(run(["docker", "exec", container, "psql", "-X", "-q", "-At", "-U", "postgres",
                                   "-d", "verify", "-c", COUNTS_SQL], capture_output=True, text=True).stdout)
        mismatched = sorted(name for name in set(manifest["tables"]) | set(restored["tables"])
                            if manifest["tables"].get(name) != restored["tables"].get(name))
        result.update({"migrationVersion": restored["migrationVersion"], "tables": len(restored["tables"]),
                       "rows": sum(restored["tables"].values()), "mismatchedTables": mismatched})
        ok = not mismatched and restored["migrationVersion"] == manifest["migrationVersion"]
        if ok and args.migrate_with:
            env = {"PATH": os.environ.get("PATH", ""),
                   "DATABASE_URL": f"postgres://postgres:{password}@127.0.0.1:{port}/verify"}
            migrate = subprocess.run([str(Path(args.migrate_with).resolve()), "migrate"], env=env,
                                     capture_output=True, text=True)
            after = run(["docker", "exec", container, "psql", "-X", "-q", "-At", "-U", "postgres", "-d", "verify",
                         "-c", "SELECT max(version) FROM _sqlx_migrations"], capture_output=True, text=True).stdout.strip()
            result["rehearsal"] = {"exitCode": migrate.returncode, "migrationVersionAfter": int(after)}
            ok = migrate.returncode == 0
        result["status"] = "PASS" if ok else "FAIL"
    finally:
        labels = subprocess.run(["docker", "inspect", "-f", f'{{{{ index .Config.Labels "{LABEL}" }}}}', container],
                                capture_output=True, text=True).stdout.strip()
        if labels == token:
            subprocess.run(["docker", "rm", "-f", "-v", container], capture_output=True)
        result["containerRemoved"] = subprocess.run(["docker", "inspect", container],
                                                    capture_output=True).returncode != 0
    print(json.dumps(result))
    return 0 if result["status"] == "PASS" and result["containerRemoved"] else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    c = sub.add_parser("create")
    c.add_argument("--db-container", default=os.environ.get("MIY_DATABASE_CONTAINER", "miy-v2-validation-postgres-1"))
    c.add_argument("--database", default="miy_core")
    c.add_argument("--out", required=True)
    v = sub.add_parser("verify")
    v.add_argument("backup")
    v.add_argument("--image", default=None)
    v.add_argument("--migrate-with", help="server binary whose core migrations are rehearsed on the restored copy")
    args = parser.parse_args()
    if args.command == "create":
        return create(args)
    args.image = args.image or compose_postgres_image()
    return verify(args)


if __name__ == "__main__":
    sys.exit(main())
