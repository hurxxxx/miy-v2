#!/usr/bin/env python3
"""Add business app frame origins (docs/APP-FRAMES.md) to an existing three-origin server.

Frames never sign in, so nothing here touches the identity provider. Steps:

  inspect        read-only: validate the frame origin template, port pool, internal bind and
                 frame runtime directory against server.json, the enabled Nginx sites and free
                 ports. Prints public data only.
  apply          back up and patch server.json, runtime.env and compose.env line by line (other
                 lines byte-identical).
  restore        undo complete or interrupted apply; retry interrupted restore. Refuses
                 files that match neither the original nor the applied digest.
  nginx-apply    render the fixed-purpose Nginx config and require it to differ from the live
                 file only by one app frame server block and the portal frame-src and
                 Referrer-Policy headers; install it with a backup, test and reload Nginx.
  nginx-restore  reinstall the backed-up Nginx file, test and reload.

Order: apply; scripts/core-rollout.py apply --widen-migration 22 (the new server needs these
settings and adds the frame tables); nginx-apply; scripts/ui-rollout.py apply --target
frame-runtime; scripts/ui-rollout.py apply (portal). Roll back in reverse order. Nothing here
restarts the core or touches PostgreSQL. Run only after an explicit production request;
credential values are never printed or recorded.
"""
import argparse
import importlib.util
import ipaddress
import json
import os
from pathlib import Path
import re
import sys
import tempfile
import urllib.parse

ROOT = Path(__file__).resolve().parent.parent
_spec = importlib.util.spec_from_file_location('dev_portal_settings', ROOT / 'scripts/dev-portal-settings.py')
_shared = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_shared)
require, digest, env_values, env_patch, private_write = (
    _shared.require, _shared.digest, _shared.env_values, _shared.env_patch, _shared.private_write)
run, port_free, SITE, ENABLED = _shared.run, _shared.port_free, _shared.SITE, _shared.ENABLED
FILES = ('server.json', 'runtime.env', 'compose.env')
# Ports browsers refuse to load (Fetch "bad port"); Core skips them in a pool too.
BAD_PORTS = {1, 7, 9, 11, 13, 15, 17, 19, 20, 21, 22, 23, 25, 37, 42, 43, 53, 69, 77, 79, 87, 95, 101, 102,
             103, 104, 109, 110, 111, 113, 115, 117, 119, 123, 135, 137, 139, 143, 161, 179, 389, 427, 465,
             512, 513, 514, 515, 526, 530, 531, 532, 540, 548, 554, 556, 563, 587, 601, 636, 989, 990, 993,
             995, 1719, 1720, 1723, 2049, 3659, 4045, 4190, 5060, 5061, 6000, 6566, 6665, 6666, 6667, 6668,
             6669, 6679, 6697, 10080}
LABEL = re.compile(r'[a-z0-9]([a-z0-9-]*)?')


def origin_of(value):
    parsed = urllib.parse.urlsplit(value)
    require(parsed.scheme in ('http', 'https') and parsed.hostname and not parsed.username
            and parsed.password is None and parsed.path == '' and not parsed.query and not parsed.fragment,
            'invalid_app_frame_origin')
    host = '[' + parsed.hostname + ']' if ':' in parsed.hostname else parsed.hostname
    default = 443 if parsed.scheme == 'https' else 80
    canonical = f'{parsed.scheme}://{host}' + (f':{parsed.port}' if parsed.port and parsed.port != default else '')
    require(canonical == value, 'app_frame_origin_not_canonical')
    return parsed


def loopback_bind(value):
    parsed = urllib.parse.urlsplit('//' + value)
    require(parsed.hostname and parsed.port and not parsed.path, 'invalid_app_frame_bind')
    require(ipaddress.ip_address(parsed.hostname).is_loopback, 'app_frame_bind_must_be_loopback')
    return value


