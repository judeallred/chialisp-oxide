"""Constructed witness-reader controls, never compiler/native/VM executions."""
import copy
import importlib.util
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from registry_codegen_evidence import FrozenFiles, digest, encode, file_identity, identity, inspect_compiled_artifact, write_json
from registry_witness_evidence import (
    ALIASES, EXPORTS, GENERATION_SCOPE, GUARD, HELPER, HELPER_SHA256, LIMITS,
    MUTANTS, RECIPE, RESOLUTION_SCOPE, RUN, check_command_logs, check_generated, check_resolve,
    inspect_ir, mutant_expected, mutate,
)


def constructed_source(module, lock=b"constructed lock", object_id="17" * 32):
    """Synthetic protocol fixture. It is never represented as emitted Rust."""
    commitment = identity(module)["module_tree_hash"]
    source = (
        "// Raw lock SHA-256: " + digest(lock) + "\n// Envelope SHA-256: " + object_id
        + "\n// Artifact blob SHA-256: " + digest(module) + "\n// Artifact tree hash: " + commitment
        + "\n// Artifact bytes: " + str(len(module))
        + "\n// cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8 puzzles/sha256tree.clib lines 4-10\n"
        + "use chialisp_oxide::{Tree, apply, sha256_parts};\n"
        + "#[rustfmt::skip]\nconst ATOM_DOMAIN: [u8; 1] = [1];\n"
        + "#[rustfmt::skip]\nconst PAIR_DOMAIN: [u8; 1] = [2];\n"
        + "#[rustfmt::skip]\nconst EXPECTED_PROGRAM_TREE_HASH: [u8; 32] = ["
        + ", ".join(f"0x{byte:02x}" for byte in bytes.fromhex(commitment)) + "];\n"
        + "#[rustfmt::skip]\npub(super) fn run<'a>(program: &'a Tree<'a>, environment: &'a Tree<'a>) -> Tree<'static> {\n"
    ).encode() + RUN + b"}\n#[rustfmt::skip]\n" + HELPER
    return source


def constructed_generation(module, *, alias="multiply", source=None, lock=b"constructed lock", status="created"):
    source = source if source is not None else constructed_source(module, lock)
    stats = inspect_compiled_artifact(module)["stats"]
    return {"status": status, "output": {"status": status, "source_sha256": digest(source), "source_bytes": len(source)},
        "generation": {"schema_version": 1, "recipe": RECIPE, "lock_sha256": digest(lock), "selected_alias": alias,
            "object_id": "17" * 32, "artifact": identity(module), "source_sha256": digest(source), "source_bytes": len(source),
            "metrics": {"artifact_bytes": len(module), "expanded_nodes": stats["nodes"], "data_depth_root_one": stats["depth_root_one"],
                "generated_source_bytes": len(source), "source_helpers": 2, "commitment_bytes": 32,
                "embedded_program_bytes": 0, "generation_work_units": 2 * len(source)},
            "limits": dict(LIMITS), "scope": GENERATION_SCOPE}}


def admitted(value, module, source, alias="multiply", status="created"):
    return check_generated(value, source, alias, module, b"constructed lock", "17" * 32, status)


