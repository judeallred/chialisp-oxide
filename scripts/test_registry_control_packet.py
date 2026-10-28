"""Pure registry transport boundaries; optional dynamic retained-packet controls."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts'))
import registry_control_packet as packet

class TransportBoundaries(unittest.TestCase):

    def minimal(self, directory):
        origins = {key: '/opaque/' + key for key in packet.ORIGINS if key != 'drafts'}
        origins['drafts'] = ['/opaque/draft-one.py', '/opaque/draft-two.py', '/opaque/draft-three.py']
        value = {'protocol': packet.PROTOCOL, 'profile': dict(packet.PROFILE), 'origins': origins, 'files': [], 'aliases': {}, 'scope': packet.SCOPE}
        (directory / 'report.json').write_bytes(packet.encode(value))
        return value

    def test_paths_preserve_normalized_containment(self):
        self.assertEqual(packet.relative('capture/bundle-one/export.ir.json'), 'capture/bundle-one/export.ir.json')
        for value in ('../escape', '/absolute', 'a//b', 'a/../b', 'a/./b', '', 'a\\b'):
            with self.subTest(value=value), self.assertRaises(ValueError):
                packet.relative(value)

    def test_native_zero_counter_is_not_false(self):
        self.assertFalse(packet.strict_equal({'executions': 0}, {'executions': False}))
        self.assertTrue(packet.strict_equal({'executions': 0}, {'executions': 0}))

    def test_production_and_historical_origin_shapes_are_distinct(self):
        with tempfile.TemporaryDirectory(prefix='registry-versioned-origins-') as name:
            directory = Path(name)
            value = self.minimal(directory)
            value['protocol'] = packet.PRODUCTION_PROTOCOL
            (directory / 'report.json').write_bytes(packet.encode(value))
            with self.assertRaisesRegex(ValueError, 'versioned opaque origins'):
                packet.Packet(directory)
            value['origins']['capture_sources'] = value['origins'].pop('drafts')
            (directory / 'report.json').write_bytes(packet.encode(value))
            admitted = packet.Packet(directory)
            self.assertEqual(admitted.code_key, 'capture_sources')
            self.assertEqual(admitted.code_prefix, 'capture-sources')

    def test_parent_aliases_are_canonicalized(self):
        with tempfile.TemporaryDirectory(prefix='registry-root-alias-') as name:
            directory = Path(name)
            self.minimal(directory)
            admitted = packet.Packet(directory)
            self.assertEqual(admitted.root, directory.resolve())

    def test_symlink_root_fails_before_index_read(self):
        with tempfile.TemporaryDirectory(prefix='registry-symlink-root-') as name:
            parent = Path(name)
            target = parent / 'target'
            target.mkdir()
            self.minimal(target)
            link = parent / 'alias'
            link.symlink_to(target, target_is_directory=True)
            original = packet.regular_read
            seen = []

            def forbidden(*args, **kwargs):
                seen.append(True)
                raise AssertionError('index read occurred before root rejection')
            packet.regular_read = forbidden
            try:
                with self.assertRaisesRegex(ValueError, 'regular packet root'):
                    packet.Packet(link)
            finally:
                packet.regular_read = original
            self.assertEqual(seen, [])

    def test_profile_types_and_physical_inventory(self):
        with tempfile.TemporaryDirectory(prefix='registry-closed-profile-') as name:
            directory = Path(name)
            value = self.minimal(directory)
            packet.Packet(directory)
            value['profile']['max_files'] = True
            (directory / 'report.json').write_bytes(packet.encode(value))
            with self.assertRaises(ValueError):
                packet.Packet(directory)
            self.minimal(directory)
            (directory / 'undeclared').write_bytes(b'')
            with self.assertRaisesRegex(ValueError, 'physical inventory'):
                packet.Packet(directory)

    def test_actual_hardlinked_host_executable_is_not_a_packet_member(self):
        with tempfile.TemporaryDirectory(prefix='registry-host-hardlink-') as name:
            parent = Path(name)
            executable = parent / 'python-binary'
            shutil.copy2(Path(sys.executable).resolve(strict=True), executable)
            directory = parent / 'packet'
            directory.mkdir()
            hardlink = directory / 'member'
            os.link(executable, hardlink)
            self.assertEqual(executable.stat().st_nlink, 2)
            actual = packet.executable_identity(hardlink)
            expected = executable.read_bytes()
            self.assertEqual(actual, {'path': str(hardlink), 'bytes': len(expected),
                                     'sha256': hashlib.sha256(expected).hexdigest()})
            with self.assertRaisesRegex(ValueError, 'unaliased'):
                packet.regular_read(hardlink, packet.MAX_EXECUTABLE)
            # Host Python binaries can exceed MAX_FILE. Use a bounded payload
            # so packet admission reaches the independent hardlink check.
            hardlink.unlink()
            payload = parent / 'bounded-payload'
            payload_bytes = b'bounded packet payload'
            payload.write_bytes(payload_bytes)
            os.link(payload, hardlink)
            index = self.minimal(directory)
            index['files'] = [{'path': 'member', **packet.identity(payload_bytes)}]
            (directory / 'report.json').write_bytes(packet.encode(index))
            with self.assertRaisesRegex(ValueError, 'unaliased packet member'):
                packet.Packet(directory)

    def test_executable_ceiling_is_separate_from_unchanged_packet_member_limit(self):
        self.assertEqual(packet.PROFILE, {'max_files': 1024, 'max_file_bytes': 2 * 1024 * 1024,
                                         'max_total_bytes': 16 * 1024 * 1024})
        self.assertEqual(packet.MAX_EXECUTABLE, 512 * 1024 * 1024)
        with tempfile.TemporaryDirectory(prefix='registry-host-large-') as name:
            executable = Path(name) / 'large-script'
            data = b'#!/bin/sh\nexit 0\n#' + b'x' * packet.MAX_FILE
            executable.write_bytes(data)
            executable.chmod(0o700)
            self.assertEqual(packet.executable_identity(executable)['sha256'], hashlib.sha256(data).hexdigest())
            with self.assertRaisesRegex(ValueError, 'bounded regular unaliased file'):
                packet.regular_read(executable)

    def test_executable_declared_oversize_is_rejected_before_hash_or_read(self):
        with tempfile.TemporaryDirectory(prefix='registry-host-oversize-') as name:
            executable = Path(name) / 'sparse-oversized-binary'
            with executable.open('wb') as stream:
                stream.truncate(packet.MAX_EXECUTABLE + 1)
            with patch.object(packet.os, 'read', side_effect=AssertionError('read before size admission')) as reads, \
                 patch.object(packet.hashlib, 'sha256', side_effect=AssertionError('hash before size admission')) as hashes:
                with self.assertRaisesRegex(ValueError, 'before hashing'):
                    packet.executable_identity(executable)
                reads.assert_not_called()
                hashes.assert_not_called()

    def test_executable_caller_cannot_increase_or_booleanize_byte_ceiling(self):
        with patch.object(packet.os, 'open', side_effect=AssertionError('open before ceiling admission')) as opened:
            for maximum in (True, 0, -1, packet.MAX_EXECUTABLE + 1):
                with self.subTest(maximum=maximum), self.assertRaisesRegex(ValueError, 'fixed host executable byte ceiling'):
                    packet.executable_identity(Path('/not-opened'), maximum)
            opened.assert_not_called()

    def test_executable_fifo_refusal_is_nonblocking_and_never_reads_it(self):
        with tempfile.TemporaryDirectory(prefix='registry-host-fifo-') as name:
            fifo = Path(name) / 'fifo'
            os.mkfifo(fifo)
            script = """import sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
