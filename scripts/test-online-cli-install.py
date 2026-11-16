#!/usr/bin/env python3
"""Exercise the online shell installer with isolated homes and HTTP packages."""
import hashlib
import http.server
import io
import os
from pathlib import Path
import shlex
import subprocess
import tarfile
import tempfile
import threading
import unittest

ROOT = Path(__file__).resolve().parents[1]


class OnlineInstall(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="miy-online-test-")
        self.root = Path(self.temp.name)
        self.home = self.root / "home with spaces"
        self.home.mkdir()
        self.prefix = self.home / "bin with spaces"
        self.env = {**os.environ, "HOME": str(self.home), "SHELL": "/bin/bash",
                    "MIY_INSTALL_DIR": str(self.prefix), "MIY_CONFIG_DIR": str(self.home / "config")}
        for name in ("BASH_ENV", "ENV", "XDG_CONFIG_HOME", "ZDOTDIR"):
            self.env.pop(name, None)
        # Test the real packaged installer, with a tiny CLI to observe its calls.
        binary = b'#!/usr/bin/env bash\nif [ "$1" = --version ]; then echo "miy 9.9.9"; else printf "%s\\n" "$@" >> "$HOME/cli-calls"; echo "{}"; fi\n'
        self.binary_hash = hashlib.sha256(binary).hexdigest()
        buffer = io.BytesIO()
        with tarfile.open(fileobj=buffer, mode="w:gz") as archive:
            for name, data, mode in [("miy", binary, 0o755), ("install-cli.sh", (ROOT / "scripts/install-cli.sh").read_bytes(), 0o755), ("release.json", b"{}", 0o644)]:
                item = tarfile.TarInfo(name)
                item.size, item.mode = len(data), mode
                archive.addfile(item, io.BytesIO(data))
        self.package = buffer.getvalue()
        self.expected_hash = hashlib.sha256(self.package).hexdigest()
        self.response = self.package
        self.paths = []
        fixture = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                fixture.paths.append(self.path)
                self.send_response(200)
                self.end_headers()
                self.wfile.write(fixture.response)

            def log_message(self, *args):
                pass

        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.origin = f"http://127.0.0.1:{self.server.server_port}"
        self.script = self.root / "install.sh"
        self.render()

    def render(self):
        # Values use the same fixed template slots as Core; Rust separately
        # tests origin canonicalization, all targets, quoting and HTTP routes.
        cases = []
        for pattern, asset in [("Linux:x86_64", "linux-x64"), ("Linux:aarch64|Linux:arm64", "linux-arm64"), ("Darwin:x86_64", "mac-x64"), ("Darwin:arm64", "mac-arm64")]:
            cases.append(f"{pattern}) url={shlex.quote(self.origin + '/' + asset)}; sha256={self.expected_hash}; binary_sha256={self.binary_hash} ;;")
        script = (ROOT / "scripts/install-cli-online.sh").read_text()
        for marker, value in {"@MIY_SERVER@": shlex.quote(self.origin), "@MIY_VERSION@": "'9.9.9'", "# @MIY_ASSETS@": "\n".join(cases)}.items():
            script = script.replace(marker, value)
        self.script.write_text(script)

    def run_install(self, *args):
        return subprocess.run(["bash", str(self.script), *args], env=self.env, capture_output=True, text=True, timeout=30)

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()
        self.temp.cleanup()

    def test_download_verify_register_and_idempotent_user_path(self):
        config = self.home / "config"
        config.mkdir()
        saved = config / "credentials.json"
        saved.write_text("existing-credential-sentinel")
        profile = self.home / ".profile"
        profile.write_text("# user's existing configuration\n")
        for _ in range(2):
            result = self.run_install()
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.prefix / "miy").is_file())
        self.assertEqual(saved.read_text(), "existing-credential-sentinel")
        self.assertNotIn("existing-credential-sentinel", result.stdout + result.stderr)
        self.assertEqual(profile.read_text().count("# miy CLI"), 1)
        self.assertTrue(profile.read_text().startswith("# user's existing configuration\n"))
        calls = (self.home / "cli-calls").read_text()
        self.assertIn("context\ninit\n--url\n" + self.origin, calls)
        self.assertNotIn("auth", calls)
        self.assertEqual(len(self.paths), 2)

    def test_archive_tamper_preserves_existing_install(self):
        self.prefix.mkdir()
        installed = self.prefix / "miy"
        installed.write_text("old binary")
        self.response = b"tampered archive"
        result = self.run_install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("checksum mismatch", result.stderr)
        self.assertEqual(installed.read_text(), "old binary")
        self.assertFalse((self.home / "cli-calls").exists())
        self.assertFalse((self.home / ".profile").exists())

    def test_binary_hash_checked_before_any_execution(self):
        self.binary_hash = "0" * 64
        self.render()
        result = self.run_install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("binary checksum mismatch", result.stderr)
        self.assertFalse((self.home / "cli-calls").exists())
        self.assertFalse(self.prefix.exists())

    def test_platform_selection_and_no_profile_changes(self):
        tools = self.root / "tools"
        tools.mkdir()
        uname = tools / "uname"
        uname.write_text('#!/bin/sh\ncase "$1" in -s) echo Darwin;; -m) echo arm64;; esac\n')
        uname.chmod(0o755)
        self.env["PATH"] = str(tools) + os.pathsep + self.env["PATH"]
        result = self.run_install("--no-modify-path")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.paths, ["/mac-arm64"])
        self.assertFalse((self.home / ".profile").exists())
        uname.write_text('#!/bin/sh\necho unsupported\n')
        result = self.run_install()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.paths, ["/mac-arm64"])


if __name__ == "__main__":
    unittest.main()
