#!/usr/bin/env python3
"""Check generated HTTP/HTTPS configuration using isolated temporary state only."""
import http.server
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import threading

ROOT = Path(__file__).resolve().parent.parent
ENV = {key: value for key, value in os.environ.items() if not key.startswith('MIY_')}


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def bootstrap(state, *args, expected=0):
    result = subprocess.run(
        [sys.executable, str(ROOT / 'scripts/bootstrap.py'), '--state-dir', str(state), *args],
        env=ENV, capture_output=True, text=True, timeout=15,
    )
    require(result.returncode == expected, 'Unexpected bootstrap result')
    if expected == 0:
        require('PASSWORD' not in result.stdout and 'postgres://' not in result.stdout,
                'Configuration output exposed private data')


def load_env(state, filename):
    return {key: ' '.join(shlex.split(value)) for key, value in
            (line.split('=', 1) for line in (state / filename).read_text().splitlines())}


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


with tempfile.TemporaryDirectory(prefix='miy-server-config-') as temporary:
    root = Path(temporary)
    missing = root / 'missing'
    bootstrap(missing, '--profile', 'server', expected=2)
    require(not missing.exists(), 'Missing server origin created configuration')
    for index, origin in enumerate(['http://10.2.3.4:8080', 'http://intranet.example', 'http://[fd00::4]:8080']):
        state = root / f'http-refused-{index}'
        bootstrap(state, '--profile', 'server', '--origin', origin, expected=2)
        require(not state.exists(), 'HTTP opt-out created configuration')
    print('PASS explicit server origin and HTTP opt-in')

    state = root / 'http state with spaces'
    bootstrap(state, '--profile', 'server', '--origin', 'http://10.2.3.4:8080/', '--allow-http',
              '--bind', '127.0.0.1:18180', '--postgres-port', '15439', '--identity-port', '18189')
    runtime = load_env(state, 'runtime.env')
    compose = load_env(state, 'compose.env')
    realm = json.loads((state / 'realm.json').read_text())
    require(runtime['MIY_ORIGIN'] == 'http://10.2.3.4:8080' and runtime['MIY_ALLOW_HTTP'] == '1', 'HTTP configuration mismatch')
    require(runtime['MIY_BIND_ADDR'] == '127.0.0.1:18180' and runtime['MIY_DEV_LOGIN'] == '0', 'Unsafe development profile')
    require('MIY_CA_FILE' not in runtime and realm['sslRequired'] == 'none', 'HTTP requires a certificate')
    require(compose['MIY_CADDY_CONFIG'].endswith('Caddyfile.http'), 'Wrong HTTP proxy configuration')
    require(compose['MIY_STATE_DIR'] == str(state), 'Space-containing state path was corrupted')
    require(runtime['MIY_CLI_INSTALLER_DIR'] == str(state / 'cli-installers'), 'Installer directory was not configured')
    require(realm['clients'][0]['redirectUris'] == ['http://10.2.3.4:8080/auth/callback'], 'Incorrect browser callback')
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
    require('MIY_CA_FILE' not in runtime and runtime['MIY_ALLOW_HTTP'] == '0', 'HTTPS defaults were weakened')
    require(json.loads((secure / 'realm.json').read_text())['sslRequired'] == 'external', 'HTTPS realm policy changed')
    result = subprocess.run([sys.executable, str(ROOT / 'scripts/render-server-config.py'), '--state-dir', str(secure)],
                            env=ENV, capture_output=True, timeout=15)
    require(result.returncode == 2, 'HTTPS renderer accepted missing certificates')
    subprocess.run([sys.executable, str(ROOT / 'scripts/render-server-config.py'), '--state-dir', str(secure),
                    '--certificate', str(secure / 'certificate.pem'), '--key', str(secure / 'private-key.pem')],
                   env=ENV, check=True, capture_output=True, timeout=15)
    proxy = (secure / 'nginx-miy.conf').read_text()
    require('listen 443 ssl;' in proxy and 'ssl_certificate_key' in proxy and 'server_name "platform.example";' in proxy,
            'HTTPS renderer did not retain the configured domain and TLS')
    local = root / 'local'
    bootstrap(local, '--profile', 'local')
    runtime = load_env(local, 'runtime.env')
    require(runtime['MIY_ORIGIN'] == 'https://localhost:18443' and runtime['MIY_DEV_LOGIN'] == '1', 'Local validation profile changed')
    bootstrap(local, '--origin', 'http://10.2.3.4:18000', '--allow-http')
    runtime = load_env(local, 'runtime.env')
    require(runtime['MIY_DEV_LOGIN'] == '0' and 'MIY_CA_FILE' not in runtime, 'Non-loopback HTTP enabled synthetic login or inherited CA')
    print('PASS HTTPS system roots and loopback-only synthetic login')
