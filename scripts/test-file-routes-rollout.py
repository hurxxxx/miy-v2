#!/usr/bin/env python3
"""Offline fault-injection tests: only temporary files, no sudo/Nginx/services/Git."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location('file_routes_rollout', ROOT / 'scripts/file-routes-rollout.py')
tool = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(tool)
COMMIT = 'a' * 40


def configurations():
    block = (b'    location ~ ^/api/v1/example/files {\n'
             b'        client_max_body_size 10m;\n'
             b'        proxy_request_buffering off;\n'
             b'        proxy_pass http://127.0.0.1:18180;\n    }\n')
    before = (b'# fixture\nserver {\n    portal\n}\nserver {\n    console\n    deny '
              + tool.OLD_DENY + b';\n}\nserver {\n    dev\n}\nserver {\n'
              + tool.OLD_COMMENT + b'    access_log off;\n}\n')
    after = before.replace(b'    portal\n', b'    portal\n' + tool.PORTAL_START + block)
    after = after.replace(b'    dev\n', b'    dev\n' + tool.PORTAL_START + block)
    after = after.replace(tool.OLD_COMMENT, tool.FRAME_START + block + tool.NEW_COMMENT)
    return before, after.replace(tool.OLD_DENY, tool.NEW_DENY)


class Interrupted(BaseException):
    """Model process death after a visible write, bypassing exception recovery."""


class FileRoutesRolloutTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix='miy-file-routes-test-'))
        self.state, self.site = self.root / 'state', self.root / 'site.conf'
        self.state.mkdir(mode=0o700)
        self.copy = self.state / 'nginx-miy.conf'
        self.release = self.state / 'release'
        self.source = self.root / 'source'
        (self.source / 'scripts').mkdir(parents=True)
        (self.source / 'scripts/render-server-config.py').write_text('# fixture\n')
        (self.state / 'server.json').write_text('{}\n')
        self.before, self.after = configurations()
        self.site.write_bytes(self.before)
        self.copy.write_bytes(self.before)
        self.events, self.hooks = [], {}
        self.rendered = self.after
        self.patches = contextlib.ExitStack()
        for name, value in [('STATE', self.state), ('SITE', self.site), ('COPY', self.copy)]:
            self.patches.enter_context(patch.object(tool, name, value))
        self.patches.enter_context(patch.object(tool, 'sudo', self.sudo))
        self.patches.enter_context(patch.object(tool, 'git', lambda _source, *args: COMMIT if args[0] == 'rev-parse' else ''))
        self.patches.enter_context(patch.object(tool.subprocess, 'run', side_effect=AssertionError('external process attempted')))

    def tearDown(self):
        self.patches.close()
        shutil.rmtree(self.root)

    def event(self, name):
        self.events.append(name)
        hook = self.hooks.pop(name, None)
        if hook:
            hook()

    def sudo(self, args, code, capture=False):
        args = list(map(str, args))
        if args[:3] == [tool.sys.executable, '-c', tool.INSTALL_CODE]:
            kind = 'backup' if Path(args[4]).name == 'nginx-backup.conf' else 'candidate'
            self.event('install_' + kind)
            self.assertEqual(Path(args[5]), self.site)
            # Exercise the real replacement helper, only replacing its root chown.
            with patch.object(tool.os, 'fchown') as chown:
                tool.atomic_install(*args[4:])
                chown.assert_called_once()
                self.assertEqual(chown.call_args.args[1:], (0, 0))
            self.event('installed_' + kind)
        elif args == ['nginx', '-t']:
            self.event('test')
        elif args == ['systemctl', 'reload', 'nginx']:
            self.event('reload')
        elif len(args) > 2 and args[1] == str(self.source / 'scripts/render-server-config.py'):
            self.event('render')
            work = Path(args[args.index('--state-dir') + 1])
            self.assertNotEqual(work, self.state)
            (work / 'nginx-miy.conf').write_bytes(self.rendered)
        elif args[:2] == ['cat', '--']:
            self.event('render_read')
            return Path(args[2]).read_bytes()
        elif args[:3] == ['rm', '-rf', '--']:
            self.event('render_cleanup')
            self.assertTrue(Path(args[3]).name.startswith('miy-file-routes-render-'))
            shutil.rmtree(args[3])
        else:
            self.fail('unexpected external action: ' + repr(args))

    def prepared(self):
        return tool.prepare(self.source, COMMIT, self.release, '/fixture/certificate', '/fixture/key')

    def legacy(self, status='NGINX_APPLIED', site=None, copy=None):
        self.release.mkdir(exist_ok=True)
        (self.release / 'nginx-backup.conf').write_bytes(self.before)
        (self.release / 'nginx-candidate.conf').write_bytes(self.after)
        record = {'status': status, 'sourceCommit': COMMIT, 'previousSha256': tool.sha(self.before),
                  'nginxSha256': tool.sha(self.after), 'addedBlocks': 3}
        (self.release / tool.RECORD).write_text(json.dumps(record) + '\n')
        self.site.write_bytes(self.after if site is None else site)
        self.copy.write_bytes(self.after if copy is None else copy)
        return record

    def record(self):
        return json.loads((self.release / tool.RECORD).read_bytes())

    def restored(self):
        self.assertEqual(self.site.read_bytes(), self.before)
        self.assertEqual(self.copy.read_bytes(), self.before)
        self.assertEqual((self.release / 'nginx-backup.conf').read_bytes(), self.before)
        self.assertEqual((self.release / 'nginx-candidate.conf').read_bytes(), self.after)

    def fault(self, code='injected_failure'):
        def failure():
            raise RuntimeError(code)
        return failure

    def test_import_never_opens_state_locks_or_runs_external_actions(self):
        spec = importlib.util.spec_from_file_location('file_routes_import_probe', ROOT / 'scripts/file-routes-rollout.py')
        with patch.object(Path, 'open', side_effect=AssertionError('file opened during import')), \
                patch.object(tool.fcntl, 'flock', side_effect=AssertionError('lock during import')), \
                patch.object(tool.os, 'umask', side_effect=AssertionError('umask during import')), \
                patch.object(tool.os, 'open', side_effect=AssertionError('descriptor during import')):
            spec.loader.exec_module(importlib.util.module_from_spec(spec))

    def test_prepare_binds_both_hashes_and_targets_without_installing(self):
        result = self.prepared()
        self.assertEqual(result['status'], 'PREPARED')
        self.assertEqual((result['previousSha256'], result['nginxSha256']),
                         (tool.sha(self.before), tool.sha(self.after)))
        self.assertEqual(result['targets'], {'site': str(self.site), 'copy': str(self.copy)})
        self.restored()
        self.assertEqual(self.events, ['render', 'render_read', 'render_cleanup'])
        for name in ('nginx-backup.conf', 'nginx-candidate.conf', tool.RECORD):
            self.assertEqual((self.release / name).stat().st_mode & 0o777, 0o600)

    def test_prepare_rejects_dirty_source_extra_diff_and_disallowed_block_directives(self):
        with patch.object(tool, 'git', return_value='dirty'), self.assertRaisesRegex(RuntimeError, 'source_not_clean'):
            self.prepared()
        for candidate in (self.after + b'# unrelated\n', self.after.replace(b'proxy_request_buffering off;', b'root /tmp;'),
                          self.after.replace(tool.NEW_DENY, tool.OLD_DENY), self.after.replace(tool.FRAME_START, b'')):
            with self.subTest(candidate=candidate[:30]):
                self.rendered = candidate
                with self.assertRaises(RuntimeError):
                    self.prepared()
                self.assertFalse(self.release.exists())
                self.assertEqual(self.site.read_bytes(), self.before)

    def test_prepare_detects_a_live_change_during_render(self):
        self.hooks['render'] = lambda: self.copy.write_bytes(b'other administrator edit')
        with self.assertRaisesRegex(RuntimeError, 'baseline_changed_during_prepare'):
            self.prepared()
        self.assertEqual(self.copy.read_bytes(), b'other administrator edit')
        self.assertFalse(self.release.exists())

    def test_prepare_backup_or_journal_write_failure_changes_no_live_file(self):
        write = tool.durable
        for name in ('nginx-backup.conf', 'nginx-candidate.conf', tool.RECORD):
            with self.subTest(name=name):
                self.release = self.state / ('release-' + name)

                def fail_write(path, *args, **kwargs):
                    if path == self.release / name:
                        raise OSError('injected persistence failure')
                    return write(path, *args, **kwargs)

                with patch.object(tool, 'durable', fail_write), self.assertRaises(OSError):
                    self.prepared()
                self.assertEqual(self.site.read_bytes(), self.before)
                self.assertEqual(self.copy.read_bytes(), self.before)
                self.assertFalse(any(event.startswith('install') for event in self.events))

    def test_apply_records_intent_before_install_and_keeps_verified_backups(self):
        self.prepared()
        self.events.clear()
        self.hooks['install_candidate'] = lambda: self.assertEqual(self.record()['status'], 'APPLYING')
        result = tool.apply(self.release)
        self.assertEqual(result['status'], 'NGINX_APPLIED')
        self.assertEqual(self.site.read_bytes(), self.after)
        self.assertEqual(self.copy.read_bytes(), self.after)
        self.assertEqual((self.release / 'nginx-backup.conf').read_bytes(), self.before)
        self.assertEqual(self.events, ['install_candidate', 'installed_candidate', 'test', 'reload'])
        with self.assertRaisesRegex(RuntimeError, 'release_not_prepared'):
            tool.apply(self.release)

    def test_legacy_restore_accepts_all_owned_mixed_states_and_completed_retries(self):
        for status in ('PREPARED', 'APPLYING', 'FAIL', 'NGINX_APPLIED', 'ROLLED_BACK', 'RESTORED', 'RESTORE_FAILED'):
            for site, copy in ((self.before, self.before), (self.before, self.after),
                               (self.after, self.before), (self.after, self.after)):
                with self.subTest(status=status, site=site == self.after, copy=copy == self.after):
                    self.legacy(status, site, copy)
                    result = tool.restore(self.release)
                    self.assertEqual(result['status'], 'RESTORED')
                    self.restored()
                    result = tool.restore(self.release)
                    self.assertEqual(result['recovery']['attempt'], 2)
                    self.restored()

    def test_restore_install_test_and_reload_failures_are_recorded_and_retryable(self):
        for event, step in [('install_backup', 'installing_backup'), ('installed_backup', 'installing_backup'),
                            ('test', 'testing_backup'), ('reload', 'reloading_backup')]:
            with self.subTest(event=event):
                self.legacy()
                self.hooks[event] = self.fault(event)
                with self.assertRaisesRegex(RuntimeError, event):
                    tool.restore(self.release)
                failed = self.record()
                self.assertEqual(failed['status'], 'RESTORE_FAILED')
                self.assertEqual(failed['recovery']['step'], step)
                self.assertEqual(failed['recovery']['failure'], event)
                self.assertEqual(tool.restore(self.release)['status'], 'RESTORED')
                self.assertIn({'action': 'restore', 'step': step, 'failure': event}, self.record()['failures'])
                self.restored()

    def test_copy_write_failure_before_or_after_replace_can_be_restored_again(self):
        write = tool.durable
        for after in (False, True):
            with self.subTest(after_replace=after):
                self.legacy()

                def fail_copy(path, *args, **kwargs):
                    if path == self.copy:
                        if after:
                            write(path, *args, **kwargs)
                        raise OSError('injected copy persistence failure')
                    return write(path, *args, **kwargs)

                with patch.object(tool, 'durable', fail_copy), self.assertRaises(OSError):
                    tool.restore(self.release)
                self.assertEqual(self.record()['status'], 'RESTORE_FAILED')
                self.assertEqual(self.record()['recovery']['step'], 'updating_copy')
                self.assertEqual(tool.restore(self.release)['status'], 'RESTORED')
                self.restored()

    def test_transient_progress_record_failure_does_not_stop_recovery(self):
        self.legacy()
        save = tool.save
        for after in (False, True):
            self.legacy()
            failed = False

            def fail_once(release, record, *args):
                nonlocal failed
                if record['status'] == 'RESTORING' and not failed:
                    failed = True
                    if after:
                        save(release, record, *args)
                    raise OSError('injected record failure')
                return save(release, record, *args)

            with self.subTest(after_replace=after), patch.object(tool, 'save', fail_once):
                self.assertEqual(tool.restore(self.release)['status'], 'RESTORED')
            self.assertTrue(any(item['action'] == 'restore_record' for item in self.record()['failures']))
            self.restored()

    def test_final_and_failure_record_write_failures_leave_completed_bytes_retryable(self):
        save = tool.save
        for after in (False, True):
            with self.subTest(after_replace=after):
                self.legacy()

                def fail_final(release, record, *args):
                    if record['status'] in ('RESTORED', 'RESTORE_FAILED'):
                        if after:
                            save(release, record, *args)
                        raise OSError('injected final record failure')
                    return save(release, record, *args)

                with patch.object(tool, 'save', fail_final), self.assertRaisesRegex(RuntimeError, 'restore_record_failed'):
                    tool.restore(self.release)
                self.restored()
                self.assertEqual(tool.restore(self.release)['status'], 'RESTORED')
                self.restored()

    def test_apply_failures_including_final_record_failure_restore_automatically(self):
        save, write = tool.save, tool.durable
        for failure in ('install', 'test', 'reload', 'copy', 'final_record_before', 'final_record_after'):
            with self.subTest(failure=failure):
                self.legacy('PREPARED', self.before, self.before)
                if failure in ('install', 'test', 'reload'):
                    self.hooks['install_candidate' if failure == 'install' else failure] = self.fault(failure)

                def fail_save(release, record, *args):
                    if record['status'] == 'NGINX_APPLIED' and failure.startswith('final_record'):
                        if failure.endswith('_after'):
                            save(release, record, *args)
                        raise OSError('injected final apply record failure')
                    return save(release, record, *args)

                def fail_write(path, data, *args, **kwargs):
                    if path == self.copy and data == self.after and failure == 'copy':
                        raise OSError('injected apply copy failure')
                    return write(path, data, *args, **kwargs)

                with patch.object(tool, 'save', fail_save), patch.object(tool, 'durable', fail_write), \
                        self.assertRaisesRegex(RuntimeError, '^apply_failed_rolled_back$'):
                    tool.apply(self.release)
                self.assertEqual(self.record()['status'], 'ROLLED_BACK')
                self.assertTrue(any(item['action'] == 'apply' for item in self.record()['failures']))
                self.restored()

    def test_failed_apply_failure_record_does_not_prevent_automatic_recovery(self):
        self.legacy('PREPARED', self.before, self.before)
        self.hooks['test'] = self.fault('nginx_test_failed')
        save = tool.save

        def fail_save(release, record, *args):
            if record['status'] == 'FAIL':
                raise OSError('injected failure record failure')
            return save(release, record, *args)

        with patch.object(tool, 'save', fail_save), self.assertRaisesRegex(RuntimeError, '^apply_failed_rolled_back$'):
            tool.apply(self.release)
        self.restored()
        self.assertEqual(self.record()['status'], 'ROLLED_BACK')
        self.assertTrue(any(item['action'] == 'apply_record' for item in self.record()['failures']))

    def test_failed_automatic_recovery_can_be_resumed_explicitly(self):
        self.legacy('PREPARED', self.before, self.before)

        def apply_failure():
            self.hooks['test'] = self.fault('restore_test_failed')
            raise RuntimeError('nginx_test_failed')

        self.hooks['test'] = apply_failure
        with self.assertRaisesRegex(RuntimeError, 'apply_failed_restore_incomplete'):
            tool.apply(self.release)
        self.assertEqual(self.record()['status'], 'RESTORE_FAILED')
        self.assertEqual(self.site.read_bytes(), self.before)
        self.assertEqual(tool.restore(self.release)['status'], 'RESTORED')
        self.restored()

    def test_process_interruption_after_install_leaves_a_restorable_applying_record(self):
        self.legacy('PREPARED', self.before, self.before)

        def stop():
            raise Interrupted()

        self.hooks['installed_candidate'] = stop
        with self.assertRaises(Interrupted):
            tool.apply(self.release)
        self.assertEqual(self.record()['status'], 'APPLYING')
        self.assertEqual(self.site.read_bytes(), self.after)
        self.assertEqual(self.copy.read_bytes(), self.before)
        self.assertEqual(tool.restore(self.release)['status'], 'RESTORED')
        self.restored()

    def test_apply_never_installs_without_durable_intent_or_with_a_changed_baseline(self):
        self.legacy('PREPARED', self.before, self.before)
        with patch.object(tool, 'save', side_effect=OSError('initial journal failure')), self.assertRaises(OSError):
            tool.apply(self.release)
        self.assertEqual(self.events, [])
        self.restored()
        self.copy.write_bytes(self.after)
        with self.assertRaisesRegex(RuntimeError, 'baseline_changed'):
            tool.apply(self.release)
        self.assertEqual(self.events, [])
        self.assertEqual(self.site.read_bytes(), self.before)
        self.assertEqual(self.copy.read_bytes(), self.after)

    def test_corrupt_recovery_inputs_or_other_targets_are_refused_before_live_actions(self):
        for changed in ('backup', 'candidate', 'candidate_and_hash', 'target', 'status'):
            with self.subTest(changed=changed):
                self.legacy()
                if changed in ('backup', 'candidate', 'candidate_and_hash'):
                    path = self.release / ('nginx-backup.conf' if changed == 'backup' else 'nginx-candidate.conf')
                    path.write_bytes(path.read_bytes() + b'# foreign edit\n')
                record = self.record()
                if changed == 'candidate_and_hash':
                    record['nginxSha256'] = tool.sha((self.release / 'nginx-candidate.conf').read_bytes())
                if changed == 'target':
                    record['targets'] = {'site': '/another/site', 'copy': str(self.copy)}
                if changed == 'status':
                    record['status'] = ['corrupt']
                (self.release / tool.RECORD).write_text(json.dumps(record))
                self.events.clear()
                with self.assertRaises(RuntimeError):
                    tool.restore(self.release)
                self.assertEqual(self.events, [])
                self.assertEqual(self.site.read_bytes(), self.after)
                self.assertEqual(self.copy.read_bytes(), self.after)

    def test_third_live_bytes_are_never_overwritten_at_entry_or_during_recovery(self):
        for path in (self.site, self.copy):
            with self.subTest(path=path.name):
                self.legacy()
                path.write_bytes(b'third generation')
                self.events.clear()
                with self.assertRaisesRegex(RuntimeError, 'another_change'):
                    tool.restore(self.release)
                self.assertEqual(self.events, [])
                self.assertEqual(path.read_bytes(), b'third generation')
        self.legacy()
        self.hooks['reload'] = lambda: self.copy.write_bytes(b'concurrent edit')
        with self.assertRaisesRegex(RuntimeError, 'another_change'):
            tool.restore(self.release)
        self.assertEqual(self.copy.read_bytes(), b'concurrent edit')

    def test_symlink_live_recovery_and_parent_paths_are_refused_without_following(self):
        for name in ('site', 'copy', 'backup', 'candidate', 'record'):
            with self.subTest(name=name):
                self.legacy()
                path = {'site': self.site, 'copy': self.copy, 'backup': self.release / 'nginx-backup.conf',
                        'candidate': self.release / 'nginx-candidate.conf', 'record': self.release / tool.RECORD}[name]
                preserved = path.read_bytes()
                other = self.root / 'unrelated'
                other.write_bytes(preserved)
                path.unlink()
                path.symlink_to(other)
                self.events.clear()
                with self.assertRaisesRegex(RuntimeError, 'symlink_refused'):
                    tool.restore(self.release)
                self.assertEqual(other.read_bytes(), preserved)
                self.assertEqual(self.events, [])
                path.unlink()
                path.write_bytes(preserved)
        alias = self.root / 'alias'
        alias.symlink_to(self.state, target_is_directory=True)
        with self.assertRaisesRegex(RuntimeError, 'symlink_refused'):
            tool.restore(alias / self.release.name)

    def test_changed_record_during_restore_is_not_overwritten(self):
        self.legacy()
        changed = b'{"status":"another operators record"}\n'
        self.hooks['test'] = lambda: (self.release / tool.RECORD).write_bytes(changed)
        with self.assertRaisesRegex(RuntimeError, 'deployment_record_changed'):
            tool.restore(self.release)
        self.assertEqual((self.release / tool.RECORD).read_bytes(), changed)
        self.assertNotIn('reload', self.events)

    def test_non_regular_live_and_recovery_files_are_refused_without_blocking(self):
        for kind in ('fifo', 'directory'):
            for name in ('site', 'backup'):
                with self.subTest(kind=kind, name=name):
                    self.legacy()
                    path = self.site if name == 'site' else self.release / 'nginx-backup.conf'
                    data = path.read_bytes()
                    path.unlink()
                    if kind == 'fifo':
                        os.mkfifo(path)
                    else:
                        path.mkdir()
                    self.events.clear()
                    with self.assertRaisesRegex(RuntimeError, 'non_regular_file_refused'):
                        tool.restore(self.release)
                    self.assertEqual(self.events, [])
                    if kind == 'fifo':
                        path.unlink()
                    else:
                        path.rmdir()
                    path.write_bytes(data)

    def test_atomic_install_does_not_expose_partial_bytes_and_syncs_parent(self):
        self.legacy()
        args = (str(self.release / 'nginx-backup.conf'), str(self.site), tool.sha(self.before),
                tool.sha(self.before), tool.sha(self.after))
        with patch.object(tool.os, 'fchown'), patch.object(tool.os, 'replace', side_effect=OSError('rename failure')), \
                self.assertRaises(OSError):
            tool.atomic_install(*args)
        self.assertEqual(self.site.read_bytes(), self.after)
        with patch.object(tool.os, 'fchown'), patch.object(tool, 'directory_sync', side_effect=OSError('fsync failure')), \
                self.assertRaises(OSError):
            tool.atomic_install(*args)
        self.assertEqual(self.site.read_bytes(), self.before)
        with patch.object(tool.os, 'fchown'), patch.object(tool, 'directory_sync', wraps=tool.directory_sync) as sync:
            tool.atomic_install(*args)
            sync.assert_called_once_with(self.site.parent)
        self.assertFalse(list(self.site.parent.glob('.nginx-rollout-*')))

    def test_shared_lock_excludes_other_rollouts_and_rejects_symlinks(self):
        with tool.publication_lock():
            with self.assertRaisesRegex(RuntimeError, 'nginx_rollout_locked'):
                with tool.publication_lock():
                    self.fail('nested lock acquired')
        lock = self.state / 'frame-nginx-rollout.lock'
        lock.unlink()
        lock.symlink_to(self.site)
        with self.assertRaisesRegex(RuntimeError, 'symlink_refused'):
            with tool.publication_lock():
                self.fail('symlink lock acquired')

    def test_main_uses_guarded_dispatch_and_returns_structured_failure(self):
        self.legacy()
        self.hooks['test'] = self.fault('restore_test_failed')
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            result = tool.main(['restore', str(self.release)])
        self.assertEqual(result, 1)
        self.assertEqual(json.loads(output.getvalue()),
                         {'status': 'FAIL', 'action': 'restore', 'failure': 'restore_test_failed'})
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            self.assertEqual(tool.main(['restore', str(self.release)]), 0)
        self.assertEqual(json.loads(output.getvalue())['status'], 'RESTORED')


if __name__ == '__main__':
    unittest.main()
