#!/usr/bin/env python3
"""Reproduce Docker gateway-induced PostgreSQL disconnects in owned fixtures only."""
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parent.parent


def cleanup_owned_resources(docker, prefix, database, networks):
    failures = []
    resources = [(database,
                  ['inspect', '--format', '{{ index .Config.Labels "org.miy.gateway-test" }}', database],
                  ['rm', '-f', database],
                  {f'Error: No such object: {database}',
                   f'Error: No such container: {database}',
                   f'Error response from daemon: No such container: {database}'})]
    for network in networks:
        resources.append((network,
                          ['network', 'inspect', '--format', '{{ index .Labels "org.miy.gateway-test" }}', network],
                          ['network', 'rm', network],
                          {f'Error: No such network: {network}',
                           f'Error response from daemon: network {network} not found'}))
    for name, inspect_args, remove_args, absent_errors in resources:
        try:
            meta = docker(*inspect_args, checked=False)
            if meta.returncode:
                if meta.stderr.strip() not in absent_errors:
                    failures.append({'resource': name, 'stage': 'inspect', 'error': meta.stderr})
                continue
            if meta.stdout.strip() != prefix:
                failures.append({'resource': name, 'stage': 'ownership', 'error': 'fixture label mismatch'})
                continue
            removed = docker(*remove_args, checked=False)
            if removed.returncode:
                failures.append({'resource': name, 'stage': 'remove', 'error': removed.stderr})
        except Exception as error:
            failures.append({'resource': name, 'stage': 'cleanup', 'error': str(error)})
    return failures


def main():
    os.umask(0o077)
    prefix = 'miy-gateway-test-' + secrets.token_hex(6)
    root = ROOT / '.local' / prefix
    root.mkdir(parents=True)
    installation = str(uuid.uuid4())
    primary = 'zz-' + prefix
    control = 'aa-' + prefix
    fixed = 'miy-runtime-' + installation.replace('-', '')
    database = prefix + '-db'
    networks = [primary, control, fixed]
    label = 'org.miy.gateway-test=' + prefix
    config = {'prefix': prefix, 'database': database, 'primaryNetwork': primary,
              'controlNetwork': control, 'installation': installation}
    (root / 'fixture.json').write_text(json.dumps(config, indent=2))
    password = secrets.token_hex(24)
    (root / 'postgres.env').write_text('POSTGRES_PASSWORD=' + password + '\n')
    status = 1
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        port = listener.getsockname()[1]

    def docker(*args, checked=True):
        result = subprocess.run(['docker', *args], capture_output=True, text=True)
        if checked and result.returncode:
            (root / 'docker-error.log').write_text(result.stderr)
            raise RuntimeError('Owned fixture Docker command failed; inspect private evidence')
        return result

    try:
        for network in networks:
            labels = ['--label', label]
            if network == fixed:
                labels += ['--label', 'org.miy.installation=' + installation]
            docker('network', 'create', *labels, network)
        docker('run', '-d', '--name', database, '--label', label, '--network', primary,
               '--env-file', str(root / 'postgres.env'), '--memory', '256m', '--pids-limit', '128',
               '-p', f'127.0.0.1:{port}:5432', '--tmpfs', '/var/lib/postgresql:rw,size=384m', 'postgres:18.3')
        for _ in range(120):
            if docker('exec', database, 'pg_isready', '-h', '127.0.0.1', '-U', 'postgres', checked=False).returncode == 0:
                break
            time.sleep(.25)
        else:
            raise RuntimeError('Owned PostgreSQL fixture did not become ready')
        assert int(docker('port', database, '5432/tcp').stdout.strip().rsplit(':', 1)[1]) == port
        env = {key: value for key, value in os.environ.items() if not key.startswith('MIY_')}
        env.update({'DATABASE_URL': f'postgres://postgres:{password}@127.0.0.1:{port}/postgres',
                    'MIY_DATABASE_CONTAINER': database, 'MIY_GATEWAY_TEST_ROOT': str(root), 'RUST_LOG': 'off'})
        (root / 'private-env.json').write_text(json.dumps({key: env[key] for key in
            ['DATABASE_URL', 'MIY_DATABASE_CONTAINER', 'MIY_GATEWAY_TEST_ROOT']}, indent=2))
        cargo = shutil.which('cargo') or str(Path.home() / '.cargo/bin/cargo')
        with (root / 'test.log').open('w') as log:
            status = subprocess.run([cargo, 'test', '--locked',
                'runtime::network_regression::database_attachment_retains_published_connections',
                '--', '--ignored', '--exact', '--nocapture'], cwd=ROOT, env=env,
                stdout=log, stderr=subprocess.STDOUT, timeout=600).returncode
        (root / 'identity.json').write_text(json.dumps({
            'sourceCommit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
            'workingTreeDirty': bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT)),
            'runtimeSourceSha256': hashlib.sha256((ROOT / 'crates/miy/src/runtime.rs').read_bytes()).hexdigest(),
        }, indent=2))
    finally:
        failures = cleanup_owned_resources(docker, prefix, database, networks)
        if failures:
            status = 1
            (root / 'cleanup-failures.json').write_text(json.dumps(failures, indent=2))
    print(json.dumps({'status': 'PASS' if status == 0 else 'FAIL', 'evidence': str(root / 'test.log')}))
    return status


if __name__ == '__main__':
    raise SystemExit(main())
