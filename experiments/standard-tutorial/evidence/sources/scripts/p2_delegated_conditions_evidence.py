"""Closed recorded raw-domain and owner-instance consistency checks.

The Rust producer checks actual SDK submissions and full simulator Eq in memory.
This reader checks typed receipts, source freshness and independently derived
fixture identities. It does not authenticate execution or prove state equality,
BLS, compiler preservation, exhaustive authorization or production readiness.
"""
import hashlib
from pathlib import Path
import re

from cargo_authoring import snapshot
from hashing_vectors import deserialize, proper, serialize, sha_tree
from vm_failures import check_execution, check_native_outcome
from p2_delegated_conditions_vectors import (
    ROOT, SLUG, DIRECTORY, SOURCE, ARTIFACT, CANONICAL_HASH, DEPENDENCIES, NATIVE, MODES,
    NAMED, closed, digest, expected, read, require, source_inputs,
)

PROTOCOL = 'chia-consensus-0.36.1/p2-delegated-conditions-v1'
OWNER = '8bc61e93380f3427ed2ace813fdff67504f55268a99a72cea57133c2b32a1d836fd52381ae9ca10957557b80064059e5'
ATTACKER = 'b47fa1e15162a5b1f9444d1aae57d131a1d5c98bade8f8e93b8ebfe1649b95c55787b8731d2f1a0638b3e1d23ada48e8'
# The SDK's compressed BLS identity is c0 followed by95 zero bytes. All-zero
# bytes are not that serialized signature; this reader does not validate BLS.
EMPTY_SIGNATURE = bytes.fromhex('c0' + '00' * 95)
PARENT = 'ae2915f8495b82ab18f7f06a86c7498e34d193781a0906f753a08d4b9afd3fc8'
OTHER_PARENT = '1c9bdb76cf327fe9af5a533ffc71baa6c93ca0ebc7fe37eb86f4ae90502771a7'
RECIPIENT, RETARGET = b'B' * 32, b'C' * 32
SCENARIOS = [
    ('missing_signature', 'BadAggregateSignature'),
    ('wrong_nonempty_key_signature', 'BadAggregateSignature'),
    ('changed_conditions_reuse_old_signature', 'BadAggregateSignature'),
    ('other_coin_reuse_signature', 'BadAggregateSignature'),
    ('correct_signature_cannot_mint_value', 'MintingCoin'),
    ('owner_key_curry_cannot_be_replaced', 'WrongPuzzleHash'),
    ('ignored_improper_tail_preserves_signature', None),
    ('owner_signature_exact_payout', None),
    ('spent_coin_cannot_replay', 'DoubleSpend'),
    ('unsafe_uncurried_caller_selects_attacker_key', None),
    ('unsafe_uncurried_alternative_owner_key', None),
    ('different_language_reveal_cannot_spend_input', 'WrongPuzzleHash'),
]
ACCEPTED_SCOPE = {
    SCENARIOS[6][0]: 'Curried owner key; arbitrary ignored tail is preserved.',
    SCENARIOS[7][0]: 'Curried owner key; finite selected-build consensus observation.',
    SCENARIOS[9][0]: 'Intentional missing-policy counterexample: funding raw module does not commit an owner key; not a compiler-prevented error.',
    SCENARIOS[10][0]: 'Alternative spend of the identical raw coin in a cloned state; not a second spend in one ledger.',
}
SIGNER_SCOPE = 'Signer-only alternate reveal on the same coin; not a separately admitted transaction.'
LEDGER_SCOPE = 'Actual TESTNET11 submissions for this compiled module and its own owner-key-curried instance. Full simulator Eq is asserted in memory after each exact typed rejection; the recorded Boolean is not a state proof or authentication. The two uncurried key-selection successes are intentional unsafe policy controls. Finite fixtures do not prove exhaustive authorization, BLS or compiler preservation.'
RAW_SCOPE = 'Additional retained independent200-row native/five-build replay, separate from ordinary20named+180disjoint generated and2059generic raw comparisons. Actual fault codes/text are retained and not normalized. No SDK attempts arise from these raw VM calls.'
COMPONENT_SCOPE = 'Raw template plus separately funded owner-key instances in five builds.60 actual SDK fixture submissions, not60generic ledger groups. Full simulator equality is checked in memory, not serialized as Debug or proved by its recordedBoolean. Unsafe uncurried key selection is intentionally admitted. Source freshness and finite native/VM/SDK checks are not execution authentication, compiler preservation, exhaustive authorization, BLS proof or production readiness.'
OLD_SLUGS = ['fixed-payment', 'capped-payment', 'split-payment', 'timelocked-payment', 'signature-payment',
             'two-signature-payment', 'recovery-payment', 'bounded-counter', 'boolean-gate', 'intro-list',
             'intro-add', 'intro-conditional', 'p2-conditions', 'rightmost-tree', 'last-list-element',
             'prefix-list', 'atom-not-equal', 'insertion-sort-by-keys', 'sha256tree', 'singleton-launcher',
             'password-coin', 'p2-singleton']


