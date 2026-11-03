#!/usr/bin/env python3
"""Deploy the explicitly reviewed current core source; UI publication is separate.

Requires a clean frozen SOURCE worktree, full source commit and candidate/installed SHA256,
explicit current schema 17 or 19, and a successful source/candidate/dump-bound rollback proof.
Before live settings or migration change, stop idle core services, take a fresh private dump,
and restore/migrate it in a separate disposable database. Keep the existing PostgreSQL
container and its app networks; apply connection settings only when they are still old.
Compare stable old-column row hashes/counts and all production installation containers.
Failures before candidate startup may restore the verified dump. Failures after startup
stop core for assessment instead of silently overwriting newly accepted writes.

Usage: deploy.py COMMIT BINARY SHA256 --expected-server-sha256 SHA256 --expected-schema 17 \
--rollback-proof proof.json [--preflight-backup DIR]
Only the main operator executes this script after explicit production authorization.
"""
import contextlib, fcntl, hashlib, inspect, json, os, pathlib, re, shutil, subprocess, sys, tempfile, time, urllib.request, uuid

LIVE = pathlib.Path('/projects/miy-v2')
REL = LIVE / '.local/release-review-20261104/core'
SOURCE = LIVE / '.local/release-review-20261104/source'
BACKUP = REL / 'rollback'
EVIDENCE = pathlib.Path(__file__).resolve().parent
CLI_COMMIT = '01e4d4924f8c8fc4e11720c1af81adf17ddb20bf'
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


def require_core_state(expected):
    for service in SERVICES:
        require(call(['systemctl', 'show', '--property=ActiveState', '--value', service]) == expected,
                'core_not_stopped' if expected == 'inactive' else 'core_not_active')


def stop_core():
    call(['sudo', '-n', 'systemctl', 'stop', *SERVICES])
    require_core_state('inactive')
    REPORT.setdefault('serviceStoppedAt', now())
    REPORT.setdefault('serviceStopEvents', []).append(now())
    save()


def failure_code(error):
    value = str(error)
    return value if isinstance(error, RuntimeError) and re.fullmatch(r'[a-z0-9_]+', value) else type(error).__name__


def quiet(args, **kwargs):
    """Capture only; caller extracts public metadata and never persists raw output."""
    result = subprocess.run(args, capture_output=True, text=True, **kwargs)
    require(result.returncode == 0, 'inspection_failed')
    return result.stdout.strip()


def source_preflight(commit):
    require(re.fullmatch(r'[0-9a-f]{40}', commit) is not None, 'invalid_source_commit')
    require(quiet(['git', '-C', str(SOURCE), 'rev-parse', 'HEAD']) == commit, 'source_commit_mismatch')
    require(quiet(['git', '-C', str(SOURCE), 'status', '--porcelain']) == '', 'source_worktree_dirty')
    require(digest(SOURCE / 'compose.yaml') == digest(LIVE / 'compose.yaml'), 'live_compose_differs_from_reviewed_source')


def backup_module():
    import types
    module = types.ModuleType('reviewed_backup')
    module.__file__ = str(SOURCE / 'scripts/core-backup.py')
    exec(compile(pathlib.Path(module.__file__).read_text(), module.__file__, 'exec'), module.__dict__)
    return module


def idle():
    counts = json.loads(sql(call,
        "SELECT json_build_object("
        "'runtime',(SELECT count(*) FROM runtime_jobs WHERE status IN ('queued','running')),"
        "'builds',(SELECT count(*) FROM app_builds WHERE status IN ('receiving','queued','running')),"
        "'database',(SELECT count(*) FROM jobs WHERE status IN ('queued','running')),"
        "'development',(SELECT count(*) FROM app_dev_sessions WHERE status IN ('queued','preparing','ready')),"
        "'uploads',(SELECT count(*) FROM image_uploads WHERE status IN ('receiving','pending','loading')))"))
    require(all(value == 0 for value in counts.values()), 'active_work_prevents_rollout')
    return counts


def stable_columns():
    tables = ['users', 'apps', 'app_registrations', 'grants', 'installations', 'releases', 'release_checks',
              'db_resources', 'groups', 'group_members', 'group_grants', 'app_admission', 'app_controls']
    result = {}
    for table in tables:
        columns = json.loads(sql(call,
            "SELECT coalesce(json_agg(column_name ORDER BY ordinal_position),'[]') FROM information_schema.columns "
            f"WHERE table_schema='public' AND table_name='{table}'"))
        if table == 'installations':
            columns = [name for name in columns if not name.startswith('health_')]
        if columns:
            require(all(re.fullmatch(r'[a-z_][a-z0-9_]*', name) for name in columns), 'invalid_column_name')
            result[table] = columns
    return result


