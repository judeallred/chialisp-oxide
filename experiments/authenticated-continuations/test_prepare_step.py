"""Actual offline SDK prepare tests. No live network or negative consensus credit."""
import copy
import json
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import prepare_step as p
import workflow as w


@unittest.skipUnless(os.environ.get('CHIALISP_CONTINUATION_PREPARE'), 'explicit locally built prepare-step executable required')
class PrepareStepTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name)/'journal.json'
        self.exe = Path(os.environ['CHIALISP_CONTINUATION_PREPARE']).resolve(strict=True)
        self.recipients = ['11'*32, '22'*32, '33'*32]

    def funded(self, mode='on'):
        state = w.initialize(self.path, mode, self.recipients)
        return w.mutate(self.path, w.token(state), parent='aa'*32)

    def raw(self, value):
        path = self.path.with_name('request.json')
        path.write_bytes(w.encode(value))
        return subprocess.run([str(self.exe), str(path)], capture_output=True, timeout=60)

    def test_both_modes_actual_sdk_exact_spend_and_generators(self):
        for mode in ('on', 'off'):
            with self.subTest(mode=mode):
                if self.path.exists(): self.path.unlink()
                state = self.funded(mode)
                before = self.path.read_bytes()
                result = p.prepare(self.path, self.exe, 0)
                self.assertEqual(before, self.path.read_bytes())
                self.assertEqual(result['module_hash'], state['plan']['artifact']['module_hash'])
                self.assertTrue(result['sdk_preflight']['accepted'])
                self.assertEqual(len(result['children']), 2)
                self.assertEqual(result['children'][0]['amount'], 1000)
                self.assertEqual(result['children'][1]['amount'], 2000)
                self.assertEqual(result['fees'], 0)
                self.assertEqual(hashlib.sha256(bytes.fromhex(result['spend_bundle_hex'])).hexdigest(), result['spend_bundle_id'])
                self.assertEqual(result['spend_bundle_id'], result['spend_bundle_sha256'])
                self.assertEqual(len(result['spend_bundle']['coin_spends']), 1)
                signature = result['spend_bundle']['aggregated_signature']
                self.assertEqual(signature, 'c0'+'00'*95)
                for generator in result['costs']['generators']:
                    self.assertTrue(generator['metrics']['signature_verified'])
                    self.assertLessEqual(generator['metrics']['total_cost'], 5_500_000_000)

    def test_terminal_spend_and_process_recovery(self):
        state = self.funded()
        for _ in range(3):
            result = p.prepare(self.path, self.exe, 0)
            state = w.mutate(self.path, w.token(state), observation={
                'network':'testnet11', 'coin_id':result['current_coin']['coin_id'],
                'children':result['children']})
        self.assertEqual(len(result['children']), 1)
        with self.assertRaisesRegex(ValueError, 'incomplete'):
            p.prepare(self.path, self.exe, 0)

    def test_independent_rust_rejects_root_queue_coin_and_module_mutations(self):
        self.funded()
        _, original = p.request(self.path, 0)
        mutations = []
        for key, value in [('funding_parent','bb'*32), ('completed',1), ('network','mainnet'),
                           ('schema',2), ('mode','off'), ('module_hex','01'), ('recipients',['44'*32]*3)]:
            altered = copy.deepcopy(original)
            altered[key] = value
            mutations.append(altered)
        for key,value in [('amount',3001),('parent_id','bb'*32),('puzzle_hash','bb'*32),('coin_id','bb'*32)]:
            altered = copy.deepcopy(original)
            altered['current_coin'][key] = value
            mutations.append(altered)
        for altered in mutations:
            with self.subTest(altered=next(k for k in altered if altered[k] != original[k])):
                result = self.raw(altered)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, b'')

    def test_unfunded_and_invalid_height_rejected_before_child(self):
        w.initialize(self.path, 'on', self.recipients)
        for height in (0, -1, True, 2**32-1, 2**32):
            with self.assertRaises(ValueError):
                p.prepare(self.path, self.exe, height)

    def test_rust_bounds_and_duplicate_fields(self):
        self.funded()
        _, value = p.request(self.path, 0)
        path = self.path.with_name('request.json')
        for raw in (b' '*262145, b'{"schema":1,"schema":1}', b'{"unexpected":1}'):
            path.write_bytes(raw)
            result = subprocess.run([str(self.exe), str(path)], capture_output=True, timeout=60)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, b'')
        value['recipients'] = ['11'*32]*9
        self.assertNotEqual(self.raw(value).returncode, 0)


if __name__ == '__main__':
    unittest.main()
