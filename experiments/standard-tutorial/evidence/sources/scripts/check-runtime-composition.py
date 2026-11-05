#!/usr/bin/env python3
"""Recorded current composition first; exact unsigned/signed history afterwards.

No compiler/VM/SDK/solver execution by default. Explicit paired fresh bundles
query the installed compiler and driver through the existing checked interface.
"""
import argparse
import json
from pathlib import Path
import unittest

from cargo_authoring import unique_object
import p2_cli
import runtime_cost_migration as cost
import runtime_composition_migration as migration


def validate(report, payload, *, fresh_args=None):
    cost.validate_current(report)
    cost.check_report_bytes(payload, report)
    fresh = p2_cli._check_after_current_validation(report, fresh_args) if fresh_args is not None else None
    checkpoint = cost.composition_checkpoint(report)
    intermediate = migration.project_validated(checkpoint)
    anchored = migration.source.project_validated(intermediate)
    old = migration.append.project_validated(anchored)
    result = {"current_report_sha256": cost.sha(payload),
        "unsigned_intermediate_sha256": migration.INTERMEDIATE["report_sha256"],
        "signed_anchor_commit": migration.ANCHOR["commit"],
        "signed_anchor_canonical_sha256": cost.sha(cost.canonical(anchored)),
        "pre_append_canonical_sha256": cost.sha(cost.canonical(old)),
        "scope": "Recorded FULL active-current source/native/typed/sidecar/raw/requested-fresh consistency before exact unsigned22 and unsigned M1 and signed E20 history. No authenticated execution or compiler-preservation claim."}
    if fresh is not None:
        result["fresh"] = fresh
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--self-test", action="store_true")
    p2_cli.arguments(parser)
    args = parser.parse_args()
    # Pending authorization fails before any attempt to treat older on-disk
    # corpus data as current source. No option can bypass this fence.
    cost.require(migration.load()["status"] == migration.FINAL,
        "Runtime composition pending actual capture/review")
    payload = migration.report_payload(args.report or cost.ROOT / "corpus/validation.json")
    report = json.loads(payload, object_pairs_hook=unique_object)
    args.ir_bundle = None
    result = validate(report, payload, fresh_args=args)
    if args.self_test:
        suite = unittest.defaultTestLoader.discover(str(cost.ROOT / "scripts"), pattern="test_runtime_composition.py")
        cost.require(suite.countTestCases() > 0, "Missing runtime composition controls")
        cost.require(unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful(), "Runtime composition controls failed")
    print("PASS: closed runtime composition evidence", result)


if __name__ == "__main__":
    main()
