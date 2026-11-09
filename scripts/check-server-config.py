#!/usr/bin/env python3
"""Check generated HTTP/HTTPS configuration using isolated temporary state only."""
import http.server
import json
import os
from pathlib import Path
import shlex
import shutil
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
ENV = {key: value for key, value in os.environ.items() if not key.startswith('MIY_')}
CADDY = 'caddy:2.11.2@sha256:25cdc846626b62d05f6b633b9b40c2c9f6ef89b515dc76133cefd920f7dbe562'
PUBLIC_PROXY_ENV = {'MIY_ORIGIN', 'MIY_CONSOLE_ORIGIN', 'MIY_DEV_PORTAL_ORIGIN', 'MIY_CORE_UPSTREAM',
                    'MIY_CONSOLE_CORE_UPSTREAM', 'MIY_DEV_PORTAL_CORE_UPSTREAM', 'MIY_IDENTITY_UPSTREAM',
                    'MIY_PORTAL_HOST', 'MIY_CONSOLE_HOST', 'MIY_DEV_PORTAL_HOST', 'MIY_PORTAL_PORT',
                    'MIY_CONSOLE_PORT', 'MIY_DEV_PORTAL_PORT', 'MIY_PUBLIC_SCHEME', 'MIY_PROXY_BIND',
                    'MIY_HTTP_PORT', 'MIY_APP_FRAME_SITES', 'MIY_APP_FRAME_SOURCES',
                    'MIY_APP_FRAME_CORE_UPSTREAM'}


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def bootstrap(state, *args, expected=0, environment=None):
    result = subprocess.run(
        [sys.executable, str(ROOT / 'scripts/bootstrap.py'), '--state-dir', str(state), *args],
        env={**ENV, **(environment or {})}, capture_output=True, text=True, timeout=15,
    )
    require(result.returncode == expected, 'Unexpected bootstrap result')
    if expected == 0:
        require('PASSWORD' not in result.stdout and 'postgres://' not in result.stdout,
                'Configuration output exposed private data')


def load_env(state, filename):
    return {key: ' '.join(shlex.split(value)) for key, value in
            (line.split('=', 1) for line in (state / filename).read_text().splitlines())}


def tls_fixture(directory):
    directory.mkdir()
    def openssl(*args):
        subprocess.run(['openssl', *map(str, args)], check=True, capture_output=True, timeout=30)
    ca, ca_key = directory / 'ca.pem', directory / 'ca-key.pem'
    certificate, key, request = directory / 'server.pem', directory / 'server-key.pem', directory / 'server.csr'
    openssl('req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', ca_key, '-out', ca,
            '-days', '2', '-subj', '/CN=MIY isolated configuration CA',
            '-addext', 'basicConstraints=critical,CA:TRUE',
            '-addext', 'keyUsage=critical,keyCertSign,cRLSign')
    openssl('req', '-newkey', 'rsa:2048', '-nodes', '-keyout', key, '-out', request,
            '-subj', '/CN=MIY isolated configuration server')
    extensions = directory / 'extensions.cnf'
    extensions.write_text('subjectAltName=DNS:platform.example,DNS:localhost,IP:127.0.0.1,IP:10.2.3.4\n'
                          'basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\n'
                          'extendedKeyUsage=serverAuth\n')
    openssl('x509', '-req', '-in', request, '-CA', ca, '-CAkey', ca_key, '-CAcreateserial',
            '-out', certificate, '-days', '1', '-extfile', extensions)
    (directory / 'index').touch()
    (directory / 'serial').write_text('1000\n')
    config = directory / 'ca.cnf'
    config.write_text(f'[ca]\ndefault_ca=local\n[local]\ndatabase={directory}/index\n'
                      f'new_certs_dir={directory}\nserial={directory}/serial\n'
                      'default_md=sha256\npolicy=subject\n[subject]\ncommonName=supplied\n')
    openssl('ca', '-batch', '-config', config, '-cert', ca, '-keyfile', ca_key, '-in', request,
            '-out', directory / 'expired.pem', '-startdate', '20200101000000Z', '-enddate', '20200102000000Z')
    return ca, certificate, key


def caddy_command(state, *command):
    values = load_env(state, 'compose.env')
    result = ['docker', 'run', '--rm', '--pull=never', '--read-only', '--network', 'none']
    for key in sorted(PUBLIC_PROXY_ENV):
        result += ['-e', key + '=' + values[key]]
    return result + ['-v', values['MIY_CADDY_CONFIG'] + ':/etc/caddy/Caddyfile:ro',
                     '-v', str(ROOT / 'ops/Caddyfile.routes') + ':/etc/caddy/routes.caddy:ro',
                     CADDY, 'caddy', *command]


