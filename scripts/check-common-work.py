#!/usr/bin/env python3
"""Accept the common-work business overlay against a disposable app database.

The real generated-template SDK and app HTTP handlers execute. Core directory
and Storage are local protocol fixtures; the Core permission suite tests their
real implementation separately. This does not deploy a platform or publish a
component. No production configuration or credentials are read.
"""
from __future__ import annotations

import argparse
import base64
import contextlib
import datetime as dt
import hashlib
import hmac
import http.client
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import signal
import socket
import subprocess
import tempfile
import threading
import time
from urllib.parse import parse_qs, urlsplit
import uuid

ROOT = Path(__file__).resolve().parents[1]


def uid() -> str:
    return str(uuid.uuid4())


def require(value: object, code: str) -> None:
    if not value:
        raise RuntimeError(code)


def run(args: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, **kwargs)


def jwt(secret: str, installation: str, release: str, actor: str, proof: str, environment: str = "dev") -> str:
    encode = lambda value: base64.urlsafe_b64encode(json.dumps(value, separators=(",", ":")).encode()).decode().rstrip("=")
    header = encode({"alg": "HS256", "typ": "JWT"})
    claims = encode({"sub": actor, "admin": False, "installation_id": installation,
                     "release_id": release, "iss": "miy", "aud": "miy-app", "exp": int(time.time()) + 30,
                     "environment": environment, "callers": [], "effective_permissions": ["platform:directory:read", "platform:files:read", "platform:files:write"], "delegation_token": proof})
    signature = base64.urlsafe_b64encode(hmac.new(secret.encode(), f"{header}.{claims}".encode(), hashlib.sha256).digest()).decode().rstrip("=")
    return f"{header}.{claims}.{signature}"


