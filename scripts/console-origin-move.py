#!/usr/bin/env python3
"""Move the console (and its Keycloak issuer) to another origin on this host, e.g. a hostname.

The console origin is also the OpenID issuer, and the core reads the issuer's discovery at start,
so this is one short maintenance step rather than additive steps:

  inspect  read-only: validate the target, the live Nginx file, the rendered candidate (only the
           console server block may change), the identity client and the idle core.
  apply    stop core (idle and quiescent) -> Keycloak realm frontendUrl and miy-web console
           callback/logout/web origin replaced (ids, secret and users kept) -> server.json,
           runtime.env (MIY_CONSOLE_ORIGIN, MIY_ISSUER), compose.env, realm.json patched line by
           line -> Nginx: rendered config plus a redirect-only site for the old console origin ->
           start core and verify health, issuer, MCP metadata and the old-origin redirect. Any
           failure restores identity, files and Nginx and starts the core on the old origin.
  restore  the same full restore later, from apply's backups.

Every browser user signs in again once (new cookie host and issuer). CLI credentials are kept.
Run only after an explicit production request; credential values are never printed or recorded.
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import sys
import urllib.parse

ROOT = Path(__file__).resolve().parent.parent
LEGACY_SITE = Path('/etc/nginx/sites-available/v2-console-legacy-redirect')
LEGACY_LINK = Path('/etc/nginx/sites-enabled/v2-console-legacy-redirect')


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


settings_tool = load('dev_portal_settings', 'dev-portal-settings.py')
require, digest, run = settings_tool.require, settings_tool.digest, settings_tool.run
env_patch, env_values, private_write = settings_tool.env_patch, settings_tool.env_values, settings_tool.private_write
FILES = ('server.json', 'runtime.env', 'compose.env', 'realm.json')


def blocks(text):
    """Header, then the server blocks in render order: portal, console, dev portal."""
    parts = text.split('\nserver {')
    return parts[0], ['server {' + part for part in parts[1:]]


def plan(state, console_origin, enabled=None):
    settings = json.loads((state / 'server.json').read_text())
    old = settings['consoleOrigin']
    new = settings_tool.canonical_origin(console_origin)
    portal, dev = settings['origin'], settings.get('devPortalOrigin')
    require(dev, 'dev_portal_required_first')
    require(new != old, 'console_already_there')
    require(new not in (portal, dev) and urllib.parse.urlsplit(new).scheme == urllib.parse.urlsplit(portal).scheme,
            'console_origin_must_be_distinct_with_same_transport')
    host = urllib.parse.urlsplit(new).hostname
    require(host not in {urllib.parse.urlsplit(value).hostname for value in (portal, dev)}
            and settings_tool.name_unclaimed(host, enabled), 'console_hostname_unavailable')
    parsed = urllib.parse.urlsplit(new)
    port = parsed.port or (443 if parsed.scheme == 'https' else 80)
    return {'old': old, 'new': new, 'oldIssuer': old + '/identity/realms/miy', 'issuer': new + '/identity/realms/miy',
            'server': {'consoleOrigin': new},
            'runtime': {'MIY_CONSOLE_ORIGIN': new, 'MIY_ISSUER': new + '/identity/realms/miy'},
            'compose': {'MIY_CONSOLE_ORIGIN': new, 'MIY_CONSOLE_HOST': parsed.netloc, 'MIY_CONSOLE_PORT': str(port)}}


def replace(values, old, new):
    return [new if value == old else value for value in values]


def identity_move(api, original, old, new, frontend):
    """Replace only the console callback/logout/web origin and the realm frontendUrl."""
    current = api.snapshot()
    require(current['client']['id'] == original['client']['id']
            and current['clientSecretSha256'] == original['clientSecretSha256'], 'identity_id_or_secret_changed')
    client = current['client']
    redirects = replace(client.get('redirectUris', []), old + '/auth/callback', new + '/auth/callback')
    origins = replace(client.get('webOrigins', []), old, new)
    attributes = dict(client.get('attributes', {}))
    attributes['post.logout.redirect.uris'] = '##'.join(
        replace(settings_tool.logout_uris(client), old + '/', new + '/'))
    require(new + '/auth/callback' in redirects and new in origins, 'identity_console_entries_missing')
    api.update(client['id'], {'redirectUris': redirects, 'webOrigins': origins, 'attributes': attributes})
    realm_attributes = dict(api.realm().get('attributes', {}))
    realm_attributes['frontendUrl'] = frontend
    api.update_realm({'attributes': realm_attributes})
    after = api.snapshot()
    require(after['client']['id'] == original['client']['id']
            and after['clientSecretSha256'] == original['clientSecretSha256'], 'identity_id_or_secret_not_preserved')
    require(sorted(after['client']['redirectUris']) == sorted(redirects)
            and sorted(after['client']['webOrigins']) == sorted(origins)
            and api.realm().get('attributes', {}).get('frontendUrl') == frontend, 'identity_move_not_verified')


class Identity(settings_tool.Identity):
    def realm(self):
        return self.call('')

    def update_realm(self, body):
        self.call('', 'PUT', body)

    def discovery_issuer(self):
        with self.opener.open(self.base + '/realms/miy/.well-known/openid-configuration', timeout=20) as response:
            return json.load(response)['issuer']


def realm_json_patch(raw, target):
    realm = json.loads(raw)
    realm.setdefault('attributes', {})['frontendUrl'] = target['new'] + '/identity'
    for client in realm.get('clients', []):
        if client.get('clientId') == 'miy-web':
            client['redirectUris'] = replace(client['redirectUris'], target['old'] + '/auth/callback',
                                             target['new'] + '/auth/callback')
            client['webOrigins'] = replace(client['webOrigins'], target['old'], target['new'])
            client['attributes']['post.logout.redirect.uris'] = '##'.join(
                replace(settings_tool.logout_uris(client), target['old'] + '/', target['new'] + '/'))
    return (json.dumps(realm, indent=2) + '\n').encode()


def render(state, settings, certificate, key, sudo):
    import tempfile
    work = Path(tempfile.mkdtemp(prefix='miy-console-render-'))
    try:
        (work / 'server.json').write_text(json.dumps(settings))
        run([*sudo, sys.executable, ROOT / 'scripts/render-server-config.py', '--state-dir', work,
             '--binary', state / 'bin/miy-server', '--certificate', certificate, '--key', key,
             '--user', os.environ.get('USER', 'root')])
        return run([*sudo, 'cat', work / 'nginx-miy.conf']).encode()
    finally:
        run([*sudo, 'rm', '-rf', work])


def legacy_site(old, new, certificate, key):
    parsed = urllib.parse.urlsplit(old)
    port = parsed.port or 443
    return (f'# Old console origin: redirect bookmarks and cached identity URLs to {new}.\n'
            f'server {{\n    listen {port} ssl;\n    server_name "{parsed.hostname}";\n'
            f'    ssl_certificate "{certificate}";\n    ssl_certificate_key "{key}";\n'
            f'    ssl_protocols TLSv1.2 TLSv1.3;\n    return 301 {new}$request_uri;\n}}\n').encode()


def candidate_nginx(state, target, certificate, key, site, sudo):
    live = site.read_bytes()
    require((state / 'nginx-miy.conf').read_bytes() == live, 'live_nginx_differs_from_rendered_copy')
    settings = json.loads((state / 'server.json').read_text())
    _, live_blocks = blocks(live.decode())
    require(len(live_blocks) == 3, 'live_nginx_is_not_three_purposes')
    candidate = render(state, {**settings, **target['server']}, certificate, key, sudo)
    _, new_blocks = blocks(candidate.decode())
    # The portal's CLI approval redirect names the console origin; nothing else may change
    # outside the console block.
    same = [new_blocks[i] == live_blocks[i].replace(target['old'] + '/', target['new'] + '/') for i in (0, 2)]
    require(len(new_blocks) == 3 and all(same) and new_blocks[1] != live_blocks[1],
            'rendered_nginx_changes_more_than_the_console')
    return live, candidate


def inspect(state, console_origin, certificate, key, core, identity=Identity, site=settings_tool.SITE,
            sudo=('sudo', '-n'), enabled=None):
    target = plan(state, console_origin, enabled)
    candidate_nginx(state, target, certificate, key, site, sudo)
    core.require_core_state('active')
    core.idle()
    client = identity(state).snapshot()['client']
    require(target['old'] + '/auth/callback' in client.get('redirectUris', []), 'identity_console_callback_missing')
    return {'status': 'READY', 'from': target['old'], 'to': target['new'], 'issuer': target['issuer']}


def nginx_install(release, data, legacy, site, sudo, legacy_site_path, legacy_link):
    staged = release / 'nginx-candidate.conf'
    staged.write_bytes(data)
    run([*sudo, 'install', '-m', '0644', '-o', 'root', '-g', 'root', staged, site])
    if legacy is None:
        run([*sudo, 'rm', '-f', legacy_link, legacy_site_path])
    else:
        (release / 'nginx-legacy.conf').write_bytes(legacy)
        run([*sudo, 'install', '-m', '0644', '-o', 'root', '-g', 'root', release / 'nginx-legacy.conf', legacy_site_path])
        run([*sudo, 'ln', '-sfn', legacy_site_path, legacy_link])
    run([*sudo, 'nginx', '-t'])
    run([*sudo, 'systemctl', 'reload', 'nginx'])


def full_restore(state, release, core, identity, site, sudo, legacy_site_path, legacy_link):
    """Return identity, files and Nginx to the backups and start the core on the old origin."""
    record = json.loads((release / 'console-move.json').read_text())
    backup = release / 'backup'
    if any(state_ in core.service_states().values() for state_ in ('active', 'activating')):
        core.stop_core()
    original = json.loads((backup / 'identity.json').read_text())
    api = identity(state)
    current = api.snapshot()
    require(current['client']['id'] == original['client']['id']
            and current['clientSecretSha256'] == original['clientSecretSha256'], 'identity_id_or_secret_changed')
    client = original['client']
    api.update(client['id'], {'redirectUris': client.get('redirectUris', []), 'webOrigins': client.get('webOrigins', []),
                              'attributes': client.get('attributes', {})})
    api.update_realm({'attributes': original['realm'].get('attributes', {})})
    for name in FILES:
        private_write(state / name, (backup / name).read_bytes())
    nginx_install(release, (backup / 'nginx.conf').read_bytes(), None, site, sudo, legacy_site_path, legacy_link)
    private_write(state / 'nginx-miy.conf', (backup / 'nginx.conf').read_bytes())
    core.start_core()
    core.wait(lambda: core.get_json(record['old'] + '/api/health') == {'status': 'ok'}, 90, 'restore_health_failed')
    return {'status': 'RESTORED', 'consoleOrigin': record['old']}


def apply(state, release, console_origin, certificate, key, core, identity=Identity, site=settings_tool.SITE,
          sudo=('sudo', '-n'), enabled=None, legacy_site_path=LEGACY_SITE, legacy_link=LEGACY_LINK):
    target = plan(state, console_origin, enabled)
    live, candidate = candidate_nginx(state, target, certificate, key, site, sudo)
    core.require_core_state('active')
    core.idle()
    backup = release / 'backup'
    require(not backup.exists(), 'console_move_backup_exists')
    release.mkdir(mode=0o700, parents=True, exist_ok=True)
    backup.mkdir(mode=0o700)
    api = identity(state)
    original = {**api.snapshot(), 'realm': api.realm()}
    (backup / 'identity.json').write_text(json.dumps(original))
    (backup / 'nginx.conf').write_bytes(live)
    record = {'old': target['old'], 'new': target['new'], 'files': {}}
    written = {}
    for name in FILES:
        raw = (state / name).read_bytes()
        (backup / name).write_bytes(raw)
        if name == 'server.json':
            value = json.loads(raw)
            value.update(target['server'])
            written[name] = (json.dumps(value, indent=2) + '\n').encode()
        elif name == 'realm.json':
            written[name] = realm_json_patch(raw, target)
        else:
            written[name] = env_patch(raw, target['runtime' if name == 'runtime.env' else 'compose'])
            require(env_values(written[name]).keys() >= env_values(raw).keys(), 'environment_patch_lost_keys')
        record['files'][name] = {'before': digest(raw), 'after': digest(written[name])}
    for path in backup.iterdir():
        path.chmod(0o600)
    (release / 'console-move.json').write_text(json.dumps(record, indent=2))
    try:
        core.idle()
        core.stop_core()
        identity_move(api, original, target['old'], target['new'], target['new'] + '/identity')
        require(api.discovery_issuer() == target['issuer'], 'identity_issuer_not_moved')
        for name in FILES:
            private_write(state / name, written[name])
        nginx_install(release, candidate, legacy_site(target['old'], target['new'], certificate, key),
                      site, sudo, legacy_site_path, legacy_link)
        private_write(state / 'nginx-miy.conf', candidate)
        core.start_core()
        core.wait(lambda: core.get_json(target['new'] + '/api/health') == {'status': 'ok'}, 90,
                  'console_health_failed')
        portal = json.loads((backup / 'server.json').read_bytes())['origin']
        require(core.get_json(portal + '/api/health') == {'status': 'ok'}, 'portal_health_failed')
        require(core.get_json(portal + '/auth/options')['consoleOrigin'] == target['new'], 'console_origin_not_served')
        metadata = core.get_json(portal + '/.well-known/oauth-protected-resource/mcp')
        require(metadata['authorization_servers'] == [target['issuer']], 'mcp_issuer_not_moved')
    except (Exception, KeyboardInterrupt) as error:
        failure = str(error) if isinstance(error, RuntimeError) else type(error).__name__
        try:
            full_restore(state, release, core, identity, site, sudo, legacy_site_path, legacy_link)
        except Exception:
            raise RuntimeError('restore_failed_manual_recovery_required:' + failure) from None
        raise RuntimeError('rolled_back:' + failure) from None
    return {'status': 'MOVED', 'from': target['old'], 'to': target['new'], 'issuer': target['issuer'],
            'nginxSha256': digest(candidate)}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('action', choices=['inspect', 'apply', 'restore'])
    parser.add_argument('--state-dir', type=Path, default=Path(os.getenv('MIY_STATE_DIR', str(ROOT / '.local'))))
    parser.add_argument('--console-origin')
    parser.add_argument('--release-dir', type=Path)
    parser.add_argument('--certificate', type=Path)
    parser.add_argument('--key', type=Path)
    args = parser.parse_args(argv)
    state = args.state_dir.resolve()
    core = load('core_rollout', 'core-rollout.py')
    try:
        if args.action in ('inspect', 'apply'):
            require(args.console_origin and args.certificate and args.key, 'missing_console_origin_or_tls')
        if args.action != 'inspect':
            require(args.release_dir is not None, 'missing_release_dir')
            release = args.release_dir.resolve()
            require(release.is_relative_to(state), 'release_dir_must_be_under_state')
        if args.action == 'inspect':
            result = inspect(state, args.console_origin, args.certificate, args.key, core)
        elif args.action == 'apply':
            result = apply(state, release, args.console_origin, args.certificate, args.key, core)
        else:
            result = full_restore(state, release, core, Identity, settings_tool.SITE, ('sudo', '-n'),
                                  LEGACY_SITE, LEGACY_LINK)
    except (RuntimeError, OSError, KeyError, ValueError) as error:
        code = str(error) if isinstance(error, RuntimeError) else type(error).__name__
        print(json.dumps({'status': 'FAIL', 'action': args.action, 'failure': code}))
        return 1
    print(json.dumps(result))
    return 0


if __name__ == '__main__':
    sys.exit(main())
