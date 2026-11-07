#!/usr/bin/env python3
"""Prepared two-origin settings transition. No action runs on import.

Only root's deployment orchestrator should call mutation functions. They do not
stop/start services, migrate databases, reload nginx, or recreate containers.
Private backup/candidate files are never returned in command output.
"""
import copy
import hashlib
import ipaddress
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import ssl
import subprocess
import tempfile
import urllib.parse
import urllib.request

LIVE = Path('/projects/miy-v2')
STATE = LIVE / '.local'
SITE = Path('/etc/nginx/sites-available/v2.dwdcc.kr')
PORTAL = 'https://v2.dwdcc.kr'
CONSOLE = 'https://v2.dwdcc.kr:9443'
CONSOLE_BIND = '127.0.0.1:18182'
CERTIFICATE = Path('/etc/lego/production/certificates/dwdcc.kr.crt')
KEY = Path('/etc/lego/production/certificates/dwdcc.kr.key')


def require(ok, code):
    if not ok:
        raise RuntimeError(code)


def run(command, **kwargs):
    result = subprocess.run([str(v) for v in command], capture_output=True, **kwargs)
    require(result.returncode == 0, 'command_failed:' + Path(str(command[0])).name)
    return result.stdout


def read_private(path):
    path = Path(path)
    try:
        return path.read_bytes()
    except PermissionError:
        return run(['sudo', '-n', 'cat', path])


def digest_bytes(value):
    return hashlib.sha256(value).hexdigest()


def private_write(path, value):
    path = Path(path)
    flags = os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW
    fd = os.open(path, flags, 0o600)
    with os.fdopen(fd, 'wb') as handle:
        handle.write(value.encode() if isinstance(value, str) else value)
        handle.flush()
        os.fsync(handle.fileno())
    path.chmod(0o600)


def write_json(path, value):
    private_write(path, json.dumps(value, indent=2) + '\n')


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
    # Keep every unrelated credential/configuration line byte-for-byte.
    pending = dict(updates)
    lines = []
    for line in raw.decode().splitlines(keepends=True):
        key = line.split('=', 1)[0]
        if key in pending:
            lines.append(key + '=' + shlex.quote(pending.pop(key)) + '\n')
        else:
            lines.append(line)
    if lines and not lines[-1].endswith('\n'):
        lines[-1] += '\n'
    lines.extend(key + '=' + shlex.quote(value) + '\n' for key, value in pending.items())
    return ''.join(lines).encode()


def manifest(prepared):
    return json.loads((Path(prepared) / 'manifest.json').read_text())


def validate_frontend(value):
    u = urllib.parse.urlsplit(value)
    require(u.scheme == 'https' and u.hostname == 'v2.dwdcc.kr' and u.port == 9443
            and u.path == '/identity' and not u.query and not u.fragment
            and u.username is None and u.password is None, 'invalid_frontend_override')


