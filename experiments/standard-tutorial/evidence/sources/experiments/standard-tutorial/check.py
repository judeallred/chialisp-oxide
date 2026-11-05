#!/usr/bin/env python3
"""Check recorded standard-tutorial evidence without compiler/VM/SDK/network work."""
import argparse
import copy
import json
from pathlib import Path
import shutil
import tempfile

import packet

ROOT = Path(__file__).resolve().parents[2]


def self_test(root):
    """Constructed record corruptions add no new policy or SDK execution credit."""
    original = packet.Packet(root)
    count = 0
    with tempfile.TemporaryDirectory(prefix='standard-tutorial-record-controls-') as folder:
        path = Path(folder) / 'packet'
        shutil.copytree(original.root, path)
        pristine_receipt = (path / 'receipt.json').read_bytes()
        def reject(label, mutate):
            nonlocal count
            restore = mutate()
            try:
                try:
                    packet.validate(path)
                except (ValueError, KeyError, TypeError, StopIteration):
                    count += 1
                else:
                    raise AssertionError('accepted corrupt retained record: ' + label)
            finally:
                restore()
                (path / 'receipt.json').write_bytes(pristine_receipt)
        def index_change(action):
            value = copy.deepcopy(original.index)
            action(value)
            (path / 'receipt.json').write_text(json.dumps(value, indent=2) + '\n')
            return lambda: None
        reject('boolean byte profile', lambda: index_change(lambda v: v['profile'].update(max_files=True)))
        reject('inferred source subset', lambda: index_change(lambda v: v['files'].pop('sources/Cargo.lock')))
        reject('unknown root authority', lambda: index_change(lambda v: v.update(execution_authenticated=True)))
        def extra():
            file = path / 'extra.json'
            file.write_text('{}\n')
            return lambda: file.unlink()
        reject('extra physical member', extra)
        def changed_count():
            receipt = copy.deepcopy(original.index)
            names = (packet.BINDINGS['report'], packet.BINDINGS['repeat'])
            prior = {name: (path / name).read_bytes() for name in names}
            value = original.load(packet.BINDINGS['report'])
            value['counts']['transaction_attempts'] = True
            raw = (json.dumps(value, indent=2) + '\n').encode()
            for name in names:
                (path / name).write_bytes(raw)
                receipt['files'][name] = {'bytes': len(raw), 'sha256': packet.sha(raw)}
            (path / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
            return lambda: [(path / name).write_bytes(data) for name, data in prior.items()]
        reject('coherently rehashed SDK boolean count', changed_count)
    return count


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--packet', type=Path, default=Path(__file__).resolve().parent / 'evidence')
    parser.add_argument('--current', action='store_true', help='also compare explicit current repository sources and complete48 report')
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    result = packet.validate(args.packet, current_root=ROOT if args.current else None)
    if args.self_test:
        result['constructed_record_controls'] = self_test(args.packet)
    print(json.dumps(result, sort_keys=True))


if __name__ == '__main__':
    main()
