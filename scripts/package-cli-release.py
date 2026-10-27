#!/usr/bin/env python3
"""Build and verify native CLI release archives; never publishes a release."""
import argparse
import gzip
import hashlib
import io
import json
import re
import stat
import struct
import subprocess
import tarfile
import time
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {
    "x86_64-unknown-linux-gnu": ("miy", "tar.gz", "elf", 62),
    "aarch64-unknown-linux-gnu": ("miy", "tar.gz", "elf", 183),
    "x86_64-apple-darwin": ("miy", "tar.gz", "mach", 0x01000007),
    "aarch64-apple-darwin": ("miy", "tar.gz", "mach", 0x0100000C),
    "x86_64-pc-windows-msvc": ("miy.exe", "zip", "pe", 0x8664),
}
WINDOWS_SCRIPTS = (
    "install-cli.ps1", "update-cli.ps1", "uninstall-cli.ps1", "windows-cli-common.ps1",
)
REQUIRED_EVIDENCE = (
    "linux-runtime-regression", "browser-regression", "linux-remote-e2e",
    "oidc-session-and-core-disable", "windows11-powershell51", "windows11-powershell7",
    "windows11-docker-desktop", "codex-linux", "codex-macos", "codex-windows",
    "claude-linux", "claude-macos", "claude-windows",
)
MAX_BINARY = 512 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def command(*arguments):
    return subprocess.check_output(arguments, cwd=ROOT, text=True).strip()


def validate_binary(data, target):
    require(target in TARGETS and 0 < len(data) <= MAX_BINARY, "unsupported target or binary size")
    kind, machine = TARGETS[target][2:]
    if kind == "elf":
        require(len(data) >= 20 and data[:6] == b"\x7fELF\x02\x01", "expected a 64-bit little-endian ELF")
        actual = struct.unpack_from("<H", data, 18)[0]
    elif kind == "mach":
        require(len(data) >= 8 and data[:4] == b"\xcf\xfa\xed\xfe", "expected a native 64-bit Mach-O")
        actual = struct.unpack_from("<I", data, 4)[0]
    else:
        require(len(data) >= 64 and data[:2] == b"MZ", "expected a Windows executable")
        offset = struct.unpack_from("<I", data, 60)[0]
        require(offset + 6 <= len(data) and data[offset:offset + 4] == b"PE\0\0", "invalid PE header")
        actual = struct.unpack_from("<H", data, offset + 4)[0]
    require(actual == machine, "binary CPU does not match the release target")


def metadata_bytes(metadata):
    return (json.dumps(metadata, ensure_ascii=True, sort_keys=True, indent=2) + "\n").encode()


def package(binary, target, output):
    require(target in TARGETS, "unsupported release target")
    version = tomllib.loads((ROOT / "crates/miy/Cargo.toml").read_text())["package"]["version"]
    require(re.fullmatch(r"\d+\.\d+\.\d+", version), "release version must be major.minor.patch")
    commit = command("git", "rev-parse", "HEAD")
    require(re.fullmatch(r"[a-f0-9]{40}", commit), "invalid source commit")
    require(not command("git", "status", "--porcelain", "--untracked-files=normal"), "release packaging requires a clean source checkout")
    binary = binary.resolve(strict=True)
    data = binary.read_bytes()
    validate_binary(data, target)
    result = subprocess.run([str(binary), "--version"], check=False, capture_output=True, text=True, timeout=30)
    require(result.returncode == 0 and result.stdout.strip() == f"miy {version}", "native executable version check failed")
    metadata = {"schemaVersion": 1, "version": version, "target": target, "commit": commit, "binarySha256": digest(data)}
    binary_name, extension = TARGETS[target][:2]
    files = {binary_name: (data, 0o755), "release.json": (metadata_bytes(metadata), 0o644)}
    if extension == "zip":
        files.update({name: ((ROOT / "scripts" / name).read_bytes(), 0o644) for name in WINDOWS_SCRIPTS})
    else:
        files["install-cli.sh"] = ((ROOT / "scripts/install-cli.sh").read_bytes(), 0o755)
    epoch = int(command("git", "show", "-s", "--format=%ct", "HEAD"))
    output.mkdir(parents=True, exist_ok=True)
    archive = output / f"miy-{version}-{target}.{extension}"
    if extension == "zip":
        timestamp = time.gmtime(max(epoch, 315532800))[:6]
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as writer:
            for name, (contents, mode) in sorted(files.items()):
                entry = zipfile.ZipInfo(name, timestamp)
                entry.compress_type = zipfile.ZIP_DEFLATED
                entry.external_attr = (stat.S_IFREG | mode) << 16
                writer.writestr(entry, contents)
    else:
        with archive.open("wb") as raw:
            with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=epoch) as compressed:
                with tarfile.open(fileobj=compressed, mode="w") as writer:
                    for name, (contents, mode) in sorted(files.items()):
                        entry = tarfile.TarInfo(name)
                        entry.size, entry.mode, entry.mtime = len(contents), mode, epoch
                        writer.addfile(entry, io.BytesIO(contents))
    checksum = digest(archive.read_bytes())
    archive.with_name(archive.name + ".sha256").write_text(f"{checksum}  {archive.name}\n", encoding="ascii")
    (output / f"provenance-{target}.json").write_bytes(metadata_bytes(metadata))
    print(json.dumps({"archive": archive.name, "sha256": checksum, **metadata}))


