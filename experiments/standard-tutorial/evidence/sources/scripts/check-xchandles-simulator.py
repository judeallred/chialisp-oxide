#!/usr/bin/env python3
"""Offline recorded-native pilot checks. No Cargo/network/VM by default or install."""
import argparse
import json
from pathlib import Path
import xchandles_simulator_evidence as evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--packet', type=Path, default=Path(__file__).resolve().parents[1] / 'corpus/xchandles-simulator')
    parser.add_argument('--list-json', action='store_true', help='fully admit then emit portable descriptors and summary')
    parser.add_argument('--install', action='store_true', help='copy explicitly supplied frozen inputs into new packet; no execution')
    for name in ('source', 'capture', 'baseline-lock', 'driver-archive', 'settings', 'old-checkpoint'):
        parser.add_argument('--' + name, type=Path)
    args = parser.parse_args()
    if args.install:
        required = [args.source, args.capture, args.baseline_lock, args.driver_archive, args.settings, args.old_checkpoint]
        parser.error('--install requires all six explicit input paths') if any(p is None for p in required) else None
        result = evidence.install(args.packet, *required)
    else:
        if any(getattr(args, n.replace('-', '_')) is not None for n in ('source', 'capture', 'baseline-lock', 'driver-archive', 'settings', 'old-checkpoint')):
            parser.error('wrapper paths require --install')
        result = evidence.validate(args.packet)
    print(json.dumps(result if args.list_json else result['summary'], indent=2, sort_keys=True))


if __name__ == '__main__':
    main()