def frame_pattern(template, ports, portal):
    """Validate like Core's app_frames::Config and return the sample origin and proxy values."""
    require(template.count('{key}') + template.count('{port}') == 1, 'app_frame_origin_needs_one_key_or_port')
    scheme = template.partition('://')[0]
    require(scheme == urllib.parse.urlsplit(portal).scheme, 'app_frame_origin_must_use_the_portal_transport')
    if '{key}' in template:
        require(ports is None, 'app_frame_ports_need_a_port_template')
        prefix, suffix = template.split('{key}', 1)
        label = prefix.partition('://')[2]
        require('.' not in label and ':' not in label and suffix.startswith('.')
                and LABEL.fullmatch(label + '0') is not None, 'app_frame_key_must_be_in_the_first_host_label')
        sample = template.replace('{key}', '0' * 32)
        origin_of(sample)
        sites = sources = f"{scheme}://*.{template.partition('://')[2].split('.', 1)[1]}"
        return {'sample': sample, 'ports': None, 'sites': sites, 'sources': sources,
                'host': urllib.parse.urlsplit(sample).hostname, 'prefixHost': label}
    require(template.endswith(':{port}'), 'app_frame_port_must_be_the_origin_port')
    first, _, last = (ports or '').partition('-')
    require(first.isdigit() and last.isdigit() and 0 < int(first) <= int(last) <= 65535,
            'invalid_app_frame_port_pool')
    pool = [port for port in range(int(first), int(last) + 1) if port not in BAD_PORTS]
    require(pool, 'invalid_app_frame_port_pool')
    sample = template.replace('{port}', str(pool[0]))
    origin_of(sample)
    return {'sample': sample, 'ports': (int(first), int(last)), 'pool': pool,
            'sites': ', '.join(template.replace('{port}', str(port)) for port in pool),
            'sources': template.replace('{port}', '*'), 'host': urllib.parse.urlsplit(sample).hostname}


def plan(state, template, bind, ports=None, runtime_dir=None):
    settings = json.loads((state / 'server.json').read_text())
    portal, console, dev = settings['origin'], settings.get('consoleOrigin'), settings.get('devPortalOrigin')
    require(console and dev and settings.get('consoleBind') and settings.get('devPortalBind'),
            'three_origin_server_required')
    frame = frame_pattern(template, ports, portal)
    platform = (portal, console, dev)
    require(frame['sample'] not in platform, 'app_frame_origin_must_not_be_a_platform_origin')
    if frame['ports']:
        taken = {urllib.parse.urlsplit(value).port or (443 if value.startswith('https:') else 80)
                 for value in platform if urllib.parse.urlsplit(value).hostname == frame['host']}
        require(not taken & set(range(frame['ports'][0], frame['ports'][1] + 1)),
                'app_frame_port_pool_overlaps_a_platform_port')
    bind = loopback_bind(bind)
    require(bind not in (settings['bind'], settings['consoleBind'], settings['devPortalBind']),
            'app_frame_bind_must_be_distinct')
    runtime = Path(runtime_dir) if runtime_dir else state / 'frame-runtime'
    require(runtime.is_absolute() and '..' not in runtime.parts, 'app_frame_runtime_dir_must_be_absolute')
    existing = settings.get('appFrameOrigin')
    require(existing in (None, template) and settings.get('appFrameBind') in (None, bind)
            and settings.get('appFramePorts') in (None, ports), 'different_app_frames_already_configured')
    server = {'appFrameOrigin': template, 'appFrameBind': bind}
    env = {'MIY_APP_FRAME_ORIGIN': template, 'MIY_APP_FRAME_BIND_ADDR': bind, 'MIY_FRAME_RUNTIME_DIR': str(runtime)}
    if ports is not None:
        server['appFramePorts'] = ports
        env['MIY_APP_FRAME_PORTS'] = ports
    return {
        'portalOrigin': portal, 'appFrameOrigin': template, 'appFramePorts': ports, 'appFrameBind': bind,
        'frameRuntimeDir': str(runtime), 'sampleOrigin': frame['sample'], 'frameSources': frame['sources'],
        'frame': frame, 'alreadyConfigured': existing == template,
        'server': server, 'runtime': env,
        'compose': {'MIY_APP_FRAME_CORE_UPSTREAM': bind, 'MIY_APP_FRAME_SITES': frame['sites'],
                    'MIY_APP_FRAME_SOURCES': frame['sources']},
    }


