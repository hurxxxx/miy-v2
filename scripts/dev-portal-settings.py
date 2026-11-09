#!/usr/bin/env python3
"""Add the dev app portal origin to an existing two-origin server, one reversible step at a time.

Every step is additive and the previous server binary keeps working after it, so the steps can
run in this order with the maintained server/UI rollouts in between:

  inspect        read-only: validate the requested origin/bind against server.json, the free
                 public port, the identity client and the live Nginx file. Prints public data.
  apply          back up and patch server.json, runtime.env and compose.env line by line (other
                 lines byte-identical), then add only the dev portal callback and post-logout URI
                 to Keycloak's miy-web client (web origins unchanged; ids and secret verified).
  restore        undo apply from its backups, refusing if a file changed since apply.
  nginx-apply    render the fixed-purpose Nginx config, require it to equal the live file plus
                 one dev portal server block, install it with a backup, test and reload Nginx.
  nginx-restore  reinstall the backed-up Nginx file, test and reload.

Then: scripts/core-rollout.py apply --widen-migration 21 (the server needs the new settings),
nginx-apply, scripts/ui-rollout.py apply. Roll back in reverse order. Nothing here restarts the
core, touches PostgreSQL or replaces Keycloak users, secrets, scopes or realm settings. Run only
after an explicit production request; credential values are never printed or recorded.
"""
import argparse
import hashlib
import ipaddress
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
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
SITE = Path('/etc/nginx/sites-available/v2.dwdcc.kr')
FILES = ('server.json', 'runtime.env', 'compose.env')


def require(ok, code):
    if not ok:
        raise RuntimeError(code)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def run(args, **kwargs):
    result = subprocess.run([str(a) for a in args], capture_output=True, text=True, **kwargs)
    require(result.returncode == 0, 'command_failed:' + Path(str(args[0])).name)
    return result.stdout


def canonical_origin(value):
    parsed = urllib.parse.urlsplit(value)
    require(parsed.scheme in ('http', 'https') and parsed.hostname and not parsed.username
            and parsed.password is None and parsed.path in ('', '/') and not parsed.query
            and not parsed.fragment, 'invalid_dev_portal_origin')
    host = '[' + parsed.hostname + ']' if ':' in parsed.hostname else parsed.hostname
    default = 443 if parsed.scheme == 'https' else 80
    return f'{parsed.scheme}://{host}' + (f':{parsed.port}' if parsed.port and parsed.port != default else '')


def loopback_bind(value):
    parsed = urllib.parse.urlsplit('//' + value)
    require(parsed.hostname and parsed.port and not parsed.path, 'invalid_dev_portal_bind')
    require(ipaddress.ip_address(parsed.hostname).is_loopback, 'dev_portal_bind_must_be_loopback')
    return value


def env_values(raw):
    values = {}
    for line in raw.decode().splitlines():
        if not line or line.startswith('#'):
            continue
        key, value = line.split('=', 1)
        parts = shlex.split(value)
        require(len(parts) == 1 and key not in values, 'invalid_environment_file')
        values[key] = parts[0]
    return values


def env_patch(raw, updates):
    """Replace or append only the given keys; every other line stays byte-identical."""
    pending = dict(updates)
    lines = []
    for line in raw.decode().splitlines(keepends=True):
        key = line.split('=', 1)[0]
        lines.append(key + '=' + shlex.quote(pending.pop(key)) + '\n' if key in pending else line)
    if lines and not lines[-1].endswith('\n'):
        lines[-1] += '\n'
    lines.extend(key + '=' + shlex.quote(value) + '\n' for key, value in pending.items())
    return ''.join(lines).encode()


def private_write(path, data):
    mode = path.stat().st_mode & 0o777 if path.exists() else 0o600
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as handle:
        handle.write(data)
        temporary = Path(handle.name)
    temporary.chmod(mode)
    os.replace(temporary, path)


