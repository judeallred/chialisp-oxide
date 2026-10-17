#!/usr/bin/env python3
"""Check fresh corpus evidence and optionally an exact source-refactor baseline.

This reads reports; it does not execute the compiler or prove Rust semantics.
Regenerate corpus/validation.json with ./scripts/chialisp-oxide validate first. Supply a
report captured before a source-only refactor to require unchanged artifacts,
costs, native/reference outcomes, vectors, and ledger results. No permanent
compiler-byte baseline is imposed when --baseline is omitted. Explicit semantic
comparison permits Rust codegen/cost/diagnostic changes while retaining recorded
outcomes and independent reference evidence. Cargo-migration mode permits only
verified Cargo authoring metadata and source-snapshot changes. Explicitly allowing
added puzzles still compares every baseline puzzle in its original order.
The add-rue comparison admits newly validated Rue sources/builds and cross-language
ledger runs while preserving every existing Rust/Chialisp observation.
Runtime-frames mode permits only the reviewed optimization pipeline transition
and optimized Rust artifacts/metrics for the two recursive tree helpers.
"""

import argparse
import copy
import hashlib
import json
from pathlib import Path
import re
import tempfile
import unittest

from cargo_authoring import check_recorded, contained_file, snapshot
from vm_failures import PROTOCOL, check_execution, check_native_outcome
from ledger_rejections import PROTOCOL as LEDGER_PROTOCOL, PUZZLES as LEDGER_PUZZLES, check_ledger_report


ROOT = Path(__file__).resolve().parents[1]
HEX = re.compile(r"(?:[0-9a-f]{2})+")
HASH = re.compile(r"[0-9a-f]{64}")
SLUG = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*")
PIPELINE = {
    "rust_mir": {"unoptimized": [], "optimized": ["typed_ir_tree_guards", "runtime_frame_elision", "rue_lir"]},
    "rue_source": {"unoptimized": [], "optimized": ["rue_lir"]},
}
PREVIOUS_PIPELINE = {
    "rust_mir": {"unoptimized": [], "optimized": ["typed_ir_tree_guards", "rue_lir"]},
    "rue_source": {"unoptimized": [], "optimized": ["rue_lir"]},
}
RUNTIME_FRAME_PUZZLES = {"rightmost-tree", "last-list-element"}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def object_without_duplicates(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, f"Duplicate JSON key: {key}")
        result[key] = value
    return result


def load_report(path):
    return json.loads(path.read_text(), object_pairs_hook=object_without_duplicates)


def modes(rows, label):
    result = {}
    for row in rows:
        backend, optimize = row["backend"], row["optimize_lir"]
        require(isinstance(backend, str) and type(optimize) is bool,
                f"{label}: malformed backend/options")
        key = (backend, optimize)
        require(key not in result, f"{label}: duplicate execution mode {key}")
        result[key] = row
    return result


def check_outcome(outcome, label):
    require(outcome["status"] in ("accepted", "rejected"), f"{label}: unknown outcome")
    if outcome["status"] == "accepted":
        require(isinstance(outcome.get("output_hex"), str)
                and HEX.fullmatch(outcome["output_hex"]), f"{label}: malformed output bytes")


def check_report(report, *, require_typed_failures=True):
    require(type(report.get("schema_version")) is int and report["schema_version"] == 1,
            "Expected corpus schema_version 1")
    if require_typed_failures:
        require(report.get("vm_failure_protocol") == PROTOCOL, "Missing/unsupported VM failure protocol")
        require(report.get("optimization_pipeline") == PIPELINE, "Missing or unsupported recorded optimization pipeline")
    elif "optimization_pipeline" in report:
        require(report["optimization_pipeline"] in (PIPELINE, PREVIOUS_PIPELINE), "Unsupported historical optimization pipeline")
    check_ledger_report(report, require_typed=require_typed_failures)
    puzzles = report["puzzles"]
    require(isinstance(puzzles, list) and puzzles, "Expected a nonempty puzzle corpus")
    slugs = set()
    cases = ledger = 0
    for puzzle in puzzles:
        slug = puzzle["slug"]
        require(isinstance(slug, str) and SLUG.fullmatch(slug), "Invalid puzzle slug")
        require(slug not in slugs, f"Duplicate puzzle: {slug}")
        slugs.add(slug)
        require(puzzle["validated"] is True, f"{slug}: not validated")
        artifacts = modes(puzzle["artifacts"], slug)
        require({("rust_mir", False), ("rust_mir", True), ("chialisp", True)} <= artifacts.keys(),
                f"{slug}: missing optimized/unoptimized Rust or independent Chialisp artifact")
        if "rue" in puzzle:
            require(artifacts.keys() == {("rust_mir", False), ("rust_mir", True), ("chialisp", True),
                                         ("rue_source", False), ("rue_source", True)},
                    f"{slug}: missing independently compiled Rue mode")
        for mode, artifact in artifacts.items():
            value = artifact["clvm_hex"]
            require(isinstance(value, str) and HEX.fullmatch(value), f"{slug} {mode}: bad CLVM hex")
            require(type(artifact["bytes"]) is int and artifact["bytes"] == len(value) // 2,
                    f"{slug} {mode}: serialized size mismatch")
            require(isinstance(artifact["tree_hash"], str) and HASH.fullmatch(artifact["tree_hash"]),
                    f"{slug} {mode}: malformed tree hash")
        require(puzzle["cases"], f"{slug}: no explicit execution vectors")
        for case in puzzle["cases"]:
            label = f"{slug}/{case['label']}"
            check_native_outcome(case["native"], label)
            runs = modes(case["runs"], label)
            require(runs.keys() == artifacts.keys(), f"{label}: execution modes differ from artifacts")
            for mode, run in runs.items():
                execution = run["execution"]
                check_execution(execution, f"{label} {mode}", require_typed=require_typed_failures)
                require(execution["outcome"] == case["native"], f"{label} {mode}: native/VM mismatch")
            cases += 1
        for control in puzzle.get("resource_controls", {}).get("cases", []):
            modes(control["runs"], f"{slug} resource control")
        if "module_identity" in puzzle:
            modes(puzzle["module_identity"]["comparisons"], f"{slug} module identity")
        for factory in puzzle.get("factory_checks", []):
            modes([factory], f"{slug} produced-program check")
            for run in factory["application_runs"]:
                check_execution(run["execution"], f"{slug} factory application", require_typed=require_typed_failures)
        for scenario in puzzle["simulator"]:
            require(scenario["passed"] is True, f"{slug}: ledger scenario failed: {scenario['name']}")
            ledger += 1
        if "rue" in puzzle:
            comparisons = puzzle["simulator_comparisons"]
            if puzzle["kind"] in ("pure_function", "program_factory"):
                require(not comparisons and not puzzle["simulator"], f"{slug}: non-spend function claims ledger evidence")
            else:
                require(modes(comparisons, f"{slug} ledger comparisons").keys()
                        == {("chialisp", True), ("rue_source", True)},
                        f"{slug}: missing Chialisp/Rue ledger comparisons")
                require(all(item["scenarios"] == puzzle["simulator"] for item in comparisons),
                        f"{slug}: ledger scenario results differ across languages")
    return len(puzzles), cases, ledger


