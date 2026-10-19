"""Offline publication/identity controls; no Cargo, compiler, VM or solver runs."""

import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import cargo_authoring
import cargo_bundle
from cargo_bundle import load_declared_export


COMPILER = "rustc 1.99.0-nightly (test 2026-07-13)\ncommit-hash: " + "1" * 40 + "\nhost: test-host\n"
DRIVER = "2" * 64
TOOLCHAIN = "nightly-2026-07-14"
RUE = "3" * 40


class Fixture:
    def __init__(self, path):
        self.root, self.output = path / "source", path / "output"
        self.bundle = self.output / "run"
        self.root.mkdir()
        self.bundle.mkdir(parents=True)
        for name in ["Cargo.toml", "lib.rs", "other.rs"]:
            (self.root / name).write_text(f"declared {name}\n")
        self.configuration = {
            "mode": "cargo_library", "package": "controls", "manifest": "Cargo.toml", "library": "lib.rs",
            "exports": {"first": {"entry": "controls::one", "source": "lib.rs"},
                        "second": {"entry": "controls::two", "source": "other.rs"}},
            "inputs": ["Cargo.toml", "lib.rs", "other.rs", "authoring.json"],
        }
        self.write(self.root / "authoring.json", self.configuration)
        self.authoring = self.snapshot()
        self.program = {"name": "controls::one", "parameters": [], "result": {"kind": "bool"},
                        "body": {"op": "bool", "value": True}}
        self.write(self.output / "latest.json", {"bundle": "run", "manifest": "run/manifest.json"})
        self.manifest = {
            "schema_version": 1, "status": "wip_prototype_not_ready_for_use", "package": "controls",
            "manifest_path": str(self.root / "Cargo.toml"), "workspace_root": str(self.root),
            "compiler": COMPILER, "driver_sha256": DRIVER, "toolchain": TOOLCHAIN,
            "analysis_target": "test-host", "rue_revision": RUE,
            "cargo_options": ["--locked"], "entries": [],
        }
        for name in ["one", "two"]:
            program = {**self.program, "name": f"controls::{name}"}
            self.write(self.bundle / f"{name}.ir.json", program)
            (self.bundle / f"{name}.clvm.hex").write_text("01\n")
            self.manifest["entries"].append({"entry": program["name"], "ir": f"{name}.ir.json",
                "clvm": f"{name}.clvm.hex", "artifact": {"backend": "rust_mir", "bytes": 1,
                "clvm_hex": "01", "tree_hash": "4" * 64, "disassembly": "1", "optimize_lir": True}})
        self.save()

    @staticmethod
    def write(path, value):
        path.write_text(json.dumps(value))

    def save(self):
        self.write(self.bundle / "manifest.json", self.manifest)

    def snapshot(self):
        config = json.loads((self.root / "authoring.json").read_text())
        return {key: value for key, value in config.items() if key != "inputs"} | {
            "sources": [{"path": path, "sha256": hashlib.sha256((self.root / path).read_bytes()).hexdigest()}
                        for path in sorted(config["inputs"])]}

    def load(self, **overrides):
        arguments = {"root": self.root, "authoring": self.authoring, "selected_entry": "controls::one",
                     "compiler": COMPILER, "driver_sha256": DRIVER, "toolchain": TOOLCHAIN,
                     "current_authoring": self.snapshot, "rue_revision": RUE, "cargo_options": ["--locked"], "optimize": True}
        return load_declared_export(self.output, **(arguments | overrides))