def fixture_import_checks(root):
    dist = root / 'fixture-dist'
    (dist / 'reference').mkdir(parents=True)
    (dist / 'reference/mf-manifest.json').write_text('{}')
    state = root / 'fixture-import-state'
    bootstrap(state, '--origin', 'http://localhost:18000')
    isolated = {'MIY_STATE_DIR': str(state), 'MIY_FIXTURE_DIST': str(dist),
                'MIY_DATABASE_CONTAINER': 'miy-unit-fixture-postgres'}
    def run(environment):
        return subprocess.run([sys.executable, str(ROOT / 'scripts/import-fixture.py')],
                              env={**ENV, **environment}, capture_output=True, timeout=15)
    for environment in [{}, {**isolated, 'MIY_STATE_DIR': str(ROOT / '.local')},
                        {**isolated, 'MIY_DATABASE_CONTAINER': 'miy-v2-validation-postgres-1'}]:
        require(run(environment).returncode != 0, 'Fixture importer accepted missing/production scope')
    result = run(isolated)
    require(result.returncode == 0, 'Explicit isolated fixture import failed')
    artifact = json.loads(result.stdout)['artifactHash']
    require((state / 'artifacts' / artifact / 'manifest.json').is_file(), 'Fixture import ignored MIY_STATE_DIR')
    escaped = root / 'fixture-import-symlink'
    bootstrap(escaped, '--origin', 'http://localhost:18000')
    (escaped / 'artifacts').rmdir()
    outside = root / 'outside-artifacts'
    outside.mkdir()
    (escaped / 'artifacts').symlink_to(outside, target_is_directory=True)
    require(run({**isolated, 'MIY_STATE_DIR': str(escaped)}).returncode != 0 and not list(outside.iterdir()),
            'Fixture importer followed an artifact symlink outside isolated state')


def check_caddy_config(state, expected_ports):
    result = subprocess.run(caddy_command(state, 'adapt', '--config', '/etc/caddy/Caddyfile'),
                            check=True, capture_output=True, timeout=30)
    model = json.loads(result.stdout)
    listeners = {address for value in model['apps']['http']['servers'].values() for address in value['listen']}
    require(listeners == {f'127.0.0.1:{number}' for number in expected_ports},
            'Caddy opened an unconfigured listener')
    require(model.get('admin', {}).get('disabled') is True, 'Host-network Caddy exposed its administration API')
    for value in model['apps']['http']['servers'].values():
        require(value.get('automatic_https', {}).get('disable_redirects') is True
                or value.get('automatic_https', {}).get('disable') is True,
                'Caddy could automatically open another redirect listener')


