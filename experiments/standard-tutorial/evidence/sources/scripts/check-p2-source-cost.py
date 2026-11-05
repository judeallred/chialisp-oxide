#!/usr/bin/env python3
"""Check the separately reviewed source-only P2 size/conditions-cost transition.

Default checks actual current source/native/typed records before pure historical
projection. --checkpoint-only reads the unsigned reviewed source-only checkpoint
and its link to the actual signed anchor. Explicit cost execution measures the
current P2 bundles, separately from that historical source-only comparison.
"""
import argparse
import json
from pathlib import Path
import subprocess
import unittest

from cargo_authoring import unique_object
import p2_cli
import p2_source_cost_migration as migration
import runtime_cost_migration as cost
import runtime_guarded_projection_migration as guarded
import runtime_composition_migration as composition


def validate(report, payload):
    cost.validate_current(report)  # Actual current source/native/typed FIRST.
    return _validate_after_current(report, payload)


def _validate_after_current(report, payload):
    """Internal: the caller already checked the actual current report."""
    cost.check_report_bytes(payload, report)
    if guarded.is_transition(report):
        checkpoint = cost.source_checkpoint(report)
        cost.require(cost.same(checkpoint, composition.checkpoint_recorded()), "Changed pure source-only checkpoint")
        captures = {"scope": "Current evidence checked first; pinned historical source-only sidecars only, no current compiler/source freshness claim for history."}
    else:
        checkpoint = report
        captures = migration.check_sidecars(report)
    previous = migration.project_validated(checkpoint)
    # Both restoration calls are pure. No current validator sees old93/old21.
    historical = migration.append.project_validated(previous)
    return {"source_cost_baseline_canonical_sha256":cost.sha(cost.canonical(previous)),
        "pre_append_canonical_sha256":cost.sha(cost.canonical(historical)),**captures}


def execute_costs(report, runner, directory):
    directory.mkdir(parents=True,exist_ok=False)
    runner = runner.resolve(strict=True)
    before = cost.sha(runner.read_bytes())
    selected = cost.current22_migration(report)
    matches = [p for p in report["puzzles"] if p["slug"] == "p2-singleton"]
    cost.require(len(matches) == 1, "Expected exactly one current P2 component")
    # Only the pure arithmetic/source-window reader sees this view; full
    # current validation and requested fresh checks already ran on the caller.
    view = {**report, "puzzles": matches}
    active_reader = Path(selected.__file__).resolve().relative_to(cost.ROOT.resolve()).as_posix()
    cost.require(active_reader.startswith("scripts/") and active_reader.endswith(".py"),
        "Current cost reader must be a repository Python module")
    reader_paths = {active_reader, *("scripts/" + name for name in
        ("check-p2-source-cost.py", "p2_source_cost_migration.py", "p2_cli.py",
            "runtime_cost_migration.py", "runtime_composition_migration.py"))}

    def source_window():
        return {
            "compiler_native": migration.control_source_window(view),
            "readers": [{"path": path, "sha256": cost.sha((cost.ROOT / path).read_bytes())}
                for path in sorted(reader_paths)],
        }

    window = source_window()
    protocol = directory/"protocol.json"
    protocol.write_text(json.dumps(matches[0]["protocol_validation"],indent=2)+"\n")
    output = directory/"costs.json"
    process = subprocess.run([str(runner),str(protocol),str(output)],cwd=cost.ROOT,
        text=True,capture_output=True,timeout=120)
    (directory/"stdout.log").write_text(process.stdout)
    (directory/"stderr.log").write_text(process.stderr)
    cost.require(process.returncode == 0 and not process.stderr,"Conditions metric runner failed")
    cost.require(cost.sha(runner.read_bytes()) == before and window == source_window(),
        "Conditions source/tool window changed during execution")
    cost.require(output.read_bytes() == selected.read(selected.METRICS),
        "Executed conditions metrics differ from the retained current capture")
    result = migration.validate_metrics(view,json.loads(output.read_bytes(),object_pairs_hook=unique_object))
    return {**result,"runner_sha256":before,"scope":"Executed pinned conditions/generator helper for fifteen exact separately admitted bundles; no additional signature verification or SDK admission."}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report",type=Path)
    parser.add_argument("--self-test",action="store_true")
    parser.add_argument("--checkpoint-only",action="store_true",
        help="recorded unsigned reviewed source-only checkpoint and signed anchor; no current source or execution checks")
    parser.add_argument("--execute-costs",action="store_true")
    parser.add_argument("--cost-runner",type=Path,default=cost.ROOT/"target/debug/examples/p2_singleton_cost")
    parser.add_argument("--execution-output",type=Path)
    parser.set_defaults(ir_bundle=[])
    p2_cli.arguments(parser)
    args=parser.parse_args()
    if args.checkpoint_only:
        cost.require(args.fresh_on is None and args.fresh_off is None
            and args.fresh_cargo_options is None and not args.execute_costs
            and args.execution_output is None and args.report is None,
            "--checkpoint-only accepts no report/fresh/execution overrides")
        report = composition.checkpoint_recorded()
        fresh = None
        previous = migration.project_validated(report)
        print("PASS: closed P2 source-cost evidence", {"source_only_checkpoint_sha256": guarded.BEFORE_RAW,
            "source_cost_baseline_canonical_sha256": cost.sha(cost.canonical(previous)),
            "scope": "Immutable unsigned reviewed checkpoint and actual signed anchor only; no current source or new execution credit."})
    else:
        payload=guarded.report_payload(args.report or cost.ROOT/"corpus/validation.json")
        report=json.loads(payload,object_pairs_hook=unique_object)
        cost.validate_current(report)
        fresh=p2_cli._check_after_current_validation(report,args)
        print("PASS: closed P2 source-cost evidence",_validate_after_current(report,payload))
    if fresh is not None:print("PASS: actual current paired fresh bundles",fresh)
    if args.self_test:
        suite=unittest.defaultTestLoader.discover(str(cost.ROOT/"scripts"),pattern="test_p2_source_cost.py")
        cost.require(suite.countTestCases()>0,"Missing P2 source-cost corruption controls")
        if not unittest.TextTestRunner().run(suite).wasSuccessful():return 1
    if args.execute_costs:
        cost.require(args.execution_output is not None,"--execute-costs requires a new --execution-output directory to retain exact inputs/results")
        print("PASS: actual accepted-bundle conditions metrics",execute_costs(report,args.cost_runner,args.execution_output))
    else:cost.require(args.execution_output is None,"--execution-output requires --execute-costs")
    return 0


if __name__=="__main__":
    raise SystemExit(main())
