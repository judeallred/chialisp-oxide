"""Pure-reader controls; a retained fixture is never new SDK execution.

Default: sibling evidence/ with an explicitly current recorded packet. A caller
may instead set STANDARD_TUTORIAL_SDK_FIXTURE and the required matching
STANDARD_TUTORIAL_SDK_FIXTURE_LABEL=historical-standard55 or current-recorded.
The historical label binds the unchanged55 report; it asserts no current source
freshness. Full packet/source admission belongs to the separate packet loader.
"""
import copy
import hashlib
import json
import os
from pathlib import Path
import unittest

import sdk_reader as s

HISTORICAL_SHA256 = 'f33e9d7cbd4fed60d455ec0d7fdadc597ed9f41e9c30c707229a21e8f484153a'


def _json(path):
    require = s.require
    require(path.is_file() and not path.is_symlink() and path.stat().st_size <= 16 * 1024 * 1024,
            'bounded retained test fixture file')
    raw = path.read_bytes()
    require(len(raw) <= 16 * 1024 * 1024, 'fixture grew during bounded read')
    return json.loads(raw), hashlib.sha256(raw).hexdigest()


class RecordedSdkReader(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        configured = os.environ.get('STANDARD_TUTORIAL_SDK_FIXTURE')
        if configured:
            label = os.environ.get('STANDARD_TUTORIAL_SDK_FIXTURE_LABEL')
            s.require(label in ('historical-standard55', 'current-recorded'), 'explicit external fixture historical/current label')
            base = Path(configured)
        else:
            label = 'current-recorded'
            base = Path(__file__).resolve().parent / 'evidence'
        report_path = 'capture/sdk-a01.json' if label == 'historical-standard55' else 'report.json'
        request_path = 'capture/sdk-request.json' if label == 'historical-standard55' else 'request.json'
        cls.report, digest = _json(base / report_path)
        cls.request, _ = _json(base / request_path)
        if label == 'historical-standard55':
            s.require(digest == HISTORICAL_SHA256, 'unchanged explicitly historical55 test report')
        cls.fixture_label = label
        cls.modules = {row['lane']: bytes.fromhex(row['module_hex']) for row in cls.request['runs']}

    def validate(self, request=None, report=None, modules=None):
        return s.validate(self.request if request is None else request,
                          self.report if report is None else report,
                          self.modules if modules is None else modules)

    def test_complete_recorded_fixture_has_separate_sdk_primary_and_execution_scope(self):
        result = self.validate()
        self.assertEqual(result['status'], 'recorded_sdk_consistency_checked')
        self.assertEqual(result['counts'], s.COUNTS)
        self.assertEqual(result['primary_observations'], {'total': 85, 'accepted': 65, 'typed_program_rejected': 20})
        self.assertIs(result['execution_credit'], False)
        self.assertIs(result['bls_verification'], False)
        self.assertEqual([row['lane'] for row in result['modules']], list(s.LANES))
        self.assertIn(self.fixture_label, ('historical-standard55', 'current-recorded'))

    def test_closed_schema_and_cross_record_corruption_controls(self):
        def entry(report, index=0):
            return report['builds'][0]['attempts'][index]

        def coherent_parent(report):
            for attempt in report['builds'][0]['attempts'][:12]:
                coin = attempt['transaction']['spends'][0]['coin']
                coin['parent_coin_id'] = '00' * 32
                coin['coin_id'] = s.coin_id(bytes(32), bytes.fromhex(coin['puzzle_hash']), coin['amount']).hex()
                if attempt['status'] == 'accepted':
                    child = attempt['child']
                    child['parent_coin_id'] = coin['coin_id']
                    child['coin_id'] = s.coin_id(bytes.fromhex(child['parent_coin_id']), bytes.fromhex(child['puzzle_hash']), child['amount']).hex()

        def coherent_child(report):
            child = entry(report, 10)['child']
            child['puzzle_hash'] = '44' * 32
            child['coin_id'] = s.coin_id(bytes.fromhex(child['parent_coin_id']), bytes.fromhex(child['puzzle_hash']), child['amount']).hex()

        mutations = {
            'unknown_report_field': lambda q, r, m: r.update(extra=True),
            'unknown_build_field': lambda q, r, m: r['builds'][0].update(extra=True),
            'unknown_attempt_field': lambda q, r, m: entry(r).update(extra=True),
            'unknown_transaction_field': lambda q, r, m: entry(r)['transaction'].update(extra=True),
            'unknown_execution_field': lambda q, r, m: entry(r)['primary_vm'].update(extra=True),
            'unknown_request_field': lambda q, r, m: q.update(extra=True),
            'request_wrong_lane': lambda q, r, m: q['runs'][0].update(lane='rue-on'),
            'request_wrong_module': lambda q, r, m: q['runs'][0].update(module_hex='01'),
            'reported_module_not_caller_selected': lambda q, r, m: r['builds'][0].update(module_hex='01'),
            'reported_module_hash_changed': lambda q, r, m: r['builds'][0].update(module_tree_hash='00' * 32),
            'reordered_builds': lambda q, r, m: r['builds'].reverse(),
            'duplicate_build': lambda q, r, m: r['builds'].__setitem__(1, copy.deepcopy(r['builds'][0])),
            'missing_attempt': lambda q, r, m: r['builds'][0]['attempts'].pop(),
            'duplicate_attempt': lambda q, r, m: r['builds'][0]['attempts'].__setitem__(1, copy.deepcopy(entry(r))),
            'float_total_count': lambda q, r, m: r['counts'].update(accepted=15.0),
            'boolean_build_count': lambda q, r, m: r['builds'][0]['counts'].update(accepted=True),
            'float_primary_cost': lambda q, r, m: entry(r)['primary_vm'].update(cost=1.0),
            'wrong_sdk_scope': lambda q, r, m: r.update(scope='production ready'),
            'wrong_signature_domain': lambda q, r, m: r['builds'][0].update(agg_sig_me_domain='00' * 32),
            'self_selected_synthetic_key': lambda q, r, m: r['builds'][0].update(committed_synthetic_public_key=s.ORIGINAL_PUBLIC_KEY.hex()),
            'wrong_hidden_hash': lambda q, r, m: r['builds'][0].update(hidden_program_tree_hash='00' * 32),
            'wrong_reveal': lambda q, r, m: r['builds'][0].update(reveal_hex='80'),
            'coherently_changed_input_parent': lambda q, r, m: coherent_parent(r),
            'coherently_changed_child': lambda q, r, m: coherent_child(r),
            'wrong_coin_amount': lambda q, r, m: entry(r)['transaction']['spends'][0]['coin'].update(amount=True),
            'empty_instead_of_nonempty_invalid_signature': lambda q, r, m: entry(r, 1)['transaction'].update(signature_hex=s.IDENTITY_SIGNATURE),
            'nonempty_instead_of_empty_signature': lambda q, r, m: entry(r)['transaction'].update(signature_hex='11' * 96),
            'invalid_signature_replaced_by_accepted_signature': lambda q, r, m: entry(r, 1)['transaction'].update(signature_hex=entry(r, 10)['transaction']['signature_hex']),
            'wrong_typed_consensus_code': lambda q, r, m: entry(r).update(code='MintingCoin'),
            'missing_full_state_equality': lambda q, r, m: entry(r).update(full_simulator_equality=False),
            'runtime_credited_as_semantic_rejection': lambda q, r, m: entry(r, 8).update(semantic_rejection_credit=True),
            'resource_as_primary_program_failure': lambda q, r, m: entry(r, 8)['primary_vm'].update(failure={'kind': 'resource', 'code': 'cost_exceeded'}),
            'wrong_primary_code_for_lane': lambda q, r, m: entry(r, 9)['primary_vm']['failure'].update(code='invalid_op_arg'),
            'wrong_primary_output': lambda q, r, m: entry(r)['primary_vm']['outcome'].update(output_hex='80'),
            'accepted_primary_error': lambda q, r, m: entry(r)['primary_vm'].update(error='unexpected error'),
            'accepted_input_not_consumed': lambda q, r, m: entry(r, 10).update(input_consumed=False),
            'contemplated_solution_replaced_with_actual': lambda q, r, m: entry(r, 15).update(contemplated_solution_hex=entry(r, 15)['transaction']['spends'][0]['solution_hex']),
            'contemplated_output_replaced_with_actual': lambda q, r, m: entry(r, 15).update(contemplated_primary_vm=copy.deepcopy(entry(r, 15)['primary_vm'])),
            'replay_signature_changed': lambda q, r, m: entry(r, 11)['transaction'].update(signature_hex='11' * 96),
        }
        for label, mutate in mutations.items():
            with self.subTest(label=label):
                q, r, m = copy.deepcopy(self.request), copy.deepcopy(self.report), dict(self.modules)
                mutate(q, r, m)
                with self.assertRaises(ValueError):
                    s.validate(q, r, m)

    def test_caller_admitted_module_inventory_is_closed_and_bounded(self):
        variants = [dict(self.modules), dict(self.modules), dict(self.modules), dict(self.modules)]
        variants[0].pop('rust-off')
        variants[1]['unrelated'] = b'\x01'
        variants[2]['rust-off'] = b'\x80' * (s.MAX_MODULE_BYTES + 1)
        variants[3]['rust-off'] = bytearray(self.modules['rust-off'])
        for modules in variants:
            with self.subTest(keys=list(modules)):
                with self.assertRaises(ValueError):
                    self.validate(modules=modules)


class IterativeWireBoundaries(unittest.TestCase):
    def test_canonical_atom_payloads_and_no_wire_alias_or_trailing_data(self):
        for raw in (b'\x80', b'\x00', b'\x01', b'\x82\x00\x01', b'\xff\x01\x80'):
            self.assertEqual(s.encode(s.decode(raw)), raw)
        for raw in (b'', b'\x81\x01', b'\xc0\x00', b'\x01\x80', b'\xff\x01', b'\xfe\x00'):
            with self.subTest(raw=raw.hex()), self.assertRaises(ValueError):
                s.decode(raw)
        self.assertEqual(s.tree_hash(b'').hex(), '4bf5122f344554c53bde2ebb8cd2b7e3d1600ad631c385a5d7cce23c7785459a')

    def test_deep_wire_and_expanded_shared_tree_stop_with_bounded_value_errors(self):
        deep = b'\xff' * (s.MAX_DEPTH + 1) + b'\x80' * (s.MAX_DEPTH + 2)
        with self.assertRaises(ValueError):
            s.decode(deep)
        shared = b''
        for _ in range(16):
            shared = shared, shared
        for operation in (s.encode, s.tree_hash):
            with self.subTest(operation=operation.__name__), self.assertRaises(ValueError):
                operation(shared)
        with self.assertRaises(ValueError):
            s.decode(b'\x80' * (s.MAX_WIRE_BYTES + 1))
        with self.assertRaises(ValueError):
            s.coin_id(bytes(32), bytes(32), True)


if __name__ == '__main__':
    unittest.main()
