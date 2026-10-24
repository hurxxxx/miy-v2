"""Remove only this run's resources, including partially completed setup."""
import datetime
import json
import os
import pathlib
import re
import subprocess
import uuid

root = pathlib.Path(os.environ["MIY_AUDIT_ROOT"]).resolve()
ev = root / "evidence"
removed, errors, notes = [], [], []


def docker(*args):
    return subprocess.check_output([os.environ["AUDIT_REAL_DOCKER"], *args], stderr=subprocess.PIPE).decode().strip()


def meta(kind, identifier):
    return json.loads(docker(kind, "inspect", identifier))[0]


def attempt(label, operation):
    try:
        operation()
    except Exception as error:
        errors.append(label + ": " + type(error).__name__)


def cleanup(cfg):
    prefix = cfg["prefix"]
    assert re.fullmatch(r"miy-audit-1024-[a-f0-9]{6}", prefix)
    assert cfg["databaseContainer"] == prefix + "-db" and cfg["network"] == prefix
    baseline_ids = set(cfg["baselineIds"])
    label = "miy.audit=" + prefix
    containers = docker("container", "ls", "--all", "--quiet", "--no-trunc", "--filter", "label=" + label).splitlines()
    installations = set()
    owned = []
    for identifier in containers:
        m = meta("container", identifier)
        assert m["Config"]["Labels"].get("miy.audit") == prefix
        assert m["Id"] not in baseline_ids
        owned.append(m)
        installation = m["Config"]["Labels"].get("org.miy.installation")
        if installation:
            installations.add(str(uuid.UUID(installation)))
    # DB evidence is optional; a failed DB must not prevent resource cleanup.
    if any(m["Name"] == "/" + cfg["databaseContainer"] for m in owned):
        try:
            def sql(query):
                return docker("exec", cfg["databaseContainer"], "psql", "-U", "postgres", "-d", "miy_audit_core", "-At", "-v", "ON_ERROR_STOP=1", "-c", query)
            if sql("SELECT to_regclass('public.installations')"):
                rows = json.loads(sql("SELECT coalesce(json_agg(json_build_object('id',id,'container',runtime_container)),'[]') FROM installations"))
                (ev / "owned-installations.json").write_text(json.dumps(rows, indent=2))
                installations.update(str(uuid.UUID(row["id"])) for row in rows)
                (ev / "final-jobs.json").write_text(sql("SELECT json_agg(json_build_object('id',id,'action',action,'environment',environment,'status',status,'step',step,'errorCode',error_code) ORDER BY created_at) FROM runtime_jobs"))
        except Exception:
            notes.append("Database evidence unavailable; using owned labels and this run's Docker journal.")
    # Networks can exist before an app container was created. The journal belongs
    # only to this isolated worker; actual ownership is rechecked before removal.
    journal = ev / "docker-commands.jsonl"
    if journal.exists():
        for line in journal.read_text().splitlines():
            args = json.loads(line)["args"]
            if args[:2] == ["network", "create"]:
                for arg in args:
                    if arg.startswith("org.miy.installation="):
                        installation = str(uuid.UUID(arg.split("=", 1)[1]))
                        assert args[-1] == "miy-runtime-" + installation.replace("-", "")
                        installations.add(installation)
    for m in owned:
        def remove_container(m=m):
            docker("rm", "-f", m["Id"])
            removed.append(m["Name"])
        attempt("container " + m["Id"], remove_container)
    network_names = {prefix: None, **{"miy-runtime-" + i.replace("-", ""): i for i in installations}}
    networks = [json.loads(line) for line in docker("network", "ls", "--format", "{{json .}}").splitlines()]
    for network in networks:
        name = network["Name"]
        if name not in network_names:
            continue
        def remove_network(name=name):
            m = meta("network", name)
            expected = network_names[name]
            assert (m["Labels"].get("org.miy.installation") == expected if expected else m["Labels"].get("miy.audit") == prefix)
            assert not m["Containers"]
            docker("network", "rm", m["Id"])
            removed.append(name)
        attempt("network " + name, remove_network)
    # Include images built successfully before setup recorded their tag.
    images = set(docker("image", "ls", "--quiet", "--no-trunc", "--filter", "label=" + label).splitlines())
    for identifier in images:
        def remove_image(identifier=identifier):
            m = meta("image", identifier)
            assert m["Config"]["Labels"].get("miy.audit") == prefix
            tags = m.get("RepoTags") or []
            assert all(tag.startswith(prefix + ":") for tag in tags)
            docker("image", "rm", *(tags or [identifier]))
            removed.append(identifier)
        attempt("image " + identifier, remove_image)


cfg_path = root / "config.json"
if cfg_path.exists():
    attempt("owned resources", lambda: cleanup(json.loads(cfg_path.read_text())))
comparisons, after = [], []
baseline = ev / "baseline-containers.json"
if baseline.exists():
    for b in json.loads(baseline.read_text()):
        def compare(b=b):
            m = meta("container", b["Id"])
            after.append({k: m[k] for k in ["Id", "Name", "Image", "State"]})
            row = {"name": b["Name"], "sameId": m["Id"] == b["Id"], "sameImage": m["Image"] == b["Image"], "sameStartedAt": m["State"]["StartedAt"] == b["State"]["StartedAt"], "stillRunning": m["State"]["Running"]}
            comparisons.append(row)
            assert all(row[k] for k in ["sameId", "sameImage", "sameStartedAt", "stillRunning"])
        attempt("baseline " + b["Id"], compare)
(ev / "after-containers.json").write_text(json.dumps(after, indent=2))
summary = {"completedAt": datetime.datetime.now(datetime.timezone.utc).isoformat(), "removed": removed, "baselineComparison": comparisons, "notes": notes, "errors": errors}
(ev / "cleanup.json").write_text(json.dumps(summary, indent=2))
print(json.dumps(summary, indent=2))
raise SystemExit(1 if errors else 0)
