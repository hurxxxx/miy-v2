#!/usr/bin/env python3
"""Maintained core rollout for this host (v2.dwdcc.kr): replace the server binary, same schema.

Run the copy inside the frozen source checkout of the commit being deployed; any other copy is
refused. The candidate's source must carry exactly the live core migration set. This tool never
changes PostgreSQL or Keycloak settings and never touches UI assets (see scripts/ui-rollout.py).
A schema or settings rollout needs a reviewed extension with a real-dump rollback rehearsal.

  inspect   (default) the read-only preflight of apply; prints public metadata only.
  apply     preflight -> stop core -> fresh dump, restored and migrate-checked in a disposable
            PostgreSQL -> install -> start -> verify. A failure before the candidate starts
            reinstalls the previous binary and starts core again. A failure after it starts stops
            core for assessment instead of overwriting writes it may have accepted.
  rollback  operator recovery after assessment: reinstall the previous binary kept by this
            release directory's apply, then start and verify core.

Every apply attempt needs a new --release-dir under .local; it keeps the 0700 recovery data and
the 0600 deployment record. apply/rollback run only after an explicit production request.

--widen-migration N allows exactly one pinned, constraint-widening migration (see WIDEN_ONLY): the
backup is still restored and migrate-checked first, the candidate migrates the live database after
core stops, and a later rollback reinstalls the previous binary on the widened schema, which that
binary keeps using unchanged. No other migration, setting or identity change is ever applied.

The preflight requires the exact source commit to have passed every required main-push GitHub
workflow (web, cli-foundation, runtime-regression), every job included. A local PASS or a run of
another commit never substitutes for it.
"""
import argparse
import contextlib
import fcntl
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import time
import tomllib
import urllib.request

LIVE = pathlib.Path('/projects/miy-v2')
DB = 'miy-v2-validation-postgres-1'
KEYCLOAK = 'miy-v2-validation-keycloak-1'
ORIGIN = 'https://v2.dwdcc.kr'
SERVICES = ['miy-v2-serve', 'miy-v2-worker']
# These states require an independent process check: failed/inactive can retain cgroup processes.
STOPPED_STATES = ('inactive', 'failed')
CGROUP_ROOT = pathlib.Path('/sys/fs/cgroup')
PROCESS_PROPERTIES = ('Id', 'LoadState', 'ActiveState', 'Slice', 'ControlGroup', 'MainPID', 'ControlPID')
# Old-column projections of these tables must survive the rollout unchanged.
STABLE_TABLES = ['users', 'apps', 'app_registrations', 'grants', 'installations', 'releases', 'release_checks',
                 'db_resources', 'groups', 'group_members', 'group_grants', 'app_admission', 'app_controls']
REPOSITORY = 'hurxxxx/miy-v2'
# Migrations an older binary keeps working with because they only widen constraints:
# version -> (file, sha256 of its exact bytes).
WIDEN_ONLY = {21: ('0021_dev_portal.sql', 'f2b2c022638d618bb337ca1ab53d6ea2f2e0cd7b2cce474ed8659c2f7060b82a')}
REQUIRED_CI = ('cli-foundation.yml', 'runtime-regression.yml', 'web.yml')
HEX40 = re.compile(r'[0-9a-f]{40}')
HEX64 = re.compile(r'[0-9a-f]{64}')
REL = None  # release directory of this apply/rollback
REPORT_FILE = None  # private record; inspect keeps its report in memory only
REPORT = {'commands': []}


def now():
    return time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())


def require(condition, code):
    # Not assert: safety checks must survive python -O.
    if not condition:
        raise RuntimeError(code)


def save():
    if REPORT_FILE is None:
        return
    fd = os.open(REPORT_FILE, os.O_CREAT | os.O_TRUNC | os.O_WRONLY | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'w') as handle:
        json.dump(REPORT, handle, indent=2)
        handle.write('\n')


def plain(path):
    """Absolute path without symlinked components (normalized, not resolved)."""
    path = pathlib.Path(os.path.abspath(path))
    for part in (path, *path.parents):
        require(not part.is_symlink(), 'symlink_path_refused')
    return path


def recovery():
    return REL / 'rollback'


def installed():
    return LIVE / '.local/bin/miy-server'


