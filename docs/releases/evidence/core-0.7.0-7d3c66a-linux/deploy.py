#!/usr/bin/env python3
"""Install the verified 0.7.0 server built from 7d3c66a on v2.dwdcc.kr: user/group app permissions,
audit query and data protection (core migrations 18-19, role connection limits, storage floor).
The published CLI 0.7.0 (01e4d49) and its download/installer indexes are not changed.

Order: preconditions → back up the binary → stop the core → dump miy_core (checked readable) →
PostgreSQL max_connections=200 through ALTER SYSTEM and a container restart (re-creating the container
with compose would drop the per-installation network attachments the core adds at deployment) →
re-create Keycloak with KC_DB_POOL_MAX_SIZE=10 → migrate 17→19 with the new binary → install the
binary → start the core → health and install guide. On failure the binary and, after migration,
miy_core are restored and the core restarted. The PostgreSQL and Keycloak settings stay: the
previous binary works with both.

Usage: deploy.py COMMIT BINARY SHA256"""
import hashlib, json, os, pathlib, shutil, subprocess, sys, tempfile, time, urllib.request

LIVE = pathlib.Path('/projects/miy-v2')
REL = LIVE / '.local/release-0.7.0-7d3c66a'
BACKUP = REL / 'rollback'
EVIDENCE = pathlib.Path(__file__).resolve().parent
CLI_COMMIT = '01e4d4924f8c8fc4e11720c1af81adf17ddb20bf'
DB = 'miy-v2-validation-postgres-1'
KEYCLOAK = 'miy-v2-validation-keycloak-1'
ORIGIN = 'https://v2.dwdcc.kr'
IDENTITY = 'http://127.0.0.1:18181/identity/realms/miy/.well-known/openid-configuration'
SERVICES = ['miy-v2-serve', 'miy-v2-worker']
# Live miy_core properties (pg_database, 2026-11-04): owner miy_core, UTF8, en_US.utf8, PUBLIC revoked.
DB_PROPERTIES = "OWNER miy_core TEMPLATE template0 ENCODING 'UTF8' LC_COLLATE 'en_US.utf8' LC_CTYPE 'en_US.utf8'"
REPORT = {'commands': [], 'rollbackUsed': False, 'databaseRestored': False}


def now(): return time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())
def save(): (REL / 'deployment.json').write_text(json.dumps(REPORT, indent=2) + '\n')


def call(args, cwd=LIVE, env=None, stdin=None, stdout=None):
    p = subprocess.run(args, cwd=cwd, env=env, stdin=stdin, stdout=stdout or subprocess.PIPE,
                       stderr=subprocess.STDOUT if stdout is None else subprocess.PIPE, text=stdout is None)
    REPORT['commands'].append({'command': [str(a) for a in args], 'exitCode': p.returncode}); save()
    if stdout is None:
        with (REL / 'deployment-commands.log').open('a') as log: log.write(p.stdout or '')
    if p.returncode: raise RuntimeError('command_failed')
    return (p.stdout or '').strip() if stdout is None else ''


def digest(path): return hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()


def atomic_copy(source, destination):
    fd, name = tempfile.mkstemp(prefix=destination.name + '.deploy-', dir=destination.parent); os.close(fd)
    tmp = pathlib.Path(name)
    try:
        shutil.copy2(source, tmp)
        with tmp.open('rb') as f: os.fsync(f.fileno())
        os.replace(tmp, destination)
    finally: tmp.unlink(missing_ok=True)


def sql(run, query, db='miy_core', container=DB):
    return run(['docker', 'exec', container, 'psql', '-X', '-q', '-At', '-U', 'postgres', '-d', db, '-c', query])


def restore_database(run, dump, container=DB, db='miy_core'):
    """Replace db with the dump. The migrated database is kept under another name; pg_restore --clean
    cannot drop tables that newer migrations reference, so an empty database with the live properties
    receives the dump in one transaction. Returns the name the replaced database was kept under."""
    kept = f"{db}_failed_{time.strftime('%Y%m%dt%H%M%S', time.gmtime())}"
    sql(run, f"SELECT count(pg_terminate_backend(pid)) FROM pg_stat_activity WHERE datname = '{db}'", 'postgres', container)
    sql(run, f'ALTER DATABASE {db} RENAME TO {kept}', 'postgres', container)
    sql(run, f'CREATE DATABASE {db} {DB_PROPERTIES}', 'postgres', container)
    sql(run, f'REVOKE ALL ON DATABASE {db} FROM PUBLIC', 'postgres', container)
    with open(dump, 'rb') as handle:
        run(['docker', 'exec', '-i', container, 'pg_restore', '-U', 'postgres', '-d', db, '--single-transaction',
             '--exit-on-error'], stdin=handle, stdout=subprocess.DEVNULL)
    return kept


def get_json(url):
    with urllib.request.urlopen(url, timeout=5) as response: return json.load(response)


def wait(check, seconds, failure):
    for _ in range(seconds):
        try:
            if check(): return
        except Exception: pass
        time.sleep(1)
    raise RuntimeError(failure)


def migrations(): return sql(call, 'SELECT max(version) FROM _sqlx_migrations')
def networks(): return sorted(json.loads(call(['docker', 'inspect', '-f', '{{json .NetworkSettings.Networks}}', DB])))
def container_id(name): return call(['docker', 'inspect', '-f', '{{.Id}}', name])


def container_setting(name, key):
    # The environment holds secrets: read it without the command log and return one key only.
    env = subprocess.run(['docker', 'inspect', '-f', '{{json .Config.Env}}', name], capture_output=True, text=True,
                         check=True).stdout
    return dict(e.split('=', 1) for e in json.loads(env)).get(key)


