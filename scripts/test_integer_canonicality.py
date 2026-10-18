"""Executed Seq8 law plus nonvacuous model/report/solver protocol controls.

No Rust compiler, CLVM execution or native puzzle process runs here.
"""
import copy
import hashlib
import json
from pathlib import Path
import runpy
import subprocess
import tempfile
import unittest
from unittest.mock import patch

GATE = runpy.run_path(str(Path(__file__).with_name("check-integer-canonicality.py")))
GLOBALS = GATE["produce"].__globals__


class IntegerCanonicalityTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Do not silently skip when the required pinned solver is unavailable.
        cls.report = GATE["produce"]()

    def test_actual_solver_checks_the_fixed_law_and_nonvacuous_source_mutants(self):
        report = self.report
        self.assertEqual(report["schema_version"], 1)
        self.assertEqual(report["protocol"], GATE["PROTOCOL"])
        self.assertEqual(report["solver"]["version"], GATE["Z3_VERSION"])
        self.assertEqual(report["baseline"]["queries"][0],
            {"name": "equivalence", "expected_status": "unsat", "observed_status": "unsat"})
        self.assertEqual(len(report["baseline"]["queries"]), 14)
        self.assertEqual([row["name"] for row in report["model_source_mutations"]],
                         ["lower-007f", "upper-ff81", "drop-nil"])
        for mutation in report["model_source_mutations"]:
            self.assertEqual(mutation["original_query"]["queries"][0]["observed_status"], "unsat")
            self.assertEqual([row["observed_status"] for row in mutation["mutated_query"]["queries"]], ["sat", "sat"])
            self.assertNotEqual(mutation["mutated_model_sha256"], GATE["MODEL_SHA"])
        self.assertGreaterEqual(report["solver_protocol_controls"], 19)

    def test_model_and_query_bytes_are_retained_exactly(self):
        self.assertEqual(hashlib.sha256(GATE["model_text"]().encode()).hexdigest(), GATE["MODEL_SHA"])
        records = [self.report["baseline"]]
        for mutation in self.report["model_source_mutations"]:
            records.extend([mutation["original_query"], mutation["mutated_query"]])
        for record in records:
            self.assertEqual(hashlib.sha256(record["smt2"].encode()).hexdigest(), record["smt2_sha256"])
        self.assertEqual([row["path"] for row in self.report["implementation_sources"]], [GATE["MODEL"], GATE["CHECKER"]])
        self.assertEqual([row["path"] for row in self.report["decoder_sources"]], list(GATE["DECODER_SOURCES"]))
        self.assertIn("metadata freshness only", self.report["scope"]["decoder_source_binding"])
        self.assertIn("compiler or optimizer preservation", self.report["scope"]["not_proven"])

    def test_offline_recorded_check_binds_sources_queries_scope_and_status_without_solver(self):
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "report.json"
            import json
            def write(value):
                report.write_text(json.dumps(value))
            write(self.report)
            with patch.object(subprocess, "run", side_effect=AssertionError("offline gate executed a process")):
                GATE["check_recorded_sources"](report)
                for mutate in (
                    lambda r: r["baseline"]["queries"][0].update(observed_status="sat"),
                    lambda r: r["baseline"].update(smt2=r["baseline"]["smt2"] + "; edited"),
                    lambda r: r["scope"].update(decoder_source_binding="compiler proof"),
                    lambda r: r["decoder_sources"][0].update(sha256="0" * 64),
                    lambda r: r["model_source_mutations"].reverse(),
                    lambda r: r.update(solver_protocol_controls=True),
                    lambda r: r.update(extra="unreviewed"),
                ):
                    bad = copy.deepcopy(self.report)
                    mutate(bad)
                    self.assertNotEqual(bad, self.report)
                    write(bad)
                    with self.assertRaises(ValueError):
                        GATE["check_recorded_sources"](report)

    def test_arbitrary_model_edits_cannot_self_authorize(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            model = root / GATE["MODEL"]
            model.parent.mkdir(parents=True)
            model.write_text(GATE["model_text"]() + "; unreviewed change\n")
            with patch.dict(GLOBALS, ROOT=root), self.assertRaisesRegex(ValueError, "model changed"):
                GATE["model_text"]()
        with self.assertRaisesRegex(ValueError, "Unknown"):
            GATE["mutate_model"](GATE["model_text"](), "unreviewed")
        ambiguous = GATE["model_text"]().replace("(not (less_than_two value #x00 #x80))", "true")
        with self.assertRaisesRegex(ValueError, "missing or ambiguous"):
            GATE["mutate_model"](ambiguous, "lower-007f")

    def test_a_noop_model_mutation_fails_its_actual_solver_witness(self):
        model = GATE["model_text"]()
        with patch.dict(GLOBALS, mutate_model=lambda source, name: source), self.assertRaisesRegex(ValueError, "protocol/status"):
            GATE["mutation_checks"]("z3", model)

    def test_solver_protocol_controls_have_an_accepted_positive_first(self):
        self.assertGreaterEqual(GATE["protocol_controls"](), 19)
        with patch.dict(GLOBALS, check_response=lambda result, queries: []), self.assertRaisesRegex(ValueError, "corruption passed"):
            GATE["protocol_controls"]()

    def test_unknown_error_noise_and_stderr_cannot_be_proof(self):
        queries = [("law", "unsat", [])]
        GATE["check_response"](subprocess.CompletedProcess([], 0, "law\nunsat\n", ""), queries)
        for stdout in ("law\nunknown\n", "law\nsat\n", "law\nunsat\n(error fake)\n", "unsat\nlaw\n"):
            with self.subTest(stdout=stdout), self.assertRaises(ValueError):
                GATE["check_response"](subprocess.CompletedProcess([], 0, stdout, ""), queries)
        with self.assertRaises(ValueError):
            GATE["check_response"](subprocess.CompletedProcess([], 0, "law\nunsat\n", "\n"), queries)

    def test_execution_timeout_or_missing_binary_remains_inconclusive(self):
        for error in (subprocess.TimeoutExpired("z3", 20), FileNotFoundError("missing")):
            def fail(*args, **kwargs):
                raise error
            with self.subTest(error=type(error).__name__), self.assertRaisesRegex(ValueError, "inconclusive"):
                GATE["call_z3"](["z3"], runner=fail)

    def test_solver_version_is_part_of_the_closed_protocol(self):
        for version, code, stderr in (("Z3 version 4.15.3 - 64 bit\n", 0, ""),
                                      (GATE["Z3_VERSION"] + "\n", 1, ""),
                                      (GATE["Z3_VERSION"] + "\n", 0, "warning")):
            result = subprocess.CompletedProcess([], code, version, stderr)
            with patch.dict(GLOBALS, call_z3=lambda *args, **kwargs: result), self.assertRaisesRegex(ValueError, "pinned"):
                GATE["check_version"]("z3")

    def test_declared_input_change_during_solver_work_is_rejected(self):
        before = GATE["source_rows"]()
        after = copy.deepcopy(before)
        after[-1]["sha256"] = "0" * 64
        # Use a local iterator so the actual solver work still executes.
        rows = iter([before, after])
        with patch.dict(GLOBALS, source_rows=lambda: next(rows)), self.assertRaisesRegex(ValueError, "changed during"):
            GATE["produce"]()

    def test_recorded_schema_claims_observations_and_sources_are_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "report.json"
            path.write_bytes(GATE["canonical"](self.report))
            GATE["check_recorded"](path, self.report)
            for mutation in ("schema", "numeric_schema", "protocol", "claim", "source", "decoder", "status", "query",
                             "witness", "missing_mutant", "controls", "unknown"):
                bad = copy.deepcopy(self.report)
                if mutation == "schema": bad["schema_version"] = 2
                elif mutation == "numeric_schema": bad["schema_version"] = True
                elif mutation == "protocol": bad["protocol"] += "-unknown"
                elif mutation == "claim": bad["scope"]["not_proven"].remove("compiler or optimizer preservation")
                elif mutation == "source": bad["implementation_sources"][0]["sha256"] = "0" * 64
                elif mutation == "decoder": bad["decoder_sources"][0]["sha256"] = "0" * 64
                elif mutation == "status": bad["baseline"]["queries"][0]["observed_status"] = "sat"
                elif mutation == "query": bad["baseline"]["smt2"] += "(exit)\n"
                elif mutation == "witness": bad["model_source_mutations"][0]["witness_hex"] = "007f"
                elif mutation == "missing_mutant": bad["model_source_mutations"].pop()
                elif mutation == "controls": bad["solver_protocol_controls"] = float(bad["solver_protocol_controls"])
                elif mutation == "unknown": bad["compiler_preservation"] = True
                self.assertNotEqual(GATE["canonical"](bad), GATE["canonical"](self.report))
                path.write_bytes(GATE["canonical"](bad))
                with self.subTest(mutation=mutation), self.assertRaisesRegex(ValueError, "Stale or corrupted"):
                    GATE["check_recorded"](path, self.report)
            path.write_text('{"schema_version": 1, "schema_version": 1}')
            with self.assertRaisesRegex(ValueError, "Duplicate"):
                GATE["check_recorded"](path, self.report)


if __name__ == "__main__":
    unittest.main()