class Identity:
    """Keycloak admin REST over the configured loopback endpoint (never a public address)."""

    def __init__(self, state):
        settings = json.loads((state / 'server.json').read_text())
        credentials = json.loads((state / 'credentials.json').read_text())
        self.base = settings['identityAdminUrl']
        parsed = urllib.parse.urlsplit(self.base)
        require(parsed.scheme in ('http', 'https') and ipaddress.ip_address(parsed.hostname).is_loopback
                and parsed.path == '/identity', 'identity_admin_must_be_loopback')

        class NoRedirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, *_args, **_kwargs):
                return None

        handlers = [NoRedirect(), urllib.request.ProxyHandler({})]
        if parsed.scheme == 'https':
            handlers.append(urllib.request.HTTPSHandler(
                context=ssl.create_default_context(cafile=settings.get('caFile'))))
        self.opener = urllib.request.build_opener(*handlers)
        form = urllib.parse.urlencode({'client_id': 'admin-cli', 'grant_type': 'password', 'username': 'bootstrap',
                                       'password': credentials['identity_admin']}).encode()
        try:
            with self.opener.open(urllib.request.Request(
                    self.base + '/realms/master/protocol/openid-connect/token', data=form), timeout=20) as response:
                self.token = json.load(response)['access_token']
        except Exception:
            raise RuntimeError('identity_admin_authentication_failed') from None

    def call(self, path, method='GET', body=None):
        request = urllib.request.Request(self.base + '/admin/realms/miy' + path, method=method,
                                         headers={'Authorization': 'Bearer ' + self.token,
                                                  'Content-Type': 'application/json'},
                                         data=json.dumps(body).encode() if body is not None else None)
        try:
            with self.opener.open(request, timeout=20) as response:
                raw = response.read()
                return json.loads(raw) if raw else None
        except Exception:
            raise RuntimeError('identity_admin_request_failed:' + method) from None

    def snapshot(self):
        clients = self.call('/clients?clientId=miy-web')
        require(len(clients) == 1 and clients[0]['clientId'] == 'miy-web', 'identity_target_ambiguous')
        client = self.call('/clients/' + clients[0]['id'])
        secret = self.call('/clients/' + client['id'] + '/client-secret')['value']
        return {'client': client, 'clientSecretSha256': digest(secret.encode())}

    def update(self, client_id, body):
        self.call('/clients/' + client_id, 'PUT', body)


def logout_uris(client):
    value = client.get('attributes', {}).get('post.logout.redirect.uris') or ''
    return [item for item in value.split('##') if item]


def identity_change(api, original, dev_origin, add):
    """Add or remove only the dev portal callback/logout URI; verify ids and secret."""
    current = api.snapshot()
    require(current['client']['id'] == original['client']['id']
            and current['clientSecretSha256'] == original['clientSecretSha256'], 'identity_id_or_secret_changed')
    callback, logout = dev_origin + '/auth/callback', dev_origin + '/'
    client = current['client']
    redirects = [item for item in client.get('redirectUris', []) if item != callback]
    logouts = [item for item in logout_uris(client) if item != logout]
    if add:
        redirects.append(callback)
        logouts.append(logout)
    attributes = dict(client.get('attributes', {}))
    attributes['post.logout.redirect.uris'] = '##'.join(logouts)
    api.update(client['id'], {'redirectUris': redirects, 'webOrigins': client.get('webOrigins', []),
                              'attributes': attributes})
    after = api.snapshot()
    require(after['client']['id'] == original['client']['id']
            and after['clientSecretSha256'] == original['clientSecretSha256'], 'identity_id_or_secret_not_preserved')
    require(sorted(after['client'].get('redirectUris', [])) == sorted(redirects)
            and sorted(logout_uris(after['client'])) == sorted(logouts)
            and sorted(after['client'].get('webOrigins', [])) == sorted(client.get('webOrigins', [])),
            'identity_change_not_verified')
    return {'redirectUris': len(redirects), 'postLogoutUris': len(logouts),
            'devPortalCallback': callback in after['client']['redirectUris']}


def plan(state, dev_origin, dev_bind):
    settings = json.loads((state / 'server.json').read_text())
    portal, console = settings['origin'], settings.get('consoleOrigin')
    require(console and settings.get('consoleBind'), 'two_origin_server_required')
    dev_origin = canonical_origin(dev_origin)
    dev_bind = loopback_bind(dev_bind)
    scheme = urllib.parse.urlsplit(portal).scheme
    require(dev_origin not in (portal, console) and urllib.parse.urlsplit(dev_origin).scheme == scheme,
            'dev_portal_origin_must_be_distinct_with_same_transport')
    require(dev_bind not in (settings['bind'], settings['consoleBind']), 'dev_portal_bind_must_be_distinct')
    existing = settings.get('devPortalOrigin')
    require(existing in (None, dev_origin) and settings.get('devPortalBind') in (None, dev_bind),
            'different_dev_portal_already_configured')
    public = urllib.parse.urlsplit(dev_origin)
    port = public.port or (443 if scheme == 'https' else 80)
    return {
        'portalOrigin': portal, 'consoleOrigin': console, 'devPortalOrigin': dev_origin, 'devPortalBind': dev_bind,
        'publicPort': port, 'alreadyConfigured': existing == dev_origin,
        'server': {'devPortalOrigin': dev_origin, 'devPortalBind': dev_bind},
        'runtime': {'MIY_DEV_PORTAL_ORIGIN': dev_origin, 'MIY_DEV_PORTAL_BIND_ADDR': dev_bind},
        'compose': {'MIY_DEV_PORTAL_ORIGIN': dev_origin, 'MIY_DEV_PORTAL_CORE_UPSTREAM': dev_bind,
                    'MIY_DEV_PORTAL_HOST': public.netloc, 'MIY_DEV_PORTAL_PORT': str(port)},
    }


