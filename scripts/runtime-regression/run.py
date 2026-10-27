#!/usr/bin/env python3
"""Opt-in isolated regression, including cleanup of partially completed setup."""
import json
import hashlib
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import traceback

scripts = pathlib.Path(__file__).resolve().parent
repo = scripts.parent.parent
root = pathlib.Path(tempfile.mkdtemp(prefix="miy-runtime-regression-"))
for name in ["evidence", "artifacts", "bin"]:
    (root / name).mkdir()
env = os.environ.copy()
env["MIY_AUDIT_ROOT"] = str(root)
env["AUDIT_REAL_DOCKER"] = shutil.which("docker") or "docker"
result_code = 1
print("Evidence: " + str(root), flush=True)
try:
    for name in ["postgres:18.3", "python:3.14-slim"]:
        subprocess.run([env["AUDIT_REAL_DOCKER"], "image", "inspect", name],
                       stdout=subprocess.DEVNULL, check=True)
    with (root / "evidence/setup.log").open("w") as log:
        subprocess.run([sys.executable, str(scripts / "setup.py")], env=env,
                       stdout=log, stderr=subprocess.STDOUT, check=True)
    env.update(json.loads((root / "private-env.json").read_text()))
    shutil.copyfile(scripts / "docker", root / "bin/docker")
    (root / "bin/docker").chmod(0o755)
    env["PATH"] = str(root / "bin") + os.pathsep + env["PATH"]
    cargo = shutil.which("cargo") or str(pathlib.Path.home() / ".cargo/bin/cargo")
    with (root / "evidence/cli-build.log").open("w") as log:
        subprocess.run([cargo, "build", "--offline", "--locked", "-p", "miy"], cwd=repo, env=env,
                       stdout=log, stderr=subprocess.STDOUT, check=True)
    binary = root / "bin/miy"
    shutil.copy2(repo / "target/debug/miy", binary)
    env["MIY_CLI_TEST_BINARY"] = str(binary)
    (root / "evidence/cli-identity.json").write_text(json.dumps({
        "sourceCommit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
        "workingTreeDirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=repo)),
        "binarySha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    }, indent=2))
    args = [cargo, "test", "--offline", "--locked", "--workspace",
            "runtime_regression::isolated_runtime_regression", "--", "--exact",
            "--ignored", "--nocapture", "--test-threads=1"]
    with (root / "evidence/integration.log").open("w") as log:
        result_code = subprocess.run(args, cwd=repo, env=env, stdout=log,
                                     stderr=subprocess.STDOUT).returncode
    print((root / "evidence/integration.log").read_text()[-14000:])
    observations = root / "evidence/results.jsonl"
    rows = [json.loads(line) for line in observations.read_text().splitlines()] if observations.exists() else []
    counts = {suite: sum(row.get("evidence", {}).get("suite", "runtime") == suite for row in rows)
              for suite in ("runtime", "operations", "cli", "auth")}
    print(json.dumps({"observations": len(rows), "suites": counts}))
    if counts != {"runtime": 33, "operations": 6, "cli": 6, "auth": 7} or any(row["status"] != "PASS" for row in rows):
        result_code = result_code or 1
except KeyboardInterrupt:
    result_code = 130
except Exception:
    (root / "evidence/runner-error.txt").write_text(traceback.format_exc())
    print("Regression failed; inspect " + str(root / "evidence"), flush=True)
    result_code = 1
finally:
    with (root / "evidence/cleanup.log").open("w") as log:
        cleanup = subprocess.run([sys.executable, str(scripts / "cleanup.py")],
                                 env=env, stdout=log, stderr=subprocess.STDOUT)
    if cleanup.returncode:
        print("Cleanup requires attention: " + str(root / "evidence/cleanup.log"), flush=True)
    result_code = result_code or cleanup.returncode
sys.exit(result_code)
