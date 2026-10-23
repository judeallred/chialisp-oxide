#!/usr/bin/env python3
"""Lightweight host controls only: no Cargo, driver, VM, SDK or network calls."""
import copy
import json
import os
from pathlib import Path
import signal
import sys
import tempfile
import time
import unittest
from unittest import mock

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
import rust_capture as capture
import rust_lifecycle as reader


class ProcessControls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="cat-capture-host-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def invoke(self, code, **options):
        return capture.command(self.root, "control", [sys.executable, "-c", code],
                               target=self.root / "unused-target", **options)

    def receipt(self):
        return json.loads((self.root / "control.command.json").read_text())

    def test_success_retains_exact_bounded_streams(self):
        self.assertEqual(self.invoke("import sys; print('out'); print('err',file=sys.stderr)", timeout=10), b"out\n")
        receipt = self.receipt()
        self.assertEqual(receipt["status"], "passed")
        self.assertIsNone(receipt["failure_kind"])
        self.assertEqual(receipt["stdout"], capture.identity(self.root / "control.stdout"))
        self.assertEqual(receipt["stderr"], capture.identity(self.root / "control.stderr"))

    def test_nonzero_and_spawn_failure_have_distinct_retained_receipts(self):
        with self.assertRaises(capture.CommandFailure) as error:
            self.invoke("import sys; print('failed'); sys.exit(7)", timeout=10)
        self.assertEqual(error.exception.kind, "nonzero_exit")
        self.assertEqual(self.receipt()["exit_code"], 7)
        for path in self.root.iterdir(): path.unlink()
        with self.assertRaises(capture.CommandFailure) as error:
            capture.command(self.root, "control", [self.root / "missing-executable"], target=self.root)
        self.assertEqual(error.exception.kind, "infrastructure")
        self.assertEqual(self.receipt()["stdout"]["bytes"], 0)
        self.assertIsNone(self.receipt()["exit_code"])

    def test_each_stream_is_capped_before_disk_write(self):
        for name in ("stdout", "stderr"):
            with self.subTest(stream=name):
                for path in self.root.iterdir(): path.unlink()
                with self.assertRaises(capture.CommandFailure) as error:
                    self.invoke(f"import sys,time; sys.{name}.buffer.write(b'x'*100000); sys.{name}.flush(); time.sleep(30)",
                                maximum=257, timeout=10)
                self.assertEqual(error.exception.kind, "output_limit")
                receipt = self.receipt()
                self.assertEqual(receipt["failure_detail"], name)
                self.assertEqual(receipt[name]["bytes"], 257)
                for stream in ("stdout", "stderr"):
                    self.assertLessEqual((self.root / ("control." + stream)).stat().st_size, 257)

    def descendant_control(self, early_exit):
        ready, heartbeat = self.root / "ready", self.root / "heartbeat"
        descendant = ("from pathlib import Path; import os,time; "
                      f"p=Path({str(heartbeat)!r}); Path({str(ready)!r}).write_text(str(os.getpid())); "
                      "\nwhile True:\n with p.open('ab') as f: f.write(b'x')\n time.sleep(.05)\n")
        parent = ("import subprocess,sys,time; from pathlib import Path; "
                  f"subprocess.Popen([sys.executable,'-c',{descendant!r}]); "
                  f"\nwhile not Path({str(heartbeat)!r}).exists(): time.sleep(.01)\n"
                  + ("sys.exit(0)" if early_exit else "time.sleep(30)"))
        try:
            if early_exit:
                self.assertEqual(self.invoke(parent, timeout=10), b"")
                self.assertEqual(self.receipt()["status"], "passed")
            else:
                with self.assertRaises(capture.CommandFailure) as error:
                    self.invoke(parent, timeout=3)
                self.assertEqual(error.exception.kind, "timeout")
            self.assertTrue(ready.exists(), "descendant handshake did not run")
            size = heartbeat.stat().st_size
            time.sleep(.3)
            self.assertGreater(size, 0)
            self.assertEqual(heartbeat.stat().st_size, size, "grandchild survived parent cleanup")
        finally:
            if ready.exists():
                try: os.kill(int(ready.read_text()), signal.SIGKILL)
                except ProcessLookupError: pass

    def test_timeout_kills_running_cargo_style_grandchild(self):
        self.descendant_control(False)

    def test_successful_parent_exit_still_kills_pipe_holding_grandchild(self):
        self.descendant_control(True)

    def test_invalid_limits_and_log_paths_start_no_process(self):
        with mock.patch.object(capture.subprocess, "Popen", side_effect=AssertionError("must not spawn")):
            for options in ({"timeout":True}, {"timeout":float("inf")}, {"maximum":True}, {"maximum":capture.MAX_FILE+1}):
                with self.assertRaises(ValueError): self.invoke("", **options)
            with self.assertRaises(ValueError):
                capture.command(self.root, "../escape", [], target=self.root)
        self.assertEqual(list(self.root.iterdir()), [])


