"""Closed source, include, license and Git identities for two published hash ports."""
import hashlib
import importlib.util
import json
from pathlib import Path

from cargo_authoring import contained_file, unique_object
from hashing_vectors import GENERATED_SHA256, deserialize, generated_payload, named, require, sha_tree, validation_budget

ROOT = Path(__file__).resolve().parents[1]
PIN = "cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8"
TREE = "e131409ed769ef4def743d4ebbcf859b40fcec2d"
REPOSITORY = "https://github.com/Chia-Network/chia_puzzles"
BASE = f"corpus/upstream/chia-puzzles/{PIN}"
TREE_PATH = f"corpus/upstream/published-puzzles/Chia-Network--chia_puzzles/{PIN}/tree.json"
TREE_SHA = "9900f500c6397560e20046c5bf4d4016a651f9000d6893512317ce6067c7842d"
COMMIT = f"{BASE}/commit.git"
COMMIT_SHA = "68502b042abaad03f35dd59c0fce3d7d1ce000c1302ccf50c762a955b2ec4215"
LICENSE = f"{BASE}/LICENSE"
HELPER = f"{BASE}/puzzles/sha256tree.clib"
SPAN = f"{BASE}/sha256tree.clinc"
SPAN_SHA = "946a967edff3fdc1eed3260cc7a1f28972e6d78c3ea248a814a0cfe2599dec7e"
LAUNCHER = f"{BASE}/puzzles/singleton_launcher.clsp"
ARTIFACT = LAUNCHER + ".hex"
INCLUDE = f"{BASE}/puzzles/condition_codes.clib"
CANONICAL_HASH = "eff07522495060c066f66f32acc2a77e3a3e737aca8baea4d1a64ea4cdc13da9"
CANONICAL_HEX = "ff02ffff01ff04ffff04ff04ffff04ff05ffff04ff0bff80808080ffff04ffff04ff0affff04ffff02ff0effff04ff02ffff04ffff04ff05ffff04ff0bffff04ff17ff80808080ff80808080ff808080ff808080ffff04ffff01ff33ff3cff02ffff03ffff07ff0580ffff01ff0bffff0102ffff02ff0effff04ff02ffff04ff09ff80808080ffff02ff0effff04ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff018080"
PINS = {
    HELPER: ("eb75275835724f5c6c7c715290c3f33883b81fc71906c8864c6b6aa4fedb3e4e", "c374eb3e1d83139ed477cad7353a4c52606cacdd"),
    LAUNCHER: ("028b1dd7a1fe4bfad7165236b3a4076c6a1bb022aa6db0f9b5dac386299efbbb", "80d66651ec5723a03e065c22b173d71c29cd2e8e"),
    ARTIFACT: ("f516e4f9b16eccb84042462735b5e49a7fff85d7975f74f43c7808d360131540", "2495bb3d0faa2ccadf07f0b0cd110cf7b5eba4da"),
    INCLUDE: ("891ea562e6fdb783bcfceea0cdea92318acfe4a67c4b6deb238d5fc4af15377f", "a6a5d22b5883487b87b51f5cf3f66d63440d68cf"),
    LICENSE: ("7db0cf1060026bf2a544ea29cf9a96fba7d3781f93a1d06d6fee18251f1c207a", "37b84d60dd8a34d854a41e94c34eaf587648ac6c"),
}
SDK = [f"crates/chialisp-oxide/src/{name}.rs" for name in ("lib", "tree", "hashing")]
DEPENDENCIES = {"sha256tree": SDK + ["puzzles/sha256tree/generated-cases.json"],
                "singleton-launcher": SDK + [INCLUDE]}
NATIVE = [f"crates/chialisp-oxide-corpus/{name}" for name in
          ("Cargo.toml", "build.rs", "src/lib.rs", "src/native.rs", "src/native_tree.rs", "src/native_hash.rs")]
NATIVE_SOURCES = {"sha256tree": NATIVE, "singleton-launcher": NATIVE + ["crates/chialisp-oxide-corpus/src/spends.rs",
    "crates/chialisp-oxide-corpus/src/spends/singleton.rs",
    "crates/chialisp-oxide-simulator/Cargo.toml", "crates/chialisp-oxide-simulator/src/lib.rs",
    "crates/chialisp-oxide-simulator/rejection-protocol.json"]}