def restore(migrated):
    REPORT['rollbackUsed'] = True; save()
    subprocess.run(['sudo', '-n', 'systemctl', 'stop', *SERVICES])
    atomic_copy(BACKUP / 'miy-server', LIVE / '.local/bin/miy-server')
    if migrated:
        REPORT['failedDatabaseKeptAs'] = restore_database(call, BACKUP / 'miy_core.dump')
        REPORT['databaseRestored'] = True
    call(['sudo', '-n', 'systemctl', 'start', *SERVICES])
    REPORT['status'] = 'ROLLED_BACK'; save()


def main():
    commit, binary, binary_sha = sys.argv[1], pathlib.Path(sys.argv[2]), sys.argv[3]
    REPORT.update({'startedAt': now(), 'commit': commit, 'serverSha256': binary_sha, 'cliCommit': CLI_COMMIT})
    migrated = False
    # Preconditions: exact binary, current schema, unchanged CLI publication, expected compose change.
    assert digest(binary) == binary_sha and call([str(binary), '--version']) == 'miy 0.7.0', 'binary'
    assert migrations() == '17', 'migration_version_before'
    guide = get_json(ORIGIN + '/api/v1/cli/install')
    assert guide['cliVersion'] == '0.7.0' and guide['sourceCommit'] == CLI_COMMIT, 'install_guide_before'
    installer = guide['windowsInstaller']['sha256']
    diff = json.loads(call(['python3', str(EVIDENCE / 'compose-diff.py')]))
    assert diff['keycloak']['differs'] == ['env:KC_DB_POOL_MAX_SIZE'], 'unexpected_identity_difference'
    database_networks, database_id = networks(), container_id(DB)
    REPORT['previousSha256'] = digest(LIVE / '.local/bin/miy-server')
    BACKUP.mkdir(parents=True, exist_ok=True)
    shutil.copy2(LIVE / '.local/bin/miy-server', BACKUP / 'miy-server')
    assert digest(BACKUP / 'miy-server') == REPORT['previousSha256'], 'binary_backup'
    save()
    try:
        call(['sudo', '-n', 'systemctl', 'stop', *SERVICES])
        dump = BACKUP / 'miy_core.dump'
        with dump.open('wb') as handle:
            call(['docker', 'exec', DB, 'pg_dump', '-U', 'postgres', '-Fc', 'miy_core'], stdout=handle)
        os.chmod(dump, 0o600)
        with dump.open('rb') as handle:
            listing = subprocess.run(['docker', 'exec', '-i', DB, 'pg_restore', '--list'], stdin=handle,
                                     capture_output=True, text=True)
        assert listing.returncode == 0 and 'TABLE DATA public installations' in listing.stdout, 'backup_unreadable'
        REPORT['databaseBackup'] = {'bytes': dump.stat().st_size, 'sha256': digest(dump), 'migrationVersion': 17}
        save()

        sql(call, "ALTER SYSTEM SET max_connections = '200'", 'postgres')
        call(['docker', 'restart', '-t', '60', DB])
        wait(lambda: subprocess.run(['docker', 'exec', DB, 'pg_isready', '-U', 'postgres', '-h', '127.0.0.1'],
                                    capture_output=True).returncode == 0, 120, 'database_not_ready')
        assert sql(call, 'SHOW max_connections', 'postgres') == '200', 'max_connections'
        assert networks() == database_networks and container_id(DB) == database_id, 'database_networks_changed'
        REPORT['database'] = {'maxConnections': 200, 'networksKept': len(database_networks), 'sameContainer': True}
        save()

        env = {**os.environ, 'MIY_STATE_DIR': str(LIVE / '.local')}
        call(['docker', 'compose', '--env-file', str(LIVE / '.local/compose.env'), 'up', '-d', '--no-deps',
              '--pull', 'never', 'keycloak'], env=env)
        wait(lambda: get_json(IDENTITY)['issuer'] == ORIGIN + '/identity/realms/miy', 240, 'identity_not_ready')
        assert container_setting(KEYCLOAK, 'KC_DB_POOL_MAX_SIZE') == '10', 'identity_pool'
        REPORT['identity'] = {'poolMaxSize': 10, 'discovery': 'ok'}; save()

        migrated = True
        call(['bash', 'scripts/run-service.sh', 'migrate'],
             env={**os.environ, 'MIY_STATE_DIR': str(LIVE / '.local'), 'MIY_BINARY': str(binary)})
        assert migrations() == '19', 'migration_version_after'
        atomic_copy(binary, LIVE / '.local/bin/miy-server')
        call(['sudo', '-n', 'systemctl', 'start', *SERVICES])

        def healthy():
            health, after = get_json(ORIGIN + '/api/health'), get_json(ORIGIN + '/api/v1/cli/install')
            return (health == {'status': 'ok'} and after['cliVersion'] == '0.7.0'
                    and after['sourceCommit'] == CLI_COMMIT and after['windowsInstaller']['sha256'] == installer)
        wait(healthy, 90, 'post_deployment_health_failed')
        call(['systemctl', 'is-active', *SERVICES])
        REPORT['health'] = {'status': 'ok', 'cliVersion': '0.7.0', 'cliSourceCommit': CLI_COMMIT}
        REPORT['status'] = 'PASS'
    except Exception as error:
        REPORT['failureCategory'] = str(error) or type(error).__name__; save()
        restore(migrated)
        raise
    finally:
        REPORT['finishedAt'] = now(); save()
    print(json.dumps({'status': REPORT['status'], 'serverSha256': binary_sha, 'rollbackUsed': REPORT['rollbackUsed']}))


if __name__ == '__main__':
    main()