class RetainedControls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="cat-retained-host-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "a").write_bytes(b"aaa")
        (self.root / "b").write_bytes(b"bbbb")

    def test_exact_aggregate_and_count_boundary(self):
        rows = capture.retained_identities(self.root, ["b", "a"], maximum=7, max_files=2)
        self.assertEqual([row["bytes"] for row in rows], [3, 4])
        self.assertEqual(sum(row["bytes"] for row in rows), 7)

    def test_all_stat_reservations_precede_any_reads_or_hashes(self):
        with mock.patch.object(capture, "read", side_effect=AssertionError("read before reservation")), \
             mock.patch.object(capture, "digest", side_effect=AssertionError("hash before reservation")):
            with self.assertRaisesRegex(ValueError, "aggregate"): capture.retained_identities(self.root, ["a", "b"], maximum=6)
            with self.assertRaisesRegex(ValueError, "count"): capture.retained_identities(self.root, ["a", "b"], max_files=1)
            with self.assertRaisesRegex(ValueError, "duplicates"): capture.retained_identities(self.root, ["a", "a"])
            with (self.root / "b").open("wb") as target: target.truncate(capture.MAX_FILE + 1)
            with self.assertRaisesRegex(ValueError, "aggregate"): capture.retained_identities(self.root, ["b"])

    def test_symlink_is_rejected_before_hash(self):
        (self.root / "alias").symlink_to(self.root / "a")
        with mock.patch.object(capture, "digest", side_effect=AssertionError("hash symlink")):
            with self.assertRaisesRegex(ValueError, "regular"): capture.retained_identities(self.root, ["alias"])

    def test_growth_or_shrink_after_preflight_fails_before_hash(self):
        original_read = capture.read
        for raw in (b"aaaa", b""):
            (self.root / "a").write_bytes(b"aaa")
            def changed(path, maximum):
                self.assertEqual(maximum, 3)
                path.write_bytes(raw)
                return original_read(path, maximum)
            with mock.patch.object(capture, "read", side_effect=changed), \
                 mock.patch.object(capture, "digest", side_effect=AssertionError("hash changed file")):
                with self.assertRaises(ValueError): capture.retained_identities(self.root, ["a"])

    def test_raw_tree_is_in_reviewed_reader_closure(self):
        with mock.patch.object(capture, "source_files", return_value=[]), \
             mock.patch.object(capture, "materialized", return_value=[]), \
             mock.patch.object(capture, "identity", side_effect=lambda path, *args: {"path":str(path)}):
            window = capture.quiet(self.root)
        self.assertIn(str(capture.ROOT / "scripts/raw_tree.py"), [row["path"] for row in window["sources"]])


