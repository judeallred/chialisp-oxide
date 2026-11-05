"""Constructed transport controls, not source compilation or policy evidence."""
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import unittest
from unittest.mock import patch

import cargo_bundle
import cargo_source_receipt as receipt
from test_cargo_bundle import Fixture, COMPILER, DRIVER, TOOLCHAIN, RUE


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def source_fixture(directory):
    fixture = Fixture(directory)
    raw = b"\xef\xbb\xbf// original CRLF bytes\r\npub fn source() {}\r\n"
    (fixture.root / "lib.rs").write_bytes(raw)
    fixture.authoring = fixture.snapshot()
    fixture.manifest["schema_version"] = 2
    fixture.manifest["package_id"] = "constructed-controls-package"
    files, expected = [], {}
    for label, contents, kind in [
        ("generated/payload.bin", b"\x00\xff\r\n", "compiler_binary"),
        ("package/Cargo.toml", (fixture.root / "Cargo.toml").read_bytes(), "launcher_manifest"),
        ("package/empty.bin", b"", "compiler_binary"),
        ("package/lib.rs", raw, "compiler_text"),
    ]:
        blob = f"source-{digest(contents)}.bin"
        (fixture.bundle / blob).write_bytes(contents)
        files.append({"path": label, "kinds": [kind], "compiler_read_count": int(kind.startswith("compiler_")),
                      "bytes": len(contents), "sha256": digest(contents), "blob": blob})
        expected[label] = contents
    exports = []
    for entry in fixture.manifest["entries"]:
        raw_ir = (fixture.bundle / entry["ir"]).read_bytes()
        exports.append({"entry": entry["entry"], "ir": entry["ir"],
                        "bytes": len(raw_ir), "sha256": digest(raw_ir)})
    fixture.manifest["source_receipt"] = {
        "protocol": receipt.PROTOCOL, "scope": receipt.SCOPE,
        "build": {"package": "controls", "package_id": fixture.manifest["package_id"],
                  "compiler_sha256": digest(COMPILER.encode()), "toolchain": TOOLCHAIN,
                  "analysis_target": "test-host", "driver_sha256": DRIVER, "rue_revision": RUE,
                  "cargo_options": ["--locked"], "optimize_lir": True},
        "unit": {"crate_name": "controls", "analysis_target": "test-host", "features": [],
                 "library_source": "package/lib.rs"},
        "exports": exports, "files": files,
        "snapshots": {"package_manifest": "package/Cargo.toml",
                      "workspace_manifest": "package/Cargo.toml",
                      "lockfile": None, "build_script": None},
        "omitted": {"outside_roots_reads": 0, "outside_roots_bytes": 0},
    }
    fixture.save()
    return fixture, expected


