"""Nonvacuous constructed metadata controls; no new execution or promotion.

Actual reviewed current23 corpus/source/typed records are the positive basis. Only review bindings
are constructed in memory for corruption controls, independent of the committed
profile review state. This never changes its on-disk status/pin or grants
compiler/VM/ledger execution credit.
"""
import copy
from contextlib import nullcontext
import importlib.util
import json
from pathlib import Path
import tempfile
import shutil
import unittest
from unittest.mock import patch

import p2_source_cost_migration as migration
import p2_append_migration as append
import runtime_cost_migration as cost
import runtime_refinement_migration as refinement
import runtime_guarded_projection_migration as guarded
import runtime_composition_migration as composition
import p2dc_append_migration as outer
import corpus_append as current


def existing_p2(report):
    rows = [row for row in report["puzzles"] if row["slug"] == "p2-singleton"]
    cost.require(len(rows) == 1, "Unique existing P2 component required")
    return rows[0]


def constructed_profile(report, profile=None):
    if current.is_append(report):
        # The generic corpus append is the actual current root. Mutations may
        # rebind only the tip digests in memory so deep validators stay reachable.
        value = copy.deepcopy(current.load() if profile is None else profile)
        cost.require(value["status"] == current.FINAL, "Actual reviewed current corpus fixture required")
        payload = (cost.ROOT / "corpus/validation.json").read_bytes()
        if not cost.same(json.loads(payload), report):
            payload = cost.canonical(report)
        value["reviewed_after"].update(report_sha256=cost.sha(payload),
            canonical_report_sha256=cost.sha(cost.canonical(report)),
            added_puzzles=[dict(slug=row["slug"], sha256=cost.sha(cost.canonical(row))) for row in report["puzzles"][len(current.PRESERVED):]])
        return value
    if outer.is_append(report):
        # A real reviewed actual23 capture is mandatory. Mutations may rebind
        # only the tip digest in memory so deep validators remain reachable.
        value = copy.deepcopy(outer.load() if profile is None else profile)
        cost.require(value["status"] == outer.FINAL, "Actual reviewed23 fixture required")
        payload = (cost.ROOT / "corpus/validation.json").read_bytes()
        if not cost.same(json.loads(payload), report):
            payload = cost.canonical(report)
        added = next(row for row in report["puzzles"] if row["slug"] == outer.SLUG)
        value["reviewed_after"].update(report_sha256=cost.sha(payload),
            canonical_report_sha256=cost.sha(cost.canonical(report)),
            added_puzzle_sha256=cost.sha(cost.canonical(added)),
            protocol_sha256=cost.sha(cost.canonical(added["protocol_validation"])))
        return value
    if guarded.is_transition(report):
        value = copy.deepcopy(composition.load() if profile is None else profile)
        value["status"] = composition.FINAL
        after = value["reviewed_after"]
        payload = (cost.ROOT / "corpus/validation.json").read_bytes()
        if not cost.same(json.loads(payload), report):
            payload = cost.canonical(report)
        after.update(report_sha256=cost.sha(payload),
            canonical_report_sha256=cost.sha(cost.canonical(report)),
            p2_sha256=cost.sha(cost.canonical(existing_p2(report))),
            protocol_sha256=cost.sha(cost.canonical(existing_p2(report)["protocol_validation"])))
        return value
    value = copy.deepcopy(migration.load() if profile is None else profile)
    value["status"] = "reviewed_final_source_cost"
    payload = (cost.ROOT/"corpus/validation.json").read_bytes()
    if not cost.same(json.loads(payload),report):payload = cost.canonical(report)
    value["reviewed_after"] = {"report_sha256":cost.sha(payload),
        "canonical_report_sha256":cost.sha(cost.canonical(report)),
        "p2_sha256":cost.sha(cost.canonical(existing_p2(report))),
        "protocol_sha256":cost.sha(cost.canonical(existing_p2(report)["protocol_validation"])),
        "raw_ir_sha256":"3"*64,"metrics_sha256":cost.sha(migration.read(migration.METRICS_PATH)),
        "controls_sha256":cost.sha(migration.read(migration.CONTROLS_PATH)),
        "bundles":{"on":{"manifest_sha256":"1"*64,"optimize_lir":True},
                   "off":{"manifest_sha256":"2"*64,"optimize_lir":False}}}
    return value


