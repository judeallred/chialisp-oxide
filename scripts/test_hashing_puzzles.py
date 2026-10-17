#!/usr/bin/env python3
"""Offline controls for hash-port artifacts, input budgets and replay routing."""
import hashlib
import json
from pathlib import Path
import runpy
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from hashing_provenance import (CANONICAL_HASH, CANONICAL_HEX, DEPENDENCIES, GENERATED_SHA256,
                                INCLUDES, NATIVE_SOURCES, ROOT, digest, read_source)
from hashing_vectors import (CODEC, SLUGS, expected, generated_for, generated_payload,
                             named, parse, sha_tree)

GATE = runpy.run_path(str(ROOT / "scripts/check-hashing-puzzles.py"))


class HashingGateTests(unittest.TestCase):
    def test_json_mutation_detection_preserves_wire_types(self):
        from hashing_controls import json_wire
        for original, changed in [(False, 0), (True, 1), (1, 1.0), (False, 0.0)]:
            before = {"nested": [{"value": original}]}
            after = {"nested": [{"value": changed}]}
            self.assertEqual(before, after)  # This is the Python-equality trap.
            with self.subTest(original=original, changed=changed):
                self.assertNotEqual(json_wire(before), json_wire(after))
        self.assertEqual(json_wire({"a": False, "b": 1}), json_wire({"b": 1, "a": False}))
        for value in [float("nan"), float("inf"), -float("inf")]:
            with self.subTest(non_json_number=value), self.assertRaises(ValueError):
                json_wire({"value": value})

    def test_published_artifact_and_noncanonical_serialization(self):
        artifact = {"backend": "chialisp", "optimize_lir": True, "bytes": 175,
                    "tree_hash": CANONICAL_HASH, "clvm_hex": CANONICAL_HEX, "disassembly": "source checked separately"}
        self.assertEqual(GATE["artifact_bytes"](artifact), bytes.fromhex(CANONICAL_HEX))
        # 00 is a one-byte atom, and must not be accepted as 81 00 in an
        # allegedly canonical retained artifact even with a matching tree hash.
        bad = {**artifact, "bytes": 2, "clvm_hex": "8100", "tree_hash": sha_tree(b"\0").hex()}
        with self.assertRaisesRegex(ValueError, "canonically serialized"):
            GATE["artifact_bytes"](bad)
        with self.assertRaisesRegex(ValueError, "tree hash"):
            GATE["artifact_bytes"]({**artifact, "tree_hash": "00" * 32})

    def test_module_identity_rejects_numeric_boolean_flags_in_every_mode(self):
        import copy
        from hashing_vectors import MODES
        artifacts = [{"backend": backend, "optimize_lir": optimize,
                      "clvm_hex": CANONICAL_HEX if backend == "chialisp" else "80",
                      "tree_hash": CANONICAL_HASH if backend == "chialisp" else "00" * 32,
                      "bytes": 175 if backend == "chialisp" else 1}
                     for backend, optimize in MODES]
        original = {"artifacts": artifacts, "module_identity": {
            "upstream_hex": CANONICAL_HEX, "upstream_tree_hash": CANONICAL_HASH,
            "comparisons": [{"backend": row["backend"], "optimize_lir": row["optimize_lir"],
                             "serialized_bytes_equal": row["clvm_hex"] == CANONICAL_HEX,
                             "tree_hash_equal": row["tree_hash"] == CANONICAL_HASH} for row in artifacts]}}
        GATE["identity"](original)
        for index, row in enumerate(original["module_identity"]["comparisons"]):
            for field in ("optimize_lir", "serialized_bytes_equal", "tree_hash_equal"):
                for numeric in (int, float):
                    changed = copy.deepcopy(original)
                    changed["module_identity"]["comparisons"][index][field] = numeric(row[field])
                    self.assertEqual(changed, original)
                    with self.subTest(mode=index, field=field, numeric=numeric.__name__), self.assertRaisesRegex(ValueError, "exact booleans"):
                        GATE["identity"](changed)

    def test_generated_file_identity_and_raw_suite_counts(self):
        payload = generated_payload()
        self.assertEqual((len(payload), hashlib.sha256(payload).hexdigest()), (63221, GENERATED_SHA256))
        self.assertEqual([len(generated_for(slug)) for slug in SLUGS], [2334, 2059])
        self.assertEqual([len(named(slug)) for slug in SLUGS], [39, 20])

    def test_every_input_fits_whole_tree_native_preflight(self):
        for slug in SLUGS:
            for row in named(slug) + generated_for(slug):
                pending, nodes, total = [(parse(row["solution"]), 0)], 0, 0
                while pending:
                    value, depth = pending.pop()
                    nodes += 1
                    self.assertLessEqual(depth, 256, row["label"])
                    self.assertLessEqual(nodes, 16384, row["label"])
                    if isinstance(value, tuple):
                        pending.extend([(value[0], depth + 1), (value[1], depth + 1)])
                    else:
                        total += len(value)
                        self.assertLessEqual(total, 1048576, row["label"])
                outcome = expected(slug, row["solution"])
                if outcome["status"] == "accepted":
                    CODEC["deserialize"](bytes.fromhex(outcome["output_hex"]))

    def test_native_closure_cannot_omit_or_coherently_rehash_a_changed_input(self):
        for slug in SLUGS:
            paths = DEPENDENCIES[slug] + NATIVE_SOURCES[slug] + [f"puzzles/{slug}/puzzle.rs"]
            authoring = {"sources": [{"path": path, "sha256": digest(read_source(path))} for path in dict.fromkeys(paths)]}
            puzzle = {"source_dependencies": [{"path": path, "sha256": digest(read_source(path)), "source": read_source(path).decode()}
                                                for path in DEPENDENCIES[slug]],
                      "reference_includes": INCLUDES[slug],
                      "native_sources": [{"path": path, "sha256": digest(read_source(path))} for path in NATIVE_SOURCES[slug]]}
            GATE["dependencies"](puzzle, slug, authoring, read_source)
            with self.subTest(slug=slug, mutation="missing compiled input"):
                bad = {"sources": authoring["sources"][:-1]}
                with self.assertRaises(ValueError):
                    GATE["dependencies"](puzzle, slug, bad, read_source)
            path = NATIVE_SOURCES[slug][-1]
            changed = read_source(path) + b"\n"
            reader = lambda p: changed if p == path else read_source(p)
            # Updating both the live input and its per-port digest must still
            # disagree with the independently retained compiled-input closure.
            puzzle["native_sources"][-1]["sha256"] = digest(changed)
            with self.subTest(slug=slug, mutation="coherent per-port hash edit"), self.assertRaises(ValueError):
                GATE["dependencies"](puzzle, slug, authoring, reader)

    def test_replay_uses_only_retained_hex_and_solution_file(self):
        cli, artifact, solutions = Path("/tmp/cli"), Path("/tmp/retained.hex"), Path("/tmp/solutions.json")
        execution = {"outcome": {"status": "accepted", "output_hex": "80"}, "cost": 100, "error": None}
        with patch.object(GATE["subprocess"], "run", return_value=SimpleNamespace(returncode=0, stdout=json.dumps([execution]), stderr="")) as run:
            self.assertEqual(GATE["run_batch"](cli, artifact, solutions, 1), [execution])
        self.assertEqual(run.call_args.args[0], [str(cli), "run-batch", str(artifact), "--solutions", str(solutions), "--max-cost", "1000000"])
        self.assertNotIn("build", run.call_args.args[0])

    def test_replay_routes_the_validated_old_and_refined_resource_profiles(self):
        import copy
        from hashing_vectors import RESOURCE_BUDGET, validation_budget
        from runtime_cost_migration import preserve

        current = json.loads((ROOT / "corpus/validation.json").read_text())
        historical = preserve(current)
        for name, report, refined in (("old", historical, False), ("refined", current, True)):
            by_slug = {puzzle["slug"]: puzzle for puzzle in report["puzzles"] if puzzle["slug"] in SLUGS}
            diagnostic_modes = []

            def recorded_batch(cli, path, solutions, count, *, max_cost, require_conclusive=True):
                # Exercise replay's real routing and input files without invoking
                # a VM. Named costs/outcomes and low-budget diagnostics stay exact.
                slug, mode = Path(path).stem.rsplit("-", 1)
                mode = int(mode)
                puzzle = by_slug[slug]
                if require_conclusive:
                    cases = puzzle["cases"] + generated_for(slug)
                    self.assertEqual(max_cost, validation_budget(slug))
                else:
                    self.assertEqual(slug, "sha256tree")
                    self.assertEqual(max_cost, RESOURCE_BUDGET)
                    cases = puzzle["resource_controls"]["cases"]
                    diagnostic_modes.append(mode)
                self.assertEqual(count, len(cases))
                self.assertEqual(json.loads(Path(solutions).read_text()), [case["solution"] for case in cases])
                return [copy.deepcopy(case["runs"][mode]["execution"]) if "runs" in case
                        else {"outcome": expected(slug, case["solution"])} for case in cases]

            with self.subTest(profile=name), patch.dict(GATE["replay"].__globals__, {
                "run_batch": recorded_batch, "print": lambda *args, **kwargs: None,
            }):
                total = GATE["replay"](report, Path("/unused-no-vm"))
                self.assertEqual(total, sum((len(puzzle["cases"]) + len(generated_for(slug))) * 5
                                            for slug, puzzle in by_slug.items()) + 25)
                self.assertEqual(diagnostic_modes, list(range(5)))
                controls = by_slug["sha256tree"]["resource_controls"]["cases"]
                for index in (2, 3, 4):
                    self.assertEqual(controls[index]["runs"][1]["execution"]["outcome"]["status"],
                                     "accepted" if refined else "inconclusive")

    def test_replay_rejects_missing_extra_and_duplicate_json_records(self):
        execution = {"outcome": {"status": "accepted", "output_hex": "80"}, "cost": 1, "error": None}
        for payload in ["[]", json.dumps([execution, execution]), json.dumps(execution),
                        '[{"outcome":{"status":"accepted","status":"rejected"},"cost":1,"error":null}]']:
            with self.subTest(payload=payload), patch.object(GATE["subprocess"], "run", return_value=SimpleNamespace(returncode=0, stdout=payload, stderr="")):
                with self.assertRaises(ValueError):
                    GATE["run_batch"](Path("/tmp/cli"), Path("/tmp/retained.hex"), Path("/tmp/solutions.json"), 1)

    def test_closed_resource_controls_require_five_cases_and_exact_failure_categories(self):
        import copy
        from hashing_controls import resource_fixture
        original = resource_fixture()
        self.assertEqual(len(GATE["resource_evidence"](original)), 5)
        for mutation in ("omit", "budget", "missing", "program", "infrastructure", "unknown", "wrong_digest", "mode", "accepted_over_budget"):
            bad = copy.deepcopy(original)
            controls = bad["resource_controls"]
            case = controls["cases"][0]
            execution = case["runs"][0]["execution"]
            if mutation == "omit": controls["cases"].pop()
            elif mutation == "budget": controls["max_cost"] = 10_000_000
            elif mutation == "missing": del execution["failure"]
            elif mutation == "program": execution["outcome"]["status"] = "rejected"
            elif mutation == "infrastructure": execution["failure"] = {"kind": "infrastructure", "code": "internal_error"}
            elif mutation == "unknown": execution["failure"]["code"] = "unknown"
            elif mutation == "wrong_digest": case["native"]["output_hex"] = "80"
            elif mutation == "mode": case["runs"].reverse()
            else: case["runs"][1]["execution"]["cost"] = 1_000_001
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                GATE["resource_evidence"](bad)

    def test_explicit_comparison_budget_is_routed_and_bounded(self):
        execution = {"outcome": {"status": "accepted", "output_hex": "80"}, "cost": 2_000_000, "error": None}
        with patch.object(GATE["subprocess"], "run", return_value=SimpleNamespace(returncode=0, stdout=json.dumps([execution]), stderr="")) as run:
            GATE["run_batch"](Path("/tmp/cli"), Path("/tmp/retained.hex"), Path("/tmp/solutions.json"), 1, max_cost=10_000_000)
            self.assertEqual(run.call_args.args[0][-2:], ["--max-cost", "10000000"])
            with self.assertRaises(ValueError):
                GATE["run_batch"](Path("/tmp/cli"), Path("/tmp/retained.hex"), Path("/tmp/solutions.json"), 1)
        for invalid in (0, 10_000_001, True):
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                GATE["run_batch"](Path("/tmp/cli"), Path("/tmp/retained.hex"), Path("/tmp/solutions.json"), 1, max_cost=invalid)

    def test_replay_rejects_resource_and_forged_failures(self):
        for status, kind, code in [("rejected", "resource", "cost_exceeded"),
                                   ("inconclusive", "resource", "cost_exceeded"),
                                   ("rejected", "program", "unknown")]:
            execution = {"outcome": {"status": status}, "cost": None, "error": "Raise(spoofed)",
                         "failure": {"kind": kind, "code": code}}
            with self.subTest(status=status, code=code), patch.object(GATE["subprocess"], "run", return_value=SimpleNamespace(returncode=0, stdout=json.dumps([execution]), stderr="")):
                with self.assertRaises(ValueError):
                    GATE["run_batch"](Path("/tmp/cli"), Path("/tmp/retained.hex"), Path("/tmp/solutions.json"), 1)


if __name__ == "__main__":
    unittest.main()