def check_sources(report, root, require_rue=False):
    root = root.resolve()
    if require_rue:
        require(report.get("optimization_pipeline") == PIPELINE, "Missing or unsupported recorded optimization pipeline")
        require(report.get("rue_local_patches") == [
            f"https://github.com/xch-dev/rue/issues/{number}" for number in (108, 109, 110, 111, 112, 113)
        ], "Incomplete Rue local patch provenance")
    for puzzle in report["puzzles"]:
        source = contained_file(root, f"puzzles/{puzzle['slug']}/puzzle.rs")
        require(source.read_bytes().decode() == puzzle["source"],
                f"{puzzle['slug']}: report contains stale puzzle source; rerun validation")
        reference = contained_file(root, f"puzzles/{puzzle['slug']}/reference.clsp")
        require(reference.read_bytes().decode() == puzzle["reference"],
                f"{puzzle['slug']}: report contains stale Chialisp reference; rerun validation")
        if require_rue or "rue" in puzzle:
            rue = puzzle["rue"]
            require(rue["path"] == f"puzzles/{puzzle['slug']}/puzzle.rue", "Rue source identity mismatch")
            for dependency in [rue, *rue["dependencies"]]:
                payload = contained_file(root, dependency["path"]).read_bytes()
                require(payload.decode() == dependency["source"] and hashlib.sha256(payload).hexdigest() == dependency["sha256"],
                        f"Stale Rue source: {dependency['path']}")
        seen = set()
        for dependency in puzzle["source_dependencies"]:
            path = dependency["path"]
            require(path not in seen, f"{puzzle['slug']}: duplicate dependency {path}")
            seen.add(path)
            resolved = contained_file(root, path)
            payload = resolved.read_bytes()
            require(payload.decode() == dependency["source"], f"Stale dependency source: {path}")
            require(hashlib.sha256(payload).hexdigest() == dependency["sha256"],
                    f"Stale dependency hash: {path}")


def behavior(report):
    """Only authoring text/dependency snapshots may differ in a source refactor."""
    result = copy.deepcopy(report)
    for puzzle in result["puzzles"]:
        del puzzle["source"]
        del puzzle["source_dependencies"]
    return result


def semantic_behavior(report):
    """Permit only measured Rust codegen changes; preserve all recorded outcomes."""
    result = behavior(report)
    for puzzle in result["puzzles"]:
        for artifact in puzzle["artifacts"]:
            if artifact["backend"] == "rust_mir":
                for field in ("bytes", "clvm_hex", "tree_hash", "disassembly"):
                    del artifact[field]
        for case in puzzle["cases"]:
            for run in case["runs"]:
                if run["backend"] == "rust_mir":
                    del run["execution"]["cost"]
                    del run["execution"]["error"]
                    run["execution"].pop("failure", None)
    return result


def cargo_migration_behavior(report):
    """Current authoring must be verified before permitting its metadata to change."""
    result = behavior(report)
    result.pop("authoring", None)
    return result


def before_rue_addition(report):
    """Project out only the new independently validated Rue evidence."""
    result = copy.deepcopy(report)
    for puzzle in result["puzzles"]:
        puzzle.pop("rue", None)
        puzzle.pop("simulator_comparisons", None)
        puzzle["artifacts"] = [item for item in puzzle["artifacts"] if item["backend"] != "rue_source"]
        for case in puzzle["cases"]:
            case["runs"] = [item for item in case["runs"] if item["backend"] != "rue_source"]
    return cargo_migration_behavior(result)


def retained_puzzles(baseline, current):
    """Permit new rows without weakening comparisons of existing puzzles."""
    expected = [puzzle["slug"] for puzzle in baseline["puzzles"]]
    require(len(expected) == len(set(expected)), "Baseline has duplicate puzzle slugs")
    retained = [puzzle for puzzle in current["puzzles"] if puzzle["slug"] in expected]
    require([puzzle["slug"] for puzzle in retained] == expected,
            "Baseline puzzles were removed, duplicated, or reordered")
    result = copy.deepcopy(current)
    result["puzzles"] = copy.deepcopy(retained)
    return result


