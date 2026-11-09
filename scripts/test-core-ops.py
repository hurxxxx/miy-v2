#!/usr/bin/env python3
"""Offline safety regression for core backup, capacity reports and the maintained rollout tools.

Every process invocation is mocked. Never calls Docker, systemd, a server binary or a database.
Rollout tests cover scripts/core-rollout.py and scripts/ui-rollout.py, the tools later rollouts
run; per-release evidence copies under docs/releases/evidence stay as history.
"""
import contextlib
from concurrent.futures import ThreadPoolExecutor
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
from types import ModuleType, SimpleNamespace
import unittest
from unittest.mock import Mock, patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent


def load(name, path):
    module = ModuleType(name)
    module.__file__ = str(path)
    exec(compile(path.read_text(), str(path), 'exec'), module.__dict__)
    return module


backup = load('backup_under_test', ROOT / 'scripts/core-backup.py')
budget = load('budget_under_test', ROOT / 'scripts/check-db-connections.py')
CID = 'a' * 64
TOKEN = 'b' * 16
COUNTS = {'migrationVersion': 17, 'tables': {'fixture': 1}}


class OfflineTest(unittest.TestCase):
    def setUp(self):
        self.no_process = patch.object(subprocess, 'run', side_effect=AssertionError('unmocked process forbidden')).start()
        patch.object(subprocess, 'Popen', side_effect=AssertionError('real process forbidden')).start()
        self.addCleanup(patch.stopall)
        self.temp = tempfile.TemporaryDirectory(prefix='miy-core-ops-test-')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def fixture_backup(self, root=None):
        root = root or self.root
        root.mkdir(parents=True, exist_ok=True)
        payload = b'synthetic dump; pg_restore is mocked'
        (root / 'miy_core.dump').write_bytes(payload)
        manifest = {'dump': {'file': 'miy_core.dump', 'bytes': len(payload),
                             'sha256': hashlib.sha256(payload).hexdigest()}, **COUNTS}
        (root / 'manifest.json').write_text(json.dumps(manifest))
        return manifest


class BackupCleanupTests(OfflineTest):
    def verify_with(self, mode):
        self.fixture_backup()
        state = {'alive': False, 'removals': 0, 'lists': 0}

        def run(argv, **kwargs):
            self.assertEqual(argv[0], 'docker')
            rc, output, error = 0, '', ''
            if argv[1] == 'run':
                state['alive'] = True
                output = CID + '\n'
            elif argv[1] == 'exec':
                if 'psql' in argv:
                    output = json.dumps(COUNTS)
                else:
                    self.assertTrue(any(word in argv for word in ('pg_isready', 'createdb', 'pg_restore')))
            elif argv[1:3] == ['container', 'ls']:
                self.assertEqual(argv[-1], 'id=' + CID)
                state['lists'] += 1
                if mode == 'daemon_before_cleanup' or (mode == 'daemon_after_rm' and state['lists'] > 1):
                    rc, error = 1, 'daemon unavailable'
                else:
                    output = CID + '\n' if state['alive'] else ''
            elif argv[1] == 'inspect':
                self.assertEqual(argv[-1], CID)
                if mode == 'ownership_unavailable':
                    rc, error = 1, 'daemon unavailable'
                else:
                    output = ('foreign-run' if mode == 'foreign' else TOKEN) + '\n'
            elif argv[1] == 'rm':
                self.assertEqual(argv, ['docker', 'rm', '-f', '-v', CID])
                state['removals'] += 1
                if mode in ('daemon_after_rm', 'rm_failed'):
                    rc, error = 1, 'daemon unavailable'
                else:
                    state['alive'] = False
                    if mode == 'rm_response_lost':
                        rc, error = 1, 'response lost after removal'
            else:
                self.fail('unexpected mocked Docker operation')
            if not kwargs.get('text'):
                output, error = output.encode(), error.encode()
            if kwargs.get('check') and rc:
                raise subprocess.CalledProcessError(rc, argv, output, error)
            return subprocess.CompletedProcess(argv, rc, output, error)

        output = io.StringIO()
        args = SimpleNamespace(backup=str(self.root), image='postgres:fixture', migrate_with=None)
        with patch.object(subprocess, 'run', side_effect=run), \
             patch.object(backup.secrets, 'token_hex', side_effect=[TOKEN, 'private-fixture-password']), \
             contextlib.redirect_stdout(output):
            code = backup.verify(args)
        return code, json.loads(output.getvalue()), state

    def test_daemon_failures_never_report_successful_cleanup(self):
        for mode in ('daemon_before_cleanup', 'ownership_unavailable', 'daemon_after_rm', 'rm_failed'):
            with self.subTest(mode=mode):
                code, report, state = self.verify_with(mode)
                self.assertEqual(code, 1)
                self.assertEqual(report['status'], 'FAIL')
                self.assertFalse(report['containerRemoved'])
                self.assertIn('cleanupError', report)
                self.assertTrue(state['alive'])

    def test_foreign_ownership_is_preserved(self):
        code, report, state = self.verify_with('foreign')
        self.assertEqual(code, 1)
        self.assertEqual(report['cleanupError'], 'container_ownership_mismatch')
        self.assertEqual(state['removals'], 0)
        self.assertTrue(state['alive'])

    def test_success_and_lost_removal_response_require_confirmed_absence(self):
        for mode in ('success', 'rm_response_lost'):
            with self.subTest(mode=mode):
                code, report, state = self.verify_with(mode)
                self.assertEqual(code, 0)
                self.assertEqual(report['status'], 'PASS')
                self.assertTrue(report['containerRemoved'])
                self.assertFalse(state['alive'])
                self.assertEqual(state['lists'], 2)

    def test_absent_owned_id_needs_successful_listing_only(self):
        self.no_process.side_effect = None
        self.no_process.return_value = subprocess.CompletedProcess([], 0, '', '')
        self.assertEqual(backup.cleanup_container(CID, 'fixture.label', TOKEN), {'containerRemoved': True})
        self.assertEqual(self.no_process.call_count, 1)
        self.assertEqual(self.no_process.call_args.args[0][1:3], ['container', 'ls'])

    def test_invalid_id_does_not_invoke_docker(self):
        self.assertEqual(backup.cleanup_container('--all', 'fixture.label', TOKEN)['cleanupError'], 'invalid_container_id')
        self.no_process.assert_not_called()

    def test_ambiguous_lookup_does_not_remove_anything(self):
        self.no_process.side_effect = None
        self.no_process.return_value = subprocess.CompletedProcess([], 0, CID + '\n' + 'c' * 64, '')
        self.assertEqual(backup.cleanup_container(CID, 'fixture.label', TOKEN)['cleanupError'], 'container_lookup_ambiguous')
        self.assertEqual(self.no_process.call_count, 1)

    def test_failed_command_does_not_echo_password_arguments(self):
        args = ['docker', 'run', '-e', 'POSTGRES_PASSWORD=private-sentinel']
        self.no_process.side_effect = subprocess.CalledProcessError(1, args)
        with self.assertRaisesRegex(RuntimeError, '^command_failed$'):
            backup.run(args)


