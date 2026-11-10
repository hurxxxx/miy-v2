#!/usr/bin/env python3
"""Maintained UI rollout for this host: build a reviewed commit, add immutable assets, swap the index.

Targets: platform (default; Nginx serves web/platform/dist directly) and frame-runtime (Core
serves .local/frame-runtime to business app frames as MIY_FRAME_RUNTIME_DIR, reading its
index.html per frame document). This tool never builds into a live directory and changes it only
by apply/rollback: new asset files are added without replacing any, then index.html is replaced
atomically. Earlier assets stay so open tabs and frames keep loading their chunks. The first
frame-runtime apply creates the directory; rolling that back removes only its index.html.

  build     vite build of a clean checkout at --commit into RELEASE/dist, then RELEASE/manifest.json
            binding the tracked build inputs, the candidate bytes and the current live index.
  inspect   (default) read-only comparison of the candidate with the live dist.
  apply     copy the live dist to RELEASE/production-backup, verify it, add assets, swap the index.
  rollback  restore the backed-up index.html; assets of both generations stay.

RELEASE is a new directory under .local per rollout. apply/rollback run only after the core
rollout checks and an explicit production request. No service or database access.
"""
import argparse
import contextlib
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
# target: (package, source directory, live directory under ROOT, public URL base of its assets)
TARGETS = {
    'platform': ('@miy/platform', 'web/platform', 'web/platform/dist', '/'),
    'frame-runtime': ('@miy/frame-runtime', 'web/frame-runtime', '.local/frame-runtime', '/_miy/runtime/'),
}
# Tracked files that can change a bundle; build requires them clean at --commit.
SHARED_INPUTS = ['package.json', 'pnpm-lock.yaml', 'pnpm-workspace.yaml', 'tsconfig.json', 'packages']


def inputs(target):
    return SHARED_INPUTS + [TARGETS[target][1]]


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


def inventory(root, absent_ok=False):
    root = plain(root)
    if absent_ok and not root.exists():
        return {}
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


def assets(root):
    """Inventory of a live directory whose index.html may not exist yet."""
    root = plain(root)
    values = {}
    for path in sorted(root.rglob('*')):
        require(not path.is_symlink(), 'symlink asset refused')
        if path.is_file():
            require(stat.S_ISREG(path.stat().st_mode), 'non-regular asset refused')
            values[path.relative_to(root).as_posix()] = digest(path)
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
    def __init__(self, base='/'):
        super().__init__()
        self.paths = set()
        self.base = base

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        value = attrs.get('src') if tag == 'script' else attrs.get('href') if tag == 'link' else None
        if not value:
            return
        url = urlsplit(value)
        require(not url.scheme and not url.netloc, 'external index asset refused')
        location = unquote(url.path)
        if location.startswith('/'):
            require(location.startswith(self.base), 'index asset outside the target base')
            location = location[len(self.base):]
        path = Path(location)
        require(path.parts and '..' not in path.parts and not path.is_absolute(), 'invalid index asset path')
        self.paths.add(path.as_posix())


def references(index, files, base='/'):
    parser = References(base)
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


def build_inputs(root, target='platform'):
    """Cover Vite's production dotenv/public inputs as well as tracked source bytes.

    The maintained platform config uses Vite's default envDir/publicDir. Dotenv files are
    forbidden even if tracked; public files must be regular, tracked files. Do not read their
    contents before these checks: an ignored public file may contain private local data.
    """
    source = TARGETS[target][1]
    for directory in (root, root / source):
        for name in ('.env', '.env.local', '.env.production', '.env.production.local'):
            path = directory / name
            require(not path.exists() and not path.is_symlink(),
                    'dotenv build input refused: ' + path.relative_to(root).as_posix())
    names = [name for name in git(root, 'ls-files', '-z', '--', *inputs(target)).split('\0') if name]
    public = plain(root / source / 'public')
    if public.exists():
        require(public.is_dir(), 'public build input is not a directory')
        for path in public.rglob('*'):
            plain(path)
            if path.is_dir():
                continue
            name = path.relative_to(root).as_posix()
            require(stat.S_ISREG(path.stat().st_mode), 'non-regular public build input: ' + name)
            require(name in names, 'untracked public build input: ' + name)
    files = {}
    for name in names:
        path = plain(root / name)
        require(stat.S_ISREG(path.stat().st_mode), 'non-regular tracked build input: ' + name)
        files[name] = digest(path)
    return files