def port_free(port, probe=None):
    """True when nothing listens on the public port yet (Nginx will listen there)."""
    if probe:
        return probe(port)
    with socket.socket() as sock:
        sock.settimeout(1)
        return sock.connect_ex(('127.0.0.1', port)) != 0


def inspect(state, dev_origin, dev_bind, identity=Identity, probe=None):
    target = plan(state, dev_origin, dev_bind)
    require(target['alreadyConfigured'] or port_free(target['publicPort'], probe), 'dev_portal_port_in_use')
    snapshot = identity(state).snapshot()
    client = snapshot['client']
    return {'status': 'READY', 'devPortalOrigin': target['devPortalOrigin'], 'devPortalBind': target['devPortalBind'],
            'alreadyConfigured': target['alreadyConfigured'],
            'identity': {'redirectUris': len(client.get('redirectUris', [])),
                         'webOrigins': len(client.get('webOrigins', [])),
                         'devPortalCallback': target['devPortalOrigin'] + '/auth/callback'
                         in client.get('redirectUris', [])}}


def apply(state, release, dev_origin, dev_bind, identity=Identity, probe=None):
    target = plan(state, dev_origin, dev_bind)
    require(target['alreadyConfigured'] or port_free(target['publicPort'], probe), 'dev_portal_port_in_use')
    backup = release / 'settings-backup'
    require(not backup.exists(), 'settings_backup_exists')
    release.mkdir(mode=0o700, parents=True, exist_ok=True)
    backup.mkdir(mode=0o700)
    api = identity(state)
    original_identity = api.snapshot()
    (backup / 'identity.json').write_text(json.dumps(original_identity))
    (backup / 'identity.json').chmod(0o600)
    record = {'devPortalOrigin': target['devPortalOrigin'], 'devPortalBind': target['devPortalBind'], 'files': {}}
    for name in FILES:
        original = (state / name).read_bytes()
        shutil.copy2(state / name, backup / name)
        (backup / name).chmod(0o600)
        if name == 'server.json':
            settings = json.loads(original)
            settings.update(target['server'])
            written = (json.dumps(settings, indent=2) + '\n').encode()
        else:
            written = env_patch(original, target['runtime' if name == 'runtime.env' else 'compose'])
            require(env_values(written).keys() >= env_values(original).keys(), 'environment_patch_lost_keys')
        private_write(state / name, written)
        record['files'][name] = {'before': digest(original), 'after': digest(written)}
    record['identity'] = identity_change(api, original_identity, target['devPortalOrigin'], add=True)
    (release / 'settings.json').write_text(json.dumps(record, indent=2))
    return {'status': 'APPLIED', **{key: record[key] for key in ('devPortalOrigin', 'devPortalBind', 'identity')}}


def restore(state, release, identity=Identity):
    record = json.loads((release / 'settings.json').read_text())
    backup = release / 'settings-backup'
    for name, hashes in record['files'].items():
        require(digest((state / name).read_bytes()) == hashes['after'], 'settings_changed_since_apply:' + name)
        require(digest((backup / name).read_bytes()) == hashes['before'], 'settings_backup_changed:' + name)
    original_identity = json.loads((backup / 'identity.json').read_text())
    result = identity_change(identity(state), original_identity, record['devPortalOrigin'], add=False)
    for name in record['files']:
        private_write(state / name, (backup / name).read_bytes())
    return {'status': 'RESTORED', 'identity': result}


