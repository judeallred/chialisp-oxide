"""Offline controls of actual current22 records and constructed review bindings.

These tests never authorize/publish a profile or claim new compiler/VM/ledger
execution. Fresh-bundle controls construct manifests/IR metadata and execute the
real file/source/identity loader; the caller's expected identities are controls.
"""
import argparse
import copy
import json
from pathlib import Path
from types import SimpleNamespace
import runpy
import shutil
import tempfile
import unittest
from unittest.mock import patch

import p2_append_migration as append
import p2_cli
import runtime_cost_migration as cost
import runtime_inline_migration as inline
import runtime_refinement_migration as refinement


def bind_after(report, profile=None):
    profile = copy.deepcopy(append.load() if profile is None else profile)
    component = next(p for p in report["puzzles"] if p["slug"] == append.ADDED)
    profile["reviewed_after"].update(status="reviewed_final_current22",
        report_sha256=cost.sha(cost.canonical(report)), canonical_report_sha256=cost.sha(cost.canonical(report)),
        authoring=copy.deepcopy(report["authoring"]),
        native_source_after={p["slug"]:copy.deepcopy(p["native_sources"]) for p in report["puzzles"] if p["slug"] in append.AFFECTED},
        added_puzzle_sha256=cost.sha(cost.canonical(component)), raw_ir_added_sha256="3"*64,
        bundles={"on":{"manifest_sha256":"1"*64,"optimize_lir":True},"off":{"manifest_sha256":"2"*64,"optimize_lir":False}},
        dedicated_evidence={"protocol_validation_sha256":cost.sha(cost.canonical(component["protocol_validation"])),
            "read_protocol":"chialisp-oxide-p2-singleton/v1", "independent_named_cases":50,
            "ordinary_generated_cases":2187,"scope":"Constructed review binding; Rust state equality is checked in memory, not serialized; no new execution credit"})
    return profile


class OuterAppendTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.report = json.loads((cost.ROOT/"corpus/validation.json").read_bytes())
        if not append.is_append(cls.report):
            raise ValueError("Explicit actual current22 record fixture required; no silent skip")
        cls.profile = bind_after(cls.report)

    def test_actual_current_checks_never_project_inside_hash_source_or_protocol_gates(self):
        with patch.object(append,"load",return_value=self.profile), patch.object(append,"project_validated",side_effect=AssertionError("history before current checks")):
            cost.validate_current(self.report)
            self.assertTrue(cost.hash_diagnostic_profile(self.report))

    def test_current22_checks_then_exact21_and_older_boundaries(self):
        with patch.object(append,"load",return_value=self.profile):
            cost.validate_current(self.report)
            normalized=append.project_validated(self.report)
            self.assertEqual(cost.sha(cost.canonical(normalized)),append.REPORT_CANONICAL)
            self.assertEqual(cost.sha(cost.canonical(refinement.preserve(self.report))),refinement.REPORT_CANONICAL)
            self.assertEqual(cost.sha(cost.canonical(inline.preserve(self.report))),inline.REPORT_CANONICAL)
            self.assertEqual(cost.sha(cost.canonical(cost.preserve(self.report))),cost.load()["baseline"]["canonical_report_sha256"])

    def test_only_current22_reaches_source_validation(self):
        calls=[]
        original=append.project_validated
        def validate(value):
            self.assertIs(value,self.report);calls.append("current22")
        def project(value):
            self.assertIs(value,self.report);self.assertEqual(calls,["current22"]);calls.append("append-to21");return original(value)
        with patch.object(append,"load",return_value=self.profile),patch.object(cost,"validate_current",side_effect=validate),patch.object(append,"project_validated",side_effect=project):
            historical=cost.preserve(self.report)
        self.assertEqual(calls,["current22","append-to21"])
        self.assertEqual(cost.sha(cost.canonical(historical)),cost.load()["baseline"]["canonical_report_sha256"])

    def test_current_semantic_corruptions_survive_rebinding_the_constructed_outer_digest(self):
        def p2(report):return report["puzzles"][-1]
        def raw(report):return next(c for c in p2(report)["cases"] if c["native"]["status"]=="accepted")
        controls={
            "source":lambda r:p2(r).__setitem__("source",p2(r)["source"]+"\n"),
            "native_closure":lambda r:p2(r)["native_sources"][0].__setitem__("sha256","0"*64),
            "protocol_missing":lambda r:p2(r)["protocol_validation"]["observations"].pop(),
            "protocol_resource":lambda r:p2(r)["protocol_validation"]["observations"][0]["negatives"][0].__setitem__("observed","CostExceeded"),
            "protocol_not_submitted":lambda r:p2(r)["protocol_validation"]["observations"][0]["negatives"][0].__setitem__("stage","signer_preflight"),
            "protocol_state":lambda r:p2(r)["protocol_validation"]["observations"][0]["negatives"][0].__setitem__("full_simulator_equality",False),
            "protocol_receipt":lambda r:p2(r)["protocol_validation"]["observations"][0]["lifecycle"]["receipts"].pop(),
            "protocol_debug":lambda r:p2(r)["protocol_validation"]["observations"][0]["negatives"][0].__setitem__("before_debug","private SDK text"),
            "lifecycle_debug":lambda r:p2(r)["protocol_validation"]["observations"][0]["lifecycle"].__setitem__("joint_after_debug","private SDK text"),
            "generated_count":lambda r:p2(r).__setitem__("generated_cases",2186),
            "native_output":lambda r:raw(r).__setitem__("native",{"status":"rejected"}),
            "coherent_wrong_output":lambda r:[raw(r).__setitem__("native",{"status":"accepted","output_hex":"80"}),*[row["execution"].__setitem__("outcome",{"status":"accepted","output_hex":"80"}) for row in raw(r)["runs"]]],
            "non_boolean_mode":lambda r:p2(r)["artifacts"][0].__setitem__("optimize_lir",0),
            "generic_ledger_credit":lambda r:p2(r)["simulator"].append({"name":"invented","passed":True,"scope":"ledger","rejections":[]}),
        }
        for name,mutation in controls.items():
            with patch.object(append,"load",return_value=self.profile):cost.validate_current(self.report)
            bad=copy.deepcopy(self.report);mutation(bad);self.assertNotEqual(cost.canonical(bad),cost.canonical(self.report))
            profile=bind_after(bad,self.profile)
            with self.subTest(control=name),patch.object(append,"load",return_value=profile),patch.object(append,"project_validated",side_effect=AssertionError("projected corrupt current")),self.assertRaises((ValueError,KeyError,TypeError,IndexError)):
                cost.validate_current(bad)

    def test_pending_and_exact_report_bytes_fail_closed(self):
        pending=copy.deepcopy(self.profile);pending["reviewed_after"]["status"]="pending"
        with patch.object(append,"load",return_value=pending),self.assertRaisesRegex(ValueError,"pending"):
            cost.validate_current(self.report)
        with patch.object(append,"load",return_value=self.profile):
            payload=cost.canonical(self.report);cost.check_report_bytes(payload,self.report)
            with self.assertRaisesRegex(ValueError,"report bytes"):
                cost.check_report_bytes(payload+b"\n",self.report)

    def test_historical_measurements_exclude_p2_without_silent_zip_truncation(self):
        with patch.object(append,"load",return_value=self.profile):
            for script in ["check-cost-optimization.py","check-helper-inlining.py","check-runtime-refinement.py"]:
                checker=runpy.run_path(str(cost.ROOT/'scripts'/script));summary=checker["measurements"](self.report)
                self.assertEqual(len(summary["puzzles"]),21);self.assertEqual(summary["named_cases"],389)
                self.assertNotIn(append.ADDED,[p["slug"] for p in summary["puzzles"]]);self.assertIn("excluded",summary["scope"])

    def test_explicit_fresh_flags_are_paired_and_follow_current_checks(self):
        base={"fresh_on":None,"fresh_off":None,"driver":Path("driver"),"ir_bundle":[]}
        self.assertIsNone(p2_cli.check(self.report,argparse.Namespace(**base)))
        for changes in [{"fresh_on":Path("on")},{"fresh_off":Path("off")},{"ir_bundle":[Path("old21.json")]}]:
            with self.subTest(options=changes),self.assertRaises(ValueError):p2_cli.check(self.report,argparse.Namespace(**{**base,**changes}))

    def test_fresh_cli_queries_identities_only_after_current22_validation(self):
        # Constructed process response/driver bytes test caller ordering; no
        # extraction, rustc, VM or ledger execution is credited here.
        calls = []
        compiler = "rustc independently-queried-control\nhost: control-host\n"
        with tempfile.TemporaryDirectory(prefix="p2-cli-order-control-") as directory:
            driver = Path(directory) / "driver"
            driver.write_bytes(b"constructed-driver-identity")
            args = argparse.Namespace(fresh_on=Path(directory)/"on", fresh_off=Path(directory)/"off",
                                      driver=driver, ir_bundle=[])
            def validate(value):
                self.assertIs(value, self.report)
                calls.append("current22")
            def query(command, **options):
                self.assertEqual(calls, ["current22"])
                self.assertEqual(command[:2], ["rustup", "run"])
                self.assertEqual(command[-2:], ["rustc", "-vV"])
                self.assertEqual(options["timeout"], 30)
                calls.append("query-compiler")
                return SimpleNamespace(returncode=0, stdout=compiler, stderr="")
            def fresh(value, on, off, **identities):
                self.assertIs(value, self.report)
                self.assertEqual(calls, ["current22", "query-compiler"])
                self.assertEqual(identities["compiler"], compiler)
                self.assertEqual(identities["driver_sha256"], cost.sha(driver.read_bytes()))
                self.assertEqual(identities["cargo_options"], ["--manifest-path", "puzzles/Cargo.toml",
                    "--package", "chialisp-oxide-puzzles", "--locked"])
                calls.append("fresh22")
                return [{"constructed_order_control": True}]
            with patch.object(cost,"validate_current",side_effect=validate), patch.object(p2_cli.subprocess,"run",side_effect=query), patch.object(append,"verify_fresh_bundles",side_effect=fresh), patch.object(append,"project_validated",side_effect=AssertionError("early history")):
                self.assertEqual(p2_cli.check(self.report,args),[{"constructed_order_control": True}])
            self.assertEqual(calls,["current22","query-compiler","fresh22"])
            with patch.object(cost,"validate_current",side_effect=ValueError("stale current")), patch.object(p2_cli.subprocess,"run",side_effect=AssertionError("queried before current validation")), self.assertRaisesRegex(ValueError,"stale current"):
                p2_cli.check(self.report,args)

    def test_second_valid_fresh_identity_is_allowed_but_modes_ir_artifacts_and_actual_sources_are_closed(self):
        with tempfile.TemporaryDirectory(prefix="p2-fresh-constructed-control-") as directory:
            root=Path(directory)/"source";root.mkdir()
            for row in self.report["authoring"]["sources"]:
                target=root/row["path"];target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(cost.ROOT/row["path"],target)
            (root/"puzzles/authoring.json").write_bytes((cost.ROOT/"puzzles/authoring.json").read_bytes())
            profile=copy.deepcopy(self.profile);artifacts={p["slug"]:p["artifacts"] for p in self.report["puzzles"]};expected_ir={};bundles={};compiler="rustc constructed-control\nhost: test-host\n";driver="a"*64
            for optimize in [True,False]:
                output=Path(directory)/("on" if optimize else "off");folder=output/"capture";folder.mkdir(parents=True);entries=[]
                for slug,export in self.report["authoring"]["exports"].items():
                    name=export["entry"];stem=cost.sha(name.encode());ir={"name":name,"parameters":[],"result":{"kind":"unit"},"body":{"op":"unit"}};payload=cost.canonical(ir);(folder/(stem+".ir.json")).write_bytes(payload);expected_ir[name]=cost.sha(payload)
                    artifact=next(a for a in artifacts[slug] if a["backend"]=="rust_mir" and a["optimize_lir"] is optimize);(folder/(stem+".clvm.hex")).write_text(artifact["clvm_hex"]+"\n");entries.append({"entry":name,"ir":stem+".ir.json","clvm":stem+".clvm.hex","artifact":copy.deepcopy(artifact)})
                manifest={"schema_version":1,"status":"wip_prototype_not_ready_for_use","package":self.report["authoring"]["package"],"manifest_path":str(root/self.report["authoring"]["manifest"]),"workspace_root":str(root),"compiler":compiler,"driver_sha256":driver,"analysis_target":"test-host","toolchain":"nightly-2026-07-14","cargo_options":[],"rue_revision":self.report["rue_revision"],"entries":entries};(folder/"manifest.json").write_bytes(cost.canonical(manifest));(output/"latest.json").write_bytes(cost.canonical({"bundle":"capture","manifest":"capture/manifest.json"}));bundles[optimize]=(output,folder,manifest)
            profile["raw_ir_before"]=[{"entry":r["entry"],"sha256":expected_ir[r["entry"]]} for r in profile["raw_ir_before"]];profile["reviewed_after"]["raw_ir_added_sha256"]=expected_ir["chialisp_oxide_puzzles::p2_singleton::puzzle"]
            def verify(expected_compiler=compiler,expected_driver=driver):
                return append.verify_fresh_bundles(self.report,bundles[True][0],bundles[False][0],compiler=expected_compiler,driver_sha256=expected_driver,toolchain="nightly-2026-07-14",cargo_options=[])
            with patch.object(append,"ROOT",root),patch.object(append,"load",return_value=profile):
                first=verify();self.assertNotEqual(first[0]["fresh_manifest_sha256"],first[0]["recorded_manifest_sha256"])
                second_compiler=compiler+"release: second-controlled-capture\n";second_driver="b"*64
                for _,folder,manifest in bundles.values():manifest.update(compiler=second_compiler,driver_sha256=second_driver);(folder/"manifest.json").write_bytes(cost.canonical(manifest))
                second=verify(second_compiler,second_driver);self.assertNotEqual(first[0]["fresh_manifest_sha256"],second[0]["fresh_manifest_sha256"])
                for field in ["mode","ir","artifact","source","driver"]:
                    for _,folder,manifest in bundles.values():manifest.update(compiler=compiler,driver_sha256=driver);(folder/"manifest.json").write_bytes(cost.canonical(manifest))
                    verify();_,folder,manifest=bundles[True];row=manifest["entries"][0];target=None;original=None
                    if field=="mode":row["artifact"]["optimize_lir"]=False;(folder/"manifest.json").write_bytes(cost.canonical(manifest))
                    elif field=="artifact":row["artifact"]["disassembly"]+=" changed";(folder/"manifest.json").write_bytes(cost.canonical(manifest))
                    elif field=="driver":manifest["driver_sha256"]="c"*64;(folder/"manifest.json").write_bytes(cost.canonical(manifest))
                    else:
                        target=(folder/row["ir"]) if field=="ir" else (root/self.report["authoring"]["sources"][0]["path"]);original=target.read_bytes();target.write_bytes(original+b"\n")
                    try:
                        with self.subTest(field=field),self.assertRaises(ValueError):verify()
                    finally:
                        if target is not None:target.write_bytes(original)
                        if field=="mode":row["artifact"]["optimize_lir"]=True
                        if field=="artifact":row["artifact"]["disassembly"]=row["artifact"]["disassembly"].removesuffix(" changed")
                        manifest["driver_sha256"]=driver;(folder/"manifest.json").write_bytes(cost.canonical(manifest))
                verify()


if __name__=="__main__":unittest.main()