def constructed_authorization(report):
    return bind_review(constructed_profile(report)) if migration.is_transition(report) else nullcontext()


def bind_review(profile):
    module = current if profile["protocol"] == current.PROTOCOL else outer if profile["protocol"] == outer.PROTOCOL else composition if profile["protocol"] == composition.PROTOCOL else migration if profile["protocol"] == "chialisp-oxide-p2-source-cost/v1" else append
    return patch.object(module,"load",return_value=profile)


def source_transition(report):
    checkpoint = cost.source_checkpoint(report)
    return migration.project_validated(checkpoint)


class PendingAndRetained(unittest.TestCase):
    def test_pending_default_cannot_restore_or_select_current(self):
        profile=migration.load()
        self.assertIn(profile["status"],("pending_root_final_capture","reviewed_final_source_cost"))
        pending=copy.deepcopy(profile);pending["status"]="pending_root_final_capture";pending["reviewed_after"]=None
        with patch.object(migration,"load",return_value=pending):
            for action in (migration.reviewed_after,migration.project_validated):
                with self.assertRaisesRegex(ValueError,"pending root final capture"):
                    action({"puzzles":[]})

    def test_retained_complete_row_recipes_and_four_profiles(self):
        self.assertEqual(migration.before_p2()["slug"],"p2-singleton")
        self.assertEqual(migration.validate_before_controls()["reconstructible_source_mutations"],3)
        for path,pin in migration.IMMUTABLE.items():
            self.assertEqual(cost.sha(migration.read(path,limit=16*1024*1024)),pin)
        value=migration.load()
        for key in ("scope","baseline","retained","immutable_profiles","source_transition","status","unknown"):
            changed=copy.deepcopy(value)
            if key=="unknown":changed[key]=True
            elif key=="scope":changed[key]+=" changed"
            else:changed[key]={}
            with self.subTest(key=key),self.assertRaisesRegex(ValueError,"profile bytes changed"):
                migration.load(cost.canonical(changed))

    def test_closed_profile_schema_controls_after_explicit_constructed_byte_rebinding(self):
        original=migration.load()
        for name in ("schema_bool","protocol","baseline","predecessor","source","status","pending_after","unknown"):
            migration.load()  # The positive byte/schema/retained basis is real.
            changed=copy.deepcopy(original)
            if name=="schema_bool":changed["schema_version"]=True
            elif name=="protocol":changed["protocol"]+="unknown"
            elif name=="baseline":changed["baseline"]["commit"]="0"*40
            elif name=="predecessor":changed["immutable_profiles"]["unreviewed.json"]="0"*64
            elif name=="source":changed["source_transition"]["path"]="puzzles/other/puzzle.rs"
            elif name=="status":changed["status"]="automatically_promoted"
            elif name=="pending_after":changed["status"]="pending_root_final_capture";changed["reviewed_after"]={}
            else:changed["unknown_allowance"]=True
            payload=cost.canonical(changed)
            with self.subTest(name=name),patch.object(migration,"PIN",cost.sha(payload)),self.assertRaises(ValueError):
                migration.load(payload)

    def test_retained_real_file_corruption_after_positive(self):
        profile=migration.load()
        original=migration.read
        for path in migration.RETAINED.values():
            migration.load()
            def changed(name,**kwargs):
                result=original(name,**kwargs)
                return result+b"\n" if name==path else result
            with self.subTest(path=path),patch.object(migration,"read",side_effect=changed),self.assertRaisesRegex(ValueError,"retained source-cost baseline"):
                migration.load()
        self.assertEqual(migration.load(),profile)


