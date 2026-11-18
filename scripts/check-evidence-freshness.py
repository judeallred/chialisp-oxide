#!/usr/bin/env python3
"""Find retained evidence packets whose current-source pins no longer match.

Many retained packets record `{path, sha256}` identities for repository source
files and refuse to load once any of those files changes. This reader finds
them all in one pass, so a source edit can be paired with every recapture it
requires before pushing rather than discovered gate by gate in CI.

A JSON file is a *current-source* record when every repository path it pins
matched the `--base` revision (default HEAD; choose a commit whose CI passed).
Those are the records the gates enforce, so any of them that no longer match
the working tree need a recapture. Records that already differed at the base
are frozen baselines or historical snapshots and are never reported as stale.
Uncommitted or modified records are treated as fresh captures and compared
with the working tree directly.

This is a planning aid. It runs no compiler, VM or Cargo command, and a clean
result neither replaces the dedicated packet gates nor grants execution credit.
It can over-report: a frozen historical record whose pins happened to still
match the base looks current here, so confirm each flagged packet with its own
reader before recapturing it.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCAN_ROOTS = ("corpus", "experiments", "proofs")
MAX_JSON_BYTES = 32 * 1024 * 1024
DIGEST = re.compile(r"^(?:sha256:)?([0-9a-f]{64})$")

# Packet prefix -> how that packet is refreshed. Longest prefix wins.
REFRESH = {
    "corpus/": "scripts/capture-current-corpus.sh",
    "proofs/": "scripts/capture-current-corpus.sh",
    "experiments/tree-theorems/": "scripts/capture-current-corpus.sh",
    "experiments/spend-costs/": "scripts/capture-current-corpus.sh",
    "experiments/continuation-batches/": "experiments/stateful-ledger/ci.py capture + --promote",
    "experiments/spuds/": "experiments/stateful-ledger/ci.py capture + --promote",
    "experiments/stateful-ledger/": "experiments/stateful-ledger/ci.py capture + --promote",
    "experiments/standard-tutorial/": "experiments/standard-tutorial/capture.py --execute, check, install",
    "experiments/authenticated-continuations/": "check_retained.py decides; a declared-input change needs ci.py and a reader revalidation",
    "corpus/xchandles-": "scripts/check-xchandles-history-v2.py and check-xchandles-simulator.py decide",
}


def git(*args: str, data: bytes | None = None) -> bytes:
    return subprocess.run(["git", *args], cwd=ROOT, input=data, check=True, capture_output=True).stdout


def pins(value, out: dict[str, set[str]]) -> None:
    """Collect path -> {sha256} pairs from the shapes packets use.

    A packet may pin the same relative name under several roots (for example a
    nested package's `Cargo.toml`), so every recorded digest is kept.
    """
    if isinstance(value, dict):
        path, digest = value.get("path"), value.get("sha256")
        if isinstance(path, str) and isinstance(digest, str) and DIGEST.match(digest):
            out.setdefault(path, set()).add(DIGEST.match(digest).group(1))
        for key, item in value.items():
            if isinstance(item, str) and "/" in key and DIGEST.match(item):
                out.setdefault(key, set()).add(DIGEST.match(item).group(1))
            else:
                pins(item, out)
    elif isinstance(value, list):
        for item in value:
            pins(item, out)


def group_key(path: str) -> tuple[str, str]:
    """(directory, extension) membership group for closure-set comparisons."""
    directory, _, filename = path.rpartition("/")
    return directory, filename.rpartition(".")[2] if "." in filename else ""


def repo_path(path: str, files: set[str]) -> str | None:
    """Map a pinned name to a repository file, including absolute host paths."""
    if path in files:
        return path
    if path.startswith(f"{ROOT}/"):
        relative = path[len(str(ROOT)) + 1:]
        return relative if relative in files else None
    if path.startswith("/"):
        # Another host's checkout root: take the longest suffix naming a tracked
        # file, requiring a directory component so bare names never alias.
        parts = path.split("/")
        for start in range(1, len(parts) - 1):
            candidate = "/".join(parts[start:])
            if candidate in files:
                return candidate
    return None


def blob_digests(requests: list[tuple[str, str]]) -> dict[tuple[str, str], str | None]:
    """sha256 of `<commit>:<path>` blobs via one `git cat-file --batch`."""
    if not requests:
        return {}
    raw = git("cat-file", "--batch", data="".join(f"{c}:{p}\n" for c, p in requests).encode())
    result, offset = {}, 0
    for request in requests:
        end = raw.index(b"\n", offset)
        header = raw[offset:end].split()
        offset = end + 1
        if header[-1] == b"missing":
            result[request] = None
            continue
        size = int(header[2])
        result[request] = hashlib.sha256(raw[offset:offset + size]).hexdigest()
        offset += size + 1
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--json", action="store_true", help="machine-readable report")
    parser.add_argument("--show-historical", action="store_true", help="also list frozen historical records")
    parser.add_argument("--base", default="HEAD", help="revision whose pinned records were current (default HEAD)")
    args = parser.parse_args()
    base = git("rev-parse", "--verify", f"{args.base}^{{commit}}").decode().strip()

    files = set(git("ls-files", "-co", "--exclude-standard", "-z").decode().split("\0")) - {""}
    dirty = set(git("ls-files", "-mo", "--exclude-standard", "-z").decode().split("\0")) - {""}
    candidates = sorted(f for f in files if f.endswith(".json") and f.split("/", 1)[0] in SCAN_ROOTS)

    records: dict[str, dict[str, str]] = {}
    for name in candidates:
        path = ROOT / name
        if not path.is_file() or path.stat().st_size > MAX_JSON_BYTES:
            continue
        try:
            document = json.loads(path.read_bytes())
        except (ValueError, UnicodeDecodeError):
            continue
        found: dict[str, set[str]] = {}
        pins(document, found)
        repo_pins: dict[str, set[str]] = {}
        for pinned, digests in found.items():
            mapped = repo_path(pinned, files)
            if mapped:
                repo_pins.setdefault(mapped, set()).update(digests)
        if repo_pins:
            records[name] = repo_pins

    current = {}
    for name in sorted({p for r in records.values() for p in r}):
        current[name] = hashlib.sha256((ROOT / name).read_bytes()).hexdigest() if (ROOT / name).is_file() else None

    def groups_of(names):
        index = defaultdict(set)
        for name in names:
            index[group_key(name)].add(name)
        return index

    base_groups = groups_of(git("ls-tree", "-r", "--name-only", "-z", base).decode().split("\0")[:-1])
    current_groups = groups_of(f for f in files if (ROOT / f).is_file())
    base_tree = blob_digests(sorted({(base, p) for r, ps in records.items() if r not in dirty for p in ps}))

    # Per pin: enforced if it matched the base; stale if enforced and the
    # working tree differs. Uncommitted records were just written by a capture,
    # which owns their freshness; they are counted, not judged, here.
    stale, historical, recaptured = defaultdict(list), [], []
    for name, repo_pins in sorted(records.items()):
        if name in dirty:
            recaptured.append(name)
            continue
        enforced = {p for p, ds in repo_pins.items() if base_tree.get((base, p)) in ds}
        if not enforced:
            historical.append(name)
            continue
        changed = sorted(p for p in enforced if current[p] not in repo_pins[p])
        # Closure readers also require the exact file set of directories they
        # enumerate: a record that pinned every base file of one extension in a
        # directory goes stale when such a file is added or removed there.
        groups = defaultdict(set)
        for p in enforced:
            groups[group_key(p)].add(p)
        for key, pinned in groups.items():
            if len(pinned) >= 3 and pinned == base_groups.get(key, set()) != current_groups.get(key, set()):
                changed += [f"{p} (added)" for p in sorted(current_groups.get(key, set()) - pinned)]
                changed += [f"{p} (removed)" for p in sorted(pinned - current_groups.get(key, set()))]
        if changed:
            stale[name] = changed

    packets = defaultdict(lambda: {"records": [], "changed": set()})
    for name, changed in stale.items():
        packet = "/".join(name.split("/")[:2])
        packets[packet]["records"].append(name)
        packets[packet]["changed"].update(changed)
    plan = {
        packet: {
            "refresh": next((REFRESH[k] for k in sorted(REFRESH, key=len, reverse=True) if f"{packet}/".startswith(k)), "unknown: inspect its README"),
            "records": len(info["records"]),
            "changed_sources": sorted(info["changed"]),
        }
        for packet, info in sorted(packets.items())
    }
    summary = {"base": base, "records_checked": len(records), "historical_records": len(historical),
               "uncommitted_records": len(recaptured), "stale_packets": plan}

    if args.json:
        if args.show_historical:
            summary["historical"] = historical
        print(json.dumps(summary, indent=2))
    else:
        print(f"{len(records)} records pin repository sources; {len(historical)} already differed at {base[:12]} (frozen); "
              f"{len(recaptured)} are uncommitted captures (not judged).")
        if args.show_historical:
            for name in historical:
                print(f"  historical: {name}")
        if not plan:
            print("All current-source records match the working tree.")
        for packet, info in plan.items():
            print(f"\nSTALE {packet} ({info['records']} records)\n  refresh: {info['refresh']}")
            for source in info["changed_sources"]:
                print(f"  changed: {source}")
    return 1 if plan else 0


if __name__ == "__main__":
    sys.exit(main())
