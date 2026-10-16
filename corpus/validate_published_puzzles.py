#!/usr/bin/env python3
"""Validate the pinned publication catalog against complete, local Git trees.

This checks enumeration/provenance consistency, not puzzle validity, deployment,
license permission, Rust-to-CLVM support, or a compiler-preservation theorem.
No network access or third-party Python dependencies are required.
"""
from __future__ import annotations

import argparse
import collections
import copy
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import sys
from urllib.parse import quote

ROOT = Path(__file__).resolve().parent.parent
SUFFIXES = (".clsp.hex", ".clvm.hex", ".clsp", ".clib", ".clinc", ".clvm", ".rue", ".rs", ".cl")
SHA1 = re.compile(r"[0-9a-f]{40}\Z")
CLASSES = {"official", "community", "library", "example", "test", "generated", "host_tooling"}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def confined(path: str) -> Path:
    require(isinstance(path, str) and PurePosixPath(path).as_posix() == path, "manifest path must be normalized")
    parts = PurePosixPath(path).parts
    require(bool(parts) and not PurePosixPath(path).is_absolute(), f"not a relative path: {path}")
    require(all(part not in {".", ".."} for part in parts), f"unconfined path: {path}")
    result = (ROOT / path).resolve(strict=True)
    require(result.is_relative_to(ROOT), f"path escapes repository: {path}")
    return result


def source_url(repository: str, revision: str, path: str) -> str:
    return f"https://github.com/{repository}/blob/{revision}/{quote(path, safe='/')}"


def language(path: str) -> str | None:
    if path.endswith((".clsp.hex", ".clvm.hex")):
        return "clvm_hex"
    suffix = PurePosixPath(path).suffix
    return {
        ".clsp": "chialisp", ".clib": "chialisp", ".clinc": "chialisp",
        ".cl": "chialisp", ".clvm": "clvm", ".rue": "rue", ".rs": "rust",
    }.get(suffix)


def classify(repository: str, path: str, lang: str) -> tuple[str, str]:
    """Explicit path-based triage, deliberately not semantic certification."""
    lower = path.lower()
    parts = PurePosixPath(lower).parts
    basename = parts[-1]
    test = any(part in {"test", "tests", "_tests", "fuzz", "fuzz_targets", "benchmark", "benchmarks", "benches"} for part in parts)
    test = test or basename.startswith(("test_", "smoke_test", "compile_chialisp_test", "compile_rue_test"))
    test = test or basename.endswith(("_test.clsp", "_test.rue", "_test.rs"))
    generated_data = any(token in basename for token in (".gen_env.", ".gen_exp."))
    if generated_data:
        return "test", "test_data"
    if lang == "clvm_hex":
        return "generated", "compiled_test_artifact" if test else "compiled_clvm_artifact"
    if test:
        return "test", "rust_test_or_fuzzer" if lang == "rust" else "language_or_puzzle_test"
    if lang == "rust":
        if repository == "judeallred/chialisp-oxide" and lower.startswith("puzzles/") and basename == "puzzle.rs":
            return "example", "rust_to_clvm_puzzle_source"
        if repository == "Chia-Network/chia_puzzles" and lower == "src/programs.rs":
            return "generated", "rust_compiled_puzzle_bindings"
        if repository == "xch-dev/chia-wallet-sdk":
            if lower.startswith("crates/chia-sdk-driver/src/layers/") or lower == "examples/custom_p2_puzzle.rs":
                return "library", "rust_puzzle_builder"
            if lower.startswith("crates/chia-sdk-types/src/puzzles/") or lower == "crates/chia-sdk-types/src/puzzles.rs":
                return "library", "rust_puzzle_types_and_bindings"
        if repository == "Chia-Network/chia_rs" and lower.startswith("crates/chia-puzzle-types/"):
            return "library", "rust_puzzle_types_and_bindings"
        return "host_tooling", "rust_host_module"
    if lower.endswith((".clib", ".clinc")):
        return "library", "chialisp_library"
    if repository == "xch-dev/rue":
        if lower.startswith("crates/rue-compiler/src/std/"):
            return "library", "rue_library"
        return "example", "puzzle_source" if lower.startswith("examples/puzzles/") else "language_example"
    if repository == "Chia-Network/chialisp":
        return "test", "language_or_puzzle_test"
    if repository in {"Chia-Network/clvm", "Chia-Network/clvm_rs"}:
        return "test", "vm_program_or_fixture"
    if any(part in {"example", "examples", "tutorial", "tutorials"} for part in parts):
        return "example", "puzzle_or_language_example"
    if repository == "Chia-Network/chia_puzzles":
        if any(part in {"consensus_puzzles", "full_node_puzzles"} for part in parts):
            return "official", "consensus_program_source"
        return "official", "puzzle_source"
    return ("official" if repository.startswith("Chia-Network/") else "community"), "puzzle_or_program_source"


