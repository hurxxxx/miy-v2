#!/usr/bin/env python3
"""Native CLI report/outbox and credential-failure contracts; loopback fixture only."""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import threading


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", default="target/debug/miy")
    cli = str(Path(parser.parse_args().cli).resolve())
    received, mode = [], ["ok"]

    receipt_keys = {}

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            assert self.path in ("/api/v1/support/reports", "/api/v1/support/reports/result")
            assert not self.headers.get("Authorization") and not self.headers.get("Cookie")
            payload = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            key = self.headers.get("x-miy-report-key")
            if self.path.endswith("/result"):
                assert any(v == payload for v in received) and key == receipt_keys[payload["id"]]
                self.send_response(200)
                self.end_headers()
                self.wfile.write(json.dumps({"id":payload["id"],"status":"resolved","result":"고쳤습니다"}).encode())
                return
            if payload["id"] in receipt_keys:
                assert receipt_keys[payload["id"]] == key
            receipt_keys[payload["id"]] = key
            received.append(payload)
            if mode[0] == "drop":
                self.connection.shutdown(socket.SHUT_RDWR)
                return
            status = 429 if mode[0] == "limited" else 201
            body = {"id": payload["id"], "status": "received"} if status == 201 else {"error": {"code": "report_intake_limited"}}
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.end_headers()
            self.wfile.write(json.dumps(body).encode())

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    origin = "http://127.0.0.1:" + str(server.server_port)
    checks = []
    try:
        with tempfile.TemporaryDirectory(prefix="miy-reports-") as temp:
            root = Path(temp).resolve()
            config = root / "config"
            config.mkdir(mode=0o700)
            env = {**os.environ, "MIY_CONFIG_DIR": str(config), "MIY_CREDENTIAL_BACKEND": "invalid"}
            env.pop("MIY_CONTEXT", None)

            def run(*args, exit_code=0):
                result = subprocess.run([cli, "--output", "json", *args], cwd=root, env=env, capture_output=True, text=True, encoding="utf-8")
                assert result.returncode == exit_code, (args[0:2], result.returncode)
                assert "PRIVATE-SENTINEL" not in result.stdout + result.stderr
                assert not result.stderr, "unexpected CLI diagnostics"
                return json.loads(result.stdout)

            preview = run("report", "send", "--server", origin, "--message", "실패\npassword=PRIVATE-SENTINEL", "--dry-run")["data"]
            assert preview["status"] == "preview" and not received and not (config / "reports").exists()
            assert "redacted" in preview["report"]["message"]
            checks.append("preview_without_network_or_files_and_redaction")
            run("context", "init", "--url", origin)
            # A corrupt credential directory reproduces a local read failure,
            # without ever creating or reading real credentials.
            cfg = json.loads((config / "config.json").read_text())
            cfg["contexts"]["default"]["userId"] = "fixture-user"
            (config / "config.json").write_text(json.dumps(cfg))
            (config / "credentials").write_text("unrelated fixture")
            failed = run("auth", "status", exit_code=7)["error"]
            assert failed["code"] == "credential_storage_unavailable"
            assert failed["details"]["operation"] == "read"
            assert failed["details"]["reason"] == "credential_storage_invalid_path"
            response = run("report", "send", "--message", "승인 후 인증 오류")["data"]
            assert response["status"] == "received" and len(received) == 1
            assert received[0]["diagnostics"]["lastError"]["reason"] == "credential_storage_invalid_path"
            assert received[0]["diagnostics"]["lastError"]["occurredAt"] > 0
            checks.append("broken_credential_store_does_not_block_anonymous_send")
            if os.name != "nt":
                assert Path(response["file"]).stat().st_mode & 0o077 == 0
                assert (config / "reports").stat().st_mode & 0o077 == 0
            checks.append("private_local_outbox")
            assert receipt_keys[response["id"]] and "resultKey" not in run("report","show",response["id"])["data"]
            action = run(*response["resultCommand"]["args"])["data"]
            assert action == {"id":response["id"],"status":"resolved","result":"고쳤습니다"}
            checks.append("origin_bound_anonymous_receipt_result")
            mode[0] = "drop"
            queued = run("report", "send", "--message", "응답 유실", exit_code=8)["error"]
            assert queued["code"] == "report_queued" and queued["recovery"]["args"][-1] == received[-1]["id"]
            before = received[-1]
            mode[0] = "ok"
            run(*queued["recovery"]["args"])
            assert received[-1] == before
            checks.append("lost_response_retry_preserves_id_and_payload")
            mode[0] = "limited"
            queued = run("report", "send", "--message", "접수 제한", exit_code=8)["error"]
            assert queued["details"]["cause"] == "report_intake_limited"
            mode[0] = "ok"
            run(*queued["recovery"]["args"])
            checks.append("rate_limit_keeps_retryable_report")
            count = len(received)
            saved = run("report", "send", "--message", "오프라인 신고", "--save-only")["data"]
            assert len(received) == count and saved["status"] == "saved"
            assert run("report", "show", saved["id"])["data"]["sent"] is False
            run("report", "retry", saved["id"])
            assert run("report", "show", saved["id"])["data"]["sent"] is True
            assert len(run("report", "list")["data"]["reports"]) == 4
            checks.append("save_show_list_and_explicit_retry")
            (config / "config.json").write_text("invalid fixture")
            run("report", "--outbox", str(root / "separate"), "send", "--server", origin, "--message", "설정 파일 오류")
            checks.append("explicit_server_and_outbox_bypass_broken_context")
            long_file = root / "feedback.txt"
            message = "한글😀 " * 8000
            long_file.write_text(message, encoding="utf-8-sig")
            parent = response["id"]
            run("report", "send", "--server", origin, "--message-file", str(long_file), "--thread-id", parent)
            assert received[-1]["message"] == message and received[-1]["threadId"] == parent
            long_file.write_text(message + "끝", encoding="utf-8")
            run("report","send","--server",origin,"--message-file",str(long_file),exit_code=2)
            checks.append("bounded_input")
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
    print(json.dumps({"status": "PASS", "checks": checks}, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
