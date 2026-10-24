"""Pure counter-contract and report corruption controls; no VM/SDK execution."""
import sys
sys.dont_write_bytecode = True
import copy
import json
from pathlib import Path
import unittest
from unittest.mock import patch
import tempfile
import protocol_oracle as o




class Contract(unittest.TestCase):
    def test_real_profile_reader_rejects_finite_float_substitutions(self):
        report={'protocol':'chialisp-oxide-action14-prototype/v2','artifact_packet_sha256':'00'*32,
            'primary_vm':{'version':'clvmr 0.21.0','flags':535,'max_cost':250_000_000},
            'sdk':{'simulator':'chia-sdk-test 0.36.0','consensus':'chia-consensus 0.36.1','vm':'clvmr 0.16.2','network':'TESTNET11','max_transaction_cost':5_500_000_000},'runs':[]}
        with patch.object(o,'admit_packet',return_value=('00'*32,{str(i):None for i in range(18)})):
            for group,key in (('primary_vm','flags'),('primary_vm','max_cost'),('sdk','max_transaction_cost')):
                changed=copy.deepcopy(report);changed[group][key]=float(changed[group][key])
                with self.subTest(key=key),self.assertRaisesRegex(ValueError,'primary protocol' if group=='primary_vm' else 'SDK protocol'):
                    o.check_minimal(changed,Path('/constructed/not-read'))

    def test_json_profiles_preserve_integer_float_and_bool_types(self):
        for value in (535.0,True):
            self.assertFalse(o.same_json({'flags':value},{'flags':535}))
        self.assertFalse(o.same_json({'max_cost':250_000_000.0},{'max_cost':250_000_000}))
        self.assertFalse(o.same_json({'max_transaction_cost':5_500_000_000.0},{'max_transaction_cost':5_500_000_000}))
        self.assertTrue(o.same_json({'flags':535},{'flags':535}))

    @classmethod
    def setUpClass(cls):
        cls.pinned = o.modules()
        cls.policy = o.Policy(b"\x01", b"\x01", bytes(48), bytes(32), cls.pinned)

    def test_integer_boundaries(self):
        for n, raw in [(0, b""), (127, b"\x7f"), (128, b"\x00\x80"), (255, b"\x00\xff"), (256, b"\x01\x00"), (o.MAX, b"\x00" + b"\xff"*8)]:
            self.assertEqual(o.uint(n), raw)
        for n in (-1, o.MAX+1, True, 1.0):
            with self.assertRaises(ValueError):
                o.uint(n)

    def test_bounded_transport_and_closed_controls(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "input"
            path.write_bytes(b"1234")
            self.assertEqual(o.bounded_read(path, 4), b"1234")
            with self.assertRaises(ValueError):
                o.bounded_read(path, 3)
            alias = Path(directory) / "alias"
            alias.symlink_to(path)
            with self.assertRaises(OSError):
                o.bounded_read(alias, 4)
        for actions, control in [([], "original"), ([0]*9, "original"), ([0], "typo"), ([True], "original")]:
            with self.assertRaises(ValueError):
                self.policy.transition(0, actions, control)

    def test_order_and_overflow(self):
        self.assertEqual(self.policy.transition(5, [0, 1])[0], 0)
        self.assertEqual(self.policy.transition(5, [1, 0])[0], 1)
        with self.assertRaises(ValueError):
            self.policy.transition(o.MAX, [0])
        self.assertEqual(self.policy.transition(o.MAX, [0], "increment_no_change"), (o.MAX, []))
        self.assertEqual(len(self.policy.transition(5, [1, 1])[1]), 2)
        self.assertEqual(self.policy.transition(5, [1], "reset_drop_signature"), (0, []))

    def test_merkle_domains_and_order(self):
        i, r = (o.treehash(p) for p in self.policy.actions)
        self.assertEqual(self.policy.root, o.sha(b"\x02" + o.sha(b"\x01"+i) + o.sha(b"\x01"+r)))
        self.assertNotEqual(self.policy.root, o.sha(b"\x02"+i+r))
        self.assertNotEqual(self.policy.root, o.sha(b"\x02"+o.sha(b"\x01"+r)+o.sha(b"\x01"+i)))

    def test_proof_scan_and_reuse(self):
        def as_list(tree):
            out=[]
            while type(tree) is tuple:
                value, tree=tree
                out.append(value)
            self.assertEqual(tree, b"")
            return out
        puzzles, entries, solutions = as_list(self.policy.solution([0, 1]))
        self.assertEqual(as_list(puzzles), list(self.policy.actions))
        self.assertEqual(as_list(entries), [(b"\x05", o.proper(b"\x01", self.policy.leaves[0])), (b"\x02", o.proper(b"", self.policy.leaves[1]))])
        _, entries, _ = as_list(self.policy.solution([0, 0]))
        self.assertEqual(as_list(entries), [(b"\x02", o.proper(b"", self.policy.leaves[1])), (b"\x02", b"")])

    def test_curry_exact_small_wire(self):
        # Hand specified standard curry: (2 (1 . 42) (4 (1 . 7) 1)).
        tree = o.curry(b"\x2a", b"\x07")
        self.assertEqual(o.codec.encode(tree).hex(), "ff02ffff012affff04ffff0107ff018080")
        self.assertEqual(o.treehash(tree).hex(), o.codec.identity(o.codec.encode(tree))["module_tree_hash"])



if __name__ == "__main__":
    unittest.main()