def names_unclaimed(prefix_host, enabled=None):
    """No enabled Nginx site already names a host (or pattern) with the frame label prefix."""
    prefix = prefix_host.lower()
    return not any(prefix in name for name in _shared.server_names(enabled))


def available(target, probe=None, enabled=None):
    if target['alreadyConfigured']:
        return True
    probe = probe or port_free
    bind_port = urllib.parse.urlsplit('//' + target['appFrameBind']).port
    if not probe(bind_port):
        return False
    frame = target['frame']
    if frame['ports']:
        return all(probe(port) for port in frame['pool'])
    return names_unclaimed(frame['prefixHost'], enabled)


def public(target):
    return {key: target[key] for key in ('portalOrigin', 'appFrameOrigin', 'appFramePorts', 'appFrameBind',
                                         'frameRuntimeDir', 'sampleOrigin', 'frameSources', 'alreadyConfigured')}


def inspect(state, template, bind, ports=None, runtime_dir=None, probe=None, enabled=None):
    target = plan(state, template, bind, ports, runtime_dir)
    require(available(target, probe, enabled), 'app_frame_address_in_use')
    runtime = Path(target['frameRuntimeDir'])
    return {'status': 'READY', **public(target), 'frameRuntimeIndex': (runtime / 'index.html').is_file()}


