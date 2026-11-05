#!/usr/bin/env python3
"""Keep the API responses readable by published CLIs.

contracts/api-v1.published.json holds the response schema that published CLIs may
read (refreshed when a CLI is published, see docs/CLI-RELEASE.md). The current
schema (contracts/api-v1.schema.json, generated from crates/miy/src/api_contract.rs)
may only add to it. For every published type and field this check fails when

- the type or field is gone, or a required field became optional,
- a value that was not allowed before can now be sent (a type or `null` added, a
  reference changed, an enum value added),

unless the field is listed under "removable" in the published file with its reason.
An enum that already has the `unknown` value readers fall back to may gain values.

Usage: scripts/check-api-compat.py [--published PATH] [--current PATH]
       scripts/check-api-compat.py --refresh VERSION COMMIT   (when publishing a CLI)
"""
import argparse
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def values(schema):
    """What a value may be: ('ref', name) entries, JSON types, or enum literals."""
    if "$ref" in schema:
        return {("ref", schema["$ref"].rsplit("/", 1)[-1])}
    if "const" in schema:
        return {("literal", json.dumps(schema["const"]))}
    if "enum" in schema:
        return {("literal", json.dumps(value)) for value in schema["enum"]}
    for key in ("anyOf", "oneOf"):
        if key in schema:
            return set().union(*(values(part) for part in schema[key]))
    kinds = schema.get("type", [])
    kinds = kinds if isinstance(kinds, list) else [kinds]
    found = set()
    for kind in kinds:
        if kind == "array":
            found |= {("array", item) for item in values(schema.get("items", {}))}
        else:
            found.add(("type", "number" if kind == "integer" else kind))
    return found


def describe(value):
    kind, detail = value
    return f"array of {describe(detail)}" if kind == "array" else detail


def open_enum(schema):
    return ("literal", json.dumps("unknown")) in values(schema)


def compare(where, old, new, removable, problems):
    added = values(new) - values(old)
    if added and not (open_enum(old) and all(kind == "literal" for kind, _ in added)):
        problems.append(f"{where} may now be {', '.join(sorted(map(describe, added)))}")
    old_properties, new_properties = old.get("properties", {}), new.get("properties", {})
    old_required, new_required = set(old.get("required", [])), set(new.get("required", []))
    for name, schema in old_properties.items():
        path = f"{where}.{name}"
        if path in removable:
            continue
        if name not in new_properties:
            problems.append(f"{path} was removed")
            continue
        if name in old_required and name not in new_required:
            problems.append(f"{path} is no longer always sent")
        compare(path, schema, new_properties[name], removable, problems)


def check(published, current):
    removable = {entry["path"] for entry in published.get("removable", [])}
    problems = []
    for name, old in published["$defs"].items():
        if name in removable:
            continue
        new = current["$defs"].get(name)
        if new is None:
            problems.append(f"{name} was removed")
        else:
            compare(name, old, new, removable, problems)
    return problems


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--published", default=str(ROOT / "contracts/api-v1.published.json"))
    parser.add_argument("--current", default=str(ROOT / "contracts/api-v1.schema.json"))
    parser.add_argument("--refresh", nargs=2, metavar=("VERSION", "COMMIT"),
                        help="record the current schema as what the CLI VERSION built from COMMIT reads")
    args = parser.parse_args()
    current = json.loads(pathlib.Path(args.current).read_text(encoding="utf-8"))
    if args.refresh:
        version, commit = args.refresh
        record = {
            "publishedCli": {"version": version, "sourceCommit": commit},
            "removable": [],
            "$defs": current["$defs"],
        }
        pathlib.Path(args.published).write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
        print(f"wrote {args.published}")
        return 0
    published = json.loads(pathlib.Path(args.published).read_text(encoding="utf-8"))
    problems = check(published, current)
    for problem in problems:
        print(f"incompatible with published CLIs: {problem}", file=sys.stderr)
    if problems:
        return 1
    print(f"PASS api compatibility with {published['publishedCli']['version']} ({len(published['$defs'])} types)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
