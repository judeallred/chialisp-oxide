"""Offline independent password relation and curry-syntax controls."""
import hashlib
import copy
import json
from pathlib import Path
import runpy
import tempfile
import shutil
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from password_vectors import (PUBLIC_HASH, PUBLIC_PASSWORD, conditions, curry_tree,
                              deserialize, expected, generated, generated_for,
                              instance_cases, named, named_trees,
                              native_environment, proper, relation, serialize)


class PasswordRelationTests(unittest.TestCase):
    def test_disassembler_ascii_strings_preserve_bytes_and_reject_other_escapes(self):
        from password_vectors import parse
        self.assertEqual(parse('("abc" "a b" "a\\\"b" "a\\\\b")'), proper(b"abc", b"a b", b'a"b', b"a\\b"))
        for text in ['"unterminated', '"a\\nb"', '"é"']:
            with self.subTest(text=text), self.assertRaises(ValueError):
                parse(text)

    def test_exact_bytes_arbitrary_output_and_ignored_tail(self):
        for password in [b"", b"\0", b"\0\0", b"\0\x01", b"\x80\xff", b"a" * 8192]:
            for selected in [b"", b"\0", proper(b"\x08"), (b"\x01", b"\x02")]:
                for tail in [b"", b"\0", b"\0\0", (b"ignored", b"tail")]:
                    tree = hashlib.sha256(password).digest(), (password, (selected, tail))
                    self.assertEqual(relation(tree), {"status": "accepted", "output_hex": serialize(selected).hex()})

    def test_byte_normalization_and_narrower_domain_are_distinguished(self):
        tree = hashlib.sha256(b"\0\x01").digest(), (b"\0\x01", (b"\0", b"\0\0"))
        self.assertEqual(relation(tree)["status"], "accepted")
        for mutation in ("normalize", "strict_tail", "require_conditions_list"):
            self.assertEqual(relation(tree, **{mutation: True}), {"status": "rejected"})
        self.assertNotEqual(hashlib.sha256(b"\0").digest(), hashlib.sha256(b"").digest())

    def test_malformed_projection_hash_and_password_reject(self):
        for tree in [b"", (PUBLIC_HASH, b""), (PUBLIC_HASH, proper(PUBLIC_PASSWORD)),
                     ((b"x", b"y"), proper(PUBLIC_PASSWORD, b"")),
                     (PUBLIC_HASH, proper((b"x", b"y"), b"")),
                     (b"\0" + PUBLIC_HASH, proper(PUBLIC_PASSWORD, b"")),
                     (PUBLIC_HASH, proper(b"wrong", conditions()))]:
            with self.subTest(tree=tree):
                self.assertEqual(relation(tree), {"status": "rejected"})

    def test_curry_shape_preserves_solution_as_tail(self):
        module = proper(b"\x01", b"module")
        argument = PUBLIC_HASH
        solution = PUBLIC_PASSWORD, (conditions(), b"\0\0")
        self.assertEqual(native_environment(argument, solution), (argument, solution))
        curry = curry_tree(module, argument)
        self.assertEqual(curry, (b"\x02", ((b"\x01", module), ((b"\x04", ((b"\x01", argument), (b"\x01", b""))), b""))))
        self.assertEqual(deserialize(serialize(curry)), curry)
        self.assertNotEqual(curry, proper(b"\x02", (b"\x01", module), proper(b"\x04", argument, b"\x01")))
        self.assertEqual(relation(native_environment(argument, solution))["status"], "accepted")
        self.assertEqual(relation(proper(argument, solution)), {"status": "rejected"})

    def test_fixed_named_recipe_is_complete_and_has_success_and_failure(self):
        rows = named()
        self.assertEqual(len(rows), 48)
        self.assertEqual(len({row["label"] for row in rows}), 48)
        self.assertEqual([expected(row["solution"]) for row in rows], [relation(tree) for _, tree in named_trees()])
        self.assertEqual({expected(row["solution"])["status"] for row in rows}, {"accepted", "rejected"})

    def test_generated_pairs_are_balanced_and_keep_every_byte(self):
        rows = generated()
        self.assertEqual(len(rows), 256)
        self.assertEqual(len(generated_for()), 2315)
        self.assertEqual(sum(expected(row["solution"])["status"] == "accepted" for row in rows), 128)
        from password_vectors import parse
        for correct, wrong in zip(rows[::2], rows[1::2]):
            good, bad = parse(correct["solution"]), parse(wrong["solution"])
            self.assertEqual(good[1], bad[1])
            self.assertNotEqual(good[0], bad[0])
            self.assertEqual(relation(bad), {"status": "rejected"})

    def test_fixed_instance_never_takes_its_hash_from_the_solution(self):
        from password_vectors import parse
        rows = {row["label"]: row for row in instance_cases()}
        self.assertEqual(len(rows), 48)
        good = native_environment(PUBLIC_HASH, parse(rows["derived tail: valid_spend"]["solution"]))
        attacker = native_environment(PUBLIC_HASH, parse(rows["derived tail: different_committed_hash"]["solution"]))
        self.assertEqual(relation(good)["status"], "accepted")
        self.assertEqual(relation(attacker), {"status": "rejected"})
        self.assertEqual(dict(named_trees())["different_committed_hash"][0], hashlib.sha256(b"xyz").digest())


