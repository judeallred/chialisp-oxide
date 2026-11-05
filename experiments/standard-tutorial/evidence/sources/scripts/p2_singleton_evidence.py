"""Closed finite canonical-counterpart ledger records; never an execution proof.

Checks declared source/retained bytes and recorded typed consensus receipts.
Full Simulator equality is checked in memory by Rust at submission; this reader
validates that recorded Boolean, typed receipts and transaction bindings. Reading
the Boolean does not prove state equality or authenticate execution. Private SDK
Debug fields are forbidden and are never treated as state serialization.
"""
from collections import Counter
import hashlib
import json
from pathlib import Path
import re

from cargo_authoring import contained_file, unique_object
from password_evidence import artifact_bytes
from password_vectors import CODEC, MODES, deserialize, proper, serialize

ROOT = Path(__file__).resolve().parents[1]
PROTOCOL = "chialisp-oxide-p2-singleton/v1"
REJECTION = "chia-consensus-0.36.1/p2-singleton-v1"
TABLE_PATH = "crates/chialisp-oxide-simulator/p2-singleton-rejection-protocol.json"
REVISION = "cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8"
UPSTREAM = f"corpus/upstream/chia-puzzles/{REVISION}"
NEGATIVES = [
    ("missing singleton participant", "AssertPuzzleAnnouncementFailed"),
    ("missing p2 participant with independently balanced singleton", "AssertCoinAnnouncementFailed"),
    ("wrong inner-puzzle hash", "AssertPuzzleAnnouncementFailed"),
    ("wrong my_id with announcement otherwise matched", "AssertMyCoinIdFailed"),
    ("another p2 coin ID cannot identify this input", "AssertMyCoinIdFailed"),
    ("wrong singleton puzzle-announcement message", "AssertPuzzleAnnouncementFailed"),
    ("wrong dollar announcement binding", "AssertCoinAnnouncementFailed"),
    ("wrong p2 issuer in reciprocal coin announcement", "AssertCoinAnnouncementFailed"),
    ("wrong singleton module identity", "AssertPuzzleAnnouncementFailed"),
    ("wrong launcher identity", "AssertPuzzleAnnouncementFailed"),
    ("wrong launcher puzzle hash", "AssertPuzzleAnnouncementFailed"),
    ("another puzzle cannot issue the singleton announcement", "AssertPuzzleAnnouncementFailed"),
    ("omitted signature submitted to consensus", "BadAggregateSignature"),
    ("nonempty genesis signature cannot authorize singleton spend", "BadAggregateSignature"),
    ("signed continuation recipient cannot be changed", "BadAggregateSignature"),
    ("singleton asserted input amount must match", "AssertMyAmountFailed"),
    ("different-language reveal submitted to consensus", "WrongPuzzleHash"),
]
LIFECYCLE_NEGATIVES = [("joint-spend replay", "DoubleSpend"),
                       ("Eve proof cannot spend the next singleton", "AssertMyParentIdFailed")]
RECEIPTS = ["genesis", "joint", "lineage"]
MODULES = [
    ("puzzles/singleton_launcher.clsp", 175, "028b1dd7a1fe4bfad7165236b3a4076c6a1bb022aa6db0f9b5dac386299efbbb", "f516e4f9b16eccb84042462735b5e49a7fff85d7975f74f43c7808d360131540", "eff07522495060c066f66f32acc2a77e3a3e737aca8baea4d1a64ea4cdc13da9"),
    ("puzzles/singleton_top_layer_v1_1.clsp", 967, "634ffaee9b2876133e4dfddffb5b61abd5e8b10cc37c48fd0515ed56dd86b04a", "6bd2546a352cf58e8a1c81ddd64f9ad3ed0a58d7e18e2e8cab5171670ab10538", "7faa3253bfddd1e0decb0906b2dc6247bbc4cf608f58345d173adb63e8b47c9f"),
    ("puzzles/p2_delegated_puzzle_or_hidden_puzzle.clsp", 227, "8a766b6979bd915b08143ddb702ac386dc859fe525506f634e5dd528ead7f9da", "81ac2c9256e356df768c1fb6819cc4d157c733cfdd51881c51cf3c61f424fd66", "e9aaa49f45bad5c889b86ee3341550c155cfdd10c3a6757de618d20612fffd52"),
]

