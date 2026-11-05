#!/usr/bin/env python3
"""Prepare a bounded five-artifact request only; no Cargo, VM, SDK or network."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import stat

ROOT = Path(__file__).resolve().parents[2]
LANES = ('rust-off', 'rust-on', 'chialisp', 'rue-off', 'rue-on')
MAX_HEX = 512 * 1024
MAX_DECODED = 2 * 1024 * 1024
PROTOCOL = 'chialisp-oxide-standard-tutorial-ledger-request/v1'


def require(value, message):
    if not value:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for lane in LANES:
        parser.add_argument('--' + lane, type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    require(out.is_relative_to(ROOT / 'target') and not out.exists(), 'fresh output below this private worktree target required')
    context_out = out.with_name(out.name + '.context.json')
    require(not context_out.exists(), 'fresh context sidecar required')
    selected = []
    total = 0
    for lane in LANES:
        path = getattr(args, lane.replace('-', '_'))
        info = path.lstat()
        require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and 0 < info.st_size <= MAX_HEX + 1,
                'bounded regular single-link selected CLVM hex file')
        total += info.st_size
        require(total <= 2 * MAX_DECODED + len(LANES), 'complete selected source byte precharge')
        selected.append((lane, path, info.st_size))
    runs = []
    bindings = []
    decoded = 0
    for lane, path, size in selected:
        with path.open('rb') as source:
            raw = source.read(MAX_HEX + 2)
        require(len(raw) == size, 'selected file changed during bounded read')
        text = raw.removesuffix(b'\n')
        require(0 < len(text) <= MAX_HEX and len(text) % 2 == 0 and re.fullmatch(b'[0-9a-f]+', text),
                'canonical lowercase CLVM hex with at most one trailing LF required')
        require(len(text) // 2 <= MAX_DECODED - decoded, 'complete decoded module byte precharge')
        decoded += len(text) // 2
        module = bytes.fromhex(text.decode('ascii'))
        runs.append({'lane': lane, 'module_hex': text.decode('ascii')})
        bindings.append({'lane': lane, 'selected_path': str(path.resolve()), 'file_bytes': len(raw),
                         'file_sha256': digest(raw), 'module_bytes': len(module), 'module_blob_sha256': digest(module)})
    request = (json.dumps({'protocol': PROTOCOL, 'runs': runs}, indent=2) + '\n').encode()
    require(len(request) <= 4 * 1024 * 1024, 'SDK request byte bound')
    context = {'status': 'prepared_not_executed', 'request_bytes': len(request), 'request_sha256': digest(request),
               'selected_inputs': bindings, 'expected_builds': len(LANES),
               'caller_must_independently_admit_complete_sources_bundles_and_selected_artifacts': True,
               'scope': 'Selected local file/hex byte identities only. No CLVM program admission, module tree-hash authentication, source authenticity, VM or SDK execution, wallet or network credit.'}
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open('xb') as destination:
        destination.write(request)
    with context_out.open('x') as destination:
        json.dump(context, destination, indent=2)
        destination.write('\n')
    print(json.dumps({'status': 'prepared_not_executed', 'request': str(out), 'context': str(context_out)}))


if __name__ == '__main__':
    main()
