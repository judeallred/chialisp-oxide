#!/usr/bin/env python3
"""Current23 recorded/fresh checks before pure reviewed22 history.

No compiler/native/VM/SDK/solver execution by default. Explicit fresh bundle
checks query installed rustc and hash the supplied driver, without extraction.
The pending profile cannot authorize a report or restored historical source.
"""
import argparse
import json
from pathlib import Path
import unittest

from cargo_authoring import unique_object
import p2_cli
import p2dc_append_migration as migration
import runtime_cost_migration as cost


def validate(report,payload,*,fresh_args=None):
    cost.validate_current(report)
    migration.check_report_bytes(payload,report)
    fresh=p2_cli._check_after_current_validation(report,fresh_args) if fresh_args is not None else None
    before=migration.project_validated(report)
    intermediate=cost.source_checkpoint(before)
    anchor=migration.composition.anchor_checkpoint(intermediate)
    result=dict(current23_report_sha256=cost.sha(payload),unsigned22_report_sha256=migration.BEFORE_RAW,
        unsigned22_canonical_sha256=cost.sha(cost.canonical(before)),
        unsigned_source_checkpoint_sha256=cost.sha(cost.canonical(intermediate)),
        signed_anchor_commit=migration.composition.ANCHOR['commit'],signed_anchor_canonical_sha256=cost.sha(cost.canonical(anchor)),
        scope='Current23 full source/native/typed components/current23 oldP2 sidecars and requested fresh checks precede pure unsigned22→composition→source-only→signedE20 history. No authenticated execution, BLS/state/preservation proof or production-readiness credit.')
    if fresh is not None:result['fresh']=fresh
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report',type=Path,default=cost.ROOT/'corpus/validation.json')
    parser.add_argument('--self-test',action='store_true')
    p2_cli.arguments(parser);args=parser.parse_args();args.ir_bundle=None
    cost.require(migration.load()['status']==migration.FINAL,'P2dc append pending actual23 capture/review')
    payload=migration.report_payload(args.report);report=json.loads(payload,object_pairs_hook=unique_object)
    result=validate(report,payload,fresh_args=args)
    if args.self_test:
        suite=unittest.defaultTestLoader.discover(str(cost.ROOT/'scripts'),pattern='test_p2dc_append_migration.py')
        cost.require(suite.countTestCases()>0,'missing P2dc append controls')
        cost.require(unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful(),'P2dc append controls failed')
    print(json.dumps(result,indent=2))


if __name__=='__main__':main()
