#!/usr/bin/env python3
"""Check native CLI scaffolds and compare their bundle hashes across OS jobs.

This is an offline build/scaffolding check, not Windows Docker Desktop E2E.
SQL bytes are compared to the Git index and are never normalized or rewritten.
Generated projects are also committed and cloned with core.autocrlf=true to
exercise their managed .gitattributes policy without changing global Git config.
"""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
TEMPLATE = ROOT / "templates" / "rust-react"
EXCLUDED = {"node_modules", "target", "dist", ".git", ".pnpm-store", ".miy", ".miy-tools"}
APP_ID = "portability-check"
APP_NAME = "번들 호환 검사"


def sha256(contents):
    return hashlib.sha256(contents).hexdigest()


def source_files():
    files = {}
    for path in sorted(TEMPLATE.rglob("*")):
        relative = path.relative_to(TEMPLATE)
        if any(part in EXCLUDED for part in relative.parts):
            continue
        if path.is_symlink():
            raise ValueError(f"template_symlink: {relative.as_posix()}")
        if path.is_dir():
            continue
        if not path.is_file() or "\\" in relative.as_posix():
            raise ValueError(f"invalid_template_path: {relative.as_posix()}")
        contents = path.read_bytes()
        contents.decode("utf-8")
        if path.suffix != ".sql" and b"\r" in contents:
            raise ValueError(f"managed_text_requires_lf: {relative.as_posix()}")
        files[relative.as_posix()] = contents
    if not files:
        raise ValueError("template_is_empty")
    return files


def check_sources():
    files = source_files()
    guidance = ROOT / "crates" / "miy" / "guidance" / "miy-cli"
    for path in [guidance / "SKILL.md", guidance / "references" / "workflow.md"]:
        contents = path.read_bytes()
        contents.decode("utf-8")
        if b"\r" in contents:
            raise ValueError(f"managed_text_requires_lf: {path.relative_to(ROOT).as_posix()}")
    tracked_sql = subprocess.check_output(
        ["git", "ls-files", "-z", "--", "*.sql"], cwd=ROOT
    ).split(b"\0")
    for name in filter(None, tracked_sql):
        relative = name.decode("utf-8")
        indexed = subprocess.check_output(["git", "show", f":{relative}"], cwd=ROOT)
        if (ROOT / relative).read_bytes() != indexed:
            raise ValueError(f"sql_checkout_bytes_differ_from_index: {relative}")
    return files


def managed_guidance(project):
    lock = json.loads((project / "miy.cli.json").read_bytes())
    hashes = lock["files"]
    if ".gitattributes" not in hashes:
        raise ValueError("git_attributes_not_managed_by_cli")
    for relative, expected in hashes.items():
        if (
            not relative
            or relative.startswith("/")
            or "\\" in relative
            or ".." in relative.split("/")
        ):
            raise ValueError(f"invalid_cli_guidance_path: {relative}")
        actual = (project / relative).read_bytes()
        if b"\r" in actual:
            raise ValueError(f"cli_guidance_requires_lf: {relative}")
        if sha256(actual) != expected:
            raise ValueError(f"cli_guidance_hash_mismatch: {relative}")
    return hashes