# Every runtime file opened by public Modules::load, including compiler includes.
INCLUDES = {
    "puzzles/condition_codes.clib":"891ea562e6fdb783bcfceea0cdea92318acfe4a67c4b6deb238d5fc4af15377f",
    "puzzles/curry-and-treehash.clib":"5132c3d2428264deb1c405a1debf7a75a84495f96717b0795f18ea0f0f16120e",
    "puzzles/singleton_truths.clib":"422098da12e4992d05a51c69003a9c0c6b8c22534bbc3b0b61f8c611eb9f62ae",
    "puzzles/utility_macros.clib":"f58331cdceeb52133d780bc3adc9b4d1154277510f39be9d4fa78a4d9acd2d4a",
}
RUNTIME_INPUTS = {f"{UPSTREAM}/{path}":sha for path,sha in INCLUDES.items()}
RUNTIME_INPUTS.update({f"{UPSTREAM}/{path}{suffix}":sha
    for path,_,source_sha,hex_sha,_ in MODULES for suffix,sha in (("",source_sha),(".hex",hex_sha))})


def require(condition, message):
    if not condition:
        raise ValueError(message)


def closed(value, keys, label):
    require(type(value) is dict and set(value) == set(keys), label + ": missing/unknown fields")


def hex_bytes(value, *, size=None):
    require(type(value) is str and len(value) <= 4_000_000 and len(value) % 2 == 0
            and re.fullmatch(r"[0-9a-f]*", value) and (size is None or len(value) == 2*size), "Malformed retained hex")
    return bytes.fromhex(value)


def tree(value):
    payload = hex_bytes(value)
    require(payload, "Missing serialized CLVM tree")
    result = deserialize(payload)
    require(serialize(result) == payload, "Noncanonical retained CLVM serialization")
    return result


