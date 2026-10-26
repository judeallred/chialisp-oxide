"""Pure registry transport boundaries; optional dynamic retained-packet controls."""
import argparse
import json
from pathlib import Path
import sys
import tempfile
import unittest
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
