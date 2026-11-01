"""Explicit recovery controls with injected providers; no real HTTP or pushes."""
import copy
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import live_workflow as live
import recovery_step as recovery
import test_live_workflow as fixtures
import workflow as w


class Provider(fixtures.Provider):
    def __init__(self, prepared):
        super().__init__(prepared)
        self.mempool_present = False
        self.effects = set()

    def _call(self, endpoint, body):
        if endpoint == 'get_mempool_item_by_tx_id':
            self.calls.append((endpoint, copy.deepcopy(body)))
            bundle_id = body['tx_id']
            if self.mempool_present:
                return dict(success=True, mempool_item=dict(spend_bundle_name=bundle_id))
            return dict(success=False, error='Tx '+bundle_id+' not in mempool',
                        structuredError=dict(code='TX_NOT_IN_MEMPOOL', message='Tx not in mempool',
                                             data=dict(tx_id=bundle_id)))
        result = super()._call(endpoint, body)
        if endpoint == 'push_tx':
            # Same-input transactions cannot execute twice. This is an injected
            # provider control, not additional SDK or consensus evidence.
            self.effects.add(self.prepared['spend_bundle_id'])
        return result


@unittest.skipUnless(os.environ.get('CHIALISP_CONTINUATION_PREPARE'), 'explicit locally built prepare-step executable required')
class RecoveryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name)/'journal.json'
        self.exe = Path(os.environ['CHIALISP_CONTINUATION_PREPARE']).resolve(strict=True)
        state = w.initialize(self.path, 'on', ['11'*32, '22'*32, '33'*32])
        self.state = w.mutate(self.path, w.token(state), parent='aa'*32)
        self.intent = live.prepare(self.path, self.exe, 0, w.token(self.state))
        self.intent['status'] = 'ambiguous'
        live.write_intent(live.intent_path(self.path), self.intent)
        self.bundle_id = self.intent['prepared']['spend_bundle_id']
        self.provider = Provider(self.intent['prepared'])

    def retry(self, **kwargs):
        return recovery.retry(self.path, self.provider, self.bundle_id, self.exe,
                              explicitly_retry=True, **kwargs)

    def pushes(self):
        return [body for endpoint, body in self.provider.calls if endpoint == 'push_tx']

    def test_one_exact_retry_preserves_archive_and_cannot_add_effects_again(self):
        raw = live.intent_path(self.path).read_bytes()
        def before_push():
            self.assertEqual(recovery.archive_path(self.path, self.bundle_id).read_bytes(), raw)
            self.assertEqual(live.read_intent(self.path)['status'], 'push_started')
        self.provider.push_hook = before_push
        result = self.retry()
        self.assertEqual(result['action'], 'one_explicit_exact_retry')
        self.assertEqual(result['intent']['prepared']['spend_bundle_hex'], self.intent['prepared']['spend_bundle_hex'])
        self.assertEqual(len(self.pushes()), 1)
        self.assertEqual(self.provider.effects, {self.bundle_id})
        with self.assertRaisesRegex(ValueError, 'already reserved'):
            self.retry()
        self.assertEqual(len(self.pushes()), 1)
        self.assertEqual(self.provider.effects, {self.bundle_id})

    def test_confirmed_and_present_mempool_do_not_push_or_archive(self):
        self.provider.spent = True
        self.assertEqual(self.retry()['action'], 'confirmed_without_push')
        self.assertEqual(self.pushes(), [])
        self.assertFalse(recovery.archive_path(self.path, self.bundle_id).exists())

    def test_present_mempool_monitors_without_assuming_confirmation(self):
        self.provider.mempool_present = True
        before = self.path.read_bytes()
        self.assertEqual(self.retry()['action'], 'monitor_existing_mempool_bundle')
        self.assertEqual(self.pushes(), [])
        self.assertEqual(self.path.read_bytes(), before)
        self.assertFalse(recovery.archive_path(self.path, self.bundle_id).exists())

    def test_explicit_flag_and_reviewed_identity_required(self):
        for expected, flag in ((self.bundle_id, False), ('00'*32, True)):
            with self.assertRaises(ValueError):
                recovery.retry(self.path, self.provider, expected, self.exe, explicitly_retry=flag)
        self.assertEqual(self.provider.calls, [])

    def test_stale_journal_and_corrupt_wire_halt_without_push(self):
        original = live.intent_path(self.path).read_bytes()
        altered = copy.deepcopy(self.intent)
        altered['prepared']['spend_bundle_hex'] += '00'
        live.write_intent(live.intent_path(self.path), altered)
        with self.assertRaises(ValueError): self.retry()
        live.intent_path(self.path).write_bytes(original)
        advanced = copy.deepcopy(self.state)
        advanced['completed'] += 1
        advanced['revision'] += 1
        w.write(self.path, advanced)
        with self.assertRaisesRegex(ValueError, 'journal changed'): self.retry()
        self.assertEqual(self.pushes(), [])

    def test_missing_coin_and_changing_tip_halt_before_retry_reservation(self):
        self.provider.missing = True
        with self.assertRaises(live.Inconclusive): self.retry()
        self.provider.missing = False
        self.provider.change_tip = True
        self.provider.tips = 0
        with self.assertRaises(live.Inconclusive): self.retry()
        self.assertEqual(self.pushes(), [])
        self.assertFalse(recovery.archive_path(self.path, self.bundle_id).exists())

    def test_crash_after_archive_consumes_retry_without_another_push(self):
        with patch.object(live, 'submit', side_effect=RuntimeError('injected process boundary failure')):
            with self.assertRaises(RuntimeError): self.retry()
        self.assertTrue(recovery.archive_path(self.path, self.bundle_id).exists())
        with self.assertRaisesRegex(ValueError, 'already reserved'): self.retry()
        self.assertEqual(self.pushes(), [])

    def test_mempool_identity_mismatch_is_not_absence(self):
        class Wrong:
            def call(self, endpoint, body):
                return dict(success=False, structuredError=dict(code='TX_NOT_IN_MEMPOOL', data=dict(tx_id='0x'+'00'*32)))
        with self.assertRaises(ValueError): recovery.mempool(Wrong(), self.bundle_id)


if __name__ == '__main__':
    unittest.main()
