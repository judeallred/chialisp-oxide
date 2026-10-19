"""Historical22 projection controls after actualcurrent23 validation.

The native file controls construct an explicit22 authoring view; no new frontend,
policy, VM, simulator or historical source-execution credit is claimed.
"""
import copy
import json
import shutil
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch

import p2_append_migration as append
import runtime_cost_migration as cost
import runtime_inline_migration as inline
import runtime_refinement_migration as refinement
import p2dc_append_migration as outer


class AppendProjection(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        current = json.loads((append.ROOT / "corpus/validation.json").read_bytes())
        cls.profile = append.load()
        from test_p2_source_cost import constructed_authorization
        with constructed_authorization(current):
            cost.validate_current(current)
            cls.baseline = cost.project_append(current)
        assert cost.sha(cost.canonical(cls.baseline)) == append.REPORT_CANONICAL

    def candidate(self):
        profile = copy.deepcopy(self.profile)
        report = copy.deepcopy(self.baseline)
        after = profile["reviewed_after"]
        sources = report["authoring"]["sources"]
        for row in sources:
            if row["path"] in append.CHANGED:
                row["sha256"] = cost.sha(row["path"].encode())
        for path in profile["authoring"]["prototype_added_inputs"] + profile["authoring"]["reviewed_integration_added_inputs"]:
            sources.append({"path":path, "sha256":cost.sha(path.encode())})
        sources.sort(key=lambda r:r["path"])
        report["authoring"]["exports"][append.ADDED] = {
            "entry":"chialisp_oxide_puzzles::p2_singleton::puzzle", "source":"puzzles/p2-singleton/puzzle.rs"}
        for puzzle in report["puzzles"]:
            for row in puzzle.get("native_sources", []):
                if row["path"] in append.CHANGED[2:]:
                    row["sha256"] = cost.sha(row["path"].encode())
        # Explicit constructed gate control, never represented as executed P2.
        report["puzzles"].append({"slug":append.ADDED, "kind":"protocol_component", "constructed_gate_control":True,
            "protocol_validation":{"constructed_gate_control":True}, "cases":[{} for _ in range(50)], "generated_cases":2187})
        after.update(status="reviewed_final_current22", report_sha256="1"*64,
            authoring=copy.deepcopy(report["authoring"]),
            native_source_after={p["slug"]:copy.deepcopy(p["native_sources"]) for p in report["puzzles"] if p["slug"] in append.AFFECTED},
            added_puzzle_sha256=cost.sha(cost.canonical(report["puzzles"][-1])),
            raw_ir_added_sha256="2"*64,
            bundles={"on":{"manifest_sha256":"1"*64,"optimize_lir":True}, "off":{"manifest_sha256":"2"*64,"optimize_lir":False}},
            dedicated_evidence={"protocol_validation_sha256":cost.sha(cost.canonical(report["puzzles"][-1]["protocol_validation"])),
                "read_protocol":"chialisp-oxide-p2-singleton/v1", "independent_named_cases":50,
                "ordinary_generated_cases":2187,"scope":"Constructed projection control only; no execution credit"})
        after["canonical_report_sha256"] = cost.sha(cost.canonical(report))
        return report, profile

    def project(self, report, profile):
        with patch.object(append, "load", return_value=profile):
            return append.project_validated(report)

    def test_pending_profile_rejects_before_projection(self):
        report, profile = self.candidate()
        profile["reviewed_after"]["status"] = "pending_current22_integration_and_fresh_validation"
        with patch.object(append, "load", return_value=profile), self.assertRaisesRegex(ValueError, "pending actual current22"):
            append.project_validated(report)

    def test_positive_normalization_then_actual_all_historical_projections(self):
        report, profile = self.candidate()
        normalized = self.project(report, profile)
        self.assertEqual(normalized, self.baseline)
        self.assertEqual(cost.sha(cost.canonical(normalized)), append.REPORT_CANONICAL)
        # Existing real profiles and pure projections execute; no mocks here.
        c879 = refinement.project_validated(normalized)
        self.assertEqual(cost.sha(cost.canonical(c879)), refinement.REPORT_CANONICAL)
        signed405 = inline.project_validated(c879)
        self.assertEqual(cost.sha(cost.canonical(signed405)), inline.REPORT_CANONICAL)
        oldcost = cost.project_validated(signed405)
        self.assertEqual(cost.sha(cost.canonical(oldcost)), cost.load()["baseline"]["canonical_report_sha256"])

    def test_actual_declared_native_bytes_rechecked_before_historical_projection(self):
        from cargo_authoring import snapshot
        with tempfile.TemporaryDirectory(prefix="p2-append-native-") as directory:
            root = Path(directory)
            metadata = json.loads((append.ROOT / "puzzles/authoring.json").read_bytes())
            # This exercises the retained historical22 native loader with real
            # copied source bytes and constructed declarations, independently
            # of the active23 checker tested by test_p2_outer_integration.
            metadata["exports"].pop(outer.SLUG)
            metadata["inputs"] = [path for path in metadata["inputs"] if path not in outer.ADDED]
            metadata["exports"][append.ADDED] = {"entry":"chialisp_oxide_puzzles::p2_singleton::puzzle", "source":"puzzles/p2-singleton/puzzle.rs"}
            metadata["inputs"] = sorted(set(metadata["inputs"]) | set(self.profile["authoring"]["prototype_added_inputs"] + self.profile["authoring"]["reviewed_integration_added_inputs"]))
            for relative in metadata["inputs"]:
                target = root / relative
                target.parent.mkdir(parents=True,exist_ok=True)
                source = append.ROOT / relative
                if source.is_file(): shutil.copy2(source,target)
                else: target.write_text("constructed native-freshness gate control\n")
            for relative in append.CHANGED:
                if relative != "puzzles/authoring.json":
                    path = root/relative
                    path.write_bytes(path.read_bytes()+b"\n// constructed registration control\n")
            (root/"puzzles/authoring.json").write_text(json.dumps(metadata))
            report, profile = self.candidate()
            report["authoring"] = snapshot(root)
            inputs = {r["path"]:r["sha256"] for r in report["authoring"]["sources"]}
            for puzzle in report["puzzles"]:
                target = root/f"puzzles/{puzzle['slug']}/puzzle.json"
                target.parent.mkdir(parents=True,exist_ok=True)
                if puzzle["slug"] != append.ADDED:
                    shutil.copy2(append.ROOT/target.relative_to(root),target)
                    for row in puzzle.get("native_sources",[]):row["sha256"] = inputs[row["path"]]
                else:
                    paths = append.CHANGED[2:] + profile["authoring"]["reviewed_integration_added_inputs"]
                    target.write_text(json.dumps({"native_sources":paths}))
                    puzzle["native_sources"] = [{"path":path,"sha256":inputs[path]} for path in paths]
            after = profile["reviewed_after"]
            after["authoring"] = copy.deepcopy(report["authoring"])
            after["native_source_after"] = {p["slug"]:copy.deepcopy(p["native_sources"]) for p in report["puzzles"] if p["slug"] in append.AFFECTED}
            after["added_puzzle_sha256"] = cost.sha(cost.canonical(report["puzzles"][-1]))
            after["canonical_report_sha256"] = cost.sha(cost.canonical(report))
            with patch.object(append,"ROOT",root), patch.object(append,"load",return_value=profile):
                self.assertTrue(append.current_native_sources(report))
                # Actual file mutation, not only metadata. Build scripts matter.
                source = root/"crates/chialisp-oxide-corpus/build.rs"
                original = source.read_bytes();source.write_bytes(original+b"\n// changed\n")
                with self.assertRaisesRegex(ValueError,"stale|mismatch"):
                    append.current_native_sources(report)
                source.write_bytes(original)
                self.assertTrue(append.current_native_sources(report))
                bad = copy.deepcopy(report)
                bad["puzzles"][-1]["native_sources"].pop()
                profile["reviewed_after"]["canonical_report_sha256"] = cost.sha(cost.canonical(bad))
                with self.assertRaisesRegex(ValueError,"closure"):
                    append.current_native_sources(bad)

    def test_immutable_profile_bytes_and_predecessor(self):
        for key in ("scope", "baseline", "predecessor", "raw_ir_before", "authoring", "reviewed_after"):
            profile = copy.deepcopy(self.profile)
            profile[key] = "corrupt"
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "profile bytes"):
                append.load(cost.canonical(profile))

    def test_meaningful_retained_report_corruptions_after_new_digest_binding(self):
        mutations = {
            "old_source": lambda r:r["puzzles"][0].__setitem__("source", r["puzzles"][0]["source"]+"\n"),
            "old_artifact": lambda r:r["puzzles"][0]["artifacts"][0].__setitem__("clvm_hex", "80"),
            "old_cost": lambda r:r["puzzles"][0]["cases"][0]["runs"][0]["execution"].__setitem__("cost", 999999),
            "old_native": lambda r:r["puzzles"][0]["cases"][0].__setitem__("native", {"status":"rejected"}),
            "old_order": lambda r:r["puzzles"].__setitem__(slice(0,2), reversed(r["puzzles"][:2])),
            "extra_metadata": lambda r:r.__setitem__("unknown_append_allowance", True),
            "missing_new": lambda r:r["puzzles"].pop(),
            "duplicate_new": lambda r:r["puzzles"].append(copy.deepcopy(r["puzzles"][-1])),
            "changed_pipeline": lambda r:r["optimization_pipeline"].__setitem__("unknown", []),
        }
        for name, mutate in mutations.items():
            report, profile = self.candidate()
            self.assertEqual(self.project(report, profile), self.baseline)  # nonvacuous positive first
            mutate(report)
            profile["reviewed_after"]["canonical_report_sha256"] = cost.sha(cost.canonical(report))
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.project(report, profile)

    def test_authoring_native_scope_corruptions_survive_outer_digest_rebinding(self):
        for name in ("extra_existing", "missing_registration", "extra_input", "extra_export", "native_unknown", "native_missing"):
            report, profile = self.candidate()
            self.assertEqual(self.project(report, profile), self.baseline)
            after = profile["reviewed_after"]
            if name == "extra_existing":
                next(r for r in report["authoring"]["sources"] if r["path"] == "Cargo.toml")["sha256"] = "f"*64
            elif name == "missing_registration":
                old = next(r for r in self.baseline["authoring"]["sources"] if r["path"] == append.CHANGED[0])
                next(r for r in report["authoring"]["sources"] if r["path"] == append.CHANGED[0]).update(old)
            elif name == "extra_input":
                report["authoring"]["sources"].append({"path":"unknown.rs", "sha256":"f"*64})
                report["authoring"]["sources"].sort(key=lambda r:r["path"])
            elif name == "extra_export":
                report["authoring"]["exports"]["unknown"] = {"entry":"unknown", "source":"unknown.rs"}
            elif name == "native_unknown":
                next(p for p in report["puzzles"] if p["slug"] == "sha256tree")["native_sources"][0]["path"] = "unknown.rs"
            else:
                next(p for p in report["puzzles"] if p["slug"] == "sha256tree")["native_sources"].pop()
            after["authoring"] = copy.deepcopy(report["authoring"])
            after["native_source_after"] = {p["slug"]:copy.deepcopy(p["native_sources"]) for p in report["puzzles"] if p["slug"] in append.AFFECTED}
            after["canonical_report_sha256"] = cost.sha(cost.canonical(report))
            with self.subTest(name=name), self.assertRaises(ValueError):
                self.project(report, profile)


if __name__ == "__main__":
    unittest.main()
