"""Read-only evidence controls; optional --report selects an explicit fixture.

A constructed schema wrapper is a gate control, never new submission credit.
"""
import argparse
import copy
import json
from pathlib import Path
import shutil
import tempfile
import unittest

import p2_singleton_evidence as p2

REPORT = p2.ROOT / "corpus/validation.json"


class P2Evidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        report = json.loads(REPORT.read_bytes())
        selected = [p for p in report["puzzles"] if p["slug"] == "p2-singleton"]
        if len(selected) != 1:
            raise ValueError("Explicit current22 or constructed-schema gate fixture required; no silent skip")
        cls.puzzle = selected[0]
        cls.control_scope = report.get("control_scope", "actual recorded corpus report; no authentication claim")
        print("P2 evidence fixture scope:", cls.control_scope)
        cls.positive = p2.validate_component(cls.puzzle)

    def test_positive_five_groups_110_computed_submissions(self):
        self.assertEqual((self.positive["groups"],self.positive["actual_recorded_submissions"],self.positive["accepted"],self.positive["typed_rejected_with_recorded_full_rollback"]), (5,110,15,95))
        self.assertEqual(sum(self.positive["codes"].values()),95)

    def test_meaningful_typed_protocol_and_coverage_corruptions(self):
        def row(p):return p["protocol_validation"]["observations"][0]
        def negative(p):return row(p)["negatives"][0]
        def life(p):return row(p)["lifecycle"]
        def runtime_source(p):
            return next(r for r in p["source_dependencies"] if r["path"] == next(iter(p2.RUNTIME_INPUTS)))
        controls = {
            "kind":lambda p:p.__setitem__("kind","pure_function"),
            "generic_simulator_credit":lambda p:p.__setitem__("simulator",[{}]),
            "version_bool":lambda p:p["protocol_validation"].__setitem__("schema_version",True),
            "ledger_VM":lambda p:p["protocol_validation"].__setitem__("ledger_VM","clvmr 0.21.0"),
            "unknown_header":lambda p:p["protocol_validation"].__setitem__("unknown",True),
            "missing_group":lambda p:p["protocol_validation"]["observations"].pop(),
            "numeric_mode":lambda p:row(p).__setitem__("optimize_lir",0),
            "different_module":lambda p:row(p).__setitem__("module_tree_hash","0"*64),
            "missing_runtime_source":lambda p:p["source_dependencies"].pop(),
            "stale_runtime_source":lambda p:runtime_source(p).__setitem__("sha256","0"*64),
            "different_runtime_source":lambda p:runtime_source(p).__setitem__("source","changed"),
            "missing_negative":lambda p:row(p)["negatives"].pop(),
            "duplicate_negative":lambda p:row(p)["negatives"].append(copy.deepcopy(negative(p))),
            "wrong_label":lambda p:negative(p).__setitem__("name","some failure"),
            "wrong_expected_and_observed":lambda p:negative(p).update(expected="CostExceeded",observed="CostExceeded"),
            "signer_preflight":lambda p:negative(p).__setitem__("stage","signer_preflight"),
            "state_changed":lambda p:negative(p).__setitem__("full_simulator_equality",False),
            "numeric_state_flag":lambda p:negative(p).__setitem__("full_simulator_equality",1),
            "untyped_envelope":lambda p:negative(p).pop("consensus"),
            "wrong_protocol":lambda p:negative(p)["consensus"].__setitem__("protocol","chia-consensus-0.36.1/v1"),
            "resource_not_semantic":lambda p:negative(p)["consensus"].__setitem__("code","CostExceeded"),
            "numeric_credit":lambda p:negative(p)["consensus"].__setitem__("semantic_rejection_credit",1),
            "missing_receipt":lambda p:life(p)["receipts"].pop(),
            "synthetic_receipt":lambda p:life(p)["receipts"][0].__setitem__("stage","synthetic_counterpart"),
            "wrong_receipt_binding":lambda p:life(p)["receipts"][0].__setitem__("bundle_field","joint"),
            "changed_coin_amount":lambda p:life(p)["continuation"].__setitem__("amount",2),
            "wrong_parent":lambda p:life(p)["eve"].__setitem__("parent_id","0"*64),
            "wrong_signature_bytes":lambda p:row(p)["negatives"][13]["transaction"].__setitem__("signature_hex","c0"+"00"*95),
            "missing_actual_transaction":lambda p:negative(p).pop("transaction"),
            "malformed_reveal":lambda p:life(p)["genesis"]["spends"][0].__setitem__("reveal_hex","80"),
        }
        for name,mutate in controls.items():
            self.assertEqual(p2.validate_component(self.puzzle),self.positive)  # positive before every corruption
            altered=copy.deepcopy(self.puzzle);mutate(altered)
            with self.subTest(name=name),self.assertRaises((ValueError,KeyError,IndexError)):
                p2.validate_component(altered)

    def test_sdk_debug_fields_are_forbidden_by_the_closed_prepublication_schema(self):
        # Full equality is a strict in-memory Rust check. Debug text must never
        # become serialized state evidence, including a misleading matching pair.
        for container in ("negatives", "lifecycle_rejections"):
            for field in ("before_debug", "after_debug"):
                altered = copy.deepcopy(self.puzzle)
                group = altered["protocol_validation"]["observations"][0]
                negative = group["negatives"][0] if container == "negatives" else group["lifecycle"]["rejections"][0]
                negative[field] = "matching diagnostic text is not state equality"
                with self.subTest(container=container, field=field), self.assertRaises(ValueError):
                    p2.validate_component(altered)
        for field in ("genesis_before_debug", "genesis_after_debug", "joint_after_debug", "lineage_after_debug"):
            altered = copy.deepcopy(self.puzzle)
            altered["protocol_validation"]["observations"][0]["lifecycle"][field] = "private SDK Debug"
            with self.subTest(field=field), self.assertRaises(ValueError):
                p2.validate_component(altered)
        for value in (None, False, 1, "true"):
            altered = copy.deepcopy(self.puzzle)
            negative = altered["protocol_validation"]["observations"][0]["negatives"][0]
            if value is None:
                negative.pop("full_simulator_equality")
            else:
                negative["full_simulator_equality"] = value
            with self.subTest(state_equality=value), self.assertRaises(ValueError):
                p2.validate_component(altered)
        altered = copy.deepcopy(self.puzzle)
        negative = altered["protocol_validation"]["observations"][0]["negatives"][0]
        negative.pop("full_simulator_equality")
        negative.update(before_debug="identical", after_debug="identical")
        with self.assertRaises(ValueError):
            p2.validate_component(altered)

    def test_source_and_protocol_closure_corruptions(self):
        with tempfile.TemporaryDirectory(prefix="p2-evidence-") as directory:
            root=Path(directory)
            paths=[p2.TABLE_PATH]+list(p2.RUNTIME_INPUTS)
            for relative in paths:
                target=root/relative;target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(p2.ROOT/relative,target)
            self.assertEqual(p2.validate_component(self.puzzle,root=root),self.positive)
            for relative in paths:
                path=root/relative;before=path.read_bytes()
                path.write_bytes(before+b"\n")
                if relative==p2.TABLE_PATH:
                    table=json.loads(before);table["codes"].append("CostExceeded");path.write_text(json.dumps(table))
                try:
                    with self.subTest(path=relative),self.assertRaises(ValueError):
                        p2.validate_component(self.puzzle,root=root)
                finally:path.write_bytes(before)
                self.assertEqual(p2.validate_component(self.puzzle,root=root),self.positive)


if __name__=="__main__":
    parser=argparse.ArgumentParser();parser.add_argument("--report",type=Path,default=REPORT)
    arguments,remaining=parser.parse_known_args();REPORT=arguments.report.resolve()
    unittest.main(argv=[__file__,*remaining])
