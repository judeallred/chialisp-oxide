"""Independent retained five-build CAT TAIL lifecycle consistency reader.

No compiler, native policy, VM, SDK or network execution occurs here. Source
freshness and exact recorded wire relationships do not authenticate execution.
"""
import argparse
import copy
import importlib.util
import json
from pathlib import Path
import re
import sys

from registry_codegen_evidence import read, require
import port_evidence

ROOT = Path(__file__).resolve().parents[1]
SLUG = "cat-genesis-by-coin-id"
PROTOCOL = "chialisp-oxide-cat-genesis-by-coin-id/v1"
MODES = [("rust_mir", False), ("rust_mir", True), ("chialisp", True), ("rue_source", False), ("rue_source", True)]
PROFILE = {"sdk": "chia-sdk-test 0.36.0", "consensus": "chia-consensus 0.36.1", "sdk_vm": "clvmr 0.16.2",
           "network": "TESTNET11", "primary_vm": "clvmr 0.21.0", "primary_protocol": "clvmr-0.21.0/v1",
           "primary_flags": 535, "primary_max_cost": 250_000_000,
           "sdk_max_block_cost_clvm": 11_000_000_000, "sdk_transaction_max_cost": 5_500_000_000}
COUNTS = {"sdk_transaction_attempts": 75, "accepted": 20, "typed_rejected": 35,
          "runtime_umbrellas": 20, "component_primary_vm_runs": 75}
TOP_FIELDS = {"schema_version", "protocol", "status", *PROFILE, "cat_module_hex", "cat_module_tree_hash",
              "canonical_tail_hex", "canonical_tail_tree_hash", "owner_public_key", "recipient_public_key",
              "builds", *COUNTS, "scope"}


def closed(value, keys, label):
    require(type(value) is dict and set(value) == set(keys), label + ": exact fields required")


def _load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# Isolated module globals: the broad compiled transport profile must not mutate
# the canonical standalone reader imported elsewhere in the same process.
DIRECTORY = ROOT / "experiments/cat-genesis-tail"
sys.path.insert(0, str(DIRECTORY))
try:
    _vectors = _load(DIRECTORY / "vectors.py", "_cat_corpus_vectors")
    _previous = sys.modules.get("vectors")
    sys.modules["vectors"] = _vectors
    try:
        lifecycle = _load(DIRECTORY / "lifecycle.py", "_cat_corpus_lifecycle")
    finally:
        if _previous is None:
            sys.modules.pop("vectors", None)
        else:
            sys.modules["vectors"] = _previous
    codec = _load(DIRECTORY / "compiled_codec.py", "_cat_corpus_codec")
finally:
    sys.path.remove(str(DIRECTORY))
lifecycle.decode, lifecycle.encode, lifecycle.identity = codec.decode, codec.encode, codec.identity


def compiled_raw(value, maximum=codec.MAX_BYTES):
    require(type(value) is str and len(value) <= maximum * 2 and re.fullmatch(r"(?:[0-9a-f]{2})+", value),
            "bounded exact compiled hexadecimal data")
    return bytes.fromhex(value)


lifecycle.raw = compiled_raw


