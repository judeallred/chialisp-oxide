import unittest

import oracle as o


class QueueRecipeTests(unittest.TestCase):
    def setUp(self):
        self.fixture = o.fixtures()
        self.module = bytes.fromhex(self.fixture["fixture_module_hash"])
        self.steps = self.fixture["steps"]

    def test_fixed_fixtures(self):
        import json
        from pathlib import Path
        self.assertEqual(self.fixture, json.loads(Path(__file__).with_name("fixtures.json").read_text()))

    def test_three_steps_link_and_conserve(self):
        for i, s in enumerate(self.steps):
            out = s["conditions"]
            self.assertEqual(out[0][1], sum(c[2] for c in out[1:]))
            if i < 2:
                self.assertEqual(out[2][1], self.steps[i+1]["instance_hash"])
            else:
                self.assertEqual(len(out), 2)

    def test_skipped_reordered_injected_wrong_next(self):
        a, b, c = self.steps
        for recipient, next_digest in [(b["recipient"], b["next"]),
                                      (b["recipient"], a["next"]),
                                      ("44"*32, a["next"]),
                                      (a["recipient"], c["head"])]:
            with self.assertRaises(ValueError):
                o.step(self.module, bytes.fromhex(a["head"]), 3,
                       bytes.fromhex(recipient), bytes.fromhex(next_digest))

    def test_early_end_and_excess_continuation(self):
        for s, count in [(self.steps[0], 1), (self.steps[2], 2)]:
            with self.assertRaises(ValueError):
                o.step(self.module, bytes.fromhex(s["head"]), count,
                       bytes.fromhex(s["recipient"]), bytes.fromhex(s["next"]))

    def test_wrong_module_is_funding_preflight_not_template_rejection(self):
        s = self.steps[0]
        with self.assertRaises(ValueError):
            o.funding_hash(b"b"*32, self.module, bytes.fromhex(s["head"]), 3)
        changed = o.step(b"b"*32, bytes.fromhex(s["head"]), 3,
                         bytes.fromhex(s["recipient"]), bytes.fromhex(s["next"]))
        self.assertNotEqual(changed[2][1], s["conditions"][2][1])

    def test_bounds_fixed_fields_and_canonical_integer_recipe(self):
        for count in [0, 9, 255]:
            with self.assertRaises(ValueError):
                o.instance_hash(self.module, b"h"*32, count)
        for length in [0, 31, 33]:
            with self.assertRaises(ValueError):
                o.item(b"r"*length, b"h"*32)
        self.assertEqual([o.integer(n) for n in range(1, 9)], [bytes([n]) for n in range(1, 9)])


if __name__ == "__main__":
    unittest.main()
