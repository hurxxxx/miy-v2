#!/usr/bin/env python3
"""Reviewed host-specific UI rollout: immutable additions, then atomic index swap.

Default is read-only inspect. apply/rollback must be called explicitly by the
deployment owner after the server and migration checks. No service or DB access.
"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
from html.parser import HTMLParser
from urllib.parse import unquote, urlsplit

ROOT = Path('/projects/miy-v2')
HERE = ROOT / '.local/release-review-20261104/ui'


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def plain(path):
    path = path.absolute()
    for part in (path, *path.parents):
        require(not part.is_symlink(), 'symlink path refused: ' + str(path))
    return path


def inventory(root):
    root = plain(root)
    require(root.is_dir(), 'missing directory: ' + str(root))
    values = {}
    for path in sorted(root.rglob('*')):
        require(not path.is_symlink(), 'symlink asset refused')
        if path.is_dir():
            continue
        require(stat.S_ISREG(path.stat().st_mode), 'non-regular asset refused')
        values[path.relative_to(root).as_posix()] = digest(path)
    require('index.html' in values, 'index.html missing')
    return values


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')
    with path.open('rb') as file:
        os.fsync(file.fileno())


def directory_sync(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


class References(HTMLParser):
    def __init__(self):
        super().__init__()
        self.paths = set()

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        value = attrs.get('src') if tag == 'script' else attrs.get('href') if tag == 'link' else None
        if not value:
            return
        url = urlsplit(value)
        require(not url.scheme and not url.netloc, 'external index asset refused')
        path = Path(unquote(url.path).lstrip('/'))
        require(path.parts and '..' not in path.parts and not path.is_absolute(), 'invalid index asset path')
        self.paths.add(path.as_posix())


def references(index, files):
    parser = References()
    parser.feed(index.read_text())
    require(parser.paths, 'index has no assets')
    require(parser.paths <= files.keys(), 'index references a missing asset')
    return sorted(parser.paths)


def sources(manifest, expected):
    require(expected == manifest['sourceCommit'], 'source commit does not match approved manifest')
    for name, expected_hash in manifest['sourceFiles'].items():
        require(digest(plain(ROOT / name)) == expected_hash, 'worktree build input changed: ' + name)
        content = subprocess.check_output(['git', 'show', expected + ':' + name], cwd=ROOT)
        require(hashlib.sha256(content).hexdigest() == expected_hash, 'commit build input mismatch: ' + name)


def check(candidate, live, manifest):
    new = inventory(candidate)
    require(new == manifest['candidateFiles'], 'candidate bytes changed since review')
    old = inventory(live)
    collisions = [name for name in new if name != 'index.html' and name in old and new[name] != old[name]]
    require(not collisions, 'immutable asset collision: ' + ', '.join(collisions))
    references(candidate / 'index.html', new)
    return new, old


def add(source, target, expected):
    plain(target)
    require(digest(source) == expected, 'asset source changed')
    if target.exists():
        require(target.is_file() and digest(target) == expected, 'existing asset differs: ' + str(target))
        return
    target.parent.mkdir(parents=True, exist_ok=True)
    descriptor, temporary = tempfile.mkstemp(prefix='.miy-ui-add-', dir=target.parent)
    temporary = Path(temporary)
    try:
        with os.fdopen(descriptor, 'wb') as file:
            file.write(source.read_bytes())
            file.flush()
            os.fchmod(file.fileno(), 0o644)
            os.fsync(file.fileno())
        require(digest(temporary) == expected, 'asset changed during copy')
        try:
            os.link(temporary, target)  # O_EXCL-equivalent publication; never replace.
        except FileExistsError:
            require(digest(target) == expected, 'concurrent immutable asset collision')
        directory_sync(target.parent)
    finally:
        temporary.unlink(missing_ok=True)


def replace_index(source, live, before, expected):
    require(digest(source) == expected, 'index source changed')
    descriptor, temporary = tempfile.mkstemp(prefix='.miy-ui-index-', dir=live)
    temporary = Path(temporary)
    try:
        with os.fdopen(descriptor, 'wb') as file:
            file.write(source.read_bytes())
            file.flush()
            os.fchmod(file.fileno(), 0o644)
            os.fsync(file.fileno())
        require(digest(temporary) == expected, 'index changed during copy')
        require(digest(live / 'index.html') == before, 'live index changed concurrently')
        os.replace(temporary, live / 'index.html')
        directory_sync(live)
    finally:
        temporary.unlink(missing_ok=True)


def run(args):
    manifest = json.loads(args.manifest.read_text())
    candidate, live = plain(args.candidate), plain(args.live)
    require(candidate != live and candidate not in live.parents and live not in candidate.parents, 'candidate/live overlap')
    if args.action == 'inspect':
        new, old = check(candidate, live, manifest)
        sources(manifest, manifest['sourceCommit'])
        print(json.dumps({'sourceCommit': manifest['sourceCommit'], 'candidateFiles': len(new),
                          'newAssets': [name for name in new if name not in old],
                          'identicalAssets': sum(name != 'index.html' and old.get(name) == value for name, value in new.items()),
                          'currentIndexSha256': old['index.html'], 'candidateIndexSha256': new['index.html'],
                          'matchesReviewedLiveIndex': old['index.html'] == manifest['expectedLiveIndexSha256']}))
        return
    require(args.backup is not None, '--backup is required')
    backup = plain(args.backup)
    require(backup != live and backup != candidate and live not in backup.parents and candidate not in backup.parents,
            'backup must be outside asset directories')
    if live == ROOT / 'web/platform/dist':
        require(HERE in backup.parents, 'production backup must be under this isolated release directory')
    with (HERE / 'ui-rollout.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if args.action == 'apply':
            require(args.expected_commit is not None, '--expected-commit is required')
            sources(manifest, args.expected_commit)
            new, old = check(candidate, live, manifest)
            require(old['index.html'] == manifest['expectedLiveIndexSha256'], 'live index differs from reviewed baseline')
            backup.mkdir(parents=True, exist_ok=False, mode=0o700)
            shutil.copytree(live, backup / 'dist', symlinks=False)
            require(inventory(backup / 'dist') == old, 'backup restore verification failed')
            require(inventory(live) == old, 'live tree changed during backup')
            record = {'sourceCommit': manifest['sourceCommit'], 'livePath': str(live), 'files': old,
                      'candidateIndexSha256': new['index.html'], 'phase': 'backup_verified'}
            save(backup / 'manifest.json', record)
            for name, value in new.items():
                if name != 'index.html':
                    add(candidate / name, live / name, value)
            installed = inventory(live)
            require(all(installed.get(name) == value for name, value in old.items()), 'previous assets changed')
            require(all(installed.get(name) == value for name, value in new.items() if name != 'index.html'), 'candidate asset verification failed')
            references(candidate / 'index.html', installed)
            replace_index(candidate / 'index.html', live, old['index.html'], new['index.html'])
            record['phase'] = 'applied'
            save(backup / 'receipt.json', record)
        else:
            record = json.loads((backup / 'manifest.json').read_text())
            require(record['livePath'] == str(live) and record['sourceCommit'] == manifest['sourceCommit'], 'backup belongs to another deployment')
            require(inventory(backup / 'dist') == record['files'], 'backup bytes changed')
            current = digest(live / 'index.html')
            require(current in (record['candidateIndexSha256'], record['files']['index.html']), 'live index belongs to another deployment')
            for name, value in record['files'].items():
                if name != 'index.html':
                    add(backup / 'dist' / name, live / name, value)
            references(backup / 'dist/index.html', inventory(live))
            replace_index(backup / 'dist/index.html', live, current, record['files']['index.html'])
            record['phase'] = 'rolled_back'
            save(backup / 'rollback-receipt.json', record)
        print(json.dumps({'phase': record['phase'], 'sourceCommit': record['sourceCommit'], 'backup': str(backup),
                          'indexSha256': digest(live / 'index.html')}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', nargs='?', choices=['inspect', 'apply', 'rollback'], default='inspect')
    parser.add_argument('--manifest', type=Path, default=HERE / 'manifest.json')
    parser.add_argument('--candidate', type=Path, default=HERE / 'dist')
    parser.add_argument('--live', type=Path, default=ROOT / 'web/platform/dist')
    parser.add_argument('--backup', type=Path)
    parser.add_argument('--expected-commit')
    run(parser.parse_args())
