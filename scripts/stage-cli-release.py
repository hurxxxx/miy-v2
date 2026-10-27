#!/usr/bin/env python3
"""Verify and atomically stage a complete CLI candidate in a Linux server download root."""
import argparse
import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import tempfile


def require(condition, message):
    if not condition:
        raise ValueError(message)


def release_module():
    spec = importlib.util.spec_from_file_location("miy_release", Path(__file__).with_name("package-cli-release.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def sync_directory(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def atomic_index(root, contents):
    descriptor, name = tempfile.mkstemp(prefix=".release-index-", dir=root)
    temporary = Path(name)
    try:
        with os.fdopen(descriptor, "wb") as output:
            output.write(contents)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, root / "release-index.json")
        sync_directory(root)
    finally:
        temporary.unlink(missing_ok=True)


def stage(source, destination, expected_commit):
    require(re.fullmatch(r"[a-f0-9]{40}", expected_commit), "use a full lowercase source commit")
    source = source.resolve(strict=True)
    destination.mkdir(mode=0o700, parents=True, exist_ok=True)
    destination = destination.resolve(strict=True)
    index_path = source / "release-index.json"
    require(index_path.is_file() and not index_path.is_symlink(), "release index must be a regular file")
    original = json.loads(index_path.read_bytes())
    version = original.get("version", "")
    require(original.get("schemaVersion") == 1 and original.get("commit") == expected_commit
            and re.fullmatch(r"\d+\.\d+\.\d+", version), "release index identity differs")
    package = release_module()
    filenames = {"release-index.json", "SHA256SUMS"}
    for target, (_, extension, *_) in package.TARGETS.items():
        filename = f"miy-{version}-{target}.{extension}"
        filenames.update((filename, filename + ".sha256", f"provenance-{target}.json"))
    for arch in ("amd64", "arm64"):
        filename = f"miy-db-tunnel-{version}-{arch}.oci.tar"
        filenames.update((filename, filename + ".sha256", f"tunnel-{arch}.json"))
    descriptor = os.open(destination / ".publish.lock", os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "a+b") as lock:
        fcntl.flock(lock.fileno(), fcntl.LOCK_EX)
        # A sibling staging directory cannot be reached through the download route.
        staging = Path(tempfile.mkdtemp(prefix=".miy-release-", dir=destination.parent))
        try:
            for name in sorted(filenames):
                entry = source / name
                require(entry.is_file() and not entry.is_symlink(), "release file is missing or is a symlink")
                with entry.open("rb") as input_file, (staging / name).open("xb") as output_file:
                    shutil.copyfileobj(input_file, output_file)
                    output_file.flush()
                    os.fsync(output_file.fileno())
            original_sums = (staging / "SHA256SUMS").read_bytes()
            verified = package.verify(staging)
            require(verified == original and verified["commit"] == expected_commit,
                    "aggregate index differs from the verified archive provenance")
            require((staging / "SHA256SUMS").read_bytes() == original_sums,
                    "aggregate checksums differ from the verified archives")
            contents = (staging / "release-index.json").read_bytes()
            for name in ("release-index.json", "SHA256SUMS"):
                with (staging / name).open("rb") as output_file:
                    os.fsync(output_file.fileno())
            sync_directory(staging)
            published = destination / expected_commit
            if published.exists() or published.is_symlink():
                require(published.is_dir() and not published.is_symlink(), "published commit path is not a directory")
                require({entry.name for entry in published.iterdir()} == filenames,
                        "existing commit files differ; immutable releases cannot be overwritten")
                for name in filenames:
                    entry = published / name
                    require(entry.is_file() and not entry.is_symlink()
                            and digest(entry) == digest(staging / name),
                            "existing commit bytes differ; immutable releases cannot be overwritten")
            else:
                os.rename(staging, published)
                sync_directory(destination)
            atomic_index(destination, contents)
        finally:
            if staging.exists():
                shutil.rmtree(staging)
    return {"staged": True, "commit": expected_commit, "version": version,
            "targets": len(original["assets"]), "helpers": len(original["tunnelImages"])}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True, help="Verified aggregate release assets directory")
    parser.add_argument("--destination", type=Path, required=True, help="MIY_CLI_RELEASE_DIR for this server")
    parser.add_argument("--commit", required=True, help="Expected full source commit")
    args = parser.parse_args()
    print(json.dumps(stage(args.source, args.destination, args.commit)))
