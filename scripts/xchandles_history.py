"""Closed offline XCHandles historical component packet reader.

No network, subprocess, VM, SDK or compiler calls. Recorded absolute paths are
opaque provenance strings. Passing this reader grants retained-byte consistency,
not new execution, chain inclusion, source authentication or a complete spend.
"""
import collections
import csv
import hashlib
import io
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import stat

from cargo_source_receipt import decode_json
from registry_codegen_evidence import inspect_compiled_artifact

PROFILE = 'xchandles-mainnet-premine-history/v1'
MANIFEST = 'history-packet.json'
MAX_FILES = 2048
MAX_TOTAL = 128 * 1024 * 1024
MAX_MEMBER = 2 * 1024 * 1024
MAX_MANIFEST = 512 * 1024
SEED = '6f314f69a9cf776311e1c7781a3013e4daae3696c71d8a04d049a5af1530f050'
SINGLETON = '7faa3253bfddd1e0decb0906b2dc6247bbc4cf608f58345d173adb63e8b47c9f'
LAUNCHER = 'eff07522495060c066f66f32acc2a77e3a3e737aca8baea4d1a64ea4cdc13da9'
EVE = 'f44621b5cff07a5e02f78b95e579871d62e3df5e3ee31425a61dda846336b83c'
FIRST = '57f0e719db51fdc7d1400462372a6dc7cfb8526e9237e720cf78eda72e66524f'
FULL = '4b9b1135a504bf907cafbc17b7565dbcd66be728c5449a5d351c9eeac1314e81'
ACTION = 'afa03f2903f9e4a293523237799074b13ab2361f250df10de5df884b0b09a22b'
REGISTER = '150ae295efe60fa049b87cff5c05c0031fa768959ba466b425ef5a58b4a95ee8'
REGISTER_BLOB = 'f7a33ee12388d19c1eb96e47734803366572d94706dc6f24ab0cdd5776a6123f'
FINALIZER = '9d274de59aeaa128d1b0a802b52911838420ef9720d605c78942e8dadfc0810b'
ROOT = '98744bd52977a2977337a3bcac734c12c28916257c9cbacac41a8e1cedae669b'
STATE = 'ae396f69c89206da3f05fe15e999920bfb46f8b44c42946d0a91bf56a0242457'
CSV_SHA = '44697ad7265b4869226825741b0e0466a3031b382b462925a051a69190405860'
LICENSE_SHA = '257f43a5c0407fb3a90f3c0771aecdd77a1160dee1ee2be8250ac629dcac1962'
MAX_COST = 11_000_000_000
SCOPE = ('Retained consistency of 129 recorded primary clvmr0.21 MEMPOOL_MODE component runs; '
         'this reader performs zero VM/SDK/network runs. Exact CSV handle-name coverage only; '
         '19 selected older dependencies are not the complete39-request original transcript; '
         'no child-list completeness, recipient/NFT/expiration agreement, full-bundle cost, fee attribution, chain/source/'
         'deployment/execution authentication, final/current registry, autonomous remainder, '
         'liveness or compiler-preservation claim.')


def require(value, message):
    if not value:
        raise ValueError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def ident(raw):
    return dict(bytes=len(raw), sha256=sha(raw))


def closed(value, keys, label):
    require(type(value) is dict and set(value) == set(keys), label + ' exact fields')


def integer(value, maximum=2**64-1, minimum=0):
    require(type(value) is int and minimum <= value <= maximum, 'integer (not bool) bound')
    return value


def relative(name):
    require(type(name) is str and 0 < len(name.encode()) <= 512 and '\\' not in name
            and not any(ord(c) < 32 for c in name), 'relative path text')
    p = PurePosixPath(name)
    require(not p.is_absolute() and p.as_posix() == name and len(p.parts) <= 8
            and all(part not in ('', '.', '..') for part in p.parts), 'relative path containment')
    return name


def read_regular(path, maximum, expected=None):
    path = Path(path)
    # Open each directory component without following an alias, including if
    # an ancestor changed after the inventory walk. No historical path enters here.
    absolute = path.absolute()
    directory_fd = os.open(absolute.anchor, os.O_RDONLY | os.O_DIRECTORY)
    try:
        for part in absolute.parts[1:-1]:
            child_fd = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=directory_fd)
            os.close(directory_fd)
            directory_fd = child_fd
        fd = os.open(absolute.name, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW, dir_fd=directory_fd)
    finally:
        os.close(directory_fd)
    with os.fdopen(fd, 'rb') as stream:
        before = os.fstat(stream.fileno())
        require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1
                and 0 <= before.st_size <= maximum, 'bounded regular member')
        if expected is not None:
            require((before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns)
                    == expected, 'member changed after inventory')
        raw = stream.read(before.st_size + 1)
        after = os.fstat(stream.fileno())
    require(len(raw) == before.st_size and
            (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns, before.st_nlink)
            == (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns, after.st_nlink),
            'stable exact member read')
    return raw


def json_value(raw, maximum=MAX_MEMBER):
    require(type(raw) is bytes and len(raw) <= maximum, 'JSON byte bound')
    value = decode_json(raw, max_depth=64, max_nodes=100000 if maximum == MAX_MANIFEST else 200000)
    # Floats only represent explicitly named observational timers, never roles
    # such as amount/cost/count. All integer roles are checked separately too.
    pending = [(value, '')]
    while pending:
        item, key = pending.pop()
        integer_roles = {'amount', 'cost', 'primary_cost', 'action_count', 'index', 'step_index',
            'bytes', 'maximum', 'max_cost', 'max_primary_cost', 'max_steps', 'pinned_csv_names',
            'http_status', 'confirmed_block_index', 'spent_block_index', 'timestamp',
            'retained_maximum', 'remaining_retained_bytes', 'exit_code', 'retries',
            'expected_confirmed_height', 'max_blob_bytes', 'max_nodes', 'max_depth_root_one',
            'nodes', 'pairs', 'atoms', 'atom_payload_bytes', 'depth_root_one', 'height'}
        if key in integer_roles:
            integer(item)
        if key == 'elapsed_seconds':
            require(type(item) in (int, float) and math.isfinite(item) and item >= 0, 'timing role')
        if type(item) is float:
            require(key == 'elapsed_seconds' and math.isfinite(item) and item >= 0,
                    'float outside timing role')
        elif type(item) is dict:
            pending.extend((v, k) for k, v in item.items())
        elif type(item) is list:
            pending.extend((v, '') for v in item)
    return value


