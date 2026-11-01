"""Actual isolated SDK cost preparation tests, no live or mined-chain credit."""
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import plan_costs as planner


@unittest.skipUnless(os.environ.get('CHIALISP_CONTINUATION_PREPARE'), 'explicit locally built prepare-step executable required')
class CostPlannerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.exe = Path(os.environ['CHIALISP_CONTINUATION_PREPARE']).resolve(strict=True)
        cls.recipients = ['11'*32, '22'*32]
        cls.report = planner.plan_costs(cls.recipients, 'on', cls.exe, 0)

    def test_both_modes_measured_queue_sums_and_maxima(self):
        for mode in ('on', 'off'):
            report = self.report if mode == 'on' else planner.plan_costs(self.recipients, mode, self.exe, 0)
            self.assertEqual(report['artifact']['mode'], mode)
            self.assertEqual(report['isolated_sdk_preflights'], 2)
            self.assertEqual(report['ledger_chain_runs'], 0)
            self.assertEqual(report['live_network_requests'], 0)
            self.assertEqual(report['policy_spend_fees_mojos'], 0)
            self.assertEqual([row['prepared']['children'][0]['puzzle_hash'] for row in report['steps']], self.recipients)
            self.assertEqual(len(report['steps'][-1]['prepared']['children']), 1)
            for encoding in ('uncompressed', 'backrefs'):
                for metric in planner.METRICS:
                    self.assertEqual(report['totals'][encoding][metric], sum(row['metrics'][encoding][metric] for row in report['steps']))
                self.assertEqual(report['totals'][encoding]['maximum_step_total_cost'], max(row['metrics'][encoding]['total_cost'] for row in report['steps']))
                self.assertTrue(all(row['prepared']['costs']['generators'][0]['metrics']['signature_verified'] for row in report['steps']))
            self.assertLessEqual(len(planner.w.encode(report)), planner.MAX_REPORT_BYTES)
            planner.validate(report)

    def test_maximum_queue_and_deterministic_small_plan(self):
        self.assertEqual(self.report, planner.plan_costs(self.recipients, 'on', self.exe, 0))
        recipients = ['11'*32]*8
        maximum = planner.plan_costs(recipients, 'on', self.exe, 0)
        self.assertEqual(len(maximum['steps']), 8)
        self.assertEqual(maximum['steps'][0]['prepared']['current_coin']['amount'], 8000)
        self.assertEqual(len(maximum['steps'][-1]['prepared']['children']), 1)
        self.assertEqual(sum(row['prepared']['children'][0]['amount'] for row in maximum['steps']), 8000)
        self.assertEqual(maximum['ledger_chain_runs'], 0)

    def test_synthetic_parent_is_explicitly_unobserved_and_supplied_parent_distinct(self):
        self.assertEqual(self.report['funding_context'], dict(kind='synthetic_fixture_not_observed', parent_id=planner.SYNTHETIC_PARENT))
        supplied = planner.plan_costs(['11'*32], 'on', self.exe, 9, 'aa'*32)
        self.assertEqual(supplied['funding_context'], dict(kind='caller_supplied_public_parent_not_observed', parent_id='aa'*32))
        self.assertEqual(supplied['steps'][0]['prepared']['current_coin']['parent_id'], 'aa'*32)
        self.assertEqual(supplied['steps'][0]['prepared']['costs']['previous_transaction_height'], 9)

    def test_invalid_inputs_rejected_before_subprocess_or_journal(self):
        invalid = [([], 'on', 0, None), (['11'*32]*9, 'on', 0, None),
                   (['AA'*32], 'on', 0, None), ([True], 'on', 0, None),
                   (self.recipients, 'canonical', 0, None), (self.recipients, 'on', True, None),
                   (self.recipients, 'on', -1, None), (self.recipients, 'on', 0xffffffff, None),
                   (self.recipients, 'on', 0, 'a'*63)]
        with patch.object(planner.prepare_step, 'prepare', side_effect=AssertionError('must not execute')):
            for recipients, mode, height, parent in invalid:
                with self.subTest(recipients=recipients, mode=mode, height=height):
                    with self.assertRaises(ValueError): planner.plan_costs(recipients, mode, self.exe, height, parent)

    def test_report_corruptions_rejected_including_raw_generator_and_wire(self):
        for kind in ('sum', 'maximum', 'generator', 'wire', 'recipient', 'module', 'fee', 'scope', 'omit_step', 'reorder_steps', 'sdk_type', 'height_type', 'float_projection', 'float_total', 'float_maximum'):
            altered = copy.deepcopy(self.report)
            if kind == 'sum': altered['totals']['backrefs']['charged_bytes'] += 1
            if kind == 'maximum': altered['totals']['uncompressed']['maximum_step_total_cost'] += 1
            if kind == 'generator': altered['steps'][0]['prepared']['costs']['generators'][0]['generator_hex'] += '80'
            if kind == 'wire': altered['steps'][0]['prepared']['spend_bundle_hex'] += '00'
            if kind == 'recipient': altered['steps'][0]['prepared']['children'][0]['puzzle_hash'] = '99'*32
            if kind == 'module': altered['artifact']['module_hash'] = '99'*32
            if kind == 'fee': altered['policy_spend_fees_mojos'] = 1
            if kind == 'scope': altered['ledger_chain_runs'] = 1
            if kind == 'omit_step': altered['steps'].pop()
            if kind == 'reorder_steps': altered['steps'].reverse()
            if kind == 'sdk_type': altered['steps'][0]['prepared']['sdk_preflight']['accepted'] = 1
            if kind == 'height_type': altered['steps'][0]['prepared']['costs']['previous_transaction_height'] = False
            if kind == 'float_projection': altered['steps'][0]['metrics']['uncompressed']['charged_bytes'] = float(altered['steps'][0]['metrics']['uncompressed']['charged_bytes'])
            if kind == 'float_total': altered['totals']['backrefs']['total_cost'] = float(altered['totals']['backrefs']['total_cost'])
            if kind == 'float_maximum': altered['totals']['uncompressed']['maximum_step_total_cost'] = float(altered['totals']['uncompressed']['maximum_step_total_cost'])
            with self.subTest(kind=kind):
                with self.assertRaises(ValueError): planner.validate(altered)

    def test_capacity_is_charged_before_preparation(self):
        with patch.object(planner, 'MAX_REPORT_BYTES', planner.MAX_STEP_BYTES-1), \
             patch.object(planner.prepare_step, 'prepare', side_effect=AssertionError('must not execute')):
            with self.assertRaisesRegex(ValueError, 'capacity'): planner.plan_costs(self.recipients, 'on', self.exe, 0)

    def test_private_workflow_does_not_change_existing_journal_or_artifacts(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'existing-live-journal.json'
            state = planner.w.initialize(path, 'on', self.recipients)
            original = path.read_bytes()
            artifact_before = copy.deepcopy(state['plan']['artifact'])
            planner.plan_costs(['11'*32], 'on', self.exe, 0)
            self.assertEqual(path.read_bytes(), original)
            self.assertEqual(planner.w.selected('on'), artifact_before)
            self.assertEqual(sorted(p.name for p in path.parent.iterdir()), ['existing-live-journal.json', 'existing-live-journal.json.lock'])

    def test_actual_cli_records_full_generator_trace_without_network(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'recipients.json'
            path.write_text(json.dumps(['11'*32]))
            result = subprocess.run([sys.executable, '-B', str(Path(planner.__file__)),
                '--recipients', str(path), '--mode', 'on', '--executable', str(self.exe),
                '--previous-transaction-height', '0'], capture_output=True, check=True, timeout=60)
            report = json.loads(result.stdout)
            self.assertEqual(report['ledger_chain_runs'], 0)
            self.assertEqual(report['funding_context']['kind'], 'synthetic_fixture_not_observed')
            self.assertEqual(len(report['steps'][0]['prepared']['costs']['generators']), 2)
            self.assertEqual(list(Path(directory).iterdir()), [path])
            planner.validate(report)


if __name__ == '__main__':
    unittest.main()
