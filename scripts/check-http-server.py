#!/usr/bin/env python3
"""Exercise an isolated two-origin IP server with real Keycloak and browser/CLI login.

Requires Docker Compose, an explicit isolated web build, the Playwright dependency,
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
import ssl
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


def private_tls(fixture, address, run):
    """Create fixture-only CA material; never import it into the user's trust store."""
    directory = fixture / 'tls'
    directory.mkdir()
    ca, key, certificate = directory / 'ca.pem', directory / 'server-key.pem', directory / 'server.pem'
    run(['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(directory / 'ca-key.pem'),
         '-out', str(ca), '-days', '2', '-subj', '/CN=MIY isolated acceptance CA',
         '-addext', 'basicConstraints=critical,CA:TRUE', '-addext', 'keyUsage=critical,keyCertSign,cRLSign'], 'tls-ca')
    run(['openssl', 'req', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(key),
         '-out', str(directory / 'server.csr'), '-subj', '/CN=MIY isolated acceptance IP'], 'tls-request')
    extensions = directory / 'extensions.cnf'
    extensions.write_text(f'subjectAltName=IP:{address}\nbasicConstraints=critical,CA:FALSE\n'
                          'keyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n')
    run(['openssl', 'x509', '-req', '-in', str(directory / 'server.csr'), '-CA', str(ca),
         '-CAkey', str(directory / 'ca-key.pem'), '-CAcreateserial', '-out', str(certificate),
         '-days', '1', '-extfile', str(extensions)], 'tls-certificate')
    return ca, certificate, key


