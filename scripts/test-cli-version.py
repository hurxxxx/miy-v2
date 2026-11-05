#!/usr/bin/env python3
"""The CLI version gate counts client build inputs and ignores server-only modules."""
import importlib.util
import pathlib
import subprocess
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location("check_cli_version", pathlib.Path(__file__).with_name("check-cli-version.py"))
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)

MAIN = """#[cfg(feature = "server")]
mod runtime;
mod cli_remote;
#[cfg(all(test, feature = "server"))]
mod runtime_regression;
fn main() {}
"""


class CliVersionGate(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.directory.name)
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.email", "test@example.invalid")
        self.git("config", "user.name", "test")
        self.write("crates/miy/src/main.rs", MAIN)
        for name in ("runtime", "cli_remote", "runtime_regression"):
            self.write(f"crates/miy/src/{name}.rs", "// v1\n")
        self.write("templates/rust-react/AGENTS.md", "v1\n")
        self.write("docs/notes.md", "v1\n")
        self.published = {"version": "0.7.0", "sourceCommit": self.commit("0.7.0")}

    def tearDown(self):
        self.directory.cleanup()

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.root, check=True, capture_output=True, text=True).stdout.strip()

    def write(self, path, text):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)

    def commit(self, version):
        self.write("crates/miy/Cargo.toml", f'[package]\nname = "miy"\nversion = "{version}"\n')
        self.git("add", "-A")
        self.git("commit", "-q", "--allow-empty", "-m", version)
        return self.git("rev-parse", "HEAD")

    def check(self, candidate="HEAD"):
        return gate.check(self.root, self.published, candidate)

    def test_the_published_source_itself_passes(self):
        result = self.check()
        self.assertEqual((result["status"], result["clientInputsChanged"]), ("PASS", []))

    def test_server_only_documentation_and_server_test_changes_keep_the_version(self):
        self.write("crates/miy/src/runtime.rs", "// v2\n")
        self.write("crates/miy/src/runtime_regression.rs", "// v2\n")
        self.write("docs/notes.md", "v2\n")
        self.commit("0.7.0")
        self.assertEqual(self.check()["status"], "PASS")

    def test_shared_module_or_embedded_template_change_requires_a_new_version(self):
        for path in ("crates/miy/src/cli_remote.rs", "templates/rust-react/AGENTS.md", "Cargo.lock"):
            with self.subTest(path=path):
                self.write(path, "changed " + path + "\n")
                self.commit("0.7.0")
                result = self.check()
                self.assertEqual(result["status"], "FAIL")
                self.assertEqual(result["reason"], "client_inputs_changed_without_version_bump")
                self.assertIn(path, result["clientInputsChanged"])

    def test_a_higher_version_passes_and_a_lower_one_fails(self):
        self.write("crates/miy/src/cli_remote.rs", "// v2\n")
        self.commit("0.7.1")
        self.assertEqual(self.check()["status"], "PASS")
        self.commit("0.6.9")
        self.assertEqual(self.check()["reason"], "candidate_version_lower_than_published")

    def test_a_new_server_only_module_does_not_count_but_a_new_shared_one_does(self):
        self.write("crates/miy/src/main.rs", MAIN + '#[cfg(feature = "server")]\nmod audit;\n')
        self.write("crates/miy/src/audit.rs", "// server only\n")
        self.commit("0.7.0")
        result = self.check()
        self.assertNotIn("crates/miy/src/audit.rs", result["clientInputsChanged"])
        self.assertEqual(result["clientInputsChanged"], ["crates/miy/src/main.rs"])
        self.write("crates/miy/src/main.rs", MAIN + "mod shared;\n")
        self.write("crates/miy/src/shared.rs", "// client\n")
        self.commit("0.7.0")
        self.assertIn("crates/miy/src/shared.rs", self.check()["clientInputsChanged"])

    def test_a_removed_client_module_counts(self):
        (self.root / "crates/miy/src/cli_remote.rs").unlink()
        self.write("crates/miy/src/main.rs", MAIN.replace("mod cli_remote;\n", ""))
        self.commit("0.7.0")
        self.assertIn("crates/miy/src/cli_remote.rs", self.check()["clientInputsChanged"])

    def test_a_module_that_becomes_shared_counts_as_client_input(self):
        self.write("crates/miy/src/main.rs", MAIN.replace('#[cfg(feature = "server")]\nmod runtime;', "mod runtime;"))
        self.write("crates/miy/src/runtime.rs", "// now in the client\n")
        self.commit("0.7.0")
        result = self.check()
        self.assertEqual(result["status"], "FAIL")
        self.assertIn("crates/miy/src/runtime.rs", result["clientInputsChanged"])

    def test_missing_history_or_a_mismatched_record_is_refused(self):
        with self.assertRaisesRegex(ValueError, "fetch the full history"):
            gate.check(self.root, {"version": "0.7.0", "sourceCommit": "0" * 40}, "HEAD")
        with self.assertRaisesRegex(ValueError, "differ"):
            gate.check(self.root, {**self.published, "version": "0.6.3"}, "HEAD")


if __name__ == "__main__":
    unittest.main()
