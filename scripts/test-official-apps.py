#!/usr/bin/env python3
"""Exercise the official installer through stateful CLI data, without contacting a server."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location("official_apps", Path(__file__).with_name("official-apps.py"))
tool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tool)

APP = "planner"
FIRST = {"version": "1.2.0", "sourceCommit": "a" * 40, "archiveSha256": "1" * 64}
NEXT = {"version": "1.10.0", "sourceCommit": "b" * 40, "archiveSha256": "2" * 64}
RID = "00000000-0000-4000-8000-000000000001"
IID = "00000000-0000-4000-8000-000000000002"
PREVIOUS = "00000000-0000-4000-8000-000000000003"
JOB = "00000000-0000-4000-8000-000000000004"
REQUEST = "00000000-0000-4000-8000-000000000005"
IMAGE = "sha256:" + "3" * 64
APP_INFO = {"appId": APP, "defaultInstall": True, "releases": {p["version"]: p for p in (FIRST, NEXT)}}


def installation(env="dev", pin=FIRST, release=PREVIOUS):
    return {"id": IID, "appId": APP, "distribution": "default_official", "environment": env,
            "releaseId": release, "version": pin["version"], "sourceCommit": pin["sourceCommit"],
            "desiredState": "running", "runtimeStatus": "running", "imageId": IMAGE,
            "dbStatus": "ready", "generation": 2, "canManage": True}


class FakeClient:
    def __init__(self):
        self.context = "fixture"
        self.executable = "miy"
        self.rows = {}
        self.product = {"id": APP, "distribution": "default_official", "disabled": False}
        self.source = dict(appId=APP, **NEXT)
        self.receipt = dict(appId=APP, version=NEXT["version"], sourceCommit=NEXT["sourceCommit"],
                            archiveHash=NEXT["archiveSha256"], imageId=IMAGE)
        self.candidate = dict(structuredAcceptance=True, **NEXT)
        self.events = []
        self.last_plan = None
        self.after_build = lambda: None
        self.after_plan = lambda: None
        self.fail_deployment = False

    def call(self, app_id, cmd, *, directory=None, body=None, confirm=False, group="app"):
        assert app_id == APP
        action = cmd[0]
        self.events.append((action, tuple(cmd)))
        if group == "job":
            assert cmd == ["get", JOB]
            active_env = next(env for env, row in self.rows.items() if row.get("lastOperation", {}).get("id") == JOB)
            return {"appId": app_id, "environment": active_env, "id": JOB, "requestId": REQUEST,
                    "action": "deploy" if active_env == "dev" else "promote", "releaseId": RID}
        if action == "status":
            return copy.deepcopy({"app": self.product, "installations": list(self.rows.values())})
        if action == "source-pin":
            return self.source
        if action == "build":
            self.after_build()
            return {"metadataWritten": True, "serverBuild": self.receipt, "status": "succeeded"}
        assert action in ("deploy", "promote")
        env = cmd[cmd.index("--environment") + 1]
        assert (env == "dev") == (action == "deploy")
        target = {"appId": app_id, "environment": env}
        current = tool.reviewed_state(self.rows.get(env))
        if "--dry-run" in cmd:
            self.last_plan = {"canSubmit": True, "input": {"action": action, "target": target,
                              "imageId": IMAGE if action == "deploy" else None,
                              "releaseId": RID if action == "promote" else None},
                              "expectedState": {"target": target, "installation": current, "officialAppDisabled": False}}
            if env == "prod":
                self.last_plan["officialRelease"] = self.candidate
            self.after_plan()
            return self.last_plan
        assert cmd[-2:] == ["--if-current", "-"] and body == self.last_plan
        assert body["expectedState"]["installation"] == current
        assert confirm == (env == "prod")
        self.rows[env] = installation(env, NEXT, RID)
        if self.fail_deployment:
            self.rows[env].update(runtimeStatus="failed", desiredState="stopped", dbStatus="failed",
                                  lastOperation={"id": JOB, "action": action, "status": "failed",
                                                 "errorCode": "migration_failed", "retryable": False})
            raise tool.Problem("remote_job_failed")
        return {"appId": app_id, "environment": env, "action": action, "status": "succeeded", "releaseId": RID}


class OfficialApps(unittest.TestCase):
    def setUp(self):
        self.client = FakeClient()
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.args = SimpleNamespace(command="install", source=Path(self.scratch.name), sources=None)

    def stage(self):
        return tool.stage(self.client, self.args, APP_INFO, NEXT)

    def test_default_selection_optional_and_unreleased(self):
        script = str(Path(tool.__file__))
        # No credentials, CLI, live DB or source required for a default with no published release.
        catalog = self.args.source / "catalog.json"
        entries = [{"appId": "planner", "name": "Planner", "defaultInstall": True, "releases": []},
                   {"appId": "pms", "name": "PMS", "defaultInstall": False, "releases": [NEXT, FIRST]}]
        catalog.write_text(json.dumps({"schemaVersion": 1, "apps": entries}))
        def run(*args):
            return subprocess.run([sys.executable, script, *args, "--catalog", catalog],
                                  capture_output=True, text=True)
        result = run("install", "--context", "unused", "--miy", "nonexistent-cli")
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual("unreleased", json.loads(result.stdout)["status"])
        result = run("plan", "--app", "pms")
        self.assertEqual(NEXT, json.loads(result.stdout)["release"])
        result = run("plan", "--app", "pms", "--version", FIRST["version"])
        self.assertEqual(FIRST, json.loads(result.stdout)["release"])
        result = run("install", "--app", "planner", "--context", "unused")
        self.assertNotEqual(0, result.returncode)
        self.assertIn("requested_app_unreleased", result.stderr)

    def test_stage_release_without_grants_or_implicit_promotion(self):
        self.assertEqual(RID, self.stage()["releaseId"])
        self.assertEqual(["status", "source-pin", "build", "deploy", "status", "deploy"],
                         [entry[0] for entry in self.client.events])
        self.client.events.clear()
        self.assertEqual("already_installed", self.stage()["status"])
        self.assertEqual(1, len(self.client.events))

    def test_entire_catalog_and_selection_validated_before_any_install(self):
        good = {"appId": APP, "name": "Planner", "defaultInstall": True, "releases": [FIRST]}
        other = {"appId": "pms", "name": "PMS", "defaultInstall": False, "releases": []}
        document = {"schemaVersion": 1, "apps": [good, other]}
        invalid = [dict(document, schemaVersion=True), dict(document, schemaVersion=1.0), dict(document, extra=7)]
        for field, value in (("name", " "), ("name", "작" * 41), ("name", "a\u0007b"), ("name", False),
                             ("extra", "unknown"), ("defaultInstall", 1), ("appId", APP),
                             ("releases", [FIRST, FIRST]), ("releases", [dict(FIRST, extra="unknown")]),
                             ("releases", [dict(FIRST, sourceCommit=None)])):
            invalid.append(dict(document, apps=[good, {**other, field: value}]))
        invalid_json = [json.dumps(value) for value in invalid]
        invalid_json.append(json.dumps(document).replace('"schemaVersion": 1', '"schemaVersion": 1, "schemaVersion": 1'))
        catalog = self.args.source / "catalog.json"
        invocation = SimpleNamespace(command="install", catalog=catalog, app=None, source=None,
                                     version=None, context="fixture", miy="unused")
        with patch.object(tool, "Client") as mock_cli:
            for raw in invalid_json:
                catalog.write_text(raw, encoding="utf-8")
                with self.subTest(raw=raw), self.assertRaises(tool.Problem):
                    tool.main(invocation)
            catalog.write_text(json.dumps(document), encoding="utf-8")
            invocation.app = [APP, "pms"]
            with self.assertRaisesRegex(tool.Problem, "pms: requested_app_unreleased"):
                tool.main(invocation)
            mock_cli.assert_not_called()

    def test_update_pin_order_and_immutable_receipt(self):
        self.args.command = "update"
        with self.assertRaisesRegex(tool.Problem, "dev_installation_required"):
            self.stage()
        self.client.rows["dev"] = installation()
        self.assertEqual("awaiting_acceptance", self.stage()["status"])
        self.assertEqual("current", self.stage()["status"])
        with self.assertRaisesRegex(tool.Problem, "downgrade_refused"):
            tool.stage(self.client, self.args, APP_INFO, FIRST)
        self.client.rows["dev"]["sourceCommit"] = "c" * 40
        with self.assertRaisesRegex(tool.Problem, "release_pin_mismatch"):
            self.stage()

    def test_source_tampering_and_cross_app_receipt_rejected(self):
        self.client.source["archiveSha256"] = "c" * 64
        with self.assertRaisesRegex(tool.Problem, "source_pin_mismatch"):
            self.stage()
        self.assertEqual("source-pin", self.client.events[-1][0])
        self.client.source["archiveSha256"] = NEXT["archiveSha256"]
        self.client.receipt["appId"] = "pms"
        with self.assertRaisesRegex(tool.Problem, "build_pin_mismatch"):
            self.stage()
        self.assertEqual("build", self.client.events[-1][0])

    def test_disabled_stopped_removed_and_organization_are_preserved(self):
        self.client.product["distribution"] = "organization"
        with self.assertRaisesRegex(tool.Problem, "official_catalog_sync_required"):
            self.stage()
        self.client.product["distribution"] = "optional_official"
        with self.assertRaisesRegex(tool.Problem, "catalog_install_policy_mismatch"):
            self.stage()
        self.client.product.update(distribution="default_official", disabled=True)
        self.assertEqual("disabled", self.stage()["status"])
        self.client.product["disabled"] = False
        self.args.command = "update"
        for state in ("stopped", "removed"):
            self.client.rows["dev"] = dict(installation(), desiredState="stopped", runtimeStatus=state)
            self.assertEqual("preserved", self.stage()["status"])
            self.args.command = "install"
            self.assertEqual("preserved", self.stage()["status"])
            self.args.command = "update"
            self.client.rows["prod"] = dict(installation("prod"), desiredState="stopped", runtimeStatus=state)
            self.assertEqual("preserved", tool.promote(self.client, APP_INFO, RID)["status"])
        self.assertTrue(all(event[0] == "status" for event in self.client.events))

    def test_failed_first_deployment_never_becomes_success_on_reentry(self):
        self.client.fail_deployment = True
        with self.assertRaisesRegex(tool.Problem, "remote_job_failed"):
            self.stage()
        for command in ("install", "update"):
            self.args.command = command
            self.client.events.clear()
            with self.assertRaisesRegex(tool.Problem, "installation_failed") as failed:
                self.stage()
            details = json.loads(str(failed.exception).split(" ", 1)[1])
            self.assertEqual((JOB, REQUEST, "failed", "stopped", "migration_failed"),
                             (details["jobId"], details["requestId"], details["runtimeStatus"],
                              details["desiredState"], details["errorCode"]))
            self.assertEqual(f"miy --context fixture --app planner job get {JOB}", details["inspect"])
            self.assertEqual(RID, details["attemptedReleaseId"])
            self.assertIn(f"app deploy --environment dev --release {RID} --dry-run", details["reviewDeployment"])
            self.assertNotIn("reviewRetry", details)
            self.assertEqual(["status", "get"], [event[0] for event in self.client.events])

    def test_cli_utf8_and_authoritative_build_recovery(self):
        args = ["--context", "fixture", "--app", APP, "job", "resume", REQUEST, "--environment", "dev"]
        reply = {"ok": False, "error": {"code": "build_metadata_unavailable",
                 "details": {"requestId": REQUEST, "jobId": JOB}, "recovery": {"executable": "miy", "args": args}}}
        failed = subprocess.CompletedProcess([], 7, json.dumps(reply), "")
        client = tool.Client(sys.executable, "fixture")
        with patch.object(tool.subprocess, "run", return_value=failed) as invoked, self.assertRaises(tool.Problem) as error:
            client.call(APP, ["build"])
        self.assertEqual("utf-8", invoked.call_args.kwargs["encoding"])
        details = json.loads(str(error.exception).split(" ", 2)[2])
        self.assertEqual(args, details["recoveryCommand"]["args"])
        self.assertIn("job resume " + REQUEST, details["recovery"])
        result = subprocess.run([sys.executable, tool.__file__, "plan"], capture_output=True, check=True,
                                env={**os.environ, "PYTHONIOENCODING": "ascii"})
        self.assertEqual({"플래너", "메신저"}, {json.loads(row)["name"] for row in result.stdout.decode("utf-8").splitlines()})

    def test_cli_reentry_exits_nonzero_without_an_installed_receipt(self):
        root = self.args.source
        catalog = root / "catalog.json"
        product = dict(appId=APP, name="Planner", defaultInstall=True, releases=[NEXT])
        catalog.write_text(json.dumps({"schemaVersion": 1, "apps": [product]}))
        row = dict(installation(), runtimeStatus="failed", desiredState="stopped", lastOperation={
            "id": JOB, "action": "deploy", "status": "failed", "retryable": False})
        responses = {"status": {"app": self.client.product, "installations": [row]},
                     "get": {"id": JOB, "requestId": REQUEST, "appId": APP, "environment": "dev"}}
        (root / "responses.json").write_text(json.dumps(responses))
        stub = root / "fixture-miy"
        stub.write_text("""#!/usr/bin/env python3
