#!/usr/bin/env python3
"""Find the largest duplicated subtree in each retained optimized Rust module (#6).

Reads `corpus/validation.json`, deserializes each selected export's optimized
Rust artifact and the independently compiled Chialisp artifact, and reports the
largest subtree that occurs more than once, how much of the module it accounts
for, and the `if` node whose two arms contain the copies. This is a read-only
diagnostic of retained bytes; it changes no compiler behavior and is not a cost
or preservation claim.
"""
import collections
import hashlib
import json
import os
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
sys.setrecursionlimit(1_000_000)
import corpus_report  # noqa: E402
import raw_tree  # noqa: E402

SLUGS = ("ordered-payout", "one-armed-bandit", "raffle", "spuds")


def largest_duplicate(tree):
    occurrences, sizes = collections.defaultdict(list), {}

    def walk(node, path):
        if type(node) is not tuple:
            return b"a" + node, len(raw_tree.serialize(node))
        left, left_size = walk(node[0], path + "f")
        right, right_size = walk(node[1], path + "r")
        key = hashlib.sha256(b"p" + left + right).digest()
        sizes[key] = 1 + left_size + right_size
        occurrences[key].append(path)
        return key, sizes[key]

    walk(tree, "")
    repeated = [key for key, paths in occurrences.items() if len(paths) > 1]
    if not repeated:
        return None
    key = max(repeated, key=lambda k: sizes[k])
    first, second = occurrences[key][:2]
    common = os.path.commonpath(["/" + "/".join(first), "/" + "/".join(second)]).replace("/", "")

    def at(path):
        node = tree
        for step in path:
            node = node[0] if step == "f" else node[1]
        return node

    split = common
    while split and not (type(at(split)) is tuple and at(split)[0] == b"\x03"):
        split = split[:-1]
    branch = at(split) if split else None
    condition = raw_tree.serialize(branch[1][0]).hex() if branch else None
    return dict(subtree_bytes=sizes[key], copies=len(occurrences[key]), branch_condition=condition)


def main():
    report = corpus_report.decode((ROOT / "corpus/validation.json").read_bytes())
    rows = []
    for slug in SLUGS:
        puzzle = next(p for p in report["puzzles"] if p["slug"] == slug)
        artifacts = {(a["backend"], a["optimize_lir"]): a for a in puzzle["artifacts"]}
        rust = artifacts[("rust_mir", True)]
        found = largest_duplicate(raw_tree.deserialize(bytes.fromhex(rust["clvm_hex"])))
        rows.append(dict(slug=slug, rust_on_bytes=rust["bytes"], chialisp_bytes=artifacts[("chialisp", True)]["bytes"],
                         rue_on_bytes=artifacts[("rue_source", True)]["bytes"], largest_duplicate=found,
                         duplicate_share_of_module=round(found["subtree_bytes"] * (found["copies"] - 1) / rust["bytes"], 3) if found else 0))
    print(json.dumps(dict(scope=__doc__.strip().split("\n\n")[1], exports=rows), indent=2))


if __name__ == "__main__":
    main()
