#!/usr/bin/env python3
"""Offline history packet admission or explicit-input wrapper construction."""
import argparse
import json
from pathlib import Path
import xchandles_history as history


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    check = sub.add_parser('check')
    check.add_argument('packet', type=Path)
    build = sub.add_parser('wrap')
    build.add_argument('--research', type=Path, required=True)
    build.add_argument('--csv', type=Path, required=True)
    build.add_argument('--license', type=Path, required=True)
    build.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == 'check':
            report = history.verify(args.packet)
        else:
            report = history.build_packet(args.research, args.csv, args.license, args.output)
        print(json.dumps(report, indent=2))
    except (ValueError, OSError, KeyError, TypeError, UnicodeError) as error:
        parser.exit(2, f'History packet not admitted: {error}\n')


if __name__ == '__main__':
    main()
