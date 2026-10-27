#!/usr/bin/env python3
"""Release gates reject wrong CPUs, incomplete targets and unverified evidence."""
import copy
import importlib.util
import struct
import sys
import unittest
from pathlib import Path

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location("package_cli_release", Path(__file__).with_name("package-cli-release.py"))
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseGates(unittest.TestCase):
    def test_cpu_and_executable_headers_must_match(self):
        elf = bytearray(64)
        elf[:6] = b"\x7fELF\x02\x01"
        struct.pack_into("<H", elf, 18, 62)
        release.validate_binary(elf, "x86_64-unknown-linux-gnu")
        with self.assertRaises(ValueError):
            release.validate_binary(elf, "aarch64-unknown-linux-gnu")
        with self.assertRaises(ValueError):
            release.validate_binary(elf, "x86_64-pc-windows-msvc")
        pe = bytearray(128)
        pe[:2] = b"MZ"
        struct.pack_into("<I", pe, 60, 64)
        pe[64:68] = b"PE\0\0"
        struct.pack_into("<H", pe, 68, 0x8664)
        release.validate_binary(pe, "x86_64-pc-windows-msvc")
        struct.pack_into("<H", pe, 68, 0xAA64)
        with self.assertRaises(ValueError):
            release.validate_binary(pe, "x86_64-pc-windows-msvc")

    def evidence(self):
        rows = []
        for requirement in release.REQUIRED_EVIDENCE:
            row = {"requirement": requirement, "status": "PASS", "commit": "a" * 40,
                   "evidence": "fixture:evidence", "redactionPassed": True}
            if requirement.startswith(("codex-", "claude-")):
                row.update(freshSession=True, sessionId=requirement, calls=1, readBytes=12,
                           manualInterventions=0, recoverySeconds=1, unexpectedChanges=0,
                           permissionBypasses=0, duplicateOperations=0)
            rows.append(row)
        return {"schemaVersion": 1, "candidateCommit": "a" * 40, "cliVersion": "0.5.0", "records": rows}

    def gate(self, evidence, commit="a" * 40):
        release.verify_evidence(evidence, {"commit": "a" * 40, "version": "0.5.0"}, "v0.5.0", commit)

    def test_pending_windows_and_wrong_commit_never_publish(self):
        self.gate(self.evidence())
        for change in ("NOT_RUN", "FAIL"):
            evidence = self.evidence()
            next(row for row in evidence["records"] if row["requirement"] == "windows11-powershell51")["status"] = change
            with self.assertRaises(ValueError):
                self.gate(evidence)
        with self.assertRaises(ValueError):
            self.gate(self.evidence(), "b" * 40)
        evidence = self.evidence()
        evidence["records"].pop()
        with self.assertRaises(ValueError):
            self.gate(evidence)

    def test_duplicate_ai_sessions_and_unintended_operations_block_release(self):
        evidence = self.evidence()
        ai = [row for row in evidence["records"] if row["requirement"].startswith("codex-")]
        ai[1]["sessionId"] = ai[0]["sessionId"]
        with self.assertRaises(ValueError):
            self.gate(evidence)
        for field, value in (("freshSession", False), ("permissionBypasses", 1), ("duplicateOperations", 1), ("redactionPassed", False)):
            example = copy.deepcopy(self.evidence())
            row = next(row for row in example["records"] if row["requirement"] == "claude-windows")
            row[field] = value
            with self.assertRaises(ValueError):
                self.gate(example)


if __name__ == "__main__":
    unittest.main()
