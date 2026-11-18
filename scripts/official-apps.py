#!/usr/bin/env python3
"""Plan official apps, stage a publisher-pinned release in dev, or promote an accepted release."""
import argparse
import json
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import unicodedata
from uuid import UUID


DEFAULT_CATALOG = Path(__file__).resolve().parents[1] / "registry/apps/catalog.json"
OFFICIAL = {"default_official", "optional_official"}
VERSION = re.compile(r"(?:0|[1-9][0-9]{0,8})(?:\.(?:0|[1-9][0-9]{0,8})){2}")
PIN_FIELDS = ("version", "sourceCommit", "archiveSha256")


class Problem(Exception):
    pass


def require(condition, message):
    if not condition:
        raise Problem(message)


def semver(version):
    require(isinstance(version, str) and VERSION.fullmatch(version), "invalid_catalog_version")
    return tuple(map(int, version.split(".")))


def products(path):
    def unique_object(pairs):
        obj = dict(pairs)
        require(len(obj) == len(pairs), "duplicate_catalog_field")
        return obj

    data = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_object)
    require(isinstance(data, dict) and data.keys() == {"schemaVersion", "apps"}
            and type(data["schemaVersion"]) is int and data["schemaVersion"] == 1
            and isinstance(data["apps"], list), "invalid_catalog")
    indexed = {}
    for app in data["apps"]:
        require(isinstance(app, dict) and app.keys() == {"appId", "name", "defaultInstall", "releases"},
                "invalid_catalog_app")
        app_id = app["appId"]
        name = app["name"]
        require(isinstance(app_id, str) and re.fullmatch(r"[a-z][a-z0-9-]{0,47}", app_id)
                and app_id not in indexed and isinstance(name, str) and name.strip()
                and len(name.encode("utf-8")) <= 120 and not any(unicodedata.category(c) == "Cc" for c in name)
                and type(app["defaultInstall"]) is bool
                and isinstance(app["releases"], list), "invalid_catalog_app")
        releases = {}
        for pin in app["releases"]:
            require(isinstance(pin, dict) and pin.keys() == set(PIN_FIELDS), "invalid_catalog_release")
            semver(pin["version"])
            require(pin["version"] not in releases and isinstance(pin["sourceCommit"], str)
                    and re.fullmatch(r"[a-f0-9]{40}", pin["sourceCommit"]) and isinstance(pin["archiveSha256"], str)
                    and re.fullmatch(r"[a-f0-9]{64}", pin["archiveSha256"]), "invalid_catalog_release")
            releases[pin["version"]] = pin
        indexed[app_id] = {**app, "releases": releases}
    return indexed


def select_release(app, version):
    releases = app["releases"]
    if version:
        require(version in releases, "version_not_in_catalog")
        return releases[version]
    return releases[max(releases, key=semver)] if releases else None


def same_pin(data, pin):
    return isinstance(data, dict) and all(data.get(key) == pin[key] for key in PIN_FIELDS)


def is_uuid(value):
    try:
        return isinstance(value, str) and UUID(value).int != 0 and str(UUID(value)) == value
    except ValueError:
        return False


