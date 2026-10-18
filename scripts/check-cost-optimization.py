#!/usr/bin/env python3
"""Validate the closed measured21 cost transition; optionally replay retained VM.

Default is source/record freshness, not proof that a claimed run occurred.
--fresh-on/--fresh-off checks supplied current22 Cargo output before projection.
--ir-bundle is retained for historical21 reports; no Cargo is invoked.
--replay executes finite retained old/new programs, not native Rust or consensus.
"""
import argparse
import json
from pathlib import Path
import p2_cli

from cargo_authoring import unique_object
from runtime_cost_migration import (ROOT, load, preserve, verify_ir_bundle)


from runtime_cost_checks import measurements as _measurements, replay as _replay, benchmark_check as _benchmark_check


def measurements(report):
    return _measurements(report, project=preserve)


def replay(report, cli):
    return _replay(report, cli, project=preserve)


def benchmark_check(path, report):
    return _benchmark_check(path, report, project=preserve,
        baseline_report_sha=load()["baseline"]["report_sha256"], raw_ir=load()["raw_ir"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=ROOT / "corpus/validation.json")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--replay", action="store_true")
    parser.add_argument("--cli", type=Path, default=ROOT / "target/debug/chialisp-oxide")
    parser.add_argument("--ir-bundle", type=Path, action="append", default=[])
    parser.add_argument("--benchmark", type=Path)
    p2_cli.arguments(parser)
    args = parser.parse_args()
    report_bytes = args.report.read_bytes()
    report = json.loads(report_bytes, object_pairs_hook=unique_object)
    from runtime_cost_migration import check_report_bytes
    check_report_bytes(report_bytes, report)
    fresh = p2_cli.check(report, args)
    if fresh is not None:
        print("PASS: explicit current22 fresh bundles before historical projection", fresh)
    print("PASS: closed cost evidence", measurements(report))
    for path in args.ir_bundle:
        print("PASS: explicit frozen21 rawIR retention", verify_ir_bundle(path.resolve(), report))
    if args.self_test:
        from p2_outer_controls import self_test as append_controls
        count = append_controls(report)
        if count:
            print("PASS:", count, "nonvacuous current22 controls before historical21 projection")
        from cost_controls import self_test
        print("PASS:", self_test(report), "nonvacuous cost-profile/current-source corruption controls")
    if args.replay:
        print("PASS: retained finite executions", replay(report, args.cli.resolve()))
    if args.benchmark:
        print("PASS: external benchmark/profile comparison", benchmark_check(args.benchmark, report))


if __name__ == "__main__":
    main()