import json, sys
from pathlib import Path
root = Path(__file__).parent
argv = sys.argv[1:]
group = "job" if "job" in argv else "app"
action = argv[argv.index(group) + 1]
with (root / "calls.txt").open("a") as log:
    print(group, action, file=log)
data = json.loads((root / "responses.json").read_text())[action]
print(json.dumps({"schemaVersion": 1, "ok": True, "data": data, "meta": {"context": "fixture", "origin": "https://fixture.invalid"}}))
""")
        stub.chmod(0o700)
        executable = stub
        if os.name == "nt":
            executable = stub.with_suffix(".cmd")
            executable.write_text(f'@"{sys.executable}" "{stub}" %*\n')
        result = subprocess.run([sys.executable, tool.__file__, "install", "--context", "fixture",
                                 "--catalog", catalog, "--miy", executable], capture_output=True, text=True)
        self.assertEqual(1, result.returncode, result.stderr)
        self.assertFalse(result.stdout)
        self.assertIn("installation_failed", result.stderr)
        self.assertEqual(REQUEST, json.loads(result.stderr.split("installation_failed ", 1)[1])["requestId"])
        self.assertEqual(["app status", "job get"], (root / "calls.txt").read_text().splitlines())

    def test_partial_pending_and_failed_prod_states(self):
        for runtime, desired, db, expected in (
            ("not_deployed", "stopped", "not_prepared", "installation_incomplete"),
            ("starting", "running", "ready", "installation_incomplete"),
            ("stopped", "running", "ready", "installation_incomplete"),
            ("running", "running", "failed", "installation_failed"),
        ):
            self.client.rows["prod"] = dict(installation("prod"), desiredState=desired, runtimeStatus=runtime, dbStatus=db)
            with self.subTest(runtime=runtime), self.assertRaisesRegex(tool.Problem, expected) as problem:
                tool.promote(self.client, APP_INFO, RID)
            self.assertIn("job list --environment prod", str(problem.exception))
            self.assertIn("--release " + RID + " --dry-run", str(problem.exception))
        row = self.client.rows["prod"] = installation("prod")
        row["lastOperation"] = {"id": JOB, "action": "promote", "status": "running"}
        with self.assertRaisesRegex(tool.Problem, "installation_in_progress") as pending:
            tool.promote(self.client, APP_INFO, RID)
        self.assertIn("job watch " + JOB, str(pending.exception))
        row.update(runtimeStatus="stopped", desiredState="stopped")
        row["lastOperation"].update(status="failed", retryable=True)
        with self.assertRaisesRegex(tool.Problem, "installation_failed") as failed:
            tool.promote(self.client, APP_INFO, RID)
        self.assertIn(f"job retry {JOB} --environment prod --dry-run", str(failed.exception))
        row.update(dbStatus="failed")
        row["lastOperation"].update(status="succeeded", action="stop")
        self.assertEqual("preserved", tool.promote(self.client, APP_INFO, RID)["status"])
        self.assertTrue(all(event[0] in ("status", "get") for event in self.client.events))

    def test_competing_install_stop_and_company_switch(self):
        self.client.after_build = lambda: self.client.rows.update(dev=installation())
        with self.assertRaisesRegex(tool.Problem, "installation_changed"):
            self.stage()
        self.args.command = "update"
        self.client.after_build = lambda: self.client.rows["dev"].update(desiredState="stopped", generation=3)
        with self.assertRaisesRegex(tool.Problem, "installation_changed"):
            self.stage()
        self.client.rows.clear()
        self.args.command = "install"
        self.client.after_build = lambda: None
        self.client.after_plan = lambda: self.client.product.update(disabled=True)
        with self.assertRaisesRegex(tool.Problem, "app_disabled_or_installation_changed"):
            self.stage()
        self.assertFalse(any("--if-current" in entry[1] for entry in self.client.events))

    def test_prod_only_deployer_needs_accepted_pinned_release(self):
        # An independent prod deployer cannot read the dev install, test notes or logs.
        self.client.candidate["structuredAcceptance"] = False
        with self.assertRaisesRegex(tool.Problem, "accepted_official_release_required"):
            tool.promote(self.client, APP_INFO, RID)
        self.client.candidate["structuredAcceptance"] = True
        self.client.candidate["sourceCommit"] = "9" * 40
        with self.assertRaisesRegex(tool.Problem, "accepted_official_release_required"):
            tool.promote(self.client, APP_INFO, RID)
        self.client.candidate["sourceCommit"] = NEXT["sourceCommit"]
        self.assertEqual("promoted", tool.promote(self.client, APP_INFO, RID)["status"])
        self.assertEqual("current", tool.promote(self.client, APP_INFO, RID)["status"])
        self.assertNotIn("dev", self.client.rows)


if __name__ == "__main__":
    unittest.main()
