#!/usr/bin/env python3
"""Offline CLI output/exit contracts with a loopback HTTP fixture.

Uses only Python's standard library. No Docker, SDK builds, real authentication,
or deployed services are used. Temporary projects and fixture sessions are removed.
This is a native CLI contract check, not Windows PowerShell or Docker Desktop E2E.
"""

import argparse
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
from datetime import datetime, timedelta, timezone
from urllib.parse import urlsplit, parse_qs
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import threading
import uuid


SENTINEL = "miy_access_" + "PRIVATEFIXTURETOKEN97b4d2" * 2
EXITS = {0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 130}


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def decoded(result):
    stdout = result.stdout.decode("utf-8")
    stderr = result.stderr.decode("utf-8")
    require(SENTINEL not in stdout + stderr, "Fixture secret appeared in CLI output")
    return stdout, stderr


def envelope(result, expected_exit, command=None):
    require(expected_exit in EXITS, "Expected exit must name a contract category")
    require(
        result.returncode == expected_exit,
        f"CLI exit mismatch: expected {expected_exit}, received {result.returncode}",
    )
    stdout, stderr = decoded(result)
    require(len(stdout.splitlines()) == 1, "JSON stdout must have exactly one response line")
    try:
        value = json.loads(stdout)
    except ValueError:
        raise AssertionError("CLI stdout is not one JSON envelope") from None
    require(isinstance(value, dict), "CLI envelope must be an object")
    require(
        {"schemaVersion", "command", "ok", "data", "error", "meta"} <= value.keys(),
        "Missing CLI envelope fields",
    )
    require(type(value["schemaVersion"]) is int and value["schemaVersion"] == 1, "Unexpected schema version")
    require(isinstance(value["command"], str) and value["command"], "Missing command identity")
    if command is not None:
        require(value["command"] == command, f"Unexpected command identity for {command}")
    require(isinstance(value["meta"], dict), "CLI meta must be an object")
    require(value["ok"] is (expected_exit == 0), "CLI success state disagrees with exit code")
    if expected_exit == 0:
        require(value["error"] is None, "Successful CLI response contains an error")
    else:
        require(value["data"] is None, "Failed CLI response contains success data")
        error = value["error"]
        require(isinstance(error, dict), "Failed CLI response requires an error object")
        require({"code", "message", "retryable", "details"} <= error.keys(), "Incomplete typed error")
        require(isinstance(error["code"], str) and error["code"], "Missing stable error code")
        require(isinstance(error["message"], str), "Error message must be text")
        require(type(error["retryable"]) is bool, "Retryable must be boolean")
        require(isinstance(error["details"], dict), "Error details must be structured")
        if error.get("recovery") is not None:
            recovery = error["recovery"]
            require(isinstance(recovery, dict), "Recovery must be an executable/argv object")
            require(isinstance(recovery.get("executable"), str) and recovery["executable"], "Recovery executable missing")
            require(isinstance(recovery.get("args"), list), "Recovery args must be an array")
            require(all(isinstance(arg, str) for arg in recovery["args"]), "Recovery args must be strings")
    # These fixture commands do not run build tools; no diagnostics are expected.
    require(not stderr.strip(), "Unexpected diagnostics for an offline contract command")
    return value


class Fixture:
    def __init__(self):
        self.status = 200
        self.body = b"[]"
        self.request_id = "fixture-request-123"
        self.job_id = str(uuid.uuid4())
        self.job_status = "failed"
        self.actor = "fixture-user"
        self.login_id = str(uuid.uuid4())
        self.sequence = 1
        self.access_seconds=900
        self.fail_refresh=False
        self.refresh_requests=0
        self.session_id=str(uuid.uuid4())
        self.requests = []
        self.auth_failures = 0
        self.job_observed = threading.Event()

    def reply(self, status, body, request_id="fixture-request-123"):
        self.status = status
        self.body = json.dumps(body, ensure_ascii=False).encode("utf-8") if not isinstance(body, bytes) else body
        self.request_id = request_id