class ConnectionBudgetTests(OfflineTest):
    def report(self, installations=7, sessions=0, maximum=100, unbounded=False, identity=10):
        roles = []
        for _ in range(installations):
            roles.extend([{'kind': 'runtime', 'limit': -1 if unbounded else 6}, {'kind': 'migration', 'limit': 2}])
        roles.extend({'kind': 'development_session', 'limit': 16} for _ in range(sessions))
        output = io.StringIO()
        with patch.object(sys, 'argv', ['check-db-connections.py']), \
             patch.object(budget, 'psql', side_effect=[f'max_connections|{maximum}\nsuperuser_reserved_connections|3\nreserved_connections|0',
                                                      json.dumps(roles), '{}']), \
             patch.object(budget, 'identity_pool', return_value=(identity, 'fixture')), \
             contextlib.redirect_stdout(output):
            code = budget.main()
        self.no_process.assert_not_called()
        return code, json.loads(output.getvalue())

    def test_reserves_all_independent_admin_lanes(self):
        code, report = self.report()
        self.assertEqual(code, 0)
        self.assertEqual(report['worstCase'], 20 + 4 + 10 + 7 * 8)
        self.assertEqual(report['reserved']['coreTransientSources'],
                         {'provisioning': 2, 'developmentLeaseMonitor': 1, 'serverBuildHistory': 1})
        self.assertEqual(report['reserved']['coreTransient'], 4)

    def test_previous_two_connection_margin_is_not_accepted(self):
        code, report = self.report(installations=8)
        self.assertEqual(code, 1)
        self.assertEqual(report['worstCase'], 98)
        self.assertEqual(report['usableConnections'], 97)
        self.assertEqual(report['status'], 'FAIL')

    def test_development_and_migration_roles_are_counted_once(self):
        code, report = self.report(sessions=1, maximum=200)
        self.assertEqual(code, 0)
        self.assertEqual(report['worstCase'], 20 + 4 + 10 + 7 * (6 + 2) + 16)

    def test_unknown_identity_and_unlimited_roles_fail_closed(self):
        for options in ({'identity': None}, {'unbounded': True}):
            with self.subTest(options=options):
                code, report = self.report(**options)
                self.assertEqual(code, 1)
                self.assertTrue(report['problems'])

    def test_invalid_identity_pool_is_unknown_without_echoing_value(self):
        for value in ('0', '-1', 'private-sentinel'):
            with self.subTest(value=value):
                self.no_process.side_effect = None
                self.no_process.return_value = subprocess.CompletedProcess([], 0, 'KC_DB_POOL_MAX_SIZE=' + value, '')
                count, reason = budget.identity_pool('fixture-identity')
                self.assertIsNone(count)
                self.assertEqual(reason, 'invalid KC_DB_POOL_MAX_SIZE')


ZERO_WORK = {'runtime': 0, 'builds': 0, 'database': 0, 'development': 0, 'uploads': 0}
OLD = b'previous server binary'
NEW = b'candidate server binary'
OLD_SHA = hashlib.sha256(OLD).hexdigest()
NEW_SHA = hashlib.sha256(NEW).hexdigest()
COMMIT = 'c' * 40
APP_CONTAINER = 'miy-app-' + 'd' * 32


class FakeHost:
    """Stateful systemd, PostgreSQL, Docker, git and HTTP stand-in with an ordered event log."""

    def __init__(self, test):
        self.test = test
        self.events = []
        self.states = {service: 'active' for service in test.rollout.SERVICES}
        self.running = OLD_SHA
        self.started = []
        self.schema = 19
        self.head = COMMIT
        self.dirty = ''
        self.version = 'miy 0.7.0'
        self.max_connections = '200'
        self.pool = '10'
        self.work = [ZERO_WORK]
        self.stop_works = True
        self.fail_on_stop = set()  # units a stop leaves failed, e.g. after a stop timeout
        self.crash_candidate = set()  # units that fail right after the candidate starts
        self.retain_processes = set()
        self.process_properties = {}
        self.on_stop = None
        self.on_reset_failed = None
        cgroups = test.rollout.CGROUP_ROOT
        shutil.rmtree(cgroups, ignore_errors=True)
        (cgroups / 'system.slice').mkdir(parents=True)
        (cgroups / 'cgroup.controllers').write_text('cpu memory pids\n')
        (cgroups / 'system.slice/cgroup.events').write_text('populated 1\nfrozen 0\n')
        for service in self.states:
            self.population(service, True)
        self.restarts = 0
        self.verify = {'status': 'PASS', 'containerRemoved': True, 'mismatchedTables': [],
                       'rehearsal': {'exitCode': 0, 'migrationVersionAfter': 19}}
        self.on_verify = None
        self.healthy_candidate = True
        self.records_change_with_candidate = False
        self.budget_before = True
        self.budget_with_candidate = True
        # Newest first, like the GitHub API. Each test may replace a run or its jobs.
        self.ci_runs = [{'id': 7000 + index, 'path': '.github/workflows/' + name, 'head_sha': COMMIT,
                         'head_branch': 'main', 'event': 'push', 'status': 'completed', 'conclusion': 'success',
                         'run_attempt': 1, 'html_url': f'https://github.test/run/{index}'}
                        for index, name in enumerate(test.rollout.REQUIRED_CI)]
        self.ci_jobs = {run['id']: [{'status': 'completed', 'conclusion': 'success'}] for run in self.ci_runs}

    def candidate_running(self):
        return self.running == NEW_SHA and all(state == 'active' for state in self.states.values())

    def population(self, service, populated):
        directory = self.test.rollout.CGROUP_ROOT / 'system.slice' / (service + '.service')
        directory.mkdir(exist_ok=True)
        (directory / 'cgroup.events').write_text(f'populated {int(populated)}\nfrozen 0\n')
        return directory

    def call(self, args):
        args = [str(a) for a in args]
        self.test.rollout.REPORT['commands'].append({'command': args, 'exitCode': 0})
        if args[:2] == ['systemctl', 'show'] and args[2] == '--property=ActiveState':
            return self.states[args[-1]]
        if args[:2] == ['systemctl', 'show'] and args[2].startswith('--property=Id,'):
            service = args[-1]
            fields = {'Id': service + '.service', 'LoadState': 'loaded', 'ActiveState': self.states[service],
                      'Slice': 'system.slice', 'ControlGroup': '/system.slice/' + service + '.service',
                      'MainPID': '0' if self.states[service] in ('inactive', 'failed') else '123', 'ControlPID': '0'}
            fields.update(self.process_properties.get(service, {}))
            return '\n'.join(key + '=' + value for key, value in fields.items())
        if args[:4] == ['sudo', '-n', 'systemctl', 'stop']:
            self.events.append('stop')
            if self.stop_works:
                # Like systemd: a failed unit stays failed after stop; only reset-failed clears it.
                self.states = {service: 'failed' if state == 'failed' or service in self.fail_on_stop
                               else 'inactive' for service, state in self.states.items()}
                for service in self.states:
                    self.population(service, service in self.retain_processes)
            if self.on_stop:
                self.on_stop()
            return ''
        if args[:4] == ['sudo', '-n', 'systemctl', 'reset-failed']:
            self.events.append('reset-failed')
            for service in args[4:]:
                self.test.assertEqual(self.states[service], 'failed')
                self.states[service] = 'inactive'
            if self.on_reset_failed:
                self.on_reset_failed()
            return ''
        if args[:4] == ['sudo', '-n', 'systemctl', 'start']:
            self.events.append('start')
            self.running = hashlib.sha256(self.test.installed.read_bytes()).hexdigest()
            self.started.append(self.running)
            self.states = {service: 'active' for service in self.states}
            if self.running == NEW_SHA:
                self.states.update({service: 'failed' for service in self.crash_candidate})
            for service, state in self.states.items():
                self.population(service, state == 'active')
            return ''
        if args[-1] == '--version':
            return self.version
        if args[:2] == ['docker', 'exec']:
            return self.sql(args[-1])
        if args[0] == 'python3' and args[2] == 'create':
            self.events.append('backup')
            out = Path(args[-1])
            self.test.fixture_backup(out)
            manifest = json.loads((out / 'manifest.json').read_text())
            manifest['migrationVersion'] = self.schema
            (out / 'manifest.json').write_text(json.dumps(manifest))
            return '{"status":"CREATED"}'
        if args[0] == 'python3' and args[2] == 'verify':
            self.events.append('verify')
            self.test.assertEqual(args[-1], str(self.test.rollout.REL / 'rollback/miy-server.candidate'))
            if self.on_verify:
                self.on_verify()
            return json.dumps(self.verify)
        self.test.fail('unexpected command: ' + ' '.join(args))

    def sql(self, query):
        if query == 'SELECT max(version) FROM _sqlx_migrations':
            return str(self.schema)
        if query == 'SHOW max_connections':
            return self.max_connections
        if 'runtime_jobs' in query:
            return json.dumps(self.work.pop(0) if len(self.work) > 1 else self.work[0])
        if 'information_schema.columns' in query:
            return '["id","name"]'
        if 'string_agg' in query:
            changed = self.records_change_with_candidate and self.candidate_running()
            return json.dumps({'rows': 3 if changed else 2, 'sha256': ('f' if changed else 'e') * 64})
        if 'json_agg(runtime_container' in query:
            return json.dumps([APP_CONTAINER])
        self.test.fail('unexpected SQL: ' + query)

    def quiet(self, args):
        args = [str(a) for a in args]
        if args[:2] == ['gh', 'api']:
            self.test.assertIn(f'head_sha={COMMIT}&event=push', args[2])
            return json.dumps({'workflow_runs': self.ci_runs})
        if args[:3] == ['gh', 'run', 'view']:
            return json.dumps({'jobs': self.ci_jobs[int(args[3])]})
        if args[0] == 'git':
            return self.head if 'rev-parse' in args else self.dirty
        if args[:4] == ['docker', 'inspect', '-f', '{{json .NetworkSettings.Networks}}']:
            return json.dumps({'miy': {}, 'app-network': {}})
        if args[:4] == ['docker', 'inspect', '-f', '{{.Id}}']:
            return 'database-container'
        if args[:4] == ['docker', 'inspect', '-f', '{{json .Config.Env}}']:
            return json.dumps([f'KC_DB_POOL_MAX_SIZE={self.pool}', 'KC_DB_PASSWORD=private-sentinel'])
        if args[:2] == ['docker', 'inspect'] and args[2] == APP_CONTAINER:
            return json.dumps([{'Name': '/' + APP_CONTAINER, 'Id': 'app-container', 'Image': 'sha256:app',
                                'State': {'Status': 'running', 'StartedAt': 'fixture'}, 'RestartCount': self.restarts}])
        self.test.fail('unexpected inspection: ' + ' '.join(args))

    def get_json(self, url):
        if url.endswith('/api/health'):
            if not all(state == 'active' for state in self.states.values()):
                raise OSError('core unavailable')
            if self.running == NEW_SHA and not self.healthy_candidate:
                return {'status': 'error'}
            return {'status': 'ok'}
        if url.endswith('/api/v1/cli/install'):
            return {'cliVersion': '0.7.0', 'sourceCommit': 'a' * 40, 'windowsInstaller': {'sha256': 'b' * 64}}
        self.test.fail('unexpected URL ' + url)

    def executable_hashes(self, expected):
        self.test.rollout.require(self.running == expected, 'running_binary_hash_mismatch')
        return {service: {'pid': 100 + index, 'sha256': self.running}
                for index, service in enumerate(self.test.rollout.SERVICES)}

    def connection_budget(self, source, seconds):
        if not (self.budget_with_candidate if self.candidate_running() else self.budget_before):
            raise RuntimeError('connection_budget_not_ready')
        return {'status': 'PASS', 'worstCase': 90, 'usableConnections': 197}


