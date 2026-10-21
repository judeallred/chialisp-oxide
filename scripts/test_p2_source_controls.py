"""Constructed bundle/file controls only; no compiler or execution credit."""
import copy
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from cargo_authoring import snapshot
import p2_source_cost_migration as source_cost

SPEC = importlib.util.spec_from_file_location("p2_source_control_gate", Path(__file__).with_name("check-p2-source-controls.py"))
GATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GATE)


class SourceControlIdentity(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="p2-source-loader-constructed-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "source"
        inputs = [row["path"] for row in snapshot(GATE.ROOT)["sources"]]
        inputs += [f"{GATE.CONTROL}/{p}" for p in GATE.PRIVATE_FILES]
        for path in inputs:
            destination = self.root / path
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(GATE.ROOT / path, destination)
        self.output = Path(self.temporary.name) / "bundle"
        self.folder = self.output / "capture"
        self.folder.mkdir(parents=True)
        self.compiler = "rustc constructed-loader-control\nhost: control-host\n"
        self.driver = "a" * 64
        self.toolchain = "nightly-2026-07-14"
        entries = []
        for name in GATE.NAMES:
            entry = f"chialisp_oxide_p2_singleton_controls::{name}::puzzle"
            # Identity fixtures deliberately have unit bodies, NOT policy IR.
            self.write(self.folder / f"{name}.ir.json", {"name": entry, "parameters": [],
                       "result": {"kind": "unit"}, "body": {"op": "unit"}})
            (self.folder / f"{name}.clvm.hex").write_text("80\n")
            entries.append({"entry": entry, "ir": f"{name}.ir.json", "clvm": f"{name}.clvm.hex",
                "artifact": {"backend": "rust_mir", "optimize_lir": True, "bytes": 1,
                             "clvm_hex": "80", "tree_hash": "0" * 64, "disassembly": "()"}})
        control = self.root / GATE.CONTROL
        self.manifest = {"schema_version": 1, "status": "wip_prototype_not_ready_for_use",
            "package": GATE.PACKAGE, "manifest_path": str(control / "Cargo.toml"),
            "workspace_root": str(control), "compiler": self.compiler, "driver_sha256": self.driver,
            "analysis_target": "control-host", "toolchain": self.toolchain,
            "rue_revision": GATE.RUE, "cargo_options": GATE.CARGO_OPTIONS, "entries": entries}
        self.publish()
        self.write(self.output / "latest.json", {"bundle": "capture", "manifest": "capture/manifest.json"})

    @staticmethod
    def write(path, value):
        path.write_text(json.dumps(value, sort_keys=True) + "\n")

    def publish(self):
        self.write(self.folder / "manifest.json", self.manifest)

    def check(self):
        return GATE.validate_bundle(self.output, root=self.root, compiler=self.compiler,
                                   driver_sha256=self.driver, toolchain=self.toolchain)

    def test_positive_constructed_all_three_exports_and_actual_seven_sources(self):
        result = self.check()
        self.assertEqual([r["name"] for r in result["exports"]], list(GATE.NAMES))
        self.assertEqual(len(result["private_authoring"]["sources"]), 7)
        self.assertEqual(result["main_authoring"], snapshot(self.root))
        self.assertIn("does not extract", result["scope"])
        self.assertIn("authenticate execution", result["scope"])
        self.assertEqual(result["implementation_sources"], [
            {"path": "scripts/" + path.name, "sha256": GATE.digest(path.read_bytes())}
            for path in [Path(GATE.__file__), Path(GATE.cargo_authoring.__file__),
                         Path(GATE.cargo_bundle.__file__), Path(GATE.cargo_source_receipt.__file__)]])

    def test_schema2_supplied_bundle_keeps_complete_receipt_admission(self):
        self.manifest["schema_version"] = 2
        self.manifest["package_id"] = "constructed-supplied-source-control-package"
        files = []
        control = self.root / GATE.CONTROL
        for label, path, kind, count in [
            ("package/Cargo.toml", control / "Cargo.toml", "launcher_manifest", 0),
            ("package/src/lib.rs", control / "src/lib.rs", "compiler_text", 1),
        ]:
            raw = path.read_bytes(); digest = GATE.digest(raw); blob = f"source-{digest}.bin"
            (self.folder / blob).write_bytes(raw)
            files.append(dict(path=label, kinds=[kind], compiler_read_count=count,
                              bytes=len(raw), sha256=digest, blob=blob))
        exports = []
        for row in self.manifest["entries"]:
            raw = (self.folder / row["ir"]).read_bytes()
            exports.append(dict(entry=row["entry"], ir=row["ir"], bytes=len(raw), sha256=GATE.digest(raw)))
        self.manifest["source_receipt"] = dict(
            protocol=GATE.cargo_source_receipt.PROTOCOL, scope=GATE.cargo_source_receipt.SCOPE,
            build=dict(package=GATE.PACKAGE, package_id=self.manifest["package_id"],
                       compiler_sha256=GATE.digest(self.compiler.encode()), toolchain=self.toolchain,
                       analysis_target="control-host", driver_sha256=self.driver,
                       rue_revision=GATE.RUE, cargo_options=GATE.CARGO_OPTIONS, optimize_lir=True),
            unit=dict(crate_name="chialisp_oxide_p2_singleton_controls", analysis_target="control-host",
                      features=[], library_source="package/src/lib.rs"),
            exports=exports, files=files,
            snapshots=dict(package_manifest="package/Cargo.toml", workspace_manifest="package/Cargo.toml",
                           lockfile=None, build_script=None),
            omitted=dict(outside_roots_reads=0, outside_roots_bytes=0))
        self.publish()
        with patch.object(subprocess, "run", side_effect=AssertionError("identity gate cannot execute tools")):
            self.assertEqual(len(self.check()["exports"]), 3)
        receipt = self.manifest["source_receipt"]
        # A non-selected export's receipt and every declared source blob remain
        # mandatory, even though this caller only requests identity observations.
        receipt["exports"][-1]["sha256"] = "0" * 64
        self.publish()
        with self.assertRaisesRegex(ValueError, "exact IR"):
            self.check()
        receipt["exports"][-1]["sha256"] = exports[-1]["sha256"] = GATE.digest(
            (self.folder / self.manifest["entries"][-1]["ir"]).read_bytes())
        self.publish(); self.check()
        (self.folder / files[-1]["blob"]).write_bytes(b"x" * files[-1]["bytes"])
        with self.assertRaisesRegex(ValueError, "digest"):
            self.check()

    def test_manifest_and_unselected_IR_identity_corruptions(self):
        original = copy.deepcopy(self.manifest)
        for kind in ("swapped_IR", "wrong_IR_name", "mode", "driver", "package", "options",
                     "toolchain", "workspace", "compiler", "Rue", "duplicate", "CLVM"):
            self.manifest = copy.deepcopy(original)
            self.publish()
            self.check()  # Positive loader path precedes every corruption.
            restore = None
            row = self.manifest["entries"][2]  # Also checks an unselected export.
            if kind == "swapped_IR":
                first = self.manifest["entries"][0]
                first["ir"], row["ir"] = row["ir"], first["ir"]
            elif kind == "wrong_IR_name":
                path = self.folder / row["ir"]
                restore = (path, path.read_bytes())
                value = json.loads(path.read_bytes()); value["name"] = "different::puzzle"
                self.write(path, value)
            elif kind == "mode": row["artifact"]["optimize_lir"] = False
            elif kind == "driver": self.manifest["driver_sha256"] = "b" * 64
            elif kind == "package": self.manifest["package"] = "different-package"
            elif kind == "options": self.manifest["cargo_options"] += ["--offline"]
            elif kind == "toolchain": self.manifest["toolchain"] = "stable"
            elif kind == "workspace": self.manifest["workspace_root"] = str(self.root)
            elif kind == "compiler": self.manifest["compiler"] += "different release\n"
            elif kind == "Rue": self.manifest["rue_revision"] = "0" * 40
            elif kind == "duplicate": row["entry"] = self.manifest["entries"][0]["entry"]
            else:
                path = self.folder / row["clvm"]
                restore = (path, path.read_bytes()); path.write_text("01\n")
            self.publish()
            try:
                with self.subTest(kind=kind), self.assertRaises(ValueError): self.check()
            finally:
                if restore: restore[0].write_bytes(restore[1])
        self.manifest = original; self.publish(); self.check()

    def test_reviewed_recipe_baseline_and_exact_source_mutations(self):
        for relative in (GATE.BASELINE, f"{GATE.CONTROL}/mutations.json",
                         f"{GATE.CONTROL}/src/wrong_dollar.rs"):
            self.check()
            path = self.root / relative; old = path.read_bytes(); path.write_bytes(old + b"\n")
            try:
                with self.subTest(path=relative), self.assertRaises(ValueError): self.check()
            finally: path.write_bytes(old)

    def test_private_and_external_SDK_actual_bytes_change_during_loading(self):
        real_loader = GATE.load_declared_export
        for relative in (f"{GATE.CONTROL}/Cargo.lock", "crates/chialisp-oxide/src/hashing.rs"):
            self.check()
            path = self.root / relative; old = path.read_bytes()
            def mutate_then_load(*args, **kwargs):
                path.write_bytes(old + b"\n")
                return real_loader(*args, **kwargs)
            try:
                with self.subTest(path=relative), patch.object(GATE, "load_declared_export", side_effect=mutate_then_load), self.assertRaises(ValueError): self.check()
            finally: path.write_bytes(old)

    def test_main_and_private_sources_checked_after_last_loader(self):
        real_loader = GATE.load_declared_export
        for relative in (f"{GATE.CONTROL}/Cargo.lock", "crates/chialisp-oxide/src/tree.rs"):
            self.check()
            path = self.root / relative; old = path.read_bytes(); calls = []
            def load_then_mutate(*args, **kwargs):
                result = real_loader(*args, **kwargs); calls.append(result)
                if len(calls) == 3: path.write_bytes(old + b"\n")
                return result
            try:
                with self.subTest(path=relative), patch.object(GATE, "load_declared_export", side_effect=load_then_mutate), self.assertRaises(ValueError): self.check()
                self.assertEqual(len(calls), 3)
            finally: path.write_bytes(old)

    def test_cli_independently_queries_compiler_and_hashes_actual_driver(self):
        driver = Path(self.temporary.name) / "driver"; driver.write_bytes(b"constructed driver identity")
        response = subprocess.CompletedProcess([], 0, self.compiler, "")
        with patch.object(GATE.subprocess, "run", return_value=response) as query:
            compiler, identity, toolchain = GATE.captured_identities(self.root, driver)
        self.assertEqual((compiler, identity, toolchain), (self.compiler, GATE.digest(driver.read_bytes()), self.toolchain))
        self.assertEqual(query.call_args.args[0], ["rustup", "run", self.toolchain, "rustc", "-vV"])
        for returncode, stderr in ((1, ""), (0, "warning")):
            with patch.object(GATE.subprocess, "run", return_value=subprocess.CompletedProcess([], returncode, self.compiler, stderr)), self.assertRaises(ValueError): GATE.captured_identities(self.root, driver)
        with patch.object(GATE.subprocess, "run", side_effect=subprocess.TimeoutExpired("rustc", 30)), self.assertRaises(subprocess.TimeoutExpired): GATE.captured_identities(self.root, driver)


class CurrentSourceClosure(unittest.TestCase):
    def current_fixture(self):
        # Construct current metadata around retained finite observations. These
        # edited declarations are test fixtures, never fresh execution evidence.
        report = json.loads((GATE.ROOT / "corpus/validation.json").read_bytes())
        report["puzzles"] = [row for row in report["puzzles"] if row["slug"] == "p2-singleton"]
        self.assertEqual(len(report["puzzles"]), 1)
        value = json.loads((GATE.ROOT / "corpus/corpus-append-existing-p2-controls.json").read_bytes())
        value["window"]["sources"] = source_cost.control_source_window(report)
        value["identity"]["implementation_sources"] = [
            dict(path="scripts/" + name, sha256=source_cost.sha(source_cost.read("scripts/" + name)))
            for name in ("check-p2-source-controls.py", "cargo_authoring.py", "cargo_bundle.py", "cargo_source_receipt.py")]
        source_cost.validate_controls(report, value)
        return report, value

    def test_current_control_window_requires_imported_receipt_helper(self):
        report, value = self.current_fixture()
        self.assertIn("scripts/cargo_source_receipt.py", [row["path"] for row in value["window"]["sources"]])
        missing_identity = copy.deepcopy(value)
        missing_identity["identity"]["implementation_sources"].pop()
        with self.assertRaisesRegex(ValueError, "Stale control compiler"):
            source_cost.validate_controls(report, missing_identity)
        missing_window = copy.deepcopy(value)
        missing_window["window"]["sources"] = [row for row in missing_window["window"]["sources"]
                                               if row["path"] != "scripts/cargo_source_receipt.py"]
        with self.assertRaisesRegex(ValueError, "Stale control source window"):
            source_cost.validate_controls(report, missing_window)

    def test_current_helper_bytes_are_bound_by_window_and_identity_separately(self):
        report, value = self.current_fixture()
        original = source_cost.read
        def changed(path, **kwargs):
            raw = original(path, **kwargs)
            return raw + b"\n" if path == "scripts/cargo_source_receipt.py" else raw
        with patch.object(source_cost, "read", side_effect=changed):
            with self.assertRaisesRegex(ValueError, "Stale control source window"):
                source_cost.validate_controls(report, value)
            value["window"]["sources"] = source_cost.control_source_window(report)
            with self.assertRaisesRegex(ValueError, "Stale control compiler"):
                source_cost.validate_controls(report, value)

    def test_historical_control_retention_stays_an_immutable_data_relation(self):
        result = source_cost.validate_before_controls()
        self.assertEqual(result["retained_private_inputs"], 7)
        self.assertEqual(result["reconstructible_source_mutations"], 3)
        self.assertIn("no current freshness or new execution credit", result["scope"])

    def test_capture_producer_explicitly_requests_artifact_only(self):
        spec = importlib.util.spec_from_file_location("constructed_capture_p2_controls", GATE.ROOT / "scripts/capture-p2-controls.py")
        producer = importlib.util.module_from_spec(spec); spec.loader.exec_module(producer)
        class StopBeforeExtraction(Exception):
            pass
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            with patch.object(sys, "argv", ["capture-p2-controls.py", "--bundle", str(base / "bundle"),
                                             "--replay", str(base / "replay.json")]), \
                    patch.object(producer.corpus_append, "report_payload", return_value=b'{"puzzles":[{"slug":"p2-singleton"}]}'), \
                    patch.object(producer.source, "control_source_window", return_value=[]), \
                    patch.object(producer.gate, "captured_identities", return_value=("constructed", "a" * 64, "pinned")), \
                    patch.object(producer, "run", side_effect=StopBeforeExtraction) as command, \
                    self.assertRaises(StopBeforeExtraction):
                producer.main()
            self.assertEqual(command.call_args.args[0], [producer.ROOT / "target/debug/cargo-chialisp-oxide",
                "build", "--artifact-only", *GATE.CARGO_OPTIONS, "--output-dir", base / "bundle"])


if __name__ == "__main__":
    unittest.main()