class Packet:
    """Manifest then complete physical precharge, then lazy bounded payload reads."""
    def __init__(self, root):
        self.root = Path(root)
        require(self.root.is_dir() and not self.root.is_symlink(), 'packet root directory')
        # Check all ancestors without resolving aliases or following symlinks.
        for p in [self.root, *self.root.parents]:
            require(not p.is_symlink(), 'packet root ancestor symlink')
        raw = read_regular(self.root / MANIFEST, MAX_MANIFEST)
        self.manifest = json_value(raw, MAX_MANIFEST)
        closed(self.manifest, ('protocol', 'profile', 'members'), 'manifest')
        require(self.manifest['protocol'] == 'chialisp-oxide-history-packet/v1'
                and self.manifest['profile'] == PROFILE, 'named history profile')
        rows = self.manifest['members']
        require(type(rows) is list and 0 < len(rows) < MAX_FILES, 'manifest member count')
        self.rows = {}
        total = len(raw)
        for row in rows:
            closed(row, ('path', 'role', 'bytes', 'sha256'), 'member')
            name = relative(row['path'])
            require(name != MANIFEST and name not in self.rows, 'duplicate/reserved member')
            require(type(row['role']) is str and row['role'] == role(name), 'closed member role')
            integer(row['bytes'], MAX_MEMBER)
            require(type(row['sha256']) is str and re.fullmatch('[0-9a-f]{64}', row['sha256']), 'SHA256')
            total += row['bytes']
            require(total <= MAX_TOTAL, 'aggregate precharge')
            self.rows[name] = row
        self.physical = {}
        pending = [(self.root, 0)]
        entries = 0
        inodes = set()
        while pending:
            directory, depth = pending.pop()
            require(depth <= 8, 'directory depth bound')
            with os.scandir(directory) as stream:
                for entry in stream:
                    entries += 1
                    require(entries <= 8192, 'directory entry bound')
                    st = entry.stat(follow_symlinks=False)
                    require(not stat.S_ISLNK(st.st_mode), 'member symlink')
                    if stat.S_ISDIR(st.st_mode):
                        pending.append((Path(entry.path), depth + 1))
                        continue
                    require(stat.S_ISREG(st.st_mode) and st.st_nlink == 1, 'regular nonalias inventory')
                    name = relative(Path(entry.path).relative_to(self.root).as_posix())
                    require((st.st_dev, st.st_ino) not in inodes, 'physical alias')
                    inodes.add((st.st_dev, st.st_ino))
                    require(len(self.physical) < MAX_FILES, 'physical file count')
                    require(st.st_size <= (MAX_MANIFEST if name == MANIFEST else MAX_MEMBER), 'physical member size')
                    self.physical[name] = (st.st_dev, st.st_ino, st.st_size, st.st_mtime_ns)
        require(set(self.physical) == set(self.rows) | {MANIFEST}, 'missing/extraneous payload')
        for name, row in self.rows.items():
            require(self.physical[name][2] == row['bytes'], 'declared physical size')
        require(sum(v[2] for v in self.physical.values()) <= MAX_TOTAL, 'physical aggregate')
        require(read_regular(self.root / MANIFEST, MAX_MANIFEST, self.physical[MANIFEST]) == raw,
                'stable admitted manifest')

    def raw(self, name):
        relative(name)
        require(name in self.rows, 'undeclared payload')
        raw = read_regular(self.root / name, MAX_MEMBER, self.physical[name])
        require(ident(raw) == {k: self.rows[name][k] for k in ('bytes', 'sha256')}, 'member byte identity')
        return raw

    def json(self, name):
        return json_value(self.raw(name))


def role(name):
    if name == 'attempt4/conclusions.json':
        return 'later_derived_conclusions'
    if name.startswith('attempt4/'):
        return 'original_execution'
    if name.startswith('attempt2/'):
        return 'earlier_observer_dependency'
    if name == 'attempt5/prefix-dependencies.json':
        return 'closed_prefix_inventory'
    if name.startswith('attempt5/'):
        require(name in {'attempt5/request.json', 'attempt5/coin-record.raw.json',
                         'attempt5/terminal-observation.json'}, 'terminal role')
        return 'later_terminal_observation'
    require(name in {'inputs/premine.csv', 'inputs/register.clvm', 'inputs/LICENSE.md',
                     'inputs/provenance.json', 'execute-trace-attempt4.py'}, 'closed input role')
    return 'pinned_input' if name.startswith('inputs/') else 'historical_collector'


def unhex(value, maximum=1048576):
    require(type(value) is str and len(value) <= maximum * 2 + 2
            and re.fullmatch(r'(?:0x)?(?:[0-9a-f]{2})*', value), 'canonical bounded hex')
    return bytes.fromhex(value.removeprefix('0x'))


def inspect(raw):
    v = inspect_compiled_artifact(raw, max_bytes=1048576, max_nodes=65536, max_depth=512)
    st = v['stats']
    require(st['nodes'] * 192 + len(raw) + st['atom_payload_bytes'] <= 16777216,
            'decoded modeled work bound')
    return v


def decode(raw):
    inspect(raw)
    cursor, stack = 0, []
    while True:
        head = raw[cursor]
        cursor += 1
        if head == 255:
            stack.append(None)
            continue
        if head < 128:
            value = bytes([head])
        elif head == 128:
            value = b''
        else:
            width = 0
            while head & (128 >> width):
                width += 1
            length = head & ((1 << (7-width))-1)
            for byte in raw[cursor:cursor+width-1]:
                length = length * 256 + byte
            cursor += width-1
            value = raw[cursor:cursor+length]
            cursor += length
        while stack and stack[-1] is not None:
            value = (stack.pop(), value)
        if stack:
            stack[-1] = value
        else:
            return value


def treehash(tree):
    pending, values = [(tree, False)], []
    while pending:
        item, done = pending.pop()
        if type(item) is bytes:
            values.append(hashlib.sha256(b'\1' + item).digest())
        elif done:
            right, left = values.pop(), values.pop()
            values.append(hashlib.sha256(b'\2' + left + right).digest())
        else:
            pending.extend(((item, True), (item[1], False), (item[0], False)))
    return values[0]


