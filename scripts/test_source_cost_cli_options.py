"""Actual pure CLI controls and constructed process/source-window controls.

No Cargo, native policy, compiler, VM, SDK or current23 admission is executed.
"""
from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import runpy
from types import SimpleNamespace
import sys
import tempfile
import unittest
from unittest.mock import patch

import runtime_cost_migration as cost
import runtime_composition_migration as composition
import p2dc_append_migration as outer


class HistoricalOptions(unittest.TestCase):
    def invoke(self, *options):
        path = cost.ROOT / "scripts/check-p2-source-cost.py"
        with patch.object(sys, "argv", [str(path), "--checkpoint-only", *options]), \
                patch.object(cost, "validate_current", side_effect=AssertionError("current on history")), \
                patch("subprocess.run", side_effect=AssertionError("no process")), \
                patch("subprocess.Popen", side_effect=AssertionError("no process")), \
                redirect_stdout(io.StringIO()) as output:
            try:
                runpy.run_path(str(path), run_name="__main__")
            except SystemExit as result:
                self.assertEqual(result.code, 0)
        return output.getvalue()

    def test_real_pinned_unsigned_checkpoint_and_signed_anchor_are_pure(self):
        self.assertIn(composition.INTERMEDIATE["report_sha256"], self.invoke())

    def test_each_fresh_or_execution_override_rejects_before_historical_read(self):
        for options in (
            ["--fresh-cargo-option=--offline"],
            ["--fresh-cargo-option=--manifest-path", "--fresh-cargo-option=puzzles/Cargo.toml"],
            ["--fresh-on=constructed-on"], ["--fresh-off=constructed-off"],
            ["--execute-costs"], ["--execution-output=constructed-output"],
            ["--report=constructed-report"],
        ):
            with self.subTest(options=options), \
                    patch.object(composition, "checkpoint_recorded", side_effect=AssertionError("read before option fence")), \
                    self.assertRaisesRegex(ValueError, "accepts no report/fresh/execution overrides"):
                self.invoke(*options)


class ConstructedExecutionWindow(unittest.TestCase):
    def test_active_outer_reader_mutation_is_detected_without_running_a_tool(self):
        gate = runpy.run_path(str(cost.ROOT / "scripts/check-p2-source-cost.py"))
        report = json.loads((cost.ROOT / "corpus/checkpoints/runtime-composition-validation.json").read_bytes())
        metrics = (cost.ROOT / "corpus/runtime-composition-costs.json").read_bytes()
        active = Path(outer.__file__)
        original = active.read_bytes()
        with tempfile.TemporaryDirectory(prefix="constructed-metric-window-") as folder:
            location = Path(folder)
            runner = location / "runner"
            runner.write_bytes(b"constructed identity; never executed")

            def respond(argv, **kwargs):
                self.assertEqual(argv[0], str(runner.resolve()))
                Path(argv[2]).write_bytes(metrics)
                return SimpleNamespace(returncode=0, stdout="", stderr="")

            # The controlled response checks only the execution reader. It is
            # not an actual conditions-helper run or a current23 positive.
            with patch("subprocess.run", side_effect=respond), \
                    patch("subprocess.Popen", side_effect=AssertionError("no process")), \
                    patch.object(outer, "read", return_value=metrics):
                self.assertEqual(gate["execute_costs"](report, runner, location / "positive")["accepted_bundle_metrics"], 15)

                def changed(argv, **kwargs):
                    result = respond(argv, **kwargs)
                    active.write_bytes(original + b"\n# constructed source-window corruption\n")
                    return result

                try:
                    with patch("subprocess.run", side_effect=changed), \
                            self.assertRaisesRegex(ValueError, "source/tool window changed"):
                        gate["execute_costs"](report, runner, location / "negative")
                finally:
                    active.write_bytes(original)
        self.assertEqual(active.read_bytes(), original)


if __name__ == "__main__":
    unittest.main()