class CoreRolloutTests(OfflineTest):
    """scripts/core-rollout.py: what each failure leaves installed and running."""

    def setUp(self):
        super().setUp()
        self.rollout = load('core_rollout_under_test', ROOT / 'scripts/core-rollout.py')
        self.rollout.LIVE = self.root
        self.rollout.CGROUP_ROOT = self.root / 'cgroup'
        (self.root / '.local/bin').mkdir(parents=True)
        self.installed = self.root / '.local/bin/miy-server'
        self.candidate = self.root / 'candidate'
        self.source = self.root / '.local/release-source'
        (self.source / 'scripts').mkdir(parents=True)
        (self.source / 'crates/miy').mkdir(parents=True)
        (self.source / 'migrations/core').mkdir(parents=True)
        (self.source / 'crates/miy/Cargo.toml').write_text('[package]\nname = "miy"\nversion = "0.7.0"\n')
        for name in ('0001_init.sql', '0019_audit_query.sql'):
            (self.source / 'migrations/core' / name).write_text('-- fixture\n')
        self.attempt = 0
        self.reset()
        for name in ('call', 'quiet', 'get_json', 'executable_hashes', 'connection_budget'):
            patch.object(self.rollout, name, lambda *args, _name=name: getattr(self.host, _name)(*args)).start()
        patch.object(self.rollout.time, 'sleep').start()

    def reset(self):
        self.installed.write_bytes(OLD)
        self.candidate.write_bytes(NEW)
        shutil.copyfile(ROOT / 'scripts/core-rollout.py', self.source / 'scripts/core-rollout.py')
        (self.source / 'migrations/core/0020_next.sql').unlink(missing_ok=True)
        self.host = FakeHost(self)

    def run_tool(self, action='apply', release=None, **changes):
        self.attempt += 1
        release = release or self.root / f'.local/release-{self.attempt}'
        values = {'--source': self.source, '--commit': COMMIT, '--candidate': self.candidate,
                  '--candidate-sha256': NEW_SHA, '--installed-sha256': OLD_SHA, '--release-dir': release}
        values.update({'--' + key.replace('_', '-'): value for key, value in changes.items()})
        argv = [action] + [str(item) for key, value in values.items() if value is not None for item in (key, value)]
        self.rollout.REPORT, self.rollout.REL, self.rollout.REPORT_FILE = {'commands': []}, None, None
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            code = self.rollout.main(argv)
        return code, json.loads(output.getvalue()), release

    def assert_untouched(self, installed=OLD):
        self.assertNotIn('stop', self.host.events)
        self.assertNotIn('start', self.host.events)
        self.assertEqual(self.host.started, [])
        self.assertEqual(self.installed.read_bytes(), installed)

    def test_apply_installs_only_after_a_verified_backup_and_keeps_private_records(self):
        code, result, release = self.run_tool()
        self.assertEqual((code, result['status']), (0, 'PASS'))
        self.assertEqual(self.host.events, ['stop', 'backup', 'verify', 'start'])
        self.assertEqual(self.installed.read_bytes(), NEW)
        self.assertEqual(self.host.started, [NEW_SHA])
        record_text = (release / 'deployment.json').read_text()
        record = json.loads(record_text)
        self.assertEqual(record['status'], 'PASS')
        self.assertEqual((record['schema'], record['previousSha256'], record['serverSha256']), (19, OLD_SHA, NEW_SHA))
        self.assertTrue(record['backupVerified'])
        self.assertNotIn('private-sentinel', record_text)
        self.assertEqual((release / 'deployment.json').stat().st_mode & 0o777, 0o600)
        self.assertEqual((release / 'rollback').stat().st_mode & 0o777, 0o700)
        self.assertEqual((release / 'rollback/miy-server').read_bytes(), OLD)

    def test_inspect_reads_only(self):
        code, result, release = self.run_tool('inspect')
        self.assertEqual((code, result['status'], result['schema']), (0, 'READY', 19))
        self.assertEqual(self.host.events, [])
        self.assert_untouched()
        self.assertFalse(release.exists())

    def test_preflight_refusals_change_nothing(self):
        cases = {
            'source_commit_mismatch': lambda: setattr(self.host, 'head', 'e' * 40),
            'source_worktree_dirty': lambda: setattr(self.host, 'dirty', ' M crates/miy/src/http.rs'),
            'rollout_tool_differs_from_source': lambda: (self.source / 'scripts/core-rollout.py').write_text('# other\n'),
            'candidate_hash_mismatch': lambda: self.candidate.write_bytes(b'unreviewed'),
            'candidate_version_mismatch': lambda: setattr(self.host, 'version', 'miy 0.6.3'),
            'installed_binary_changed': lambda: self.installed.write_bytes(b'unexpected installed'),
            'running_binary_hash_mismatch': lambda: setattr(self.host, 'running', 'a' * 64),
            'migration_rollout_not_supported': lambda: (self.source / 'migrations/core/0020_next.sql').write_text(''),
            'source_schema_older_than_live': lambda: setattr(self.host, 'schema', 20),
            'database_settings_differ': lambda: setattr(self.host, 'max_connections', '100'),
            'identity_settings_differ': lambda: setattr(self.host, 'pool', '100'),
            'ci_not_successful': lambda: self.host.ci_runs[0].update(conclusion='failure'),
            'ci_source_mismatch': lambda: self.host.ci_runs[1].update(head_branch='feature'),
            'ci_jobs_not_successful': lambda: self.host.ci_jobs[7002].append(
                {'status': 'completed', 'conclusion': 'cancelled'}),
            'required_ci_missing': lambda: self.host.ci_runs.pop(),
            # A newer failed attempt is not hidden by an older successful run of the same workflow.
            'ci_not_successful ': lambda: self.host.ci_runs.insert(0, {**self.host.ci_runs[0], 'id': 7100,
                                                                        'conclusion': 'failure'}),
            'connection_budget_not_ready': lambda: setattr(self.host, 'budget_before', False),
            'active_work_prevents_rollout': lambda: setattr(self.host, 'work', [{**ZERO_WORK, 'runtime': 1}]),
        }
        for code_name, change in cases.items():
            with self.subTest(code_name):
                self.reset()
                change()
                code, result, release = self.run_tool()
                self.assertEqual((code, result['failure']), (1, code_name.strip()))
                record = json.loads((release / 'deployment.json').read_text())
                self.assertEqual(record['status'], 'PREFLIGHT_FAILED')
                self.assertFalse(record['productionMutations'])
                self.assert_untouched(b'unexpected installed' if code_name == 'installed_binary_changed' else OLD)

    def test_source_must_be_a_separate_checkout(self):
        code, result, _ = self.run_tool('inspect', source=self.root)
        self.assertEqual(result['failure'], 'source_must_be_separate_checkout')
        self.assert_untouched()

    def test_failures_before_start_reinstall_and_start_the_previous_binary(self):
        def changed_container():
            self.host.restarts = 1
        cases = {
            'active_work_prevents_rollout': lambda: setattr(self.host, 'work', [ZERO_WORK, {**ZERO_WORK, 'runtime': 1}]),
            'backup_restore_not_verified': lambda: self.host.verify.update(containerRemoved=False),
            'runtime_containers_changed_before_start': lambda: setattr(self.host, 'on_verify', changed_container),
        }
        for code_name, change in cases.items():
            with self.subTest(code_name):
                self.reset()
                change()
                code, result, release = self.run_tool()
                self.assertEqual((code, result['failure']), (1, code_name))
                record = json.loads((release / 'deployment.json').read_text())
                self.assertEqual((record['status'], record['failureCategory']), ('ROLLED_BACK', code_name))
                self.assertTrue(record['rollbackUsed'])
                self.assertEqual(self.installed.read_bytes(), OLD)
                self.assertEqual(self.host.started, [OLD_SHA])
                self.assertEqual(set(self.host.states.values()), {'active'})

    def test_failures_after_start_stop_core_and_keep_the_candidate_for_assessment(self):
        cases = {
            'post_deployment_health_failed': lambda: setattr(self.host, 'healthy_candidate', False),
            'post_start_stable_records_changed': lambda: setattr(self.host, 'records_change_with_candidate', True),
            'connection_budget_not_ready': lambda: setattr(self.host, 'budget_with_candidate', False),
        }
        for code_name, change in cases.items():
            with self.subTest(code_name):
                self.reset()
                change()
                code, result, release = self.run_tool()
                self.assertEqual((code, result['failure']), (1, 'post_start_failed_manual_assessment_required'))
                record = json.loads((release / 'deployment.json').read_text())
                self.assertEqual((record['status'], record['failureCategory']), ('POST_START_FAILED', code_name))
                self.assertTrue(record['coreStoppedForAssessment'])
                self.assertFalse(record['rollbackUsed'])
                self.assertEqual(set(self.host.states.values()), {'inactive'})
                self.assertEqual(self.installed.read_bytes(), NEW)
                self.assertEqual(self.host.events[-2:], ['start', 'stop'])

    def test_changed_recovery_binary_is_never_started(self):
        def tamper():
            (self.rollout.REL / 'rollback/miy-server').write_bytes(b'tampered')
        self.host.verify.update(status='FAIL')
        self.host.on_verify = tamper
        code, result, release = self.run_tool()
        self.assertEqual(result['failure'], 'rollback_failed_manual_recovery_required')
        record = json.loads((release / 'deployment.json').read_text())
        self.assertEqual((record['status'], record['rollbackFailureCategory']), ('ROLLBACK_FAILED', 'rollback_binary_changed'))
        self.assertNotIn('start', self.host.events)
        self.assertEqual(self.installed.read_bytes(), OLD)

    def test_stop_that_leaves_a_service_active_never_installs_or_starts(self):
        self.host.stop_works = False
        code, result, release = self.run_tool()
        self.assertEqual(result['failure'], 'rollback_failed_manual_recovery_required')
        record = json.loads((release / 'deployment.json').read_text())
        self.assertEqual((record['failureCategory'], record['rollbackFailureCategory']), ('core_not_stopped', 'core_not_stopped'))
        self.assertNotIn('backup', self.host.events)
        self.assertNotIn('start', self.host.events)
        self.assertEqual(self.installed.read_bytes(), OLD)

    def test_active_serve_does_not_hide_failed_worker(self):
        self.host.states['miy-v2-worker'] = 'failed'
        with self.assertRaisesRegex(RuntimeError, 'core_not_active'):
            self.rollout.require_core_state('active')

    def test_existing_recovery_data_is_not_overwritten(self):
        release = self.root / '.local/release-existing'
        (release / 'rollback').mkdir(parents=True)
        marker = release / 'rollback/miy-server'
        marker.write_bytes(b'previous recovery binary')
        code, result, _ = self.run_tool(release=release)
        self.assertEqual((code, result['failure']), (1, 'FileExistsError'))
        self.assertEqual(marker.read_bytes(), b'previous recovery binary')
        self.assertEqual(self.host.events, [])

    def test_concurrent_rollout_is_rejected(self):
        self.rollout.REL = self.root / '.local/release-holder'
        with self.rollout.rollout_lock(fresh=True):
            code, result, release = self.run_tool()
        self.assertEqual((code, result['failure']), (1, 'BlockingIOError'))
        self.assertFalse((release / 'rollback').exists())
        self.assertEqual(self.host.events, [])

    def test_release_directory_must_be_under_private_state(self):
        code, result, _ = self.run_tool(release=self.root / 'outside')
        self.assertEqual(result['failure'], 'release_directory_outside_state')
        self.assertFalse((self.root / 'outside').exists())

    def test_operator_rollback_reinstalls_the_previous_binary(self):
        code, _, release = self.run_tool()
        self.assertEqual(code, 0)
        code, result, _ = self.run_tool('rollback', release=release)
        self.assertEqual((code, result['status']), (0, 'ROLLED_BACK'))
        self.assertEqual(self.installed.read_bytes(), OLD)
        self.assertEqual(self.host.started, [NEW_SHA, OLD_SHA])
        receipt = json.loads(next(release.glob('rollback-*.json')).read_text())
        self.assertEqual((receipt['status'], receipt['fromSha256']), ('ROLLED_BACK', NEW_SHA))

    def test_operator_rollback_refuses_another_rollouts_binary(self):
        code, _, release = self.run_tool()
        self.assertEqual(code, 0)
        self.installed.write_bytes(b'later rollout')
        events = list(self.host.events)
        code, result, _ = self.run_tool('rollback', release=release)
        self.assertEqual((code, result['failure']), (1, 'installed_binary_belongs_to_another_rollout'))
        receipt = json.loads(next(release.glob('rollback-*.json')).read_text())
        self.assertEqual((receipt['status'], receipt['productionMutations']), ('REFUSED', False))
        self.assertEqual(self.host.events, events)
        self.assertEqual(self.installed.read_bytes(), b'later rollout')

    def test_operator_rollback_refuses_busy_work_with_a_partial_service_failure(self):
        for failed in self.rollout.SERVICES:
            with self.subTest(failed=failed):
                self.reset()
                code, _, release = self.run_tool()
                self.assertEqual(code, 0)
                self.host.states[failed] = 'failed'
                self.host.work = [{**ZERO_WORK, 'runtime': 1}]
                events = list(self.host.events)
                states = dict(self.host.states)
                code, result, _ = self.run_tool('rollback', release=release)
                self.assertEqual((code, result['failure']), (1, 'active_work_prevents_rollout'))
                receipt = json.loads(next(release.glob('rollback-*.json')).read_text())
                self.assertEqual((receipt['status'], receipt['productionMutations']), ('REFUSED', False))
                self.assertEqual(self.host.events, events)
                self.assertEqual(self.host.states, states)
                self.assertEqual(self.installed.read_bytes(), NEW)

    def test_operator_rollback_rechecks_work_after_stopping_before_replacing_binary(self):
        code, _, release = self.run_tool()
        self.assertEqual(code, 0)
        self.host.work = [ZERO_WORK, {**ZERO_WORK, 'runtime': 1}]
        events = list(self.host.events)
        code, result, _ = self.run_tool('rollback', release=release)
        self.assertEqual((code, result['failure']), (1, 'rollback_failed_manual_recovery_required'))
        receipt = json.loads(next(release.glob('rollback-*.json')).read_text())
        self.assertEqual((receipt['status'], receipt['failureCategory']),
                         ('ROLLBACK_FAILED', 'active_work_prevents_rollout'))
        self.assertEqual(self.host.events, events + ['stop'])
        self.assertEqual(set(self.host.states.values()), {'inactive'})
        self.assertEqual(self.installed.read_bytes(), NEW)

    def test_operator_rollback_of_an_already_inactive_core_keeps_manual_recovery_available(self):
        code, _, release = self.run_tool()
        self.assertEqual(code, 0)
        self.host.states = {service: 'inactive' for service in self.rollout.SERVICES}
        for service in self.rollout.SERVICES:
            self.host.population(service, False)
        # An assessed, stopped core can have persisted work awaiting recovery by the previous binary.
        self.host.work = [{**ZERO_WORK, 'runtime': 1}]
        code, result, _ = self.run_tool('rollback', release=release)
        self.assertEqual((code, result['status']), (0, 'ROLLED_BACK'))
        self.assertEqual(self.installed.read_bytes(), OLD)

    def rollback_receipt(self, release):
        return json.loads(next(release.glob('rollback-*.json')).read_text())

    def reset_failed_commands(self, record):
        return [entry['command'] for entry in record['commands'] if 'reset-failed' in entry['command']]

    def test_operator_rollback_with_a_failed_unit_checks_the_live_peer_and_completes(self):
        # systemctl stop leaves the failed unit failed; rollback resets it instead of giving up.
        for failed in self.rollout.SERVICES:
            with self.subTest(failed=failed):
                self.reset()
                code, _, release = self.run_tool()
                self.assertEqual(code, 0)
                self.host.states[failed] = 'failed'
                events = list(self.host.events)
                code, result, _ = self.run_tool('rollback', release=release)
                self.assertEqual((code, result['status']), (0, 'ROLLED_BACK'))
                receipt = self.rollback_receipt(release)
                self.assertEqual((receipt['idleBeforeStop'], receipt['idleAfterStop']), (ZERO_WORK, ZERO_WORK))
                self.assertEqual(self.host.events, events + ['stop', 'reset-failed', 'start'])
                self.assertEqual(self.reset_failed_commands(receipt), [['sudo', '-n', 'systemctl', 'reset-failed', failed]])
                self.assertEqual(set(self.host.states.values()), {'active'})
                self.assertEqual(self.installed.read_bytes(), OLD)

    def test_operator_rollback_of_an_entirely_failed_core_uses_the_stopped_recovery_path(self):
        code, _, release = self.run_tool()
        self.assertEqual(code, 0)
        self.host.states = {service: 'failed' for service in self.rollout.SERVICES}
        for service in self.rollout.SERVICES:
            self.host.population(service, False)
        # No unit runs: persisted work waits for the previous binary, as for an inactive core.
        self.host.work = [{**ZERO_WORK, 'runtime': 1}]
        events = list(self.host.events)
        code, result, _ = self.run_tool('rollback', release=release)
        self.assertEqual((code, result['status']), (0, 'ROLLED_BACK'))
        receipt = self.rollback_receipt(release)
        self.assertNotIn('idleBeforeStop', receipt)
        self.assertEqual(self.host.events, events + ['stop', 'reset-failed', 'start'])
        self.assertEqual(self.reset_failed_commands(receipt), [['sudo', '-n', 'systemctl', 'reset-failed', *self.rollout.SERVICES]])
        self.assertEqual(self.installed.read_bytes(), OLD)

    def test_apply_continues_when_stopping_leaves_a_unit_failed(self):
        # A stop timeout kills the unit and leaves it failed; nothing runs, so the rollout proceeds.
        self.host.fail_on_stop = {'miy-v2-worker'}
        code, result, release = self.run_tool()
        self.assertEqual((code, result['status']), (0, 'PASS'))
        self.assertEqual(self.host.events, ['stop', 'reset-failed', 'backup', 'verify', 'start'])
        record = json.loads((release / 'deployment.json').read_text())
        self.assertEqual(self.reset_failed_commands(record), [['sudo', '-n', 'systemctl', 'reset-failed', 'miy-v2-worker']])
        self.assertEqual(self.installed.read_bytes(), NEW)

    def test_crashed_candidate_unit_is_stopped_for_assessment_and_rolled_back(self):
        self.host.crash_candidate = {'miy-v2-worker'}
        code, result, release = self.run_tool()
        self.assertEqual((code, result['failure']), (1, 'post_start_failed_manual_assessment_required'))
        record = json.loads((release / 'deployment.json').read_text())
        self.assertEqual((record['status'], record['failureCategory']), ('POST_START_FAILED', 'post_deployment_health_failed'))
        self.assertTrue(record['coreStoppedForAssessment'])
        self.assertEqual(self.host.events[-3:], ['start', 'stop', 'reset-failed'])
        self.assertEqual(set(self.host.states.values()), {'inactive'})
        self.assertEqual(self.installed.read_bytes(), NEW)
        code, result, _ = self.run_tool('rollback', release=release)
        self.assertEqual((code, result['status']), (0, 'ROLLED_BACK'))
        self.assertEqual(self.host.events[-2:], ['stop', 'start'])
        self.assertEqual(set(self.host.states.values()), {'active'})
        self.assertEqual(self.installed.read_bytes(), OLD)

    def assert_stop_refused_without_replacement(self, failure):
        code, _, release = self.run_tool()
        self.assertEqual(code, 1)
        record = json.loads((release / 'deployment.json').read_text())
        self.assertEqual(record['failureCategory'], failure)
        self.assertNotIn('backup', self.host.events)
        self.assertNotIn('start', self.host.events)
        self.assertEqual(self.installed.read_bytes(), OLD)

    def test_failed_stop_with_residual_descendants_never_resets_or_publishes(self):
        worker = 'miy-v2-worker'
        self.host.fail_on_stop = {worker}
        self.host.retain_processes = {worker}

        def descendant():
            directory = self.rollout.CGROUP_ROOT / 'system.slice' / (worker + '.service')
            (directory / 'cgroup.procs').write_text('')
            (directory / 'build').mkdir(exist_ok=True)
            (directory / 'build/cgroup.procs').write_text('456\n')
            self.host.process_properties[worker] = {'ControlGroup': ''}

        self.host.on_stop = descendant
        self.assert_stop_refused_without_replacement('core_processes_remaining')
        self.assertNotIn('reset-failed', self.host.events)

    def test_stop_checks_both_pid_fields_even_when_the_cgroup_is_empty(self):
        worker = 'miy-v2-worker'
        for key in ('MainPID', 'ControlPID'):
            with self.subTest(key=key):
                self.reset()
                self.host.fail_on_stop = {worker}
                self.host.on_stop = lambda: self.host.process_properties.update({worker: {key: '456'}})
                self.assert_stop_refused_without_replacement('core_processes_remaining')
                self.assertNotIn('reset-failed', self.host.events)

    def test_stop_unknown_cgroup_evidence_is_not_empty(self):
        worker = 'miy-v2-worker'
        for content in (None, '', 'frozen 0\n', 'populated unknown\n',
                        'populated 0\npopulated 1\n', 'populated 0 extra\n'):
            with self.subTest(content=content):
                self.reset()
                self.host.fail_on_stop = {worker}
                path = self.rollout.CGROUP_ROOT / 'system.slice' / (worker + '.service/cgroup.events')
                self.host.on_stop = lambda: path.unlink() if content is None else path.write_text(content)
                self.assert_stop_refused_without_replacement('core_process_state_unknown')
                self.assertNotIn('reset-failed', self.host.events)

    def test_stop_refuses_unreadable_cgroup_evidence(self):
        path = self.rollout.CGROUP_ROOT / 'system.slice/miy-v2-worker.service/cgroup.events'
        original = Path.read_text

        def read(file, *args, **kwargs):
            if file == path and 'stop' in self.host.events:
                raise PermissionError('fixture denied')
            return original(file, *args, **kwargs)

        with patch.object(Path, 'read_text', read):
            self.assert_stop_refused_without_replacement('core_process_state_unknown')

    def test_reset_failed_is_followed_by_another_real_process_check(self):
        worker = 'miy-v2-worker'
        self.host.fail_on_stop = {worker}

        def unexpected_process():
            self.host.population(worker, True)
            self.host.retain_processes.add(worker)
            self.host.process_properties[worker] = {'ControlGroup': ''}

        self.host.on_reset_failed = unexpected_process
        self.assert_stop_refused_without_replacement('core_processes_remaining')
        self.assertEqual(self.host.events.count('reset-failed'), 1)

    def test_rollback_of_failed_or_inactive_core_requires_empty_cgroups_before_mutation(self):
        for state in ('failed', 'inactive'):
            with self.subTest(state=state):
                self.reset()
                code, _, release = self.run_tool()
                self.assertEqual(code, 0)
                self.host.states = {service: state for service in self.rollout.SERVICES}
                self.host.process_properties = {s: {'ControlGroup': ''} for s in self.rollout.SERVICES}
                events = list(self.host.events)
                code, result, _ = self.run_tool('rollback', release=release)
                self.assertEqual((code, result.get('failure')), (1, 'core_processes_remaining'))
                record = self.rollback_receipt(release)
                self.assertEqual((record['status'], record['productionMutations']), ('REFUSED', False))
                self.assertEqual(self.host.events, events)
                self.assertEqual(self.installed.read_bytes(), NEW)

    def test_stopped_core_with_removed_cgroups_and_empty_properties_can_recover(self):
        code, _, release = self.run_tool()
        self.assertEqual(code, 0)

        def removed():
            for service in self.rollout.SERVICES:
                directory = self.rollout.CGROUP_ROOT / 'system.slice' / (service + '.service')
                shutil.rmtree(directory)
                self.host.process_properties[service] = {'ControlGroup': ''}

        self.host.states = {service: 'failed' for service in self.rollout.SERVICES}
        removed()
        self.host.on_stop = removed
        self.host.work = [{**ZERO_WORK, 'runtime': 1}]
        code, result, _ = self.run_tool('rollback', release=release)
        self.assertEqual((code, result['status']), (0, 'ROLLED_BACK'))
        self.assertEqual(self.installed.read_bytes(), OLD)

    def test_missing_parent_or_v2_marker_is_not_a_removed_service_cgroup(self):
        for relative in ('cgroup.controllers', 'system.slice/cgroup.events', 'system.slice'):
            with self.subTest(relative=relative):
                self.reset()
                path = self.rollout.CGROUP_ROOT / relative
                if path.is_dir():
                    shutil.rmtree(path)
                else:
                    path.unlink()
                code, result, _ = self.run_tool()
                self.assertEqual((code, result['failure']), (1, 'core_process_state_unknown'))
                self.assert_untouched()

    def test_unknown_unit_identity_or_cgroup_layout_is_refused_before_stop(self):
        for fields in ({'Id': 'other.service'}, {'Slice': 'other.slice'}, {'LoadState': 'not-found'},
                       {'ControlGroup': '/system.slice/other.service'}, {'ControlGroup': '/'},
                       {'ControlGroup': '/system.slice/../other.service'}):
            with self.subTest(fields=fields):
                self.reset()
                self.host.process_properties['miy-v2-worker'] = fields
                code, result, _ = self.run_tool()
                self.assertEqual((code, result['failure']), (1, 'core_cgroup_layout_unsupported'))
                self.assert_untouched()

    def test_pid_and_unit_property_errors_are_not_assumed_to_be_zero(self):
        for fields in ({'MainPID': ''}, {'ControlPID': 'unknown'}, {'MainPID': '-1'},
                       {'MainPID': '0\nMainPID=123'}):
            with self.subTest(fields=fields):
                self.reset()
                self.host.process_properties['miy-v2-worker'] = fields
                code, result, _ = self.run_tool()
                self.assertEqual((code, result['failure']), (1, 'core_process_state_unknown'))
                self.assert_untouched()

    def test_cgroup_symlinks_are_refused_before_stop(self):
        path = self.rollout.CGROUP_ROOT / 'system.slice/miy-v2-worker.service'
        target = self.root / 'empty-cgroup'
        target.mkdir()
        (target / 'cgroup.events').write_text('populated 0\n')
        shutil.rmtree(path)
        path.symlink_to(target, target_is_directory=True)
        code, result, _ = self.run_tool()
        self.assertEqual((code, result['failure']), (1, 'symlink_path_refused'))
        self.assert_untouched()

    def test_safety_require_survives_optimized_python(self):
        path = ROOT / 'scripts/core-rollout.py'
        module = ModuleType('optimized_rollout')
        module.__file__ = str(path)
        exec(compile(path.read_text(), module.__file__, 'exec', optimize=2), module.__dict__)
        with self.assertRaisesRegex(RuntimeError, 'precondition_failed'):
            module.require(False, 'precondition_failed')


