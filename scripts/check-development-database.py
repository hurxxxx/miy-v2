#!/usr/bin/env python3
"""Run shared development database regression in an owned disposable PostgreSQL.

Only the uniquely labeled test container is removed. Credentials stay in a private
fixture directory and are never printed. Existing platform services are untouched.
"""
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
    parser = argparse.ArgumentParser()
    parser.add_argument('--keep', action='store_true', help='Preserve the isolated fixture for debugging')
    args = parser.parse_args()
    os.umask(0o077)
    fixture = ROOT / '.local' / ('development-regression-' + secrets.token_hex(5))
    fixture.mkdir(parents=True)
    container = 'miy-devdb-test-' + secrets.token_hex(5)
    password = secrets.token_hex(24)
    core_password = secrets.token_hex(24)
    (fixture / 'postgres.env').write_text('POSTGRES_PASSWORD=' + password + '\n')
    metadata = {'container': container, 'fixture': str(fixture)}
    (fixture / 'fixture.json').write_text(json.dumps(metadata, indent=2))
    result = 1
    try:
        subprocess.run(['docker', 'run', '-d', '--name', container,
            '--label', 'org.miy.development-test=' + container,
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
            "CREATE ROLE miy_test_core LOGIN PASSWORD '" + core_password + "' NOSUPERUSER NOCREATEDB NOCREATEROLE;\n"
            "CREATE DATABASE miy_test_core OWNER miy_test_core;\n"
            "REVOKE CONNECT, TEMPORARY ON DATABASE postgres FROM PUBLIC;\n"
            "REVOKE CONNECT, TEMPORARY ON DATABASE template1 FROM PUBLIC;\n"
            "REVOKE CONNECT, TEMPORARY ON DATABASE miy_test_core FROM PUBLIC;\n"
            "GRANT CONNECT ON DATABASE miy_test_core TO miy_test_core;\n"
        )
        subprocess.run(['docker', 'exec', '-i', container, 'psql', '-U', 'postgres', '-v', 'ON_ERROR_STOP=1'],
            input=sql, text=True, check=True, stdout=subprocess.DEVNULL)
        env = dict(os.environ)
        env.pop('MIY_CA_FILE', None)
        env.update({
            'DATABASE_URL': f'postgres://miy_test_core:{core_password}@127.0.0.1:{port}/miy_test_core',
            'PROVISION_DATABASE_URL': f'postgres://postgres:{password}@127.0.0.1:{port}/postgres',
            'MIY_ENCRYPTION_KEY': secrets.token_hex(32),
            'MIY_ARTIFACTS': str(fixture / 'artifacts'),
            'MIY_DEVELOPMENT_AUDIT': str(fixture),
            'MIY_WEB_SECRET': secrets.token_hex(24),
            'MIY_RESOURCE_SECRET': secrets.token_hex(24),
            'MIY_DEV_LOGIN': '0',
            'RUST_LOG': 'off',
        })
        (fixture / 'private-env.json').write_text(json.dumps({key: value for key, value in env.items() if key.startswith('MIY_') or key in ('DATABASE_URL', 'PROVISION_DATABASE_URL')}, indent=2))
        cargo = shutil.which('cargo') or str(Path.home() / '.cargo/bin/cargo')
        command = [cargo, 'test', '--locked', 'development::regression::shared_development_database', '--', '--ignored', '--exact', '--nocapture']
        with (fixture / 'test.log').open('w') as log:
            process = subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        result = process.returncode
        print(json.dumps({'status': 'PASS' if result == 0 else 'FAIL', 'evidence': str(fixture / 'test.log'), 'container': container}))
    finally:
        if not args.keep:
            inspected = subprocess.run(['docker', 'inspect', '--format', '{{ index .Config.Labels "org.miy.development-test" }}', container], text=True, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
            if inspected.returncode == 0 and inspected.stdout.strip() == container:
                subprocess.run(['docker', 'rm', '-f', container], stdout=subprocess.DEVNULL, check=True)
    return result

if __name__ == '__main__':
    raise SystemExit(main())