class MetricBindings(unittest.TestCase):
    def setUp(self):
        self.report={"puzzles":[migration.before_p2()]}
        self.metrics=json.loads(migration.read(migration.RETAINED["metrics"]))

    def test_exact_old_actual_generators_and_arithmetic_are_a_pure_positive(self):
        self.assertEqual(migration.validate_metrics(self.report,self.metrics)["accepted_bundle_metrics"],15)

    def test_cost_and_bundle_corruptions_have_positive_basis(self):
        for name in ("total","execution","condition","byte_cost","charged_bytes","generator","backrefs_length",
                     "coin","height","byte_rule","scope","mode","missing","float","bool","unknown"):
            migration.validate_metrics(self.report,self.metrics)
            bad=copy.deepcopy(self.metrics);row=bad["observations"][1]["joint"]
            if name=="total":row["total_conditions_cost"]+=1
            elif name=="execution":row["execution_cost"]+=1
            elif name=="condition":row["condition_cost"]+=1
            elif name=="byte_cost":row["observed_byte_cost_component"]+=1
            elif name=="charged_bytes":row["charged_generator_bytes_pinned_source_rule"]+=1
            elif name=="generator":row["generator_uncompressed_hex"]+="80"
            elif name=="backrefs_length":row["generator_backrefs_bytes"]+=1
            elif name=="coin":row["per_spend"][0]["coin_id"]="0"*64
            elif name=="height":row["previous_transaction_height"]+=1
            elif name=="byte_rule":row["cost_per_byte"]+=1
            elif name=="scope":bad["scope"]+=" authenticated"
            elif name=="mode":bad["observations"][1]["optimize_lir"]=1
            elif name=="missing":bad["observations"].pop()
            elif name=="float":row["total_conditions_cost"]=float(row["total_conditions_cost"])
            elif name=="bool":row["condition_cost"]=True
            else:row["unknown_cost_credit"]=True
            with self.subTest(name=name),self.assertRaises((ValueError,KeyError,TypeError)):
                migration.validate_metrics(self.report,bad)
        changed=copy.deepcopy(self.report)
        changed["puzzles"][-1]["protocol_validation"]["observations"][1]["lifecycle"]["joint"]["spends"][0]["solution_hex"]="80"
        with self.assertRaises(ValueError):migration.validate_metrics(changed,self.metrics)


