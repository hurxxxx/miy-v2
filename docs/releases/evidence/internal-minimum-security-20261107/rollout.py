#!/usr/bin/env python3
"""One-time 19->20 portal/console transition. Default is read-only inspect.

Reviewed helpers, exact-SHA CI, private backups and real-dump rollback rehearsal
are required. Public writes remain closed until all prepublication checks pass.
After publication a failure never automatically replaces an accepting database.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request

sys.dont_write_bytecode = True
os.umask(0o077)
HERE = Path(__file__).resolve().parent
LIVE = Path('/projects/miy-v2')
SOURCE = Path('/tmp/miy-release-58cecff')
COMMIT = 'a4ad37006c58bc80de61892517cd3e1a153208ba'
CANDIDATE = HERE / 'miy-server-58cecff'
CANDIDATE_SHA = '450a1709dea67803ce120bcc8c4fc55d9823f28006cf420378f312c93c26e5a2'
PREVIOUS_SHA = 'd9ce25351221bce31d5342785a9906f6163fdc7d2bc83e468092d20613d5c46e'
REL = LIVE / '.local/release-a4ad370-boundaries/core-02'
UI = LIVE / '.local/release-a4ad370-boundaries/ui'
CONSOLE = 'https://v2.dwdcc.kr:9443'
DELETED = ('sessions', 'development_sessions', 'login_flows', 'cli_authorizations')
PREPARED = HERE / 'settings-prepared-a4ad370-attempt-02'


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


core = load('core_rollout', SOURCE / 'scripts/core-rollout.py')
restore = load('restore_core', HERE / 'rehearsal/restore_core.py')
core.REL = REL
core.STABLE_TABLES += ['installation_permissions', 'installation_permission_history',
                       'cli_tokens', 'cli_sessions', 'cli_access_tokens', 'cli_refresh_tokens', 'delegations']
require = core.require
digest = core.digest


def command(args, *, input=None, stdin=None, stdout=None, env=None, timeout=180):
    result = subprocess.run([str(a) for a in args], cwd=LIVE, input=input, stdin=stdin,
                            stdout=subprocess.PIPE if stdout is None else stdout,
                            stderr=subprocess.PIPE, text=stdout is None, env=env, timeout=timeout)
    # No output or environment is recorded: database/identity commands may contain secrets.
    require(result.returncode == 0, 'transition_command_failed')
    return (result.stdout or '').strip() if stdout is None else ''


def sql(query, database='miy_core'):
    return command(['docker', 'exec', '-i', core.DB, 'psql', '-X', '-qAt', '-v',
                    'ON_ERROR_STOP=1', '-U', 'postgres', '-d', database], input=query)


def ci_gate():
    response = json.loads(command(['gh', 'api', 'repos/hurxxxx/miy-v2/actions/runs?head_sha='
                                  + COMMIT + '&event=push&per_page=100']))
    required = {'cli-foundation.yml', 'web.yml', 'runtime-regression.yml'}
    selected = {}
    for run in response['workflow_runs']:
        name = run['path'].rsplit('/', 1)[-1]
        if name in required and name not in selected:
            require(run['head_sha'] == COMMIT and run['head_branch'] == 'main' and run['event'] == 'push',
                    'ci_source_mismatch')
            require(run['status'] == 'completed' and run['conclusion'] == 'success', 'ci_not_successful')
            jobs = json.loads(command(['gh', 'run', 'view', str(run['id']), '--json', 'jobs']))['jobs']
            require(jobs and all(job['status'] == 'completed' and job['conclusion'] == 'success' for job in jobs),
                    'ci_jobs_not_successful')
            selected[name] = {'runId': run['id'], 'attempt': run['run_attempt'], 'url': run['html_url'],
                              'headSha': run['head_sha'], 'conclusion': run['conclusion'], 'jobs': len(jobs)}
    require(set(selected) == required, 'required_ci_missing')
    return selected


def migration_set():
    return json.loads(sql("SELECT json_object_agg(version::text,encode(checksum,'hex')) "
                          "FROM _sqlx_migrations WHERE success"))


def expected_migrations(maximum):
    return {str(int(p.name[:4])): hashlib.sha384(p.read_bytes()).hexdigest()
            for p in (SOURCE / 'migrations/core').glob('*.sql') if int(p.name[:4]) <= maximum}


def proof(path, dump=None):
    value = json.loads(path.read_text())
    require(value['status'] == 'PASS' and value['containerRemoved'] and value['sourceCommit'] == COMMIT
            and value['candidateSha256'] == CANDIDATE_SHA and value['migratedVersion'] == 20
            and value['restoredVersion'] == 19 and value['stableRowDigestsMatch']
            and value['retainedRowsMatch'] and value['ownershipAndObjectACLMatch']
            and value['restoreHelperSha256'] == digest(HERE / 'rehearsal/restore_core.py'),
            'rollback_rehearsal_not_verified')
    if dump:
        require(value['backupSha256'] == digest(dump), 'rehearsal_backup_changed')
    return {'path': str(path), 'sha256': digest(path), 'status': value['status']}


class Settings:
    def __init__(self):
        self.helper = load('boundary_settings', HERE / 'settings_transition.py')

    def verify_prepared(self):
        info = self.helper.manifest(PREPARED)
        require(info['sourceCommit'] == COMMIT and info['consoleOrigin'] == CONSOLE, 'prepared_source_changed')
        for name, entry in info['files'].items():
            require(digest(PREPARED / 'backup' / name) == entry['beforeSha256'], 'configuration_backup_changed')
            require(hashlib.sha256(self.helper.read_private(entry['destination'])).hexdigest() == entry['beforeSha256'],
                    'live_configuration_changed')
            if 'candidateSha256' in entry:
                require(digest(PREPARED / 'candidate' / name) == entry['candidateSha256'], 'configuration_candidate_changed')
        for phase, expected in info['nginx'].items():
            require(digest(PREPARED / 'candidate' / f'nginx.{phase}.conf') == expected, 'proxy_candidate_changed')
        require((PREPARED / 'backup/identity.json').is_file(), 'identity_backup_missing')
        require(digest(PREPARED / 'backup/identity.json') == info['identityBackupSha256'], 'identity_patch_backup_changed')
        identity_proof = json.loads((HERE / 'rehearsal/identity/result-01.json').read_text())
        identity_manifest = json.loads((HERE / 'rehearsal/identity/manifest.json').read_text())
        require(identity_proof['status'] == 'PASS' and identity_proof['containerRemoved']
                and identity_proof['backupSha256'] == identity_manifest['dump']['sha256']
                == digest(HERE / 'rehearsal/identity/identity.dump'), 'identity_restore_not_verified')

    def reload(self):
        command(['sudo', '-n', 'nginx', '-t'])
        command(['sudo', '-n', 'systemctl', 'reload', 'nginx'])

    def maintenance_apply(self):
        self.helper.proxy_apply(PREPARED, 'maintenance')
        self.reload()
        def closed():
            for origin in [core.ORIGIN, CONSOLE]:
                try:
                    with urllib.request.urlopen(origin + '/auth/options', timeout=5):
                        return False
                except urllib.error.HTTPError as error:
                    if error.code != 503:
                        return False
            return True
        # systemctl reload returns once HUP is delivered, before new workers have
        # necessarily replaced the old listeners. Require observable readiness.
        core.wait(closed, 15, 'public_traffic_not_closed')

    def identity_apply(self):
        self.helper.identity_apply(PREPARED)

    def identity_restore(self):
        self.helper.identity_restore(PREPARED)

    def config_apply(self):
        self.helper.config_apply(PREPARED)

    def config_restore(self):
        self.helper.config_restore(PREPARED)

    def proxy_restore(self):
        info = self.helper.manifest(PREPARED)
        rollback = PREPARED / 'candidate/nginx.rollback.conf'
        require(digest(rollback) == info['nginx']['rollback'], 'rollback_proxy_changed')
        for name in ['nginx.site.conf', 'nginx.generated.conf']:
            entry = info['files'][name]
            self.helper.install_file(rollback, entry, [entry['beforeSha256'], *info['nginx'].values()])
        self.reload()

    def verify_identity(self):
        value = core.get_json(CONSOLE + '/identity/realms/miy/.well-known/openid-configuration')
        expected = CONSOLE + '/identity/realms/miy'
        require(value['issuer'] == expected and all(value[key].startswith(expected + '/') for key in
                ['authorization_endpoint', 'token_endpoint', 'userinfo_endpoint', 'jwks_uri', 'end_session_endpoint']),
                'public_identity_origin_mismatch')

    def final_apply(self):
        self.helper.proxy_apply(PREPARED, 'final')
        self.reload()


def preflight(settings):
    core.source_preflight(SOURCE, COMMIT)
    require(digest(CANDIDATE) == CANDIDATE_SHA and digest(core.installed()) == PREVIOUS_SHA,
            'binary_changed')
    require(command([CANDIDATE, '--version']) == 'miy 0.7.0', 'candidate_version_mismatch')
    require(core.source_facts(SOURCE)['schema'] == 20 and migration_set() == expected_migrations(19),
            'unexpected_migration_set')
    core.require_core_state('active')
    core.core_units()
    core.core_cgroup_population()
    core.executable_hashes(PREVIOUS_SHA)
    require(int(sql('SHOW max_connections', 'postgres')) == 200, 'database_settings_changed')
    require(core.container_setting(core.KEYCLOAK, 'KC_DB_POOL_MAX_SIZE') == '10', 'identity_pool_changed')
    require(core.get_json(core.ORIGIN + '/api/health') == {'status': 'ok'}, 'old_health_failed')
    settings.verify_prepared()
    result = {'sourceCommit': COMMIT, 'candidateSha256': CANDIDATE_SHA, 'ci': ci_gate(),
              'rehearsal': proof(HERE / 'rehearsal/result-02.json'),
              'idle': core.idle(), 'cliGuide': core.cli_guide(),
              'appContainers': core.app_containers(), 'databaseContainer': core.container_id(core.DB),
              'databaseNetworks': core.networks(), 'identityContainer': core.container_id(core.KEYCLOAK),
              'connectionBudget': core.connection_budget(SOURCE, 0)}
    inspection = json.loads(ui_command('inspect'))
    require(inspection['matchesExpectedLiveIndex'], 'live_ui_changed')
    return result


def credential_counts():
    return {table: int(sql(f'SELECT count(*) FROM {table}')) for table in DELETED}


def ui_command(action, *extra):
    # ui-rollout binds its lock/state root to its checkout. Use the live checkout's
    # identical, hash-verified script with the frozen source's build manifest.
    tool = LIVE / 'scripts/ui-rollout.py'
    require(digest(tool) == digest(SOURCE / 'scripts/ui-rollout.py'), 'ui_tool_changed')
    return command(['python3', tool, action, '--release-dir', UI, '--live', LIVE / 'web/platform/dist', *extra])


def ui_rollback_if_needed():
    manifest = json.loads((UI / 'manifest.json').read_text())
    current = digest(LIVE / 'web/platform/dist/index.html')
    if current == manifest['expectedLiveIndexSha256']:
        return
    require(current == manifest['candidateIndexSha256'] and (UI / 'production-backup/manifest.json').is_file(),
            'ui_recovery_evidence_missing')
    ui_command('rollback')


def stage(name):
    core.REPORT['stage'] = name
    core.REPORT.setdefault('stages', []).append({'name': name, 'at': core.now()})
    core.save()


def env_file(path):
    result = dict(os.environ)
    for line in path.read_text().splitlines():
        if not line or line.startswith('#'):
            continue
        key, value = line.split('=', 1)
        result[key] = ' '.join(shlex.split(value))
    return result


def apply(settings):
    core.REPORT_FILE = REL / 'deployment.json'
    core.REPORT.update({'status': 'PREFLIGHT', 'sourceCommit': COMMIT, 'serverSha256': CANDIDATE_SHA,
                        'previousSha256': PREVIOUS_SHA, 'startedAt': core.now(), 'rollbackUsed': False,
                        'publicWritesOpened': False, 'toolSha256': digest(__file__)})
    core.save()
    before = preflight(settings)
    core.REPORT['preflight'] = before
    core.save()
    shutil.copy2(core.installed(), core.recovery() / 'miy-server')
    shutil.copy2(CANDIDATE, core.recovery() / 'miy-server.candidate')
    candidate = core.recovery() / 'miy-server.candidate'
    require(digest(candidate) == CANDIDATE_SHA and digest(core.recovery() / 'miy-server') == PREVIOUS_SHA,
            'binary_backup_changed')
    maintenance = identity = migrated = ui_changed = False
    try:
        stage('close_public_writes')
        maintenance = True
        settings.maintenance_apply()
        stage('stop_core_and_final_snapshot')
        core.stop_core()
        core.idle()
        columns = core.stable_columns()
        stable = core.stable_records(columns)
        old_counts = credential_counts()
        core.REPORT['stableRecordsBefore'] = stable
        core.REPORT['invalidatedBrowserRowsBefore'] = old_counts
        backup = core.recovery() / 'core'
        command(['python3', SOURCE / 'scripts/core-backup.py', 'create', '--db-container', core.DB,
                 '--database', 'miy_core', '--out', backup])
        final_proof = core.recovery() / 'final-rehearsal.json'
        command(['python3', HERE / 'rehearsal/rehearse.py', backup, candidate, '--out', final_proof], timeout=240)
        core.REPORT['finalBackupProof'] = proof(final_proof, backup / 'miy_core.dump')
        core.save()
        require(migration_set() == expected_migrations(19), 'live_migrations_changed')
        stage('identity_and_configuration_transition')
        identity = True
        settings.identity_apply()
        settings.config_apply()
        stage('migrate_core')
        migrated = True
        command([candidate, 'migrate'], env=env_file(LIVE / '.local/runtime.env'))
        require(migration_set() == expected_migrations(20), 'migrations_differ_after_transition')
        require(credential_counts() == {table: 0 for table in DELETED}, 'old_browser_credentials_remain')
        require(sql("SELECT count(*)=0 FROM cli_device_authorizations WHERE status IN ('pending','approved')") == 't',
                'pending_approval_remains')
        require(sql('SELECT count(*)=count(id) AND count(*)=count(DISTINCT id) FROM cli_tokens') == 't',
                'legacy_public_id_invalid')
        require(core.stable_records(columns) == stable, 'migration_changed_stable_records')
        stage('install_core_and_ui')
        core.atomic_copy(candidate, core.installed())
        ui_changed = True
        ui_command('apply', '--expected-commit', COMMIT)
        core.start_core()
        core.wait(lambda: core.get_json('http://127.0.0.1:18180/api/health') == {'status': 'ok'}
                  and core.get_json('http://127.0.0.1:18182/api/health') == {'status': 'ok'},
                  90, 'candidate_health_failed')
        core.require_core_state('active')
        core.REPORT['runningExecutables'] = core.executable_hashes(CANDIDATE_SHA)
        require(core.stable_records(columns) == stable, 'post_start_stable_records_changed')
        require(core.app_containers(list(before['appContainers'])) == before['appContainers'], 'app_containers_changed')
        require(core.container_id(core.DB) == before['databaseContainer'] and core.networks() == before['databaseNetworks'],
                'database_container_changed')
        require(core.container_id(core.KEYCLOAK) == before['identityContainer'], 'identity_container_changed')
        core.REPORT['connectionBudgetAfter'] = core.connection_budget(SOURCE, 35)
        settings.verify_identity()
        stage('publish')
        # Once final routes may become visible, never automatically overwrite accepted writes.
        core.REPORT['publicWritesOpened'] = True
        core.save()
        settings.final_apply()
        core.wait(lambda: core.healthy(before['cliGuide'])
                  and core.get_json(CONSOLE + '/api/health') == {'status': 'ok'}, 30, 'public_health_failed')
        portal = core.get_json(core.ORIGIN + '/auth/options')
        console = core.get_json(CONSOLE + '/auth/options')
        require(portal['surface'] == 'portal' and console['surface'] == 'console'
                and portal['consoleOrigin'] == CONSOLE and console['portalOrigin'] == core.ORIGIN,
                'public_purpose_mismatch')
        require(portal['developmentLogin'] is False and console['developmentLogin'] is False,
                'shared_development_login_exposed')
        core.REPORT.update({'status': 'PASS', 'schema': 20, 'stableRecordsPreserved': True,
                            'appContainersPreserved': True, 'independentCliCredentialsPreserved': True})
    except (Exception, KeyboardInterrupt) as error:
        core.REPORT['failureCategory'] = core.failure_code(error)
        core.REPORT['status'] = 'FAILED'
        core.save()
        if core.REPORT['publicWritesOpened']:
            core.REPORT['status'] = 'POST_PUBLICATION_FAILED_ASSESSMENT_REQUIRED'
            for name, action in [('close_public_writes', settings.maintenance_apply), ('stop_core', core.stop_core)]:
                try:
                    action()
                    core.REPORT[name + 'AfterFailure'] = 'PASS'
                except Exception as isolation_error:
                    core.REPORT[name + 'AfterFailure'] = core.failure_code(isolation_error)
            core.save()
            raise RuntimeError('post_publication_failure_preserved_for_assessment') from None
        if maintenance:
            stage('rollback_before_publication')
            core.stop_core()
            if migrated:
                core.REPORT['preservedFailedDatabase'] = restore.restore_database(
                    command, core.recovery() / 'core/miy_core.dump', container=core.DB)
                require(migration_set() == expected_migrations(19), 'rollback_schema_mismatch')
                # Do not reactivate old browser credentials and unfinished CLI approvals.
                sql(';'.join('DELETE FROM ' + table for table in DELETED)
                    + ";DELETE FROM cli_device_authorizations WHERE status IN ('pending','approved')")
            if identity:
                settings.identity_restore()
            settings.config_restore()
            core.atomic_copy(core.recovery() / 'miy-server', core.installed())
            if ui_changed:
                ui_rollback_if_needed()
            settings.proxy_restore()
            core.start_core()
            core.wait(lambda: core.healthy(before['cliGuide']), 60, 'rollback_health_failed')
            core.executable_hashes(PREVIOUS_SHA)
            core.REPORT.update({'status': 'ROLLED_BACK', 'rollbackUsed': True})
        raise
    finally:
        core.REPORT['finishedAt'] = core.now()
        core.save()
    return {'status': 'PASS', 'sourceCommit': COMMIT, 'serverSha256': CANDIDATE_SHA, 'schema': 20}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['inspect', 'apply'], nargs='?', default='inspect')
    args = parser.parse_args()
    try:
        settings = Settings()
        if args.action == 'inspect':
            data = preflight(settings)
            result = {'status': 'READY', 'sourceCommit': COMMIT, 'ci': data['ci'],
                      'apps': len(data['appContainers']), 'schemaTransition': '19->20'}
        else:
            with core.rollout_lock(fresh=True):
                result = apply(settings)
    except (Exception, KeyboardInterrupt) as error:
        if args.action == 'apply' and core.REPORT_FILE is not None:
            if core.REPORT.get('status') == 'PREFLIGHT':
                core.REPORT.update({'status': 'PREFLIGHT_FAILED', 'productionMutations': False,
                                    'failureCategory': core.failure_code(error), 'finishedAt': core.now()})
            elif core.REPORT.get('stage') == 'rollback_before_publication' and core.REPORT.get('status') != 'ROLLED_BACK':
                core.REPORT.update({'status': 'ROLLBACK_FAILED_ASSESSMENT_REQUIRED',
                                    'rollbackFailureCategory': core.failure_code(error), 'finishedAt': core.now()})
            core.save()
        result = {'status': 'FAIL', 'failure': core.failure_code(error)}
        print(json.dumps(result))
        return 1
    print(json.dumps(result))
    return 0


if __name__ == '__main__':
    sys.exit(main())
