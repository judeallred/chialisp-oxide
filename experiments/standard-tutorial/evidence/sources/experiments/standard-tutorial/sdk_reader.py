"""Pure bounded reader of the standard tutorial's recorded SDK fixture.

No filesystem, origin paths, compiler, VM, SDK, BLS verifier or network is used.
The caller must admit complete bundles and supply their exact selected bytes.
"""
import hashlib
import re

LANES = ('rust-off', 'rust-on', 'chialisp', 'rue-off', 'rue-on')
REQUEST_PROTOCOL = 'chialisp-oxide-standard-tutorial-ledger-request/v1'
REPORT_PROTOCOL = 'chialisp-oxide-standard-tutorial-ledger/v1'
MAX_MODULE_BYTES = 256 * 1024
MAX_MODULE_TOTAL = 2 * 1024 * 1024
MAX_WIRE_BYTES = MAX_MODULE_BYTES + 256
MAX_NODES = 65536
MAX_DEPTH = 256
MAX_PRIMARY_COST = 10_000_000
AMOUNT = 1000
DESTINATION = bytes([0x42]) * 32
ALTERNATE_DESTINATION = bytes([0x43]) * 32
IDENTITY_SIGNATURE = 'c0' + '00' * 95

# Public, disposable fixtures from chia-bls0.36.1/chia-puzzle-types0.36.1:
# original seed[7;32], wrong seed[8;32], and DeriveSynthetic::derive_synthetic_hidden
# for (q . ((51 0x4242...4242 1000))). The helper interprets SHA256 as signed.
# The public Simulator1337 parents below are fixed observations of three new_coin
# calls under chia-sdk-test0.36.0, not authenticated chain records or random keys.
# Source: unchanged standard55 report SHA256
# f33e9d7cbd4fed60d455ec0d7fdadc597ed9f41e9c30c707229a21e8f484153a.
ORIGINAL_PUBLIC_KEY = bytes.fromhex('a010d140e7c43146b5bb59695e6c444abbb62e964a535d0034351a90d1192bff0130de95f9bbc58af254c4dab4e65d3a')
WRONG_PUBLIC_KEY = bytes.fromhex('a5a0ced8c174d59f3e56551633f92dc8d102bd60f0cc16299c7c4c6c3a3002952413df06b65217dd18dd7c12027134e2')
SYNTHETIC_PUBLIC_KEY = bytes.fromhex('aec6754c0ec299b8d16bbfe1ad8c8f55ebd8f6b99b6faf213c7bd590ce22bf00b6118ccebc3ee4a2a3d8f6c641469a56')
AGG_SIG_ME_DOMAIN = '37a90eb5185a9c4439a91ddc98bbadce7b4feba060d50116a067de66bf236615'
FIXTURE_PARENTS = tuple(bytes.fromhex(x) for x in (
    'ae2915f8495b82ab18f7f06a86c7498e34d193781a0906f753a08d4b9afd3fc8',
    '1c9bdb76cf327fe9af5a533ffc71baa6c93ca0ebc7fe37eb86f4ae90502771a7',
    'adc72db11a87a9a3dc1d95c7628e644b70788b9d6210d1870cda0f02887c9e28',
))
REPORT_SCOPE = ('Finite five-build committed-instance SDK observations only. Every coin commits its own actual module and chosen synthetic key before direct simulator fixture funding. Public hidden payment fixture needs no signature. Native/template/primaryVM/SDK phases remain distinct. Runtime umbrellas add no semantic rejection credit. Caller artifact admission/source authenticity are separate. No wallet/network/live-chain, canonical Chia module identity, whole-page completion, execution authentication, compiler preservation or production readiness claim.')
COUNTS = dict(builds=5, scenario_groups=15, direct_fixture_funding_coins=15,
              transaction_attempts=80, accepted=15, typed_consensus_rejected=45, runtime_umbrellas=20)
BUILD_COUNTS = dict(scenario_groups=3, transaction_attempts=16, accepted=3,
                    typed_consensus_rejected=9, runtime_umbrellas=4)
NAMES = ('empty_signature', 'original_key_signature', 'solution_hash_instead_of_program_hash',
         'wrong_coin_id_signature', 'wrong_network_domain_signature', 'pair_destination',
         'pair_amount', 'overmint_one_mojo', 'missing_solution_fields',
         'nonempty_zero_original_key_selects_hidden_path', 'signed_delegated_payment',
         'consumed_input_replay', 'wrong_original_key_hidden_opening',
         'wrong_hidden_program_opening', 'public_hidden_fixed_payment_without_synthetic_signature',
         'signed_identity_program_allows_chosen_solution_recipient')
