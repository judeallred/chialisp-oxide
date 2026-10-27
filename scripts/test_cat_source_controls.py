#!/usr/bin/env python3
"""Pure portable transport controls; actual packet semantics need fresh capture."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import cat_source_controls as c


class TransportTests(unittest.TestCase):
    def test_relative_paths_are_closed_ascii(self):
        for name in ("../x", "/x", "a//b", "a\\b", "é", "a/./b", "a:b", ""):
            with self.subTest(name=name), self.assertRaises(ValueError): c.relative(name)
        self.assertEqual(c.relative("bundle-off/source-12.bin"), "bundle-off/source-12.bin")

    def test_aggregate_is_reserved_before_hashing(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ("a", "b"): (root / name).write_bytes(b"12")
            with patch.object(c, "MAX_TOTAL", 3), patch.object(c, "digest", side_effect=AssertionError("hash before reserve")):
                with self.assertRaises(ValueError): c.inventory([root / "a", root / "b"], root)

    def test_per_file_is_reserved_before_hashing(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); (root / "a").write_bytes(b"123")
            with patch.object(c, "MAX_FILE", 2), patch.object(c, "digest", side_effect=AssertionError("hash before reserve")):
                with self.assertRaises(ValueError): c.inventory([root / "a"], root)

    def test_empty_directories_are_charged(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for i in range(129): (root / str(i)).mkdir()
            with self.assertRaises(ValueError): c.files(root)

    def test_unlisted_empty_directory_is_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); (root / "unused").mkdir()
            with self.assertRaises(ValueError): c.files(root)

    def test_depth_is_bounded(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); path = root
            for _ in range(7): path = path / "d"; path.mkdir()
            with self.assertRaises(ValueError): c.files(root)

    def test_regular_file_count_is_bounded(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for i in range(129): (root / str(i)).write_bytes(b"")
            with self.assertRaises(ValueError): c.files(root)

    def test_symlink_and_hardlink_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); (root / "a").write_bytes(b"x")
            (root / "b").symlink_to("a")
            with self.assertRaises(ValueError): c.files(root)
            (root / "b").unlink(); os.link(root / "a", root / "b")
            with self.assertRaises(ValueError): c.files(root)

    def test_origin_is_only_a_closed_relation(self):
        self.assertEqual(c.origin_name("/old/capture/project/Cargo.toml", "/old/capture"), "project/Cargo.toml")
        with self.assertRaises(ValueError): c.origin_name("/old/capture-evil/project", "/old/capture")
        with self.assertRaises(ValueError): c.origin_name("/old/capture/../secret", "/old/capture")

    def test_structural_download_listing_executes_nothing(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); (root / "capture.json").write_text("{}")
            packet = {"protocol": c.PROTOCOL, "scope": "recorded_consistency_only", "origin_capture": "/opaque/origin",
                      "files": c.inventory([root / "capture.json"], root),
                      "aliases": {"rust-lifecycle-attempt1.stdout": "capture.json", "rust-lifecycle-attempt2.stdout": "capture.json", "rust-lifecycle-attempt2.stderr": "rust-lifecycle-attempt1.stderr"}}
            (root / "report.json").write_text(json.dumps(packet))
            with patch("subprocess.run", side_effect=AssertionError("process")), patch("subprocess.Popen", side_effect=AssertionError("process")), patch.object(c.capture, "quiet", side_effect=AssertionError("producer")), patch.object(c.capture, "source_input_paths", side_effect=AssertionError("current source")):
                self.assertEqual(c.recorded_packet_paths(root), ("capture.json", "report.json"))
            (root / "capture.json").write_text("bad")
            with self.assertRaises(ValueError): c.recorded_packet_paths(root)

    def test_lock_resolution_cannot_change_external_pins(self):
        root = b'''[[package]]
name="dep"
version="1.0.0"
source="registry+https://example.invalid/index"
checksum="1234"
'''
        local = ('[[package]]\nname="' + c.capture.PACKAGE + '"\nversion="0.0.0"\n').encode()
        c.capture.validate_lock(root, root + local)
        for old, new in ((b"1.0.0", b"1.1.0"), (b"1234", b"5678"),
                         (b"https://example.invalid/index", b"https://other.invalid/index"),
                         (b'name="dep"', b'name="other"')):
            with self.subTest(old=old), self.assertRaises(ValueError):
                c.capture.validate_lock(root, root.replace(old, new) + local)

    def test_independent_compiler_host_requires_exactly_one_line(self):
        self.assertEqual(c.compiler_host("rustc pinned\nhost: aarch64-apple-darwin\n"), "aarch64-apple-darwin")
        for declaration in ("rustc pinned", "host: ", "host: x\nhost: y", "x" * 16385):
            with self.subTest(declaration=declaration[:30]), self.assertRaises(ValueError):
                c.compiler_host(declaration)

    def test_generated_inputs_are_exact_independent_of_refreshed_hashes(self):
        expected = c.capture.expected_project_inputs("/opaque/source-origin")
        with tempfile.TemporaryDirectory() as directory:
            project = Path(directory)
            for name, raw in expected.items():
                path = project / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(raw)
            c.capture.check_project_inputs(project, "/opaque/source-origin")
            for name, raw in expected.items():
                with self.subTest(name=name):
                    (project / name).write_bytes(b"replacement input")
                    # Recomputed digests cannot make the input match construction.
                    self.assertNotEqual(c.digest(raw), c.digest((project / name).read_bytes()))
                    with self.assertRaises(ValueError):
                        c.capture.check_project_inputs(project, "/opaque/source-origin")
                    (project / name).write_bytes(raw)

    def test_corruption_controls_reach_validator_near_aggregate_bound(self):
        # Use the retained packet's real nested report shape. Only the semantic
        # validator is replaced: inventory construction and every mutation run.
        # The original pretty rewrites overflow this admitted-size budget.
        root = c.ROOT / "corpus/cat-genesis-source-controls"
        original_bytes = sum(path.stat().st_size for path in c.files(root))
        calls = []
        def semantic_validator(candidate):
            calls.append(candidate)
            if len(calls) == 1:
                return {}
            self.assertLessEqual(sum(path.stat().st_size for path in c.files(candidate)),
                                 original_bytes + 1024)
            raise ValueError("constructed semantic rejection")
        with patch.object(c, "MAX_TOTAL", original_bytes + 1024), \
                patch.object(c, "validate", side_effect=semantic_validator):
            controls = c.self_test(root)
        self.assertEqual(controls, 29)
        self.assertEqual(len(calls), controls + 1)

    def test_unknown_index_fields_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "report.json").write_text(json.dumps({"protocol": c.PROTOCOL, "extra": True}))
            with self.assertRaises(ValueError): c.recorded_packet_paths(root)


if __name__ == "__main__":
    unittest.main()