def stable_records(columns):
    # Hash the old column projection: migration 18's new users.primary_group_id
    # does not invalidate old row identity. No row values leave PostgreSQL.
    result = {}
    for table, names in columns.items():
        projection = ','.join('"' + name + '"' for name in names)
        result[table] = json.loads(sql(call,
            "SELECT json_build_object('rows',count(*),'sha256',encode(sha256(convert_to("
            "coalesce(string_agg(row_data::text,E'\\n' ORDER BY row_data::text),''),'UTF8')),'hex')) "
            f'FROM (SELECT to_jsonb(record) row_data FROM (SELECT {projection} FROM "{table}") record) records'))
    return result


def app_containers(names=None):
    if names is None:
        names = json.loads(sql(call, "SELECT coalesce(json_agg(runtime_container ORDER BY runtime_container),'[]') "
                                   "FROM installations WHERE runtime_container IS NOT NULL"))
    result = {}
    for name in names:
        require(re.fullmatch(r'miy-app-[0-9a-f]{32}', name) is not None, 'invalid_runtime_container_name')
        metadata = json.loads(quiet(['docker', 'inspect', name]))[0]
        require(metadata['Name'] == '/' + name, 'runtime_container_identity_changed')
        result[name] = {'id': metadata['Id'], 'image': metadata['Image'], 'status': metadata['State']['Status'],
                        'startedAt': metadata['State']['StartedAt'], 'restartCount': metadata['RestartCount']}
    return result


def executable_hashes(expected):
    hashes = {}
    for service in SERVICES:
        pid = call(['systemctl', 'show', '--property=MainPID', '--value', service])
        require(pid.isdecimal() and int(pid) > 1, 'service_pid_unavailable')
        try:
            actual = digest(pathlib.Path('/proc') / pid / 'exe')
        except PermissionError:
            actual = quiet(['sudo', '-n', 'sha256sum', f'/proc/{pid}/exe']).split()[0]
        require(actual == expected, 'running_binary_hash_mismatch')
        hashes[service] = {'pid': int(pid), 'sha256': actual}
    return hashes


def verify_rollback_proof(path, preflight_backup, commit, candidate_sha, expected_schema):
    manifest = json.loads((preflight_backup / 'manifest.json').read_text())
    dump = preflight_backup / manifest['dump']['file']
    require(digest(dump) == manifest['dump']['sha256'], 'preflight_dump_hash_mismatch')
    require(manifest['migrationVersion'] == expected_schema, 'preflight_schema_mismatch')
    proof = json.loads(path.read_text())
    # The rehearsal uses ast.get_source_segment, which excludes the final newline.
    restore_sha = hashlib.sha256(inspect.getsource(restore_database).rstrip('\r\n').encode()).hexdigest()
    require(proof.get('status') == 'PASS' and proof.get('containerRemoved') is True
            and proof.get('propertiesMatchLive') is True and proof.get('mismatchedTables') == []
            and proof.get('stableRowDigestsMatch') is True and proof.get('restoreFunctionSha256') == restore_sha
            and proof.get('restoredVersion') == expected_schema and proof.get('migratedVersion') == 19
            and proof.get('migrateExitCode') == 0 and proof.get('sourceCommit') == commit
            and proof.get('candidateSha256') == candidate_sha and proof.get('backupSha256') == digest(dump),
            'rollback_rehearsal_not_verified')
    return {'proofSha256': digest(path), 'backupSha256': digest(dump)}


def capacity_ready():
    deadline = time.monotonic() + 35
    while time.monotonic() < deadline:
        try:
            result = subprocess.run(['python3', str(SOURCE / 'scripts/check-db-connections.py')],
                                    capture_output=True, text=True, timeout=10)
            report = json.loads(result.stdout)
            if result.returncode == 0 and report.get('status') == 'PASS':
                return report
        except (subprocess.TimeoutExpired, ValueError):
            pass
        time.sleep(1)
    raise RuntimeError('connection_budget_not_ready')


def restore_current(migrated, expected_schema):
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
    REPORT['rollbackServicesStartedAt'] = now(); save()
    wait(lambda: get_json(ORIGIN + '/api/health') == {'status': 'ok'}, 90, 'rollback_health_failed')
    wait(lambda: get_json(IDENTITY)['issuer'] == ORIGIN + '/identity/realms/miy', 90, 'rollback_identity_failed')
    require_core_state('active')
    require(migrations() == str(expected_schema), 'rollback_migration_version')
    REPORT['rollbackExecutables'] = executable_hashes(REPORT['previousSha256'])
    REPORT['status'] = 'ROLLED_BACK'; save()