def fields(tree, maximum=65536):
    result = []
    while type(tree) is tuple:
        require(len(result) < maximum, 'proper list length')
        result.append(tree[0])
        tree = tree[1]
    require(tree == b'', 'proper list tail')
    return result


def take(tree, count):
    result = []
    for _ in range(count):
        require(type(tree) is tuple, 'pair projection')
        result.append(tree[0])
        tree = tree[1]
    return result, tree


def uncurry(tree):
    v = fields(tree, 3)
    require(len(v) == 3 and v[0] == b'\2' and type(v[1]) is tuple and v[1][0] == b'\1', 'curry outer')
    module, tree, args = v[1][1], v[2], []
    while tree != b'\1':
        v = fields(tree, 3)
        require(len(args) < 16 and len(v) == 3 and v[0] == b'\4'
                and type(v[1]) is tuple and v[1][0] == b'\1', 'curry argument')
        args.append(v[1][1])
        tree = v[2]
    return module, args


def assembly_preflight(raw):
    st = inspect(raw)['stats']
    bound = st['pairs'] * 5 + st['atom_payload_bytes'] * 2 + st['atoms'] * 2
    require(bound <= 240000, 'assembly precharge')
    return bound


def assembly(raw):
    bound = assembly_preflight(raw)
    pending, parts = [decode(raw)], []
    while pending:
        v = pending.pop()
        if type(v) is str:
            parts.append(v)
        elif type(v) is tuple:
            pending.extend((')', v[1], ' . ', v[0], '('))
        else:
            parts.append('0x' + v.hex() if v else '()')
    result = ''.join(parts)
    require(len(result) <= bound, 'assembly accounting')
    return result


