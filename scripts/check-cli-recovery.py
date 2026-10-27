#!/usr/bin/env python3
"""CLI wire recovery against a loopback fault fixture, using synthetic archive bytes.

Runs the real CLI's export/hash/upload/journal/resume paths. A temporary fake Docker
executable only writes a fixture archive; this does not validate Docker, image
import, worker execution, or deployed services. Those require runtime regression.
Only Python's standard library and Git are needed. No global settings are changed.
"""

import argparse
from contextlib import contextmanager
from datetime import datetime, timedelta, timezone
import hashlib
from http.server import BaseHTTPRequestHandler, HTTPServer
import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
from urllib.parse import parse_qs, urlsplit
import uuid


# Share the strict envelope/secret assertions with the existing output fixture.
spec = importlib.util.spec_from_file_location(
    "cli_output_contract", Path(__file__).with_name("check-cli-output.py")
)
output_contract = importlib.util.module_from_spec(spec)
spec.loader.exec_module(output_contract)
envelope = output_contract.envelope
require = output_contract.require
ACCESS = output_contract.SENTINEL
REFRESH = "miy_refresh_" + "r" * 48
DEVICE = "miy_device_" + "d" * 48
APP = "recovery-probe"
IMAGE = "sha256:" + "a" * 64
ARCHIVE = (b"CLI wire fixture, not a Docker image\0\r\n" * 4096)
ARCHIVE_HASH = hashlib.sha256(ARCHIVE).hexdigest()


