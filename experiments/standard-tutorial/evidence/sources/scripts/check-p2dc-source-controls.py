#!/usr/bin/env python3
"""Read retained source controls only; never extract, compile or execute programs."""
import argparse
import json
from pathlib import Path
import unittest
import p2dc_source_controls as controls


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('report',type=Path)
    parser.add_argument('--self-test',action='store_true')
    args=parser.parse_args()
    result=controls.validate_recorded(controls.load(args.report),args.report.resolve().parent)
    if args.self_test:
        suite=unittest.defaultTestLoader.discover(str(controls.ROOT/'scripts'),pattern='test_p2dc_source_controls.py')
        controls.require(suite.countTestCases()>0,'missing source control reader tests')
        controls.require(unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful(),'source control tests failed')
    print(json.dumps(result,indent=2))


if __name__=='__main__':main()
