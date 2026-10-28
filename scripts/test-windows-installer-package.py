#!/usr/bin/env python3
"""Regression checks for installer integrity, CLI binding, and immutable staging."""
import importlib.util
import io
import json
import os
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch
import zipfile


def load_module(filename):
    spec = importlib.util.spec_from_file_location(filename.replace("-", "_"), Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


PACKAGE = load_module("package-windows-installer.py")
STAGE = load_module("stage-windows-installer.py") if os.name != "nt" else None
INSTALLER_COMMIT = "a" * 40
CLI_COMMIT = "b" * 40


def pe(gui=False):
    data = bytearray(512)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 60, 64)
    data[64:68] = b"PE\0\0"
    struct.pack_into("<H", data, 68, 0x8664)
    struct.pack_into("<H", data, 84, 240)
    struct.pack_into("<H", data, 88, 0x20B)
    struct.pack_into("<H", data, 156, 2 if gui else 3)
    return bytes(data)


def archive(binary=None, cli_commit=CLI_COMMIT, extra=None):
    binary = binary or pe()
    entries = {name: b"# synthetic script\n" for name in PACKAGE.ARCHIVE_NAMES}
    entries["miy.exe"] = binary
    entries["release.json"] = PACKAGE.metadata_bytes({"schemaVersion": 1, "version": "0.6.0",
        "target": "x86_64-pc-windows-msvc", "commit": cli_commit, "binarySha256": PACKAGE.digest(binary)})
    if extra:
        entries.update(extra)
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as writer:
        for name, data in sorted(entries.items()):
            writer.writestr(name, data)
    return output.getvalue()


class InstallerPackageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="miy-installer-package-")
        self.root = Path(self.temporary.name)
        self.payload = archive()
        self.provenance = PACKAGE.provenance_bytes({"version": "0.6.0", "installerCommit": INSTALLER_COMMIT,
            "cliCommit": CLI_COMMIT, "archiveSha256": PACKAGE.digest(self.payload), "cliBinarySha256": PACKAGE.digest(pe())})
        self.executable = (pe(gui=True) + b"managed resource header\0" + self.payload
                           + b"resource footer\0" + self.provenance)
        self.installer = self.root / "build.exe"
        self.installer.write_bytes(self.executable)
        self.archive = self.root / "cli.zip"
        self.archive.write_bytes(self.payload)
        self.output = self.root / "package"

    def tearDown(self):
        self.temporary.cleanup()

    def package(self, commit=INSTALLER_COMMIT):
        if commit != INSTALLER_COMMIT:
            self.installer.write_bytes(self.installer.read_bytes().replace(INSTALLER_COMMIT.encode(), commit.encode()))
        with patch.object(PACKAGE, "validate_checkout"):
            return PACKAGE.package(self.installer, self.archive, self.output, commit, CLI_COMMIT,
                                   PACKAGE.digest(self.payload), PACKAGE.digest(pe()), "0.6.0")

    def write_index(self, index):
        (self.output / "installer-index.json").write_bytes(PACKAGE.metadata_bytes(index))

    def replace_executable(self, index, contents):
        (self.output / index["file"]).write_bytes(contents)
        index["sha256"], index["bytes"] = PACKAGE.digest(contents), len(contents)
        (self.output / (index["file"] + ".sha256")).write_text(f"{index['sha256']}  {index['file']}\n", encoding="ascii")
        self.write_index(index)

    def test_package_and_verify_keep_distinct_source_commits(self):
        index = self.package()
        self.assertEqual(index["installerCommit"], INSTALLER_COMMIT)
        self.assertEqual(index["cliCommit"], CLI_COMMIT)
        self.assertEqual(PACKAGE.verify(self.output, INSTALLER_COMMIT, CLI_COMMIT), index)
        self.assertEqual(self.package(), index)

    def test_archive_hash_rejected_before_packaging(self):
        self.archive.write_bytes(self.payload + b"changed")
        with self.assertRaisesRegex(ValueError, "archive checksum"):
            self.package()

    def test_embedded_payload_required(self):
        self.installer.write_bytes(pe(gui=True))
        with self.assertRaisesRegex(ValueError, "exactly one"):
            self.package()

    def test_duplicate_embedded_payload_rejected(self):
        self.installer.write_bytes(self.executable + self.payload)
        with self.assertRaisesRegex(ValueError, "exactly one"):
            self.package()

    def test_installer_tampering_rejected(self):
        index = self.package()
        (self.output / index["file"]).write_bytes(self.executable + b"tampering")
        with self.assertRaisesRegex(ValueError, "bytes differ"):
            PACKAGE.verify(self.output)

    def test_relabeling_cli_provenance_rejected(self):
        index = self.package()
        index["cliCommit"] = "c" * 40
        self.write_index(index)
        with self.assertRaisesRegex(ValueError, "provenance differs"):
            PACKAGE.verify(self.output)

    def test_relabeling_installer_provenance_rejected(self):
        index = self.package()
        index["installerCommit"] = "c" * 40
        self.write_index(index)
        with self.assertRaisesRegex(ValueError, "embedded installer provenance"):
            PACKAGE.verify(self.output)

    def test_duplicate_installer_provenance_rejected(self):
        self.installer.write_bytes(self.executable + self.provenance)
        with self.assertRaisesRegex(ValueError, "embedded installer provenance"):
            self.package()

    def test_wrong_binary_hash_rejected(self):
        index = self.package()
        index["cliBinarySha256"] = "c" * 64
        self.write_index(index)
        with self.assertRaisesRegex(ValueError, "provenance differs"):
            PACKAGE.verify(self.output)

    def test_wrong_installer_architecture_rejected_even_with_matching_outer_hash(self):
        index = self.package()
        wrong = bytearray(self.executable)
        struct.pack_into("<H", wrong, 68, 0x14C)
        self.replace_executable(index, bytes(wrong))
        with self.assertRaisesRegex(ValueError, "AMD64"):
            PACKAGE.verify(self.output)

    def test_cli_architecture_rejected(self):
        wrong = bytearray(pe())
        struct.pack_into("<H", wrong, 68, 0x14C)
        payload = archive(bytes(wrong))
        with self.assertRaisesRegex(ValueError, "AMD64"):
            PACKAGE.verify_archive(payload, "0.6.0", CLI_COMMIT, PACKAGE.digest(payload), PACKAGE.digest(wrong))

    def test_archive_paths_and_duplicate_entries_rejected(self):
        payload = archive(extra={"../escape": b"test"})
        with self.assertRaisesRegex(ValueError, "unexpected or duplicate"):
            PACKAGE.verify_archive(payload, "0.6.0", CLI_COMMIT, PACKAGE.digest(payload), PACKAGE.digest(pe()))
        source = io.BytesIO(self.payload)
        with zipfile.ZipFile(source, "a") as writer:
            import warnings
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", UserWarning)
                writer.writestr("miy.exe", pe())
        payload = source.getvalue()
        with self.assertRaisesRegex(ValueError, "unexpected or duplicate"):
            PACKAGE.verify_archive(payload, "0.6.0", CLI_COMMIT, PACKAGE.digest(payload), PACKAGE.digest(pe()))

    def test_index_rejects_path_traversal_unknown_fields_boolean_size_and_oversize(self):
        index = self.package()
        for replacement in ({"file": "../escape.exe"}, {"unexpected": True}, {"bytes": True},
                            {"bytes": PACKAGE.MAX_INSTALLER + 1}, {"schemaVersion": True}):
            with self.subTest(replacement=replacement):
                self.write_index({**index, **replacement})
                with self.assertRaises(ValueError):
                    PACKAGE.verify(self.output)

    def test_index_duplicate_keys_rejected(self):
        self.package()
        path = self.output / "installer-index.json"
        path.write_text(path.read_text().replace('{', '{"schemaVersion": 1,', 1))
        with self.assertRaisesRegex(ValueError, "duplicate JSON"):
            PACKAGE.verify(self.output)

    def test_checksum_sidecar_rejected(self):
        index = self.package()
        (self.output / (index["file"] + ".sha256")).write_text("0" * 64)
        with self.assertRaisesRegex(ValueError, "checksum file"):
            PACKAGE.verify(self.output)

    @unittest.skipIf(os.name == "nt", "Linux publication contract")
    def test_symlink_payload_rejected(self):
        index = self.package()
        path = self.output / index["file"]
        path.unlink()
        path.symlink_to(self.installer)
        with self.assertRaisesRegex(ValueError, "regular file"):
            PACKAGE.verify(self.output)

    @unittest.skipIf(os.name == "nt", "Linux publication contract")
    def test_stage_is_repeatable_and_keeps_old_installer(self):
        index = self.package()
        destination = self.root / "published"
        first = STAGE.stage(self.output, destination, INSTALLER_COMMIT, CLI_COMMIT)
        self.assertTrue(first["staged"])
        self.assertEqual(STAGE.stage(self.output, destination, INSTALLER_COMMIT, CLI_COMMIT), first)
        old_bytes = (destination / INSTALLER_COMMIT / index["file"]).read_bytes()
        second_commit = "c" * 40
        self.output = self.root / "second"
        self.package(second_commit)
        STAGE.stage(self.output, destination, second_commit, CLI_COMMIT)
        self.assertEqual(json.loads((destination / "installer-index.json").read_bytes())["installerCommit"], second_commit)
        self.assertEqual((destination / INSTALLER_COMMIT / index["file"]).read_bytes(), old_bytes)
        self.assertFalse(list(self.root.glob(".miy-installer-*")))

    @unittest.skipIf(os.name == "nt", "Linux publication contract")
    def test_stage_rejects_changed_same_commit_without_changing_pointer(self):
        self.package()
        destination = self.root / "published"
        STAGE.stage(self.output, destination, INSTALLER_COMMIT, CLI_COMMIT)
        old_pointer = (destination / "installer-index.json").read_bytes()
        self.output = self.root / "different-package"
        self.installer.write_bytes(self.executable + b"new installer")
        self.package()
        with self.assertRaisesRegex(ValueError, "immutable installer bytes"):
            STAGE.stage(self.output, destination, INSTALLER_COMMIT, CLI_COMMIT)
        self.assertEqual((destination / "installer-index.json").read_bytes(), old_pointer)
        self.assertFalse(list(self.root.glob(".miy-installer-*")))

    @unittest.skipIf(os.name == "nt", "Linux publication contract")
    def test_stage_rejects_corruption_and_wrong_identity_without_publishing(self):
        index = self.package()
        destination = self.root / "published"
        with self.assertRaisesRegex(ValueError, "source commit differs"):
            STAGE.stage(self.output, destination, "c" * 40, CLI_COMMIT)
        (self.output / index["file"]).write_bytes(b"corrupt")
        with self.assertRaisesRegex(ValueError, "bytes differ"):
            STAGE.stage(self.output, destination, INSTALLER_COMMIT, CLI_COMMIT)
        self.assertFalse((destination / "installer-index.json").exists())


if __name__ == "__main__":
    unittest.main()