class Identity:
    def __init__(self, state=STATE):
        state = Path(state)
        settings = json.loads((state / 'server.json').read_text())
        credentials = json.loads((state / 'credentials.json').read_text())
        self.base = settings['identityAdminUrl']
        u = urllib.parse.urlsplit(self.base)
        require(u.scheme in ['http', 'https'] and ipaddress.ip_address(u.hostname).is_loopback
                and u.path == '/identity' and not u.query and not u.fragment
                and u.username is None, 'identity_admin_must_be_loopback')
        class NoRedirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, *_args, **_kwargs):
                return None
        handlers = [NoRedirect(), urllib.request.ProxyHandler({})]
        if u.scheme == 'https':
            handlers.append(urllib.request.HTTPSHandler(context=ssl.create_default_context(cafile=settings.get('caFile'))))
        self.opener = urllib.request.build_opener(*handlers)
        form = urllib.parse.urlencode({'client_id': 'admin-cli', 'grant_type': 'password',
                                       'username': 'bootstrap', 'password': credentials['identity_admin']}).encode()
        try:
            with self.opener.open(urllib.request.Request(self.base + '/realms/master/protocol/openid-connect/token', data=form), timeout=20) as response:
                self.token = json.load(response)['access_token']
        except Exception:
            raise RuntimeError('identity_admin_authentication_failed') from None

    def call(self, path, method='GET', body=None):
        request = urllib.request.Request(self.base + '/admin/realms/miy' + path, method=method,
                    headers={'Authorization': 'Bearer ' + self.token, 'Content-Type': 'application/json'},
                    data=json.dumps(body).encode() if body is not None else None)
        try:
            with self.opener.open(request, timeout=20) as response:
                raw = response.read()
                return json.loads(raw) if raw else None
        except Exception:
            raise RuntimeError('identity_admin_request_failed:' + method) from None

    def snapshot(self):
        realm = self.call('')
        clients = self.call('/clients?clientId=miy-web')
        require(len(clients) == 1 and clients[0]['clientId'] == 'miy-web' and realm['realm'] == 'miy', 'identity_target_ambiguous')
        client = self.call('/clients/' + clients[0]['id'])
        secret = self.call('/clients/' + client['id'] + '/client-secret')['value']
        return {'realm': realm, 'client': client, 'clientSecretSha256': digest_bytes(secret.encode())}

    def discovery(self):
        with self.opener.open(self.base + '/realms/miy/.well-known/openid-configuration', timeout=20) as response:
            return json.load(response)


def identity_prepare(prepared, state=STATE):
    prepared = Path(prepared)
    target = prepared / 'backup/identity.json'
    require(not target.exists(), 'identity_backup_already_exists')
    value = Identity(state).snapshot()
    write_json(target, value)
    return {'realm': value['realm']['realm'], 'client': value['client']['clientId'], 'backedUp': True}


def _identity_patch(api, original, portal, console, restore=False):
    frontend = console + '/identity'
    current = api.snapshot()
    require(current['realm']['id'] == original['realm']['id']
            and current['client']['id'] == original['client']['id']
            and current['clientSecretSha256'] == original['clientSecretSha256'], 'identity_id_or_secret_changed')
    prior_realm_attrs = original['realm'].get('attributes', {})
    realm_attrs = dict(current['realm'].get('attributes', {}))
    allowed = [prior_realm_attrs.get('frontendUrl'), frontend]
    require(realm_attrs.get('frontendUrl') in allowed, 'concurrent_frontend_change')
    client = current['client']
    attrs = dict(client.get('attributes', {}))
    original_client = original['client']
    old_logout = original_client.get('attributes', {}).get('post.logout.redirect.uris')
    new_logout = '##'.join(value + '/' for value in [portal, console])
    desired_redirects = [value + '/auth/callback' for value in [portal, console]]
    require(sorted(client.get('redirectUris', [])) in [sorted(original_client.get('redirectUris', [])), sorted(desired_redirects)]
            and sorted(client.get('webOrigins', [])) in [sorted(original_client.get('webOrigins', [])), sorted([portal, console])]
            and attrs.get('post.logout.redirect.uris') in [old_logout, new_logout], 'concurrent_client_redirect_change')
    if restore:
        if 'frontendUrl' in prior_realm_attrs:
            realm_attrs['frontendUrl'] = prior_realm_attrs['frontendUrl']
        else:
            realm_attrs.pop('frontendUrl', None)
        if old_logout is None:
            attrs.pop('post.logout.redirect.uris', None)
        else:
            attrs['post.logout.redirect.uris'] = old_logout
        redirects = original_client.get('redirectUris', [])
        origins = original_client.get('webOrigins', [])
    else:
        realm_attrs['frontendUrl'] = frontend
        attrs['post.logout.redirect.uris'] = new_logout
        redirects, origins = desired_redirects, [portal, console]
    # Existing users, roles, client secret, scopes, mappers and realm settings are not replaced.
    api.call('/clients/' + client['id'], 'PUT', {'redirectUris': redirects, 'webOrigins': origins, 'attributes': attrs})
    api.call('', 'PUT', {'attributes': realm_attrs})
    after = api.snapshot()
    require(after['realm']['id'] == original['realm']['id']
            and after['client']['id'] == original['client']['id']
            and after['clientSecretSha256'] == original['clientSecretSha256'], 'identity_id_or_secret_not_preserved')
    require(sorted(after['client']['redirectUris']) == sorted(redirects) and sorted(after['client']['webOrigins']) == sorted(origins)
            and after['client'].get('attributes', {}).get('post.logout.redirect.uris') == attrs.get('post.logout.redirect.uris')
            and after['realm'].get('attributes', {}).get('frontendUrl') == realm_attrs.get('frontendUrl'), 'identity_patch_not_verified')
    return after