def proxy_regression(state, kind, tls_material=None):
    """Exercise real proxies against disposable loopback HTTP stubs, without DB/apps."""
    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def do_GET(self):
            payload = json.dumps({'surface': self.server.surface, 'path': self.path,
                                  'headers': dict(self.headers.items())}).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('X-Content-Type-Options', 'nosniff')
            if self.server.surface == 'identity':
                self.send_header('X-Frame-Options', 'SAMEORIGIN')
                self.send_header('Content-Security-Policy', "script-src 'self'")
            elif self.server.surface != 'frame':
                # Core's browser listeners deny framing; the frame listener answers
                # its own frame-ancestors per document instead.
                self.send_header('X-Frame-Options', 'DENY')
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

    stubs, threads = [], []
    process = None
    log = None
    reserved = []
    cidfile = state / 'caddy.cid'
    try:
        for surface in ['portal', 'console', 'identity', 'dev_portal', 'frame']:
            stub = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
            stub.surface = surface
            thread = threading.Thread(target=stub.serve_forever, daemon=True)
            thread.start()
            stubs.append(stub)
            threads.append(thread)
        for _ in range(3):
            sock = socket.socket()
            sock.bind(('127.0.0.1', 0))
            reserved.append(sock)
        ports = [sock.getsockname()[1] for sock in reserved]
        # Two consecutive free ports form the app frame pool (one origin each),
        # below the kernel's ephemeral range where outgoing connections bind.
        for base in range(29000, 30000, 2):
            pair = []
            try:
                for number in (base, base + 1):
                    sock = socket.socket()
                    pair.append(sock)
                    sock.bind(('127.0.0.1', number))
            except OSError:
                for sock in pair:
                    sock.close()
                continue
            reserved.extend(pair)
            break
        frame_ports = [base, base + 1]
        scheme = 'https' if tls_material else 'http'
        portal, console, dev_portal = [f'{scheme}://127.0.0.1:{number}' for number in ports]
        frame_origin = f'{scheme}://127.0.0.1:{frame_ports[0]}'
        tls_options = ['--ca-file', str(tls_material[0])] if tls_material else []
        bootstrap(state, '--profile', 'server' if tls_material else 'local', '--origin', portal, '--console-origin', console,
                  '--dev-portal-origin', dev_portal,
                  '--bind', f'127.0.0.1:{stubs[0].server_port}',
                  '--console-bind', f'127.0.0.1:{stubs[1].server_port}',
                  '--dev-portal-bind', f'127.0.0.1:{stubs[3].server_port}',
                  '--app-frame-bind', f'127.0.0.1:{stubs[4].server_port}',
                  '--app-frame-origin', scheme + '://127.0.0.1:{port}',
                  '--app-frame-ports', f'{frame_ports[0]}-{frame_ports[1]}',
                  '--identity-port', str(stubs[2].server_port), *tls_options)
        site = state / 'site'
        site.mkdir()
        (site / 'index.html').write_text('<!doctype html><title>Trusted fixture shell</title>')
        if kind == 'nginx':
            certificate_options = ['--certificate', str(tls_material[1]), '--key', str(tls_material[2])] if tls_material else []
            subprocess.run([sys.executable, str(ROOT / 'scripts/render-server-config.py'), '--state-dir', str(state),
                            *certificate_options],
                           env=ENV, check=True, capture_output=True, timeout=15)
            config = state / 'nginx-miy.conf'
            config.write_text(config.read_text().replace(str(ROOT / 'web/platform/dist'), str(site)))
            wrapper = state / 'nginx-validation.conf'
            wrapper.write_text(f'pid "{state / "nginx.pid"}"; error_log stderr; events {{}} '
                               f'http {{ access_log off; include "{config}"; }}')
            command = ['nginx', '-c', str(wrapper), '-p', str(state), '-g', 'daemon off; master_process off;']
        else:
            check_caddy_config(state, ports + frame_ports)
            command = caddy_command(state, 'run', '--config', '/etc/caddy/Caddyfile')
            command[command.index('none')] = 'host'
            command[2:2] = ['--cidfile', str(cidfile), '--tmpfs', '/data', '--tmpfs', '/config',
                            '-v', str(site) + ':/srv/platform:ro']
        for sock in reserved:
            sock.close()
        log = (state / 'proxy.log').open('wb')
        process = subprocess.Popen(command, env=ENV, stdout=log, stderr=log)

        class NoRedirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, *_args, **_kwargs):
                return None

        handlers = [NoRedirect(), urllib.request.ProxyHandler({})]
        if tls_material:
            handlers.append(urllib.request.HTTPSHandler(context=ssl.create_default_context(cafile=str(tls_material[0]))))
        opener = urllib.request.build_opener(*handlers)
        def fetch(origin, path, headers=None):
            try:
                response = opener.open(urllib.request.Request(origin + path, headers=headers or {}), timeout=2)
            except urllib.error.HTTPError as error:
                response = error
            with response:
                return response.status, response.headers, response.read()
        last_error = 'no response'
        for _ in range(50):
            if process.poll() is not None:
                raise AssertionError(f'{kind} exited before the isolated proxy checks')
            try:
                status = fetch(portal, '/api/health')[0]
                if status == 200:
                    break
                last_error = f'HTTP {status}'
            except (OSError, urllib.error.URLError) as error:
                last_error = str(error)
            time.sleep(.1)
        else:
            raise AssertionError(f'{kind} did not start on its temporary ports: {last_error}')

        for origin, surface in [(portal, 'portal'), (console, 'console'), (dev_portal, 'dev_portal')]:
            status, headers, body = fetch(origin, '/api/health', {
                'X-Forwarded-Host': 'attacker.invalid', 'X-Forwarded-Proto': 'https',
                'X-Forwarded-Port': '443', 'Forwarded': 'host=attacker.invalid;proto=https',
                'Origin': 'https://attacker.invalid', 'X-Miy-Surface': 'console',
            })
            value = json.loads(body)
            forwarded = {key.lower(): item for key, item in value['headers'].items()}
            require(status == 200 and value['surface'] == surface, 'Client header selected the upstream purpose')
            require(forwarded['host'] == origin.split('://', 1)[1]
                    and forwarded['x-forwarded-host'] == origin.split('://', 1)[1]
                    and forwarded['x-forwarded-proto'] == scheme
                    and forwarded['x-forwarded-port'] == origin.rsplit(':', 1)[1]
                    and 'forwarded' not in forwarded, 'Untrusted forwarding headers reached the backend')
            require(forwarded['origin'] == 'https://attacker.invalid', 'Proxy rewrote Origin and could bypass server CSRF')
            require(headers.get_all('X-Frame-Options') == ['DENY']
                    and headers.get_all('X-Content-Type-Options') == ['nosniff'], 'Proxy security headers missing or duplicated')
        for flow in ['authorize', 'device']:
            path = f'/cli/{flow}/00000000-0000-4000-8000-000000000001'
            status, headers, _ = fetch(portal, path + '?return=https://attacker.invalid/&extra=ignored')
            require(status == 302 and headers['Location'] == console + path,
                    'CLI verification redirect copied an untrusted destination or query')
        require(fetch(portal, '/identity/realms/miy/.well-known/openid-configuration')[0] == 404,
                'Portal can still proxy identity')
        # The dev portal runs dev app code: no identity pages, CLI approval, MCP or
        # OAuth metadata. App API, assets and browser sign-in still reach its Core.
        for path in ['/identity/realms/miy/.well-known/openid-configuration', '/identity/',
                     '/cli/authorize/00000000-0000-4000-8000-000000000001',
                     '/cli/device/00000000-0000-4000-8000-000000000001',
                     '/auth/cli/device', '/mcp', '/.well-known/oauth-protected-resource/mcp']:
            require(fetch(dev_portal, path)[0] == 404, 'Dev portal exposed a non-app surface: ' + path)
        for path in ['/api/v1/installations/id/api/test', '/artifacts/hash/ui/remoteEntry.js', '/auth/options']:
            status, _, body = fetch(dev_portal, path)
            require(status == 200 and json.loads(body)['surface'] == 'dev_portal',
                    'Dev portal did not route app content to its own fixed Core listener')
        require(fetch(dev_portal, '/assets/worker.js', {'Service-Worker': 'script'})[0] == 403,
                'Dev portal accepted a service-worker script request')
        dev_headers = fetch(dev_portal, '/')[1]
        require("worker-src 'none'" in ';'.join(dev_headers.get_all('Content-Security-Policy', []))
                and "frame-ancestors 'none'" in ';'.join(dev_headers.get_all('Content-Security-Policy', [])),
                'Dev portal shell permits new service workers or framing')
        for path in ['/identity/admin/', '/identity/realms/master/', '/identity/']:
            require(fetch(console, path)[0] == 404, 'Console exposed an internal identity endpoint')
        status, headers, body = fetch(console, '/identity/realms/miy/.well-known/openid-configuration')
        require(status == 200 and json.loads(body)['surface'] == 'identity'
                and headers.get('X-Frame-Options') == 'DENY'
                and "frame-ancestors 'none'" in ';'.join(headers.get_all('Content-Security-Policy')),
                'Public identity discovery or login anti-frame protection failed')
        for path in ['/artifacts/hash/ui/remoteEntry.js', '/preview/app', '/apps/app',
                     '/environments/dev/apps/app', '/api/v1/preview/apps/app/platform/directory',
                     '/api/v1/installations/id/api/test', '/api/v1/installations/id/connections/target/api/test']:
            require(fetch(console, path)[0] == 404, 'Console returned app content or SPA fallback')
        status, headers, _ = fetch(console, '/')
        csp = ';'.join(headers.get_all('Content-Security-Policy', []))
        require(status == 200 and "script-src 'self'" in csp and "worker-src 'none'" in csp
                and "object-src 'none'" in csp, 'Console static shell lacks the strict script/worker CSP')
        require(fetch(console, '/assets/worker.js', {'Service-Worker': 'script'})[0] == 403,
                'Console accepted a service-worker script request')
        require(fetch(portal, '/assets/worker.js', {'Service-Worker': 'script'})[0] == 403,
                'Portal accepted a service-worker script request')
        portal_headers = fetch(portal, '/')[1]
        require("worker-src 'none'" in ';'.join(portal_headers.get_all('Content-Security-Policy', [])),
                'Portal shell permits new service workers')
        # Business app frames: portals frame only frame origins and keep the launch
        # POST Origin; frame origins reach the frame listener with their own Host
        # and never get the anti-frame headers of the portal surfaces.
        for shell in [portal, dev_portal]:
            shell_headers = fetch(shell, '/')[1]
            require(f'frame-src {scheme}://127.0.0.1:*' in ';'.join(shell_headers.get_all('Content-Security-Policy', []))
                    and shell_headers.get_all('Referrer-Policy') == ['strict-origin-when-cross-origin'],
                    'Portal shell does not frame app origins or may drop the launch Origin')
        for number in frame_ports:
            frame = f'{scheme}://127.0.0.1:{number}'
            status, headers, body = fetch(frame, '/_miy/launch', {'X-Forwarded-Host': 'attacker.invalid'})
            value = json.loads(body)
            forwarded = {key.lower(): item for key, item in value['headers'].items()}
            require(status == 200 and value['surface'] == 'frame' and forwarded['host'] == frame.split('://', 1)[1]
                    and 'x-forwarded-host' not in forwarded and 'forwarded' not in forwarded,
                    'Frame origin did not reach the frame listener with its own Host')
            require(not headers.get_all('X-Frame-Options')
                    and headers.get_all('X-Content-Type-Options') == ['nosniff'],
                    'Proxy added anti-frame headers to app frames or lost nosniff')
            require(fetch(frame, '/_miy/runtime/a.js', {'Service-Worker': 'script'})[0] == 403,
                    'Frame origin accepted a service-worker script request')
        if tls_material:
            subprocess.run([sys.executable, str(ROOT / 'scripts/check-server-tls.py'), '--state-dir', str(state),
                            '--certificate', str(tls_material[1]), '--key', str(tls_material[2]), '--connect'],
                           env=ENV, check=True, capture_output=True, timeout=30)
    finally:
        for sock in reserved:
            sock.close()
        if cidfile.exists():
            identifier = cidfile.read_text().strip()
            if identifier and all(value in '0123456789abcdef' for value in identifier):
                subprocess.run(['docker', 'stop', '--time', '2', identifier], capture_output=True, timeout=15)
        if process and process.poll() is None:
            process.terminate()
        if process:
            process.wait(timeout=15)
        if log:
            log.close()
        for stub, thread in zip(stubs, threads):
            stub.shutdown()
            stub.server_close()
            thread.join()


