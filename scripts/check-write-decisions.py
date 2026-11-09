#!/usr/bin/env python3
"""Refuse server writes that can commit without an in-transaction decision.

An authorized write commits through `policy::Write`: its transaction decides after
its locks and writes, and only the decided transaction commits. This check keeps new
code on that path. It reads the server sources and fails when

- a function opens a raw transaction (`.begin()`) and is not listed below, or
- a function that receives a `Principal` runs an INSERT, UPDATE or DELETE without
  `Write::begin` and is not listed below, or
- a listed function no longer needs its entry (the list stays exact).

Every entry says why the function is not an authorized write. Regression modules,
`integration.rs` and `#[cfg(test)]` code are not scanned.

Usage: scripts/check-write-decisions.py [--root crates/miy/src]
"""
import argparse
import pathlib
import re
import sys

REASONS = {
    "identity": "signs a user in or issues a session; identity, not an app decision",
    "own": "reduces the caller's own credential or lease; the owner condition is in the statement",
    "snapshot": "read-only snapshot transaction",
    "worker": "worker bookkeeping without a request principal",
    "write-type": "the policy::Write type itself",
}

# (module, function): reason key
TRANSACTIONS = {
    ("auth.rs", "callback"): "identity",
    ("auth.rs", "development_session"): "identity",
    ("auth.rs", "session"): "identity",
    ("auth_sessions.rs", "approve"): "identity",
    ("auth_sessions.rs", "exchange_device"): "identity",
    ("auth_sessions.rs", "limits"): "identity",
    ("auth_sessions.rs", "rotate"): "identity",
    ("auth_sessions.rs", "start"): "identity",
    ("cli_auth.rs", "revoke_legacy"): "own",
    ("delegations.rs", "revoke"): "own",
    ("development.rs", "close"): "own",
    ("job_api.rs", "events"): "snapshot",
    ("server_build.rs", "logs"): "snapshot",
    ("server_build.rs", "append_log"): "worker",
    ("runtime_health.rs", "persist"): "worker",
    ("server_build_worker.rs", "tick"): "worker",
    ("worker.rs", "verify_development_migrations"): "worker",
    ("policy.rs", "begin"): "write-type",
}
WRITERS = {
    ("auth.rs", "session"): "identity",
    ("auth_sessions.rs", "approve"): "identity",
    ("auth_sessions.rs", "revoke_session"): "own",
    ("cli_auth.rs", "revoke_legacy"): "own",
    ("delegations.rs", "revoke"): "own",
    ("development.rs", "close"): "own",
}

FUNCTION = re.compile(r"^\s*(?:pub(?:\([a-z]+\))?\s+)?(?:async\s+)?fn\s+([a-z_0-9]+)")
WRITE = re.compile(r"INSERT INTO|UPDATE [a-z_]+ SET|DELETE FROM")
PRINCIPAL = re.compile(r"Principal|:\s*P\s*[,)]")
TEST_CODE = re.compile(r"^#\[cfg\((?:all\()?test")


def functions(path):
    """(name, signature, body) per function, up to the file's first test module."""
    found, current = [], None
    for line in path.read_text(encoding="utf-8").splitlines():
        if TEST_CODE.match(line):
            break
        match = FUNCTION.match(line)
        if match:
            current = {"name": match.group(1), "signature": "", "body": [], "open": True}
            found.append(current)
        if current is None:
            continue
        if current["open"]:
            current["signature"] += line + "\n"
            if line.rstrip().endswith(("{", ";")):
                current["open"] = False
        else:
            current["body"].append(line)
    return [(f["name"], f["signature"], "\n".join(f["body"])) for f in found]


def check(root):
    problems, used_transactions, used_writers = [], set(), set()
    for path in sorted(pathlib.Path(root).glob("*.rs")):
        if path.name.endswith("_regression.rs") or path.name == "integration.rs":
            continue
        for name, signature, body in functions(path):
            key = (path.name, name)
            if ".begin()" in body:
                if key in TRANSACTIONS:
                    used_transactions.add(key)
                else:
                    problems.append(f"{path.name}: {name} opens a transaction outside policy::Write")
            if PRINCIPAL.search(signature) and WRITE.search(body) and "Write::begin" not in body:
                if key in WRITERS:
                    used_writers.add(key)
                else:
                    problems.append(f"{path.name}: {name} writes for a principal outside policy::Write")
    for key in sorted(set(TRANSACTIONS) - used_transactions):
        problems.append(f"{key[0]}: {key[1]} is listed for a raw transaction it no longer opens")
    for key in sorted(set(WRITERS) - used_writers):
        problems.append(f"{key[0]}: {key[1]} is listed as a principal writer it no longer is")
    return problems


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=str(pathlib.Path(__file__).resolve().parent.parent / "crates/miy/src"))
    args = parser.parse_args()
    assert set(TRANSACTIONS.values()) | set(WRITERS.values()) <= set(REASONS)
    problems = check(args.root)
    for problem in problems:
        print(problem, file=sys.stderr)
    if problems:
        return 1
    print(f"PASS write decisions: {len(TRANSACTIONS)} listed transactions, {len(WRITERS)} listed writers")
    return 0


if __name__ == "__main__":
    sys.exit(main())
