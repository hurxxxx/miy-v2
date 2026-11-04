#!/usr/bin/env python3
"""Maintained UI rollout for this host: build a reviewed commit, add immutable assets, swap the index.

Nginx serves web/platform/dist directly. This tool never builds into it and changes it only by
apply/rollback: new asset files are added without replacing any, then index.html is replaced
atomically. Earlier assets stay so open tabs keep loading their chunks.

  build     vite build of a clean checkout at --commit into RELEASE/dist, then RELEASE/manifest.json
            binding the tracked build inputs, the candidate bytes and the current live index.
  inspect   (default) read-only comparison of the candidate with the live dist.
  apply     copy the live dist to RELEASE/production-backup, verify it, add assets, swap the index.
  rollback  restore the backed-up index.html; assets of both generations stay.

RELEASE is a new directory under .local per rollout. apply/rollback run only after the core
rollout checks and an explicit production request. No service or database access.
"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import time
from html.parser import HTMLParser
from urllib.parse import unquote, urlsplit

ROOT = Path('/projects/miy-v2')
# Tracked files that can change the platform bundle; build requires them clean at --commit.
INPUTS = ['package.json', 'pnpm-lock.yaml', 'pnpm-workspace.yaml', 'tsconfig.json', 'packages', 'web/platform']


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(path):
    with open(path, 'rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()


def plain(path):
    path = Path(os.path.abspath(path))
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


def create(path, value):
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, 'w') as file:
        file.write(json.dumps(value, indent=2) + '\n')
        file.flush()
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


def git(root, *args):
    result = subprocess.run(['git', '-C', str(root), *args], capture_output=True, text=True)
    require(result.returncode == 0, 'git inspection failed')
    return result.stdout


def sources(manifest, expected):
    """The manifest's input hashes must be the commit's own files."""
    require(expected == manifest['sourceCommit'], 'source commit does not match approved manifest')
    root = plain(Path(manifest['sourceRoot']))
    for name, expected_hash in manifest['sourceFiles'].items():
        content = subprocess.run(['git', '-C', str(root), 'show', f'{expected}:{name}'], capture_output=True)
        require(content.returncode == 0 and hashlib.sha256(content.stdout).hexdigest() == expected_hash,
                'commit build input mismatch: ' + name)


def disjoint(*paths):
    for index, path in enumerate(paths):
        for other in paths[index + 1:]:
            require(path != other and path not in other.parents and other not in path.parents,
                    'overlapping rollout directories refused')


def check(candidate, live, manifest):
    new = inventory(candidate)
    require(new == manifest['candidateFiles'], 'candidate bytes changed since build')
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


def build(release, args):
    require(args.commit is not None and re.fullmatch(r'[0-9a-f]{40}', args.commit) is not None,
            '--commit must be a full commit')
    environment = sorted(key for key in os.environ if key.startswith('VITE_'))
    require(not environment, 'VITE_* environment would change the build: ' + ', '.join(environment))
    root, live = plain(args.source_root), plain(args.live)
    candidate, manifest_path = release / 'dist', release / 'manifest.json'
    disjoint(candidate, live)
    require(not candidate.exists() and not manifest_path.exists(), 'release directory already has a build')
    require(git(root, 'rev-parse', 'HEAD').strip() == args.commit, 'source checkout is not at --commit')
    require(git(root, 'status', '--porcelain', '--', *INPUTS) == '', 'build inputs are not clean')
    command = ['pnpm', '--filter', '@miy/platform', 'exec', 'vite', 'build', '--mode', 'production',
               '--outDir', str(candidate), '--emptyOutDir']
    descriptor = os.open(release / 'build.log', os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, 'w') as log:
        result = subprocess.run(command, cwd=root, stdout=log, stderr=subprocess.STDOUT)
    require(result.returncode == 0, 'vite build failed; see build.log')
    require(git(root, 'status', '--porcelain', '--', *INPUTS) == '', 'build changed its inputs')
    names = [name for name in git(root, 'ls-files', '-z', '--', *INPUTS).split('\0') if name]
    files = {name: digest(root / name) for name in names}
    new = inventory(candidate)
    references(candidate / 'index.html', new)
    old = inventory(live)
    tools = {name: subprocess.run([name, '--version'], cwd=root, capture_output=True, text=True).stdout.strip()
             for name in ('node', 'pnpm')}
    manifest = {
        'schemaVersion': 1, 'sourceCommit': args.commit, 'sourceRoot': str(root), 'mode': 'production',
        'inputs': INPUTS, 'sourceFiles': files,
        'sourceDigest': hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest(),
        'viteEnvironmentKeys': environment, 'candidateFiles': new, 'candidateIndexSha256': new['index.html'],
        'expectedLiveIndexSha256': old['index.html'], 'buildCommand': ' '.join(command), 'buildResult': 'PASS',
        'nodeVersion': tools['node'], 'pnpmVersion': tools['pnpm'],
        'builtAt': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
    }
    create(manifest_path, manifest)
    return {'phase': 'built', 'sourceCommit': args.commit, 'sourceFiles': len(files), 'candidateFiles': len(new),
            'newAssets': sum(name not in old for name in new), 'candidateIndexSha256': new['index.html']}


def run(args):
    release = plain(args.release_dir)
    require(ROOT / '.local' in release.parents, 'release directory must be under .local')
    if args.action == 'build':
        release.mkdir(parents=True, exist_ok=True, mode=0o700)
        os.chmod(release, 0o700)
        return build(release, args)
    manifest = json.loads((args.manifest or release / 'manifest.json').read_text())
    candidate, live = plain(args.candidate or release / 'dist'), plain(args.live)
    disjoint(candidate, live)
    if args.action == 'inspect':
        new, old = check(candidate, live, manifest)
        sources(manifest, manifest['sourceCommit'])
        return {'sourceCommit': manifest['sourceCommit'], 'candidateFiles': len(new),
                'newAssets': [name for name in new if name not in old],
                'identicalAssets': sum(name != 'index.html' and old.get(name) == value for name, value in new.items()),
                'currentIndexSha256': old['index.html'], 'candidateIndexSha256': new['index.html'],
                'matchesExpectedLiveIndex': old['index.html'] == manifest['expectedLiveIndexSha256']}
    backup = plain(args.backup or release / 'production-backup')
    disjoint(backup, live, candidate)
    require(release in backup.parents, 'backup must be inside the release directory')
    with (release / 'ui-rollout.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if args.action == 'apply':
            require(args.expected_commit is not None, '--expected-commit is required')
            sources(manifest, args.expected_commit)
            new, old = check(candidate, live, manifest)
            require(old['index.html'] == manifest['expectedLiveIndexSha256'], 'live index differs from build baseline')
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
            require(all(installed.get(name) == value for name, value in new.items() if name != 'index.html'),
                    'candidate asset verification failed')
            references(candidate / 'index.html', installed)
            replace_index(candidate / 'index.html', live, old['index.html'], new['index.html'])
            record['phase'] = 'applied'
            save(backup / 'receipt.json', record)
        else:
            record = json.loads((backup / 'manifest.json').read_text())
            require(record['livePath'] == str(live) and record['sourceCommit'] == manifest['sourceCommit'],
                    'backup belongs to another deployment')
            require(inventory(backup / 'dist') == record['files'], 'backup bytes changed')
            current = digest(live / 'index.html')
            require(current in (record['candidateIndexSha256'], record['files']['index.html']),
                    'live index belongs to another deployment')
            for name, value in record['files'].items():
                if name != 'index.html':
                    add(backup / 'dist' / name, live / name, value)
            references(backup / 'dist/index.html', inventory(live))
            replace_index(backup / 'dist/index.html', live, current, record['files']['index.html'])
            record['phase'] = 'rolled_back'
            save(backup / 'rollback-receipt.json', record)
    return {'phase': record['phase'], 'sourceCommit': record['sourceCommit'], 'backup': str(backup),
            'indexSha256': digest(live / 'index.html')}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('action', nargs='?', choices=['build', 'inspect', 'apply', 'rollback'], default='inspect')
    parser.add_argument('--release-dir', type=Path, required=True, help='new directory under .local per rollout')
    parser.add_argument('--commit', help='build: full commit the source checkout must be at')
    parser.add_argument('--source-root', type=Path, default=ROOT, help='build: checkout with installed dependencies')
    parser.add_argument('--manifest', type=Path)
    parser.add_argument('--candidate', type=Path)
    parser.add_argument('--live', type=Path, default=ROOT / 'web/platform/dist')
    parser.add_argument('--backup', type=Path)
    parser.add_argument('--expected-commit', help='apply: the commit approved for production')
    try:
        result = run(parser.parse_args(argv))
    except (RuntimeError, OSError, ValueError, KeyError) as error:
        print(json.dumps({'status': 'FAIL', 'error': str(error) or type(error).__name__}))
        return 1
    print(json.dumps(result))
    return 0


if __name__ == '__main__':
    sys.exit(main())