def git_checkout_round_trip(project, destination, protected, guidance):
    fixture = project / "migrations" / "9999_portability_crlf.sql"
    if fixture.exists():
        raise ValueError("portability_fixture_path_already_exists")
    fixture.write_bytes(b"-- Byte preservation fixture only; never executed.\r\nSELECT 1;\r\n")
    component = project / "platform-components" / "portability-fixture" / "Component.tsx"
    if component.exists():
        raise ValueError("component_portability_fixture_path_already_exists")
    component.parent.mkdir(parents=True, exist_ok=True)
    component.write_bytes(b"// Immutable component byte-preservation fixture.\r\nexport const fixture = 1;\r\n")
    relative_sql = {path.relative_to(project).as_posix() for path in project.rglob("*.sql")}
    checked = set(protected) | set(guidance) | relative_sql | {
        "miy.bundle.json",
        "miy.cli.json",
        component.relative_to(project).as_posix(),
    }
    original = {relative: (project / relative).read_bytes() for relative in checked}

    def git(*arguments, cwd=project):
        try:
            subprocess.run(
                ["git", "-c", "core.autocrlf=true", *arguments],
                cwd=cwd,
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
        except subprocess.CalledProcessError as error:
            detail = error.stderr.decode("utf-8", errors="replace").strip()
            raise RuntimeError(f"git_portability_check_failed: {detail}") from error

    git("init", "--quiet")
    git("add", "--all")
    git(
        "-c", "user.name=MIY portability check",
        "-c", "user.email=miy-portability@example.invalid",
        "-c", "commit.gpgSign=false",
        "commit", "--quiet", "-m", "Check generated bundle checkout bytes",
    )
    destination.parent.mkdir(parents=True, exist_ok=True)
    git(
        "clone", "--quiet", "--no-hardlinks", "-c", "core.autocrlf=true",
        str(project), str(destination),
        cwd=project.parent,
    )
    for relative, contents in original.items():
        if (destination / relative).read_bytes() != contents:
            raise ValueError(f"autocrlf_checkout_changed_preserved_bytes: {relative}")
    if managed_guidance(destination) != guidance:
        raise ValueError("autocrlf_checkout_changed_cli_lock")
    print(f"Verified autocrlf=true Git checkout preserves {len(checked)} managed/SQL/component files")


def check_cli(cli, destination):
    files = check_sources()
    cli = cli.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="miy-portability-") as temporary:
        project = Path(temporary) / "한글 경로" / "app with spaces"
        subprocess.run(
            [str(cli), "app", "init", str(project), "--id", APP_ID, "--name", APP_NAME],
            cwd=ROOT,
            check=True,
            stdout=subprocess.PIPE,
        )
        scaffold = {}
        for relative, contents in files.items():
            expected = (
                contents.decode("utf-8")
                .replace("{{APP_ID}}", APP_ID)
                .replace("{{APP_MODULE}}", APP_ID.replace("-", "_"))
                .replace("{{APP_NAME}}", APP_NAME)
                .encode("utf-8")
            )
            actual = (project / relative).read_bytes()
            # init serializes manifest values; compare its contract, then hash actual bytes.
            if relative == "manifest.json":
                if json.loads(actual) != json.loads(expected):
                    raise ValueError("scaffold_manifest_mismatch")
            elif actual != expected:
                raise ValueError(f"embedded_template_mismatch: {relative}")
            scaffold[relative] = sha256(actual)
        lock = json.loads((project / "miy.bundle.json").read_bytes())
        for relative, expected in lock["protectedFiles"].items():
            if relative not in files or "\\" in relative:
                raise ValueError(f"invalid_protected_path: {relative}")
            if sha256(files[relative]) != expected:
                raise ValueError(f"protected_source_hash_mismatch: {relative}")
            if sha256((project / relative).read_bytes()) != expected:
                raise ValueError(f"protected_scaffold_hash_mismatch: {relative}")
        guidance = managed_guidance(project)
        git_checkout_round_trip(
            project,
            Path(temporary) / "다른 경로" / "autocrlf checkout",
            lock["protectedFiles"],
            guidance,
        )
        result = {
            "bundleVersion": lock["bundleVersion"],
            "protectedFiles": lock["protectedFiles"],
            "embeddedScaffoldFiles": scaffold,
            "gitAttributes": guidance[".gitattributes"],
            "cliGuidanceFiles": guidance,
        }
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(
        (json.dumps(result, ensure_ascii=True, sort_keys=True, indent=2) + "\n").encode("utf-8")
    )
    print(f"Verified {len(scaffold)} embedded files; manifest: {destination}")


def compare_manifests(paths):
    if len(paths) < 2:
        raise ValueError("at_least_two_os_manifests_required")
    expected = json.loads(paths[0].read_bytes())
    for path in paths[1:]:
        if json.loads(path.read_bytes()) != expected:
            raise ValueError(f"bundle_hashes_differ: {paths[0]} vs {path}")
    print(f"Identical embedded bundle/scaffold hashes across {len(paths)} native builds")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="command", required=True)
    subcommands.add_parser("source", help="Check source LF policy and unchanged SQL checkout bytes")
    check = subcommands.add_parser("check", help="Run the native CLI's offline app init and hash output")
    check.add_argument("--cli", type=Path, required=True)
    check.add_argument("--output", type=Path, required=True)
    compare = subcommands.add_parser("compare", help="Compare manifests from separate native OS builds")
    compare.add_argument("manifests", type=Path, nargs="+")
    args = parser.parse_args()
    if args.command == "source":
        files = check_sources()
        print(f"Verified LF policy for {len(files)} template files and preserved tracked SQL bytes")
    elif args.command == "check":
        check_cli(args.cli, args.output)
    else:
        compare_manifests(args.manifests)


if __name__ == "__main__":
    main()
