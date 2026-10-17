#!/usr/bin/env python3
"""Check finite password-module, host-curry and ledger evidence.

Default checks recorded source/input freshness. --replay runs retained CLVM,
never a compiler or Cargo. These checks do not establish password security,
cryptographic properties or compiler preservation.
"""
import argparse
import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from cargo_authoring import check_recorded, unique_object
from password_evidence import (artifact_bytes, execution, instances, resource_cases,
                               validate)
from password_provenance import ROOT, source_contract
from password_vectors import (MAX_COST, PUBLIC_HASH, expected, generated_for,
                              native_environment, parse, relation, require)
from runtime_password_migration import current_native_sources, preserve
from runtime_cost_migration import preserve as cost_preservation, verify_ir_bundle

READABLE = runpy.run_path(str(ROOT / "scripts/check-readable-puzzles.py"))


def check(report):
    READABLE["check_report"](report)
    READABLE["check_sources"](report, ROOT, require_rue=True)
    check_recorded(report.get("authoring"), ROOT, [p["slug"] for p in report["puzzles"]])
    current_native_sources(report)
    summary = validate(report)
    preserve(cost_preservation(report))
    return summary


def run_batch(cli, artifact, solutions, budget, *, conclusive=True):
    require(isinstance(solutions, list) and 1 <= len(solutions) <= 4096, "Invalid replay batch count")
    require(type(budget) is int and 1 <= budget <= MAX_COST, "Invalid replay budget")
    with tempfile.TemporaryDirectory(prefix="chialisp-oxide-password-replay-") as directory:
        code, inputs = Path(directory) / "retained.hex", Path(directory) / "solutions.json"
        code.write_text(artifact.hex() + "\n")
        payload = json.dumps(solutions).encode()
        require(len(payload) <= 8 * 1024 * 1024, "Replay input batch exceeds CLI bound")
        inputs.write_bytes(payload)
        result = subprocess.run([str(cli), "run-batch", str(code), "--solutions", str(inputs), "--max-cost", str(budget)],
                                capture_output=True, text=True, timeout=120)
        require(result.returncode == 0, "Retained-byte replay failed: " + result.stderr[:2000])
        observations = json.loads(result.stdout, object_pairs_hook=unique_object)
    require(isinstance(observations, list) and len(observations) == len(solutions), "Missing/extra replay observations")
    # Individual expected outcomes are checked by the caller; even diagnostic
    # batches must retain exact typed resource/program classifications.
    from vm_failures import check_execution
    for observed in observations:
        check_execution(observed, require_conclusive=conclusive)
    return observations


def replay(report, cli):
    check(report)
    puzzle = next(p for p in report["puzzles"] if p["slug"] == "password-coin")
    total = 0
    recipes = puzzle["cases"] + generated_for()
    for index, artifact in enumerate(puzzle["artifacts"]):
        code = artifact_bytes(artifact)
        observed = run_batch(cli, code, [r["solution"] for r in recipes], MAX_COST)
        for position, (row, actual) in enumerate(zip(recipes, observed)):
            execution(actual, expected(row["solution"]))
            if position < 48:
                require(actual == row["runs"][index]["execution"], "Named raw replay changed cost/failure/output")
        total += len(observed)
        diagnostics = puzzle["resource_controls"]["cases"]
        observed = run_batch(cli, code, [r["solution"] for r in diagnostics], 100_000, conclusive=False)
        for row, actual in zip(diagnostics, observed):
            execution(actual, expected(row["solution"]), budget=100_000, conclusive=False)
            require(actual == row["runs"][index]["execution"], "Low-budget retained replay differs")
        total += len(observed)
    curried = puzzle["curried_instances"]
    for index, record in enumerate(curried["artifacts"]):
        observed = run_batch(cli, artifact_bytes(record["artifact"]), [r["solution"] for r in curried["cases"]], MAX_COST)
        for row, actual in zip(curried["cases"], observed):
            execution(actual, relation(native_environment(PUBLIC_HASH, parse(row["solution"]))))
            require(actual == row["runs"][index]["execution"], "Fixed-H instance replay changed cost/failure/output")
        total += len(observed)
    for control in curried["varying_arguments"]:
        for run in control["runs"]:
            actual = run_batch(cli, artifact_bytes(run["artifact"]), [control["solution"]], MAX_COST)[0]
            execution(actual, control["native"])
            require(actual == run["execution"], "Varying-H retained instance replay changed")
            total += 1
    # These replay the primary0.21 VM observations only. The separately recorded
    #0.16 simulator/consensus attempts are not rerun by this command.
    for comparison in curried["ledger"]:
        groups = {}
        for attempt in comparison["attempts"]:
            groups.setdefault(attempt["reveal_hex"], []).append(attempt)
        for reveal, attempts in groups.items():
            observed = run_batch(cli, bytes.fromhex(reveal), [r["solution"] for r in attempts], MAX_COST)
            for row, actual in zip(attempts, observed):
                require(actual == row["vm_021"], "Ledger primary-VM retained replay changed")
            total += len(observed)
    return total


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=ROOT / "corpus/validation.json")
    parser.add_argument("--source-only", action="store_true", help="check pinned sources/input recipes without an execution claim")
    parser.add_argument("--self-test", action="store_true", help="run independent oracle/provenance/evidence corruption controls")
    parser.add_argument("--replay", action="store_true", help="execute exact retained VM bytes, including generated and diagnostic inputs")
    parser.add_argument("--cli", type=Path, default=ROOT / "target/debug/chialisp-oxide")
    parser.add_argument("--ir-bundle", type=Path, action="append", default=[], help="compare old20 exact IR/artifacts in an externally freshly extracted21-export manifest")
    parser.add_argument("--extract-ir", action="store_true", help="sequentially extract/check fresh on/off Cargo bundles using prebuilt tools; run only after Rust builds/tests finish")
    args = parser.parse_args()
    require(not (args.source_only and (args.replay or args.ir_bundle or args.extract_ir)), "Source-only checks cannot claim execution/extraction")
    report = None
    if args.source_only:
        source_contract()
        print("PASS: pinned source/span/license and independent raw/fixed-H input recipes; no execution claim")
    else:
        report = json.loads(args.report.read_bytes(), object_pairs_hook=unique_object)
        print("PASS: recorded finite password evidence", check(report))
        for bundle in args.ir_bundle:
            print("PASS: exact committed20 rawIR/artifact comparisons", verify_ir_bundle(bundle, report))
    if args.self_test:
        from password_controls import self_test
        previous = cost_preservation(report) if report is not None else None
        print("PASS:", self_test(previous), "password oracle/provenance/evidence corruption controls")
    if args.replay:
        print("PASS:", replay(report, args.cli.resolve()), "actual retained primary-VM executions; no consensus resubmission")
    if args.extract_ir:
        from password_cargo import extract
        print("PASS: fresh Cargo identity/old20 rawIR retention checks", extract(report))


if __name__ == "__main__":
    main()
