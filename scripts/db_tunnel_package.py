#!/usr/bin/env python3
"""Package immutable native Linux DB tunnel helpers alongside a CLI candidate."""
import argparse
import hashlib
import json
import re
import subprocess
import tarfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PLATFORMS = ("linux/amd64", "linux/arm64")


def require(test, message):
    if not test:
        raise ValueError(message)


def command(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def archive_identity(path, platform, commit, tag=None):
    """Use config bytes, not Docker's storage-backend-dependent inspect .Id."""
    with tarfile.open(path, "r:") as archive:
        def member(name, limit):
            require(isinstance(name, str) and not name.startswith("/")
                    and all(part not in ("", ".", "..") for part in name.split("/"))
                    and "\\" not in name, "invalid helper archive member")
            entries = [entry for entry in archive.getmembers() if entry.name == name]
            require(len(entries) == 1 and entries[0].isfile()
                    and entries[0].size <= limit, "missing, duplicate or invalid helper archive member")
            return archive.extractfile(entries[0]).read()

        manifest = json.loads(member("manifest.json", 64 * 1024))
        require(isinstance(manifest, list) and len(manifest) == 1, "helper must contain exactly one image")
        if tag is not None:
            require(manifest[0].get("RepoTags") == [tag], "helper archive tag differs")
        data = member(manifest[0].get("Config"), 2 * 1024 * 1024)
        config = json.loads(data)
        require(f"{config.get('os')}/{config.get('architecture')}" == platform, "helper config platform differs")
        labels = config.get("config", {}).get("Labels", {})
        require(labels.get("org.miy.component") == "db-tunnel"
                and labels.get("org.opencontainers.image.revision") == commit, "helper config provenance differs")
        return hashlib.sha256(data).hexdigest()


def build(platform, output, development=False):
    commit = command("git", "rev-parse", "HEAD")
    version = tomllib.loads((ROOT / "crates/miy/Cargo.toml").read_text())["package"]["version"]
    if not development:
        require(not command("git", "status", "--porcelain", "--untracked-files=normal"), "candidate helpers require a clean checkout")
    arch = platform.split("/")[1]
    tag = f"miy-db-tunnel:{commit}-{arch}" + ("-development" if development else "")
    subprocess.run(["docker", "build", "--platform", platform, "--build-arg", f"MIY_SOURCE_COMMIT={commit}",
                    "-f", "ops/db-tunnel.Dockerfile", "-t", tag, "."], cwd=ROOT, check=True)
    meta = json.loads(command("docker", "image", "inspect", tag))[0]
    require(f"{meta['Os']}/{meta['Architecture']}" == platform, "helper architecture mismatch")
    require(command("docker", "run", "--rm", "--network", "none", tag, "--version") == f"miy {version}", "helper version mismatch")
    output.mkdir(parents=True, exist_ok=True)
    filename = f"miy-db-tunnel-{version}-{arch}.oci.tar"
    path = output / filename
    subprocess.run(["docker", "save", "--output", str(path), tag], check=True)
    config_sha256 = archive_identity(path, platform, commit, tag)
    row = {"schemaVersion": 1, "platform": platform, "version": version, "commit": commit,
           "imageId": meta["Id"], "imageTag": tag, "configSha256": config_sha256,
           "file": filename, "sha256": digest(path), "development": development}
    (output / f"tunnel-{arch}.json").write_text(json.dumps(row, indent=2) + "\n")
    (output / (filename + ".sha256")).write_text(f"{row['sha256']}  {filename}\n")
    print(json.dumps(row))


def verify_helpers(directory, commit, version):
    rows = []
    for platform in PLATFORMS:
        arch = platform.split("/")[1]
        row = json.loads((directory / f"tunnel-{arch}.json").read_text())
        filename = f"miy-db-tunnel-{version}-{arch}.oci.tar"
        require(row.get("schemaVersion") == 1 and row.get("platform") == platform
                and row.get("version") == version and row.get("commit") == commit
                and row.get("file") == filename and row.get("development") is False, "helper provenance differs")
        require(re.fullmatch(r"sha256:[a-f0-9]{64}", row.get("imageId", "")), "invalid helper image ID")
        require(row.get("imageTag") == f"miy-db-tunnel:{commit}-{arch}", "helper image tag differs")
        require(re.fullmatch(r"[a-f0-9]{64}", row.get("configSha256", "")), "invalid helper config hash")
        require(digest(directory / filename) == row.get("sha256"), "helper archive checksum differs")
        require(archive_identity(directory / filename, platform, commit, row["imageTag"])
                == row["configSha256"], "helper config checksum differs")
        require((directory / (filename + ".sha256")).read_text().strip() == f"{row['sha256']}  {filename}", "helper checksum file differs")
        rows.append(row)
    return rows


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--platform", required=True, choices=PLATFORMS)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--development", action="store_true", help="Unpublishable local test helper")
    args = parser.parse_args()
    build(args.platform, args.output, args.development)
