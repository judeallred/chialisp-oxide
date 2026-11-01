#!/usr/bin/env python3
"""Local public continuation planning/recovery, not a network or signer adapter.

Only the complete admitted retained Rust step packet is selectable. Supplied
observations are not chain authentication. Journals require a private directory
and cooperating writers; fsync/rename is not a hostile-directory or power-loss
safety claim. No secrets, funding transactions or submissions are accepted.
"""
import argparse
import contextlib
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import tempfile

import check_retained
import oracle as o

BASE = Path(__file__).resolve().parent
LIMIT = 65536
SCOPE = 'local public workflow; supplied observations are not chain authentication'


def require(value, message):
    if not value:
        raise ValueError(message)


def fields(value, names):
    require(type(value) is dict and set(value) == set(names.split()), 'exact object fields')


def hex32(value):
    require(type(value) is str and re.fullmatch('[0-9a-f]{64}', value), 'canonical 32-byte hex')
    return bytes.fromhex(value)


def encode(value):
    return (json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False)+'\n').encode()


def payload(path, limit=LIMIT):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, 'rb') as stream:
        info = os.fstat(stream.fileno())
        require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and info.st_size <= limit, 'bounded regular single-link input')
        raw = stream.read(limit+1)
    require(len(raw) <= limit, 'bounded input')
    return raw


def read(path):
    raw = payload(path)
    # Bound nested containers before json.loads allocates recursive structures.
    depth = 0
    quoted = escaped = False
    for byte in raw:
        if quoted:
            if escaped:
                escaped = False
            elif byte == 92:
                escaped = True
            elif byte == 34:
                quoted = False
        elif byte == 34:
            quoted = True
        elif byte in (91, 123):
            depth += 1
            require(depth <= 24, 'JSON nesting bound')
        elif byte in (93, 125):
            depth -= 1
    def pairs(rows):
        result = {}
        for key, value in rows:
            require(key not in result, 'duplicate JSON key')
            result[key] = value
        return result
    def invalid(_):
        raise ValueError('nonfinite JSON number')
    return json.loads(raw, object_pairs_hook=pairs, parse_constant=invalid)


def selected(mode):
    require(mode in ('on', 'off'), 'exact Rust mode')
    receipt = check_retained.check(BASE)
    relative = f'rust-{mode}/manifest.json'
    raw = payload(BASE/'evidence'/relative, limit=256*1024)
    require(hashlib.sha256(raw).hexdigest() == receipt['files'][relative],
            'selected manifest changed after complete admission')
    manifest = json.loads(raw)
    artifact = manifest['entries'][0]['artifact']
    return dict(mode=mode, module_hash=artifact['tree_hash'],
                blob_sha256=hashlib.sha256(bytes.fromhex(artifact['clvm_hex'])).hexdigest(),
                source_files=receipt['source_files'], compiler=receipt['compiler'],
                driver_sha256=receipt['tool_files']['chialisp-oxide-driver'])


def plan(artifact, recipients):
    require(type(recipients) is list and 1 <= len(recipients) <= o.MAX_ITEMS, 'queue length 1..8')
    recipients_bytes = [hex32(value) for value in recipients]
    module = hex32(artifact['module_hash'])
    heads = [o.terminal()]
    for recipient in reversed(recipients_bytes):
        heads.insert(0, o.item(recipient, heads[0]))
    steps = []
    for index, recipient in enumerate(recipients_bytes):
        remaining = len(recipients)-index
        conditions = o.step(module, heads[index], remaining, recipient, heads[index+1])
        steps.append(dict(puzzle_hash=o.instance_hash(module, heads[index], remaining).hex(),
                          amount=o.PAYOUT*remaining, recipient=recipient.hex(), next_digest=heads[index+1].hex(),
                          conditions=conditions))
    for row in steps:
        outputs = [(condition[1], condition[2]) for condition in row['conditions'] if condition[0] == 51]
        require(len(outputs) == len(set(outputs)), 'duplicate child coin identities')
    return dict(artifact=artifact, recipients=recipients, steps=steps)


def coin(parent, puzzle_hash, amount):
    require(type(amount) is int and 0 < amount <= 8000, 'bounded positive mojo amount')
    return dict(parent_id=parent, puzzle_hash=puzzle_hash, amount=amount,
                coin_id=o.sha(hex32(parent)+hex32(puzzle_hash)+o.integer(amount)).hex())


def expected_coin(state, index):
    parent = state['funding_parent']
    for position in range(index+1):
        step = state['plan']['steps'][position]
        current = coin(parent, step['puzzle_hash'], step['amount'])
        parent = current['coin_id']
    return current


