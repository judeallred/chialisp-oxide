"""Offline closed-boundary controls; no compiler, solver, VM or consensus."""
import copy
import hashlib
import json
import runpy
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch

import runtime_cost_migration as cost
import runtime_inline_migration as inline
import runtime_refinement_migration as refinement
from refinement_controls import self_test


class RuntimeRefinementTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.current = json.loads((refinement.ROOT / "corpus/validation.json").read_bytes())
        cost.validate_current(cls.current)
        cls.old = refinement.project_validated(cost.project_append(cls.current))
        # The signed serializer is sorted UTF-8 pretty JSON plus one newline.
        # Recover its complete old byte string after projecting current data.
        cls.payload = (json.dumps(cls.old, indent=2, sort_keys=True, ensure_ascii=False) + "\n").encode()

    def candidate(self):
        report = copy.deepcopy(self.old)
        report["optimization_pipeline"] = copy.deepcopy(refinement.NEW_PIPELINE)
        profile = copy.deepcopy(refinement.load())
        profile["reviewed_after"].update(status="reviewed_final",
            report_sha256=refinement.sha(refinement.canonical(report)),
            puzzles=refinement.capture_after(report))
        return report, profile

    def test_signed_baseline_is_the_whole_report_not_only_measured_slots(self):
        self.assertEqual(refinement.sha(self.payload), refinement.REPORT_SHA)
        self.assertEqual(refinement.sha(refinement.canonical(self.old)), refinement.REPORT_CANONICAL)
        self.assertEqual(hashlib.sha1(b"blob " + str(len(self.payload)).encode() + b"\0" + self.payload).hexdigest(), refinement.REPORT_BLOB)
        previous = refinement.project_validated(self.old)
        self.assertEqual(refinement.canonical(previous), refinement.canonical(self.old))
        for key in ("simulator", "toolchain", "execution_vm", "rue_revision", "vm_failure_protocol"):
            bad = copy.deepcopy(self.old)
            bad[key] = {"unreviewed": True}
            with self.subTest(key=key), self.assertRaises(ValueError):
                refinement.project_validated(bad)

    def test_pending_profile_rejects_new_pipeline_even_with_unchanged_artifacts(self):
        report, _ = self.candidate()
        pending = copy.deepcopy(refinement.load())
        pending["reviewed_after"]["status"] = "pending"
        with patch.object(refinement, "load", return_value=pending):
            with self.assertRaisesRegex(ValueError, "pending"):
                refinement.project_validated(report)
            with self.assertRaisesRegex(ValueError, "pending"):
                refinement.expected_patches(report)

    def test_reviewed_synthetic_transition_restores_entire_signed_baseline(self):
        report, profile = self.candidate()
        with patch.object(refinement, "load", return_value=profile):
            previous = refinement.project_validated(report)
            self.assertEqual(refinement.canonical(previous), refinement.canonical(self.old))
            signed405 = inline.project_validated(previous)
            self.assertEqual(inline.sha(inline.canonical(signed405)), inline.REPORT_CANONICAL)
            self.assertEqual(cost.sha(cost.canonical(cost.project_validated(signed405))), cost.load()["baseline"]["canonical_report_sha256"])

    def test_direct_new_boundary_has_nonvacuous_corruption_controls(self):
        report, profile = self.candidate()
        with patch.object(refinement, "load", return_value=profile):
            self.assertGreaterEqual(self_test(report, current_validation=refinement.project_validated), 42)

    def test_historical_controls_receive_positive_normalized_objects(self):
        import cost_controls
        import inline_controls
        report, profile = self.candidate()
        with patch.object(refinement, "load", return_value=profile):
            self.assertGreaterEqual(inline_controls.self_test(report, current_validation=refinement.project_validated), 42)
            self.assertGreaterEqual(cost_controls.self_test(report, current_validation=refinement.project_validated), 35)

    def test_current_source_checks_happen_before_projection(self):
        report = copy.deepcopy(self.current)
        report["puzzles"][-1]["native_sources"][0]["sha256"] = "0" * 64
        with patch.object(refinement, "project_validated", side_effect=AssertionError("projected first")), self.assertRaises(ValueError):
            refinement.preserve(report)

    def test_dispatch_validates_current_once_without_revalidating_history(self):
        report, profile = self.candidate()
        calls = []
        def validate(value):
            self.assertIs(value, report)
            calls.append("current")
        with patch.object(refinement, "load", return_value=profile), patch.object(cost, "validate_current", side_effect=validate):
            historical = cost.preserve(report)
        self.assertEqual(calls, ["current"])
        self.assertEqual(cost.sha(cost.canonical(historical)), cost.load()["baseline"]["canonical_report_sha256"])

    def test_direct_hash_selection_does_not_recurse_into_preservation(self):
        report, profile = self.candidate()
        with patch.object(refinement, "load", return_value=profile), patch.object(refinement, "preserve", side_effect=AssertionError("recursive")):
            self.assertIs(refinement.hash_diagnostic_profile(report), True)

    def test_capture_does_not_write_or_authorize_profile(self):
        before = refinement.PATH.read_bytes()
        status = refinement.load()["reviewed_after"]["status"]
        rows = refinement.capture_after(self.old)
        self.assertEqual([p["slug"] for p in rows], [p["slug"] for p in self.old["puzzles"]])
        self.assertEqual(refinement.PATH.read_bytes(), before)
        self.assertEqual(refinement.load()["reviewed_after"]["status"], status)

    def test_snapshot_predecessor_and_pipeline_are_independently_pinned(self):
        self.assertEqual(refinement.sha(inline.PATH.read_bytes()), inline.PIN)
        self.assertEqual(refinement.sha(cost.PATH.read_bytes()), cost.PIN)
        for field in ("baseline", "predecessor", "optimization_pipeline", "reviewed_after"):
            bad = copy.deepcopy(refinement.load())
            bad[field] = {}
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "profile bytes"):
                refinement.load(refinement.canonical(bad))

    def test_unknown_stage_order_cannot_alias_reviewed_pipeline(self):
        report, profile = self.candidate()
        with patch.object(refinement, "load", return_value=profile):
            for change in ("reverse", "omit", "unknown"):
                bad = copy.deepcopy(report)
                stages = bad["optimization_pipeline"]["rust_mir"]["optimized"]
                if change == "reverse": stages.reverse()
                elif change == "omit": stages.remove("lazy_integer_decoder")
                else: stages.append("unreviewed_optimizer")
                with self.subTest(change=change), self.assertRaises(ValueError):
                    refinement.project_validated(bad)

    def benchmark_extra_fixture(self):
        # Constructed record fixture, not an execution claim.
        report = self.old
        password = report["puzzles"][-1]["curried_instances"]
        measured = {"resources": [], "puzzles": []}
        for puzzle in report["puzzles"]:
            measured["puzzles"].append({"synthetic": []})
            if "resource_controls" not in puzzle:
                continue
            controls = puzzle["resource_controls"]
            measured["resources"].append({"slug": puzzle["slug"], "max_cost": controls["max_cost"],
                "cases": copy.deepcopy(controls["cases"]),
                "retained_baseline_runs": [{"backend": artifact["backend"], "optimize_lir": artifact["optimize_lir"],
                    "executions": [copy.deepcopy(next(run["execution"] for run in row["runs"]
                        if run["backend"] == artifact["backend"] and run["optimize_lir"] is artifact["optimize_lir"]))
                        for row in controls["cases"]]} for artifact in puzzle["artifacts"]],
                "new_rust_on_runs": [copy.deepcopy(cost.rust_on(row["runs"])["execution"]) for row in controls["cases"]]})
        artifact = next(row["artifact"] for row in password["artifacts"]
                        if row["artifact"]["backend"] == "rust_mir" and row["artifact"]["optimize_lir"] is True)
        measured["password_instances"] = {"fixed_argument": password["fixed_argument"], "scheme": password["scheme"],
            "old_reveal_hex": artifact["clvm_hex"], "new_reveal_hex": artifact["clvm_hex"],
            "old_tree_hash": artifact["tree_hash"], "new_tree_hash": artifact["tree_hash"],
            "cases": [{"label": row["label"], "solution": row["solution"], "native": copy.deepcopy(row["native"]),
                       "max_cost": 1_000_000, "old_on": copy.deepcopy(cost.rust_on(row["runs"])["execution"]),
                       "new_on": copy.deepcopy(cost.rust_on(row["runs"])["execution"])} for row in password["cases"]]}
        measured["password_varying"] = []
        for row in password["varying_arguments"]:
            run = next(v for v in row["runs"] if v["artifact"]["backend"] == "rust_mir" and v["artifact"]["optimize_lir"] is True)
            measured["password_varying"].append({**{key: copy.deepcopy(row[key]) for key in ("argument_hex", "password_hex", "solution", "native")},
                "old_reveal_hex": run["artifact"]["clvm_hex"], "new_reveal_hex": run["artifact"]["clvm_hex"],
                "old_on": copy.deepcopy(run["execution"]), "new_on": copy.deepcopy(run["execution"])})
        primary = {"baseline_attempts": [], "candidate_rust_attempts": []}
        for ledger in password["ledger"]:
            for attempt in ledger["attempts"]:
                row = {"backend": ledger["backend"], "optimize_lir": ledger["optimize_lir"],
                    **{key: copy.deepcopy(attempt[key]) for key in ("label", "solution", "reveal_hex", "reveal_tree_hash")},
                    "expected": copy.deepcopy(attempt["vm_021"]["outcome"]), "execution": copy.deepcopy(attempt["vm_021"])}
                primary["baseline_attempts"].append(row)
                if ledger["backend"] == "rust_mir":
                    primary["candidate_rust_attempts"].append({**copy.deepcopy(row), "argument_hex": attempt["argument_hex"]})
        measured["dedicated_primary_vm"] = primary
        row = next(c for c in report["puzzles"][0]["cases"] if c["native"]["status"] == "accepted")
        measured["puzzles"][0]["synthetic"] = [{"label": "constructed fixture only", "solution": row["solution"],
            "native": copy.deepcopy(row["native"]), "old_on": copy.deepcopy(cost.rust_on(row["runs"])["execution"]),
            "new_on": copy.deepcopy(cost.rust_on(row["runs"])["execution"])} for _ in range(12)]
        measured.update(named_cases=389, synthetic_cases=12, ordinary_vm_executions=802,
            retained_diagnostic_vm_executions=40, new_diagnostic_vm_executions=8,
            password_instance_vm_executions=96, password_varying_vm_executions=6,
            dedicated_primary_vm_executions=28, total_vm_executions=980)
        return measured

    def test_extra_benchmark_controls_are_nonvacuous_and_match_recorded_scope(self):
        from runtime_refinement_checks import benchmark_extra
        measured = self.benchmark_extra_fixture()
        self.assertEqual(benchmark_extra(measured, self.old, self.old)["recorded_vm_observations_compared"], 956)
        for mutation in ("diagnostic_input", "diagnostic_old", "diagnostic_new", "diagnostic_credit", "fixed_artifact",
                         "fixed_cost", "fixed_native", "varying_input", "varying_cost", "primary_expected",
                         "primary_artifact", "primary_cost", "synthetic_credit", "count", "count_type", "mode_type"):
            bad = copy.deepcopy(measured)
            if mutation == "diagnostic_input": bad["resources"][0]["cases"][0]["solution"] += " "
            elif mutation == "diagnostic_old": bad["resources"][0]["retained_baseline_runs"][1]["executions"][0]["cost"] += 1
            elif mutation == "diagnostic_new": bad["resources"][0]["new_rust_on_runs"][0]["cost"] += 1
            elif mutation == "diagnostic_credit": bad["resources"][-1]["new_rust_on_runs"][0]["outcome"] = {"status": "rejected"}
            elif mutation == "fixed_artifact": bad["password_instances"]["new_reveal_hex"] = "80"
            elif mutation == "fixed_cost": bad["password_instances"]["cases"][0]["new_on"]["cost"] += 1
            elif mutation == "fixed_native": bad["password_instances"]["cases"][0]["native"] = {"status": "rejected"}
            elif mutation == "varying_input": bad["password_varying"][0]["argument_hex"] = "80"
            elif mutation == "varying_cost": bad["password_varying"][0]["new_on"]["cost"] += 1
            elif mutation == "primary_expected": bad["dedicated_primary_vm"]["candidate_rust_attempts"][0]["expected"] = {"status": "rejected"}
            elif mutation == "primary_artifact": bad["dedicated_primary_vm"]["baseline_attempts"][0]["reveal_hex"] = "80"
            elif mutation == "primary_cost": bad["dedicated_primary_vm"]["candidate_rust_attempts"][0]["execution"]["cost"] += 1
            elif mutation == "synthetic_credit": bad["puzzles"][0]["synthetic"][0]["new_on"] = {"outcome": {"status": "inconclusive"}, "cost": None, "error": "CostExceeded", "failure": {"kind": "resource", "code": "cost_exceeded"}}
            elif mutation == "count": bad["total_vm_executions"] += 1
            elif mutation == "count_type": bad["total_vm_executions"] = 980.0
            elif mutation == "mode_type": bad["resources"][0]["retained_baseline_runs"][1]["optimize_lir"] = 1
            self.assertNotEqual(refinement.canonical(bad), refinement.canonical(measured))
            with self.subTest(mutation=mutation), self.assertRaises((ValueError, KeyError, TypeError, IndexError)):
                benchmark_extra(bad, self.old, self.old)

    def test_full_benchmark_scope_identity_controls(self):
        checker = runpy.run_path(str(refinement.ROOT / "scripts/check-runtime-refinement.py"))
        check = checker["benchmark_check"]
        measured = self.benchmark_extra_fixture()
        measured.update(schema_version=1, baseline_commit=refinement.COMMIT,
            execution_vm=self.old["execution_vm"], vm_failure_protocol=self.old["vm_failure_protocol"],
            input_files={"constructed-fixture-only": "1" * 64}, input_files_after={"constructed-fixture-only": "1" * 64},
            claims={"all21_off_artifacts_byte_exact": True, "raw_ir_reextraction": False,
                    "universal_equivalence_proof": False, "universal_cost_improvement": False,
                    "consensus_resubmitted": False, "generated_cases_executed": False, "native_executed": False})
        # Named/OFF comparisons have separate direct controls. Exercise the
        # full CLI metadata/extra path here without manufacturing a second
        # complete basic fixture or crediting any process execution.
        with tempfile.TemporaryDirectory() as temporary, patch.dict(check.__globals__,
                {"_benchmark_check": lambda *args, **kwargs: {"named_cases": 389}, "preserve": lambda report: self.old}):
            path = Path(temporary) / "constructed.json"
            path.write_bytes(refinement.canonical(measured))
            self.assertEqual(check(path, self.old)["recorded_vm_observations_compared"], 956)
            for field, value in (("baseline_commit", "0" * 40), ("schema_version", True),
                                 ("execution_vm", "unreviewed"), ("vm_failure_protocol", "unreviewed"),
                                 ("input_files_after", {})):
                bad = copy.deepcopy(measured)
                bad[field] = value
                path.write_bytes(refinement.canonical(bad))
                with self.subTest(field=field), self.assertRaises(ValueError):
                    check(path, self.old)
            for key in measured["claims"]:
                bad = copy.deepcopy(measured)
                bad["claims"][key] = not bad["claims"][key]
                path.write_bytes(refinement.canonical(bad))
                with self.subTest(claim=key), self.assertRaises(ValueError):
                    check(path, self.old)

    def test_benchmark_checks_actual_off_bytes_even_when_flag_remains_true(self):
        from runtime_cost_checks import benchmark_check as basic_benchmark_check
        def benchmark_check(path, report):
            return basic_benchmark_check(path, report, project=refinement.project_validated,
                baseline_report_sha=refinement.REPORT_SHA, raw_ir=inline.predecessor()["raw_ir"])
        report, profile = self.candidate()
        pins = {r["entry"]: r["sha256"] for r in inline.predecessor()["raw_ir"]}
        measured = {"status": "passed", "baseline_report_sha256": refinement.REPORT_SHA, "puzzles": []}
        for old, current in zip(self.old["puzzles"], report["puzzles"]):
            off = lambda puzzle: next(r for r in puzzle["artifacts"] if r["backend"] == "rust_mir" and r["optimize_lir"] is False)
            measured["puzzles"].append({"slug": current["slug"], "off_byte_exact": True,
                "source_ir_sha256": pins[f"chialisp_oxide_puzzles::{current['slug'].replace('-', '_')}::puzzle"],
                "old_off": copy.deepcopy(off(old)), "new_off": copy.deepcopy(off(current)),
                "old_on": copy.deepcopy(cost.rust_on(old["artifacts"])), "new_on": copy.deepcopy(cost.rust_on(current["artifacts"])),
                "named": [{"label": a["label"], "solution": a["solution"],
                    "old_on": cost.rust_on(a["runs"])["execution"], "new_on": cost.rust_on(b["runs"])["execution"]}
                    for a, b in zip(old["cases"], current["cases"])]})
        with patch.object(refinement, "load", return_value=profile), tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "benchmark.json"
            path.write_bytes(refinement.canonical(measured))
            self.assertEqual(benchmark_check(path, report)["named_cases"], 389)
            for mode, key in [("old_off", "clvm_hex"), ("new_off", "tree_hash"), ("new_off", "bytes")]:
                bad = copy.deepcopy(measured)
                row = bad["puzzles"][0][mode]
                row[key] = {"clvm_hex": "80", "tree_hash": "0" * 64, "bytes": float(row["bytes"])}[key]
                self.assertIs(bad["puzzles"][0]["off_byte_exact"], True)
                path.write_bytes(refinement.canonical(bad))
                with self.subTest(mode=mode, key=key), self.assertRaisesRegex(ValueError, "OFF bytes/size/hash"):
                    benchmark_check(path, report)


if __name__ == "__main__":
    unittest.main()