def identity_apply(prepared, state=STATE):
    prepared = Path(prepared)
    info = manifest(prepared)
    validate_frontend(info['consoleOrigin'] + '/identity')
    require((prepared / 'backup/identity.json').exists(), 'prepare_identity_backup_first')
    original = json.loads((prepared / 'backup/identity.json').read_text())
    api = Identity(state)
    _identity_patch(api, original, info['portalOrigin'], info['consoleOrigin'])
    value = api.discovery()
    expected = info['consoleOrigin'] + '/identity/realms/miy'
    require(value['issuer'] == expected and all(value[key].startswith(expected + '/') for key in
            ['authorization_endpoint', 'token_endpoint', 'introspection_endpoint', 'userinfo_endpoint', 'jwks_uri', 'end_session_endpoint']),
            'identity_discovery_did_not_move')
    return {'identityApplied': True, 'issuer': expected}


def identity_restore(prepared, state=STATE):
    prepared = Path(prepared)
    info = manifest(prepared)
    original = json.loads((prepared / 'backup/identity.json').read_text())
    api = Identity(state)
    _identity_patch(api, original, info['portalOrigin'], info['consoleOrigin'], restore=True)
    return {'identityRestored': True, 'issuer': api.discovery()['issuer']}


def _metadata(path):
    stat = Path(path).stat()
    return {'mode': stat.st_mode & 0o777, 'uid': stat.st_uid, 'gid': stat.st_gid}


