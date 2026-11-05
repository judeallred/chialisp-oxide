"""Historical guarded checkpoint controls, never current-source validation.

Actual C99 bytes are retained independently of moving corpus/validation.json.
Constructed in-memory old66 review bindings exercise pure projectors only; they
do not promote the pending profile or fabricate a signed M1 checkpoint. Actual
current ordering/control checks live in test_runtime_composition.py.
"""
import copy
import json
import subprocess
from pathlib import Path
import unittest
import tempfile
from unittest.mock import patch

import runtime_guarded_projection_migration as migration
import runtime_cost_migration as cost


HISTORICAL_CHECKPOINT = cost.ROOT / "corpus/checkpoints/guarded-projection-validation.json"
HISTORICAL_RAW = "c99faadc4aac2214a703e3a80c917d28e3f90c594ef31b3d0a3973daae1febe8"


def historical_payload():
    """Bounded regular raw bytes; no source/native/control validator on history."""
    payload = migration.report_payload(HISTORICAL_CHECKPOINT)
    cost.require(cost.sha(payload) == HISTORICAL_RAW, "Changed immutable historical guarded checkpoint bytes")
    return payload


def historical_report():
    return json.loads(historical_payload(), object_pairs_hook=cost.unique_object)


def constructed_profile(report, profile=None):
    """Pure historical fixture only; signed/null on-disk authority stays untouched."""
    value = copy.deepcopy(migration.load() if profile is None else profile)
    value["status"] = migration.FINAL
    payload = historical_payload()
    if not cost.same(json.loads(payload), report): payload = cost.canonical(report)
    # Preserve the actual reviewed delta basis while rebinding outer identity;
    # malformed metadata must reach the real source/schema/comparator checks.
    delta = migration.differences(migration.predecessor(), historical_report())
    value["reviewed_after"] = {"report_sha256": cost.sha(payload),
        "canonical_report_sha256": cost.sha(cost.canonical(report)),
        "p2_sha256": cost.sha(cost.canonical(report["puzzles"][-1])),
        "protocol_sha256": cost.sha(cost.canonical(report["puzzles"][-1]["protocol_validation"])),
        "raw_ir_sha256": migration.source_cost.load()["reviewed_after"]["raw_ir_sha256"],
        "metrics_sha256": cost.sha(migration.read(migration.METRICS)),
        "controls_sha256": cost.sha(migration.read(migration.CONTROLS)),
        "bundles": {"on": {"manifest_sha256": "1" * 64, "optimize_lir": True},
                    "off": {"manifest_sha256": "2" * 64, "optimize_lir": False}},
        "delta": {"leaf_count": len(delta), "inventory_sha256": cost.sha((json.dumps(delta, indent=2) + "\n").encode())}}
    return value


def bind_review(profile):
    return patch.object(migration, "load", return_value=profile)


