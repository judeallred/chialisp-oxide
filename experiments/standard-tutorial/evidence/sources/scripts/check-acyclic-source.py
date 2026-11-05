#!/usr/bin/env python3
"""Explicit actual-source acyclic scalar bridge; never a default proof-API widening."""
import argparse
import json
from pathlib import Path
import shutil
import sys
import unittest

from acyclic_source_bridge import ROOT, produce, recorded_controls, require, validate_recorded, write


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--acyclic-scalar", action="store_true", help="Explicitly opt into the bounded post-ABI scalar model")
    parser.add_argument("--execute", action="store_true", help="Build private experiment, extract both modes, execute model/native/VM checks")
    parser.add_argument("--check-recorded", type=Path, help="Read-only metadata freshness/consistency; no execution credit")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--tools", type=Path, default=ROOT / "target/debug")
    parser.add_argument("--target-dir", type=Path)
    parser.add_argument("--z3", type=Path, default=Path(shutil.which("z3") or "/missing/z3"))
    parser.add_argument("--self-test", action="store_true", help="Constructed metadata and reader controls, not source execution")
    args = parser.parse_args()
    require(not (args.execute and args.check_recorded), "Choose execute or recorded-only check")
    if args.execute or args.check_recorded:
        require(args.acyclic_scalar, "--acyclic-scalar is required; default models remain unchanged")
    if args.execute:
        require(args.output is not None and args.target_dir is not None, "Execution needs new --output and private --target-dir")
        result = produce(ROOT, args.output.resolve(), args.tools.resolve(), args.target_dir.resolve(), args.z3.resolve(strict=True))
        validate_recorded(result, root=ROOT, output=args.output.resolve())
        controls = recorded_controls(result, root=ROOT, output=args.output.resolve())
        write(args.output.resolve() / "recorded-controls.json", controls)
        print(json.dumps({"status": "validated", "scope": result["scope"], "counts": result["counts"],
                          "report": str(args.output.resolve() / "result.json")}, indent=2))
    elif args.check_recorded:
        path = args.check_recorded.resolve(strict=True)
        from acyclic_source_bridge import load
        print(json.dumps(validate_recorded(load(path.parent, path.name), root=ROOT, output=path.parent), indent=2))
    require(args.execute or args.check_recorded or args.self_test, "Choose an explicit action")
    if args.self_test:
        suite = unittest.defaultTestLoader.discover(str(ROOT / "scripts"), pattern="test_acyclic_source.py")
        require(suite.countTestCases() > 0, "No acyclic source controls found")
        if not unittest.TextTestRunner(stream=sys.stderr).run(suite).wasSuccessful():
            return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
