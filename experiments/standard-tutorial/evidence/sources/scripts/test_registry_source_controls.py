"""Pure SDK record controls; retained positive fixtures require explicit CLI paths.

No test runs a policy, compiler, VM, SDK or signer. Fixture checks and constructed
corruptions establish recorded consistency only, never fresh execution credit.
"""
import argparse
import copy
import os
from pathlib import Path
import stat
import unittest
from unittest.mock import patch

import registry_source_controls as s


def metadata_inputs():
    """Incomplete execution data for predecode admission tests only."""
    capture = {'protocol': 'chialisp-oxide-registry-source-controls-capture/v1',
               'sdk': 'chia-sdk-test 0.36.0', 'consensus': 'chia-consensus 0.36.1',
               'vm': 'clvmr 0.16.2', 'network': 'TESTNET11',
               'rejection_protocol': 'chia-consensus-0.36.1/v1',
               'record': {'schema_version': 1, 'protocol': 'chialisp-oxide-registry-source-controls/v1',
                          'reference_artifacts': [], 'groups': [], 'counts': copy.deepcopy(s.COUNTS),
                          'scope': s.SCOPE}}
    runs = []
    for side, kind in s.CONTROLS:
        for optimize in (False, True):
            runs.append({'name': side + '-' + kind.replace('_', '-'), 'optimize_lir': optimize,
                         'entry': f'chialisp_oxide_registry_controls::{side}_{kind}::puzzle', 'module_hex': '80'})
    request = {'protocol': 'chialisp-oxide-registry-source-controls-request/v1', 'runs': runs}
    references = [{'slug': slug, 'source': '(mod () ())', 'module_hex': '80'} for slug in s.r.SLUGS]
    return capture, request, references, [{} for _ in range(6)]


class AdmissionTests(unittest.TestCase):
    def setUp(self):
        self.inputs = metadata_inputs()

    def rejects_before_decode(self, message='.'):
        with patch.object(s.codec, 'decode') as decode:
            with self.assertRaisesRegex(ValueError, message):
                s.validate_capture(*self.inputs)
            decode.assert_not_called()

    def test_last_candidate_bound_precedes_every_decode(self):
        self.inputs[1]['runs'][-1]['module_hex'] = '00' * (s.LIMIT + 1)
        self.rejects_before_decode()

    def test_last_reference_bound_precedes_every_decode(self):
        self.inputs[2][-1]['module_hex'] = '00' * (s.LIMIT + 1)
        self.rejects_before_decode()

    def test_eleven_module_aggregate_precedes_every_decode(self):
        text = '00' * (900 * 1024)
        for row in [*self.inputs[1]['runs'], *self.inputs[2]]:
            row['module_hex'] = text
        self.rejects_before_decode('reference module lengths precharged')

    def test_missing_and_reordered_last_artifacts_fail_before_decode(self):
        self.inputs[1]['runs'].pop()
        self.rejects_before_decode('complete control artifact inventory')
        self.inputs = metadata_inputs()
        runs = self.inputs[1]['runs']
        runs[-1], runs[-2] = runs[-2], runs[-1]
        self.rejects_before_decode('ordered source control identity/profile')

    def test_numeric_bools_and_unknown_fields_fail_before_decode(self):
        for mutate in (
            lambda x: x[0]['record']['counts'].update(groups=True),
            lambda x: x[0]['record'].update(schema_version=True),
            lambda x: x[1]['runs'][-1].update(optimize_lir=1),
            lambda x: x[1]['runs'][-1].update(unreviewed=True),
        ):
            self.inputs = metadata_inputs()
            mutate(self.inputs)
            self.rejects_before_decode()

    def test_strict_json_and_direct_value_bounds(self):
        for raw in (b'{"x":1,"x":2}', b'{"x":1.0}', b'{"x":NaN}', b'[' * 129 + b']' * 129):
            with self.assertRaises(ValueError):
                s.load_bytes(raw)
        for value in ({'x': 1.0}, {'x': -1}, {'x': 2 ** 64}, {'x': 'x' * (2 * s.LIMIT + 1)}):
            with self.assertRaises(ValueError):
                s.bounded_value(value, s.TOTAL)
        with self.assertRaises(ValueError):
            s.bounded_value({'bounded': True}, 1)

    def test_incomplete_metadata_is_not_positive_execution_evidence(self):
        with self.assertRaises(ValueError):
            s.validate_capture(*self.inputs)


