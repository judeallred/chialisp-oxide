"""Bounded independent registry/slot sidecar reader; recorded consistency only.

Reconstructs exact curries, coins, creator/current-spender proofs, conditions and
causal controls. No VM/SDK execution, BLS verification or authentication credit.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import tempfile
from types import SimpleNamespace

from counter_protocol import codec, oracle

ROOT = Path(__file__).resolve().parents[1]
SLUGS = ('registry-initialize', 'registry-set-alpha', 'registry-set-beta')
MODES = (('rust_mir', False, 'rust-off'), ('rust_mir', True, 'rust-on'),
         ('chialisp', True, 'chialisp'), ('rue_source', False, 'rue-off'),
         ('rue_source', True, 'rue-on'))
TRANSPORT = 'registry-slot-sidecars/v1'
INDEX = 'registry-slot-protocol/index.json'
MEMBER_LIMIT = 2 * 1024 * 1024
TOTAL_LIMIT = 16 * 1024 * 1024
OWNER = 'b1b01d4389e60a12b1c9312312f706825184320ed91f2a8f16510cbea3ea900f4e6b465a045781841997b209017fb6a4'
CANONICAL = (
    ('action_layer', 630, 'afa03f2903f9e4a293523237799074b13ab2361f250df10de5df884b0b09a22b'),
    ('default_finalizer', 494, '9d274de59aeaa128d1b0a802b52911838420ef9720d605c78942e8dadfc0810b'),
    ('singleton_launcher', 175, 'eff07522495060c066f66f32acc2a77e3a3e737aca8baea4d1a64ea4cdc13da9'),
    ('singleton_top_layer_v1_1', 967, '7faa3253bfddd1e0decb0906b2dc6247bbc4cf608f58345d173adb63e8b47c9f'),
    ('slot', 311, '8ab9f3b57a65d7f7a0810f79f7bc1e96bda680d16dd6f51d51e868e13fc7bbb3'),
    ('standard_wallet', 227, 'e9aaa49f45bad5c889b86ee3341550c155cfdd10c3a6757de618d20612fffd52'),
)
HEADER_SCOPE = 'Original fixed two-name teaching actions, all three selected artifacts in each of five builds. Exact primary outputs and typed SDK rejection/full in-memory Simulator equality. Runtime umbrellas have separate independent primary program failures; resources/infrastructure are inconclusive. Native calls, generic five-mode VM vectors, source controls and this composed SDK record remain distinct. No canonical registry port, BLS/general security/uniqueness/compiler-preservation proof, authenticated execution, production readiness, live Testnet11 or complete issue14 claim.'
COUNTS = {
    'ordinary': {'scenario_runs': 1, 'sdk_transaction_attempts': 11, 'accepted': 6,
                 'typed_consensus_rejections': 5, 'component_primary_observations': 19},
    'adversarial': {'scenario_runs': 1, 'sdk_transaction_attempts': 10, 'accepted': 2,
                    'typed_consensus_rejections': 3, 'runtime_umbrellas': 5,
                    'component_primary_observations': 18},
    'adversarial_setup': {'sdk_transaction_attempts': 2, 'accepted': 2,
                          'component_primary_observations': 3},
    'causal_primary': {'primary_observations': 3, 'accepted': 2, 'typed_program_rejections': 1},
}


def require(ok, message):
    if not ok:
        raise ValueError(message)


def closed(value, keys, label):
    require(type(value) is dict and set(value) == set(keys), 'closed ' + label)


def same(actual, expected, label):
    require(json.dumps(actual, sort_keys=True, separators=(',', ':')) ==
            json.dumps(expected, sort_keys=True, separators=(',', ':')), label)


def unique(pairs):
    value = {}
    for key, item in pairs:
        require(key not in value, 'duplicate JSON key')
        value[key] = item
    return value


def no_float(_):
    raise ValueError('float/nonfinite registry JSON forbidden')


def json_value(data):
    require(type(data) is bytes and 0 < len(data) <= TOTAL_LIMIT, 'bounded JSON input')
    # Charge lexical depth before the recursive standard JSON parser.
    depth = 0
    tokens = 0
    quoted = escaped = False
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
        elif byte in (91, 123):
            tokens += 1
            require(tokens <= 250000, 'registry JSON structure budget before parsing')
            depth += 1
            require(depth <= 128, 'registry JSON depth bound')
        elif byte in (93, 125):
            depth -= 1
        elif byte in (44, 58):
            tokens += 1
            require(tokens <= 250000, 'registry JSON structure budget before parsing')
    return json.loads(data, object_pairs_hook=unique, parse_float=no_float, parse_constant=no_float)


def raw_hex(text, maximum=1024 * 1024):
    require(type(text) is str and 0 < len(text) <= maximum * 2 and
            re.fullmatch(r'(?:[0-9a-f]{2})+', text), 'bounded canonical registry hex')
    return bytes.fromhex(text)


def tree(text, maximum=1024 * 1024):
    return codec.decode(raw_hex(text, maximum))


def digest(data):
    return hashlib.sha256(data).hexdigest()


def descriptor(value, name):
    closed(value, ('path', 'bytes', 'sha256'), 'registry descriptor')
    require(value['path'] == name, 'fixed registry sidecar name')
    require(type(value['bytes']) is int and 0 < value['bytes'] <= MEMBER_LIMIT,
            'registry declared member bound before read')
    require(type(value['sha256']) is str and re.fullmatch('[0-9a-f]{64}', value['sha256']),
            'registry descriptor digest')


def safe_stat(root, name, expected):
    current = root
    require(current.is_dir() and not current.is_symlink(), 'real sidecar root')
    for part in name.split('/'):
        current = current / part
        info = current.lstat()
        require(not stat.S_ISLNK(info.st_mode), 'registry sidecar symlink forbidden')
    require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and info.st_size == expected,
            'registry physical member shape/declared size')
    return current


def read_member(path, item):
    # O_NOFOLLOW closes the final-component symlink race; immutable source window
    # remains an explicit assumption, not a filesystem authentication claim.
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        info = os.fstat(fd)
        require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and
                info.st_size == item['bytes'], 'registry member changed before read')
        with os.fdopen(fd, 'rb', closefd=False) as stream:
            data = stream.read(item['bytes'] + 1)
        require(len(data) == item['bytes'] and digest(data) == item['sha256'],
                'exact registry member bytes/hash')
        return data
    finally:
        os.close(fd)


def identity(artifact, slug):
    data = raw_hex(artifact['clvm_hex'])
    require(type(artifact['bytes']) is int and artifact['bytes'] == len(data), 'registry artifact size')
    return {'slug': slug, 'bytes': len(data), 'blob_sha256': digest(data),
            'module_tree_hash': oracle.treehash(codec.decode(data)).hex()}


def validate_report(report, *, sidecar_root=ROOT / 'corpus'):
    """Fully validate one triplet; sidecar_root comes only from the caller."""
    rows = [p for p in report['puzzles'] if p.get('slug') in SLUGS]
    require(len(rows) == 3 and {p['slug'] for p in rows} == set(SLUGS), 'complete registry triplet')
    by_slug = {p['slug']: p for p in rows}
    index_descriptor = None
    for slug in SLUGS:
        row = by_slug[slug]
        require(row.get('kind') == 'protocol_component' and row.get('validated') is True,
                'validated registry component classification')
        require(row.get('simulator') == [] and row.get('simulator_comparisons') == [] and
                'coin_instances' not in row, 'registry not ordinary ledger groups')
        reference = row.get('protocol_validation')
        closed(reference, ('protocol', 'role', 'index'), 'registry reference')
        require(reference['protocol'] == TRANSPORT and reference['role'] == slug,
                'registry reference role/protocol')
        descriptor(reference['index'], INDEX)
        if index_descriptor is None:
            index_descriptor = reference['index']
        same(reference['index'], index_descriptor, 'one shared registry index identity')
    root = Path(sidecar_root)
    index_path = safe_stat(root, INDEX, index_descriptor['bytes'])
    index = json_value(read_member(index_path, index_descriptor))
    closed(index, ('schema_version', 'transport', 'header', 'builds'), 'registry index')
    same(index['schema_version'], 1, 'registry index schema')
    require(index['transport'] == TRANSPORT, 'registry index transport')
    require(type(index['builds']) is list and len(index['builds']) == 5, 'five registry builds')
    inventory = [(index_descriptor, index_path)]
    all_artifacts = []
    total = index_descriptor['bytes']
    module_remaining = 9 * 1024 * 1024
    for build, (backend, optimize, mode) in zip(index['builds'], MODES, strict=True):
        closed(build, ('backend', 'optimize_lir', 'mode', 'artifacts', 'counts', 'journals'), 'registry build')
        same([build['backend'], build['optimize_lir'], build['mode']], [backend, optimize, mode], 'registry build mode')
        same(build['counts'], COUNTS, 'registry lane counts')
        require(type(build['artifacts']) is list and len(build['artifacts']) == 3, 'three registry identities')
        for slug, claimed in zip(SLUGS, build['artifacts'], strict=True):
            artifacts = by_slug[slug].get('artifacts')
            require(type(artifacts) is list and len(artifacts) == 5, 'complete five-artifact entry')
            selected = [a for a in artifacts if a.get('backend') == backend and a.get('optimize_lir') is optimize]
            require(len(selected) == 1, 'unique registry artifact mode')
            artifact = selected[0]
            require(type(artifact.get('bytes')) is int and 0 < artifact['bytes'] <= 1024 * 1024 and
                    artifact['bytes'] <= module_remaining and type(artifact.get('clvm_hex')) is str and
                    len(artifact['clvm_hex']) == 2 * artifact['bytes'], 'complete registry module precharge')
            module_remaining -= artifact['bytes']
            all_artifacts.append((slug, mode, artifact, claimed))
        closed(build['journals'], ('ordinary', 'adversarial'), 'registry journal lanes')
        for lane in ('ordinary', 'adversarial'):
            item = build['journals'][lane]
            name = f'registry-slot-protocol/{mode}-{lane}.json'
            descriptor(item, name)
            require(item['bytes'] <= TOTAL_LIMIT - total, 'registry aggregate precharge before journal reads')
            total += item['bytes']
            inventory.append((item, safe_stat(root, name, item['bytes'])))
    names = {item['path'] for item, _ in inventory}
    require(len(names) == 11, 'closed eleven registry members')
    folder = root / 'registry-slot-protocol'
    with os.scandir(folder) as entries:
        physical = set()
        for entry in entries:
            require(len(physical) < 11 and entry.is_file(follow_symlinks=False), 'bounded physical registry inventory')
            physical.add('registry-slot-protocol/' + entry.name)
    require(physical == names, 'exact physical registry inventory')
    # No journal bytes have been read before the complete declared inventory,
    # aggregate budget and fifteen module lengths are charged.
    base = []
    for slug, mode, artifact, claimed in all_artifacts:
        actual = identity(artifact, slug)
        same(claimed, actual, 'main registry artifact cross-link')
        require(artifact['tree_hash'] == actual['module_tree_hash'], 'main registry module tree hash')
        base.append({'action': ('initialize', 'set_alpha', 'set_beta')[SLUGS.index(slug)],
                     'mode': mode, 'clvm_hex': artifact['clvm_hex']})
    header = index['header']
    closed(header, ('schema_version', 'protocol', 'status', 'primary_vm', 'sdk', 'owner_public_key',
                   'canonical_components', 'independently_compiled_chialisp', 'totals',
                   'source_control_scope', 'scope'), 'registry header')
    same(header['schema_version'], 1, 'registry producer schema')
    require(header['protocol'] == 'chialisp-oxide-action-registry/v1' and header['status'] == 'validated', 'registry producer protocol/status')
    same(header['primary_vm'], {'version':'clvmr 0.21.0', 'failure_protocol':'clvmr-0.21.0/v1', 'flags':535,
                               'max_cost':250000000, 'allocator_limit_bytes':33554432}, 'registry primary profile')
    same(header['sdk'], {'simulator':'chia-sdk-test 0.36.0', 'consensus':'chia-consensus 0.36.1',
                        'vm':'clvmr 0.16.2', 'network':'TESTNET11', 'transaction_max_cost':5500000000,
                        'max_block_cost_clvm':11000000000}, 'registry SDK profile')
    require(header['owner_public_key'] == OWNER, 'registry public fixture owner')
    same(header['totals'], {lane:{key:value*5 for key,value in values.items()} for lane,values in COUNTS.items()}, 'registry group totals')
    require(header['source_control_scope'] == 'No source controls are included in this base producer. Research Chialisp controls and actual Rust source-mutant observations belong to separate captures and receive no credit from this record.', 'registry source-control scope')
    require(header['scope'] == HEADER_SCOPE, 'registry finite scope')
    canonical = {}
    require(type(header['canonical_components']) is list and len(header['canonical_components']) == 6, 'canonical registry inventory')
    for component, (name, size, module_hash) in zip(header['canonical_components'], CANONICAL, strict=True):
        closed(component, ('name','module_hex','bytes','blob_sha256','module_tree_hash'), 'canonical registry component')
        data = raw_hex(component['module_hex'])
        same([component['name'],component['bytes'],len(data),component['blob_sha256'],component['module_tree_hash']],
             [name,size,size,digest(data),module_hash], 'canonical registry identity')
        require(oracle.treehash(codec.decode(data)).hex() == module_hash, 'canonical registry module hash')
        canonical[name] = codec.decode(data)
    references = header['independently_compiled_chialisp']
    require(type(references) is list and len(references) == 3, 'registry independent Chialisp inventory')
    for slug, reference in zip(SLUGS, references, strict=True):
        closed(reference, ('artifact','source_sha256'), 'registry independent reference')
        artifact = next(a for s,m,a,_ in all_artifacts if s == slug and m == 'chialisp')
        same(reference['artifact'], identity(artifact, slug), 'independent Chialisp artifact')
        source = by_slug[slug]['reference']
        require(type(source) is str and reference['source_sha256'] == digest(source.encode()), 'independent Chialisp source digest')
    journals = {item['path']: json_value(read_member(path, item)) for item, path in inventory[1:]}
    for name, journal in journals.items():
        validate_journal_shape(journal, name)
    validate_journals(journals, base, canonical)
    # Recheck the index after admission, preserving the mutable-file assumption.
    require(json_value(read_member(index_path,index_descriptor)) == index, 'registry index stable across read')
    return {'protocol': TRANSPORT, 'status':'recorded_consistency_checked', 'files':11, 'bytes':total,
            'index_sha256':index_descriptor['sha256'], 'totals':header['totals']}



def validate_journal_shape(journal, name):
    ordinary = name.endswith('-ordinary.json')
    fields = ('mode','owner_hex','nonce','merkle_root','curried_actions_hex','slot_first_curry_hex',
              'slot_first_curry_hash','genesis','launcher','controller_progress','slots','attempts',
              'sdk_transaction_attempts','accepted','typed_rejected','component_primary_vm_runs','scope') if ordinary else (
              'mode','owner_hex','genesis','launcher','eve','controller','alpha0','beta0','setup_attempts',
              'setup_sdk_submissions','setup_primary_observations','attempts','sdk_transaction_attempts',
              'accepted','typed_consensus_rejections','runtime_umbrellas','component_primary_vm_runs',
              'causal_primary','causal_primary_observations','scope')
    closed(journal, fields, 'registry journal')
    require(name == f"registry-slot-protocol/{journal['mode']}-{'ordinary' if ordinary else 'adversarial'}.json",
            'registry journal mode cross-link')
    require(journal['owner_hex'] == OWNER, 'journal public owner')
    if ordinary:
        same(journal['nonce'], {'u64_hex':'0x434f5852454731','canonical_atom_hex':'434f5852454731',
                               'decimal_string':'18946064195602225'}, 'opaque nonce exact metadata')
        closed(journal['slots'], ('alpha0','beta0','alpha1','beta1','alpha2','alpha3'), 'registry slots')
    if ordinary:
        require(type(journal['controller_progress']) is list and len(journal['controller_progress']) == 6,
                'exact registry progression inventory')
        for row in journal['controller_progress']:
            closed(row, ('name','coin','state'), 'registry progression')
        for row in journal['slots'].values():
            closed(row, ('coin','creator_controller','creator_state','entry_hash'), 'registry slot record')
    else:
        for name in ('eve','controller'):
            closed(journal[name], ('coin','state'), 'registry adversarial controller')
        for name in ('alpha0','beta0'):
            closed(journal[name], ('coin','creator_controller','creator_state','entry_hash'), 'registry adversarial slot')
    pending = [journal]
    budget = 250000
    numeric = {'action','path','proof_reuse_selector','sdk_transaction_attempts','accepted','typed_rejected',
               'typed_consensus_rejections','runtime_umbrellas','component_primary_vm_runs',
               'setup_sdk_submissions','setup_primary_observations','causal_primary_observations'}
    while pending:
        value = pending.pop()
        budget -= 1
        require(budget >= 0, 'registry JSON node budget')
        if type(value) is dict:
            for key,item in value.items():
                if key in numeric:
                    require(type(item) is int and item >= 0, 'strict registry count/selector')
                if key in ('state','creator_state','actual_state','mutated_state'):
                    closed(item, ('initialized','alpha_hash','beta_hash'), 'registry state')
                if key == 'initialized':
                    require(type(item) is bool, 'strict registry state boolean')
                if key.endswith('_hex') and key != 'u64_hex' and type(item) is str:
                    raw_hex(item, 1024 * 1024)
                pending.append(item)
        elif type(value) is list:
            pending.extend(value)
    if not ordinary:
        require(type(journal['setup_attempts']) is list and len(journal['setup_attempts']) == 2,
                'exact adversarial setup inventory')
        require(type(journal['causal_primary']) is list and len(journal['causal_primary']) == 3,
                'exact causal primary inventory')
        for row in journal['causal_primary']:
            closed(row, ('name','reveal_hex','environment_hex','execution'), 'causal primary row')
            closed(row['execution'], ('outcome','cost','failure'), 'causal primary execution')
            outcome = row['execution']['outcome']
            closed(outcome, ('status','output_hex') if outcome.get('status') == 'accepted' else ('status',),
                   'causal primary outcome')
    attempts = journal['attempts'] + ([] if ordinary else journal['setup_attempts'])
    require(len(journal['attempts']) == (11 if ordinary else 10), 'exact registry attempt inventory')
    for attempt in attempts:
        closed(attempt, ('name','transaction','primary_vm','outcome') if 'mutation' not in attempt else
               ('name','transaction','primary_vm','outcome','mutation'), 'registry attempt')
        closed(attempt['transaction'], ('signature_hex','spends'), 'registry transaction')
        require(len(raw_hex(attempt['transaction']['signature_hex'])) == 96, 'registry aggregate signature width')
        require(type(attempt['transaction']['spends']) is list and 1 <= len(attempt['transaction']['spends']) <= 2,
                'bounded complete registry spends')
        require(type(attempt['primary_vm']) is list and len(attempt['primary_vm']) == len(attempt['transaction']['spends']),
                'complete primary component inventory')
        for spend in attempt['transaction']['spends']:
            closed(spend, ('coin','reveal_hex','solution_hex'), 'registry spend')
            codec.decode(raw_hex(spend['reveal_hex'], 1024*1024))
            codec.decode(raw_hex(spend['solution_hex'], 1024*1024))
        outcome = attempt['outcome']
        closed(outcome, ('status',) if outcome.get('status') == 'accepted' else
               ('status','stage','code','complete_state_unchanged'), 'registry SDK outcome')
        if outcome['status'] != 'accepted':
            require(outcome['complete_state_unchanged'] is True, 'full simulator equality receipt')
        for execution in attempt['primary_vm']:
            closed(execution, ('outcome','cost','failure'), 'registry primary execution')
            if execution['outcome'].get('status') == 'accepted':
                closed(execution['outcome'], ('status','output_hex'), 'accepted primary outcome')
                codec.decode(raw_hex(execution['outcome']['output_hex'], 1024*1024))
            else:
                same(execution, {'outcome':{'status':'rejected'},'cost':None,
                                 'failure':{'code':'raise','kind':'program'}}, 'typed primary program rejection')


def validate_journals(journals, base, canonical):
    m = SimpleNamespace(codec=codec, sha=lambda value: hashlib.sha256(value).digest(), proper=oracle.proper, uint=oracle.uint, curry=oracle.curry, tree_hash=oracle.treehash, entry=lambda name, value: hashlib.sha256(b'\x02' + hashlib.sha256(b'\x01' + name).digest() + hashlib.sha256(b'\x01' + value).digest()).digest(), coin_id=lambda parent, ph, amount: hashlib.sha256(parent + ph + oracle.uint(amount)).digest())
    ACTION = canonical['action_layer']
    FINAL = canonical['default_finalizer']
    SLOT = canonical['slot']
    TOP = canonical['singleton_top_layer_v1_1']
    LAUNCH = canonical['singleton_launcher']
    STD = canonical['standard_wallet']
    report = {'runs': [journals[f'registry-slot-protocol/{mode}-ordinary.json'] for _, _, mode in MODES], 'adversarial_runs': [journals[f'registry-slot-protocol/{mode}-adversarial.json'] for _, _, mode in MODES]}
    
    def exact_hex(actual, tree):
        require(actual == m.codec.encode(tree).hex(), 'independent registry wire/coin/causal relation')
    
    def h(tree):
        return m.tree_hash(tree)
    
    def value_hash(side, value):
        return m.entry(m.sha(('chialisp-oxide/registry/v1/' + side).encode()), value)
    
    def coin(row, amount, parent=None, puzzle=None):
        require(set(row) == {'coin_id', 'parent_coin_id', 'puzzle_hash', 'amount'} and type(row['amount']) is int and (row['amount'] == amount), 'independent registry wire/coin/causal relation')
        raw = [bytes.fromhex(row[k]) for k in ['coin_id', 'parent_coin_id', 'puzzle_hash']]
        require(all((len(v) == 32 for v in raw)), 'independent registry wire/coin/causal relation')
        require(m.coin_id(raw[1], raw[2], amount) == raw[0], 'independent registry wire/coin/causal relation')
        require(parent is None or raw[1] == parent, 'independent registry wire/coin/causal relation')
        require(puzzle is None or raw[2] == puzzle, 'independent registry wire/coin/causal relation')
        return raw[0]
    
    def state(initialized, av, bv):
        return {'initialized': initialized, 'alpha_hash': value_hash('alpha', av).hex(), 'beta_hash': value_hash('beta', bv).hex()}
    
    def state_tree(s):
        require(type(s['initialized']) is bool, 'independent registry wire/coin/causal relation')
        return (m.uint(int(s['initialized'])), (bytes.fromhex(s['alpha_hash']), bytes.fromhex(s['beta_hash'])))
    
    class Policy:
    
        def __init__(self, templates, owner, launcher):
            self.owner = owner
            self.launcher = launcher
            self.ss = (h(TOP), (launcher, h(LAUNCH)))
            self.first = m.curry(SLOT, (h(TOP), h(self.ss)), m.uint(18946064195602225))
            self.first_hash = h(self.first)
            self.actions = [m.curry(templates[0], self.first_hash), m.curry(templates[1], owner, self.first_hash), m.curry(templates[2], owner, self.first_hash)]
            leaves = [m.sha(b'\x01' + h(p)) for p in self.actions]
            self.root = m.sha(b'\x02' + m.sha(b'\x02' + leaves[0] + leaves[1]) + leaves[2])
            first = m.curry(FINAL, h(ACTION), launcher)
            self.final = m.curry(first, h(first))
    
        def inner(self, s):
            return m.curry(ACTION, self.final, self.root, state_tree(s))
    
        def full(self, s):
            return m.curry(TOP, self.ss, self.inner(s))
    
        def slot(self, entry):
            return m.curry(self.first, entry)
    
        def create(self, entry):
            return m.proper(m.uint(51), h(self.slot(entry)), b'', m.proper(self.first_hash))
    
        def solution(self, action, old, new, lineage):
            index = ('initialize','set_alpha','set_beta').index(action)
            leaves = [m.sha(b'\x01'+h(p)) for p in self.actions]
            left = m.sha(b'\x02'+leaves[0]+leaves[1])
            path, siblings = [(0,[leaves[1],leaves[2]]),(1,[leaves[0],leaves[2]]),(1,[left])][index]
            proof = m.proper(m.uint(path), *siblings)
            solution = b'' if index == 0 else m.proper(old,new)
            inside = m.proper(m.proper(self.actions[index]),m.proper((m.uint(2),proof)),m.proper(solution))
            return m.proper(lineage,m.uint(1),inside)

        def conditions(self, current, next_state, action, old, new, control=None):
            result = [m.proper(m.uint(73), m.uint(1)), m.proper(m.uint(71), bytes.fromhex(current['parent_coin_id'])), m.proper(m.uint(51), h(self.full(next_state)), m.uint(1), m.proper(self.launcher))]
            if action == 'initialize':
                result += [self.create(bytes.fromhex(next_state['alpha_hash'])), self.create(bytes.fromhex(next_state['beta_hash']))]
            else:
                side = action[4:]
                domain = m.sha(('chialisp-oxide/registry/v1/set-' + side).encode())
                if control != 'drop_send':
                    result.append(m.proper(m.uint(66), m.uint(18), b'', h(self.slot(value_hash(side, old)))))
                if control != 'drop_create':
                    result.append(self.create(value_hash(side, new)))
                result.append(m.proper(m.uint(50), self.owner, m.sha(domain + old + new)))
            return m.proper(*result)
    
    def checked_attempt(attempt, name, code, expected):
        require(attempt['name'] == name and len(attempt['transaction']['spends']) == len(attempt['primary_vm']) == len(expected), 'independent registry wire/coin/causal relation')
        expected_out = {'status': 'accepted'} if code is None else {'status': 'rejected', 'stage': 'consensus', 'code': code, 'complete_state_unchanged': True}
        require(attempt['outcome'] == expected_out, 'independent registry wire/coin/causal relation')
        if code:
            require(attempt['outcome']['complete_state_unchanged'] is True, 'independent registry wire/coin/causal relation')
        for spend, execution, (expected_coin, reveal, solution, output) in zip(attempt['transaction']['spends'], attempt['primary_vm'], expected, strict=True):
            require(spend['coin'] == expected_coin, 'independent registry wire/coin/causal relation')
            require(h(m.codec.decode(bytes.fromhex(spend['reveal_hex']))).hex() == expected_coin['puzzle_hash'], 'independent registry wire/coin/causal relation')
            exact_hex(spend['reveal_hex'], reveal)
            if solution is not None:
                exact_hex(spend['solution_hex'], solution)
            require(execution['failure'] is None and type(execution['cost']) is int and (0 < execution['cost'] <= 250000000), 'independent registry wire/coin/causal relation')
            require(execution['outcome']['status'] == 'accepted', 'independent registry wire/coin/causal relation')
            exact_hex(execution['outcome']['output_hex'], output)
        require(len(bytes.fromhex(attempt['transaction']['signature_hex'])) == 96, 'independent registry wire/coin/causal relation')
        return attempt['transaction']
    
    def bootstrap(attempts, genesis, launcher, eve, p, initial, initialized):
        wallet = m.curry(STD, p.owner)
        coin(genesis, 1, puzzle=h(wallet))
        launch_solution = m.proper(h(p.full(initial)), m.uint(1), b'')
        announcement = m.sha(bytes.fromhex(launcher['coin_id']) + h(launch_solution))
        fund_conditions = m.proper(m.proper(m.uint(51), h(LAUNCH), m.uint(1)), m.proper(m.uint(61), announcement))
        delegated = (m.uint(1), fund_conditions)
        funding_solution = m.proper(b'', delegated, b'')
        fund_output = (m.proper(m.uint(50), p.owner, h(delegated)), fund_conditions)
        launch_output = m.proper(m.proper(m.uint(51), h(p.full(initial)), m.uint(1)), m.proper(m.uint(60), h(launch_solution)))
        checked_attempt(attempts[0], 'fund_and_launch', None, [(genesis, wallet, funding_solution, fund_output), (launcher, LAUNCH, launch_solution, launch_output)])
        require(attempts[0]['transaction']['signature_hex'] != 'c0' + '00' * 95, 'independent registry wire/coin/causal relation')
        checked_attempt(attempts[1], 'initialize', None, [(eve, p.full(initial), p.solution('initialize',bytes(32),bytes(32),m.proper(bytes.fromhex(genesis['coin_id']),m.uint(1))), p.conditions(eve, initialized, 'initialize', bytes(32), bytes(32)))])
        require(attempts[1]['transaction']['signature_hex'] == 'c0' + '00' * 95, 'independent registry wire/coin/causal relation')
    
    def slot_expect(p, slot, current, current_state):
        creator = slot['creator_controller']
        creator_state = slot['creator_state']
        entry = bytes.fromhex(slot['entry_hash'])
        creator_id = coin(creator, 1, puzzle=h(p.full(creator_state)))
        coin(slot['coin'], 0, parent=creator_id, puzzle=h(p.slot(entry)))
        compact = (bytes.fromhex(creator['parent_coin_id']), (h(p.inner(creator_state)), m.uint(1)))
        solution = (compact, h(p.inner(current_state)))
        output = m.proper(m.proper(m.uint(71), creator_id), m.proper(m.uint(67), m.uint(18), b'', bytes.fromhex(current['puzzle_hash'])))
        return (slot['coin'], p.slot(entry), solution, output)
    ZERO = bytes(32)
    A = bytes([17]) * 32
    B = bytes([34]) * 32
    states = [state(False, ZERO, ZERO), state(True, ZERO, ZERO), state(True, A, ZERO), state(True, A, B), state(True, A, B), state(True, A, B)]
    ordinary_sdk = ordinary_primary = 0
    for run in report['runs']:
        owner = bytes.fromhex(run['owner_hex'])
        genesis = coin(run['genesis'], 1)
        launcher = coin(run['launcher'], 1, parent=genesis, puzzle=h(LAUNCH))
        templates = [m.codec.decode(bytes.fromhex(next((a['clvm_hex'] for a in base if a['action'] == name and a['mode'] == run['mode'])))) for name in ['initialize', 'set_alpha', 'set_beta']]
        p = Policy(templates, owner, launcher)
        require(p.root.hex() == run['merkle_root'], 'independent registry wire/coin/causal relation')
        exact_hex(run['slot_first_curry_hex'], p.first)
        require(p.first_hash.hex() == run['slot_first_curry_hash'], 'independent registry wire/coin/causal relation')
        bootstrap(run['attempts'], run['genesis'], run['launcher'], run['controller_progress'][0]['coin'], p, states[0], states[1])
        for actual, expected in zip(run['curried_actions_hex'], p.actions, strict=True):
            exact_hex(actual, expected)
        progress = run['controller_progress']
        require([r['name'] for r in progress] == ['eve', 'c1', 'c2', 'c3', 'c4', 'c5'], 'independent registry wire/coin/causal relation')
        parent = launcher
        for row, s in zip(progress, states, strict=True):
            require(row['state'] == s, 'independent registry wire/coin/causal relation')
            parent = coin(row['coin'], 1, parent=parent, puzzle=h(p.full(s)))
        slots = run['slots']
        creators = {'alpha0': 0, 'beta0': 0, 'alpha1': 1, 'beta1': 2, 'alpha2': 3, 'alpha3': 4}
        for name, slot in slots.items():
            index = creators[name]
            require(slot['creator_controller'] == progress[index]['coin'] and slot['creator_state'] == states[index], 'independent registry wire/coin/causal relation')
            side = 'alpha' if name.startswith('alpha') else 'beta'
            value = ZERO if name.endswith('0') else A if side == 'alpha' else B
            require(slot['entry_hash'] == value_hash(side, value).hex(), 'independent registry wire/coin/causal relation')
            coin(slot['coin'], 0, parent=bytes.fromhex(progress[index]['coin']['coin_id']), puzzle=h(p.slot(value_hash(side, value))))
        names = ['fund_and_launch', 'initialize', 'missing_owner_signature', 'nonempty_wrong_owner_signature', 'controller_without_slot', 'slot_without_controller', 'set_alpha', 'set_beta_with_older_creator', 'alpha_noop', 'consumed_slot_with_fresh_controller', 'live_slot_same_fresh_controller']
        require([a['name'] for a in run['attempts']] == names, 'independent registry wire/coin/causal relation')
        for i, (current_i, next_i, action, old, new, slot_name, code) in enumerate([(0, 1, 'initialize', ZERO, ZERO, None, None), (1, 2, 'set_alpha', ZERO, A, 'alpha0', 'BadAggregateSignature'), (1, 2, 'set_alpha', ZERO, A, 'alpha0', 'BadAggregateSignature'), (1, 2, 'set_alpha', ZERO, A, None, 'MessageNotSentOrReceived'), (1, 2, None, ZERO, A, 'alpha0', 'MessageNotSentOrReceived'), (1, 2, 'set_alpha', ZERO, A, 'alpha0', None), (2, 3, 'set_beta', ZERO, B, 'beta0', None), (3, 4, 'set_alpha', A, A, 'alpha1', None), (4, 5, 'set_alpha', A, A, 'alpha1', 'DoubleSpend'), (4, 5, 'set_alpha', A, A, 'alpha2', None)], start=1):
            current = progress[current_i]['coin']
            expected = []
            if action is not None:
                lineage = m.proper(bytes.fromhex(run['genesis']['coin_id']),m.uint(1)) if current_i == 0 else m.proper(bytes.fromhex(progress[current_i-1]['coin']['parent_coin_id']),h(p.inner(states[current_i-1])),m.uint(1))
                expected.append((current, p.full(states[current_i]), p.solution(action,old,new,lineage), p.conditions(current, states[next_i], action, old, new)))
            if slot_name:
                expected.append(slot_expect(p, slots[slot_name], current, states[current_i]))
            checked_attempt(run['attempts'][i], names[i], code, expected)
        good = run['attempts'][6]['transaction']
        missing = run['attempts'][2]['transaction']
        wrong = run['attempts'][3]['transaction']
        require(good['spends'] == missing['spends'] == wrong['spends'], 'independent registry wire/coin/causal relation')
        require(missing['signature_hex'] == 'c0' + '00' * 95 and wrong['signature_hex'] != missing['signature_hex'] and (wrong['signature_hex'] != good['signature_hex']), 'independent registry wire/coin/causal relation')
        stale = run['attempts'][9]['transaction']
        live = run['attempts'][10]['transaction']
        require(stale['spends'][0] == live['spends'][0] and stale['signature_hex'] == live['signature_hex'], 'independent registry wire/coin/causal relation')
        require(stale['spends'][1]['coin'] == slots['alpha1']['coin'] and live['spends'][1]['coin'] == slots['alpha2']['coin'], 'independent registry wire/coin/causal relation')
        require(stale['spends'][1]['reveal_hex'] == live['spends'][1]['reveal_hex'], 'independent registry wire/coin/causal relation')
        require(run['attempts'][8]['transaction']['spends'][1]['coin'] == slots['alpha1']['coin'], 'independent registry wire/coin/causal relation')
        require(stale['spends'][0]['coin'] == progress[4]['coin'] != progress[3]['coin'], 'independent registry wire/coin/causal relation')
        require(progress[3]['coin']['puzzle_hash'] == progress[4]['coin']['puzzle_hash'] == progress[5]['coin']['puzzle_hash'], 'independent registry wire/coin/causal relation')
        require(slots['alpha1']['coin']['puzzle_hash'] == slots['alpha2']['coin']['puzzle_hash'], 'independent registry wire/coin/causal relation')
        require(slots['beta0']['creator_controller'] == progress[0]['coin'] != progress[2]['coin'], 'independent registry wire/coin/causal relation')
        require(run['sdk_transaction_attempts'] == 11 and run['accepted'] == 6 and (run['typed_rejected'] == 5) and (run['component_primary_vm_runs'] == 19), 'independent registry wire/coin/causal relation')
        ordinary_sdk += 11
        ordinary_primary += 19
    
    def decode_hex(value):
        return m.codec.decode(bytes.fromhex(value))
    
    def positive_primary(execution, expected):
        require(execution['failure'] is None and execution['outcome']['status'] == 'accepted', 'independent registry wire/coin/causal relation')
        require(type(execution['cost']) is int and 0 < execution['cost'] <= 250000000, 'independent registry wire/coin/causal relation')
        exact_hex(execution['outcome']['output_hex'], expected)
    
    def raising_primary(execution):
        require(execution == {'outcome': {'status': 'rejected'}, 'cost': None, 'failure': {'code': 'raise', 'kind': 'program'}}, 'independent registry wire/coin/causal relation')
    
    def list_tail(tree, n):
        for _ in range(n):
            require(type(tree) is tuple, 'independent registry wire/coin/causal relation')
            tree = tree[1]
        return tree
    adversarial_sdk = adversarial_primary = setup_sdk = causal_primary = 0
    for run in report['adversarial_runs']:
        owner = bytes.fromhex(run['owner_hex'])
        genesis = coin(run['genesis'], 1)
        launcher = coin(run['launcher'], 1, parent=genesis, puzzle=h(LAUNCH))
        templates = [decode_hex(next((a['clvm_hex'] for a in base if a['action'] == name and a['mode'] == run['mode']))) for name in ['initialize', 'set_alpha', 'set_beta']]
        p = Policy(templates, owner, launcher)
        eve = run['eve']['coin']
        c1 = run['controller']['coin']
        require(run['eve']['state'] == states[0] and run['controller']['state'] == states[1], 'independent registry wire/coin/causal relation')
        eve_id = coin(eve, 1, parent=launcher, puzzle=h(p.full(states[0])))
        c1_id = coin(c1, 1, parent=eve_id, puzzle=h(p.full(states[1])))
        bootstrap(run['setup_attempts'], run['genesis'], run['launcher'], eve, p, states[0], states[1])
        alpha = run['alpha0']
        beta = run['beta0']
        require(alpha['creator_controller'] == beta['creator_controller'] == eve, 'independent registry wire/coin/causal relation')
        require(alpha['creator_state'] == beta['creator_state'] == states[0], 'independent registry wire/coin/causal relation')
        require(alpha['entry_hash'] == value_hash('alpha', ZERO).hex() and beta['entry_hash'] == value_hash('beta', ZERO).hex(), 'independent registry wire/coin/causal relation')
        good_slot = slot_expect(p, alpha, c1, states[1])
        slot_expect(p, beta, c1, states[1])
        leaves = [m.sha(b'\x01' + h(action)) for action in p.actions]
        proof = m.proper(m.uint(1), leaves[0], leaves[2])
        old_new = m.proper(ZERO, A)
    
        def inner(selected_proof=proof, solution=old_new):
            return m.proper(m.proper(p.actions[1]), m.proper((m.uint(2), selected_proof)), m.proper(solution))
        lineage = m.proper(bytes.fromhex(eve['parent_coin_id']), h(p.inner(states[0])), m.uint(1))
    
        def full_solution(inside):
            return m.proper(lineage, m.uint(1), inside)
        good_cs = (c1, p.full(states[1]), full_solution(inner()), p.conditions(c1, states[2], 'set_alpha', ZERO, A))
        good_signature = run['attempts'][-1]['transaction']['signature_hex']
        require(good_signature != 'c0' + '00' * 95, 'independent registry wire/coin/causal relation')
        expected = []
        altered = bytes([leaves[0][0] ^ 1]) + leaves[0][1:]
        for missing in [False, True]:
            after = [leaves[0]] if missing else [altered, leaves[2]]
            pr = m.proper(m.uint(1), *after)
            mutation = {'kind': 'remove_last_membership_sibling' if missing else 'xor_first_sibling_byte0', 'action': 1, 'path': 1, 'before_siblings_hex': [leaves[0].hex(), leaves[2].hex()], 'after_siblings_hex': [v.hex() for v in after], 'signature_source': 'valid_same_coin_alpha_transition'}
            expected.append(('missing_membership_sibling' if missing else 'wrong_membership_sibling', 'GeneratorRuntimeError', [(c1, p.full(states[1]), full_solution(inner(pr)), None), good_slot], mutation))
        mutated_creator = dict(eve)
        v = bytes.fromhex(eve['parent_coin_id'])
        v = bytes([v[0] ^ 1]) + v[1:]
        mutated_creator['parent_coin_id'] = v.hex()
        mutated_creator['coin_id'] = m.coin_id(v, h(p.full(states[0])), 1).hex()
        compact = (v, (h(p.inner(states[0])), m.uint(1)))
        output = m.proper(m.proper(m.uint(71), bytes.fromhex(mutated_creator['coin_id'])), m.proper(m.uint(67), m.uint(18), b'', bytes.fromhex(c1['puzzle_hash'])))
        expected.append(('wrong_creator_proof', 'AssertMyParentIdFailed', [good_cs, (alpha['coin'], p.slot(value_hash('alpha', ZERO)), (compact, h(p.inner(states[1]))), output)], {'kind': 'xor_creator_parent_parent_byte0', 'original_creator': eve, 'mutated_creator': mutated_creator, 'unchanged_current_controller': c1}))
        wrong_state = state(True, bytes([68]) * 32, ZERO)
        wrong_hash = h(p.full(wrong_state))
        require(wrong_hash.hex() != c1['puzzle_hash'], 'independent registry wire/coin/causal relation')
        compact = (bytes.fromhex(eve['parent_coin_id']), (h(p.inner(states[0])), m.uint(1)))
        output = m.proper(m.proper(m.uint(71), eve_id), m.proper(m.uint(67), m.uint(18), b'', wrong_hash))
        expected.append(('different_current_spender_inner_hash', 'MessageNotSentOrReceived', [good_cs, (alpha['coin'], p.slot(value_hash('alpha', ZERO)), (compact, h(p.inner(wrong_state))), output)], {'kind': 'replace_spender_inner_hash', 'actual_state': states[1], 'mutated_state': wrong_state, 'actual_full_hash': c1['puzzle_hash'], 'mutated_full_hash': wrong_hash.hex()}))
        wrong_old = bytes([51]) * 32
        expected.append(('wrong_old_value', 'GeneratorRuntimeError', [(c1, p.full(states[1]), full_solution(inner(solution=m.proper(wrong_old, A))), None), good_slot], {'kind': 'replace_old_value', 'original_hex': ZERO.hex(), 'mutated_hex': wrong_old.hex(), 'new_hex': A.hex(), 'signature_source': 'valid_same_coin_alpha_transition; rejected program emits no signature conditions'}))
        initial_proof = m.proper(b'', leaves[1], leaves[2])
        init_inner = m.proper(m.proper(p.actions[0]), m.proper((m.uint(2), initial_proof)), m.proper(b''))
        expected.append(('reinitialize', 'GeneratorRuntimeError', [(c1, p.full(states[1]), full_solution(init_inner), None)], {'kind': 'initialize_initialized_state', 'state': states[1]}))
        two_inner = m.proper(m.proper(p.actions[1]), m.proper((m.uint(2), proof), (m.uint(2), b'')), m.proper(m.proper(ZERO, A), m.proper(A, B)))
        expected.append(('two_registry_actions_one_spend', 'GeneratorRuntimeError', [(c1, p.full(states[1]), full_solution(two_inner), None), good_slot], {'kind': 'repeat_alpha_action_with_verified_selector_reuse', 'first_old_hex': ZERO.hex(), 'first_new_hex': A.hex(), 'second_old_hex': A.hex(), 'second_new_hex': B.hex(), 'proof_reuse_selector': 2}))
        require(len(run['attempts']) == 10, 'independent registry wire/coin/causal relation')
        for actual, (name, code, spends, mutation) in zip(run['attempts'][:7], expected, strict=True):
            require(actual['name'] == name and actual['mutation'] == mutation, 'independent registry wire/coin/causal relation')
            stage = 'consensus_runtime_umbrella' if code == 'GeneratorRuntimeError' else 'consensus'
            require(actual['outcome'] == {'status': 'rejected', 'stage': stage, 'code': code, 'complete_state_unchanged': True}, 'independent registry wire/coin/causal relation')
            require(actual['transaction']['signature_hex'] == ('c0' + '00' * 95 if name == 'reinitialize' else good_signature), 'independent registry wire/coin/causal relation')
            require(len(actual['transaction']['spends']) == len(actual['primary_vm']) == len(spends), 'independent registry wire/coin/causal relation')
            for s, e, (c, reveal, solution, output) in zip(actual['transaction']['spends'], actual['primary_vm'], spends, strict=True):
                require(s['coin'] == c, 'independent registry wire/coin/causal relation')
                coin(c, c['amount'], puzzle=h(reveal))
                exact_hex(s['reveal_hex'], reveal)
                exact_hex(s['solution_hex'], solution)
                raising_primary(e) if output is None else positive_primary(e, output)
        require(len(run['causal_primary']) == 3, 'independent registry wire/coin/causal relation')
        for row, (name, ephemeral, current, old, new, accepted) in zip(run['causal_primary'], [('first_action_nil', b'', states[1], ZERO, A, True), ('second_action_used_ephemeral', b'\x01', states[2], A, B, False), ('second_action_nil_only_change', b'', states[2], A, B, True)], strict=True):
            require(row['name'] == name, 'independent registry wire/coin/causal relation')
            exact_hex(row['reveal_hex'], p.actions[1])
            exact_hex(row['environment_hex'], m.proper((ephemeral, state_tree(current)), old, new))
            if accepted:
                next_state = state(True, new, ZERO)
                action_conditions = list_tail(p.conditions(c1, next_state, 'set_alpha', old, new), 3)
                positive_primary(row['execution'], ((b'\x01', state_tree(next_state)), action_conditions))
            else:
                raising_primary(row['execution'])
        funding = run['attempts'][7]
        look = run['attempts'][8]
        last = run['attempts'][9]
        require(funding['name'] == 'fund_independent_lookalike_slot' and len(funding['transaction']['spends']) == 1, 'independent registry wire/coin/causal relation')
        source = funding['transaction']['spends'][0]['coin']
        source_id = coin(source, 0, puzzle=h(m.curry(STD, owner)))
        look_coin = look['transaction']['spends'][1]['coin']
        coin(look_coin, 0, parent=source_id, puzzle=h(p.slot(value_hash('alpha', ZERO))))
        require(look_coin['parent_coin_id'] != alpha['coin']['parent_coin_id'] and look_coin['puzzle_hash'] == alpha['coin']['puzzle_hash'] and (look_coin['amount'] == alpha['coin']['amount']), 'independent registry wire/coin/causal relation')
        conditions = m.proper(m.proper(m.uint(51), bytes.fromhex(look_coin['puzzle_hash']), b''))
        delegated = (m.uint(1), conditions)
        fund_out = (m.proper(m.uint(50), owner, h(delegated)), conditions)
        checked_attempt(funding, 'fund_independent_lookalike_slot', None, [(source, m.curry(STD, owner), m.proper(b'', delegated, b''), fund_out)])
        expected_slot = (look_coin, good_slot[1], good_slot[2], good_slot[3])
        checked_attempt(look, 'independently_funded_lookalike_parent', 'AssertMyParentIdFailed', [good_cs, expected_slot])
        require(look['mutation'] == {'kind': 'replace_only_slot_coin_with_independently_funded_lookalike', 'funding_source': source, 'original_slot': alpha['coin'], 'lookalike_slot': look_coin, 'creator_proof_source': 'unchanged canonical registry creator proof; its emitted parent assertion must fail for unrelated funding parent', 'same_controller_spend_and_signature': True}, 'independent registry wire/coin/causal relation')
        checked_attempt(last, 'valid_alpha_after_all_rejections', None, [good_cs, good_slot])
        require(look['transaction']['spends'][0] == last['transaction']['spends'][0] and look['transaction']['signature_hex'] == last['transaction']['signature_hex'] == good_signature, 'independent registry wire/coin/causal relation')
        require(look['transaction']['spends'][1]['reveal_hex'] == last['transaction']['spends'][1]['reveal_hex'] and look['transaction']['spends'][1]['solution_hex'] == last['transaction']['spends'][1]['solution_hex'], 'independent registry wire/coin/causal relation')
        require(run['setup_sdk_submissions'] == 2 and run['setup_primary_observations'] == 3 and (run['sdk_transaction_attempts'] == 10) and (run['accepted'] == 2) and (run['typed_consensus_rejections'] == 3) and (run['runtime_umbrellas'] == 5) and (run['component_primary_vm_runs'] == 18) and (run['causal_primary_observations'] == 3), 'independent registry wire/coin/causal relation')
        adversarial_sdk += 10
        adversarial_primary += 18
        setup_sdk += 2
        causal_primary += 3



def corruption_recipes():
    """Constructed corruptions of retained observations, never source/VM runs."""
    return [('spent-slot-fresh-controller', 'ordinary',lambda j:j['attempts'][9]['transaction']['spends'][0].update(coin=j['controller_progress'][3]['coin'])),
    ('older-beta-creator','ordinary',lambda j:j['slots']['beta0'].update(creator_controller=j['controller_progress'][2]['coin'])),
    ('current-spender-wire','ordinary',lambda j:j['attempts'][7]['transaction']['spends'][1].update(solution_hex='80')),
    ('ordinary-controller-wire','ordinary',lambda j:j['attempts'][6]['transaction']['spends'][0].update(solution_hex='80')),
    ('signature-isolation','ordinary',lambda j:j['attempts'][9]['transaction'].update(signature_hex='c0'+'00'*95)),
    ('wrong-membership','adversarial',lambda j:j['attempts'][0]['transaction']['spends'][0].update(solution_hex='80')),
    ('creator-output','adversarial',lambda j:j['attempts'][2]['primary_vm'][1]['outcome'].update(output_hex='80')),
    ('spender-distinct','adversarial',lambda j:j['attempts'][3]['mutation'].update(mutated_full_hash=j['attempts'][3]['mutation']['actual_full_hash'])),
    ('umbrella','adversarial',lambda j:j['attempts'][4]['outcome'].update(code='AssertMyParentIdFailed')),
    ('reuse-selector','adversarial',lambda j:j['attempts'][6]['mutation'].update(proof_reuse_selector=True)),
    ('causal-env','adversarial',lambda j:j['causal_primary'][1].update(environment_hex=j['causal_primary'][2]['environment_hex'])),
    ('lookalike-parent','adversarial',lambda j:j['attempts'][8]['transaction']['spends'][1]['coin'].update(parent_coin_id='00'*32))]


def self_test(report, *, sidecar_root=ROOT / 'corpus'):
    """Admit the actual input first; corrupt bounded private record copies only."""
    admitted = validate_report(report, sidecar_root=sidecar_root)
    rows = [copy.deepcopy(p) for p in report['puzzles'] if p.get('slug') in SLUGS]
    original_report = {'puzzles': rows}
    root = Path(sidecar_root)
    item = rows[0]['protocol_validation']['index']
    index_raw = read_member(root / INDEX, item)
    index = json_value(index_raw)
    descriptors = [item] + [b['journals'][lane] for b in index['builds'] for lane in ('ordinary','adversarial')]
    originals = {item['path']: read_member(root / item['path'], item) for item in descriptors}
    require(sum(map(len, originals.values())) == admitted['bytes'], 'self-test complete bounded source inventory')
    rejected = []
    with tempfile.TemporaryDirectory(prefix='registry-record-controls-') as directory:
        private = Path(directory)
        (private / 'registry-slot-protocol').mkdir()
        for name, data in originals.items():
            (private / name).write_bytes(data)

        def write_index(value, record):
            data = (json.dumps(value, indent=2) + '\n').encode()
            require(len(data) <= MEMBER_LIMIT, 'bounded constructed index')
            (private / INDEX).write_bytes(data)
            for row in record['puzzles']:
                row['protocol_validation']['index'] = {'path':INDEX,'bytes':len(data),'sha256':digest(data)}

        def reject(label, record):
            try:
                validate_report(record, sidecar_root=private)
            except ValueError:
                rejected.append(label)
            else:
                raise ValueError('accepted constructed registry corruption ' + label)

        for label,lane,mutate in corruption_recipes():
            record = copy.deepcopy(original_report)
            altered = copy.deepcopy(index)
            name = f'registry-slot-protocol/rust-off-{lane}.json'
            journal = json_value(originals[name])
            mutate(journal)
            data = (json.dumps(journal, indent=2) + '\n').encode()
            require(len(data) <= MEMBER_LIMIT, 'bounded constructed journal')
            (private / name).write_bytes(data)
            build = next(b for b in altered['builds'] if b['mode'] == 'rust-off')
            build['journals'][lane].update(bytes=len(data),sha256=digest(data))
            write_index(altered,record)
            reject(label,record)
            (private / name).write_bytes(originals[name])

        for label,mutate in (
            ('oversized-declared-member',lambda i:i['builds'][4]['journals']['adversarial'].update(bytes=MEMBER_LIMIT+1)),
            ('boolean-declared-size',lambda i:i['builds'][0]['journals']['ordinary'].update(bytes=True)),
            ('unconfined-member-name',lambda i:i['builds'][0]['journals']['ordinary'].update(path='../outside')),
        ):
            record=copy.deepcopy(original_report);altered=copy.deepcopy(index)
            mutate(altered);write_index(altered,record);reject(label,record)
        record=copy.deepcopy(original_report);write_index(index,record)
        artifact=next(a for a in record['puzzles'][2]['artifacts'] if a['backend']=='rue_source' and a['optimize_lir'] is True)
        artifact['clvm_hex']='8080';reject('unselected-mode-artifact',record)
        record=copy.deepcopy(original_report);write_index(index,record)
        name='registry-slot-protocol/rust-off-ordinary.json'
        (private/name).unlink();(private/name).symlink_to(private/INDEX)
        reject('physical-member-symlink',record)
    return {'count':len(rejected),'rejected':rejected,
            'scope':'Constructed retained-record corruption controls only; no VM/SDK/source-mutation or execution-authentication credit.'}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path)
    parser.add_argument('--stdin', action='store_true')
    parser.add_argument('--sidecar-root', type=Path)
    parser.add_argument('--repeat-report', type=Path)
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    require(bool(args.report) != args.stdin, 'choose one report input')
    if args.stdin:
        import sys
        data = sys.stdin.buffer.read(TOTAL_LIMIT + 1)
    else:
        require(args.report.stat().st_size <= TOTAL_LIMIT, 'bounded main report')
        data = args.report.read_bytes()
    sidecar_root = args.sidecar_root or (args.report.parent if args.report else ROOT / 'corpus')
    result = validate_report(json_value(data), sidecar_root=sidecar_root)
    if args.self_test:
        result['corruption_controls'] = self_test(json_value(data), sidecar_root=sidecar_root)
    if args.repeat_report:
        require(args.report is not None, 'repeat requires file report')
        require(args.repeat_report.stat().st_size <= TOTAL_LIMIT, 'bounded repeat report')
        repeated = args.repeat_report.read_bytes()
        validate_report(json_value(repeated), sidecar_root=args.repeat_report.parent)
        require(data == repeated, 'byte-identical independent main report repeat')
        document = json_value(data)
        index_item = next(p for p in document['puzzles'] if p['slug'] == SLUGS[0])['protocol_validation']['index']
        first_index = read_member(sidecar_root / INDEX, index_item)
        second_index = read_member(args.repeat_report.parent / INDEX, index_item)
        require(first_index == second_index, 'byte-identical independent index repeat')
        admitted_index = json_value(first_index)
        for build in admitted_index['builds']:
            for lane in ('ordinary','adversarial'):
                item = build['journals'][lane]
                require(read_member(sidecar_root / item['path'], item) ==
                        read_member(args.repeat_report.parent / item['path'], item),
                        'byte-identical independent sidecar repeat')
        result['repeat_byte_identical'] = True
    result['scope'] = 'Recorded consistency reader only; no new execution/authentication or preservation credit.'
    print(json.dumps(result))


if __name__ == '__main__':
    main()
