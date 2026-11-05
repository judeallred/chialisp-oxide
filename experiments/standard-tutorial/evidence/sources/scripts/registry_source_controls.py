"""Independent bounded SDK source-control record consistency; never executes tools.

Complete Cargo bundle/source/native/tool-window admission is the capture layer's
separate responsibility. A matching record does not authenticate execution,
verify BLS, prove compiler preservation, or add native/template VM observations.
"""
import copy
import hashlib
import json

import registry_protocol as r

LIMIT = 1024 * 1024
TOTAL = 16 * LIMIT
CONTROLS = (('alpha', 'drop_send'), ('alpha', 'drop_create'),
            ('beta', 'drop_send'), ('beta', 'drop_create'))
COUNTS = {'groups': 8, 'setup_sdk_attempts': 16, 'setup_primary_observations': 24,
          'mutation_sdk_attempts': 8, 'mutation_primary_observations': 12,
          'accepted_sdk_attempts': 24}
SCOPE = "Local own-root SDK observations of deliberately omitted returned conditions in the original teaching actions. All partner modules are independently compiled Chialisp. Native observations and template VM replays belong to the caller's separate capture. Two-condition mutant outputs intentionally differ from the original output ABI; inputs and their source recipes are admitted separately. Repeats add no coverage. No live funds, BLS/security proof, authenticated execution, compiler preservation or general registry claim."
EMPTY_SIGNATURE = 'c0' + '00' * 95
require, closed, same = r.require, r.closed, r.same
codec, o = r.codec, r.oracle
proper, uint, curry, h = o.proper, o.uint, o.curry, o.treehash
sha = lambda data: hashlib.sha256(data).digest()


def load_bytes(raw):
    """Use the existing duplicate-key/depth/token bounded parser before validation."""
    return r.json_value(raw)


def bounded_value(value, limit):
    """Charge an already parsed value before recursive encoding or comparison.

    File/transport callers still bound bytes before parsing via load_bytes().
    This catches direct API callers and enforces the runner's per-group role.
    """
    pending = [(value, 0)]
    remaining = 250000
    while pending:
        item, depth = pending.pop()
        require(depth <= 128, 'control JSON depth before recursive encoding')
        if type(item) is dict:
            require(len(item) <= remaining, 'control JSON structure precharge')
            remaining -= len(item)
            require(all(type(k) is str and len(k) <= 128 for k in item), 'bounded JSON field names')
            pending.extend((v, depth + 1) for v in item.values())
        elif type(item) is list:
            require(len(item) <= remaining, 'control JSON array precharge')
            remaining -= len(item)
            pending.extend((v, depth + 1) for v in item)
        elif type(item) is str:
            require(len(item) <= 2 * LIMIT, 'control JSON string bound')
        elif type(item) is int:
            require(0 <= item < 2 ** 64, 'bounded unsigned JSON integer')
        else:
            require(item is None or type(item) is bool, 'exact supported JSON scalar')
    # Sum bounded chunks without materializing a complete encoded copy.
    remaining_bytes = limit
    for chunk in json.JSONEncoder(ensure_ascii=False, indent=2).iterencode(value):
        size = len(chunk.encode())
        require(size <= remaining_bytes, 'control serialized role byte bound')
        remaining_bytes -= size
    require(remaining_bytes >= 1, 'control role includes final newline')


def checked_hex(text, limit=LIMIT):
    return r.raw_hex(text, limit)


def hash32(text):
    raw = checked_hex(text, 32)
    require(len(raw) == 32, 'exact hash32')
    return raw


def descriptor(module, raw):
    return {'bytes': len(raw), 'blob_sha256': r.digest(raw),
            'module_tree_hash': h(module).hex()}


def check_coin(row, amount, parent=None, puzzle=None):
    closed(row, ('coin_id', 'parent_coin_id', 'puzzle_hash', 'amount'), 'control coin')
    require(type(row['amount']) is int and row['amount'] == amount, 'exact coin amount')
    identity, actual_parent, ph = [hash32(row[k]) for k in ('coin_id', 'parent_coin_id', 'puzzle_hash')]
    require(identity == sha(actual_parent + ph + uint(amount)), 'independent coin identity')
    require(parent is None or actual_parent == parent, 'actual coin parent relation')
    require(puzzle is None or ph == puzzle, 'actual coin puzzle commitment')
    return identity


def coin(parent, puzzle, amount):
    return {'coin_id': sha(parent + puzzle + uint(amount)).hex(),
            'parent_coin_id': parent.hex(), 'puzzle_hash': puzzle.hex(), 'amount': amount}


