"""Constructed corruption controls for the retained corpus source-control packet.

These edit copies of the recorded report in memory; they add no execution credit.
"""
import copy
import json
import unittest

import corpus_source_controls as controls


class RetainedPacketRejectsCorruption(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.packet = controls.ROOT / controls.PACKET
        cls.report = json.loads(controls.read(cls.packet / "report.json"))

    def rejects(self, mutate, message):
        report = copy.deepcopy(self.report)
        mutate(report)
        with self.assertRaisesRegex(ValueError, message):
            controls.validate(report, self.packet)

    def test_recorded_packet_validates(self):
        self.assertEqual(controls.validate(self.report, self.packet)["controls"], 4)

    def test_stale_source_window_rejects(self):
        self.rejects(lambda r: r["source_window"].update({controls.RECIPE: "00" * 32}), "stale recipe")

    def test_original_must_match_the_corpus_artifact(self):
        self.rejects(lambda r: r["modules"]["ordered_payout:on"].update(tree_hash="00" * 32), "exact corpus artifact")

    def test_retained_bytes_are_bound(self):
        self.rejects(lambda r: r["modules"]["raffle:off"].update(clvm_sha256="00" * 32), "retained program bytes")

    def test_original_must_match_the_oracle(self):
        def flip(r):
            r["controls"][0]["modes"]["on"]["original"] = {"status": "accepted", "output_hex": "80"}
        self.rejects(flip, "independent oracle")

    def test_mutant_must_diverge(self):
        def same(r):
            mode = r["controls"][2]["modes"]["off"]
            mode["mutant"] = copy.deepcopy(mode["original"])
        self.rejects(same, "must diverge")

    def test_inventory_and_order_are_closed(self):
        self.rejects(lambda r: r["modules"].pop("one_armed_bandit:off"), "complete module inventory")
        self.rejects(lambda r: r["controls"].reverse(), "complete ordered controls")


if __name__ == "__main__":
    unittest.main()
