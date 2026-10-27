#!/usr/bin/env python3
"""Exercise an isolated HTTP/IP server with real Keycloak and browser/CLI login.

Requires Docker Compose, a built web/platform/dist, the Playwright dependency,
Chrome, and a server-capable binary (default .local/bin/miy-server). Every
container, network, volume, port, config, and CLI credential belongs to a fresh
fixture. Live server configuration is never sourced or modified.
"""
import argparse
import hashlib
import ipaddress
import json
import os
from pathlib import Path
import secrets
import shlex
import shutil
import signal
import socket
import subprocess
import sys
import time
import urllib.request


ROOT = Path(__file__).resolve().parent.parent
OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def local_ip():
    # Selecting a route does not send a packet to this documentation address.
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as connection:
        connection.connect(("192.0.2.1", 9))
        return connection.getsockname()[0]


def free_port(address, selected):
    while True:
        with socket.socket() as listener:
            listener.bind((address, 0))
            value = listener.getsockname()[1]
        if value not in selected:
            selected.add(value)
            return value


def clean_env():
    return {
        key: value for key, value in os.environ.items()
        if not key.startswith("MIY_")
        and key not in ("DATABASE_URL", "PROVISION_DATABASE_URL", "RUST_LOG")
        and key.lower() not in ("http_proxy", "https_proxy", "all_proxy", "no_proxy")
    }


def runtime_env(path):
    values = {}
    for line in path.read_text().splitlines():
        key, value = line.split("=", 1)
        parts = shlex.split(value)
        require(len(parts) == 1, "Invalid generated runtime environment")
        values[key] = parts[0]
    values.pop("PROVISION_DATABASE_URL", None)
    return values


def wait_json(url, seconds=180):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        try:
            with OPENER.open(url, timeout=2) as response:
                return json.load(response)
        except (OSError, ValueError):
            time.sleep(1)
    raise RuntimeError("HTTP readiness timed out; inspect the private fixture logs")


