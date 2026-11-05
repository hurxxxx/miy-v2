#!/usr/bin/env python3
"""Refuse a CLI candidate that reuses the published version for different client code.

Installers and updates treat an equal version as already installed, so a candidate
whose client build inputs changed since the published source must carry a higher
version. Modules that main.rs compiles only with `feature = "server"` do not count.
Shared modules count even when only their server or test parts changed: the check
is deliberately conservative. Server deployments keep the version; only client
candidates are bound by this rule.
"""
import argparse
import json
import pathlib
import re
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
# Everything the client build reads: the crate (sources, build.rs, embedded guidance,
# bundles and Dockerfile), the template that build.rs embeds, and the lock/toolchain.
CLIENT_INPUTS = ("crates/miy/", "templates/", "Cargo.lock", "Cargo.toml", "rust-toolchain.toml")
SERVER_GATE = re.compile(r'\s*#\[cfg\((?:feature = "server"|all\(test, feature = "server"\))\)\]')
MODULE = re.compile(r"\s*(?:pub(?:\([a-z]+\))?\s+)?mod\s+(\w+);")


def git(root, *args):
    return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True, text=True).stdout


def resolve(root, revision):
    try:
        return git(root, "rev-parse", "--verify", "--quiet", revision + "^{commit}").strip()
    except subprocess.CalledProcessError:
        raise ValueError(f"commit {revision} is not in this clone; fetch the full history") from None


def version_at(root, commit):
    version = tomllib.loads(git(root, "show", f"{commit}:crates/miy/Cargo.toml"))["package"]["version"]
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError(f"unsupported version {version!r} at {commit}")
    return version


def client_inputs(root, commit):
    """Paths at `commit` that the client build reads (conservatively)."""
    lines = git(root, "show", f"{commit}:crates/miy/src/main.rs").splitlines()
    server_only = set()
    for gate, line in zip(lines, lines[1:]):
        module = MODULE.fullmatch(line)
        if module and SERVER_GATE.fullmatch(gate):
            server_only.add(f"crates/miy/src/{module.group(1)}.rs")
    tracked = git(root, "ls-tree", "-r", "--name-only", commit).splitlines()
    return {path for path in tracked if path.startswith(CLIENT_INPUTS) and path not in server_only}


def check(root, published, candidate):
    published_commit = resolve(root, published["sourceCommit"])
    candidate_commit = resolve(root, candidate)
    published_version = published["version"]
    if version_at(root, published_commit) != published_version:
        raise ValueError("the published record and the version at its source commit differ")
    candidate_version = version_at(root, candidate_commit)
    result = {
        "published": {"version": published_version, "sourceCommit": published_commit},
        "candidate": {"version": candidate_version, "sourceCommit": candidate_commit},
    }
    order = lambda version: tuple(int(part) for part in version.split("."))
    if order(candidate_version) < order(published_version):
        return {**result, "status": "FAIL", "reason": "candidate_version_lower_than_published"}
    changed = git(root, "diff", "--name-only", published_commit, candidate_commit).splitlines()
    # A changed path counts when the client build reads it at either commit: added,
    # removed, or moved between the server-only and shared module sets.
    inputs = client_inputs(root, published_commit) | client_inputs(root, candidate_commit)
    client = sorted(path for path in changed if path in inputs)
    result["clientInputsChanged"] = client
    if client and candidate_version == published_version:
        return {**result, "status": "FAIL", "reason": "client_inputs_changed_without_version_bump"}
    return {**result, "status": "PASS"}


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--published", default="docs/releases/windows-candidate.json",
                        help="record of the published CLI (version and sourceCommit)")
    parser.add_argument("--candidate", default="HEAD", help="candidate source revision")
    args = parser.parse_args()
    published = json.loads((ROOT / args.published).read_text())
    try:
        result = check(ROOT, published, args.candidate)
    except ValueError as error:
        print(json.dumps({"status": "FAIL", "reason": str(error)}))
        return 1
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