class SourceReceiptTests(unittest.TestCase):
    def fixture(self):
        import tempfile
        directory = tempfile.TemporaryDirectory(prefix="chialisp-source-receipt-controls-")
        self.addCleanup(directory.cleanup)
        return source_fixture(Path(directory.name))

    def test_exact_raw_source_bytes_and_complete_ir_inventory(self):
        fixture, expected = self.fixture()
        with patch("subprocess.run", side_effect=AssertionError("offline reader cannot execute")):
            result = fixture.load()
        self.assertEqual(result.source_receipt, fixture.manifest["source_receipt"])
        self.assertEqual({row.label: row.contents for row in result.source_files}, expected)
        for row in result.source_files:
            self.assertEqual(digest(row.contents), row.sha256)
            self.assertEqual(row.contents, row.path.read_bytes())
        self.assertEqual(result.source_files[0].kinds, ("compiler_binary",))

    def test_standalone_attachment_check_does_not_read_original_host_paths(self):
        fixture, expected = self.fixture()
        details = {row["entry"]: (row["ir"], row["bytes"], row["sha256"])
                   for row in fixture.manifest["source_receipt"]["exports"]}
        artifact_paths = {fixture.bundle / entry[key] for entry in fixture.manifest["entries"]
                          for key in ("ir", "clvm")}
        shutil.rmtree(fixture.root)
        result = receipt.validate_receipt(fixture.manifest, fixture.bundle, details,
                                         cargo_bundle._Reader(), artifact_paths)
        self.assertEqual({row.label: row.contents for row in result}, expected)
        # The separate fresh-source API intentionally still needs its declared
        # current checkout; the portable attachment checker does not.
        with self.assertRaises((OSError, ValueError)):
            fixture.load()

    def test_private_unit_is_rejected_but_unreserved_sidecars_are_allowed(self):
        fixture, _ = self.fixture()
        (fixture.bundle / "source-notes.txt").write_text("unrelated presentation sidecar")
        self.assertIsNotNone(fixture.load().source_receipt)
        (fixture.bundle / "source-unit.json").write_text("{}")
        with self.assertRaisesRegex(ValueError, "Private source unit"):
            fixture.load()

    def test_schema1_compatibility_and_source_receipt_downgrade_rejection(self):
        import tempfile
        with tempfile.TemporaryDirectory() as directory:
            old = Fixture(Path(directory))
            self.assertIsNone(old.load().source_receipt)
            self.assertEqual(old.load().source_files, ())
            # Retained historical readers still use this three-value API.
            program, path, ir_hash = cargo_bundle._artifact(
                old.manifest["entries"][0], old.bundle.resolve(),
                cargo_bundle._Reader(), set(), True)
            self.assertEqual(program["name"], old.manifest["entries"][0]["entry"])
            self.assertEqual(ir_hash, digest(path.read_bytes()))
        fixture, _ = self.fixture()
        fixture.manifest["schema_version"] = 1
        fixture.save()
        with self.assertRaisesRegex(ValueError, "cannot carry"):
            fixture.load()

    def test_closed_fields_typed_values_and_independent_metadata_relations(self):
        corruptions = [
            lambda r: r.update(extra=True),
            lambda r: r.update(protocol="unknown"),
            lambda r: r.update(scope="complete_build_graph"),
            lambda r: r["build"].update(extra=True),
            lambda r: r["build"].update(compiler_sha256="0" * 64),
            lambda r: r["build"].update(package_id="another-package"),
            lambda r: r["build"].update(cargo_options=[]),
            lambda r: r["build"].update(optimize_lir=1),
            lambda r: r["unit"].update(features=["b", "a"]),
            lambda r: r["unit"].update(features=["a", "a"]),
            lambda r: r["unit"].update(features=[{}]),
            lambda r: r["unit"].update(library_source="workspace/Cargo.toml"),
            lambda r: r["exports"].pop(),
            lambda r: r["exports"][1].update(sha256="0" * 64),
            lambda r: r["exports"][1].update(bytes=True),
            lambda r: r["exports"][1].update(ir=r["exports"][0]["ir"]),
            lambda r: r["files"][0].update(kinds=["unknown"]),
            lambda r: r["files"][0].update(kinds=[{"compiler_binary": {}}]),
            lambda r: r["files"][0].update(compiler_read_count=True),
            lambda r: r["files"][0].update(bytes=-1),
            lambda r: r["files"][0].update(path="../escape"),
            lambda r: r["files"][0].update(path="generated//alias"),
            lambda r: r["files"][0].update(path="outside/asset"),
            lambda r: r["files"][0].update(blob="manifest.json"),
            lambda r: r["files"].reverse(),
            lambda r: r["snapshots"].pop("lockfile"),
            lambda r: r["snapshots"].update(lockfile="package/empty.bin"),
            lambda r: r["omitted"].update(outside_roots_bytes=1),
            lambda r: r["omitted"].update(outside_roots_reads=1 << 64),
        ]
        for index, corrupt in enumerate(corruptions):
            with self.subTest(index=index):
                fixture, _ = self.fixture()
                corrupt(fixture.manifest["source_receipt"])
                fixture.save()
                with self.assertRaises(ValueError):
                    fixture.load()

    def test_unselected_ir_and_blob_corruption_cannot_hide_behind_selection(self):
        fixture, _ = self.fixture()
        path = fixture.bundle / fixture.manifest["entries"][1]["ir"]
        raw = json.loads(path.read_bytes())
        raw["body"]["value"] = False
        # Keep this changed, valid IR below its old declared length so it
        # exercises exact-byte correspondence separately from the read cap.
        path.write_text(json.dumps(raw, separators=(",", ":")))
        with self.assertRaisesRegex(ValueError, "exact IR"):
            fixture.load()
        fixture, _ = self.fixture()
        blob = fixture.bundle / fixture.manifest["source_receipt"]["files"][0]["blob"]
        blob.write_bytes(b"xxxx")  # same length: content identity must still fail
        with self.assertRaisesRegex(ValueError, "digest"):
            fixture.load()

    def test_missing_extra_private_symlink_and_nonregular_inventory(self):
        for kind in ("missing", "extra", "private", "symlink", "directory"):
            with self.subTest(kind=kind):
                fixture, _ = self.fixture()
                row = fixture.manifest["source_receipt"]["files"][0]
                blob = fixture.bundle / row["blob"]
                if kind == "missing":
                    blob.unlink()
                elif kind == "extra":
                    (fixture.bundle / ("source-" + "0" * 64 + ".bin")).write_bytes(b"")
                elif kind == "private":
                    (fixture.bundle / "source-unit.json").write_text("{}")
                elif kind == "symlink":
                    blob.unlink()
                    blob.symlink_to(fixture.root / "lib.rs")
                else:
                    blob.unlink()
                    blob.mkdir()
                with self.assertRaises(ValueError):
                    fixture.load()

    @unittest.skipUnless(hasattr(os, "mkfifo"), "requires Unix FIFO support")
    def test_real_fifo_is_rejected_before_open(self):
        fixture, _ = self.fixture()
        blob = fixture.bundle / fixture.manifest["source_receipt"]["files"][0]["blob"]
        blob.unlink()
        os.mkfifo(blob)
        with self.assertRaises(ValueError):
            fixture.load()

    def test_source_budgets_are_charged_before_any_blob_open(self):
        fixture, _ = self.fixture()
        manifest = fixture.manifest
        details = {r["entry"]: (r["ir"], r["bytes"], r["sha256"])
                   for r in manifest["source_receipt"]["exports"]}
        with patch.object(receipt, "MAX_SOURCE_BYTES", 1), \
                patch.object(Path, "open", side_effect=AssertionError("opened before source budget")):
            with self.assertRaisesRegex(ValueError, "budget"):
                receipt.validate_receipt(manifest, fixture.bundle, details, cargo_bundle._Reader(), set())
        blob = fixture.bundle / manifest["source_receipt"]["files"][0]["blob"]
        blob.write_bytes(b"x" * (receipt.MAX_FILE_BYTES + 1))
        with patch.object(Path, "open", side_effect=AssertionError("opened oversized blob")):
            with self.assertRaisesRegex(ValueError, "length"):
                receipt.validate_receipt(manifest, fixture.bundle, details, cargo_bundle._Reader(), set())

    def test_v2_artifact_limit_precedes_hex_scan_or_clvm_read(self):
        fixture, _ = self.fixture()
        entry = fixture.manifest["entries"][0]
        entry["artifact"]["bytes"] = cargo_bundle.MAX_BLOB_BYTES + 1
        reader = cargo_bundle._Reader()
        declaration = fixture.manifest["source_receipt"]["exports"][0]
        with patch.object(reader, "bytes", wraps=reader.bytes) as reads:
            with self.assertRaisesRegex(ValueError, "artifact byte limit"):
                cargo_bundle._artifact_details(entry, fixture.bundle.resolve(), reader, set(), True, 2,
                    ir_declaration=declaration)
            self.assertEqual(reads.call_count, 0, "oversized artifact opened IR/CLVM")

    def test_declared_ir_length_caps_read_before_open_or_json_decoding(self):
        fixture, _ = self.fixture()
        row = fixture.manifest["source_receipt"]["exports"][0]
        row["bytes"] = 3
        ir = fixture.bundle / row["ir"]
        ir.write_bytes(ir.read_bytes() + b" " * 4096)
        fixture.save()
        real_read = cargo_bundle._Reader.bytes
        limits = []
        def bounded(reader, path, maximum=cargo_bundle.MAX_JSON_BYTES):
            if path.suffixes[-2:] == [".ir", ".json"]:
                limits.append(maximum)
                with patch.object(Path, "open", side_effect=AssertionError("oversized IR was opened")):
                    return real_read(reader, path, maximum)
            return real_read(reader, path, maximum)
        with patch.object(cargo_bundle._Reader, "bytes", bounded), \
                patch.object(cargo_bundle, "decode_json", wraps=cargo_bundle.decode_json) as decoded:
            with self.assertRaisesRegex(ValueError, "read budget"):
                fixture.load()
        self.assertEqual(limits, [3])
        self.assertEqual(decoded.call_count, 2, "oversized IR reached JSON decoding after pointer/manifest")
        reader = cargo_bundle._Reader(); reader.remaining = 2
        with patch.object(reader, "bytes", wraps=reader.bytes) as reads, \
                patch.object(cargo_bundle, "decode_json", side_effect=AssertionError("IR reached JSON decoding")):
            with self.assertRaisesRegex(ValueError, "read budget"):
                cargo_bundle._artifact_details(fixture.manifest["entries"][0], fixture.bundle.resolve(), reader,
                    set(), True, 2, ir_declaration=row)
        self.assertEqual(reads.call_args.args[1], 2, "declared cap ignored the remaining global budget")

    def test_complete_closed_declaration_preflight_precedes_every_ir_read(self):
        corruptions = [
            lambda m: m.pop("source_receipt"),
            lambda m: m.update(source_receipt=None),
            lambda m: m["source_receipt"].update(extra=True),
            lambda m: m["source_receipt"].update(protocol="unknown"),
            lambda m: m["source_receipt"].update(exports=[]),
            lambda m: m["source_receipt"]["exports"].pop(),
            lambda m: m["source_receipt"]["exports"].reverse(),
            lambda m: m["source_receipt"]["exports"][-1].update(bytes=0),
            lambda m: m["source_receipt"]["exports"][-1].update(bytes=False),
            lambda m: m["source_receipt"]["exports"][-1].update(bytes=receipt.MAX_IR_BYTES + 1),
            lambda m: m["source_receipt"]["exports"][-1].update(sha256="not-a-sha"),
            lambda m: m["source_receipt"]["exports"][-1].update(extra=True),
            lambda m: m["source_receipt"]["exports"][-1].update(ir=m["source_receipt"]["exports"][0]["ir"]),
            lambda m: m["entries"][-1].update(extra=True),
            lambda m: m["entries"][-1]["artifact"].update(bytes=cargo_bundle.MAX_BLOB_BYTES + 1),
        ]
        real_read = cargo_bundle._Reader.bytes
        for index, corrupt in enumerate(corruptions):
            fixture, _ = self.fixture()
            corrupt(fixture.manifest); fixture.save()
            def metadata_only(reader, path, maximum=cargo_bundle.MAX_JSON_BYTES):
                self.assertNotEqual(path.suffixes[-2:], [".ir", ".json"], "read selected IR before checking the full declaration set")
                return real_read(reader, path, maximum)
            with self.subTest(index=index), patch.object(cargo_bundle._Reader, "bytes", metadata_only):
                with self.assertRaises(ValueError):
                    fixture.load()

    def test_declared_ir_aggregate_budget_is_charged_before_first_ir(self):
        fixture, _ = self.fixture()
        for row in fixture.manifest["source_receipt"]["exports"]:
            row["bytes"] = 8192
        fixture.save()
        metadata_bytes = (fixture.output / "latest.json").stat().st_size + (fixture.bundle / "manifest.json").stat().st_size
        real_read = cargo_bundle._Reader.bytes
        def metadata_only(reader, path, maximum=cargo_bundle.MAX_JSON_BYTES):
            self.assertNotEqual(path.suffixes[-2:], [".ir", ".json"], "IR read preceded aggregate preflight")
            return real_read(reader, path, maximum)
        with patch.object(cargo_bundle, "MAX_TOTAL_BYTES", metadata_bytes + 8192), \
                patch.object(cargo_bundle._Reader, "bytes", metadata_only):
            with self.assertRaisesRegex(ValueError, "aggregate read budget"):
                fixture.load()

    def test_expanded_and_repeated_read_accounting_and_directory_bound(self):
        for kind in ("repeated", "directory", "expanded"):
            with self.subTest(kind=kind):
                fixture, _ = self.fixture()
                data = fixture.manifest["source_receipt"]
                if kind == "repeated":
                    data["files"][0]["compiler_read_count"] = receipt.MAX_READS + 1
                    fixture.save()
                    with self.assertRaises(ValueError):
                        fixture.load()
                elif kind == "directory":
                    with patch.object(receipt, "MAX_DIRECTORY_ENTRIES", 1):
                        with self.assertRaisesRegex(ValueError, "directory entry"):
                            fixture.load()
                else:
                    original = data["files"][0]
                    clone = copy.deepcopy(original)
                    clone["path"] = "generated/second.bin"
                    data["files"].append(clone)
                    data["files"].sort(key=lambda row: row["path"])
                    fixture.save()
                    total = sum(row["bytes"] for row in data["files"])
                    with patch.object(receipt, "MAX_SOURCE_BYTES", total - 1):
                        with self.assertRaisesRegex(ValueError, "budget"):
                            fixture.load()

    def test_large_omitted_bytes_and_rehashed_declarations_are_not_authentication(self):
        fixture, _ = self.fixture()
        data = fixture.manifest["source_receipt"]
        data["omitted"] = {"outside_roots_reads": 1, "outside_roots_bytes": (1 << 64) - 1}
        data["unit"]["features"] = ["an-unverified-feature-declaration"]
        fixture.save()
        self.assertEqual(fixture.load().source_receipt["omitted"], data["omitted"])
        # A consistent changed source declaration is admitted as different bytes.
        # This checker does not infer that the source was compiled.
        row = data["files"][0]
        old_blob = fixture.bundle / row["blob"]
        new_raw = b"changed declared source"
        row.update(bytes=len(new_raw), sha256=digest(new_raw), blob=f"source-{digest(new_raw)}.bin")
        old_blob.unlink()
        (fixture.bundle / row["blob"]).write_bytes(new_raw)
        fixture.save()
        self.assertEqual(fixture.load().source_files[0].contents, new_raw)

    def test_selected_feature_count_and_utf8_aggregate_limits(self):
        for features in ([f"feature-{i:03}" for i in range(129)],
                         [f"{i:03}" + "x" * 4093 for i in range(17)]):
            fixture, _ = self.fixture()
            fixture.manifest["source_receipt"]["unit"]["features"] = features
            fixture.save()
            with self.assertRaisesRegex(ValueError, "feature"):
                fixture.load()