# No HTTP response bodies, CLI credentials, OAuth URLs/codes, passwords, browser
# traces, or storage state are written to public test output.
BROWSER = r'''
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash, randomBytes, randomUUID } from "node:crypto";
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { chromium } from "@playwright/test";
const fixture = process.argv[2];
const settings = JSON.parse(readFileSync(join(fixture, "server.json"), "utf8"));
const secrets = JSON.parse(readFileSync(join(fixture, "credentials.json"), "utf8"));
const origin = settings.origin;
const consoleOrigin = settings.consoleOrigin;
const secure = origin.startsWith("https://");
const prefix = secure ? "__Host-" : "";
const expectedUser = "00000000-0000-4000-8000-000000000002";
const cli = join(fixture, "installed", "miy");
const passed = [];
function pass(name) { passed.push(name); console.log(JSON.stringify({status:"PASS",test:name})); }
function command(args) {
  const output = execFileSync(cli, ["--output", "json", "--no-input", ...args], {
    cwd:fixture, env:{...process.env,MIY_CONFIG_DIR:join(fixture,"cli-config"),MIY_CREDENTIAL_BACKEND:"file",RUST_LOG:"off"},
    encoding:"utf8", timeout:60000, stdio:["ignore","pipe","pipe"],
  });
  assert(!/miy_(?:access|refresh|device|cli)_/.test(output), "CLI exposed an authentication secret");
  const envelope = JSON.parse(output);
  assert.equal(envelope.ok, true, "CLI command failed");
  return envelope.data;
}
const browser = process.env.MIY_TEST_CHROME_ENDPOINT
  ? await chromium.connectOverCDP(process.env.MIY_TEST_CHROME_ENDPOINT)
  : await chromium.launch({channel:process.env.MIY_TEST_BROWSER_CHANNEL || "chrome", headless:true,args:["--no-proxy-server"]});
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
  pass("installation guidance preserves the configured portal IP origin and ignores forged forwarding headers");

  for (const [url,surface] of [[origin,"portal"],[consoleOrigin,"console"]]) {
    const options = await (await context.request.get(url+"/auth/options")).json();
    assert.equal(options.developmentLogin,false);
    assert.equal(options.accounts.length,0);
    assert.equal(options.oauthUrl,url+"/auth/login");
    assert.equal(options.surface,surface);
    assert.equal(options.consoleOrigin,consoleOrigin);
    assert.equal((await context.request.post(url+"/auth/development",{headers:{origin:url},data:{account:"developer"}})).status(),404);
  }
  assert.equal((await context.request.get(origin+"/identity/realms/miy/.well-known/openid-configuration")).status(),404);
  assert.equal((await context.request.get(consoleOrigin+"/identity/admin/")).status(),404);
  assert.equal((await context.request.get(origin+"/api/v1/me")).status(),401);
  pass("two fixed server purposes disable synthetic login and expose identity only on the trusted console");

  await page.goto(origin+"/auth/login");
  await page.locator("#username").waitFor();
  assert.equal(new URL(page.url()).origin,consoleOrigin);
  assert(new URL(page.url()).pathname.startsWith("/identity/realms/miy/"));
  await page.locator("#username").fill("developer");
  await page.locator("#password").fill(secrets.developer);
  await page.locator("#kc-login").click();
  await page.waitForURL(url => url.origin === origin && url.pathname === "/");
  const me = await (await context.request.get(origin+"/api/v1/me")).json();
  assert.equal(me.id,expectedUser);
  assert.equal(me.name,"developer");
  const portalCookie = (await context.cookies(origin)).find(item => item.name === prefix+"miy_portal_session");
  assert(portalCookie,"OIDC callback did not create a purpose-bound browser session");
  assert.equal(portalCookie.secure,secure);
  assert.equal(portalCookie.httpOnly,true);
  assert.equal(portalCookie.sameSite,"Lax");
  assert.equal(portalCookie.path,"/");
  assert(!(await page.evaluate(() => document.cookie)).includes("miy_portal_session="));
  assert.equal((await context.request.get(consoleOrigin+"/api/v1/me")).status(),401);
  assert.equal((await context.request.get(origin+"/api/v1/runtime/installations")).status(),403);
  pass("real OIDC keeps the existing realm/user subject while portal cookies cannot authorize console requests");

  await page.goto(consoleOrigin+"/auth/login");
  await page.waitForURL(url => url.origin === consoleOrigin && url.pathname === "/");
  const consoleMe = await (await context.request.get(consoleOrigin+"/api/v1/me")).json();
  assert.equal(consoleMe.id,expectedUser);
  assert.notEqual(consoleMe.csrf,me.csrf);
  const consoleCookie = (await context.cookies(consoleOrigin)).find(item => item.name === prefix+"miy_console_session");
  assert(consoleCookie && consoleCookie.httpOnly && consoleCookie.secure === secure);
  for (const path of ["/artifacts/test/ui/remoteEntry.js","/api/v1/installations/test/api/html","/environments/dev/apps/test"])
    assert.equal((await context.request.get(consoleOrigin+path)).status(),404);
  const consoleHeaders = (await context.request.get(consoleOrigin+"/")).headers();
  assert(consoleHeaders["content-security-policy"].includes("script-src 'self'"));
  assert(consoleHeaders["content-security-policy"].includes("frame-ancestors 'none'"));
  pass("console OIDC uses its own session and callback; app content and embedding are refused");

  const flow = command(["auth","login","--device-code","--no-wait"]);
  assert.equal(flow.status,"pending");
  assert.equal(flow.verificationUrl,origin+"/cli/device/"+flow.loginId);
  const path = "/api/v1/cli/devices/"+flow.loginId+"/decision";
  const data = {userCode:flow.userCode,decision:"approve"};
  assert.equal((await context.request.post(origin+path,{headers:{origin,"x-miy-csrf":me.csrf},data})).status(),403);
  assert.equal((await context.request.post(consoleOrigin+path,{headers:{origin:consoleOrigin},data})).status(),403);
  assert.equal((await context.request.post(consoleOrigin+path,{headers:{origin:consoleOrigin,"x-miy-csrf":"incorrect"},data})).status(),403);
  assert.equal((await context.request.post(consoleOrigin+path,{headers:{origin,"x-miy-csrf":consoleMe.csrf},data})).status(),403);
  assert.equal((await context.request.post(origin+"/auth/logout",{headers:{origin:consoleOrigin,"x-miy-csrf":me.csrf},data:{}})).status(),403);
  pass("portal approval and cross-purpose Origin/CSRF requests are denied even when both cookies are present");

  const redirect = await context.request.get(flow.verificationUrl+"?return=https://attacker.invalid/",{maxRedirects:0});
  assert.equal(redirect.status(),302);
  assert.equal(redirect.headers().location,consoleOrigin+"/cli/device/"+flow.loginId);
  await page.goto(flow.verificationUrl);
  assert.equal(new URL(page.url()).origin,consoleOrigin);
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
  pass("installed CLI preserves relative verification URLs, follows the fixed console redirect and polls successfully");

  let saved, previousAccess, previousRefresh;
  for (const name of readdirSync(join(fixture,"cli-config","credentials"))) {
    if (!name.endsWith(".json")) continue;
    const path = join(fixture,"cli-config","credentials",name);
    const record = JSON.parse(readFileSync(path,"utf8"));
    if (typeof record.value !== "string") continue;
    let credential; try { credential=JSON.parse(record.value); } catch { continue; }
    if (!credential.accessToken) continue;
    previousAccess=credential.accessToken; previousRefresh=credential.refreshToken;
    credential.expiresAt=Math.floor(Date.now()/1000)-1;
    record.value=JSON.stringify(credential);
    writeFileSync(path,JSON.stringify(record),{mode:0o600}); saved=path; break;
  }
  assert(saved,"isolated CLI file credential was not found");
  assert.equal(command(["auth","status"]).sessionId,accepted.sessionId);
  const rotated=JSON.parse(JSON.parse(readFileSync(saved,"utf8")).value);
  assert(rotated.accessToken !== previousAccess && rotated.refreshToken !== previousRefresh);
  pass("actual CLI refresh rotates credentials without changing the portal context or session family");

  const verifier=randomBytes(32).toString("hex");
  const start=await context.request.post(origin+"/auth/cli/start",{data:{challenge:createHash("sha256").update(verifier).digest("hex")}});
  assert.equal(start.status(),410);
  assert.equal((await start.json()).error.code,"cli_upgrade_required");
  const legacyId=randomUUID();
  const approvePath="/api/v1/cli-authorizations/"+legacyId+"/approve";
  assert.equal((await context.request.post(origin+approvePath,{headers:{origin,"x-miy-csrf":me.csrf},data:{}})).status(),403);
  const legacyRedirect=await context.request.get(origin+"/cli/authorize/"+legacyId,{maxRedirects:0});
  assert.equal(legacyRedirect.headers().location,consoleOrigin+"/cli/authorize/"+legacyId);
  assert.equal((await context.request.post(consoleOrigin+approvePath,{headers:{origin:consoleOrigin,"x-miy-csrf":consoleMe.csrf},data:{}})).status(),410);
  assert.equal((await context.request.post(origin+"/auth/cli/exchange",{data:{id:legacyId,verifier}})).status(),410);
  const legacySessions=await (await context.request.get(consoleOrigin+"/api/v1/cli/legacy-sessions")).json();
  assert.equal(legacySessions.sessions.length,0);
  pass("retired legacy start/approve/exchange answer 410 cli_upgrade_required; the published relative approval path still reaches the console notice");

  const logout = await context.request.post(origin+"/auth/logout",{headers:{origin,"x-miy-csrf":me.csrf},data:{}});
  assert.equal(logout.status(),200);
  assert.equal((await context.request.get(origin+"/api/v1/me")).status(),401);
  assert(!(await context.cookies(origin)).some(item=>item.name===prefix+"miy_portal_session"));
  assert.equal(command(["auth","status"]).authenticated,true);
  command(["auth","logout"]);
  const remaining = await (await context.request.get(consoleOrigin+"/api/v1/cli/sessions")).json();
  assert(remaining.sessions.find(item=>item.id===accepted.sessionId)?.revokedAt);
  assert.equal((await context.request.post(consoleOrigin+"/auth/logout",{headers:{origin:consoleOrigin,"x-miy-csrf":consoleMe.csrf},data:{}})).status(),200);
  assert.equal((await context.request.get(consoleOrigin+"/api/v1/me")).status(),401);
  pass("purpose-specific browser logout and explicit CLI revocation preserve their separate lifetimes");
  if (secure) pass("single-IP HTTPS works with private CA trust in Core, CLI and an isolated Chrome trust store");
  await context.close();
  writeFileSync(join(fixture,"browser-results.json"),JSON.stringify({passed},null,2),{mode:0o600});
} finally { await browser.close(); }
'''