class WitnessEvidenceTests(unittest.TestCase):
    def test_three_artifact_sizes_use_one_fixed_small_recipe(self):
        modules = {"multiply": bytes.fromhex("ff12ff02ff0580")}
        modules.update({f"quote{size}": encode((b"\x01", b"\xa5" * size)) for size in (257, 8192)})
        self.assertEqual(digest(HELPER), HELPER_SHA256)
        self.assertEqual(len(EXPORTS), 6)
        for alias, module in modules.items():
            source = constructed_source(module)
            for status in ("created", "already_present", "checked"):
                value = constructed_generation(module, source=source, alias=alias, status=status)
                self.assertEqual(admitted(value, module, source, alias, status)["artifact"], identity(module))
                self.assertLess(len(source), 8192)
                self.assertNotIn(b"\xa5" * 257, source)

    def test_exact_record_protocol_identity_and_metrics_corruptions_fail(self):
        module = bytes.fromhex("ff12ff02ff0580")
        source = constructed_source(module)
        original = constructed_generation(module, source=source)
        for key, value in (("recipe", "chialisp-oxide-generated-tree/v1"), ("schema_version", 2),
                           ("selected_alias", "other"), ("object_id", "00" * 32), ("lock_sha256", "00" * 32),
                           ("source_bytes", len(source) - 1), ("scope", "compiler preservation")):
            corrupted = copy.deepcopy(original)
            corrupted["generation"][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                admitted(corrupted, module, source)
        for key, value in (("embedded_program_bytes", 1), ("commitment_bytes", 31), ("source_helpers", 1),
                           ("expanded_nodes", 1), ("generation_work_units", LIMITS["max_generation_work_units"] + 1),
                           ("artifact_bytes", True)):
            corrupted = copy.deepcopy(original)
            corrupted["generation"]["metrics"][key] = value
            with self.subTest(metric=key), self.assertRaises(ValueError):
                admitted(corrupted, module, source)
        corrupted = copy.deepcopy(original)
        corrupted["generation"]["limits"]["max_source_bytes"] += 1
        with self.assertRaises(ValueError):
            admitted(corrupted, module, source)
        for change in (lambda row: row.update(extra="unknown"), lambda row: row["output"].update(status="checked"),
                       lambda row: row["generation"]["artifact"].update(blob_sha256="00" * 32)):
            corrupted = copy.deepcopy(original)
            change(corrupted)
            with self.assertRaises(ValueError):
                admitted(corrupted, module, source)

    def test_source_recipe_rejects_extra_data_even_with_matching_source_digest(self):
        module = bytes.fromhex("ff12ff02ff0580")
        source = constructed_source(module)
        for changed in (source + b"static HIDDEN: [u8; 257] = [165; 257];\n",
                        source.replace(b"sha256_parts(&ATOM_DOMAIN", b"sha256_parts(&PAIR_DOMAIN", 1),
                        source.replace(b"// Artifact bytes: 7", b"// Artifact bytes: 8"),
                        source.replace(b"pub(super) fn run", b"pub fn run", 1)):
            value = constructed_generation(module, source=changed)
            with self.subTest(change=changed[-60:]), self.assertRaises(ValueError):
                admitted(value, module, changed)

    def test_all_local_source_mutants_are_distinct_and_not_original_generation(self):
        module = bytes.fromhex("ff12ff02ff0580")
        source = constructed_source(module)
        mutants = [mutate(source, name) for name in MUTANTS]
        self.assertEqual(len(MUTANTS), 6)
        self.assertEqual(len(set(mutants)), len(MUTANTS))
        for name, changed in zip(MUTANTS, mutants, strict=True):
            with self.subTest(name=name):
                self.assertNotEqual(changed, source)
                self.assertEqual(mutate(source, name), changed)
                with self.assertRaises(ValueError):
                    admitted(constructed_generation(module, source=changed), module, changed)
        self.assertNotIn(GUARD, mutants[0])
        self.assertIn(b"let _ = tree;", mutants[1])
        self.assertIn(b"const ATOM_DOMAIN: [u8; 1] = [2];", mutants[3])
        self.assertLess(mutants[-1].index(b"let output = apply"), mutants[-1].index(GUARD))

    def test_mutation_site_count_and_size_preflight(self):
        source = constructed_source(bytes.fromhex("ff12ff02ff0580"))
        for name in MUTANTS:
            for damaged in (b"", source + source, b"x" * 8193):
                with self.subTest(name=name, length=len(damaged)), self.assertRaises(ValueError):
                    mutate(damaged, name)
        with self.assertRaises(ValueError):
            mutate(source, "unknown")

    def test_finite_mutant_controls_preserve_order_discriminator(self):
        raised = {"status": "rejected", "code": "raise"}
        path = {"status": "rejected", "code": "path_into_atom"}
        for name in MUTANTS:
            actual = mutant_expected(name, discarded=False)
            discarded = mutant_expected(name, discarded=True)
            self.assertEqual(len(actual), 3)
            if name in ("guard_omitted", "constant_hash_helper"):
                self.assertEqual(actual[1], {"status": "accepted", "output_hex": "80"})
            elif name in ("altered_commitment", "altered_atom_domain", "swapped_pair_hash"):
                self.assertEqual(actual, [raised] * 3)
            else:
                self.assertEqual(actual[1:], [raised, path])
            self.assertEqual([row for row in actual if row["status"] == "rejected"],
                             [row for row in discarded if row["status"] == "rejected"])

    def test_ir_inventory_checks_both_atom_and_fixed_bytes_and_keeps_scope(self):
        program = {"ir_version": 4, "items": [{"op": "apply"}, {"op": "sha256"}, {"op": "sha256"},
                    {"op": "atom", "hex": "00" * 32}, {"op": "bytes", "hex": "01" * 32}]}
        checked = inspect_ir(program)
        self.assertEqual(checked["maximum_byte_constant_bytes"], 32)
        self.assertEqual(checked["apply_nodes"], 1)
        self.assertIn("no generic", checked["scope"])
        for index in (3, 4):
            changed = copy.deepcopy(program)
            changed["items"][index]["hex"] += "00"
            with self.assertRaises(ValueError):
                inspect_ir(changed)
        with self.assertRaises(ValueError):
            inspect_ir({"items": [{"op": "apply"}]})
        self.assertEqual(inspect_ir({"items": [{"op": "apply"}]}, baseline=False)["sha256_nodes"], 0)
        with self.assertRaises(ValueError):
            inspect_ir({"items": [{"op": "apply"}, {"op": "apply"}]}, baseline=False)

    def test_ir_inventory_preflights_collections_and_deep_shapes(self):
        deep = {"op": "apply"}
        for _ in range(256):
            deep = [deep]
        with self.assertRaises(ValueError):
            inspect_ir(deep, baseline=False)
        with self.assertRaises(ValueError):
            inspect_ir({str(index): 0 for index in range(129)}, baseline=False)
        with self.assertRaises(ValueError):
            inspect_ir([0] * 262145, baseline=False)

    def test_resolver_protocol_binds_exact_lock_all_pins_and_scope(self):
        lock, pins = b"constructed raw lock\n", [{"name": alias, "binding": None} for alias in ALIASES]
        value = {"status": "resolved", "lock_sha256": digest(lock), "dependencies": pins, "scope": RESOLUTION_SCOPE}
        self.assertIs(check_resolve(value, lock, pins), value)
        for changed in (value | {"dependencies": pins[:1]}, value | {"scope": "authenticated"},
                        value | {"lock_sha256": digest(lock.rstrip())}, value | {"schema_version": 1}):
            with self.assertRaises(ValueError):
                check_resolve(changed, lock, pins)

    def test_command_logs_detect_changed_raw_native_query_and_manifest_bytes(self):
        # Constructed log bytes/rows. No command is launched or substituted.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            stdout, stderr = root / "constructed-native.stdout", root / "constructed-native.stderr"
            stdout.write_bytes(b"constructed result A\n")
            stderr.write_bytes(b"")
            commands = [{"label": "constructed-native", "status": "completed", "exit_code": 0,
                         "stdout": file_identity(stdout), "stderr": file_identity(stderr)}]
            write_json(root / "commands.json", commands)
            original = check_command_logs(root, commands)
            stdout.write_bytes(b"constructed result B\n")
            with self.assertRaises(ValueError):
                check_command_logs(root, commands)
            stdout.write_bytes(b"constructed result A\n")
            self.assertEqual(check_command_logs(root, commands), original)
            write_json(root / "commands.json", commands + commands)
            with self.assertRaises(ValueError):
                check_command_logs(root, commands)
            write_json(root / "commands.json", commands)
            for changed in (commands + commands, [{**commands[0], "status": "starting"}],
                            [{**commands[0], "exit_code": None}]):
                with self.assertRaises(ValueError):
                    check_command_logs(root, changed)

    def test_command_log_length_preflight_does_not_hash_over_budget_declaration(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            commands = [{"label": "constructed-log", "status": "completed", "exit_code": 0,
                         "stdout": {"path": str(root / "constructed-log.stdout"), "bytes": 16777217, "sha256": "00" * 32}}]
            with patch("registry_witness_evidence.file_identity") as hasher, self.assertRaises(ValueError):
                check_command_logs(root, commands)
            hasher.assert_not_called()

    def test_quiet_source_closure_retains_new_gate_and_transitive_json_helper(self):
        script = Path(__file__).with_name("check-registry-witness.py")
        spec = importlib.util.spec_from_file_location("constructed_registry_witness_gate", script)
        gate = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(gate)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            paths = ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml",
                "scripts/check-registry-codegen.py", "scripts/registry_codegen_evidence.py",
                "scripts/cargo_bundle.py", "scripts/cargo_source_receipt.py", "scripts/cargo_authoring.py",
                "scripts/source_walk.py", "scripts/vm_failures.py", "scripts/check-registry-witness.py",
                "scripts/registry_witness_evidence.py", "scripts/test_registry_witness_evidence.py",
                "puzzles/sha256tree/puzzle.rs", "puzzles/sha256tree/puzzle.json",
                "puzzles/inner-puzzle/puzzle.rs", "docs/dynamic-apply.md")
            for name in paths:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"constructed source A")
            tool = root / "never-executed-tool"
            tool.write_bytes(b"constructed nonexecutable identity")
            run = object.__new__(gate.WitnessRun)
            run.args, run.sdk = SimpleNamespace(registry_cli=tool, cli=tool, driver=tool, rustup=tool), False
            run.snapshot = lambda: {"scope": "constructed authoring inventory only"}
            run.freeze_host = lambda: None
            run.helper_provenance = lambda: {"retained_inputs": []}
            run.host_inputs = FrozenFiles()
            with patch.object(gate, "ROOT", root), patch.object(gate, "TEMPLATE", root / "scripts/registry_witness_fixture"), \
                 patch.object(gate.process_gate, "ROOT", root), patch.object(gate.process_gate, "FIXTURE", root / "fixture"):
                before = run.quiet_inputs()
                for relative in ("scripts/cargo_authoring.py", "scripts/registry_witness_evidence.py"):
                    path = root / relative
                    path.write_bytes(b"constructed source B")
                    self.assertNotEqual(run.quiet_inputs(), before)
                    path.write_bytes(b"constructed source A")
                    self.assertEqual(run.quiet_inputs(), before)
                (root / "scripts/cargo_authoring.py").unlink()
                with self.assertRaises(FileNotFoundError):
                    run.quiet_inputs()


if __name__ == "__main__":
    unittest.main()