import registry_control_packet as packet
try:
    packet.executable_identity(Path(sys.argv[2]))
except ValueError:
    print('refused_nonregular')
else:
    raise SystemExit('FIFO admitted')
"""
            result = subprocess.run([sys.executable, '-B', '-c', script, str(ROOT / 'scripts'), str(fifo)],
                                    capture_output=True, timeout=2, check=True)
            self.assertEqual(result.stdout, b'refused_nonregular\n')
            self.assertEqual(result.stderr, b'')

    def test_executable_explicit_alias_selection_resolves_once_before_nofollow_read(self):
        with tempfile.TemporaryDirectory(prefix='registry-host-alias-') as name:
            parent = Path(name)
            physical = parent / 'physical'
            physical.mkdir()
            executable = physical / 'cli'
            executable.write_bytes(b'#!/bin/sh\nexit 0\n')
            executable.chmod(0o700)
            link = parent / 'selected-cli'
            link.symlink_to(executable)
            with self.assertRaises(OSError):
                packet.executable_identity(link)
            # replay_templates preserves explicit caller selection: aliases
            # resolve to one canonical path before the descriptor-only role.
            selected = link.resolve(strict=True)
            expected = packet.executable_identity(executable.resolve(strict=True))
            self.assertEqual(packet.executable_identity(selected), expected)
            directory_alias = parent / 'selected-directory'
            directory_alias.symlink_to(physical, target_is_directory=True)
            self.assertEqual(packet.executable_identity((directory_alias / 'cli').resolve(strict=True)), expected)

    def test_executable_descriptor_detects_same_size_changes_during_hashing(self):
        with tempfile.TemporaryDirectory(prefix='registry-host-changed-') as name:
            executable = Path(name) / 'cli'
            executable.write_bytes(b'original')
            before = executable.stat()
            original_read = os.read
            changed = False
            def mutate_after_read(descriptor, maximum):
                nonlocal changed
                data = original_read(descriptor, maximum)
                if data and not changed:
                    changed = True
                    executable.write_bytes(b'mutated!')
                    os.utime(executable, ns=(before.st_atime_ns, before.st_mtime_ns + 1_000_000_000))
                return data
            with patch.object(packet.os, 'read', side_effect=mutate_after_read):
                with self.assertRaisesRegex(ValueError, 'changed during bounded hashing'):
                    packet.executable_identity(executable)
            self.assertTrue(changed)

    def test_executable_leaf_swapped_after_explicit_resolution_is_not_followed(self):
        with tempfile.TemporaryDirectory(prefix='registry-host-swapped-') as name:
            parent = Path(name)
            selected = parent / 'selected'
            other = parent / 'other'
            selected.write_bytes(b'selected')
            other.write_bytes(b'not-selected')
            resolved = selected.resolve(strict=True)
            original_open = os.open
            def swap_leaf(path, flags):
                self.assertTrue(flags & os.O_NONBLOCK)
                self.assertTrue(flags & os.O_NOFOLLOW)
                selected.unlink()
                selected.symlink_to(other)
                return original_open(path, flags)
            with patch.object(packet.os, 'open', side_effect=swap_leaf), \
                 patch.object(packet.hashlib, 'sha256', side_effect=AssertionError('hashed replacement symlink')) as hashes:
                with self.assertRaises(OSError):
                    packet.executable_identity(resolved)
                hashes.assert_not_called()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--packet', type=Path, help='Also run complete recorded admission and 25 dynamic packet corruption controls; adds no execution credit')
    args = parser.parse_args()
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(TransportBoundaries))
    if not result.wasSuccessful():
        raise SystemExit(1)
    report = {'pure_unit_tests': result.testsRun, 'scope': 'Pure transport boundary tests only; no compiler, native, VM, SDK or authentication credit.'}
    if args.packet is not None:
        report['packet_controls'] = packet.corruption_controls(args.packet)
    else:
        report['packet_controls'] = 'not requested; use --packet PATH or the admitted CLI --self-test gate'
    print(json.dumps(report))
if __name__ == '__main__':
    main()