def entry(side, value):
    return sha(b'\x02' + sha(b'\x01' + sha(('chialisp-oxide/registry/v1/' + side).encode()))
               + sha(b'\x01' + value))


def state(initialized, alpha=bytes(32), beta=bytes(32)):
    return {'initialized': initialized, 'alpha_hash': entry('alpha', alpha).hex(),
            'beta_hash': entry('beta', beta).hex()}


def state_tree(value):
    closed(value, ('initialized', 'alpha_hash', 'beta_hash'), 'control state')
    require(type(value['initialized']) is bool, 'strict initialized bool')
    return (uint(int(value['initialized'])), (hash32(value['alpha_hash']), hash32(value['beta_hash'])))


class Policy:
    def __init__(self, templates, owner, launcher, canonical):
        self.c = canonical
        self.owner, self.launcher = owner, launcher
        self.ss = (h(canonical['singleton_top_layer_v1_1']),
                   (launcher, h(canonical['singleton_launcher'])))
        self.first = curry(canonical['slot'],
                           (h(canonical['singleton_top_layer_v1_1']), h(self.ss)), uint(18946064195602225))
        self.first_hash = h(self.first)
        self.actions = [curry(templates[0], self.first_hash),
                        curry(templates[1], owner, self.first_hash),
                        curry(templates[2], owner, self.first_hash)]
        self.leaves = [sha(b'\x01' + h(a)) for a in self.actions]
        self.left = sha(b'\x02' + self.leaves[0] + self.leaves[1])
        self.root = sha(b'\x02' + self.left + self.leaves[2])
        first = curry(canonical['default_finalizer'], h(canonical['action_layer']), launcher)
        self.final = curry(first, h(first))

    def inner(self, s):
        return curry(self.c['action_layer'], self.final, self.root, state_tree(s))

    def full(self, s):
        return curry(self.c['singleton_top_layer_v1_1'], self.ss, self.inner(s))

    def slot(self, e):
        return curry(self.first, e)

    def create(self, e):
        return proper(uint(51), h(self.slot(e)), b'', proper(self.first_hash))

    def solution(self, index, old, new, lineage):
        path, siblings = [(0, self.leaves[1:]), (1, [self.leaves[0], self.leaves[2]]),
                          (1, [self.left])][index]
        proof = proper(uint(path), *siblings)
        solution = b'' if index == 0 else proper(old, new)
        inside = proper(proper(self.actions[index]), proper((uint(2), proof)), proper(solution))
        return proper(lineage, uint(1), inside)

    def conditions(self, current, next_state, index, old, new):
        result = [proper(uint(73), uint(1)), proper(uint(71), hash32(current['parent_coin_id'])),
                  proper(uint(51), h(self.full(next_state)), uint(1), proper(self.launcher))]
        if index == 0:
            result += [self.create(hash32(next_state['alpha_hash'])), self.create(hash32(next_state['beta_hash']))]
        else:
            side = ('alpha', 'beta')[index - 1]
            result += [proper(uint(66), uint(18), b'', h(self.slot(entry(side, old)))),
                       self.create(entry(side, new)),
                       proper(uint(50), self.owner,
                              sha(sha(('chialisp-oxide/registry/v1/set-' + side).encode()) + old + new))]
        return result


def attempt(actual, name, expected, empty_signature):
    closed(actual, ('name', 'transaction', 'primary_vm', 'outcome'), 'control SDK attempt')
    require(actual['name'] == name, 'exact control attempt name')
    same(actual['outcome'], {'status': 'accepted'}, 'actual accepted SDK outcome required')
    tx = actual['transaction']
    closed(tx, ('signature_hex', 'spends'), 'control transaction')
    require(len(checked_hex(tx['signature_hex'], 96)) == 96, 'aggregate signature width')
    require((tx['signature_hex'] == EMPTY_SIGNATURE) is empty_signature, 'empty/nonempty signature role')
    require(type(tx['spends']) is list and type(actual['primary_vm']) is list and
            len(tx['spends']) == len(actual['primary_vm']) == len(expected), 'complete spend/primary inventory')
    for spend, execution, (expected_coin, reveal, solution, output) in zip(tx['spends'], actual['primary_vm'], expected, strict=True):
        closed(spend, ('coin', 'reveal_hex', 'solution_hex'), 'control spend')
        same(spend['coin'], expected_coin, 'exact spent coin')
        require(spend['reveal_hex'] == codec.encode(reveal).hex() and
                spend['solution_hex'] == codec.encode(solution).hex(), 'exact independent control reveal/solution')
        closed(execution, ('outcome', 'cost', 'failure'), 'control primary execution')
        require(execution['failure'] is None and type(execution['cost']) is int and
                0 < execution['cost'] <= 250000000, 'bounded successful primary observation')
        same(execution['outcome'], {'status': 'accepted', 'output_hex': codec.encode(output).hex()},
             'exact independent primary output')
    return len(expected)