def deploy(args):
    commit, binary, binary_sha = args.commit, args.binary.resolve(), args.sha256
    REPORT.update({'startedAt': now(), 'commit': commit, 'serverSha256': binary_sha, 'cliCommit': CLI_COMMIT,
                   'deployScriptSha256': digest(__file__), 'expectedSchema': args.expected_schema})
    migrated = False
    started_candidate = False
    source_preflight(commit)
    require(re.fullmatch(r'[0-9a-f]{64}', binary_sha) is not None, 'invalid_binary_sha256')
    require(digest(binary) == binary_sha, 'candidate_hash_mismatch')
    require(digest(LIVE / '.local/bin/miy-server') == args.expected_server_sha256, 'installed_binary_changed')
    REPORT['previousSha256'] = args.expected_server_sha256
    REPORT['previousExecutables'] = executable_hashes(args.expected_server_sha256)
    require(migrations() == str(args.expected_schema), 'migration_version_before')
    require_database_properties()
    REPORT['rollbackRehearsal'] = verify_rollback_proof(args.rollback_proof, args.preflight_backup,
                                                      commit, binary_sha, args.expected_schema)
    guide = get_json(ORIGIN + '/api/v1/cli/install')
    require(guide['cliVersion'] == '0.7.0' and guide['sourceCommit'] == CLI_COMMIT, 'install_guide_before')
    installer = guide['windowsInstaller']['sha256']
    diff = json.loads(call(['python3', str(SOURCE / 'docs/releases/evidence/core-0.7.0-7d3c66a-linux/compose-diff.py')]))
    require(set(diff['keycloak']['differs']) <= {'env:KC_DB_POOL_MAX_SIZE'}, 'unexpected_identity_difference')
    require(set(diff['postgres']['differs']) <= {'command', 'networks'}, 'unexpected_database_difference')
    max_connections = int(sql(call, 'SHOW max_connections', 'postgres'))
    pool = container_setting(KEYCLOAK, 'KC_DB_POOL_MAX_SIZE')
    require(max_connections in (100, 200), 'unexpected_connection_limit')
    require(pool in (None, '100', '10'), 'unexpected_identity_pool')
    database_networks, database_id = networks(), container_id(DB)
    REPORT['idleBeforeStop'] = idle()
    REPORT['appContainersBefore'] = app_containers()
    candidate = BACKUP / 'miy-server.candidate'
    shutil.copy2(binary, candidate)
    require(digest(candidate) == binary_sha and call([str(candidate), '--version']) == 'miy 0.7.0', 'candidate_changed')
    binary = candidate
    shutil.copy2(LIVE / '.local/bin/miy-server', BACKUP / 'miy-server')
    require(digest(BACKUP / 'miy-server') == args.expected_server_sha256, 'binary_backup_changed')
    save()
    try:
        stop_core()
        REPORT['idleAfterStop'] = idle()
        columns = stable_columns()
        before_records = stable_records(columns)
        backup_dir = BACKUP / 'core'
        backup_tool = SOURCE / 'scripts/core-backup.py'
        call(['python3', str(backup_tool), 'create', '--db-container', DB, '--database', 'miy_core', '--out', str(backup_dir)])
        manifest = json.loads((backup_dir / 'manifest.json').read_text())
        require(manifest['migrationVersion'] == args.expected_schema, 'backup_migration_version')
        REPORT['databaseBackup'] = {**manifest['dump'], 'migrationVersion': manifest['migrationVersion']}
        REPORT['rowCountsBefore'] = manifest['tables']
        REPORT['stableRecordsBefore'] = before_records
        REPORT['stableColumns'] = columns
        verification = json.loads(call(['python3', str(backup_tool), 'verify', str(backup_dir), '--migrate-with', str(binary)]))
        require(verification.get('status') == 'PASS' and verification.get('containerRemoved') is True
                and verification.get('rehearsal') == {'exitCode': 0, 'migrationVersionAfter': 19}, 'backup_restore_not_verified')
        REPORT['backupVerified'] = True; save()
        if max_connections != 200:
            sql(call, "ALTER SYSTEM SET max_connections = '200'", 'postgres')
            call(['docker', 'restart', '-t', '60', DB])
            wait(lambda: subprocess.run(['docker', 'exec', DB, 'pg_isready', '-U', 'postgres', '-h', '127.0.0.1'],
                                        capture_output=True).returncode == 0, 120, 'database_not_ready')
        require(sql(call, 'SHOW max_connections', 'postgres') == '200', 'max_connections')
        require(networks() == database_networks and container_id(DB) == database_id, 'database_networks_changed')
        REPORT['database'] = {'maxConnections': 200, 'sameContainer': True, 'networksKept': len(database_networks)}
        if pool != '10':
            call(['docker', 'compose', '--project-directory', str(LIVE), '-f', str(SOURCE / 'compose.yaml'),
                  '--env-file', str(LIVE / '.local/compose.env'), 'up', '-d', '--no-deps', '--pull', 'never', 'keycloak'],
                 env={**os.environ, 'MIY_STATE_DIR': str(LIVE / '.local')})
        wait(lambda: get_json(IDENTITY)['issuer'] == ORIGIN + '/identity/realms/miy', 240, 'identity_not_ready')
        require(container_setting(KEYCLOAK, 'KC_DB_POOL_MAX_SIZE') == '10', 'identity_pool')
        if args.expected_schema < 19:
            migrated = True
            call(['bash', str(SOURCE / 'scripts/run-service.sh'), 'migrate'],
                 env={**os.environ, 'MIY_STATE_DIR': str(LIVE / '.local'), 'MIY_BINARY': str(binary)})
        require(migrations() == '19', 'migration_version_after')
        require(stable_records(columns) == before_records, 'migration_changed_stable_records')
        after_migration = json.loads(sql(call, backup_module().COUNTS_SQL))['tables']
        require(all(after_migration.get(table) == count for table, count in manifest['tables'].items()
                    if table != '_sqlx_migrations'), 'migration_changed_row_counts')
        REPORT['rowCountsAfterMigration'] = after_migration
        require(app_containers(REPORT['appContainersBefore']) == REPORT['appContainersBefore'], 'runtime_containers_changed_before_start')
        require(digest(binary) == binary_sha, 'candidate_changed')
        atomic_copy(binary, LIVE / '.local/bin/miy-server')
        require(digest(LIVE / '.local/bin/miy-server') == binary_sha, 'installed_candidate_hash_mismatch')
        source_preflight(commit)
        # Once the new API is reachable, a failed check must not discard new accepted writes.
        started_candidate = True
        call(['sudo', '-n', 'systemctl', 'start', *SERVICES])
        REPORT['servicesStartedAt'] = now(); save()
        def healthy():
            health, after = get_json(ORIGIN + '/api/health'), get_json(ORIGIN + '/api/v1/cli/install')
            return health == {'status': 'ok'} and after['cliVersion'] == '0.7.0' and after['sourceCommit'] == CLI_COMMIT and after['windowsInstaller']['sha256'] == installer
        wait(healthy, 90, 'post_deployment_health_failed')
        require_core_state('active')
        REPORT['runningExecutables'] = executable_hashes(binary_sha)
        REPORT['connectionBudget'] = capacity_ready()
        REPORT['stableRecordsAfter'] = stable_records(columns)
        REPORT['rowCountsAfter'] = json.loads(sql(call, backup_module().COUNTS_SQL))['tables']
        REPORT['appContainersAfter'] = app_containers(REPORT['appContainersBefore'])
        require(REPORT['stableRecordsAfter'] == before_records, 'post_start_stable_records_changed')
        require(REPORT['appContainersAfter'] == REPORT['appContainersBefore'], 'runtime_containers_changed')
        REPORT['status'] = 'PASS'
    except (Exception, KeyboardInterrupt) as error:
        REPORT['failureCategory'] = failure_code(error)
        REPORT['status'] = 'POST_START_FAILED' if started_candidate else 'DEPLOYMENT_FAILED'; save()
        if started_candidate:
            try:
                stop_core()
                REPORT['coreStoppedForAssessment'] = True
            except Exception as stop_error:
                REPORT['coreStoppedForAssessment'] = False
                REPORT['stopFailureCategory'] = failure_code(stop_error)
            save()
            raise RuntimeError('post_start_failed_manual_assessment_required') from None
        try:
            restore_current(migrated, args.expected_schema)
        except (Exception, KeyboardInterrupt) as rollback_error:
            REPORT['status'] = 'ROLLBACK_FAILED'
            REPORT['rollbackFailureCategory'] = failure_code(rollback_error); save()
            raise RuntimeError('rollback_failed_manual_recovery_required') from None
        raise
    finally:
        REPORT['finishedAt'] = now(); save()
    print(json.dumps({'status': REPORT['status'], 'sourceCommit': commit, 'serverSha256': binary_sha,
                      'schema': 19, 'rollbackUsed': REPORT['rollbackUsed']}))


def main():
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('commit')
    parser.add_argument('binary', type=pathlib.Path)
    parser.add_argument('sha256')
    parser.add_argument('--expected-server-sha256', required=True)
    parser.add_argument('--expected-schema', required=True, type=int, choices=[17, 19])
    parser.add_argument('--rollback-proof', required=True, type=pathlib.Path)
    parser.add_argument('--preflight-backup', type=pathlib.Path, default=LIVE / '.local/release-review-20261104/preflight/core')
    args = parser.parse_args()
    require(re.fullmatch(r'[0-9a-f]{64}', args.expected_server_sha256) is not None, 'invalid_expected_server_hash')
    with rollout_lock():
        deploy(args)


if __name__ == '__main__':
    main()