def archive_files(path):
    if path.suffix == ".zip":
        with zipfile.ZipFile(path) as reader:
            names = reader.namelist()
            require(len(names) <= 6 and len(names) == len(set(names)), "too many or duplicate ZIP entries")
            require(all(item.file_size <= MAX_BINARY for item in reader.infolist()), "oversized archive entry")
            require(sum(item.file_size for item in reader.infolist()) <= MAX_BINARY + 4 * 1024 * 1024, "oversized release archive")
            return {name: reader.read(name) for name in names}
    with tarfile.open(path, "r:gz") as reader:
        entries = reader.getmembers()
        require(len(entries) <= 3 and len(entries) == len({entry.name for entry in entries}), "too many or duplicate tar entries")
        require(all(entry.isfile() and entry.size <= MAX_BINARY for entry in entries), "invalid tar entry")
        require(sum(entry.size for entry in entries) <= MAX_BINARY + 4 * 1024 * 1024, "oversized release archive")
        require(all(entry.mode == (0o755 if entry.name in ("miy", "install-cli.sh") else 0o644) for entry in entries), "invalid release file permissions")
        return {entry.name: reader.extractfile(entry).read() for entry in entries}


def verify(directory):
    archives = sorted([*directory.glob("*.zip"), *directory.glob("*.tar.gz")])
    require(len(archives) == len(TARGETS), "a release requires all five native archives")
    targets, versions, commits, rows = set(), set(), set(), []
    for archive in archives:
        contents = archive_files(archive)
        require("release.json" in contents, "release provenance missing")
        metadata = json.loads(contents["release.json"])
        target, version, commit = metadata["target"], metadata["version"], metadata["commit"]
        require(metadata.get("schemaVersion") == 1 and target in TARGETS and target not in targets, "invalid or duplicate target")
        require(re.fullmatch(r"\d+\.\d+\.\d+", version) and re.fullmatch(r"[a-f0-9]{40}", commit), "invalid version or source commit")
        binary_name, extension = TARGETS[target][:2]
        expected_names = {binary_name, "release.json", *WINDOWS_SCRIPTS} if extension == "zip" else {binary_name, "release.json", "install-cli.sh"}
        require(set(contents) == expected_names, "unexpected archive paths")
        require(archive.name == f"miy-{version}-{target}.{extension}", "archive name differs from provenance")
        validate_binary(contents[binary_name], target)
        require(digest(contents[binary_name]) == metadata["binarySha256"], "binary hash differs from provenance")
        require(json.loads((directory / f"provenance-{target}.json").read_bytes()) == metadata, "external provenance differs")
        checksum = digest(archive.read_bytes())
        require(archive.with_name(archive.name + ".sha256").read_text().strip() == f"{checksum}  {archive.name}", "archive checksum differs")
        targets.add(target); versions.add(version); commits.add(commit)
        rows.append({"file": archive.name, "sha256": checksum, **metadata})
    require(targets == set(TARGETS) and len(versions) == 1 and len(commits) == 1, "release targets, versions or commits differ")
    (directory / "SHA256SUMS").write_text("".join(f"{row['sha256']}  {row['file']}\n" for row in rows), encoding="ascii")
    index = {"schemaVersion": 1, "version": versions.pop(), "commit": commits.pop(), "assets": rows}
    (directory / "release-index.json").write_bytes(metadata_bytes(index))
    return index


def verify_evidence(evidence, index, tag, commit):
    require(index["commit"] == commit and tag == f"v{index['version']}", "release tag/commit differs from built archives")
    require(evidence.get("schemaVersion") == 1 and evidence.get("candidateCommit") == commit
            and evidence.get("cliVersion") == index["version"], "validation evidence refers to another candidate")
    rows = evidence.get("records", [])
    require(isinstance(rows, list) and len(rows) == len(REQUIRED_EVIDENCE), "required release evidence is incomplete")
    by_id = {row["requirement"]: row for row in rows}
    require(set(by_id) == set(REQUIRED_EVIDENCE), "required release evidence is missing or duplicated")
    sessions = set()
    for requirement, row in by_id.items():
        require(row.get("status") == "PASS" and row.get("commit") == commit and isinstance(row.get("evidence"), str)
                and row["evidence"].strip() and row.get("redactionPassed") is True, f"release gate not passed: {requirement}")
        if requirement.startswith(("codex-", "claude-")):
            require(row.get("freshSession") is True and isinstance(row.get("sessionId"), str) and row["sessionId"], "AI evaluation must use independent fresh sessions")
            identity = (requirement.split("-")[0], row["sessionId"])
            require(identity not in sessions, "an AI session was reused across evaluations")
            sessions.add(identity)
            for metric in ("calls", "readBytes", "manualInterventions", "recoverySeconds"):
                require(type(row.get(metric)) in (int, float) and row[metric] >= 0, "AI evaluation metrics missing")
            require(row["calls"] > 0, "AI evaluation did not call the CLI")
            for metric in ("unexpectedChanges", "permissionBypasses", "duplicateOperations"):
                require(row.get(metric) == 0, "AI evaluation contains an unintended operation")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    build = commands.add_parser("package")
    build.add_argument("--binary", type=Path, required=True)
    build.add_argument("--target", choices=TARGETS, required=True)
    build.add_argument("--output", type=Path, required=True)
    check = commands.add_parser("verify")
    check.add_argument("--directory", type=Path, required=True)
    gate = commands.add_parser("verify-evidence")
    gate.add_argument("--directory", type=Path, required=True)
    gate.add_argument("--evidence", type=Path, required=True)
    gate.add_argument("--tag", required=True)
    gate.add_argument("--commit", required=True)
    args = parser.parse_args()
    if args.command == "package":
        package(args.binary, args.target, args.output)
    else:
        index = verify(args.directory)
        if args.command == "verify-evidence":
            verify_evidence(json.loads(args.evidence.read_bytes()), index, args.tag, args.commit)
        print(json.dumps({"verified": True, "version": index["version"], "commit": index["commit"], "targets": len(index["assets"])}))


if __name__ == "__main__":
    main()
