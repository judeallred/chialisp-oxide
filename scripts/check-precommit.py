#!/usr/bin/env python3
"""Check precommit template, ledger and separate actual-source control evidence.

Retained-byte replay executes only the primary VM. Recorded SDK observations
are consistency checks, not authenticated execution or a security proof.
"""
import argparse
import json
from pathlib import Path
import unittest
import tempfile

import precommit_source_controls as controls
import port_evidence
import precommit_oracle


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=controls.ROOT / "corpus/validation.json")
    parser.add_argument("--source-controls", type=Path, default=controls.ROOT / "corpus/precommit-source-controls/report.json")
    parser.add_argument("--source-controls-only", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--replay", action="store_true")
    parser.add_argument("--cli", type=Path, default=controls.ROOT / "target/debug/chialisp-oxide")
    args = parser.parse_args()
    packet = controls.load(args.source_controls)
    result = controls.validate(packet, args.source_controls.resolve().parent)
    if not args.source_controls_only:
        report = controls.load(args.report)
        port_evidence.check_header(report)
        for slug in ("precommit-claim", "precommit-registry"):
            expected = lambda solution, slug=slug: precommit_oracle.expected(slug, solution)
            port_evidence.validate_port(report, slug, expected=expected)
            if args.replay:
                port_evidence.replay_port(report, slug, args.cli.resolve(), expected=expected)
    if args.replay:
        directory = args.source_controls.resolve().parent
        for mode in ("off", "on"):
            entries = controls.recorded_bundle(directory / "bundles" / mode, packet["context"], mode == "on")
            for spec in controls.witnesses():
                code = directory / "bundles" / mode / entries[spec["name"]]["clvm"]
                with tempfile.TemporaryDirectory(prefix="precommit-replay-") as temporary:
                    solutions = Path(temporary) / "solutions.json"
                    controls.write(solutions, [spec["solution"]])
                    observations = port_evidence.run_batch(args.cli.resolve(), code, solutions, 1, max_cost=10_000_000)
                controls.require(controls.check_execution(observations[0]) == spec["expected"], "retained control artifact replay differs")
        result["retained_primary_vm_replays"] = 8
    if args.self_test:
        suite = unittest.defaultTestLoader.discover(str(controls.ROOT / "scripts"), pattern="test_precommit_source_controls.py")
        controls.require(suite.countTestCases() > 0 and unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful(), "precommit corruption controls failed")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