class CurrentBeforeHistory(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.report=json.loads((cost.ROOT/"corpus/validation.json").read_bytes())
        if not outer.is_append(cls.report):
            raise ValueError("Actual source-cost current23 corpus fixture required; no silent skip")
        cls.profile=constructed_profile(cls.report)

    def test_positive_actual_current_then_exact_whole_signed_baseline(self):
        with bind_review(self.profile):
            cost.validate_current(self.report)
            old=source_transition(self.report)
            self.assertEqual(cost.sha(cost.canonical(old)),migration.BEFORE_CANONICAL)
            old21=cost.project_append(self.report)
            self.assertEqual(cost.sha(cost.canonical(old21)),append.REPORT_CANONICAL)
            self.assertEqual(cost.sha(cost.canonical(cost.preserve(self.report))),cost.load()["baseline"]["canonical_report_sha256"])

    def test_actual23_generic_checks_finish_without_any_historical_projection(self):
        calls = []
        original = cost.READABLE["check_report"]
        def checked(report):
            self.assertIs(report, self.report)
            calls.append("actual23")
            return original(report)
        with bind_review(self.profile), patch.dict(cost.READABLE, {"check_report": checked}), \
                patch.object(outer, "project_validated", side_effect=AssertionError("23 history during current validation")), \
                patch.object(composition, "project_validated", side_effect=AssertionError("22 history during current validation")), \
                patch.object(migration, "project_validated", side_effect=AssertionError("M1 history during current validation")), \
                patch.object(append, "project_validated", side_effect=AssertionError("E20 history during current validation")):
            cost.validate_current(self.report)
        self.assertEqual(calls, ["actual23"])

    def test_historical_objects_never_reach_current_validator(self):
        actual_validate=cost.validate_current;actual_restore=migration.project_validated;actual_append=append.project_validated;calls=[]
        def current(report):
            self.assertEqual(cost.canonical(report),cost.canonical(self.report));calls.append("actual23");return actual_validate(report)
        def source_restore(report):
            self.assertIn(calls, [["actual23"], ["actual23", "pure source"]])
            self.assertEqual(cost.sha(cost.canonical(report)), composition.INTERMEDIATE["canonical_report_sha256"])
            calls.append("pure source")
            return actual_restore(report)
        def append_restore(report):
            self.assertEqual(cost.sha(cost.canonical(report)),migration.BEFORE_CANONICAL)
            self.assertEqual(calls,["actual23","pure source","pure source"]);calls.append("pure append");return actual_append(report)
        with bind_review(self.profile),patch.object(cost,"validate_current",side_effect=current),patch.object(migration,"project_validated",side_effect=source_restore),patch.object(append,"project_validated",side_effect=append_restore):
            refinement.preserve(self.report)
        self.assertEqual(calls,["actual23","pure source","pure source","pure append"])

    def test_current_source_native_and_protocol_controls_reach_the_real_checks(self):
        for name in ("source","native","authoring","code","rollback","receipt","debug","preflight"):
            with bind_review(self.profile):cost.validate_current(self.report)
            bad=copy.deepcopy(self.report);p2=existing_p2(bad);negative=p2["protocol_validation"]["observations"][0]["negatives"][0]
            if name=="source":p2["source"]+="\n"
            elif name=="native":p2["native_sources"][0]["sha256"]="0"*64
            elif name=="authoring":bad["authoring"]["sources"][0]["sha256"]="0"*64
            elif name=="code":negative["consensus"]["code"]="CostExceeded"
            elif name=="rollback":negative["full_simulator_equality"]=False
            elif name=="receipt":p2["protocol_validation"]["observations"][0]["lifecycle"]["receipts"].pop()
            elif name=="debug":negative["before_debug"]="same text"
            else:negative["stage"]="signer_preflight"
            # Deliberately rebind the outer digest so it cannot mask deep failures.
            rebound=constructed_profile(bad)
            with self.subTest(name=name),bind_review(rebound),self.assertRaises((ValueError,KeyError)):
                cost.validate_current(bad)

    def test_pure_restoration_rejects_every_unreviewed_slot_after_positive_and_digest_rebind(self):
        for name in ("old21_source","old21_artifact","old21_cost","old21_ledger","old21_native","unknown",
                     "P2_reference","P2_Rue","P2_ABI","P2_coverage","P2_native","P2_output","P2_mode","P2_wrong_reveal","P2_foreign_protocol"):
            with bind_review(self.profile):source_transition(self.report)
            bad=copy.deepcopy(self.report);first=bad["puzzles"][0];p2=existing_p2(bad)
            if name=="old21_source":first["source"]+="\n"
            elif name=="old21_artifact":first["artifacts"][0]["clvm_hex"]+="80"
            elif name=="old21_cost":first["cases"][0]["runs"][0]["execution"]["cost"]+=1
            elif name=="old21_ledger":first["simulator"][0]["name"]+=" changed"
            elif name=="old21_native":next(p for p in bad["puzzles"] if p.get("native_sources"))["native_sources"][0]["sha256"]="0"*64
            elif name=="unknown":bad["unreviewed_allowance"]=True
            elif name=="P2_reference":p2["reference"]+="\n"
            elif name=="P2_Rue":p2["rue"]["source"]+="\n"
            elif name=="P2_ABI":p2["environment"]="strict"
            elif name=="P2_coverage":p2["generated_cases"]+=1
            elif name=="P2_native":p2["native_sources"].pop()
            elif name=="P2_output":p2["cases"][0]["runs"][0]["execution"]["outcome"]["output_hex"]="80"
            elif name=="P2_mode":p2["artifacts"][0]["optimize_lir"]=True
            elif name=="P2_wrong_reveal":p2["protocol_validation"]["observations"][2]["negatives"][16]["transaction"]["spends"][1]["reveal_hex"]="80"
            else:p2["protocol_validation"]["observations"][4]["inner_hash"]="0"*64
            rebound=constructed_profile(bad)
            with self.subTest(name=name),bind_review(rebound),self.assertRaises((ValueError,KeyError)):
                source_transition(bad)

    def test_paired_fresh_routing_selects_all23_and_preserves_requested_options(self):
        with bind_review(self.profile), patch.object(current, "verify_fresh_bundles", return_value=[{"mode": True}, {"mode": False}]) as checker:
            result = cost.verify_current22_bundles(self.report, Path("on"), Path("off"),
                compiler="independent", driver_sha256="a"*64, toolchain="dated", cargo_options=["--locked", "--offline"])
        self.assertEqual(result, [{"mode": True}, {"mode": False}])
        self.assertIs(checker.call_args.args[0], self.report)
        self.assertEqual(checker.call_args.kwargs["compiler"], "independent")
        self.assertEqual(checker.call_args.kwargs["cargo_options"], ["--locked", "--offline"])
        actual = {row["entry"]: row["sha256"] for row in self.profile["reviewed_after"]["raw_ir"]}
        added = {row["entry"] for row in current.load()["added"]}
        self.assertEqual(len(actual), len(current.current_slugs()))
        self.assertIn(outer.ENTRY, actual)
        self.assertEqual(actual[migration.ENTRY], migration.load()["reviewed_after"]["raw_ir_sha256"])
        self.assertEqual({entry: pin for entry, pin in actual.items() if entry not in (migration.ENTRY, outer.ENTRY) and entry not in added},
            {row["entry"]: row["sha256"] for row in append.load()["raw_ir_before"]})


class SidecarBindings(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.report=json.loads((cost.ROOT/"corpus/validation.json").read_bytes())
        cls.outer = current if current.is_append(cls.report) else outer if outer.is_append(cls.report) else composition if guarded.is_transition(cls.report) else migration
        cls.controls_path = cls.outer.CONTROLS if cls.outer in (current, outer, composition) else migration.CONTROLS_PATH
        cls.value=json.loads(cls.outer.read(cls.controls_path))
        migration.validate_controls(cls.outer.p2_component_view(cls.report),cls.value)

    def rehash_manifest(self,value,mutate):
        manifest=json.loads(value["manifest"]);mutate(manifest)
        value["manifest"]=json.dumps(manifest,indent=2)
        pin=cost.sha(value["manifest"].encode())
        value["identity"]["manifest_sha256"]=value["vm_replay"]["manifest_sha256"]=pin

    def test_rehashed_sidecar_metadata_corruptions_reach_the_pure_reader(self):
        names=("schema_bool","identity_schema_bool","manifest_path","unnormalized_path","relative_source_root",
            "toolchain","missing_host","duplicate_host","full_commit","compiler_header","analysis_target",
            "manifest_schema_bool","manifest_workspace","manifest_options","manifest_artifact",
            "native_count","native_observation","VM_mode","VM_distinction","VM_resource","VM_IR",
            "VM_missing","VM_duplicate","source_window","source_flag","scope","unknown")
        for name in names:
            profile=constructed_profile(self.report)
            with bind_review(profile):self.outer.check_sidecars(self.report)
            bad=copy.deepcopy(self.value)
            first=bad["vm_replay"]["controls"][0]
            if name=="schema_bool":bad["schema_version"]=True
            elif name=="identity_schema_bool":bad["identity"]["schema_version"]=True
            elif name=="manifest_path":bad["vm_replay"]["manifest_path"]="/constructed/other/manifest.json"
            elif name=="unnormalized_path":
                for part in ("identity","vm_replay"):bad[part]["manifest_path"]=bad[part]["manifest_path"].replace("/manifest.json","/./manifest.json")
            elif name=="relative_source_root":bad["window"]["source_root"]="relative/source"
            elif name=="toolchain":
                bad["window"]["toolchain"]=bad["identity"]["toolchain"]="stable"
                self.rehash_manifest(bad,lambda v:v.__setitem__("toolchain","stable"))
            elif name in ("missing_host","duplicate_host","full_commit","compiler_header"):
                compiler=bad["window"]["compiler"]
                if name=="missing_host":compiler="\n".join(line for line in compiler.splitlines() if not line.startswith("host: "))+"\n"
                elif name=="duplicate_host":compiler+="host: contradictory-host\n"
                elif name=="full_commit":compiler=compiler.replace("daf2e5e18b3abc4311d9da3d96fd64138f43d508","daf2e5e18"+"0"*31)
                else:compiler=compiler.replace("rustc 1.99.0-nightly","rustc 1.98.0-nightly",1)
                bad["window"]["compiler"]=bad["identity"]["compiler"]=compiler
                self.rehash_manifest(bad,lambda v:v.__setitem__("compiler",compiler))
            elif name=="analysis_target":self.rehash_manifest(bad,lambda v:v.__setitem__("analysis_target","contradictory-host"))
            elif name=="manifest_schema_bool":self.rehash_manifest(bad,lambda v:v.__setitem__("schema_version",True))
            elif name=="manifest_workspace":self.rehash_manifest(bad,lambda v:v.__setitem__("workspace_root","/constructed/other"))
            elif name=="manifest_options":self.rehash_manifest(bad,lambda v:v["cargo_options"].append("--release"))
            elif name=="manifest_artifact":self.rehash_manifest(bad,lambda v:v["entries"][0]["artifact"].__setitem__("disassembly","changed"))
            elif name=="native_count":bad["native"]["tests"]=True
            elif name=="native_observation":bad["native"]["observations"][0]["digest"]="0"*64
            elif name=="VM_mode":first["runs"][0]["optimized"]=True
            elif name=="VM_distinction":first["runs"][0]["distinguished"]=1
            elif name=="VM_resource":first["runs"][0]["execution"]={"outcome":{"status":"inconclusive"},"cost":None,"error":"resource","failure":{"kind":"resource","code":"cost_exceeded"}}
            elif name=="VM_IR":first["ir_sha256"]="0"*64
            elif name=="VM_missing":bad["vm_replay"]["controls"].pop()
            elif name=="VM_duplicate":bad["vm_replay"]["controls"][1]=copy.deepcopy(first)
            elif name=="source_window":bad["window"]["sources"][0]["sha256"]="0"*64
            elif name=="source_flag":bad["window"]["unchanged"]=1
            elif name=="scope":bad["scope"]="authenticated security proof"
            else:bad["unreviewed_credit"]=True
            payload=cost.canonical(bad);profile["reviewed_after"]["controls_sha256"]=cost.sha(payload)
            original_read=self.outer.read;reader=migration.validate_controls
            def altered(path,**kwargs):
                return payload if path==self.controls_path else original_read(path,**kwargs)
            with self.subTest(name=name),bind_review(profile),patch.object(self.outer,"read",side_effect=altered),patch.object(migration,"validate_controls",wraps=reader) as observed:
                with self.assertRaises((ValueError,KeyError,TypeError)):self.outer.check_sidecars(self.report)
                self.assertEqual(observed.call_count,1,"Outer digest masked the actual sidecar reader")

    def test_actual_temp_sources_and_declared_custom_entry_fail_closed(self):
        with tempfile.TemporaryDirectory(prefix="source-cost-window-control-") as directory:
            root=Path(directory)
            for row in self.value["window"]["sources"]:
                target=root/row["path"];target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(cost.ROOT/row["path"],target)
            with patch.object(migration,"ROOT",root):
                migration.validate_controls(self.outer.p2_component_view(self.report),self.value)
                for path in ("crates/chialisp-oxide-driver/build.rs","experiments/p2-singleton-controls/Cargo.lock",
                             "crates/chialisp-oxide/src/hashing.rs","scripts/cargo_bundle.py"):
                    migration.validate_controls(self.outer.p2_component_view(self.report),self.value)
                    target=root/path;old=target.read_bytes();target.write_bytes(old+b"\n")
                    try:
                        with self.subTest(path=path),self.assertRaises(ValueError):migration.validate_controls(self.outer.p2_component_view(self.report),self.value)
                    finally:target.write_bytes(old)
                # Known compiler source directories do not include an unused Cargo cache.
                cache=root/"crates/chialisp-oxide-ir/target/release/build/control/out/generated.rs"
                cache.parent.mkdir(parents=True);cache.write_text("ignored unused build output")
                migration.validate_controls(self.outer.p2_component_view(self.report),self.value)
                manifest=root/"crates/chialisp-oxide-ir/Cargo.toml";old=manifest.read_text();self.assertNotIn("[lib]",old)
                manifest.write_text(old+'\n[lib]\npath = "target/lib.rs"\n')
                (manifest.parent/"target/lib.rs").write_text("pub fn actual_custom_entry() {}")
                try:
                    with self.assertRaisesRegex(ValueError,"source entry outside reviewed src layout"):
                        migration.validate_controls(self.outer.p2_component_view(self.report),self.value)
                finally:manifest.write_text(old)
                migration.validate_controls(self.outer.p2_component_view(self.report),self.value)


if __name__=="__main__":unittest.main()
