#!/usr/bin/env python3
"""Check the finite original boomerang policy and five-build instance records.

--replay executes retained primary-VM bytes only, using the prebuilt CLI. It
never rebuilds a host, compiler, driver or puzzle. Recorded ledger checks do not
authenticate execution, verify BLS, prove state equality or establish safety.
"""
import argparse
import json
import os
from pathlib import Path
import stat
import unittest

import boomerang_evidence as evidence


def read_report(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
    try:
        metadata = os.fstat(descriptor)
        evidence.require(stat.S_ISREG(metadata.st_mode) and 0 < metadata.st_size <= 16 * 1024 * 1024,
                         "Boomerang report must be a bounded regular file")
        with os.fdopen(descriptor, "rb", closefd=False) as stream:
            payload = stream.read(16 * 1024 * 1024 + 1)
        evidence.require(len(payload) <= 16 * 1024 * 1024, "Boomerang report byte bound")
        return evidence.load(payload)
    finally:
        os.close(descriptor)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=evidence.ROOT / "corpus/validation.json")
    parser.add_argument("--sources-only", action="store_true", help="input recipes/oracle consistency only; no execution credit")
    parser.add_argument("--self-test", action="store_true", help="constructed oracle/reader corruption controls")
    parser.add_argument("--replay", action="store_true", help="execute retained template/instance artifacts in the primary VM")
    parser.add_argument("--cli", type=Path, default=evidence.ROOT / "target/debug/chialisp-oxide")
    parser.add_argument("--source-controls", type=Path, help="also check a separately retained actual Rust source-mutant packet")
    parser.add_argument("--source-controls-only", action="store_true", help="check only the separate recorded source-mutant packet; no execution")
    args = parser.parse_args()
    evidence.require(not (args.sources_only and (args.replay or args.source_controls)), "Source-only mode cannot claim retained execution/control evidence")
    evidence.require(not args.source_controls_only or (args.source_controls and not args.replay and not args.sources_only),
                     "Source-controls-only requires its packet and cannot replay")
    if args.self_test:
        suite = unittest.defaultTestLoader.discover(str(evidence.ROOT / "scripts"), pattern="test_boomerang.py")
        evidence.require(suite.countTestCases() > 0, "Missing boomerang reader controls")
        evidence.require(unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful(), "Boomerang reader controls failed")
    if args.source_controls_only:
        packet = read_report(args.source_controls)
        result = evidence.validate_source_controls(packet, args.source_controls.resolve().parent)
    elif args.sources_only:
        evidence.source_contract()
        result = {"slug": evidence.SLUG, "status": "source_recipe_oracle_consistency_only",
                  "template_cases": 101, "instance_cases": 14,
                  "scope": "No compiler/native/VM/SDK/source-mutation execution or security proof is credited."}
    else:
        report = read_report(args.report)
        from port_evidence import check_header, puzzle_row
        check_header(report)
        puzzle = puzzle_row(report, evidence.SLUG)
        result = evidence.validate_coin(puzzle)
        if args.replay:
            result.update(evidence.replay_retained(puzzle, args.cli.resolve(strict=True)))
        if args.source_controls:
            packet = read_report(args.source_controls)
            result["source_controls"] = evidence.validate_source_controls(packet, args.source_controls.resolve().parent)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