def directory_sync(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def private_directory(path):
    """Persist every new directory entry before recovery files can be relied on."""
    if not path.is_dir():
        private_directory(path.parent)
        path.mkdir(mode=0o700)
    # An earlier attempt may have created this directory but failed its fsync.
    directory_sync(path.parent)


def settings_write(path, data):
    """Persist both bytes and the atomic rename before changing another live file."""
    private_write(path, data)
    with path.open('rb') as handle:
        os.fsync(handle.fileno())
    directory_sync(path.parent)


def apply(state, release, template, bind, ports=None, runtime_dir=None, probe=None, enabled=None):
    target = plan(state, template, bind, ports, runtime_dir)
    require(available(target, probe, enabled), 'app_frame_address_in_use')
    backup = release / 'settings-backup'
    manifest = release / 'settings.json'
    require(not backup.exists() and not manifest.exists(), 'settings_backup_exists')
    private_directory(backup)
    record = {**public(target), 'phase': 'prepared', 'files': {}}
    pending = {}
    for name in FILES:
        original = (state / name).read_bytes()
        settings_write(backup / name, original)
        if name == 'server.json':
            settings = json.loads(original)
            settings.update(target['server'])
            written = (json.dumps(settings, indent=2) + '\n').encode()
        else:
            written = env_patch(original, target['runtime' if name == 'runtime.env' else 'compose'])
            require(env_values(written).keys() >= env_values(original).keys(), 'environment_patch_lost_keys')
        record['files'][name] = {'before': digest(original), 'after': digest(written)}
        pending[name] = written
    # Recovery must be complete and durable before the first live change. The
    # prepared record also recovers a failed write of the final applied record.
    settings_write(manifest, (json.dumps(record, indent=2) + '\n').encode())
    for name, written in pending.items():
        require(digest((state / name).read_bytes()) == record['files'][name]['before'],
                'settings_changed_during_apply:' + name)
        settings_write(state / name, written)
    record['phase'] = 'applied'
    settings_write(manifest, (json.dumps(record, indent=2) + '\n').encode())
    return {'status': 'APPLIED', **public(target)}


def restore(state, release):
    record = json.loads((release / 'settings.json').read_text())
    backup = release / 'settings-backup'
    require(set(record['files']) == set(FILES), 'settings_manifest_incomplete')
    originals = {}
    for name, hashes in record['files'].items():
        require(digest((state / name).read_bytes()) in (hashes['before'], hashes['after']),
                'settings_changed_since_apply:' + name)
        originals[name] = (backup / name).read_bytes()
        require(digest(originals[name]) == hashes['before'], 'settings_backup_changed:' + name)
    for name, hashes in record['files'].items():
        current = digest((state / name).read_bytes())
        require(current in (hashes['before'], hashes['after']), 'settings_changed_since_apply:' + name)
        # The original bytes can be visible after an interrupted fsync. Persist
        # them again before recording success even when the digest already matches.
        settings_write(state / name, originals[name])
    record['phase'] = 'restored'
    settings_write(release / 'settings.json', (json.dumps(record, indent=2) + '\n').encode())
    return {'status': 'RESTORED', 'appFrameOrigin': record['appFrameOrigin']}


def frame_only_change(live, candidate, sources):
    """The candidate is the live file plus one frame server block and portal frame headers."""
    live_blocks, blocks = live.split('\nserver {'), candidate.split('\nserver {')
    require(len(blocks) == len(live_blocks) + 1, 'rendered_nginx_changes_more_than_app_frames')
    frame = 'server {' + blocks[-1]
    directives = [line.strip() for line in frame.split('\n') if line.strip() and not line.strip().startswith('#')]
    # Frame documents send their own frame-ancestors: the proxy adds no headers here.
    require(any(line.startswith('proxy_pass ') for line in directives) and 'access_log off;' in directives
            and 'error_log /dev/null;' in directives
            and not any(line.startswith('add_header ') for line in directives), 'rendered_frame_block_unexpected')
    added = f'; frame-src {sources} blob:'
    referrer = 'add_header Referrer-Policy strict-origin-when-cross-origin always;'
    head = '\nserver {'.join(blocks[:-1]).split('\n', 1)[1]
    require(head.count(added) >= 1 and head.count(added) == head.count(referrer), 'portal_frame_headers_missing')
    normalized = '\n'.join(line.replace(added, '') for line in head.split('\n') if line.strip() != referrer)
    require(normalized.rstrip() == live.split('\n', 1)[1].rstrip(), 'rendered_nginx_changes_more_than_app_frames')
    return {'frameBlockLines': frame.count('\n'), 'portalHeaderLocations': head.count(added)}


def nginx_apply(state, release, certificate, key, site=SITE, sudo=('sudo', '-n')):
    live = site.read_bytes()
    rendered_copy = state / 'nginx-miy.conf'
    require(rendered_copy.read_bytes() == live, 'live_nginx_differs_from_rendered_copy')
    settings = json.loads((state / 'server.json').read_text())
    require(settings.get('appFrameOrigin') and settings.get('appFrameBind'), 'apply_settings_first')
    sources = frame_pattern(settings['appFrameOrigin'], settings.get('appFramePorts'), settings['origin'])['sources']
    work = Path(tempfile.mkdtemp(prefix='miy-app-frame-render-'))
    try:
        (work / 'server.json').write_text(json.dumps(settings))
        run([*sudo, sys.executable, ROOT / 'scripts/render-server-config.py', '--state-dir', work,
             '--binary', state / 'bin/miy-server', '--certificate', certificate, '--key', key,
             '--user', os.environ.get('USER', 'root')])
        candidate = run([*sudo, 'cat', work / 'nginx-miy.conf']).encode()
    finally:
        run([*sudo, 'rm', '-rf', work])
    change = frame_only_change(live.decode(), candidate.decode(), sources)
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
    return {'status': 'NGINX_APPLIED', 'previousSha256': digest(live), 'nginxSha256': digest(candidate), **change}


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
    parser.add_argument('--app-frame-origin', help='https://v2-app-{key}.example.com or https://host:{port}')
    parser.add_argument('--app-frame-ports', help='first-last pool for a {port} template')
    parser.add_argument('--app-frame-bind', default='127.0.0.1:18184')
    parser.add_argument('--frame-runtime-dir', help='defaults to STATE/frame-runtime')
    parser.add_argument('--release-dir', type=Path)
    parser.add_argument('--certificate', type=Path)
    parser.add_argument('--key', type=Path)
    args = parser.parse_args(argv)
    state = args.state_dir.resolve()
    try:
        if args.action in ('inspect', 'apply'):
            require(args.app_frame_origin is not None, 'missing_app_frame_origin')
        if args.action != 'inspect':
            require(args.release_dir is not None, 'missing_release_dir')
            release = args.release_dir.resolve()
            require(release.is_relative_to(state), 'release_dir_must_be_under_state')
        options = (args.app_frame_origin, args.app_frame_bind, args.app_frame_ports, args.frame_runtime_dir)
        if args.action == 'inspect':
            result = inspect(state, *options)
        elif args.action == 'apply':
            result = apply(state, release, *options)
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