def canonical(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()


def install_fake_docker(root, env):
    directory = root / "fake-tools"
    directory.mkdir()
    script = directory / ("docker.py" if os.name == "nt" else "docker")
    script.write_text(
        "#!/usr/bin/env python3\n"
        "import os, sys\nfrom pathlib import Path\n"
        f"expected_image = {IMAGE!r}\n"
        "if len(sys.argv) != 5 or sys.argv[1:3] != ['save', '--output'] or sys.argv[4] != expected_image:\n"
        "    raise SystemExit(97)\n"
        f"Path(sys.argv[3]).write_bytes({ARCHIVE[:len(ARCHIVE)//4096]!r} * 4096)\n"
        "with open(os.environ['MIY_RECOVERY_DOCKER_LOG'], 'a', encoding='utf-8') as log:\n"
        "    log.write('save\\n')\n",
        encoding="utf-8",
    )
    script.chmod(0o700)
    if os.name == "nt":
        (directory / "docker.cmd").write_text(
            f'@"{sys.executable}" "{script}" %*\r\n', encoding="utf-8"
        )
    env["PATH"] = str(directory) + os.pathsep + env.get("PATH", "")
    env["MIY_RECOVERY_DOCKER_LOG"] = str(root / "docker-calls.txt")


class Fixture:
    def __init__(self, config):
        self.config = config
        self.login_id = str(uuid.uuid4())
        self.session_id = str(uuid.uuid4())
        self.upload = None
        self.job = None
        self.requests = []
        self.failures = []
        self.upload_posts = 0
        self.runtime_posts = 0
        self.lifecycle_posts = 0
        self.lifecycle_preflights = 0
        self.lifecycle_jobs = []
        self.lifecycle_permission_restored = False
        self.lifecycle_generation = 1
        self.installation_id = str(uuid.uuid4())
        self.release_id = str(uuid.uuid4())
        self.receipt_override = {"archiveHash": "0" * 64}
        self.first_submission = None

    def journal(self, request_id):
        path = self.config / "journal" / f"{request_id}.json"
        require(path.is_file(), "Upload arrived before a durable request journal existed")
        entry = json.loads(path.read_bytes())
        require(entry["requestId"] == request_id, "Journal request identity differs")
        require(entry["context"] == "fixture" and entry["appId"] == APP, "Journal target differs")
        require(entry["userId"] == "fixture-user", "Journal actor differs")
        require(entry["submitPath"] == f"/api/v1/apps/{APP}/runtime", "Checked submission path missing")
        require(entry["payload"]["idempotencyKey"] == request_id, "Checked submission key missing")
        expected_image = IMAGE if entry["intent"]["action"] == "deploy" else None
        require(entry["payload"]["imageId"] == expected_image, "Checked image differs")
        require(entry["payload"]["expectedState"]["target"] == {"appId": APP, "environment": "dev"}, "Expected target missing")
        require(entry["payloadHash"] == hashlib.sha256(canonical(entry["payload"])).hexdigest(), "Checked payload hash missing or different")
        head = self.config / "journal" / f"head-{entry['inputHash']}.json"
        require(json.loads(head.read_bytes())["requestId"] == request_id, "Complete journal was not published")
        return entry


@contextmanager
def fixture_server(config):
    state = Fixture(config)

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def send_json(self, status, body, cookie=False):
            encoded = json.dumps(body).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(encoded)))
            if cookie:
                self.send_header("Set-Cookie", "fixture=synthetic; Path=/; HttpOnly")
            self.end_headers()
            try:
                self.wfile.write(encoded)
            except (BrokenPipeError, ConnectionResetError):
                pass

        def authenticated(self):
            require(
                self.headers.get("Authorization") == f"Bearer {ACCESS}"
                or self.headers.get("Cookie") == "fixture=synthetic",
                "Fixture request omitted authentication",
            )

        def lose_ack(self):
            # Consume and commit the entire request, then drop its acknowledgement.
            self.close_connection = True
            try:
                self.connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            self.connection.close()

        def do_GET(self):
            try:
                self.get()
            except Exception as error:
                state.failures.append(str(error))
                self.send_json(500, {"error": {"code": "fixture_validation_failed"}})

        def get(self):
            state.requests.append(("GET", self.path))
            path = urlsplit(self.path).path
            query = parse_qs(urlsplit(self.path).query)
            known_job = next((job for job in [state.job, *state.lifecycle_jobs]
                              if job and path == f"/api/v1/runtime/jobs/{job['id']}"), None)
            if path != "/api/v1/capabilities":
                self.authenticated()
            if path == "/api/v1/capabilities":
                self.send_json(200, {"schemaVersion": 1, "cliContractVersion": 1, "runtimePlatform": "linux/amd64", "features": ["auth.device.v1", "auth.sessions.v1", "runtime.preflight.v1", "runtime.jobs.v1", "runtime.events.v1", "images.idempotency.v1"]})
            elif path == "/api/v1/me":
                self.send_json(200, {"id": "fixture-user", "csrf": "fixture-csrf"})
            elif path.startswith(f"/api/v1/apps/{APP}/image-uploads/by-request/") or path.startswith("/api/v1/image-uploads/"):
                require(state.upload is not None, "Upload receipt requested before acceptance")
                identifier = path.rsplit("/", 1)[-1]
                require(identifier in [state.upload["id"], state.upload["requestId"]], "Receipt lookup changed identity")
                self.send_json(200, {**state.upload, **state.receipt_override})
            elif path == "/api/v1/runtime/jobs":
                require(query.get("appId") == [APP], "Recovery lookup omitted app scope")
                require(state.upload and query.get("requestId") == [state.upload["requestId"]], "Recovery lookup changed request identity")
                self.send_json(200, {"jobs": [state.job] if state.job else [], "nextCursor": None})
            elif known_job:
                self.send_json(200, known_job)
            elif state.job and path == f"/api/v1/runtime/jobs/{state.job['id']}/events":
                terminal = state.job["status"] == "succeeded"
                sequence = 2 if terminal else 1
                after = query.get("after", [None])[0]
                cursor = f"fixture-cursor-{sequence}"
                events = [] if after == cursor else [{"cursor": cursor, "sequence": sequence, "createdAt": "2026-01-01T00:00:00Z", "job": state.job}]
                self.send_json(200, {"jobId": state.job["id"], "events": events, "nextCursor": cursor, "terminal": terminal, "status": state.job["status"]})
            else:
                self.send_json(404, {"error": {"code": "not_found"}})

        def do_POST(self):
            try:
                self.post()
            except Exception as error:
                state.failures.append(str(error))
                self.send_json(500, {"error": {"code": "fixture_validation_failed"}})

        def post(self):
            state.requests.append(("POST", self.path))
            raw = self.rfile.read(int(self.headers.get("Content-Length", "0")))
            if self.path == f"/api/v1/apps/{APP}/images":
                self.authenticated()
                state.upload_posts += 1
                request_id = self.headers.get("x-miy-request-id")
                require(str(uuid.UUID(request_id)) == request_id, "Upload key must be a UUID")
                require(self.headers.get("x-miy-image") == IMAGE, "Upload image changed")
                require(raw == ARCHIVE and hashlib.sha256(raw).hexdigest() == ARCHIVE_HASH, "Exported archive bytes changed")
                require(self.headers.get("x-miy-archive-sha256") == ARCHIVE_HASH, "Upload digest differs from bytes")
                entry = state.journal(request_id)
                require(entry["upload"]["archiveHash"] == ARCHIVE_HASH and entry["upload"]["sizeBytes"] == len(raw), "Upload metadata not durable before transfer")
                state.upload = {"id": str(uuid.uuid4()), "requestId": request_id, "appId": APP, "imageId": IMAGE, "archiveHash": ARCHIVE_HASH, "sizeBytes": len(raw), "status": "ready"}
                self.lose_ack()
                return
            body = json.loads(raw or b"{}")
            if self.path == "/auth/cli/device":
                self.send_json(200, {"id": state.login_id, "deviceCode": DEVICE, "userCode": "ABCDE-FGHJK", "verificationPath": f"/cli/device/{state.login_id}", "expiresIn": 300, "interval": 1})
            elif self.path == "/auth/development":
                self.send_json(200, {}, cookie=True)
            elif self.path == f"/api/v1/cli/devices/{state.login_id}/decision":
                self.authenticated()
                self.send_json(200, {"approved": True})
            elif self.path == "/auth/cli/token":
                self.send_json(200, {"status": "approved", "sessionId": state.session_id, "userId": "fixture-user", "accessToken": ACCESS, "refreshToken": REFRESH, "tokenType": "Bearer", "expiresIn": 900, "refreshExpiresAt": (datetime.now(timezone.utc) + timedelta(days=30)).isoformat()})
            elif self.path == f"/api/v1/apps/{APP}/runtime/preflight":
                self.authenticated()
                installation = None
                if body["action"] in ["start", "stop"]:
                    state.lifecycle_preflights += 1
                    installation = {"installationId": state.installation_id, "releaseId": state.release_id, "generation": state.lifecycle_generation, "desiredState": "running", "runtimeStatus": "running", "dbStatus": "ready", "imageId": IMAGE}
                self.send_json(200, {"input": body, "expectedState": {"target": body["target"], "installation": installation}, "checks": [{"name": "authorization", "status": "passed", "code": None, "reason": None}], "changes": [], "canSubmit": True, "fullyChecked": True, "desiredState": "stopped" if body["action"] == "stop" else "running", "requiresConfirmation": False, "preservesDatabase": True})
            elif self.path == f"/api/v1/apps/{APP}/runtime":
                self.authenticated()
                if body["action"] in ["start", "stop"]:
                    self.lifecycle(body)
                    return
                state.runtime_posts += 1
                require(state.upload and body["idempotencyKey"] == state.upload["requestId"], "Runtime key differs from uploaded request")
                entry = state.journal(body["idempotencyKey"])
                require(entry["payload"] == body and entry["submissionStatus"] == "sending", "Runtime POST preceded durable sending state")
                require(body["action"] == "deploy" and body["imageId"] == IMAGE, "Runtime changed checked input")
                state.first_submission = body
                state.job = {"id": str(uuid.uuid4()), "requestId": body["idempotencyKey"], "appId": APP, "environment": "dev", "action": "deploy", "imageId": IMAGE, "status": "running", "errorCode": None}
                self.lose_ack()
            else:
                state.failures.append("Unexpected mutating request")
                self.send_json(405, {"error": {"code": "fixture_unexpected_mutation"}})

        def lifecycle(self, body):
            state.lifecycle_posts += 1
            entry = state.journal(body["idempotencyKey"])
            require(entry["payload"] == body and entry["submissionStatus"] == "sending", "Lifecycle POST preceded complete durable request")
            require(body["expectedState"]["installation"]["generation"] == state.lifecycle_generation, "New lifecycle request reused a stale expected state")
            require(body["idempotencyKey"] not in [job["requestId"] for job in state.lifecycle_jobs], "New lifecycle command reused a failed request key")
            restored = state.lifecycle_permission_restored
            job = {"id": str(uuid.uuid4()), "requestId": body["idempotencyKey"], "appId": APP, "environment": "dev", "action": body["action"], "status": "succeeded" if restored else "failed", "errorCode": None if restored else "permission_revoked", "retryable": False, "failureClass": "never"}
            state.lifecycle_jobs.append(job)
            self.send_json(200, job)

    server = HTTPServer(("127.0.0.1", 0), Handler)
    server.timeout = 10
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield state, f"http://127.0.0.1:{server.server_port}"
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)


