#!/usr/bin/env python3
"""Package/verify a Windows installer without changing the native CLI release."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import stat
import struct
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[1]
MAX_BINARY = 512 * 1024 * 1024
MAX_ARCHIVE = MAX_BINARY + 4 * 1024 * 1024
MAX_INSTALLER = MAX_ARCHIVE + 16 * 1024 * 1024
INDEX_KEYS = {
    "schemaVersion", "platform", "version", "installerCommit", "cliCommit",
    "cliBinarySha256", "archiveSha256", "file", "sha256", "bytes",
}
ARCHIVE_NAMES = {
    "miy.exe", "release.json", "install-cli.ps1", "update-cli.ps1",
    "uninstall-cli.ps1", "windows-cli-common.ps1",
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def metadata_bytes(value):
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=True) + "\n").encode("ascii")


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "duplicate JSON key")
        result[key] = value
    return result


def parse_json(data):
    return json.loads(data, object_pairs_hook=unique_object)


def read_regular(path, limit):
    require(path.is_file() and not path.is_symlink(), "expected a regular file: " + path.name)
    require(0 < path.stat().st_size <= limit, "invalid file size: " + path.name)
    with path.open("rb") as source:
        data = source.read(limit + 1)
    require(0 < len(data) <= limit, "file exceeds size limit: " + path.name)
    return data


def identity(value, kind):
    pattern = r"[a-f0-9]{40}" if kind == "commit" else r"[a-f0-9]{64}"
    require(isinstance(value, str) and re.fullmatch(pattern, value), "invalid " + kind)


def validate_pe(data, installer=False):
    require(64 <= len(data) <= (MAX_INSTALLER if installer else MAX_BINARY)
            and data[:2] == b"MZ", "expected a Windows executable")
    offset = struct.unpack_from("<I", data, 60)[0]
    require(offset >= 64 and offset + 94 <= len(data)
            and data[offset:offset + 4] == b"PE\0\0", "invalid PE header")
    require(struct.unpack_from("<H", data, offset + 4)[0] == 0x8664,
            "expected an AMD64 Windows executable")
    optional_size = struct.unpack_from("<H", data, offset + 20)[0]
    require(optional_size >= 70 and offset + 24 + optional_size <= len(data)
            and struct.unpack_from("<H", data, offset + 24)[0] == 0x20B,
            "expected a PE32+ executable")
    if installer:
        require(struct.unpack_from("<H", data, offset + 24 + 68)[0] == 2,
                "expected a Windows GUI installer")


def validate_index(index):
    require(isinstance(index, dict) and set(index) == INDEX_KEYS, "unexpected installer index fields")
    require(type(index["schemaVersion"]) is int and index["schemaVersion"] == 1
            and index["platform"] == "windows-x64", "unsupported installer index")
    version = index["version"]
    require(isinstance(version, str) and re.fullmatch(r"\d+\.\d+\.\d+", version), "invalid CLI version")
    for field in ("installerCommit", "cliCommit"):
        identity(index[field], "commit")
    for field in ("cliBinarySha256", "archiveSha256", "sha256"):
        identity(index[field], "sha256")
    require(index["file"] == f"MiySetup-{version}-windows-x64.exe", "unexpected installer filename")
    require(type(index["bytes"]) is int and 0 < index["bytes"] <= MAX_INSTALLER, "invalid installer size")


def verify_archive(data, version, cli_commit, archive_sha256, binary_sha256):
    require(0 < len(data) <= MAX_ARCHIVE and digest(data) == archive_sha256, "CLI archive checksum differs")
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        require(len(entries) == len(ARCHIVE_NAMES) and {entry.filename for entry in entries} == ARCHIVE_NAMES,
                "CLI archive has unexpected or duplicate entries")
        require(all(not entry.is_dir() and not entry.flag_bits & 1
                    and stat.S_IFMT(entry.external_attr >> 16) in (0, stat.S_IFREG)
                    and 0 < entry.file_size <= MAX_BINARY for entry in entries), "invalid CLI archive entry")
        require(sum(entry.file_size for entry in entries) <= MAX_ARCHIVE, "oversized expanded CLI archive")
        contents = {entry.filename: archive.read(entry) for entry in entries}
    require(len(contents["release.json"]) <= 65536, "oversized CLI provenance")
    provenance = parse_json(contents["release.json"])
    expected = {"schemaVersion": 1, "version": version, "target": "x86_64-pc-windows-msvc",
                "commit": cli_commit, "binarySha256": binary_sha256}
    require(provenance == expected and type(provenance["schemaVersion"]) is int,
            "embedded CLI provenance differs from the installer index")
    validate_pe(contents["miy.exe"])
    require(digest(contents["miy.exe"]) == binary_sha256, "embedded CLI binary checksum differs")
    return provenance


def embedded_archive(data, expected_sha256):
    """Locate an unmodified embedded ZIP, using its own bounded EOCD offsets.

    The WinForms build embeds the ZIP as a raw manifest resource. Inspecting the
    resource bytes also works on Linux and does not execute the installer. The
    candidate is trusted only after its full SHA-256 and contents are verified.
    ZIP64 and multi-volume ZIPs are intentionally outside this small payload format.
    """
    offset, matches, signatures = 0, [], 0
    while True:
        offset = data.find(b"PK\x05\x06", offset)
        if offset < 0:
            break
        signatures += 1
        require(signatures <= 4096, "too many embedded ZIP end records")
        if offset + 22 <= len(data):
            disk, directory_disk, disk_count, count, size, start, comment = struct.unpack_from("<4H2IH", data, offset + 4)
            first, last = offset - size - start, offset + 22 + comment
            if (disk == directory_disk == 0 and disk_count == count == len(ARCHIVE_NAMES)
                    and 0 <= first < offset < last <= len(data) and last - first <= MAX_ARCHIVE
                    and data[first:first + 4] == b"PK\x03\x04"
                    and data[offset - size:offset - size + 4] == b"PK\x01\x02"):
                candidate = data[first:last]
                if digest(candidate) == expected_sha256:
                    matches.append(candidate)
        offset += 4
    require(len(matches) == 1, "installer must embed exactly one verified CLI archive")
    return matches[0]


def provenance_bytes(index):
    # Exact UTF-8, compact, ordered Miy.Installer.json resource emitted by
    # ops/windows-installer/build.ps1. All values have already been restricted
    # to ASCII identifiers, so PowerShell and Python serialize these identically.
    metadata = {"schemaVersion": 1, "version": index["version"], "target": "windows-x64",
                "installerCommit": index["installerCommit"], "cliCommit": index["cliCommit"],
                "archiveSha256": index["archiveSha256"], "cliBinarySha256": index["cliBinarySha256"]}
    return json.dumps(metadata, separators=(",", ":"), ensure_ascii=True).encode("ascii")


def verify_embedded_provenance(data, index):
    require(data.count(provenance_bytes(index)) == 1,
            "embedded installer provenance differs from index or is duplicated")


def verify(directory, installer_commit=None, cli_commit=None):
    index = parse_json(read_regular(directory / "installer-index.json", 65536))
    validate_index(index)
    if installer_commit is not None:
        identity(installer_commit, "commit")
        require(index["installerCommit"] == installer_commit, "installer source commit differs")
    if cli_commit is not None:
        identity(cli_commit, "commit")
        require(index["cliCommit"] == cli_commit, "CLI source commit differs")
    executable = read_regular(directory / index["file"], MAX_INSTALLER)
    require(len(executable) == index["bytes"] and digest(executable) == index["sha256"], "installer bytes differ from index")
    validate_pe(executable, installer=True)
    checksum = read_regular(directory / (index["file"] + ".sha256"), 1024)
    require(checksum == f"{index['sha256']}  {index['file']}\n".encode("ascii"), "installer checksum file differs")
    archive = embedded_archive(executable, index["archiveSha256"])
    verify_archive(archive, index["version"], index["cliCommit"], index["archiveSha256"], index["cliBinarySha256"])
    verify_embedded_provenance(executable, index)
    return index


def validate_checkout(commit):
    actual = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    require(actual == commit, "installer source commit differs from checkout")
    changed = subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=normal"], cwd=ROOT, text=True).strip()
    require(not changed, "installer packaging requires a clean source checkout")


def write_immutable(path, contents):
    if path.exists() or path.is_symlink():
        require(read_regular(path, MAX_INSTALLER) == contents, "existing package file differs: " + path.name)
    else:
        with path.open("xb") as output:
            output.write(contents)


def package(installer, archive, output, installer_commit, cli_commit, archive_sha256, binary_sha256, version):
    identity(installer_commit, "commit")
    identity(cli_commit, "commit")
    identity(archive_sha256, "sha256")
    identity(binary_sha256, "sha256")
    validate_checkout(installer_commit)
    executable = read_regular(installer, MAX_INSTALLER)
    index = {"schemaVersion": 1, "platform": "windows-x64", "version": version,
             "installerCommit": installer_commit, "cliCommit": cli_commit,
             "cliBinarySha256": binary_sha256, "archiveSha256": archive_sha256,
             "file": f"MiySetup-{version}-windows-x64.exe", "sha256": digest(executable), "bytes": len(executable)}
    validate_index(index)
    validate_pe(executable, installer=True)
    payload = read_regular(archive, MAX_ARCHIVE)
    verify_archive(payload, version, cli_commit, archive_sha256, binary_sha256)
    require(embedded_archive(executable, archive_sha256) == payload, "installer embeds another CLI payload")
    verify_embedded_provenance(executable, index)
    output.mkdir(parents=True, exist_ok=True)
    write_immutable(output / index["file"], executable)
    write_immutable(output / (index["file"] + ".sha256"), f"{index['sha256']}  {index['file']}\n".encode("ascii"))
    write_immutable(output / "installer-index.json", metadata_bytes(index))
    return verify(output, installer_commit, cli_commit)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    build = commands.add_parser("package")
    build.add_argument("--installer", type=Path, required=True)
    build.add_argument("--cli-archive", type=Path, required=True)
    build.add_argument("--output", type=Path, required=True)
    build.add_argument("--installer-commit", required=True)
    build.add_argument("--cli-commit", required=True)
    build.add_argument("--archive-sha256", required=True)
    build.add_argument("--cli-binary-sha256", required=True)
    build.add_argument("--version", required=True)
    check = commands.add_parser("verify")
    check.add_argument("--directory", type=Path, required=True)
    check.add_argument("--installer-commit")
    check.add_argument("--cli-commit")
    args = parser.parse_args()
    if args.command == "package":
        index = package(args.installer, args.cli_archive, args.output, args.installer_commit,
                        args.cli_commit, args.archive_sha256, args.cli_binary_sha256, args.version)
    else:
        index = verify(args.directory, args.installer_commit, args.cli_commit)
    print(json.dumps({"verified": True, **index}))


if __name__ == "__main__":
    main()
