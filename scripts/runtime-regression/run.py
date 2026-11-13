#!/usr/bin/env python3
"""Opt-in isolated regression, including cleanup of partially completed setup."""
import json
import os
import pathlib
import shutil
import signal
import subprocess
import sys
import tempfile
import traceback
from workdir import create_owner, file_sha256, lock


class Interrupted(Exception):
    def __init__(self, signum):
        self.signum = signum


def interrupt(signum, _frame):
    raise Interrupted(signum)


def run_child(args, *, check=False, **kwargs):
    """Reap this invocation's process group before filesystem/Docker cleanup."""
    with subprocess.Popen(args, start_new_session=True, **kwargs) as child:
        try:
            stdout, stderr = child.communicate()
        finally:
            try:
                os.killpg(child.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
            # Reap lingering workers in the owned group, even after an early
            # integration-test failure. Docker resources are cleaned separately.
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
    result = subprocess.CompletedProcess(args, child.returncode, stdout, stderr)
    if check:
        result.check_returncode()
    return result

scripts = pathlib.Path(__file__).resolve().parent
repo = scripts.parent.parent
base = pathlib.Path(os.environ.get("MIY_RUNTIME_REGRESSION_DIR", repo / ".local/evidence/runtime-regression"))
base.mkdir(parents=True, exist_ok=True, mode=0o700)
root = pathlib.Path(tempfile.mkdtemp(prefix="miy-runtime-regression-", dir=base)).resolve()
create_owner(root)
owner_lock = lock(root)
for name in ["evidence", "artifacts", "bin"]:
    (root / name).mkdir()
env = os.environ.copy()
env["MIY_AUDIT_ROOT"] = str(root)
env["AUDIT_REAL_DOCKER"] = shutil.which("docker") or "docker"
result_code = 1
print("Evidence: " + str(root), flush=True)
signal.signal(signal.SIGTERM, interrupt)
try:
    for name in ["postgres:18.3", "python:3.14-slim"]:
        run_child([env["AUDIT_REAL_DOCKER"], "image", "inspect", name],
                       stdout=subprocess.DEVNULL, check=True)
    with (root / "evidence/setup.log").open("w") as log:
        run_child([sys.executable, str(scripts / "setup.py")], env=env,
                       stdout=log, stderr=subprocess.STDOUT, check=True)
    env.update(json.loads((root / "private-env.json").read_text()))
    shutil.copyfile(scripts / "docker", root / "bin/docker")
    (root / "bin/docker").chmod(0o755)
    env["PATH"] = str(root / "bin") + os.pathsep + env["PATH"]
    cargo = shutil.which("cargo") or str(pathlib.Path.home() / ".cargo/bin/cargo")
    with (root / "evidence/cli-build.log").open("w") as log:
        run_child([cargo, "build", "--offline", "--locked", "-p", "miy"], cwd=repo, env=env,
                       stdout=log, stderr=subprocess.STDOUT, check=True)
    binary = root / "bin/miy"
    shutil.copy2(repo / "target/debug/miy", binary)
    env["MIY_CLI_TEST_BINARY"] = str(binary)
    (root / "evidence/cli-identity.json").write_text(json.dumps({
        "sourceCommit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
        "workingTreeDirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=repo)),
        "binarySha256": file_sha256(binary),
    }, indent=2))
    # Worker subprocesses re-execute the test binary. Pin it outside Cargo's
    # shared target directory just like the CLI: another build can replace that
    # directory's executable while this long-running suite is still using it.
    with (root / "evidence/test-build.log").open("w") as log:
        build = run_child(
            [cargo, "test", "--offline", "--locked", "-p", "miy", "--bin", "miy",
             "--no-run", "--message-format=json"],
            cwd=repo, env=env, stdout=subprocess.PIPE, stderr=log, text=True)
        log.write(build.stdout)
        build.check_returncode()
    artifacts = [json.loads(line) for line in build.stdout.splitlines() if line.startswith("{")]
    executables = [row["executable"] for row in artifacts
                   if row.get("reason") == "compiler-artifact" and row.get("profile", {}).get("test")
                   and row.get("target", {}).get("name") == "miy" and row.get("executable")]
    if len(executables) != 1:
        raise RuntimeError("expected exactly one runtime test executable")
    test_binary = root / "bin/miy-runtime-tests"
    shutil.copy2(executables[0], test_binary)
    (root / "evidence/test-identity.json").write_text(json.dumps({
        "binarySha256": file_sha256(test_binary),
        "isolatedExecutable": str(test_binary),
    }, indent=2))
    args = [str(test_binary), "runtime_regression::isolated_runtime_regression", "--exact",
            "--ignored", "--nocapture", "--test-threads=1"]
    with (root / "evidence/integration.log").open("w") as log:
        result_code = run_child(args, cwd=repo, env=env, stdout=log,
                                     stderr=subprocess.STDOUT).returncode
    print((root / "evidence/integration.log").read_text()[-14000:])
    observations = root / "evidence/results.jsonl"
    rows = [json.loads(line) for line in observations.read_text().splitlines()] if observations.exists() else []
    counts = {suite: sum(row.get("evidence", {}).get("suite", "runtime") == suite for row in rows)
              for suite in ("runtime", "operations", "cli", "auth", "permissions", "delegation", "boundary", "migration", "frames")}
    print(json.dumps({"observations": len(rows), "suites": counts}))
    if counts != {"runtime": 46, "operations": 6, "cli": 6, "auth": 8, "permissions": 24, "delegation": 10, "boundary": 12, "migration": 3, "frames": 14} or any(row["status"] != "PASS" for row in rows):
        result_code = result_code or 1
except KeyboardInterrupt:
    result_code = 130
except Interrupted as error:
    result_code = 128 + error.signum
except Exception:
    (root / "evidence/runner-error.txt").write_text(traceback.format_exc())
    print("Regression failed; inspect " + str(root / "evidence"), flush=True)
    result_code = 1
finally:
    signal.signal(signal.SIGINT, signal.SIG_IGN)
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    owner_lock.close()
    with (root / "evidence/cleanup.log").open("w") as log:
        cleanup = subprocess.run([sys.executable, str(scripts / "cleanup.py")],
                                 env=env, stdout=log, stderr=subprocess.STDOUT)
    if cleanup.returncode:
        print("Cleanup requires attention: " + str(root / "evidence/cleanup.log"), flush=True)
    (root / 'evidence/runner-result.json').write_text(json.dumps({
        'testExitCode': result_code, 'cleanupExitCode': cleanup.returncode,
        'exitCode': result_code or cleanup.returncode,
    }, indent=2))
    result_code = result_code or cleanup.returncode
sys.exit(result_code)
