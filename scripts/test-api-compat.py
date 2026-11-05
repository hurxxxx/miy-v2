#!/usr/bin/env python3
"""Tests for check-api-compat.py on synthetic schemas."""
import copy
import importlib.util
import json
import pathlib
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parent / "check-api-compat.py"
spec = importlib.util.spec_from_file_location("check_api_compat", SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

PUBLISHED = {
    "publishedCli": {"version": "1.0.0", "sourceCommit": "c0ffee"},
    "removable": [],
    "$defs": {
        "Job": {
            "type": "object",
            "properties": {
                "id": {"type": "string", "format": "uuid"},
                "status": {"type": "string"},
                "errorCode": {"type": ["string", "null"]},
                "environment": {"$ref": "#/$defs/Environment"},
                "health": {"$ref": "#/$defs/Health"},
                "tags": {"type": "array", "items": {"type": "string"}},
            },
            "required": ["id", "status", "errorCode", "environment", "health", "tags"],
        },
        "Environment": {"type": "string", "enum": ["dev", "prod"]},
        "Health": {
            "oneOf": [
                {"type": "string", "enum": ["healthy", "unhealthy"]},
                {"type": "string", "const": "unknown"},
            ]
        },
    },
}


class CompatibilityTests(unittest.TestCase):
    def changed(self, change):
        current = copy.deepcopy({"$defs": PUBLISHED["$defs"]})
        change(current["$defs"])
        return module.check(PUBLISHED, current)

    def test_identical_and_additive_schemas_pass(self):
        self.assertEqual(self.changed(lambda defs: None), [])

        def add(defs):
            defs["Job"]["properties"]["step"] = {"type": "string"}
            defs["Job"]["required"].append("step")
            defs["Added"] = {"type": "object", "properties": {}}
            defs["Health"]["oneOf"][0]["enum"].append("degraded")
            defs["Job"]["properties"]["errorCode"] = {"type": "string"}

        self.assertEqual(self.changed(add), [])

    def test_removed_optional_widened_and_retyped_fields_fail(self):
        def remove(defs):
            del defs["Job"]["properties"]["status"]
            defs["Job"]["required"].remove("errorCode")
            defs["Job"]["properties"]["id"] = {"type": ["string", "null"]}
            defs["Job"]["properties"]["environment"] = {"type": "string"}
            defs["Job"]["properties"]["tags"]["items"] = {"type": "integer"}
            defs["Environment"]["enum"].append("staging")
            del defs["Health"]

        self.assertEqual(
            self.changed(remove),
            [
                "Job.id may now be null",
                "Job.status was removed",
                "Job.errorCode is no longer always sent",
                "Job.environment may now be string",
                "Job.tags may now be array of number",
                'Environment may now be "staging"',
                "Health was removed",
            ],
        )

    def test_listed_removals_are_allowed(self):
        published = copy.deepcopy(PUBLISHED)
        published["removable"] = [{"path": "Job.status", "reason": "unused since 1.1.0"}]
        current = copy.deepcopy({"$defs": PUBLISHED["$defs"]})
        del current["$defs"]["Job"]["properties"]["status"]
        self.assertEqual(module.check(published, current), [])

    def test_the_repository_passes(self):
        root = SCRIPT.parent.parent
        published = json.loads((root / "contracts/api-v1.published.json").read_text())
        current = json.loads((root / "contracts/api-v1.schema.json").read_text())
        self.assertEqual(module.check(published, current), [])


if __name__ == "__main__":
    unittest.main()