class Client:
    def __init__(self, executable, context):
        self.executable = shutil.which(executable)
        self.context = context
        self.origin = None

    def call(self, app_id, command, *, directory=None, body=None, confirm=False, group="app"):
        require(self.executable, "miy_cli_not_found")
        argv = [str(Path(self.executable).resolve()), "--context", self.context,
                "--app", app_id, "--no-input", "--output", "json"]
        if confirm:
            argv.append("--yes")
        result = subprocess.run(argv + [group, *command], cwd=directory, text=True, encoding="utf-8",
                                input=json.dumps(body) if body is not None else "",
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            reply = json.loads(result.stdout)
        except ValueError as error:
            raise Problem(f"invalid_cli_response ({command[0]})") from error
        require(isinstance(reply, dict), "invalid_cli_response")
        if result.returncode or reply.get("ok") is not True:
            error = reply.get("error") or {}
            require(isinstance(error, dict), "invalid_cli_error")
            code = error.get("code", "cli_failed")
            if not isinstance(code, str) or not re.fullmatch(r"[a-z][a-z0-9_]{0,95}", code):
                code = "cli_failed"
            details = error.get("details")
            refs = {key: details[key] for key in ("requestId", "jobId")
                    if isinstance(details, dict) and is_uuid(details.get(key))}
            recovery = error.get("recovery")
            if (isinstance(recovery, dict) and recovery.get("executable") == "miy"
                    and isinstance(recovery.get("args"), list) and recovery["args"]
                    and all(isinstance(arg, str) for arg in recovery["args"])):
                args = recovery["args"]
                if args[0] != "--context":
                    args = ["--context", self.context, "--app", app_id, *args]
                refs["recoveryCommand"] = {"executable": self.executable, "args": args}
                refs["recovery"] = display_command([self.executable, *args])
            elif refs and group == "app":
                env = "prod" if command[0] == "promote" else "dev"
                if "jobId" in refs:
                    cmd = ["job", "get", refs["jobId"]]
                else:
                    cmd = ["job", "resume", refs["requestId"], "--environment", env]
                refs["recovery"] = cli_command(self, app_id, cmd)
            raise Problem(f"{command[0]} {code}" + (" " + json.dumps(refs) if refs else ""))
        meta = reply["meta"]
        require(reply["schemaVersion"] == 1 and meta.get("context") == self.context
                and meta.get("origin"), "invalid_cli_context")
        require(self.origin in (None, meta["origin"]), "context_origin_changed")
        self.origin = meta["origin"]
        return reply["data"]


def status(client, product):
    app_id = product["appId"]
    detail = client.call(app_id, ["status"])
    app = detail["app"]
    require(app["id"] == app_id and app.get("distribution") in OFFICIAL
            and type(app.get("disabled")) is bool, "official_catalog_sync_required")
    require((app["distribution"] == "default_official") == product["defaultInstall"],
            "catalog_install_policy_mismatch")
    installations = {}
    for row in detail["installations"]:
        env = row["environment"]
        require(row["appId"] == app_id and env in ("dev", "prod")
                and env not in installations and row.get("distribution") in OFFICIAL,
                "installation_not_official")
        installations[env] = row
    return app["disabled"], installations


def display_command(argv):
    if sys.platform == "win32":
        words = [arg if re.fullmatch(r"[\w.:/\\-]+", arg) else "'" + arg.replace("'", "''") + "'" for arg in argv]
        return ("& " if argv[0] != words[0] else "") + " ".join(words)
    return shlex.join(argv)


def cli_command(client, app_id, command):
    return display_command([client.executable or "miy", "--context", client.context, "--app", app_id, *command])


def installation_state(client, app_id, row, release=None):
    runtime, desired, db = row["runtimeStatus"], row["desiredState"], row["dbStatus"]
    env = row["environment"]
    last = row.get("lastOperation") or {}
    job_status = last.get("status")
    paused = desired == "stopped" and runtime in ("stopped", "removed")
    if job_status in ("queued", "running"):
        problem = "installation_in_progress"
    elif runtime == "recovery_required":
        problem = "installation_recovery_required"
    elif paused and job_status == "succeeded" and last.get("action") in ("stop", "remove"):
        return "preserved"
    elif job_status == "failed" or runtime in ("failed", "unreachable") or db == "failed":
        problem = "installation_failed"
    elif paused and db == "ready":
        return "preserved"
    elif (row.get("health") or {}).get("status") == "unhealthy":
        problem = "installation_failed"
    elif desired == "running" and runtime == "running" and db == "ready":
        return "running"
    else:
        problem = "installation_incomplete"

    # The portal can hide management fields. A previous query or job lookup
    # is never interpreted as authority to resubmit the failed operation.
    details = {"environment": env, "runtimeStatus": runtime, "desiredState": desired, "dbStatus": db}
    if row.get("health"):
        details["healthStatus"] = row["health"].get("status")
    if is_uuid(row.get("releaseId")):
        details["releaseId"] = row["releaseId"]
    cmd = ["job", "list", "--environment", env]
    job_id = last.get("id")
    if row.get("canManage") is True and is_uuid(job_id):
        details.update(jobId=job_id, jobStatus=job_status)
        for key in ("action", "step", "errorCode"):
            value = last.get(key)
            if isinstance(value, str) and re.fullmatch(r"[A-Za-z0-9_.-]{1,96}", value):
                details[key] = value
        try:
            job = client.call(app_id, ["get", job_id], group="job")
            if (isinstance(job, dict) and job.get("id") == job_id and job.get("appId") == app_id
                    and job.get("environment") == env):
                if is_uuid(job.get("requestId")):
                    details["requestId"] = job["requestId"]
                if job.get("action") in ("deploy", "promote") and is_uuid(job.get("releaseId")):
                    details["attemptedReleaseId"] = job["releaseId"]
        except (Problem, OSError):
            pass  # The installation and original job ID are still actionable.
        cmd = ["job", "watch" if problem == "installation_in_progress" else "get", job_id]
    details["inspect"] = cli_command(client, app_id, cmd)
    if problem == "installation_failed" and job_status == "failed" and last.get("retryable") is True and "jobId" in details:
        details["reviewRetry"] = cli_command(client, app_id, ["job", "retry", job_id, "--environment", env, "--dry-run"])
    elif problem != "installation_in_progress":
        if env == "dev" and details.get("attemptedReleaseId"):
            details["reviewDeployment"] = cli_command(client, app_id, [
                "app", "deploy", "--environment", env, "--release", details["attemptedReleaseId"], "--dry-run"])
        elif env == "dev":
            details["afterDiagnosis"] = "After checking outstanding jobs and resolving the failure, verify the pinned checkout with app source-pin, run app build, then review the dev deployment."
            details["reviewDeployment"] = cli_command(client, app_id, ["app", "deploy", "--environment", env, "--dry-run"])
        elif release:
            details["reviewPromotion"] = cli_command(client, app_id, ["app", "promote", "--environment", env, "--release", release, "--dry-run"])
        else:
            details["afterDiagnosis"] = "After resolving the production failure, review promotion of the exact accepted dev release with app promote --dry-run."
    raise Problem(f"{problem} " + json.dumps(details))


def reviewed_state(row):
    if row is None:
        return None
    require(row["canManage"] is True, "management_read_required")
    return {"installationId": row["id"],
            **{key: row[key] for key in ("releaseId", "generation", "desiredState",
                                         "runtimeStatus", "dbStatus", "imageId")}}


def preflight(client, app_id, action, env, row, directory=None):
    plan = client.call(app_id, [*action, "--dry-run"], directory=directory)
    target = {"appId": app_id, "environment": env}
    require(plan["input"]["target"] == target and plan["expectedState"]["target"] == target, "preflight_target_mismatch")
    if plan["canSubmit"] is not True:
        checks = [check.get("code") for check in plan["checks"] if check.get("status") == "failed"]
        raise Problem("preflight_denied " + json.dumps({
            "checks": checks, "inspect": cli_command(client, app_id, ["job", "list", "--environment", env])}))
    require(plan["expectedState"].get("officialAppDisabled") is False,
            "official_app_disabled_or_catalog_missing")
    require(plan["expectedState"]["installation"] == reviewed_state(row), "installation_changed")
    return plan


def submit(client, app, action, env, row, plan, directory=None):
    # Re-read the company switch after a potentially slow build. The CLI pins
    # this exact dry-run through admission and the worker's expected-state check.
    app_id = app["appId"]
    disabled, latest = status(client, app)
    require(not disabled and reviewed_state(latest.get(env)) == reviewed_state(row),
            "app_disabled_or_installation_changed")
    job = client.call(app_id, [*action, "--if-current", "-"], body=plan,
                      directory=directory, confirm=env == "prod")
    require(job["appId"] == app_id and job["action"] == action[0] and job["environment"] == env
            and job["status"] == "succeeded" and is_uuid(job.get("releaseId")), "unexpected_runtime_job")
    return job["releaseId"]


def stage(client, args, app, pin):
    app_id = app["appId"]
    disabled, installed = status(client, app)
    if disabled:
        return {"status": "disabled"}
    dev = installed.get("dev")
    if args.command == "install" and installed:
        states = {env: installation_state(client, app_id, row) for env, row in installed.items()}
        row = dev or installed["prod"]
        state = states[row["environment"]]
        return {"status": "already_installed" if state == "running" else state,
                "environment": row["environment"], "runtimeStatus": row["runtimeStatus"], "releaseId": row["releaseId"]}
    if args.command == "update":
        require(dev is not None, "dev_installation_required")
        if installation_state(client, app_id, dev) == "preserved":
            return {"status": "preserved", "runtimeStatus": dev["runtimeStatus"]}
        require(dev["canManage"] is True, "management_read_required")
        if dev["version"] == pin["version"]:
            require(dev["sourceCommit"] == pin["sourceCommit"], "release_pin_mismatch")
            return {"status": "current", "releaseId": dev["releaseId"]}
        require(semver(dev["version"]) < semver(pin["version"]), "downgrade_refused")

    require(args.source or args.sources, "provide_--sources_or_--source")
    project = args.source if args.source else args.sources / app_id
    require(project.is_dir(), f"app_source_directory_required ({project})")
    local = client.call(app_id, ["source-pin"], directory=project)
    require(local["appId"] == app_id and same_pin(local, pin), "source_pin_mismatch")
    build = client.call(app_id, ["build"], directory=project)
    receipt = build.get("serverBuild", {})
    require(build["status"] == "succeeded" and build.get("metadataWritten") is True
            and receipt.get("appId") == app_id and receipt.get("version") == pin["version"]
            and receipt.get("sourceCommit") == pin["sourceCommit"]
            and receipt.get("archiveHash") == pin["archiveSha256"], "build_pin_mismatch")

    action = ["deploy", "--environment", "dev"]
    plan = preflight(client, app_id, action, "dev", dev, project)
    require(plan["input"]["imageId"] == receipt["imageId"], "build_image_changed")
    release = submit(client, app, action, "dev", dev, plan, project)
    return {"status": "awaiting_acceptance", "version": pin["version"], "releaseId": release}


def promote(client, app, release):
    app_id = app["appId"]
    disabled, installed = status(client, app)
    if disabled:
        return {"status": "disabled"}
    prod = installed.get("prod")
    if prod and installation_state(client, app_id, prod, release) == "preserved":
        return {"status": "preserved", "runtimeStatus": prod["runtimeStatus"]}
    if prod and prod["releaseId"] == release:
        return {"status": "current", "releaseId": release}

    action = ["promote", "--environment", "prod", "--release", release]
    plan = preflight(client, app_id, action, "prod", prod)
    candidate = plan.get("officialRelease") or {}
    pin = app["releases"].get(candidate.get("version"))
    require(plan["input"]["releaseId"] == release and pin and same_pin(candidate, pin)
            and candidate.get("structuredAcceptance") is True, "accepted_official_release_required")
    require(not prod or semver(prod["version"]) < semver(pin["version"]), "downgrade_refused")
    result = submit(client, app, action, "prod", prod, plan)
    require(result == release, "promoted_release_mismatch")
    return {"status": "promoted", "version": pin["version"], "releaseId": release}


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for command, description in (
        ("plan", "Show the selected catalog pins and products awaiting publication"),
        ("install", "Stage fresh default or explicitly selected apps in dev"),
        ("update", "Stage an explicit update of an existing running dev app"),
        ("promote", "Apply an explicitly accepted release to prod"),
    ):
        cmd = commands.add_parser(command, help=description)
        cmd.add_argument("--catalog", type=Path, default=DEFAULT_CATALOG)
        cmd.add_argument("--app", action="append", required=command in ("update", "promote"),
                         help="Exact selection; repeat for several apps (defaults when omitted)")
        if command != "promote":
            cmd.add_argument("--version", help="Exact version for one selected app (default: newest)")
        if command != "plan":
            cmd.add_argument("--context", required=True)
            cmd.add_argument("--miy", default="miy", help="Path or name of the matching CLI")
        if command in ("install", "update"):
            source = cmd.add_mutually_exclusive_group()
            source.add_argument("--sources", type=Path, help="Parent of pinned app directories named by app ID")
            source.add_argument("--source", type=Path, help="Pinned project directory for one explicitly selected app")
        if command == "promote":
            cmd.add_argument("--release", required=True, help="Exact accepted dev release UUID")
            cmd.add_argument("--confirm-prod", action="store_true", required=True,
                             help="Approve this production promotion")
    return parser.parse_args()


def main(args):
    catalog = products(args.catalog)
    ids = args.app or [key for key, app in catalog.items() if app["defaultInstall"]]
    require(len(ids) == len(set(ids)) and all(key in catalog for key in ids), "unknown_or_duplicate_app")
    if getattr(args, "version", None) or getattr(args, "source", None) or args.command == "promote":
        require(args.app and len(ids) == 1, "one_explicit_app_required")
    if args.command == "promote":
        require(is_uuid(args.release), "invalid_release_id")
    pins = {} if args.command == "promote" else {
        app_id: select_release(catalog[app_id], args.version) for app_id in ids
    }
    if args.app and args.command in ("install", "update"):
        for app_id, pin in pins.items():
            require(pin, f"{app_id}: requested_app_unreleased")
    client = Client(args.miy, args.context) if args.command != "plan" else None
    for app_id in ids:
        app = catalog[app_id]
        try:
            if args.command == "promote":
                result = promote(client, app, args.release)
            else:
                pin = pins[app_id]
                if args.command == "plan":
                    result = {"status": "available" if pin else "unreleased", "release": pin}
                elif pin:
                    result = stage(client, args, app, pin)
                else:
                    result = {"status": "unreleased"}
        except Problem as error:
            raise Problem(f"{app_id}: {error}") from error
        print(json.dumps({"appId": app_id, "name": app["name"], "defaultInstall": app["defaultInstall"],
                          **result}, ensure_ascii=False), flush=True)


if __name__ == "__main__":
    if not sys.stdout.isatty():
        sys.stdout.reconfigure(encoding="utf-8")
    if not sys.stderr.isatty():
        sys.stderr.reconfigure(encoding="utf-8")
    try:
        main(arguments())
    except (Problem, OSError, ValueError, TypeError, KeyError) as exc:
        sys.exit(f"official-apps: {exc}")
