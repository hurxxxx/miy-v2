"""Own a regression work directory and discard only reproducible payloads."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat

MARKER = '.miy-runtime-owner.json'
LOCK = '.miy-runtime.lock'
PAYLOADS = ('bin', 'artifacts', 'images', 'private-env.json', 'fixture-ca.key',
            'fixture-ca.pem', 'evidence/native-image.tar', 'evidence/classic-image.tar')


def require(condition, code):
    if not condition:
        raise RuntimeError(code)


def plain(root):
    root = Path(os.path.abspath(root))
    require(not any(p.is_symlink() for p in (root, *root.parents)), 'workspace_symlink_refused')
    require(root.is_dir() and root.stat().st_uid == os.getuid(), 'workspace_owner_mismatch')
    require(re.fullmatch(r'miy-runtime-regression-[A-Za-z0-9_-]+', root.name), 'workspace_name_mismatch')
    return root


def create_owner(root):
    root = plain(root)
    with (root / MARKER).open('x') as handle:
        json.dump({'version': 1, 'root': str(root), 'uid': os.getuid()}, handle)


def owned(root):
    root = plain(root)
    marker = root / MARKER
    require(marker.is_file() and not marker.is_symlink(), 'workspace_marker_missing')
    require(json.loads(marker.read_text()) == {'version': 1, 'root': str(root), 'uid': os.getuid()},
            'workspace_marker_mismatch')
    return root


def lock(root):
    root = owned(root)
    descriptor = os.open(root / LOCK, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    handle = os.fdopen(descriptor, 'a')
    try:
        fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except Exception:
        handle.close()
        raise RuntimeError('workspace_in_use') from None
    return handle


def under(value, root):
    value = value.removesuffix(' (deleted)')
    prefix = str(root).rstrip('/')
    return value == prefix or value.startswith(prefix + '/')


def process_users(root, proc=Path('/proc')):
    """Check path references without logging process arguments or environments."""
    require(proc.is_dir(), 'process_inspection_unavailable')
    users = []
    for process in proc.iterdir():
        if not process.name.isdecimal() or int(process.name) == os.getpid():
            continue
        try:
            if process.stat().st_uid == os.getuid() and (
                b'MIY_AUDIT_ROOT=' + str(root).encode()
            ) in (process / 'environ').read_bytes().split(b'\0'):
                users.append(int(process.name))
                continue
        except (FileNotFoundError, ProcessLookupError, PermissionError):
            pass
        paths = []
        try:
            paths.extend(value.decode(errors='replace') for value in (process / 'cmdline').read_bytes().split(b'\0'))
        except (FileNotFoundError, ProcessLookupError):
            continue
        except PermissionError:
            pass  # Other users' processes may hide details; the private root is not shared.
        for link in ('cwd', 'exe', 'root'):
            try:
                paths.append(os.readlink(process / link))
            except (FileNotFoundError, ProcessLookupError, PermissionError):
                pass
        if any(under(value, root) for value in paths):
            users.append(int(process.name))
            continue
        paths = []
        try:
            for descriptor in (process / 'fd').iterdir():
                try:
                    paths.append(os.readlink(descriptor))
                except (FileNotFoundError, ProcessLookupError, PermissionError):
                    pass
        except (FileNotFoundError, ProcessLookupError, PermissionError):
            pass
        try:
            paths.extend(line.split(maxsplit=5)[-1] for line in (process / 'maps').read_text().splitlines())
        except (FileNotFoundError, ProcessLookupError, PermissionError):
            pass
        # Logs under evidence can be held by the waiting runner. Only payload
        # references prevent removal; the log and its parent are retained.
        if any(under(value, root / relative) for value in paths for relative in PAYLOADS):
            users.append(int(process.name))
    return users


def candidates(root):
    result = []
    for relative in PAYLOADS:
        path = root / relative
        require(not any(p.is_symlink() for p in (path, *path.parents)), 'payload_symlink_refused')
        if not path.exists():
            continue
        entries = [path, *path.rglob('*')] if path.is_dir() else [path]
        for entry in entries:
            mode = entry.lstat().st_mode
            require(stat.S_ISREG(mode) or stat.S_ISDIR(mode), 'payload_special_file_refused')
        result.append((relative, path, path.stat().st_dev, path.stat().st_ino,
                       sum(p.stat().st_size for p in entries if p.is_file())))
    return result


def prune(root, docker):
    root = owned(root)
    require(shutil.rmtree.avoids_symlink_attacks, 'safe_directory_removal_unavailable')
    pending = candidates(root)
    require(not process_users(root), 'payload_process_still_running')
    # A root-owned container process is not necessarily inspectable via /proc.
    # Reject even a stopped container's bind mount: it could be restarted later.
    containers = docker('container', 'ls', '--all', '--quiet', '--no-trunc').splitlines()
    for identifier in containers:
        container = json.loads(docker('container', 'inspect', identifier))[0]
        for mount in container.get('Mounts', []):
            source = mount.get('Source', '')
            if mount.get('Type') == 'bind':
                require(Path(source).is_absolute(), 'container_mount_source_invalid')
                require(not under(source, root) and not under(str(root), Path(source)),
                        'payload_container_mount_exists')
    removed = []
    for relative, path, device, inode, size in pending:
        require(not path.is_symlink() and (path.stat().st_dev, path.stat().st_ino) == (device, inode),
                'payload_changed_during_cleanup')
        metadata = {'path': relative, 'bytes': size}
        if relative.startswith('evidence/'):
            metadata['sha256'] = file_sha256(path)
        if path.is_dir():
            shutil.rmtree(path)
        else:
            path.unlink()
        removed.append(metadata)
    return {'removed': removed, 'removedBytes': sum(row['bytes'] for row in removed)}


def file_sha256(path):
    with Path(path).open('rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()
