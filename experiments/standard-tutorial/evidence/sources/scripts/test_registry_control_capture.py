"""Pure capture preflight controls; no compiler, VM or SDK execution."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import registry_control_capture as capture
import precommit_source_controls


class CapturePreflight(unittest.TestCase):
    def test_bytecode_cannot_enter_native_window_or_start_a_command(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            for filename in ('puzzles/__pycache__/vectors.cpython-314.pyc', 'puzzles/vectors.pyo'):
                with self.subTest(filename=filename), \
                     patch.object(precommit_source_controls, 'ROOT', root), \
                     patch.object(precommit_source_controls, 'source_window', return_value={'sources':[]}), \
                     patch.object(capture, 'source_files', return_value=[filename]), \
                     patch.object(capture, 'read') as read, \
                     patch.object(capture.subprocess, 'Popen') as command:
                    with self.assertRaisesRegex(ValueError, 'generated Python bytecode'):
                        capture.full_sources(root)
                    read.assert_not_called()
                    command.assert_not_called()

    def test_source_inside_a_cache_named_directory_is_still_hashed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            name = 'puzzles/__pycache__/policy.rs'
            for filename in (name, 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo/config.toml'):
                path = root / filename
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b'actual declared source\n')
            with patch.object(precommit_source_controls, 'ROOT', root), \
                 patch.object(precommit_source_controls, 'source_window', return_value={'sources':[]}), \
                 patch.object(capture, 'source_files', return_value=[name]):
                rows = capture.full_sources(root)
                retained = next(row for row in rows if row['path'] == name)
                self.assertEqual(retained['sha256'], capture.digest((root / name).read_bytes()))

    def test_exact_regular_file_bound(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'input'
            path.write_bytes(b'1234')
            self.assertEqual(capture.read(path, 4), b'1234')
            with self.assertRaises(ValueError):
                capture.read(path, 3)
            alias = path.with_name('alias')
            alias.symlink_to(path)
            with self.assertRaises(ValueError):
                capture.read(alias, 4)

    @unittest.skipUnless(hasattr(os, 'mkfifo'), 'Unix regular-file preflight')
    def test_fifo_rejected_without_waiting_for_a_writer(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'fifo'
            os.mkfifo(path)
            command = [sys.executable, '-B', '-c',
                'import registry_control_capture as c; import sys; c.read(sys.argv[1])', str(path)]
            result = subprocess.run(command, cwd=Path(__file__).parent,
                capture_output=True, timeout=5, check=False)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b'bounded regular input', result.stderr)

    def test_unreviewed_lock_stops_before_any_native_command(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with patch.object(capture, 'environment', return_value={'RUSTUP_TOOLCHAIN':'test'}), \
                 patch.object(capture, 'full_sources', return_value=[]), \
                 patch.object(capture, 'compiler_tools', return_value=[]), \
                 patch.object(capture, 'lock_admission', side_effect=ValueError('unreviewed pins')), \
                 patch.object(capture.subprocess, 'Popen') as command:
                with self.assertRaisesRegex(ValueError, 'unreviewed pins'):
                    capture.run_phase(root, root/'target/cache', root/'target/run', 'native')
                command.assert_not_called()


if __name__ == '__main__':
    unittest.main()
