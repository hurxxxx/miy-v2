#!/usr/bin/env python3
"""App-scoped egress boundary for an independently sandboxed acceptance agent.

Run outside the agent sandbox: python3 agent-acceptance-proxy.py --config FILE
FILE must be private JSON with upstream (HTTPS origin), appId, accessToken,
proxyToken, socket, auditFile and stateFile. Optional refreshToken and
accessExpiresAt (Unix seconds or RFC3339) enable serialized refresh rotation.
The file is atomically updated after rotation; the operator must use its latest
refreshToken for final logout. Never mount this file/state/audit into the agent.

Inside a network namespace, expose only the Unix socket through a local TCP
bridge and configure HTTP(S)_PROXY. Platform origin is exactly
http://miy-acceptance.invalid; its private synthetic CLI credential must contain
proxyToken, never the real tokens. A socket mount alone does not isolate a host:
the caller must also deny host network, source, secrets and management sockets.

No app registration, production, global auth, grants, user administration,
local image upload, DB preparation or development tunnel endpoint is exposed.
Remote builds and deployment do not require a client Docker socket. Browser
acceptance runs separately, outside the development agent's filesystem.
"""

import argparse
import datetime
import hashlib
import hmac
import http.client
import http.server
import ipaddress
import json
import os
from pathlib import Path
import re
import select
import signal
import socket
import socketserver
import ssl
import stat
import tempfile
import threading
import time
import urllib.parse
import uuid


PLATFORM = "miy-acceptance.invalid"
MAX_SOURCE = 64 * 1024 * 1024
MAX_JSON = 256 * 1024
MAX_API = 4 * 1024 * 1024
MAX_RESPONSE = 16 * 1024 * 1024
CONNECT_DOMAINS = (
    "openai.com", "chatgpt.com", "oaistatic.com", "oaiusercontent.com",
    "anthropic.com", "claude.ai", "claude.com",
    "registry.npmjs.org", "crates.io", "static.rust-lang.org",
    "github.com", "githubusercontent.com",
)
PUBLIC_PATHS = {"/api/health", "/api/v1/capabilities"}
READ_PATHS = PUBLIC_PATHS | {
    "/api/v1/contracts", "/api/v1/catalog", "/api/v1/openapi.json",
    "/api/v1/components", "/api/v1/me",
}
BUILD_HEADERS = {
    "x-miy-source-commit", "x-miy-archive-sha256", "x-miy-platform",
    "x-miy-request-id",
}
ID_PATTERN = r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"
ACTION_SET = {"deploy", "start", "stop", "restart"}
STATE_SETS = ("builds", "runtimeJobs", "buildRequests", "runtimeRequests", "images", "releases")


class Rejected(Exception):
    def __init__(self, code="proxy_scope_denied", status=403):
        self.code, self.status = code, status
        super().__init__(code)


def require(condition, code="proxy_scope_denied", status=403):
    if not condition:
        raise Rejected(code, status)


def identifier(value):
    return isinstance(value, str) and re.fullmatch(ID_PATTERN, value) and uuid.UUID(value).int != 0


def image_id(value):
    return isinstance(value, str) and re.fullmatch(r"sha256:[0-9a-f]{64}", value)


def json_bytes(value):
    return json.dumps(value, ensure_ascii=True, separators=(",", ":")).encode()


def parse_json(body):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "proxy_duplicate_json_key", 400)
            result[key] = value
        return result
    try:
        return json.loads(body, object_pairs_hook=unique,
                          parse_constant=lambda _: (_ for _ in ()).throw(ValueError()))
    except Rejected:
        raise
    except (ValueError, UnicodeError, RecursionError):
        raise Rejected("proxy_invalid_json", 400) from None


def private_parent(path):
    require(path.is_absolute(), "proxy_absolute_private_path_required", 500)
    info = path.parent.lstat()
    require(stat.S_ISDIR(info.st_mode) and info.st_uid == os.geteuid()
            and info.st_mode & 0o077 == 0, "proxy_private_directory_required", 500)


def private_file(path):
    private_parent(path)
    info = path.lstat()
    require(stat.S_ISREG(info.st_mode) and info.st_uid == os.geteuid()
            and info.st_mode & 0o077 == 0, "proxy_private_file_required", 500)


