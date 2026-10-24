"""Constructed controls for the independent paired main-journal reader."""
import json
import os
from pathlib import Path
import unittest
from unittest.mock import patch

import counter_protocol as reader


class RetainedCounterProtocol(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        path = Path(os.environ.get("COUNTER_PROTOCOL_REPORT", reader.ROOT / "corpus/validation.json"))
        cls.report = json.loads(reader.oracle.bounded_read(path, 64 * reader.codec.MAX_BYTES))
        if not any(row.get("slug") in reader.SLUGS for row in cls.report["puzzles"]):
            raise unittest.SkipTest("No counter journals in this retained report")
        cls.partial = len(cls.report["puzzles"]) == 2

    def test_complete_reader_does_not_execute_processes(self):
        with patch("subprocess.Popen", side_effect=AssertionError("reader launched a producer")):
            counts = reader.validate_report(self.report, partial=self.partial)
        self.assertEqual(sum(c["sdk_transaction_attempts"] for c in counts["entries"].values()), 60)
        self.assertEqual(sum(c["component_primary_vm_runs"] for c in counts["entries"].values()), 70)

    def test_constructed_corruptions(self):
        self.assertEqual(reader.self_test(self.report, partial=self.partial), 16)

    def test_boolean_cannot_replace_integer_protocol_fields(self):
        with self.assertRaises(ValueError):
            reader.exact(True, 1, "version type")
        with self.assertRaises(ValueError):
            reader.exact(1, True, "state equality type")


if __name__ == "__main__":
    unittest.main()
