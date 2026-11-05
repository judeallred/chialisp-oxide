"""Temporary filesystem controls only; no compiler, Cargo or evidence capture."""
from dataclasses import replace
import os
from pathlib import Path
import tempfile
import unittest

from source_walk import WalkLimits, source_files


class SourceWalkTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="oxide-source-walk-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for name in ["pkg/Cargo.toml", "pkg/src/lib.rs", "pkg/src/target/real.rs", "pkg/plain/target/real.rs",
                     "pkg/tests/fixtures/consumer/Cargo.toml", "pkg/tests/fixtures/consumer/src/lib.rs",
                     "pkg/target/deps/cache.rs", "pkg/tests/fixtures/consumer/target/deps/cache.rs", "pkg/notes.txt"]:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("temporary fixture\n")
        self.caches = [self.root / "pkg/target", self.root / "pkg/tests/fixtures/consumer/target"]

    def guarded(self, path):
        self.assertTrue(all(not Path(path).is_relative_to(cache) for cache in self.caches),
                        "entered a Cargo build cache")
        return os.scandir(path)

    def test_cargo_caches_are_never_opened_and_target_modules_stay_source(self):
        self.assertEqual(source_files(self.root, ["pkg"], scandir=self.guarded), [
            "pkg/plain/target/real.rs", "pkg/src/lib.rs", "pkg/src/target/real.rs",
            "pkg/tests/fixtures/consumer/src/lib.rs"])
        all_files = source_files(self.root, "pkg", suffixes=None, scandir=self.guarded)
        self.assertIn("pkg/notes.txt", all_files)
        self.assertIn("pkg/Cargo.toml", all_files)
        self.assertFalse(any("/target/deps/" in name for name in all_files))

    def test_permission_and_nested_absence_errors_are_not_omissions(self):
        for failure in [PermissionError(13, "constructed EACCES"), FileNotFoundError(2, "vanished child")]:
            def blocked(path):
                if Path(path) == self.root / "pkg/plain/target":
                    raise failure
                return self.guarded(path)
            with self.subTest(failure=type(failure).__name__), self.assertRaises(type(failure)):
                source_files(self.root, "pkg", scandir=blocked)
        self.assertEqual(source_files(self.root, "missing"), [])

    def test_regular_manifest_cache_links_and_source_symlinks_fail_closed(self):
        manifest = self.root / "pkg/name-only/Cargo.toml"
        manifest.mkdir(parents=True)
        module = manifest.parent / "target/module.rs"
        module.parent.mkdir(); module.write_text("real Rust module")
        outside = self.root / "outside.rs"; outside.write_text("outside")
        result = source_files(self.root, "pkg", scandir=self.guarded)
        self.assertIn("pkg/name-only/target/module.rs", result)
        for name, target in [("alias.rs", outside), ("alias-directory", self.root / "pkg/src"),
                             ("broken", self.root / "missing-source")]:
            link = self.root / "pkg" / name
            link.symlink_to(target)
            try:
                with self.subTest(name=name), self.assertRaisesRegex(ValueError, "child symlinks"):
                    source_files(self.root, "pkg", scandir=self.guarded)
                if name == "alias-directory":
                    for relative in ["pkg/alias-directory", "pkg/alias-directory/target"]:
                        with self.subTest(relative=relative), self.assertRaisesRegex(ValueError, "symlinks"):
                            source_files(self.root, relative)
            finally:
                link.unlink()
        cache = self.root / "pkg/cache-link"
        cache.mkdir(); (cache / "Cargo.toml").write_text("regular Cargo manifest")
        self.caches.append(cache / "target")
        for target in [self.root / "pkg/src", self.root / "missing-cache"]:
            (cache / "target").symlink_to(target)
            try:
                self.assertEqual(source_files(self.root, "pkg", scandir=self.guarded), result)
            finally:
                (cache / "target").unlink()
        linked = self.root / "pkg/linked-manifest"
        (linked / "target").mkdir(parents=True)
        (linked / "Cargo.toml").symlink_to(self.root / "pkg/Cargo.toml")
        (linked / "target/module.rs").write_text("source")
        with self.assertRaisesRegex(ValueError, "child symlinks"):
            source_files(self.root, "pkg", scandir=self.guarded)

    def test_each_host_budget_is_enforced_and_exact_empty_tree_fits(self):
        tight = self.root / "tight"; tight.mkdir()
        for name in ["a.rs", "b.rs"]:
            (tight / name).write_text("source")
        for limits in [replace(WalkLimits(), sources=1), replace(WalkLimits(), directory_entries=1),
                       replace(WalkLimits(), entries=1)]:
            with self.subTest(limits=limits), self.assertRaises(ValueError):
                source_files(self.root, "tight", limits=limits)
        (self.root / "tree/child/grandchild").mkdir(parents=True)
        for limits in [replace(WalkLimits(), depth=1), replace(WalkLimits(), directories=1)]:
            with self.subTest(limits=limits), self.assertRaises(ValueError):
                source_files(self.root, "tree", limits=limits)
        self.assertEqual(source_files(self.root, "tree", limits=replace(WalkLimits(), depth=2, directories=3, entries=2)), [])
        with self.assertRaises(ValueError):
            source_files(self.root, "tight", limits=replace(WalkLimits(), sources=4097))
        for relative in ["../pkg", "/pkg", "pkg//src", "pkg/./src", "pkg\\src"]:
            with self.subTest(relative=relative), self.assertRaises(ValueError):
                source_files(self.root, relative)

    def test_stream_stops_at_first_over_limit_entry_and_closes(self):
        (self.root / "virtual").mkdir()
        seen, closed = [], []
        class Stream:
            def __enter__(self): return self
            def __exit__(self, *args): closed.append(True)
            def __iter__(self):
                for index in range(3):
                    seen.append(index)
                    if index > 1:
                        raise AssertionError("consumed beyond first excess entry")
                    yield object()  # The budget must fail before retaining/inspecting it.
        with self.assertRaisesRegex(ValueError, "directory entry count"):
            source_files(self.root, "virtual", scandir=lambda _: Stream(), limits=replace(WalkLimits(), directory_entries=1))
        self.assertEqual(seen, [0, 1]); self.assertEqual(closed, [True])

    def test_shared_start_root_budgets_and_shallow_inventory(self):
        for directory in ["left", "right"]:
            path = self.root / directory
            path.mkdir(); (path / "file.rs").write_text("source")
        exact = replace(WalkLimits(), directories=2, entries=2, sources=2, depth=0)
        self.assertEqual(source_files(self.root, ["left", "right"], limits=exact), ["left/file.rs", "right/file.rs"])
        for limits in [replace(exact, directories=1), replace(exact, entries=1), replace(exact, sources=1)]:
            with self.subTest(limits=limits), self.assertRaises(ValueError):
                source_files(self.root, ["left", "right"], limits=limits)
        with self.assertRaises(ValueError):
            source_files(self.root, ["missing", "also-missing"], limits=replace(exact, directories=1))
        (self.root / "shallow/nested").mkdir(parents=True)
        (self.root / "shallow/direct.py").write_text("source")
        (self.root / "shallow/nested/hidden.py").write_text("not in shallow inventory")
        def direct_only(path):
            self.assertEqual(Path(path), self.root / "shallow", "entered a shallow source child")
            return os.scandir(path)
        self.assertEqual(source_files(self.root, "shallow", suffixes=(".py",), recursive=False, scandir=direct_only), ["shallow/direct.py"])


if __name__ == "__main__":
    unittest.main()