class NativeReaderControls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="cat-native-reader-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.project = self.root / "project"
        self.expected = [{"control":"original","name":row["name"],"actual":row["expected"]}
                         for row in reader.vector_specification()["cases"]]
        self.expected += [{"control":control,"name":name,"actual":{"status":"accepted","output_hex":"80"}}
                          for control,name in (("drop_delta_guard","non-nil-delta-one"),("drop_genesis_equality","mismatched-parent"))]
        self.command = {
            "argv":["rustup","run",capture.TOOLCHAIN,"cargo","test","--manifest-path",str(self.project/"Cargo.toml"),"--locked","--test","native","--","--nocapture"],
            "cwd":str(capture.ROOT),"exit_code":0,"target":str(self.root/"native-target"),"jobs":2,"offline":True,
            "status":"passed","failure_kind":None,"failure_detail":None,"timeout_seconds":capture.COMMAND_TIMEOUT,
            "cleanup_timeout_seconds":capture.COMMAND_CLEANUP_TIMEOUT,"max_stream_bytes":capture.MAX_FILE,
        }
        # Constructed correspondence fixtures. No Cargo/native policy execution.
        self.native = {"status":"passed","native_test_groups":1,"observations":copy.deepcopy(self.expected),
                       "runner":{"path":str(self.root/"native-target/debug/examples/rust_lifecycle")}}
        self.save_logs(self.expected)

    def save_logs(self, observations, summary="1 passed; 0 failed; 0 ignored;"):
        stdout = "\n".join("CAT_GENESIS_RUST_NATIVE " + json.dumps(row) for row in observations)
        (self.root/"native-tests.stdout").write_text(stdout + "\ntest result: ok. " + summary + "\n")
        (self.root/"native-tests.stderr").write_text("")
        self.command.update({name:capture.identity(self.root/("native-tests."+name)) for name in ("stdout","stderr")})
        self.save_command()
        self.native["native_stdout"] = capture.identity(self.root/"native-tests.stdout")

    def save_command(self):
        (self.root/"native-tests.command.json").write_text(json.dumps(self.command))
        self.native["native_command"] = capture.identity(self.root/"native-tests.command.json")

    def test_valid_reader_runs_no_process(self):
        with mock.patch.object(capture.subprocess,"Popen",side_effect=AssertionError("no process")), \
             mock.patch.object(capture.subprocess,"run",side_effect=AssertionError("no process")):
            self.assertEqual(reader.validate_native(self.native,self.root,self.project),self.expected)

    def test_native_witness_corruption_fails_even_with_coherent_stdout_and_hashes(self):
        for index in (0,36,37,38):
            with self.subTest(index=index):
                changed = copy.deepcopy(self.expected)
                changed[index]["actual"] = {"status":"accepted","output_hex":"ff"}
                self.native["observations"] = changed
                self.save_logs(changed)
                with self.assertRaises(ValueError): reader.validate_native(self.native,self.root,self.project)

    def test_native_order_count_group_and_stdout_corruptions_fail(self):
        for change in (lambda n:n["observations"].reverse(),lambda n:n["observations"].pop(),
                       lambda n:n.__setitem__("native_test_groups",True)):
            bad = copy.deepcopy(self.native); change(bad)
            with self.assertRaises(ValueError): reader.validate_native(bad,self.root,self.project)
        changed = copy.deepcopy(self.expected); changed[-1]["actual"]={"status":"rejected"}
        self.save_logs(changed)
        with self.assertRaises(ValueError): reader.validate_native(self.native,self.root,self.project)
        self.save_logs(self.expected,summary="0 passed; 0 failed; 0 ignored;")
        with self.assertRaises(ValueError): reader.validate_native(self.native,self.root,self.project)

    def test_native_command_corruption_fails_with_coherent_receipt_identity(self):
        original = copy.deepcopy(self.command)
        for key,value in (("argv",["cargo","test"]),("exit_code",True),("exit_code",7),("status","failed"),
                          ("max_stream_bytes",True),("target","/unrelated"),("offline",1),("jobs",True)):
            with self.subTest(field=key,value=value):
                self.command = copy.deepcopy(original); self.command[key]=value; self.save_command()
                with self.assertRaises(ValueError): reader.validate_native(self.native,self.root,self.project)


if __name__ == "__main__":
    unittest.main()
