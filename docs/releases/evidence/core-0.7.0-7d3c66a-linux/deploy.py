#!/usr/bin/env python3
"""Install the verified 0.7.0 server built from 7d3c66a on v2.dwdcc.kr: user/group app permissions,
audit query and data protection (core migrations 18-19, role connection limits, storage floor).
The published CLI 0.7.0 (01e4d49) and its download/installer indexes are not changed.

Order: preconditions → back up the binary → stop the core → dump miy_core and verify its restore
and candidate migrations in a disposable database →
PostgreSQL max_connections=200 through ALTER SYSTEM and a container restart (re-creating the container
with compose would drop the per-installation network attachments the core adds at deployment) →
re-create Keycloak with KC_DB_POOL_MAX_SIZE=10 → migrate 17→19 with the new binary → install the
binary → start the core → health and install guide. On failure the binary and, after migration,
miy_core are restored and the core restarted. The PostgreSQL and Keycloak settings stay: the
previous binary works with both. An existing rollback directory is never reused. This remains
the historical 7d3c66a / schema 17→19 rollout, not an updater for arbitrary new commits.

Usage: deploy.py COMMIT BINARY SHA256"""
import contextlib, fcntl, hashlib, json, os, pathlib, re, shutil, subprocess, sys, tempfile, time, urllib.request, uuid

LIVE = pathlib.Path('/projects/miy-v2')
REL = LIVE / '.local/release-0.7.0-7d3c66a'
BACKUP = REL / 'rollback'
EVIDENCE = pathlib.Path(__file__).resolve().parent
CLI_COMMIT = '01e4d4924f8c8fc4e11720c1af81adf17ddb20bf'
SERVER_COMMIT = '7d3c66afc2ffb83e9c5422c5d8368ef61b018212'
DB = 'miy-v2-validation-postgres-1'
KEYCLOAK = 'miy-v2-validation-keycloak-1'
ORIGIN = 'https://v2.dwdcc.kr'
IDENTITY = 'http://127.0.0.1:18181/identity/realms/miy/.well-known/openid-configuration'
SERVICES = ['miy-v2-serve', 'miy-v2-worker']
# The preflight below must match these properties before any service/DB mutation.
DB_PROPERTIES = "OWNER miy_core TEMPLATE template0 ENCODING 'UTF8' LOCALE_PROVIDER libc LC_COLLATE 'en_US.utf8' LC_CTYPE 'en_US.utf8'"
EXPECTED_DB_PROPERTIES = {
    'owner': 'miy_core', 'encoding': 'UTF8', 'collate': 'en_US.utf8', 'ctype': 'en_US.utf8',
    'provider': 'c', 'publicRevoked': True,
}
REPORT = {'commands': [], 'rollbackUsed': False, 'databaseRestored': False}


def now(): return time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())
def save():
    fd = os.open(REL / 'deployment.json', os.O_CREAT | os.O_TRUNC | os.O_WRONLY | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'w') as handle:
        json.dump(REPORT, handle, indent=2); handle.write('\n')


def require(condition, code):
    if not condition:
        raise RuntimeError(code)


