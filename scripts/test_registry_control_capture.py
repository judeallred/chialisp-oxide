"""Pure capture preflight controls; no compiler, VM or SDK execution."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import registry_control_capture as capture


class CapturePreflight(unittest.TestCase):
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