def bounded_load(path):
    path = Path(path)
    s.require(not path.is_symlink(), 'fixture cannot be a symlink')
    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
    with os.fdopen(fd, 'rb') as stream:
        info = os.fstat(stream.fileno())
        s.require(stat.S_ISREG(info.st_mode) and 0 < info.st_size <= s.TOTAL, 'bounded regular fixture')
        raw = stream.read(s.TOTAL + 1)
    s.require(len(raw) <= s.TOTAL, 'fixture grew beyond bound')
    return s.load_bytes(raw)


def fixture_inputs(capture_path, request_path, base_path):
    """Explicit retained admission; the complete control packet gate is separate."""
    capture, request, base = [bounded_load(p) for p in (capture_path, request_path, base_path)]
    s.r.validate_report(base, sidecar_root=base_path.parent)
    s.require([row['slug'] for row in base['puzzles']] == list(s.r.SLUGS), 'exact registry triplet')
    descriptor = base['puzzles'][0]['protocol_validation']['index']
    index = s.r.json_value(s.r.read_member(base_path.parent / s.r.INDEX, descriptor))
    references = []
    for row in base['puzzles']:
        artifacts = [a for a in row['artifacts'] if a['backend'] == 'chialisp' and a['optimize_lir'] is True]
        s.require(len(artifacts) == 1, 'exact retained Chialisp reference')
        references.append({'slug': row['slug'], 'source': row['reference'], 'module_hex': artifacts[0]['clvm_hex']})
    return capture, request, references, index['header']['canonical_components']


def fixture_controls(capture_path, request_path, base_path):
    inputs = fixture_inputs(capture_path, request_path, base_path)
    result = s.validate_capture(*inputs)
    s.require(result['status'] == 'recorded_consistency_checked', 'retained positive consistency')
    controls = s.corruption_controls(*inputs)
    additional = [
        ('unselected_last_group', lambda x: x[0]['record']['groups'][-1]['postconditions'].update(replacement_child_count=1)),
        ('last_reference_source', lambda x: x[2][-1].update(source='(mod () ())')),
        ('last_reference_artifact', lambda x: x[2][-1].update(module_hex='80')),
        ('last_request_artifact', lambda x: x[1]['runs'][-1].update(module_hex='80')),
        ('old_slot_creator', lambda x: x[0]['record']['groups'][0]['policy']['old_slot'].update(creator_state={'initialized': True, 'alpha_hash': '00' * 32, 'beta_hash': '00' * 32})),
        ('controller_child_parent', lambda x: x[0]['record']['groups'][0]['postconditions']['controller_children'][0].update(parent_coin_id='00' * 32)),
        ('slot_spend_missing', lambda x: x[0]['record']['groups'][2]['mutation']['transaction']['spends'].pop()),
        ('mode_reorder', lambda x: x[0]['record']['groups'].reverse()),
        ('bool_spent_state', lambda x: x[0]['record']['groups'][0]['postconditions'].update(old_slot_spent=0)),
        ('unknown_nested_spend_field', lambda x: x[0]['record']['groups'][0]['mutation']['transaction']['spends'][0].update(unknown=True)),
    ]
    for name, mutate in additional:
        changed = copy.deepcopy(inputs)
        mutate(changed)
        try:
            s.validate_capture(*changed)
        except ValueError:
            continue
        raise ValueError('accepted constructed SDK record corruption: ' + name)
    print(f"PASS retained SDK record: positive plus {controls['constructed_corruption_controls'] + len(additional)} constructed corruptions; no fresh execution credit")


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--capture', type=Path)
    parser.add_argument('--request', type=Path)
    parser.add_argument('--base-report', type=Path)
    args, rest = parser.parse_known_args()
    if any((args.capture, args.request, args.base_report)):
        s.require(all((args.capture, args.request, args.base_report)) and not rest, 'all three explicit fixture paths required')
        fixture_controls(args.capture, args.request, args.base_report)
    else:
        unittest.main(argv=['test_registry_source_controls.py', *rest])