def prepare(directory, source='/tmp/miy-release-58cecff', capture_identity=False):
    directory, source = Path(directory).absolute(), Path(source).resolve()
    require(directory.is_relative_to(STATE / 'evidence'), 'preparation_must_be_in_private_evidence')
    require(not any(path.is_symlink() for path in [directory, *directory.parents]), 'preparation_symlink_refused')
    directory.mkdir(mode=0o700, parents=True, exist_ok=False)
    backup, candidate = directory / 'backup', directory / 'candidate'
    backup.mkdir(mode=0o700); candidate.mkdir(mode=0o700)
    entries = {}
    for name, path in [('runtime.env', STATE / 'runtime.env'), ('server.json', STATE / 'server.json'),
                       ('compose.env', STATE / 'compose.env'), ('realm.json', STATE / 'realm.json'),
                       ('nginx.generated.conf', STATE / 'nginx-miy.conf'), ('nginx.site.conf', SITE)]:
        raw = read_private(path)
        private_write(backup / name, raw)
        entries[name] = {'destination': str(path), 'beforeSha256': digest_bytes(raw), **_metadata(path)}
    settings = json.loads((backup / 'server.json').read_text())
    values = env_values((backup / 'runtime.env').read_bytes())
    require(settings['origin'] == PORTAL and values['MIY_ORIGIN'] == PORTAL
            and settings['bind'] == '127.0.0.1:18180' and values['MIY_BIND_ADDR'] == settings['bind'], 'unexpected_live_portal_or_bind')
    require(not settings.get('consoleOrigin') and not settings.get('consoleBind'), 'live_console_already_configured')
    settings.update(consoleOrigin=CONSOLE, consoleBind=CONSOLE_BIND, sharedDevelopmentLogin=False, developmentLogin=False)
    write_json(candidate / 'server.json', settings)
    private_write(candidate / 'runtime.env', env_patch((backup / 'runtime.env').read_bytes(), {
        'MIY_CONSOLE_ORIGIN': CONSOLE, 'MIY_CONSOLE_BIND_ADDR': CONSOLE_BIND,
        'MIY_ISSUER': CONSOLE + '/identity/realms/miy', 'MIY_DEV_LOGIN': '0'}))
    private_write(candidate / 'compose.env', env_patch((backup / 'compose.env').read_bytes(), {
        'MIY_CONSOLE_ORIGIN': CONSOLE, 'MIY_CORE_UPSTREAM': settings['bind'],
        'MIY_CONSOLE_CORE_UPSTREAM': CONSOLE_BIND, 'MIY_IDENTITY_UPSTREAM': '127.0.0.1:18181',
        'MIY_PORTAL_HOST': 'v2.dwdcc.kr', 'MIY_CONSOLE_HOST': 'v2.dwdcc.kr:9443',
        'MIY_PUBLIC_SCHEME': 'https', 'MIY_PORTAL_PORT': '443', 'MIY_CONSOLE_PORT': '9443',
        'MIY_CONSOLE_HTTP_PORT': '18001', 'MIY_CONSOLE_HTTPS_PORT': '9443'}))
    realm = json.loads((backup / 'realm.json').read_text())
    realm.setdefault('attributes', {})['frontendUrl'] = CONSOLE + '/identity'
    web = next(client for client in realm['clients'] if client['clientId'] == 'miy-web')
    web.update(redirectUris=[value + '/auth/callback' for value in [PORTAL, CONSOLE]], webOrigins=[PORTAL, CONSOLE])
    web.setdefault('attributes', {})['post.logout.redirect.uris'] = '##'.join(value + '/' for value in [PORTAL, CONSOLE])
    write_json(candidate / 'realm.json', realm)
    # sudo is read-only for live certificate/key access; output files are confined
    # to the new candidate directory. Generated service units are never installed.
    run(['sudo', '-n', 'python3', source / 'scripts/render-server-config.py', '--state-dir', candidate,
         '--user', 'dwdcc', '--binary', STATE / 'bin/miy-server', '--certificate', CERTIFICATE, '--key', KEY])
    generated = read_private(candidate / 'nginx-miy.conf').decode()
    generated = generated.replace(str(source / 'web/platform/dist'), str(LIVE / 'web/platform/dist'))
    private_write(candidate / 'nginx.final.conf', generated)
    # The original host's HTTP/80 410 site is outside this file and stays untouched.
    maintenance = []
    for port in [443, 9443]:
        identity = ''
        if port == 9443:
            identity = f'''    location ~ ^/identity/(realms/miy(/|$)|resources/) {{
        proxy_pass http://127.0.0.1:18181;
        proxy_set_header Host "v2.dwdcc.kr:9443";
        proxy_set_header X-Forwarded-Host "v2.dwdcc.kr:9443";
        proxy_set_header X-Forwarded-Proto https;
        proxy_set_header X-Forwarded-Port 9443;
        proxy_set_header X-Forwarded-For $remote_addr;
        proxy_set_header Forwarded "";
    }}
'''
        maintenance.append(f'''server {{
    listen {port} ssl;
    server_name "v2.dwdcc.kr";
    ssl_certificate "{CERTIFICATE}";
    ssl_certificate_key "{KEY}";
    ssl_protocols TLSv1.2 TLSv1.3;
    proxy_hide_header X-Frame-Options;
    proxy_hide_header X-Content-Type-Options;
    add_header X-Frame-Options DENY always;
    add_header X-Content-Type-Options nosniff always;
    add_header Content-Security-Policy "frame-ancestors 'none'; worker-src 'none'" always;
{identity}    location / {{ return 503; }}
}}
''')
    private_write(candidate / 'nginx.maintenance.conf', '\n'.join(maintenance))
    for variant in ['final', 'maintenance']:
        wrapper = candidate / ('nginx.' + variant + '.validation.conf')
        private_write(wrapper, f'pid "{candidate}/nginx-validation.pid"; error_log stderr; events {{}} http {{ access_log off; include /etc/nginx/mime.types; include "{candidate}/nginx.{variant}.conf"; }}\n')
        run(['sudo', '-n', 'nginx', '-t', '-c', wrapper, '-p', candidate])
    for name in ['runtime.env', 'server.json', 'compose.env', 'realm.json']:
        entries[name]['candidateSha256'] = digest_bytes((candidate / name).read_bytes())
    info = {'portalOrigin': PORTAL, 'consoleOrigin': CONSOLE, 'consoleBind': CONSOLE_BIND,
            'source': str(source), 'sourceCommit': run(['git', '-C', source, 'rev-parse', 'HEAD']).decode().strip(),
            'files': entries, 'nginx': {phase: digest_bytes((candidate / f'nginx.{phase}.conf').read_bytes()) for phase in ['final', 'maintenance']}}
    write_json(directory / 'manifest.json', info)
    if capture_identity:
        identity_prepare(directory)
    return {'prepared': str(directory), 'portalOrigin': PORTAL, 'consoleOrigin': CONSOLE, 'consoleBind': CONSOLE_BIND,
            'nginxValidated': ['maintenance', 'final'], 'identityBackupCaptured': capture_identity}


