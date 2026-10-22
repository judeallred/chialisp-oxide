"""Constructed reader/corruption controls; no native/compiler/VM executions."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from registry_codegen_evidence import (
    CRATE, TREE, Construction, FrozenFiles, Inconclusive, decode, digest, encode,
    facade_feature_record, file_identity, identity, inspect_compiled_artifact, normalized, proper, read, text, unhex,
)


def atom(raw):
    return {"op": "tree_atom", "value": {"op": "atom", "hex": raw.hex()}}


def constructed_envelope(*, discarded=False):
    """One synthetic IR control, never represented as rustc-emitted evidence."""
    apply = {"op": "apply", "program": atom(b"\x01"), "environment": {"op": "arg", "index": 0}}
    body = {"op": "let", "local_id": 0, "ty": TREE, "value": apply,
            "body": atom(b"") if discarded else {"op": "local", "local_id": 0}}
    name = "strict_unused_apply" if discarded else "atom255"
    return {"ir_version": 4, "environment": "raw", "program": {
        "name": CRATE + "::" + name,
        "parameters": [{"name": "solution" if discarded else "environment", "ty": TREE}],
        "result": TREE,
        "body": {"op": "call", "function_id": 0, "arguments": [{"op": "arg", "index": 0}]},
        "functions": [{"id": 0, "name": CRATE + "::" + name,
                       "parameters": [{"name": "arg0", "ty": TREE}], "result": TREE, "body": body}]}}


class RegistryCodegenReaderTests(unittest.TestCase):
    def test_prelaunch_failures_retain_terminal_inconclusive_command_records(self):
        # Constructed process/log failures only. Popen is replaced before it can
        # launch anything; these controls add no real command execution credit.
        script = Path(__file__).with_name("check-registry-codegen.py")
        spec = importlib.util.spec_from_file_location("registry_codegen_launch_record_test", script)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        for fault in ("popen", "stderr_open"):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                run = object.__new__(module.Run)
                run.output, run.commands = root, []
                run.environment, run.offline = {}, {}
                original_open = Path.open

                def controlled_open(path, *args, **kwargs):
                    if fault == "stderr_open" and path == root / "constructed.stderr" and args == ("xb",):
                        raise PermissionError("constructed log-open failure")
                    return original_open(path, *args, **kwargs)

                with patch.object(module.subprocess, "Popen", side_effect=FileNotFoundError("constructed missing tool")) as child, \
                        patch.object(Path, "open", new=controlled_open):
                    with self.assertRaises(Inconclusive):
                        run.command("constructed", ["/nonexistent/never-executed"], offline=True)
                self.assertEqual(child.call_count, 1 if fault == "popen" else 0)
                self.assertEqual(len(run.commands), 1)
                row = run.commands[0]
                self.assertEqual(row["status"], "launch_error")
                self.assertIsNone(row["exit_code"])
                self.assertIn("FileNotFoundError" if fault == "popen" else "PermissionError", row["error"])
                self.assertEqual(row["stdout"], file_identity(root / "constructed.stdout"))
                if fault == "popen":
                    self.assertEqual(row["stderr"], file_identity(root / "constructed.stderr"))
                else:
                    self.assertNotIn("stderr", row)
                self.assertEqual(json.loads((root / "commands.json").read_bytes()), run.commands)

    def test_source_window_detects_changed_or_missing_imported_json_helper(self):
        # Constructed source inventory only. The fake tool files are never
        # executed, and no original checkout source is modified.
        script = Path(__file__).with_name("check-registry-codegen.py")
        spec = importlib.util.spec_from_file_location("registry_codegen_source_window_test", script)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml",
                         "scripts/check-registry-codegen.py", "scripts/registry_codegen_evidence.py",
                         "scripts/cargo_bundle.py", "scripts/cargo_source_receipt.py",
                         "scripts/cargo_authoring.py", "scripts/source_walk.py", "scripts/vm_failures.py"):
                target = root / name
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(b"constructed source A")
            tool = root / "nonexecutable-tool"
            tool.write_bytes(b"constructed identity, never executed")
            run = object.__new__(module.Run)
            run.args = SimpleNamespace(registry_cli=tool, cli=tool, driver=tool)
            run.snapshot = lambda: {"scope": "constructed source-window control"}
            helper = root / "scripts/cargo_authoring.py"
            with patch.object(module, "ROOT", root), patch.object(module, "FIXTURE", root / "fixture"):
                before = run.quiet_inputs()
                original = next(row for row in before["repository_sources"] if row["path"] == str(helper))
                helper.write_bytes(b"constructed source B")
                after = run.quiet_inputs()
                changed = next(row for row in after["repository_sources"] if row["path"] == str(helper))
                self.assertEqual(original["bytes"], changed["bytes"])
                self.assertNotEqual(original["sha256"], changed["sha256"])
                self.assertNotEqual(before, after)
                helper.unlink()
                with self.assertRaises(FileNotFoundError):
                    run.quiet_inputs()

    def test_driver_one_line_and_full_pinned_rustc_protocol_are_cross_checked(self):
        # Constructed query text exercises protocol validation only. No command
        # is replaced, and this test earns no compiler-query/execution credit.
        script = Path(__file__).with_name("check-registry-codegen.py")
        spec = importlib.util.spec_from_file_location("registry_codegen_gate_reader_test", script)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        declaration = "rustc 1.99.0-nightly (daf2e5e18 2026-07-13)\n"
        complete = declaration + (
            "binary: rustc\ncommit-hash: daf2e5e18b3abc4311d9da3d96fd64138f43d508\n"
            "commit-date: 2026-07-13\nhost: aarch64-apple-darwin\n"
            "release: 1.99.0-nightly\nLLVM version: 22.1.8\n")
        self.assertEqual(module.check_compiler_queries(declaration, complete)["host"], "aarch64-apple-darwin")
        for driver, compiler in (
            (declaration, declaration), (complete, complete),
            (declaration.replace("1.99.0", "1.98.0"), complete),
            (declaration, complete.replace("host: aarch64-apple-darwin\n", "")),
            (declaration, complete.replace("host: aarch64-apple-darwin", "host: ")),
            (declaration, complete.replace("LLVM version: 22.1.8", "host: second-host")),
            (declaration, complete.replace("release: 1.99.0-nightly", "release: 1.98.0-nightly")),
            (declaration, complete.replace("daf2e5e18b3abc4311d9da3d96fd64138f43d508", "daf2e5e18")),
        ):
            with self.subTest(driver=driver, compiler=compiler), self.assertRaises(ValueError):
                module.check_compiler_queries(driver, compiler)

    def test_exact_atom_header_boundaries_and_payloads(self):
        for size in (0, 1, 63, 64, 127, 128, 255, 256, 8191, 8192):
            payload = b"\xff" * size
            with self.subTest(size=size):
                wire = encode(payload)
                self.assertEqual(decode(wire), payload)
                self.assertEqual(identity(wire)["module_tree_hash"], digest(b"\x01" + payload))
        self.assertEqual(encode(b"\x01"), b"\x01")
        self.assertEqual(encode(b"\x80"), b"\x81\x80")

    def test_padded_signed_nonutf8_and_improper_trees_are_not_normalized(self):
        tree = ((b"\x00\x01", b"\xff\x80"), (b"", b"\x00"))
        self.assertEqual(decode(encode(tree)), tree)
        self.assertEqual(text((b"\x00\x01", b"\xff")), "(0x0001 . 0xff)")
        self.assertNotEqual(identity(encode(b"\x00")), identity(encode(b"")))
        self.assertEqual(decode(encode(proper([b"\x01", b"\x02"]))), (b"\x01", (b"\x02", b"")))

    def test_compiled_identity_matches_small_codec_and_preserves_separate_limits(self):
        trees = (b"", b"\x00", b"\x00\x01", b"\xff\x80\xfd\xfe", proper([b"\x01", b"\x02"]),
                 ((b"\x00\x01", b"\xff"), b"improper"))
        for tree in trees:
            raw = encode(tree)
            with self.subTest(tree=tree):
                self.assertEqual(inspect_compiled_artifact(raw)["identity"], identity(raw))
        deep = b"\xff\x80" * 128 + b"\x80"  # Depth 129, 257 logical nodes.
        inspected = inspect_compiled_artifact(deep)
        self.assertEqual(inspected["stats"]["depth_root_one"], 129)
        self.assertEqual(inspected["stats"]["nodes"], 257)
        nil_hash = hashlib.sha256(b"\x01").digest()
        expected = nil_hash
        for _ in range(128):
            expected = hashlib.sha256(b"\x02" + nil_hash + expected).digest()
        self.assertEqual(inspected["identity"]["module_tree_hash"], expected.hex())
        with self.assertRaises(ValueError):
            identity(deep)
        tree = b""
        for _ in range(128):
            tree = (b"", tree)
        with self.assertRaises(ValueError):
            encode(tree)
        wide = b"\x80"
        for _ in range(13):
            wide = b"\xff" + wide + wide
        self.assertEqual(inspect_compiled_artifact(wide)["stats"]["nodes"], 16383)
        with self.assertRaises(ValueError):
            decode(wide)

    def test_compiled_wide_atom_headers_preserve_payload_markers(self):
        for size, header in ((63, b"\xbf"), (64, b"\xc0\x40"), (255, b"\xc0\xff"),
                             (256, b"\xc1\x00"), (8191, b"\xdf\xff"), (8192, b"\xe0\x20\x00"),
                             (1048576, b"\xf0\x10\x00\x00")):
            payload = (b"\xff\x80\xfd\xfe" * ((size + 3) // 4))[:size]
            raw = header + payload
            with self.subTest(size=size):
                inspected = inspect_compiled_artifact(raw)
                self.assertEqual(inspected["stats"], {"nodes": 1, "pairs": 0, "atoms": 1,
                                                     "atom_payload_bytes": size, "depth_root_one": 1})
                self.assertEqual(inspected["identity"]["module_tree_hash"], digest(b"\x01" + payload))
                self.assertEqual(inspected["identity"]["blob_sha256"], digest(raw))

    def test_compiled_noncanonical_truncated_trailing_and_alternate_formats_fail(self):
        for raw in (b"", b"\x81\x01", b"\xc0\x01\x80", b"\xc0\x00", b"\xe0\x00\x40" + b"x" * 64,
                    b"\xf0\x00\x20\x00" + b"x" * 8192, b"\xff\x80", b"\xc1", b"\xc1\x00" + b"x" * 255,
                    b"\x80\x80", b"\xfe\x01", b"\xfd\xff2026", b"\xfc\x00\x00\x00\x00\x01"):
            with self.subTest(raw=raw[:12]), self.assertRaises(ValueError):
                inspect_compiled_artifact(raw)
        # Markers inside an atom remain opaque payload bytes.
        self.assertEqual(inspect_compiled_artifact(b"\x83\xff\xfd\xfe")["stats"]["atom_payload_bytes"], 3)

    def test_compiled_transport_exact_and_one_under_bounds(self):
        raw = b"\xff\x80\x80"  # Exactly three bytes/nodes, depth two.
        for bounds in ({"max_bytes": 3, "max_nodes": 3, "max_depth": 2},
                       {"max_bytes": 4, "max_nodes": 4, "max_depth": 3}):
            self.assertEqual(inspect_compiled_artifact(raw, **bounds)["stats"]["nodes"], 3)
        for bounds in ({"max_bytes": 2}, {"max_nodes": 2}, {"max_depth": 1}):
            with self.subTest(bounds=bounds), self.assertRaises(ValueError):
                inspect_compiled_artifact(raw, **bounds)
        self.assertEqual(inspect_compiled_artifact(b"\x80", max_bytes=1, max_nodes=1, max_depth=1)["stats"]["nodes"], 1)
        for bounds in ({"max_bytes": 0}, {"max_nodes": 0}, {"max_depth": 0},
                       {"max_bytes": True}, {"max_nodes": 262145}, {"max_depth": 4097}, {"max_bytes": 2097153}):
            with self.subTest(bounds=bounds), self.assertRaises(ValueError):
                inspect_compiled_artifact(b"\x80", **bounds)

    def test_compiled_package_ceiling_depth_nodes_and_bytes(self):
        deepest = b"\xff\x80" * 4095 + b"\x80"
        self.assertEqual(inspect_compiled_artifact(deepest)["stats"]["depth_root_one"], 4096)
        with self.assertRaises(ValueError):
            inspect_compiled_artifact(b"\xff\x80" + deepest)
        broad = b"\x80"
        for _ in range(17):
            broad = b"\xff" + broad + broad
        self.assertEqual(inspect_compiled_artifact(broad)["stats"]["nodes"], 262143)
        with self.assertRaises(ValueError):
            inspect_compiled_artifact(b"\xff\x80" + broad)  # 262145 nodes.
        payload = b"\xa5" * (2097152 - 4)
        largest = b"\xf0\x1f\xff\xfc" + payload
        self.assertEqual(inspect_compiled_artifact(largest)["identity"]["bytes"], 2097152)
        with patch("registry_codegen_evidence.hashlib.sha256", wraps=hashlib.sha256) as hash_reader:
            with self.assertRaises(ValueError):
                inspect_compiled_artifact(largest, max_bytes=2097151)
            with self.assertRaises(ValueError):
                inspect_compiled_artifact(largest + b"\x80")
            self.assertEqual(hash_reader.call_count, 0)

    def test_nonminimal_trailing_truncated_and_deep_data_are_rejected(self):
        malformed = (b"", b"\x81\x01", b"\xc0\x01\x80", b"\xc0\x00", b"\xff\x80",
                     b"\x80\x80", b"\xfe", b"\xc1\x00" + b"\xa5" * 255)
        for wire in malformed:
            with self.subTest(wire=wire[:8]), self.assertRaises(ValueError):
                decode(wire)
        with self.assertRaises(ValueError):
            decode(b"\xff\x80" * 129 + b"\x80")
        with self.assertRaises(ValueError):
            encode(b"x" * 8193)

    def test_typed_resources_and_infrastructure_never_become_rejection(self):
        for kind, code in (("resource", "cost_exceeded"), ("infrastructure", "internal_error")):
            execution = {"outcome": {"status": "inconclusive"}, "cost": None, "error": "not policy rejection",
                         "failure": {"kind": kind, "code": code}}
            with self.subTest(kind=kind), self.assertRaises(Inconclusive):
                normalized(execution, "constructed")
        execution = {"outcome": {"status": "rejected"}, "cost": None, "error": "human text says something else",
                     "failure": {"kind": "program", "code": "path_into_atom"}}
        self.assertEqual(normalized(execution, "constructed"), {"status": "rejected", "code": "path_into_atom"})
        execution["failure"] = {"kind": "program", "code": "cost_exceeded"}
        with self.assertRaises(ValueError):
            normalized(execution, "constructed forged kind")

    def test_regular_file_and_streamed_digest_bounds(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "input"
            path.write_bytes(b"abc")
            self.assertEqual(read(path, 3), b"abc")
            self.assertEqual(file_identity(path, 3)["sha256"], digest(b"abc"))
            for reader in (read, file_identity):
                with self.subTest(reader=reader.__name__), self.assertRaises(ValueError):
                    reader(path, 2)
            link = Path(directory) / "link"
            link.symlink_to(path)
            for reader in (read, file_identity):
                with self.subTest(reader=reader.__name__), self.assertRaises(ValueError):
                    reader(link)

    def test_hex_requires_exact_lowercase_bytes(self):
        for raw in ("f", "FF", " 00", "00\n", 0, "gg"):
            with self.subTest(raw=raw), self.assertRaises(ValueError):
                unhex(raw)
        self.assertEqual(unhex("00ff"), b"\x00\xff")

    def test_frozen_actual_input_inventory_deduplicates_and_detects_same_size_corruption(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "program.hex"
            path.write_bytes(b"01\n")
            inventory = FrozenFiles()
            first = inventory.observe([path, path])
            self.assertEqual(len(first), 1)
            self.assertEqual(inventory.observe([path]), first)
            self.assertEqual(inventory.snapshot()["status"], "matched")
            path.write_bytes(b"02\n")
            changed = inventory.snapshot()
            self.assertEqual(changed["status"], "changed")
            self.assertEqual(changed["files"][0]["bytes"], first[0]["bytes"])
            self.assertNotEqual(changed["files"][0]["sha256"], first[0]["sha256"])
            with self.assertRaises(ValueError):
                inventory.observe([path])
            self.assertEqual(inventory.files(), first)
            path.unlink()
            unreadable = inventory.snapshot()
            self.assertEqual(unreadable["status"], "unreadable")
            self.assertEqual(unreadable["errors"][0]["path"], str(path))

    def test_frozen_input_count_and_aggregate_bounds_preserve_prior_inventory(self):
        with tempfile.TemporaryDirectory() as directory:
            paths = [Path(directory) / name for name in ("program", "solutions")]
            for path in paths:
                path.write_bytes(b"abc")
            for inventory in (FrozenFiles(max_files=1), FrozenFiles(max_bytes=5)):
                first = inventory.observe(paths[:1])
                with self.assertRaises(ValueError):
                    inventory.observe(paths)
                self.assertEqual(inventory.files(), first)
            inventory = FrozenFiles(max_bytes=5)
            first = inventory.observe(paths[:1])
            original_open, opened = Path.open, []
            def recording_open(path, *args, **kwargs):
                opened.append(str(path))
                return original_open(path, *args, **kwargs)
            with patch("registry_codegen_evidence.hashlib.sha256", wraps=hashlib.sha256) as hash_reader, \
                    patch("registry_codegen_evidence.Path.open", new=recording_open):
                with self.assertRaises(ValueError):
                    inventory.observe(paths)
                # The first three-byte input is rechecked. The second is
                # rejected by metadata before a hash state/read is created.
                self.assertEqual(hash_reader.call_count, 1)
                self.assertEqual(opened, [str(paths[0])])
            self.assertEqual(inventory.files(), first)
            exact = FrozenFiles(max_bytes=6)
            self.assertEqual(sum(row["bytes"] for row in exact.observe(paths)), 6)
            self.assertEqual(exact.snapshot()["status"], "matched")
            with self.assertRaises(ValueError):
                FrozenFiles().observe([Path("relative")])
            with self.assertRaisesRegex(ValueError, "path scan bound"):
                FrozenFiles().observe([paths[0]] * 129)

    def test_growing_identity_input_is_bounded_by_preflight_length(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "growing"
            path.write_bytes(b"abc")
            original_open = Path.open
            def grow_before_read(opened_path, *args, **kwargs):
                if opened_path == path and args == ("rb",):
                    with original_open(path, "ab") as stream:
                        stream.write(b"d")
                return original_open(opened_path, *args, **kwargs)
            with patch("registry_codegen_evidence.Path.open", new=grow_before_read), \
                    patch("registry_codegen_evidence.hashlib.sha256", wraps=hashlib.sha256) as hash_reader:
                with self.assertRaisesRegex(ValueError, "grew beyond declared length"):
                    FrozenFiles(max_bytes=3).observe([path])
                self.assertEqual(hash_reader.call_count, 1)

    def test_actual_feature_metadata_scope_and_retained_corruption_control(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "lib-chialisp_oxide.json"
            path.write_text(json.dumps({"features": '["default"]', "profile": 1}))
            checked = facade_feature_record(path)
            self.assertEqual(checked["features"], ["default"])
            inventory = FrozenFiles()
            self.assertEqual(inventory.observe([path]), [checked["file"]])
            path.write_text(json.dumps({"features": '["default"]', "profile": 2}))
            self.assertEqual(facade_feature_record(path)["features"], ["default"])
            self.assertEqual(inventory.snapshot()["status"], "changed")
            for features in ('["default", "vm"]', '["alloc", "default"]', '["default", "default"]', '[]'):
                path.write_text(json.dumps({"features": features}))
                with self.subTest(features=features), self.assertRaises(ValueError):
                    facade_feature_record(path)

    def test_closed_constructed_apply_and_discard_are_distinct(self):
        result = Construction(constructed_envelope(), "atom255", b"\x01", None).inspect()
        self.assertEqual(result["apply_count"], 1)
        self.assertFalse(result["discarded_result"])
        discarded = Construction(constructed_envelope(discarded=True), "strict_unused_apply", b"\x01", None).inspect(True)
        self.assertTrue(discarded["discarded_result"])
        with self.assertRaises(ValueError):
            Construction(constructed_envelope(discarded=True), "strict_unused_apply", b"\x01", None).inspect(False)

    def test_wrong_module_environment_and_unknown_operation_fail_closed(self):
        with self.assertRaises(ValueError):
            Construction(constructed_envelope(), "atom255", b"\x02", None).inspect()
        for operation in (atom(b""), {"op": "raise"}, {"op": "arg", "index": True}):
            changed = constructed_envelope()
            changed["program"]["functions"][0]["body"]["value"]["environment"] = operation
            with self.subTest(operation=operation), self.assertRaises(ValueError):
                Construction(changed, "atom255", b"\x01", None).inspect()

    def test_unknown_dead_function_and_recursive_calls_are_rejected(self):
        changed = constructed_envelope()
        extra = copy.deepcopy(changed["program"]["functions"][0])
        extra.update(id=1, name=CRATE + "::unexpected_dead_helper")
        changed["program"]["functions"].append(extra)
        with self.assertRaises(ValueError):
            Construction(changed, "atom255", b"\x01", None).inspect()
        changed = constructed_envelope()
        changed["program"]["functions"][0]["body"]["value"] = {
            "op": "call", "function_id": 0, "arguments": [{"op": "arg", "index": 0}]}
        with self.assertRaises(ValueError):
            Construction(changed, "atom255", b"\x01", None).inspect()


if __name__ == "__main__":
    unittest.main()
