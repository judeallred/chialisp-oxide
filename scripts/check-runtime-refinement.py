#!/usr/bin/env python3
"""Validate the closed signed c879 runtime-refinement transition.

Default checks current source/record freshness, not authenticated execution.
Explicit --ir-bundle reads caller-supplied fresh Cargo output without extracting.
Explicit --replay executes retained finite old/new VM artifacts, without native
Rust or consensus resubmission. Resource failures never earn rejection credit.
"""
import argparse
import json
from pathlib import Path

from cargo_authoring import unique_object
from runtime_refinement_migration import ROOT, COMMIT, REPORT_SHA, is_refinement, load, preserve, require, sha
from runtime_cost_migration import verify_ir_bundle
from runtime_cost_checks import measurements as _measurements, replay as _replay, benchmark_check as _benchmark_check
from runtime_inline_migration import predecessor


def measurements(report):
    return _measurements(report, project=preserve)


def replay(report, cli):
    return _replay(report, cli, project=preserve)


def benchmark_check(path, report):
    result = _benchmark_check(path, report, project=preserve,
        baseline_report_sha=REPORT_SHA, raw_ir=predecessor()["raw_ir"])
    from runtime_refinement_checks import benchmark_extra
    measured = json.loads(path.read_bytes(), object_pairs_hook=unique_object)
    require(type(measured["schema_version"]) is int and measured["schema_version"] == 1
            and measured["baseline_commit"] == COMMIT
            and measured["execution_vm"] == report["execution_vm"]
            and measured["vm_failure_protocol"] == report["vm_failure_protocol"],
            "Wrong external runtime-refinement benchmark identity/protocol")
    require(measured["input_files"] == measured["input_files_after"]
            and bool(measured["input_files"]), "External benchmark inputs changed during its recorded window")
    claims = measured["claims"]
    require(claims["all21_off_artifacts_byte_exact"] is True
            and all(claims[key] is False for key in ("raw_ir_reextraction", "universal_equivalence_proof",
                "universal_cost_improvement", "consensus_resubmitted", "generated_cases_executed", "native_executed")),
            "External benchmark scope or OFF preservation credit changed")
    return {**result, **benchmark_extra(measured, preserve(report), report)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=ROOT / "corpus/validation.json")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--replay", action="store_true")
    parser.add_argument("--cli", type=Path, default=ROOT / "target/debug/chialisp-oxide")
    parser.add_argument("--ir-bundle", type=Path, action="append", default=[])
    parser.add_argument("--benchmark", type=Path)
    args = parser.parse_args()
    payload = args.report.read_bytes()
    report = json.loads(payload, object_pairs_hook=unique_object)
    if is_refinement(report):
        require(sha(payload) == load()["reviewed_after"]["report_sha256"],
                "Current report bytes differ from final reviewed runtime-refinement profile")
    print("PASS: closed runtime-refinement evidence", measurements(report))
    for path in args.ir_bundle:
        print("PASS: explicit frozen21 rawIR retention", verify_ir_bundle(path.resolve(), report))
    if args.self_test:
        from refinement_controls import self_test
        print("PASS:", self_test(report), "nonvacuous runtime-refinement/current-source controls")
    if args.replay:
        print("PASS: retained finite executions", replay(report, args.cli.resolve()))
    if args.benchmark:
        print("PASS: external benchmark/profile comparison", benchmark_check(args.benchmark, report))


if __name__ == "__main__":
    main()