@contextlib.contextmanager
def rollout_lock(fresh):
    """Serialize core rollouts; apply never reuses a recovery directory."""
    require(REL is not None and LIVE / '.local' in REL.parents, 'release_directory_outside_state')
    if fresh:
        REL.mkdir(parents=True, exist_ok=True, mode=0o700)
    require(REL.is_dir() and not REL.is_symlink(), 'release_directory_missing')
    os.chmod(REL, 0o700)
    fd = os.open(LIVE / '.local/core-rollout.lock', os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if fresh:
            # An interrupted run needs deliberate recovery; never overwrite its dump or binary.
            recovery().mkdir(mode=0o700, exist_ok=False)
        else:
            require(recovery().is_dir() and not recovery().is_symlink(), 'recovery_directory_missing')
        yield
    finally:
        os.close(fd)


def call(args):
    result = subprocess.run([str(a) for a in args], cwd=LIVE, capture_output=True, text=True)
    REPORT['commands'].append({'command': [str(a) for a in args], 'exitCode': result.returncode})
    save()
    # Keep command metadata only: output can contain private rows or settings.
    require(result.returncode == 0, 'command_failed')
    return result.stdout.strip()


def quiet(args):
    """Capture without recording; callers keep only public facts from the output."""
    result = subprocess.run([str(a) for a in args], capture_output=True, text=True)
    require(result.returncode == 0, 'inspection_failed')
    return result.stdout.strip()


def digest(path):
    with open(path, 'rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()


def atomic_copy(source, destination):
    fd, name = tempfile.mkstemp(prefix=destination.name + '.rollout-', dir=destination.parent)
    os.close(fd)
    temporary = pathlib.Path(name)
    try:
        shutil.copy2(source, temporary)
        with temporary.open('rb') as handle:
            os.fsync(handle.fileno())
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)


def sql(query, db='miy_core'):
    return call(['docker', 'exec', DB, 'psql', '-X', '-q', '-At', '-U', 'postgres', '-d', db, '-c', query])


def get_json(url):
    with urllib.request.urlopen(url, timeout=5) as response:
        return json.load(response)


def wait(check, seconds, failure):
    for _ in range(seconds):
        try:
            if check():
                return
        except Exception:
            pass
        time.sleep(1)
    raise RuntimeError(failure)


def migrations():
    return int(sql('SELECT max(version) FROM _sqlx_migrations'))


def networks():
    return sorted(json.loads(quiet(['docker', 'inspect', '-f', '{{json .NetworkSettings.Networks}}', DB])))


def container_id(name):
    return quiet(['docker', 'inspect', '-f', '{{.Id}}', name])


def container_setting(name, key):
    # The environment holds secrets: read it without the command log and return one key only.
    env = json.loads(quiet(['docker', 'inspect', '-f', '{{json .Config.Env}}', name]))
    return dict(item.split('=', 1) for item in env).get(key)


def service_states():
    return {service: call(['systemctl', 'show', '--property=ActiveState', '--value', service]) for service in SERVICES}


def require_core_state(expected):
    # Check every service: an active serve must not hide a failed worker.
    for state in service_states().values():
        require(state == expected, 'core_not_stopped' if expected == 'inactive' else 'core_not_active')


def core_units():
    """Bind empty ControlGroup values to this host's known unit paths, never to the cgroup root."""
    units = {}
    for service in SERVICES:
        raw = call(['systemctl', 'show', '--property=' + ','.join(PROCESS_PROPERTIES), service])
        fields = {}
        for line in raw.splitlines():
            key, separator, value = line.partition('=')
            require(separator and key in PROCESS_PROPERTIES and key not in fields, 'core_process_state_unknown')
            fields[key] = value
        require(set(fields) == set(PROCESS_PROPERTIES), 'core_process_state_unknown')
        expected = '/system.slice/' + service + '.service'
        require(fields['Id'] == service + '.service' and fields['LoadState'] == 'loaded'
                and fields['Slice'] == 'system.slice' and fields['ControlGroup'] in ('', expected),
                'core_cgroup_layout_unsupported')
        require(all(re.fullmatch(r'[0-9]+', fields[key]) for key in ('MainPID', 'ControlPID')),
                'core_process_state_unknown')
        units[service] = fields
    return units


def cgroup_populated(directory):
    # cgroup.events includes descendants, unlike the parent cgroup.procs alone.
    fields = {}
    for line in plain(directory / 'cgroup.events').read_text().splitlines():
        parts = line.split()
        require(len(parts) == 2 and parts[0] not in fields, 'core_process_state_unknown')
        fields[parts[0]] = parts[1]
    require(fields.get('populated') in ('0', '1'), 'core_process_state_unknown')
    return fields['populated'] == '1'


def core_cgroup_population():
    """Read descendants as well as main processes; missing/unknown evidence is not empty."""
    population = {}
    try:
        plain(CGROUP_ROOT / 'cgroup.controllers').read_text()
        parent = plain(CGROUP_ROOT / 'system.slice')
        cgroup_populated(parent)
        for service in SERVICES:
            directory = plain(parent / (service + '.service'))
            try:
                directory.lstat()
            except FileNotFoundError:
                # Do not mistake a missing/unreadable parent for a removed service cgroup.
                cgroup_populated(parent)
                population[service] = False
            else:
                population[service] = cgroup_populated(directory)
    except (OSError, UnicodeError):
        raise RuntimeError('core_process_state_unknown') from None
    return population


def require_core_quiescent():
    """Require stopped units AND empty canonical cgroups, even if ControlGroup was cleared.

    This host uses system.slice on cgroup v2. A vanished service directory is empty only when
    the v2 root and parent slice are readable. Recheck after reset-failed: it never kills a PID.
    """
    units = core_units()
    for fields in units.values():
        require(fields['ActiveState'] in STOPPED_STATES, 'core_not_stopped')
        require(int(fields['MainPID']) == 0 and int(fields['ControlPID']) == 0,
                'core_processes_remaining')
    require(not any(core_cgroup_population().values()), 'core_processes_remaining')
    # Recheck properties after filesystem inspection, before treating the stop as complete.
    require(core_units() == units, 'core_process_state_changed')
    return units


def stop_core():
    core_units()  # Refuse unknown unit identities/layouts before issuing stop.
    core_cgroup_population()
    call(['sudo', '-n', 'systemctl', 'stop', *SERVICES])
    units = require_core_quiescent()
    # Reset failure/start-limit state only after proving no processes remain, then check again.
    failed = [service for service, fields in units.items() if fields['ActiveState'] == 'failed']
    if failed:
        call(['sudo', '-n', 'systemctl', 'reset-failed', *failed])
    require_core_quiescent()
    require_core_state('inactive')
    REPORT.setdefault('serviceStopEvents', []).append(now())
    save()


def start_core():
    call(['sudo', '-n', 'systemctl', 'start', *SERVICES])
    REPORT.setdefault('serviceStartEvents', []).append(now())
    save()


def failure_code(error):
    value = str(error)
    return value if isinstance(error, RuntimeError) and re.fullmatch(r'[a-z0-9_]+', value) else type(error).__name__


def source_preflight(source, commit):
    require(quiet(['git', '-C', source, 'rev-parse', 'HEAD']) == commit, 'source_commit_mismatch')
    require(quiet(['git', '-C', source, 'status', '--porcelain']) == '', 'source_worktree_dirty')


def ci_gate(commit):
    """The latest run of every required workflow for this exact commit on main passed. Read-only."""
    runs = json.loads(quiet(['gh', 'api', f'repos/{REPOSITORY}/actions/runs?head_sha={commit}'
                                          '&event=push&per_page=100']))['workflow_runs']
    selected = {}
    for run in runs:  # newest first: a later failed attempt is not hidden by an older success
        name = run['path'].rsplit('/', 1)[-1]
        if name not in REQUIRED_CI or name in selected:
            continue
        require(run['head_sha'] == commit and run['head_branch'] == 'main' and run['event'] == 'push',
                'ci_source_mismatch')
        require(run['status'] == 'completed' and run['conclusion'] == 'success', 'ci_not_successful')
        jobs = json.loads(quiet(['gh', 'run', 'view', str(run['id']), '--repo', REPOSITORY,
                                 '--json', 'jobs']))['jobs']
        require(bool(jobs) and all(job['status'] == 'completed' and job['conclusion'] == 'success'
                                   for job in jobs), 'ci_jobs_not_successful')
        selected[name] = {'runId': run['id'], 'attempt': run['run_attempt'], 'url': run['html_url'],
                          'headSha': run['head_sha'], 'conclusion': run['conclusion'], 'jobs': len(jobs)}
    require(set(selected) == set(REQUIRED_CI), 'required_ci_missing')
    return selected


def source_facts(source):
    version = tomllib.loads((source / 'crates/miy/Cargo.toml').read_text())['package']['version']
    numbers = [int(path.name[:4]) for path in (source / 'migrations/core').iterdir()
               if re.fullmatch(r'[0-9]{4}_[a-z0-9_]+\.sql', path.name)]
    require(bool(numbers), 'source_migrations_missing')
    return {'version': version, 'schema': max(numbers)}


def idle():
    counts = json.loads(sql(
        "SELECT json_build_object("
        "'runtime',(SELECT count(*) FROM runtime_jobs WHERE status IN ('queued','running')),"
        "'builds',(SELECT count(*) FROM app_builds WHERE status IN ('receiving','queued','running')),"
        "'database',(SELECT count(*) FROM jobs WHERE status IN ('queued','running')),"
        "'development',(SELECT count(*) FROM app_dev_sessions WHERE status IN ('queued','preparing','ready')),"
        "'uploads',(SELECT count(*) FROM image_uploads WHERE status IN ('receiving','pending','loading')))"))
    require(all(value == 0 for value in counts.values()), 'active_work_prevents_rollout')
    return counts


def stable_columns():
    result = {}
    for table in STABLE_TABLES:
        columns = json.loads(sql(
            "SELECT coalesce(json_agg(column_name ORDER BY ordinal_position),'[]') FROM information_schema.columns "
            f"WHERE table_schema='public' AND table_name='{table}'"))
        if table == 'installations':
            # Health observations change on their own schedule.
            columns = [name for name in columns if not name.startswith('health_')]
        if columns:
            require(all(re.fullmatch(r'[a-z_][a-z0-9_]*', name) for name in columns), 'invalid_column_name')
            result[table] = columns
    return result


def stable_records(columns):
    # Row counts and one hash per table; no row values leave PostgreSQL.
    result = {}
    for table, names in columns.items():
        projection = ','.join('"' + name + '"' for name in names)
        result[table] = json.loads(sql(
            "SELECT json_build_object('rows',count(*),'sha256',encode(sha256(convert_to("
            "coalesce(string_agg(row_data::text,E'\\n' ORDER BY row_data::text),''),'UTF8')),'hex')) "
            f'FROM (SELECT to_jsonb(record) row_data FROM (SELECT {projection} FROM "{table}") record) records'))
    return result


def app_containers(names=None):
    if names is None:
        names = json.loads(sql("SELECT coalesce(json_agg(runtime_container ORDER BY runtime_container),'[]') "
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
    """The bytes systemd actually runs, not only the installed file."""
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


def cli_guide():
    guide = get_json(ORIGIN + '/api/v1/cli/install')
    return {'cliVersion': guide['cliVersion'], 'sourceCommit': guide['sourceCommit'],
            'windowsInstallerSha256': guide['windowsInstaller']['sha256']}


def healthy(guide):
    return get_json(ORIGIN + '/api/health') == {'status': 'ok'} and cli_guide() == guide


def connection_budget(source, seconds):
    """At least one attempt; PASS within the deadline or the rollout stops."""
    deadline = time.monotonic() + seconds
    while True:
        try:
            result = subprocess.run(['python3', str(source / 'scripts/check-db-connections.py')],
                                    capture_output=True, text=True, timeout=10)
            report = json.loads(result.stdout)
            if result.returncode == 0 and report.get('status') == 'PASS':
                return {key: report.get(key) for key in ('status', 'worstCase', 'usableConnections')}
        except (subprocess.TimeoutExpired, ValueError):
            pass
        if time.monotonic() >= deadline:
            raise RuntimeError('connection_budget_not_ready')
        time.sleep(1)


def preflight(args):
    """Every check apply needs before it may stop anything. Read-only."""
    for name in ('source', 'commit', 'candidate', 'candidate_sha256', 'installed_sha256'):
        require(getattr(args, name) is not None, 'missing_' + name)
    require(HEX40.fullmatch(args.commit) is not None, 'invalid_source_commit')
    require(HEX64.fullmatch(args.candidate_sha256) is not None, 'invalid_candidate_sha256')
    require(HEX64.fullmatch(args.installed_sha256) is not None, 'invalid_installed_sha256')
    source = plain(args.source)
    require(source != plain(LIVE), 'source_must_be_separate_checkout')
    source_preflight(source, args.commit)
    require(digest(pathlib.Path(__file__).resolve()) == digest(source / 'scripts/core-rollout.py'),
            'rollout_tool_differs_from_source')
    facts = source_facts(source)
    facts['ci'] = ci_gate(args.commit)
    require(digest(args.candidate) == args.candidate_sha256, 'candidate_hash_mismatch')
    require(call([plain(args.candidate), '--version']) == f"miy {facts['version']}", 'candidate_version_mismatch')
    require(digest(installed()) == args.installed_sha256, 'installed_binary_changed')
    require_core_state('active')
    core_units()
    core_cgroup_population()
    facts['executables'] = executable_hashes(args.installed_sha256)
    live_schema = migrations()
    facts['liveSchema'] = live_schema
    if args.widen_migration is None:
        require(live_schema == facts['schema'],
                'migration_rollout_not_supported' if facts['schema'] > live_schema else 'source_schema_older_than_live')
    else:
        pinned = WIDEN_ONLY.get(args.widen_migration)
        require(pinned is not None and facts['schema'] == args.widen_migration == live_schema + 1,
                'widen_migration_not_allowed')
        migration = source / 'migrations/core' / pinned[0]
        require(migration.is_file() and digest(migration) == pinned[1], 'widen_migration_changed')
    require(int(sql('SHOW max_connections', 'postgres')) == args.max_connections, 'database_settings_differ')
    require(container_setting(KEYCLOAK, 'KC_DB_POOL_MAX_SIZE') == str(args.identity_pool), 'identity_settings_differ')
    require(get_json(ORIGIN + '/api/health') == {'status': 'ok'}, 'health_failed_before_rollout')
    facts['cliGuide'] = cli_guide()
    # The rollout does not change the budget; never start a candidate into a failing one.
    facts['connectionBudget'] = connection_budget(source, 0)
    facts['idle'] = idle()
    facts['appContainers'] = app_containers()
    facts['databaseContainer'] = container_id(DB)
    facts['databaseNetworks'] = networks()
    return facts


def public(facts):
    containers = facts['appContainers'].values()
    return {'version': facts['version'], 'schema': facts['schema'], 'executables': facts['executables'],
            'cliGuide': facts['cliGuide'], 'connectionBudget': facts['connectionBudget'], 'idle': facts['idle'],
            'ci': facts['ci'], 'appContainers': len(facts['appContainers']),
            'runningAppContainers': sum(c['status'] == 'running' for c in containers),
            'databaseNetworks': len(facts['databaseNetworks'])}


def restore_previous(schema):
    """Before the candidate started: reinstall the verified previous binary and start core."""
    REPORT['rollbackUsed'] = True
    save()
    stop_core()
    previous = REPORT['previousSha256']
    require(digest(recovery() / 'miy-server') == previous, 'rollback_binary_changed')
    if digest(installed()) != previous:
        atomic_copy(recovery() / 'miy-server', installed())
    require(digest(installed()) == previous, 'rollback_install_mismatch')
    start_core()
    wait(lambda: get_json(ORIGIN + '/api/health') == {'status': 'ok'}, 90, 'rollback_health_failed')
    require_core_state('active')
    require(migrations() == schema, 'rollback_migration_version')
    REPORT['rollbackExecutables'] = executable_hashes(previous)
    REPORT['status'] = 'ROLLED_BACK'
    save()


def apply(args):
    REPORT.update({'action': 'apply', 'tool': 'scripts/core-rollout.py', 'toolSha256': digest(__file__),
                   'startedAt': now(), 'sourceCommit': args.commit, 'serverSha256': args.candidate_sha256,
                   'previousSha256': args.installed_sha256, 'rollbackUsed': False, 'status': 'PREFLIGHT'})
    save()
    source = plain(args.source)
    candidate = recovery() / 'miy-server.candidate'
    try:
        facts = preflight(args)
        schema, sha = facts['schema'], args.candidate_sha256
        before_containers = facts['appContainers']
        REPORT.update({'version': facts['version'], 'schema': schema, 'liveSchema': facts['liveSchema'],
                       'widenMigration': args.widen_migration, 'previousExecutables': facts['executables'],
                       'cliGuide': facts['cliGuide'], 'connectionBudgetBefore': facts['connectionBudget'],
                       'idleBeforeStop': facts['idle'], 'appContainersBefore': before_containers,
                       'databaseNetworks': len(facts['databaseNetworks'])})
        # Verification, the migrate check and installation all use these same copied bytes.
        shutil.copy2(args.candidate, candidate)
        require(digest(candidate) == sha and call([candidate, '--version']) == f"miy {facts['version']}",
                'candidate_changed')
        shutil.copy2(installed(), recovery() / 'miy-server')
        require(digest(recovery() / 'miy-server') == args.installed_sha256, 'binary_backup_changed')
    except (Exception, KeyboardInterrupt) as error:
        REPORT.update({'status': 'PREFLIGHT_FAILED', 'failureCategory': failure_code(error),
                       'productionMutations': False, 'finishedAt': now()})
        save()
        raise
    REPORT['status'] = 'IN_PROGRESS'
    save()
    started = False
    try:
        stop_core()
        REPORT['idleAfterStop'] = idle()
        columns = stable_columns()
        before = stable_records(columns)
        REPORT.update({'stableColumns': columns, 'stableRecordsBefore': before})
        backup, tool = recovery() / 'core', source / 'scripts/core-backup.py'
        call(['python3', tool, 'create', '--db-container', DB, '--database', 'miy_core', '--out', backup])
        manifest = json.loads((backup / 'manifest.json').read_text())
        require(manifest['migrationVersion'] == facts['liveSchema'], 'backup_migration_version')
        REPORT['databaseBackup'] = {**manifest['dump'], 'migrationVersion': manifest['migrationVersion'],
                                    'tables': len(manifest['tables']), 'rows': sum(manifest['tables'].values())}
        verification = json.loads(call(['python3', tool, 'verify', backup, '--migrate-with', candidate]))
        require(verification.get('status') == 'PASS' and verification.get('containerRemoved') is True
                and verification.get('mismatchedTables') == []
                and verification.get('rehearsal') == {'exitCode': 0, 'migrationVersionAfter': schema},
                'backup_restore_not_verified')
        REPORT['backupVerified'] = True
        save()
        require(container_id(DB) == facts['databaseContainer'] and networks() == facts['databaseNetworks'],
                'database_container_changed')
        require(app_containers(list(before_containers)) == before_containers, 'runtime_containers_changed_before_start')
        require(digest(candidate) == sha, 'candidate_changed')
        atomic_copy(candidate, installed())
        require(digest(installed()) == sha, 'installed_candidate_hash_mismatch')
        source_preflight(source, args.commit)
        if args.widen_migration is not None:
            # The verified candidate applies only the pinned widening migration while
            # core is stopped. The previous binary keeps working on the result.
            call(['env', f'MIY_BINARY={installed()}', 'bash', LIVE / 'scripts/run-service.sh', 'migrate'])
            REPORT['migratedSchema'] = migrations()
            save()
            require(REPORT['migratedSchema'] == schema, 'widen_migration_not_applied')
        # From here a failed check must not discard writes the new API may have accepted.
        started = True
        start_core()
        wait(lambda: healthy(facts['cliGuide']), 90, 'post_deployment_health_failed')
        require_core_state('active')
        REPORT['runningExecutables'] = executable_hashes(sha)
        require(migrations() == schema, 'schema_changed_after_start')
        REPORT['stableRecordsAfter'] = stable_records(columns)
        require(REPORT['stableRecordsAfter'] == before, 'post_start_stable_records_changed')
        REPORT['appContainersAfter'] = app_containers(list(before_containers))
        require(REPORT['appContainersAfter'] == before_containers, 'runtime_containers_changed')
        REPORT['connectionBudgetAfter'] = connection_budget(source, 35)
        REPORT['status'] = 'PASS'
    except (Exception, KeyboardInterrupt) as error:
        REPORT['failureCategory'] = failure_code(error)
        REPORT['status'] = 'POST_START_FAILED' if started else 'DEPLOYMENT_FAILED'
        save()
        if started:
            try:
                stop_core()
                REPORT['coreStoppedForAssessment'] = True
            except Exception as stop_error:
                REPORT['coreStoppedForAssessment'] = False
                REPORT['stopFailureCategory'] = failure_code(stop_error)
            save()
            raise RuntimeError('post_start_failed_manual_assessment_required') from None
        try:
            restore_previous(REPORT.get('migratedSchema', facts['liveSchema']))
        except (Exception, KeyboardInterrupt) as rollback_error:
            REPORT['status'] = 'ROLLBACK_FAILED'
            REPORT['rollbackFailureCategory'] = failure_code(rollback_error)
            save()
            raise RuntimeError('rollback_failed_manual_recovery_required') from None
        raise
    finally:
        REPORT['finishedAt'] = now()
        save()
    return {'status': 'PASS', 'sourceCommit': args.commit, 'serverSha256': sha, 'schema': schema}


def rollback():
    deployment = json.loads((REL / 'deployment.json').read_text())
    require(deployment.get('action') == 'apply' and deployment.get('status') in ('PASS', 'POST_START_FAILED'),
            'rollback_not_applicable')
    previous, candidate, schema = deployment['previousSha256'], deployment['serverSha256'], deployment['schema']
    REPORT.update({'action': 'rollback', 'tool': 'scripts/core-rollout.py', 'toolSha256': digest(__file__),
                   'startedAt': now(), 'deploymentStatus': deployment['status'], 'sourceCommit': deployment['sourceCommit'],
                   'fromSha256': candidate, 'previousSha256': previous, 'schema': schema, 'status': 'IN_PROGRESS'})
    save()
    try:
        require(digest(recovery() / 'miy-server') == previous, 'rollback_binary_changed')
        require(digest(installed()) in (candidate, previous), 'installed_binary_belongs_to_another_rollout')
        require(migrations() == schema, 'schema_changed_since_rollout')
        check_idle = any(state not in STOPPED_STATES for state in service_states().values())
        if check_idle:
            # A failed service does not mean its peer has stopped accepting or processing work.
            REPORT['idleBeforeStop'] = idle()
        else:
            # Only a proved-empty core may hand its persisted work to the previous binary.
            require_core_quiescent()
    except (Exception, KeyboardInterrupt) as error:
        REPORT.update({'status': 'REFUSED', 'failureCategory': failure_code(error), 'productionMutations': False,
                       'finishedAt': now()})
        save()
        raise
    try:
        stop_core()
        if check_idle:
            REPORT['idleAfterStop'] = idle()
        atomic_copy(recovery() / 'miy-server', installed())
        require(digest(installed()) == previous, 'rollback_install_mismatch')
        start_core()
        wait(lambda: get_json(ORIGIN + '/api/health') == {'status': 'ok'}, 90, 'rollback_health_failed')
        require_core_state('active')
        REPORT['runningExecutables'] = executable_hashes(previous)
        require(migrations() == schema, 'rollback_migration_version')
        REPORT['status'] = 'ROLLED_BACK'
    except (Exception, KeyboardInterrupt) as error:
        REPORT['status'] = 'ROLLBACK_FAILED'
        REPORT['failureCategory'] = failure_code(error)
        raise RuntimeError('rollback_failed_manual_recovery_required') from None
    finally:
        REPORT['finishedAt'] = now()
        save()
    return {'status': 'ROLLED_BACK', 'serverSha256': previous}


def main(argv=None):
    global REL, REPORT_FILE
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('action', nargs='?', choices=['inspect', 'apply', 'rollback'], default='inspect')
    parser.add_argument('--source', type=pathlib.Path, help='frozen clean checkout of the deployed commit')
    parser.add_argument('--commit', help='full source commit')
    parser.add_argument('--candidate', type=pathlib.Path, help='server binary built from --source')
    parser.add_argument('--candidate-sha256')
    parser.add_argument('--installed-sha256', help='expected current installed and running binary')
    parser.add_argument('--release-dir', type=pathlib.Path, help='new directory under .local per apply attempt')
    parser.add_argument('--max-connections', type=int, default=200, help='expected; never changed')
    parser.add_argument('--identity-pool', type=int, default=10, help='expected KC_DB_POOL_MAX_SIZE; never changed')
    parser.add_argument('--widen-migration', type=int, help='allow exactly this pinned constraint-widening migration')
    args = parser.parse_args(argv)
    try:
        if args.action == 'inspect':
            result = {'status': 'READY', 'sourceCommit': args.commit, **public(preflight(args))}
        else:
            require(args.release_dir is not None, 'missing_release_dir')
            REL = plain(args.release_dir)
            if args.action == 'apply':
                REPORT_FILE = REL / 'deployment.json'
                with rollout_lock(fresh=True):
                    result = apply(args)
            else:
                with rollout_lock(fresh=False):
                    REPORT_FILE = REL / time.strftime('rollback-%Y%m%dT%H%M%SZ.json', time.gmtime())
                    require(not REPORT_FILE.exists(), 'rollback_record_exists')
                    result = rollback()
    except Exception as error:
        print(json.dumps({'status': 'FAIL', 'action': args.action, 'failure': failure_code(error)}))
        return 1
    print(json.dumps(result))
    return 0


if __name__ == '__main__':
    sys.exit(main())