def runtime_frames_behavior(report):
    """Retain every source/contract and only omit the two reviewed on metrics."""
    result = copy.deepcopy(report)
    result["optimization_pipeline"] = copy.deepcopy(PIPELINE)
    require(RUNTIME_FRAME_PUZZLES <= {p["slug"] for p in result["puzzles"]}, "Missing reviewed runtime-frame fixture")
    for puzzle in result["puzzles"]:
        if puzzle["slug"] not in RUNTIME_FRAME_PUZZLES:
            continue
        for artifact in puzzle["artifacts"]:
            if artifact["backend"] == "rust_mir" and artifact["optimize_lir"] is True:
                for field in ("bytes", "clvm_hex", "tree_hash", "disassembly"):
                    del artifact[field]
        for case in puzzle["cases"]:
            for run in case["runs"]:
                if run["backend"] == "rust_mir" and run["optimize_lir"] is True:
                    for field in ("cost", "error", "failure"):
                        run["execution"].pop(field, None)
    return result


def comparison_projection(baseline, current, comparison, *, allow_added=False):
    """Validate current failures before dropping any comparison-only metadata.

    Historical reports predate typed failures. Omit only the newly introduced
    failure field at matching old VM envelopes; retain it when the baseline has
    typed metadata, and never use the baseline exemption to validate current data.
    """
    check_report(current)
    check_report(baseline, require_typed_failures=False)
    if comparison == "runtime-frames":
        require(not allow_added, "Runtime-frame comparison cannot add puzzles")
        require(baseline.get("optimization_pipeline") == PREVIOUS_PIPELINE, "Runtime-frame baseline must use the reviewed previous pipeline")
        # Unlike older migration modes, this baseline already has typed failures;
        # validate them rather than applying historical field exemptions.
        typed_baseline = {**baseline, "optimization_pipeline": PIPELINE}
        check_report(typed_baseline)
        return runtime_frames_behavior(baseline), runtime_frames_behavior(current)
    compared = retained_puzzles(baseline, current) if allow_added else copy.deepcopy(current)

    def historical_shape(old, new):
        if isinstance(old, dict) and isinstance(new, dict):
            if {"outcome", "cost", "error"} <= old.keys() and "failure" not in old:
                new.pop("failure", None)
            if {"name", "passed", "scope"} <= old.keys() and old.get("scope") == "ledger" and "rejections" not in old:
                new.pop("rejections", None)
            if {"sdk", "consensus", "vm", "network"} <= old.keys() and "rejection_protocol" not in old:
                new.pop("rejection_protocol", None)
            for key in old.keys() & new.keys():
                historical_shape(old[key], new[key])
        elif isinstance(old, list) and isinstance(new, list):
            for left, right in zip(old, new):
                historical_shape(left, right)

    historical_shape(baseline, compared)
    if "vm_failure_protocol" not in baseline:
        compared.pop("vm_failure_protocol", None)
    projection = {"exact": behavior, "semantic": semantic_behavior,
                  "cargo-migration": cargo_migration_behavior, "add-rue": before_rue_addition}[comparison]
    return projection(baseline), projection(compared)


def codegen_changes(before, after):
    changes = []
    for left, right in zip(before["puzzles"], after["puzzles"]):
        left_modes = modes(left["artifacts"], left["slug"])
        right_modes = modes(right["artifacts"], right["slug"])
        for mode, old in left_modes.items():
            if mode[0] != "rust_mir":
                continue
            new = right_modes[mode]
            costs, diagnostics = [], []
            for old_case, new_case in zip(left["cases"], right["cases"]):
                previous = modes(old_case["runs"], left["slug"])[mode]["execution"]
                current = modes(new_case["runs"], right["slug"])[mode]["execution"]
                if previous["cost"] != current["cost"]:
                    costs.append({"case": old_case["label"], "before": previous["cost"],
                                  "after": current["cost"]})
                if previous["error"] != current["error"]:
                    diagnostics.append({"case": old_case["label"], "before": previous["error"],
                                        "after": current["error"]})
            if old != new or costs or diagnostics:
                changes.append({
                    "puzzle": left["slug"], "optimize_lir": mode[1],
                    "bytes_before": old["bytes"], "bytes_after": new["bytes"],
                    "bytes_delta": new["bytes"] - old["bytes"],
                    "clvm_bytes_changed": old["clvm_hex"] != new["clvm_hex"],
                    "tree_hash_before": old["tree_hash"], "tree_hash_after": new["tree_hash"],
                    "cost_changes": costs, "diagnostic_changes": diagnostics,
                })
    return changes


def first_difference(before, after, path="$"):
    if type(before) is not type(after):
        return f"{path}: value type changed"
    if isinstance(before, dict):
        if before.keys() != after.keys():
            return f"{path}: object keys changed"
        for key in sorted(before):
            difference = first_difference(before[key], after[key], f"{path}.{key}")
            if difference:
                return difference
    elif isinstance(before, list):
        if len(before) != len(after):
            return f"{path}: item count changed ({len(before)} -> {len(after)})"
        for index, (left, right) in enumerate(zip(before, after)):
            difference = first_difference(left, right, f"{path}[{index}]")
            if difference:
                return difference
    elif before != after:
        return f"{path}: value changed"
    return None