def check_tree(snapshot: dict) -> list[dict]:
    """Reconstruct every Git tree, detecting omitted or modified path entries."""
    require(snapshot.get("truncated") is False, "truncated upstream tree")
    require(SHA1.fullmatch(snapshot["root_tree_sha1"]) is not None, "invalid root tree hash")
    entries = snapshot["entries"]
    paths = [entry["path"] for entry in entries]
    require(paths == sorted(paths) and len(paths) == len(set(paths)), "tree entries must be unique and sorted")
    by_path = {entry["path"]: entry for entry in entries}
    children: dict[str, list[dict]] = collections.defaultdict(list)
    for entry in entries:
        raw = entry["path"]
        require(isinstance(raw, str) and PurePosixPath(raw).as_posix() == raw, "Git path must be normalized")
        path = PurePosixPath(raw)
        require(bool(path.parts) and not path.is_absolute() and all(part not in {".", ".."} for part in path.parts), "invalid Git path")
        require(SHA1.fullmatch(entry["git_sha1"]) is not None, f"invalid Git hash: {path}")
        require((entry["kind"], entry["mode"]) in {
            ("tree", "040000"), ("blob", "100644"), ("blob", "100755"),
            ("blob", "120000"), ("commit", "160000"),
        }, f"unexpected Git entry type: {path}")
        parent = str(path.parent)
        parent = "" if parent == "." else parent
        require(not parent or by_path.get(parent, {}).get("kind") == "tree", f"missing parent tree: {path}")
        children[parent].append(entry)
    directories = {"": snapshot["root_tree_sha1"]}
    directories.update({entry["path"]: entry["git_sha1"] for entry in entries if entry["kind"] == "tree"})
    for directory, expected in directories.items():
        def order(entry: dict) -> bytes:
            name = PurePosixPath(entry["path"]).name.encode()
            return name + (b"/" if entry["kind"] == "tree" else b"")
        payload = b"".join(
            entry["mode"].lstrip("0").encode() + b" "
            + PurePosixPath(entry["path"]).name.encode() + b"\0"
            + bytes.fromhex(entry["git_sha1"])
            for entry in sorted(children[directory], key=order)
        )
        actual = hashlib.sha1(f"tree {len(payload)}\0".encode() + payload).hexdigest()
        require(actual == expected, f"Git tree content mismatch: {snapshot['repository']}:{directory or '/'}")
    return entries


