#!/usr/bin/env python3
"""Prepare a permissionless zero-fee step offline; never send a transaction.

Admits the complete current-source retained packet and the rooted public
journal before executing the supplied host binary. Supplied coin/height are not
chain authentication. Requires an explicitly selected locally built executable.
"""
import argparse
import hashlib
import json
from pathlib import Path
import tempfile

import bounded_process
import workflow as w


def request(journal, height):
    w.require(type(height) is int and 0 <= height < 0xffffffff, 'supplied pre-hard-fork-2 height (< 4294967295)')
    state = w.load(journal)
    view = w.view(state)
    w.require(view['current_coin'] is not None, 'recorded incomplete funded queue required')
    mode = state['plan']['artifact']['mode']
    manifest = json.loads(w.payload(w.BASE/'evidence'/f'rust-{mode}'/'manifest.json', limit=256*1024))
    artifact = manifest['entries'][0]['artifact']
    w.require(hashlib.sha256(bytes.fromhex(artifact['clvm_hex'])).hexdigest() == state['plan']['artifact']['blob_sha256'],
              'selected actual blob differs from admitted journal')
    return state, dict(schema=1, network='testnet11', mode=mode, module_hex=artifact['clvm_hex'],
                       recipients=state['plan']['recipients'], completed=state['completed'],
                       funding_parent=state['funding_parent'], current_coin=view['current_coin'],
                       previous_transaction_height=height)


def prepare(journal, executable, height):
    state, value = request(journal, height)
    executable = executable.resolve(strict=True)
    executable_hash = hashlib.sha256(executable.read_bytes()).hexdigest()
    with tempfile.TemporaryDirectory(prefix='continuation-prepare-') as directory:
        path = Path(directory)/'public-input.json'
        path.write_bytes(w.encode(value))
        result = bounded_process.run([executable, path], check=True, timeout=60,
                                     stdout_limit=2*1024*1024, stderr_limit=65536)
    output = json.loads(result.stdout)
    expected = [w.coin(value['current_coin']['coin_id'], state['plan']['steps'][state['completed']]['recipient'], 1000)]
    if state['completed']+1 < len(state['plan']['steps']):
        expected.append(w.expected_coin(state, state['completed']+1))
    w.require(output['current_coin'] == value['current_coin'] and output['children'] == expected
              and output['module_hash'] == state['plan']['artifact']['module_hash']
              and output['module_blob_sha256'] == state['plan']['artifact']['blob_sha256'], 'actual prepared coin/module/children relation')
    w.require(w.token(w.load(journal)) == w.token(state), 'journal changed during preparation')
    w.require(hashlib.sha256(executable.read_bytes()).hexdigest() == executable_hash, 'host executable changed during preparation')
    output['journal_revision_token'] = w.token(state)
    output['host_executable_sha256'] = executable_hash
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--journal', type=Path, required=True)
    parser.add_argument('--executable', type=Path, required=True)
    parser.add_argument('--previous-transaction-height', type=int, required=True)
    args = parser.parse_args()
    print(json.dumps(prepare(args.journal, args.executable, args.previous_transaction_height), sort_keys=True))


if __name__ == '__main__':
    main()
