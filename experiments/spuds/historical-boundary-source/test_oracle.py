"""Finite independent host observations, not signature or coin-age tests."""
import json
from pathlib import Path
import unittest
import oracle as o


class SpudsRecipeTests(unittest.TestCase):
    def setUp(self):
        self.module=b'\xaa'*32
        self.key,self.next_key=o.fixture_keys()
        self.payout,self.next_payout=b'\x11'*32,b'\x22'*32

    def call(self, round=1, take=True):
        return o.step(self.module,self.key,self.payout,round,take,self.next_key,self.next_payout)

    def test_exact_independent_fixtures(self):
        self.assertEqual(o.fixtures(),json.loads(Path(__file__).with_name('fixtures.json').read_text()))

    def test_every_finite_take_and_claim(self):
        for round in range(1,9):
            claim=self.call(round,False)
            self.assertEqual([row[0] for row in claim],[50,73,82,82,51,60])
            self.assertEqual(claim[0][1],self.key.hex())
            self.assertEqual(claim[4],[51,self.payout.hex(),round*1000])
            self.assertEqual(claim[3],[82,5])
            if round<8:
                take=self.call(round)
                successor=o.instance_hash(self.module,self.next_key,self.next_payout,round+1)
                self.assertEqual([row[0] for row in take],[50,73,82,86,51,60])
                self.assertEqual(take[4],[51,successor.hex(),(round+1)*1000])
                self.assertEqual(take[0][2],o.sha(o.TAKE_DOMAIN+successor).hex())
                self.assertEqual(take[5],[60,take[0][2]])
                self.assertNotEqual(take[5],claim[5])

    def test_round_bool_and_atom_width_boundaries(self):
        for round in (0,9,255,True,-1):
            with self.assertRaises(ValueError):self.call(round)
        for take in (0,1,'take',None):
            with self.assertRaises(ValueError):self.call(1,take)
        with self.assertRaises(ValueError):self.call(8)
        for field,value in ((0,b'a'*31),(1,b'a'*47),(2,b'a'*33),(5,b'a'*49),(6,b'a'*31)):
            args=[self.module,self.key,self.payout,1,True,self.next_key,self.next_payout]
            args[field]=value
            with self.assertRaises(ValueError):o.step(*args)

    def test_take_message_changes_with_every_successor_field(self):
        original=self.call()[0][2]
        variants=[(b'\xbb'*32,self.next_key,self.next_payout,2),
                  (self.module,self.key,self.next_payout,2),
                  (self.module,self.next_key,self.payout,2),
                  (self.module,self.next_key,self.next_payout,3)]
        for terms in variants:
            self.assertNotEqual(o.sha(o.TAKE_DOMAIN+o.instance_hash(*terms)).hex(),original)

    def test_top_up_binds_take_announcement_not_just_concurrent_spend(self):
        take=self.call()
        claim=self.call(1,False)
        old_coin=b'\x77'*32
        take_id=o.top_up_announcement_id(old_coin,bytes.fromhex(take[5][1]))
        self.assertNotEqual(take_id,o.top_up_announcement_id(old_coin,bytes.fromhex(claim[5][1])))
        self.assertNotEqual(take_id,o.top_up_announcement_id(b'\x88'*32,bytes.fromhex(take[5][1])))

    def test_claim_ignores_decoded_next_holder_fields(self):
        result=self.call(1,False)
        self.assertEqual(result,o.step(self.module,self.key,self.payout,1,False,self.key,self.payout))
        self.assertEqual(result[2],[82,1])
        self.assertEqual(result[5],[60,o.sha(o.CLAIM_DOMAIN).hex()])


if __name__=='__main__':unittest.main()
