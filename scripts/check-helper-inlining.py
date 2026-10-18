#!/usr/bin/env python3
"""Validate the closed signed405 helper-inlining transition; optionally replay retained VM.

Default is source/record freshness, not proof that a claimed run occurred.
--ir-bundle checks explicitly supplied fresh Cargo output; no Cargo is invoked.
--replay executes finite retained old/new programs, not native Rust or consensus.
"""
import argparse
import json
from pathlib import Path

from cargo_authoring import unique_object
from runtime_inline_migration import (ROOT, is_inline as is_new, load, preserve, require,
                                      rust_on, same, sha, predecessor)
from runtime_cost_migration import verify_ir_bundle


from runtime_cost_checks import measurements as _measurements, replay as _replay, benchmark_check as _benchmark_check


def measurements(report):
    return _measurements(report, project=preserve)


def replay(report, cli):
    return _replay(report, cli, project=preserve)


def benchmark_check(path, report):
    return _benchmark_check(path, report, project=preserve,
        baseline_report_sha=load()["baseline"]["report_sha256"], raw_ir=predecessor()["raw_ir"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=ROOT / "corpus/validation.json")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--replay", action="store_true")
    parser.add_argument("--cli", type=Path, default=ROOT / "target/debug/chialisp-oxide")
    parser.add_argument("--ir-bundle", type=Path, action="append", default=[])
    parser.add_argument("--benchmark", type=Path)
    args = parser.parse_args()
    report_bytes = args.report.read_bytes()
    report = json.loads(report_bytes, object_pairs_hook=unique_object)
    from runtime_refinement_migration import is_refinement, load as refinement_profile
    if is_refinement(report) or is_new(report):
        profile = refinement_profile() if is_refinement(report) else load()
        require(sha(report_bytes) == profile["reviewed_after"]["report_sha256"], "Current report bytes differ from final reviewed optimization profile")
    print("PASS: closed helper-inlining evidence", measurements(report))
    for path in args.ir_bundle:
        print("PASS: explicit frozen21 rawIR retention", verify_ir_bundle(path.resolve(), report))
    if args.self_test:
        from inline_controls import self_test
        print("PASS:", self_test(report), "nonvacuous helper-inlining/current-source corruption controls")
    if args.replay:
        print("PASS: retained finite executions", replay(report, args.cli.resolve()))
    if args.benchmark:
        print("PASS: external benchmark/profile comparison", benchmark_check(args.benchmark, report))


if __name__ == "__main__":
    main()
