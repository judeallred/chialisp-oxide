"""Independent retained counter-component journals; no execution credit.

Reconstructs complete funding, singleton lineage, action selection and outputs
using Python hashes and bounded tree codecs. It does not verify BLS signatures,
authenticate captured execution or prove compiler preservation.
"""
import argparse
import copy
import importlib.util
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("_counter_main_oracle", ROOT / "experiments/action-counter/protocol_oracle.py")
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)
codec = oracle.codec
require = oracle.require
proper, uint, treehash, curry = oracle.proper, oracle.uint, oracle.treehash, oracle.curry
SLUGS = ("counter-increment", "counter-reset")
MODES = [("rust_mir", False), ("rust_mir", True), ("chialisp", True), ("rue_source", False), ("rue_source", True)]
OWNER = "b1b01d4389e60a12b1c9312312f706825184320ed91f2a8f16510cbea3ea900f4e6b465a045781841997b209017fb6a4"
EMPTY_SIGNATURE = "c0" + "00" * 95
PROFILE = {"sdk": "chia-sdk-test 0.36.0", "consensus": "chia-consensus 0.36.1", "sdk_vm": "clvmr 0.16.2", "network": "TESTNET11", "primary_vm": "clvmr 0.21.0", "primary_protocol": "clvmr-0.21.0/v1", "primary_flags": 535, "primary_max_cost": 250_000_000, "sdk_max_block_cost_clvm": 11_000_000_000, "sdk_transaction_max_cost": 5_500_000_000}
COUNT_KEYS = {"sdk_transaction_attempts", "accepted", "typed_rejected", "runtime_umbrellas", "component_primary_vm_runs"}
CANONICAL = [
    ("action_layer", 630, "afa03f2903f9e4a293523237799074b13ab2361f250df10de5df884b0b09a22b"),
    ("default_finalizer", 494, "9d274de59aeaa128d1b0a802b52911838420ef9720d605c78942e8dadfc0810b"),
    ("singleton_launcher", 175, "eff07522495060c066f66f32acc2a77e3a3e737aca8baea4d1a64ea4cdc13da9"),
    ("singleton_top_layer_v1_1", 967, "7faa3253bfddd1e0decb0906b2dc6247bbc4cf608f58345d173adb63e8b47c9f"),
]


def closed(value, fields, label):
    require(type(value) is dict and set(value) == set(fields), "exact " + label + " fields")


def exact(value, expected, label):
    require(type(value) is type(expected) and value == expected, label)


def raw(text, limit=codec.MAX_BYTES):
    require(type(text) is str and 0 < len(text) <= 2 * limit and re.fullmatch(r"(?:[0-9a-f]{2})+", text), "bounded canonical hexadecimal")
    return bytes.fromhex(text)


def pinned_components(rows):
    require(type(rows) is list and len(rows) == 4, "four exact canonical modules")
    modules = []
    for row, (name, length, digest) in zip(rows, CANONICAL):
        closed(row, {"name", "module_hex", "bytes", "module_tree_hash"}, "canonical component")
        exact(row["name"], name, "canonical module order")
        blob = raw(row["module_hex"])
        exact(row["bytes"], length, "canonical module byte count")
        require(len(blob) == length and codec.identity(blob)["module_tree_hash"] == row["module_tree_hash"] == digest, "pinned canonical module tree")
        modules.append(codec.decode(blob))
    standard = json.loads(oracle.bounded_read(ROOT / "crates/chialisp-oxide-package/tests/fixtures/mainnet-standard/expected.json", codec.MAX_BYTES))
    blob = raw(standard["module_hex"])
    require(codec.identity(blob)["module_tree_hash"] == "e9aaa49f45bad5c889b86ee3341550c155cfdd10c3a6757de618d20612fffd52", "standard wallet pin")
    return (*modules, codec.decode(blob))


def signature(transaction, empty=None):
    blob = raw(transaction["signature_hex"], 96)
    require(len(blob) == 96, "aggregate signature byte count")
    if empty is not None:
        exact(transaction["signature_hex"] == EMPTY_SIGNATURE, empty, "expected empty/nonempty signature")
    return transaction["signature_hex"]


def check_coin(row, parent=None, puzzle=None):
    closed(row, {"coin_id", "parent_coin_id", "puzzle_hash", "amount"}, "coin")
    for field in ("coin_id", "parent_coin_id", "puzzle_hash"):
        require(len(raw(row[field], 32)) == 32, "exact coin hash width")
    return oracle.check_coin(row, parent, puzzle)


def spend(actual, expected_coin, reveal, solution):
    closed(actual, {"coin", "reveal_hex", "solution_hex"}, "spend")
    require(actual["coin"] == expected_coin, "exact actual spent coin")
    oracle.exact_hex(actual["reveal_hex"], reveal, "exact committed reveal")
    oracle.exact_hex(actual["solution_hex"], solution, "exact independent solution")