def validate_capture(capture, request, reference_inputs, canonical_components):
    """Admit every artifact before checking all eight journals; no selected-group API.

    reference_inputs are caller-admitted rows {slug,source,module_hex}; canonical
    rows have the base registry header's exact six-component shape. Neither is
    fetched from disk or network here. Full source authenticity remains separate.
    """
    for value in (capture, request, reference_inputs, canonical_components):
        bounded_value(value, TOTAL)
    closed(capture, ('protocol', 'sdk', 'consensus', 'vm', 'network', 'rejection_protocol', 'record'), 'control capture')
    same({k: capture[k] for k in capture if k != 'record'}, {
        'protocol': 'chialisp-oxide-registry-source-controls-capture/v1', 'sdk': 'chia-sdk-test 0.36.0',
        'consensus': 'chia-consensus 0.36.1', 'vm': 'clvmr 0.16.2', 'network': 'TESTNET11',
        'rejection_protocol': 'chia-consensus-0.36.1/v1'}, 'exact SDK capture profile')
    record = capture['record']
    closed(record, ('schema_version', 'protocol', 'reference_artifacts', 'groups', 'counts', 'scope'), 'control record')
    same(record['schema_version'], 1, 'control schema')
    require(record['protocol'] == 'chialisp-oxide-registry-source-controls/v1' and record['scope'] == SCOPE, 'control protocol/scope')
    same(record['counts'], COUNTS, 'separate control lane counts')
    closed(request, ('protocol', 'runs'), 'control request')
    require(request['protocol'] == 'chialisp-oxide-registry-source-controls-request/v1', 'request protocol')
    require(type(request['runs']) is list and len(request['runs']) == 8, 'complete control artifact inventory')
    require(type(reference_inputs) is list and len(reference_inputs) == 3, 'complete reference inventory')
    require(type(canonical_components) is list and len(canonical_components) == 6, 'complete canonical inventory')
    pending = []
    remaining = 9 * LIMIT
    for index, row in enumerate(request['runs']):
        side, kind = CONTROLS[index // 2]
        closed(row, ('name', 'optimize_lir', 'entry', 'module_hex'), 'control request row')
        same([row['name'], row['optimize_lir'], row['entry']],
             [side + '-' + kind.replace('_', '-'), bool(index % 2),
              f'chialisp_oxide_registry_controls::{side}_{kind}::puzzle'], 'ordered source control identity/profile')
        text = row['module_hex']
        require(type(text) is str and 0 < len(text) <= 2 * LIMIT and len(text) % 2 == 0 and
                len(text) // 2 <= remaining, 'all control module lengths precharged')
        remaining -= len(text) // 2
        pending.append(text)
    for slug, row in zip(r.SLUGS, reference_inputs, strict=True):
        closed(row, ('slug', 'source', 'module_hex'), 'control reference input')
        require(row['slug'] == slug and type(row['source']) is str and
                0 < len(row['source']) <= LIMIT and len(row['source'].encode()) <= LIMIT, 'reference source identity/bound')
        text = row['module_hex']
        require(type(text) is str and 0 < len(text) <= 2 * LIMIT and len(text) % 2 == 0 and
                len(text) // 2 <= remaining, 'all reference module lengths precharged')
        remaining -= len(text) // 2
        pending.append(text)
    # No candidate or reference bytes decoded until the complete inventory fits.
    blobs = [checked_hex(text) for text in pending]
    decoded = [codec.decode(raw) for raw in blobs]
    modules, refs = decoded[:8], decoded[8:]
    canonical = {}
    for row, (name, size, pin) in zip(canonical_components, r.CANONICAL, strict=True):
        closed(row, ('name', 'module_hex', 'bytes', 'blob_sha256', 'module_tree_hash'), 'canonical control component')
        data = checked_hex(row['module_hex'], size)
        same([row['name'], row['bytes'], len(data), row['blob_sha256'], row['module_tree_hash']],
             [name, size, size, r.digest(data), pin], 'canonical component pin/identity')
        module = codec.decode(data)
        require(h(module).hex() == pin, 'independent pinned canonical module hash')
        canonical[name] = module
    require(type(record['reference_artifacts']) is list and len(record['reference_artifacts']) == 3, 'complete reference artifact crosslinks')
    for actual, row, module, raw in zip(record['reference_artifacts'], reference_inputs, refs, blobs[8:], strict=True):
        same(actual, {'artifact': descriptor(module, raw), 'source_sha256': r.digest(row['source'].encode())}, 'reference artifact/source crosslink')
    require(type(record['groups']) is list and len(record['groups']) == 8, 'all eight control journals')
    for group in record['groups']:
        bounded_value(group, 2 * LIMIT)
    setup_primary = mutation_primary = 0
    for index, (group, module) in enumerate(zip(record['groups'], modules, strict=True)):
        side, kind = CONTROLS[index // 2]
        closed(group, ('side', 'kind', 'backend', 'optimize_lir', 'entry', 'artifact', 'setup', 'mutation', 'policy', 'postconditions'), 'control journal')
        same([group['side'], group['kind'], group['backend'], group['optimize_lir'], group['entry'], group['artifact']],
             [side, kind, 'rust_mir', bool(index % 2), request['runs'][index]['entry'], descriptor(module, blobs[index])], 'control journal/artifact identity')
        policy = group['policy']
        closed(policy, ('merkle_root', 'curried_actions_hex', 'genesis', 'launcher', 'eve', 'controller_before', 'state_before', 'state_after', 'old_slot', 'peer_slot', 'omitted_original_condition_hex'), 'control policy')
        genesis_id = check_coin(policy['genesis'], 1)
        launcher_id = check_coin(policy['launcher'], 1, genesis_id, h(canonical['singleton_launcher']))
        selected = list(refs)
        selected[1 if side == 'alpha' else 2] = module
        p = Policy(selected, bytes.fromhex(r.OWNER), launcher_id, canonical)
        require(policy['merkle_root'] == p.root.hex(), 'own mutant action root before funding')
        same(policy['curried_actions_hex'], [codec.encode(a).hex() for a in p.actions], 'complete own-root curried action set')
        initial, initialized = state(False), state(True)
        next_state = state(True, *([bytes([17]) * 32, bytes(32)] if side == 'alpha' else [bytes(32), bytes([17]) * 32]))
        same(policy['state_before'], initialized, 'exact initialized control state')
        same(policy['state_after'], next_state, 'exact updated state/unchanged peer')
        eve_id = check_coin(policy['eve'], 1, launcher_id, h(p.full(initial)))
        current_id = check_coin(policy['controller_before'], 1, eve_id, h(p.full(initialized)))
        same(policy['old_slot'], {'coin': coin(eve_id, h(p.slot(entry(side, bytes(32)))), 0),
             'creator_controller': policy['eve'], 'creator_state': initial, 'entry_hash': entry(side, bytes(32)).hex()}, 'actual old slot/creator')
        peer = 'beta' if side == 'alpha' else 'alpha'
        same(policy['peer_slot'], {'coin': coin(eve_id, h(p.slot(entry(peer, bytes(32)))), 0),
             'creator_controller': policy['eve'], 'creator_state': initial, 'entry_hash': entry(peer, bytes(32)).hex()}, 'actual peer slot/creator')
        wallet = curry(canonical['standard_wallet'], p.owner)
        check_coin(policy['genesis'], 1, puzzle=h(wallet))
        launch_solution = proper(h(p.full(initial)), uint(1), b'')
        fund_conditions = proper(proper(uint(51), h(canonical['singleton_launcher']), uint(1)),
                                 proper(uint(61), sha(launcher_id + h(launch_solution))))
        delegated = (uint(1), fund_conditions)
        funding_solution = proper(b'', delegated, b'')
        fund_output = (proper(uint(50), p.owner, h(delegated)), fund_conditions)
        launch_output = proper(proper(uint(51), h(p.full(initial)), uint(1)), proper(uint(60), h(launch_solution)))
        require(type(group['setup']) is list and len(group['setup']) == 2, 'exact two setup attempts')
        setup_primary += attempt(group['setup'][0], 'fund_and_launch', [
            (policy['genesis'], wallet, funding_solution, fund_output),
            (policy['launcher'], canonical['singleton_launcher'], launch_solution, launch_output)], False)
        setup_primary += attempt(group['setup'][1], 'initialize', [
            (policy['eve'], p.full(initial), p.solution(0, bytes(32), bytes(32), proper(genesis_id, uint(1))),
             proper(*p.conditions(policy['eve'], initialized, 0, bytes(32), bytes(32))))], True)
        action_index = 1 if side == 'alpha' else 2
        original_conditions = p.conditions(policy['controller_before'], next_state, action_index, bytes(32), bytes([17]) * 32)
        omitted = original_conditions.pop(3 if kind == 'drop_send' else 4)
        require(policy['omitted_original_condition_hex'] == codec.encode(omitted).hex(), 'exact omitted returned condition')
        lineage = proper(hash32(policy['eve']['parent_coin_id']), h(p.inner(initial)), uint(1))
        expected = [(policy['controller_before'], p.full(initialized),
                     p.solution(action_index, bytes(32), bytes([17]) * 32, lineage), proper(*original_conditions))]
        if kind == 'drop_create':
            slot = policy['old_slot']
            compact = (hash32(policy['eve']['parent_coin_id']), (h(p.inner(initial)), uint(1)))
            slot_solution = (compact, h(p.inner(initialized)))
            slot_output = proper(proper(uint(71), eve_id), proper(uint(67), uint(18), b'', hash32(policy['controller_before']['puzzle_hash'])))
            expected.append((slot['coin'], p.slot(entry(side, bytes(32))), slot_solution, slot_output))
        mutation_primary += attempt(group['mutation'], 'selected_condition_omitted', expected, False)
        replacement_hash = h(p.slot(entry(side, bytes([17]) * 32)))
        children = [coin(current_id, h(p.full(next_state)), 1)]
        if kind == 'drop_send':
            children.append(coin(current_id, replacement_hash, 0))
        same(group['postconditions'], {
            'old_slot_spent': kind == 'drop_create', 'peer_slot_unspent': True,
            'replacement_puzzle_hash': replacement_hash.hex(), 'replacement_amount': 0,
            'replacement_child_count': int(kind == 'drop_send'), 'controller_children': children,
            'old_slot_state_query': 'Simulator::coin_state(...).spent_height',
            'replacement_query': 'Simulator::children(controller_before.coin_id) filtered by exact replacement puzzle hash and amount'},
            'exact simulator query postcondition records')
    require(setup_primary == 24 and mutation_primary == 12, 'independent counted primary lanes')
    return {'status': 'recorded_consistency_checked', 'protocol': capture['protocol'], 'counts': COUNTS,
            'scope': 'SDK record consistency only; no BLS verification, execution authentication, native/template execution or compiler-preservation credit.'}


def corruption_controls(capture, request, reference_inputs, canonical_components):
    """Constructed corruption checks require a separately admitted positive fixture.

    They add no executed policy/consensus observations. Inputs are copied so the
    retained positive fixture is never mutated or silently repaired.
    """
    validate_capture(capture, request, reference_inputs, canonical_components)
    mutations = [
        ('missing_last_group', lambda c: c['record']['groups'].pop()),
        ('boolean_count', lambda c: c['record']['counts'].update(groups=True)),
        ('mode', lambda c: c['record']['groups'][7].update(optimize_lir=False)),
        ('artifact_digest', lambda c: c['record']['groups'][0]['artifact'].update(blob_sha256='00' * 32)),
        ('owner_root', lambda c: c['record']['groups'][0]['policy'].update(merkle_root='00' * 32)),
        ('false_old_slot_spent', lambda c: c['record']['groups'][0]['postconditions'].update(old_slot_spent=True)),
        ('invented_replacement', lambda c: c['record']['groups'][2]['postconditions'].update(replacement_child_count=1)),
        ('peer_not_preserved', lambda c: c['record']['groups'][0]['postconditions'].update(peer_slot_unspent=False)),
        ('unknown_postcondition', lambda c: c['record']['groups'][0]['postconditions'].update(unreviewed=True)),
        ('mutant_not_funded', lambda c: c['record']['groups'][0]['setup'][0]['transaction']['spends'][1].update(reveal_hex='80')),
        ('changed_condition_output', lambda c: c['record']['groups'][0]['mutation']['primary_vm'][0]['outcome'].update(output_hex='80')),
        ('resource_as_acceptance', lambda c: c['record']['groups'][0]['mutation']['primary_vm'][0].update(failure={'kind':'resource','code':'cost_exceeded'})),
        ('boolean_cost', lambda c: c['record']['groups'][0]['mutation']['primary_vm'][0].update(cost=True)),
        ('wrong_network', lambda c: c.update(network='MAINNET')),
        ('missing_signature', lambda c: c['record']['groups'][0]['mutation']['transaction'].update(signature_hex=EMPTY_SIGNATURE)),
    ]
    for label, mutate in mutations:
        changed = copy.deepcopy(capture)
        mutate(changed)
        try:
            validate_capture(changed, request, reference_inputs, canonical_components)
        except (ValueError, KeyError, TypeError):
            continue
        raise ValueError('corruption accepted: ' + label)
    return {'constructed_corruption_controls': len(mutations), 'scope': 'record-reader negative controls only'}
