#!/usr/bin/env python3
"""Offline safety regression for core backup, capacity reports and rollout helpers.

Every process invocation is mocked. Never calls Docker, systemd, a server binary or a database.
"""
import contextlib
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from types import ModuleType, SimpleNamespace
import unittest
from unittest.mock import Mock, patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
ROLLOUT = ROOT / 'docs/releases/evidence/core-0.7.0-7d3c66a-linux'


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


class RolloutSafetyTests(OfflineTest):
    def setUp(self):
        super().setUp()
        self.deploy = load('rollout_under_test', ROLLOUT / 'deploy.py')
        self.deploy.LIVE = self.root
        (self.root / '.local/bin').mkdir(parents=True)
        self.deploy.REL = self.root / '.local/release'
        self.deploy.BACKUP = self.deploy.REL / 'rollback'
        self.deploy.REPORT = {'commands': [], 'rollbackUsed': False, 'databaseRestored': False}

    def test_existing_recovery_data_is_not_overwritten(self):
        self.deploy.BACKUP.mkdir(parents=True)
        marker = self.deploy.BACKUP / 'miy_core.dump'
        marker.write_bytes(b'previous recovery dump')
        with self.assertRaises(FileExistsError), self.deploy.rollout_lock():
            self.fail('must reject existing recovery data')
        self.assertEqual(marker.read_bytes(), b'previous recovery dump')
        self.no_process.assert_not_called()

    def test_concurrent_rollout_is_rejected_and_private_state_is_private(self):
        with self.deploy.rollout_lock():
            self.deploy.save()
            self.assertEqual(self.deploy.BACKUP.stat().st_mode & 0o777, 0o700)
            self.assertEqual((self.deploy.REL / 'deployment.json').stat().st_mode & 0o777, 0o600)
            with self.assertRaises(BlockingIOError), self.deploy.rollout_lock():
                self.fail('second rollout must not acquire ownership')
        self.no_process.assert_not_called()

    def test_failed_stop_prevents_binary_and_database_rollback(self):
        with patch.object(self.deploy, 'save'), \
             patch.object(self.deploy, 'call', side_effect=RuntimeError('stop failed')), \
             patch.object(self.deploy, 'atomic_copy') as copy, \
             patch.object(self.deploy, 'restore_database') as restore:
            with self.assertRaisesRegex(RuntimeError, 'stop failed'):
                self.deploy.restore(True)
            copy.assert_not_called()
            restore.assert_not_called()

    def test_stop_success_with_active_service_also_blocks_rollback(self):
        with patch.object(self.deploy, 'save'), \
             patch.object(self.deploy, 'call', side_effect=['', 'active']), \
             patch.object(self.deploy, 'atomic_copy') as copy, \
             patch.object(self.deploy, 'restore_database') as restore:
            with self.assertRaisesRegex(RuntimeError, 'core_not_stopped'):
                self.deploy.restore(True)
            copy.assert_not_called()
            restore.assert_not_called()

    def test_active_serve_does_not_hide_failed_worker(self):
        with patch.object(self.deploy, 'call', side_effect=['active', 'failed']):
            with self.assertRaisesRegex(RuntimeError, 'core_not_active'):
                self.deploy.require_core_state('active')

    def test_restore_blocks_reconnections_before_terminating_and_renaming(self):
        self.fixture_backup()
        run = Mock(return_value='')
        kept = self.deploy.restore_database(run, self.root / 'miy_core.dump', container='fixture-db')
        commands = [call.args[0] for call in run.call_args_list]
        self.assertEqual(commands[0][-1], 'ALTER DATABASE miy_core ALLOW_CONNECTIONS false')
        self.assertIn('pg_terminate_backend', commands[1][-1])
        self.assertEqual(commands[2][-1], 'ALTER DATABASE miy_core RENAME TO ' + kept)
        self.assertTrue(all(command[2] == 'fixture-db' for command in commands[:-1]))
        self.assertIn('--single-transaction', commands[-1])
        self.assertTrue(kept.startswith('miy_core_failed_'))
        self.no_process.assert_not_called()

    def test_invalid_database_name_is_rejected_before_commands(self):
        run = Mock()
        with self.assertRaisesRegex(RuntimeError, 'invalid_database_name'):
            self.deploy.restore_database(run, self.root / 'missing', db='core; DROP DATABASE postgres')
        run.assert_not_called()

    def test_database_properties_require_exact_restore_contract(self):
        expected = self.deploy.EXPECTED_DB_PROPERTIES
        with patch.object(self.deploy, 'sql', return_value=json.dumps(expected)) as sql:
            self.deploy.require_database_properties()
            query = sql.call_args.args[1]
            self.assertIn("acldefault('d', datdba)", query)
            self.assertIn('WHERE grantee=0', query)
        for field, value in [('owner', 'other'), ('encoding', 'LATIN1'), ('collate', 'C'),
                             ('ctype', 'C'), ('provider', 'i'), ('publicRevoked', False)]:
            with self.subTest(field=field), patch.object(self.deploy, 'sql', return_value=json.dumps({**expected, field: value})):
                with self.assertRaisesRegex(RuntimeError, 'unexpected_database_properties'):
                    self.deploy.require_database_properties()
        self.no_process.assert_not_called()

    def test_database_property_mismatch_prevents_service_stop(self):
        self.deploy.BACKUP.mkdir(parents=True)
        candidate = self.root / 'candidate'
        candidate.write_bytes(b'reviewed candidate')
        with patch.object(self.deploy, 'call', return_value='miy 0.7.0'), \
             patch.object(self.deploy, 'migrations', return_value='17'), \
             patch.object(self.deploy, 'sql', return_value=json.dumps(None)), \
             patch.object(self.deploy, 'stop_core') as stop, \
             patch.object(self.deploy, 'restore') as restore:
            with self.assertRaisesRegex(RuntimeError, 'unexpected_database_properties'):
                self.deploy.deploy(self.deploy.SERVER_COMMIT, candidate, self.deploy.digest(candidate))
            stop.assert_not_called()
            restore.assert_not_called()
        self.no_process.assert_not_called()

    def test_failed_database_restore_does_not_install_or_start_services(self):
        self.deploy.REPORT.update({'backupVerified': True, 'databaseBackup': {'sha256': 'fixture'}})
        with patch.object(self.deploy, 'save'), patch.object(self.deploy, 'stop_core'), \
             patch.object(self.deploy, 'digest', return_value='fixture'), \
             patch.object(self.deploy, 'restore_database', side_effect=RuntimeError('restore failed')), \
             patch.object(self.deploy, 'atomic_copy') as copy, patch.object(self.deploy, 'call') as call:
            with self.assertRaisesRegex(RuntimeError, 'restore failed'):
                self.deploy.restore(True)
            copy.assert_not_called()
            call.assert_not_called()

    def test_failed_separate_restore_check_blocks_all_live_database_changes(self):
        candidate = self.root / 'candidate'
        candidate.write_bytes(b'reviewed candidate')
        (self.root / '.local/bin/miy-server').write_bytes(b'old binary')
        commands = []

        def call(argv, **kwargs):
            commands.append(argv)
            if argv[-1] == '--version':
                return 'miy 0.7.0'
            if 'compose-diff.py' in argv[-1]:
                return json.dumps({'keycloak': {'differs': ['env:KC_DB_POOL_MAX_SIZE']},
                                   'postgres': {'differs': ['command', 'networks']}})
            if argv[:4] == ['sudo', '-n', 'systemctl', 'stop']:
                return ''
            if argv[:2] == ['systemctl', 'show']:
                return 'inactive'
            if len(argv) > 2 and argv[2] == 'create':
                self.fixture_backup(Path(argv[-1]))
                return '{}'
            if len(argv) > 2 and argv[2] == 'verify':
                return json.dumps({'status': 'FAIL', 'containerRemoved': False})
            self.fail('unexpected operation before backup verification')

        guide = {'cliVersion': '0.7.0', 'sourceCommit': self.deploy.CLI_COMMIT,
                 'windowsInstaller': {'sha256': 'installer-fixture'}}
        with self.deploy.rollout_lock(), patch.object(self.deploy, 'call', side_effect=call), \
             patch.object(self.deploy, 'migrations', return_value='17'), \
             patch.object(self.deploy, 'require_database_properties'), \
             patch.object(self.deploy, 'get_json', return_value=guide), \
             patch.object(self.deploy, 'networks', return_value=['fixture']), \
             patch.object(self.deploy, 'container_id', return_value=CID), \
             patch.object(self.deploy, 'restore') as restore:
            with self.assertRaisesRegex(RuntimeError, 'backup_restore_not_verified'):
                self.deploy.deploy(self.deploy.SERVER_COMMIT, candidate, self.deploy.digest(candidate))
            restore.assert_called_once_with(False)
        self.assertFalse(any(argv[0] == 'docker' or argv[0] == 'bash' for argv in commands))
        self.assertEqual((self.deploy.BACKUP / 'miy-server').read_bytes(), b'old binary')

    def test_safety_require_survives_optimized_python(self):
        module = ModuleType('optimized_rollout')
        module.__file__ = str(ROLLOUT / 'deploy.py')
        exec(compile((ROLLOUT / 'deploy.py').read_text(), module.__file__, 'exec', optimize=2), module.__dict__)
        with self.assertRaisesRegex(RuntimeError, 'precondition_failed'):
            module.require(False, 'precondition_failed')

    def test_rehearsal_rejects_changed_dump_before_live_queries(self):
        self.fixture_backup()
        (self.root / 'miy_core.dump').write_bytes(b'changed')
        rehearsal = load('rehearsal_under_test', ROLLOUT / 'rollback-rehearsal.py')
        with patch.object(sys, 'argv', ['rollback-rehearsal.py', str(self.root), '/fixture/binary']), \
             patch.object(rehearsal, 'psql') as sql:
            with self.assertRaisesRegex(RuntimeError, 'dump_hash_mismatch'):
                rehearsal.main()
            sql.assert_not_called()
        self.no_process.assert_not_called()


if __name__ == '__main__':
    unittest.main(verbosity=2)
