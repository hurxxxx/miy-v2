#!/usr/bin/env python3
"""Guarded Nginx file-route rollout for this host; no work is done on import.

  prepare SOURCE COMMIT RELEASE_DIR CERTIFICATE KEY
      Render a clean source commit, require the narrow file-route diff, and save
      durable backups and a PREPARED nginx-deployment.json. Nothing is installed.
  apply RELEASE_DIR
      Atomically install, test, reload and update the rendered copy. Any failure,
      including recording success, attempts recovery using the same backups.
  restore RELEASE_DIR
      Restore either generation of each live file, including interrupted mixed
      states and completed restores. Always test/reload and persist the restored
      files again: matching bytes alone do not prove a prior reload/fsync finished.

The original nginx-deployment.json, nginx-backup.conf and nginx-candidate.conf
format is accepted. Both backup/candidate hashes and the narrow diff are checked
on every action. Other live bytes, changed recovery inputs and symlinks are refused.
Progress and failures are recorded atomically with file and parent-directory fsync.
A transient journal write failure does not prevent recovery; a final unrecorded
recovery is reported as a failure and remains retryable from the retained record.

apply/restore require the user's explicit deployment request. This tool does not
change Core, databases, certificates or the historical rollout script.
"""
import argparse
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile

STATE = Path('/projects/miy-v2/.local')
SITE = Path('/etc/nginx/sites-available/v2.dwdcc.kr')
COPY = STATE / 'nginx-miy.conf'
RECORD = 'nginx-deployment.json'
STATUSES = {'PREPARED', 'APPLYING', 'NGINX_APPLIED', 'FAIL', 'ROLLED_BACK',
            'RESTORING', 'RESTORE_FAILED', 'RESTORED'}
OLD_DENY = b'api/v1/(preview(/|$)|installations/[^/]+/(api(/|$)|connections/[^/]+/api(/|$))))'
NEW_DENY = (b'api/v1/(preview(/|$)|installations/[^/]+/(api(/|$)|files(/|$)|platform/files(/|$)'
            b'|connections/[^/]+/(api|files)(/|$))))')
OLD_COMMENT = b'    # Asset paths carry a frame capability: never write them to access logs.\n'
NEW_COMMENT = (b'    # Asset paths carry a frame capability. Nginx cannot redact its error log\n'
               b'    # request URI, including upstream failures, so disable both logs here.\n')
PORTAL_START = b'    # Backend storage and app file transfers keep request bodies streaming.\n'
FRAME_START = b'    # Small business attachments have their own bounded streaming gateway. The\n'
# Only the atomic replacement needs root. run_path uses a non-main module name,
# so the child neither takes the rollout lock nor dispatches a rollout action.
INSTALL_CODE = ("import runpy,sys; m=runpy.run_path(sys.argv[1]); "
                "m['atomic_install'](*sys.argv[2:])")


def require(ok, code):
    if not ok:
        raise RuntimeError(code)


def sha(value):
    return hashlib.sha256(value).hexdigest()


def plain(path):
    path = Path(os.path.abspath(path))
    for part in (path, *path.parents):
        require(not part.is_symlink(), 'symlink_refused')
    return path


