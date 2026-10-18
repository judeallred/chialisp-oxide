"""Offline tests of the closed comparison boundary; no compiler/VM calls."""
import copy
import json
import unittest
from unittest.mock import patch

import runtime_cost_migration as cost
from cost_controls import self_test


class CostMigrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.current = json.loads((cost.ROOT / "corpus/validation.json").read_bytes())
        cost.validate_current(cls.current)
        cls.report = cost.project_append(cls.current)

    def test_current_profile_restores_the_signed21_report_exactly(self):
        previous = cost.preserve(self.current)
        self.assertEqual(cost.sha(cost.canonical(previous)), cost.load()["baseline"]["canonical_report_sha256"])
        self.assertEqual(len(previous["puzzles"]), 21)

    def test_corruption_controls_are_nonvacuous(self):
        self.assertGreaterEqual(self_test(self.current), 35)

    def test_historical_pipeline_bytes_remain_the_old_normalization_target(self):
        self.assertEqual(cost.OLD_PIPELINE["rust_mir"]["optimized"], ["typed_ir_tree_guards", "runtime_frame_elision", "rue_lir"])
        self.assertEqual(cost.NEW_PIPELINE["rust_mir"]["optimized"], ["typed_ir_constants", "typed_ir_success_facts", "typed_ir_constants",
            "typed_ir_tree_guards", "runtime_frame_elision", "rue_lir", "static_pair_codegen"])
        unknown = copy.deepcopy(self.report)
        unknown["optimization_pipeline"]["rust_mir"]["optimized"].append("unreviewed")
        with self.assertRaisesRegex(ValueError, "Unknown optimization"):
            cost.is_new(unknown)

    def test_stale_current_native_sources_fail_before_projection(self):
        bad = copy.deepcopy(self.current)
        bad["puzzles"][-1]["native_sources"][0]["sha256"] = "0" * 64
        with patch.object(cost, "project_validated", side_effect=AssertionError("projection ran first")), self.assertRaises(ValueError):
            cost.preserve(bad)

    def test_starting_and_child_coin_ids_are_independently_derived(self):
        ledger = cost.rust_on(self.report["puzzles"][-1]["curried_instances"]["ledger"])
        expected = cost.coin_id(cost.PARENT, ledger["instance_tree_hash"])
        self.assertTrue(all(row["coin_id"] == expected for row in ledger["attempts"]))
        old = cost.load()["puzzles"][-1]["curried_instances"]["ledger"]
        for mutation in ("parent", "child"):
            bad = copy.deepcopy(ledger)
            if mutation == "parent":
                for row in bad["attempts"]:
                    row["coin_id"] = "11" * 32
                    for child in row["after"]["children"]:
                        child["parent_coin_id"] = row["coin_id"]
                        child["coin_id"] = cost.coin_id(row["coin_id"], child["puzzle_hash"])
            else:
                bad["attempts"][0]["after"]["children"][0]["coin_id"] = "22" * 32
            with self.subTest(mutation=mutation), self.assertRaisesRegex(ValueError, "independently derived"):
                cost.check_ledger_transition(bad, old)

    def test_snapshot_changes_cannot_authorize_extra_slots_or_resources(self):
        value = cost.load()
        for field in ("puzzles", "raw_ir", "authoring", "reviewed_after", "simulator_parent_coin_info"):
            bad = copy.deepcopy(value)
            bad[field] = {} if field != "simulator_parent_coin_info" else "0" * 64
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "baseline/profile bytes"):
                cost.load(cost.canonical(bad))


if __name__ == "__main__":
    unittest.main()