STATUSES = ('consensus_rejected',) * 8 + ('runtime_umbrella',) * 2 + ('accepted', 'consensus_rejected') + ('runtime_umbrella',) * 2 + ('accepted',) * 2
CODES = ('BadAggregateSignature',) * 5 + ('InvalidPuzzleHash', 'InvalidCoinAmount', 'MintingCoin',
          'GeneratorRuntimeError', 'GeneratorRuntimeError', None, 'DoubleSpend',
          'GeneratorRuntimeError', 'GeneratorRuntimeError', None, None)


def require(value, message):
    if not value:
        raise ValueError(message)


def closed(value, names, label):
    require(type(value) is dict and len(value) == len(names) and set(value) == set(names), label + ': exact fields')


def integer(value, expected, label):
    require(type(value) is int and value == expected, label + ': exact integer')


def _counts(value, expected, label):
    closed(value, expected, label)
    for name, number in expected.items():
        integer(value[name], number, label + '/' + name)


def _hex(value, limit, label, *, length=None):
    require(type(value) is str and 0 < len(value) <= 2 * limit
            and len(value) % 2 == 0 and re.fullmatch('[0-9a-f]+', value), label + ': bounded canonical hex')
    if length is not None:
        require(len(value) == 2 * length, label + ': exact width')
    return bytes.fromhex(value)


def _atom_prefix(size):
    if size == 0:
        return b'\x80'
    for count in range(1, 6):
        if size < 1 << (7 - count + 8 * (count - 1)):
            prefix = ((255 << (8 - count)) & 255) | (size >> (8 * (count - 1)))
            return bytes([prefix]) + size.to_bytes(count, 'big')[1:]
    raise ValueError('wire atom length bound')


def decode(raw):
    """Decode one canonical object, charging literal occurrences before copying."""
    require(type(raw) is bytes and 0 < len(raw) <= MAX_WIRE_BYTES, 'bounded wire bytes')
    pending = [('node', 0)]
    values = []
    cursor = nodes = atom_bytes = 0
    while pending:
        kind, depth = pending.pop()
        if kind == 'pair':
            right, left = values.pop(), values.pop()
            values.append((left, right))
            continue
        require(nodes < MAX_NODES and depth <= MAX_DEPTH, 'wire node/depth budget')
        nodes += 1
        require(cursor < len(raw), 'truncated wire object')
        start = cursor
        head = raw[cursor]
        cursor += 1
        if head == 255:
            require(depth < MAX_DEPTH, 'wire depth budget')
            pending.extend((('pair', depth), ('node', depth + 1), ('node', depth + 1)))
            continue
        if head < 128:
            require(atom_bytes < MAX_WIRE_BYTES, 'wire atom byte budget')
            atom_bytes += 1
            values.append(bytes([head]))
            continue
        count = 0
        while head & (128 >> count):
            count += 1
        require(count <= 5 and cursor + count - 1 <= len(raw), 'wire atom header budget')
        size = head & ((1 << (7 - count)) - 1)
        for byte in raw[cursor:cursor + count - 1]:
            size = size * 256 + byte
        cursor += count - 1
        require(size <= MAX_WIRE_BYTES - atom_bytes and size <= len(raw) - cursor, 'wire atom precharge')
        require(raw[start:cursor] == _atom_prefix(size), 'noncanonical wire length prefix')
        require(not (size == 1 and raw[cursor] < 128), 'noncanonical single-byte wire atom')
        atom_bytes += size
        values.append(raw[cursor:cursor + size])
        cursor += size
    require(cursor == len(raw) and len(values) == 1, 'exact complete wire object')
    return values[0]


def _visit(tree):
    """Charge expanded occurrences; shared data never bypasses work/depth bounds."""
    pending = [(tree, 0)]
    nodes = atom_bytes = 0
    while pending:
        value, depth = pending.pop()
        require(nodes < MAX_NODES and depth <= MAX_DEPTH, 'tree node/depth budget')
        nodes += 1
        if type(value) is tuple:
            require(len(value) == 2 and depth < MAX_DEPTH, 'binary tree/depth shape')
            yield value, True
            pending.extend(((value[1], depth + 1), (value[0], depth + 1)))
        else:
            require(type(value) is bytes and len(value) <= MAX_WIRE_BYTES - atom_bytes, 'tree atom precharge')
            atom_bytes += len(value)
            yield value, False