class PasswordEvidenceBoundaryTests(unittest.TestCase):
    def test_fresh_extraction_is_sequential_and_checks_actual_tool_and_bundle_identity(self):
        import password_cargo
        declared = {"manifest": "puzzles/Cargo.toml", "exports": {
            "password-coin": {"entry": "chialisp_oxide_puzzles::password_coin::puzzle"}}}
        report = {"authoring": declared, "rue_revision": "reviewed-rue-revision"}
        compiler = "rustc 1.99.0-nightly\nhost: test-host\ncommit-hash: reviewed-compiler\n"
        events = []
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cargo, driver = root / "cargo-chialisp-oxide", root / "chialisp-oxide-driver"
            cargo.write_bytes(b"prebuilt-cargo")
            driver.write_bytes(b"prebuilt-driver")
            (root / "rust-toolchain.toml").write_text('[toolchain]\nchannel = "nightly-2026-07-14"\n')

            def command(arguments, timeout):
                events.append(("command", arguments, timeout))
                return compiler if arguments == ["rustup", "run", "nightly-2026-07-14", "rustc", "-vV"] else ""

            def load(output, **identity):
                events.append(("load", identity["optimize"]))
                self.assertEqual(identity["root"], root)
                self.assertEqual(identity["authoring"], declared)
                self.assertEqual(identity["selected_entry"], declared["exports"]["password-coin"]["entry"])
                self.assertEqual(identity["compiler"], compiler)
                self.assertEqual(identity["driver_sha256"], hashlib.sha256(driver.read_bytes()).hexdigest())
                self.assertEqual(identity["toolchain"], "nightly-2026-07-14")
                self.assertEqual(identity["rue_revision"], report["rue_revision"])
                self.assertEqual(identity["cargo_options"], ["--manifest-path", "puzzles/Cargo.toml", "--locked", "--offline"])
                self.assertEqual(identity["current_authoring"](), declared)
                return SimpleNamespace(manifest_path=output / "fresh" / "manifest.json")

            def verify(manifest, observed):
                events.append(("verify", manifest.parent.parent.name))
                self.assertIs(observed, report)
                return {"optimize_lir": manifest.parent.parent.name == "on", "old_ir_exports": 20}

            with patch.object(password_cargo, "ROOT", root), \
                    patch.object(password_cargo, "validate_current", side_effect=lambda value: events.append(("current", value))), \
                    patch.object(password_cargo, "snapshot", side_effect=lambda actual: copy.deepcopy(declared)), \
                    patch.object(password_cargo, "command", side_effect=command), \
                    patch.object(password_cargo, "load_declared_export", side_effect=load), \
                    patch.object(password_cargo, "verify_ir_bundle", side_effect=verify):
                summaries = password_cargo.extract(report, cargo=cargo, driver=driver)

        self.assertEqual([event[0] for event in events], ["current", "command", "command", "load", "verify", "command", "load", "verify"])
        self.assertEqual(events[1], ("command", ["rustup", "run", "nightly-2026-07-14", "rustc", "-vV"], 30))
        builds = [event for event in events[2:] if event[0] == "command"]
        for event, mode in zip(builds, ("on", "off")):
            arguments = event[1]
            self.assertEqual(arguments[:6], [cargo, "build", "--manifest-path", "puzzles/Cargo.toml", "--locked", "--offline"])
            self.assertEqual(arguments[6], "--output-dir")
            self.assertEqual(arguments[7].name, mode)
            self.assertEqual(arguments[8:], [] if mode == "on" else ["--no-optimize"])
            self.assertEqual(event[2], 300)
        self.assertEqual([summary["optimize_lir"] for summary in summaries], [True, False])
        self.assertTrue(all(summary["compiler_driver_source_identity_checked"] for summary in summaries))

    def test_fresh_extraction_aborts_before_second_build_when_sources_or_tools_change(self):
        import password_cargo
        for mutation in ("source", "cargo", "driver", "mode"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                cargo, driver = root / "cargo-chialisp-oxide", root / "chialisp-oxide-driver"
                cargo.write_bytes(b"cargo-before")
                driver.write_bytes(b"driver-before")
                (root / "rust-toolchain.toml").write_text('[toolchain]\nchannel = "nightly-2026-07-14"\n')
                declared = {"manifest": "puzzles/Cargo.toml", "exports": {"password-coin": {"entry": "actual::password"}}}
                current = copy.deepcopy(declared)
                report = {"authoring": declared, "rue_revision": "reviewed"}

                def verify(*_args):
                    if mutation == "source": current["manifest"] = "changed/Cargo.toml"
                    elif mutation == "cargo": cargo.write_bytes(b"cargo-after")
                    elif mutation == "driver": driver.write_bytes(b"driver-after")
                    return {"optimize_lir": False if mutation == "mode" else True}

                with patch.object(password_cargo, "ROOT", root), \
                        patch.object(password_cargo, "validate_current", return_value=None), \
                        patch.object(password_cargo, "snapshot", side_effect=lambda actual: copy.deepcopy(current)), \
                        patch.object(password_cargo, "command", return_value="rustc test\nhost: test-host\n") as command, \
                        patch.object(password_cargo, "load_declared_export", return_value=SimpleNamespace(manifest_path=root / "manifest.json")), \
                        patch.object(password_cargo, "verify_ir_bundle", side_effect=verify), \
                        self.assertRaisesRegex(ValueError, {"source": "inputs changed", "cargo": "changed during extraction", "driver": "changed during extraction", "mode": "mode differs"}[mutation]):
                    password_cargo.extract(report, cargo=cargo, driver=driver)
                self.assertEqual(command.call_count, 2)  # rustc identity, first extraction only.

    def test_current_native_corruptions_fail_before_any_historical_projection(self):
        from password_provenance import ROOT
        import p2_append_migration as append
        import runtime_cost_migration as cost
        runtime = runpy.run_path(str(ROOT / "scripts/check-runtime-frames.py"))
        report = json.loads((ROOT / "corpus/validation.json").read_bytes())
        self.assertTrue(append.is_append(report))
        profile = copy.deepcopy(append.load())
        allowed = {row["path"] for row in report["authoring"]["sources"]}
        allowed.update(f"puzzles/{puzzle['slug']}/puzzle.json" for puzzle in report["puzzles"])
        native_globals = append.current_native_sources.__globals__
        real_contained = native_globals["contained_file"]

        def confined(root, path):
            self.assertIn(path, allowed)  # No unknown report-supplied path read.
            return real_contained(root, path)

        def no_projection(*_args, **_kwargs):
            self.fail("A historical projection ran before current native freshness failed")

        for mutation in ("stale", "missing", "duplicate", "unknown"):
            bad = copy.deepcopy(report)
            rows = next(p for p in bad["puzzles"] if p["slug"] == "sha256tree")["native_sources"]
            if mutation == "stale": rows[2]["sha256"] = "0" * 64
            elif mutation == "missing": rows.pop()
            elif mutation == "duplicate": rows.append(copy.deepcopy(rows[0]))
            else: rows[0]["path"] = "../../outside-native.rs"
            # Constructed authorization around a corrupted record: bypass only
            # its outer digest so the real current native closure is tested.
            profile["reviewed_after"]["canonical_report_sha256"] = cost.sha(cost.canonical(bad))
            with self.subTest(mutation=mutation), patch.object(append,"load",return_value=profile), \
                    patch.dict(native_globals, {"contained_file": confined}), \
                    patch.dict(runtime["check"].__globals__, {"password_preservation": no_projection, "hashing_preservation": no_projection}), \
                    self.assertRaisesRegex(ValueError, "Current22.*native"):
                runtime["check"](bad, {})

    def test_replay_invokes_only_retained_bytes_with_the_exact_budget(self):
        from password_provenance import ROOT
        gate = runpy.run_path(str(ROOT / "scripts/check-password-coin.py"))
        accepted = {"outcome": {"status": "accepted", "output_hex": "80"}, "cost": 1, "error": None}

        def invoke(command, **_kwargs):
            self.assertEqual(command[0:2], ["/task/cli", "run-batch"])
            self.assertEqual(Path(command[2]).read_text(), "01\n")
            self.assertEqual(json.loads(Path(command[4]).read_text()), ["()"])
            self.assertEqual(command[3::2], ["--solutions", "--max-cost"])
            self.assertEqual(command[-1], "100000")
            self.assertNotIn("build", command)
            return SimpleNamespace(returncode=0, stdout=json.dumps([accepted]), stderr="")

        with patch.object(gate["subprocess"], "run", side_effect=invoke):
            self.assertEqual(gate["run_batch"](Path("/task/cli"), b"\x01", ["()"], 100_000), [accepted])

    def test_replay_resource_failure_cannot_confirm_a_program_negative(self):
        from password_provenance import ROOT
        gate = runpy.run_path(str(ROOT / "scripts/check-password-coin.py"))
        for outcome, kind, code in [({"status": "inconclusive"}, "resource", "cost_exceeded"),
                                    ({"status": "rejected"}, "resource", "cost_exceeded"),
                                    ({"status": "rejected"}, "program", "unknown")]:
            observation = {"outcome": outcome, "cost": None, "error": "Raise(spoofed)", "failure": {"kind": kind, "code": code}}
            result = SimpleNamespace(returncode=0, stdout=json.dumps([observation]), stderr="")
            with self.subTest(kind=kind, code=code), patch.object(gate["subprocess"], "run", return_value=result), self.assertRaises(ValueError):
                gate["run_batch"](Path("/task/cli"), b"\x01", ["()"], 1_000_000)


class PasswordCurrent22Tests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import runtime_cost_migration as cost
        import p2_append_migration as append
        cls.root = cost.ROOT
        cls.report = json.loads((cls.root / "corpus/validation.json").read_bytes())
        if not append.is_append(cls.report):
            raise ValueError("Actual current22 fixture required; no silent skip")
        cost.validate_current(cls.report)
        cls.historical = cost.preserve(cls.report)

    def test_password_controls_validate_current_before_pure_history_and_real_native_checks(self):
        import password_controls
        import runtime_password_migration as migration
        real = migration.current_native_sources
        native_calls = []
        def current(value):
            self.assertEqual(len(value["puzzles"]), 22, "Historical object reached source freshness")
            native_calls.append(value)
            return real(value)
        with patch.object(migration, "current_native_sources", side_effect=current):
            self.assertGreater(password_controls.self_test(self.report), 80)
        self.assertEqual(len(native_calls), 11)  # initial + five positive/negative pairs
        self.assertEqual(sum(value is self.report for value in native_calls), 6)
        # A historical-only default must fail its positive freshness basis,
        # rather than claiming five vacuous native rejections.
        with self.assertRaises(ValueError): migration.controls(self.historical)

    def test_both_callers_pass_actual_current_report_to_native_migration_controls(self):
        import password_controls
        import runtime_password_migration as migration
        real = migration.controls
        calls = []
        def capture(history, *, current_report):
            self.assertEqual(len(history["puzzles"]), 21)
            self.assertIs(current_report, self.report)
            calls.append((history, current_report))
            return real(history, current_report=current_report)
        with patch.object(password_controls, "migration_controls", side_effect=capture):
            password_controls.self_test(self.report)
        runtime = runpy.run_path(str(self.root / "scripts/check-runtime-frames.py"))
        baseline = runtime["load_baseline"](runtime["BASELINE_PATH"].read_bytes())
        with patch.dict(runtime["self_test"].__globals__, {"password_controls": capture}):
            runtime["self_test"](self.report, baseline)
        self.assertEqual(len(calls), 2)

    def test_cli_extract_and_supplied_fresh_checks_precede_any_historical_check(self):
        import password_cargo
        import password_controls
        gate = runpy.run_path(str(self.root / "scripts/check-password-coin.py"))
        for mode in ("extract", "supplied"):
            events = []
            def current(value):
                self.assertEqual(len(value["puzzles"]), 22); events.append("check22"); return {}
            def extract(value, **kwargs):
                self.assertEqual(len(value["puzzles"]), 22); events.append("fresh22"); return []
            def self_test(value):
                self.assertEqual(len(value["puzzles"]), 22); events.append("selftest22"); return 1
            def supplied(value, args):
                if mode == "supplied": events.append("fresh22"); return []
                return None
            args = ["password-check", "--self-test"] + (["--extract-ir"] if mode == "extract" else ["--fresh-on", "/on", "--fresh-off", "/off"])
            with patch.object(sys, "argv", args), patch.dict(gate["main"].__globals__, {"check": current, "print": lambda *a, **k: None}), patch.object(password_cargo, "extract", side_effect=extract), patch.object(password_controls, "self_test", side_effect=self_test), patch.object(gate["p2_cli"], "check", side_effect=supplied):
                gate["main"]()
            self.assertEqual(events, ["fresh22", "check22", "selftest22"])

    def test_constructed22_pair_checks_actual_sources_metadata_IR_and_artifacts(self):
        """Unit IR/headers plus recorded artifacts; NOT fresh policy executions."""
        import password_cargo
        import p2_append_migration as append
        from cargo_authoring import snapshot
        # In-memory rawIR bindings authorize only constructed loader controls.
        # Real source bytes and recorded CLVM are used without a preservation claim.
        with tempfile.TemporaryDirectory(prefix="password-current22-loader-control-") as directory:
            root = Path(directory) / "source"; root.mkdir()
            for row in self.report["authoring"]["sources"]:
                target = root / row["path"]; target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(self.root / row["path"], target)
            cargo, driver = root / "cargo-chialisp-oxide", root / "chialisp-oxide-driver"
            cargo.write_bytes(b"constructed cargo identity"); driver.write_bytes(b"constructed driver identity")
            compiler = "rustc constructed-current22-loader\nhost: test-host\n"
            profile = copy.deepcopy(append.load())
            expected_ir = {}
            def command(arguments, timeout):
                if arguments[0] == "rustup": return compiler
                output = Path(arguments[arguments.index("--output-dir") + 1]); folder = output / "capture"; folder.mkdir(parents=True)
                optimize = "--no-optimize" not in arguments
                entries = []
                for slug, export in self.report["authoring"]["exports"].items():
                    entry = export["entry"]; stem = hashlib.sha256(entry.encode()).hexdigest()
                    # Constructed identity fixture, not admitted policy semantics.
                    ir = {"name": entry, "parameters": [], "result": {"kind":"unit"}, "body":{"op":"unit"}}
                    payload = json.dumps(ir, sort_keys=True).encode(); (folder / f"{stem}.ir.json").write_bytes(payload); expected_ir[entry] = hashlib.sha256(payload).hexdigest()
                    artifact = copy.deepcopy(next(a for p in self.report["puzzles"] if p["slug"] == slug for a in p["artifacts"] if a["backend"] == "rust_mir" and a["optimize_lir"] is optimize))
                    (folder / f"{stem}.clvm.hex").write_text(artifact["clvm_hex"] + "\n")
                    entries.append({"entry":entry,"ir":f"{stem}.ir.json","clvm":f"{stem}.clvm.hex","artifact":artifact})
                profile["raw_ir_before"] = [{"entry":r["entry"],"sha256":expected_ir[r["entry"]]} for r in profile["raw_ir_before"]]
                profile["reviewed_after"]["raw_ir_added_sha256"] = expected_ir["chialisp_oxide_puzzles::p2_singleton::puzzle"]
                manifest = {"schema_version":1,"status":"wip_prototype_not_ready_for_use","package":self.report["authoring"]["package"],"manifest_path":str(root / "puzzles/Cargo.toml"),"workspace_root":str(root),"compiler":compiler,"driver_sha256":hashlib.sha256(driver.read_bytes()).hexdigest(),"analysis_target":"test-host","toolchain":"nightly-2026-07-14","cargo_options":["--manifest-path","puzzles/Cargo.toml","--locked","--offline"],"rue_revision":self.report["rue_revision"],"entries":entries}
                if mutation[0] == "mode": manifest["entries"][0]["artifact"]["optimize_lir"] = not optimize
                elif mutation[0] == "driver": manifest["driver_sha256"] = "0" * 64
                elif mutation[0] == "options": manifest["cargo_options"].append("--release")
                elif mutation[0] == "IR" and not optimize:
                    path = folder / manifest["entries"][0]["ir"]; path.write_bytes(path.read_bytes()+b"\n")
                elif mutation[0] == "artifact" and not optimize: manifest["entries"][0]["artifact"]["disassembly"] += " changed"
                (folder / "manifest.json").write_text(json.dumps(manifest)); (output / "latest.json").write_text(json.dumps({"bundle":"capture","manifest":"capture/manifest.json"}))
                return ""
            mutation = [None]
            def validate(value):
                self.assertIs(value, self.report)
                self.assertEqual(snapshot(root), value["authoring"])
            with patch.object(password_cargo,"ROOT",root), patch.object(append,"ROOT",root), patch.object(append,"load",return_value=profile), patch.object(append,"project_validated",side_effect=AssertionError("history before fresh22 pair")), patch.object(password_cargo,"validate_current",side_effect=validate), patch.object(password_cargo,"command",side_effect=command), patch.object(password_cargo,"verify_ir_bundle",side_effect=AssertionError("legacy21 validator received current22")):
                for kind in ("mode","driver","options","IR","artifact"):
                    mutation[0] = None
                    summaries = password_cargo.extract(self.report,cargo=cargo,driver=driver)
                    self.assertEqual([r["optimize_lir"] for r in summaries],[True,False])
                    self.assertTrue(all(r["checked_current_exports"] == 22 for r in summaries))
                    mutation[0] = kind
                    with self.subTest(mutation=kind), self.assertRaises(ValueError): password_cargo.extract(self.report,cargo=cargo,driver=driver)


if __name__ == "__main__":
    unittest.main()
