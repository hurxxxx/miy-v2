#!/usr/bin/env python3
"""Atomically stage a verified Windows GUI installer in its own Linux download root."""
import argparse
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile


def package_module():
    spec = importlib.util.spec_from_file_location("miy_installer_package", Path(__file__).with_name("package-windows-installer.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def sync_directory(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def stage(source, destination, installer_commit, cli_commit):
    package = package_module()
    package.identity(installer_commit, "commit")
    package.identity(cli_commit, "commit")
    source = source.resolve(strict=True)
    original = package.verify(source, installer_commit, cli_commit)
    names = {original["file"], original["file"] + ".sha256", "installer-index.json"}
    destination.mkdir(parents=True, exist_ok=True, mode=0o700)
    destination = destination.resolve(strict=True)
    descriptor = os.open(destination / ".publish.lock", os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "a+b") as lock:
        fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
        staging = Path(tempfile.mkdtemp(prefix=".miy-installer-", dir=destination.parent))
        try:
            for name in sorted(names):
                entry = source / name
                package.require(entry.is_file() and not entry.is_symlink(), "installer file missing or symlinked")
                with entry.open("rb") as input_file, (staging / name).open("xb") as output_file:
                    shutil.copyfileobj(input_file, output_file)
                    output_file.flush()
                    os.fsync(output_file.fileno())
            package.require(package.verify(staging, installer_commit, cli_commit) == original,
                            "installer changed while being staged")
            sync_directory(staging)
            published = destination / installer_commit
            if published.exists() or published.is_symlink():
                package.require(published.is_dir() and not published.is_symlink(), "published path is not a directory")
                package.require({entry.name for entry in published.iterdir()} == names,
                                "immutable installer directory has different files")
                for name in names:
                    entry = published / name
                    package.require(entry.is_file() and not entry.is_symlink()
                                    and digest(entry) == digest(staging / name),
                                    "immutable installer bytes cannot be overwritten")
            else:
                os.rename(staging, published)
                sync_directory(destination)
            descriptor, temporary_name = tempfile.mkstemp(prefix=".installer-index-", dir=destination)
            temporary = Path(temporary_name)
            try:
                with os.fdopen(descriptor, "wb") as output:
                    output.write((published / "installer-index.json").read_bytes())
                    output.flush()
                    os.fsync(output.fileno())
                os.replace(temporary, destination / "installer-index.json")
                sync_directory(destination)
            finally:
                temporary.unlink(missing_ok=True)
        finally:
            if staging.exists():
                shutil.rmtree(staging)
    return {"staged": True, **original,
            "urlPath": f"/cli/installers/{installer_commit}/{original['file']}"}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--destination", type=Path, default=Path(".local/cli-installers"))
    parser.add_argument("--installer-commit", required=True)
    parser.add_argument("--cli-commit", required=True)
    args = parser.parse_args()
    print(json.dumps(stage(args.source, args.destination, args.installer_commit, args.cli_commit)))
