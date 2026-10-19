#!/usr/bin/env python3
"""Current corpus recorded/fresh checks before PURE retained-checkpoint history.

No compiler/native/VM/SDK/solver execution by default. Explicit fresh bundle
checks query installed rustc and hash the supplied driver, without extraction.
``--record`` rebuilds the reviewed capture from the actual current report and
paired fresh bundles, writes the profile and prints the pin the reviewer must
place in ``scripts/corpus_append.py``. A pending profile cannot authorize a
report or restored historical source.
"""
import argparse
import json
from pathlib import Path
import subprocess
import tomllib
import unittest

from cargo_authoring import unique_object
import corpus_append as migration
import p2_cli
import runtime_cost_migration as cost


def validate(report, payload, *, fresh_args=None):
    cost.validate_current(report)
    migration.check_report_bytes(payload, report)
    fresh = p2_cli._check_after_current_validation(report, fresh_args) if fresh_args is not None else None
    before = migration.project_validated(report)
    retained22 = migration.outer.project_validated(before)
    intermediate = cost.source_checkpoint(retained22)
    anchor = migration.composition.anchor_checkpoint(intermediate)
    result = dict(current_report_sha256=cost.sha(payload), current_exports=len(migration.current_slugs()),
                  added_slugs=migration.added_slugs(migration.load()),
                  retained_checkpoint_sha256=migration.load()['baseline']['report_sha256'],
                  retained_canonical_sha256=cost.sha(cost.canonical(before)),
                  unsigned22_canonical_sha256=cost.sha(cost.canonical(retained22)),
                  unsigned_source_checkpoint_sha256=cost.sha(cost.canonical(intermediate)),
                  signed_anchor_commit=migration.composition.ANCHOR['commit'],
                  signed_anchor_canonical_sha256=cost.sha(cost.canonical(anchor)),
                  scope='Current complete source/native/typed/added-row checks and requested fresh checks precede pure retained→unsigned22→composition→source-only→signedE20 history. No authenticated execution, BLS/state/preservation proof or production-readiness credit.')
    if fresh is not None:
        result['fresh'] = fresh
    return result


def record(args, report, payload):
    cost.require(args.fresh_on is not None and args.fresh_off is not None, '--record requires paired --fresh-on/--fresh-off bundles')
    toolchain = tomllib.loads((cost.ROOT / 'rust-toolchain.toml').read_text())['toolchain']['channel']
    compiler = subprocess.run(['rustup', 'run', toolchain, 'rustc', '-vV'], cwd=cost.ROOT, text=True, capture_output=True, timeout=30)
    cost.require(compiler.returncode == 0 and not compiler.stderr, 'Cannot independently query pinned compiler')
    driver = args.driver.resolve(strict=True)
    profile = migration.record(report, payload, args.fresh_on.resolve(), args.fresh_off.resolve(),
                               compiler=compiler.stdout, driver_sha256=cost.sha(driver.read_bytes()), toolchain=toolchain)
    encoded = (json.dumps(profile, indent=2) + '\n').encode()
    (cost.ROOT / migration.PATH).write_bytes(encoded)
    print(json.dumps(dict(status=profile['status'], written=migration.PATH, new_pin=cost.sha(encoded),
                          note='Review the profile diff, then set PIN in scripts/corpus_append.py to new_pin and rerun without --record.'), indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, default=cost.ROOT / 'corpus/validation.json')
    parser.add_argument('--self-test', action='store_true')
    parser.add_argument('--record', action='store_true', help='rebuild the reviewed capture from the actual report and fresh bundles')
    p2_cli.arguments(parser)
    args = parser.parse_args()
    args.ir_bundle = None
    payload = migration.report_payload(args.report)
    report = json.loads(payload, object_pairs_hook=unique_object)
    if args.record:
        record(args, report, payload)
        return
    cost.require(migration.load()['status'] == migration.FINAL, 'corpus append pending capture/review')
    result = validate(report, payload, fresh_args=args)
    if args.self_test:
        suite = unittest.defaultTestLoader.discover(str(cost.ROOT / 'scripts'), pattern='test_corpus_append.py')
        cost.require(suite.countTestCases() > 0, 'missing corpus append controls')
        cost.require(unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful(), 'corpus append controls failed')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