def nginx_apply(state, release, certificate, key, site=SITE, sudo=('sudo', '-n')):
    live = site.read_bytes()
    rendered_copy = state / 'nginx-miy.conf'
    require(rendered_copy.read_bytes() == live, 'live_nginx_differs_from_rendered_copy')
    settings = json.loads((state / 'server.json').read_text())
    require(settings.get('devPortalOrigin') and settings.get('devPortalBind'), 'apply_settings_first')
    work = Path(tempfile.mkdtemp(prefix='miy-dev-portal-render-'))
    try:
        (work / 'server.json').write_text(json.dumps(settings))
        run([*sudo, sys.executable, ROOT / 'scripts/render-server-config.py', '--state-dir', work,
             '--binary', state / 'bin/miy-server', '--certificate', certificate, '--key', key,
             '--user', os.environ.get('USER', 'root')])
        candidate = run([*sudo, 'cat', work / 'nginx-miy.conf']).encode()
    finally:
        run([*sudo, 'rm', '-rf', work])
    blocks = candidate.decode().split('\nserver {')
    require(len(blocks) == 4 and '\nserver {'.join(blocks[:-1]).split('\n', 1)[1].rstrip()
            == live.decode().split('\n', 1)[1].rstrip(), 'rendered_nginx_changes_more_than_the_dev_portal')
    backup = release / 'nginx-backup.conf'
    require(not backup.exists(), 'nginx_backup_exists')
    backup.write_bytes(live)
    backup.chmod(0o600)
    staged = release / 'nginx-candidate.conf'
    staged.write_bytes(candidate)
    run([*sudo, 'install', '-m', '0644', '-o', 'root', '-g', 'root', staged, site])
    try:
        run([*sudo, 'nginx', '-t'])
    except RuntimeError:
        run([*sudo, 'install', '-m', '0644', '-o', 'root', '-g', 'root', backup, site])
        raise RuntimeError('nginx_test_failed_restored') from None
    run([*sudo, 'systemctl', 'reload', 'nginx'])
    private_write(rendered_copy, candidate)
    return {'status': 'NGINX_APPLIED', 'previousSha256': digest(live), 'nginxSha256': digest(candidate)}


def nginx_restore(state, release, site=SITE, sudo=('sudo', '-n')):
    backup = (release / 'nginx-backup.conf').read_bytes()
    candidate = (release / 'nginx-candidate.conf').read_bytes()
    require(site.read_bytes() == candidate, 'live_nginx_changed_since_apply')
    run([*sudo, 'install', '-m', '0644', '-o', 'root', '-g', 'root', release / 'nginx-backup.conf', site])
    run([*sudo, 'nginx', '-t'])
    run([*sudo, 'systemctl', 'reload', 'nginx'])
    private_write(state / 'nginx-miy.conf', backup)
    return {'status': 'NGINX_RESTORED', 'nginxSha256': digest(backup)}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('action', choices=['inspect', 'apply', 'restore', 'nginx-apply', 'nginx-restore'])
    parser.add_argument('--state-dir', type=Path, default=Path(os.getenv('MIY_STATE_DIR', str(ROOT / '.local'))))
    parser.add_argument('--dev-portal-origin')
    parser.add_argument('--dev-portal-bind', default='127.0.0.1:18183')
    parser.add_argument('--release-dir', type=Path)
    parser.add_argument('--certificate', type=Path)
    parser.add_argument('--key', type=Path)
    args = parser.parse_args(argv)
    state = args.state_dir.resolve()
    try:
        if args.action in ('inspect', 'apply'):
            require(args.dev_portal_origin is not None, 'missing_dev_portal_origin')
        if args.action != 'inspect':
            require(args.release_dir is not None, 'missing_release_dir')
            release = args.release_dir.resolve()
            require(release.is_relative_to(state), 'release_dir_must_be_under_state')
        if args.action == 'inspect':
            result = inspect(state, args.dev_portal_origin, args.dev_portal_bind)
        elif args.action == 'apply':
            result = apply(state, release, args.dev_portal_origin, args.dev_portal_bind)
        elif args.action == 'restore':
            result = restore(state, release)
        elif args.action == 'nginx-apply':
            require(args.certificate and args.key, 'missing_certificate_or_key')
            result = nginx_apply(state, release, args.certificate, args.key)
        else:
            result = nginx_restore(state, release)
    except (RuntimeError, OSError, KeyError, ValueError) as error:
        code = str(error) if isinstance(error, RuntimeError) else type(error).__name__
        print(json.dumps({'status': 'FAIL', 'action': args.action, 'failure': code}))
        return 1
    print(json.dumps(result))
    return 0


if __name__ == '__main__':
    sys.exit(main())
