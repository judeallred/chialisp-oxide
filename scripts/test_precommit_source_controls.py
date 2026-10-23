"""Pure policy relations and retained-packet corruption controls; no execution credit."""
import copy
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest import mock

import precommit_source_controls as c
from vm_failures import FAILURES


class SourceRelations(unittest.TestCase):
    def test_originals_are_exact_current_sources_and_mutants_are_distinct(self):
        sources = c.recipe_sources()
        for role in ("claim", "registry"):
            original = sources[f"src/{role}_original.rs"]
            self.assertEqual(original, c.read(c.ROOT / f"puzzles/precommit-{role}/puzzle.rs"))
            mutant = sources[f"src/{role}_{'drop_age' if role == 'claim' else 'drop_signature'}.rs"]
            self.assertNotEqual(original, mutant)
        self.assertIn(b"let _unused_age = assert_height_relative(age);", sources["src/claim_drop_age.rs"])
        self.assertIn(b"assert_height_relative(chialisp_oxide::BlockAge::from_blocks(0))", sources["src/claim_drop_age.rs"])
        self.assertIn(b"let _unused_authorization = agg_sig_me", sources["src/registry_drop_signature.rs"])

    def test_independent_outputs_change_only_age_and_signature_conditions(self):
        rows = c.witnesses()
        self.assertEqual([row["name"] for row in rows], list(c.NAMES))
        outputs = [c.raw_tree.deserialize(bytes.fromhex(row["expected"]["output_hex"])) for row in rows]
        original = c.oracle.fields(outputs[0], 6)
        weakened = c.oracle.fields(outputs[1], 6)
        self.assertEqual(rows[0]["solution"], rows[1]["solution"])
        self.assertEqual(original[:2] + original[3:], weakened[:2] + weakened[3:])
        self.assertEqual(original[2], c.oracle.proper(c.oracle.integer(82), c.oracle.integer(2)))
        self.assertEqual(weakened[2], c.oracle.proper(c.oracle.integer(82), b""))
        original = c.oracle.fields(outputs[2], 6)
        weakened = c.oracle.fields(outputs[3], 5)
        self.assertEqual(rows[2]["solution"], rows[3]["solution"])
        self.assertEqual(original[:4] + original[5:], weakened)
        self.assertEqual(c.oracle.fields(original[4], 3)[0], c.oracle.integer(50))

    def test_bounded_json_rejects_duplicates_and_excessive_depth(self):
        for payload in (b'{"a":1,"a":2}', b"[" * 100 + b"0" + b"]" * 100):
            with self.subTest(payload=payload[:30]), self.assertRaises(ValueError):
                c.decode_json(payload)


