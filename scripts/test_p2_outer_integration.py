"""Offline controls of actual current23 records and constructed review bindings.

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
import p2_source_cost_migration as source_cost
import runtime_guarded_projection_migration as guarded
import runtime_composition_migration as composition
import p2dc_append_migration as outer
import corpus_append as current
from test_p2_source_cost import constructed_profile, bind_review, existing_p2


def bind_after(report, profile=None):
    if outer.is_append(report) or source_cost.is_transition(report):
        return constructed_profile(report,profile)
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
        if not outer.is_append(cls.report):
            raise ValueError("Explicit actual current23 record fixture required; no silent skip")
        cls.profile = bind_after(cls.report)

    def test_actual_current_checks_never_project_inside_hash_source_or_protocol_gates(self):
        with bind_review(self.profile), patch.object(append,"project_validated",side_effect=AssertionError("history before current checks")):
            cost.validate_current(self.report)
            self.assertTrue(cost.hash_diagnostic_profile(self.report))

    def test_current23_checks_then_exact21_and_older_boundaries(self):
        with bind_review(self.profile):
            cost.validate_current(self.report)
            normalized=cost.project_append(self.report)
            self.assertEqual(cost.sha(cost.canonical(normalized)),append.REPORT_CANONICAL)
            self.assertEqual(cost.sha(cost.canonical(refinement.preserve(self.report))),refinement.REPORT_CANONICAL)
            self.assertEqual(cost.sha(cost.canonical(inline.preserve(self.report))),inline.REPORT_CANONICAL)
            self.assertEqual(cost.sha(cost.canonical(cost.preserve(self.report))),cost.load()["baseline"]["canonical_report_sha256"])

    def test_only_current23_reaches_source_validation(self):
        calls=[]
        original=append.project_validated
        def validate(value):
            self.assertIs(value,self.report);calls.append("current23")
        def project(value):
            self.assertEqual(cost.sha(cost.canonical(value)),source_cost.BEFORE_CANONICAL)
            self.assertEqual(calls,["current23"]);calls.append("append-to21");return original(value)
        with bind_review(self.profile),patch.object(cost,"validate_current",side_effect=validate),patch.object(append,"project_validated",side_effect=project):
            historical=cost.preserve(self.report)
        self.assertEqual(calls,["current23","append-to21"])
        self.assertEqual(cost.sha(cost.canonical(historical)),cost.load()["baseline"]["canonical_report_sha256"])

    def test_current_semantic_corruptions_survive_rebinding_the_constructed_outer_digest(self):
        def raw(report):return next(c for c in existing_p2(report)["cases"] if c["native"]["status"]=="accepted")
        controls={
            "source":lambda r:existing_p2(r).__setitem__("source",existing_p2(r)["source"]+"\n"),
            "native_closure":lambda r:existing_p2(r)["native_sources"][0].__setitem__("sha256","0"*64),
            "protocol_missing":lambda r:existing_p2(r)["protocol_validation"]["observations"].pop(),
            "protocol_resource":lambda r:existing_p2(r)["protocol_validation"]["observations"][0]["negatives"][0].__setitem__("observed","CostExceeded"),
            "protocol_not_submitted":lambda r:existing_p2(r)["protocol_validation"]["observations"][0]["negatives"][0].__setitem__("stage","signer_preflight"),
            "protocol_state":lambda r:existing_p2(r)["protocol_validation"]["observations"][0]["negatives"][0].__setitem__("full_simulator_equality",False),
            "protocol_receipt":lambda r:existing_p2(r)["protocol_validation"]["observations"][0]["lifecycle"]["receipts"].pop(),
            "protocol_debug":lambda r:existing_p2(r)["protocol_validation"]["observations"][0]["negatives"][0].__setitem__("before_debug","private SDK text"),
            "lifecycle_debug":lambda r:existing_p2(r)["protocol_validation"]["observations"][0]["lifecycle"].__setitem__("joint_after_debug","private SDK text"),
            "generated_count":lambda r:existing_p2(r).__setitem__("generated_cases",2186),
            "native_output":lambda r:raw(r).__setitem__("native",{"status":"rejected"}),
            "coherent_wrong_output":lambda r:[raw(r).__setitem__("native",{"status":"accepted","output_hex":"80"}),*[row["execution"].__setitem__("outcome",{"status":"accepted","output_hex":"80"}) for row in raw(r)["runs"]]],
            "non_boolean_mode":lambda r:existing_p2(r)["artifacts"][0].__setitem__("optimize_lir",0),
            "generic_ledger_credit":lambda r:existing_p2(r)["simulator"].append({"name":"invented","passed":True,"scope":"ledger","rejections":[]}),
        }
        for name,mutation in controls.items():
            with bind_review(self.profile):cost.validate_current(self.report)
            bad=copy.deepcopy(self.report);mutation(bad);self.assertNotEqual(cost.canonical(bad),cost.canonical(self.report))
            profile=bind_after(bad,self.profile)
            with self.subTest(control=name),bind_review(profile),patch.object(append,"project_validated",side_effect=AssertionError("projected corrupt current")),self.assertRaises((ValueError,KeyError,TypeError,IndexError)):
                cost.validate_current(bad)

    def test_missing_duplicate_and_reordered_current_rows_cannot_dispatch_to_history(self):
        with bind_review(self.profile):
            cost.validate_current(self.report)
            current_projector = cost.current22_migration(self.report).project_validated
            current_projector(self.report)
            for name in ("missing", "duplicate", "order"):
                bad = copy.deepcopy(self.report)
                if name == "missing": bad["puzzles"].pop()
                elif name == "duplicate": bad["puzzles"].append(copy.deepcopy(bad["puzzles"][-1]))
                else: bad["puzzles"].reverse()
                for action in (cost.validate_current, current_projector):
                    with self.subTest(shape=name, action=action.__name__), self.assertRaises((ValueError, KeyError, TypeError, IndexError)):
                        action(bad)
            # Pure historical dispatch remains independent of current files.
            historical = cost.project_append(self.report)
            with patch.object(cost, "validate_current", side_effect=AssertionError("current checks on history")):
                self.assertIs(cost.project_append(historical), historical)

    def test_fifteen_outer_controls_target_unique_existing_component_in_actual23(self):
        import p2_outer_controls

        # Real actual FINAL23 validation first; no constructed review binding.
        cost.validate_current(self.report)
        old = next(p for p in self.report["puzzles"] if p["slug"] == append.ADDED)
        new = next(p for p in self.report["puzzles"] if p["slug"] == outer.SLUG)
        self.assertIs(self.report["puzzles"][len(outer.component.OLD_SLUGS)], new)
        total = len(self.report["puzzles"])
        others = {p["slug"]: cost.canonical(p) for p in self.report["puzzles"] if p["slug"] != append.ADDED}
        seen = {"current": [], "project": []}
        # The first projection away from the whole current input is now the
        # generic corpus append; the delegated projector receives history.
        import corpus_append as generic  # `current` is reused as a loop name below.
        real_current, real_project = cost.validate_current, generic.project_validated

        def observe(kind, action, value):
            if value is not self.report:
                # These remain whole current-input corruptions, not restored
                # history or a one-component view sent to current validators.
                self.assertEqual(value["authoring"], self.report["authoring"])
                self.assertEqual({p["slug"]: cost.canonical(p) for p in value["puzzles"]
                                  if p["slug"] != append.ADDED}, others)
                self.assertEqual(next(p for p in value["puzzles"] if p["slug"] == outer.SLUG), new)
                self.assertNotEqual(cost.canonical(value), cost.canonical(self.report))
                seen[kind].append(copy.deepcopy(value))
            return action(value)

        with patch.object(cost, "validate_current", side_effect=lambda value: observe("current", real_current, value)), \
                patch.object(generic, "project_validated", side_effect=lambda value: observe("project", real_project, value)):
            self.assertEqual(p2_outer_controls.self_test(self.report), 15)
        names = ("source", "native", "resource", "preflight", "rollback", "receipt", "debug_rejection",
                 "debug_lifecycle", "artifact", "generated", "missing", "duplicate", "order", "pipeline", "unknown")
        self.assertEqual(len(seen["current"]), len(names))
        self.assertEqual(len(seen["project"]), len(names))
        for name, current, projected in zip(names, seen["current"], seen["project"], strict=True):
            with self.subTest(control=name):
                self.assertEqual(cost.canonical(current), cost.canonical(projected))
                selected = [p for p in current["puzzles"] if p["slug"] == append.ADDED]
                if name == "missing":
                    self.assertEqual(selected, [])
                    self.assertEqual(len(current["puzzles"]), total - 1)
                elif name == "duplicate":
                    self.assertEqual(selected, [old, old])
                    self.assertEqual(len(current["puzzles"]), total + 1)
                else:
                    self.assertEqual(len(selected), 1)
                    if name in ("order", "pipeline", "unknown"):
                        self.assertEqual(selected[0], old)
                    else:
                        self.assertNotEqual(cost.canonical(selected[0]), cost.canonical(old))

    def test_pending_and_exact_report_bytes_fail_closed(self):
        pending=copy.deepcopy(self.profile)
        if pending["protocol"] == current.PROTOCOL:pending["status"]=current.PENDING;pending["reviewed_after"]=None
        elif pending["protocol"] == outer.PROTOCOL:pending["status"]=outer.PENDING;pending["reviewed_after"]=None
        elif pending["protocol"] == composition.PROTOCOL:pending["status"]=composition.PENDING;pending["reviewed_after"]=None
        elif pending["protocol"] == "chialisp-oxide-runtime-guarded-projection/v1":pending["status"]=guarded.PENDING;pending["reviewed_after"]=None
        elif pending["protocol"] == "chialisp-oxide-p2-source-cost/v1":pending["status"]="pending_root_final_capture"
        else:pending["reviewed_after"]["status"]="pending"
        with bind_review(pending),self.assertRaisesRegex(ValueError,"pending"):
            cost.validate_current(self.report)
        with bind_review(self.profile):
            payload=(cost.ROOT/"corpus/validation.json").read_bytes();cost.check_report_bytes(payload,self.report)
            with self.assertRaisesRegex(ValueError,"report bytes"):
                cost.check_report_bytes(payload+b"\n",self.report)

    def test_historical_measurements_exclude_p2_without_silent_zip_truncation(self):
        with bind_review(self.profile):
            for script in ["check-cost-optimization.py","check-helper-inlining.py","check-runtime-refinement.py"]:
                checker=runpy.run_path(str(cost.ROOT/'scripts'/script));summary=checker["measurements"](self.report)
                self.assertEqual(len(summary["puzzles"]),21);self.assertEqual(summary["named_cases"],389)
                self.assertNotIn(append.ADDED,[p["slug"] for p in summary["puzzles"]]);self.assertIn("excluded",summary["scope"])

    def test_explicit_fresh_flags_are_paired_and_follow_current_checks(self):
        base={"fresh_on":None,"fresh_off":None,"driver":Path("driver"),"ir_bundle":[]}
        self.assertIsNone(p2_cli.check(self.report,argparse.Namespace(**base)))
        for changes in [{"fresh_on":Path("on")},{"fresh_off":Path("off")},{"ir_bundle":[Path("old21.json")]}]:
            with self.subTest(options=changes),self.assertRaises(ValueError):p2_cli.check(self.report,argparse.Namespace(**{**base,**changes}))

    def test_fresh_cli_queries_identities_only_after_current23_validation(self):
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
                calls.append("current23")
            def query(command, **options):
                self.assertEqual(calls, ["current23"])
                self.assertEqual(command[:2], ["rustup", "run"])
                self.assertEqual(command[-2:], ["rustc", "-vV"])
                self.assertEqual(options["timeout"], 30)
                calls.append("query-compiler")
                return SimpleNamespace(returncode=0, stdout=compiler, stderr="")
            def fresh(value, on, off, **identities):
                self.assertIs(value, self.report)
                self.assertEqual(calls, ["current23", "query-compiler"])
                self.assertEqual(identities["compiler"], compiler)
                self.assertEqual(identities["driver_sha256"], cost.sha(driver.read_bytes()))
                self.assertEqual(identities["cargo_options"], ["--manifest-path", "puzzles/Cargo.toml",
                    "--package", "chialisp-oxide-puzzles", "--locked"])
                calls.append("fresh23")
                return [{"constructed_order_control": True}]
            with patch.object(cost,"validate_current",side_effect=validate), patch.object(p2_cli.subprocess,"run",side_effect=query), patch.object(cost,"verify_current22_bundles",side_effect=fresh), patch.object(append,"project_validated",side_effect=AssertionError("early history")):
                self.assertEqual(p2_cli.check(self.report,args),[{"constructed_order_control": True}])
            self.assertEqual(calls,["current23","query-compiler","fresh23"])
            calls.clear()
            with patch.object(cost,"validate_current",side_effect=validate), patch.object(p2_cli.subprocess,"run",side_effect=query), patch.object(cost,"verify_current22_bundles",side_effect=fresh):
                cost.validate_current(self.report)
                with patch.object(cost,"validate_current",side_effect=AssertionError("repeated current validation")):
                    self.assertEqual(p2_cli._check_after_current_validation(self.report,args),[{"constructed_order_control": True}])
            self.assertEqual(calls,["current23","query-compiler","fresh23"])
            with patch.object(cost,"validate_current",side_effect=ValueError("stale current")), patch.object(p2_cli.subprocess,"run",side_effect=AssertionError("queried before current validation")), self.assertRaisesRegex(ValueError,"stale current"):
                p2_cli.check(self.report,args)

    def test_new_and_historical_default_clis_validate_actual_current_once(self):
        payload = (cost.ROOT / "corpus/validation.json").read_bytes()
        for script in ("check-runtime-composition.py", "check-p2-source-cost.py"):
            gate = runpy.run_path(str(cost.ROOT / "scripts" / script))
            calls, original = [], cost.validate_current
            def validate(report):
                self.assertIs(report, self.report)
                calls.append("actual-current23")
                return original(report)
            with self.subTest(script=script), bind_review(self.profile), patch.object(cost, "validate_current", side_effect=validate):
                gate["validate"](self.report, payload)
            self.assertEqual(calls, ["actual-current23"])

    def test_checkpoint_reader_is_pure_and_never_checks_old174_against_current(self):
        with bind_review(self.profile), patch.object(cost, "validate_current", side_effect=AssertionError("current on history")), \
                patch.object(source_cost, "validate_controls", side_effect=AssertionError("old174 current freshness")):
            report = composition.checkpoint_recorded()
        self.assertEqual(cost.sha(cost.canonical(report)), guarded.BEFORE_CANONICAL)

    def test_second_valid_fresh_identity_is_allowed_but_modes_ir_artifacts_and_actual_sources_are_closed(self):
        with tempfile.TemporaryDirectory(prefix="p2-fresh-constructed-control-") as directory:
            root=Path(directory)/"source";root.mkdir()
            for row in self.report["authoring"]["sources"]:
                target=root/row["path"];target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(cost.ROOT/row["path"],target)
            (root/"puzzles/authoring.json").write_bytes((cost.ROOT/"puzzles/authoring.json").read_bytes())
            with bind_review(self.profile):cost.validate_current(self.report)
            profile=copy.deepcopy(self.profile);artifacts={p["slug"]:p["artifacts"] for p in self.report["puzzles"]};expected_ir={};bundles={};compiler="rustc constructed-control\nhost: test-host\n";driver="a"*64
            for optimize in [True,False]:
                output=Path(directory)/("on" if optimize else "off");folder=output/"capture";folder.mkdir(parents=True);entries=[]
                for slug,export in self.report["authoring"]["exports"].items():
                    name=export["entry"];stem=cost.sha(name.encode());ir={"name":name,"parameters":[],"result":{"kind":"unit"},"body":{"op":"unit"}};payload=cost.canonical(ir);(folder/(stem+".ir.json")).write_bytes(payload);expected_ir[name]=cost.sha(payload)
                    artifact=next(a for a in artifacts[slug] if a["backend"]=="rust_mir" and a["optimize_lir"] is optimize);(folder/(stem+".clvm.hex")).write_text(artifact["clvm_hex"]+"\n");entries.append({"entry":name,"ir":stem+".ir.json","clvm":stem+".clvm.hex","artifact":copy.deepcopy(artifact)})
                manifest={"schema_version":1,"status":"wip_prototype_not_ready_for_use","package":self.report["authoring"]["package"],"manifest_path":str(root/self.report["authoring"]["manifest"]),"workspace_root":str(root),"compiler":compiler,"driver_sha256":driver,"analysis_target":"test-host","toolchain":"nightly-2026-07-14","cargo_options":["--locked"],"rue_revision":self.report["rue_revision"],"entries":entries};(folder/"manifest.json").write_bytes(cost.canonical(manifest));(output/"latest.json").write_bytes(cost.canonical({"bundle":"capture","manifest":"capture/manifest.json"}));bundles[optimize]=(output,folder,manifest)
            profile["reviewed_after"]["raw_ir"]=[{"entry":entry,"sha256":pin} for entry,pin in sorted(expected_ir.items())]
            active = current if current.is_append(self.report) else outer
            def verify(expected_compiler=compiler,expected_driver=driver):
                return active.verify_fresh_bundles(self.report,bundles[True][0],bundles[False][0],
                    compiler=expected_compiler,driver_sha256=expected_driver,toolchain="nightly-2026-07-14",cargo_options=["--locked"])
            with patch.object(active,"ROOT",root),patch.object(active,"load",return_value=profile):
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