class GuardedProfile(unittest.TestCase):
    def test_report_payload_has_pre_read_regular_file_and_size_bounds(self):
        with tempfile.TemporaryDirectory(prefix="guarded-report-IO-") as folder:
            path = Path(folder) / "report.json"
            path.write_bytes(b"{}\n")
            self.assertEqual(migration.report_payload(path), b"{}\n")
            path.write_bytes(b"")
            with self.assertRaisesRegex(ValueError, "byte bound"): migration.report_payload(path)
            with path.open("wb") as stream:
                stream.seek(16 * 1024 * 1024)
                stream.write(b"x")
            with patch.object(migration.os, "fdopen", side_effect=AssertionError("read before size preflight")), \
                    self.assertRaisesRegex(ValueError, "byte bound"):
                migration.report_payload(path)
            with self.assertRaisesRegex(ValueError, "regular-file"): migration.report_payload(Path(folder))
            if hasattr(migration.os, "mkfifo"):
                fifo = Path(folder) / "report.pipe"
                migration.os.mkfifo(fifo)
                original = migration.os.open
                def nonblocking(filename, flags):
                    self.assertTrue(flags & migration.os.O_NONBLOCK, "FIFO must never enter a blocking open")
                    return original(filename, flags)
                with patch.object(migration.os, "open", side_effect=nonblocking), \
                        patch.object(migration.os, "fdopen", side_effect=AssertionError("read nonregular input")), \
                        self.assertRaisesRegex(ValueError, "regular-file"):
                    migration.report_payload(fifo)

    def test_historical_raw_checkpoint_is_pinned_before_parsing(self):
        payload = historical_payload()
        self.assertEqual(cost.sha(payload), HISTORICAL_RAW)
        self.assertEqual(historical_report()["optimization_pipeline"], migration.NEW_PIPELINE)
        for bad in (payload + b"\n", b"{}\n"):
            self.assertNotEqual(cost.sha(bad), HISTORICAL_RAW)
            with self.subTest(raw_size=len(bad)), \
                    patch.object(migration, "report_payload", return_value=bad), \
                    self.assertRaisesRegex(ValueError, "immutable historical guarded"):
                historical_report()

    def test_pending_profile_is_readable_but_public_gate_rejects_it(self):
        value = copy.deepcopy(migration.load())
        value.update(status=migration.PENDING, reviewed_after=None)
        payload = (json.dumps(value, indent=2) + "\n").encode()
        with patch.object(migration, "PIN", cost.sha(payload)):
            self.assertIsNone(migration.load(payload)["reviewed_after"])
        with patch.object(migration, "load", return_value=value), self.assertRaisesRegex(ValueError, "pending"):
            migration.reviewed_after(historical_report())

    def test_schema_boolean_and_unknown_keys_fail_with_rebound_fixture_pin(self):
        for change in (lambda v: v.update(schema_version=True), lambda v: v.update(unreviewed=True)):
            value = migration.load()
            change(value)
            payload = (json.dumps(value, indent=2) + "\n").encode()
            with self.subTest(value=value["schema_version"]), patch.object(migration, "PIN", cost.sha(payload)), self.assertRaises(ValueError):
                migration.load(payload)

    def test_exact_checkpoint_is_loaded_without_current_source_validation(self):
        with patch.object(cost, "validate_current", side_effect=AssertionError("current on history")):
            previous = migration.predecessor()
        self.assertEqual(cost.sha(cost.canonical(previous)), migration.BEFORE_CANONICAL)

    def test_old_raw_metric_and_control_bytes_are_pinned_before_use(self):
        original = migration.read
        for target in (migration.source_cost.METRICS_PATH, migration.source_cost.CONTROLS_PATH):
            migration.load()
            def changed(path, **options):
                payload = original(path, **options)
                return payload + b"\n" if path == target else payload
            with self.subTest(path=target), patch.object(migration, "read", side_effect=changed), self.assertRaisesRegex(ValueError, "immutable source-only"):
                migration.load()


