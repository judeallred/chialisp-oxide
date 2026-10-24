"""Bounded retained canonical host provenance for original counter demos.

Identity checks and source retention do not add compiler/VM/SDK execution credit.
"""
import sys
sys.dont_write_bytecode = True
import hashlib
import importlib.util
import json
from pathlib import Path
import re

from cargo_authoring import unique_object
from port_evidence import check_pins
from registry_codegen_evidence import inspect_compiled_artifact, read, require

ROOT = Path(__file__).resolve().parents[1]
SLOT_PIN = "6edc94856306d0f77e065c6c99aacf1055e51e2f"
SDK_PIN = "e9a08f6e54ea5132f7da06327836a3b386a944b1"
BASES = ((f"corpus/upstream/slot-machine/{SLOT_PIN}", "Yakuhito/slot-machine", SLOT_PIN,
          "7ceb5d708945ff039f3a7e217fbbfa6960840b7f", 263, 3083549, True),
         (f"corpus/upstream/chia-wallet-sdk/{SDK_PIN}", "xch-dev/chia-wallet-sdk", SDK_PIN,
          "45f3a22736cd4bc1b9913a5a9ed77bb94e195d09", 23, 217727, False))
MAX_FILES = 1024
MAX_FILE = 2 * 1024 * 1024
MAX_TOTAL = 16 * 1024 * 1024


def load(raw):
    return json.loads(raw, object_pairs_hook=unique_object)


def git_object(kind, raw):
    return hashlib.sha1(f"{kind} {len(raw)}\0".encode() + raw).hexdigest()


def local(relative):
    part = Path(relative)
    require(type(relative) is str and not part.is_absolute() and part.as_posix() == relative
            and part.parts and all(p not in (".", "..") for p in part.parts), "confined upstream path")
    path = ROOT / part
    require(path.resolve().is_relative_to(ROOT), "upstream path escape")
    return path


def check():
    spec = importlib.util.spec_from_file_location("counter_upstream_tree_checker", ROOT / "corpus/validate_published_puzzles.py")
    checker = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(checker)
    results = []
    for base, repository, pin, root_tree, count, size, complete in BASES:
        provenance = load(read(local(base + "/provenance.json"), MAX_FILE))
        inventory = load(read(local(base + "/source-inventory.json"), MAX_FILE))
        tree = load(read(local(base + "/tree.json"), MAX_FILE))
        commit = read(local(base + "/commit.git"), MAX_FILE)
        require(git_object("commit", commit) == pin and commit.startswith(f"tree {root_tree}\n".encode()), "exact pinned commit/tree")
        require(tree["repository"] == repository and tree["revision"] == pin
                and tree["root_tree_sha1"] == root_tree, "exact complete tree identity")
        entries = checker.check_tree(tree)
        blobs = {r["path"]: r for r in entries if r["kind"] == "blob"}
        require(inventory["repository"] == repository and inventory["revision"] == pin
                and inventory["root_tree_sha1"] == root_tree
                and inventory["complete_source_snapshot"] is complete, "exact source retention scope")
        rows = inventory["files"]
        require(type(rows) is list and len(rows) == count <= MAX_FILES
                and type(inventory["tracked_source_files"]) is int and inventory["tracked_source_files"] == count
                and type(inventory["source_bytes"]) is int and inventory["source_bytes"] == size, "fixed retained inventory size/count")
        names = [r["path"] for r in rows]
        require(len(names) == len(set(names)) and (not complete or set(names) == set(blobs)), "complete source snapshot membership")
        planned = []
        remaining = MAX_TOTAL
        # Charge every immutable declaration before any source payload read.
        for row in rows:
            require(set(row) == {"path", "bytes", "sha256", "git_blob_sha1", "mode"}, "exact retained source fields")
            length = row["bytes"]
            require(type(length) is int and 0 <= length <= MAX_FILE and length <= remaining, "complete source inventory precharge")
            remaining -= length
            path = local(base + "/" + row["path"])
            item = blobs.get(row["path"])
            require(item is not None and item["git_sha1"] == row["git_blob_sha1"]
                    and item["mode"] == row["mode"] and item["size_bytes"] == length, "actual Git source membership")
            planned.append((row, path))
        require(MAX_TOTAL - remaining == size, "complete declared source bytes")
        for row, path in planned:
            raw = read(path, row["bytes"])
            require(len(raw) == row["bytes"] and hashlib.sha256(raw).hexdigest() == row["sha256"]
                    and git_object("blob", raw) == row["git_blob_sha1"], "exact retained upstream source blob")
        checked = check_pins(provenance, reader=lambda path: read(local(path), MAX_FILE))
        results.append({"repository": repository, "files": count, "bytes": size, "pinned_checks": checked})
    # Preserve the source/license discrepancy explicitly; SDK identity does not
    # turn the upstream license into Apache or certify legal compatibility.
    slot = load(read(local(BASES[0][0] + "/provenance.json"), MAX_FILE))
    require(slot["license"]["cargo_license_field"] == "MIT"
            and slot["license"]["declared_root_license"] == "Prosperity Public License 3.0.0", "visible upstream license discrepancy")
    require([a["module"] for a in slot["artifacts"]] == ["action", "finalizer", "slot"], "complete canonical module relation inventory")
    for artifact in slot["artifacts"]:
        spelling = read(local(artifact["retained_hex"]["path"]), MAX_FILE).decode("ascii")
        raw = bytes.fromhex("".join(spelling.split()))
        sdk = artifact["sdk_literal"]
        source = read(local(sdk["path"]), MAX_FILE).decode("ascii")
        source = re.sub(r"//[^\n]*", "", source)
        pattern = r"pub const " + re.escape(sdk["constant"]) + r": \[u8; (\d+)\] = hex!\(\s*\"([0-9a-f\s]+)\"\s*\)"
        matches = re.findall(pattern, source)
        require(len(matches) == 1, "exact named SDK module constant")
        length, text = matches[0]
        require(int(length) == len(raw) and bytes.fromhex(text) == raw, "actual SDK/canonical serialized module equality")
        identity = inspect_compiled_artifact(raw)["identity"]
        require(identity["bytes"] == artifact["serialized_bytes"]
                and identity["blob_sha256"] == artifact["blob_sha256"]
                and identity["module_tree_hash"] == artifact["module_tree_hash"]
                and read(local(artifact["retained_hash"]["path"]), MAX_FILE).strip().decode("ascii") == identity["module_tree_hash"], "actual bounded module identities")
    return results
