#!/usr/bin/env python3
"""Read current23 component evidence; source-only mode adds no execution credit.

This is not the future aggregate22→23 append migration or a producer. The normal
corpus CLI must freshly execute and record native/VM/SDK observations first.
No compiler, Cargo, VM, SDK or solver process is launched here.
"""
import argparse
import json
import os
from pathlib import Path
import stat
import unittest

from p2_delegated_conditions_vectors import ROOT, source_inputs
from p2_delegated_conditions_evidence import validate_current


def read_report(path):
    # Nonblocking open followed by fstat prevents a mistaken FIFO from hanging.
    # A frozen regular input is still assumed; this is not a mutation sandbox.
    descriptor = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
    try:
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise ValueError('report must be a regular file')
        with os.fdopen(descriptor, 'rb', closefd=False) as report:
            raw = report.read(16 * 1024 * 1024 + 1)
        if len(raw) > 16 * 1024 * 1024:
            raise ValueError('current report exceeds16MiB bound')
        from p2_delegated_conditions_vectors import load
        return load(raw)
    finally:
        os.close(descriptor)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, default=ROOT / 'corpus/validation.json')
    parser.add_argument('--sources-only', action='store_true', help='check only pinned sources/spec/recipes; no executed port credit')
    parser.add_argument('--self-test', action='store_true', help='run constructed reader/spec controls; never new execution credit')
    args = parser.parse_args()
    if args.self_test:
        suite = unittest.defaultTestLoader.discover(str(ROOT / 'scripts'), pattern='test_p2_delegated_conditions.py')
        if not unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful():
            raise SystemExit(1)
    if args.sources_only:
        manifest, rows, mutations = source_inputs()
        result = {'status': 'source_spec_consistency_only', 'component': manifest['slug'],
                  'independent_vectors': len(rows), 'named': 20, 'disjoint_generated': 180,
                  'source_mutation_recipes': len(mutations['mutations']),
                  'scope': 'No new Rust/Cargo, VM, SDK, source mutation or compiler-preservation execution is credited.'}
    else:
        result = validate_current(read_report(args.report))
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
