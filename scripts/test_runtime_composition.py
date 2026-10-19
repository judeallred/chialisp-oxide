"""Historical227 composition controls, never current23 freshness positives.

Use immutable raw227 and its recorded authoring/embedded SDK text for pure
normalization/delta/projector checks. In-memory schema bindings grant no
execution/admission authority. Current23 source/order/sidecar positives belong
to the outer append suite only after an actual23 capture exists.
"""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

import runtime_composition_migration as migration
import runtime_cost_migration as cost


CHECKPOINT = migration.ROOT / "corpus/checkpoints/runtime-composition-validation.json"
RAW227 = "227ce91137f786919ce47cf0b0e09038efa84ee2f9c0df6a4e3fc0894084dc64"


def historical_payload():
    payload = migration.report_payload(CHECKPOINT)
    cost.require(cost.sha(payload) == RAW227, "Changed immutable historical227 bytes")
    return payload


def historical_report():
    return json.loads(historical_payload(), object_pairs_hook=cost.unique_object)


def historical_controls_payload():
    """Fixed historical bytes only, never a current-source/control validator."""
    payload = migration.read(migration.CONTROLS)
    cost.require(cost.sha(payload) == migration.load()["reviewed_after"]["controls_sha256"],
        "Changed immutable historical227 control bytes")
    return payload


def profile_bytes(value):
    return (json.dumps(value, indent=2) + "\n").encode()


def constructed_metadata_fixture(before):
    value = copy.deepcopy(before)
    value["optimization_pipeline"] = copy.deepcopy(migration.guarded.NEW_PIPELINE)
    recorded = historical_report()
    value["authoring"] = copy.deepcopy(recorded["authoring"])
    copies = {row["source"] for puzzle in recorded["puzzles"]
              for row in puzzle["source_dependencies"] if row["path"] == migration.SDK}
    if len(copies) != 1:
        raise AssertionError("Expected one exact recorded SDK body, not current SDK input")
    text = copies.pop()
    if cost.sha(text.encode()) != migration.NEW_SDK:
        raise AssertionError("Changed recorded SDK body/hash")
    for puzzle in value["puzzles"]:
        for row in puzzle["source_dependencies"]:
            if row["path"] == migration.SDK:
                row.update(sha256=migration.NEW_SDK, source=text)
    return value


def constructed_review_binding(report):
    """Test-only review status over actual retained bytes; no on-disk promotion."""
    value = copy.deepcopy(migration.load())
    value["status"] = migration.FINAL
    value["reviewed_after"] = {
        "report_sha256": "227ce91137f786919ce47cf0b0e09038efa84ee2f9c0df6a4e3fc0894084dc64",
        "canonical_report_sha256": cost.sha(cost.canonical(report)),
        "p2_sha256": cost.sha(cost.canonical(report["puzzles"][-1])),
        "protocol_sha256": cost.sha(cost.canonical(report["puzzles"][-1]["protocol_validation"])),
        "raw_ir_sha256": migration.source.load()["reviewed_after"]["raw_ir_sha256"],
        "metrics_sha256": "a252d5929d35822735d34cc3b58ab63c8a62bc011529dbb25b7827bba9a5bbbd",
        "controls_sha256": "e0f9037680232411430df6ea633c6dced764df40ffa158e06b90e75898a39f99",
        "bundles": {
            "on": {"manifest_sha256": "7ea8e6aafe99172d43349388ba9d7ab58be7b24e6a174ee12fb528065412c2dd", "optimize_lir": True},
            "off": {"manifest_sha256": "bf7615388cf00480e6e81882c1adf1b1c8f17e2c2d549ed817be419e71d43c6d", "optimize_lir": False}},
        "delta": {"leaf_count": 460, "inventory_sha256": "2192a8a07098dbb3cbd107f0ae12dde49b5ef3d621ee57a3f6b3e3cd0ad8901f"}}
    return value


