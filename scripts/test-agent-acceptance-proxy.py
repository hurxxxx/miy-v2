#!/usr/bin/env python3
"""Local Unix-socket regressions; no real server, tokens, DNS or model calls."""
import hashlib
from concurrent.futures import ThreadPoolExecutor
import http.client
import importlib.util
import io
import json
from pathlib import Path
import socket
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import Mock
import uuid

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location(
    "acceptance_proxy", Path(__file__).with_name("agent-acceptance-proxy.py"))
proxy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(proxy)


class FakeBoundary(proxy.Boundary):
    def __init__(self, path):
        self.socket_path = path
        self.app = "fragmented-build-fixture"
        self.actor = "synthetic-test-actor"
        self.config = {"proxyToken": "synthetic_proxy_token_for_local_test_only"}
        self.lock = threading.RLock()
        self.state = {key: [] for key in proxy.STATE_SETS}
        self.state["installations"] = {}
        self.calls, self.audit_events = [], []

    def save(self):
        pass

    def audit(self, *event):
        self.audit_events.append(event)

    def upstream_request(self, method, path, body=b"", headers=None):
        self.calls.append((method, path, body, headers))
        if method == "POST" and path == f"/api/v1/apps/{self.app}/builds":
            response = {"id": str(uuid.uuid4()), "appId": self.app, "environment": "dev",
                        "actor": self.actor, "userId": self.actor, "action": "build",
                        "requestId": headers["x-miy-request-id"], "status": "queued"}
            return 200, "application/json", proxy.json_bytes(response)
        if method == "GET" and path.startswith(f"/api/v1/apps/{self.app}/build-requests/"):
            return 404, "application/json", proxy.json_bytes({"error": {"code": "not_found"}})
        raise AssertionError("Unexpected fake upstream request")


class RefreshBoundary(proxy.Boundary):
    """Run real rotation/persistence logic against a gated in-memory upstream."""
    def __init__(self, directory, expired=True, lose_refresh_response=False):
        self.config_path = Path(directory) / "config.json"
        self.state_path = Path(directory) / "state.json"
        self.config = {"accessToken": "miy_access_" + "a" * 32,
                       "refreshToken": "miy_refresh_" + "b" * 32}
        self.access_expiry = time.time() - 1 if expired else time.time() + 600
        self.actor = "synthetic-test-actor"
        self.lock, self.refresh_lock = threading.RLock(), threading.Lock()
        self.state = {"counts": {"refreshes": 0}}
        self.entered, self.release = threading.Event(), threading.Event()
        self.refresh_calls, self.api_tokens = 0, []
        self.lose_refresh_response = lose_refresh_response
        proxy.atomic_json(self.config_path, self.config)

    def raw_upstream(self, method, path, body=b"", headers=None, token=None, refresh=False):
        if refresh:
            self.refresh_calls += 1
            self.entered.set()
            if not self.release.wait(2):
                raise AssertionError("Test did not release refresh response")
            if self.lose_refresh_response:
                raise OSError("synthetic lost refresh response")
            value = {"status": "approved", "userId": self.actor, "expiresIn": 900,
                     "accessToken": "miy_access_" + "c" * 32,
                     "refreshToken": "miy_refresh_" + "d" * 32}
            return 200, "application/json", proxy.json_bytes(value)
        self.api_tokens.append(token)
        # The non-expired branch exercises refresh after a server 401 as well.
        status = 200 if token == "miy_access_" + "c" * 32 else 401
        return status, "application/json", b"{}"


class ProxyRefreshConcurrencyTest(unittest.TestCase):
    def concurrent_requests(self, boundary, failed=False):
        second_started, second_done = threading.Event(), threading.Event()

        def second_request():
            second_started.set()
            try:
                return boundary.upstream_request("GET", "/api/v1/catalog")
            finally:
                second_done.set()

        with ThreadPoolExecutor(max_workers=2) as pool:
            first = pool.submit(boundary.upstream_request, "GET", "/api/v1/contracts")
            try:
                self.assertTrue(boundary.entered.wait(1), "Refresh did not start")
                self.assertTrue(proxy.parse_json(boundary.config_path.read_bytes())["refreshUncertain"])
                second = pool.submit(second_request)
                self.assertTrue(second_started.wait(1))
                self.assertFalse(second_done.wait(0.05), "Concurrent request rejected an active refresh")
            finally:
                boundary.release.set()
            if failed:
                with self.assertRaises(OSError):
                    first.result(timeout=2)
                with self.assertRaises(proxy.Rejected) as caught:
                    second.result(timeout=2)
                self.assertEqual((caught.exception.status, caught.exception.code),
                                 (503, "proxy_refresh_outcome_uncertain"))
            else:
                self.assertEqual(first.result(timeout=2)[0], 200)
                self.assertEqual(second.result(timeout=2)[0], 200)

    def test_parallel_requests_wait_for_one_rotation_without_deadlock(self):
        for expired in (True, False):
            with self.subTest(proactive_expiry=expired), tempfile.TemporaryDirectory(prefix="miy-refresh-test-") as directory:
                boundary = RefreshBoundary(directory, expired=expired)
                self.concurrent_requests(boundary)
                self.assertEqual(boundary.refresh_calls, 1)
                self.assertEqual(boundary.state["counts"]["refreshes"], 1)
                self.assertEqual(boundary.api_tokens[-2:], ["miy_access_" + "c" * 32] * 2)
                persisted = proxy.parse_json(boundary.config_path.read_bytes())
                self.assertFalse(persisted["refreshUncertain"])
                self.assertEqual(persisted["refreshToken"], "miy_refresh_" + "d" * 32)

    def test_lost_refresh_response_keeps_waiter_and_later_requests_fail_closed(self):
        with tempfile.TemporaryDirectory(prefix="miy-refresh-test-") as directory:
            boundary = RefreshBoundary(directory, lose_refresh_response=True)
            self.concurrent_requests(boundary, failed=True)
            boundary.config = proxy.parse_json(boundary.config_path.read_bytes())
            with self.assertRaises(proxy.Rejected) as caught:
                boundary.upstream_request("GET", "/api/v1/me")
            self.assertEqual(caught.exception.code, "proxy_refresh_outcome_uncertain")
            self.assertEqual(boundary.refresh_calls, 1)
            self.assertEqual(boundary.api_tokens, [])


class ProxyBodyAndRecoveryTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="miy-proxy-test-")
        self.boundary = FakeBoundary(Path(self.directory.name) / "proxy.sock")
        self.server = proxy.Server(self.boundary)
        self.thread = threading.Thread(target=self.server.serve_forever,
                                       kwargs={"poll_interval": 0.02}, daemon=True)
        self.thread.start()

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)
        self.directory.cleanup()

    def connect(self):
        client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.addCleanup(client.close)
        client.settimeout(3)
        client.connect(str(self.boundary.socket_path))
        return client

    def headers(self, method, path, length=0, extra=None):
        rows = {"Host": proxy.PLATFORM, "Content-Length": str(length),
                "Authorization": "Bearer " + self.boundary.config["proxyToken"],
                "Connection": "close", **(extra or {})}
        return (f"{method} http://{proxy.PLATFORM}{path} HTTP/1.1\r\n"
                + "".join(f"{key}: {value}\r\n" for key, value in rows.items()) + "\r\n").encode()

    def response(self, client):
        response = http.client.HTTPResponse(client)
        response.begin()
        return response.status, json.loads(response.read())

    def test_fragmented_300kib_upload_reaches_upstream_byte_for_byte(self):
        body = bytes(range(256)) * 1200
        request_id = str(uuid.uuid4())
        client = self.connect()
        client.sendall(self.headers("POST", f"/api/v1/apps/{self.boundary.app}/builds", len(body), {
            "Content-Type": "application/x-tar", "X-Miy-Request-Id": request_id,
            "X-Miy-Source-Commit": "a" * 40, "X-Miy-Platform": "linux/amd64",
            "X-Miy-Archive-Sha256": hashlib.sha256(body).hexdigest(),
        }))
        client.sendall(body[:137])
        # Ensure the unbuffered reader sees an incomplete prefix before the rest.
        time.sleep(0.03)
        for offset in range(137, len(body), 1379):
            client.sendall(body[offset:offset + 1379])
            time.sleep(0.0005)
        status, result = self.response(client)
        self.assertEqual(status, 200)
        self.assertEqual(result["requestId"], request_id)
        self.assertEqual(len(self.boundary.calls), 1)
        self.assertEqual(self.boundary.calls[0][2], body)
        self.assertIn(result["id"], self.boundary.state["builds"])

    def test_unrecorded_own_request_lookup_preserves_404_and_foreign_ids_stay_denied(self):
        request_id = str(uuid.uuid4())
        own_path = f"/api/v1/apps/{self.boundary.app}/build-requests/{request_id}"
        client = self.connect()
        client.sendall(self.headers("GET", own_path))
        status, result = self.response(client)
        self.assertEqual((status, result["error"]["code"]), (404, "not_found"))
        self.assertEqual(self.boundary.calls[0][:2], ("GET", own_path))
        for path in (f"/api/v1/apps/another-app/build-requests/{request_id}",
                     f"/api/v1/builds/{request_id}", f"/api/v1/runtime/jobs/{request_id}"):
            client = self.connect()
            client.sendall(self.headers("GET", path))
            status, result = self.response(client)
            self.assertEqual((status, result["error"]["code"]), (403, "proxy_scope_denied"))
        self.assertEqual(len(self.boundary.calls), 1)

    def test_body_eof_is_fixed_400_without_upstream_request(self):
        client = self.connect()
        client.sendall(self.headers("POST", f"/api/v1/apps/{self.boundary.app}/builds", 300000) + b"prefix")
        client.shutdown(socket.SHUT_WR)
        status, result = self.response(client)
        self.assertEqual((status, result["error"]["code"]), (400, "proxy_incomplete_request_body"))
        self.assertEqual(self.boundary.calls, [])

    def test_body_timeout_is_fixed_408_and_restores_socket_timeout(self):
        handler = object.__new__(proxy.Handler)
        handler.connection = Mock()
        handler.connection.gettimeout.return_value = 90
        handler.rfile = Mock(spec=io.RawIOBase)
        handler.rfile.read.side_effect = TimeoutError()
        with self.assertRaises(proxy.Rejected) as caught:
            handler.read_body(300000)
        self.assertEqual((caught.exception.status, caught.exception.code), (408, "proxy_request_body_timeout"))
        self.assertEqual(handler.connection.settimeout.call_args.args, (90,))


if __name__ == "__main__":
    unittest.main()