INCLUDES = {"sha256tree": {}, "singleton-launcher": {"condition_codes.clib": INCLUDE}}


def read_source(path):
    return contained_file(ROOT, path).read_bytes()


def digest(value):
    return hashlib.sha256(value).hexdigest()


def git_object(kind, value):
    return hashlib.sha1(f"{kind} {len(value)}\0".encode() + value).hexdigest()


def load(value):
    return json.loads(value, object_pairs_hook=unique_object)


def fields(actual, expected, label):
    require(isinstance(actual, dict), f"Missing {label}")
    for key, value in expected.items():
        require(actual.get(key) == value, f"False {label}/{key}")


def check_tree(reader):
    for path, (sha, blob) in PINS.items():
        data = reader(path)
        require(digest(data) == sha and git_object("blob", data) == blob, f"Changed pinned source/license/artifact: {path}")
    commit = reader(COMMIT)
    require(digest(commit) == COMMIT_SHA and git_object("commit", commit) == PIN
            and commit.startswith(f"tree {TREE}\n".encode()), "Changed canonical commit/tree identity")
    require(digest(reader(TREE_PATH)) == TREE_SHA, "Changed complete Git tree snapshot")
    tree = load(reader(TREE_PATH))
    fields(tree, {"repository": "Chia-Network/chia_puzzles", "revision": PIN, "root_tree_sha1": TREE}, "Git tree")
    spec = importlib.util.spec_from_file_location("hash_published_tree_validator", ROOT / "corpus/validate_published_puzzles.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.check_tree(tree)
    entries = {row["path"]: row for row in tree["entries"]}
    for local, (_, blob) in PINS.items():
        fields(entries[local.removeprefix(BASE + "/")], {"kind": "blob", "mode": "100644", "git_sha1": blob,
               "size_bytes": len(reader(local))}, "pinned source tree membership")
    span = b"".join(reader(HELPER).splitlines(keepends=True)[3:10])
    require(span == reader(SPAN) and digest(span) == SPAN_SHA, "Changed exact sha256tree helper span")
    return span


def common_provenance(provenance, original):
    fields(provenance["upstream"], {"repository": REPOSITORY, "revision": PIN,
        "source_path": original.removeprefix(BASE + "/"), "snapshot": original, "source_sha256": PINS[original][0],
        "git_blob_sha1": PINS[original][1], "root_tree_sha1": TREE}, "upstream")
    fields(provenance["license"], {"spdx": "Apache-2.0", "source_path": "LICENSE", "snapshot": LICENSE,
        "sha256": PINS[LICENSE][0], "git_blob_sha1": PINS[LICENSE][1]}, "license")
    fields(provenance["commit"], {"snapshot": COMMIT, "sha256": COMMIT_SHA, "git_commit_sha1": PIN,
        "root_tree_sha1": TREE}, "commit")
    fields(provenance["tree"], {"manifest": TREE_PATH, "sha256": TREE_SHA, "root_tree_sha1": TREE}, "tree")


def sources(reader=read_source):
    """Validate sources and test inputs only, without an execution claim."""
    span = check_tree(reader)
    canonical = bytes.fromhex(reader(ARTIFACT).decode().strip())
    require(canonical.hex() == CANONICAL_HEX and len(canonical) == 175
            and sha_tree(deserialize(canonical)).hex() == CANONICAL_HASH, "Changed published 175-byte module/hash")
    manifests = {}
    for slug in ("sha256tree", "singleton-launcher"):
        base = f"puzzles/{slug}"
        manifest = load(reader(base + "/puzzle.json"))
        original = HELPER if slug == "sha256tree" else LAUNCHER
        classification = "canonical_helper_port" if slug == "sha256tree" else "canonical_puzzle_port"
        fields(manifest, {"slug": slug, "kind": "pure_function" if slug == "sha256tree" else "coin_puzzle",
                         "environment": "raw", "source_dependencies": DEPENDENCIES[slug],
                         "native_sources": NATIVE_SOURCES[slug], "cases": named(slug)}, "fixture contract")
        if slug == "sha256tree":
            require(type(manifest.get("validation_max_cost")) is int
                    and manifest["validation_max_cost"] == validation_budget(slug), "Changed/missing fixed hash comparison budget")
        else:
            require("validation_max_cost" not in manifest, "Launcher must retain the omitted default comparison budget")
        require(manifest.get("reference_mode", "module") == "module" and manifest.get("rue_dependencies", []) == []
                and manifest.get("reference_includes", {}) == INCLUDES[slug], "Changed reference/include/Rue contract")
        # These paths are intentionally a closed input closure, not derived from
        # an arbitrary reference include name or a report-supplied path.
        for path in DEPENDENCIES[slug] + NATIVE_SOURCES[slug] + [base + "/puzzle.rs", base + "/puzzle.rue"]:
            require(reader(path), f"Empty declared source input: {path}")
        p = manifest["provenance"]
        require(p["classification"] == classification, "Changed helper/module coverage classification")
        common_provenance(p, original)
        common_keys = {"upstream", "license", "commit", "tree", "classification", "references", "source_adaptation", "limitations"}
        reference = reader(base + "/reference.clsp")
        if slug == "sha256tree":
            require(set(p) == common_keys | {"source_span"}, "Helper cannot claim an upstream module identity")
            header = b"; Raw whole-environment adapter; the published helper span below is verbatim.\n(mod ENV\n"
            require(reference == header + span + b"  (sha256tree ENV)\n)\n", "Changed visible raw helper adapter")
            fields(p["source_span"], {"helper": "sha256tree", "start_line": 4, "end_line": 10,
                "snapshot": SPAN, "sha256": SPAN_SHA}, "helper span")
            fields(p["source_adaptation"], {"kind": "raw_environment_module_wrapper", "reference": base + "/reference.clsp",
                "reference_sha256": digest(reference), "helper_span_sha256": SPAN_SHA}, "helper adapter")
            require(p["references"] == [f"{REPOSITORY}/blob/{PIN}/puzzles/sha256tree.clib#L4-L10",
                    f"{REPOSITORY}/blob/{PIN}/LICENSE", f"{REPOSITORY}/commit/{PIN}"], "Changed helper provenance URLs")
        else:
            require(set(p) == common_keys | {"upstream_artifact", "include_dependencies", "module_identity_obligation"},
                    "Changed whole-module provenance fields")
            require(reference == reader(LAUNCHER) and reader(base + "/reference.clsp.hex") == reader(ARTIFACT),
                    "Canonical launcher source/artifact must remain byte exact")
            fields(p["source_adaptation"], {"kind": "none", "reference_sha256": PINS[LAUNCHER][0]}, "unchanged module")
            require(p["references"] == [f"{REPOSITORY}/blob/{PIN}/puzzles/singleton_launcher.clsp",
                    f"{REPOSITORY}/blob/{PIN}/puzzles/singleton_launcher.clsp.hex", f"{REPOSITORY}/blob/{PIN}/LICENSE"],
                    "Changed canonical module URLs")
            require(p["include_dependencies"] == [{"source_path": "puzzles/condition_codes.clib", "snapshot": INCLUDE,
                "source_sha256": PINS[INCLUDE][0], "git_blob_sha1": PINS[INCLUDE][1]}], "Changed/unknown module include")
            fields(p["upstream_artifact"], {"path": ARTIFACT, "source_url": p["references"][1], "source_sha256": PINS[ARTIFACT][0],
                "git_blob_sha1": PINS[ARTIFACT][1], "clvm_hex": CANONICAL_HEX, "bytes": 175,
                "tree_hash": CANONICAL_HASH}, "canonical module artifact")
        manifests[slug] = manifest
    payload = generated_payload()
    require(len(payload) == 63221 and digest(payload) == GENERATED_SHA256
            and reader("puzzles/sha256tree/generated-cases.json") == payload, "Changed 275 generated hash inputs")
    return manifests