def identity_sync(state):
    realm = json.loads((state / 'realm.json').read_text())
    requests = []

    class Handler(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_args):
            pass

        def respond(self, method):
            body = self.rfile.read(int(self.headers.get('Content-Length', '0')))
            requests.append((method, self.path, body))
            data = None
            if self.path.endswith('/realms/master/protocol/openid-connect/token'):
                data = {'access_token': 'fixture-admin-token'}
            elif method == 'GET':
                if self.path.endswith('/client-scopes'):
                    data = [{'id': name, 'name': name} for name in ['miy.read', 'miy.dev', 'offline_access']]
                elif self.path.endswith('/roles'):
                    data = [{'id': 'offline', 'name': 'offline_access'}]
                elif self.path.endswith('/roles/offline_access'):
                    data = {'id': 'offline', 'name': 'offline_access'}
                elif self.path.endswith('/clients'):
                    data = [{**client, 'id': str(index)} for index, client in enumerate(realm['clients'])]
                else:
                    data = []
            payload = json.dumps(data).encode() if data is not None else b''
            self.send_response(200 if payload else 204)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def do_GET(self): self.respond('GET')
        def do_POST(self): self.respond('POST')
        def do_PUT(self): self.respond('PUT')
        def do_DELETE(self): self.respond('DELETE')

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    settings = json.loads((state / 'server.json').read_text())
    settings['identityAdminUrl'] = f'http://127.0.0.1:{server.server_port}/identity'
    (state / 'server.json').write_text(json.dumps(settings))
    try:
        result = subprocess.run([sys.executable, str(ROOT / 'scripts/configure-identity.py')],
                                env={**ENV, 'MIY_STATE_DIR': str(state)},
                                capture_output=True, timeout=15)
        require(result.returncode == 0, 'Identity configuration did not complete')
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
    updates = [json.loads(body) for method, path, body in requests
               if method == 'PUT' and path == '/identity/admin/realms/miy']
    require(updates == [{'sslRequired': 'none'}], 'Existing realm HTTP policy was not updated')
    require(not any('/admin/realms/master' in path for _, path, _ in requests),
            'Configuration changed the master realm')
    clients = [json.loads(body) for method, path, body in requests if method == 'PUT' and '/clients/' in path
               and not ('/protocol-mappers/' in path or '-client-scopes/' in path)]
    web = next(client for client in clients if client.get('clientId') == 'miy-web')
    require(web['redirectUris'] == [settings[key] + '/auth/callback'
                                    for key in ['origin', 'consoleOrigin', 'devPortalOrigin']],
            'Existing miy-web client lost an exact callback')
    require(web['webOrigins'] == [settings['origin'], settings['consoleOrigin']],
            'The dev portal gained identity provider CORS access')
    require(not any(method in ['PUT', 'DELETE'] and '/users/' in path for method, path, _ in requests),
            'Identity synchronization replaced existing user subjects')


