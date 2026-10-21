"""Constructed metadata controls only; actual-source execution is separate."""
import copy
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

import acyclic_source_bridge as B


def copy_experiment(source_root, destination_root):
    """Copy declared inputs; IDE/Cargo outputs are outside the test fixture."""
    for relative in B.PRIVATE:
        source = source_root / B.EXPERIMENT / relative
        destination = destination_root / B.EXPERIMENT / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)


def witness(n):
    return {"arguments": [{"kind": "int", "value": str(n)}]}


def verdict(wanted, n=0):
    result = {"status": wanted, "nonvacuity": witness(0)}
    if wanted == "refuted":
        result["counterexample"] = witness(n)
    return {"model": "acyclic_scalar_calls/v1", "scope": B.MODEL_SCOPE, "verification": result}


def execution(module, policy, n):
    value = B.expected(module, policy, n)
    if value["status"] == "accepted":
        return {"outcome": {"status": "accepted", "output_hex": B.encoded(value["value"])}, "cost": 1, "error": None}
    return {"outcome": {"status": "rejected"}, "cost": None, "error": "diagnostic only",
            "failure": {"kind": "program", "code": "raise"}}


class SourceTests(unittest.TestCase):
    def test_actual_producer_requests_artifact_only_in_both_modes(self):
        # Run producer orchestration only: every subprocess/native identity and
        # export load is constructed, and stop before IR/model/VM processing.
        class StopAfterSecondExport(Exception):
            pass
        commands = []
        def run(argv, **kwargs):
            commands.append(list(argv))
            if len(commands) == 3:
                raise StopAfterSecondExport
            return subprocess.CompletedProcess(argv, 0, "constructed tool log\n", "")
        def exported(directory, **kwargs):
            return types.SimpleNamespace(manifest_path=directory / "manifest.json", ir_sha256="a" * 64)
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory).resolve()
            tools, target, z3 = base / "tools", base / "target", base / "tools/z3"
            with patch.object(B, "reviewed_contract", return_value=({}, {})), \
                    patch.object(B, "source_snapshot", return_value={"experiment": {}}), \
                    patch.object(B, "identities", return_value={"toolchain": "constructed"}), \
                    patch.object(B, "file_sha", return_value="a" * 64), \
                    patch.object(B.shutil, "which", return_value=str(z3)), \
                    patch.object(B, "check_export", side_effect=exported), \
                    patch.object(B, "run", side_effect=run), \
                    self.assertRaises(StopAfterSecondExport):
                B.produce(B.ROOT, base / "output", tools, target, z3)
            self.assertEqual(len(commands), 3)
            self.assertNotIn("--artifact-only", commands[0])  # Native Cargo remains ordinary.
            for index, mode in [(1, "off"), (2, "on")]:
                expected = [tools / "cargo-chialisp-oxide", "build", "--artifact-only", *B.OPTIONS,
                            "--output-dir", base / "output" / ("fresh-" + mode)]
                if mode == "off":
                    expected.append("--no-optimize")
                self.assertEqual(commands[index], expected)

    def test_exact_current_contract_and_actual_file_corruptions(self):
        B.reviewed_contract(B.ROOT)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            copy_experiment(B.ROOT, root)
            B.reviewed_contract(root)
            for relative in B.PRIVATE:
                path = root / B.EXPERIMENT / relative
                original = path.read_bytes()
                try:
                    path.write_bytes(original + b"\n")
                    with self.subTest(path=relative), self.assertRaises(ValueError):
                        B.reviewed_contract(root)
                finally:
                    path.write_bytes(original)

    def test_snapshot_rehashes_actual_private_file_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            copy_experiment(B.ROOT, root)
            before = B.private_snapshot(root)
            source = root / B.EXPERIMENT / "src/base.rs"
            source.write_text(source.read_text().replace("value < 100", "value < 101"))
            self.assertNotEqual(B.private_snapshot(root), before)

    def test_fixture_copy_uses_only_declared_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            source_root = Path(directory) / "source"
            destination_root = Path(directory) / "destination"
            copy_experiment(B.ROOT, source_root)
            ignored = source_root / B.EXPERIMENT / "target/debug/ignored-build-output"
            ignored.parent.mkdir(parents=True)
            ignored.symlink_to(Path(directory) / "missing-output")
            copy_experiment(source_root, destination_root)
            copied = {path.relative_to(destination_root / B.EXPERIMENT).as_posix()
                      for path in (destination_root / B.EXPERIMENT).rglob("*") if path.is_file()}
            self.assertEqual(copied, set(B.PRIVATE))
            self.assertFalse((destination_root / B.EXPERIMENT / "target").exists())

    def test_native_exact_cardinality_types_and_independent_ranges(self):
        rows = [B.expected("base", "guarded", n) for n in range(256)]
        B.checked_native(rows, "base", "guarded")
        for mutate in (lambda v: v.pop(), lambda v: v.append(v[0]), lambda v: v.reverse(),
                       lambda v: v[0].update(input=False), lambda v: v[0].update(value=True),
                       lambda v: v[100].update(status="accepted", value=101)):
            bad = copy.deepcopy(rows)
            mutate(bad)
            with self.assertRaises(ValueError):
                B.checked_native(bad, "base", "guarded")

    def test_vm_missing_resource_and_wrong_failure_never_become_rejection(self):
        rows = [execution("base", "guarded", n) for n in (0, 100)]
        B.checked_vm(rows, [0, 100], "base", "guarded")
        for mutate in (lambda v: v.pop(), lambda v: v[0]["outcome"].update(output_hex="02"),
                       lambda v: v[1].update(failure={"kind": "resource", "code": "cost_exceeded"}),
                       lambda v: v[1].update(failure={"kind": "program", "code": "invalid_op_arg"}),
                       lambda v: v[1].pop("failure")):
            bad = copy.deepcopy(rows)
            mutate(bad)
            with self.assertRaises(ValueError):
                B.checked_vm(bad, [0, 100], "base", "guarded")

    def test_vacuity_malformed_verdict_and_domain_witness_controls(self):
        B.checked_verdict(verdict("proven"), "proven")
        B.checked_verdict(verdict("refuted", 100), "refuted")
        for mutate in (lambda v: v.update(model="scalar"), lambda v: v["verification"].update(nonvacuity=None),
                       lambda v: v["verification"].update(status="vacuous"),
                       lambda v: v["verification"].update(counterexample=witness(256)),
                       lambda v: v["verification"]["counterexample"]["arguments"][0].update(value=100)):
            bad = verdict("refuted", 100)
            mutate(bad)
            with self.assertRaises(ValueError):
                B.checked_verdict(bad, "refuted")
        for bad in (witness(-1), witness(256), {"arguments": []}, {"arguments": [{"kind": "int", "value": "00"}]},
                    {"arguments": [{"kind": "bool", "value": True}]}):
            with self.assertRaises(ValueError):
                B.witness_value(bad)

    def test_source_mutants_have_distinguishing_finite_witnesses(self):
        for module, policy, n in [("guard_mutant", "guarded", 100), ("output_mutant", "guarded", 0),
                                  ("strict_mutant", "unused_argument", 100), ("strict_mutant", "unused_initializer", 100)]:
            self.assertNotEqual(B.expected("base", policy, n), B.expected(module, policy, n))
        self.assertEqual(B.encoded(0), "80")
        self.assertEqual(B.encoded(127), "7f")
        self.assertEqual(B.encoded(128), "820080")
        self.assertEqual(B.encoded(255), "8200ff")

    def test_explicit_opt_in_rejects_before_build_or_output_creation(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "must-not-be-created"
            result = subprocess.run([sys.executable, str(B.ROOT / "scripts/check-acyclic-source.py"),
                                     "--execute", "--output", str(output), "--target-dir", str(Path(directory) / "target")],
                                    capture_output=True, text=True, timeout=10)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("--acyclic-scalar is required", result.stderr)
            self.assertFalse(output.exists())

    def test_independent_specs_do_not_assume_the_source_body_success_domain(self):
        _, specs = B.reviewed_contract(B.ROOT)
        for policy in B.POLICIES:
            for kind in ("acceptance", "output"):
                self.assertEqual(set(specs[policy][kind]), {"obligation"})
            predicate = specs[policy]["acceptance"]["obligation"]["predicate"]
            self.assertEqual(predicate["relation"], "lt")
            self.assertEqual(predicate["right"], {"term": "int", "value": "100"})

    def test_driver_short_version_is_checked_against_full_independent_compiler(self):
        full = "rustc reviewed-nightly\nbinary: rustc\nhost: constructed-host\ncommit-hash: pinned\n"
        with tempfile.TemporaryDirectory() as directory:
            tools = Path(directory)
            for name in (*B.TOOLS, "z3"):
                (tools / name).write_bytes(b"constructed executable identity only")
            values = [full, full.splitlines()[0] + "\n", "Z3 version constructed\n"]
            with patch.object(B, "run", side_effect=[subprocess.CompletedProcess([], 0, value, "") for value in values]):
                record = B.identities(B.ROOT, tools, tools / "z3", {})
            self.assertEqual(record["compiler"], full)
            with patch.object(B, "run", side_effect=[subprocess.CompletedProcess([], 0, full, ""),
                                                     subprocess.CompletedProcess([], 0, "rustc different", "")]):
                with self.assertRaises(ValueError):
                    B.identities(B.ROOT, tools, tools / "z3", {})

    def test_nonvacuity_is_grounded_in_native_and_both_retained_vm_modes(self):
        exported = {"native": [B.expected("base", "guarded", n) for n in range(256)],
                    "builds": [{"optimize": mode, "canonical": [execution("base", "guarded", n) for n in range(256)]}
                               for mode in (False, True)]}
        verification = verdict("proven")["verification"]
        self.assertEqual(B.ground_nonvacuity(verification, exported), 0)
        bad = copy.deepcopy(verification)
        bad["nonvacuity"] = witness(100)
        with self.assertRaises(ValueError):
            B.ground_nonvacuity(bad, exported)
        for mutate in (lambda v: v["native"][0].update(status="rejected"),
                       lambda v: v["builds"].pop(),
                       lambda v: v["builds"][0]["canonical"][0]["outcome"].update(output_hex="02"),
                       lambda v: v["builds"][1].update(optimize=1)):
            candidate = copy.deepcopy(exported)
            mutate(candidate)
            with self.assertRaises(ValueError):
                B.ground_nonvacuity(verification, candidate)

    def test_retained_host_and_past_source_paths_have_independent_consistency(self):
        identity = {"source_root": "/past/checkout", "compiler": "rustc pinned\nhost: constructed-host\n", "toolchain": "pinned",
                    "tools": [{"name": "chialisp-oxide-driver", "sha256": "a" * 64}]}
        manifest = {"analysis_target": "constructed-host", "cargo_options": B.OPTIONS, "compiler": identity["compiler"],
                    "driver_sha256": "a" * 64, "entries": [], "manifest_path": "/past/checkout/experiments/acyclic-scalar/Cargo.toml",
                    "package": B.PACKAGE, "package_id": "constructed captured metadata", "rue_revision": B.RUE,
                    "schema_version": 1, "status": "wip_prototype_not_ready_for_use", "toolchain": "pinned",
                    "workspace_root": "/past/checkout/experiments/acyclic-scalar"}
        B.checked_manifest(manifest, identity)  # Past location need not exist or equal this checkout.
        for mutate in (lambda v: v.update(analysis_target="other-host"),
                       lambda v: v.update(manifest_path="/other/Cargo.toml"),
                       lambda v: v.update(workspace_root="/other"),
                       lambda v: v.update(schema_version=True),
                       lambda v: v.update(extra="unknown")):
            bad = copy.deepcopy(manifest)
            mutate(bad)
            with self.assertRaises(ValueError):
                B.checked_manifest(bad, identity)
        for compiler in ("rustc pinned\n", "rustc pinned\nhost: one\nhost: two\n", "rustc pinned\nhost: invalid host\n"):
            with self.assertRaises(ValueError):
                B.compiler_host(compiler)
        for path in ("relative", "/past/../other", "/past//other"):
            with self.assertRaises(ValueError):
                B.stored_absolute(path)

    def test_wire_nonvacuity_distinguishes_boolean_from_numeric_mutants(self):
        self.assertEqual({"input": False}, {"input": 0})
        self.assertTrue(B.distinct_records({"input": False}, {"input": 0}))
        self.assertTrue(B.distinct_records({"optimize": 1}, {"optimize": True}))
        self.assertFalse(B.distinct_records({"a": 1, "b": False}, {"b": False, "a": 1}))

    def test_recorded_reader_rejects_before_any_process_on_bad_top_level(self):
        with patch.object(B.subprocess, "run", side_effect=AssertionError("must not spawn")):
            with self.assertRaises(ValueError):
                B.validate_recorded({}, root=B.ROOT, output=B.ROOT)


if __name__ == "__main__":
    unittest.main()
