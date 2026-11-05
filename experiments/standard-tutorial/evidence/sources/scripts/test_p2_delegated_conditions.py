"""Constructed reader/spec controls; these launch no Rust, VM, SDK or producer.

Synthetic signatures below are deliberately NOT BLS-valid. They test recorded
field/recipe consistency only, never an actual spend or authentication claim.
Actual integration must separately run the Rust SDK producer and fresh exports.
"""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import p2_delegated_conditions_vectors as vectors
import p2_delegated_conditions_evidence as evidence
from hashing_vectors import deserialize, proper, serialize, sha_tree

ROOT = Path(__file__).resolve().parents[1]


def modules():
    result = {mode: f'constructed-{index}'.encode() for index, mode in enumerate(vectors.MODES)}
    result[('chialisp', True)] = deserialize(bytes.fromhex((ROOT / vectors.ARTIFACT).read_text().strip()))
    return result


def fixture(wanted=('chialisp', True)):
    """Constructed metadata, not outputs of a compiler, VM or SDK."""
    selected = modules()
    module = selected[wanted]
    owner_key, attacker_key = bytes.fromhex(evidence.OWNER), bytes.fromhex(evidence.ATTACKER)
    owner, attacker = evidence.curry(module, owner_key), evidence.curry(module, attacker_key)
    owner_hash = sha_tree(owner).hex()
    def coin(parent, puzzle, value=1000):
        raw = value.to_bytes((value.bit_length()+7)//8, 'big')
        if raw and raw[0] >= 128: raw = b'\0' + raw
        return {'parent_id': parent, 'puzzle_hash': puzzle, 'amount': value,
                'id': hashlib.sha256(bytes.fromhex(parent+puzzle)+raw).hexdigest()}
    selected_coin = coin(evidence.PARENT, owner_hash)
    raw_coin = coin(evidence.PARENT, sha_tree(module).hex())
    def tx(c, reveal, solution, signature):
        return {'signature_hex': (evidence.EMPTY_SIGNATURE.hex() if signature == 0 else bytes([signature]).hex()*96),
                'spends': [{'coin': c, 'reveal_hex': serialize(reveal).hex(), 'solution_hex': serialize(solution).hex()}]}
    regular = proper(proper(b'\x33', b'B'*32, b'\x03\xe8'))
    ordinary = regular, b''
    alternative_mode = ('rue_source', True) if wanted == ('chialisp', True) else ('chialisp', True)
    alternate = evidence.curry(selected[alternative_mode], owner_key)
    records = []
    for index, (label, code) in enumerate(evidence.SCENARIOS):
        c, reveal, solution, signature = copy.deepcopy(selected_coin), owner, ordinary, 0x11
        if index == 0: signature = 0
        elif index == 1: signature = 0x22
        elif index == 2: solution = proper(proper(b'\x33', b'C'*32, b'\x03\xe8')), b''
        elif index == 3: c = coin(evidence.OTHER_PARENT, owner_hash)
        elif index == 4:
            solution = proper(proper(b'\x33', b'B'*32, b'\x03\xe9')), b''
            signature = 0x33
        elif index == 5: reveal, signature = attacker, 0x22
        elif index == 6: solution = regular, (b'\0', b'\x81')
        elif index == 9:
            c, reveal, solution, signature = copy.deepcopy(raw_coin), module, (attacker_key, (proper(proper(b'\x33', b'C'*32, b'\x03\xe8')), b'')), 0x44
        elif index == 10: c, reveal, solution, signature = copy.deepcopy(raw_coin), module, (owner_key, ordinary), 0x55
        elif index == 11: reveal = alternate
        row = dict(label=label, status='consensus_rejected' if code else 'accepted', transaction=tx(c, reveal, solution, signature))
        if code:
            row.update(code=code, full_simulator_equality=True)
        else:
            row.update(scope=evidence.ACCEPTED_SCOPE[label], spent_input_observed=True,
                       child=coin(c['id'], (b'C'*32 if index == 9 else b'B'*32).hex()))
        if index == 1:
            row.update(signature_source=tx(copy.deepcopy(selected_coin), attacker, ordinary, 0x22), signature_source_scope=evidence.SIGNER_SCOPE)
        if index == 11:
            row.update(alternate_build=dict(backend=alternative_mode[0], optimize_lir=alternative_mode[1],
                        module_tree_hash=sha_tree(selected[alternative_mode]).hex(), owner_curried_reveal_hex=serialize(alternate).hex(), owner_instance_hash=sha_tree(alternate).hex()),
                       signature_matches_selected_reveal=True, selected_reveal_signature_source=tx(copy.deepcopy(selected_coin), owner, ordinary, 0x11))
        records.append(row)
    return dict(backend=wanted[0], optimize_lir=wanted[1], status='validated', module_tree_hash=sha_tree(module).hex(),
                module_hex=serialize(module).hex(), owner_public_key=evidence.OWNER, attacker_public_key=evidence.ATTACKER,
                owner_curried_reveal_hex=serialize(owner).hex(), owner_instance_hash=owner_hash,
                transaction_attempts=12, accepted=4, typed_rejected_with_full_rollback=8,
                observations=records, scope=evidence.LEDGER_SCOPE), selected


def raw_record(spec):
    """Constructed typed metadata; no interpreter or native policy is run."""
    rejected = spec['expected']['status'] == 'rejected'
    runs = []
    for backend, optimize in vectors.MODES:
        execution = dict(outcome=copy.deepcopy(spec['expected']), cost=10, error=None)
        if rejected:
            execution.update(cost=None, error='constructed diagnostic', failure=dict(
                kind='program', code='path_into_atom' if backend=='chialisp' else 'raise'))
        runs.append(dict(backend=backend, optimize_lir=optimize, execution=execution))
    return dict(spec, canonical_failure_code='path_into_atom' if rejected else None,
                native=copy.deepcopy(spec['expected']), runs=runs)


def component_fixture():
    """Complete constructed component metadata, including fake signatures.

    This exercises reader composition; its artifact atoms and disassembly labels
    are deliberately not compiler outputs and it gives no execution credit.
    """
    manifest, specs, _ = vectors.source_inputs()
    result = {key: copy.deepcopy(manifest[key]) for key in
              ('slug', 'title', 'kind', 'environment', 'security_scope', 'provenance', 'reference_includes')}
    result.update(source=vectors.read(ROOT, vectors.DIRECTORY+'/puzzle.rs').decode(),
                  reference=vectors.read(ROOT, vectors.DIRECTORY+'/reference.clsp').decode(),
                  rue=dict(path=vectors.DIRECTORY+'/puzzle.rue', sha256=vectors.RUE_SHA,
                           source=vectors.read(ROOT, vectors.DIRECTORY+'/puzzle.rue').decode(), dependencies=[]),
                  source_dependencies=[dict(path=p, sha256=vectors.digest(vectors.read(ROOT,p)), source=vectors.read(ROOT,p).decode()) for p in vectors.DEPENDENCIES],
                  native_sources=[dict(path=p, sha256=vectors.digest(vectors.read(ROOT,p))) for p in vectors.NATIVE],
                  generated_cases=2239, simulator=[], simulator_comparisons=[], validated=True)
    selected = modules()
    result['artifacts'] = [dict(backend=b, optimize_lir=o, bytes=len(serialize(selected[(b,o)])),
        tree_hash=sha_tree(selected[(b,o)]).hex(), clvm_hex=serialize(selected[(b,o)]).hex(),
        disassembly='constructed reader control') for b,o in vectors.MODES]
    canonical = serialize(selected[('chialisp', True)]).hex()
    result['module_identity'] = dict(upstream_hex=canonical, upstream_tree_hash=vectors.CANONICAL_HASH,
        comparisons=[dict(backend=b,optimize_lir=o,serialized_bytes_equal=serialize(selected[(b,o)]).hex()==canonical,
                          tree_hash_equal=sha_tree(selected[(b,o)]).hex()==vectors.CANONICAL_HASH) for b,o in vectors.MODES])
    rows = [raw_record(spec) for spec in specs]
    by_label = {row['label']: row for row in rows}
    result['cases'] = [{k: copy.deepcopy(by_label[label][k]) for k in ('label','solution','native','runs')} for label in vectors.NAMED]
    result['protocol_validation'] = dict(schema_version=1, protocol=evidence.PROTOCOL, status='validated',
        simulator='chia-sdk-test 0.36.0', consensus='chia-consensus 0.36.1', vm='clvmr 0.16.2',network='TESTNET11',
        transaction_attempts=60, accepted=20, typed_rejected_with_full_rollback=40,
        builds=[fixture(wanted)[0] for wanted in vectors.MODES],
        raw_validation=dict(max_cost=1_000_000, execution_vm='clvmr 0.21.0 MEMPOOL_MODE', accepted=195, rejected=5,
            native_evaluations=200, VM_evaluations=1000, cases=rows, scope=evidence.RAW_SCOPE), scope=evidence.COMPONENT_SCOPE)
    return result


class IndependentSpecification(unittest.TestCase):
    def test_pinned_source_provenance_and_exact_disjoint_200_suite(self):
        manifest, rows, recipes = vectors.source_inputs()
        self.assertEqual(len(rows), 200)
        self.assertEqual(sum(row['expected']['status']=='accepted' for row in rows), 195)
        self.assertEqual(len(manifest['cases']), 20)
        self.assertEqual(sum(row['label'] not in vectors.NAMED for row in rows), 180)
        self.assertEqual(len(recipes['mutations']), 3)

    def test_nil_nonempty_zero_padding_and_raw_tails_remain_distinct(self):
        outputs = [vectors.expected(f'(() . ({value} . ()))')['output_hex'] for value in ['()', '0x00', '0x0000']]
        self.assertEqual(len(set(outputs)), 3)
        base = vectors.expected('((1 . 2) . ((3 . 4) . ()))')
        self.assertEqual(base, vectors.expected('((1 . 2) . ((3 . 4) . (8 . 9)))'))
        self.assertNotEqual(base, vectors.expected('((1 . 2) . ((3 . 4) . (8 . 9)))', strict_tail=True))

    def test_missing_projections_not_conditions_type_are_rejection(self):
        for value in ['()', '0x00', '(() . ())']:
            self.assertEqual(vectors.expected(value), {'status':'rejected'})
        for value in ['(() . (0xff . 0x0000))', '((1 . 2) . (() . 0x81))']:
            self.assertEqual(vectors.expected(value)['status'], 'accepted')

    def test_source_counterfactuals_have_nonvacuous_independent_witnesses(self):
        _, rows, recipes = vectors.source_inputs()
        by_name = {row['label']: row for row in rows}
        options = [{'atom_domain': b'\x02'}, {'wrong_rest': True}, {'strict_tail': True}]
        source = vectors.read(ROOT, vectors.DIRECTORY+'/puzzle.rs').decode()
        for recipe, option in zip(recipes['mutations'], options, strict=True):
            witness = by_name[recipe['witness']]
            self.assertEqual(source.count(recipe['before']), 1)
            self.assertNotEqual(witness['expected'], vectors.expected(witness['solution'], **option))
        # These are spec/recipe controls; no mutated Rust was compiled or run.

    def test_changed_pinned_bytes_and_licensing_metadata_fail(self):
        original = lambda path: vectors.read(ROOT, path)
        for target in [vectors.SOURCE, vectors.ARTIFACT, vectors.INCLUDE, vectors.BASE+'/LICENSE', vectors.DIRECTORY+'/independent-vectors.json']:
            with self.subTest(target=target), self.assertRaises(ValueError):
                vectors.source_inputs(reader=lambda path: original(path)+b'\n' if path==target else original(path))
        data = json.loads(original(vectors.DIRECTORY+'/puzzle.json'))
        data['provenance']['license']['spdx'] = 'constructed-other-license'
        changed = json.dumps(data).encode()
        with self.assertRaises(ValueError):
            vectors.source_inputs(reader=lambda path: changed if path==vectors.DIRECTORY+'/puzzle.json' else original(path))

    def test_independent_input_size_bound(self):
        with self.assertRaises(ValueError): vectors.expected('x'*(16*1024+1))


class RecordedLedgerConsistency(unittest.TestCase):
    def test_constructed_positive_for_each_build_is_only_reader_credit(self):
        for wanted in vectors.MODES:
            row, selected = fixture(wanted)
            evidence.check_ledger(row, wanted, selected)

    def test_nonnoop_identity_signature_receipt_and_rollback_corruptions(self):
        base, selected = fixture()
        evidence.check_ledger(base, ('chialisp', True), selected)
        mutations = [
            lambda d: d.__setitem__('module_tree_hash', '00'*32),
            lambda d: d.__setitem__('owner_instance_hash', '00'*32),
            lambda d: d.__setitem__('transaction_attempts', 11),
            lambda d: d.__setitem__('accepted', True),
            lambda d: d['observations'][0].__setitem__('full_simulator_equality', False),
            lambda d: d['observations'][0].__setitem__('before_debug', '{}'),
            lambda d: d['observations'][0].__setitem__('code', 'MissingKey'),
            lambda d: d['observations'][0]['transaction'].__setitem__('signature_hex', '11'*96),
            lambda d: d['observations'][1]['signature_source']['spends'][0].__setitem__('solution_hex', '80'),
            lambda d: d['observations'][1].__setitem__('signature_source_scope', 'admitted transaction'),
            lambda d: d['observations'][2]['transaction'].__setitem__('signature_hex', '22'*96),
            lambda d: d['observations'][3]['transaction']['spends'][0].__setitem__('coin', copy.deepcopy(base['observations'][0]['transaction']['spends'][0]['coin'])),
            lambda d: d['observations'][6].__setitem__('spent_input_observed', 1),
            lambda d: d['observations'][7]['child'].__setitem__('amount', 999),
            lambda d: d['observations'][10]['transaction']['spends'][0].__setitem__('coin', copy.deepcopy(base['observations'][0]['transaction']['spends'][0]['coin'])),
            lambda d: d['observations'][11]['alternate_build'].__setitem__('module_tree_hash', '00'*32),
            lambda d: d['observations'][11]['alternate_build'].__setitem__('owner_instance_hash', '00'*32),
            lambda d: d['observations'][11].__setitem__('signature_matches_selected_reveal', False),
            lambda d: d['observations'][11]['selected_reveal_signature_source'].__setitem__('signature_hex', '22'*96),
        ]
        for index, mutate in enumerate(mutations):
            changed = copy.deepcopy(base); mutate(changed)
            # JSON scalar types matter: Python's True == 1 must not turn a
            # Boolean-to-integer corruption into a supposed no-op.
            self.assertNotEqual(json.dumps(base, sort_keys=True), json.dumps(changed, sort_keys=True), f'mutation {index}')
            with self.subTest(index=index), self.assertRaises(ValueError):
                evidence.check_ledger(changed, ('chialisp', True), selected)

    def test_rebound_coin_id_does_not_hide_wrong_committed_coin(self):
        base, selected = fixture()
        evidence.check_ledger(base, ('chialisp', True), selected)
        changed = copy.deepcopy(base)
        wrong = changed['observations'][0]['transaction']['spends'][0]['coin']
        wrong.update(evidence.coin_recipe(evidence.PARENT, '00'*32))
        evidence.coin(wrong)  # Independently internally consistent but wrong fixture.
        with self.assertRaises(ValueError): evidence.check_ledger(changed, ('chialisp', True), selected)

    def test_sdk_default_signature_encoding_is_not_an_all_zero_byte_string(self):
        base, selected = fixture()
        evidence.check_ledger(base, ('chialisp', True), selected)
        self.assertEqual(base['observations'][0]['transaction']['signature_hex'], 'c0'+'00'*95)
        changed = copy.deepcopy(base)
        changed['observations'][0]['transaction']['signature_hex'] = '00'*96
        with self.assertRaises(ValueError): evidence.check_ledger(changed, ('chialisp', True), selected)
        changed = copy.deepcopy(base)
        changed['observations'][7]['transaction']['signature_hex'] = evidence.EMPTY_SIGNATURE.hex()
        with self.assertRaises(ValueError): evidence.check_ledger(changed, ('chialisp', True), selected)
        # Rebinding all mutually compared wrong-key signatures must still not
        # turn malformed all-zero bytes into an actual nonempty SDK fixture.
        changed = copy.deepcopy(base)
        for transaction in (changed['observations'][1]['transaction'],
                            changed['observations'][1]['signature_source'],
                            changed['observations'][5]['transaction']):
            transaction['signature_hex'] = '00'*96
        with self.assertRaises(ValueError): evidence.check_ledger(changed, ('chialisp', True), selected)


class TypedRawConsistency(unittest.TestCase):
    def spec_and_record(self, rejected=False):
        _, rows, _ = vectors.source_inputs()
        spec = next(row for row in rows if (row['expected']['status']=='rejected') == rejected)
        return spec, raw_record(spec)

    def test_constructed_raw_positive_and_fault_contrast(self):
        for rejected in [False, True]:
            spec, record = self.spec_and_record(rejected)
            evidence.check_raw_case(record, spec)

    def test_resource_exhaustion_never_becomes_policy_rejection(self):
        spec, record = self.spec_and_record(True)
        evidence.check_raw_case(record, spec)
        record['runs'][0]['execution'].update(outcome={'status':'inconclusive'}, failure={'kind':'resource','code':'cost_exceeded'})
        with self.assertRaises(ValueError): evidence.check_raw_case(record, spec)

    def test_canonical_fault_not_normalized_and_budget_not_fabricated(self):
        spec, record = self.spec_and_record(True)
        record['runs'][2]['execution']['failure']['code'] = 'raise'
        with self.assertRaises(ValueError): evidence.check_raw_case(record, spec)
        spec, record = self.spec_and_record(False)
        record['runs'][0]['execution']['cost'] = 1_000_001
        with self.assertRaises(ValueError): evidence.check_raw_case(record, spec)

    def test_current23_selection_fails_before_any_current_source_check_on_history(self):
        for slugs in [evidence.OLD_SLUGS, evidence.OLD_SLUGS+[vectors.SLUG, vectors.SLUG], [vectors.SLUG]+evidence.OLD_SLUGS]:
            with self.subTest(slugs=slugs), patch.object(evidence, 'snapshot', side_effect=RuntimeError('must not inspect history')), self.assertRaises(ValueError):
                evidence.validate_current({'schema_version':1,'puzzles':[{'slug':slug} for slug in slugs]})


class ComponentComposition(unittest.TestCase):
    def test_complete_constructed_component_positive_and_closed_metadata_controls(self):
        base = component_fixture()
        self.assertEqual(evidence.validate_component(base)['SDK_attempts'],60)
        mutations = [
            lambda d: d.__setitem__('validated',1),
            lambda d: d.__setitem__('generated_cases',2059),
            lambda d: d['native_sources'][0].__setitem__('sha256','00'*32),
            lambda d: d['artifacts'][0].__setitem__('optimize_lir',True),
            lambda d: d['protocol_validation'].__setitem__('schema_version',True),
            lambda d: d['protocol_validation'].__setitem__('transaction_attempts',59),
            lambda d: d['protocol_validation']['raw_validation'].__setitem__('VM_evaluations',999),
            lambda d: d['protocol_validation']['raw_validation']['cases'][0]['runs'][0]['execution']['outcome'].__setitem__('output_hex','80'),
            lambda d: d['cases'][0]['native'].__setitem__('output_hex','80'),
            lambda d: d['protocol_validation']['builds'][0]['observations'][0].__setitem__('after_debug','{}'),
        ]
        for index, mutate in enumerate(mutations):
            changed=copy.deepcopy(base); mutate(changed)
            self.assertNotEqual(json.dumps(base,sort_keys=True),json.dumps(changed,sort_keys=True))
            with self.subTest(index=index), self.assertRaises(ValueError): evidence.validate_component(changed)

    def test_current_component_reader_binds_own_snapshot_without_claiming_old22_validation(self):
        component=component_fixture()
        marker={'constructed':'snapshot'}
        import corpus_append
        trailing=[{'slug':p} for p in corpus_append.current_slugs()[len(evidence.OLD_SLUGS)+1:]]
        report=dict(schema_version=1,authoring=marker,puzzles=[{'slug':p} for p in evidence.OLD_SLUGS]+[component]+trailing)
        with patch.object(evidence,'snapshot',return_value=marker):
            self.assertEqual(evidence.validate_current(report)['component'],vectors.SLUG)
        with patch.object(evidence,'snapshot',return_value={'constructed':'different'}), self.assertRaises(ValueError):
            evidence.validate_current(report)


class BoundedReportTransport(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        spec = importlib.util.spec_from_file_location('p2dc_cli_test', ROOT/'scripts/check-p2-delegated-conditions.py')
        cls.cli = importlib.util.module_from_spec(spec); spec.loader.exec_module(cls.cli)

    def test_regular_positive_unknown_duplicate_and_oversize_json(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary)/'report.json'; path.write_text('{"a":1}')
            self.assertEqual(self.cli.read_report(path), {'a':1})
            path.write_text('{"a":1,"a":2}')
            with self.assertRaises(ValueError): self.cli.read_report(path)
            with path.open('wb') as file: file.truncate(16*1024*1024+1)
            with self.assertRaises(ValueError): self.cli.read_report(path)

    def test_nonregular_directory_and_fifo_do_not_block(self):
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaises(ValueError): self.cli.read_report(temporary)
            if hasattr(os, 'mkfifo'):
                path = Path(temporary)/'report.fifo'; os.mkfifo(path)
                with self.assertRaises(ValueError): self.cli.read_report(path)


if __name__ == '__main__':
    unittest.main()