def hex_bytes(value, size=None):
    require(type(value) is str and len(value) <= 4 * 1024 * 1024 and len(value) % 2 == 0
            and re.fullmatch('[0-9a-f]*', value) is not None
            and (size is None or len(value) == size * 2), 'malformed retained hex')
    return bytes.fromhex(value)


def tree(value):
    raw = hex_bytes(value)
    require(bool(raw), 'missing serialized tree')
    result = deserialize(raw)
    require(serialize(result) == raw, 'retained tree serialization is not canonical')
    return result


def mode(value):
    require(type(value) is dict and type(value.get('backend')) is str
            and type(value.get('optimize_lir')) is bool, 'malformed build identity')
    result = value['backend'], value['optimize_lir']
    require(result in MODES, 'unexpected build identity')
    return result


def amount(value):
    require(type(value) is int and 0 <= value < 2**64, 'invalid coin amount')
    raw = value.to_bytes((value.bit_length() + 7) // 8, 'big')
    return b'\0' + raw if raw and raw[0] >= 128 else raw


def coin_recipe(parent, puzzle, value=1000):
    return dict(parent_id=parent, puzzle_hash=puzzle, amount=value,
                id=hashlib.sha256(hex_bytes(parent, 32) + hex_bytes(puzzle, 32) + amount(value)).hexdigest())


def coin(value):
    closed(value, ['id', 'parent_id', 'puzzle_hash', 'amount'], 'coin')
    require(value == coin_recipe(value['parent_id'], value['puzzle_hash'], value['amount']), 'coin ID does not bind fields')
    return value


def curry(module, key):
    return proper(b'\x02', (b'\x01', module), proper(b'\x04', (b'\x01', key), b'\x01'))


def conditions(destination=RECIPIENT, value=1000):
    return proper(proper(b'\x33', destination, amount(value)))


def transaction(value, expected_coin, reveal, solution):
    closed(value, ['signature_hex', 'spends'], 'transaction')
    signature = hex_bytes(value['signature_hex'], 96)
    require(signature != bytes(96), 'all-zero bytes are not an accepted compressed signature fixture')
    require(type(value['spends']) is list and len(value['spends']) == 1, 'fixture requires exactly one spend')
    spend = value['spends'][0]
    closed(spend, ['coin', 'reveal_hex', 'solution_hex'], 'coin spend')
    require(coin(spend['coin']) == expected_coin and tree(spend['reveal_hex']) == reveal
            and tree(spend['solution_hex']) == solution, 'transaction differs from independent fixture recipe')
    return signature


def artifacts(values):
    require(type(values) is list and len(values) == 5, 'all five artifact modes are required')
    result = {}
    for value, wanted in zip(values, MODES, strict=True):
        closed(value, ['backend', 'optimize_lir', 'bytes', 'tree_hash', 'clvm_hex', 'disassembly'], 'artifact')
        require(mode(value) == wanted, 'artifact order/identity differs')
        code = hex_bytes(value['clvm_hex'])
        require(type(value['bytes']) is int and value['bytes'] == len(code) > 0
                and type(value['disassembly']) is str and value['disassembly'], 'artifact byte metadata')
        program = tree(value['clvm_hex'])
        require(sha_tree(program).hex() == value['tree_hash'], 'artifact tree hash')
        result[wanted] = program
    require(sha_tree(result[('chialisp', True)]).hex() == CANONICAL_HASH, 'canonical137 identity changed')
    return result


def check_raw_case(value, spec):
    closed(value, ['label', 'solution', 'environment_hex', 'expected', 'canonical_failure_code', 'native', 'runs'], 'raw-domain case')
    require({key: value[key] for key in ('label', 'solution', 'environment_hex', 'expected')} == spec, 'raw vector identity/expectation changed')
    require(value['canonical_failure_code'] == ('path_into_atom' if spec['expected']['status'] == 'rejected' else None), 'canonical fault contrast')
    require(check_native_outcome(value['native']) == spec['expected'], 'native source differs from independent expectation')
    require(type(value['runs']) is list and len(value['runs']) == 5, 'raw vector needs all five runs')
    for run, wanted in zip(value['runs'], MODES, strict=True):
        closed(run, ['backend', 'optimize_lir', 'execution'], 'raw VM run')
        require(mode(run) == wanted and check_execution(run['execution']) == spec['expected'], 'raw VM result differs from independent expectation')
        if spec['expected']['status'] == 'accepted':
            require(run['execution']['cost'] <= 1_000_000, 'accepted execution exceeds recorded VM budget')
        if spec['expected']['status'] == 'rejected':
            code = run['execution']['failure']['code']
            require(code in ('raise', 'path_into_atom') and (wanted[0] != 'chialisp' or code == 'path_into_atom'), 'unexpected projection fault or normalized canonical code')


def check_ledger(value, wanted, modules):
    closed(value, ['backend', 'optimize_lir', 'status', 'module_tree_hash', 'module_hex', 'owner_public_key',
                  'attacker_public_key', 'owner_curried_reveal_hex', 'owner_instance_hash', 'transaction_attempts',
                  'accepted', 'typed_rejected_with_full_rollback', 'observations', 'scope'], 'owner-instance build')
    require(mode(value) == wanted and value['status'] == 'validated' and value['scope'] == LEDGER_SCOPE, 'owner-instance mode/status/scope')
    module = modules[wanted]
    require(tree(value['module_hex']) == module and value['module_tree_hash'] == sha_tree(module).hex(), 'ledger module is not selected artifact')
    require(value['owner_public_key'] == OWNER and value['attacker_public_key'] == ATTACKER, 'fixture key identity')
    owner_key, attacker_key = hex_bytes(OWNER, 48), hex_bytes(ATTACKER, 48)
    owner, attacker = curry(module, owner_key), curry(module, attacker_key)
    owner_hash = sha_tree(owner).hex()
    require(tree(value['owner_curried_reveal_hex']) == owner and value['owner_instance_hash'] == owner_hash, 'owner instance must curry actual module/key')
    for key, expected_value in [('transaction_attempts', 12), ('accepted', 4), ('typed_rejected_with_full_rollback', 8)]:
        require(type(value[key]) is int and value[key] == expected_value, 'ledger count differs')
    observations = value['observations']
    require(type(observations) is list and len(observations) == 12, 'ledger scenario count')
    selected_coin = coin_recipe(PARENT, owner_hash)
    other_coin = coin_recipe(OTHER_PARENT, owner_hash)
    raw_coin = coin_recipe(PARENT, sha_tree(module).hex())
    regular = conditions()
    ordinary = regular, b''
    by_label, signatures = {}, {}
    alternate_mode = ('rue_source', True) if wanted == ('chialisp', True) else ('chialisp', True)
    alternate_module = modules[alternate_mode]
    alternate_reveal = curry(alternate_module, owner_key)
    require(sha_tree(alternate_reveal).hex() != owner_hash, 'cross-language control requires a genuinely different identity')
    for index, (row, (label, code)) in enumerate(zip(observations, SCENARIOS, strict=True)):
        fields = ['label', 'status', 'transaction'] + (['scope', 'spent_input_observed', 'child'] if code is None else ['code', 'full_simulator_equality'])
        if index == 1: fields += ['signature_source', 'signature_source_scope']
        if index == 11: fields += ['alternate_build', 'signature_matches_selected_reveal', 'selected_reveal_signature_source']
        closed(row, fields, 'ledger observation')
        require(row['label'] == label, 'ledger scenario order/identity')
        if code is None:
            require(row['status'] == 'accepted' and row['scope'] == ACCEPTED_SCOPE[label]
                    and row['spent_input_observed'] is True, 'accepted receipt scope')
        else:
            require(row['status'] == 'consensus_rejected' and row['code'] == code
                    and row['full_simulator_equality'] is True, 'expected typed consensus code/full in-memory Eq flag missing')
        expected_coin, reveal, solution = selected_coin, owner, ordinary
        if index == 2: solution = conditions(RETARGET), b''
        elif index == 3: expected_coin = other_coin
        elif index == 4: solution = conditions(RECIPIENT, 1001), b''
        elif index == 5: reveal = attacker
        elif index == 6: solution = regular, (b'\0', b'\x81')
        elif index in (9, 10):
            expected_coin, reveal = raw_coin, module
            solution = (attacker_key, (conditions(RETARGET), b'')) if index == 9 else (owner_key, ordinary)
        elif index == 11: reveal = alternate_reveal
        signature = transaction(row['transaction'], expected_coin, reveal, solution)
        require((signature == EMPTY_SIGNATURE) == (index == 0), 'expected actual nonempty signature or deliberate omitted signature')
        if code is None:
            destination = RETARGET if index == 9 else RECIPIENT
            require(coin(row['child']) == coin_recipe(expected_coin['id'], destination.hex()), 'accepted exact payout child differs')
        by_label[label], signatures[label] = row, signature
    complete = signatures[SCENARIOS[7][0]]
    for index in (2, 3, 6, 8, 11):
        require(signatures[SCENARIOS[index][0]] == complete, 'signature reuse differs from intended message/coin control')
    wrong = by_label[SCENARIOS[1][0]]
    source_signature = transaction(wrong['signature_source'], selected_coin, attacker, ordinary)
    require(wrong['signature_source_scope'] == SIGNER_SCOPE and source_signature == signatures[SCENARIOS[1][0]] != complete
            and signatures[SCENARIOS[5][0]] == source_signature, 'same-coin wrong-key signer-only provenance differs')
    cross = by_label[SCENARIOS[11][0]]
    closed(cross['alternate_build'], ['backend', 'optimize_lir', 'module_tree_hash', 'owner_curried_reveal_hex', 'owner_instance_hash'], 'alternate build')
    require(mode(cross['alternate_build']) == alternate_mode
            and cross['alternate_build']['module_tree_hash'] == sha_tree(alternate_module).hex()
            and tree(cross['alternate_build']['owner_curried_reveal_hex']) == alternate_reveal
            and cross['alternate_build']['owner_instance_hash'] == sha_tree(alternate_reveal).hex()
            and cross['signature_matches_selected_reveal'] is True, 'alternate identity/reveal/signature binding differs')
    require(cross['selected_reveal_signature_source'] == by_label[SCENARIOS[7][0]]['transaction'], 'cross-language signature source must be exact owner transaction')


def validate_component(puzzle, *, root=ROOT):
    closed(puzzle, ['slug', 'title', 'kind', 'security_scope', 'provenance', 'environment', 'source',
                   'reference', 'reference_includes', 'rue', 'source_dependencies', 'native_sources',
                   'artifacts', 'cases', 'generated_cases', 'simulator', 'simulator_comparisons',
                   'module_identity', 'protocol_validation', 'validated'], 'corpus component')
    manifest, specs, _ = source_inputs(root)
    for key in ('slug', 'title', 'kind', 'environment', 'security_scope', 'provenance', 'reference_includes'):
        require(puzzle.get(key) == manifest[key], 'component/source manifest metadata differs: ' + key)
    require(puzzle.get('source') == read(root, DIRECTORY + '/puzzle.rs').decode()
            and puzzle.get('reference') == read(root, DIRECTORY + '/reference.clsp').decode(), 'published code text differs')
    closed(puzzle.get('rue'), ['path', 'sha256', 'source', 'dependencies'], 'Rue source')
    require(puzzle['rue'] == dict(path=DIRECTORY + '/puzzle.rue', sha256=digest(read(root, DIRECTORY + '/puzzle.rue')),
                source=read(root, DIRECTORY + '/puzzle.rue').decode(), dependencies=[]), 'actual Rue source differs')
    expected_sources = [dict(path=path, sha256=digest(read(root, path)), source=read(root, path).decode()) for path in DEPENDENCIES]
    require(puzzle.get('source_dependencies') == expected_sources, 'declared source dependencies differ')
    require(puzzle.get('native_sources') == [dict(path=path, sha256=digest(read(root, path))) for path in NATIVE], 'native implementation/source snapshot differs')
    require(puzzle.get('validated') is True and puzzle.get('simulator') == [] and puzzle.get('simulator_comparisons') == [], 'dedicated instance evidence must not impersonate generic ledger groups')
    require(type(puzzle.get('generated_cases')) is int and puzzle['generated_cases'] == 180 + 2059, 'ordinary disjoint180 + existing2059raw coverage changed')
    modules = artifacts(puzzle.get('artifacts'))
    canonical_hex = read(root, ARTIFACT).decode().strip()
    require(serialize(modules[('chialisp', True)]).hex() == canonical_hex, 'canonical published module bytes differ')
    identity = puzzle.get('module_identity')
    closed(identity, ['upstream_hex', 'upstream_tree_hash', 'comparisons'], 'module identity')
    require(identity['upstream_hex'] == canonical_hex and identity['upstream_tree_hash'] == CANONICAL_HASH, 'canonical identity obligation differs')
    expected_comparisons = [dict(backend=backend, optimize_lir=optimize,
        serialized_bytes_equal=serialize(modules[(backend, optimize)]).hex() == canonical_hex,
        tree_hash_equal=sha_tree(modules[(backend, optimize)]).hex() == CANONICAL_HASH) for backend, optimize in MODES]
    require(identity['comparisons'] == expected_comparisons
            and all(type(row[key]) is bool for row in identity['comparisons'] for key in ('optimize_lir', 'serialized_bytes_equal', 'tree_hash_equal')), 'module identity comparison must reflect actual artifacts')
    by_label = {row['label']: row for row in specs}
    require(type(puzzle.get('cases')) is list and len(puzzle['cases']) == 20, 'named case count')
    for row, label in zip(puzzle['cases'], NAMED, strict=True):
        closed(row, ['label', 'solution', 'native', 'runs'], 'named case')
        normalized = dict(row, environment_hex=by_label[label]['environment_hex'], expected=by_label[label]['expected'],
                          canonical_failure_code='path_into_atom' if by_label[label]['expected']['status'] == 'rejected' else None)
        check_raw_case(normalized, by_label[label])
    protocol = puzzle.get('protocol_validation')
    closed(protocol, ['schema_version', 'protocol', 'status', 'simulator', 'consensus', 'vm', 'network',
                      'transaction_attempts', 'accepted', 'typed_rejected_with_full_rollback', 'builds', 'raw_validation', 'scope'], 'dedicated protocol')
    require(type(protocol['schema_version']) is int and protocol['schema_version'] == 1
            and protocol['protocol'] == PROTOCOL and protocol['status'] == 'validated'
            and protocol['simulator'] == 'chia-sdk-test 0.36.0' and protocol['consensus'] == 'chia-consensus 0.36.1'
            and protocol['vm'] == 'clvmr 0.16.2' and protocol['network'] == 'TESTNET11'
            and protocol['scope'] == COMPONENT_SCOPE, 'dedicated SDK versions/scope')
    for key, count in [('transaction_attempts', 60), ('accepted', 20), ('typed_rejected_with_full_rollback', 40)]:
        require(type(protocol[key]) is int and protocol[key] == count, 'computed SDK count contract')
    require(type(protocol['builds']) is list and len(protocol['builds']) == 5, 'five SDK builds required')
    for value, wanted in zip(protocol['builds'], MODES, strict=True):
        check_ledger(value, wanted, modules)
    raw = protocol['raw_validation']
    closed(raw, ['max_cost', 'execution_vm', 'accepted', 'rejected', 'native_evaluations', 'VM_evaluations', 'cases', 'scope'], 'retained raw suite')
    require(raw['execution_vm'] == 'clvmr 0.21.0 MEMPOOL_MODE' and raw['scope'] == RAW_SCOPE, 'primary VM/scope mismatch')
    for key, count in [('max_cost', 1_000_000), ('accepted', 195), ('rejected', 5), ('native_evaluations', 200), ('VM_evaluations', 1000)]:
        require(type(raw[key]) is int and raw[key] == count, 'raw suite count/cost contract')
    require(type(raw['cases']) is list and len(raw['cases']) == 200, 'retained raw row count')
    for row, spec in zip(raw['cases'], specs, strict=True):
        check_raw_case(row, spec)
    return {'component': SLUG, 'independent_raw_vectors': 200, 'SDK_attempts': 60, 'accepted': 20, 'typed_full_Eq_rejected': 40,
            'scope': 'Recorded consistency/source freshness only; no new VM/SDK execution, state equality proof or authentication.'}


def validate_current(report, *, root=ROOT):
    import corpus_append  # Lazy: the generic root owns the complete current coverage.
    require(type(report) is dict and type(report.get('schema_version')) is int and report['schema_version'] == 1, 'current report schema')
    slugs = [row.get('slug') for row in report['puzzles']] if type(report.get('puzzles')) is list else None
    require(slugs == corpus_append.current_slugs(), 'current exact corpus puzzle order required')
    require(report.get('authoring') == snapshot(Path(root)), 'current declared native/Cargo source snapshot differs')
    # This component gate is deliberately not an aggregate cost migration.
    # Historical whole22 projection and all prior-component validation belong
    # to the separately reviewed append transitions, after this gate.
    return validate_component(report['puzzles'][len(OLD_SLUGS)], root=root)