PIN = "d122456feeef55708d354a065623037bfa010714"
TREE = "89e5f55c5afdde2c436b9a2c08b8e30ecf11eb62"
BASE = f"corpus/upstream/chia-puzzles/{PIN}"
TREE_PATH = f"corpus/upstream/published-puzzles/Chia-Network--chia_puzzles/{PIN}/tree.json"
TREE_SHA = "e3983e72f648a10e1c8bdf0e7ac9e2d333933521ae2e93df1a7c492411105906"
COMMIT_SHA = "542f5b33c048f6abdebe9d6ed5482f5c9c009ca45c93521c3e123f665654ddd1"
# Independently retained experiment source/include/license/artifact pins.
PINS = {'puzzles/cat_puzzles/genesis_by_coin_id.clsp': ('1914e2d935d0b8d8b514e0d9c07098ce488000a7bcf1b826266cab5e3a265d82', 'ac2ae60b18a01a2b8165ed77514a58139710e0d8', 499), 'puzzles/cat_puzzles/genesis_by_coin_id.clsp.hex': ('00cb7c17f4bf44ce835930f7d6a9d02c1b811e3cb0966e0482cf46338c960235', 'fe7dc034ad0e81fd6cc0b81ca00a43f99a4093bb', 91), 'puzzles/cat_puzzles/cat_truths.clib': ('6dd8c22abf35e750cefda8005f3540078bd7f63749f38af6b0ae47e962fbbb9b', '563ec65f3a56b382685974c2d3fc86dc8de6aec9', 1198), 'LICENSE': ('d2db94fa34f37910c925ac45d98084a41c1bcb08296eb5525ead62e9f26aeb75', 'ef0cf9fb38e4a29b044ac2d48873ae03c9f80a6b', 11347), 'puzzles/cat_puzzles/cat_v2.clsp': ('9e251314fd7f3525e970458eb1ad223af6db2f793a4eac53c57bcda573f48f87', 'aeebd641205c09b97243aebf3306218bd739a1fa', 15726), 'puzzles/cat_puzzles/cat_v2.clsp.hex': ('37128f5e626a634835b4e6fd22d73def9c2f1ffdd5c8f18acdb2cfb74b327e6d', '5cbde1ad697c5084f9b3ba8832afb4584dbc101a', 3345), 'puzzles/condition_codes.clib': ('891ea562e6fdb783bcfceea0cdea92318acfe4a67c4b6deb238d5fc4af15377f', 'a6a5d22b5883487b87b51f5cf3f66d63440d68cf', 2236), 'puzzles/curry-and-treehash.clib': ('5132c3d2428264deb1c405a1debf7a75a84495f96717b0795f18ea0f0f16120e', '0b7632aad3682cfbf9038d37033a006ce3d2ed8f', 3869), 'puzzles/utility_macros.clib': ('f58331cdceeb52133d780bc3adc9b4d1154277510f39be9d4fa78a4d9acd2d4a', '271a4ba5136a260f1eca0df3ab8b247dc8dfb7c2', 850)}


def check_tree(reader=port_evidence.read_source):
    commit = reader(BASE + "/commit.git")
    require(port_evidence.digest(commit) == COMMIT_SHA
            and port_evidence.git_object("commit", commit) == PIN
            and commit.startswith(f"tree {TREE}\n".encode()), "exact retained CAT commit/tree identity")
    raw_tree = reader(TREE_PATH)
    require(port_evidence.digest(raw_tree) == TREE_SHA, "exact complete CAT Git tree snapshot")
    tree = port_evidence.load_json(raw_tree)
    require(tree["repository"] == "Chia-Network/chia_puzzles" and tree["revision"] == PIN
            and tree["root_tree_sha1"] == TREE and len(tree["entries"]) == 234,
            "closed pinned complete CAT tree profile")
    validator = _load(ROOT / "corpus/validate_published_puzzles.py", "_cat_published_tree_validator")
    validator.check_tree(tree)
    entries = {row["path"]: row for row in tree["entries"]}
    for path, (sha, blob, length) in PINS.items():
        raw = reader(BASE + "/" + path)
        require(len(raw) == length and port_evidence.digest(raw) == sha
                and port_evidence.git_object("blob", raw) == blob, "exact pinned CAT source/license/include/artifact")
        entry = entries[path]
        require(entry["kind"] == "blob" and entry["mode"] == "100644"
                and entry["git_sha1"] == blob and entry["size_bytes"] == length,
                "selected CAT file belongs to the complete pinned tree")
    return len(PINS)


