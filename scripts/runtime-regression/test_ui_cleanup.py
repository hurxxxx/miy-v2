#!/usr/bin/env python3
"""UI harness ownership regressions; no Docker daemon, server or real state used."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


SOURCE = Path(__file__).resolve().parent
NODE = shutil.which("node")
STUB = r'''
import json, os, sys
from pathlib import Path
name = Path(sys.argv[0]).name
args = sys.argv[1:]
data_file = Path(os.environ["UI_STUB_DATA"])
data = json.loads(data_file.read_text())
with Path(os.environ["UI_STUB_LOG"]).open("a") as log:
    log.write(json.dumps([name, *args]) + "\n")
def save():
    data_file.write_text(json.dumps(data))
def option(flag):
    return args[args.index(flag) + 1] if flag in args else ""
if name == "docker":
    if args[0] == "inspect":
        item = data["containers"].get(args[-1])
        if item is None: sys.exit(1)
        print(item["owner"] if "-f" in args else "{}")
    elif args[0] == "ps":
        if "--format" in args:
            for key in sorted(data["containers"]): print(key + " " + key + " running")
        else:
            filt = option("--filter")
            for key, item in data["containers"].items():
                if filt == "label=miy.server-ui" and item.get("kind") == "server": print(key)
                if filt == "label=com.docker.compose.project=miy-stack-ui" and item.get("owner") == "miy-stack-ui": print(key)
    elif args[:2] in (["network", "ls"], ["volume", "ls"]):
        key = "networks" if args[0] == "network" else "volumes"
        for item in data[key]:
            if "--filter" not in args or item.startswith("miy-stack-ui"):
                print(item + " " + item if "--format" in args else item)
    elif args[0] == "images":
        pass
    elif args[0] == "run":
        if os.environ.get("UI_FAIL_DOCKER_RUN"): sys.exit(23)
        key = option("--name")
        data["containers"][key] = {"kind": "server", "owner": option("--label").split("=", 1)[1]}
        save()
        print(key)
    elif args[0] == "rm":
        for key in args[1:]: data["containers"].pop(key, None)
        save()
    elif args[0] == "port":
        print("127.0.0.1:19999")
    elif args[0] == "exec":
        pass  # pg_isready and empty fixture DB queries
    elif args[0] == "compose":
        if "up" in args:
            data["containers"]["miy-stack-ui-postgres-1"] = {"owner": "miy-stack-ui"}
            data["networks"] = ["miy-stack-ui_default", "miy-stack-ui-app-runtime"]
            data["volumes"] = ["miy-stack-ui_pgdata"]
            save()
        elif "down" in args:
            data["containers"] = {k:v for k,v in data["containers"].items() if v.get("owner") != "miy-stack-ui"}
            data["networks"] = [n for n in data["networks"] if not n.startswith("miy-stack-ui")]
            data["volumes"] = [n for n in data["volumes"] if not n.startswith("miy-stack-ui")]
            save()
    else:
        sys.exit("unexpected mocked docker call")
elif name == "python3":
    if args[0].endswith("bootstrap.py"):
        if os.environ.get("UI_FAIL_BOOTSTRAP"): sys.exit(24)
        state = Path(option("--state-dir"))
        (state / "runtime.env").write_text("fixture only\n")
        (state / "compose.env").write_text("fixture only\n")
elif name == "pnpm":
    if os.environ.get("UI_FAIL_BUILD"): sys.exit(25)
    if "playwright" in args and os.environ.get("UI_FAIL_TESTS"): sys.exit(26)
elif name == "ss":
    if os.environ.get("UI_PORT_BUSY"): print("LISTEN")
elif name not in ("cargo", "curl", "miy"):
    sys.exit("unexpected mock executable")
'''


class CleanupOwnership(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="miy-ui-cleanup-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        scripts = self.root / "scripts/runtime-regression"
        scripts.mkdir(parents=True)
        for name in ("stack-ui.sh", "server-ui.mjs"):
            shutil.copyfile(SOURCE / name, scripts / name)
        (self.root / "scripts/run-service.sh").write_text("#!/bin/sh\nexit 0\n")
        self.bin = self.root / "stub-bin"
        self.bin.mkdir()
        for name in ("docker", "python3", "pnpm", "ss", "cargo", "curl"):
            self.executable(self.bin / name, STUB)
        binary = self.root / "target/debug/miy"
        binary.parent.mkdir(parents=True)
        self.executable(binary, STUB)
        self.data_file = self.root / "docker.json"
        self.data_file.write_text(json.dumps({"containers": {}, "networks": [], "volumes": []}))
        self.log_file = self.root / "commands.jsonl"
        self.env = {
            "PATH": f"{self.bin}:/usr/bin:/bin",
            "HOME": str(self.root / "home"),
            "UI_STUB_DATA": str(self.data_file),
            "UI_STUB_LOG": str(self.log_file),
        }
        self.state = self.root / ".local/stack-ui"

    @staticmethod
    def executable(path, body):
        path.write_text(f"#!{sys.executable}\n" + body)
        path.chmod(0o700)

    def data(self):
        return json.loads(self.data_file.read_text())

    def commands(self):
        return [json.loads(line) for line in self.log_file.read_text().splitlines()] if self.log_file.exists() else []

    def run_stack(self, action="run", **extra):
        return subprocess.run(
            ["/bin/bash", str(self.root / "scripts/runtime-regression/stack-ui.sh"), action],
            cwd=self.root, env={**self.env, **extra}, capture_output=True, text=True, timeout=15,
        )

    def seed_resources(self, **values):
        data = self.data()
        data.update(values)
        self.data_file.write_text(json.dumps(data))

    def assert_no_cleanup(self):
        for command in self.commands():
            self.assertFalse(command[:2] == ["docker", "rm"], command)
            self.assertFalse(command[:2] == ["docker", "compose"] and "down" in command, command)
            self.assertFalse(command[:3] == ["python3", "scripts/services.py", "stop"], command)

    def test_existing_state_is_not_deleted_by_rejected_run(self):
        self.state.mkdir(parents=True)
        sentinel = self.state / "existing.txt"
        sentinel.write_text("keep")
        evidence = self.root / ".local/evidence/stack-ui"
        evidence.mkdir(parents=True)
        (evidence / "host-before.txt").write_text("previous run evidence")
        result = self.run_stack()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(sentinel.read_text(), "keep")
        self.assertEqual((evidence / "host-before.txt").read_text(), "previous run evidence")
        self.assert_no_cleanup()

    def test_existing_database_is_not_adopted_or_deleted(self):
        self.seed_resources(containers={"miy-stack-ui-postgres-1": {"owner": "other"}})
        before = self.data()
        self.assertNotEqual(self.run_stack().returncode, 0)
        self.assertEqual(self.data(), before)
        self.assertFalse(self.state.exists())
        self.assert_no_cleanup()

    def test_partial_existing_project_is_not_adopted(self):
        self.seed_resources(networks=["miy-stack-ui-app-runtime"])
        before = self.data()
        self.assertNotEqual(self.run_stack().returncode, 0)
        self.assertEqual(self.data(), before)
        self.assert_no_cleanup()

    def test_port_conflict_does_not_claim_or_cleanup_state(self):
        self.assertNotEqual(self.run_stack(UI_PORT_BUSY="1").returncode, 0)
        self.assertFalse(self.state.exists())
        self.assert_no_cleanup()

    def test_owned_partial_startup_is_cleaned_and_failure_preserved(self):
        result = self.run_stack(UI_FAIL_BOOTSTRAP="1")
        self.assertEqual(result.returncode, 24, result.stderr)
        self.assertFalse(self.state.exists())
        self.assertEqual(self.data(), {"containers": {}, "networks": [], "volumes": []})

    def test_failed_tests_cleanup_only_the_owned_stack(self):
        foreign = {"production-postgres": {"owner": "production"}}
        self.seed_resources(containers=foreign)
        result = self.run_stack(UI_FAIL_TESTS="1")
        self.assertEqual(result.returncode, 26, result.stderr)
        self.assertFalse(self.state.exists())
        self.assertEqual(self.data(), {"containers": foreign, "networks": [], "volumes": []})
        self.assertEqual(self.run_stack("down").returncode, 0)

    def test_manual_down_refuses_unmarked_or_symlinked_state(self):
        self.state.mkdir(parents=True)
        sentinel = self.state / "existing.txt"
        sentinel.write_text("keep")
        self.assertNotEqual(self.run_stack("down").returncode, 0)
        self.assertEqual(sentinel.read_text(), "keep")
        shutil.rmtree(self.state)
        external = self.root / "external"
        external.mkdir()
        (external / "keep").write_text("keep")
        self.state.symlink_to(external, target_is_directory=True)
        self.assertNotEqual(self.run_stack("down").returncode, 0)
        self.assertEqual((external / "keep").read_text(), "keep")
        self.assert_no_cleanup()

    def test_manual_down_checks_database_ownership(self):
        self.state.mkdir(parents=True)
        (self.state / ".stack-ui-owner").write_text(f"{self.root}\nfixture-owner\n")
        self.seed_resources(containers={"miy-stack-ui-postgres-1": {"owner": "production"}})
        before = self.data()
        self.assertNotEqual(self.run_stack("down").returncode, 0)
        self.assertEqual(self.data(), before)
        self.assertTrue(self.state.exists())
        self.assert_no_cleanup()

    @unittest.skipUnless(NODE, "Node is required for the JavaScript harness")
    def test_server_cleanup_preserves_other_live_harness_database(self):
        other = {"miy-server-ui-other": {"kind": "server", "owner": "other"}}
        self.seed_resources(containers=other)
        result = subprocess.run(
            [NODE, str(self.root / "scripts/runtime-regression/server-ui.mjs")],
            cwd=self.root, env={**self.env, "UI_FAIL_BUILD": "1"}, capture_output=True, text=True, timeout=15,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.data()["containers"], other)
        removed = [c[-1] for c in self.commands() if c[:2] == ["docker", "rm"]]
        self.assertEqual(len(removed), 1)
        self.assertNotIn("miy-server-ui-other", removed)
        self.assertFalse((self.root / ".local/evidence/server-ui.lock").exists())

    @unittest.skipUnless(NODE, "Node is required for the JavaScript harness")
    def test_server_start_rejects_other_run_before_touching_evidence(self):
        evidence = self.root / ".local/evidence/server-ui"
        evidence.mkdir(parents=True)
        (evidence / "keep").write_text("keep")
        lock = evidence.with_name("server-ui.lock")
        lock.mkdir()
        result = subprocess.run(
            [NODE, str(self.root / "scripts/runtime-regression/server-ui.mjs")],
            cwd=self.root, env=self.env, capture_output=True, text=True, timeout=15,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual((evidence / "keep").read_text(), "keep")
        self.assertTrue(lock.exists())
        self.assertEqual(self.commands(), [])

    @unittest.skipUnless(NODE, "Node is required for the JavaScript harness")
    def test_server_failed_container_creation_does_not_claim_cleanup(self):
        other = {"miy-server-ui-other": {"kind": "server", "owner": "other"}}
        self.seed_resources(containers=other)
        result = subprocess.run(
            [NODE, str(self.root / "scripts/runtime-regression/server-ui.mjs")],
            cwd=self.root, env={**self.env, "UI_FAIL_DOCKER_RUN": "1"}, capture_output=True, text=True, timeout=15,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.data()["containers"], other)
        self.assert_no_cleanup()
        self.assertFalse((self.root / ".local/evidence/server-ui.lock").exists())


if __name__ == "__main__":
    unittest.main()
