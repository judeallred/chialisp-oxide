#!/usr/bin/env python3
"""Check fresh corpus evidence and optionally an exact source-refactor baseline.

This reads reports; it does not execute the compiler or prove Rust semantics.
Regenerate corpus/validation.json with ./scripts/chialisp-oxide validate first. Supply a
report captured before a source-only refactor to require unchanged artifacts,
costs, native/reference outcomes, vectors, and ledger results. No permanent
compiler-byte baseline is imposed when --baseline is omitted. Explicit semantic
comparison permits Rust codegen/cost/diagnostic changes while retaining recorded
outcomes and independent reference evidence. Cargo-migration mode permits only
verified Cargo authoring metadata and source-snapshot changes.
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


ROOT = Path(__file__).resolve().parents[1]
HEX = re.compile(r"(?:[0-9a-f]{2})+")
HASH = re.compile(r"[0-9a-f]{64}")
SLUG = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*")


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


def check_report(report):
    require(type(report.get("schema_version")) is int and report["schema_version"] == 1,
            "Expected corpus schema_version 1")
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
            check_outcome(case["native"], label)
            runs = modes(case["runs"], label)
            require(runs.keys() == artifacts.keys(), f"{label}: execution modes differ from artifacts")
            for mode, run in runs.items():
                execution = run["execution"]
                check_outcome(execution["outcome"], label)
                require(execution["outcome"] == case["native"], f"{label} {mode}: native/VM mismatch")
                require("CostExceeded" not in (execution["error"] or ""),
                        f"{label} {mode}: cost exhaustion cannot stand in for policy rejection")
                if execution["outcome"]["status"] == "accepted":
                    require(type(execution["cost"]) is int and execution["cost"] > 0,
                            f"{label} {mode}: missing successful execution cost")
                    require(execution["error"] is None, f"{label} {mode}: successful execution has an error")
            cases += 1
        for scenario in puzzle["simulator"]:
            require(scenario["passed"] is True, f"{slug}: ledger scenario failed: {scenario['name']}")
            ledger += 1
    return len(puzzles), cases, ledger


def check_sources(report, root):
    root = root.resolve()
    for puzzle in report["puzzles"]:
        source = root / "puzzles" / puzzle["slug"] / "puzzle.rs"
        require(source.read_bytes().decode() == puzzle["source"],
                f"{puzzle['slug']}: report contains stale puzzle source; rerun validation")
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
    return result


def cargo_migration_behavior(report):
    """Current authoring must be verified before permitting its metadata to change."""
    result = behavior(report)
    result.pop("authoring", None)
    return result


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
            dependency = root / "domain.rs"
            dependency.write_text("// domain source\n")
            report = {"puzzles": [{
                "slug": "example", "source": puzzle.read_text(),
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
                     "crates/chialisp-oxide/src/lib.rs", "crates/chialisp-oxide/src/domain.rs",
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--current", type=Path, default=ROOT / "corpus/validation.json")
    parser.add_argument("--baseline", type=Path, help="explicit pre-refactor corpus report")
    parser.add_argument("--comparison", choices=("exact", "semantic", "cargo-migration"), default="exact",
                        help="exact by default; semantic allows Rust codegen changes; cargo-migration allows verified authoring metadata")
    parser.add_argument("--changes", type=Path, help="write comparison scope and codegen metric changes as JSON")
    parser.add_argument("--source-root", type=Path, default=ROOT)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(ComparisonTests)
        result = unittest.TextTestRunner().run(suite)
        raise SystemExit(0 if result.wasSuccessful() else 1)
    if (args.comparison != "exact" or args.changes) and not args.baseline:
        parser.error("non-default comparison and --changes require an explicit --baseline")
    changes = []
    try:
        current = load_report(args.current)
        counts = check_report(current)
        check_sources(current, args.source_root)
        check_recorded(current.get("authoring"), args.source_root, [puzzle["slug"] for puzzle in current["puzzles"]])
        if args.baseline:
            baseline = load_report(args.baseline)
            check_report(baseline)
            projection = {"exact": behavior, "semantic": semantic_behavior,
                          "cargo-migration": cargo_migration_behavior}[args.comparison]
            difference = first_difference(projection(baseline), projection(current))
            require(difference is None, f"{args.comparison.capitalize()} baseline changed: {difference}")
            changes = codegen_changes(baseline, current)
            if args.changes:
                args.changes.parent.mkdir(parents=True, exist_ok=True)
                args.changes.write_text(json.dumps({
                    "schema_version": 1, "comparison": args.comparison,
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
