#!/usr/bin/env python3
"""Rehearse deploy.py's database rollback on a disposable PostgreSQL. Restores a live core dump
under the live owner, migrates it with the new binary, writes to a new table, then runs
deploy.restore_database() and compares the result with the backup manifest (migration version,
every table's row count) and with the live database properties (owner, encoding, locale, ACL).
Reads the live cluster only for pg_database properties. Removes only its own labelled container.
Usage: rollback-rehearsal.py BACKUP_DIR NEW_BINARY"""
import importlib.util, json, pathlib, secrets, socket, subprocess, sys, time

HERE = pathlib.Path(__file__).resolve().parent
IMAGE = 'postgres:18.3@sha256:7e32e9833a6fb1c92c32552794cb6ed569d51b445a54907d35fc112ef39684db'
LABEL = 'miy.rollback-rehearsal'
PROPERTIES = ("SELECT json_build_object('owner', pg_get_userbyid(datdba), 'encoding', pg_encoding_to_char(encoding), "
              "'collate', datcollate, 'ctype', datctype, 'provider', datlocprovider, 'acl', datacl::text) "
              "FROM pg_database WHERE datname = 'miy_core'")


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module); return module


deploy = load('deploy', HERE / 'deploy.py')
core_backup = load('core_backup', '/projects/miy-v2/scripts/core-backup.py')


def run(args, cwd=None, env=None, stdin=None, stdout=None):
    p = subprocess.run(args, cwd=cwd, env=env, stdin=stdin, stdout=stdout or subprocess.PIPE,
                       stderr=subprocess.PIPE, text=stdout is None)
    if p.returncode:  # never echo arguments: they can carry the disposable password
        raise RuntimeError(f'command_failed: {args[0]} {args[1] if len(args) > 1 else ""}')
    return (p.stdout or '').strip() if stdout is None else ''


def psql(container, query, db='postgres'):
    return run(['docker', 'exec', container, 'psql', '-X', '-q', '-At', '-U', 'postgres', '-d', db, '-c', query])


def main():
    backup, binary = pathlib.Path(sys.argv[1]).resolve(), pathlib.Path(sys.argv[2]).resolve()
    manifest = json.loads((backup / 'manifest.json').read_text())
    dump = backup / manifest['dump']['file']
    live = json.loads(psql(deploy.DB, PROPERTIES))
    token, password, owner_password = secrets.token_hex(8), secrets.token_hex(16), secrets.token_hex(16)
    with socket.socket() as s:
        s.bind(('127.0.0.1', 0)); port = s.getsockname()[1]
    container = run(['docker', 'run', '-d', '--label', f'{LABEL}={token}', '--tmpfs', '/var/lib/postgresql:rw',
                     '-e', f'POSTGRES_PASSWORD={password}', '-p', f'127.0.0.1:{port}:5432', IMAGE])
    result = {'status': 'FAIL', 'container': container[:12]}
    try:
        for _ in range(60):
            if subprocess.run(['docker', 'exec', container, 'pg_isready', '-U', 'postgres', '-h', '127.0.0.1'],
                              capture_output=True).returncode == 0: break
            time.sleep(1)
        psql(container, f"CREATE ROLE miy_core LOGIN PASSWORD '{owner_password}'")
        psql(container, f'CREATE DATABASE miy_core {deploy.DB_PROPERTIES}')
        psql(container, 'REVOKE ALL ON DATABASE miy_core FROM PUBLIC')
        with dump.open('rb') as handle:
            run(['docker', 'exec', '-i', container, 'pg_restore', '-U', 'postgres', '-d', 'miy_core',
                 '--single-transaction', '--exit-on-error'], stdin=handle, stdout=subprocess.DEVNULL)
        url = f'postgres://miy_core:{owner_password}@127.0.0.1:{port}/miy_core'
        migrate = subprocess.run([str(binary), 'migrate'], env={'DATABASE_URL': url, 'PATH': '/usr/bin:/bin'},
                                 capture_output=True)
        migrated = psql(container, 'SELECT max(version) FROM _sqlx_migrations', 'miy_core')
        psql(container, "INSERT INTO groups(id, name, source) VALUES (gen_random_uuid(), 'rollback probe', 'local')",
             'miy_core')
        kept = deploy.restore_database(run, dump, container=container)
        restored = json.loads(psql(container, core_backup.COUNTS_SQL, 'miy_core'))
        mismatched = sorted(t for t in set(manifest['tables']) | set(restored['tables'])
                            if manifest['tables'].get(t) != restored['tables'].get(t))
        properties = json.loads(psql(container, PROPERTIES))
        result.update({
            'migrateExitCode': migrate.returncode, 'migratedVersion': int(migrated),
            'restoredVersion': restored['migrationVersion'], 'tables': len(restored['tables']),
            'mismatchedTables': mismatched,
            'groupsTableGone': psql(container, "SELECT to_regclass('public.groups') IS NULL", 'miy_core') == 't',
            'keptDatabaseHasProbe': psql(container, "SELECT count(*) FROM groups WHERE name = 'rollback probe'", kept) == '1',
            'propertiesMatchLive': properties == live, 'properties': properties,
        })
        ok = (migrate.returncode == 0 and result['migratedVersion'] == 19 and result['restoredVersion'] == 17
              and not mismatched and result['groupsTableGone'] and result['keptDatabaseHasProbe']
              and result['propertiesMatchLive'])
        result['status'] = 'PASS' if ok else 'FAIL'
    finally:
        labels = subprocess.run(['docker', 'inspect', '-f', f'{{{{ index .Config.Labels "{LABEL}" }}}}', container],
                                capture_output=True, text=True).stdout.strip()
        if labels == token:
            subprocess.run(['docker', 'rm', '-f', '-v', container], capture_output=True)
        result['containerRemoved'] = subprocess.run(['docker', 'inspect', container], capture_output=True).returncode != 0
    print(json.dumps(result))
    return 0 if result['status'] == 'PASS' and result['containerRemoved'] else 1


if __name__ == '__main__':
    sys.exit(main())
