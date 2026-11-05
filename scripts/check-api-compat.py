#!/usr/bin/env python3
"""Keep the API responses readable by published CLIs.

contracts/api-v1.published.json holds the response schema that published CLIs may
read (refreshed when a CLI is published, see docs/CLI-RELEASE.md). The current
schema (contracts/api-v1.schema.json, generated from crates/miy/src/api_contract.rs)
may only add to it. For every published type and field this check fails when

- the type or field is gone, or a required field became optional,
- a value that was not allowed before can now be sent (a type or `null` added, a
  reference changed, an enum value added, an integer range widened, or a string
  format such as UUID/date-time removed or changed),

unless the field is listed under "removable" in the published file with its reason.
An enum that already has the `unknown` value readers fall back to may gain values.

Usage: scripts/check-api-compat.py [--published PATH] [--current PATH]
       scripts/check-api-compat.py --refresh VERSION COMMIT   (when publishing a CLI)
"""
import argparse
import json
import math
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
# Schemars describes Rust integer readers by format, without explicit min/max.
INTEGER_RANGES = {
    **{f"int{bits}": (-(2 ** (bits - 1)), 2 ** (bits - 1) - 1) for bits in (8, 16, 32, 64, 128)},
    **{f"uint{bits}": (0, 2 ** bits - 1) for bits in (8, 16, 32, 64, 128)},
}


def numeric_bounds(schema, kind):
    """The reader's numeric interval, including Rust integer formats and explicit bounds."""
    lower, upper = INTEGER_RANGES.get(schema.get("format"), (-math.inf, math.inf)) if kind == "integer" else (-math.inf, math.inf)
    lower_closed = upper_closed = True
    for key, closed in (("minimum", True), ("exclusiveMinimum", False)):
        if key in schema:
            value = schema[key]
            if value > lower or (value == lower and not closed):
                lower, lower_closed = value, closed
    for key, closed in (("maximum", True), ("exclusiveMaximum", False)):
        if key in schema:
            value = schema[key]
            if value < upper or (value == upper and not closed):
                upper, upper_closed = value, closed
    if kind == "integer":
        if math.isfinite(lower):
            lower = math.ceil(lower) if lower_closed else math.floor(lower) + 1
        if math.isfinite(upper):
            upper = math.floor(upper) if upper_closed else math.ceil(upper) - 1
        lower_closed = upper_closed = True
    return lower, lower_closed, upper, upper_closed


def values(schema):
    """Generated schema alternatives, preserving formats/ranges even inside arrays or nullables."""
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
        elif kind in ("integer", "number"):
            found.add((kind, numeric_bounds(schema, kind)))
        elif kind == "string":
            found.add(("string", schema.get("format")))
        else:
            found.add(("type", kind))
    return found


def describe(value):
    kind, detail = value
    if kind == "array":
        return f"array of {describe(detail)}"
    if kind == "string":
        return f"{detail} string" if detail else "string"
    if kind in ("integer", "number"):
        if detail == (-math.inf, True, math.inf, True):
            return kind
        lower, lower_closed, upper, upper_closed = detail
        return f"{kind} in {'[' if lower_closed else '('}{lower}, {upper}{']' if upper_closed else ')'}"
    return detail


def covered(value, previous):
    """May an old reader accept every value of this generated alternative?"""
    if value == previous:
        return True
    kind, detail = value
    old_kind, old_detail = previous
    if kind == old_kind == "array":
        return covered(detail, old_detail)
    if kind == old_kind == "string":
        # A generic string reader accepts a newly constrained format; the reverse
        # would let invalid UUID/date-time values reach typed Rust readers.
        return old_detail is None
    if kind in ("integer", "number") and old_kind in ("integer", "number"):
        if kind == "number" and old_kind == "integer":
            return False
        lower, lower_closed, upper, upper_closed = detail
        old_lower, old_lower_closed, old_upper, old_upper_closed = old_detail
        return ((lower > old_lower or (lower == old_lower and (old_lower_closed or not lower_closed)))
                and (upper < old_upper or (upper == old_upper and (old_upper_closed or not upper_closed))))
    return False


def open_enum(schema):
    return ("literal", json.dumps("unknown")) in values(schema)


def compare(where, old, new, removable, problems):
    previous = values(old)
    added = {value for value in values(new) if not any(covered(value, before) for before in previous)}
    if added and not (open_enum(old) and all(kind == "literal" and isinstance(json.loads(value), str)
                                           for kind, value in added)):
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