def validate_component(puzzle):
    """Check all five actual main journals; source mutants remain separate."""
    require(puzzle.get("slug") == SLUG and puzzle.get("kind") == "protocol_component"
            and puzzle.get("environment") == "raw" and puzzle.get("validated") is True,
            "actual raw CAT protocol component")
    require(puzzle.get("simulator") == [] and puzzle.get("simulator_comparisons") == [],
            "dedicated CAT groups are separate from generic scenario counts")
    manifest = port_evidence.load_manifest(SLUG)
    check_tree()
    provenance = manifest["provenance"]
    require(provenance["classification"] == "canonical_puzzle_port"
            and provenance["upstream"]["revision"] == PIN
            and provenance["upstream"]["root_tree_sha1"] == TREE
            and provenance["upstream"]["source_path"] == "puzzles/cat_puzzles/genesis_by_coin_id.clsp"
            and provenance["upstream"]["snapshot"] == BASE + "/puzzles/cat_puzzles/genesis_by_coin_id.clsp"
            and provenance["license"]["spdx"] == "Apache-2.0"
            and provenance["tree"]["manifest"] == TREE_PATH
            and provenance["tree"]["sha256"] == TREE_SHA
            and provenance["commit"]["snapshot"] == BASE + "/commit.git"
            and provenance["commit"]["git_commit_sha1"] == PIN,
            "manifest binds the exact canonical Git provenance")
    require({BASE + "/" + name for name in PINS} <= set(manifest["source_dependencies"])
            and manifest["reference_includes"] == {"cat_truths.clib": BASE + "/puzzles/cat_puzzles/cat_truths.clib"}
            and provenance["source_adaptation"]["kind"] == "none", "complete retained source/include input declaration")
    port_evidence.check_pins(provenance)
    port_evidence.check_identity(puzzle, manifest)
    require(puzzle["reference"].encode() == port_evidence.read_source(BASE + "/puzzles/cat_puzzles/genesis_by_coin_id.clsp"),
            "reference preserves the complete original canonical source")
    value = puzzle.get("protocol_validation")
    closed(value, TOP_FIELDS, "CAT protocol")
    require(type(value["schema_version"]) is int and value["schema_version"] == 1
            and value["protocol"] == PROTOCOL and value["status"] == "validated", "CAT schema/protocol/status")
    for key, expected in PROFILE.items():
        require(type(value[key]) is type(expected) and value[key] == expected, "pinned CAT execution profile: " + key)
    require(type(value["scope"]) is str and value["scope"], "explicit finite evidence scope")
    canonical = compiled_raw(value["canonical_tail_hex"])
    cat = compiled_raw(value["cat_module_hex"])
    require(canonical == bytes.fromhex(read(DIRECTORY / "upstream/genesis_by_coin_id.clsp.hex").decode().strip())
            and len(canonical) == 45 and codec.identity(canonical)["module_tree_hash"] == value["canonical_tail_tree_hash"] == lifecycle.TAIL,
            "exact independently retained canonical 45-byte TAIL")
    require(cat == bytes.fromhex(read(DIRECTORY / "upstream/cat_v2.clsp.hex").decode().strip())
            and len(cat) == 1672 and codec.identity(cat)["module_tree_hash"] == value["cat_module_tree_hash"] == lifecycle.CAT,
            "exact independently retained canonical CAT2 module")
    artifacts = puzzle["artifacts"]
    builds = value["builds"]
    require(type(artifacts) is list and type(builds) is list and len(artifacts) == len(builds) == 5,
            "exactly five artifacts and five original lifecycle builds")
    require([(row["backend"], row["optimize_lir"]) for row in artifacts] == MODES
            and [(row["backend"], row["optimize_lir"]) for row in builds] == MODES,
            "complete independent Rust/Chialisp/Rue modes")
    totals = dict.fromkeys(COUNTS, 0)
    for artifact, build in zip(artifacts, builds):
        closed(build, {"backend", "optimize_lir", "module_tree_hash", "lifecycle"}, "CAT build")
        require(type(build["optimize_lir"]) is bool and type(artifact["optimize_lir"]) is bool,
                "explicit Boolean compiler modes")
        module = compiled_raw(artifact["clvm_hex"])
        identity = codec.identity(module)
        require(identity["module_tree_hash"] == artifact["tree_hash"] == build["module_tree_hash"]
                and type(artifact["bytes"]) is int and artifact["bytes"] == len(module),
                "own complete selected artifact identity")
        if artifact["backend"] == "chialisp":
            require(module == canonical, "independently compiled unchanged canonical TAIL identity")
        else:
            require(identity["module_tree_hash"] != lifecycle.TAIL, "Rust/Rue module identities remain distinct")
        envelope = {key: value[key] for key in ("sdk", "consensus", "sdk_vm", "network", "primary_vm", "primary_protocol",
                    "primary_max_cost", "cat_module_hex", "cat_module_tree_hash", "owner_public_key", "recipient_public_key")}
        envelope.update(protocol="chialisp-oxide-canonical-cat-genesis-lifecycle/v1", status="executed",
                        tail_module_hex=module.hex(), lifecycle=build["lifecycle"])
        counts = lifecycle.validate_main_lifecycle(envelope, admitted_tail=module, max_cost=PROFILE["primary_max_cost"])
        for key, count in counts.items():
            totals[key] += count
    require(totals == COUNTS and all(type(value[key]) is int and value[key] == count for key, count in totals.items()),
            "exact five-group CAT attempt/outcome/component counts")
    return {"groups": 5, "attempts": totals["sdk_transaction_attempts"], "accepted": totals["accepted"],
            "rejected": totals["typed_rejected"], "runtime_umbrellas": totals["runtime_umbrellas"],
            "vm": totals["component_primary_vm_runs"], "scope": "Retained consistency only; no new execution or authentication."}