class CompositionProfile(unittest.TestCase):
    def test_real_pending_profile_and_old_pending_are_unchanged_and_reject(self):
        value = migration.load()
        self.assertEqual(migration.guarded.load()["status"], migration.guarded.PENDING)
        value.update(status=migration.PENDING, reviewed_after=None)
        with patch.object(migration, "load", return_value=value):
            with self.assertRaisesRegex(ValueError, "pending"):
                migration.reviewed_after({})
            with self.assertRaisesRegex(ValueError, "pending"):
                migration.checkpoint_recorded()

    def test_signed_E20_and_unsigned_M1_are_distinct_closed_variants(self):
        value = migration.load()
        self.assertEqual(value["signed_anchor"]["commit"], migration.source.BASELINE["commit"])
        self.assertEqual(value["reviewed_intermediate"]["report_sha256"], migration.source.load()["reviewed_after"]["report_sha256"])
        self.assertFalse({"commit", "tree_sha1", "report_git_blob_sha1"} & set(value["reviewed_intermediate"]))
        mutations = {
            "bool-version": lambda v: v.update(schema_version=True),
            "unknown-version": lambda v: v.update(protocol="chialisp-oxide-runtime-composition/v2"),
            "M1-fake-commit": lambda v: v["reviewed_intermediate"].update(commit="1" * 40),
            "M1-anchor-relabel": lambda v: v["reviewed_intermediate"].update(kind="signed_git_checkpoint"),
            "M1-raw": lambda v: v["reviewed_intermediate"].update(report_sha256="0" * 64),
            "E20-wrong": lambda v: v["signed_anchor"].update(commit="0" * 40),
            "E20-unsigned": lambda v: v["signed_anchor"].update(kind="unsigned_reviewed_content_addressed_checkpoint"),
            "partial-final": lambda v: v.update(status=migration.FINAL, reviewed_after=None),
            "empty-final": lambda v: v.update(status=migration.FINAL, reviewed_after={}),
            "mixed-pending": lambda v: v.update(reviewed_after={}),
            "unknown-status": lambda v: v.update(status="reviewed_final_guarded_projection"),
            "old-profile": lambda v: v["immutable_profiles"].update({migration.guarded.PATH: "0" * 64}),
            "metadata-scope": lambda v: v["implementation_transition"]["source_metadata"]["added"].update({"unreviewed.rs": "0" * 64}),
        }
        for name, change in mutations.items():
            bad = copy.deepcopy(value)
            change(bad)
            self.assertFalse(cost.same(bad, value))  # Python True == 1; JSON types differ.
            payload = profile_bytes(bad)
            # Constructed byte-PIN rebinding permits reaching closed schema
            # checks; no production file or authorization is changed.
            with self.subTest(name=name), patch.object(migration, "PIN", cost.sha(payload)), self.assertRaises(ValueError):
                migration.load(payload)

    def test_checkpoint_restores_real_signed_anchor_without_current_validators(self):
        with patch.object(cost, "validate_current", side_effect=AssertionError("current on history")), \
                patch.object(migration.source, "validate_controls", side_effect=AssertionError("current controls on history")), \
                patch.object(subprocess, "run", side_effect=AssertionError("no execution")):
            before = migration.predecessor()
            restored = migration.source.project_validated(before)
        self.assertEqual(cost.sha(cost.canonical(before)), migration.INTERMEDIATE["canonical_report_sha256"])
        self.assertEqual(cost.sha(cost.canonical(restored)), migration.ANCHOR["canonical_report_sha256"])

    def test_immutable_raw_source_sidecars_are_bound_before_historical_use(self):
        migration.load()
        original = migration.read
        for path in [*migration.IMMUTABLE, migration.source.METRICS_PATH, migration.source.CONTROLS_PATH]:
            def damaged(filename, **kwargs):
                payload = original(filename, **kwargs)
                return payload + b"\n" if filename == path else payload
            with self.subTest(path=path), patch.object(migration, "read", side_effect=damaged), self.assertRaises(ValueError):
                migration.load()


class ActualCapturedRecordControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.payload = historical_payload()
        if cost.sha(cls.payload) != "227ce91137f786919ce47cf0b0e09038efa84ee2f9c0df6a4e3fc0894084dc64":
            raise AssertionError("Expected exact actual final captured corpus, not a constructed substitute")
        cls.current = json.loads(cls.payload)
        cls.before = migration.predecessor()
        cls.proposed = constructed_review_binding(cls.current)

    def test_actual_complete_delta_with_no_v1_admission_or_subprocess(self):
        with patch.object(subprocess, "run", side_effect=AssertionError("no execution")), \
                patch.object(migration.guarded, "load", side_effect=AssertionError("old66 remains pending")):
            self.assertEqual(migration.compare_delta(self.before, self.current,
                current_snapshot=self.current["authoring"]), self.proposed["reviewed_after"]["delta"])

    def test_partially_promoted_or_malformed_actual_capture_header_fails(self):
        payload = profile_bytes(self.proposed)
        with patch.object(migration, "PIN", cost.sha(payload)):
            self.assertEqual(migration.load(payload)["status"], migration.FINAL)
        for name in ("missing", "unknown", "mode", "bool-leaves", "rawIR", "manifest", "capture-hash"):
            bad = copy.deepcopy(self.proposed)
            after = bad["reviewed_after"]
            if name == "missing": del after["report_sha256"]
            elif name == "unknown": after["invented_commit"] = "0" * 40
            elif name == "mode": after["bundles"]["on"]["optimize_lir"] = 1
            elif name == "bool-leaves": after["delta"]["leaf_count"] = True
            elif name == "rawIR": after["raw_ir_sha256"] = "0" * 64
            elif name == "manifest": after["bundles"]["off"]["manifest_sha256"] = ""
            else: after["report_sha256"] = False
            self.assertFalse(cost.same(bad, self.proposed))
            payload = profile_bytes(bad)
            with self.subTest(name=name), patch.object(migration, "PIN", cost.sha(payload)), self.assertRaises(ValueError):
                migration.load(payload)

    def test_historical_composition_checkpoint_never_uses_current23_validators(self):
        with patch.object(cost, "validate_current", side_effect=AssertionError("current on historical227")), \
                patch.object(migration, "check_sidecars", side_effect=AssertionError("current sidecars on historical227")), \
                patch.object(migration.source, "validate_controls", side_effect=AssertionError("current controls on historical227")), \
                patch.object(migration, "snapshot", side_effect=AssertionError("current23 snapshot on historical227")), \
                patch.object(subprocess, "run", side_effect=AssertionError("no execution")):
            checkpoint = cost.composition_checkpoint(self.current)
            intermediate = cost.source_checkpoint(checkpoint)
        self.assertIs(checkpoint, self.current)
        self.assertEqual(cost.sha(cost.canonical(intermediate)), migration.INTERMEDIATE["canonical_report_sha256"])

    def test_historical_raw_checkpoint_and_current_raw_payload_are_distinct(self):
        payload = historical_payload()
        migration.check_report_bytes(payload, self.current) # Explicit historical8024 bytes, never23 payload.
        for bad in (payload + b"\n", b'{"constructed_current23":true}\n'):
            self.assertNotEqual(cost.sha(bad), RAW227)
            with self.subTest(size=len(bad)), self.assertRaises(ValueError):
                migration.check_report_bytes(bad, self.current)
        with patch.object(migration, "report_payload", return_value=payload+b"\n"), \
                self.assertRaisesRegex(ValueError, "historical227 bytes"):
            historical_payload()

    def test_actual_pure_history_never_reads_current_implementation_or_sidecars(self):
        original_read = migration.read
        forbidden = {migration.SDK, migration.CONS, migration.AUTHORING, migration.METRICS,
            migration.CONTROLS, *(r["path"] for r in self.proposed["implementation_transition"]["compiler_sources"])}
        def historical_read(path, **kwargs):
            self.assertNotIn(path, forbidden, "Current source/control check on restored history")
            return original_read(path, **kwargs)
        with patch.object(migration, "load", return_value=self.proposed), \
                patch.object(migration, "read", side_effect=historical_read), \
                patch.object(migration, "snapshot", side_effect=AssertionError("current snapshot on history")), \
                patch.object(migration, "check_sidecars", side_effect=AssertionError("current sidecars on history")), \
                patch.object(cost, "validate_current", side_effect=AssertionError("current validation on history")), \
                patch.object(subprocess, "run", side_effect=AssertionError("no execution")):
            intermediate = migration.project_validated(self.current)
            anchored = migration.source.project_validated(intermediate)
            historical = migration.append.project_validated(anchored)
        self.assertEqual(cost.sha(cost.canonical(intermediate)), migration.INTERMEDIATE["canonical_report_sha256"])
        self.assertEqual(cost.sha(cost.canonical(anchored)), migration.ANCHOR["canonical_report_sha256"])
        self.assertEqual(len(historical["puzzles"]), 21)

    def test_historical_sidecar_corruptions_are_fixed_byte_bindings_not_current_checks(self):
        # The prior current22 source-positive is invalid in this23 checkout.
        # Retain its six field mutations as explicit historical byte-binding
        # controls, not rebound semantic-source checker or execution credit.
        controls = json.loads(historical_controls_payload())
        original = migration.read
        for name in ("root", "manifest", "compiler", "mode", "native-count", "vm-resource"):
            bad = copy.deepcopy(controls)
            if name == "root": bad["window"]["source_root"] += "/wrong"
            elif name == "manifest": bad["vm_replay"]["manifest_path"] += ".other"
            elif name == "compiler": bad["window"]["compiler"] = bad["window"]["compiler"].replace("host: ", "host: corrupted-")
            elif name == "mode": bad["identity"]["optimize_lir"] = False
            elif name == "native-count": bad["native"]["tests"] = True
            else: bad["vm_replay"]["controls"][0]["runs"][0]["execution"]["failure"] = {"kind":"resource", "code":"cost_exceeded"}
            self.assertFalse(cost.same(bad, controls))
            def replaced(path, **kwargs):
                return profile_bytes(bad) if path == migration.CONTROLS else original(path, **kwargs)
            with self.subTest(name=name), patch.object(migration, "read", side_effect=replaced), \
                    patch.object(migration.source, "validate_controls", side_effect=AssertionError("current controls on historical227")), \
                    self.assertRaisesRegex(ValueError, "historical227 control bytes"):
                historical_controls_payload()


class ConstructedMetadataControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.before = migration.predecessor()
        cls.current = constructed_metadata_fixture(cls.before)
        cls.snapshot = historical_report()["authoring"] # Recorded pure argument, never current23 snapshot.

    def test_positive_normalization_is_exact_M1_except_explicit_pipeline(self):
        with patch.object(subprocess, "run", side_effect=AssertionError("no execution")), \
                patch.object(migration.guarded, "load", side_effect=AssertionError("no pending v1 admission")), \
                patch.object(cost, "validate_current", side_effect=AssertionError("no current validation on history")):
            value = migration.normalize_metadata(self.before, self.current, current_snapshot=self.snapshot)
            value["optimization_pipeline"] = copy.deepcopy(self.before["optimization_pipeline"])
            self.assertTrue(cost.same(value, self.before))
            self.assertGreater(migration.compare_delta(self.before, self.current, current_snapshot=self.snapshot)["leaf_count"], 0)

    def test_metadata_rehashed_and_unrelated_changes_reject_after_positive(self):
        migration.compare_delta(self.before, self.current, current_snapshot=self.snapshot)
        for name in ("missing-cons", "stale-sdk", "unknown-authoring", "sdk-body", "missing-copy", "duplicate-copy", "other-dependency", "source", "native", "off", "rue", "coverage", "typed-failure", "protocol", "pipeline"):
            bad = copy.deepcopy(self.current)
            if name == "missing-cons":
                bad["authoring"]["sources"] = [r for r in bad["authoring"]["sources"] if r["path"] != migration.CONS]
            elif name == "stale-sdk":
                next(r for r in bad["authoring"]["sources"] if r["path"] == migration.SDK)["sha256"] = migration.OLD_SDK
            elif name == "unknown-authoring": bad["authoring"]["extra"] = True
            elif name in ("sdk-body", "missing-copy", "duplicate-copy", "other-dependency"):
                puzzle = next(p for p in bad["puzzles"] if any(r["path"] == migration.SDK for r in p["source_dependencies"]))
                row = next(r for r in puzzle["source_dependencies"] if r["path"] == migration.SDK)
                if name == "sdk-body":
                    row["source"] += "\n// constructed corruption\n"
                    row["sha256"] = cost.sha(row["source"].encode())
                elif name == "missing-copy": puzzle["source_dependencies"].remove(row)
                elif name == "duplicate-copy": puzzle["source_dependencies"].append(copy.deepcopy(row))
                else:
                    next(r for p in bad["puzzles"] for r in p["source_dependencies"] if r["path"] != migration.SDK)["sha256"] = "0" * 64
            elif name == "source": bad["puzzles"][0]["source"] += "\n"
            elif name == "native": bad["puzzles"][0]["cases"][0]["native"] = {"status": "rejected"}
            elif name == "off": bad["puzzles"][0]["artifacts"][0]["bytes"] += 1
            elif name == "rue": bad["puzzles"][0]["cases"][0]["runs"][3]["execution"]["cost"] += 1
            elif name == "coverage": bad["puzzles"].pop()
            elif name == "typed-failure": bad["puzzles"][0]["cases"][0]["runs"][1]["execution"]["failure"] = {"kind":"resource", "code":"cost_exceeded"}
            elif name == "protocol": bad["puzzles"][-1]["protocol_validation"]["observations"][1]["module_tree_hash"] = "0" * 64
            else: bad["optimization_pipeline"] = copy.deepcopy(self.before["optimization_pipeline"])
            self.assertFalse(cost.same(bad, self.current))
            with self.subTest(name=name), patch.object(subprocess, "run", side_effect=AssertionError("no execution")), self.assertRaises((ValueError, KeyError)):
                migration.compare_delta(self.before, bad, current_snapshot=self.snapshot)

    def test_historical22_never_becomes_current23_or_falls_back_authority(self):
        import corpus_append as current
        import p2dc_append_migration as outer
        self.assertIs(cost.current22_migration(self.current), current)
        with patch.object(current, "reviewed_after", side_effect=ValueError("pending actual current")), \
                patch.object(outer, "reviewed_after", side_effect=AssertionError("fallback to retained23")), \
                patch.object(migration, "reviewed_after", side_effect=AssertionError("fallback to historical8024")), \
                patch.object(migration.guarded, "reviewed_after", side_effect=AssertionError("fallback to pending66")), \
                self.assertRaisesRegex(ValueError, "pending actual current"):
            cost.reviewed_current22(self.current)

    def test_pure_historical_chain_uses_recorded_identity_without_current_spies(self):
        actual = historical_report()
        with patch.object(cost, "validate_current", side_effect=AssertionError("current on history")), \
                patch.object(migration, "snapshot", side_effect=AssertionError("current23 snapshot on history")), \
                patch.object(migration, "check_sidecars", side_effect=AssertionError("current sidecars on history")), \
                patch.object(migration.guarded, "project_validated", side_effect=AssertionError("pending66 projection")), \
                patch.object(subprocess, "run", side_effect=AssertionError("no execution")):
            self.assertIs(cost.composition_checkpoint(actual), actual)
            historical = cost.project_append(actual)
        self.assertEqual(len(historical["puzzles"]), 21)



if __name__ == "__main__":
    unittest.main()