class CoreFixture:
    def __init__(self, installation: str, actors: list[str]):
        self.installation = installation
        self.proofs = {"miy_call_" + secrets.token_urlsafe(32): actor for actor in actors}
        self.users = {actor: {"id": actor, "name": actor, "primaryGroup": None} for actor in actors}
        self.group = uid()
        self.members = {actor: [] for actor in actors}
        self.files: dict[str, dict] = {}
        self.requests: dict[str, str] = {}
        self.bytes: dict[str, bytes] = {}
        self.attach_keys: dict[str, str] = {}
        self.lose_reserve = False
        self.lose_attach = False
        self.calls = 0
        self.lock = threading.RLock()

    def proof(self, actor: str) -> str:
        return next(proof for proof, owner in self.proofs.items() if owner == actor)

    def handler(self):
        fixture = self

        class Handler(BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def log_message(self, *_args):
                pass  # Never log identity headers, capabilities or response bodies.

            def answer(self, status: int, value: object, binary: bool = False, headers: dict | None = None):
                data = value if binary else json.dumps(value, ensure_ascii=False).encode()
                self.send_response(status)
                self.send_header("Content-Type", "application/octet-stream" if binary else "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.send_header("Connection", "close")
                for name, field in (headers or {}).items():
                    self.send_header(name, field)
                self.end_headers()
                self.wfile.write(data)

            def reject(self, status: int, code: str):
                self.answer(status, {"error": {"code": code}})

            def dispatch(self):
                actor = fixture.proofs.get(self.headers.get("Authorization", "").removeprefix("Bearer "))
                if not actor:
                    return self.reject(403, "fixture_authentication_required")
                parts = urlsplit(self.path)
                prefix = f"/api/v1/installations/{fixture.installation}/platform/"
                if not parts.path.startswith(prefix):
                    return self.reject(403, "fixture_installation_mismatch")
                tail = parts.path[len(prefix):]
                length = int(self.headers.get("Content-Length", "0"))
                if length > 10 * 1024 * 1024:
                    return self.reject(413, "fixture_body_too_large")
                data = self.rfile.read(length) if length else b""
                with fixture.lock:
                    fixture.calls += 1
                    if tail == "directory" and self.command == "GET":
                        query = parse_qs(parts.query)
                        kind = query.get("kind", ["users"])[0]
                        ids = query.get("ids", [""])[0].split(",") if "ids" in query else None
                        group = {"id": fixture.group, "name": "Fixture group", "source": "local"}
                        users = [value for key, value in fixture.users.items() if ids is None or key in ids]
                        groups = [group] if ids is None or fixture.group in ids else []
                        if kind == "memberships":
                            groups = [value for value in groups if value["id"] in fixture.members[actor]]
                        return self.answer(200, {"kind": kind, "subjectId": actor, "installationId": fixture.installation,
                                                 "environment": "dev", "users": users if kind == "users" else [],
                                                 "groups": groups if kind != "users" else [], "nextCursor": None})
                    if tail == "files" and self.command == "POST":
                        value = json.loads(data)
                        request = value["requestId"]
                        existing = fixture.requests.get(request)
                        if existing:
                            row = fixture.files[existing]
                            if any(row[key] != value[key] for key in ("name", "mime", "size")):
                                return self.reject(409, "file_request_conflict")
                        else:
                            file = uid()
                            row = {"id": file, "version": 1, "installationId": fixture.installation,
                                   "requestId": request, "name": value["name"], "mime": value["mime"], "size": value["size"],
                                   "sha256": None, "status": "pending", "reference": None, "expiresAt": None}
                            fixture.files[file] = row
                            fixture.requests[request] = file
                        if fixture.lose_reserve:
                            fixture.lose_reserve = False
                            return self.reject(503, "fixture_result_unknown")
                        return self.answer(200, row)
                    if tail.startswith("files/by-request/") and self.command == "GET":
                        file = fixture.requests.get(tail.rsplit("/", 1)[1])
                        if not file:
                            return self.reject(404, "not_found")
                        return self.answer(200, fixture.files[file])
                    route = tail.split("/")
                    if len(route) < 2 or route[0] != "files" or route[1] not in fixture.files:
                        return self.reject(404, "not_found")
                    file = route[1]
                    row = fixture.files[file]
                    if len(route) == 2 and self.command == "GET":
                        return self.answer(200, row)
                    if len(route) == 2 and self.command == "DELETE":
                        if row["status"] == "attached":
                            return self.reject(409, "file_attached")
                        row["status"] = "cancelled"
                        fixture.bytes.pop(file, None)
                        return self.answer(200, row)
                    if route[2:] == ["content"] and self.command == "PUT":
                        if row["status"] != "pending":
                            return self.reject(409, "file_upload_state_changed")
                        if len(data) != row["size"]:
                            return self.reject(400, "file_size_mismatch")
                        fixture.bytes[file] = data
                        row.update(status="ready", sha256=hashlib.sha256(data).hexdigest())
                        return self.answer(200, row)
                    if route[2:] == ["content"] and self.command == "GET":
                        if row["status"] not in ("ready", "attached"):
                            return self.reject(404, "not_found")
                        return self.answer(200, fixture.bytes[file], True, {"Content-Disposition": "attachment; filename=fixture.bin",
                            "X-Miy-File-Id": file, "X-Miy-File-Version": "1", "X-Miy-File-Sha256": row["sha256"],
                            "Set-Cookie": "fixture=must-not-forward", "Clear-Site-Data": '"cookies"', "X-Miy-Private": "must-not-forward"})
                    if route[2:] == ["attach"] and self.command == "POST":
                        value = json.loads(data)
                        if row["status"] not in ("ready", "attached"):
                            return self.reject(409, "file_not_ready")
                        if row["status"] == "attached" and (row["reference"] != value["reference"] or fixture.attach_keys[file] != value["requestId"]):
                            return self.reject(409, "file_reference_conflict")
                        row.update(status="attached", reference=value["reference"])
                        fixture.attach_keys[file] = value["requestId"]
                        if fixture.lose_attach:
                            fixture.lose_attach = False
                            return self.reject(503, "fixture_result_unknown")
                        return self.answer(200, row)
                    return self.reject(404, "not_found")

            do_GET = dispatch
            do_POST = dispatch
            do_PUT = dispatch
            do_DELETE = dispatch

        return Handler


def assemble(work: Path) -> Path:
    app = work / "app"
    shutil.copytree(ROOT / "templates/rust-react", app)
    for path in app.rglob("*"):
        if path.is_file():
            try:
                content = path.read_text()
            except UnicodeDecodeError:
                continue
            path.write_text(content.replace("{{APP_ID}}", "common-work").replace("{{APP_MODULE}}", "common_work").replace("{{APP_NAME}}", "프로젝트와 업무"))
    for path in (ROOT / "examples/common-work").rglob("*"):
        if path.is_file():
            destination = app / path.relative_to(ROOT / "examples/common-work")
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, destination)
    metadata = json.loads((ROOT / "registry/components/ui/1.1.0/component.json").read_text())
    target = app / "platform-components/ui"
    target.mkdir(parents=True)
    for relative in metadata["files"]:
        path = Path(relative)
        require(not path.is_absolute() and ".." not in path.parts, "invalid_candidate_component_path")
        (target / path).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / "registry/components/ui/1.1.0" / path, target / path)
    # The product worker authenticates as the migration login, then switches to
    # the app's non-login owner before SQLx creates history and applies SQL.
    helper = app / "src/bin/common_work_migrate.rs"
    helper.parent.mkdir(parents=True)
    helper.write_text('''use sqlx::{Connection, Executor};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut connection = sqlx::PgConnection::connect(&std::env::var("DATABASE_URL")?).await?;
    connection.execute("SET ROLE fixture_owner").await?;
    connection.execute("SET search_path=app").await?;
    let migrator = sqlx::migrate::Migrator::new(std::path::Path::new("migrations")).await?;
    let applied = migrator.run(&mut connection).await;
    let history: bool = sqlx::query_scalar("SELECT to_regclass('app._sqlx_migrations') IS NOT NULL").fetch_one(&mut connection).await?;
    if history {
        connection.execute("REVOKE ALL ON TABLE app._sqlx_migrations FROM fixture_runtime").await?;
    }
    applied?;
    Ok(())
}
''')
    return app