def attempt(row, name, expected_spends, outputs, outcome, program_codes):
    closed(row, {"name", "transaction", "primary_vm", "outcome"}, "attempt")
    exact(row["name"], name, "ordered causal scenario")
    closed(row["transaction"], {"signature_hex", "spends"}, "transaction")
    require(type(row["transaction"]["spends"]) is list and len(row["transaction"]["spends"]) == len(expected_spends), "complete transaction spend inventory")
    for actual, (coin, reveal, solution) in zip(row["transaction"]["spends"], expected_spends):
        spend(actual, coin, reveal, solution)
    executions = row["primary_vm"]
    require(type(executions) is list and len(executions) == len(outputs), "exact component VM count")
    for execution, expected_output in zip(executions, outputs):
        closed(execution, {"outcome", "cost", "failure"}, "primary VM execution")
        if expected_output is None:
            require(execution["outcome"] == {"status": "rejected"} and execution["cost"] is None, "runtime control primary rejection")
            failure = execution["failure"]
            closed(failure, {"code", "kind"}, "typed failure")
            require(failure["kind"] == "program" and failure["code"] in program_codes, "resource/infrastructure failure is inconclusive")
        else:
            closed(execution["outcome"], {"status", "output_hex"}, "accepted VM outcome")
            oracle.check_execution(execution, expected_output)
    require(row["outcome"] == outcome, "exact ledger outcome and unchanged state marker")
    if outcome["status"] == "rejected":
        exact(row["outcome"]["complete_state_unchanged"], True, "Boolean complete-state equality")
    signature(row["transaction"])


def rejected(code, runtime=False):
    return {"status": "rejected", "stage": "runtime_umbrella" if runtime else "consensus", "code": code, "complete_state_unchanged": True}


def check_lifecycle(journal, slug, increment, reset, pinned, program_codes):
    fields = {"selected_action", "initial_state", "selected_child_state", "continued_child_state", "owner_public_key", "merkle_root", "increment_module_hex", "reset_template_hex", "genesis", "launcher", "eve", "selected_child", "continued_child", "attempts", *COUNT_KEYS}
    closed(journal, fields, "lifecycle")
    selected = SLUGS.index(slug)
    state = 6 if selected == 0 else 0
    for key, value in {"selected_action": slug, "initial_state": 5, "selected_child_state": state, "continued_child_state": state + 1, "owner_public_key": OWNER}.items():
        exact(journal[key], value, "fixed lifecycle " + key)
    oracle.exact_hex(journal["increment_module_hex"], increment, "actual selected/counterpart increment")
    oracle.exact_hex(journal["reset_template_hex"], reset, "actual selected/counterpart reset template")
    owner = bytes.fromhex(OWNER)
    wallet = curry(pinned[4], owner)
    genesis_id = check_coin(journal["genesis"], puzzle=treehash(wallet))
    launcher_id = check_coin(journal["launcher"], genesis_id, treehash(pinned[2]))
    policy = oracle.Policy(increment, reset, owner, launcher_id, pinned=pinned)
    exact(journal["merkle_root"], policy.root.hex(), "selected/counterpart committed root")
    eve_reveal = policy.full(5)
    eve_id = check_coin(journal["eve"], launcher_id, treehash(eve_reveal))
    child_id = check_coin(journal["selected_child"], eve_id, treehash(policy.full(state)))
    check_coin(journal["continued_child"], child_id, treehash(policy.full(state + 1)))
    launch_solution = proper(treehash(eve_reveal), uint(1), b"")
    announcement = oracle.sha(launcher_id + treehash(launch_solution))
    funding_conditions = proper(proper(uint(51), treehash(pinned[2]), uint(1)), proper(uint(61), announcement))
    delegated = (uint(1), funding_conditions)
    funding_solution = proper(b"", delegated, b"")
    funding_outputs = proper(proper(uint(50), owner, treehash(delegated)), proper(uint(51), treehash(pinned[2]), uint(1)), proper(uint(61), announcement))
    launcher_outputs = proper(proper(uint(51), treehash(eve_reveal), uint(1)), proper(uint(60), treehash(launch_solution)))
    eve_proof = proper(genesis_id, uint(1))
    solution = proper(eve_proof, uint(1), policy.solution([selected]))
    correct_spend = (journal["eve"], eve_reveal, solution)
    bad_path = [True, True, False, True, False, False, True, True, False]
    invalid_solution = oracle.replace_path(solution, bad_path, b"\x45" * 32)
    rows = journal["attempts"]
    expected_count = 5 if selected == 0 else 7
    require(type(rows) is list and len(rows) == expected_count, "exact five/seven main attempts")
    accepted = {"status": "accepted"}
    attempt(rows[0], "fund_and_launch_singleton", [(journal["genesis"], wallet, funding_solution), (journal["launcher"], pinned[2], launch_solution)], [funding_outputs, launcher_outputs], accepted, program_codes)
    signature(rows[0]["transaction"], False)
    attempt(rows[1], "wrong_membership_sibling", [(journal["eve"], eve_reveal, invalid_solution)], [None], rejected("GeneratorRuntimeError", True), program_codes)
    signature(rows[1]["transaction"], True)
    index = 2
    expected_selected = policy.conditions(5, [selected], full=True, parent=launcher_id)
    if selected == 1:
        for name, empty in [("missing_reset_signature", True), ("wrong_key_reset_signature", False)]:
            attempt(rows[index], name, [correct_spend], [expected_selected], rejected("BadAggregateSignature"), program_codes)
            signature(rows[index]["transaction"], empty)
            index += 1
    attempt(rows[index], "selected_action", [correct_spend], [expected_selected], accepted, program_codes)
    valid_signature = signature(rows[index]["transaction"], selected == 0)
    if selected == 1:
        require(valid_signature != rows[3]["transaction"]["signature_hex"], "wrong signature differs from accepted signature")
    index += 1
    attempt(rows[index], "consumed_bundle_replay", [correct_spend], [expected_selected], rejected("DoubleSpend"), program_codes)
    exact(rows[index]["transaction"]["signature_hex"], valid_signature, "replay exact accepted aggregate")
    lineage = proper(launcher_id, treehash(policy.inner(5)), uint(1))
    continuation = proper(lineage, uint(1), policy.solution([0]))
    attempt(rows[index + 1], "increment_actual_child", [(journal["selected_child"], policy.full(state), continuation)], [policy.conditions(state, [0], full=True, parent=eve_id)], accepted, program_codes)
    signature(rows[index + 1]["transaction"], True)
    counts = {"sdk_transaction_attempts": expected_count, "accepted": 3, "typed_rejected": expected_count - 4, "runtime_umbrellas": 1, "component_primary_vm_runs": expected_count + 1}
    for key, value in counts.items():
        exact(journal[key], value, "exact lifecycle " + key)
    return counts


