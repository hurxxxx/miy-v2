#!/usr/bin/env python3
"""Check server-build persistence/API using an owned disposable PostgreSQL only."""
import argparse
import json
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import time

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--keep', action='store_true', help='Keep only this isolated fixture container for debugging')
    args = parser.parse_args()
    os.umask(0o077)
    fixture = ROOT / '.local' / ('server-build-regression-' + secrets.token_hex(6))
    fixture.mkdir(parents=True)
    container = 'miy-build-api-test-' + secrets.token_hex(6)
    password, core_password = secrets.token_hex(24), secrets.token_hex(24)
    (fixture / 'postgres.env').write_text('POSTGRES_PASSWORD=' + password + '\n')
    (fixture / 'fixture.json').write_text(json.dumps({'kind': 'server-build-api', 'container': container, 'database': 'miy_build_audit'}, indent=2))
    result = 1
    try:
        subprocess.run(['docker', 'run', '-d', '--name', container,
                        '--label', 'org.miy.server-build-test=' + container,
                        '--memory', '512m', '--pids-limit', '128',
                        '-p', '127.0.0.1::5432', '--env-file', str(fixture / 'postgres.env'),
                        '--tmpfs', '/var/lib/postgresql:rw,size=768m', 'postgres:18.3'],
                       check=True, stdout=subprocess.DEVNULL)
        for _ in range(120):
            ready = subprocess.run(['docker', 'exec', container, 'pg_isready', '-h', '127.0.0.1', '-U', 'postgres'],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if ready.returncode == 0:
                break
            time.sleep(0.25)
        else:
            raise RuntimeError('Isolated PostgreSQL did not become ready')
        published = subprocess.check_output(['docker', 'port', container, '5432/tcp'], text=True).strip()
        port = int(published.rsplit(':', 1)[1])
        sql = (
            "CREATE ROLE miy_build_audit LOGIN PASSWORD '" + core_password + "' NOSUPERUSER NOCREATEDB NOCREATEROLE;\n"
            "CREATE DATABASE miy_build_audit OWNER miy_build_audit;\n"
            "REVOKE CONNECT, TEMPORARY ON DATABASE postgres FROM PUBLIC;\n"
            "REVOKE CONNECT, TEMPORARY ON DATABASE template1 FROM PUBLIC;\n"
            "REVOKE CONNECT, TEMPORARY ON DATABASE miy_build_audit FROM PUBLIC;\n"
            "GRANT CONNECT ON DATABASE miy_build_audit TO miy_build_audit;\n"
        )
        subprocess.run(['docker', 'exec', '-i', container, 'psql', '-U', 'postgres', '-v', 'ON_ERROR_STOP=1'],
                       input=sql, text=True, check=True, stdout=subprocess.DEVNULL)
        env = dict(os.environ)
        # Never inherit a real database, CLI context, certificate or server setting.
        for key in list(env):
            if key.startswith('MIY_') or key in ('DATABASE_URL', 'PROVISION_DATABASE_URL'):
                env.pop(key)
        env.update({
            'DATABASE_URL': f'postgres://miy_build_audit:{core_password}@127.0.0.1:{port}/miy_build_audit',
            'MIY_ARTIFACTS': str(fixture / 'artifacts'),
            'MIY_SERVER_BUILD_AUDIT': str(fixture),
            'MIY_CONFIG_DIR': str(fixture / 'cli-config'),
            'MIY_ENCRYPTION_KEY': secrets.token_hex(32),
            'MIY_WEB_SECRET': secrets.token_hex(24),
            'MIY_RESOURCE_SECRET': secrets.token_hex(24),
            'MIY_DEV_LOGIN': '0',
            'RUST_LOG': 'off',
        })
        cargo = shutil.which('cargo') or str(Path.home() / '.cargo/bin/cargo')
        command = [cargo, 'test', '--locked', '--bin', 'miy',
                   'server_build_regression::durable_server_build_api', '--', '--ignored', '--exact', '--nocapture']
        with (fixture / 'test.log').open('w') as log:
            process = subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        result = process.returncode
        print(json.dumps({'status': 'PASS' if result == 0 else 'FAIL', 'evidence': str(fixture / 'test.log'),
                          'summary': str(fixture / 'summary.json'), 'container': container}))
    finally:
        if not args.keep:
            inspected = subprocess.run(['docker', 'inspect', '--format', '{{ index .Config.Labels "org.miy.server-build-test" }}', container],
                                       text=True, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
            if inspected.returncode == 0 and inspected.stdout.strip() == container:
                subprocess.run(['docker', 'rm', '-f', container], stdout=subprocess.DEVNULL, check=True)
    return result


if __name__ == '__main__':
    raise SystemExit(main())