class RecordedCompilerDelta(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.before = migration.predecessor()
        cls.current = historical_report()

    def test_actual_captured_delta_has_384_closed_changes(self):
        result = migration.compare_compiler_delta(self.before, self.current)
        self.assertEqual(result, {"leaf_count": 384,
            "inventory_sha256": "f297e6623d6c5d063d8ef25682fe23f852468c7f697e08d9355a4fac7a23dfe7"})

    def test_password_ledger_derived_identities_bind_both_actual_sides(self):
        migration.compare_compiler_delta(self.before, self.current)
        for side in ("before", "current"):
            for field in ("module_tree_hash", "instance_tree_hash", "coin_puzzle_hash", "reveal_tree_hash", "reveal_hex", "wrong_commitment_reveal"):
                before, current = copy.deepcopy(self.before), copy.deepcopy(self.current)
                report = before if side == "before" else current
                instances = next(p for p in report["puzzles"] if p["slug"] == "password-coin")["curried_instances"]
                value = cost.rust_on(instances["ledger"])
                if field in ("module_tree_hash", "instance_tree_hash"):
                    self.assertNotEqual(value[field], "0" * 64)
                    value[field] = "0" * 64
                elif field == "wrong_commitment_reveal":
                    # Substitute a different real retained artifact with a
                    # self-consistent hash; preserve the actual xyz argument.
                    artifact = cost.rust_on([row["artifact"] for row in instances["varying_arguments"][0]["runs"]])
                    self.assertNotEqual(value["attempts"][-1]["reveal_hex"], artifact["clvm_hex"])
                    value["attempts"][-1].update(reveal_hex=artifact["clvm_hex"], reveal_tree_hash=artifact["tree_hash"])
                else:
                    replacement = "80" if field == "reveal_hex" else "0" * 64
                    self.assertNotEqual(value["attempts"][0][field], replacement)
                    value["attempts"][0][field] = replacement
                with self.subTest(side=side, field=field), self.assertRaises(ValueError):
                    migration.compare_compiler_delta(before, current)

    def test_protocol_module_and_own_curried_hash_bind_both_actual_sides(self):
        migration.compare_compiler_delta(self.before, self.current)
        for side in ("before", "current"):
            for field in ("module_tree_hash", "curried_p2_hash"):
                before, current = copy.deepcopy(self.before), copy.deepcopy(self.current)
                report = before if side == "before" else current
                value = report["puzzles"][-1]["protocol_validation"]["observations"][1]
                self.assertNotEqual(value[field], "0" * 64)
                value[field] = "0" * 64
                with self.subTest(side=side, field=field), self.assertRaises(ValueError):
                    migration.compare_compiler_delta(before, current)

    def test_non_noop_semantic_metadata_and_coverage_changes_fail(self):
        for name in ("source", "native", "solution", "order", "missing", "unknown", "off", "rue", "outcome", "failure"):
            bad = copy.deepcopy(self.current)
            puzzle = bad["puzzles"][0]
            if name == "source": puzzle["source"] += "\n"
            elif name == "native": puzzle["cases"][0]["native"] = {"status": "rejected"}
            elif name == "solution": puzzle["cases"][0]["solution"] = "(99)"
            elif name == "order": bad["puzzles"][0:2] = bad["puzzles"][1::-1]
            elif name == "missing": bad["puzzles"].pop()
            elif name == "unknown": puzzle["new_metadata"] = True
            elif name == "off": puzzle["artifacts"][0]["bytes"] += 1
            elif name == "rue": puzzle["cases"][0]["runs"][3]["execution"]["cost"] += 1
            elif name == "outcome": puzzle["cases"][0]["runs"][1]["execution"]["outcome"] = {"status": "rejected"}
            else: puzzle["cases"][1]["runs"][1]["execution"]["failure"] = {"kind": "resource", "code": "cost_exceeded"}
            with self.subTest(name=name), self.assertRaises((ValueError, KeyError, TypeError)):
                migration.compare_compiler_delta(self.before, bad)

    def test_historical_pure_projection_chain_never_uses_current_validators(self):
        profile = constructed_profile(self.current)
        # Pure historical observations only. The old66 fixture does not grant
        # public/current admission, change its null signed fields or run Cargo.
        with bind_review(profile), \
                patch.object(cost, "validate_current", side_effect=AssertionError("current on history")), \
                patch.object(cost, "current_native_sources", side_effect=AssertionError("current native on history")), \
                patch.object(cost, "check_recorded", side_effect=AssertionError("current authoring on history")), \
                patch.object(migration, "check_sidecars", side_effect=AssertionError("current guarded sidecars on history")), \
                patch.object(migration.source_cost, "validate_controls", side_effect=AssertionError("current controls on history")), \
                patch.object(subprocess, "run", side_effect=AssertionError("no execution")):
            source_only = migration.project_validated(self.current)
            anchor = migration.source_cost.project_validated(source_only)
            old21 = migration.append.project_validated(anchor)
            from runtime_inline_migration import project_validated as inline_history
            old = cost.project_validated(inline_history(old21))
        self.assertEqual(cost.sha(cost.canonical(source_only)), migration.BEFORE_CANONICAL)
        self.assertEqual(cost.sha(cost.canonical(anchor)), migration.source_cost.BEFORE_CANONICAL)
        self.assertEqual(cost.sha(cost.canonical(old)), cost.load()["baseline"]["canonical_report_sha256"])

    def test_historical_projection_never_reads_moving_report_or_current_controls(self):
        profile = constructed_profile(self.current)
        pending_payload = migration.read(migration.PATH)
        original_read = migration.read
        forbidden = {"corpus/validation.json", migration.METRICS, migration.CONTROLS}
        def historical_read(path, **options):
            self.assertNotIn(path, forbidden, "Moving current report/control input on historical path")
            return original_read(path, **options)
        with bind_review(profile), \
                patch.object(migration, "read", side_effect=historical_read), \
                patch.object(cost, "validate_current", side_effect=AssertionError("current on history")), \
                patch.object(migration.source_cost, "validate_controls", side_effect=AssertionError("current controls on history")), \
                patch.object(migration.source_cost, "control_source_window", side_effect=AssertionError("current source window on history")), \
                patch.object(subprocess, "run", side_effect=AssertionError("no execution")):
            previous = migration.project_validated(self.current)
        self.assertEqual(cost.sha(cost.canonical(previous)), migration.BEFORE_CANONICAL)
        self.assertEqual(migration.read(migration.PATH), pending_payload)
        self.assertEqual(migration.load()["status"], migration.PENDING)

    def test_non_noop_protocol_receipt_solution_amount_and_genesis_controls(self):
        for name in ("rollback", "amount", "solution", "reveal", "preflight", "unknown"):
            bad = copy.deepcopy(self.current)
            group = bad["puzzles"][-1]["protocol_validation"]["observations"][1]
            if name == "rollback": group["negatives"][0]["full_simulator_equality"] = False
            elif name == "amount": group["lifecycle"]["p2"]["amount"] += 1
            elif name == "solution": group["lifecycle"]["joint"]["spends"][1]["solution_hex"] = "80"
            elif name == "reveal": group["lifecycle"]["joint"]["spends"][1]["reveal_hex"] = "80"
            elif name == "preflight": group["negatives"][0]["stage"] = "signer_preflight"
            else: group["lifecycle"]["new_claim"] = True
            with self.subTest(name=name), self.assertRaises((ValueError, KeyError, TypeError, IndexError)):
                migration.compare_compiler_delta(self.before, bad)


class MetricDelta(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.before = migration.predecessor()
        cls.current = historical_report()
        cls.old = json.loads(migration.read(migration.source_cost.METRICS_PATH))
        cls.new = json.loads(migration.read(migration.METRICS))

    def test_actual_metrics_have_closed_joint_and_genesis_delta(self):
        migration.compare_metrics(self.before, self.current, self.old, self.new)

    def test_rebound_genesis_joint_lineage_and_nonrust_controls(self):
        for name in ("genesis_numeric", "genesis_hex", "genesis_missing_hash", "joint_condition", "joint_scope", "joint_growth", "lineage", "rue"):
            migration.compare_metrics(self.before, self.current, self.old, self.new)
            bad = copy.deepcopy(self.new)
            if name == "genesis_numeric": bad["observations"][1]["genesis"]["execution_cost"] += 1
            elif name == "genesis_hex": bad["observations"][1]["genesis"]["generator_uncompressed_hex"] += "80"
            elif name == "genesis_missing_hash": bad["observations"][1]["genesis"]["generator_backrefs_hex"] = self.old["observations"][1]["genesis"]["generator_backrefs_hex"]
            elif name == "joint_condition": bad["observations"][1]["joint"]["condition_cost"] += 1
            elif name == "joint_scope": bad["observations"][1]["joint"]["signature_scope"] += " verified"
            elif name == "joint_growth": bad["observations"][1]["joint"]["execution_cost"] = self.old["observations"][1]["joint"]["execution_cost"] + 1
            elif name == "lineage": bad["observations"][1]["lineage"]["execution_cost"] += 1
            else: bad["observations"][3]["joint"]["execution_cost"] += 1
            with self.subTest(name=name), self.assertRaises((ValueError, KeyError, TypeError)):
                migration.compare_metrics(self.before, self.current, self.old, bad)


if __name__ == "__main__":
    unittest.main()