def read(path):
    descriptor = os.open(plain(path), os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        require(stat.S_ISREG(os.fstat(descriptor).st_mode), 'non_regular_file_refused')
        with os.fdopen(descriptor, 'rb', closefd=False) as handle:
            return handle.read()
    finally:
        os.close(descriptor)


def current(path):
    try:
        return sha(read(path))
    except FileNotFoundError:
        return None


def directory_sync(path):
    descriptor = os.open(plain(path), os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def durable(path, data, allowed=(None,), mode=0o600, owner=None):
    """Replace only an owned generation, and persist the rename before returning."""
    path = plain(path)
    require(current(path) in allowed, 'file_changed_before_replace')
    descriptor, temporary = tempfile.mkstemp(dir=path.parent, prefix='.nginx-rollout-')
    try:
        with os.fdopen(descriptor, 'wb') as handle:
            if owner is not None:
                os.fchown(handle.fileno(), *owner)
            os.fchmod(handle.fileno(), mode)
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        require(current(path) in allowed, 'file_changed_before_replace')
        os.replace(temporary, path)
        directory_sync(path.parent)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def encoded(record):
    return (json.dumps(record, indent=2) + '\n').encode()


def save(release, record, allowed=(None,)):
    durable(release / RECORD, encoded(record), allowed)


class Journal:
    def __init__(self, release, record, original=None):
        self.release, self.record = release, record
        self.expected = {sha(original) if original is not None else None}

    def owned(self):
        require(current(self.release / RECORD) in self.expected, 'deployment_record_changed')

    def save(self):
        self.owned()
        candidate = sha(encoded(self.record))
        allowed = set(self.expected)
        # A failed directory fsync can leave the new bytes visible. Retain both
        # possibilities so an error checkpoint does not overwrite a third edit.
        self.expected.add(candidate)
        save(self.release, self.record, allowed)
        self.expected = {candidate}

    def failure(self, action, step, error):
        code = str(error) if isinstance(error, RuntimeError) else type(error).__name__
        self.record.setdefault('failures', []).append({'action': action, 'step': step, 'failure': code})
        return code


def sudo(args, code, capture=False):
    result = subprocess.run(['sudo', '-n', *map(str, args)], capture_output=True, check=False)
    require(result.returncode == 0, code)
    return result.stdout if capture else None


def git(source, *args):
    result = subprocess.run(['git', '-C', str(source), *args], capture_output=True, text=True, check=False)
    require(result.returncode == 0, 'source_inspection_failed')
    return result.stdout.strip()


def without_block(text, start):
    index = text.index(start)
    end = text.index(b'\n    }\n', index) + len(b'\n    }\n')
    return text[:index] + text[end:], text[index:end]


def reverse(candidate):
    """Undo only the original rollout's three blocks and two literal changes."""
    text, blocks = candidate, []
    require(text.count(PORTAL_START) == 2 and text.count(FRAME_START) == 1, 'unexpected_file_route_blocks')
    for start in (PORTAL_START, PORTAL_START, FRAME_START):
        text, block = without_block(text, start)
        blocks.append(block)
    for block in blocks:
        lines = [line.strip() for line in block.split(b'\n') if line.strip() and not line.strip().startswith(b'#')]
        require(lines and lines[0].startswith(b'location ') and lines[-1] == b'}', 'file_route_block_shape')
        require(all(line.split(b' ')[0] in (b'client_max_body_size', b'client_body_timeout', b'proxy_pass',
                                            b'proxy_http_version', b'proxy_read_timeout', b'proxy_send_timeout',
                                            b'proxy_request_buffering', b'proxy_buffering', b'proxy_set_header')
                    for line in lines[1:-1]), 'file_route_block_directive')
    require(text.count(NEW_DENY) == 1 and text.count(NEW_COMMENT) == 1, 'expected_line_changes_missing')
    return text.replace(NEW_DENY, OLD_DENY).replace(NEW_COMMENT, OLD_COMMENT), blocks


def targets():
    site, copy = plain(SITE), plain(COPY)
    require(site != copy, 'overlapping_live_files')
    return {'site': str(site), 'copy': str(copy)}


def recovery_inputs(journal):
    record, release = journal.record, journal.release
    journal.owned()
    require(record.get('targets', targets()) == targets(), 'release_belongs_to_other_targets')
    for name in ('previousSha256', 'nginxSha256'):
        require(isinstance(record.get(name), str) and re.fullmatch('[0-9a-f]{64}', record[name]),
                'invalid_deployment_hash')
    before, after = read(release / 'nginx-backup.conf'), read(release / 'nginx-candidate.conf')
    require(sha(before) == record['previousSha256'] and sha(after) == record['nginxSha256'],
            'backup_or_candidate_changed')
    require(reverse(after)[0] == before, 'candidate_no_longer_reverses_to_backup')
    return before, after


def live(record):
    allowed = (record['previousSha256'], record['nginxSha256'])
    values = {name: current(Path(path)) for name, path in targets().items()}
    require(all(value in allowed for value in values.values()), 'live_nginx_belongs_to_another_change')
    return values


def load(release):
    release = plain(release)
    raw = read(release / RECORD)
    record = json.loads(raw)
    require(isinstance(record, dict) and isinstance(record.get('status'), str)
            and record['status'] in STATUSES, 'invalid_deployment_record')
    require(isinstance(record.get('failures', []), list), 'invalid_deployment_record')
    journal = Journal(release, record, raw)
    recovery_inputs(journal)
    live(record)
    return journal


def atomic_install(source, destination, source_hash, before_hash, after_hash):
    """Privileged helper; source and destination are rechecked inside the child."""
    data = read(Path(source))
    require(sha(data) == source_hash, 'install_source_changed')
    durable(Path(destination), data, (before_hash, after_hash), mode=0o644, owner=(0, 0))


def install(journal, name, code):
    recovery_inputs(journal)
    live(journal.record)
    expected = journal.record['previousSha256' if name == 'nginx-backup.conf' else 'nginxSha256']
    sudo([sys.executable, '-c', INSTALL_CODE, str(plain(__file__)), str(journal.release / name), str(SITE),
          expected, journal.record['previousSha256'], journal.record['nginxSha256']], code)
    require(current(SITE) == expected, 'installed_bytes_mismatch')


def prepare(source, commit, release, certificate, key):
    source, release = plain(source), plain(release)
    target_paths = targets()
    require(not release.exists(), 'release_already_exists')
    require(re.fullmatch('[0-9a-f]{40}', commit), 'invalid_source_commit')
    before = read(SITE)
    require(read(COPY) == before, 'live_nginx_differs_from_rendered_copy')
    require(git(source, 'rev-parse', 'HEAD') == commit and not git(source, 'status', '--porcelain'),
            'source_not_clean_at_commit')
    renderer = plain(source / 'scripts/render-server-config.py')
    work = Path(tempfile.mkdtemp(prefix='miy-file-routes-render-'))
    try:
        durable(work / 'server.json', read(STATE / 'server.json'))
        sudo([sys.executable, str(renderer), '--state-dir', str(work), '--binary', str(STATE / 'bin/miy-server'),
              '--certificate', certificate, '--key', key, '--user', os.environ.get('USER', 'root')], 'render_failed')
        rendered = plain(work / 'nginx-miy.conf')
        require(stat.S_ISREG(rendered.stat().st_mode), 'non_regular_file_refused')
        candidate = sudo(['cat', '--', str(rendered)], 'render_read_failed', capture=True)
    finally:
        sudo(['rm', '-rf', '--', str(work)], 'render_cleanup_failed')
    restored, blocks = reverse(candidate)
    require(restored == before, 'rendered_nginx_changes_more_than_file_routes')
    require(read(SITE) == before and read(COPY) == before, 'baseline_changed_during_prepare')
    release.mkdir(mode=0o700)
    directory_sync(release.parent)
    durable(release / 'nginx-backup.conf', before)
    durable(release / 'nginx-candidate.conf', candidate)
    record = {'status': 'PREPARED', 'sourceCommit': commit, 'previousSha256': sha(before),
              'nginxSha256': sha(candidate), 'targets': target_paths,
              'change': 'Streaming file-transfer locations on the portal and dev portal, /_miy/files/ on the '
                        'app-frame server, file routes added to the console denial pattern, one comment',
              'addedBlocks': len(blocks), 'addedBlockLines': [block.count(b'\n') for block in blocks]}
    Journal(release, record).save()
    return record


def recover(journal, completed='RESTORED'):
    before, _ = recovery_inputs(journal)
    record = journal.record
    live(record)
    previous = record.get('recovery', {})
    require(isinstance(previous, dict) and isinstance(previous.get('attempt', 0), int), 'invalid_recovery_record')
    record['recovery'] = {'attempt': previous.get('attempt', 0) + 1, 'completion': completed}

    def checkpoint(step, final=False):
        record['status'] = completed if final else 'RESTORING'
        record['recovery']['step'] = step
        try:
            journal.save()
        except OSError as error:
            journal.failure('restore_record', step, error)
            if final:
                raise RuntimeError('restore_record_failed') from None

    try:
        checkpoint('installing_backup')
        install(journal, 'nginx-backup.conf', 'restore_install_failed')
        checkpoint('testing_backup')
        recovery_inputs(journal)
        require(live(record)['site'] == record['previousSha256'], 'restored_site_changed')
        sudo(['nginx', '-t'], 'restore_test_failed')
        checkpoint('reloading_backup')
        recovery_inputs(journal)
        require(live(record)['site'] == record['previousSha256'], 'restored_site_changed')
        sudo(['systemctl', 'reload', 'nginx'], 'restore_reload_failed')
        checkpoint('updating_copy')
        recovery_inputs(journal)
        require(live(record)['site'] == record['previousSha256'], 'restored_site_changed')
        durable(COPY, before, (record['previousSha256'], record['nginxSha256']))
        require(all(value == record['previousSha256'] for value in live(record).values()), 'restored_bytes_mismatch')
        checkpoint('complete', final=True)
    except (RuntimeError, OSError, ValueError) as error:
        step = record['recovery'].get('step', 'starting')
        record['status'] = 'RESTORE_FAILED'
        record['recovery']['failure'] = journal.failure('restore', step, error)
        try:
            journal.save()
        except OSError as record_error:
            journal.failure('restore_record', 'recording_failure', record_error)
        raise
    return record


def restore(release):
    return recover(load(release))


def apply(release):
    journal = load(release)
    record = journal.record
    require(record['status'] == 'PREPARED', 'release_not_prepared')
    _, after = recovery_inputs(journal)
    require(all(value == record['previousSha256'] for value in live(record).values()), 'baseline_changed')
    record['status'], record['applyStep'] = 'APPLYING', 'installing_candidate'
    journal.save()  # No live writes until a durable recovery record exists.
    try:
        install(journal, 'nginx-candidate.conf', 'install_failed')
        record['applyStep'] = 'testing_candidate'
        journal.save()
        require(live(record)['site'] == record['nginxSha256'], 'installed_site_changed')
        sudo(['nginx', '-t'], 'nginx_test_failed')
        record['applyStep'] = 'reloading_candidate'
        journal.save()
        require(live(record)['site'] == record['nginxSha256'], 'installed_site_changed')
        sudo(['systemctl', 'reload', 'nginx'], 'nginx_reload_failed')
        record['applyStep'] = 'updating_copy'
        journal.save()
        recovery_inputs(journal)
        require(live(record)['site'] == record['nginxSha256'], 'installed_site_changed')
        durable(COPY, after, (record['previousSha256'], record['nginxSha256']))
        require(all(value == record['nginxSha256'] for value in live(record).values()), 'installed_bytes_mismatch')
        record['status'], record['applyStep'] = 'NGINX_APPLIED', 'complete'
        journal.save()
    except (RuntimeError, OSError, ValueError) as error:
        record['status'] = 'FAIL'
        record['failure'] = journal.failure('apply', record['applyStep'], error)
        try:
            journal.save()
        except OSError as record_error:
            journal.failure('apply_record', 'recording_failure', record_error)
        try:
            recover(journal, completed='ROLLED_BACK')
        except (RuntimeError, OSError, ValueError):
            raise RuntimeError('apply_failed_restore_incomplete') from None
        raise RuntimeError('apply_failed_rolled_back') from None
    return record


@contextlib.contextmanager
def publication_lock():
    path = plain(STATE / 'frame-nginx-rollout.lock')
    descriptor = os.open(path, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600)
    with os.fdopen(descriptor, 'a') as lock:
        require(stat.S_ISREG(os.fstat(lock.fileno()).st_mode), 'non_regular_lock_refused')
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError('nginx_rollout_locked') from None
        yield


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    actions = parser.add_subparsers(dest='action', required=True)
    prepare_parser = actions.add_parser('prepare')
    prepare_parser.add_argument('source', type=Path)
    prepare_parser.add_argument('commit')
    prepare_parser.add_argument('release', type=Path)
    prepare_parser.add_argument('certificate')
    prepare_parser.add_argument('key')
    for action in ('apply', 'restore'):
        actions.add_parser(action).add_argument('release', type=Path)
    args = parser.parse_args(argv)
    old_umask = os.umask(0o077)
    try:
        with publication_lock():
            if args.action == 'prepare':
                result = prepare(args.source, args.commit, args.release, args.certificate, args.key)
            else:
                result = (apply if args.action == 'apply' else restore)(args.release)
    except (RuntimeError, OSError, ValueError, KeyError) as error:
        code = str(error) if isinstance(error, RuntimeError) else type(error).__name__
        print(json.dumps({'status': 'FAIL', 'action': args.action, 'failure': code}))
        return 1
    finally:
        os.umask(old_umask)
    print(json.dumps(result))
    return 0


if __name__ == '__main__':
    sys.exit(main())