with tempfile.TemporaryDirectory(prefix='miy-server-config-') as temporary:
    root = Path(temporary)
    fixture_import_checks(root)
    print('PASS fixture import requires isolated state/build/database and refuses artifact symlink escapes')
    missing = root / 'missing'
    bootstrap(missing, '--profile', 'server', expected=2)
    require(not missing.exists(), 'Missing server origin created configuration')
    for index, origin in enumerate(['http://10.2.3.4:8080', 'http://intranet.example', 'http://[fd00::4]:8080']):
        state = root / f'http-refused-{index}'
        bootstrap(state, '--profile', 'server', '--origin', origin, expected=2)
        require(not state.exists(), 'HTTP opt-out created configuration')
    print('PASS explicit server origin and HTTP opt-in')

    for index, options in enumerate([
        ['--console-origin', 'https://platform.example/'],
        ['--console-origin', 'http://platform.example:9443', '--allow-http'],
        ['--bind', '127.0.0.1:18081', '--console-bind', '127.0.0.1:18081'],
        ['--bind', '0.0.0.0:18080'],
        ['--console-bind', '10.2.3.4:18081'],
        ['--dev-portal-origin', 'https://platform.example'],
        ['--dev-portal-origin', 'https://platform.example:9443'],
        ['--dev-portal-origin', 'http://platform.example:9444', '--allow-http'],
        ['--dev-portal-bind', '127.0.0.1:18081'],
        ['--dev-portal-bind', '10.2.3.4:18082'],
    ]):
        rejected = root / f'boundary-refused-{index}'
        bootstrap(rejected, '--profile', 'server', '--origin', 'https://platform.example', *options, expected=2)
        require(not rejected.exists(), 'Invalid boundary configuration wrote private state')
    print('PASS distinct origins/binds, matching transport and loopback-only Core listeners')

    state = root / 'http state with spaces'
    bootstrap(state, '--profile', 'server', '--origin', 'http://10.2.3.4:8080/', '--allow-http',
              '--bind', '127.0.0.1:18180', '--postgres-port', '15439', '--identity-port', '18189')
    runtime = load_env(state, 'runtime.env')
    compose = load_env(state, 'compose.env')
    realm = json.loads((state / 'realm.json').read_text())
    require(runtime['MIY_ORIGIN'] == 'http://10.2.3.4:8080' and runtime['MIY_ALLOW_HTTP'] == '1', 'HTTP configuration mismatch')
    require(runtime['MIY_BIND_ADDR'] == '127.0.0.1:18180' and runtime['MIY_DEV_LOGIN'] == '0', 'Unsafe development profile')
    require(json.loads((state / 'server.json').read_text())['sharedDevelopmentLogin'] is False,
            'Shared development login was enabled without opt-in')
    require('MIY_CA_FILE' not in runtime and realm['sslRequired'] == 'none', 'HTTP requires a certificate')
    require(compose['MIY_CADDY_CONFIG'].endswith('Caddyfile.http'), 'Wrong HTTP proxy configuration')
    require(compose['MIY_STATE_DIR'] == str(state), 'Space-containing state path was corrupted')
    require(runtime['MIY_CLI_INSTALLER_DIR'] == str(state / 'cli-installers'), 'Installer directory was not configured')
    require(runtime['MIY_CONSOLE_ORIGIN'] == 'http://10.2.3.4:8081'
            and runtime['MIY_CONSOLE_BIND_ADDR'] == '127.0.0.1:18081'
            and runtime['MIY_ISSUER'] == runtime['MIY_CONSOLE_ORIGIN'] + '/identity/realms/miy',
            'Console/identity did not use the configured distinct listener')
    require(runtime['MIY_DEV_PORTAL_ORIGIN'] == 'http://10.2.3.4:8082'
            and runtime['MIY_DEV_PORTAL_BIND_ADDR'] == '127.0.0.1:18082',
            'Dev portal did not use its own single-IP port and listener')
    require(runtime['MIY_APP_FRAME_ORIGIN'] == 'http://10.2.3.4:{port}'
            and runtime['MIY_APP_FRAME_PORTS'] == '20000-20199'
            and runtime['MIY_APP_FRAME_BIND_ADDR'] == '127.0.0.1:18083',
            'App frames did not get a port pool on the single IP and their own listener')
    require(realm['clients'][0]['redirectUris'] == [value + '/auth/callback' for value in
            ['http://10.2.3.4:8080', 'http://10.2.3.4:8081', 'http://10.2.3.4:8082']], 'Incorrect browser callbacks')
    require(realm['clients'][0]['webOrigins'] == ['http://10.2.3.4:8080', 'http://10.2.3.4:8081'],
            'The dev portal gained identity provider CORS access')
    require(realm['clients'][0]['attributes']['post.logout.redirect.uris'] ==
            'http://10.2.3.4:8080/##http://10.2.3.4:8081/##http://10.2.3.4:8082/', 'Logout destinations are not exact')
    require(realm['clients'][2]['clientId'] == runtime['MIY_ORIGIN'] + '/mcp', 'MCP resource moved away from the portal')
    original_secrets = (state / 'credentials.json').read_bytes()
    bootstrap(state)
    require((state / 'credentials.json').read_bytes() == original_secrets, 'Credentials were regenerated')
    require(load_env(state, 'runtime.env') == runtime, 'Stored configuration was not preserved')
    require(state.stat().st_mode & 0o077 == 0 and all((state / file).stat().st_mode & 0o077 == 0 for file in ['credentials.json','runtime.env','compose.env']), 'Private file permissions changed')
    require(all((state / file).stat().st_mode & 0o777 == 0o644 for file in ['realm.json','init.sql']), 'Container seed files cannot be read under container UIDs')
    subprocess.run(['docker', 'compose', '--env-file', str(state / 'compose.env'), 'config', '--quiet'],
                   cwd=ROOT, env=ENV, check=True, capture_output=True, timeout=15)
    subprocess.run(['docker', 'run', '--rm', '--read-only', '--network', 'none', '--user', '65534:65534',
                    '--entrypoint', 'sh', '-v', f'{state / "init.sql"}:/miy-init.sql:ro',
                    '-v', f'{state / "realm.json"}:/miy-realm.json:ro', 'postgres:18.3',
                    '-c', 'test -r /miy-init.sql && test -r /miy-realm.json'],
                   cwd=ROOT, env=ENV, check=True, capture_output=True, timeout=30)
    print('PASS certificate-free HTTP, configured ports, private files and credential preservation')
    subprocess.run([sys.executable, str(ROOT / 'scripts/render-server-config.py'), '--state-dir', str(state)],
                   env=ENV, check=True, capture_output=True, timeout=15)
    proxy = (state / 'nginx-miy.conf').read_text()
    require('listen 8080;' in proxy and 'ssl_certificate' not in proxy and 'Strict-Transport-Security' not in proxy,
            'HTTP render accidentally enabled TLS')
    require('cli/(downloads|installers)' in proxy and 'proxy_set_header Upgrade' in proxy, 'Download, installer or WebSocket route missing')
    blocks = proxy.split('\nserver {')
    frame_blocks = [block for block in blocks if 'proxy_pass http://127.0.0.1:18083;' in block]
    require(len(frame_blocks) == 1, 'Nginx has no single app frame block')
    frame_block = frame_blocks[0]
    # Only the frame block passes the client Host: Core maps it to a registered
    # installation origin; it never selects a purpose.
    require(all('$http_host' not in block for block in blocks if block is not frame_block)
            and 'proxy_set_header X-Forwarded-Proto $scheme' not in proxy,
            'Proxy forwarding trusts a client-selected origin')
    require('listen 20000-20199;' in frame_block and 'proxy_set_header Host $http_host;' in frame_block
            and 'access_log off;' in frame_block and 'X-Frame-Options' not in frame_block,
            'Nginx app frame block lost its port pool, Host, log or framing rules')
    require('frame-src http://10.2.3.4:*' in proxy and 'Referrer-Policy strict-origin-when-cross-origin' in proxy,
            'Nginx portals do not frame app origins or keep the launch Origin')
    require('listen 8081;' in proxy and 'proxy_pass http://127.0.0.1:18081;' in proxy,
            'Nginx console uses the wrong fixed-purpose listener')
    require('listen 8082;' in proxy and 'proxy_pass http://127.0.0.1:18082;' in proxy,
            'Nginx dev portal uses the wrong fixed-purpose listener')
    if shutil.which('nginx'):
        wrapper = state / 'nginx-validation.conf'
        wrapper.write_text(f'pid "{state / "nginx.pid"}"; error_log stderr; events {{}} http {{ access_log off; include "{state / "nginx-miy.conf"}"; }}')
        subprocess.run(['nginx', '-t', '-c', str(wrapper), '-p', str(state)],
                       env=ENV, check=True, capture_output=True, timeout=15)
    for service in ['serve', 'worker']:
        unit = (state / f'miy-v2-{service}.service').read_text()
        require('run-service.sh' in unit and f' {service}\n' in unit, 'Service does not use secret-filtering wrapper')
        require('MIY_BINARY=' in unit and 'MIY_STATE_DIR=' in unit and 'PROVISION_DATABASE_URL' not in unit,
                'Service environment rendered private credentials')
    if shutil.which('systemd-analyze'):
        subprocess.run(['systemd-analyze', 'verify', str(state / 'miy-v2-serve.service'),
                        str(state / 'miy-v2-worker.service')],
                       env=ENV, check=True, capture_output=True, timeout=15)
    print('PASS HTTP Nginx and systemd rendering without live changes')
    identity_sync(state)
    print('PASS existing realm HTTP settings via internal administration only')

    secure = root / 'https'
    bootstrap(secure, '--profile', 'server', '--origin', 'https://PLATFORM.example:443/')
    runtime = load_env(secure, 'runtime.env')
    require(runtime['MIY_ORIGIN'] == 'https://platform.example', 'HTTPS canonical origin mismatch')
    require(runtime['MIY_DEV_LOGIN'] == '0'
            and json.loads((secure / 'server.json').read_text())['sharedDevelopmentLogin'] is False,
            'HTTPS server enabled shared development accounts by default')
    require('MIY_CA_FILE' not in runtime and runtime['MIY_ALLOW_HTTP'] == '0', 'HTTPS defaults were weakened')
    require(json.loads((secure / 'realm.json').read_text())['sslRequired'] == 'external', 'HTTPS realm policy changed')
    result = subprocess.run([sys.executable, str(ROOT / 'scripts/render-server-config.py'), '--state-dir', str(secure)],
                            env=ENV, capture_output=True, timeout=15)
    require(result.returncode == 2, 'HTTPS renderer accepted missing certificates')
    ca, certificate, key = tls_fixture(root / 'tls-material')
    bootstrap(secure, '--ca-file', str(ca))
    subprocess.run([sys.executable, str(ROOT / 'scripts/render-server-config.py'), '--state-dir', str(secure),
                    '--certificate', str(certificate), '--key', str(key)],
                   env=ENV, check=True, capture_output=True, timeout=15)
    proxy = (secure / 'nginx-miy.conf').read_text()
    require('listen 443 ssl;' in proxy and 'ssl_certificate_key' in proxy and 'server_name "platform.example";' in proxy,
            'HTTPS renderer did not retain the configured domain and TLS')
    for bad_certificate, bad_key, extra in [
        (ca.parent / 'expired.pem', key, []),
        (certificate, ca.parent / 'ca-key.pem', []),
        (certificate, key, ['--ca-file', str(certificate)]),
    ]:
        result = subprocess.run([sys.executable, str(ROOT / 'scripts/check-server-tls.py'), '--state-dir', str(secure),
                                 '--certificate', str(bad_certificate), '--key', str(bad_key), *extra],
                                env=ENV, capture_output=True, timeout=20)
        require(result.returncode == 2, 'TLS validator accepted expired/untrusted/mismatched material')
    wrong_ip = root / 'wrong-ip-san'
    bootstrap(wrong_ip, '--profile', 'server', '--origin', 'https://10.2.3.5:8443', '--ca-file', str(ca))
    result = subprocess.run([sys.executable, str(ROOT / 'scripts/check-server-tls.py'), '--state-dir', str(wrong_ip),
                             '--certificate', str(certificate), '--key', str(key)], env=ENV, capture_output=True, timeout=20)
    require(result.returncode == 2, 'TLS validator accepted a missing IP SAN')
    print('PASS certificate/key matching, private CA trust, expiry and IP SAN rejection')

    shared = root / 'shared-development'
    bootstrap(shared, '--profile', 'server', '--origin', 'https://SHARED.example:443/', '--shared-development-login')
    shared_secrets = (shared / 'credentials.json').read_bytes()
    shared_realm = (shared / 'realm.json').read_bytes()
    shared_runtime = load_env(shared, 'runtime.env')
    shared_settings = json.loads((shared / 'server.json').read_text())
    require(shared_settings['sharedDevelopmentLogin'] is True and shared_settings['developmentLogin'] is True
            and shared_settings['developmentOrigin'] == 'https://shared.example',
            'Shared development settings did not bind to the explicit HTTPS origin')
    require(shared_runtime['MIY_DEV_LOGIN'] == 'shared' and shared_runtime['MIY_DEV_ORIGIN'] == shared_runtime['MIY_ORIGIN']
            and shared_runtime['MIY_ALLOW_HTTP'] == '0', 'Shared development runtime differs from the configured origin')
    require(json.loads(shared_realm)['clients'][0]['redirectUris'] ==
            ['https://shared.example/auth/callback', 'https://shared.example:9443/auth/callback',
             'https://shared.example:9444/auth/callback'],
            'Shared development changed the normal OAuth callback')
    bootstrap(shared)
    require(load_env(shared, 'runtime.env') == shared_runtime, 'Shared development preference was not preserved on rerun')
    require((shared / 'credentials.json').read_bytes() == shared_secrets and (shared / 'realm.json').read_bytes() == shared_realm,
            'Shared development rerun regenerated credentials or OAuth configuration')
    bootstrap(shared, '--no-shared-development-login')
    require(load_env(shared, 'runtime.env')['MIY_DEV_LOGIN'] == '0'
            and json.loads((shared / 'server.json').read_text())['sharedDevelopmentLogin'] is False,
            'Explicit shared development disable did not take effect')
    bootstrap(shared)
    require(load_env(shared, 'runtime.env')['MIY_DEV_LOGIN'] == '0', 'Disabled shared development mode returned on rerun')
    require((shared / 'credentials.json').read_bytes() == shared_secrets and (shared / 'realm.json').read_bytes() == shared_realm,
            'Disabling shared development replaced credentials or OAuth configuration')
    bootstrap(shared, environment={'MIY_SHARED_DEVELOPMENT_LOGIN': '1'})
    require(load_env(shared, 'runtime.env')['MIY_DEV_LOGIN'] == 'shared', 'Shared development environment opt-in was ignored')
    bootstrap(shared, '--no-shared-development-login', environment={'MIY_SHARED_DEVELOPMENT_LOGIN': '1'})
    require(load_env(shared, 'runtime.env')['MIY_DEV_LOGIN'] == '0', 'Explicit disable did not override environment opt-in')
    bootstrap(shared, '--shared-development-login', environment={'MIY_SHARED_DEVELOPMENT_LOGIN': '0'})
    require(load_env(shared, 'runtime.env')['MIY_DEV_LOGIN'] == 'shared', 'Explicit opt-in did not override environment disable')
    bootstrap(shared, environment={'MIY_SHARED_DEVELOPMENT_LOGIN': '0'})
    require(load_env(shared, 'runtime.env')['MIY_DEV_LOGIN'] == '0', 'Shared development environment disable was ignored')
    bootstrap(shared, '--shared-development-login')
    bootstrap(shared, '--origin', 'https://different.example')
    require(load_env(shared, 'runtime.env')['MIY_DEV_LOGIN'] == '0'
            and json.loads((shared / 'server.json').read_text())['sharedDevelopmentLogin'] is False,
            'Changing origin implicitly enabled shared development accounts at the new address')
    bootstrap(shared, '--shared-development-login')
    require(load_env(shared, 'runtime.env')['MIY_DEV_ORIGIN'] == 'https://different.example'
            and load_env(shared, 'runtime.env')['MIY_DEV_LOGIN'] == 'shared',
            'New origin could not explicitly enable shared development accounts')
    invalid_shared = root / 'invalid-shared-development'
    bootstrap(invalid_shared, '--profile', 'server', '--origin', 'https://shared.example', expected=2,
              environment={'MIY_SHARED_DEVELOPMENT_LOGIN': 'yes'})
    require(not invalid_shared.exists(), 'Invalid shared development environment value wrote configuration')
    print('PASS explicit shared development origin, persistence, disable and unchanged OAuth credentials')

    local = root / 'local'
    bootstrap(local, '--profile', 'local')
    runtime = load_env(local, 'runtime.env')
    require(runtime['MIY_ORIGIN'] == 'https://localhost:18443' and runtime['MIY_DEV_LOGIN'] == '1', 'Local validation profile changed')
    require(json.loads((local / 'server.json').read_text())['sharedDevelopmentLogin'] is False,
            'Local validation implicitly enabled shared development mode')
    bootstrap(local, '--shared-development-login')
    runtime = load_env(local, 'runtime.env')
    require(runtime['MIY_DEV_LOGIN'] == 'shared' and runtime['MIY_DEV_ORIGIN'] == runtime['MIY_ORIGIN'],
            'Explicit shared mode did not override the local development origin')
    bootstrap(local, '--no-shared-development-login')
    runtime = load_env(local, 'runtime.env')
    require(runtime['MIY_DEV_LOGIN'] == '1' and runtime['MIY_DEV_ORIGIN'] == runtime['MIY_ORIGIN']
            and runtime['MIY_CONSOLE_ORIGIN'] == 'https://localhost:18444'
            and runtime['MIY_DEV_PORTAL_ORIGIN'] == 'https://localhost:18445',
            'Local development did not use the three configured HTTPS purposes')
    # The local profile also serves its default app frame pool (18320-18339).
    check_caddy_config(local, [18443, 18444, 18445, 18000, *range(18320, 18340)])
    bootstrap(local, '--origin', 'http://localhost:18000')
    check_caddy_config(local, [18000, 18001, 18002, *range(18320, 18340)])
    bootstrap(local, '--origin', 'http://10.2.3.4:18000', '--allow-http')
    runtime = load_env(local, 'runtime.env')
    require(runtime['MIY_DEV_LOGIN'] == '0' and 'MIY_CA_FILE' not in runtime, 'Non-loopback HTTP enabled synthetic login or inherited CA')
    print('PASS HTTPS system roots and loopback-only synthetic login')

    if shutil.which('nginx'):
        proxy_regression(root / 'nginx-http-boundaries', 'nginx')
        print('PASS Nginx fixed purposes, scoped IdP, CLI redirects, content denial and security headers')
        proxy_regression(root / 'nginx-single-ip-tls', 'nginx', (ca, certificate, key))
        print('PASS actual single-IP HTTPS portal/console proxy and client trust checks')
    else:
        print('NOT_RUN Nginx proxy checks: nginx is not installed')
    proxy_regression(root / 'caddy-http-boundaries', 'caddy')
    print('PASS Caddy fixed purposes, scoped IdP, CLI redirects, content denial and security headers')