def self_test(puzzle):
    validate_component(puzzle)
    mutations = [
        lambda p: p.update(kind="pure_function"),
        lambda p: p["protocol_validation"].update(schema_version=True),
        lambda p: p["protocol_validation"].update(primary_flags=0),
        lambda p: p["protocol_validation"]["builds"].pop(),
        lambda p: p["protocol_validation"]["builds"][0].update(module_tree_hash=lifecycle.TAIL),
        lambda p: p["protocol_validation"]["builds"][0]["lifecycle"].update(asset_id=lifecycle.TAIL),
        lambda p: p["protocol_validation"]["builds"][0]["lifecycle"]["attempts"].pop(),
        lambda p: p["protocol_validation"]["builds"][0]["lifecycle"]["attempts"][1]["outcome"].update(full_simulator_equality=False),
        lambda p: p["protocol_validation"]["builds"][0]["lifecycle"]["attempts"][1]["outcome"].update(full_simulator_equality=1),
        lambda p: p["protocol_validation"]["builds"][0]["lifecycle"]["attempts"][5]["outcome"].update(full_simulator_equality=1),
        lambda p: p["protocol_validation"]["builds"][0]["lifecycle"]["attempts"][5]["outcome"].update(semantic_rejection_credit=0),
        lambda p: p["protocol_validation"]["builds"][0]["lifecycle"]["attempts"][1]["outcome"].update(code="DoubleSpend"),
        lambda p: p["protocol_validation"]["builds"][0]["lifecycle"]["attempts"][5]["outcome"].update(semantic_rejection_credit=True),
        lambda p: p["protocol_validation"]["builds"][0]["lifecycle"]["attempts"][5]["primary_vm"][0].update(failure={"kind":"resource","code":"cost_exceeded"}),
        lambda p: p["protocol_validation"]["builds"][0]["lifecycle"]["attempts"][12]["transaction"].update(signature_hex=lifecycle.EMPTY_SIGNATURE),
    ]
    for mutate in mutations:
        changed = copy.deepcopy(puzzle)
        mutate(changed)
        try:
            validate_component(changed)
        except (ValueError, KeyError, TypeError, IndexError):
            continue
        raise ValueError("escaped CAT corpus reader corruption")
    return len(mutations)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=ROOT / "corpus/validation.json")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    report = port_evidence.load_json(read(args.report))
    selected = [p for p in report["puzzles"] if p["slug"] == SLUG]
    require(len(selected) == 1, "exactly one actual CAT component required")
    result = validate_component(selected[0])
    if args.self_test:
        print("PASS", self_test(selected[0]), "constructed retained-reader corruptions; no execution credit")
    print(json.dumps(result, indent=2))