def coin(value):
    closed(value, ["amount", "id", "parent_id", "puzzle_hash"], "coin")
    n = value["amount"]
    require(type(n) is int and 0 <= n < 2**64, "Malformed ledger amount")
    parent, puzzle = (hex_bytes(value[k], size=32) for k in ("parent_id", "puzzle_hash"))
    amount = n.to_bytes((n.bit_length()+7)//8, "big")
    if amount and amount[0] >= 128:
        amount = b"\0" + amount
    require(value["id"] == hashlib.sha256(parent+puzzle+amount).hexdigest(), "Ledger coin ID does not bind fields")
    return value


def bundle(value, *, accepted=False):
    closed(value, ["signature_hex", "spends"], "transaction bundle")
    hex_bytes(value["signature_hex"], size=96)
    require(type(value["spends"]) is list and 0 < len(value["spends"]) <= 4, "Missing/unbounded actual spends")
    ids = []
    for spend in value["spends"]:
        closed(spend, ["coin", "reveal_hex", "solution_hex"], "coin spend")
        c = coin(spend["coin"])
        reveal = tree(spend["reveal_hex"])
        tree(spend["solution_hex"])
        if accepted:
            require(CODEC["tree_hash"](reveal).hex() == c["puzzle_hash"], "Accepted reveal differs from committed coin")
        ids.append(c["id"])
    require(len(ids) == len(set(ids)), "Repeated coin within retained bundle")
    return ids


def protocol_table(root):
    value = json.loads(contained_file(root, TABLE_PATH).read_bytes(), object_pairs_hook=unique_object)
    expected = {"schema_version":1, "protocol":REJECTION, "stage":"consensus",
                "codes":sorted({code for _,code in NEGATIVES+LIFECYCLE_NEGATIVES}),
                "negatives":[{"name":name,"code":code} for name,code in NEGATIVES],
                "lifecycle_rejections":[{"name":name,"code":code} for name,code in LIFECYCLE_NEGATIVES],
                "accepted_receipts":RECEIPTS}
    require(value == expected and type(value["schema_version"]) is int, "Unreviewed scoped P2 consensus protocol")


def reject(value, name, code):
    closed(value, ["name", "expected", "observed", "stage", "full_simulator_equality", "transaction", "consensus"], "P2 rejection")
    require(value["name"] == name and value["expected"] == value["observed"] == code
            and value["stage"] == "actual_consensus_submission" and value["full_simulator_equality"] is True,
            "Missing/wrong typed actual submission and rollback")
    expected = {"stage":"consensus", "protocol":REJECTION, "code":code,
                "status":"rejected", "semantic_rejection_credit":True}
    require(value["consensus"] == expected and value["consensus"]["semantic_rejection_credit"] is True,
            "Wrong/nonconsensus rejection envelope or false semantic credit")
    bundle(value["transaction"])


def curry(module, arguments):
    environment = b"\1"
    for argument in reversed(arguments):
        environment = proper(b"\4", (b"\1", argument), environment)
    return proper(b"\2", (b"\1", module), environment)


def uncurry_one(program):
    def items(value, count):
        result = []
        for _ in range(count):
            require(type(value) is tuple and len(value) == 2, "Malformed standard curry")
            result.append(value[0]); value = value[1]
        require(value == b"", "Improper standard curry form")
        return result
    apply, quote, environment = items(program, 3)
    require(apply == b"\2" and type(quote) is tuple and quote[0] == b"\1", "Wrong standard apply/quote")
    cons, argument, tail = items(environment, 3)
    require(cons == b"\4" and type(argument) is tuple and argument[0] == b"\1" and tail == b"\1", "Wrong standard argument frame")
    return quote[1], argument[1]

def runtime_sources(puzzle, root):
    rows = puzzle["source_dependencies"]
    require(type(rows) is list and all(type(row) is dict and set(row) == {"path","sha256","source"} for row in rows),
            "Malformed P2 source dependency records")
    paths = [row["path"] for row in rows]
    require(len(paths) == len(set(paths)) and set(RUNTIME_INPUTS) <= set(paths),
            "P2 source dependencies omit native runtime-loader inputs")
    recorded = {row["path"]:row for row in rows}
    for path, expected in RUNTIME_INPUTS.items():
        payload = contained_file(root,path).read_bytes()
        require(hashlib.sha256(payload).hexdigest() == expected and recorded[path]["sha256"] == expected
                and recorded[path]["source"] == payload.decode(), "Stale P2 runtime-loader source/include/published bytes: " + path)


def counterparts(value, root):
    closed(value, ["modules", "repository", "revision", "root_tree_sha1", "scope"], "canonical counterparts")
    require(value["repository"] == "https://github.com/Chia-Network/chia_puzzles"
            and value["revision"] == REVISION and value["root_tree_sha1"] == "e131409ed769ef4def743d4ebbcf859b40fcec2d"
            and type(value["scope"]) is str and value["scope"] and len(value["modules"]) == len(MODULES), "Changed canonical counterpart identity/scope")
    programs = []
    for row, (path, size, source_sha, hex_sha, tree_hash) in zip(value["modules"], MODULES, strict=True):
        closed(row, ["path", "bytes", "source_sha256", "published_hex_sha256", "tree_hash", "independent_compilation_matches_published"], "counterpart module")
        require(row == {"path":path,"bytes":size,"source_sha256":source_sha,"published_hex_sha256":hex_sha,"tree_hash":tree_hash,"independent_compilation_matches_published":True}
                and type(row["bytes"]) is int and row["independent_compilation_matches_published"] is True, "Changed canonical counterpart measured identity")
        source = contained_file(root, f"{UPSTREAM}/{path}").read_bytes()
        published = contained_file(root, f"{UPSTREAM}/{path}.hex").read_bytes()
        require(hashlib.sha256(source).hexdigest() == source_sha and hashlib.sha256(published).hexdigest() == hex_sha,
                "Stale canonical counterpart source/published bytes")
        program = tree(published.decode().strip())
        require(len(bytes.fromhex(published.decode().strip())) == size and CODEC["tree_hash"](program).hex() == tree_hash,
                "Published canonical counterpart bytes/hash differ")
        programs.append(program)
    return programs


def validate_component(puzzle, *, root=ROOT):
    require(puzzle["slug"] == "p2-singleton" and puzzle["kind"] == "protocol_component"
            and puzzle["simulator"] == [] and puzzle["simulator_comparisons"] == [],
            "P2 composition must use the explicit protocol_component contract")
    value = puzzle["protocol_validation"]
    closed(value, ["schema_version", "protocol", "status", "scope", "simulator", "consensus", "ledger_VM", "network", "modules", "rejection_protocol", "observations"], "P2 protocol validation")
    require(type(value["schema_version"]) is int and value["schema_version"] == 1 and value["protocol"] == PROTOCOL
            and value["status"] == "validated" and type(value["scope"]) is str and value["scope"]
            and value["simulator"] == "chia-sdk-test 0.36.0" and value["consensus"] == "chia-consensus 0.36.1"
            and value["ledger_VM"] == "clvmr 0.16.2" and value["network"] == "TESTNET11"
            and value["rejection_protocol"] == REJECTION, "Changed P2 version/VM/network/scope")
    protocol_table(root)
    runtime_sources(puzzle,root)
    canonical = counterparts(value["modules"], root)
    artifacts = puzzle["artifacts"]
    require(type(artifacts) is list and [(a["backend"],a["optimize_lir"]) for a in artifacts] == MODES
            and all(type(a["optimize_lir"]) is bool for a in artifacts), "Missing five independent P2 builds")
    programs = [deserialize(artifact_bytes(a)) for a in artifacts]
    rows = value["observations"]
    require(type(rows) is list and [(r["backend"],r["optimize_lir"]) for r in rows] == MODES
            and all(type(r["optimize_lir"]) is bool for r in rows), "Missing/duplicate/reordered P2 ledger modes")
    counts = Counter()
    for row, artifact, module in zip(rows, artifacts, programs, strict=True):
        closed(row, ["backend", "optimize_lir", "module_tree_hash", "curried_p2_reveal_hex", "curried_p2_hash", "canonical_singleton_reveal_hex", "inner_hash", "negatives", "lifecycle"], "P2 group")
        require(row["module_tree_hash"] == artifact["tree_hash"], "Ledger group uses a different compiler module")
        lifecycle = row["lifecycle"]
        closed(lifecycle, ["name", "funding", "launcher", "eve", "p2", "continuation", "next_singleton", "genesis", "joint", "lineage", "rejections", "scope", "receipts"], "P2 lifecycle")
        require(lifecycle["name"] == "canonical Eve joint funding and next lineage spend"
                and type(lifecycle["scope"]) is str and lifecycle["scope"], "Changed finite lifecycle scope")
        coins = [coin(lifecycle[k]) for k in ["funding","launcher","eve","p2","continuation","next_singleton"]]
        funding, launcher, eve, p2, continuation, child = coins
        require([c["amount"] for c in coins] == [3,1,1,2,3,3]
                and launcher["parent_id"] == p2["parent_id"] == funding["id"]
                and eve["parent_id"] == launcher["id"] and continuation["parent_id"] == eve["id"]
                and child["parent_id"] == continuation["id"] and eve["puzzle_hash"] == continuation["puzzle_hash"] == child["puzzle_hash"],
                "Changed actual Eve/funding/lineage coin relationships")
        require(launcher["puzzle_hash"] == MODULES[0][4], "Wrong canonical launcher")
        standard = tree(lifecycle["genesis"]["spends"][0]["reveal_hex"])
        standard_module, public_key = uncurry_one(standard)
        require(standard_module == canonical[2] and type(public_key) is bytes and len(public_key) == 48,
                "Signed inner puzzle is not the canonical StandardLayer curry")
        require(funding["puzzle_hash"] == row["inner_hash"] == CODEC["tree_hash"](standard).hex(),
                "Inner commitment differs from the actual signed funding reveal")
        singleton = curry(canonical[1], [(bytes.fromhex(MODULES[1][4]), (bytes.fromhex(launcher["id"]), bytes.fromhex(MODULES[0][4]))), standard])
        own = curry(module, [bytes.fromhex(MODULES[1][4]), bytes.fromhex(launcher["id"]), bytes.fromhex(MODULES[0][4])])
        require(tree(row["canonical_singleton_reveal_hex"]) == singleton
                and CODEC["tree_hash"](singleton).hex() == eve["puzzle_hash"], "Wrong canonical top-layer/StandardLayer counterpart")
        require(tree(row["curried_p2_reveal_hex"]) == own and row["curried_p2_hash"] == p2["puzzle_hash"]
                and CODEC["tree_hash"](own).hex() == p2["puzzle_hash"], "P2 ledger reveal is not standard curry of its own module")
        require(type(row["negatives"]) is list and len(row["negatives"]) == len(NEGATIVES)
                and type(lifecycle["rejections"]) is list and len(lifecycle["rejections"]) == len(LIFECYCLE_NEGATIVES), "Missing P2 rejection coverage")
        for attempt, (name, code) in zip(row["negatives"]+lifecycle["rejections"], NEGATIVES+LIFECYCLE_NEGATIVES, strict=True):
            reject(attempt, name, code)
            counts["rejected"] += 1
            counts[code] += 1
        expected_receipts = [{"name":name,"stage":"actual_consensus_submission","status":"accepted","bundle_field":name} for name in RECEIPTS]
        require(lifecycle["receipts"] == expected_receipts, "Missing/unknown accepted actual-submission receipts")
        wanted = {"genesis":[funding["id"],launcher["id"]],"joint":[eve["id"],p2["id"]],"lineage":[continuation["id"]]}
        for receipt in lifecycle["receipts"]:
            require(bundle(lifecycle[receipt["bundle_field"]], accepted=True) == wanted[receipt["name"]], "Accepted bundle differs from the named actual inputs")
            counts["accepted"] += 1
        # Selected negative controls retain actual empty/wrong nonempty signatures.
        require(row["negatives"][12]["transaction"]["signature_hex"] == "c0"+"00"*95
                and row["negatives"][13]["transaction"]["signature_hex"] == lifecycle["genesis"]["signature_hex"]
                and row["negatives"][13]["transaction"]["signature_hex"] != "c0"+"00"*95,
                "Signature rejection lacks actual submitted empty/nonempty wrong bundles")
        counts["groups"] += 1
    require((counts["groups"], counts["accepted"], counts["rejected"]) == (5,15,95), "Incomplete computed P2 submissions")
    return {"groups":counts["groups"], "actual_recorded_submissions":counts["accepted"]+counts["rejected"],
            "accepted":counts["accepted"], "typed_rejected_with_recorded_full_rollback":counts["rejected"],
            "codes":{code:counts[code] for code in sorted({c for _,c in NEGATIVES+LIFECYCLE_NEGATIVES})},
            "scope":"Recorded finite canonical-counterpart composition, typed receipts and transaction bindings; Rust checks full Simulator equality in memory at submission, and this reader checks its recorded Boolean; checking that Boolean does not prove state equality or authenticate execution, and no state serialization is claimed; no full lifecycle or compiler-preservation proof"}