def validate(data: dict) -> None:
    require(data.get("schema_version") == 1, "unsupported publication schema")
    require(data["scope"].get("public_only") is True, "publication scope must be public-only")
    require(data["scope"].get("not_internet_exhaustive") is True, "universal discovery claim is unsupported")
    require(data["scope"]["selected_suffixes"] == list(SUFFIXES), "selection contract changed")
    require(data["scope"]["classification_method"] == "deterministic_path_triage_not_semantic_certification", "classification claim changed")
    search_path = confined(data["discovery"]["snapshot"])
    require(hashlib.sha256(search_path.read_bytes()).hexdigest() == data["discovery"]["snapshot_sha256"], "discovery snapshot hash mismatch")
    discovery = json.loads(search_path.read_text())
    discovered = set(data["discovery"]["seed_repositories"])
    for query in discovery["queries"]:
        require("is:public" in query["query"].split(), "search is not explicitly public-only")
        require(query["incomplete_results"] is False, "incomplete search response")
        require(query["total_count"] == len(query["repositories"]), "search result pages missing")
        require(all(item.get("visibility") == "public" and item.get("private") is False for item in query["repositories"]), "nonpublic search result")
        discovered.update(item["repository"] for item in query["repositories"])
    aliases = discovery["requested_to_canonical"]
    failures = {item["repository"] for item in discovery["failures"]}
    repositories = data["repositories"]
    names = [repo["repository"] for repo in repositories]
    require(names == sorted(names) and len(names) == len(set(names)), "repository identities must be unique and sorted")
    require(discovered == set(aliases) | failures, "discovery outcomes are missing or invented")
    require(set(aliases.values()) == set(names), "repository set does not match discovery outcomes")
    expected_sources = {}
    for repo in repositories:
        name, revision = repo["repository"], repo["revision"]
        require(repo.get("visibility") == "public" and repo.get("private") is False, f"nonpublic repository: {name}")
        require(SHA1.fullmatch(revision) is not None, f"repository is not commit-pinned: {name}")
        path = confined(repo["tree_manifest"])
        require(hashlib.sha256(path.read_bytes()).hexdigest() == repo["tree_manifest_sha256"], f"tree snapshot hash mismatch: {name}")
        snapshot = json.loads(path.read_text())
        require(snapshot["repository"] == name and snapshot["revision"] == revision, "tree repository/revision mismatch")
        require(snapshot.get("visibility") == "public" and snapshot.get("private") is False, "nonpublic tree snapshot")
        entries = check_tree(snapshot)
        selected = [entry for entry in entries if entry["kind"] == "blob" and entry["mode"] in {"100644", "100755"} and language(entry["path"])]
        require(repo["selected_files"] == len(selected), f"repository selection count mismatch: {name}")
        for entry in selected:
            key = (name, revision, entry["path"])
            expected_sources[key] = (repo, entry)
    sources = data["sources"]
    keys = [(item["repository"], item["revision"], item["path"]) for item in sources]
    require(keys == sorted(keys) and len(keys) == len(set(keys)), "source identities must be unique and sorted")
    require(set(keys) == set(expected_sources), "selected tracked source set is incomplete or invented")
    for item in sources:
        key = (item["repository"], item["revision"], item["path"])
        repo, entry = expected_sources[key]
        require(item["id"] == f"{key[0]}@{key[1]}:{key[2]}", "source ID does not match provenance")
        lang = language(item["path"])
        classification, kind = classify(item["repository"], item["path"], lang)
        require(item["language"] == lang and item["classification"] == classification and item["artifact_kind"] == kind, f"incorrect source triage: {key}")
        require(item["classification"] in CLASSES, "unknown source class")
        require(item["git_blob_sha1"] == entry["git_sha1"] and item["size_bytes"] == entry["size_bytes"], f"file provenance mismatch: {key}")
        require(item["source_url"] == source_url(*key), f"unpinned source link: {key}")
        require(item["license"] == repo["license"]["spdx_id"], "repository license metadata mismatch")
        require(item["validation_status"] == "not_validated_by_this_inventory", "publication was promoted to execution evidence")
        require("sha256" not in item, "source SHA256 must not be invented from a Git blob ID")
    summary = data["summary"]
    require(summary["repositories"] == len(repositories) and summary["source_files"] == len(sources), "summary count mismatch")
    for key, field in [("by_language", "language"), ("by_classification", "classification"), ("by_artifact_kind", "artifact_kind")]:
        require(summary[key] == dict(collections.Counter(item[field] for item in sources)), f"summary {key} mismatch")


def mutation_checks(data: dict) -> int:
    changes = [
        lambda d: d["sources"].pop(),
        lambda d: d["sources"][0].update(git_blob_sha1="0" * 40),
        lambda d: d["scope"].update(not_internet_exhaustive=False),
        lambda d: d["repositories"][0].update(revision="main"),
        lambda d: d["repositories"][0].update(tree_manifest_sha256="0" * 64),
        lambda d: d["summary"].update(source_files=0),
        lambda d: d["sources"][0].update(validation_status="verified"),
        lambda d: d["repositories"][0].update(private=True),
        lambda d: d["scope"].update(public_only=False),
    ]
    for change in changes:
        mutant = copy.deepcopy(data)
        change(mutant)
        try:
            validate(mutant)
        except (ValueError, KeyError):
            continue
        raise ValueError("publication validator failed to reject a mutation")
    # A path alias must not be accepted merely because its normalized basename
    # reconstructs the same Git object. This control needs no network or files.
    blob = hashlib.sha1(b"blob 0\0").hexdigest()
    payload = b"100644 leaf.clsp\0" + bytes.fromhex(blob)
    fixture = {
        "repository": "test/fixture", "truncated": False,
        "root_tree_sha1": hashlib.sha1(f"tree {len(payload)}\0".encode() + payload).hexdigest(),
        "entries": [{"path": "leaf.clsp", "kind": "blob", "mode": "100644", "git_sha1": blob, "size_bytes": 0}],
    }
    check_tree(fixture)
    aliases = ["./leaf.clsp", ".//leaf.clsp", "leaf.clsp/"]
    for alias in aliases:
        mutant = copy.deepcopy(fixture)
        mutant["entries"][0]["path"] = alias
        try:
            check_tree(mutant)
        except ValueError:
            continue
        raise ValueError("publication validator accepted a non-normalized Git path")
    return len(changes) + len(aliases)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inventory", type=Path, default=ROOT / "corpus/published-puzzles.json")
    parser.add_argument("--self-test", action="store_true", help="also reject provenance/coverage/claim mutations")
    args = parser.parse_args()
    try:
        data = json.loads(args.inventory.read_text())
        validate(data)
        count = mutation_checks(data) if args.self_test else 0
        print(f"published source inventory: {len(data['repositories'])} pinned trees, {len(data['sources'])} files; {count} mutation controls passed")
        return 0
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"published source inventory invalid: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