def stop_process(process: subprocess.Popen) -> None:
    if process.poll() is not None:
        return
    os.killpg(process.pid, signal.SIGTERM)
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.wait(timeout=5)


def interrupted(signum, _frame):
    raise RuntimeError(f"validation_interrupted_{signum}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", default="postgres:18.3")
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--build-only", action="store_true", help="Assemble and compile the candidate without starting a database")
    parser.add_argument("--keep-project", action="store_true", help="Retain only assembled source under evidence/project for a separate UI build")
    args = parser.parse_args()
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    evidence = (args.evidence or ROOT / ".local/evidence" / f"common-work-{stamp}").resolve()
    evidence.mkdir(parents=True, exist_ok=True)
    name = f"miy-common-work-{secrets.token_hex(6)}"
    password, secret = secrets.token_urlsafe(36), secrets.token_urlsafe(36)
    migration_password, runtime_password = secrets.token_urlsafe(36), secrets.token_urlsafe(36)
    installation, release = uid(), uid()
    actors = ["fixture-owner", "fixture-member"]
    fixture = CoreFixture(installation, actors)
    result = {"status": "FAIL", "scope": "real common-work HTTP app and PostgreSQL; Core directory/Storage fixture",
              "productionTouched": False, "checks": [], "notRun": ["real Core Storage (separate permission suite)", "browser UI", "CLI scaffold/registry network", "production deployment"]}
    result["runnerSha256"] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    logs: dict[str, str] = {}
    created = False
    process = None
    core = None
    started = time.monotonic()
    for signum in (signal.SIGINT, signal.SIGTERM):
        signal.signal(signum, interrupted)

    def check(name: str):
        result["checks"].append(name)

    try:
        with contextlib.ExitStack() as cleanup_stack:
            work = Path(cleanup_stack.enter_context(tempfile.TemporaryDirectory(prefix="miy-common-work-")))
            app = assemble(work)
            if args.keep_project:
                kept = evidence / "project"
                require(not kept.exists(), "evidence_project_already_exists")
                cleanup_stack.callback(shutil.copytree, app, kept, ignore=shutil.ignore_patterns("target", "node_modules", "dist", ".miy"))
                result["project"] = str(kept)
            result["sources"] = {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
                for source in [ROOT / "templates/rust-react", ROOT / "examples/common-work", ROOT / "registry/components/ui/1.1.0"]
                for path in sorted(source.rglob("*")) if path.is_file() and "target" not in path.parts and "node_modules" not in path.parts}
            cargo = shutil.which("cargo") or str(Path.home() / ".cargo/bin/cargo")
            environment = os.environ.copy()
            environment.update(CARGO_TARGET_DIR=str(ROOT / "target"), CARGO_BUILD_JOBS="2")
            compiled = run([cargo, "build", "--offline", "--manifest-path", str(app / "Cargo.toml")], cwd=app, env=environment, timeout=600)
            logs["cargo.log"] = compiled.stdout
            require(compiled.returncode == 0, "common_work_compile_failed")
            binary = work / "common-work"
            shutil.copyfile(ROOT / "target/debug/miy-app", binary)
            binary.chmod(0o700)
            result["binarySha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
            check("template SDK + business overlay + UI registry candidate assembled; real Rust app compiled")
            if args.build_only:
                result["status"] = "PASS"
                result["notRun"].append("application HTTP and PostgreSQL (--build-only)")
            else:
                envfile = work / "postgres.env"
                envfile.write_text(f"POSTGRES_PASSWORD={password}\nPOSTGRES_DB=common_work\n")
                envfile.chmod(0o600)
                require(run(["docker", "image", "inspect", args.image], timeout=10).returncode == 0, "fixture_database_image_missing")
                created_result = run(["docker", "run", "-d", "--rm", "--name", name, "--label", f"org.miy.common-work-test={name}",
                    "--memory", "256m", "--cpus", "1", "--pids-limit", "128", "--tmpfs", "/var/lib/postgresql:rw,nosuid,size=128m",
                    "--publish", "127.0.0.1::5432", "--env-file", str(envfile), args.image,
                    "postgres", "-c", "shared_buffers=32MB", "-c", "max_connections=20", "-c", "work_mem=1MB"], timeout=60)
                require(created_result.returncode == 0, "fixture_database_start_failed")
                created = True
                envfile.unlink()
                # The image first runs a temporary server on its Unix socket and restarts;
                # only the final server answers on TCP, so readiness is asked there.
                deadline = time.monotonic() + 45
                while run(["docker", "exec", name, "pg_isready", "-q", "-h", "127.0.0.1", "-U", "postgres", "-d", "common_work"], timeout=5).returncode:
                    require(time.monotonic() < deadline, "fixture_database_timeout")
                    time.sleep(0.2)
                published = run(["docker", "port", name, "5432/tcp"], timeout=10).stdout.strip()
                require(re.fullmatch(r"127\.0\.0\.1:\d+", published), "fixture_database_port_invalid")
                dbport = int(published.rsplit(":", 1)[1])

                def sql(statement: str, database: str = "common_work") -> str:
                    reply = run(["docker", "exec", "-i", name, "psql", "-XAt", "-v", "ON_ERROR_STOP=1", "-U", "postgres", "-d", database], input=statement, timeout=20)
                    require(reply.returncode == 0, "fixture_sql_failed")
                    return reply.stdout.strip()

                sql(f"""CREATE ROLE fixture_owner NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION;
CREATE ROLE fixture_migration LOGIN PASSWORD '{migration_password}' NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION CONNECTION LIMIT 2;
CREATE ROLE fixture_runtime LOGIN PASSWORD '{runtime_password}' NOSUPERUSER NOCREATEDB NOCREATEROLE NOREPLICATION CONNECTION LIMIT 6;
GRANT fixture_owner TO fixture_migration;
REVOKE ALL ON DATABASE common_work FROM PUBLIC;
GRANT CONNECT ON DATABASE common_work TO fixture_migration,fixture_runtime;
REVOKE ALL ON SCHEMA public FROM PUBLIC;
CREATE SCHEMA app AUTHORIZATION fixture_owner;
GRANT USAGE ON SCHEMA app TO fixture_runtime;
ALTER DEFAULT PRIVILEGES FOR ROLE fixture_owner IN SCHEMA app GRANT SELECT,INSERT,UPDATE,DELETE ON TABLES TO fixture_runtime;
ALTER DEFAULT PRIVILEGES FOR ROLE fixture_owner IN SCHEMA app GRANT USAGE,SELECT ON SEQUENCES TO fixture_runtime;
ALTER ROLE fixture_migration IN DATABASE common_work SET search_path=app;
ALTER ROLE fixture_runtime IN DATABASE common_work SET search_path=app;
""")
                require(sql("SELECT has_database_privilege('fixture_owner','common_work','CREATE');") == "f", "migration_owner_has_database_create")
                migration_environment = environment.copy()
                migration_environment["DATABASE_URL"] = f"postgres://fixture_migration:{migration_password}@127.0.0.1:{dbport}/common_work"
                migrated = run([str(ROOT / "target/debug/common_work_migrate")], cwd=app, env=migration_environment, timeout=45)
                logs["migrations.log"] = migrated.stdout
                require(migrated.returncode == 0, "least_privilege_migration_failed")
                require(sql("SELECT count(*) FROM app._sqlx_migrations WHERE success;") == "3", "sqlx_migration_history_incomplete")
                check("actual SQLx migrations run as non-login app owner without database CREATE permission")
                for statement in ["CREATE TABLE app.forbidden_ddl(id integer);", "CREATE SCHEMA forbidden_schema;",
                    "DROP TABLE app.projects;", "SELECT * FROM app._sqlx_migrations;",
                    "UPDATE app._sqlx_migrations SET success=false;", "SET ROLE fixture_owner;"]:
                    denied = run(["docker", "exec", "-i", name, "psql", "-XAt", "-v", "ON_ERROR_STOP=1", "-U", "fixture_runtime", "-d", "common_work"], input=statement, timeout=10)
                    require(denied.returncode != 0 and any(marker in denied.stdout.lower() for marker in ("permission denied", "must be owner")), "runtime_ddl_or_history_access_not_denied")
                result["databaseRoles"] = {"migration": "login then SET ROLE non-login owner; search_path=app", "runtime": "separate login with app CRUD and sequence privileges only", "runtimeHistoryAccess": "denied", "runtimeDdl": "denied"}
                check("runtime cannot create/drop schemas or tables, read/change migration history, or SET ROLE owner")
                core = ThreadingHTTPServer(("127.0.0.1", 0), fixture.handler())
                core.daemon_threads = True
                threading.Thread(target=core.serve_forever, daemon=True).start()
                with socket.socket() as reserved:
                    reserved.bind(("127.0.0.1", 0))
                    appport = reserved.getsockname()[1]
                environment.update(DATABASE_URL=f"postgres://fixture_runtime:{runtime_password}@127.0.0.1:{dbport}/common_work", MIY_GATEWAY_SECRET=secret,
                    MIY_INSTALLATION_ID=installation, MIY_RELEASE_ID=release, MIY_BIND_ADDR=f"127.0.0.1:{appport}",
                    MIY_CORE_ORIGIN=f"http://127.0.0.1:{core.server_port}", MIY_CORE_ALLOW_HTTP="1")
                environment.pop("MIY_PREVIEW_PLATFORM_ORIGIN", None)
                environment.pop("MIY_PREVIEW_PLATFORM_CAPABILITY", None)
                with (work / "app.log").open("w") as app_log:
                    process = subprocess.Popen([str(binary)], cwd=app, env=environment, stdout=app_log, stderr=subprocess.STDOUT, start_new_session=True)
                    cleanup_stack.callback(lambda: logs.update({"app.log": (work / "app.log").read_text()}))
                    cleanup_stack.callback(stop_process, process)

                    def request(actor: str | None, method: str, path: str, value: object = None, expected: int = 200, *, raw: bytes | None = None, env: str = "dev", inst: str | None = None):
                        connection = http.client.HTTPConnection("127.0.0.1", appport, timeout=25)
                        headers = {}
                        if actor:
                            headers["X-Miy-Identity"] = jwt(secret, inst or installation, release, actor, fixture.proof(actor), env)
                        data = raw
                        if value is not None:
                            data = json.dumps(value, ensure_ascii=False).encode()
                            headers["Content-Type"] = "application/json"
                        connection.request(method, path, body=data, headers=headers)
                        reply = connection.getresponse()
                        body = reply.read(11 * 1024 * 1024)
                        status, response_headers = reply.status, dict((k.lower(), v) for k, v in reply.getheaders())
                        connection.close()
                        require(status == expected, f"app_http_{method}_{path}_expected_{expected}_received_{status}")
                        return (body if raw is not None or "/content" in path else json.loads(body)), response_headers

                    deadline = time.monotonic() + 25
                    while True:
                        require(process.poll() is None, "common_work_app_exited")
                        try:
                            request(None, "GET", "/health")
                            break
                        except OSError:
                            require(time.monotonic() < deadline, "common_work_app_readiness_timeout")
                            time.sleep(0.1)
                    owner, member = actors
                    project, task = uid(), uid()
                    base = f"/api/projects/{project}"
                    request(None, "GET", "/api/projects", expected=401)
                    request(owner, "POST", "/api/projects", {"id": project, "title": "공통 기능 검토"})
                    require(request(member, "GET", "/api/projects")[0]["projects"] == [], "unshared_project_list_leaked")
                    request(member, "GET", base, expected=403)
                    request(owner, "POST", base + "/tasks", {"id": task, "title": "검토 업무"})
                    request(member, "GET", base + "/tasks", expected=403)
                    check("unsigned backend requests and unshared list/detail/task are denied")

                    def share(extra: list[dict]):
                        snapshot = request(owner, "GET", base + "/sharing")[0]
                        request_id = uid()
                        entries = [{"subject": {"kind": "user", "id": owner}, "role": "owner"}] + extra
                        value = {"expectedRevision": snapshot["revision"], "requestId": request_id, "entries": entries}
                        changed = request(owner, "PUT", base + "/sharing", value)[0]
                        require(request(owner, "GET", base + f"/sharing/results/{request_id}")[0] == changed, "sharing_result_lookup_changed")
                        return value

                    share([{"subject": {"kind": "user", "id": member}, "role": "viewer"}])
                    require(len(request(member, "GET", "/api/projects")[0]["projects"]) == 1, "shared_project_missing")
                    request(member, "GET", base)
                    request(member, "GET", base + f"/tasks/{task}")
                    request(member, "PATCH", base + f"/tasks/{task}", {"completed": True}, 403)
                    share([{"subject": {"kind": "user", "id": member}, "role": "editor"}])
                    request(member, "PATCH", base + f"/tasks/{task}", {"completed": True})
                    check("viewer/editor ACL actions and sharing result lookup are enforced by app HTTP")

                    # Above the ordinary JSON gateway's unchanged 4 MiB limit.
                    payload = bytes(range(256)) * (5 * 1024 * 1024 // 256)
                    reserve_id = uid()
                    value = {"requestId": reserve_id, "name": "검토.bin", "mime": "application/octet-stream", "size": len(payload)}
                    fixture.lose_reserve = True
                    request(owner, "POST", base + "/files", value, 503)
                    metadata = request(owner, "GET", base + f"/files/by-request/{reserve_id}")[0]
                    file = metadata["id"]
                    file_path = base + f"/files/{file}"
                    content_path = f"/api/files/projects/{project}/{file}/content"
                    before = len(fixture.files)
                    same = request(owner, "POST", base + "/files", value)[0]
                    require(same["id"] == file and len(fixture.files) == before, "reservation_retry_duplicated_file")
                    request(member, "GET", file_path, expected=403)
                    request(owner, "PUT", content_path, raw=payload)
                    attach_id = uid()
                    attachment = {"reference": project, "requestId": attach_id}
                    fixture.lose_attach = True
                    request(owner, "POST", file_path + "/attach", attachment, 503)
                    require(sql(f"SELECT attached FROM app.attachments WHERE request_id='{reserve_id}'") == "t", "lost_attach_response_erased_app_reference")
                    request(owner, "DELETE", file_path, expected=409)
                    request(owner, "POST", file_path + "/attach", attachment)
                    downloaded, headers = request(member, "GET", content_path)
                    require(hashlib.sha256(downloaded).hexdigest() == hashlib.sha256(payload).hexdigest(), "download_hash_mismatch")
                    require(not any(name in headers for name in ("set-cookie", "clear-site-data", "x-miy-private")), "file_headers_leaked")
                    require(headers.get("content-disposition", "").startswith("attachment") and headers.get("x-content-type-options") == "nosniff", "download_security_headers_missing")
                    require(len(request(member, "GET", base + "/files")[0]) == 1, "attached_file_missing")
                    check("reservation response loss recovers by request ID; 5 MiB streamed upload/download hashes and header allowlist match")
                    check("committed app reference survives uncertain Core attach and blocks compensating delete")

                    cancel = request(owner, "POST", base + "/files", {"requestId": uid(), "name": "cancel.bin", "mime": "application/octet-stream", "size": 1})[0]
                    cancel_path = base + f"/files/{cancel['id']}"
                    require(request(owner, "DELETE", cancel_path)[0]["status"] == "cancelled", "explicit_pending_cancel_failed")
                    share([])
                    require(request(member, "GET", "/api/projects")[0]["projects"] == [], "revoked_project_list_leaked")
                    for path in [base, base + "/tasks", base + f"/tasks/{task}", base + "/files", file_path, content_path]:
                        request(member, "GET", path, expected=403)
                    request(member, "PATCH", base + f"/tasks/{task}", {"completed": False}, 403)
                    check("sharing revocation immediately removes list/detail/task/file reads and writes")
                    with fixture.lock:
                        fixture.members[member] = [fixture.group]
                    share([{"subject": {"kind": "group", "id": fixture.group}, "role": "viewer"}])
                    request(member, "GET", base)
                    with fixture.lock:
                        fixture.members[member] = []
                    request(member, "GET", base, expected=403)
                    check("fresh platform membership revocation invalidates group-based app sharing")
                    request(owner, "GET", base, expected=403, env="prod")
                    request(owner, "GET", base, expected=401, inst=uid())
                    check("environment-scoped ACL and signed installation binding reject scope mismatch")

                    dump = run(["docker", "exec", name, "pg_dump", "-U", "postgres", "-d", "common_work", "--schema=app", "--no-owner", "--no-privileges"], timeout=30)
                    require(dump.returncode == 0 and "CREATE TABLE" in dump.stdout, "app_database_dump_failed")
                    sql("CREATE DATABASE common_work_restore;", "postgres")
                    sql(dump.stdout, "common_work_restore")
                    expected = sql("SELECT count(*) FROM app.attachments WHERE attached;")
                    restored = sql("SELECT count(*) FROM app.attachments WHERE attached;", "common_work_restore")
                    require(expected == restored == "1", "attachment_reference_restore_count_mismatch")
                    reference_sql = f"SELECT project_id::text||':'||file_id::text||':'||attach_request::text FROM app.attachments WHERE request_id='{reserve_id}';"
                    require(sql(reference_sql) == sql(reference_sql, "common_work_restore"), "attachment_reference_restore_mismatch")
                    require(sql("SELECT count(*) FROM app.miy_acl_resources;") == sql("SELECT count(*) FROM app.miy_acl_resources;", "common_work_restore"), "app_acl_restore_mismatch")
                    result["databaseDumpSha256"] = hashlib.sha256(dump.stdout.encode()).hexdigest()
                    check("independent database restore retains exact project/file/attach request references and ACL rows")
                    result["status"] = "PASS"
                logs["app.log"] = (work / "app.log").read_text()
    except (RuntimeError, OSError, subprocess.SubprocessError, ValueError, http.client.HTTPException) as exc:
        result["failure"] = type(exc).__name__ + ": " + str(exc)
    finally:
        if process is not None:
            stop_process(process)
        if core is not None:
            core.shutdown()
            core.server_close()
        cleanup = True
        if created:
            inspected = run(["docker", "inspect", "--format", '{{index .Config.Labels "org.miy.common-work-test"}}', name], timeout=10)
            cleanup = inspected.returncode == 0 and inspected.stdout.strip() == name and run(["docker", "rm", "-f", name], timeout=30).returncode == 0
        result["cleanupComplete"] = cleanup
        if not cleanup:
            result["status"] = "FAIL"
        result["durationSeconds"] = round(time.monotonic() - started, 2)
        def redact(value: str) -> str:
            for private in [password, secret, migration_password, runtime_password, *fixture.proofs]:
                value = value.replace(private, "[redacted]")
            value = re.sub(r"postgres(?:ql)?://[^\s\"']+", "[redacted-database-url]", value)
            return re.sub(r"eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+", "[redacted-jwt]", value)
        for file, text in logs.items():
            (evidence / file).write_text(redact(text))
        (evidence / "result.json").write_text(redact(json.dumps(result, ensure_ascii=False, indent=2)) + "\n")
        print(f"{result['status']} common-work: {evidence / 'result.json'}")
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