class BoundedJsonTests(unittest.TestCase):
    def test_wire_json_is_strict_utf8_without_a_bom(self):
        for raw in ("{}".encode("utf-16"), "{}".encode("utf-32"),
                    b"\xef\xbb\xbf{}", "{\"x\":\"text\"}".encode("utf-16-le")):
            with self.subTest(raw=raw):
                with self.assertRaises(ValueError):
                    receipt.decode_json(raw)
        self.assertEqual(receipt.decode_json('{"x":"\U0001D11E"}'.encode("utf-8")),
                         {"x": "\U0001D11E"})

    def test_resource_preflight_precedes_key_decoding_or_json_value_allocation(self):
        for malformed in (br'{"\uD800":null}', b'{"\xff":null}', br'{"\u'):
            with patch("cargo_source_receipt.json.loads", side_effect=AssertionError("decoded too early")):
                with self.assertRaisesRegex(ValueError, "node limit"):
                    receipt.decode_json(malformed, max_nodes=1)
        self.assertEqual(receipt.decode_json(b"{}", max_nodes=1, max_depth=1), {})
        self.assertEqual(receipt.decode_json(br'{"\u0061":{}}', max_nodes=3, max_depth=2), {"a": {}})
        with self.assertRaisesRegex(ValueError, "depth limit"):
            receipt.decode_json(b'{"a":[0]}', max_depth=2)

    def test_duplicates_invalid_grammar_utf8_numbers_and_complete_consumption(self):
        for raw in (b'{"a":1,"a":2}', br'{"a":1,"\u0061":2}', b"{}{}", b"\xff",
                    b"NaN", b"[Infinity]", b"1e999", b"[-1e999]", b'{"a":}', b'{"x":"unfinished',
                    br'{"\uD800":null}', br'["\uDC00"]', br'"\uD800\u0041"'):
            with self.subTest(raw=raw):
                with self.assertRaises(ValueError):
                    receipt.decode_json(raw)
        self.assertEqual(receipt.decode_json(b'{"x":"quoted \\" {} []", "a":[1,null,true]}'),
                         {"x": 'quoted " {} []', "a": [1, None, True]})
        self.assertEqual(receipt.decode_json(br'["\uD834\uDD1E","\\uD800"]'),
                         ["\U0001D11E", "\\uD800"])


if __name__ == "__main__":
    unittest.main()