def main():
    global OPENER
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / ".local/bin/miy-server")
    parser.add_argument('--cli-binary', type=Path, help='Optional immutable published CLI binary; defaults to the server binary')
    parser.add_argument("--ip", default=None, help="Assigned non-loopback IPv4 address (default: route source)")
    parser.add_argument("--web-dist", type=Path, required=True, help="Explicit isolated frontend build; live dist is refused")
    parser.add_argument("--https", action="store_true", help="Use IP SAN TLS and a private fixture-only browser trust store")
    parser.add_argument("--certutil", type=Path, default=shutil.which('certutil'), help="NSS certutil executable required by --https")
    parser.add_argument("--chrome", type=Path, default=shutil.which('google-chrome'), help="Chrome executable for isolated HTTPS trust")
    args = parser.parse_args()
    address = args.ip or local_ip()
    parsed = ipaddress.ip_address(address)
    require(parsed.version == 4 and not parsed.is_loopback and not parsed.is_unspecified,
            "This regression requires an assigned non-loopback IPv4 address")
    require(args.binary.is_file(), "Build a server binary or pass --binary first")
    cli_source = args.cli_binary or args.binary
    require(cli_source.is_file(), 'The selected --cli-binary is missing')
    web_dist = args.web_dist.resolve()
    require(not web_dist.is_relative_to((ROOT / 'web/platform/dist').resolve()), 'Use a separate frontend build, never live web/platform/dist')
    require((web_dist / 'index.html').is_file(), 'Build the frontend into the explicit --web-dist directory first')
    if args.https:
        require(args.certutil and Path(args.certutil).is_file() and args.chrome and args.chrome.is_file()
                and shutil.which('bwrap') and shutil.which('openssl'),
                '--https requires certutil, Chrome, bubblewrap and openssl; no TLS bypass is supported')
    project = ("miy-https-" if args.https else "miy-http-") + secrets.token_hex(6)
    fixture = ROOT / ".local" / project
    fixture.mkdir(mode=0o700, parents=True)
    os.umask(0o077)
    env = clean_env()
    selected = set()
    pg_port, identity_port, core_port, console_core_port = [free_port("127.0.0.1", selected) for _ in range(4)]
    public_port, console_port = [free_port(address, selected) for _ in range(2)]
    scheme = 'https' if args.https else 'http'
    origin, console_origin = [f'{scheme}://{address}:{number}' for number in [public_port, console_port]]
    binary = fixture / "miy-server"
    cli_binary = fixture / 'miy-client'
    shutil.copy2(args.binary, binary)
    shutil.copy2(cli_source, cli_binary)
    shutil.copytree(web_dist, fixture / "platform")
    binary_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
    cli_hash = hashlib.sha256(cli_binary.read_bytes()).hexdigest()
    passes = []
    server = None
    browser_process = None
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
        ca = certificate = tls_key = None
        if args.https:
            ca, certificate, tls_key = private_tls(fixture, address, run)
            OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}),
                         urllib.request.HTTPSHandler(context=ssl.create_default_context(cafile=str(ca))))
        # The opt-in refusal remains an HTTP check in either acceptance mode.
        refused_origin = f'http://{address}:{public_port}'
        rejected = subprocess.run([sys.executable, str(ROOT / "scripts/bootstrap.py"), "--profile", "server",
                                   "--origin", refused_origin, "--state-dir", str(fixture / "rejected")],
                                  env=env, capture_output=True, timeout=20)
        require(rejected.returncode != 0 and b"MIY_ALLOW_HTTP" in rejected.stderr,
                "Bootstrap accepted non-loopback HTTP without explicit opt-in")
        require(not (fixture / "rejected").exists(), "Rejected bootstrap created configuration")
        passed("bootstrap rejects non-loopback HTTP before creating configuration without explicit opt-in")
        transport_options = ['--ca-file', str(ca)] if args.https else ['--allow-http']
        run([sys.executable, str(ROOT / "scripts/bootstrap.py"), "--profile", "server", "--origin", origin,
             '--console-origin', console_origin, *transport_options,
             "--state-dir", str(fixture), "--bind", f"127.0.0.1:{core_port}",
             '--console-bind', f'127.0.0.1:{console_core_port}',
             "--postgres-port", str(pg_port), "--identity-port", str(identity_port),
             "--proxy-bind", address],
            "bootstrap")
        settings = json.loads((fixture / "server.json").read_text())
        realm = json.loads((fixture / "realm.json").read_text())
        require(settings["origin"] == origin and settings['consoleOrigin'] == console_origin
                and settings["allowHttp"] == (not args.https) and settings["developmentLogin"] is False,
                "Two-origin server bootstrap settings mismatch")
        require(realm["sslRequired"] == ('external' if args.https else 'none'), "Realm transport policy is incorrect")
        web = next(client for client in realm["clients"] if client["clientId"] == "miy-web")
        require(web["redirectUris"] == [value + '/auth/callback' for value in [origin, console_origin]]
                and web["webOrigins"] == [origin, console_origin]
                and web['attributes']['post.logout.redirect.uris'] == '##'.join(value+'/' for value in [origin, console_origin]),
                "OIDC client did not use both exact callbacks and logout destinations")
        override = {"name": project, "networks": {"app_runtime": {"name": project + "-runtime"}},
                    "services": {"proxy": {"volumes": [str(fixture / "platform") + ":/srv/platform:ro"]}}}
        if args.https:
            caddyfile = fixture / 'Caddyfile'
            caddyfile.write_text('''{
  auto_https disable_redirects
  admin off
}
import /etc/caddy/routes.caddy
{$MIY_ORIGIN} {
  bind {$MIY_PROXY_BIND}
  tls /miy-tls/server.pem /miy-tls/server-key.pem
  import portal
}
{$MIY_CONSOLE_ORIGIN} {
  bind {$MIY_PROXY_BIND}
  tls /miy-tls/server.pem /miy-tls/server-key.pem
  import console
}
''')
            override['services']['proxy']['volumes'].extend([
                str(caddyfile) + ':/etc/caddy/Caddyfile:ro', str(fixture / 'tls') + ':/miy-tls:ro'])
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
                if mount["type"] == "bind" and mount["target"] not in ["/etc/caddy/Caddyfile", '/etc/caddy/routes.caddy']:
                    require(Path(mount["source"]).is_relative_to(fixture), "Compose bind escaped the fixture")
        require(model['services']['proxy']['network_mode'] == 'host', 'Proxy cannot reach loopback-only Core/identity')
        for name in ['postgres', 'keycloak']:
            require(all(value.get('host_ip') == '127.0.0.1' for value in model['services'][name]['ports']),
                    'An internal database/identity port is publicly bound')
        run(compose + ["up", "-d", "postgres", "keycloak", "proxy"], "compose-up", timeout=240)
        compose_started = True
        passed("bootstrap preserves exact IP portal/console origins, callbacks and isolated Compose resources")
        discovery = wait_json(console_origin + "/identity/realms/miy/.well-known/openid-configuration")
        require(discovery["issuer"] == console_origin + "/identity/realms/miy", "Keycloak issuer is not canonical")
        require(discovery["authorization_endpoint"].startswith(console_origin + "/identity/"), "Wrong identity endpoint")
        run([sys.executable, str(ROOT / "scripts/configure-identity.py")], "configure-identity",
            {**env, "MIY_STATE_DIR": str(fixture)})
        service_env = {**env, **runtime_env(fixture / "runtime.env"), "RUST_LOG": "info"}
        if not args.https:
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
        require(wait_json(console_origin + '/api/health', 30)['status'] == 'ok', 'Console readiness failed')
        cli_env = {**env, "MIY_CONFIG_DIR": str(fixture / "cli-config"), "MIY_CREDENTIAL_BACKEND": "file"}
        if args.https:
            run([str(cli_binary), '--output', 'json', '--no-input', 'context', 'init', '--url', origin,
                 '--ca-file', str(ca)], 'cli-private-ca', cli_env)
            run([sys.executable, str(ROOT / 'scripts/check-server-tls.py'), '--state-dir', str(fixture),
                 '--certificate', str(certificate), '--key', str(tls_key), '--connect'], 'tls-live-check')
        run(["bash", str(ROOT / "scripts/install-cli.sh"), "--from", str(cli_binary), "--prefix",
             str(fixture / "installed"), "--server", origin, "--no-input"], "install-cli", cli_env)
        if args.https:
            browser_home = fixture / 'browser-home'
            nss = browser_home / '.pki/nssdb'
            nss.mkdir(parents=True)
            run([str(args.certutil), '-N', '--empty-password', '-d', 'sql:' + str(nss)], 'browser-nss-create')
            run([str(args.certutil), '-A', '-d', 'sql:' + str(nss), '-n', 'miy-fixture-ca',
                 '-t', 'C,,', '-i', str(ca)], 'browser-nss-ca')
            browser_port = free_port('127.0.0.1', selected)
            browser_log = (fixture / 'browser-process.log').open('wb')
            logs.append(browser_log)
            # The actual HOME environment variable and user trust store remain
            # untouched. Only this Chrome process sees a private mount at the
            # home path, containing its fixture-only NSS database/profile.
            browser_process = subprocess.Popen([
                'bwrap', '--die-with-parent', '--ro-bind', '/', '/', '--proc', '/proc',
                '--dev-bind', '/dev', '/dev', '--bind', '/tmp', '/tmp',
                '--bind', str(browser_home), str(Path.home()), '--', str(args.chrome),
                '--headless', '--no-sandbox', '--disable-dev-shm-usage', '--no-proxy-server',
                f'--remote-debugging-port={browser_port}', '--remote-debugging-address=127.0.0.1',
                '--user-data-dir=' + str(Path.home() / 'miy-fixture-browser-profile'),
            ], env=env, stdin=subprocess.DEVNULL, stdout=browser_log, stderr=subprocess.STDOUT,
                start_new_session=True)
            endpoint = f'http://127.0.0.1:{browser_port}'
            require(wait_json(endpoint + '/json/version', 30).get('webSocketDebuggerUrl'),
                    'Isolated trusted Chrome did not start')
            cli_env.update({'MIY_TEST_CHROME_ENDPOINT': endpoint, 'NODE_EXTRA_CA_CERTS': str(ca)})
        (fixture / "browser.mjs").write_text(BROWSER)
        result = run(["node", str(fixture / "browser.mjs"), str(fixture)], "browser", cli_env)
        print(result.stdout.decode(), end="", flush=True)
        passes.extend(json.loads((fixture / "browser-results.json").read_text())["passed"])
    except Exception as error:
        failed = str(error)
    finally:
        if browser_process is not None and browser_process.poll() is None:
            os.killpg(browser_process.pid, signal.SIGTERM)
            try:
                browser_process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(browser_process.pid, signal.SIGKILL)
                browser_process.wait(timeout=5)
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
        summary = {"origin": origin, 'consoleOrigin': console_origin, 'https': args.https,
                   "serverSha256": binary_hash, 'cliSha256': cli_hash, 'cliSource': str(cli_source.resolve()),
                   'webSource': str(web_dist), "tests": passes,
                   "cleanupSucceeded": cleanup_ok, "error": failed}
        (fixture / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        print(json.dumps({"status": "FAIL" if failed else "PASS", "observations": len(passes),
                          "cleanupSucceeded": cleanup_ok, "evidence": str(fixture)}), flush=True)
    if failed:
        raise SystemExit(failed)


if __name__ == "__main__":
    main()
