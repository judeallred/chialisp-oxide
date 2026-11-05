"""Bounded source discovery; Cargo's default output trees are not source.

Only an immediate target directory/link beside a regular Cargo.toml is pruned.
Every other symlink is rejected without following it. Read/permission errors propagate;
the one allowed absence is a missing top-level conventional source root.
"""
from dataclasses import dataclass
import os
from pathlib import Path, PurePosixPath
import stat


@dataclass(frozen=True)
class WalkLimits:
    depth: int = 64
    directory_entries: int = 4096
    sources: int = 4096
    directories: int = 4096
    entries: int = 16384


def _require(condition, message):
    if not condition:
        raise ValueError(message)


def source_files(root, directories, *, suffixes=(".rs",), names=(), recursive=True, limits=WalkLimits(), scandir=os.scandir):
    """Return sorted repo-relative regular files with bounded streaming discovery.

    All start directories share the aggregate budgets. Injectable scandir and
    smaller host limits support deterministic tests, not an evidence opt-out.
    suffixes=None selects every regular file; names adds exact basenames.
    recursive=False inspects only the supplied directories' direct files.
    """
    root = Path(root)
    if isinstance(directories, str):
        directories = [directories]
    for field, maximum in vars(WalkLimits()).items():
        value = getattr(limits, field)
        _require(type(value) is int and 0 <= value <= maximum, "Invalid source walk limit")
    counts = {"directories": 0, "entries": 0}
    found = set()

    def reserve_directory():
        _require(counts["directories"] < limits.directories, "Source directory count exceeded")
        counts["directories"] += 1

    def visit(directory, depth, reserved=False):
        _require(depth <= limits.depth, "Source directory depth exceeded")
        if not reserved:
            reserve_directory()
        entries = []
        try:
            stream = scandir(directory)
        except FileNotFoundError:
            if depth == 0:
                return
            raise
        with stream:
            for entry in stream:
                _require(len(entries) < limits.directory_entries, "Source directory entry count exceeded")
                _require(counts["entries"] < limits.entries, "Source aggregate entry count exceeded")
                counts["entries"] += 1
                entries.append(entry)
        has_manifest = any(entry.name == "Cargo.toml" and entry.is_file(follow_symlinks=False)
                           for entry in entries)
        for entry in entries:
            is_link = entry.is_symlink()
            is_directory = entry.is_dir(follow_symlinks=False)
            if entry.name == "target" and has_manifest and (is_directory or is_link):
                continue  # Never dereference even a broken Cargo cache link.
            _require(not is_link, "Source discovery rejects non-cache child symlinks")
            if is_directory:
                if recursive:
                    visit(Path(entry.path), depth + 1)
            elif entry.is_file(follow_symlinks=False) and (suffixes is None or entry.name.endswith(suffixes) or entry.name in names):
                relative = Path(entry.path).relative_to(root).as_posix()
                if relative not in found:
                    _require(len(found) < limits.sources, "Source file count exceeded")
                    found.add(relative)

    for relative in directories:
        _require(type(relative) is str and relative and "\\" not in relative
                 and len(relative.encode()) <= 4096 and len(relative.split("/")) <= 64
                 and not any(ord(c) < 32 for c in relative)
                 and PurePosixPath(relative).as_posix() == relative
                 and all(part not in ("", ".", "..") for part in relative.split("/"))
                 and not PurePosixPath(relative).is_absolute(), "Source root must be a normalized relative path")
        reserve_directory()  # Missing initial roots also consume a bounded attempt.
        directory = root
        missing = False
        for part in relative.split("/"):
            directory /= part
            try:
                metadata = directory.lstat()
            except FileNotFoundError:
                missing = True
                break
            _require(stat.S_ISDIR(metadata.st_mode) and not stat.S_ISLNK(metadata.st_mode),
                     "Source root cannot contain symlinks or non-directories")
        if not missing:
            visit(directory, 0, reserved=True)
    return sorted(found)