@contextmanager
def fixture_server():
    state = Fixture()

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass  # Never log request headers or fixture credentials.

        def send_json(self,status,body,cookie=False):
            body=body if isinstance(body,bytes) else json.dumps(body,ensure_ascii=False).encode()
            self.send_response(status)
            self.send_header("Content-Type","application/json; charset=utf-8")
            self.send_header("Content-Length",str(len(body)))
            self.send_header("x-request-id",state.request_id)
            if cookie:self.send_header("Set-Cookie","fixture=synthetic; Path=/; HttpOnly")
            self.end_headers()
            try:self.wfile.write(body)
            except (BrokenPipeError,ConnectionResetError):pass

        def job(self):
            return {"id":state.job_id,"appId":"output-probe","environment":"dev","action":"start",
                    "status":state.job_status,"step":"migration","errorCode":"migration_failed"}

        def do_GET(self):
            state.requests.append(("GET",self.path))
            path=urlsplit(self.path).path
            if path!="/api/v1/capabilities" and self.headers.get("Cookie")!="fixture=synthetic" and self.headers.get("Authorization")!=f"Bearer {SENTINEL}":state.auth_failures+=1
            if path=="/api/v1/users":self.send_json(state.status,state.body)
            elif path=="/api/v1/me":self.send_json(200,{"id":state.actor,"csrf":"fixture-csrf","name":"한글 사용자"})
            elif path=="/api/v1/capabilities":self.send_json(200,{"schemaVersion":1,"cliContractVersion":1,"runtimePlatform":"linux/amd64","features":["runtime.preflight.v1","auth.sessions.v1","auth.device.v1","runtime.jobs.v1","runtime.events.v1","runtime.retry.v1","images.idempotency.v1","runtime.logs.v1","lists.pagination.v1"]})
            elif path==f"/api/v1/runtime/jobs/{state.job_id}":
                self.send_json(200,self.job());state.job_observed.set()
            elif path==f"/api/v1/runtime/jobs/{state.job_id}/events":
                after=parse_qs(urlsplit(self.path).query).get('after')
                events=[] if after else [{"cursor":"fixture-cursor-1","sequence":1,"createdAt":"2026-01-01T00:00:00Z","job":self.job()}]
                self.send_json(200,{"jobId":state.job_id,"events":events,"nextCursor":"fixture-cursor-1","terminal":state.job_status in ["failed","succeeded"],"status":state.job_status});state.job_observed.set()
            else:self.send_json(404,{})

        def do_POST(self):
            state.requests.append(("POST",self.path))
            length=int(self.headers.get("Content-Length","0"));request_body=json.loads(self.rfile.read(length) or b"{}")
            if self.path=="/auth/cli/token" and request_body.get("grantType")=="refresh_token":
                state.refresh_requests+=1
                if state.fail_refresh:
                    self.close_connection=True
                    return
            if self.path=="/auth/cli/device":self.send_json(200,{"id":state.login_id,"deviceCode":"miy_device_"+"a"*48,"userCode":"ABCDE-FGHJK","verificationPath":f"/cli/device/{state.login_id}","expiresIn":300,"interval":1})
            elif self.path=="/auth/development":self.send_json(200,{},cookie=True)
            elif self.path==f"/api/v1/cli/devices/{state.login_id}/decision":self.send_json(200,{"approved":True})
            elif self.path=="/auth/cli/token":self.send_json(200,{"status":"approved","sessionId":state.session_id,"userId":state.actor,"accessToken":SENTINEL,"refreshToken":"miy_refresh_"+"r"*48,"tokenType":"Bearer","expiresIn":state.access_seconds,"refreshExpiresAt":(datetime.now(timezone.utc)+timedelta(days=30)).isoformat()})
            elif self.path=="/auth/cli/revoke":self.send_json(200,{"revoked":True})
            else:self.send_json(405,{})

    server = HTTPServer(("127.0.0.1", 0), Handler)
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
    env = dict(os.environ)
    # Keep fixture traffic local even on machines with configured HTTP proxies.
    env["NO_PROXY"] = env["no_proxy"] = "127.0.0.1,localhost"
    env["RUST_LOG"] = "off"

    def ok(name):
        passed.append(name)
        print(f"PASS {name}", flush=True)

    with tempfile.TemporaryDirectory(prefix="miy-cli-output-") as temporary:
        root = Path(temporary)
        env["MIY_CONFIG_DIR"]=str(root/"private-config")
        env["MIY_CREDENTIAL_BACKEND"]="file"
        env.pop("MIY_CONTEXT",None)

        def run(args, cwd=root, expected=0, command=None):
            result = subprocess.run(
                [str(cli), *args], cwd=cwd, env=env,
                stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                timeout=30, check=False,
            )
            return envelope(result, expected, command)

        for args in [
            ["describe"],
            ["--output", "json", "describe"],
            ["describe", "--output", "json"],
        ]:
            response = run(args, command="describe")
            require(isinstance(response["data"].get("cliVersion"), str), "Describe must expose CLI version")
        ok("auto nonTTY and global JSON options return one versioned envelope")

        text_result = subprocess.run(
            [str(cli), "describe", "--output", "text"], cwd=root, env=env,
            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30,
        )
        text, stderr = decoded(text_result)
        require(text_result.returncode == 0 and not stderr.strip(), "Text describe failed")
        require("cliVersion:" in text and not text.lstrip().startswith("{"), "Text mode must render a labeled human result")
        ok("text mode renders a human-readable result")

        for args in [
            [], ["app"],
            ["--output", "json", "describe", f"--{SENTINEL}"],
            ["describe", "--format", SENTINEL, "--output", "json"],
            ["app", "deploy", "--environment", "prod"],
        ]:
            response = run(args, expected=2)
            require(response["error"]["code"] == "invalid_arguments", "Argument errors must have a stable code")
        destination = root / "must not exist"
        run(["--output", "jsonl", "app", "init", str(destination), "--id", "rejected", "--name", "Rejected"], expected=2, command="app init")
        require(not destination.exists(), "Unsupported JSONL mutated the destination")
        ok("parse errors hide argument secrets; unsupported JSONL has no init side effects")

        run(["user","list"],expected=7,command="user list")
        for old in ["login","logout","describe","doctor","users","resume","control","components","mcp","codex","setup-codex"]:
            response=run(["app",old],expected=2)
            require(response["error"]["code"]=="removed_command","Removed commands must explain replacement")
            require(response["error"].get("recovery"),"Removed command needs executable/argv recovery")
        ok("remote commands require a context and removed names return replacement errors")

        project = root / "한글 앱 with spaces"
        run(["app", "init", str(project), "--id", "output-probe", "--name", "한글 앱"], command="app init")
        manifest = json.loads((project / "manifest.json").read_bytes())
        require(manifest["name"] == "한글 앱", "UTF-8 app identity was not preserved")
        run(["app", "setup"], cwd=project, command="app setup")
        before = (project / "miy.cli.json").read_bytes()
        run(["app", "setup"], cwd=project, command="app setup")
        require((project / "miy.cli.json").read_bytes() == before, "Repeated setup changed its lock")
        require((project/"docs/CLI-REFERENCE.md").read_text().startswith("# MIY CLI manual"),"Generated manual missing")
        schema=run(["describe","--command","app deploy"])["data"]
        require(schema["scope"]=="command","Focused discovery must return a command contract")
        completion=subprocess.run([str(cli),"completion","powershell"],cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30)
        require(completion.returncode==0 and b"Register-ArgumentCompleter" in completion.stdout,"PowerShell completion missing")
        ok("UTF-8 scaffold, transactional repeatable setup, focused discovery and completion")

        remote = root / "remote fixture"
        (remote / ".miy").mkdir(parents=True)
        with fixture_server() as (state, origin):
            run(["context","add","fixture","--url",origin],cwd=remote)
            missing=run(["user","list"],cwd=remote,expected=3)
            require(missing["error"].get("recovery")=={"executable":"miy","args":["auth","login"]},"Login recovery must be argv")
            run(["auth","login","--account","fixture"],cwd=remote)
            require(not(remote/".miy/session.json").exists(),"New login wrote a project token")
            state.requests.clear();state.auth_failures=0
            users = [{"id": state.actor, "name": "한글 사용자"}]
            state.reply(200, {"users":users,"nextCursor":None})
            require(run(["user", "list"], cwd=remote, command="user list")["data"]["users"] == users, "Successful API data changed")
            state.reply(200, f"invalid JSON {SENTINEL}".encode())
            run(["user", "list"], cwd=remote, expected=8, command="user list")
            ok("authenticated loopback success and malformed successful response")

            for status, expected in [(400, 2), (401, 3), (403, 4), (404, 5), (409, 6), (429, 8), (500, 8)]:
                for body in [
                    {"error": {"code": "fixture_rejected", "message": SENTINEL, "details": {"token": SENTINEL}}},
                    f"<html>{SENTINEL}</html>".encode(),
                ]:
                    state.reply(status, body)
                    response = run(["user", "list"], cwd=remote, expected=expected, command="user list")
                    require(response["error"]["details"].get("httpStatus") == status, "HTTP status context missing")
                    require(response["error"]["retryable"] is (expected == 8), "HTTP retryability disagrees with category")
                    if isinstance(body, dict):
                        require(response["error"]["code"] == "fixture_rejected", "Safe server error code was lost")
            state.reply(403, {"error": {"code": f"{SENTINEL}/unsafe", "requestId": f"{SENTINEL}/unsafe"}}, request_id=f"{SENTINEL}/unsafe")
            run(["user", "list"], cwd=remote, expected=4, command="user list")
            ok("HTTP status categories survive JSON/HTML bodies without leaking secrets")

            state.request_id = "fixture-job-request"
            response=run(["job","watch",state.job_id],cwd=remote,expected=9,command="job watch")
            require(response["error"]["details"].get("jobId")==state.job_id,"Failed observation lost its job ID")
            ok("terminal observation failure exits 9 and retains job/cursor recovery")

            if os.name == "posix":
                state.job_status = "running"
                state.job_observed.clear()
                child = subprocess.Popen(
                    [str(cli), "--output", "json", "job", "watch", state.job_id], cwd=remote, env=env,
                    stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                    start_new_session=True,
                )
                try:
                    require(state.job_observed.wait(timeout=10), "CLI never observed the fixture job")
                    child.send_signal(signal.SIGINT)
                    stdout, stderr = child.communicate(timeout=10)
                    response = envelope(subprocess.CompletedProcess([], child.returncode, stdout, stderr), 130, "job watch")
                    require(response["error"]["details"].get("jobId") == state.job_id, "Interrupted observation lost its job ID")
                finally:
                    if child.poll() is None:
                        child.kill()
                        child.communicate(timeout=5)
                ok("POSIX Ctrl+C ends local observation with exit 130 and keeps the remote job")
            else:
                print("SKIP POSIX signal delivery; Windows console Ctrl+C requires native E2E", flush=True)
            require(state.auth_failures == 0, "Fixture requests were not authenticated")
            require(all(method == "GET" for method, _path in state.requests), "Observation issued a mutating request")
            ok("all fixture operations were authenticated reads; no remote cancellation or mutation")
            state.access_seconds=1
            run(["auth","login","--account","fixture"],cwd=remote)
            state.fail_refresh=True
            response=run(["auth","status"],cwd=remote,expected=3)
            require(response["error"]["code"]=="refresh_result_unavailable","Lost renewal must require explicit reauthentication")
            attempts=state.refresh_requests
            run(["auth","status"],cwd=remote,expected=3)
            require(state.refresh_requests==attempts==1,"CLI replayed a possibly consumed refresh token")
            ok("lost refresh response persists uncertain state and prevents automatic token replay")

    print(json.dumps({"passed": len(passed), "liveServicesUsed": False, "temporaryProjectsRemoved": True}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", required=True, type=Path, help="Native miy executable to inspect")
    options = parser.parse_args()
    executable = options.cli.resolve(strict=True)
    require(executable.is_file(), "--cli must be a file")
    check(executable)
