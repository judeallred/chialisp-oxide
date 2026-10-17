"""Closed source/license/input identity for the unchanged password tutorial module."""
import hashlib
import json
from pathlib import Path

from cargo_authoring import contained_file, unique_object
from password_vectors import generated_payload, named, require

ROOT = Path(__file__).resolve().parents[1]
REVISION = "dcf06c14836c7ad3f486a5bea36c5a812fd9a5e8"
TREE = "438fe8ece6587e9d96b40e2a1c03f953d89cf150"
REPOSITORY = "https://github.com/Chia-Network/chialisp-web"
BASE = f"corpus/upstream/chialisp-web/{REVISION}"
PAGE = BASE + "/docs/chialisp-primer/first-smart-coin.md"
LICENSE = BASE + "/LICENSE"
TREE_PATH = f"corpus/upstream/published-puzzles/Chia-Network--chialisp-web/{REVISION}/tree.json"
TREE_SHA = "1edc6c85cd4c970bbfee222059e682a5fd1ea1e082c28e46299753ee4335833e"
PAGE_SHA = "a5b853e43991789723f3522a524ff8e6a512869e780d437d04b019aba145747f"
PAGE_BLOB = "58934b5299e70bc461b56886dd2a579fbe55709a"
LICENSE_SHA = "7db0cf1060026bf2a544ea29cf9a96fba7d3781f93a1d06d6fee18251f1c207a"
LICENSE_BLOB = "37b84d60dd8a34d854a41e94c34eaf587648ac6c"
SPAN_SHA = "e2a4ee5c44ad50cd851da120c0c38ba51b344815a39a9c70df42a0af9679ce29"
GENERATED_SHA = "b0efaf3404d34669f0aa759bb259c227edf900b4850668bbf83c0f6d8035e025"
DEPENDENCIES = [f"crates/chialisp-oxide/src/{name}.rs" for name in ("lib", "tree", "hashing")] + ["puzzles/password-coin/generated-cases.json"]
NATIVE_SOURCES = [f"crates/chialisp-oxide-corpus/{name}" for name in
    ("Cargo.toml", "build.rs", "src/lib.rs", "src/native.rs", "src/native_hash.rs", "src/native_tree.rs", "src/password.rs", "src/password/ledger.rs")] + [
    "crates/chialisp-oxide-simulator/Cargo.toml", "crates/chialisp-oxide-simulator/src/lib.rs",
    "crates/chialisp-oxide-simulator/rejection-protocol.json"]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def blob(data):
    return hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()


def read_source(path):
    return contained_file(ROOT, path).read_bytes()


def load(data):
    return json.loads(data, object_pairs_hook=unique_object)


def fields(value, expected, label):
    require(isinstance(value, dict), "Missing " + label)
    for key, desired in expected.items():
        require(value.get(key) == desired, f"Changed {label}/{key}")