_ATOMIC = '''import os,pathlib,sys,tempfile
source,destination=map(pathlib.Path,sys.argv[1:3]);mode,uid,gid=map(int,sys.argv[3:])
fd,name=tempfile.mkstemp(prefix=destination.name+'.boundary-',dir=destination.parent)
try:
 with os.fdopen(fd,'wb') as out:
  out.write(source.read_bytes());out.flush();os.fsync(out.fileno())
 os.chmod(name,mode);os.chown(name,uid,gid);os.replace(name,destination)
finally:
 if os.path.exists(name):os.unlink(name)
'''


def install_file(source, entry, allowed):
    require(digest_bytes(read_private(entry['destination'])) in allowed, 'live_configuration_changed_concurrently')
    run(['sudo', '-n', 'python3', '-c', _ATOMIC, source, entry['destination'], entry['mode'], entry['uid'], entry['gid']])


def proxy_apply(prepared, phase):
    require(phase in ['maintenance', 'final'], 'invalid_proxy_phase')
    prepared = Path(prepared); info = manifest(prepared)
    source = prepared / 'candidate' / f'nginx.{phase}.conf'
    require(digest_bytes(source.read_bytes()) == info['nginx'][phase], 'candidate_proxy_changed')
    for name in ['nginx.site.conf', 'nginx.generated.conf']:
        entry = info['files'][name]
        install_file(source, entry, [entry['beforeSha256'], *info['nginx'].values()])
    return {'proxyInstalled': phase, 'reloadRequired': True}


def config_apply(prepared, proxy=None):
    prepared = Path(prepared); info = manifest(prepared)
    for name in ['runtime.env', 'server.json', 'compose.env', 'realm.json']:
        entry = info['files'][name]; source = prepared / 'candidate' / name
        require(digest_bytes(source.read_bytes()) == entry['candidateSha256'], 'candidate_configuration_changed')
        install_file(source, entry, [entry['beforeSha256'], entry['candidateSha256']])
    if proxy:
        proxy_apply(prepared, proxy)
    return {'configurationInstalled': True, 'servicesNotRestarted': True}


def config_restore(prepared, restore_proxy=False):
    prepared = Path(prepared); info = manifest(prepared)
    names = ['runtime.env', 'server.json', 'compose.env', 'realm.json']
    if restore_proxy:
        names += ['nginx.site.conf', 'nginx.generated.conf']
    for name in names:
        entry = info['files'][name]; source = prepared / 'backup' / name
        require(digest_bytes(source.read_bytes()) == entry['beforeSha256'], 'backup_configuration_changed')
        allowed = [entry['beforeSha256'], entry.get('candidateSha256'), *info['nginx'].values()]
        install_file(source, entry, allowed)
    return {'configurationRestored': True, 'proxyRestored': restore_proxy, 'reloadRequired': restore_proxy}