def validate(state, artifact):
    fields(state, 'schema scope network revision plan funding_parent completed')
    require(type(state['schema']) is int and state['schema'] == 1 and state['scope'] == SCOPE
            and state['network'] == 'testnet11', 'fixed workflow scope/network/schema')
    require(type(state['revision']) is int and 0 <= state['revision'] <= 9, 'bounded revision')
    fields(state['plan'], 'artifact recipients steps')
    require(state['plan'] == plan(artifact, state['plan']['recipients']), 'fresh complete plan/artifact identity')
    count = len(state['plan']['steps'])
    require(type(state['completed']) is int and 0 <= state['completed'] <= count, 'bounded completed prefix')
    if state['funding_parent'] is None:
        require(state['completed'] == state['revision'] == 0, 'unfunded initial state')
    else:
        hex32(state['funding_parent'])
        require(state['revision'] == state['completed']+1, 'funding plus exact completed prefix revisions')
    return state


def token(state):
    return hashlib.sha256(encode(state)).hexdigest()


@contextlib.contextmanager
def locked(path):
    lock = path.with_name(path.name+'.lock')
    fd = os.open(lock, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600)
    try:
        info = os.fstat(fd)
        require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and info.st_size == 0, 'empty regular single-link lock')
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        yield
    finally:
        os.close(fd)


class PostcommitError(OSError):
    """Replacement committed; directory durability could not be confirmed."""


def write(path, state):
    payload = encode(state)
    require(len(payload) <= LIMIT, 'bounded journal output')
    fd, temporary = tempfile.mkstemp(prefix=path.name+'.', dir=path.parent)
    try:
        with os.fdopen(fd, 'wb') as stream:
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        # Failure after rename is postcommit I/O uncertainty: do not retry
        # mutation blindly; reopen and inspect the resulting revision.
        try:
            directory = os.open(path.parent, os.O_RDONLY)
            try:
                os.fsync(directory)
            finally:
                os.close(directory)
        except OSError as error:
            raise PostcommitError('journal replaced but durability uncertain; reload and inspect revision before retry') from error
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def initialize(path, mode, recipients):
    artifact = selected(mode)
    state = dict(schema=1, scope=SCOPE, network='testnet11', revision=0,
                 plan=plan(artifact, recipients), funding_parent=None, completed=0)
    with locked(path):
        require(not os.path.lexists(path), 'journal already exists')
        write(path, state)
    return state


def load(path):
    state = read(path)
    fields(state, 'schema scope network revision plan funding_parent completed')
    fields(state['plan'], 'artifact recipients steps')
    artifact = state['plan']['artifact']
    require(type(artifact) is dict and type(artifact.get('mode')) is str, 'selected artifact mode')
    return validate(state, selected(artifact['mode']))


def mutate(path, expected, *, parent=None, observation=None):
    hex32(expected)
    require((parent is None) != (observation is None), 'one explicit mutation')
    with locked(path):
        state = load(path)
        require(token(state) == expected, 'stale journal revision')
        if parent is not None:
            hex32(parent)
            require(state['funding_parent'] is None, 'already funded')
            state['funding_parent'] = parent
        else:
            fields(observation, 'network coin_id children')
            require(observation['network'] == 'testnet11' and state['funding_parent'] is not None,
                    'funded testnet11 observation required')
            index = state['completed']
            steps = state['plan']['steps']
            require(index < len(steps), 'already complete')
            current = expected_coin(state, index)
            children = [coin(current['coin_id'], steps[index]['recipient'], o.PAYOUT)]
            if index+1 < len(steps):
                children.append(expected_coin(state, index+1))
            require(observation['coin_id'] == current['coin_id'] and observation['children'] == children,
                    'exact consumed coin and ordered payout/successor observations')
            state['completed'] += 1
        state['revision'] += 1
        validate(state, state['plan']['artifact'])
        write(path, state)
        return state


def view(state):
    index = state['completed']
    current = None
    if state['funding_parent'] is not None and index < len(state['plan']['steps']):
        current = expected_coin(state, index)
    return dict(revision_token=token(state), state=state, current_coin=current,
                next_step=state['plan']['steps'][index] if index < len(state['plan']['steps']) else None)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    init = sub.add_parser('init')
    init.add_argument('--mode', choices=('on', 'off'), required=True)
    init.add_argument('--recipients', type=Path, required=True)
    for name in ('show', 'record-funding', 'reconcile'):
        command = sub.add_parser(name)
        command.add_argument('--journal', type=Path, required=True)
        if name != 'show':
            command.add_argument('--revision-token', required=True)
        if name == 'record-funding':
            command.add_argument('--parent', required=True)
        if name == 'reconcile':
            command.add_argument('--observation', type=Path, required=True)
    init.add_argument('--journal', type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == 'init':
            state = initialize(args.journal, args.mode, read(args.recipients))
        elif args.command == 'show':
            state = load(args.journal)
        elif args.command == 'record-funding':
            state = mutate(args.journal, args.revision_token, parent=args.parent)
        else:
            state = mutate(args.journal, args.revision_token, observation=read(args.observation))
        print(json.dumps(view(state), sort_keys=True))
    except (ValueError, OSError, KeyError, TypeError) as error:
        parser.exit(1, f'workflow error: {error}\n')


if __name__ == '__main__':
    main()
