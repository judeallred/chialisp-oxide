#!/usr/bin/env python3
"""Offline v2 packet admission; v1 reader/packet remain unchanged."""
import argparse
import json
from pathlib import Path
import xchandles_history_v2 as history


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    check = sub.add_parser('check')
    check.add_argument('packet', type=Path)
    wrap = sub.add_parser('wrap')
    for name in ('primary', 'blocks', 'cost-audit', 'cost-license', 'output'):
        wrap.add_argument('--'+name, type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == 'check':
            report = history.verify(args.packet)
        else:
            report = history.build_packet(args.primary, args.blocks, args.cost_audit,
                                          args.cost_license, args.output)
        print(json.dumps(report, indent=2))
    except (ValueError, OSError, KeyError, TypeError, UnicodeError) as error:
        parser.exit(2, f'History v2 packet not admitted: {error}\n')


if __name__ == '__main__':
    main()