@contextlib.contextmanager
def publication_lock(release, live):
    """Different releases publishing to the same canonical live directory share one lock.

    Keep lock files in private state, outside live assets, and never unlink them: unlinking a
    held flock would let a later process lock a different inode for the same destination.
    """
    directory = plain(ROOT / '.local/ui-rollout-locks')
    directory.mkdir(mode=0o700, exist_ok=True)
    key = hashlib.sha256(str(plain(live)).encode()).hexdigest()
    paths = (directory / (key + '.lock'), release / 'ui-rollout.lock')
    with contextlib.ExitStack() as stack:
        for index, path in enumerate(paths):
            descriptor = os.open(plain(path), os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
            lock = stack.enter_context(os.fdopen(descriptor, 'a'))
            require(stat.S_ISREG(os.fstat(lock.fileno()).st_mode), 'non-regular rollout lock refused')
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                raise RuntimeError('live target is locked by another UI rollout' if index == 0
                                   else 'release is locked by another UI rollout') from None
        yield


def disjoint(*paths):
    for index, path in enumerate(paths):
        for other in paths[index + 1:]:
            require(path != other and path not in other.parents and other not in path.parents,
                    'overlapping rollout directories refused')


def target_of(manifest):
    return manifest.get('target', 'platform')


def check(candidate, live, manifest):
    target = target_of(manifest)
    new = inventory(candidate)
    require(new == manifest['candidateFiles'], 'candidate bytes changed since build')
    old = inventory(live, absent_ok=target == 'frame-runtime')
    collisions = [name for name in new if name != 'index.html' and name in old and new[name] != old[name]]
    require(not collisions, 'immutable asset collision: ' + ', '.join(collisions))
    references(candidate / 'index.html', new, TARGETS[target][3])
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


def current_index(live):
    return digest(live / 'index.html') if (live / 'index.html').exists() else None


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
        require(current_index(live) == before, 'live index changed concurrently')
        os.replace(temporary, live / 'index.html')
        directory_sync(live)
    finally:
        temporary.unlink(missing_ok=True)


def remove_index(live, before):
    """Undo a first frame-runtime install: Core then serves frames without a runtime."""
    require(current_index(live) == before, 'live index changed concurrently')
    (live / 'index.html').unlink()
    directory_sync(live)


def build(release, args):
    require(args.commit is not None and re.fullmatch(r'[0-9a-f]{40}', args.commit) is not None,
            '--commit must be a full commit')
    environment = sorted(key for key in os.environ if key.startswith('VITE_'))
    require(not environment, 'VITE_* environment would change the build: ' + ', '.join(environment))
    target = args.target
    root, live = plain(args.source_root), plain(args.live)
    candidate, manifest_path = release / 'dist', release / 'manifest.json'
    disjoint(candidate, live)
    require(not candidate.exists() and not manifest_path.exists(), 'release directory already has a build')
    require(git(root, 'rev-parse', 'HEAD').strip() == args.commit, 'source checkout is not at --commit')
    require(git(root, 'status', '--porcelain', '--', *inputs(target)) == '', 'build inputs are not clean')
    files = build_inputs(root, target)
    sources({'sourceCommit': args.commit, 'sourceRoot': str(root), 'sourceFiles': files}, args.commit)
    command = ['pnpm', '--filter', TARGETS[target][0], 'exec', 'vite', 'build', '--mode', 'production',
               '--outDir', str(candidate), '--emptyOutDir']
    descriptor = os.open(release / 'build.log', os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, 'w') as log:
        result = subprocess.run(command, cwd=root, stdout=log, stderr=subprocess.STDOUT)
    require(result.returncode == 0, 'vite build failed; see build.log')
    require(git(root, 'rev-parse', 'HEAD').strip() == args.commit, 'source checkout changed during build')
    require(git(root, 'status', '--porcelain', '--', *inputs(target)) == '', 'build changed its inputs')
    require(build_inputs(root, target) == files, 'build input bytes changed during build')
    new = inventory(candidate)
    references(candidate / 'index.html', new, TARGETS[target][3])
    old = inventory(live, absent_ok=target == 'frame-runtime')
    tools = {name: subprocess.run([name, '--version'], cwd=root, capture_output=True, text=True).stdout.strip()
             for name in ('node', 'pnpm')}
    manifest = {
        'schemaVersion': 1, 'target': target, 'sourceCommit': args.commit, 'sourceRoot': str(root),
        'mode': 'production', 'inputs': inputs(target), 'sourceFiles': files,
        'sourceDigest': hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest(),
        'viteEnvironmentKeys': environment, 'candidateFiles': new, 'candidateIndexSha256': new['index.html'],
        'expectedLiveIndexSha256': old.get('index.html'), 'buildCommand': ' '.join(command), 'buildResult': 'PASS',
        'nodeVersion': tools['node'], 'pnpmVersion': tools['pnpm'],
        'builtAt': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
    }
    create(manifest_path, manifest)
    return {'phase': 'built', 'target': target, 'sourceCommit': args.commit, 'sourceFiles': len(files),
            'candidateFiles': len(new),
            'newAssets': sum(name not in old for name in new), 'candidateIndexSha256': new['index.html']}


def run(args):
    release = plain(args.release_dir)
    require(ROOT / '.local' in release.parents, 'release directory must be under .local')
    if args.action == 'build':
        release.mkdir(parents=True, exist_ok=True, mode=0o700)
        os.chmod(release, 0o700)
        return build(release, args)
    manifest = json.loads((args.manifest or release / 'manifest.json').read_text())
    require(target_of(manifest) == args.target, 'manifest belongs to another target')
    candidate, live = plain(args.candidate or release / 'dist'), plain(args.live)
    disjoint(candidate, live)
    if args.action == 'inspect':
        new, old = check(candidate, live, manifest)
        sources(manifest, manifest['sourceCommit'])
        return {'target': args.target, 'sourceCommit': manifest['sourceCommit'], 'candidateFiles': len(new),
                'newAssets': [name for name in new if name not in old],
                'identicalAssets': sum(name != 'index.html' and old.get(name) == value for name, value in new.items()),
                'currentIndexSha256': old.get('index.html'), 'candidateIndexSha256': new['index.html'],
                'matchesExpectedLiveIndex': old.get('index.html') == manifest['expectedLiveIndexSha256']}
    backup = plain(args.backup or release / 'production-backup')
    disjoint(backup, live, candidate)
    require(release in backup.parents, 'backup must be inside the release directory')
    with publication_lock(release, live):
        if args.action == 'apply':
            require(args.expected_commit is not None, '--expected-commit is required')
            sources(manifest, args.expected_commit)
            new, old = check(candidate, live, manifest)
            require(old.get('index.html') == manifest['expectedLiveIndexSha256'], 'live index differs from build baseline')
            backup.mkdir(parents=True, exist_ok=False, mode=0o700)
            first = not old
            if first:
                # First frame-runtime install: nothing to back up; create the directory.
                (backup / 'dist').mkdir(mode=0o700)
                live.mkdir(mode=0o755, exist_ok=True)
            else:
                shutil.copytree(live, backup / 'dist', symlinks=False)
            require(inventory(backup / 'dist', absent_ok=first) == old if not first
                    else not any((backup / 'dist').iterdir()), 'backup restore verification failed')
            require(inventory(live, absent_ok=first) == old if not first
                    else not (live / 'index.html').exists(), 'live tree changed during backup')
            record = {'sourceCommit': manifest['sourceCommit'], 'livePath': str(live), 'files': old,
                      'candidateIndexSha256': new['index.html'], 'phase': 'backup_verified'}
            save(backup / 'manifest.json', record)
            for name, value in new.items():
                if name != 'index.html':
                    add(candidate / name, live / name, value)
            installed = assets(live)
            require(all(installed.get(name) == value for name, value in old.items()), 'previous assets changed')
            require(all(installed.get(name) == value for name, value in new.items() if name != 'index.html'),
                    'candidate asset verification failed')
            references(candidate / 'index.html', installed, TARGETS[args.target][3])
            replace_index(candidate / 'index.html', live, old.get('index.html'), new['index.html'])
            record['phase'] = 'applied'
            save(backup / 'receipt.json', record)
        else:
            record = json.loads((backup / 'manifest.json').read_text())
            require(record['livePath'] == str(live) and record['sourceCommit'] == manifest['sourceCommit'],
                    'backup belongs to another deployment')
            first = not record['files']
            require((inventory(backup / 'dist') if not first else {}) == record['files'], 'backup bytes changed')
            current = current_index(live)
            require(current in (record['candidateIndexSha256'], record['files'].get('index.html')),
                    'live index belongs to another deployment')
            for name, value in record['files'].items():
                if name != 'index.html':
                    add(backup / 'dist' / name, live / name, value)
            if first:
                if current is not None:
                    remove_index(live, current)
            else:
                references(backup / 'dist/index.html', assets(live), TARGETS[args.target][3])
                replace_index(backup / 'dist/index.html', live, current, record['files']['index.html'])
            record['phase'] = 'rolled_back'
            save(backup / 'rollback-receipt.json', record)
        return {'phase': record['phase'], 'target': args.target, 'sourceCommit': record['sourceCommit'],
                'backup': str(backup), 'indexSha256': current_index(live)}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('action', nargs='?', choices=['build', 'inspect', 'apply', 'rollback'], default='inspect')
    parser.add_argument('--release-dir', type=Path, required=True, help='new directory under .local per rollout')
    parser.add_argument('--target', choices=sorted(TARGETS), default='platform')
    parser.add_argument('--commit', help='build: full commit the source checkout must be at')
    parser.add_argument('--source-root', type=Path, default=ROOT, help='build: checkout with installed dependencies')
    parser.add_argument('--manifest', type=Path)
    parser.add_argument('--candidate', type=Path)
    parser.add_argument('--live', type=Path, help='defaults to the target live directory under ROOT')
    parser.add_argument('--backup', type=Path)
    parser.add_argument('--expected-commit', help='apply: the commit approved for production')
    args = parser.parse_args(argv)
    args.live = args.live or ROOT / TARGETS[args.target][2]
    try:
        result = run(args)
    except (RuntimeError, OSError, ValueError, KeyError) as error:
        print(json.dumps({'status': 'FAIL', 'error': str(error) or type(error).__name__}))
        return 1
    print(json.dumps(result))
    return 0


if __name__ == '__main__':
    sys.exit(main())
