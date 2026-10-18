"""Constructed bundle/file controls only; no compiler or execution credit."""
import copy
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from cargo_authoring import snapshot

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


if __name__ == "__main__":
    unittest.main()