# No HTTP response bodies, CLI credentials, OAuth URLs/codes, passwords, browser
# traces, or storage state are written to public test output.
BROWSER = r'''
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { chromium } from "@playwright/test";
const fixture = process.argv[2];
const settings = JSON.parse(readFileSync(join(fixture, "server.json"), "utf8"));
const secrets = JSON.parse(readFileSync(join(fixture, "credentials.json"), "utf8"));
const origin = settings.origin;
const expectedUser = "00000000-0000-4000-8000-000000000002";
const cli = join(fixture, "installed", "miy");
const passed = [];
function pass(name) { passed.push(name); console.log(JSON.stringify({status:"PASS",test:name})); }
function command(args) {
  const output = execFileSync(cli, ["--output", "json", "--no-input", ...args], {
    cwd:fixture, env:{...process.env,MIY_CONFIG_DIR:join(fixture,"cli-config"),MIY_CREDENTIAL_BACKEND:"file",RUST_LOG:"off"},
    encoding:"utf8", timeout:60000, stdio:["ignore","pipe","pipe"],
  });
  assert(!/miy_(?:access|refresh|device)_/.test(output), "CLI exposed an authentication secret");
  const envelope = JSON.parse(output);
  assert.equal(envelope.ok, true, "CLI command failed");
  return envelope.data;
}
const browser = await chromium.launch({channel:process.env.MIY_TEST_BROWSER_CHANNEL || "chrome", headless:true,
  args:["--no-proxy-server"]});
try {
  const context = await browser.newContext();
  const page = await context.newPage();
  page.setDefaultTimeout(30000);
  const guideResponse = await context.request.get(origin+"/api/v1/cli/install");
  assert.equal(guideResponse.status(),200);
  const guide = await guideResponse.json();
  assert.equal(guide.serverUrl,origin);
  assert(guide.commands.unix.includes(origin));
  assert(guide.commands.windows.includes(origin));
  const forged = await context.request.get(origin+"/api/v1/cli/install", {
    headers:{"X-Forwarded-Host":"attacker.invalid","X-Forwarded-Proto":"https"},
  });
  assert.equal((await forged.json()).serverUrl,origin);
  pass("installation guidance uses the configured HTTP IP origin and ignores forged forwarding headers");

  const options = await (await context.request.get(origin+"/auth/options")).json();
  assert.equal(options.developmentLogin,false);
  assert.equal(options.accounts.length,0);
  assert.equal(options.oauthUrl,origin+"/auth/login");
  assert.equal((await context.request.post(origin+"/auth/development",{
    headers:{origin},data:{account:"developer"},
  })).status(),404);
  assert.equal((await context.request.get(origin+"/api/v1/me")).status(),401);
  pass("HTTP server profile disables synthetic login and requires authentication");

  await page.goto(origin+"/auth/login");
  await page.locator("#username").waitFor();
  assert.equal(new URL(page.url()).origin,origin);
  assert(new URL(page.url()).pathname.startsWith("/identity/realms/miy/"));
  await page.locator("#username").fill("developer");
  await page.locator("#password").fill(secrets.developer);
  await page.locator("#kc-login").click();
  await page.waitForURL(url => url.origin === origin && url.pathname === "/");
  const meResponse = await context.request.get(origin+"/api/v1/me");
  assert.equal(meResponse.status(),200);
  const me = await meResponse.json();
  assert.equal(me.id,expectedUser);
  assert.equal(me.name,"developer");
  const cookie = (await context.cookies(origin)).find(item => item.name === "miy_session");
  assert(cookie,"OIDC callback did not create a browser session");
  assert.equal(cookie.secure,false);
  assert.equal(cookie.httpOnly,true);
  assert.equal(cookie.sameSite,"Lax");
  assert.equal(cookie.path,"/");
  assert(!(await page.evaluate(() => document.cookie)).includes("miy_session="));
  pass("real Keycloak OIDC login succeeds over non-loopback HTTP with HttpOnly SameSite cookies");

  const flow = command(["auth","login","--device-code","--no-wait"]);
  assert.equal(flow.status,"pending");
  assert.equal(flow.verificationUrl,origin+"/cli/device/"+flow.loginId);
  const decisionUrl = origin+"/api/v1/cli/devices/"+flow.loginId+"/decision";
  const data = {userCode:flow.userCode,decision:"approve"};
  assert.equal((await context.request.post(decisionUrl,{headers:{origin},data})).status(),403);
  assert.equal((await context.request.post(decisionUrl,{
    headers:{origin,"x-miy-csrf":"incorrect"},data,
  })).status(),403);
  assert.equal((await context.request.post(decisionUrl,{
    headers:{origin:"http://attacker.invalid","x-miy-csrf":me.csrf},data,
  })).status(),403);
  assert.equal((await context.request.post(origin+"/auth/logout",{
    headers:{origin:"http://attacker.invalid","x-miy-csrf":me.csrf},data:{},
  })).status(),403);
  assert.equal((await context.request.get(origin+"/api/v1/me")).status(),200);
  pass("HTTP browser mutations reject missing or incorrect CSRF and mismatched Origin");

  await page.goto(flow.verificationUrl);
  await page.locator('input[name="userCode"]').fill(flow.userCode);
  await page.getByRole("button",{name:"이 계정으로 CLI 연결 승인"}).click();
  await page.getByRole("status").filter({hasText:"CLI 연결을 승인했습니다"}).waitFor();
  const accepted = command(["auth","wait",flow.loginId]);
  assert.equal(accepted.userId,expectedUser);
  const status = command(["auth","status"]);
  assert.equal(status.authenticated,true);
  assert.equal(status.userId,expectedUser);
  assert.equal(status.sessionId,accepted.sessionId);
  const sessions = command(["auth","sessions","list"]);
  assert(JSON.stringify(sessions).includes(accepted.sessionId));
  pass("installed CLI completes browser device approval and authenticated API requests over HTTP");

  command(["auth","logout"]);
  const logout = await context.request.post(origin+"/auth/logout",{
    headers:{origin,"x-miy-csrf":me.csrf},data:{},
  });
  assert.equal(logout.status(),200);
  assert.equal((await context.request.get(origin+"/api/v1/me")).status(),401);
  assert(!(await context.cookies(origin)).some(item=>item.name==="miy_session"));
  pass("CLI and browser logout revoke their HTTP sessions and clear the browser cookie");
  await context.close();
  writeFileSync(join(fixture,"browser-results.json"),JSON.stringify({passed},null,2),{mode:0o600});
} finally { await browser.close(); }
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / ".local/bin/miy-server")
    parser.add_argument("--ip", default=None, help="Assigned non-loopback IPv4 address (default: route source)")
    args = parser.parse_args()
    address = args.ip or local_ip()
    parsed = ipaddress.ip_address(address)
    require(parsed.version == 4 and not parsed.is_loopback and not parsed.is_unspecified,
            "This regression requires an assigned non-loopback IPv4 address")
    require(args.binary.is_file(), "Build a server binary or pass --binary first")
    require((ROOT / "web/platform/dist/index.html").is_file(), "Build the platform frontend first")
    project = "miy-http-" + secrets.token_hex(6)
    fixture = ROOT / ".local" / project
    fixture.mkdir(mode=0o700, parents=True)
    os.umask(0o077)
    env = clean_env()
    selected = set()
    pg_port, identity_port, core_port = [free_port("127.0.0.1", selected) for _ in range(3)]
    http_port, unused_tls_port = [free_port(address, selected) for _ in range(2)]
    origin = f"http://{address}:{http_port}"
    binary = fixture / "miy-server"
    shutil.copy2(args.binary, binary)
    shutil.copytree(ROOT / "web/platform/dist", fixture / "platform")
    binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
    passes = []
    server = None
    compose_started = False
    cleanup_ok = False
    failed = None
    logs = []

    def run(command, label, command_env=None, expected=0, timeout=180):
        result = subprocess.run(command, cwd=ROOT, env=command_env or env, stdin=subprocess.DEVNULL,
                                capture_output=True, timeout=timeout)
        path = fixture / (label + ".log")
        path.write_bytes(result.stdout + result.stderr)
        require(result.returncode == expected, f"{label} failed; inspect {path}")
        return result

    def passed(name):
        passes.append(name)
        print(json.dumps({"status": "PASS", "test": name}), flush=True)

    compose = ["docker", "compose", "--project-name", project, "--project-directory", str(ROOT),
               "--env-file", str(fixture / "compose.env"), "-f", str(ROOT / "compose.yaml"),
               "-f", str(fixture / "compose.override.json")]
    try:
        rejected = subprocess.run([sys.executable, str(ROOT / "scripts/bootstrap.py"), "--profile", "server",
                                   "--origin", origin, "--state-dir", str(fixture / "rejected")],
                                  env=env, capture_output=True, timeout=20)
        require(rejected.returncode != 0 and b"MIY_ALLOW_HTTP" in rejected.stderr,
                "Bootstrap accepted non-loopback HTTP without explicit opt-in")
        require(not (fixture / "rejected").exists(), "Rejected bootstrap created configuration")
        passed("bootstrap rejects non-loopback HTTP before creating configuration without explicit opt-in")
        run([sys.executable, str(ROOT / "scripts/bootstrap.py"), "--profile", "server", "--origin", origin,
             "--allow-http", "--state-dir", str(fixture), "--bind", f"0.0.0.0:{core_port}",
             "--postgres-port", str(pg_port), "--identity-port", str(identity_port),
             "--http-port", str(http_port), "--https-port", str(unused_tls_port), "--proxy-bind", address],
            "bootstrap")
        settings = json.loads((fixture / "server.json").read_text())
        realm = json.loads((fixture / "realm.json").read_text())
        require(settings["origin"] == origin and settings["allowHttp"] is True
                and settings["developmentLogin"] is False and settings["caFile"] is None,
                "HTTP server bootstrap settings mismatch")
        require(realm["sslRequired"] == "none", "HTTP realm still requires TLS")
        web = next(client for client in realm["clients"] if client["clientId"] == "miy-web")
        require(web["redirectUris"] == [origin + "/auth/callback"] and web["webOrigins"] == [origin],
                "OIDC client did not use the exact HTTP/IP callback")
        override = {"name": project, "networks": {"app_runtime": {"name": project + "-runtime"}},
                    "services": {"proxy": {"volumes": [str(fixture / "platform") + ":/srv/platform:ro"]}}}
        (fixture / "compose.override.json").write_text(json.dumps(override))
        # Validate the fully resolved scope before any Docker mutation.
        config = subprocess.run(compose + ["config", "--format", "json"], cwd=ROOT, env=env,
                                capture_output=True, check=True, timeout=20)
        model = json.loads(config.stdout)
        require(model["name"] == project, "Compose project isolation failed")
        require(all(item["name"].startswith(project) for item in model["networks"].values()),
                "Compose would attach to a pre-existing network")
        require(all(item["name"].startswith(project) for item in model["volumes"].values()),
                "Compose would attach to a pre-existing volume")
        for service in model["services"].values():
            for mount in service.get("volumes", []):
                if mount["type"] == "bind" and mount["target"] != "/etc/caddy/Caddyfile":
                    require(Path(mount["source"]).is_relative_to(fixture), "Compose bind escaped the fixture")
        run(compose + ["up", "-d", "postgres", "keycloak", "proxy"], "compose-up", timeout=240)
        compose_started = True
        passed("explicit HTTP bootstrap preserves exact IP origin, callback and isolated Compose resources")
        discovery = wait_json(origin + "/identity/realms/miy/.well-known/openid-configuration")
        require(discovery["issuer"] == origin + "/identity/realms/miy", "Keycloak issuer is not canonical")
        require(discovery["authorization_endpoint"].startswith(origin + "/identity/"), "Wrong identity endpoint")
        run([sys.executable, str(ROOT / "scripts/configure-identity.py")], "configure-identity",
            {**env, "MIY_STATE_DIR": str(fixture)})
        service_env = {**env, **runtime_env(fixture / "runtime.env"), "RUST_LOG": "info"}
        denied = subprocess.run([str(binary), "serve"], cwd=ROOT,
                                env={**service_env, "MIY_ALLOW_HTTP": "0"},
                                capture_output=True, timeout=20)
        (fixture / "server-no-opt-in.log").write_bytes(denied.stdout + denied.stderr)
        require(denied.returncode != 0 and b"MIY_ALLOW_HTTP" in denied.stderr + denied.stdout,
                "Server accepted non-loopback HTTP without explicit opt-in")
        passed("actual server rejects HTTP without MIY_ALLOW_HTTP even with a reachable real database")
        run([str(binary), "migrate"], "migrate", service_env)
        run([str(binary), "seed"], "seed", service_env)
        server_log = (fixture / "server.log").open("wb")
        logs.append(server_log)
        server = subprocess.Popen([str(binary), "serve"], cwd=ROOT, env=service_env,
                                  stdin=subprocess.DEVNULL, stdout=server_log, stderr=subprocess.STDOUT)
        require(wait_json(origin + "/api/health", 30)["status"] == "ok", "Core readiness failed")
        cli_env = {**env, "MIY_CONFIG_DIR": str(fixture / "cli-config"), "MIY_CREDENTIAL_BACKEND": "file"}
        run(["bash", str(ROOT / "scripts/install-cli.sh"), "--from", str(binary), "--prefix",
             str(fixture / "installed"), "--server", origin, "--no-input"], "install-cli", cli_env)
        (fixture / "browser.mjs").write_text(BROWSER)
        result = run(["node", str(fixture / "browser.mjs"), str(fixture)], "browser", cli_env)
        print(result.stdout.decode(), end="", flush=True)
        passes.extend(json.loads((fixture / "browser-results.json").read_text())["passed"])
    except Exception as error:
        failed = str(error)
    finally:
        if server is not None and server.poll() is None:
            server.send_signal(signal.SIGINT)
            try:
                server.wait(timeout=10)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait(timeout=5)
        for log in logs:
            log.close()
        # Even a partially failed `up` may have created resources. The generated
        # project, network and volume names were validated before starting it.
        if compose_started or (fixture / "compose.override.json").exists():
            try:
                try:
                    run(compose + ["logs", "--no-color"], "compose-logs", timeout=20)
                except Exception:
                    # Diagnostic collection must never prevent fixture cleanup.
                    pass
                run(compose + ["down", "--volumes", "--remove-orphans"], "compose-down", timeout=90)
                for resource, command in (("containers", ["ps", "-aq"]),
                                          ("networks", ["network", "ls", "-q"]),
                                          ("volumes", ["volume", "ls", "-q"])):
                    remaining = subprocess.run(["docker", *command, "--filter",
                                                "label=com.docker.compose.project=" + project],
                                               env=env, capture_output=True, check=True, timeout=20)
                    require(not remaining.stdout.strip(), f"Owned Compose {resource} remain after cleanup")
                cleanup_ok = True
            except Exception as error:
                failed = (failed + "; " if failed else "") + str(error)
        else:
            cleanup_ok = True
        summary = {"origin": origin, "serverSha256": binary_hash, "tests": passes,
                   "cleanupSucceeded": cleanup_ok, "error": failed}
        (fixture / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        print(json.dumps({"status": "FAIL" if failed else "PASS", "observations": len(passes),
                          "cleanupSucceeded": cleanup_ok, "evidence": str(fixture)}), flush=True)
    if failed:
        raise SystemExit(failed)


if __name__ == "__main__":
    main()
