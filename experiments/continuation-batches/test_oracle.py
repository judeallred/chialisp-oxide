"""Host recipe tests only; native/VM/ledger execution are separate lanes."""
import copy
import json
from pathlib import Path
import unittest

import oracle as o


class BatchRecipeTests(unittest.TestCase):
    def setUp(self):
        self.module = bytes.fromhex('aa'*32)
        self.recipients = [bytes([value])*32 for value in (17,34,51,68)]

    def test_exact_fixtures(self):
        self.assertEqual(o.fixtures(), json.loads(Path(__file__).with_name('fixtures.json').read_text()))

    def test_complete_plan_preserves_order_amounts_and_terminal(self):
        rows = o.plan(self.module, self.module, self.recipients)
        self.assertEqual([row['remaining'] for row in rows], [4,2])
        self.assertEqual(rows[0]['conditions'][-1], [51,rows[1]['instance_hash'],2000])
        self.assertEqual([condition[1] for row in rows for condition in row['conditions'][1:3]], [recipient.hex() for recipient in self.recipients])
        self.assertEqual(len(rows[-1]['conditions']),3)
        for row in rows:
            self.assertEqual(row['conditions'][0][1], sum(condition[2] for condition in row['conditions'][1:]))

    def test_independent_mutated_openings_reject(self):
        row = o.fixtures()['steps'][0]
        original = [self.module, bytes.fromhex(row['head']), row['remaining'], bytes.fromhex(row['first']), bytes.fromhex(row['middle']), bytes.fromhex(row['second']), bytes.fromhex(row['next'])]
        for index in (1,3,4,5,6):
            altered = copy.copy(original); altered[index] = bytes([0x99])*32
            with self.assertRaises(ValueError): o.step(*altered)
        swapped = copy.copy(original);swapped[3],swapped[5] = swapped[5],swapped[3]
        with self.assertRaises(ValueError):o.step(*swapped)

    def test_count_and_fixed_field_boundaries(self):
        for count in (0,1,3,5,7,9,255,True):
            with self.assertRaises(ValueError):o.terms(self.module,b'\x11'*32,count)
        for value in (b'',b'x'*31,b'x'*33,'x'*32):
            with self.assertRaises(ValueError):o.item(value,o.terminal())
        for count in o.COUNTS:
            recipients = [bytes([index+1])*32 for index in range(count)]
            self.assertEqual(len(o.plan(self.module,self.module,recipients)),count//2)

    def test_pair_duplicates_reject_but_cross_batch_repetition_is_allowed(self):
        for recipients in ([self.recipients[0]]*2,[self.recipients[0],self.recipients[1],self.recipients[2],self.recipients[2]]):
            with self.assertRaises(ValueError):o.plan(self.module,self.module,recipients)
        repeated = self.recipients[:2]*2
        self.assertEqual(len(o.plan(self.module,self.module,repeated)),2)
        middle=o.item(self.recipients[0],o.terminal())
        head=o.item(self.recipients[0],middle)
        with self.assertRaises(ValueError):o.step(self.module,head,2,self.recipients[0],middle,self.recipients[0],o.terminal())

    def test_funding_queue_scope_and_module_binding(self):
        for count in (0,1,3,5,7,9):
            with self.assertRaises(ValueError):o.plan(self.module,self.module,self.recipients[:1]*count)
        with self.assertRaises(ValueError):o.plan(b'\xbb'*32,self.module,self.recipients)
        with self.assertRaises(ValueError):o.plan(self.module,self.module,tuple(self.recipients))

    def test_early_end_and_excess_continuation(self):
        for row,count in ((o.fixtures()['steps'][0],2),(o.fixtures()['steps'][1],4)):
            with self.assertRaises(ValueError):o.step(self.module,bytes.fromhex(row['head']),count,bytes.fromhex(row['first']),bytes.fromhex(row['middle']),bytes.fromhex(row['second']),bytes.fromhex(row['next']))


if __name__ == '__main__':unittest.main()
