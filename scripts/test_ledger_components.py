"""Recorded component dispatch regressions; no compiler, VM or SDK execution.

The corpus input must contain the actual two dedicated components. Generic
checks and component-only validators do not grant current aggregate admission
or promote any pending profile. Historical22 is checked as historical data.
"""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

import ledger_rejections as ledger
import p2_delegated_conditions_evidence as delegated
import p2_singleton_evidence as singleton

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('readable_component_regression', ROOT / 'scripts/check-readable-puzzles.py')
readable = importlib.util.module_from_spec(spec)
spec.loader.exec_module(readable)


class LedgerComponentDispatch(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.actual = json.loads((ROOT / 'corpus/validation.json').read_text())
        cls.historical = json.loads((ROOT / 'corpus/checkpoints/runtime-composition-validation.json').read_text())
        cls.by_slug = {p['slug']: p for p in cls.actual['puzzles']}
        if len(cls.by_slug) != len(cls.actual['puzzles']) or not {'p2-singleton', 'p2-delegated-conditions'} <= cls.by_slug.keys():
            raise ValueError('Actual corpus with both unique dedicated components required')

    def check(self, value):
        with patch.object(subprocess, 'run', side_effect=AssertionError('metadata checks cannot execute processes')), \
                patch.object(subprocess, 'Popen', side_effect=AssertionError('metadata checks cannot execute processes')):
            return readable.check_report(value)

    def test_actual23_calls_both_component_only_validators_once(self):
        # Real recorded/source component checks run; current/outer admission must
        # not recurse from the generic checker into itself.
        import runtime_cost_migration as cost
        import p2dc_append_migration as outer
        results = {}
        old_validator, new_validator = singleton.validate_component, delegated.validate_component
        def checked(name, validator):
            def validate(puzzle):
                results[name] = validator(puzzle)
                return results[name]
            return validate
        with patch.object(cost, 'validate_current', side_effect=AssertionError('outer current recursion')), \
                patch.object(outer, 'validate_current', side_effect=AssertionError('outer current recursion')), \
                patch.object(delegated, 'validate_current', side_effect=AssertionError('whole current from component dispatch')), \
                patch.object(singleton, 'validate_component', side_effect=checked('old', old_validator)) as old, \
                patch.object(delegated, 'validate_component', side_effect=checked('new', new_validator)) as new:
            counts = self.check(self.actual)
        # 40 retained generic scenario groups plus 8 for each precommit slug.
        self.assertEqual(counts, (len(self.actual['puzzles']), sum(len(p['cases']) for p in self.actual['puzzles']), 40 + 2 * 8))
        self.assertEqual(old.call_count, 1)
        self.assertEqual(new.call_count, 1)
        self.assertIs(old.call_args.args[0], self.by_slug['p2-singleton'])
        self.assertIs(new.call_args.args[0], self.by_slug['p2-delegated-conditions'])
        self.assertEqual([results['old'][key] for key in ('groups', 'actual_recorded_submissions', 'accepted', 'typed_rejected_with_recorded_full_rollback')], [5, 110, 15, 95])
        self.assertEqual([results['new'][key] for key in ('SDK_attempts', 'accepted', 'typed_full_Eq_rejected')], [60, 20, 40])

    def test_exact_historical22_still_checks_singleton_without_new_current_checks(self):
        with patch.object(delegated, 'validate_component', side_effect=AssertionError('new component on historical22')):
            self.assertEqual(self.check(self.historical), (22, 439, 40))
        ledger.check_ledger_report(self.historical, require_typed=False)

    def test_known_component_dispatch_is_independent_of_position(self):
        changed = copy.deepcopy(self.actual)
        changed['puzzles'][-2:] = reversed(changed['puzzles'][-2:])
        self.assertEqual(self.check(changed), self.check(self.actual))

    def test_unknown_and_duplicate_components_reject(self):
        for name in ('unknown', 'duplicate-singleton', 'duplicate-delegated'):
            changed = copy.deepcopy(self.actual)
            if name == 'unknown':
                next(p for p in changed['puzzles'] if p['slug'] == 'p2-delegated-conditions')['slug'] = 'unknown-component'
            else:
                slug = 'p2-singleton' if name == 'duplicate-singleton' else 'p2-delegated-conditions'
                changed['puzzles'].append(copy.deepcopy(self.by_slug[slug]))
            self.assertNotEqual(changed, self.actual)
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, 'unknown protocol component|duplicate protocol component'):
                self.check(changed)

    def test_known_slug_cannot_hide_as_a_pure_function(self):
        for slug in ('p2-singleton', 'p2-delegated-conditions'):
            changed = copy.deepcopy(self.actual)
            row = next(p for p in changed['puzzles'] if p['slug'] == slug)
            row['kind'] = 'pure_function'
            row.pop('protocol_validation')
            self.assertNotEqual(changed, self.actual)
            with self.subTest(slug=slug), self.assertRaisesRegex(ValueError, 'requires its protocol_component contract'):
                self.check(changed)

    def test_crossed_component_payload_and_consensus_corruptions_reject(self):
        for name in ('crossed', 'bad-rejection', 'false-rollback'):
            changed = copy.deepcopy(self.actual)
            row = next(p for p in changed['puzzles'] if p['slug'] == 'p2-delegated-conditions')
            if name == 'crossed':
                row['protocol_validation'] = copy.deepcopy(self.by_slug['p2-singleton']['protocol_validation'])
            else:
                negative = row['protocol_validation']['builds'][0]['observations'][0]
                if name == 'bad-rejection':
                    negative['code'] = 'DoubleSpend'
                else:
                    negative['full_simulator_equality'] = False
            self.assertNotEqual(changed, self.actual)
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.check(changed)

    def test_component_cannot_inflate_generic_scenario_groups(self):
        changed = copy.deepcopy(self.actual)
        row = next(p for p in changed['puzzles'] if p['slug'] == 'p2-delegated-conditions')
        row['simulator'] = copy.deepcopy(changed['puzzles'][0]['simulator'])
        self.assertNotEqual(changed, self.actual)
        with self.assertRaises(ValueError):
            self.check(changed)

    def test_ordinary_ledger_checks_and_protocol_field_boundary_still_reject(self):
        for name in ('missing-scenario', 'invented-protocol'):
            changed = copy.deepcopy(self.actual)
            if name == 'missing-scenario':
                changed['puzzles'][0]['simulator'].pop()
            else:
                changed['puzzles'][0]['protocol_validation'] = {}
            self.assertNotEqual(changed, self.actual)
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.check(changed)


if __name__ == '__main__':
    unittest.main()
