"""Bounded OFFLINE admission of the fixed #15 recorded native pilot.

This checker executes no producer, Cargo, VM, BLS or network operation. Hashes
and arithmetic check consistency of retained observations, not authentication.
Only the explicit install wrapper reads caller-selected input directories.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import struct
import tomllib

PROTOCOL = 'chialisp-oxide-xchandles-recorded-native-pilot/v1'
PILOT = 'chialisp-oxide-xchandles-canonical-host-pilot/v1'
MAX_FILES, MAX_BYTES, MAX_MEMBER, MAX_INDEX = 128, 16 << 20, 2 << 20, 512 << 10
PACKAGE = 'experiments/xchandles-premine/'
SOURCE_NAMES = ['src/main.rs', 'src/launch.rs', 'src/controls.rs', 'src/metrics.rs',
                'src/journal.rs', 'Cargo.toml', 'Cargo.lock', 'fixtures/register.rue.hex']
PACKAGE_NAMES = SOURCE_NAMES + ['README.md', 'fixtures/SLOT-MACHINE-LICENSE.md']
BASELINE_LOCK_SHA = '5ad90dd3c61f4377b67a522fefbfbdaca1214ce84fc9a031e4213ec6ec1904e7'
DIRECT_DEPENDENCIES = {'anyhow': '=1.0.104', 'chia-sdk-driver': {'version': '=0.36.0', 'features': ['action-layer']}, 'chia-sdk-test': {'version': '=0.36.0', 'default-features': False}, 'chia-sdk-types': {'version': '=0.36.0', 'features': ['action-layer']}, 'chia-bls': '=0.36.1', 'chia-protocol': '=0.36.1', 'chia-consensus': '=0.36.1', 'chia-puzzle-types': '=0.36.1', 'chia-puzzles': '=0.20.3', 'clvm-utils': '=0.36.1', 'clvm-traits': '=0.36.1', 'clvmr': '=0.16.2', 'hex': '=0.4.3', 'serde_json': '=1.0.154', 'serde': '=1.0.229', 'libc': '=0.2.190', 'sha2': '=0.10.9'}
DRIVER_SHA = '33d22cc417877a290eff196fe851cfc1175a5432166f8d9bedeb9f55e010bc76'
NEW_DEPS = {'chia-sdk-driver': ('0.36.0', DRIVER_SHA), 'bigdecimal':
            ('0.4.11', '8fa3f3d8cbf4dffcfe4991de61d012bef509a409ecbe9dd41049bfe32b4d4653')}
REGISTER = '150ae295efe60fa049b87cff5c05c0031fa768959ba466b425ef5a58b4a95ee8'
RECORDED_COMPONENT_COST_PROFILE = [[332007, 18098], [11932, 1239312, 36572, 11932, 27332, 18311, 18092, 46120, 51668, 36572], [587937], [772480, 41593, 41593, 15032, 1324130, 3403935], [587937], [587937], [772480, 41593, 41593, 772480, 41593, 41593, 24272, 1346701, 4719404], [587937], [587937], [587937], [772480, 41593, 41593, 772480, 41593, 41593, 772480, 41593, 41593, 33512, 1369272, 6044349]]
# Profile consistency only; no instruction-cost derivation by this reader.

# Module identity pins from chia-sdk-types0.36.0 action_layer schemas.
# Curry arguments and the full allowed-root are derived below, not copied from
# the report's claimed root/proof. These host SDK pins are not source-auth proofs.
ACTION_MODULES = {
    'expire': 'ee1554255f27b737b73dbba2d3fc2e3d7e8de2954c35c8660341362410d5b4e0',
    'extend': '490522c6759f840a455ff5ca5a00f3eb83db2dfd3c4590553d7ff67d210bc353',
    'oracle': 'f6f82232ff1dead535602b757dcbe21ee6efb38ff01a0500e59c52161a582c1b',
    'register': REGISTER,
    'initiate_update': '23eeec5ecd731c2822adf82f23a2e73fdd4ad08df56558ad15873c68cadf68f8',
    'execute_update': '5ab577e34135251486330c53c8c16ed2b50d1b3da58096019c745b7faa94a631',
    'refund': 'a11cc9b151475ef6cc4b6e1846089dcf9c042413b7a5a943e6b11c4e0f59fcc7',
    'delegated': '491ee66c8ffd29ef41bf820d3d4d766deb1b6f650932af09f26f807dab1a362f'}
FINALIZER_MODULE = '9d274de59aeaa128d1b0a802b52911838420ef9720d605c78942e8dadfc0810b'
OFFER_MODULE = 'cfbfdeed5c4ca2de3d0bf520b9cb4bb7743a359bd2e6a188d19ce7dffc21d3e7'

HANDLES = [['pilotalpha'], ['pilotbeta', 'pilotgamma'], ['pilotdelta', 'pilotepsilon', 'pilotzeta']]
REFUSALS = ['changed-coin', 'duplicate-escaped-nested', 'duplicate-nested', 'duplicate-root',
            'fifo', 'malformed', 'old-source-checkpoint', 'oversized', 'symlink', 'unknown-field', 'wrong-recipe']
RUNS = ['format', 'native-tests', 'clippy', 'binary-build', 'full-first', 'full-repeat',
        'pause-first', 'resume-first'] + ['refuse-' + x for x in REFUSALS]
NEGATIVE_INPUTS = ['checkpoint-' + x + '.json' for x in REFUSALS
                   if x not in ('fifo', 'symlink', 'old-source-checkpoint')]
CAPTURE_NAMES = ['final-receipt.json'] + NEGATIVE_INPUTS + [r + s for r in RUNS
                                                         for s in ('.command.json', '.stdout', '.stderr')]
LIMITATIONS = ('Recorded VM outputs/instruction costs, SDK/BLS admissions, full in-memory simulator equality and process exits are producer observations. '
               'Offline derivations provide no replay, execution authentication, complete build-graph/source authenticity, compiler preservation, live-wallet or mainnet credit.')


def require(ok, message):
    if not ok:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def integer(value, lo=0, hi=(1 << 64) - 1):
    require(type(value) is int and lo <= value <= hi, 'strict bounded integer')
    return value


def keys(obj, names):
    require(type(obj) is dict and set(obj) == set(names), 'closed object fields: ' + ','.join(names))


def path_name(name):
    require(type(name) is str and len(name) <= 180 and re.fullmatch(r'[A-Za-z0-9_./-]+', name)
            and not name.startswith('/') and all(x not in ('', '.', '..') for x in name.split('/')),
            'closed portable relative path')
    return name


def read_file(path, limit=MAX_MEMBER, expected_size=None):
    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
    try:
        info = os.fstat(fd)
        require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and info.st_size <= limit,
                'bounded regular unaliased descriptor')
        require(expected_size is None or info.st_size == expected_size, 'descriptor differs from precharged size')
        result = bytearray()
        while len(result) <= info.st_size:
            chunk = os.read(fd, min(65536, info.st_size + 1 - len(result)))
            if not chunk:
                break
            result.extend(chunk)
        require(len(result) == info.st_size, 'descriptor size changed')
        return bytes(result)
    finally:
        os.close(fd)


def inventory(base):
    """Precharge ALL files before reading; count directories/entries as work."""
    require(base.is_dir() and not base.is_symlink(), 'regular packet directory')
    out, pending, entries, dirs, total = {}, [(base, '', 0)], 0, 0, 0
    while pending:
        directory, prefix, depth = pending.pop()
        require(depth <= 6, 'packet directory depth')
        dirs += 1
        require(dirs <= 128, 'packet directory work')
        with os.scandir(directory) as scan:
            for item in scan:
                entries += 1
                require(entries <= 512, 'packet entry work')
                name = path_name(prefix + item.name)
                info = item.stat(follow_symlinks=False)
                if stat.S_ISDIR(info.st_mode):
                    pending.append((Path(item.path), name + '/', depth + 1))
                else:
                    require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1, 'no special/link/alias packet members')
                    require(len(out) < MAX_FILES and info.st_size <= (MAX_INDEX if name == 'report.json' else MAX_MEMBER),
                            'packet file count/member bounds')
                    require(info.st_size <= MAX_BYTES - total, 'packet aggregate precharge')
                    total += info.st_size
                    out[name] = info.st_size
    return out


def json_read(data, limit=MAX_MEMBER):
    """Lexical depth/token bounds BEFORE the standard parser allocates values."""
    require(len(data) <= limit, 'JSON bytes')
    quoted, escaped, depth, tokens = False, False, 0, 0
    for byte in data:
        if quoted:
            if escaped:
                escaped = False
            elif byte == 92:
                escaped = True
            elif byte == 34:
                quoted = False
        elif byte == 34:
            quoted = True
            tokens += 1
        elif byte in (91, 123):
            depth += 1
            tokens += 1
            require(depth <= 96, 'JSON depth')
        elif byte in (93, 125):
            depth -= 1
        elif byte in (44, 58):
            tokens += 1
        require(tokens <= 180000, 'JSON structural work')
    def unique(rows):
        result = {}
        for key, val in rows:
            require(key not in result, 'duplicate decoded JSON key')
            result[key] = val
        return result
    def number(s):
        require(len(s) <= 20, 'JSON integer width')
        return int(s)
    def no_float(_):
        raise ValueError('noninteger JSON number')
    return json.loads(data, object_pairs_hook=unique, parse_int=number,
                      parse_float=no_float, parse_constant=no_float)


def blob(value, size=None):
    require(type(value) is str and len(value) <= 131072 and len(value) % 2 == 0
            and re.fullmatch('[0-9a-f]*', value) is not None, 'bounded lowercase hex')
    if size is not None:
        require(len(value) == size * 2, 'fixed byte width')
    return bytes.fromhex(value)


def atomint(n):
    integer(n)
    if not n:
        return b''
    raw = n.to_bytes((n.bit_length() + 7) // 8, 'big')
    return b'\0' + raw if raw[0] & 128 else raw


def decode(data):
    """Plain-only iterative codec. First scan charges nodes/depth/atom bytes;
    only a successful whole scan may construct any logical tree nodes."""
    require(len(data) <= 65536, 'CLVM blob bytes')
    pos, pending, nodes, atoms, ranges = 0, [0], 0, 0, []
    while pending:
        depth = pending.pop()
        nodes += 1
        require(nodes <= 65536 and depth <= 160 and pos < len(data), 'CLVM work/depth/truncation')
        head = data[pos]
        pos += 1
        if head == 255:
            pending.extend((depth + 1, depth + 1))
            ranges.append(None)
            continue
        if head < 128:
            start, end = pos - 1, pos
        elif head == 128:
            start = end = pos
        else:
            count = 0
            while head & (128 >> count):
                count += 1
            require(count <= 5 and pos + count - 1 <= len(data), 'CLVM backref/length prefix')
            size = head & ((1 << (7 - count)) - 1)
            for x in data[pos:pos + count - 1]:
                size = (size << 8) | x
            pos += count - 1
            start, end = pos, pos + size
            require(end <= len(data), 'CLVM atom truncation')
            pos = end
        atoms += end - start
        require(atoms <= 65536, 'CLVM atom budget')
        ranges.append((start, end))
    require(pos == len(data), 'CLVM trailing bytes')
    stack = []
    for item in reversed(ranges):
        if item is None:
            stack.append((stack.pop(), stack.pop()))
        else:
            stack.append(data[item[0]:item[1]])
    return stack[0]


def serialize(tree):
    parts, pending = [], [tree]
    while pending:
        x = pending.pop()
        if type(x) is tuple:
            parts.append(b'\xff')
            pending.extend((x[1], x[0]))
        elif not x:
            parts.append(b'\x80')
        elif len(x) == 1 and x[0] < 128:
            parts.append(x)
        else:
            n = len(x)
            for count in range(1, 6):
                if n < 1 << (7 - count + 8 * (count - 1)):
                    prefix = ((255 << (8 - count)) & 255) | (n >> (8 * (count - 1)))
                    parts.extend((bytes([prefix]) + n.to_bytes(count, 'big')[1:], x))
                    break
            else:
                raise ValueError('CLVM output atom bound')
    return b''.join(parts)


def th(tree):
    pending, vals = [(tree, False)], []
    while pending:
        x, done = pending.pop()
        if type(x) is bytes:
            vals.append(hashlib.sha256(b'\1' + x).digest())
        elif done:
            right, left = vals.pop(), vals.pop()
            vals.append(hashlib.sha256(b'\2' + left + right).digest())
        else:
            pending.extend(((x, True), (x[1], False), (x[0], False)))
    return vals[0]


def list_tree(items, tail=b''):
    for item in reversed(items):
        tail = item, tail
    return tail


def proper(tree, maximum=256):
    items = []
    while type(tree) is tuple:
        require(len(items) < maximum, 'proper list work')
        item, tree = tree
        items.append(item)
    require(tree == b'', 'proper list tail')
    return items


def uint(tree):
    require(type(tree) is bytes and len(tree) <= 9, 'amount atom')
    val = int.from_bytes(tree, 'big', signed=True) if tree else 0
    require(0 <= val < 1 << 64 and atomint(val) == tree, 'canonical u64 atom')
    return val


def coin(c):
    keys(c, ['amount', 'coin_id', 'parent_coin_id', 'puzzle_hash'])
    parent, puzzle = blob(c['parent_coin_id'], 32), blob(c['puzzle_hash'], 32)
    ident = sha(parent + puzzle + atomint(c['amount']))
    require(c['coin_id'] == ident, 'derived coin ID')
    return ident


def curry_hash(module_hash, args):
    ah = lambda b: hashlib.sha256(b'\1' + b).digest()
    ph = lambda a, b: hashlib.sha256(b'\2' + a + b).digest()
    def lh(xs):
        tail = ah(b'')
        for x in reversed(xs):
            tail = ph(x, tail)
        return tail
    quoted = ah(b'\1')
    env = quoted
    for arg in reversed(args):
        env = lh([ah(b'\4'), ph(quoted, arg), env])
    return lh([ah(b'\2'), ph(quoted, module_hash), env])


def descriptor(name, data):
    return {'path': name, 'bytes': len(data), 'sha256': sha(data)}

# Closed canonical SDK fixture identity profile; no execution authentication.
MODULES = [{'blob_sha256': 'f7a33ee12388d19c1eb96e47734803366572d94706dc6f24ab0cdd5776a6123f', 'bytes': 1478, 'name': 'register', 'tree_hash': '150ae295efe60fa049b87cff5c05c0031fa768959ba466b425ef5a58b4a95ee8'}, {'blob_sha256': '849e048c276a39f05b68f78ba2d3a3b5924f5e8b8944a7c8ca923119eb442a7a', 'bytes': 630, 'name': 'action_layer', 'tree_hash': 'afa03f2903f9e4a293523237799074b13ab2361f250df10de5df884b0b09a22b'}, {'blob_sha256': 'cd9da6f90ecc2f4cac326d94b19f2f58f13f4d96d71a1b9410c86a2f64130408', 'bytes': 311, 'name': 'slot', 'tree_hash': '8ab9f3b57a65d7f7a0810f79f7bc1e96bda680d16dd6f51d51e868e13fc7bbb3'}, {'blob_sha256': '89dd2573da138b84a5e257f9f481cd44c4df4996dfc0c8660dbc9f39878393bd', 'bytes': 343, 'name': 'precommit', 'tree_hash': '9b3be8822ce5583da2987336d59cf4bdded39d2747838249fd9afe0238cab6ea'}, {'blob_sha256': '02a2ef5ba5f6a9ea7dc286ca8906cb20f108344e37bdbfc549feab436da30adb', 'bytes': 217, 'name': 'default_cat_maker', 'tree_hash': 'd6693f7aacfeb7e6901cd71a00f5e4b318d73234fbffd5b4d84da2c1371b3181'}, {'blob_sha256': '433eea7868e3225ab2821856d81b8fb430dd05e6aeb66342c9665379e69f0301', 'bytes': 1672, 'name': 'cat2', 'tree_hash': '37bef360ee858133b69d595a906dc45d01af50379dad515eb9518abb7c1d2a7a'}, {'blob_sha256': '10f81c86b279ed9c31a79d40867c0390ddceacd78c025b6363706232328822c2', 'bytes': 967, 'name': 'singleton_top_v1_1', 'tree_hash': '7faa3253bfddd1e0decb0906b2dc6247bbc4cf608f58345d173adb63e8b47c9f'}, {'blob_sha256': 'bcaf4089ed7e750f746e8cc0d3c52e69742673edab4d954db4e03919263e7bcb', 'bytes': 175, 'name': 'singleton_launcher', 'tree_hash': 'eff07522495060c066f66f32acc2a77e3a3e737aca8baea4d1a64ea4cdc13da9'}, {'blob_sha256': '21a12e968451dcf266a1567c7562bbf2a6e84c87ab6449205d4f1dc01b9d4a8d', 'bytes': 827, 'name': 'nft_state', 'tree_hash': 'a04d9f57764f54a43e4030befb4d80026e870519aaa66334aef8304f5d0393c2'}, {'blob_sha256': '169f3df6b59e2a33424c0573ec5b96d4747fe8eaec844eadb9ec5433305669b4', 'bytes': 1226, 'name': 'nft_ownership', 'tree_hash': 'c5abea79afaa001b5427dfa0c8cf42ca6f38f5841b78f9b3c252733eb2de2726'}, {'blob_sha256': '21747ef070376012f22f7b426edaa36a244a77f8825d13d5529963b921ad1da7', 'bytes': 227, 'name': 'standard', 'tree_hash': 'e9aaa49f45bad5c889b86ee3341550c155cfdd10c3a6757de618d20612fffd52'}]


def module_scan(tree, seen):
    pending, hashes = [(tree, False)], []
    wanted = {x['tree_hash']: x for x in MODULES}
    while pending:
        x, done = pending.pop()
        if type(x) is bytes:
            digest = hashlib.sha256(b'\1' + x).digest()
        elif done:
            right, left = hashes.pop(), hashes.pop()
            digest = hashlib.sha256(b'\2' + left + right).digest()
        else:
            pending.extend(((x, True), (x[1], False), (x[0], False)))
            continue
        hashes.append(digest)
        ident = digest.hex()
        if ident in wanted and ident not in seen:
            raw = serialize(x)
            expected = wanted[ident]
            require(len(raw) == expected['bytes'] and sha(raw) == expected['blob_sha256'], 'canonical module retained bytes')
            seen.add(ident)


def check_metrics(m, spends, components, outputs, height):
    keys(m, ['api', 'byte_cost', 'charged_generator_bytes', 'condition_cost', 'cost_pass_validates_signature',
             'cost_per_byte', 'execution_cost', 'fee_mojos', 'flags', 'height', 'max_cost', 'plain_generator_bytes', 'scope', 'total_cost'])
    require(m['api'] == 'chia-consensus0.36.1/get_conditions_from_spendbundle'
            and m['cost_pass_validates_signature'] is False and m['flags'] == 720902
            and m['max_cost'] == 5500000000 and m['cost_per_byte'] == 12000 and m['height'] == height,
            'closed no-BLS cost profile')
    for name in ('byte_cost', 'charged_generator_bytes', 'condition_cost', 'cost_per_byte', 'execution_cost',
                 'fee_mojos', 'flags', 'height', 'max_cost', 'plain_generator_bytes', 'total_cost'):
        integer(m[name])
    # Pinned solution_generator::build_generator prepends spends, then quotes a
    # one-element outer list. Reconstruct actual canonical wire, not a size hint.
    items = [list_tree([blob(s['coin']['parent_coin_id'], 32), decode(blob(s['reveal_hex'])),
                       atomint(s['coin']['amount']), decode(blob(s['solution_hex']))]) for s in reversed(spends)]
    generator = serialize((b'\1', list_tree([list_tree(items)])))
    require(m['plain_generator_bytes'] == len(generator) and m['charged_generator_bytes'] == len(generator) - 2,
            'derived complete plain generator bytes')
    require(m['byte_cost'] == (len(generator) - 2) * 12000, 'derived byte charge')
    adds, condition_cost = 0, 0
    for output in outputs:
        for condition in proper(output):
            args = proper(condition)
            require(args and type(args[0]) is bytes, 'condition opcode atom')
            opcode = uint(args[0])
            require(opcode in {50, 51, 60, 61, 62, 63, 64, 66, 67, 70, 71, 73, 81, 82}, 'closed observed condition set')
            if opcode == 51:
                require(len(args) in (3, 4) and type(args[1]) is bytes and len(args[1]) == 32, 'CREATE_COIN wire')
                adds += uint(args[2])
                condition_cost += 1800000
            elif 43 <= opcode <= 50:
                require(len(args) == 3 and type(args[1]) is bytes and len(args[1]) == 48 and type(args[2]) is bytes,
                        'AGG_SIG wire')
                condition_cost += 1200000
    require(m['condition_cost'] == condition_cost, 'derived condition charges')
    removals = sum(s['coin']['amount'] for s in spends)
    require(removals >= adds and m['fee_mojos'] == removals - adds, 'derived conservation/fee')
    # get_conditions excludes the generator quote execution overhead; the fixed
    # fixture profile is an observation consistency check, not instruction replay.
    require(m['execution_cost'] == sum(c['cost'] for c in components), 'recorded execution cost relation')
    require(m['total_cost'] == m['execution_cost'] + condition_cost + m['byte_cost'] <= m['max_cost'], 'total cost arithmetic')
    require(type(m['scope']) is str and 'Not a full-block' in m['scope'], 'measurement scope')


def created_coins(parent, output):
    result = {}
    for condition in proper(output):
        args = proper(condition)
        if args and args[0] == b'3':
            require(len(args) in (3, 4), 'CREATE_COIN arity')
            c = {'parent_coin_id': parent, 'puzzle_hash': blob(args[1].hex(), 32).hex(), 'amount': uint(args[2])}
            c['coin_id'] = sha(bytes.fromhex(parent) + args[1] + atomint(c['amount']))
            require(c['coin_id'] not in result, 'duplicate child condition')
            result[c['coin_id']] = c
    return result


def slot_check(s, modules):
    keys(s, ['coin', 'creator_registry_coin', 'creator_parent_coin_id', 'creator_inner_puzzle_hash',
             'handle_hash', 'left_hash', 'right_hash', 'slot_value_tree_hash', 'slot_nonce', 'registry_launcher_id',
             'expiration', 'counter', 'owner_launcher_id', 'resolved_launcher_id'])
    coin(s['coin']); coin(s['creator_registry_coin'])
    raw = {k: blob(s[k], 32) for k in ('handle_hash', 'left_hash', 'right_hash', 'owner_launcher_id',
                                      'resolved_launcher_id', 'registry_launcher_id', 'creator_parent_coin_id', 'creator_inner_puzzle_hash')}
    value = (atomint(s['counter']), ((raw['handle_hash'], (raw['left_hash'], raw['right_hash'])),
                                    (atomint(s['expiration']), (raw['owner_launcher_id'], raw['resolved_launcher_id']))))
    require(th(value).hex() == s['slot_value_tree_hash'], 'derived slot value hash')
    singleton_struct = (modules['singleton_top_v1_1'], (raw['registry_launcher_id'], modules['singleton_launcher']))
    creator_hash = curry_hash(modules['singleton_top_v1_1'], [th(singleton_struct),
                                                            raw['creator_inner_puzzle_hash']])
    require(s['creator_registry_coin']['puzzle_hash'] == creator_hash.hex()
            and s['creator_registry_coin']['parent_coin_id'] == s['creator_parent_coin_id']
            and s['creator_registry_coin']['amount'] == 1
            and s['coin']['parent_coin_id'] == s['creator_registry_coin']['coin_id'], 'exact slot creator lineage')
    first = curry_hash(modules['slot'], [th((modules['singleton_top_v1_1'], th(singleton_struct))), th(atomint(s['slot_nonce']))])
    slot_hash = curry_hash(first, [th(blob(s['slot_value_tree_hash'], 32))])
    require(s['coin']['puzzle_hash'] == slot_hash.hex() and s['coin']['amount'] == 0 and s['slot_nonce'] == 1,
            'derived twice-curried slot identity')


def validate_report(report):
    keys(report, ['batch_sizes', 'canonical_modules', 'component_vm', 'component_vm_runs', 'consensus', 'network',
                  'protocol', 'recipe_fingerprint', 'register_module_blob_sha256', 'register_module_bytes',
                  'register_module_tree_hash', 'registration_steps', 'scope', 'sdk', 'sdk_transaction_attempts', 'status', 'transactions'])
    require(report['protocol'] == PILOT and report['status'] == 'executed_local_simulator'
            and report['sdk'] == 'chia-sdk-driver/test 0.36.0' and report['component_vm'] == 'clvmr 0.16.2'
            and report['consensus'] == 'chia-consensus 0.36.1' and report['network'] == 'TESTNET11', 'pilot profile')
    require(report['batch_sizes'] == [1, 2, 3] and all(type(x) is int for x in report['batch_sizes']), 'batch profile')
    require(report['canonical_modules'] == MODULES and report['register_module_tree_hash'] == REGISTER
            and type(report['register_module_bytes']) is int and report['register_module_bytes'] == 1478
            and report['register_module_blob_sha256'] == MODULES[0]['blob_sha256'], 'canonical module profile')
    require(integer(report['component_vm_runs']) == 48 and integer(report['sdk_transaction_attempts']) == 20, 'separate execution counts')
    txs = report['transactions']
    require(type(txs) is list and len(txs) == 20, 'complete transaction inventory')
    expected = ['issue_payment_cat', 'launch_registry_price_and_nft']
    for count in (1, 2, 3):
        expected += ['create_precommit'] * count + ['wrong_neighbors', 'invalid_signature', 'register_handle', 'consumed_registry_replay']
    require([t['name'] for t in txs] == expected, 'closed transaction order')
    seen, produced, consumed, accepted, registers, total_components = set(), {}, set(), [], [], 0
    available, timelines = {}, {}
    for index, t in enumerate(txs):
        name = t['name']
        if name in ('wrong_neighbors', 'invalid_signature', 'consumed_registry_replay'):
            fields = ['name', 'status', 'code', 'full_simulator_equality', 'component_vm']
            if name == 'wrong_neighbors':
                fields += ['semantic_consensus_rejection_credit']
            if name == 'invalid_signature':
                fields += ['input_coin_ids', 'signature_hex']
            keys(t, fields)
            require(t['full_simulator_equality'] is True, 'recorded full simulator equality')
            if name == 'wrong_neighbors':
                require(t['status'] == 'runtime_umbrella' and t['code'] == 'GeneratorRuntimeError'
                        and t['semantic_consensus_rejection_credit'] is False, 'runtime umbrella distinct')
                require(type(t['component_vm']) is list and len(t['component_vm']) == 1, 'one wrong-neighbor witness')
                witness = t['component_vm'][0]
                keys(witness, ['coin_id', 'solution_hex', 'failure'])
                require(witness['failure'] == {'protocol': 'clvmr-0.16.2/v1', 'kind': 'program', 'code': 'raise'},
                        'exact recorded program failure; resource inconclusive')
                target = txs[index + 2]['spends'][-1]
                require(witness['coin_id'] == target['coin']['coin_id'], 'witness current controller')
                changed = decode(blob(witness['solution_hex']))
                original = decode(blob(target['solution_hex']))
                require(changed != original, 'actual changed neighbor solution')
                # The exact changed paths are checked below by tree comparison.
                require(swapped_neighbors(original, changed), 'only first-action neighbors swapped')
                total_components += 1
            else:
                require(t['status'] == 'consensus_rejected' and t['code'] == ('BadAggregateSignature' if name == 'invalid_signature' else 'DoubleSpend')
                        and t['component_vm'] == [], 'exact typed SDK negative')
                if name == 'invalid_signature':
                    valid = txs[index + 1]
                    require(t['input_coin_ids'] == valid['input_coin_ids'] and blob(t['signature_hex'], 96) != blob(valid['signature_hex'], 96),
                            'actual differing nonempty signature input')
                    require(blob(t['signature_hex'], 96) != bytes(96), 'nonempty wrong signature')
            continue
        keys(t, ['name', 'status', 'component_vm', 'input_coin_ids', 'signature_hex', 'spendbundle_metrics', 'spends'])
        require(t['status'] == 'accepted', 'recorded SDK acceptance')
        spends, components = t['spends'], t['component_vm']
        require(type(spends) is list and 1 <= len(spends) <= 128 and type(components) is list
                and len(components) == len(spends), 'complete component/spend relation')
        require(t['input_coin_ids'] == [coin(s['coin']) for s in spends], 'exact spend order and coin IDs')
        blob(t['signature_hex'], 96)
        require([c['cost'] for c in components] == RECORDED_COMPONENT_COST_PROFILE[len(accepted)], 'fixed recorded component cost profile')
        outputs, local_created = [], {}
        byte_budget = 0
        for s, c in zip(spends, components):
            keys(s, ['coin', 'reveal_hex', 'solution_hex']); keys(c, ['coin', 'cost', 'output_hex'])
            require(c['coin'] == s['coin'], 'component coin vs spend')
            ident = coin(s['coin'])
            require(ident not in consumed, 'accepted double spend')
            if ident in produced:
                require(produced[ident] == s['coin'], 'retained actual parent-created input')
            consumed.add(ident)
            blobs = [blob(s['reveal_hex']), blob(s['solution_hex']), blob(c['output_hex'])]
            byte_budget += sum(map(len, blobs))
            require(byte_budget <= 4 << 20, 'aggregate CLVM precharge')
            puzzle, solution, output = [decode(b) for b in blobs]
            require(th(puzzle).hex() == s['coin']['puzzle_hash'], 'reveal hash vs actual input')
            require(integer(c['cost'], 1, 500000000), 'component cost bound')
            for tree in (puzzle, solution):
                module_scan(tree, seen)
            outputs.append(output)
            children = created_coins(ident, output)
            require(not set(children) & set(local_created), 'duplicate created coin IDs')
            local_created.update(children)
            produced.update(children)
        total_components += len(components)
        check_metrics(t['spendbundle_metrics'], spends, components, outputs, len(accepted))
        if name == 'register_handle':
            timelines[spends[-1]['coin']['coin_id']] = dict(available)
        available.update(local_created)
        for spend in spends:
            available.pop(spend['coin']['coin_id'], None)
        accepted.append((t, local_created, outputs))
        if name == 'register_handle':
            registers.append((t, local_created, outputs))
    require(len(accepted) == 11 and total_components == 48 and len(registers) == 3, 'actual retained lane counts')
    for t, _, _ in accepted:
        for spend in t['spends']:
            if spend['coin']['coin_id'] not in produced:
                mod, args = uncurry(decode(blob(spend['reveal_hex'])))
                require(th(mod).hex() == next(m['tree_hash'] for m in MODULES if m['name'] == 'standard') and len(args) == 1, 'external fixture funding inputs must be standard coins')
    require(seen == {m['tree_hash'] for m in MODULES}, 'all canonical counterpart blobs present')
    validate_steps(report['registration_steps'], registers, produced, consumed, timelines, accepted[1][0])
    return registers


def swapped_neighbors(original, changed):
    # Raw singleton/action-layer/action solution shape is validated through an
    # independent structural diff: exactly TWO atoms interchange, all other
    # nodes/atoms equal. Their values must be 32-byte neighbor hashes. A later
    # paths pin singleton.inner_solution / action.solutions[0] / neighbors.
    pending, diffs = [(original, changed, ())], []
    while pending:
        left, right, path = pending.pop()
        if type(left) is tuple and type(right) is tuple:
            pending.extend(((left[0], right[0], path + (0,)), (left[1], right[1], path + (1,))))
        elif type(left) is bytes and type(right) is bytes:
            if left != right:
                diffs.append((path, left, right))
        else:
            return False
    return (len(diffs) == 2 and {''.join('f' if n == 0 else 'r' for n in p) for p, _, _ in diffs} == {'rrfrrffrfr', 'rrfrrffrff'}
            and all(len(a) == len(b) == 32 for _, a, b in diffs)
            and diffs[0][1] == diffs[1][2] and diffs[0][2] == diffs[1][1])


def allowed_actions(modules, launcher, price_launcher):
    top, launch = modules['singleton_top_v1_1'], modules['singleton_launcher']
    structure = (top, (launcher, launch))
    def curried(name, args):
        return curry_hash(bytes.fromhex(ACTION_MODULES[name]), [th(x) for x in args])
    def slot(nonce):
        return curry_hash(modules['slot'], [th((top, th(structure))), th(atomint(nonce))])
    precommit = curry_hash(modules['precommit'], [th(top), th(th(structure)), th(atomint(1)), th(bytes([8]) * 32)])
    handle, update = slot(1), slot(2)
    register_args = [top, launch, precommit, handle]
    leaves = [curried('expire', register_args),
              curried('extend', [bytes.fromhex(OFFER_MODULE), bytes([8]) * 32, handle]),
              curried('oracle', [handle]), curried('register', register_args),
              curried('initiate_update', [top, launch, atomint(1), handle, update]),
              curried('execute_update', [top, launch, handle, update]),
              curried('refund', [precommit, handle]),
              curried('delegated', [top, th((top, (price_launcher, launch)))])]
    def merkle(xs):
        if len(xs) == 1:
            return hashlib.sha256(b'\1' + xs[0]).digest(), [(0, [])]
        half = (len(xs) + 1) // 2
        left, lp = merkle(xs[:half]); right, rp = merkle(xs[half:])
        return hashlib.sha256(b'\2' + left + right).digest(), (
            [(path, proof + [right]) for path, proof in lp] +
            [(path | (1 << len(proof)), proof + [left]) for path, proof in rp])
    root, proofs = merkle(leaves)
    first_finalizer = curry_hash(bytes.fromhex(FINALIZER_MODULE), [th(modules['action_layer']), th(launcher)])
    finalizer = curry_hash(first_finalizer, [th(first_finalizer)])
    return register_args, root, proofs[3], finalizer


def check_action_commitments(action_args, register_args, selectors, finalizer_solution, modules, launcher, price_launcher, batch_index):
    expected_args, root, (path, proof), finalizer = allowed_actions(modules, launcher, price_launcher)
    require(register_args == expected_args, 'derived precommit/SLOT first-curry arguments')
    require(th(action_args[0]) == finalizer and action_args[1] == root, 'complete derived allowed-action root/finalizer')
    expected_selectors = [(atomint(2), (atomint(path), list_tree(proof)))] + [(atomint(2), b'')] * batch_index
    require(selectors == list_tree(expected_selectors) and finalizer_solution == b'', 'exact register selector/proof and nil finalizer solution')


def price_launcher_from_launch(launch_tx, modules):
    candidates = []
    for spend in launch_tx['spends']:
        if spend['coin']['puzzle_hash'] != modules['singleton_launcher'].hex():
            continue
        ident = bytes.fromhex(coin(spend['coin']))
        structure = (modules['singleton_top_v1_1'], (ident, modules['singleton_launcher']))
        expected = curry_hash(modules['singleton_top_v1_1'], [th(structure), th(b'\1')])
        solution = proper(decode(blob(spend['solution_hex'])))
        if len(solution) == 3 and solution[:2] == [expected, atomint(1)]:
            candidates.append(ident)
    require(len(candidates) == 1, 'unique actual inner-1 price singleton launcher')
    return candidates[0]


def created_slot(template, changes, controller, inner_hash, modules):
    result = dict(template, **changes)
    result.update(creator_registry_coin=controller, creator_parent_coin_id=controller['parent_coin_id'],
                  creator_inner_puzzle_hash=inner_hash.hex())
    value = (atomint(result['counter']), ((blob(result['handle_hash'], 32), (blob(result['left_hash'], 32), blob(result['right_hash'], 32))),
             (atomint(result['expiration']), (blob(result['owner_launcher_id'], 32), blob(result['resolved_launcher_id'], 32)))))
    result['slot_value_tree_hash'] = th(value).hex()
    structure = (modules['singleton_top_v1_1'], (blob(result['registry_launcher_id'], 32), modules['singleton_launcher']))
    first = curry_hash(modules['slot'], [th((modules['singleton_top_v1_1'], th(structure))), th(atomint(1))])
    puzzle_hash = curry_hash(first, [th(blob(result['slot_value_tree_hash'], 32))]).hex()
    result['coin'] = {'parent_coin_id': controller['coin_id'], 'puzzle_hash': puzzle_hash, 'amount': 0,
                      'coin_id': sha(bytes.fromhex(controller['coin_id'] + puzzle_hash))}
    return result


def validate_steps(steps, registers, produced, consumed, timelines, launch_tx):
    require(type(steps) is list and len(steps) == 3, 'three registry states')
    modules = {m['name']: bytes.fromhex(m['tree_hash']) for m in MODULES}
    previous, state_hash, launcher, slots = None, None, None, None
    price_launcher = price_launcher_from_launch(launch_tx, modules)
    for index, (step, (tx, children, outputs)) in enumerate(zip(steps, registers)):
        keys(step, ['batch_size', 'complete_registry_children', 'current_registry', 'handles', 'neighbors',
                    'owner_resolved_nft', 'previous_registry', 'registry_state', 'registry_state_tree_hash', 'surviving_slots'])
        require(type(step['batch_size']) is int and step['batch_size'] == index + 1 and step['handles'] == HANDLES[index], 'fixed handle batches')
        current, old = step['current_registry'], step['previous_registry']
        coin(current); coin(old)
        require(tx['spends'][-1]['coin'] == old and current['parent_coin_id'] == old['coin_id']
                and old['amount'] == current['amount'] == 1, 'controller spend/current binding')
        require(previous is None or previous == old, 'successive current controller states')
        previous = current
        state = step['registry_state']
        keys(state, ['cat_maker_puzzle_hash', 'pricing_puzzle_hash', 'expired_handle_pricing_puzzle_hash'])
        h = th((blob(state['cat_maker_puzzle_hash'], 32), (blob(state['pricing_puzzle_hash'], 32),
                                                         blob(state['expired_handle_pricing_puzzle_hash'], 32)))).hex()
        require(h == step['registry_state_tree_hash'] and (state_hash is None or state_hash == h), 'derived constant registry state')
        state_hash = h
        # Bind state to actual action-layer reveal, rather than merely a reported hash.
        singleton = uncurry(decode(blob(tx['spends'][-1]['reveal_hex'])))
        require(th(singleton[0]) == modules['singleton_top_v1_1'] and len(singleton[1]) == 2, 'canonical controller singleton curry')
        singleton_struct, inner = singleton[1]
        require(type(singleton_struct) is tuple and singleton_struct[0] == modules['singleton_top_v1_1']
                and type(singleton_struct[1]) is tuple and singleton_struct[1][1] == modules['singleton_launcher'], 'singleton struct')
        actual_launcher = singleton_struct[1][0]
        require(type(actual_launcher) is bytes and len(actual_launcher) == 32 and (launcher is None or launcher == actual_launcher), 'registry launcher identity')
        launcher = actual_launcher
        action = uncurry(inner)
        require(th(action[0]) == modules['action_layer'] and len(action[1]) == 3 and th(action[1][-1]).hex() == h, 'actual action-layer state commitment')
        outer_solution = proper(decode(blob(tx['spends'][-1]['solution_hex'])))
        require(len(outer_solution) == 3 and uint(outer_solution[1]) == 1, 'actual singleton solution amount')
        raw_action, fields = outer_solution[2], []
        for _ in range(3):
            require(type(raw_action) is tuple, 'raw action-layer solution fields')
            field, raw_action = raw_action
            fields.append(field)
        puzzles, solutions = proper(fields[0]), proper(fields[2])
        require(len(puzzles) == 1 and len(solutions) == index + 1, 'one canonical register action with selected solution count')
        register_module, register_args = uncurry(puzzles[0])
        require(th(register_module).hex() == REGISTER and len(register_args) == 4
                and register_args[:2] == [modules['singleton_top_v1_1'], modules['singleton_launcher']], 'actual canonical register action')
        check_action_commitments(action[1], register_args, fields[1], raw_action, modules, launcher, price_launcher, index)
        action_values = []
        for solution, name in zip(solutions, HANDLES[index]):
            values, rest = [], solution
            for _ in range(7):
                require(type(rest) is tuple, 'register action solution fields')
                value, rest = rest
                values.append(value)
            require(values[0] == th(name.encode()) and type(values[1]) is tuple, 'actual action committed handle hash')
            require(type(values[2]) is tuple and th(values[2][0]).hex() == state['cat_maker_puzzle_hash']
                    and type(values[3]) is tuple and th(values[3][0]).hex() == state['pricing_puzzle_hash'], 'exact committed CAT-maker and pricing reveals')
            action_values.append(values)
        complete = step['complete_registry_children']
        require(type(complete) is list and len(complete) == 3 * (index + 1) + 1, 'complete ephemeral+surviving registry children')
        actual = {}
        for entry in complete:
            keys(entry, ['coin', 'created_height', 'spent_height'])
            ident = coin(entry['coin'])
            require(ident not in actual and entry['created_height'] == tx['spendbundle_metrics']['height'], 'child order/height')
            require(type(entry['created_height']) is int and (entry['spent_height'] is None or type(entry['spent_height']) is int), 'strict child heights')
            actual[ident] = entry['coin']
            require((entry['spent_height'] is not None) == (ident in {s['coin']['coin_id'] for s in tx['spends']}), 'actual ephemeral child consumption')
        controller_children = created_coins(old['coin_id'], outputs[-1])
        require(actual == controller_children and list(actual) == sorted(actual) and current['coin_id'] in actual
                and actual[current['coin_id']] == current, 'complete exact controller output child set')
        nft = step['owner_resolved_nft']
        keys(nft, ['coin', 'inner_puzzle_hash', 'launcher_id'])
        ident = coin(nft['coin'])
        require(ident in children and children[ident] == nft['coin'], 'actual newly created owner NFT coin')
        nft_spends = [s for s in tx['spends'] if s['coin']['puzzle_hash'] == nft['coin']['puzzle_hash']]
        require(len(nft_spends) == 1, 'actual owner NFT self-transfer spend')
        nft_outer = uncurry(decode(blob(nft_spends[0]['reveal_hex'])))
        require(th(nft_outer[0]) == modules['singleton_top_v1_1'] and len(nft_outer[1]) == 2
                and nft_outer[1][0] == (modules['singleton_top_v1_1'], (blob(nft['launcher_id'], 32), modules['singleton_launcher'])), 'NFT singleton launcher binding')
        nft_state = uncurry(nft_outer[1][1])
        require(th(nft_state[0]) == modules['nft_state'] and len(nft_state[1]) == 4, 'NFT state layer')
        nft_owner = uncurry(nft_state[1][-1])
        require(th(nft_owner[0]) == modules['nft_ownership'] and len(nft_owner[1]) == 4
                and th(nft_outer[1][1]).hex() == nft['inner_puzzle_hash'], 'actual NFT inner puzzle binding')
        blob(nft['inner_puzzle_hash'], 32); blob(nft['launcher_id'], 32)
        surviving = step['surviving_slots']
        require(type(surviving) is list and len(surviving) == 2 + sum(range(1, index + 2)), 'complete surviving slot count')
        for slot in surviving:
            slot_check(slot, modules)
            require(slot['registry_launcher_id'] == launcher.hex() and slot['coin']['coin_id'] in produced
                    and produced[slot['coin']['coin_id']] == slot['coin'], 'slot actual creation')
        hashes = [s['handle_hash'] for s in surviving]
        require(hashes == sorted(set(hashes)) and hashes[0] == '00' * 32 and hashes[-1] == 'ff' * 32, 'complete ordered sentinel slots')
        wanted_handles = sum(HANDLES[:index + 1], [])
        require(set(hashes[1:-1]) == {th(n.encode()).hex() for n in wanted_handles}, 'complete fixed names')
        for i, slot in enumerate(surviving):
            require(slot['left_hash'] == hashes[max(0, i - 1)] and slot['right_hash'] == hashes[min(len(hashes) - 1, i + 1)], 'linked slot neighbors')
            if i not in (0, len(hashes) - 1):
                require(slot['owner_launcher_id'] == slot['resolved_launcher_id'] == nft['launcher_id'], 'real NFT owner/resolved binding')
        require(type(step['neighbors']) is list and len(step['neighbors']) == index + 1, 'per-action neighbor records')
        selected_spends = {s['coin']['coin_id']: s for s in tx['spends']}
        slot_inputs = {}
        for ident, spend in selected_spends.items():
            try:
                once, slot_args = uncurry(decode(blob(spend['reveal_hex'])))
                mod, first_args = uncurry(once)
            except ValueError:
                continue
            if th(mod) == modules['slot']:
                slot_inputs[ident] = (spend, slot_args, first_args)
        selected_neighbor_ids = set()
        if slots is None:
            first = step['neighbors'][0]
            live_slots = {first[side]['handle_hash']: first[side] for side in ('left', 'right')}
            require(set(live_slots) == {'00' * 32, 'ff' * 32}
                    and all(x['counter'] == 0 and x['expiration'] == (1 << 64) - 1
                            and x['owner_launcher_id'] == x['resolved_launcher_id'] == '00' * 32 for x in live_slots.values()), 'initial live sentinel values')
        else:
            live_slots = {x['handle_hash']: x for x in slots}
        for neighbor, name, values in zip(step['neighbors'], HANDLES[index], action_values):
            keys(neighbor, ['handle', 'left', 'right'])
            require(neighbor['handle'] == name, 'neighbor handle')
            for s in (neighbor['left'], neighbor['right']):
                slot_check(s, modules)
                require(live_slots.get(s['handle_hash']) == s, 'neighbor exact currently available action slot version')
                ident = s['coin']['coin_id']
                require(ident in slot_inputs and ident not in selected_neighbor_ids, 'neighbor must be exact selected unspent slot version')
                selected_neighbor_ids.add(ident)
                spend, slot_args, first_args = slot_inputs[ident]
                require(spend['coin'] == s['coin'] and slot_args == [blob(s['slot_value_tree_hash'], 32)]
                        and first_args == [(modules['singleton_top_v1_1'], th(singleton_struct)), atomint(1)], 'neighbor exact coin/reveal/full value commitment')
                require((ident in timelines[old['coin_id']] and timelines[old['coin_id']][ident] == s['coin'])
                        or (ident in controller_children and controller_children[ident] == s['coin']), 'neighbor availability before transaction or genuine current ephemeral')
                solution = decode(blob(spend['solution_hex']))
                expected_solution = ((blob(s['creator_parent_coin_id'], 32), (blob(s['creator_inner_puzzle_hash'], 32), atomint(1))), th(inner))
                require(solution == expected_solution, 'neighbor exact lineage/current-controller solution')
            require(values[1] == (blob(neighbor['left']['handle_hash'], 32), blob(neighbor['right']['handle_hash'], 32)), 'actual action vs recorded neighbors')
            for value, side, outer_side in ((values[4], 'left', 'left_hash'), (values[5], 'right', 'right_hash')):
                rec = neighbor[side]
                expected_rest = (atomint(rec['counter']), (blob(rec[outer_side], 32), (atomint(rec['expiration']), (blob(rec['owner_launcher_id'], 32), blob(rec['resolved_launcher_id'], 32)))))
                require(value == expected_rest, 'actual action full neighbor slot value')
            digest = th(name.encode()).hex()
            require(neighbor['left']['handle_hash'] < digest < neighbor['right']['handle_hash'], 'correct registration interval')
            price_module, price_args = uncurry(values[3][0])
            require(price_args == [atomint(2000), atomint(366 * 24 * 60 * 60)], 'fixed factor-pricing arguments')
            price_solution = values[3][1]
            require(type(price_solution) is tuple and type(price_solution[1]) is tuple and type(price_solution[1][1]) is tuple, 'factor-pricing solution shape')
            buy, (expiration, (handle, periods)) = price_solution
            require(uint(expiration) == 0 and handle == name.encode() and uint(periods) == 1, 'fixed registration pricing terms')
            left, right = neighbor['left'], neighbor['right']
            created = [
                created_slot(left, {'counter': left['counter'] + 1, 'right_hash': digest}, old, th(inner), modules),
                created_slot(left, {'counter': 0, 'handle_hash': digest, 'left_hash': left['handle_hash'],
                                   'right_hash': right['handle_hash'], 'expiration': uint(buy) + 366 * 24 * 60 * 60,
                                   'owner_launcher_id': nft['launcher_id'], 'resolved_launcher_id': nft['launcher_id']}, old, th(inner), modules),
                created_slot(right, {'counter': right['counter'] + 1, 'left_hash': digest}, old, th(inner), modules)]
            for new in created:
                require(controller_children.get(new['coin']['coin_id']) == new['coin'], 'derived action-created slot in exact controller outputs')
                live_slots[new['handle_hash']] = new

        require(selected_neighbor_ids == set(slot_inputs), 'complete selected neighbor SLOT spend inventory')
        require(surviving == [live_slots[k] for k in sorted(live_slots)], 'complete final slots equal action-ordered unspent versions')
        slots = surviving
    return slots


def uncurry(tree):
    args = proper(tree)
    require(len(args) == 3 and args[0] == b'\2' and type(args[1]) is tuple and args[1][0] == b'\1', 'canonical curry apply')
    module, env = args[1][1], args[2]
    result = []
    while env != b'\1':
        items = proper(env)
        require(len(result) < 16 and len(items) == 3 and items[0] == b'\4'
                and type(items[1]) is tuple and items[1][0] == b'\1', 'canonical curry cons')
        result.append(items[1][1])
        env = items[2]
    return module, result


def input_roles():
    result = {'source/' + n: 'source/' + n for n in PACKAGE_NAMES}
    result.update({'capture/' + n: 'capture/' + n for n in CAPTURE_NAMES})
    result.update({x: x for x in ['reference/root-Cargo.lock', 'reference/driver.crate',
                                 'reference/build-settings.json', 'reference/old-checkpoint.json']})
    return result


def load_packet(base):
    base = Path(base)
    entries = inventory(base)
    require('report.json' in entries, 'packet index required')
    raw = read_file(base / 'report.json', MAX_INDEX, entries['report.json'])
    index = json_read(raw, MAX_INDEX)
    keys(index, ['protocol', 'scope', 'inputs', 'files'])
    require(index['protocol'] == PROTOCOL and index['scope'] == LIMITATIONS, 'recorded scope/protocol')
    require(index['inputs'] == input_roles(), 'explicit closed wrapper inputs')
    rows = index['files']
    require(type(rows) is list and len(rows) < MAX_FILES and len(rows) == len(entries) - 1, 'exact packet file inventory')
    names, reserved = [], 0
    for row in rows:
        keys(row, ['path', 'bytes', 'sha256'])
        name = path_name(row['path'])
        require(name != 'report.json', 'nonrecursive index inventory')
        integer(row['bytes'], 0, MAX_MEMBER); blob(row['sha256'], 32)
        require(name in entries and entries[name] == row['bytes'], 'declared actual member size')
        reserved += row['bytes']
        require(reserved <= MAX_BYTES - len(raw), 'declared aggregate bound')
        names.append(name)
    require(names == sorted(set(names)) and set(names) == set(input_roles()), 'unique complete closed packet paths')
    data = {}
    for row in rows:
        value = read_file(base / row['path'], expected_size=row['bytes'])
        require(descriptor(row['path'], value) == row, 'packet member hash/length')
        data[row['path']] = value
    return index, data, raw


def source_fingerprint(data):
    h = hashlib.sha256()
    for name in SOURCE_NAMES:
        content = data['source/' + name]
        h.update(struct.pack('>Q', len(name))); h.update(name.encode())
        h.update(struct.pack('>Q', len(content))); h.update(content)
    return h.hexdigest()


def check_locks(data):
    lock = tomllib.loads(data['source/Cargo.lock'].decode())
    require(sha(data['reference/root-Cargo.lock']) == BASELINE_LOCK_SHA, 'independently reviewed baseline lock identity')
    baseline = tomllib.loads(data['reference/root-Cargo.lock'].decode())
    require([p['name'] for p in lock['package'] if 'source' not in p] == ['chialisp-oxide-xchandles-premine'], 'no path dependency replacement')
    registry = 'registry+https://github.com/rust-lang/crates.io-index'
    def tuples(value):
        return {(p['name'], p['version'], p.get('source'), p.get('checksum')) for p in value['package'] if 'source' in p}
    allowed = tuples(baseline)
    allowed.update((n, v, registry, digest) for n, (v, digest) in NEW_DEPS.items())
    actual = tuples(lock)
    require(actual <= allowed, 'dependency tuple not reviewed baseline/new pair')
    require({(n, v, registry, digest) for n, (v, digest) in NEW_DEPS.items()} <= actual, 'exact two added external dependencies')
    require(sha(data['source/fixtures/SLOT-MACHINE-LICENSE.md']) == '257f43a5c0407fb3a90f3c0771aecdd77a1160dee1ee2be8250ac629dcac1962', 'retained Prosperity license')
    require(sha(data['reference/driver.crate']) == DRIVER_SHA and len(data['reference/driver.crate']) == 241590, 'opaque official driver crate pin')
    manifest = tomllib.loads(data['source/Cargo.toml'].decode())
    require(manifest['package']['name'] == 'chialisp-oxide-xchandles-premine' and manifest['package']['publish'] is False
            and 'workspace' in manifest, 'unpublished isolated host package')
    keys(manifest, ['package', 'workspace', 'dependencies'])
    require(manifest['package'] == {'name': 'chialisp-oxide-xchandles-premine', 'version': '0.0.0', 'edition': '2024', 'publish': False}
            and manifest['workspace'] == {} and json.dumps(manifest['dependencies'], sort_keys=True) == json.dumps(DIRECT_DEPENDENCIES, sort_keys=True), 'closed reviewed direct dependencies/features; no path/git/patch override')
    for name, value in manifest['dependencies'].items():
        version = value if type(value) is str else value['version']
        require(version.startswith('=') and any(n == name and v == version[1:] for n, v, _, _ in actual), 'exact direct dependency pin')


def check_commands(data):
    executable, cwd, target, logs = None, None, None, None
    for role in RUNS:
        c = json_read(data['capture/' + role + '.command.json'])
        keys(c, ['argv', 'cwd', 'exit_code', 'target', 'jobs', 'offline', 'status', 'failure_kind', 'failure_detail',
                 'timeout_seconds', 'cleanup_timeout_seconds', 'max_stream_bytes', 'stdout', 'stderr'])
        require(type(c['argv']) is list and 1 <= len(c['argv']) <= 16 and all(type(x) is str and len(x) <= 1024 for x in c['argv']), 'bounded recorded argv')
        require(c['jobs'] == 2 and type(c['jobs']) is int and c['offline'] is True and c['cleanup_timeout_seconds'] == 5
                and c['max_stream_bytes'] == 16777216, 'fixed bounded process profile')
        for stream in ('stdout', 'stderr'):
            receipt = c[stream]
            keys(receipt, ['path', 'bytes', 'sha256'])
            expected = descriptor('unused', data['capture/' + role + '.' + stream])
            require(type(receipt['path']) is str and receipt['path'].endswith('/' + role + '.' + stream)
                    and type(receipt['bytes']) is int and receipt['bytes'] == expected['bytes'] and receipt['sha256'] == expected['sha256'], 'actual retained command stream relation')
        refused = role.startswith('refuse-')
        require(type(c['exit_code']) is int and c['exit_code'] == (1 if refused else 0)
                and c['status'] == ('failed' if refused else 'passed')
                and c['failure_kind'] == ('nonzero_exit' if refused else None) and c['failure_detail'] is None
                and c['timeout_seconds'] == (2 if refused else 600), 'typed process completion, no timeout credit')
        argv = c['argv']
        require(type(c['cwd']) is str and c['cwd'].startswith('/') and type(c['target']) is str and c['target'].startswith('/'), 'recorded wrapper root strings')
        cwd, target, logs = cwd or c['cwd'], target or c['target'], logs or c['stdout']['path'].rsplit('/', 1)[0]
        require(c['cwd'] == cwd and c['target'] == target and all(c[x]['path'] == logs + '/' + role + '.' + x for x in ('stdout', 'stderr')), 'consistent explicit wrapper origins (not followed)')
        if role in ('format', 'native-tests', 'clippy', 'binary-build'):
            require(argv[0] == 'cargo', 'native Cargo command')
            action = {'format': 'fmt', 'native-tests': 'test', 'clippy': 'clippy', 'binary-build': 'build'}[role]
            expected_argv = ['cargo', action, '--manifest-path', cwd + '/experiments/xchandles-premine/Cargo.toml']
            expected_argv += ['--check'] if role == 'format' else ((['--all-targets'] if role == 'clippy' else []) + ['--locked', '--offline'] + (['--', '-D', 'warnings'] if role == 'clippy' else []))
            require(argv == expected_argv, 'closed Cargo role exact argv')
            require('--check' in argv if role == 'format' else '--locked' in argv and '--offline' in argv, 'Cargo offline lock profile')
            if role == 'clippy':
                require(argv[-3:] == ['--', '-D', 'warnings'] and '--all-targets' in argv, 'Clippy warning gate')
        else:
            require(argv[0].endswith('/debug/chialisp-oxide-xchandles-premine'), 'fixed native executable role')
            executable = executable or argv[0]
            require(executable == argv[0] == target + '/debug/chialisp-oxide-xchandles-premine', 'same recorded executable spelling (not binary authentication)')
            if role == 'pause-first':
                require(argv[1:] == ['--stop-after', '1'], 'pause argv')
            elif role == 'resume-first' or refused:
                require(len(argv) == 3 and argv[1] == '--resume', 'resume/refusal argv')
                if role == 'resume-first':
                    require(argv[2] == logs + '/pause-first.stdout', 'actual paused report resume argv')
                elif role not in ('refuse-fifo', 'refuse-symlink', 'refuse-old-source-checkpoint'):
                    require(argv[2] == logs + '/checkpoint-' + role[7:] + '.json', 'actual negative fixture argv')
                elif role in ('refuse-fifo', 'refuse-symlink'):
                    require(argv[2] == logs + '/checkpoint-' + role[7:], 'special descriptor refusal path only (not recreated)')
                else:
                    require(argv[2].endswith('/final-pause-first.stdout'), 'historical checkpoint argv')
            else:
                require(len(argv) == 1, 'full run argv')
    native = data['capture/native-tests.stdout']
    require(b'test result: ok. 4 passed; 0 failed; 0 ignored;' in native and native.count(b' ... ok') == 4 and all(('test journal::tests::' + n + ' ... ok').encode() in native for n in ('absent_checkpoint_is_infrastructure_not_policy_rejection', 'duplicate_keys_are_rejected_at_every_object_before_prefix_comparison', 'current_source_input_window_matches_compiled_recipe', 'json_work_depth_and_trailing_data_are_infrastructure_failures')), 'actual retained four native unit-test rows')
    for name in REFUSALS:
        stderr = data['capture/refuse-' + name + '.stderr']
        require(data['capture/refuse-' + name + '.stdout'] == b'' and b'Error:' in stderr, 'actual refusal stderr')
        if name in ('changed-coin', 'unknown-field'):
            require(stderr.count(b'submitting ') == 4 and b'checkpoint prefix differs' in stderr, 'two full-prefix replay refusals')
        else:
            require(b'submitting ' not in stderr, 'pre-SDK parser/source/descriptor refusal')
    return executable


def validate(base):
    index, data, index_bytes = load_packet(base)
    check_locks(data)
    fingerprint = source_fingerprint(data)
    receipt = json_read(data['capture/final-receipt.json'])
    keys(receipt, ['protocol', 'status', 'base_commit', 'source_files', 'source_recipe_fingerprint', 'full_reports', 'full_report_sha256', 'per_full_report', 'pause_prefix', 'checkpoint_refusal_processes', 'before_sdk_refusals', 'prefix_replay_refusals', 'native_unit_tests', 'gate_process_matrix_totals', 'scope'])
    require(receipt['base_commit'] == '40ae94f', 'reviewed native checkpoint base')
    require(receipt['protocol'] == 'chialisp-oxide-xchandles-host-checkpoint-receipt/v1'
            and receipt['status'] == 'hardened_executed_isolated_native' and receipt['source_recipe_fingerprint'] == fingerprint, 'frozen native receipt source binding')
    expected_source = sorted([descriptor(PACKAGE + n, data['source/' + n]) for n in PACKAGE_NAMES], key=lambda r: r['path'])
    require(receipt['source_files'] == expected_source, 'complete executed source declaration')
    raw = data['capture/full-first.stdout']
    require(raw == data['capture/full-repeat.stdout'] == data['capture/resume-first.stdout'], 'exact full repeat/resume report bytes')
    require(receipt['full_reports'] == ['full-first.stdout', 'full-repeat.stdout', 'resume-first.stdout']
            and receipt['full_report_sha256'] == sha(raw), 'native report wire binding')
    report = json_read(raw)
    require(report['recipe_fingerprint'] == fingerprint, 'report source/lock/fixture fingerprint')
    registers = validate_report(report)
    published = bytes.fromhex(''.join(data['source/fixtures/register.rue.hex'].decode().split()))
    require(th(decode(published)).hex() == REGISTER and sha(published) == report['register_module_blob_sha256'], 'retained published register artifact')
    pause = json_read(data['capture/pause-first.stdout'])
    expected_pause = dict(report)
    expected_pause.update(status='paused_after_validated_step', registration_steps=report['registration_steps'][:1],
                          transactions=report['transactions'][:7], component_vm_runs=20, sdk_transaction_attempts=7)
    require(pause == expected_pause, 'entire fresh replay pause prefix')
    check_commands(data)
    require(receipt['per_full_report'] == {'sdk_attempts': 20, 'accepted': 11, 'exact_typed_consensus_rejections': 6,
                                          'runtime_umbrellas': 3, 'component_vm_runs': 48, 'cost_passes': 11}, 'separate full-report observations')
    require(receipt['pause_prefix'] == {'sdk_attempts': 7, 'component_vm_runs': 20}
            and receipt['checkpoint_refusal_processes'] == 11 and receipt['before_sdk_refusals'] == 9
            and receipt['prefix_replay_refusals'] == 2 and receipt['native_unit_tests'] == 4, 'host control counts separate')
    matrix = {'sdk_attempts': 81, 'accepted': 45, 'exact_typed_consensus_rejections': 24,
              'runtime_umbrellas': 12, 'retained_component_vm_runs': 204, 'separate_cost_passes': 45}
    require(receipt['gate_process_matrix_totals'] == matrix, 'repeat/process matrix not extra coverage')
    check_negative_inputs(data, pause)
    settings = json_read(data['reference/build-settings.json'])
    require(settings['toolchain'] == 'nightly-2026-07-14' and settings['rustc'].endswith('/nightly-2026-07-14-aarch64-apple-darwin/bin/rustc')
            and len(settings['bigdecimal_environment']) == 6 and all(v == 'unset; reviewed crate default' for v in settings['bigdecimal_environment'].values()), 'recorded native build settings')
    summary = {'scope': LIMITATIONS, 'status': 'admitted_recorded_observations', 'recipe_fingerprint': fingerprint,
               'sdk': report['sdk'], 'consensus': report['consensus'], 'component_vm': report['component_vm'],
               'cost_profile': 'chia-consensus0.36.1/plain-generator/no-BLS/flags720902/max5500000000',
               'canonical_modules': MODULES, 'batch_sizes': [1, 2, 3], 'handles': sum(HANDLES, []),
               'sdk_transaction_attempts': 20, 'accepted': 11, 'exact_typed_consensus_rejections': 6,
               'runtime_umbrellas': 3, 'component_vm_runs': 48, 'plain_generator_cost_passes': 11,
               'checkpoint_refusal_processes': 11, 'gate_process_matrix_totals': matrix,
               'batches': [], 'source_files': expected_source,
               'licenses': {'sdk': 'Apache-2.0', 'retained_register_source': 'Prosperity-3.0',
                            'sdk_driver_archive_sha256': DRIVER_SHA,
                            'sdk_driver_vcs': 'reviewed source observation: e9a08f6e54ea5132f7da06327836a3b386a944b1 dirty:true; archive is opaque checksum-bound'},
               'limitations': LIMITATIONS}
    for names, (t, _, _) in zip(HANDLES, registers):
        m = t['spendbundle_metrics']
        summary['batches'].append(dict(batch_size=len(names), handles=names,
            **{k: m[k] for k in ('plain_generator_bytes', 'charged_generator_bytes', 'execution_cost', 'condition_cost', 'byte_cost', 'total_cost')},
            fee_mojos=str(m['fee_mojos'])))
    files = sorted(index['files'] + [descriptor('report.json', index_bytes)], key=lambda r: r['path'])
    return {'protocol': PROTOCOL, 'index_sha256': sha(index_bytes), 'files': files, 'summary': summary}


def check_negative_inputs(data, pause):
    def negative(name):
        return data['capture/checkpoint-' + name + '.json']
    for name in ('changed-coin', 'unknown-field', 'wrong-recipe'):
        actual = json_read(negative(name))
        if name == 'changed-coin':
            expected = json.loads(json.dumps(pause))
            expected['registration_steps'][0]['current_registry']['coin_id'] = '00' * 32
            require(actual == expected, 'exact changed coin fixture')
        elif name == 'unknown-field':
            require(actual == dict(pause, untrusted_assertion='accept'), 'unknown whole-prefix field fixture')
        else:
            require(actual == dict(pause, recipe_fingerprint='00' * 32), 'wrong source recipe fixture')
    # Exact attacker bytes derive from the valid paused wire. No parser
    # substitution may turn a duplicate-key control into an unrelated error.
    pause_wire = data['capture/pause-first.stdout']
    root = b'{"protocol":' + json.dumps(PILOT).encode() + b',' + pause_wire[1:]
    coin_id = pause['registration_steps'][0]['complete_registry_children'][0]['coin']['coin_id']
    original = ('"coin_id":"' + coin_id + '"').encode()
    require(pause_wire.count(original) >= 1, 'paused canonical compact coin token')
    duplicated = original + b',' + original
    escaped = original + b',"\\u0063oin_id":"' + coin_id.encode() + b'"'
    expected = {'duplicate-root': root,
                'duplicate-nested': pause_wire.replace(original, duplicated, 1),
                'duplicate-escaped-nested': pause_wire.replace(original, escaped, 1),
                'malformed': b'{'}
    for name, raw in expected.items():
        require(negative(name) == raw, 'exact named JSON refusal fixture: ' + name)
        stderr = data['capture/refuse-' + name + '.stderr']
        require(b'invalid bounded unique-key checkpoint JSON' in stderr, 'bounded parser refusal class')
        if name == 'malformed':
            require(b'EOF while parsing an object' in stderr and b'duplicate checkpoint object key' not in stderr, 'malformed JSON refusal class')
        else:
            require(b'duplicate checkpoint object key' in stderr and b'EOF while parsing' not in stderr, 'duplicate key refusal class')
        try:
            json_read(raw)
        except (ValueError, UnicodeError):
            pass
        else:
            raise ValueError('negative JSON fixture must fail unique parsing')
    require(len(negative('oversized')) > 1 << 20, 'opaque oversize negative; never parse')
    old = json_read(data['reference/old-checkpoint.json'])
    require(old['protocol'] == PILOT and old['recipe_fingerprint'] != pause['recipe_fingerprint'], 'historical source checkpoint identity')


def install(destination, source, capture, baseline_lock, driver_archive, settings, old_checkpoint):
    """Explicit wrapper input API, never consults a receipt's origin paths.
    Source/capture reads are closed known names; no process or producer runs.
    Destination must be absent, and is fully validated before returning credit.
    """
    destination, source, capture = Path(destination), Path(source), Path(capture)
    require(not destination.exists() and not destination.is_symlink(), 'new packet destination only')
    inputs = {**{'source/' + n: source / n for n in PACKAGE_NAMES},
              **{'capture/' + n: capture / n for n in CAPTURE_NAMES},
              'reference/root-Cargo.lock': Path(baseline_lock), 'reference/driver.crate': Path(driver_archive),
              'reference/build-settings.json': Path(settings), 'reference/old-checkpoint.json': Path(old_checkpoint)}
    # Reserve every file before reading/hash work.
    total, sizes = 0, {}
    for name, p in inputs.items():
        info = p.lstat()
        require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and info.st_size <= MAX_MEMBER, 'wrapper regular bounded input')
        sizes[name] = info.st_size
        total += info.st_size
        require(total <= MAX_BYTES - MAX_INDEX, 'wrapper aggregate precharge')
    require(len(inputs) < MAX_FILES, 'wrapper count')
    content = {name: read_file(p, expected_size=sizes[name]) for name, p in inputs.items()}
    index = {'protocol': PROTOCOL, 'scope': LIMITATIONS, 'inputs': input_roles(),
             'files': [descriptor(n, content[n]) for n in sorted(content)]}
    index_bytes = (json.dumps(index, indent=2, sort_keys=True) + '\n').encode()
    require(len(index_bytes) <= MAX_INDEX, 'index bytes')
    destination.mkdir(parents=False)
    for name, raw in {**content, 'report.json': index_bytes}.items():
        path = destination / name
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open('xb') as f:
            f.write(raw)
    return validate(destination)