def check(cli):
    passed = []

    def ok(name):
        passed.append(name)
        print(f"PASS {name}", flush=True)

    with tempfile.TemporaryDirectory(prefix="miy-cli-recovery-") as temporary:
        root = Path(temporary).resolve()
        config = root / "private-config"
        project = root / "한글 recovery app"
        env = dict(os.environ)
        env.update(MIY_CONFIG_DIR=str(config), MIY_CREDENTIAL_BACKEND="file", NO_PROXY="127.0.0.1,localhost", no_proxy="127.0.0.1,localhost", RUST_LOG="off", GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull)
        env.pop("MIY_CONTEXT", None)
        install_fake_docker(root, env)

        def run(args, cwd=root, expected=0, command=None):
            result = subprocess.run([str(cli), "--output", "json", *args], cwd=cwd, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30, check=False)
            require(all(secret.encode() not in result.stdout + result.stderr for secret in [ACCESS, REFRESH, DEVICE]), "Private fixture credential appeared in output")
            return envelope(result, expected, command)

        with fixture_server(config) as (state, origin):
            run(["context", "add", "fixture", "--url", origin])
            run(["--context", "fixture", "auth", "login", "--account", "fixture"])
            run(["--context", "fixture", "app", "init", str(project), "--id", APP, "--name", "Recovery fixture"])

            def git(*args):
                result = subprocess.run(["git", "-c", "user.name=MIY fixture", "-c", "user.email=fixture@localhost", "-c", "commit.gpgsign=false", "-c", "core.autocrlf=false", *args], cwd=project, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True, timeout=30)
                return result.stdout.decode().strip()

            git("-c", "init.templateDir=", "init", "--quiet")
            git("add", ".")
            git("commit", "--quiet", "-m", "Synthetic recovery project")
            manifest = json.loads((project / "manifest.json").read_bytes())
            bundle = json.loads((project / "miy.bundle.json").read_bytes())["bundleVersion"]
            metadata = {"appId": APP, "version": manifest["version"], "sourceCommit": git("rev-parse", "HEAD"), "bundleVersion": bundle, "imageId": IMAGE, "platform": "linux/amd64"}
            (project / "dist").mkdir(exist_ok=True)
            for name in ["image.json", "build.json"]:
                (project / "dist" / name).write_text(json.dumps(metadata), encoding="utf-8")
            require(git("status", "--porcelain") == "", "Synthetic metadata unexpectedly dirtied source")
            state.requests.clear()

            result = run(["--context", "fixture", "app", "deploy", "--no-wait"], cwd=project, expected=8, command="app deploy")
            require(not state.failures, "Fixture contract checks failed: " + "; ".join(state.failures))
            require(result["error"]["code"] == "upload_receipt_mismatch", "Mismatched receipt was accepted")
            require(state.upload_posts == 1 and state.runtime_posts == 0, "Lost upload acknowledgement caused duplicate or premature mutation")
            request_id = state.upload["requestId"]
            require(result["error"]["details"]["requestId"] == request_id, "Upload error lost recovery identity")
            ok("complete checked journal precedes upload; lost acknowledgement uses receipt lookup without another POST")

            resume = ["--context", "fixture", "--app", APP, "job", "resume", request_id, "--environment", "dev", "--no-wait"]
            for override in [{"requestId": str(uuid.uuid4())}, {"imageId": "sha256:" + "b" * 64}, {"archiveHash": "f" * 64}]:
                state.receipt_override = override
                result = run(resume, cwd=project, expected=8, command="job resume")
                require(result["error"]["code"] == "upload_receipt_mismatch", "Receipt binding mismatch escaped validation")
            require(state.upload_posts == 1 and state.runtime_posts == 0, "Receipt mismatch triggered remote mutation")
            ok("receipt request ID, immutable image and archive hash mismatches are rejected")

            for flag, value in [("--app", "different-app"), ("--environment", "prod")]:
                wrong = list(resume)
                wrong[wrong.index(flag) + 1] = value
                before = len(state.requests)
                result = run(wrong, cwd=project, expected=6, command="job resume")
                require(result["error"]["code"] == "resume_target_mismatch", "Explicit resume target mismatch was not rejected")
                require(not any(method == "POST" for method, _ in state.requests[before:]), "Mismatched target caused mutation")
            ok("explicit app and environment mismatch cannot resume a dev mutation")

            state.receipt_override = {}
            result = run(resume, cwd=project, expected=8, command="job resume")
            require(not state.failures, "Fixture contract checks failed: " + "; ".join(state.failures))
            require(result["error"]["code"] == "submission_result_unknown", "Lost runtime acknowledgement was not recoverable")
            require(state.upload_posts == state.runtime_posts == 1, "Original upload/runtime requests were duplicated")
            recovered = run(resume, cwd=project, command="job resume")["data"]
            require(recovered["id"] == state.job["id"], "Resume did not recover the accepted job")
            entry = state.journal(request_id)
            require(entry["jobId"] == state.job["id"] and entry["payload"] == state.first_submission, "Resume changed the saved request or lost the job")
            require(not Path(entry["upload"]["archivePath"]).exists(), "Ready receipt retained unnecessary archive bytes")
            ok("runtime acknowledgement loss resumes the original request and job without reupload or resubmission")

            before = len(state.requests)
            timed_out = run(["--context", "fixture", "job", "watch", state.job["id"], "--timeout", "1"], cwd=project, expected=8, command="job watch")
            require(timed_out["error"]["code"] == "job_watch_timeout", "Observation timeout contract changed")
            require(timed_out["error"]["details"]["remoteWorkCancelled"] is False, "Observation timeout claimed cancellation")
            require(all(method == "GET" for method, _ in state.requests[before:]), "Observation timeout issued a mutation")
            state.job["status"] = "succeeded"
            completed = run(resume[:-1], cwd=project, command="job resume")["data"]
            require(completed["id"] == recovered["id"] and completed["status"] == "succeeded", "Observation failed to resume the same remote job")
            require(state.journal(request_id)["sequence"] == 2, "Event cursor did not advance across timeout recovery")
            require(state.upload_posts == state.runtime_posts == 1, "Recovery repeated an accepted mutation")
            require((root / "docker-calls.txt").read_text().splitlines() == ["save"], "Resume exported the image again")
            require(not state.failures, "Fixture contract checks failed: " + "; ".join(state.failures))
            ok("timeout only stops observation; cursor resume completes the same job with one export and one POST per stage")

            # A new explicit lifecycle command may perform fresh admission after
            # permission/state recovery. It is distinct from retrying a Never job.
            for action in ["start", "stop"]:
                state.lifecycle_permission_restored = False
                args = ["--context", "fixture", "--app", APP, "app", action, "--environment", "dev", "--no-wait"]
                failed = run(args, cwd=project, command=f"app {action}")["data"]
                require(failed["status"] == "failed" and failed["errorCode"] == "permission_revoked", "Fixture did not return a typed permission failure")
                denied = run(["--context", "fixture", "--app", APP, "job", "retry", failed["id"], "--environment", "dev", "--no-wait"], cwd=project, expected=6, command="job retry")
                require(denied["error"]["code"] == "retry_not_allowed", "Never-class failure became retryable")
                state.lifecycle_permission_restored = True
                state.lifecycle_generation += 1
                before = state.lifecycle_preflights
                fresh = run(args, cwd=project, command=f"app {action}")["data"]
                require(fresh["status"] == "succeeded", "Restored lifecycle remained blocked by its prior failure")
                require(fresh["id"] != failed["id"] and fresh["requestId"] != failed["requestId"], "Fresh lifecycle command reused the failed job/request")
                require(state.lifecycle_preflights == before + 1, "Fresh lifecycle skipped current admission")
                require(state.journal(fresh["requestId"])["payload"]["expectedState"]["installation"]["generation"] == state.lifecycle_generation, "Fresh lifecycle journal retained the failed expected state")
            require(state.lifecycle_preflights == state.lifecycle_posts == 4, "Lifecycle scenario created unexpected admission/mutation counts")
            require(state.upload_posts == state.runtime_posts == 1, "Lifecycle recovery replayed deployment requests")
            require(not state.failures, "Fixture contract checks failed: " + "; ".join(state.failures))
            ok("failed start/stop remains nonretryable; restored explicit commands use fresh preflight and new request/job IDs")
            counts = {"deployment": {"exports": 1, "uploadPosts": state.upload_posts, "runtimePosts": state.runtime_posts}, "lifecycle": {"preflights": state.lifecycle_preflights, "runtimePosts": state.lifecycle_posts, "failedJobs": 2, "freshSuccessfulJobs": 2}}

    print(json.dumps({"passed": len(passed), "scope": "CLI wire/client recovery", "counts": counts, "liveServicesUsed": False, "realDockerUsed": False, "temporaryProjectsRemoved": True}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", required=True, type=Path, help="Native miy executable to inspect")
    options = parser.parse_args()
    executable = options.cli.resolve(strict=True)
    require(executable.is_file(), "--cli must be a file")
    check(executable)