def source_contract(reader=read_source):
    page, license_data = reader(PAGE), reader(LICENSE)
    require((digest(page), blob(page)) == (PAGE_SHA, PAGE_BLOB), "Changed pinned tutorial page")
    require((digest(license_data), blob(license_data)) == (LICENSE_SHA, LICENSE_BLOB), "Changed pinned tutorial license")
    tree_data = reader(TREE_PATH)
    require(digest(tree_data) == TREE_SHA, "Changed pinned full source-tree inventory")
    tree = load(tree_data)
    fields(tree, {"repository": "Chia-Network/chialisp-web", "revision": REVISION, "root_tree_sha1": TREE}, "upstream tree")
    entries = {row["path"]: row for row in tree["entries"]}
    for path, payload, git_hash in [("docs/chialisp-primer/first-smart-coin.md", page, PAGE_BLOB), ("LICENSE", license_data, LICENSE_BLOB)]:
        fields(entries[path], {"kind": "blob", "mode": "100644", "git_sha1": git_hash, "size_bytes": len(payload)}, "source-tree member")
    span = b"".join(page.splitlines(keepends=True)[20:40])
    require(digest(span) == SPAN_SHA and len(span.splitlines()) == 20,
            "Changed exact inclusive20-line password module span")
    reference = reader("puzzles/password-coin/reference.clsp")
    require(reference == span, "Password reference must be the unchanged complete tutorial module")
    data = generated_payload()
    require(len(data) == 301641 and digest(data) == GENERATED_SHA
            and reader("puzzles/password-coin/generated-cases.json") == data,
            "Changed deterministic password-specific input file")
    manifest = load(reader("puzzles/password-coin/puzzle.json"))
    fields(manifest, {"slug": "password-coin", "kind": "pure_function", "environment": "raw",
        "source_dependencies": DEPENDENCIES, "native_sources": NATIVE_SOURCES,
        "cases": named()}, "password fixture contract")
    require(manifest.get("reference_mode", "module") == "module"
            and manifest.get("reference_includes", {}) == {} and manifest.get("rue_dependencies", []) == []
            and "validation_max_cost" not in manifest,
            "Password must retain default raw-module comparison/compilation contract")
    for path in DEPENDENCIES + NATIVE_SOURCES + ["puzzles/password-coin/puzzle.rs", "puzzles/password-coin/puzzle.rue"]:
        require(reader(path), "Empty required password source: " + path)
    provenance = manifest["provenance"]
    require(set(provenance) == {"classification", "example_id", "upstream", "license", "tree", "source_span", "source_adaptation", "references", "limitations"},
            "Unknown password provenance/compiled-artifact claim")
    fields(provenance, {"classification": "canonical_tutorial_port", "example_id": "chialisp-example:password-coin"}, "example identity")
    fields(provenance["upstream"], {"repository": REPOSITORY, "revision": REVISION,
        "source_path": "docs/chialisp-primer/first-smart-coin.md", "snapshot": PAGE,
        "source_sha256": PAGE_SHA, "git_blob_sha1": PAGE_BLOB, "root_tree_sha1": TREE}, "tutorial source")
    fields(provenance["license"], {"spdx": "Apache-2.0", "source_path": "LICENSE", "snapshot": LICENSE,
        "sha256": LICENSE_SHA, "git_blob_sha1": LICENSE_BLOB}, "tutorial license")
    fields(provenance["tree"], {"manifest": TREE_PATH, "sha256": TREE_SHA, "root_tree_sha1": TREE}, "tutorial tree")
    fields(provenance["source_span"], {"start_line": 21, "end_line": 40, "sha256": SPAN_SHA}, "source span")
    fields(provenance["source_adaptation"], {"kind": "none", "reference": "puzzles/password-coin/reference.clsp", "reference_sha256": SPAN_SHA}, "unchanged module")
    require(provenance["references"] == [f"{REPOSITORY}/blob/{REVISION}/docs/chialisp-primer/first-smart-coin.md#L21-L40",
        f"{REPOSITORY}/blob/{REVISION}/docs/chialisp-concepts/currying.md#L22-L68"], "Changed password source/curry references")
    return manifest


def dependencies(puzzle, authoring, reader=read_source):
    require([row["path"] for row in puzzle["source_dependencies"]] == DEPENDENCIES,
            "Missing/unknown password source dependencies")
    require([row["path"] for row in puzzle.get("native_sources", [])] == NATIVE_SOURCES,
            "Missing/unknown password native input closure")
    inputs = {row["path"]: row["sha256"] for row in authoring["sources"]}
    for row in puzzle["source_dependencies"]:
        data = reader(row["path"])
        require(set(row) == {"path", "sha256", "source"} and row["sha256"] == digest(data)
                and row["source"] == data.decode(), "Stale password source dependency")
    for row in puzzle["native_sources"]:
        require(set(row) == {"path", "sha256"} and row["sha256"] == digest(reader(row["path"]))
                and inputs.get(row["path"]) == row["sha256"], "Stale compiled password native input")
    for path in DEPENDENCIES + NATIVE_SOURCES + ["puzzles/password-coin/puzzle.rs"]:
        require(inputs.get(path) == digest(reader(path)), "Password input missing from compiled Cargo closure: " + path)
