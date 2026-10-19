#!/usr/bin/env python3
"""Check current guarded-projection evidence before pure cost history.

Default and --self-test are recorded consistency only. Explicit fresh bundles,
--execute-costs and --replay have separate scopes and retain their new outputs.
"""
import argparse
import json
from pathlib import Path
import subprocess
import unittest

from cargo_authoring import contained_file, unique_object
import p2_cli
import runtime_cost_migration as cost
import runtime_guarded_projection_migration as migration
from vm_failures import check_execution


def validate(report, payload):
    cost.validate_current(report)
    return _validate_after_current(report, payload)


def _validate_after_current(report, payload):
    migration.check_report_bytes(payload, report)
    previous = migration.project_validated(report)
    historical = migration.append.project_validated(migration.source_cost.project_validated(previous))
    return {"current_report_sha256": cost.sha(payload),
        "source_only_checkpoint_canonical_sha256": cost.sha(cost.canonical(previous)),
        "pre_append_canonical_sha256": cost.sha(cost.canonical(historical)),
        "scope": "Actual current source/native/typed/sidecars checked before pure immutable history; recorded consistency only."}


def _window(report):
    paths = ("check-guarded-projection.py", "runtime_guarded_projection_migration.py",
        "runtime_cost_migration.py", "p2_cli.py", "check-readable-puzzles.py")
    return {"compiler_native": migration.source_cost.control_source_window(report),
        "readers": [{"path": "scripts/" + path, "sha256": cost.sha(contained_file(cost.ROOT, "scripts/" + path).read_bytes())}
            for path in paths]}


def execute_costs(report, runner, directory):
    directory.mkdir(parents=True, exist_ok=False)
    runner = runner.resolve(strict=True)
    binary, window = cost.sha(runner.read_bytes()), _window(report)
    protocol = directory / "protocol.json"
    protocol.write_text(json.dumps(report["puzzles"][-1]["protocol_validation"], indent=2) + "\n")
    output = directory / "costs.json"
    process = subprocess.run([str(runner), str(protocol), str(output)], cwd=cost.ROOT,
        text=True, capture_output=True, timeout=120)
    (directory / "stdout.log").write_text(process.stdout)
    (directory / "stderr.log").write_text(process.stderr)
    cost.require(process.returncode == 0 and not process.stderr, "Conditions helper failed")
    cost.require(binary == cost.sha(runner.read_bytes()) and window == _window(report), "Conditions source/tool window changed")
    cost.require(output.read_bytes() == migration.read(migration.METRICS), "Executed current15 metrics differ from retained capture")
    result = migration.source_cost.validate_metrics(report, json.loads(output.read_bytes(), object_pairs_hook=unique_object))
    return {**result, "runner_sha256": binary,
        "scope": "Executed fifteen pinned conditions/generator calls on separately admitted exact bundles. Signature verification is skipped; no additional SDK admission or preservation credit."}


def replay(report, cli, directory):
    directory.mkdir(parents=True, exist_ok=False)
    cli = cli.resolve(strict=True)
    binary, window = cost.sha(cli.read_bytes()), _window(report)
    before, total = migration.predecessor(), 0
    for old, current in zip(before["puzzles"], report["puzzles"], strict=True):
        location = directory / current["slug"]
        location.mkdir()
        solutions = location / "solutions.json"
        solutions.write_text(json.dumps([r["solution"] for r in current["cases"]]) + "\n")
        for name, puzzle in (("before", old), ("current", current)):
            code = location / (name + ".clvm.hex")
            code.write_text(cost.rust_on(puzzle["artifacts"])["clvm_hex"] + "\n")
            process = subprocess.run([str(cli), "run-batch", str(code), "--solutions", str(solutions),
                "--max-cost", str(puzzle.get("validation_max_cost", 1_000_000))],
                cwd=cost.ROOT, text=True, capture_output=True, timeout=120)
            (location / (name + ".stdout.json")).write_text(process.stdout)
            (location / (name + ".stderr.log")).write_text(process.stderr)
            cost.require(process.returncode == 0 and not process.stderr, "Retained VM replay failed")
            actual = json.loads(process.stdout, object_pairs_hook=unique_object)
            cost.require(type(actual) is list and len(actual) == len(puzzle["cases"]), "Changed replay row count")
            for row, execution in zip(puzzle["cases"], actual, strict=True):
                check_execution(execution)
                cost.require(cost.same(execution, cost.rust_on(row["runs"])["execution"]), "Retained VM result differs")
            total += len(actual)
            cost.require(binary == cost.sha(cli.read_bytes()) and window == _window(report), "Replay source/tool window changed")
    return {"executed_retained_ON_VM_cases": total, "cli_sha256": binary,
        "scope": "Exact retained before/current optimized CLVM, finite named rows only; no recompile, SDK admission, model proof or compiler-preservation credit."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=cost.ROOT / "corpus/validation.json")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--execute-costs", action="store_true")
    parser.add_argument("--cost-runner", type=Path, default=cost.ROOT / "target/debug/examples/p2_singleton_cost")
    parser.add_argument("--execution-output", type=Path)
    parser.add_argument("--replay", action="store_true")
    parser.add_argument("--replay-output", type=Path)
    parser.add_argument("--cli", type=Path, default=cost.ROOT / "target/debug/chialisp-oxide")
    parser.set_defaults(ir_bundle=[])
    p2_cli.arguments(parser)
    args = parser.parse_args()
    payload = migration.report_payload(args.report)
    report = json.loads(payload, object_pairs_hook=unique_object)
    cost.validate_current(report)
    fresh = p2_cli._check_after_current_validation(report, args)
    print("PASS: closed guarded projection evidence", _validate_after_current(report, payload))
    if fresh is not None: print("PASS: actual current22 paired fresh bundles", fresh)
    if args.self_test:
        suite = unittest.defaultTestLoader.discover(str(cost.ROOT / "scripts"), pattern="test_runtime_guarded_projection.py")
        cost.require(suite.countTestCases() > 0, "Missing guarded projection controls")
        if not unittest.TextTestRunner().run(suite).wasSuccessful(): return 1
    cost.require(args.execute_costs == (args.execution_output is not None), "--execute-costs requires new --execution-output")
    cost.require(args.replay == (args.replay_output is not None), "--replay requires new --replay-output")
    if args.execute_costs: print("PASS: actual current15 conditions metrics", execute_costs(report, args.cost_runner, args.execution_output))
    if args.replay: print("PASS: actual retained before/current VM replay", replay(report, args.cli, args.replay_output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