def encode(tree):
    parts = []
    remaining = MAX_WIRE_BYTES
    for value, pair in _visit(tree):
        if pair:
            chunk = b'\xff'
        else:
            chunk = value if len(value) == 1 and value[0] < 128 else _atom_prefix(len(value)) + value
        require(len(chunk) <= remaining, 'encoded wire byte precharge')
        remaining -= len(chunk)
        parts.append(chunk)
    return b''.join(parts)


def tree_hash(tree):
    # An initial iterative walk bounds all work before the postorder stack grows.
    for _ in _visit(tree):
        pass
    pending = [(tree, False)]
    values = []
    while pending:
        value, finish = pending.pop()
        if finish:
            right, left = values.pop(), values.pop()
            values.append(hashlib.sha256(b'\x02' + left + right).digest())
        elif type(value) is tuple:
            pending.extend(((value, True), (value[1], False), (value[0], False)))
        else:
            values.append(hashlib.sha256(b'\x01' + value).digest())
    return values[0]


def list_tree(*items):
    result = b''
    for item in reversed(items):
        result = item, result
    return result


def curry(module, key):
    return list_tree(b'\x02', (b'\x01', module), list_tree(b'\x04', (b'\x01', key), b'\x01'))


def coin_id(parent, puzzle_hash, amount):
    require(type(parent) is bytes and len(parent) == 32 and type(puzzle_hash) is bytes and len(puzzle_hash) == 32,
            'coin parent/puzzle hash widths')
    require(type(amount) is int and 0 <= amount < 1 << 64, 'coin unsigned amount')
    encoded = amount.to_bytes((amount.bit_length() + 7) // 8, 'big')
    if encoded and encoded[0] & 128:
        encoded = b'\x00' + encoded
    return hashlib.sha256(parent + puzzle_hash + encoded).digest()


def _coin(value, parent, puzzle_hash, label):
    closed(value, {'coin_id', 'parent_coin_id', 'puzzle_hash', 'amount'}, label)
    require(_hex(value['parent_coin_id'], 32, label + '/parent', length=32) == parent, label + ': fixed parent')
    require(_hex(value['puzzle_hash'], 32, label + '/puzzle', length=32) == puzzle_hash, label + ': puzzle binding')
    integer(value['amount'], AMOUNT, label + '/amount')
    expected = coin_id(parent, puzzle_hash, AMOUNT)
    require(_hex(value['coin_id'], 32, label + '/coin-id', length=32) == expected, label + ': exact coin ID')
    return expected


def _execution(value, output, failure, label):
    if failure is None:
        closed(value, {'outcome', 'cost', 'error'}, label)
        closed(value['outcome'], {'status', 'output_hex'}, label + '/outcome')
        require(value['outcome']['status'] == 'accepted' and value['error'] is None, label + ': accepted scope')
        require(type(value['cost']) is int and 0 < value['cost'] <= MAX_PRIMARY_COST, label + ': positive bounded cost')
        expected = encode(output)
        actual = _hex(value['outcome']['output_hex'], MAX_WIRE_BYTES, label + '/output')
        require(actual == expected, label + ': complete exact output')
    else:
        closed(value, {'outcome', 'cost', 'error', 'failure'}, label)
        require(value['outcome'] == {'status': 'rejected'} and value['cost'] is None
                and type(value['error']) is str and 0 < len(value['error']) <= 16384, label + ': program rejection scope')
        closed(value['failure'], {'kind', 'code'}, label + '/failure')
        require(value['failure'] == {'kind': 'program', 'code': failure}, label + ': exact typed program code')


def _fixture():
    payment = list_tree(list_tree(b'\x33', DESTINATION, b'\x03\xe8'))
    alternate = list_tree(list_tree(b'\x33', ALTERNATE_DESTINATION, b'\x03\xe8'))
    hidden = b'\x01', payment
    pair_destination = list_tree(list_tree(b'\x33', (b'\x01', b'\x02'), b'\x03\xe8'))
    pair_amount = list_tree(list_tree(b'\x33', DESTINATION, (b'\x01', b'\x02')))
    overmint = list_tree(list_tree(b'\x33', DESTINATION, b'\x03\xe9'))
    standard = list_tree(b'', hidden, b'')
    solutions = [standard] * 5 + [list_tree(b'', (b'\x01', output), b'') for output in (pair_destination, pair_amount, overmint)]
    solutions += [b'', list_tree(b'\x00', hidden, b''), standard, standard,
                  list_tree(WRONG_PUBLIC_KEY, hidden, b''), list_tree(ORIGINAL_PUBLIC_KEY, (b'\x01', alternate), b''),
                  list_tree(ORIGINAL_PUBLIC_KEY, hidden, b''), list_tree(b'', b'\x01', alternate)]
    outputs = [payment] * 5 + [pair_destination, pair_amount, overmint, None, None, payment, payment,
                              None, None, payment, alternate]
    programs = [hidden] * 5 + [(b'\x01', x) for x in (pair_destination, pair_amount, overmint)]
    programs += [None, hidden, hidden, hidden, hidden, (b'\x01', alternate), hidden, b'\x01']
    return hidden, payment, alternate, solutions, outputs, programs


def validate(request, report, expected_modules):
    """Validate recorded relations only; expected_modules are caller-admitted bytes."""
    closed(expected_modules, LANES, 'caller-admitted modules')
    total = 0
    for lane in LANES:
        payload = expected_modules[lane]
        require(type(payload) is bytes and 0 < len(payload) <= MAX_MODULE_BYTES, 'bounded exact selected module bytes')
        require(len(payload) <= MAX_MODULE_TOTAL - total, 'complete selected module byte precharge')
        total += len(payload)
    closed(request, {'protocol', 'runs'}, 'request')
    require(request['protocol'] == REQUEST_PROTOCOL and type(request['runs']) is list and len(request['runs']) == 5, 'exact five-build request')
    closed(report, {'protocol', 'status', 'builds', 'consensus', 'counts', 'network', 'primary_failure_protocol',
                    'primary_max_cost', 'primary_vm', 'rejection_protocol', 'scope', 'sdk', 'sdk_max_cost', 'sdk_vm'}, 'SDK report')
    fixed = dict(protocol=REPORT_PROTOCOL, status='executed', consensus='chia-consensus 0.36.1', network='TESTNET11',
                 primary_failure_protocol='clvmr-0.21.0/v1', primary_vm='clvmr 0.21.0 MEMPOOL_MODE',
                 rejection_protocol='chia-consensus-0.36.1/v1', scope=REPORT_SCOPE,
                 sdk='chia-sdk-test 0.36.0', sdk_vm='clvmr 0.16.2')
    for name, expected in fixed.items():
        require(report[name] == expected, 'exact SDK profile/' + name)
    integer(report['primary_max_cost'], MAX_PRIMARY_COST, 'primary cost')
    integer(report['sdk_max_cost'], 5_500_000_000, 'SDK cost')
    _counts(report['counts'], COUNTS, 'SDK counts')
    require(type(report['builds']) is list and len(report['builds']) == 5, 'exact five SDK builds')
    hidden, payment, alternate, solutions, outputs, programs = _fixture()
    hidden_hash = tree_hash(hidden)
    identities = []
    for position, lane in enumerate(LANES):
        selected = expected_modules[lane]
        module = decode(selected)
        require(encode(module) == selected, 'canonical selected module')
        item = request['runs'][position]
        closed(item, {'lane', 'module_hex'}, 'request lane')
        require(item['lane'] == lane and _hex(item['module_hex'], MAX_MODULE_BYTES, 'request module') == selected, 'request exact lane/module')
        build = report['builds'][position]
        closed(build, {'agg_sig_me_domain', 'attempts', 'committed_synthetic_public_key', 'counts',
                       'direct_fixture_funding_coins', 'hidden_fixture_requires_signature', 'hidden_program_hex',
                       'hidden_program_tree_hash', 'instance_tree_hash', 'lane', 'module_blob_sha256',
                       'module_hex', 'module_tree_hash', 'original_public_key', 'reveal_hex'}, 'SDK build')
        require(build['lane'] == lane and _hex(build['module_hex'], MAX_MODULE_BYTES, 'reported module') == selected, 'reported lane/module')
        require(build['module_blob_sha256'] == hashlib.sha256(selected).hexdigest()
                and build['module_tree_hash'] == tree_hash(module).hex(), 'module blob/tree distinction')
        require(build['original_public_key'] == ORIGINAL_PUBLIC_KEY.hex()
                and build['committed_synthetic_public_key'] == SYNTHETIC_PUBLIC_KEY.hex(), 'fixed public key fixture')
        require(build['agg_sig_me_domain'] == AGG_SIG_ME_DOMAIN, 'fixed TESTNET11 signature domain')
        require(build['hidden_program_hex'] == encode(hidden).hex() and build['hidden_program_tree_hash'] == hidden_hash.hex(), 'fixed hidden fixture')
        require(build['hidden_fixture_requires_signature'] is False, 'hidden fixture needs no synthetic signature')
        integer(build['direct_fixture_funding_coins'], 3, 'direct fixture funding')
        _counts(build['counts'], BUILD_COUNTS, 'build counts')
        reveal = encode(curry(module, SYNTHETIC_PUBLIC_KEY))
        reveal_hash = tree_hash(decode(reveal))
        require(build['reveal_hex'] == reveal.hex() and build['instance_tree_hash'] == reveal_hash.hex(), 'actual-module/key curry binding')
        attempts = build['attempts']
        require(type(attempts) is list and len(attempts) == 16, 'exact sixteen ordered attempts')
        for index, (name, status, code) in enumerate(zip(NAMES, STATUSES, CODES)):
            actual = attempts[index]
            fields = {'name', 'status', 'transaction', 'primary_vm'}
            fields |= {'child', 'input_consumed'} if status == 'accepted' else {'code', 'full_simulator_equality'}
            if status == 'runtime_umbrella':
                fields.add('semantic_rejection_credit')
            if index == 15:
                fields |= {'contemplated_primary_vm', 'contemplated_solution_hex', 'delegated_program_tree_hash'}
            closed(actual, fields, lane + '/' + name)
            require(actual['name'] == name and actual['status'] == status, 'exact ordered attempt name/status')
            if status != 'accepted':
                require(actual['code'] == code and actual['full_simulator_equality'] is True, 'exact SDK code/full-state flag')
            if status == 'runtime_umbrella':
                require(actual['semantic_rejection_credit'] is False, 'runtime umbrella has no semantic rejection credit')
            transaction = actual['transaction']
            closed(transaction, {'signature_hex', 'spends'}, 'transaction')
            require(type(transaction['spends']) is list and len(transaction['spends']) == 1, 'exact one-spend fixture')
            signature = transaction['signature_hex']
            _hex(signature, 96, 'signature', length=96)
            require((signature == IDENTITY_SIGNATURE) == (index in (0, 8, 9, 12, 13, 14)), 'signature default/nonempty distinction')
            spend = transaction['spends'][0]
            closed(spend, {'coin', 'reveal_hex', 'solution_hex'}, 'spend')
            parent = FIXTURE_PARENTS[0 if index < 12 else 1 if index < 15 else 2]
            input_id = _coin(spend['coin'], parent, reveal_hash, 'input coin')
            require(spend['reveal_hex'] == reveal.hex() and spend['solution_hex'] == encode(solutions[index]).hex(), 'exact reveal/solution fixture')
            if status == 'runtime_umbrella':
                failure = ('raise' if lane.startswith('rust-') else 'path_into_atom') if index == 8 else ('raise' if lane.startswith('rue-') else 'invalid_alloc_arg') if index == 9 else 'raise'
                _execution(actual['primary_vm'], None, failure, 'rejected primary')
            else:
                output = outputs[index]
                if index != 14:
                    output = list_tree(b'\x32', SYNTHETIC_PUBLIC_KEY, tree_hash(programs[index])), output
                _execution(actual['primary_vm'], output, None, 'accepted primary')
            if status == 'accepted':
                require(actual['input_consumed'] is True, 'accepted input consumed')
                _coin(actual['child'], input_id, ALTERNATE_DESTINATION if index == 15 else DESTINATION, 'accepted child')
            if index == 15:
                require(actual['contemplated_solution_hex'] == encode(list_tree(b'', b'\x01', payment)).hex()
                        and actual['delegated_program_tree_hash'] == tree_hash(b'\x01').hex(), 'same identity program/different solution')
                _execution(actual['contemplated_primary_vm'],
                           (list_tree(b'\x32', SYNTHETIC_PUBLIC_KEY, tree_hash(b'\x01')), payment), None, 'contemplated primary')
        require(attempts[10]['transaction'] == attempts[11]['transaction'], 'replay exact consumed transaction')
        proper = attempts[10]['transaction']['signature_hex']
        require(all(attempts[i]['transaction']['signature_hex'] != proper for i in (1, 2, 3, 4)), 'invalid signatures differ from accepted signature')
        identities.append(dict(lane=lane, module_blob_sha256=hashlib.sha256(selected).hexdigest(),
                               module_tree_hash=build['module_tree_hash'], instance_tree_hash=reveal_hash.hex()))
    return dict(status='recorded_sdk_consistency_checked', counts=dict(COUNTS),
                primary_observations=dict(total=85, accepted=65, typed_program_rejected=20),
                modules=identities, execution_credit=False, bls_verification=False,
                scope='Recorded fixed-fixture wire, hash, output, typed-code and flag consistency only. Caller supplies independently admitted module bytes. No BLS verification, new VM/SDK execution, execution authentication, source authentication or compiler-preservation credit.')