class CargoBundleTests(unittest.TestCase):
    def fixture(self):
        directory = tempfile.TemporaryDirectory(prefix="chialisp-oxide-bundle-controls-")
        self.addCleanup(directory.cleanup)
        return Fixture(Path(directory.name))

    def test_valid_export_retains_original_bytes_and_does_not_infer_policy(self):
        fixture = self.fixture()
        loaded = fixture.load()
        self.assertEqual(loaded.program, fixture.program)
        self.assertEqual(loaded.path, (fixture.bundle / "one.ir.json").resolve())
        self.assertEqual(loaded.manifest, fixture.manifest)
        self.assertEqual(loaded.ir_sha256, hashlib.sha256(loaded.path.read_bytes()).hexdigest())
        # The generic loader must not pretend to establish a policy or ABI.
        changed = copy.deepcopy(fixture.program)
        changed["body"]["value"] = False
        fixture.write(loaded.path, changed)
        self.assertEqual(fixture.load().program, changed)

    def test_supported_envelope_identities_are_checked_for_every_export(self):
        # This remains an identity reader: Rust validates capability and all IR.
        for name in ["one", "two"]:
            for version, environment in [(2, "ignored"), (3, "raw"),
                    (4, "strict"), (4, "ignored"), (4, "raw"),
                    (5, "strict"), (5, "ignored"), (5, "raw")]:
                with self.subTest(export=name, version=version, environment=environment):
                    fixture = self.fixture()
                    body = {**fixture.program, "name": f"controls::{name}"}
                    if version in (4, 5):
                        body["functions"] = []
                    if version == 5:
                        body["result"] = {"kind": "pair", "first": {"kind": "bool"},
                                          "rest": {"kind": "bool"}}
                        body["body"] = {"op": "pair", "first": {"op": "bool", "value": True},
                                        "rest": {"op": "bool", "value": False}}
                    program = {"ir_version": version, "environment": environment, "program": body}
                    fixture.write(fixture.bundle / f"{name}.ir.json", program)
                    loaded = fixture.load()
                    if name == "one":
                        self.assertEqual(loaded.program, program)
                    body["name"] = "controls::not_the_declared_export"
                    fixture.write(fixture.bundle / f"{name}.ir.json", program)
                    with self.assertRaisesRegex(ValueError, "identity"):
                        fixture.load()

    def test_v5_envelope_fields_and_environment_are_closed(self):
        valid = {"ir_version": 5, "environment": "strict", "program": {
            "name": "controls::one", "parameters": [], "result": {"kind": "bool"},
            "body": {"op": "bool", "value": True}, "functions": []}}
        mutations = {
            "unknown version": lambda p: p.update(ir_version=9),
            "boolean version": lambda p: p.update(ir_version=True),
            "float version": lambda p: p.update(ir_version=5.0),
            "string version": lambda p: p.update(ir_version="5"),
            "unknown environment": lambda p: p.update(environment="pair"),
            "boolean environment": lambda p: p.update(environment=True),
            "missing environment": lambda p: p.pop("environment"),
            "extra envelope field": lambda p: p.update(extra=True),
            "missing functions": lambda p: p["program"].pop("functions"),
            "extra program field": lambda p: p["program"].update(extra=True),
            "missing body": lambda p: p["program"].pop("body"),
            "nonobject program": lambda p: p.update(program=[]),
        }
        for name in ["one", "two"]:
            for label, mutate in mutations.items():
                with self.subTest(export=name, mutation=label):
                    fixture = self.fixture()
                    program = copy.deepcopy(valid)
                    program["program"]["name"] = f"controls::{name}"
                    fixture.write(fixture.bundle / f"{name}.ir.json", program)
                    fixture.load()
                    mutate(program)
                    fixture.write(fixture.bundle / f"{name}.ir.json", program)
                    with self.assertRaises(ValueError):
                        fixture.load()

    def test_cons_source_is_required_and_fingerprinted_in_main_authoring(self):
        source_root = Path(__file__).resolve().parent.parent
        current = cargo_authoring.snapshot(source_root)
        cons = "crates/chialisp-oxide/src/cons.rs"
        import corpus_append
        self.assertEqual(sorted(current["exports"]), sorted(corpus_append.current_slugs()))
        self.assertIn("p2-delegated-conditions", current["exports"])
        self.assertIn(cons, {item["path"] for item in current["sources"]})
        directory = tempfile.TemporaryDirectory(prefix="chialisp-oxide-main-authoring-controls-")
        self.addCleanup(directory.cleanup)
        root = Path(directory.name)
        # Copy exactly declared files; ignored Cargo outputs are not traversed.
        for item in current["sources"]:
            path = root / item["path"]
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes((source_root / item["path"]).read_bytes())
        self.assertEqual(cargo_authoring.snapshot(root), current)
        metadata_path = root / cargo_authoring.AUTHORING
        metadata = json.loads(metadata_path.read_text())
        omitted = copy.deepcopy(metadata)
        omitted["inputs"].remove(cons)
        Fixture.write(metadata_path, omitted)
        with self.assertRaisesRegex(ValueError, "omit required"):
            cargo_authoring.snapshot(root)
        metadata_path.write_bytes((source_root / cargo_authoring.AUTHORING).read_bytes())
        cons_path = root / cons
        original = cons_path.read_bytes()
        cons_path.unlink()
        with self.assertRaisesRegex(ValueError, "Missing file"):
            cargo_authoring.snapshot(root)
        cons_path.write_bytes(original + b"\n// declared-input mutation control\n")
        mutated = cargo_authoring.snapshot(root)
        expected = copy.deepcopy(current)
        next(item for item in expected["sources"] if item["path"] == cons)["sha256"] = hashlib.sha256(cons_path.read_bytes()).hexdigest()
        self.assertEqual(mutated, expected)
        self.assertNotEqual(mutated, current)

    def test_manifest_identity_and_publication_corruptions_fail_closed(self):
        mutations = {
            "wrong package": lambda f: f.manifest.update(package="different"),
            "wrong manifest": lambda f: f.manifest.update(manifest_path=str(f.root / "lib.rs")),
            "relative manifest": lambda f: f.manifest.update(manifest_path="Cargo.toml"),
            "wrong workspace": lambda f: f.manifest.update(workspace_root=str(f.output)),
            "wrong compiler": lambda f: f.manifest.update(compiler=COMPILER + "changed"),
            "wrong driver": lambda f: f.manifest.update(driver_sha256="5" * 64),
            "wrong toolchain": lambda f: f.manifest.update(toolchain="nightly-other"),
            "wrong target": lambda f: f.manifest.update(analysis_target="other-host"),
            "wrong Rue": lambda f: f.manifest.update(rue_revision="6" * 40),
            "wrong options": lambda f: f.manifest.update(cargo_options=[]),
            "wrong status": lambda f: f.manifest.update(status="complete"),
            "bool schema": lambda f: f.manifest.update(schema_version=True),
            "missing export": lambda f: f.manifest["entries"].pop(),
            "duplicate export": lambda f: f.manifest["entries"].__setitem__(1, copy.deepcopy(f.manifest["entries"][0])),
            "unexpected export": lambda f: f.manifest["entries"][1].update(entry="controls::unexpected"),
            "duplicate file": lambda f: f.manifest["entries"][1].update(ir="one.ir.json"),
            "unselected identity": lambda f: f.write(f.bundle / "two.ir.json", f.program),
            "unselected missing IR": lambda f: (f.bundle / "two.ir.json").unlink(),
            "missing CLVM": lambda f: (f.bundle / "two.clvm.hex").unlink(),
            "CLVM mismatch": lambda f: (f.bundle / "two.clvm.hex").write_text("80\n"),
            "CLVM metadata length": lambda f: f.manifest["entries"][1]["artifact"].update(bytes=2),
            "CLVM bool length": lambda f: f.manifest["entries"][1]["artifact"].update(bytes=True),
            "missing artifact": lambda f: f.manifest["entries"][1].pop("artifact"),
            "wrong optimization": lambda f: f.manifest["entries"][1]["artifact"].update(optimize_lir=False),
            "malformed options": lambda f: f.manifest.update(cargo_options="--locked"),
            "unsupported IR version": lambda f: f.write(f.bundle / "two.ir.json", {"ir_version": 9, "environment": "raw", "program": f.program}),
            "mixed wire": lambda f: f.write(f.bundle / "two.ir.json", f.program | {"environment": "raw"}),
        }
        for name, mutate in mutations.items():
            with self.subTest(mutation=name):
                fixture = self.fixture()
                mutate(fixture)
                fixture.save()
                with self.assertRaises((ValueError, FileNotFoundError)):
                    fixture.load()

    def test_pointer_and_path_aliases_traversal_and_symlink_escapes_fail_closed(self):
        for raw in ["./one.ir.json", "a//one.ir.json", "../one.ir.json", "/one.ir.json", "a/../one.ir.json", "a\\one.ir.json"]:
            with self.subTest(path=raw):
                fixture = self.fixture()
                fixture.manifest["entries"][0]["ir"] = raw
                fixture.save()
                with self.assertRaises(ValueError):
                    fixture.load()
        for pointer in [{"bundle": "run", "manifest": "run/manifest.json", "extra": True},
                        {"bundle": "../source", "manifest": "../source/manifest.json"},
                        {"bundle": "run", "manifest": "run/../run/manifest.json"},
                        {"bundle": "run", "manifest": "run/missing.json"}]:
            with self.subTest(pointer=pointer):
                fixture = self.fixture()
                fixture.write(fixture.output / "latest.json", pointer)
                with self.assertRaises(ValueError):
                    fixture.load()
        for escaped in ["one.ir.json", "two.clvm.hex", "manifest.json"]:
            with self.subTest(symlink=escaped):
                fixture = self.fixture()
                path = fixture.bundle / escaped
                outside = fixture.output.parent / (escaped + ".outside")
                path.rename(outside)
                path.symlink_to(outside)
                with self.assertRaises(ValueError):
                    fixture.load()
        fixture = self.fixture()
        (fixture.bundle / "two.ir.json").unlink()
        (fixture.bundle / "two.ir.json").symlink_to("one.ir.json")
        with self.assertRaisesRegex(ValueError, "alias"):
            fixture.load()
        fixture = self.fixture()
        sibling = fixture.output / "other-bundle"
        sibling.mkdir()
        (fixture.bundle / "manifest.json").rename(sibling / "manifest.json")
        (fixture.bundle / "manifest.json").symlink_to(sibling / "manifest.json")
        with self.assertRaisesRegex(ValueError, "named bundle"):
            fixture.load()

    def test_snapshot_callback_checks_before_and_after_without_cached_substitution(self):
        fixture = self.fixture()
        (fixture.root / "lib.rs").write_text("changed source\n")
        with self.assertRaisesRegex(ValueError, "before"):
            fixture.load()
        fixture = self.fixture()
        calls = []

        def drift():
            calls.append(1)
            if len(calls) == 2:
                (fixture.root / "lib.rs").write_text("changed while loading\n")
            return fixture.snapshot()

        with self.assertRaisesRegex(ValueError, "during"):
            fixture.load(current_authoring=drift)
        self.assertEqual(len(calls), 2)
        with self.assertRaisesRegex(ValueError, "callback"):
            fixture.load(current_authoring=None)

    def test_cached_callbacks_cannot_hide_changed_source_bytes(self):
        fixture = self.fixture()
        (fixture.root / "lib.rs").write_text("changed before with cached callback\n")
        with self.assertRaisesRegex(ValueError, "fingerprint"):
            fixture.load(current_authoring=lambda: fixture.authoring)
        fixture = self.fixture()
        original = cargo_bundle._artifact

        def mutate(*args):
            result = original(*args)
            (fixture.root / "lib.rs").write_text("changed during with cached callback\n")
            return result

        with patch("cargo_bundle._artifact", side_effect=mutate), self.assertRaisesRegex(ValueError, "fingerprint"):
            fixture.load(current_authoring=lambda: fixture.authoring)
        for setting, value in [("MAX_SOURCE_FILE_BYTES", 8), ("MAX_TOTAL_SOURCE_BYTES", 32)]:
            fixture = self.fixture()
            with patch(f"cargo_bundle.{setting}", value), self.assertRaisesRegex(ValueError, "budget"):
                fixture.load()

    def test_optimization_is_bound_independently_of_cargo_only_flags(self):
        fixture = self.fixture()
        for entry in fixture.manifest["entries"]:
            entry["artifact"]["optimize_lir"] = False
        fixture.save()
        with self.assertRaisesRegex(ValueError, "artifact metadata"):
            fixture.load()
        self.assertFalse(fixture.load(optimize=False).manifest["entries"][0]["artifact"]["optimize_lir"])
        with self.assertRaisesRegex(ValueError, "optimization mode"):
            fixture.load(optimize=0)
        with self.assertRaisesRegex(ValueError, "list of strings"):
            fixture.load(cargo_options="--locked")
        with self.assertRaisesRegex(ValueError, "list of strings"):
            fixture.load(cargo_options=[None])

    def test_duplicate_json_keys_and_read_budgets_fail_closed(self):
        for relative in ["latest.json", "run/manifest.json", "run/two.ir.json"]:
            fixture = self.fixture()
            (fixture.output / relative).write_text('{"name":"first","name":"second"}')
            with self.subTest(file=relative), self.assertRaisesRegex(ValueError, "Duplicate"):
                fixture.load()
        fixture = self.fixture()
        with patch("cargo_bundle.MAX_JSON_BYTES", 8), self.assertRaisesRegex(ValueError, "budget"):
            fixture.load()
        with patch("cargo_bundle.MAX_TOTAL_BYTES", 32), self.assertRaisesRegex(ValueError, "budget"):
            fixture.load()


if __name__ == "__main__":
    unittest.main()