class UiRolloutTests(OfflineTest):
    """scripts/ui-rollout.py: builds outside the live dist; apply adds, never replaces, assets."""

    def setUp(self):
        super().setUp()
        self.ui = load('ui_rollout_under_test', ROOT / 'scripts/ui-rollout.py')
        self.ui.ROOT = self.root
        self.live = self.root / 'web/platform/dist'
        (self.live / 'assets').mkdir(parents=True)
        (self.live / 'assets/old.js').write_text('old')
        (self.live / 'index.html').write_text('<script type="module" src="/assets/old.js"></script>')
        (self.root / 'web/platform/src').mkdir(parents=True)
        (self.root / 'web/platform/src/main.tsx').write_text('main')
        (self.root / 'package.json').write_text('{}')
        self.release = self.root / '.local/ui-release'
        self.commands = []
        self.inputs_dirty = ''
        self.git_show = None
        self.tracked = ['package.json', 'web/platform/src/main.tsx']
        self.during_build = None

    def process(self, argv, **kwargs):
        self.commands.append(argv)
        output = ''
        if argv[0] == 'git':
            if 'rev-parse' in argv:
                output = COMMIT + '\n'
            elif 'status' in argv:
                output = self.inputs_dirty
            elif 'ls-files' in argv:
                output = '\0'.join(self.tracked) + '\0'
            elif 'show' in argv:
                name = argv[-1].split(':', 1)[1]
                content = self.git_show or (self.root / name).read_bytes()
                return subprocess.CompletedProcess(argv, 0, content, b'')
        elif argv[:2] == ['pnpm', '--filter']:
            out = Path(argv[argv.index('--outDir') + 1])
            (out / 'assets').mkdir(parents=True)
            (out / 'assets/old.js').write_text('old')
            (out / 'assets/new.js').write_text('new')
            (out / 'index.html').write_text('<script type="module" src="/assets/new.js"></script>')
            if self.during_build:
                self.during_build()
        elif argv[1:] == ['--version']:
            output = 'fixture'
        else:
            self.fail('unexpected process: ' + ' '.join(argv))
        return subprocess.CompletedProcess(argv, 0, output, '')

    def tool(self, *argv):
        output = io.StringIO()
        with patch.object(subprocess, 'run', side_effect=self.process), contextlib.redirect_stdout(output):
            code = self.ui.main([*argv, '--release-dir', str(self.release)])
        return code, json.loads(output.getvalue())

    def test_build_apply_and_rollback_keep_both_asset_generations(self):
        old_index = (self.live / 'index.html').read_bytes()
        code, built = self.tool('build', '--commit', COMMIT)
        self.assertEqual((code, built['newAssets']), (0, 1))
        manifest = json.loads((self.release / 'manifest.json').read_text())
        self.assertEqual(sorted(manifest['sourceFiles']), ['package.json', 'web/platform/src/main.tsx'])
        self.assertEqual(manifest['expectedLiveIndexSha256'], hashlib.sha256(old_index).hexdigest())
        code, inspected = self.tool('inspect')
        self.assertEqual((code, inspected['newAssets']), (0, ['assets/new.js']))
        self.assertEqual((self.live / 'index.html').read_bytes(), old_index)
        code, applied = self.tool('apply', '--expected-commit', COMMIT)
        self.assertEqual((code, applied['phase']), (0, 'applied'))
        self.assertIn('new.js', (self.live / 'index.html').read_text())
        self.assertEqual((self.live / 'assets/old.js').read_text(), 'old')
        self.assertEqual((self.release / 'production-backup/dist/index.html').read_bytes(), old_index)
        code, rolled = self.tool('rollback')
        self.assertEqual((code, rolled['phase']), (0, 'rolled_back'))
        self.assertEqual((self.live / 'index.html').read_bytes(), old_index)
        self.assertEqual((self.live / 'assets/new.js').read_text(), 'new')

    def test_build_refuses_vite_environment_and_dirty_inputs_before_building(self):
        with patch.dict(os.environ, {'VITE_API_ORIGIN': 'https://elsewhere.invalid'}):
            code, result = self.tool('build', '--commit', COMMIT)
        self.assertEqual(code, 1)
        self.assertIn('VITE_API_ORIGIN', result['error'])
        self.assertEqual(self.commands, [])
        self.inputs_dirty = ' M web/platform/src/main.tsx\n'
        code, result = self.tool('build', '--commit', COMMIT)
        self.assertEqual((code, result['error']), (1, 'build inputs are not clean'))
        self.assertFalse(any(argv[0] == 'pnpm' for argv in self.commands))
        self.assertFalse((self.release / 'dist').exists())

    def test_release_directory_must_be_under_private_state(self):
        self.release = self.root / 'elsewhere'
        code, result = self.tool('build', '--commit', COMMIT)
        self.assertEqual((code, result['error']), (1, 'release directory must be under .local'))
        self.assertFalse(self.release.exists())

    def test_build_refuses_untracked_public_files_even_when_git_status_is_clean(self):
        public = self.root / 'web/platform/public'
        public.mkdir()
        for name in ('.env', 'debug.log', 'untracked.txt'):
            with self.subTest(name=name):
                path = public / name
                path.write_text('NONSECRET_REVIEW_FIXTURE')
                code, result = self.tool('build', '--commit', COMMIT)
                self.assertEqual((code, result['error']),
                                 (1, 'untracked public build input: web/platform/public/' + name))
                self.assertFalse(any(argv[0] == 'pnpm' for argv in self.commands))
                self.assertFalse((self.release / 'manifest.json').exists())
                path.unlink()

    def test_build_refuses_public_directory_and_nested_symlinks(self):
        public = self.root / 'web/platform/public'
        outside = self.root / 'not-public'
        outside.mkdir()
        (outside / 'private.txt').write_text('NONSECRET_REVIEW_FIXTURE')
        public.symlink_to(outside, target_is_directory=True)
        code, result = self.tool('build', '--commit', COMMIT)
        self.assertEqual(code, 1)
        self.assertIn('symlink path refused', result['error'])
        public.unlink()
        public.mkdir()
        for target in (outside, outside / 'private.txt', outside / 'missing'):
            with self.subTest(target=target.name):
                link = public / 'linked'
                link.symlink_to(target, target_is_directory=target.is_dir())
                code, result = self.tool('build', '--commit', COMMIT)
                self.assertEqual(code, 1)
                self.assertIn('symlink path refused', result['error'])
                link.unlink()
        self.assertFalse(any(argv[0] == 'pnpm' for argv in self.commands))

    def test_build_accepts_regular_tracked_public_files_in_the_source_manifest(self):
        public = self.root / 'web/platform/public'
        public.mkdir()
        (public / 'logo.svg').write_text('<svg/>')
        self.tracked.append('web/platform/public/logo.svg')
        self.assertEqual(self.tool('build', '--commit', COMMIT)[0], 0)
        manifest = json.loads((self.release / 'manifest.json').read_text())
        self.assertEqual(manifest['sourceFiles']['web/platform/public/logo.svg'],
                         hashlib.sha256(b'<svg/>').hexdigest())

    def test_build_refuses_production_dotenv_files_including_tracked_files(self):
        for directory in (self.root, self.root / 'web/platform'):
            for name in ('.env', '.env.local', '.env.production', '.env.production.local'):
                with self.subTest(directory=directory.name, name=name):
                    path = directory / name
                    path.write_text('NONSECRET_REVIEW_FIXTURE')
                    relative = path.relative_to(self.root).as_posix()
                    self.tracked.append(relative)
                    code, result = self.tool('build', '--commit', COMMIT)
                    self.assertEqual((code, result['error']), (1, 'dotenv build input refused: ' + relative))
                    self.assertFalse(any(argv[0] == 'pnpm' for argv in self.commands))
                    self.tracked.remove(relative)
                    path.unlink()

    def test_build_rechecks_public_and_dotenv_inputs_after_vite(self):
        public = self.root / 'web/platform/public'
        public.mkdir()
        for relative, error in (
                ('web/platform/public/.env', 'untracked public build input: web/platform/public/.env'),
                ('web/platform/.env.production.local', 'dotenv build input refused: web/platform/.env.production.local')):
            with self.subTest(path=relative):
                self.release = self.root / ('.local/post-build-' + str(len(self.commands)))
                path = self.root / relative
                self.during_build = lambda: path.write_text('NONSECRET_REVIEW_FIXTURE')
                code, result = self.tool('build', '--commit', COMMIT)
                self.assertEqual((code, result['error']), (1, error))
                self.assertFalse((self.release / 'manifest.json').exists())
                path.unlink()

    def test_build_refuses_changed_tracked_bytes_even_if_status_stays_clean(self):
        self.during_build = lambda: (self.root / 'web/platform/src/main.tsx').write_text('changed')
        code, result = self.tool('build', '--commit', COMMIT)
        self.assertEqual((code, result['error']), (1, 'build input bytes changed during build'))
        self.assertFalse((self.release / 'manifest.json').exists())

    def test_build_checks_source_bytes_against_the_commit_before_vite(self):
        self.git_show = b'not the committed file'
        code, result = self.tool('build', '--commit', COMMIT)
        self.assertEqual((code, result['error']), (1, 'commit build input mismatch: package.json'))
        self.assertFalse(any(argv[0] == 'pnpm' for argv in self.commands))

    def assert_common_publication_lock(self, competing_action, phase):
        second = self.root / '.local/second-release'
        self.release = second
        self.assertEqual(self.tool('build', '--commit', COMMIT)[0], 0)
        if competing_action == 'rollback':
            self.assertEqual(self.tool('apply', '--expected-commit', COMMIT)[0], 0)
        first = self.root / '.local/first-release'
        self.release = first
        self.assertEqual(self.tool('build', '--commit', COMMIT)[0], 0)
        entered, finish = threading.Event(), threading.Event()
        original = getattr(self.ui, phase)

        def paused(*args):
            should_pause = ((phase == 'save' and args[0] == first / 'production-backup/receipt.json')
                            or (phase == 'check' and args[0] == first / 'dist'))
            if should_pause:
                entered.set()
                self.assertTrue(finish.wait(5), 'publication test timed out')
            return original(*args)

        def arguments(action, release):
            return SimpleNamespace(action=action, release_dir=release, manifest=None, candidate=None,
                                   live=self.live, backup=None, expected_commit=COMMIT)

        # Real flock on separate file descriptors, with two distinct release directories.
        # Only subprocesses (git inspection) are mocked; index/backup/receipt writes are real temporary files.
        with patch.object(subprocess, 'run', side_effect=self.process), patch.object(self.ui, phase, side_effect=paused):
            with ThreadPoolExecutor(max_workers=1) as executor:
                running = executor.submit(self.ui.run, arguments('apply', first))
                try:
                    self.assertTrue(entered.wait(5), 'first publication did not reach the guarded phase')
                    before = self.ui.inventory(self.live)
                    with self.assertRaisesRegex(RuntimeError, 'live target is locked by another UI rollout'):
                        self.ui.run(arguments(competing_action, second))
                    self.assertEqual(self.ui.inventory(self.live), before)
                    self.assertFalse((second / 'production-backup/rollback-receipt.json').exists())
                    if competing_action == 'apply':
                        self.assertFalse((second / 'production-backup').exists())
                finally:
                    finish.set()
                self.assertEqual(running.result(timeout=5)['phase'], 'applied')
            if competing_action == 'rollback':
                self.assertEqual(self.ui.run(arguments('rollback', second))['phase'], 'rolled_back')

    def test_common_live_lock_blocks_other_release_apply_before_check(self):
        self.assert_common_publication_lock('apply', 'check')

    def test_common_live_lock_blocks_other_release_apply_through_receipt(self):
        self.assert_common_publication_lock('apply', 'save')

    def test_common_live_lock_blocks_other_release_rollback_before_check(self):
        self.assert_common_publication_lock('rollback', 'check')

    def test_common_live_lock_blocks_other_release_rollback_through_receipt(self):
        self.assert_common_publication_lock('rollback', 'save')

    def test_apply_refusals_leave_the_live_dist_unchanged(self):
        self.assertEqual(self.tool('build', '--commit', COMMIT)[0], 0)
        before = self.ui.inventory(self.live)
        (self.live / 'assets/new.js').write_text('different bytes under the same name')
        code, result = self.tool('apply', '--expected-commit', COMMIT)
        self.assertEqual((code, result['error']), (1, 'immutable asset collision: assets/new.js'))
        (self.live / 'assets/new.js').unlink()
        self.assertEqual(self.ui.inventory(self.live), before)
        (self.live / 'index.html').write_text('<script type="module" src="/assets/old.js"></script><!-- other -->')
        code, result = self.tool('apply', '--expected-commit', COMMIT)
        self.assertEqual((code, result['error']), (1, 'live index differs from build baseline'))
        (self.live / 'index.html').write_text('<script type="module" src="/assets/old.js"></script>')
        self.git_show = b'not the committed file'
        code, result = self.tool('apply', '--expected-commit', COMMIT)
        self.assertEqual((code, result['error']), (1, 'commit build input mismatch: package.json'))
        self.git_show = None
        code, result = self.tool('apply', '--expected-commit', 'e' * 40)
        self.assertEqual((code, result['error']), (1, 'source commit does not match approved manifest'))
        self.assertEqual(self.ui.inventory(self.live), before)
        self.assertFalse((self.release / 'production-backup').exists())

    def test_second_apply_never_reuses_a_backup(self):
        self.assertEqual(self.tool('build', '--commit', COMMIT)[0], 0)
        self.assertEqual(self.tool('apply', '--expected-commit', COMMIT)[0], 0)
        backup = (self.release / 'production-backup/dist/index.html').read_bytes()
        code, result = self.tool('apply', '--expected-commit', COMMIT)
        self.assertEqual(code, 1)
        self.assertEqual((self.release / 'production-backup/dist/index.html').read_bytes(), backup)

    def test_rollback_refuses_an_index_from_another_deployment(self):
        self.assertEqual(self.tool('build', '--commit', COMMIT)[0], 0)
        self.assertEqual(self.tool('apply', '--expected-commit', COMMIT)[0], 0)
        (self.live / 'index.html').write_text('<script type="module" src="/assets/old.js"></script><!-- later -->')
        code, result = self.tool('rollback')
        self.assertEqual((code, result['error']), (1, 'live index belongs to another deployment'))
        self.assertIn('later', (self.live / 'index.html').read_text())


if __name__ == '__main__':
    unittest.main(verbosity=2)