class ComparisonTests(unittest.TestCase):
    def setUp(self):
        # Values represent independent wire/result observations, not SDK output.
        self.original = {"schema_version": 1, "puzzles": [{
            "source": "old source", "source_dependencies": [],
            "artifacts": [{"clvm_hex": "ff0180", "tree_hash": "a" * 64}],
            "cases": [{"native": {"status": "accepted", "output_hex": "80"},
                       "execution": {"cost": 20}}],
            "simulator": [{"passed": True}],
        }]}

    def test_runtime_frames_preserves_every_unreviewed_record(self):
        artifact = {"clvm_hex": "80", "bytes": 1, "tree_hash": "a" * 64, "disassembly": "()"}
        builds = [("rust_mir", False), ("rust_mir", True), ("chialisp", True)]
        baseline = {"schema_version": 1, "vm_failure_protocol": PROTOCOL,
                    "optimization_pipeline": copy.deepcopy(PREVIOUS_PIPELINE),
                    "simulator": {"sdk": "chia-sdk-test 0.36.0", "consensus": "chia-consensus 0.36.1", "vm": "clvmr 0.16.2", "network": "TESTNET11", "rejection_protocol": LEDGER_PROTOCOL},
                    "puzzles": []}
        for slug in ("rightmost-tree", "last-list-element", "untouched"):
            baseline["puzzles"].append({
                "slug": slug, "kind": "pure_function", "environment": "raw", "validated": True,
                "source": "same Rust", "source_dependencies": [], "reference": "same Chialisp",
                "artifacts": [{**artifact, "backend": backend, "optimize_lir": optimize} for backend, optimize in builds],
                "cases": [{"label": "accepted", "solution": "()", "native": {"status": "accepted", "output_hex": "80"},
                           "runs": [{"backend": backend, "optimize_lir": optimize, "execution": {
                               "outcome": {"status": "accepted", "output_hex": "80"}, "cost": 20, "error": None}}
                                    for backend, optimize in builds]},
                          {"label": "rejected", "solution": "1", "native": {"status": "rejected"},
                           "runs": [{"backend": backend, "optimize_lir": optimize, "execution": {
                               "outcome": {"status": "rejected"}, "cost": None, "error": "Raise(original)",
                               "failure": {"kind": "program", "code": "raise"}}}
                                    for backend, optimize in builds]}],
                "simulator": [],
            })
        current = copy.deepcopy(baseline)
        current["optimization_pipeline"] = copy.deepcopy(PIPELINE)
        for puzzle in current["puzzles"][:2]:
            puzzle["artifacts"][1].update(clvm_hex="01", disassembly="1", tree_hash="b" * 64)
            puzzle["cases"][0]["runs"][1]["execution"]["cost"] = 10
            puzzle["cases"][1]["runs"][1]["execution"].update(
                error="PathIntoAtom", failure={"kind": "program", "code": "path_into_atom"})
        self.assertIsNone(first_difference(*comparison_projection(baseline, current, "runtime-frames")))
        for mutation in ("source", "dependency", "reference", "off_artifact", "off_cost", "reference_cost",
                         "other_artifact", "other_cost", "result", "native", "resource", "missing_failure",
                         "solution", "abi", "ledger", "remove", "reorder", "case_order", "run_order", "pipeline"):
            bad = copy.deepcopy(current)
            puzzle = bad["puzzles"][0]
            if mutation == "source": puzzle["source"] = "changed Rust"
            elif mutation == "dependency": puzzle["source_dependencies"] = [{"path": "changed.rs"}]
            elif mutation == "reference": puzzle["reference"] = "changed reference"
            elif mutation in ("off_artifact", "other_artifact"):
                row = puzzle if mutation == "off_artifact" else bad["puzzles"][2]
                row["artifacts"][0 if mutation == "off_artifact" else 1]["tree_hash"] = "c" * 64
            elif mutation in ("off_cost", "reference_cost", "other_cost"):
                row = bad["puzzles"][2] if mutation == "other_cost" else puzzle
                row["cases"][0]["runs"][{"off_cost": 0, "reference_cost": 2, "other_cost": 1}[mutation]]["execution"]["cost"] += 1
            elif mutation == "result": puzzle["cases"][0]["runs"][1]["execution"]["outcome"]["output_hex"] = "01"
            elif mutation == "native": puzzle["cases"][0]["native"]["output_hex"] = "01"
            elif mutation == "resource": puzzle["cases"][1]["runs"][1]["execution"]["failure"] = {"kind": "resource", "code": "cost_exceeded"}
            elif mutation == "missing_failure": del puzzle["cases"][1]["runs"][1]["execution"]["failure"]
            elif mutation == "solution": puzzle["cases"][0]["solution"] = "2"
            elif mutation == "abi": puzzle["environment"] = "ignored"
            elif mutation == "ledger": puzzle["simulator"] = [{"passed": True}]
            elif mutation == "remove": bad["puzzles"].pop()
            elif mutation == "reorder": bad["puzzles"].reverse()
            elif mutation == "case_order": puzzle["cases"].reverse()
            elif mutation == "run_order": puzzle["cases"][0]["runs"].reverse()
            else: bad["optimization_pipeline"]["rue_source"]["optimized"].append("unexpected")
            with self.subTest(mutation=mutation):
                try:
                    difference = first_difference(*comparison_projection(baseline, bad, "runtime-frames"))
                except (ValueError, KeyError):
                    continue
                self.assertIsNotNone(difference)
        with self.assertRaises(ValueError):
            comparison_projection(current, current, "runtime-frames")

    def test_current_failures_are_checked_before_every_comparison_projection(self):
        artifact = {"clvm_hex": "80", "bytes": 1, "tree_hash": "a" * 64, "disassembly": "()"}
        builds = [("rust_mir", False), ("rust_mir", True), ("chialisp", True)]
        current = {"schema_version": 1, "vm_failure_protocol": PROTOCOL, "optimization_pipeline": copy.deepcopy(PIPELINE),
                   "simulator": {"sdk": "chia-sdk-test 0.36.0", "consensus": "chia-consensus 0.36.1", "vm": "clvmr 0.16.2", "network": "TESTNET11", "rejection_protocol": LEDGER_PROTOCOL}, "puzzles": [{
            "slug": "example", "kind": "pure_function", "validated": True, "source": "source", "source_dependencies": [],
            "artifacts": [{**artifact, "backend": backend, "optimize_lir": optimize} for backend, optimize in builds],
            "cases": [{"label": "reject", "native": {"status": "rejected"}, "runs": [
                {"backend": backend, "optimize_lir": optimize, "execution": {
                    "outcome": {"status": "rejected"}, "cost": None, "error": "Raise(spoofed)",
                    "failure": {"kind": "program", "code": "raise"}}} for backend, optimize in builds]}],
            "simulator": [],
        }]}
        check_report(current)
        for value in (0, 1, 0.0, 1.0):
            for field in ("artifacts", "runs", "factory", "identity", "resource", "validated"):
                corrupted = copy.deepcopy(current)
                puzzle = corrupted["puzzles"][0]
                if field == "validated":
                    puzzle["validated"] = value
                else:
                    modes_to_change = {"artifacts": puzzle["artifacts"], "runs": puzzle["cases"][0]["runs"]}
                    if field in ("artifacts", "runs"):
                        rows = modes_to_change[field]
                    elif field == "factory":
                        rows = puzzle["factory_checks"] = [{"backend": "rust_mir", "optimize_lir": bool(value), "application_runs": []}]
                    elif field == "identity":
                        rows = [{"backend": "rust_mir", "optimize_lir": bool(value)}]
                        puzzle["module_identity"] = {"comparisons": rows}
                    else:
                        rows = [{"backend": "rust_mir", "optimize_lir": bool(value)}]
                        puzzle["resource_controls"] = {"cases": [{"runs": rows}]}
                    row = next(row for row in rows if row["optimize_lir"] is bool(value))
                    row["optimize_lir"] = value
                with self.subTest(field=field, numeric=value), self.assertRaises(ValueError):
                    check_report(corrupted)
        baseline = copy.deepcopy(current)
        del baseline["vm_failure_protocol"]
        del baseline["simulator"]["rejection_protocol"]
        for run in baseline["puzzles"][0]["cases"][0]["runs"]:
            del run["execution"]["failure"]
        for comparison in ("exact", "semantic", "cargo-migration", "add-rue"):
            before, after = comparison_projection(baseline, current, comparison)
            self.assertIsNone(first_difference(before, after))
            for mutation in ("missing", "unknown", "resource", "infrastructure", "inconclusive"):
                corrupted = copy.deepcopy(current)
                execution = corrupted["puzzles"][0]["cases"][0]["runs"][0]["execution"]
                if mutation == "missing": del execution["failure"]
                elif mutation == "unknown": execution["failure"]["code"] = "future_code"
                elif mutation == "infrastructure": execution["failure"] = {"kind": "infrastructure", "code": "internal_error"}
                else:
                    execution["failure"] = {"kind": "resource", "code": "cost_exceeded"}
                    if mutation == "inconclusive": execution["outcome"]["status"] = "inconclusive"
                with self.subTest(comparison=comparison, mutation=mutation), self.assertRaises(ValueError):
                    comparison_projection(baseline, corrupted, comparison)
        scenarios = [{"name": row["name"], "passed": True, "scope": "ledger",
                      "rejections": [{"stage": "consensus", "code": code} for code in row["codes"]]}
                     for row in LEDGER_PUZZLES["signature-payment"]]
        for report in (current, baseline):
            report["puzzles"][0].update(slug="signature-payment", kind="coin_puzzle", simulator=copy.deepcopy(scenarios),
                simulator_comparisons=[{"backend": backend, "optimize_lir": True, "scenarios": copy.deepcopy(scenarios)}
                                       for backend in ("chialisp", "rue_source")])
        for collection in [baseline["puzzles"][0]["simulator"], *[row["scenarios"] for row in baseline["puzzles"][0]["simulator_comparisons"]]]:
            for row in collection: del row["rejections"]
        for comparison in ("exact", "semantic", "cargo-migration", "add-rue"):
            self.assertIsNone(first_difference(*comparison_projection(baseline, current, comparison)))
            for mutation in ("missing", "MissingKey", "GeneratorRuntimeError", "CostExceeded", "UnknownCode", "DoubleSpend", "omitted_group", "unknown_coin"):
                corrupted = copy.deepcopy(current)
                puzzle = corrupted["puzzles"][0]
                record = puzzle["simulator"][0]
                if mutation == "missing": del record["rejections"]
                elif mutation == "omitted_group":
                    puzzle["simulator"].pop(0)
                    for row in puzzle["simulator_comparisons"]: row["scenarios"].pop(0)
                elif mutation == "unknown_coin":
                    puzzle["slug"] = "unreviewed-coin"
                    puzzle["simulator"] = []
                    for row in puzzle["simulator_comparisons"]: row["scenarios"] = []
                else: record["rejections"][0]["code"] = mutation
                with self.subTest(comparison=comparison, ledger_mutation=mutation), self.assertRaises(ValueError):
                    comparison_projection(baseline, corrupted, comparison)
        for index in range(2):
            for value in (0, 1, 0.0, 1.0):
                corrupted = copy.deepcopy(current)
                corrupted["puzzles"][0]["simulator_comparisons"][index]["optimize_lir"] = value
                with self.subTest(ledger_mode=index, numeric=value), self.assertRaises(ValueError):
                    check_report(corrupted)
        # Typed baselines retain their code distinction in exact comparisons.
        changed = copy.deepcopy(current)
        changed["puzzles"][0]["cases"][0]["runs"][0]["execution"]["failure"]["code"] = "path_into_atom"
        self.assertIsNotNone(first_difference(*comparison_projection(current, changed, "exact")))


    def test_source_and_dependency_snapshot_changes_are_allowed(self):
        updated = copy.deepcopy(self.original)
        updated["puzzles"][0]["source"] = "readable source"
        updated["puzzles"][0]["source_dependencies"] = [{"path": "new shared SDK source"}]
        self.assertIsNone(first_difference(behavior(self.original), behavior(updated)))

    def test_changed_wire_cost_result_or_ledger_is_detected(self):
        mutations = [
            ("artifacts", "clvm_hex", "ff0181"),
            ("artifacts", "tree_hash", "b" * 64),
            ("cases", "native", {"status": "rejected"}),
            ("cases", "execution", {"cost": 21}),
            ("simulator", "passed", False),
        ]
        for collection, field, value in mutations:
            with self.subTest(field=field):
                updated = copy.deepcopy(self.original)
                updated["puzzles"][0][collection][0][field] = value
                self.assertIsNotNone(first_difference(behavior(self.original), behavior(updated)))

    def test_missing_vectors_and_metadata_changes_are_detected(self):
        for key in ("cases", "artifacts", "simulator"):
            updated = copy.deepcopy(self.original)
            updated["puzzles"][0][key] = []
            self.assertIsNotNone(first_difference(behavior(self.original), behavior(updated)))
        updated = copy.deepcopy(self.original)
        updated["schema_version"] = True
        self.assertIsNotNone(first_difference(behavior(self.original), behavior(updated)))

    def test_stale_puzzle_and_dependency_evidence_is_rejected(self):
        with tempfile.TemporaryDirectory(prefix="chialisp-oxide-readable-test-") as temporary:
            root = Path(temporary)
            puzzle = root / "puzzles/example/puzzle.rs"
            puzzle.parent.mkdir(parents=True)
            puzzle.write_text("fn puzzle() {}\n")
            reference = puzzle.with_name("reference.clsp")
            reference.write_text("(mod () ())\n")
            dependency = root / "domain.rs"
            dependency.write_text("// domain source\n")
            report = {"puzzles": [{
                "slug": "example", "source": puzzle.read_text(), "reference": reference.read_text(),
                "source_dependencies": [{
                    "path": "domain.rs", "source": dependency.read_text(),
                    "sha256": hashlib.sha256(dependency.read_bytes()).hexdigest(),
                }],
            }]}
            check_sources(report, root)
            puzzle.write_text("fn puzzle() { panic!() }\n")
            with self.assertRaisesRegex(ValueError, "stale puzzle source"):
                check_sources(report, root)
            puzzle.write_text(report["puzzles"][0]["source"])
            reference.write_text("(mod () (x))\n")
            with self.assertRaisesRegex(ValueError, "stale Chialisp reference"):
                check_sources(report, root)
            reference.write_text(report["puzzles"][0]["reference"])
            dependency.write_text("// changed condition encoder\n")
            with self.assertRaisesRegex(ValueError, "Stale dependency source"):
                check_sources(report, root)

    def test_semantic_mode_permits_only_rust_codegen_metrics(self):
        original = {"puzzles": [{
            "source": "source", "source_dependencies": [],
            "artifacts": [{"backend": backend, "optimize_lir": True,
                           "clvm_hex": "ff0180", "bytes": 3,
                           "tree_hash": "a" * 64, "disassembly": "(q)"}
                          for backend in ("rust_mir", "chialisp")],
            "cases": [{"runs": [{"backend": backend, "optimize_lir": True,
                                  "execution": {"cost": 20, "error": None,
                                                "outcome": {"status": "accepted", "output_hex": "80"}}}
                                 for backend in ("rust_mir", "chialisp")]}],
        }]}
        updated = copy.deepcopy(original)
        updated["puzzles"][0]["artifacts"][0].update(
            {"clvm_hex": "ff0181", "tree_hash": "b" * 64, "disassembly": "(q . 1)"})
        updated["puzzles"][0]["cases"][0]["runs"][0]["execution"]["cost"] = 19
        self.assertIsNone(first_difference(semantic_behavior(original), semantic_behavior(updated)))
        self.assertIsNotNone(first_difference(behavior(original), behavior(updated)))
        for backend in range(2):
            mutated = copy.deepcopy(updated)
            mutated["puzzles"][0]["cases"][0]["runs"][backend]["execution"]["outcome"]["output_hex"] = "01"
            self.assertIsNotNone(first_difference(semantic_behavior(original), semantic_behavior(mutated)))
        updated["puzzles"][0]["artifacts"][1]["clvm_hex"] = "ff0181"
        self.assertIsNotNone(first_difference(semantic_behavior(original), semantic_behavior(updated)))

    def test_cargo_migration_retains_exact_artifacts_costs_and_wire_inputs(self):
        updated = copy.deepcopy(self.original)
        updated["authoring"] = {"mode": "cargo_library"}  # Validated separately before comparison.
        updated["puzzles"][0]["source"] = "#[chialisp_oxide::puzzle] fn puzzle() {}"
        self.assertIsNone(first_difference(cargo_migration_behavior(self.original), cargo_migration_behavior(updated)))
        self.assertIsNotNone(first_difference(behavior(self.original), behavior(updated)))
        for collection, field, value in [
            ("artifacts", "clvm_hex", "ff0181"),
            ("artifacts", "tree_hash", "b" * 64),
            ("cases", "execution", {"cost": 21}),
            ("cases", "native", {"status": "accepted", "output_hex": "01"}),
            ("cases", "solution", "((7))"),
            ("simulator", "passed", False),
        ]:
            with self.subTest(field=field):
                mutated = copy.deepcopy(updated)
                mutated["puzzles"][0][collection][0][field] = value
                self.assertIsNotNone(first_difference(cargo_migration_behavior(self.original), cargo_migration_behavior(mutated)))

    def test_cargo_authoring_hashes_export_identity_and_paths_are_checked(self):
        with tempfile.TemporaryDirectory(prefix="chialisp-oxide-authoring-test-") as temporary:
            root = Path(temporary)
            paths = [".cargo/config.toml", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
                     "puzzles/Cargo.toml", "puzzles/lib.rs", "puzzles/fixture_keys.rs", "puzzles/example/puzzle.rs",
                     "puzzles/authoring.json", "crates/chialisp-oxide/Cargo.toml",
                     "crates/chialisp-oxide/src/lib.rs", "crates/chialisp-oxide/src/domain.rs", "crates/chialisp-oxide/src/tree.rs",
                     "crates/chialisp-oxide/src/owned_tree.rs", "crates/chialisp-oxide/src/hashing.rs",
                     "crates/chialisp-oxide-macros/Cargo.toml", "crates/chialisp-oxide-macros/src/lib.rs"]
            for path in paths:
                target = root / path
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text("fixture input\n")
            metadata = {"schema_version": 1, "mode": "cargo_library", "package": "chialisp-oxide-puzzles",
                        "manifest": "puzzles/Cargo.toml", "library": "puzzles/lib.rs",
                        "exports": {"example": {"entry": "chialisp_oxide_puzzles::example::puzzle",
                                                "source": "puzzles/example/puzzle.rs"}},
                        "inputs": sorted(paths)}
            config = root / "puzzles/authoring.json"
            config.write_text(json.dumps(metadata))
            recorded = snapshot(root)
            check_recorded(recorded, root, ["example"])
            for mutation in ("hash", "source", "duplicate", "escape"):
                bad = copy.deepcopy(recorded)
                if mutation == "hash":
                    bad["sources"][0]["sha256"] = "0" * 64
                elif mutation == "source":
                    bad["exports"]["example"]["source"] = "puzzles/other/puzzle.rs"
                elif mutation == "duplicate":
                    bad["sources"].append(copy.deepcopy(bad["sources"][0]))
                else:
                    bad["sources"][0]["path"] = "../outside"
                with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                    check_recorded(bad, root, ["example"])
            (root / "crates/chialisp-oxide/src/domain.rs").write_text("changed constructor\n")
            with self.assertRaisesRegex(ValueError, "stale"):
                check_recorded(recorded, root, ["example"])
            for path in ("../outside", "puzzles/../Cargo.toml", "./Cargo.toml", "Cargo.toml"):
                invalid = copy.deepcopy(metadata)
                invalid["inputs"].append(path)
                config.write_text(json.dumps(invalid))
                with self.subTest(path=path), self.assertRaises(ValueError):
                    snapshot(root)


    def test_added_puzzles_do_not_hide_existing_artifact_abi_cost_or_policy_changes(self):
        baseline = copy.deepcopy(self.original)
        baseline["puzzles"][0]["slug"] = "original"
        baseline["puzzles"][0]["environment"] = "strict"
        baseline["puzzles"].append({**copy.deepcopy(baseline["puzzles"][0]), "slug": "second"})
        current = copy.deepcopy(baseline)
        current["authoring"] = {"mode": "cargo_library"}  # Freshness checked before comparison.
        current["puzzles"].insert(1, {**copy.deepcopy(current["puzzles"][0]), "slug": "new"})
        compare = lambda value: first_difference(cargo_migration_behavior(baseline),
                                                cargo_migration_behavior(retained_puzzles(baseline, value)))
        self.assertIsNone(compare(current))
        self.assertIsNotNone(first_difference(cargo_migration_behavior(baseline),
                                             cargo_migration_behavior(current)))
        for mutation in ("artifact", "cost", "abi", "native", "ledger", "remove", "reorder", "duplicate"):
            changed = copy.deepcopy(current)
            original = changed["puzzles"][0]
            if mutation == "artifact":
                original["artifacts"][0]["clvm_hex"] = "ff0181"
            elif mutation == "cost":
                original["cases"][0]["execution"]["cost"] += 1
            elif mutation == "abi":
                original["environment"] = "ignored"
            elif mutation == "native":
                original["cases"][0]["native"] = {"status": "rejected"}
            elif mutation == "ledger":
                original["simulator"][0]["passed"] = False
            elif mutation == "remove":
                changed["puzzles"].pop(0)
            elif mutation == "reorder":
                changed["puzzles"].reverse()
            else:
                changed["puzzles"].append(copy.deepcopy(original))
            with self.subTest(mutation=mutation):
                if mutation in ("remove", "reorder", "duplicate"):
                    with self.assertRaises(ValueError):
                        compare(changed)
                else:
                    self.assertIsNotNone(compare(changed))

    def test_added_rue_evidence_retains_every_existing_observation(self):
        baseline = {"puzzles": [{
            "source": "rust", "source_dependencies": [],
            "artifacts": [{"backend": "rust_mir", "cost": 20}, {"backend": "chialisp", "cost": 21}],
            "cases": [{"native": {"status": "accepted"}, "runs": [
                {"backend": "rust_mir", "cost": 20}, {"backend": "chialisp", "cost": 21}]}],
            "environment": "strict", "simulator": [{"passed": True}],
        }]}
        current = copy.deepcopy(baseline)
        puzzle = current["puzzles"][0]
        puzzle.update(rue={"source": "rue"}, simulator_comparisons=[])
        puzzle["artifacts"].append({"backend": "rue_source", "cost": 19})
        puzzle["cases"][0]["runs"].append({"backend": "rue_source", "cost": 19})
        expected = cargo_migration_behavior(baseline)
        self.assertIsNone(first_difference(expected, before_rue_addition(current)))
        for field in ("rust_cost", "chialisp_cost", "native", "abi", "ledger", "removed_run"):
            changed = copy.deepcopy(current)
            row = changed["puzzles"][0]
            if field == "rust_cost":
                row["cases"][0]["runs"][0]["cost"] += 1
            elif field == "chialisp_cost":
                row["artifacts"][1]["cost"] += 1
            elif field == "native":
                row["cases"][0]["native"]["status"] = "rejected"
            elif field == "abi":
                row["environment"] = "ignored"
            elif field == "ledger":
                row["simulator"][0]["passed"] = False
            else:
                row["cases"][0]["runs"].pop(0)
            with self.subTest(field=field):
                self.assertIsNotNone(first_difference(expected, before_rue_addition(changed)))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--current", type=Path, default=ROOT / "corpus/validation.json")
    parser.add_argument("--baseline", type=Path, help="explicit pre-refactor corpus report")
    parser.add_argument("--comparison", choices=("exact", "semantic", "cargo-migration", "add-rue", "runtime-frames"), default="exact",
                        help="exact by default; semantic allows Rust codegen changes; cargo-migration allows verified authoring metadata; add-rue preserves old builds while admitting independent Rue evidence; runtime-frames permits only the two reviewed optimized recursive helpers")
    parser.add_argument("--changes", type=Path, help="write comparison scope and codegen metric changes as JSON")
    parser.add_argument("--allow-added-puzzles", action="store_true",
                        help="with cargo-migration baseline only: compare every original puzzle exactly, allowing additional rows")
    parser.add_argument("--source-root", type=Path, default=ROOT)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(ComparisonTests)
        result = unittest.TextTestRunner().run(suite)
        raise SystemExit(0 if result.wasSuccessful() else 1)
    if (args.comparison != "exact" or args.changes or args.allow_added_puzzles) and not args.baseline:
        parser.error("non-default comparison and --changes require an explicit --baseline")
    if args.allow_added_puzzles and args.comparison != "cargo-migration":
        parser.error("--allow-added-puzzles requires --comparison cargo-migration")
    changes = []
    try:
        current = load_report(args.current)
        counts = check_report(current)
        check_sources(current, args.source_root, require_rue=True)
        check_recorded(current.get("authoring"), args.source_root, [puzzle["slug"] for puzzle in current["puzzles"]])
        if args.baseline:
            baseline = load_report(args.baseline)
            check_report(baseline, require_typed_failures=False)
            if args.comparison == "add-rue":
                require(all("rue" not in puzzle and all(item["backend"] != "rue_source" for item in puzzle["artifacts"])
                            for puzzle in baseline["puzzles"]),
                        "add-rue comparison requires a baseline without Rue source evidence")
            compared = retained_puzzles(baseline, current) if args.allow_added_puzzles else current
            before, after = comparison_projection(baseline, current, args.comparison,
                                                 allow_added=args.allow_added_puzzles)
            difference = first_difference(before, after)
            require(difference is None, f"{args.comparison.capitalize()} baseline changed: {difference}")
            changes = codegen_changes(baseline, compared)
            if args.changes:
                args.changes.parent.mkdir(parents=True, exist_ok=True)
                args.changes.write_text(json.dumps({
                    "schema_version": 1, "comparison": args.comparison,
                    "allow_added_puzzles": args.allow_added_puzzles,
                    "baseline_puzzles_compared": len(baseline["puzzles"]),
                    "scope": "Recorded native/reference/CLVM outcomes and ledger scenarios; no universal compiler-preservation proof",
                    "baseline_sha256": hashlib.sha256(args.baseline.read_bytes()).hexdigest(),
                    "current_sha256": hashlib.sha256(args.current.read_bytes()).hexdigest(),
                    "puzzles": counts[0], "explicit_vectors": counts[1], "ledger_scenarios": counts[2],
                    "rust_codegen_changes": changes,
                }, indent=2) + "\n")
    except (ValueError, KeyError, TypeError, OSError) as error:
        raise SystemExit(str(error)) from error
    mode = f"{args.comparison} baseline preserved" if args.baseline else "fresh report checked; no baseline comparison"
    print(f"PASS: {counts[0]} puzzles, {counts[1]} explicit vectors, {counts[2]} ledger scenarios; {mode}")
    for change in changes:
        hash_changed = change["tree_hash_before"] != change["tree_hash_after"]
        print(f"  {change['puzzle']} optimize={str(change['optimize_lir']).lower()}: "
              f"{change['bytes_before']} -> {change['bytes_after']} bytes ({change['bytes_delta']:+}); "
              f"hash {'changed' if hash_changed else 'unchanged'}; "
              f"{len(change['cost_changes'])} tested costs, "
              f"{len(change['diagnostic_changes'])} VM diagnostics changed")


if __name__ == "__main__":
    main()