def validate_component(puzzle, reference_modules):
    slug = puzzle.get("slug")
    require(slug in SLUGS and puzzle.get("kind") == "protocol_component" and puzzle.get("environment", "strict") == "strict" and puzzle.get("validated") is True, "strict typed original counter protocol component")
    require(puzzle.get("simulator") == [] and puzzle.get("simulator_comparisons") == [], "dedicated counter journals separate from generic groups")
    selected = SLUGS.index(slug)
    require(type(reference_modules) is dict and set(reference_modules) == set(SLUGS), "exact two independently compiled references")
    value = puzzle["protocol_validation"]
    closed(value, {"schema_version", "protocol", "status", "selected_action", *PROFILE, "owner_public_key", "canonical_components", "builds", *COUNT_KEYS, "scope"}, "protocol")
    for key, expected in {"schema_version": 1, "protocol": "chialisp-oxide-action-counter/v1", "status": "validated", "selected_action": slug, "owner_public_key": OWNER, **PROFILE}.items():
        exact(value[key], expected, "pinned counter profile " + key)
    require(type(value["scope"]) is str and bool(value["scope"]), "explicit finite evidence scope")
    pinned = pinned_components(value["canonical_components"])
    failure_protocol = json.loads(oracle.bounded_read(ROOT / "crates/chialisp-oxide-vm/failure-protocol.json", 65536))
    program_codes = {row["code"] for row in failure_protocol["failures"] if row["kind"] == "program"}
    builds, artifacts = value["builds"], puzzle["artifacts"]
    require(type(builds) is list and type(artifacts) is list and len(builds) == len(artifacts) == 5, "five complete counter builds/artifacts")
    require([(r["backend"], r["optimize_lir"]) for r in builds] == [(r["backend"], r["optimize_lir"]) for r in artifacts] == MODES, "five independent language modes")
    total = dict.fromkeys(COUNT_KEYS, 0)
    references = {name: codec.decode(blob) for name, blob in reference_modules.items()}
    for artifact, build in zip(artifacts, builds):
        closed(build, {"backend", "optimize_lir", "selected_action", "module_tree_hash", "counterpart", "lifecycle"}, "build")
        exact(build["optimize_lir"], artifact["optimize_lir"], "selected Boolean mode")
        require(type(build["optimize_lir"]) is bool, "Boolean mode")
        exact(build["selected_action"], slug, "selected side")
        blob = raw(artifact["clvm_hex"])
        identity = codec.identity(blob)
        require(identity["module_tree_hash"] == artifact["tree_hash"] == build["module_tree_hash"], "complete selected artifact tree identity")
        exact(artifact["bytes"], len(blob), "selected artifact bytes")
        if artifact["backend"] == "chialisp":
            require(blob == reference_modules[slug], "selected independently compiled reference artifact")
        counterpart = build["counterpart"]
        closed(counterpart, {"action", "backend", "optimize_lir", "module_hex", "module_tree_hash"}, "counterpart")
        name = SLUGS[1 - selected]
        exact(counterpart["action"], name, "counterpart side")
        exact(counterpart["backend"], "chialisp", "independent Chialisp counterpart")
        exact(counterpart["optimize_lir"], True, "counterpart compiler mode")
        require(counterpart["module_hex"] == reference_modules[name].hex() and counterpart["module_tree_hash"] == codec.identity(reference_modules[name])["module_tree_hash"], "counterpart equals other entry's actual reference artifact")
        modules = [references[name] for name in SLUGS]
        modules[selected] = codec.decode(blob)
        counts = check_lifecycle(build["lifecycle"], slug, *modules, pinned, program_codes)
        for key in total:
            total[key] += counts[key]
    for key, expected in total.items():
        exact(value[key], expected, "five-build aggregate " + key)
    return {"groups": 5, **total}


