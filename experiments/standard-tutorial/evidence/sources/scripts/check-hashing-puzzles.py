#!/usr/bin/env python3
"""Validate two pinned hash ports against independent exact-byte expectations.

Default checks recorded finite evidence and its source closure. --replay executes
retained CLVM bytes, including every declared generated input, without compiling
any language. Neither mode is a compiler-preservation or cryptographic proof.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile

from cargo_authoring import unique_object
from hashing_provenance import (ROOT, CANONICAL_HEX, CANONICAL_HASH, read_source, sources)
from hashing_vectors import (CODEC, MODES, RESOURCE_BUDGET, SLUGS, expected, generated_for, named, require,
                             resource_cases, should_exhaust, validation_budget)
from ledger_rejections import check_ledger_report
from vm_failures import PROTOCOL, check_execution, check_native_outcome

MAX_COST = 1_000_000


def artifact_bytes(artifact):
    require(isinstance(artifact, dict) and type(artifact.get("optimize_lir")) is bool
            and type(artifact.get("bytes")) is int and artifact["bytes"] > 0,
            "Malformed actual artifact mode/size")
    text = artifact.get("clvm_hex")
    require(isinstance(text, str) and re.fullmatch(r"(?:[0-9a-f]{2})+", text), "Malformed serialized CLVM hex")
    binary = bytes.fromhex(text)
    require(len(binary) == artifact["bytes"] and len(binary) <= 2_000_000, "Artifact size mismatch/bound")
    tree = CODEC["deserialize"](binary)
    require(CODEC["serialize"](tree) == binary, "Artifact is not canonically serialized")
    require(CODEC["tree_hash"](tree).hex() == artifact.get("tree_hash"), "Artifact tree hash mismatch")
    require(isinstance(artifact.get("disassembly"), str) and artifact["disassembly"], "Missing artifact disassembly")
    return binary


def modes(rows, label):
    require(isinstance(rows, list) and [(row.get("backend"), row.get("optimize_lir")) for row in rows] == MODES
            and all(type(row.get("optimize_lir")) is bool for row in rows), f"{label}: missing, extra or reordered compiler modes")


def identity(puzzle):
    recorded = puzzle.get("module_identity")
    require(isinstance(recorded, dict) and isinstance(recorded.get("comparisons"), list),
            "Missing/malformed canonical launcher identity comparisons")
    for row in recorded["comparisons"]:
        require(isinstance(row, dict) and all(type(row.get(field)) is bool for field in
                ("optimize_lir", "serialized_bytes_equal", "tree_hash_equal")),
                "Identity flags must be exact booleans")
    artifacts = puzzle["artifacts"]
    expected_identity = {"upstream_hex": CANONICAL_HEX, "upstream_tree_hash": CANONICAL_HASH,
        "comparisons": [{"backend": a["backend"], "optimize_lir": a["optimize_lir"],
                         "serialized_bytes_equal": a["clvm_hex"] == CANONICAL_HEX,
                         "tree_hash_equal": a["tree_hash"] == CANONICAL_HASH} for a in artifacts]}
    require(puzzle.get("module_identity") == expected_identity, "False/missing canonical launcher identity comparison")
    original = artifacts[2]
    require(original["clvm_hex"] == CANONICAL_HEX and original["tree_hash"] == CANONICAL_HASH
            and original["bytes"] == 175, "Independent Chialisp must reproduce the published 175-byte module")


def dependencies(puzzle, slug, authoring, reader):
    """Dependency, include and native-closure bindings through the manifest-driven gate."""
    import port_evidence
    port_evidence.check_dependencies(puzzle, sources(reader)[slug], authoring, reader)


def check_cost(execution, budget):
    require(type(budget) is int and 1 <= budget <= 10_000_000, "Invalid fixed comparison budget")
    if execution["outcome"]["status"] == "accepted":
        require(execution["cost"] <= budget, "Recorded successful execution exceeds its declared cost budget")


def resource_evidence(puzzle, *, refined_cost_profile=False):
    require(type(refined_cost_profile) is bool, "Resource profile must be an explicit Boolean")
    if puzzle["slug"] != "sha256tree":
        require("resource_controls" not in puzzle, "Launcher must not gain unexplained resource controls")
        return []
    controls = puzzle.get("resource_controls")
    require(isinstance(controls, dict) and set(controls) == {"max_cost", "cases"}
            and type(controls["max_cost"]) is int and controls["max_cost"] == RESOURCE_BUDGET,
            "Missing/changed low-budget resource-control contract")
    cases = controls["cases"]
    require(isinstance(cases, list) and [{"label": row["label"], "solution": row["solution"]} for row in cases] == resource_cases(),
            "Missing, extra or reordered resource-control inputs")
    for index, case in enumerate(cases):
        desired = expected("sha256tree", case["solution"])
        require(check_native_outcome(case["native"], case["label"]) == desired, "Wrong native resource-control digest")
        modes(case["runs"], "resource control")
        for run in case["runs"]:
            execution = run["execution"]
            outcome = check_execution(execution, case["label"], require_conclusive=False)
            check_cost(execution, RESOURCE_BUDGET)
            improved = refined_cost_profile and run["backend"] == "rust_mir" and run["optimize_lir"] is True and index in (2, 3, 4)
            if should_exhaust(run["backend"], run["optimize_lir"], index) and not improved:
                require(outcome == {"status": "inconclusive"}
                        and execution.get("failure") == {"kind": "resource", "code": "cost_exceeded"},
                        "Expected measured low-budget cost exhaustion, never policy rejection")
            else:
                require(outcome == desired, "Low-budget conclusive control differs from independent digest")
    return cases


def validate(report, reader=read_source):
    """Port-specific contract on top of the manifest-driven generic gate.

    `port_evidence.validate_port` checks claims, sources, pins, builds, module
    identity, dependencies, budget, named vectors and ledger evidence against the
    manifest and the independent hashing relation; this function keeps the raw
    module contract, the published-launcher constants and the low-budget
    resource controls that only these two ports carry.
    """
    from runtime_cost_migration import hash_diagnostic_profile
    import port_evidence
    refined_cost_profile = hash_diagnostic_profile(report)
    manifests = sources(reader)
    port_evidence.check_header(report)
    summaries = []
    for slug in SLUGS:
        matches = [p for p in report["puzzles"] if p["slug"] == slug]
        require(len(matches) == 1, "Missing/duplicate hash port: " + slug)
        puzzle, manifest = matches[0], manifests[slug]
        generic = port_evidence.validate_port(report, slug, expected=lambda solution, slug=slug: expected(slug, solution),
                                              reader=reader, generated=len(generated_for(slug)))
        require(puzzle.get("environment") == "raw" and puzzle.get("reference_mode", "module") == "module"
                and manifest["environment"] == "raw", "Changed/unvalidated raw module contract")
        require(generic["validation_max_cost"] == validation_budget(slug), "Missing/changed recorded hash comparison budget")
        require(generic["module_identity"] == (slug == "singleton-launcher"), "Module identity may only bind the published launcher")
        require("factory_checks" not in puzzle, "Hash helper/launcher is not a program factory")
        if slug == "singleton-launcher":
            identity(puzzle)
        controls = resource_evidence(puzzle, refined_cost_profile=refined_cost_profile)
        summaries.append({"slug": slug, "named": generic["named"], "accepted": generic["accepted"],
                          "rejected": generic["rejected"], "generated": generic["generated"], "builds": len(MODES),
                          "validation_max_cost": validation_budget(slug), "resource_observations": len(controls) * len(MODES)})
    return summaries


def run_batch(cli, path, solutions, count, *, max_cost=MAX_COST, require_conclusive=True):
    require(type(count) is int and 0 < count <= 4096, "Invalid retained replay batch size")
    require(type(max_cost) is int and 1 <= max_cost <= 10_000_000, "Invalid retained replay cost budget")
    result = subprocess.run([str(cli), "run-batch", str(path), "--solutions", str(solutions), "--max-cost", str(max_cost)],
                            capture_output=True, text=True, timeout=60)
    require(result.returncode == 0, "Retained VM batch invocation failed: " + result.stderr[:2000])
    executions = json.loads(result.stdout, object_pairs_hook=unique_object)
    require(isinstance(executions, list) and len(executions) == count, "Missing/extra retained VM batch observations")
    for execution in executions:
        check_execution(execution, "retained hash-port artifact replay", require_conclusive=require_conclusive)
        check_cost(execution, max_cost)
    return executions


def replay(report, cli):
    """Ten ordinary plus five diagnostic batches; no source compilation."""
    from runtime_cost_migration import hash_diagnostic_profile
    validate(report)
    refined_cost_profile = hash_diagnostic_profile(report)
    total = 0
    with tempfile.TemporaryDirectory(prefix="chialisp-oxide-hash-replay-") as temporary:
        for puzzle in report["puzzles"]:
            slug = puzzle["slug"]
            if slug not in SLUGS:
                continue
            cases = puzzle["cases"] + generated_for(slug)
            solutions = Path(temporary) / f"{slug}-solutions.json"
            payload = json.dumps([case["solution"] for case in cases]).encode()
            require(0 < len(cases) <= 4096 and len(payload) <= 8 * 1024 * 1024, "Hash replay input batch exceeds CLI bound")
            solutions.write_bytes(payload)
            outcomes = [expected(slug, case["solution"]) for case in cases]
            for mode_index, artifact in enumerate(puzzle["artifacts"]):
                path = Path(temporary) / f"{slug}-{mode_index}.hex"
                path.write_text(artifact_bytes(artifact).hex() + "\n")
                executions = run_batch(cli, path, solutions, len(cases), max_cost=validation_budget(slug))
                for index, (case, execution, desired) in enumerate(zip(cases, executions, outcomes)):
                    require(execution["outcome"] == desired,
                            "Retained artifact differs from independent hashing relation: " + slug + "/" + case["label"])
                    if index < len(puzzle["cases"]):
                        require(execution == case["runs"][mode_index]["execution"],
                                "Named retained artifact cost/failure/output differs: " + slug + "/" + case["label"])
                    total += 1
                if slug == "sha256tree":
                    controls = resource_evidence(puzzle, refined_cost_profile=refined_cost_profile)
                    control_path = Path(temporary) / "sha256tree-resource-solutions.json"
                    control_path.write_text(json.dumps([case["solution"] for case in controls]))
                    observed = run_batch(cli, path, control_path, len(controls), max_cost=RESOURCE_BUDGET, require_conclusive=False)
                    for case, execution in zip(controls, observed):
                        require(execution == case["runs"][mode_index]["execution"], "Low-budget retained-byte control changed")
                        total += 1
                print(f"PASS: {slug} {artifact['backend']} optimize={artifact['optimize_lir']}: {len(executions)} ordinary retained-byte executions", flush=True)
    return total


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=ROOT / "corpus/validation.json")
    parser.add_argument("--source-only", action="store_true", help="check source/oracle data, with no execution claim")
    parser.add_argument("--self-test", action="store_true", help="run independent relation/provenance/report corruption controls")
    parser.add_argument("--replay", action="store_true", help="execute all named+generated cases against retained actual artifacts")
    parser.add_argument("--cli", type=Path, default=ROOT / "target/debug/chialisp-oxide")
    args = parser.parse_args()
    require(not (args.source_only and args.replay), "Source-only validation cannot claim VM replay")
    report = None if args.source_only else json.loads(args.report.read_text(), object_pairs_hook=unique_object)
    if report is None:
        sources()
        print("PASS: exact pinned sources + finite independent expected data; no recorded execution claim")
    else:
        print("PASS: recorded finite hashing evidence", validate(report))
    if args.self_test:
        from hashing_controls import self_test
        print(f"PASS: {self_test(report)} hashing provenance/relation/evidence corruption controls")
    if args.replay:
        print(f"PASS: {replay(report, args.cli.resolve())} actual retained-artifact VM replays")


if __name__ == "__main__":
    main()