def atomic_json(path, value):
    private_parent(path)
    if path.exists() or path.is_symlink():
        private_file(path)
    fd, name = tempfile.mkstemp(prefix=".proxy-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(json_bytes(value) + b"\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def expiry(value):
    if value is None:
        return None
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        require(0 < value < 10**11, "proxy_invalid_expiry", 500)
        return float(value)
    require(isinstance(value, str), "proxy_invalid_expiry", 500)
    try:
        parsed = datetime.datetime.fromisoformat(value.replace("Z", "+00:00"))
        require(parsed.tzinfo is not None, "proxy_invalid_expiry", 500)
        return parsed.timestamp()
    except ValueError:
        raise Rejected("proxy_invalid_expiry", 500) from None


class Boundary:
    def __init__(self, config_path):
        self.config_path = Path(config_path)
        private_file(self.config_path)
        require(self.config_path.stat().st_size <= MAX_JSON, "proxy_config_too_large", 500)
        self.config = parse_json(self.config_path.read_bytes())
        require(isinstance(self.config, dict), "proxy_invalid_config", 500)
        for name in ("upstream", "appId", "accessToken", "proxyToken", "socket", "auditFile", "stateFile"):
            require(isinstance(self.config.get(name), str) and self.config[name], "proxy_config_field_missing", 500)
        self.origin = self.config["upstream"]
        self.upstream = urllib.parse.urlsplit(self.origin)
        require(self.upstream.scheme == "https" and self.upstream.hostname
                and self.upstream.netloc and not self.upstream.username and not self.upstream.password
                and self.upstream.path in ("", "/") and not self.upstream.query and not self.upstream.fragment,
                "proxy_https_origin_required", 500)
        self.origin = "https://" + self.upstream.netloc
        self.app = self.config["appId"]
        require(re.fullmatch(r"[a-z][a-z0-9-]{0,62}", self.app), "proxy_invalid_app_id", 500)
        for key in ("accessToken", "proxyToken", "refreshToken"):
            value = self.config.get(key)
            require(value is None or (isinstance(value, str) and 32 <= len(value) <= 512
                    and re.fullmatch(r"[A-Za-z0-9_-]+", value)), "proxy_invalid_credential", 500)
        self.socket_path = Path(self.config["socket"])
        self.audit_path = Path(self.config["auditFile"])
        self.state_path = Path(self.config["stateFile"])
        for path in (self.socket_path, self.audit_path, self.state_path):
            private_parent(path)
        require(len({str(p) for p in (self.config_path, self.socket_path, self.audit_path, self.state_path)}) == 4,
                "proxy_private_paths_must_differ", 500)
        self.lock = threading.RLock()
        self.refresh_lock = threading.Lock()
        self.ssl_context = ssl.create_default_context()
        self.access_expiry = expiry(self.config.get("accessExpiresAt", self.config.get("expiresAt")))
        self.actor = None
        self.state = {"schemaVersion": 1, "appId": self.app,
                      "upstreamSha256": hashlib.sha256(self.origin.encode()).hexdigest(),
                      "counts": {"requests": 0, "denied": 0, "refreshes": 0},
                      "installations": {}, **{key: [] for key in STATE_SETS}}
        if self.state_path.exists() or self.state_path.is_symlink():
            private_file(self.state_path)
            require(self.state_path.stat().st_size <= MAX_JSON, "proxy_invalid_state", 500)
            stored = parse_json(self.state_path.read_bytes())
            require(isinstance(stored, dict) and all(stored.get(key) == self.state[key]
                    for key in ("schemaVersion", "appId", "upstreamSha256")), "proxy_state_scope_mismatch", 500)
            require(all(isinstance(stored.get(key), list) for key in STATE_SETS)
                    and isinstance(stored.get("installations"), dict) and isinstance(stored.get("counts"), dict),
                    "proxy_invalid_state", 500)
            self.state = stored
        if self.audit_path.exists() or self.audit_path.is_symlink():
            private_file(self.audit_path)
        self.audit_fd = os.open(self.audit_path, os.O_WRONLY | os.O_APPEND | os.O_CREAT | os.O_NOFOLLOW, 0o600)
        self.upstream_ips = {item[4][0] for item in socket.getaddrinfo(
            self.upstream.hostname, self.upstream.port or 443, type=socket.SOCK_STREAM)}

    def save(self):
        atomic_json(self.state_path, self.state)

    def audit(self, method, path, decision, status, received=0, sent=0):
        # Paths are route templates, not attacker-controlled URLs or query strings.
        with self.lock:
            self.state["counts"]["requests"] += 1
            if decision == "denied":
                self.state["counts"]["denied"] += 1
            event = {"method": method, "path": path, "decision": decision, "status": status,
                     "count": self.state["counts"]["requests"], "receivedBytes": received, "sentBytes": sent}
            os.write(self.audit_fd, json_bytes(event) + b"\n")
            self.save()

    def owns(self, kind, value):
        with self.lock:
            return value in self.state[kind]

    def remember(self, kind, value):
        with self.lock:
            if value not in self.state[kind]:
                require(len(self.state[kind]) < 2000, "proxy_scope_record_limit", 429)
                self.state[kind].append(value)
                self.save()

    def raw_upstream(self, method, path, body=b"", headers=None, token=None, refresh=False):
        outgoing = {"Host": self.upstream.netloc, "Accept": "application/json",
                    "Accept-Encoding": "identity", "Content-Length": str(len(body))}
        # MIY rejects *any* Origin on CLI Bearer requests (http::session_guard),
        # and public refresh also rejects Origin. Never forward a client Origin.
        if token:
            outgoing["Authorization"] = "Bearer " + token
        outgoing.update(headers or {})
        connection = http.client.HTTPSConnection(self.upstream.hostname, self.upstream.port or 443,
                                                 timeout=90, context=self.ssl_context)
        try:
            connection.request(method, path, body=body, headers=outgoing)
            response = connection.getresponse()
            require(not 300 <= response.status < 400, "proxy_upstream_redirect_denied", 502)
            payload = response.read(MAX_RESPONSE + 1)
            require(len(payload) <= MAX_RESPONSE, "proxy_upstream_response_limit", 502)
            return response.status, response.getheader("Content-Type", "application/json"), payload
        finally:
            connection.close()

    def refresh(self, previous=None):
        with self.refresh_lock:
            require(not self.config.get("refreshUncertain"), "proxy_refresh_outcome_uncertain", 503)
            if previous is not None and self.config["accessToken"] != previous:
                return
            require(self.config.get("refreshToken"), "proxy_access_expired_operator_required", 401)
            # Persist uncertainty before transmission. A lost response must never
            # replay a one-use refresh token, including after process restart.
            self.config["refreshUncertain"] = True
            atomic_json(self.config_path, self.config)
            status, _, body = self.raw_upstream("POST", "/auth/cli/token",
                json_bytes({"grantType": "refresh_token", "refreshToken": self.config["refreshToken"]}),
                {"Content-Type": "application/json"}, refresh=True)
            require(status == 200, "proxy_refresh_failed_operator_required", 503)
            value = parse_json(body)
            require(isinstance(value, dict) and value.get("status") == "approved"
                    and isinstance(value.get("accessToken"), str) and value["accessToken"].startswith("miy_access_")
                    and isinstance(value.get("refreshToken"), str) and value["refreshToken"].startswith("miy_refresh_")
                    and isinstance(value.get("expiresIn"), int) and 1 <= value["expiresIn"] <= 900
                    and (self.actor is None or value.get("userId") == self.actor),
                    "proxy_invalid_refresh_response", 503)
            self.config.update(accessToken=value["accessToken"], refreshToken=value["refreshToken"],
                               accessExpiresAt=time.time() + value["expiresIn"], refreshUncertain=False)
            atomic_json(self.config_path, self.config)
            self.access_expiry = self.config["accessExpiresAt"]
            with self.lock:
                self.state["counts"]["refreshes"] += 1
                self.save()

    def access_snapshot(self):
        # An in-flight rotation holds this lock. Wait for its result before
        # treating the durable replay guard as an uncertain, abandoned refresh.
        with self.refresh_lock:
            require(not self.config.get("refreshUncertain"), "proxy_refresh_outcome_uncertain", 503)
            return self.config["accessToken"], self.access_expiry, bool(self.config.get("refreshToken"))

    def upstream_request(self, method, path, body=b"", headers=None):
        token, access_expiry, can_refresh = self.access_snapshot()
        if access_expiry is not None and access_expiry <= time.time() + 45:
            # Call refresh outside access_snapshot's non-reentrant lock.
            self.refresh(previous=token)
            token, _, can_refresh = self.access_snapshot()
        response = self.raw_upstream(method, path, body, headers, token)
        if response[0] == 401 and can_refresh:
            self.refresh(previous=token)
            token, _, _ = self.access_snapshot()
            response = self.raw_upstream(method, path, body, headers, token)
        return response

    def initialize_identity(self):
        status, _, body = self.upstream_request("GET", "/api/v1/me")
        require(status == 200, "proxy_developer_authentication_failed", 500)
        value = parse_json(body)
        require(isinstance(value, dict) and isinstance(value.get("id"), str)
                and value.get("admin") is False and isinstance(value.get("grants"), list)
                and any(row.get("appId") == self.app and row.get("environment") == "dev"
                        and row.get("permission") == "develop" for row in value["grants"]),
                "proxy_nonadmin_app_developer_required", 500)
        self.actor = value["id"]
        with self.lock:
            self.save()

    def connect_target(self, authority):
        require(re.fullmatch(r"[A-Za-z0-9.-]+:443", authority), "proxy_connect_target_denied")
        host = authority[:-4].lower()
        require(host != self.upstream.hostname.lower() and host != PLATFORM and not host.endswith(".")
                and any(host == domain or host.endswith("." + domain) for domain in CONNECT_DOMAINS),
                "proxy_connect_domain_denied")
        answers = socket.getaddrinfo(host, 443, type=socket.SOCK_STREAM)
        require(answers, "proxy_connect_dns_failed", 502)
        for _, _, _, _, address in answers:
            ip = ipaddress.ip_address(address[0])
            require(ip.is_global and address[0] not in self.upstream_ips
                    and not getattr(ip, "ipv4_mapped", None), "proxy_connect_address_denied")
        # Connect to the checked numeric address, not a second DNS resolution.
        family, socktype, proto, _, address = answers[0]
        remote = socket.socket(family, socktype, proto)
        try:
            remote.settimeout(20)
            remote.connect(address)
            return remote
        except Exception:
            remote.close()
            raise

    def request_route(self, method, target, headers, body):
        require(len(target) <= 8192, "proxy_url_limit", 414)
        try:
            url = urllib.parse.urlsplit(target)
            port = url.port
        except ValueError:
            raise Rejected("proxy_platform_origin_denied") from None
        require(url.scheme == "http" and url.hostname == PLATFORM and port in (None, 80)
                and not url.username and not url.password and not url.fragment,
                "proxy_platform_origin_denied")
        path = url.path or "/"
        require(len(path) <= 2048 and re.fullmatch(r"/[A-Za-z0-9_./@-]*", path)
                and "//" not in path and all(part not in (".", "..") for part in path.split("/")),
                "proxy_path_denied")
        require(not headers.get("Transfer-Encoding"), "proxy_transfer_encoding_denied", 400)
        if path not in PUBLIC_PATHS or method != "GET":
            supplied = headers.get_all("Authorization", [])
            require(len(supplied) == 1 and hmac.compare_digest(supplied[0], "Bearer " + self.config["proxyToken"]),
                    "proxy_credential_required", 401)
        try:
            pairs = urllib.parse.parse_qsl(url.query, keep_blank_values=True, strict_parsing=True,
                                          max_num_fields=12, errors="strict")
        except (ValueError, UnicodeError):
            raise Rejected("proxy_query_denied", 400) from None
        query = dict(pairs)
        require(len(query) == len(pairs) and all(len(v) <= 4096 for v in query.values()), "proxy_query_denied")
        route = {"kind": "read", "path": path, "audit": path, "query": query, "id": None}

        def queries(allowed):
            require(set(query) <= set(allowed), "proxy_query_denied")
            if "limit" in query:
                require(query["limit"].isdigit() and 1 <= int(query["limit"]) <= 200, "proxy_query_denied")

        def scope():
            require(query.get("appId", self.app) == self.app and query.get("environment", "dev") == "dev")
            query.update(appId=self.app, environment="dev")

        if method == "GET" and path in READ_PATHS:
            queries(())
            route["kind"] = "identity" if path.endswith("/me") else "read"
        elif method == "GET" and re.fullmatch(r"/api/v1/components/[a-z][a-z0-9-]*/[0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.-]+)?", path):
            queries(())
            route["audit"] = "/api/v1/components/{name}/{version}"
        elif method == "GET" and path == "/api/v1/apps":
            queries(("appId", "limit", "cursor"))
            require(query.get("appId", self.app) == self.app)
            query["appId"] = self.app
            route["kind"] = "apps"
        elif method == "GET" and path == f"/api/v1/apps/{self.app}":
            queries(())
            route.update(kind="app", audit="/api/v1/apps/{app}")
        elif method == "GET" and path in ("/api/v1/installations", "/api/v1/runtime/jobs"):
            queries(("appId", "environment", "limit", "cursor", "requestId"))
            scope()
            route["kind"] = "installations" if path.endswith("installations") else "jobs"
            if "requestId" in query:
                require(route["kind"] == "jobs" and self.owns("runtimeRequests", query["requestId"]))
        elif method == "GET" and (match := re.fullmatch(rf"/api/v1/builds/({ID_PATTERN})(/logs)?", path)):
            target_id = match[1]
            if (self.owns("runtimeJobs", target_id) or self.owns("runtimeRequests", target_id)) and not match[2]:
                route["kind"] = "not_build"  # Required by CLI's build-first job dispatch.
            else:
                require(self.owns("builds", target_id))
                route["kind"] = "build_logs" if match[2] else "build"
            queries(("after", "limit") if match[2] else ())
            route.update(id=target_id, audit="/api/v1/builds/{owned}" + ("/logs" if match[2] else ""))
        elif method == "GET" and (match := re.fullmatch(rf"/api/v1/apps/{re.escape(self.app)}/build-requests/({ID_PATTERN})", path)):
            queries(())
            # Lost acknowledgements may predate this proxy's local record. The
            # server scopes this lookup to the selected app and bearer actor;
            # a missing request must remain 404 for the CLI's recovery logic.
            route.update(kind="build", request=match[1], audit="/api/v1/apps/{app}/build-requests/{owned}")
        elif method == "GET" and (match := re.fullmatch(rf"/api/v1/runtime/jobs/({ID_PATTERN})(/events)?", path)):
            require(self.owns("runtimeJobs", match[1]))
            queries(("after", "limit") if match[2] else ())
            route.update(kind="events" if match[2] else "runtime", id=match[1],
                         audit="/api/v1/runtime/jobs/{owned}" + ("/events" if match[2] else ""))
        elif method == "POST" and path == f"/api/v1/apps/{self.app}/builds":
            queries(())
            request = headers.get_all("X-Miy-Request-Id", [])
            require(len(request) == 1 and identifier(request[0]), "proxy_build_request_required", 400)
            for key in BUILD_HEADERS:
                require(len(headers.get_all(key, [])) == 1, "proxy_build_headers_required", 400)
            require(re.fullmatch(r"[0-9a-f]{40}", headers.get("x-miy-source-commit", ""))
                    and re.fullmatch(r"[0-9a-f]{64}", headers.get("x-miy-archive-sha256", ""))
                    and headers.get("x-miy-platform") in ("linux/amd64", "linux/arm64")
                    and 0 < len(body) <= MAX_SOURCE, "proxy_invalid_build_source", 400)
            require(hashlib.sha256(body).hexdigest() == headers["x-miy-archive-sha256"], "proxy_build_hash_mismatch", 400)
            self.remember("buildRequests", request[0])
            route.update(kind="build", request=request[0], audit="/api/v1/apps/{app}/builds")
        elif method == "POST" and path in (f"/api/v1/apps/{self.app}/runtime/preflight", f"/api/v1/apps/{self.app}/runtime"):
            queries(())
            require(len(body) <= MAX_JSON, "proxy_json_limit", 413)
            value = parse_json(body)
            require(isinstance(value, dict) and value.get("action") in ACTION_SET)
            planning = path.endswith("/preflight")
            expected = value.get("expectedState")
            require(planning or isinstance(expected, dict))
            target_value = value.get("target") if planning else expected.get("target")
            require(isinstance(target_value, dict) and target_value.get("appId") == self.app
                    and target_value.get("environment") == "dev"
                    and (planning or value.get("environment") == "dev"))
            require(value.get("releaseId") is None)
            if value["action"] == "deploy":
                require(self.owns("images", value.get("imageId")))
            else:
                require(value.get("imageId") is None)
            if not planning:
                require(identifier(value.get("idempotencyKey")), "proxy_runtime_request_required", 400)
                self.remember("runtimeRequests", value["idempotencyKey"])
                route["request"] = value["idempotencyKey"]
            route.update(kind="preflight" if planning else "runtime",
                         audit="/api/v1/apps/{app}/runtime" + ("/preflight" if planning else ""))
        elif (method in ("GET", "HEAD", "POST", "PUT", "PATCH", "DELETE")
              and (match := re.fullmatch(rf"/api/v1/installations/({ID_PATTERN})/api/(.+)", path))):
            with self.lock:
                installed = self.state["installations"].get(match[1])
            require(installed is not None)
            require(len(body) <= MAX_API, "proxy_application_body_limit", 413)
            if method not in ("GET", "HEAD"):
                require(headers.get("x-miy-release") == installed.get("releaseId"), "proxy_release_header_required")
            route.update(kind="application", id=match[1], audit="/api/v1/installations/{owned}/api/{path}")
        else:
            raise Rejected()
        if method in ("GET", "HEAD"):
            require(not body, "proxy_unexpected_request_body", 400)
        route["upstream"] = path + ("?" + urllib.parse.urlencode(query) if query else "")
        return route

    def track_job(self, value, kind, expected_id=None, request=None):
        require(isinstance(value, dict) and value.get("appId") == self.app and value.get("environment") == "dev"
                and identifier(value.get("id")), "proxy_upstream_scope_mismatch", 502)
        require(expected_id is None or value["id"] == expected_id, "proxy_upstream_id_mismatch", 502)
        if kind == "build":
            require(value.get("action") == "build" and value.get("actor") == self.actor and value.get("userId") == self.actor,
                    "proxy_upstream_actor_mismatch", 502)
            require(request is None or value.get("requestId") == request, "proxy_upstream_request_mismatch", 502)
            self.remember("builds", value["id"])
        else:
            require(value.get("action") in ACTION_SET and self.owns("runtimeRequests", value.get("requestId")),
                    "proxy_upstream_request_mismatch", 502)
            require(request is None or value.get("requestId") == request, "proxy_upstream_request_mismatch", 502)
            self.remember("runtimeJobs", value["id"])
        if image_id(value.get("imageId")) and (kind != "build" or value.get("status") == "succeeded"):
            self.remember("images", value["imageId"])
        if identifier(value.get("releaseId")):
            self.remember("releases", value["releaseId"])
        if identifier(value.get("installationId")):
            with self.lock:
                self.state["installations"][value["installationId"]] = {"releaseId": value.get("releaseId")}
                self.save()

    def track_installation(self, value):
        require(isinstance(value, dict) and value.get("appId") == self.app and value.get("environment") == "dev"
                and identifier(value.get("id")), "proxy_upstream_scope_mismatch", 502)
        with self.lock:
            self.state["installations"][value["id"]] = {"releaseId": value.get("releaseId")}
            self.save()
        if image_id(value.get("imageId")):
            self.remember("images", value["imageId"])
        if identifier(value.get("releaseId")):
            self.remember("releases", value["releaseId"])
        value["canPromote"] = False
        if isinstance(value.get("lastOperation"), dict) and not self.owns("runtimeJobs", value["lastOperation"].get("id")):
            value["lastOperation"] = None

    def filter_response(self, route, body):
        if route["kind"] == "application":
            return body
        value = parse_json(body)
        kind = route["kind"]
        if kind == "identity":
            require(isinstance(value, dict) and value.get("id") == self.actor and value.get("admin") is False,
                    "proxy_upstream_actor_mismatch", 502)
            value = {"id": self.actor, "name": "Acceptance developer", "admin": False, "csrf": None,
                     "development": value.get("development") is True,
                     "grants": [row for row in value.get("grants", []) if row.get("appId") == self.app
                                and row.get("environment") == "dev" and row.get("permission") in ("use", "develop")]}
        elif kind == "apps":
            require(isinstance(value, dict) and isinstance(value.get("apps"), list), "proxy_invalid_upstream_page", 502)
            value["apps"] = [row for row in value["apps"] if row.get("id") == self.app]
        elif kind in ("app", "installations"):
            require(isinstance(value, dict) and isinstance(value.get("installations"), list), "proxy_invalid_upstream_page", 502)
            if kind == "app":
                require(isinstance(value.get("app"), dict) and value["app"].get("id") == self.app,
                        "proxy_upstream_scope_mismatch", 502)
            rows = [row for row in value["installations"] if row.get("appId") == self.app and row.get("environment") == "dev"]
            for row in rows:
                self.track_installation(row)
            value["installations"] = rows
        elif kind in ("build", "runtime"):
            self.track_job(value, kind, route.get("id"), route.get("request"))
        elif kind == "jobs":
            require(isinstance(value, dict) and isinstance(value.get("jobs"), list), "proxy_invalid_upstream_page", 502)
            rows = [row for row in value["jobs"] if row.get("appId") == self.app and row.get("environment") == "dev"
                    and self.owns("runtimeRequests", row.get("requestId"))]
            for row in rows:
                self.track_job(row, "runtime")
            value["jobs"] = rows
        elif kind == "build_logs":
            require(isinstance(value, dict) and value.get("id") == route["id"] and value.get("jobId") == route["id"],
                    "proxy_upstream_id_mismatch", 502)
        elif kind == "events":
            require(isinstance(value, dict) and value.get("jobId") == route["id"] and isinstance(value.get("events"), list),
                    "proxy_upstream_id_mismatch", 502)
            for event in value["events"]:
                # Atomic event snapshots intentionally omit requestId/image IDs.
                # The enclosing job is already owned; do not infer new ownership.
                job = event.get("job") if isinstance(event, dict) else None
                require(isinstance(job, dict) and job.get("id") == route["id"]
                        and job.get("appId") == self.app and job.get("environment") == "dev"
                        and job.get("action") in ACTION_SET, "proxy_upstream_scope_mismatch", 502)
        elif kind == "preflight":
            require(isinstance(value, dict) and value.get("expectedState", {}).get("target") == {"appId": self.app, "environment": "dev"},
                    "proxy_upstream_scope_mismatch", 502)
        return json_bytes(value)


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server_version = "AcceptanceProxy"
    sys_version = ""
    rbufsize = 0  # CONNECT must not lose TLS bytes to the HTTP reader's buffer.

    def setup(self):
        super().setup()
        self.connection.settimeout(90)

    def log_message(self, *_):
        pass

    def send_error(self, code, message=None, explain=None):
        # BaseHTTPRequestHandler otherwise reflects invalid raw input in HTML.
        self.reply(code, json_bytes({"error": {"code": "proxy_invalid_http_request"}}))
        self.server.boundary.audit("INVALID", "{unapproved}", "denied", code)

    def reply(self, status, body, content_type="application/json"):
        self.send_response_only(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("Connection", "close")
        self.end_headers()
        self.close_connection = True
        if self.command != "HEAD":
            self.wfile.write(body)

    def read_body(self, length):
        require(0 <= length <= MAX_SOURCE, "proxy_request_body_limit", 413)
        previous_timeout = self.connection.gettimeout()
        deadline = time.monotonic() + 90
        body = bytearray()
        try:
            while len(body) < length:
                remaining = deadline - time.monotonic()
                require(remaining > 0, "proxy_request_body_timeout", 408)
                self.connection.settimeout(remaining)
                try:
                    # rbufsize=0 is needed for CONNECT, so one read can return
                    # any available prefix, even when Content-Length is larger.
                    chunk = self.rfile.read(min(65536, length - len(body)))
                except TimeoutError:
                    raise Rejected("proxy_request_body_timeout", 408) from None
                except OSError:
                    raise Rejected("proxy_incomplete_request_body", 400) from None
                require(chunk, "proxy_incomplete_request_body", 400)
                body.extend(chunk)
            return bytes(body)
        finally:
            self.connection.settimeout(previous_timeout)

    def do_CONNECT(self):
        boundary = self.server.boundary
        remote, status, decision, received, sent = None, 502, "failed", 0, 0
        established = False
        try:
            require(not self.headers.get("Transfer-Encoding") and self.headers.get("Content-Length", "0") == "0",
                    "proxy_connect_body_denied")
            remote = boundary.connect_target(self.path)
            self.connection.sendall(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            established, status, decision = True, 200, "allowed"
            remote.settimeout(20)
            deadline, last_activity = time.monotonic() + 3600, time.monotonic()
            while time.monotonic() < deadline and time.monotonic() - last_activity < 180:
                readable, _, _ = select.select([self.connection, remote], [], [], 10)
                if not readable:
                    continue
                for source in readable:
                    chunk = source.recv(65536)
                    if not chunk:
                        return
                    if source is self.connection:
                        received += len(chunk)
                        remote.sendall(chunk)
                    else:
                        sent += len(chunk)
                        self.connection.sendall(chunk)
                    require(received + sent <= 512 * 1024 * 1024, "proxy_connect_byte_limit", 413)
                    last_activity = time.monotonic()
        except Rejected as failure:
            status, decision = failure.status, "denied"
            if not established:
                self.reply(status, json_bytes({"error": {"code": failure.code}}))
        except (OSError, ValueError):
            if not established:
                self.reply(502, json_bytes({"error": {"code": "proxy_connect_failed"}}))
        finally:
            self.close_connection = True
            if remote:
                remote.close()
            boundary.audit("CONNECT", "{allowed-egress}", decision, status, received, sent)

    def platform_request(self):
        boundary, route, received, sent, status, decision = self.server.boundary, None, 0, 0, 502, "failed"
        try:
            lengths = self.headers.get_all("Content-Length", [])
            require(len(lengths) <= 1 and (not lengths or re.fullmatch(r"[0-9]{1,9}", lengths[0])),
                    "proxy_invalid_content_length", 400)
            require(not self.headers.get("Transfer-Encoding"), "proxy_transfer_encoding_denied", 400)
            received = int(lengths[0]) if lengths else 0
            require(received <= MAX_SOURCE, "proxy_request_body_limit", 413)
            body = self.read_body(received)
            route = boundary.request_route(self.command, self.path, self.headers, body)
            if route["kind"] == "not_build":
                status, payload, content_type = 404, json_bytes({"error": {"code": "not_found"}}), "application/json"
            else:
                headers = {key: self.headers[key] for key in BUILD_HEADERS if key in self.headers} if route["kind"] == "build" and self.command == "POST" else {}
                for key in ("Content-Type", "Accept", "X-Miy-Release"):
                    values = self.headers.get_all(key, [])
                    require(len(values) <= 1 and (not values or len(values[0]) <= 256), "proxy_header_denied", 400)
                    if values:
                        headers[key] = values[0]
                status, content_type, payload = boundary.upstream_request(self.command, route["upstream"], body, headers)
                if 200 <= status < 300:
                    payload = boundary.filter_response(route, payload)
                elif route["kind"] != "application":
                    # Preserve stable server diagnostics, never arbitrary response text.
                    try:
                        code = parse_json(payload).get("error", {}).get("code")
                    except (Rejected, AttributeError):
                        code = None
                    if not isinstance(code, str) or not re.fullmatch(r"[A-Za-z0-9_.-]{1,96}", code):
                        code = "proxy_upstream_request_failed"
                    payload, content_type = json_bytes({"error": {"code": code}}), "application/json"
            sent, decision = len(payload), "allowed"
            self.reply(status, payload, content_type)
        except Rejected as failure:
            status, decision = failure.status, "denied"
            payload = json_bytes({"error": {"code": failure.code}})
            self.reply(status, payload)
        except (OSError, ValueError, TypeError, AttributeError, http.client.HTTPException):
            status = 502
            self.reply(status, json_bytes({"error": {"code": "proxy_upstream_unavailable"}}))
        finally:
            boundary.audit(self.command, route["audit"] if route else "{unapproved}", decision, status, received, sent)

    do_GET = platform_request
    do_HEAD = platform_request
    do_POST = platform_request
    do_PUT = platform_request
    do_PATCH = platform_request
    do_DELETE = platform_request
    do_OPTIONS = platform_request


class Server(socketserver.ThreadingMixIn, socketserver.UnixStreamServer):
    daemon_threads = True
    block_on_close = False
    request_queue_size = 32

    def __init__(self, boundary):
        self.boundary = boundary
        self.slots = threading.BoundedSemaphore(48)
        super().__init__(str(boundary.socket_path), Handler)

    def process_request(self, request, client_address):
        if not self.slots.acquire(blocking=False):
            request.close()
            return
        try:
            super().process_request(request, client_address)
        except Exception:
            self.slots.release()
            raise

    def process_request_thread(self, request, client_address):
        try:
            super().process_request_thread(request, client_address)
        finally:
            self.slots.release()

    def handle_error(self, request, client_address):
        # Never let socketserver print a traceback containing upstream details.
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", required=True)
    args = parser.parse_args()
    os.umask(0o077)
    boundary, server, inode = None, None, None
    try:
        boundary = Boundary(args.config)
        require(not boundary.socket_path.exists() and not boundary.socket_path.is_symlink(), "proxy_socket_already_exists", 500)
        boundary.initialize_identity()
        server = Server(boundary)
        os.chmod(boundary.socket_path, 0o600)
        inode = boundary.socket_path.stat().st_ino
        def stopped(_signum, _frame):
            raise KeyboardInterrupt
        signal.signal(signal.SIGTERM, stopped)
        signal.signal(signal.SIGINT, stopped)
        print(json.dumps({"status": "ready", "schemaVersion": 1, "platformOrigin": "http://" + PLATFORM}), flush=True)
        server.serve_forever(poll_interval=0.25)
    except KeyboardInterrupt:
        return 0
    except Rejected as failure:
        print(json.dumps({"status": "failed", "code": failure.code}), flush=True)
        return 1
    except Exception:
        print(json.dumps({"status": "failed", "code": "proxy_start_or_runtime_failed"}), flush=True)
        return 1
    finally:
        if server:
            server.server_close()
        if boundary:
            if inode is not None and boundary.socket_path.exists() and boundary.socket_path.stat().st_ino == inode:
                boundary.socket_path.unlink()
            os.close(boundary.audit_fd)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
