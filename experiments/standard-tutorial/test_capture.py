"""Pure compiler-format controls; no compiler, SDK or capture execution credit."""
import unittest
import hashlib
from unittest.mock import patch

import capture
import packet


SHORT = 'rustc 1.99.0-nightly (daf2e5e18 2026-07-13)'
VERBOSE = SHORT + '\nbinary: rustc\ncommit-hash: daf2e5e18b3abc4311d9da3d96fd64138f43d508\nhost: aarch64-apple-darwin\n'


class CompilerIdentity(unittest.TestCase):
    def test_short_driver_matches_independent_short_and_verbose_header(self):
        self.assertNotEqual(SHORT, VERBOSE.strip())
        with patch.object(capture.subprocess, 'check_output', side_effect=AssertionError('pure helper must not execute tools')):
            self.assertIsNone(capture.check_compiler_identity(VERBOSE, SHORT + '\n', SHORT + '\n'))
        self.assertIn('host: aarch64-apple-darwin\n', VERBOSE)

    def test_single_line_outputs_allow_normal_line_endings(self):
        for ending in ('', '\n', '\r\n'):
            with self.subTest(ending=repr(ending)):
                capture.check_compiler_identity(VERBOSE.replace('\n', '\r\n'), SHORT + ending, SHORT + ending)

    def test_every_independent_identity_mismatch_fails(self):
        other = SHORT.replace('daf2e5e18', '000000000')
        for values in ((VERBOSE, SHORT, other), (VERBOSE, other, other),
                       (VERBOSE.replace(SHORT, other), SHORT, SHORT)):
            with self.subTest(values=values), self.assertRaises(ValueError):
                capture.check_compiler_identity(*values)

    def test_empty_multiline_and_padded_formats_fail_closed(self):
        for short in ('', '\n', 'rustc ', ' ' + SHORT, SHORT + ' ', SHORT + '\nextra', SHORT + '\n\n'):
            with self.subTest(short=repr(short)), self.assertRaises(ValueError):
                capture.check_compiler_identity(VERBOSE, short, SHORT)
        for driver in ('', SHORT + '\nextra', SHORT + '\n\n', ' ' + SHORT, None):
            with self.subTest(driver=driver), self.assertRaises(ValueError):
                capture.check_compiler_identity(VERBOSE, SHORT, driver)
        with self.assertRaises(ValueError):
            capture.check_compiler_identity('\n' + VERBOSE, SHORT, SHORT)


class CargoOptions(unittest.TestCase):
    def test_exact_current_and_two_reviewed_old_forms(self):
        origin = '/opaque/producer'
        for options in (
            ['--locked', '--manifest-path', origin + '/puzzles/Cargo.toml'],
            ['--manifest-path', origin + '/puzzles/Cargo.toml', '--locked', '--offline'],
            ['--manifest-path', 'puzzles/Cargo.toml', '--package', 'chialisp-oxide-puzzles', '--locked'],
        ):
            with self.subTest(options=options):
                packet.check_cargo_options(options, origin)

    def test_wrong_paths_packages_lock_flags_and_order_fail(self):
        actual = ['--manifest-path', 'puzzles/Cargo.toml', '--package', 'chialisp-oxide-puzzles', '--locked']
        bad = [
            actual[:3] + ['other-package', '--locked'],
            ['--manifest-path', '../puzzles/Cargo.toml'] + actual[2:],
            ['--manifest-path', './puzzles/Cargo.toml'] + actual[2:],
            ['--manifest-path', '/elsewhere/puzzles/Cargo.toml'] + actual[2:],
            actual[:-1], actual + ['--offline'], actual + ['--features', 'alloc'],
            actual + ['--locked'], actual[2:4] + actual[:2] + actual[4:],
            tuple(actual),
        ]
        for options in bad:
            with self.subTest(options=options), self.assertRaises(ValueError):
                packet.check_cargo_options(options, '/opaque/producer')


class CompiledInventoryWire(unittest.TestCase):
    @staticmethod
    def artifact(raw, tree_hash):
        return dict(backend='rust_mir', optimize_lir=False, bytes=len(raw),
                    clvm_hex=raw.hex(), tree_hash=tree_hash.hex(), disassembly='fixture')

    @staticmethod
    def chain(depth):
        empty = hashlib.sha256(b'\x01').digest()
        value = empty
        for _ in range(depth):
            value = hashlib.sha256(b'\x02' + empty + value).digest()
        return b'\xff\x80' * depth + b'\x80', value

    def test_package_depth_is_distinct_from_selected_sdk_depth(self):
        raw, tree_hash = self.chain(300)
        artifact = self.artifact(raw, tree_hash)
        with patch.object(packet, 'inspect_compiled_artifact', wraps=packet.inspect_compiled_artifact) as inspect:
            self.assertEqual(packet.compiled_inventory_wire(artifact, 'complete'), raw)
        inspect.assert_called_once_with(raw, max_bytes=256 * 1024, max_nodes=65536, max_depth=4096)
        self.assertEqual(packet.sdk_reader.MAX_DEPTH, 256)
        with self.assertRaises(ValueError):
            packet.artifact_wire(artifact, 'selected SDK')

    def test_existing_byte_node_and_package_depth_caps_still_reject(self):
        raw, tree_hash = self.chain(4096)
        with self.assertRaises(ValueError):
            packet.compiled_inventory_wire(self.artifact(raw, tree_hash), 'depth')
        raw = b'\x80'
        tree_hash = hashlib.sha256(b'\x01').digest()
        for _ in range(16):
            raw = b'\xff' + raw + raw
            tree_hash = hashlib.sha256(b'\x02' + tree_hash + tree_hash).digest()
        self.assertLess(len(raw), 256 * 1024)
        with self.assertRaises(ValueError):
            packet.compiled_inventory_wire(self.artifact(raw, tree_hash), 'nodes')
        too_large = self.artifact(b'\x80', hashlib.sha256(b'\x01').digest())
        too_large['bytes'] = 256 * 1024 + 1
        with patch.object(packet, 'inspect_compiled_artifact', side_effect=AssertionError('byte precharge first')):
            with self.assertRaises(ValueError):
                packet.compiled_inventory_wire(too_large, 'bytes')

    def test_noncanonical_truncated_trailing_and_wrong_hash_fail(self):
        for raw in (b'\x81\x00', b'\xff\x80', b'\x80\x80', b'\xfe\x80'):
            with self.subTest(raw=raw), self.assertRaises(ValueError):
                packet.compiled_inventory_wire(self.artifact(raw, bytes(32)), 'bad wire')
        with self.assertRaises(ValueError):
            packet.compiled_inventory_wire(self.artifact(b'\x80', bytes(32)), 'wrong tree hash')


if __name__ == '__main__':
    unittest.main()