@contextlib.contextmanager
def rollout_lock():
    """Serialize this rollout and refuse reuse of an existing recovery directory."""
    require(not REL.is_symlink(), 'release_directory_is_symlink')
    REL.mkdir(parents=True, exist_ok=True, mode=0o700)
    os.chmod(REL, 0o700)
    fd = os.open(LIVE / '.local/core-rollout.lock', os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        # An interrupted run needs deliberate recovery; never overwrite its dump/binary.
        BACKUP.mkdir(mode=0o700, exist_ok=False)
        yield
    finally:
        os.close(fd)


def call(args, cwd=LIVE, env=None, stdin=None, stdout=None):
    p = subprocess.run(args, cwd=cwd, env=env, stdin=stdin, stdout=stdout or subprocess.PIPE,
                       stderr=subprocess.STDOUT if stdout is None else subprocess.PIPE, text=stdout is None)
    REPORT['commands'].append({'command': [str(a) for a in args], 'exitCode': p.returncode}); save()
    # Persist command/exit metadata only. Migration stderr can contain private rows.
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
    require(re.fullmatch(r'[a-z_][a-z0-9_]{0,35}', db) is not None, 'invalid_database_name')
    kept = f"{db}_failed_{uuid.uuid4().hex[:16]}"
    # Termination alone has a reconnect race. Keep the preserved failed DB closed.
    sql(run, f'ALTER DATABASE {db} ALLOW_CONNECTIONS false', 'postgres', container)
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


def require_database_properties():
    properties = json.loads(sql(call,
        "SELECT json_build_object('owner', pg_get_userbyid(datdba), "
        "'encoding', pg_encoding_to_char(encoding), 'collate', datcollate, 'ctype', datctype, "
        "'provider', datlocprovider, 'publicRevoked', NOT EXISTS ("
        "SELECT 1 FROM aclexplode(coalesce(datacl, acldefault('d', datdba))) WHERE grantee=0)) "
        "FROM pg_database WHERE datname='miy_core'", 'postgres'))
    require(properties == EXPECTED_DB_PROPERTIES, 'unexpected_database_properties')


def container_setting(name, key):
    # The environment holds secrets: read it without the command log and return one key only.
    env = subprocess.run(['docker', 'inspect', '-f', '{{json .Config.Env}}', name], capture_output=True, text=True,
                         check=True).stdout
    return dict(e.split('=', 1) for e in json.loads(env)).get(key)


def restore(migrated):
    REPORT['rollbackUsed'] = True; save()
    stop_core()
    if migrated:
        require(REPORT.get('backupVerified') is True, 'rollback_backup_not_verified')
        dump = BACKUP / 'core/miy_core.dump'
        require(digest(dump) == REPORT['databaseBackup']['sha256'], 'rollback_dump_changed')
        REPORT['failedDatabaseKeptAs'] = restore_database(call, dump)
        REPORT['databaseRestored'] = True
    require(digest(BACKUP / 'miy-server') == REPORT['previousSha256'], 'rollback_binary_changed')
    atomic_copy(BACKUP / 'miy-server', LIVE / '.local/bin/miy-server')
    call(['sudo', '-n', 'systemctl', 'start', *SERVICES])
    wait(lambda: get_json(ORIGIN + '/api/health') == {'status': 'ok'}, 90, 'rollback_health_failed')
    wait(lambda: get_json(IDENTITY)['issuer'] == ORIGIN + '/identity/realms/miy', 90, 'rollback_identity_failed')
    require_core_state('active')
    require(migrations() == '17', 'rollback_migration_version')
    REPORT['status'] = 'ROLLED_BACK'; save()


def require_core_state(expected):
    for service in SERVICES:
        require(call(['systemctl', 'show', '--property=ActiveState', '--value', service]) == expected,
                'core_not_stopped' if expected == 'inactive' else 'core_not_active')


def stop_core():
    call(['sudo', '-n', 'systemctl', 'stop', *SERVICES])
    require_core_state('inactive')


def deploy(commit, binary, binary_sha):
    REPORT.update({'startedAt': now(), 'commit': commit, 'serverSha256': binary_sha, 'cliCommit': CLI_COMMIT})
    migrated = False
    # Preconditions: exact binary, current schema, unchanged CLI publication, expected compose change.
    require(commit == SERVER_COMMIT, 'unexpected_server_commit')
    require(re.fullmatch(r'[0-9a-f]{64}', binary_sha) is not None, 'invalid_binary_sha256')
    require(digest(binary) == binary_sha, 'binary')
    # Freeze the reviewed candidate; a concurrent rebuild cannot change migration/install bytes.
    candidate = BACKUP / 'miy-server.candidate'
    shutil.copy2(binary, candidate)
    require(digest(candidate) == binary_sha and call([str(candidate), '--version']) == 'miy 0.7.0', 'binary')
    binary = candidate
    require(migrations() == '17', 'migration_version_before')
    require_database_properties()
    guide = get_json(ORIGIN + '/api/v1/cli/install')
    require(guide['cliVersion'] == '0.7.0' and guide['sourceCommit'] == CLI_COMMIT, 'install_guide_before')
    installer = guide['windowsInstaller']['sha256']
    diff = json.loads(call(['python3', str(EVIDENCE / 'compose-diff.py')]))
    require(diff['keycloak']['differs'] == ['env:KC_DB_POOL_MAX_SIZE'], 'unexpected_identity_difference')
    require(set(diff['postgres']['differs']) <= {'command', 'networks'}, 'unexpected_database_difference')
    database_networks, database_id = networks(), container_id(DB)
    REPORT['previousSha256'] = digest(LIVE / '.local/bin/miy-server')
    shutil.copy2(LIVE / '.local/bin/miy-server', BACKUP / 'miy-server')
    require(digest(BACKUP / 'miy-server') == REPORT['previousSha256'], 'binary_backup')
    save()
    try:
        stop_core()
        backup_dir = BACKUP / 'core'
        backup_tool = LIVE / 'scripts/core-backup.py'
        call(['python3', str(backup_tool), 'create', '--db-container', DB, '--database', 'miy_core',
              '--out', str(backup_dir)])
        manifest = json.loads((backup_dir / 'manifest.json').read_text())
        REPORT['databaseBackup'] = {**manifest['dump'], 'migrationVersion': manifest['migrationVersion']}
        require(manifest['migrationVersion'] == 17, 'backup_migration_version')
        verification = json.loads(call(['python3', str(backup_tool), 'verify', str(backup_dir),
                                        '--migrate-with', str(binary)]))
        require(verification['status'] == 'PASS' and verification['containerRemoved'] is True
                and verification.get('rehearsal') == {'exitCode': 0, 'migrationVersionAfter': 19},
                'backup_restore_not_verified')
        REPORT['backupVerified'] = True
        save()

        sql(call, "ALTER SYSTEM SET max_connections = '200'", 'postgres')
        call(['docker', 'restart', '-t', '60', DB])
        wait(lambda: subprocess.run(['docker', 'exec', DB, 'pg_isready', '-U', 'postgres', '-h', '127.0.0.1'],
                                    capture_output=True).returncode == 0, 120, 'database_not_ready')
        require(sql(call, 'SHOW max_connections', 'postgres') == '200', 'max_connections')
        require(networks() == database_networks and container_id(DB) == database_id, 'database_networks_changed')
        REPORT['database'] = {'maxConnections': 200, 'networksKept': len(database_networks), 'sameContainer': True}
        save()

        env = {**os.environ, 'MIY_STATE_DIR': str(LIVE / '.local')}
        call(['docker', 'compose', '--env-file', str(LIVE / '.local/compose.env'), 'up', '-d', '--no-deps',
              '--pull', 'never', 'keycloak'], env=env)
        wait(lambda: get_json(IDENTITY)['issuer'] == ORIGIN + '/identity/realms/miy', 240, 'identity_not_ready')
        require(container_setting(KEYCLOAK, 'KC_DB_POOL_MAX_SIZE') == '10', 'identity_pool')
        REPORT['identity'] = {'poolMaxSize': 10, 'discovery': 'ok'}; save()

        migrated = True
        call(['bash', 'scripts/run-service.sh', 'migrate'],
             env={**os.environ, 'MIY_STATE_DIR': str(LIVE / '.local'), 'MIY_BINARY': str(binary)})
        require(migrations() == '19', 'migration_version_after')
        atomic_copy(binary, LIVE / '.local/bin/miy-server')
        call(['sudo', '-n', 'systemctl', 'start', *SERVICES])

        def healthy():
            health, after = get_json(ORIGIN + '/api/health'), get_json(ORIGIN + '/api/v1/cli/install')
            return (health == {'status': 'ok'} and after['cliVersion'] == '0.7.0'
                    and after['sourceCommit'] == CLI_COMMIT and after['windowsInstaller']['sha256'] == installer)
        wait(healthy, 90, 'post_deployment_health_failed')
        require_core_state('active')
        REPORT['health'] = {'status': 'ok', 'cliVersion': '0.7.0', 'cliSourceCommit': CLI_COMMIT}
        REPORT['status'] = 'PASS'
    except (Exception, KeyboardInterrupt):
        REPORT['failureCategory'] = 'deployment_failed'; save()
        try:
            restore(migrated)
        except (Exception, KeyboardInterrupt):
            REPORT['status'] = 'ROLLBACK_FAILED'
            save()
            raise RuntimeError('rollback_failed_manual_recovery_required') from None
        raise
    finally:
        REPORT['finishedAt'] = now(); save()
    print(json.dumps({'status': REPORT['status'], 'serverSha256': binary_sha, 'rollbackUsed': REPORT['rollbackUsed']}))


def main():
    require(len(sys.argv) == 4, 'usage: deploy.py COMMIT BINARY SHA256')
    commit, binary, binary_sha = sys.argv[1], pathlib.Path(sys.argv[2]).resolve(), sys.argv[3]
    with rollout_lock():
        deploy(commit, binary, binary_sha)


if __name__ == '__main__':
    main()