def validate_report(report, *, partial=False):
    require(type(partial) is bool and type(report) is dict and type(report.get("puzzles")) is list, "explicit report scope")
    if partial:
        require(len(report["puzzles"]) == 2, "partial report contains exactly two entries")
    entries = {}
    references = {}
    for name in SLUGS:
        matches = [p for p in report["puzzles"] if p.get("slug") == name]
        require(len(matches) == 1, "unique counter entry " + name)
        entries[name] = matches[0]
        rows = [a for a in matches[0]["artifacts"] if a["backend"] == "chialisp" and a["optimize_lir"] is True]
        require(len(rows) == 1, "unique actual Chialisp reference " + name)
        references[name] = raw(rows[0]["clvm_hex"])
    counts = {name: validate_component(entries[name], references) for name in SLUGS}
    return {"entries": counts, "partial": partial, "scope": "Retained journal consistency only; no new native, compiler, VM, SDK or network execution and no execution authentication."}


def self_test(report, *, partial=False):
    validate_report(report, partial=partial)
    def protocol(value):
        return next(p for p in value["puzzles"] if p["slug"] == SLUGS[0])["protocol_validation"]
    mutations = [
        lambda r: next(p for p in r["puzzles"] if p["slug"] == SLUGS[0]).update(environment="raw"),
        lambda r: protocol(r).update(schema_version=True),
        lambda r: protocol(r).update(primary_flags=0),
        lambda r: protocol(r)["builds"].pop(),
        lambda r: protocol(r)["builds"][0].update(module_tree_hash="00" * 32),
        lambda r: protocol(r)["builds"][0]["counterpart"].update(backend="rust_mir"),
        lambda r: protocol(r)["builds"][0]["counterpart"].update(module_hex="80"),
        lambda r: protocol(r)["builds"][0]["lifecycle"].update(initial_state=4),
        lambda r: protocol(r)["builds"][0]["lifecycle"]["attempts"].reverse(),
        lambda r: protocol(r)["builds"][0]["lifecycle"]["attempts"][1]["outcome"].update(complete_state_unchanged=1),
        lambda r: protocol(r)["builds"][0]["lifecycle"]["attempts"][1]["primary_vm"][0].update(failure={"kind":"resource","code":"cost_exceeded"}),
        lambda r: protocol(r)["builds"][0]["lifecycle"]["attempts"][3]["outcome"].update(code="BadAggregateSignature"),
        lambda r: protocol(r)["builds"][0]["lifecycle"]["selected_child"].update(parent_coin_id="00" * 32),
        lambda r: protocol(r)["builds"][0]["lifecycle"]["attempts"][4]["primary_vm"][0]["outcome"].update(output_hex="80"),
        lambda r: protocol(r)["canonical_components"][0].update(module_hex="80"),
        lambda r: protocol(r).update(sdk_transaction_attempts=26),
    ]
    for mutate in mutations:
        changed = copy.deepcopy(report)
        mutate(changed)
        try:
            validate_report(changed, partial=partial)
        except (ValueError, KeyError, TypeError, IndexError):
            continue
        raise ValueError("escaped constructed counter journal corruption")
    return len(mutations)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=ROOT / "corpus/validation.json")
    parser.add_argument("--partial", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    report = json.loads(oracle.bounded_read(args.report, 64 * codec.MAX_BYTES))
    result = validate_report(report, partial=args.partial)
    if args.self_test:
        result["constructed_reader_corruptions"] = self_test(report, partial=args.partial)
    print(json.dumps(result, indent=2))