def coin_id(coin):
    closed(coin, ('parent_coin_info', 'puzzle_hash', 'amount'), 'coin')
    parent, puzzle = unhex(coin['parent_coin_info'], 32), unhex(coin['puzzle_hash'], 32)
    require(len(parent) == len(puzzle) == 32, 'coin hash widths')
    amount = integer(coin['amount'])
    atom = b'' if not amount else amount.to_bytes((amount.bit_length()+8)//8, 'big')
    return sha(parent + puzzle + atom)


def record(value):
    closed(value, ('coin', 'coinbase', 'confirmed_block_index', 'spent', 'spent_block_index', 'timestamp'), 'record')
    coin_id(value['coin'])
    require(type(value['coinbase']) is bool and type(value['spent']) is bool, 'record bool flags')
    for key in ('confirmed_block_index', 'spent_block_index', 'timestamp'):
        integer(value[key])
    require(value['confirmed_block_index'] <= value['spent_block_index'] and value['spent'], 'observed spent record heights')
    return value


def success(value, body_key):
    closed(value, ('success', body_key), 'RPC response')
    require(value['success'] is True, 'provider success flag')
    return value[body_key]



def provider_network(value):
    closed(value, ('success', 'network_name', 'network_prefix'), 'network response')
    require(value['success'] is True and value['network_name'] == 'mainnet'
            and value['network_prefix'] == 'xch', 'strict mainnet/xch provider metadata')
    return True


def identity_check(raw, expected):
    require(inspect(raw)['identity'] == expected, 'CLVM byte/tree identity')


def check_actions(program, solution, step, register_raw):
    module, args = uncurry(decode(program))
    require(treehash(module).hex() == SINGLETON and len(args) == 2, 'singleton module')
    require(args[0] == (bytes.fromhex(SINGLETON), (bytes.fromhex(SEED), bytes.fromhex(LAUNCHER))), 'singleton whole struct')
    module, aa = uncurry(args[1])
    require(treehash(module).hex() == ACTION and len(aa) == 3 and aa[1] == bytes.fromhex(ROOT)
            and treehash(aa[2]).hex() == STATE, 'action layer state/root')
    fm, fa = uncurry(aa[0])
    require(len(fa) == 1 and type(fa[0]) is bytes and len(fa[0]) == 32
            and fa[0] == treehash(fm), 'finalizer second curry')
    base, first = uncurry(fm)
    require(treehash(base).hex() == FINALIZER and first == [bytes.fromhex(ACTION), bytes.fromhex(SEED)], 'finalizer first curry')
    require(step['state_tree_hash'] == STATE and step['allowed_action_merkle_root'] == ROOT, 'state/root receipt')
    sol = fields(decode(solution), 3)
    require(len(sol) == 3 and sol[1] == b'\1', 'singleton solution amount')
    inner, tail = take(sol[2], 3)
    require(tail == b'', 'default finalizer solution nil')
    puzzles, selectors, solutions = map(fields, inner)
    count = integer(step['action_count'], 128, 1)
    require(len(puzzles) == 1 and len(selectors) == len(solutions) == count, 'closed action profile counts')
    mod, pa = uncurry(puzzles[0])
    require(treehash(mod).hex() == REGISTER and treehash(decode(register_raw)) == treehash(mod), 'register module bytes identity')
    require(pa == [bytes.fromhex(x) for x in (SINGLETON, LAUNCHER,
            'ac3ce0079da5db4ff11cb9bc495653a52e1e2d5ae53c60e772b853f9e5483d53',
            'c6d65bf45831009daffe9b722c97f7d7b64db943b401c536271bfa11d0fd50fd')], 'register all curried arguments')
    require(step['action_module_hashes'] == [REGISTER], 'register hash inventory')
    first_selector = selectors[0]
    require(type(first_selector) is tuple and first_selector[0] == b'\2', 'first selector2')
    proof = fields(first_selector[1], 4)
    require(len(proof) == 4 and proof[0] == b'\3'
            and all(type(x) is bytes and len(x) == 32 for x in proof[1:]), 'path3 three siblings')
    leaf, path = hashlib.sha256(b'\1' + treehash(puzzles[0])).digest(), 3
    for sibling in proof[1:]:
        leaf = hashlib.sha256(b'\2' + (sibling + leaf if path & 1 else leaf + sibling)).digest()
        path >>= 1
    require(leaf == bytes.fromhex(ROOT) and path == 0, 'action Merkle proof')
    require(all(x == (b'\2', b'') for x in selectors[1:]), 'only already verified selector2 reuse')
    names = []
    for action in solutions:
        v, _ = take(action, 4)
        require(type(v[3]) is tuple, 'pricing pair')
        pricing, _ = take(v[3][1], 3)
        name = pricing[2]
        require(type(name) is bytes and hashlib.sha256(b'\1' + name).digest() == v[0], 'handle treehash')
        try:
            names.append(name.decode('ascii'))
        except UnicodeError as e:
            raise ValueError('ASCII handle') from e
    require(names == step['handles'] and len(set(names)) == len(names), 'step name ordering/uniqueness')
    return names


def http_row(packet, row, older=False):
    endpoint_key = 'endpoint' if older else 'url'
    keys = ('endpoint', 'request', 'http_status', 'observed_at', 'path', 'bytes', 'sha256', 'maximum') if older else (
        'url', 'request', 'path', 'http_status', 'bytes', 'sha256', 'observed_at')
    closed(row, keys, 'HTTP receipt')
    integer(row['http_status'], 599, 100)
    require(row['http_status'] == 200 and type(row['request']) is dict
            and type(row['observed_at']) is str, 'HTTP observation roles')
    endpoint = row[endpoint_key]
    require(endpoint in {'https://api.coinset.org/' + x for x in ('get_network_info',
        'get_coin_record_by_name', 'get_puzzle_and_solution', 'get_coin_records_by_parent_ids')}, 'fixed observer endpoint')
    body = row['request']
    if endpoint.endswith('/get_network_info'):
        closed(body, (), 'network request')
    elif endpoint.endswith('/get_coin_record_by_name'):
        closed(body, ('name',), 'coin request')
        require(len(unhex(body['name'], 32)) == 32, 'point coin ID width')
    elif endpoint.endswith('/get_puzzle_and_solution'):
        closed(body, ('coin_id', 'height'), 'spend request')
        require(len(unhex(body['coin_id'], 32)) == 32, 'spend coin ID width')
        integer(body['height'])
    else:
        closed(body, ('parent_ids', 'include_spent_coins', 'start_height', 'end_height', 'limit', 'order'), 'parent candidate request')
        require(type(body['parent_ids']) is list and len(body['parent_ids']) == 1
                and len(unhex(body['parent_ids'][0], 32)) == 32
                and body['include_spent_coins'] is True and body['order'] == 'asc'
                and integer(body['limit']) == 257
                and integer(body['end_height']) == integer(body['start_height']) + 1, 'parent request observed shape')
    name = ('attempt2/' if older else '') + relative(row['path'])
    raw = packet.raw(name)
    integer(row['bytes'], MAX_MEMBER)
    require(ident(raw) == {k: row[k] for k in ('bytes', 'sha256')}, 'RPC raw byte relation')
    if older:
        integer(row['maximum'], 1048576, 1)
        require(len(raw) <= row['maximum'], 'old RPC role bound')
    else:
        require(len(raw) <= (1048576 if endpoint.endswith('get_puzzle_and_solution') else 65536), 'RPC body bound')
    return json_value(raw)


def verify_prefix(packet, prefix, deps, steps):
    require(type(prefix['chain']) is list and len(prefix['chain']) == 8, 'eight prefix observations')
    closed(deps, ('protocol', 'path_base', 'origin_policy', 'first_six_registry_coin_ids',
        'bootstrap_coin_ids', 'index_receipt', 'files', 'scope'), 'closed prefix inventory')
    require(deps['protocol'] == 'chialisp-oxide-xchandles15-prefix-dependencies/v1', 'prefix inventory protocol')
    require(type(prefix['requests']) is list and len(prefix['requests']) == 39
            and len({q['path'] for q in prefix['requests']}) == 39, 'original39 descriptor index')
    rows = deps['files']
    require(type(rows) is list and len(rows) == 19 and len({r['path'] for r in rows}) == 19, 'closed19 dependencies')
    expected = set()
    network_rows = [x for x in rows if x.get('endpoint') == 'get_network_info']
    require(len(network_rows) == 1 and network_rows[0]['request'] == {}
            and provider_network(packet.json(network_rows[0]['path'])),
            'older network observation')
    for row in rows:
        closed(row, ('path', 'bytes', 'sha256', 'endpoint', 'request', 'role'), 'prefix dependency')
        name = relative(row['path'])
        require(name.startswith('attempt2/'), 'prefix member namespace')
        raw = packet.raw(name)
        require(ident(raw) == {k: row[k] for k in ('bytes', 'sha256')}, 'prefix dependency identity')
        qs = [q for q in prefix['requests'] if 'attempt2/' + q['path'] == name]
        require(len(qs) == 1 and qs[0]['request'] == row['request']
                and qs[0]['endpoint'].rsplit('/', 1)[-1] == row['endpoint'], 'prefix request crosslink')
        http_row(packet, qs[0], True)
        expected.add(name)
    require(deps['index_receipt'] == {'path': 'attempt2/trace-receipt.json',
            **ident(packet.raw('attempt2/trace-receipt.json'))}, 'prefix index identity')
    require(deps['first_six_registry_coin_ids'] == [x['coin_id'] for x in steps[:6]]
            and deps['bootstrap_coin_ids'] == [SEED, EVE], 'closed prefix IDs')
    for index, state in enumerate(prefix['chain']):
        rec = record(state['record'])
        require(coin_id(rec['coin']) == state['coin_id'], 'prefix coin ID')
        query = [x for x in rows if x['request'].get('name') == '0x' + state['coin_id']]
        spendquery = [x for x in rows if x['request'].get('coin_id') == '0x' + state['coin_id']]
        require(len(query) == len(spendquery) == 1, 'prefix raw pair closure')
        require(success(packet.json(query[0]['path']), 'coin_record') == rec, 'prefix raw record')
        spend = success(packet.json(spendquery[0]['path']), 'coin_solution')
        closed(spend, ('coin', 'puzzle_reveal', 'solution'), 'prefix spend')
        require(spend['coin'] == rec['coin'] and spendquery[0]['request']['height'] == rec['spent_block_index'], 'prefix raw spend')
        identity_check(unhex(spend['puzzle_reveal']), state['puzzle_reveal']['identity'])
        identity_check(unhex(spend['solution']), state['solution']['identity'])
        if index < 2:
            require(inspect(unhex(spend['puzzle_reveal']))['identity']['module_tree_hash']
                    == (LAUNCHER, '377ae246d10d2a748dbe4d7777b577bfc81d9159d4b5ffacaa5fbb57d96f0e51')[index], 'bootstrap module identity')
            require(state['coin_id'] == (SEED, EVE)[index]
                    and rec['confirmed_block_index'] == rec['spent_block_index'] == 9174777, 'bootstrap identities/heights')
            children = [x for x in rows if x['request'].get('parent_ids') == ['0x' + state['coin_id']]]
            require(len(children) == 1, 'bootstrap child candidate raw')
            candidates = success(packet.json(children[0]['path']), 'coin_records')
            next_state = prefix['chain'][index+1]
            require(type(candidates) is list and len(candidates) <= 256, 'bootstrap candidate list bound')
            chosen = [x for x in candidates if coin_id(x['coin']) == next_state['coin_id']]
            require(len(chosen) == 1, 'unique bootstrap candidate')
            require(record(chosen[0]) == next_state['record']
                    and next_state['record']['coin']['parent_coin_info'] == '0x' + state['coin_id'], 'bootstrap candidate edge')
            require(integer(next_state['record']['confirmed_block_index']) == rec['spent_block_index'],
                    'bootstrap creation-height edge')
        else:
            require(state['coin_id'] == steps[index-2]['coin_id'] and rec == steps[index-2]['record']
                    and spendquery[0]['path'] == steps[index-2]['source_spend_response'], 'first six reused inputs')
    return expected



def preflight_packet_artifacts(packet):
    """Inspect every serialized artifact before any selected tree is decoded.

    This phase retains only bounded JSON/scanner state, never decoded CLVM trees.
    Duplicate representations are checked too, including unselected bootstrap
    observations and the final step's input/output/command data.
    """
    for name in packet.rows:
        if name == 'inputs/register.clvm':
            inspect(packet.raw(name))
        elif name.endswith(('.solution.hex', '.clvm.hex')):
            raw = packet.raw(name)
            require(raw.endswith(b'\n'), 'artifact hex line terminator')
            artifact = unhex(raw[:-1].decode('ascii'))
            if name.endswith('.solution.hex'):
                assembly_preflight(artifact)
            else:
                inspect(artifact)
        elif name.endswith('-get_puzzle_and_solution.json'):
            value = packet.json(name)
            spend = success(value, 'coin_solution')
            closed(spend, ('coin', 'puzzle_reveal', 'solution'), 'preflight spend')
            inspect(unhex(spend['puzzle_reveal']))
            assembly_preflight(unhex(spend['solution']))
        elif re.fullmatch(r'attempt4/steps/[0-9]{3}\.stdout\.json', name):
            value = packet.json(name)
            closed(value, ('outcome', 'cost', 'error'), 'preflight successful VM envelope')
            closed(value['outcome'], ('status', 'output_hex'), 'preflight VM outcome')
            require(value['outcome']['status'] == 'accepted' and value['error'] is None,
                    'preflight accepted primary observation only')
            integer(value['cost'], MAX_COST, 1)
            inspect(unhex(value['outcome']['output_hex']))
        elif re.fullmatch(r'attempt4/steps/[0-9]{3}\.command\.json', name):
            assembly_preflight(unhex(packet.json(name)['input_environment_hex']))
    # The earlier index also contains serialized curry argument observations.
    # They are metadata, but receive the same complete transport preflight.
    prefix = packet.json('attempt2/trace-receipt.json')
    for state in prefix['chain']:
        for value in state.get('outer_curry', {}).get('argument_hex', []):
            inspect(unhex(value))



def verify(packet_root):
    packet = Packet(packet_root)
    # Every physical member is hash-checked, including nonselected metadata.
    # Read one member at a time; do not retain129 decoded environments.
    for name in packet.rows:
        raw = packet.raw(name)
        if name.endswith('.json'):
            json_value(raw)
    preflight_packet_artifacts(packet)
    r = packet.json('attempt4/execution-receipt.json')
    closed(r, ('protocol', 'status', 'six_prefix_states_executed', 'error', 'source_cli_unchanged',
        'max_steps', 'retained_maximum', 'remaining_retained_bytes', 'max_primary_cost', 'http_requests',
        'steps', 'covered_names', 'pinned_csv_names', 'missing_names', 'elapsed_seconds', 'scope'), 'execution receipt')
    require(r['protocol'] == 'chialisp-oxide-xchandles15-primary-prefix/v1'
            and r['status'] == 'all_pinned_csv_names_seen_in_executed_components'
            and r['error'] is None and r['six_prefix_states_executed'] is True
            and r['source_cli_unchanged'] is True, 'historical execution status')
    for key, expected in (('max_steps', 256), ('retained_maximum', MAX_TOTAL),
                          ('max_primary_cost', MAX_COST), ('pinned_csv_names', 3836)):
        require(integer(r[key]) == expected, 'receipt declared bound/count')
    integer(r['remaining_retained_bytes'], MAX_TOTAL)
    steps = r['steps']
    require(type(steps) is list and len(steps) == 129 and type(r['http_requests']) is list
            and len(r['http_requests']) == 247, '129 runs/247 new requests')
    before = packet.json('attempt4/source-cli-before.json')
    require(before == packet.json('attempt4/source-cli-after.json'), 'historical source/CLI window equality')
    closed(before, ('cli', 'sources', 'scope'), 'source descriptor')
    for row in [before['cli'], *before['sources']]:
        closed(row, ('path', 'bytes', 'sha256'), 'opaque source/tool descriptor')
        require(type(row['path']) is str, 'opaque source path text')
        integer(row['bytes'], 512 * 1024 * 1024)
        require(type(row['sha256']) is str and re.fullmatch('[0-9a-f]{64}', row['sha256']), 'source descriptor hash')
    origin = before['cli']['path']
    suffix = '/target/debug/chialisp-oxide'
    require(origin.endswith(suffix), 'recorded CLI origin shape')
    recorded_root = origin[:-len(suffix)]
    collector = packet.raw('execute-trace-attempt4.py')
    source_rows = {x['path']: x for x in before['sources']}
    require(len(source_rows) == len(before['sources']), 'unique historical source paths')
    collector_origins = [x for x in before['sources'] if x['path'].endswith('/execute-trace-attempt4.py')]
    require(len(collector_origins) == 1 and {k: collector_origins[0][k] for k in ('bytes', 'sha256')} == ident(collector), 'historical collector frozen recipe')
    csvraw = packet.raw('inputs/premine.csv')
    require(sha(csvraw) == CSV_SHA and len(csvraw) == 1689521, 'exact pinned CSV bytes')
    csv_origin = recorded_root + '/corpus/upstream/slot-machine/6edc94856306d0f77e065c6c99aacf1055e51e2f/xchandles_premine_mainnet.csv'
    require(source_rows.get(csv_origin) == {'path': csv_origin, **ident(csvraw)}, 'frozen CSV relation')
    reader = csv.DictReader(io.StringIO(csvraw.decode('utf-8'), newline=''))
    rows = list(reader)
    require(len(rows) == 3836 and all(type(x.get('handle')) is str for x in rows), 'CSV row count')
    names = [x['handle'] for x in rows]
    require(len(set(names)) == 3836 and all(x.isascii() for x in names), 'CSV names unique before lookup')
    name_set = set(names)
    register_raw = packet.raw('inputs/register.clvm')
    require(len(register_raw) == 1478 and sha(register_raw) == REGISTER_BLOB
            and inspect(register_raw)['identity']['module_tree_hash'] == REGISTER, 'exact1478 register artifact')
    require(sha(packet.raw('inputs/LICENSE.md')) == LICENSE_SHA, 'retained source license')
    provenance = packet.json('inputs/provenance.json')
    require(provenance == provenance_record(), 'closed source correspondence provenance')
    prefix = packet.json('attempt2/trace-receipt.json')
    closed(prefix, ('protocol', 'status', 'seed', 'requests', 'chain', 'blocks', 'scope'), 'prefix index')
    deps = packet.json('attempt5/prefix-dependencies.json')
    old_paths = verify_prefix(packet, prefix, deps, steps)
    expected = old_paths | {'attempt2/trace-receipt.json', 'attempt5/prefix-dependencies.json',
        'attempt5/request.json', 'attempt5/coin-record.raw.json', 'attempt5/terminal-observation.json',
        'execute-trace-attempt4.py', 'inputs/premine.csv', 'inputs/register.clvm',
        'inputs/LICENSE.md', 'inputs/provenance.json', 'attempt4/conclusions.json',
        'attempt4/source-cli-before.json', 'attempt4/source-cli-after.json',
        'attempt4/six-prefix-states.json', 'attempt4/execution-receipt.json',
        'attempt4/programs/' + FULL + '.clvm.hex'}
    for index in range(247):
        row = r['http_requests'][index]
        value = http_row(packet, row)
        endpoint = row['url'].rsplit('/', 1)[-1]
        require(row['path'] == f'attempt4/http/{index:03}-{endpoint}.json', 'HTTP ordered filename')
        expected.add(row['path'])
        if index == 0:
            require(row['request'] == {} and provider_network(value), 'provider network declaration')
    for index in range(129):
        stem = f'attempt4/steps/{index:03}'
        expected.update(stem+'.'+suffix for suffix in ('command.json', 'result-meta.json',
            'stdout.json', 'stderr.log', 'solution.hex', 'solution.asm'))
    require(set(packet.rows) == expected, 'closed packet semantic inventory before decoding')
    costs, all_names = [], []
    previous = None
    for index, step in enumerate(steps):
        closed(step, ('index', 'coin_id', 'record', 'source_spend_response', 'program_identity',
            'solution_identity', 'handles', 'action_count', 'state_tree_hash',
            'allowed_action_merkle_root', 'action_module_hashes', 'primary_cost',
            'next_coin', 'next_coin_id', 'next_source'), 'step')
        require(integer(step['index'], 128) == index, 'step index')
        rec = record(step['record'])
        require(rec['coin']['amount'] == 1 and coin_id(rec['coin']) == step['coin_id'], 'step coin identity')
        if previous:
            require(previous['next_coin'] == rec['coin'] and previous['next_coin_id'] == step['coin_id']
                    and rec['confirmed_block_index'] == previous['record']['spent_block_index'], 'internal successor/height edge')
        else:
            require(step['coin_id'] == FIRST and rec['coin']['parent_coin_info'] == '0x' + EVE, 'first executed controller')
        spend_path = relative(step['source_spend_response'])
        spend = success(packet.json(spend_path), 'coin_solution')
        closed(spend, ('coin', 'puzzle_reveal', 'solution'), 'spend')
        require(spend['coin'] == rec['coin'], 'spend full coin')
        if index >= 6:
            qr, qs = r['http_requests'][2*(index-6)+1:2*(index-6)+3]
            require(qr['url'].endswith('/get_coin_record_by_name') and qr['request'] == {'name': '0x'+step['coin_id']}
                    and success(packet.json(qr['path']), 'coin_record') == rec, 'point record exact relation')
            require(qs['url'].endswith('/get_puzzle_and_solution') and qs['path'] == spend_path
                    and qs['request'] == {'coin_id': '0x'+step['coin_id'], 'height': rec['spent_block_index']}, 'spend request exact relation')
        program, solution = unhex(spend['puzzle_reveal']), unhex(spend['solution'])
        identity_check(program, step['program_identity'])
        identity_check(solution, step['solution_identity'])
        require(step['program_identity']['module_tree_hash'] == FULL
                and rec['coin']['puzzle_hash'] == '0x'+FULL, 'fixed full reveal')
        computed = check_actions(program, solution, step, register_raw)
        require(all(x in name_set for x in computed), 'CSV name membership')
        all_names.extend(computed)
        stem = f'attempt4/steps/{index:03}'
        paths = {suffix: stem+'.'+suffix for suffix in ('command.json', 'result-meta.json',
            'stdout.json', 'stderr.log', 'solution.hex', 'solution.asm')}
        expected.update(paths.values())
        command, meta = packet.json(paths['command.json']), packet.json(paths['result-meta.json'])
        stdout, stderr = packet.raw(paths['stdout.json']), packet.raw(paths['stderr.log'])
        execution = json_value(stdout)
        closed(execution, ('outcome', 'cost', 'error'), 'successful VM envelope')
        closed(execution['outcome'], ('status', 'output_hex'), 'successful VM outcome')
        require(execution['outcome']['status'] == 'accepted' and execution['error'] is None, 'accepted primary observation only')
        cost = integer(execution['cost'], MAX_COST, 1)
        require(integer(step['primary_cost'], MAX_COST, 1) == cost, 'actual stdout cost relation')
        costs.append(cost)
        closed(meta, ('exit_code', 'stdout_sha256', 'stderr_sha256'), 'result meta')
        require(integer(meta['exit_code'], 0) == 0 and meta['stdout_sha256'] == sha(stdout)
                and meta['stderr_sha256'] == sha(stderr), 'exit/log hash relation')
        closed(command, ('argv', 'cwd', 'max_cost', 'input_environment_hex', 'program', 'scope'), 'prewritten command')
        asm = assembly(solution)
        program_name = 'attempt4/programs/'+FULL+'.clvm.hex'
        # Compare historical origin strings only; never open/execute them.
        historical_base = collector_origins[0]['path'].rsplit('/', 1)[0]
        require(command['argv'] == [origin, 'run', historical_base+'/'+program_name, '--solution', asm, '--max-cost', str(MAX_COST)]
                and command['cwd'] == recorded_root and integer(command['max_cost']) == MAX_COST
                and command['input_environment_hex'] == solution.hex()
                and command['program'] == inspect(program), 'exact command/env/program relation')
        require(packet.raw(paths['solution.hex']) == solution.hex().encode()+b'\n'
                and packet.raw(paths['solution.asm']) == asm.encode()+b'\n'
                and packet.raw(program_name) == program.hex().encode()+b'\n', 'retained exact artifact/assembly')
        next_coin = output_successor(unhex(execution['outcome']['output_hex']), rec,
                                     step['coin_id'], len(computed))
        require(next_coin == step['next_coin'] and coin_id(next_coin) == step['next_coin_id'], 'executed output successor identity')
        previous = step
    require(len(all_names) == len(set(all_names)) == 3836 and set(all_names) == name_set
            and r['covered_names'] == sorted(all_names) and r['missing_names'] == [], 'exact global name coverage')
    require(collections.Counter(len(s['handles']) for s in steps) == {1: 1, 30: 127, 25: 1}
            and sum(costs) == 5302597203 and max(costs) == 41838769, 'recomputed count/cost profile')
    prefix_six = packet.json('attempt4/six-prefix-states.json')
    closed(prefix_six, ('status', 'steps', 'scope'), 'six prefix checkpoint')
    require(prefix_six['status'] == 'six_primary_components_executed' and prefix_six['steps'] == steps[:6], 'six checkpoint exact relation')
    verify_terminal(packet, r)
    require(set(packet.rows) == expected, 'closed packet semantic inventory')
    originals = [n for n, row in packet.rows.items() if row['role'] == 'original_execution']
    require(len(originals) == 1026, 'original1026 execution files distinct from later extras')
    return dict(protocol=PROFILE, status='retained_consistency_passed', primary_component_observations=129,
        new_reader_executions=0, exact_csv_handle_names=3836, internal_successor_edges=128,
        earlier_dependency_files=19, earlier_index_request_descriptors=39,
        earlier_unretained_uncredited_descriptors=20, bootstrap_child_lists_complete=False,
        collector_http_requests=247, later_terminal_point_requests=1,
        original_execution_files=1026, physical_files=len(packet.physical),
        physical_bytes=sum(v[2] for v in packet.physical.values()),
        primary_cost_sum=sum(costs), primary_cost_max=max(costs), scope=SCOPE)



def output_successor(raw, rec, current_id, action_count):
    conditions = [fields(c, 32) for c in fields(decode(raw))]
    require(all(c and type(c[0]) is bytes for c in conditions), 'condition shape')
    require([c for c in conditions if c[0] == b'G'] == [[b'G', unhex(rec['coin']['parent_coin_info'], 32)]]
            and [c for c in conditions if c[0] == b'I'] == [[b'I', b'\1']], 'output parent/amount assertions')
    creates = [c for c in conditions if c[0] == b'3']
    require(all(len(c) == 4 and type(c[1]) is bytes and len(c[1]) == 32
                and c[2] in (b'', b'\1') for c in creates), 'closed canonical CREATE amounts/shape')
    controllers = [c for c in creates if c[2] == b'\1']
    require(len(controllers) == 1 and len(creates) == action_count * 3 + 1,
            'one controller and three zero-mojo creations per action')
    return dict(parent_coin_info='0x'+current_id,
                puzzle_hash='0x'+controllers[0][1].hex(), amount=1)


def verify_terminal(packet, receipt):
    terminal = packet.json('attempt5/terminal-observation.json')
    closed(terminal, ('protocol', 'status', 'attempt4_receipt', 'step_index', 'coin_id',
        'expected_coin', 'expected_confirmed_height', 'record', 'request', 'raw_response',
        'http_status', 'started_at', 'observed_at', 'elapsed_seconds', 'scope'), 'terminal observation')
    last = receipt['steps'][-1]
    require(terminal['protocol'] == 'chialisp-oxide-xchandles15-terminal-point/v1'
            and terminal['status'] == 'exact_derived_coin_and_creation_height_match'
            and integer(terminal['step_index'], 128) == 128, 'terminal observation status/index')
    require(terminal['attempt4_receipt'] == {'path': 'attempt4/execution-receipt.json',
            **ident(packet.raw('attempt4/execution-receipt.json'))}, 'terminal original receipt binding')
    require(terminal['coin_id'] == last['next_coin_id'] and terminal['expected_coin'] == last['next_coin']
            and integer(terminal['expected_confirmed_height']) == last['record']['spent_block_index'], 'terminal expected successor')
    rec = record(terminal['record'])
    require(rec['coin'] == last['next_coin'] and coin_id(rec['coin']) == last['next_coin_id']
            and rec['confirmed_block_index'] == last['record']['spent_block_index'] == 9175930
            and rec['spent_block_index'] == 9176142, 'terminal point exact frontier')
    for key, name in (('request', 'attempt5/request.json'), ('raw_response', 'attempt5/coin-record.raw.json')):
        require(terminal[key] == {'path': name, **ident(packet.raw(name))}, 'terminal file identity')
    require(success(packet.json('attempt5/coin-record.raw.json'), 'coin_record') == rec, 'terminal raw point record')
    request = packet.json('attempt5/request.json')
    closed(request, ('url', 'body', 'maximum_body_bytes', 'read_timeout_seconds',
        'elapsed_guard_seconds', 'redirects', 'proxies', 'retries', 'scope'), 'terminal request')
    require(integer(request['maximum_body_bytes']) == 65536
            and integer(request['read_timeout_seconds']) == 15
            and integer(request['elapsed_guard_seconds']) == 25
            and integer(terminal['http_status']) == 200, 'terminal transport bounds')
    require(request['url'] == 'https://api.coinset.org/get_coin_record_by_name'
            and request['body'] == {'name': '0x'+last['next_coin_id']}
            and request['redirects'] is False and request['proxies'] is False
            and integer(request['retries'], 0) == 0, 'terminal bounded point request')


def provenance_record():
    return dict(source_repository='Yakuhito/slot-machine',
        source_commit='6edc94856306d0f77e065c6c99aacf1055e51e2f',
        source_tree='7ceb5d708945ff039f3a7e217fbbfa6960840b7f',
        license='Prosperity Public License 3.0.0',
        source_path='rue-puzzles/actions/xchandles/register.rue',
        artifact_path='rue-puzzles/actions/xchandles/register.rue.hex',
        csv_path='xchandles_premine_mainnet.csv',
        scope='Pinned source/artifact correspondence only. Source snapshot postdates launch; no deployed-source authentication, unchanged Chialisp module, Rust frontend port or original teaching fixture claim.')


def build_packet(research, csv_path, license_path, destination):
    """Explicit caller input paths; offline copy of originals into a new packet.

    This wrapper is not a collector and grants zero new execution. Source paths
    come from caller arguments, never historical origin descriptors.
    """
    research, destination = Path(research), Path(destination)
    require(not destination.exists(), 'new packet destination')
    prefix = json_value(read_regular(research/'attempt5/prefix-dependencies.json', MAX_MEMBER))
    sources = {}
    pending, entries = [(research/'attempt4', 0)], 0
    while pending:
        directory, depth = pending.pop()
        require(depth <= 8, 'wrapper directory depth')
        with os.scandir(directory) as stream:
            for entry in stream:
                entries += 1
                require(entries <= 8192, 'wrapper directory entry bound')
                st = entry.stat(follow_symlinks=False)
                require(not stat.S_ISLNK(st.st_mode), 'wrapper source symlink')
                path = Path(entry.path)
                if stat.S_ISDIR(st.st_mode):
                    pending.append((path, depth+1))
                else:
                    require(stat.S_ISREG(st.st_mode) and st.st_nlink == 1
                            and st.st_size <= MAX_MEMBER and len(sources) < MAX_FILES, 'wrapper physical member')
                    sources[path.relative_to(research).as_posix()] = path
    require(len(sources) == 1027, 'original1026 plus separate conclusions')
    for row in prefix['files']:
        name = relative(row['path'])
        sources[name] = research/name
    for name in ('attempt2/trace-receipt.json', 'attempt5/prefix-dependencies.json',
                 'attempt5/request.json', 'attempt5/coin-record.raw.json',
                 'attempt5/terminal-observation.json', 'execute-trace-attempt4.py'):
        sources[name] = research/name
    sources['inputs/premine.csv'] = Path(csv_path)
    sources['inputs/LICENSE.md'] = Path(license_path)
    # Extract only from explicit raw spend input, then pin exact deployed bytes.
    spend = success(json_value(read_regular(research/'attempt2/08-get_puzzle_and_solution.json', MAX_MEMBER)), 'coin_solution')
    solution = fields(decode(unhex(spend['solution'])), 3)
    inner, _ = take(solution[2], 3)
    puzzle = fields(inner[0])[0]
    mod, _ = uncurry(puzzle)
    # Historical1478-byte module appears verbatim in canonical curry serialization.
    # Obtain bytes by bounded iterative serialization, independent of collector.
    register_raw = encode(mod)
    generated = {'inputs/register.clvm': register_raw,
                 'inputs/provenance.json': (json.dumps(provenance_record(), indent=2)+'\n').encode()}
    require(len(sources)+len(generated)+1 <= MAX_FILES, 'wrapper file count')
    total = 0
    for path in sources.values():
        st = path.lstat()
        require(stat.S_ISREG(st.st_mode) and st.st_nlink == 1 and st.st_size <= MAX_MEMBER, 'wrapper regular input')
        total += st.st_size
    require(total + sum(map(len, generated.values())) + MAX_MANIFEST <= MAX_TOTAL, 'wrapper aggregate precharge')
    destination.mkdir(parents=True)
    rows = []
    for name in sorted(set(sources) | set(generated)):
        relative(name)
        raw = generated[name] if name in generated else read_regular(sources[name], MAX_MEMBER)
        out = destination/name
        out.parent.mkdir(parents=True, exist_ok=True)
        with out.open('xb') as stream:
            stream.write(raw)
        rows.append(dict(path=name, role=role(name), **ident(raw)))
    manifest = dict(protocol='chialisp-oxide-history-packet/v1', profile=PROFILE, members=rows)
    raw = (json.dumps(manifest, indent=2)+'\n').encode()
    require(len(raw) <= MAX_MANIFEST, 'wrapper manifest bound')
    with (destination/MANIFEST).open('xb') as stream:
        stream.write(raw)
    return verify(destination)


def encode(tree):
    pending, output, size, nodes = [(tree, 1)], [], 0, 0
    while pending:
        item, depth = pending.pop()
        nodes += 1
        require(nodes <= 65536 and depth <= 512, 'encode shape bound')
        if type(item) is tuple:
            raw = b'\xff'
            pending.extend(((item[1], depth+1), (item[0], depth+1)))
        else:
            require(type(item) is bytes and len(item) <= 1048576, 'encode atom bound')
            n = len(item)
            if n == 1 and item[0] < 128:
                raw = item
            elif n < 64:
                raw = bytes([128+n]) + item
            elif n < 8192:
                raw = bytes([192+(n >> 8), n & 255]) + item
            elif n < 1048576:
                raw = bytes([224+(n >> 16), (n >> 8) & 255, n & 255]) + item
            else:
                raw = b'\xf0\x10\0\0' + item
        require(size + len(raw) <= 1048576, 'encode precharge')
        size += len(raw)
        output.append(raw)
    return b''.join(output)
