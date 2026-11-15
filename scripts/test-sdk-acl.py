#!/usr/bin/env python3
"""Exercise the app-owned ACL SQL against an owned disposable PostgreSQL.

Core directory responses are a local HTTP fixture; Core directory permission
checks have their own server regressions. No production config is read. All DB
credentials remain in memory/private temporary files, never in test evidence.
"""
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import socket
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def run(args: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, **kwargs)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", default="postgres:18.3")
    parser.add_argument("--evidence", type=Path)
    args = parser.parse_args()
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    evidence = (args.evidence or ROOT / ".local/evidence" / f"sdk-acl-{stamp}").resolve()
    evidence.mkdir(parents=True, exist_ok=True)
    name = f"miy-sdk-acl-{secrets.token_hex(6)}"
    password = secrets.token_urlsafe(36)
    started = time.monotonic()
    result: dict[str, object] = {"status": "FAIL", "scope": "app-owned PostgreSQL ACL with local Core directory fixture", "productionTouched": False, "image": args.image}
    output = ""
    created = False
    try:
        with tempfile.TemporaryDirectory(prefix="miy-sdk-acl-") as work:
            work = Path(work)
            envfile = work / "postgres.env"
            envfile.write_text(f"POSTGRES_PASSWORD={password}\nPOSTGRES_DB=acl_test\n")
            envfile.chmod(0o600)
            command = ["docker", "run", "-d", "--rm", "--name", name,
                       "--label", f"org.miy.sdk-acl-test={name}",
                       "--memory", "256m", "--cpus", "1", "--pids-limit", "128",
                       "--tmpfs", "/var/lib/postgresql:rw,nosuid,size=128m",
                       "--publish", "127.0.0.1::5432", "--env-file", str(envfile),
                       args.image, "postgres", "-c", "shared_buffers=32MB", "-c", "max_connections=20", "-c", "work_mem=1MB"]
            created = True
            created_result = run(command, timeout=60)
            if created_result.returncode:
                raise RuntimeError("isolated_database_start_failed")
            envfile.unlink()
            # The image first runs a temporary server on its Unix socket and restarts;
            # only the final server answers on TCP, so readiness is asked there.
            deadline = time.monotonic() + 45
            while run(["docker", "exec", name, "pg_isready", "-q", "-h", "127.0.0.1", "-U", "postgres", "-d", "acl_test"], timeout=5).returncode:
                if time.monotonic() >= deadline:
                    raise RuntimeError("isolated_database_readiness_timeout")
                time.sleep(0.25)
            port_output = run(["docker", "port", name, "5432/tcp"], timeout=10)
            if port_output.returncode or not re.fullmatch(r"127\.0\.0\.1:\d+\s*", port_output.stdout):
                raise RuntimeError("isolated_database_port_unavailable")
            port = int(port_output.stdout.strip().rsplit(":", 1)[1])
            sdk = work / "sdk"
            shutil.copytree(ROOT / "templates/rust-react/sdk/rust", sdk)
            with (sdk / "Cargo.toml").open("a") as file:
                file.write("\n[workspace]\n")
            shutil.copyfile(ROOT / "templates/rust-react/Cargo.lock", sdk / "Cargo.lock")
            with socket.socket() as sock:
                sock.bind(("127.0.0.1", 0))
                core_port = sock.getsockname()[1]
            env = os.environ.copy()
            env.update({"MIY_ACL_TEST_DATABASE_URL": f"postgres://postgres:{password}@127.0.0.1:{port}/acl_test",
                        "MIY_CORE_ORIGIN": f"http://127.0.0.1:{core_port}", "MIY_CORE_ALLOW_HTTP": "1",
                        "CARGO_TARGET_DIR": str(ROOT / "target"), "CARGO_BUILD_JOBS": "2"})
            # A caller's unrelated preview capability must not enter this fixture.
            env.pop("MIY_PREVIEW_PLATFORM_ORIGIN", None)
            env.pop("MIY_PREVIEW_PLATFORM_CAPABILITY", None)
            cargo = shutil.which("cargo") or str(Path.home() / ".cargo/bin/cargo")
            test_args = [cargo, "test", "--offline", "--manifest-path", str(sdk / "Cargo.toml"),
                         "postgres_acl_end_to_end", "--", "--ignored", "--nocapture", "--test-threads=1"]
            result["sources"] = {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
                                  for path in sorted((ROOT / "templates/rust-react/sdk/rust").rglob("*")) if path.is_file()}
            tested = run(test_args, env=env, cwd=ROOT, timeout=300)
            output = tested.stdout
            result["status"] = "PASS" if tested.returncode == 0 and "ACL_POSTGRES_PASS:" in output else "FAIL"
            result["exitCode"] = tested.returncode
            result["checks"] = next((line.split("ACL_POSTGRES_PASS: ", 1)[1].split("; ") for line in output.splitlines() if "ACL_POSTGRES_PASS: " in line), [])
    except (RuntimeError, OSError, subprocess.SubprocessError) as exc:
        result["failure"] = type(exc).__name__ + ": " + str(exc)
    finally:
        if created:
            # Only remove the exact container this process created and labelled.
            try:
                inspected = run(["docker", "inspect", "--format", '{{index .Config.Labels "org.miy.sdk-acl-test"}}', name], timeout=10)
                if inspected.returncode == 0 and inspected.stdout.strip() == name:
                    result["cleanupComplete"] = run(["docker", "rm", "-f", name], timeout=30).returncode == 0
                elif inspected.returncode and "No such" in inspected.stdout:
                    result["cleanupComplete"] = True
                else:
                    result["cleanupComplete"] = False
            except (OSError, subprocess.SubprocessError):
                result["cleanupComplete"] = False
        else:
            result["cleanupComplete"] = True
        if not result["cleanupComplete"]:
            result["status"] = "FAIL"
        result["durationSeconds"] = round(time.monotonic() - started, 2)
        output = output.replace(password, "[redacted]")
        output = re.sub(r"postgres(?:ql)?://[^\s\"']+", "[redacted-database-url]", output)
        (evidence / "cargo.log").write_text(output)
        serialized = json.dumps(result, ensure_ascii=False, indent=2).replace(password, "[redacted]")
        serialized = re.sub(r"postgres(?:ql)?://[^\s\"']+", "[redacted-database-url]", serialized)
        (evidence / "result.json").write_text(serialized + "\n")
        print(f"{result['status']} SDK ACL PostgreSQL: {evidence / 'result.json'}")
        if result["status"] != "PASS":
            print(output[-7000:])
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
