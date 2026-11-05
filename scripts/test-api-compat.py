#!/usr/bin/env python3
"""Response compatibility, including mutations of the repository's actual Rust-generated schema."""
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
                "Job.id may now be null, string",
                "Job.status was removed",
                "Job.errorCode is no longer always sent",
                "Job.environment may now be string",
                "Job.tags may now be array of integer",
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

    def test_unknown_enum_fallback_only_accepts_new_strings(self):
        def change(defs):
            defs["Health"]["oneOf"].append({"const": 42})

        self.assertEqual(self.changed(change), ['Health may now be 42'])

    def test_array_items_keep_nullable_formats_and_ranges(self):
        for before, after in [
            ({"type": ["string", "null"], "format": "uuid"}, {"type": ["string", "null"]}),
            ({"type": "integer", "format": "int32"}, {"type": "integer", "format": "int64"}),
        ]:
            with self.subTest(before=before):
                published = copy.deepcopy(PUBLISHED)
                published["$defs"]["Job"]["properties"]["tags"]["items"] = before
                current = copy.deepcopy(published)
                current["$defs"]["Job"]["properties"]["tags"]["items"] = after
                self.assertTrue(any(problem.startswith("Job.tags may now be ")
                                    for problem in module.check(published, current)))

    def test_numeric_narrowing_and_explicit_bounds(self):
        # Wider numeric JSON types cannot sneak through bounds, but integers are
        # safe for an existing number reader when their range is a subset.
        for before, after, allowed in [
            ({"type": "number"}, {"type": "integer", "format": "int64"}, True),
            ({"type": "integer", "format": "int32"}, {"type": "number", "minimum": 0, "maximum": 1}, False),
            ({"type": "integer", "minimum": 0, "maximum": 10}, {"type": "integer", "minimum": -1, "maximum": 10}, False),
            ({"type": "integer", "exclusiveMinimum": 0, "exclusiveMaximum": 10}, {"type": "integer", "minimum": 1, "maximum": 9}, True),
            ({"type": "number", "exclusiveMinimum": 0}, {"type": "number", "minimum": 0}, False),
            ({"type": "number", "maximum": 10}, {"type": "number", "exclusiveMaximum": 10}, True),
        ]:
            with self.subTest(before=before, after=after):
                problems = []
                module.compare("Numeric", before, after, set(), problems)
                self.assertEqual(problems == [], allowed)


class RepositoryCompatibilityTests(unittest.TestCase):
    def setUp(self):
        root = SCRIPT.parent.parent
        self.published = json.loads((root / "contracts/api-v1.published.json").read_text())
        self.current = json.loads((root / "contracts/api-v1.schema.json").read_text())

    def change(self, definition, field, replacement):
        current = copy.deepcopy(self.current)
        current["$defs"][definition]["properties"][field] = replacement
        return module.check(self.published, current)

    def test_the_repository_passes(self):
        self.assertEqual(module.check(self.published, self.current), [])

    def test_real_integer_readers_reject_fractional_responses(self):
        for definition, field in [("Installation", "generation"), ("AuditEntry", "id"),
                                  ("Health", "consecutiveFailures")]:
            with self.subTest(field=f"{definition}.{field}"):
                self.assertEqual(self.change(definition, field, {"type": "number", "format": "double"}),
                                 [f"{definition}.{field} may now be number"])

    def test_real_uuid_and_datetime_readers_keep_their_format(self):
        for definition, field in [("RuntimeJob", "id"), ("RuntimeJob", "createdAt"),
                                  ("RuntimeJob", "installationId"), ("Health", "checkedAt")]:
            original = self.current["$defs"][definition]["properties"][field]
            for replacement_format in (None, "date-time" if original["format"] == "uuid" else "uuid"):
                with self.subTest(field=f"{definition}.{field}", format=replacement_format):
                    replacement = copy.deepcopy(original)
                    if replacement_format is None:
                        del replacement["format"]
                    else:
                        replacement["format"] = replacement_format
                    self.assertTrue(any(problem.startswith(f"{definition}.{field} may now be ")
                                        for problem in self.change(definition, field, replacement)))

    def test_real_integer_formats_protect_the_reader_range(self):
        for definition, field, replacement_format in [
            ("Health", "consecutiveFailures", "int64"),
            ("Installation", "generation", "uint64"),
            ("Installation", "generation", None),
            ("AuditPage", "nextCursor", None),
        ]:
            with self.subTest(field=f"{definition}.{field}", format=replacement_format):
                replacement = copy.deepcopy(self.current["$defs"][definition]["properties"][field])
                if replacement_format is None:
                    del replacement["format"]
                else:
                    replacement["format"] = replacement_format
                self.assertTrue(self.change(definition, field, replacement))

    def test_explicit_bounds_must_stay_inside_the_actual_i64_reader_range(self):
        minimum, maximum = -(2 ** 63), 2 ** 63 - 1
        exact = {"type": "integer", "minimum": minimum, "maximum": maximum}
        self.assertEqual(self.change("Installation", "generation", exact), [])
        for replacement in ({**exact, "minimum": minimum - 1}, {**exact, "maximum": maximum + 1},
                            {"type": "integer", "minimum": minimum},
                            {"type": "integer", "maximum": maximum}):
            with self.subTest(replacement=replacement):
                self.assertTrue(self.change("Installation", "generation", replacement))

    def test_compatible_format_and_range_narrowing_passes(self):
        for definition, field, replacement in [
            ("Health", "consecutiveFailures", {"type": "integer", "format": "int16"}),
            ("Installation", "generation", {"type": "integer", "format": "uint32"}),
            ("RuntimeJob", "imageId", {"type": ["string", "null"], "format": "uuid"}),
        ]:
            with self.subTest(field=f"{definition}.{field}"):
                self.assertEqual(self.change(definition, field, replacement), [])


if __name__ == "__main__":
    unittest.main()
