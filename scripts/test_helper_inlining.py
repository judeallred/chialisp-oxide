"""Offline boundary controls; no compiler, solver, VM or consensus calls."""
import copy
import json
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import patch

import runtime_cost_migration as cost
import runtime_inline_migration as inline
from inline_controls import self_test


class HelperInliningTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.current = json.loads((inline.ROOT / "corpus/validation.json").read_bytes())
        cost.validate_current(cls.current)
        cls.report = cost.project_append(cls.current)

    def test_outer_projection_reproduces_signed405_before_older_layers(self):
        previous = inline.preserve(self.current)
        self.assertEqual(inline.sha(inline.canonical(previous)), inline.REPORT_CANONICAL)
        self.assertEqual(len(previous["puzzles"]), 21)
        historical = cost.preserve(self.current)
        self.assertEqual(cost.sha(cost.canonical(historical)), cost.load()["baseline"]["canonical_report_sha256"])

    def test_nonvacuous_corruptions_exercise_the_outer_projection(self):
        self.assertGreaterEqual(self_test(self.current), 42)

    def test_pending_profile_cannot_promote_a_pipeline_or_an_issue_url(self):
        report = copy.deepcopy(self.report)
        report["optimization_pipeline"] = copy.deepcopy(inline.NEW_PIPELINE)
        report["rue_local_patches"] = inline.BEFORE_PATCHES + ["https://github.com/xch-dev/rue/issues/999999"]
        pending = copy.deepcopy(inline.load())
        pending["reviewed_after"]["status"] = "pending"
        with patch.object(inline, "load", return_value=pending), self.assertRaisesRegex(ValueError, "pending"):
            inline.reviewed_new(report)

    def test_stale_current_native_inputs_fail_before_any_projection(self):
        bad = copy.deepcopy(self.current)
        bad["puzzles"][-1]["native_sources"][0]["sha256"] = "0" * 64
        with patch.object(inline, "project_validated", side_effect=AssertionError("projection ran first")), self.assertRaises(ValueError):
            inline.preserve(bad)

    def test_dispatch_validates_current_once_then_only_projects_history(self):
        calls = []
        current = {"optimization_pipeline": inline.NEW_PIPELINE}
        previous = {"optimization_pipeline": inline.OLD_PIPELINE}
        historical = {"historical": True}
        def validate(value):
            self.assertIs(value, current)
            calls.append("validate-current")
        def outer(value):
            self.assertIs(value, current)
            calls.append("project-inline")
            return previous
        def older(value):
            self.assertIs(value, previous)
            calls.append("project-405")
            return historical
        with patch.object(cost, "validate_current", side_effect=validate), patch.object(inline, "project_validated", side_effect=outer), patch.object(cost, "project_validated", side_effect=older):
            self.assertIs(cost.preserve(current), historical)
        self.assertEqual(calls, ["validate-current", "project-inline", "project-405"])

    def test_direct_hash_selection_never_calls_full_current_preservation(self):
        synthetic = copy.deepcopy(self.report)
        synthetic["optimization_pipeline"] = copy.deepcopy(inline.NEW_PIPELINE)
        measured = {"puzzles": inline.capture_after(self.report)}
        with patch.object(inline, "reviewed_new", return_value=measured), patch.object(inline, "preserve", side_effect=AssertionError("recursive validation")):
            self.assertIs(cost.hash_diagnostic_profile(synthetic), True)

    def test_wire_numeric_modes_cannot_alias_boolean_modes(self):
        for field, value in [("on", 1), ("on", 1.0), ("off", 0), ("off", 0.0)]:
            bad = copy.deepcopy(self.report)
            index = 1 if field == "on" else 0
            bad["puzzles"][0]["artifacts"][index]["optimize_lir"] = value
            self.assertNotEqual(inline.canonical(bad), inline.canonical(self.report))
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                inline.project_validated(bad)

    def test_snapshot_and_predecessor_remain_independently_pinned(self):
        self.assertEqual(inline.sha((inline.ROOT / "corpus/cost-optimization-baseline.json").read_bytes()), cost.PIN)
        for field in ("baseline", "predecessor", "optimization_pipeline", "rue_local_patches", "reviewed_after"):
            bad = copy.deepcopy(inline.load())
            bad[field] = {}
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "profile bytes"):
                inline.load(inline.canonical(bad))

    def test_capture_has_no_write_or_self_authorization_effect(self):
        before = inline.PATH.read_bytes()
        rows = inline.capture_after(self.report)
        self.assertEqual([row["slug"] for row in rows], [p["slug"] for p in self.report["puzzles"]])
        self.assertEqual(rows[-1]["curried_instances"]["cases"][0]["label"], "derived tail: valid_spend")
        self.assertEqual(inline.PATH.read_bytes(), before)

    def test_benchmark_off_bytes_are_checked_even_when_flag_stays_true(self):
        checker = runpy.run_path(str(inline.ROOT / "scripts/check-helper-inlining.py"))
        previous = inline.preserve(self.current)
        pins = {r["entry"]: r["sha256"] for r in inline.predecessor()["raw_ir"]}
        measured = {"status": "passed", "baseline_report_sha256": inline.REPORT_SHA, "puzzles": []}
        for old, current in zip(previous["puzzles"], self.report["puzzles"]):
            off = lambda puzzle: next(r for r in puzzle["artifacts"] if r["backend"] == "rust_mir" and r["optimize_lir"] is False)
            measured["puzzles"].append({"slug": current["slug"], "off_byte_exact": True,
                "source_ir_sha256": pins[f"chialisp_oxide_puzzles::{current['slug'].replace('-', '_')}::puzzle"],
                "old_off": copy.deepcopy(off(old)), "new_off": copy.deepcopy(off(current)),
                "old_on": copy.deepcopy(cost.rust_on(old["artifacts"])), "new_on": copy.deepcopy(cost.rust_on(current["artifacts"])),
                "named": [{"label": a["label"], "solution": a["solution"],
                    "old_on": cost.rust_on(a["runs"])["execution"], "new_on": cost.rust_on(b["runs"])["execution"]}
                    for a, b in zip(old["cases"], current["cases"])]})
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "benchmark.json"
            path.write_bytes(inline.canonical(measured))
            self.assertEqual(checker["benchmark_check"](path, self.current)["named_cases"], 389)
            for mode, key in [("old_off", "clvm_hex"), ("new_off", "tree_hash"), ("new_off", "bytes")]:
                bad = copy.deepcopy(measured)
                row = bad["puzzles"][0][mode]
                row[key] = {"clvm_hex": "80", "tree_hash": "0" * 64, "bytes": float(row["bytes"])}[key]
                self.assertNotEqual(inline.canonical(bad), inline.canonical(measured))
                self.assertIs(bad["puzzles"][0]["off_byte_exact"], True)
                path.write_bytes(inline.canonical(bad))
                with self.subTest(mode=mode, key=key), self.assertRaisesRegex(ValueError, "OFF bytes/size/hash"):
                    checker["benchmark_check"](path, self.current)


if __name__ == "__main__":
    unittest.main()