class RecordedPacket(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = c.ROOT / "corpus/precommit-source-controls"
        path = cls.directory / "report.json"
        if not path.is_file():
            raise FileNotFoundError("Missing captured precommit packet; recorded corruption tests have not run")
        cls.report = c.load(path)

    def validate(self, report, directory=None, *, current=False):
        with mock.patch("subprocess.run", side_effect=AssertionError("recorded reader executed a subprocess")), \
                mock.patch("subprocess.Popen", side_effect=AssertionError("recorded reader spawned a subprocess")):
            return c.validate(report, directory or self.directory, current=current)

    def rejects(self, mutate):
        report = copy.deepcopy(self.report)
        mutate(report)
        with self.assertRaises(ValueError):
            self.validate(report)

    def test_baseline_reader_is_read_only_and_executes_nothing(self):
        before = {row["path"]: c.sha(self.directory / row["path"]) for row in self.report["files"]}
        result = self.validate(self.report, current=True)
        self.assertEqual(result["sdk_transaction_attempts"], 8)
        self.assertEqual(before, {path: c.sha(self.directory / path) for path in before})

    def test_packet_and_run_corruptions(self):
        cases = {
            "protocol": lambda r: r.update(protocol="unreviewed/v1"),
            "schema": lambda r: r.update(schema_version=True),
            "unknown field": lambda r: r.update(extra="unreviewed"),
            "mode": lambda r: r["runs"][0].update(mode="on"),
            "missing VM row": lambda r: r["runs"].pop(),
            "duplicate entry": lambda r: r["runs"].__setitem__(1, copy.deepcopy(r["runs"][0])),
            "VM output": lambda r: r["runs"][0]["execution"]["outcome"].update(output_hex="80"),
            "solution": lambda r: r["runs"][0].update(solution="()"),
            "module identity": lambda r: r["runs"][0].update(module_tree_hash="00" * 32),
            "Boolean passed": lambda r: r["ledger"]["runs"][0]["scenarios"][0].update(passed=1),
            "Boolean optimize": lambda r: r["bundles"]["off"]["recompiled"][0]["artifacts"][0].update(optimize=0),
            "SDK code": lambda r: r["ledger"]["runs"][0]["scenarios"][0]["rejections"][0].update(code="BadAggregateSignature"),
            "SDK attempts": lambda r: r["ledger"].update(transaction_attempts=7),
            "SDK unknown field": lambda r: r["ledger"]["runs"][0].update(extra=True),
            "ledger runner path": lambda r: r["context"]["ledger_runner"].update(path="/tmp/unrelated"),
            "ledger runner digest": lambda r: r["context"]["ledger_runner"].update(sha256="00" * 32),
            "command role": lambda r: r["commands"][0].update(role="unrelated"),
            "command argv": lambda r: r["commands"][0]["argv"].append("--artifact-only"),
            "command exit": lambda r: r["commands"][0].update(exit_code=1),
            "command log metadata": lambda r: r["commands"][0].update(stdout=r["commands"][0]["stderr"]),
        }
        for name, mutate in cases.items():
            with self.subTest(name=name):
                self.rejects(mutate)

    def test_consistently_wrong_counterpart_is_rejected(self):
        def mutate(report):
            for row in report["ledger"]["runs"]:
                row["reference_tree_hash"] = "00" * 32
        self.rejects(mutate)

    def test_commands_cannot_point_at_an_unrelated_capture(self):
        for role in ("export-off", "vm-off-claim_original", "sdk-ledger"):
            def mutate(report):
                command = next(row for row in report["commands"] if row["role"] == role)
                original = next(row for row in report["commands"] if row["role"].startswith("recompile-"))["argv"][1].split("/bundles/")[0]
                command["argv"] = [arg.replace(original, "/different-capture") for arg in command["argv"]]
            with self.subTest(role=role):
                self.rejects(mutate)

    def test_resource_and_infrastructure_failures_never_confirm_a_witness(self):
        for code, kind in FAILURES.items():
            if kind == "program":
                continue
            def mutate(report):
                report["runs"][0]["execution"] = dict(
                    outcome=dict(status="inconclusive"), cost=None,
                    error="constructed resource/infrastructure control", failure=dict(kind=kind, code=code))
            with self.subTest(code=code):
                self.rejects(mutate)

    def test_actual_retained_ir_rejects_wrong_identity_and_abi(self):
        directory = self.directory / "bundles/off"
        manifest = c.load(directory / "manifest.json")
        for entry in manifest["entries"]:
            name = entry["entry"].split("::")[-2]
            original = c.load(directory / entry["ir"])
            c.strict_abi(original, name)
            for label, mutate in (
                ("entry", lambda ir: ir["program"].update(name="unrelated::puzzle")),
                ("environment", lambda ir: ir.update(environment="ignored")),
                ("version", lambda ir: ir.update(ir_version=1)),
                ("parameter", lambda ir: ir["program"]["parameters"][0].update(ty=dict(kind="bool"))),
                ("result", lambda ir: ir["program"].update(result=dict(kind="bool"))),
            ):
                changed = copy.deepcopy(original)
                mutate(changed)
                with self.subTest(entry=name, corruption=label), self.assertRaises(ValueError):
                    c.strict_abi(changed, name)

    def test_retained_file_corruptions(self):
        with tempfile.TemporaryDirectory(prefix="precommit-packet-controls-") as temp:
            directory = Path(temp) / "packet"
            shutil.copytree(self.directory, directory, symlinks=True)
            blob = next(path for path in directory.glob("bundles/off/source-*.bin") if path.is_file())
            original = blob.read_bytes()
            blob.write_bytes(original + b"\n")
            with self.assertRaises(ValueError):
                self.validate(self.report, directory)
            blob.write_bytes(original)
            extra = directory / "unexpected.txt"
            extra.write_text("unlisted file\n")
            with self.assertRaises(ValueError):
                self.validate(self.report, directory)
            extra.unlink()
            # The symlink targets another retained regular file: confinement
            # alone must not make a listed symlink acceptable.
            blob.unlink()
            blob.symlink_to(directory / "native.log")
            with self.assertRaises(ValueError):
                self.validate(self.report, directory)

    def test_native_and_quiet_file_relations(self):
        for filename, mutate in (
            ("last-host-window.json", lambda row: row["ledger_runner"].update(sha256="00" * 32)),
            ("native-receipt.json", lambda row: row.update(package_root="/tmp/unrelated")),
        ):
            with self.subTest(filename=filename), tempfile.TemporaryDirectory(prefix="precommit-receipt-control-") as temp:
                directory = Path(temp) / "packet"
                shutil.copytree(self.directory, directory, symlinks=True)
                report = copy.deepcopy(self.report)
                receipt = c.load(directory / filename)
                mutate(receipt)
                c.write(directory / filename, receipt)
                for row in report["files"]:
                    if row["path"] == filename:
                        row["sha256"] = c.sha(directory / filename)
                with self.assertRaises(ValueError):
                    self.validate(report, directory)

    def test_bundle_inventory_and_coherent_source_blob_substitution(self):
        for corruption in ("duplicate export", "changed compiler-read source"):
            with self.subTest(corruption=corruption), tempfile.TemporaryDirectory(prefix="precommit-bundle-control-") as temp:
                directory = Path(temp) / "packet"
                shutil.copytree(self.directory, directory, symlinks=True)
                report = copy.deepcopy(self.report)
                manifest_path = directory / "bundles/off/manifest.json"
                manifest = c.load(manifest_path)
                if corruption == "duplicate export":
                    manifest["entries"][1] = copy.deepcopy(manifest["entries"][0])
                else:
                    rows = manifest["source_receipt"]["files"]
                    selected = next(row for row in rows if row["path"] == "package/src/precommit_recipe.rs")
                    old_blob = selected["blob"]
                    payload = c.read(manifest_path.parent / old_blob) + b"\n"
                    changed_hash = c.digest(payload)
                    changed_blob = f"source-{changed_hash}.bin"
                    (manifest_path.parent / changed_blob).write_bytes(payload)
                    (manifest_path.parent / old_blob).unlink()
                    for row in rows:
                        if row["blob"] == old_blob:
                            row.update(blob=changed_blob, bytes=len(payload), sha256=changed_hash)
                    for row in report["files"]:
                        if row["path"] == "bundles/off/" + old_blob:
                            row.update(path="bundles/off/" + changed_blob, sha256=changed_hash)
                c.write(manifest_path, manifest)
                manifest_hash = c.sha(manifest_path)
                report["bundles"]["off"]["manifest_sha256"] = manifest_hash
                for row in report["files"]:
                    if row["path"] == "bundles/off/manifest.json":
                        row["sha256"] = manifest_hash
                report["files"].sort(key=lambda row: row["path"])
                with self.assertRaises(ValueError):
                    self.validate(report, directory)


if __name__ == "__main__":
    unittest.main()
