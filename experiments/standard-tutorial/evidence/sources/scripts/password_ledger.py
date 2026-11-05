"""Check finite fixed-commitment ledger observations, not coin security proofs."""
import hashlib
import re

from password_vectors import (CODEC, PUBLIC_HASH, conditions, curry_tree,
                              deserialize, native_environment, parse, proper,
                              relation, require, serialize)
from vm_failures import check_execution

LEDGER_MODES = [("rust_mir", True), ("chialisp", True), ("rue_source", True)]
LABELS = [
    "fixed commitment accepts the public fixture password",
    "disclosed password permits unsigned retargeting",
    "wrong password yields a runtime umbrella with complete rollback",
    "pair destination reaches exact consensus rejection",
    "pair amount reaches exact consensus rejection",
    "arbitrary output atom is not valid ledger conditions",
    "different committed hash cannot replace this coin reveal",
]
CODES = [None, None, "GeneratorRuntimeError", "InvalidPuzzleHash", "InvalidCoinAmount", "InvalidCondition", "WrongPuzzleHash"]


def hash_hex(value):
    require(isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value), "Malformed ledger hash/id")
    return bytes.fromhex(value)


def states(before, after, unchanged, accepted, destination, coin_id):
    require(type(unchanged) is bool and unchanged is (not accepted), "Wrong/nonboolean complete-state rollback flag")
    for state in (before, after):
        require(isinstance(state, dict) and set(state) == {"created_height", "spent_height", "children"}, "Malformed recorded coin state")
        require(type(state["created_height"]) is int and state["created_height"] >= 0, "Missing input creation height")
        require(state["spent_height"] is None or type(state["spent_height"]) is int, "Malformed spent height")
        require(isinstance(state["children"], list), "Malformed child state list")
    require(before["spent_height"] is None and before["children"] == [], "Input was not initially unspent/childless")
    if not accepted:
        require(after == before, "Negative ledger attempt changed recorded state")
        return
    require(type(after["spent_height"]) is int and after["spent_height"] >= before["created_height"]
            and after["created_height"] == before["created_height"] and len(after["children"]) == 1,
            "Successful spend lacks its exact child/state transition")
    child = after["children"][0]
    require(set(child) == {"coin_id", "parent_coin_id", "puzzle_hash", "amount"}
            and type(child["amount"]) is int and child["amount"] == 1000
            and child["parent_coin_id"] == coin_id and child["puzzle_hash"] == destination.hex(),
            "Wrong child parent, destination or amount")
    # Pinned chia-protocol0.36.1 Coin::coin_id hashes the minimal signed-positive
    # CLVM amount bytes. For1000 this is exactly03e8, not an eight-byte u64.
    expected_id = hashlib.sha256(bytes.fromhex(coin_id) + destination + b"\x03\xe8").hexdigest()
    require(child["coin_id"] == expected_id, "Child coin id does not bind parent/puzzle/amount")


def validate(ledger, modules, instances):
    require(isinstance(ledger, list) and [(row.get("backend"), row.get("optimize_lir")) for row in ledger] == LEDGER_MODES
            and all(type(row.get("optimize_lir")) is bool for row in ledger), "Missing/unknown/reordered password ledger modes")
    ordinary = conditions()
    other_hash = hashlib.sha256(b"xyz").digest()
    solutions = [proper(b"abc", ordinary), proper(b"abc", conditions(b"C" * 32)),
                 proper(b"xyz", ordinary), proper(b"abc", conditions((b"\x01", b"\x02"))),
                 proper(b"abc", conditions(amount=(b"\x01", b"\x02"))),
                 proper(b"abc", b"\0"), proper(b"xyz", ordinary)]
    for row in ledger:
        require(set(row) == {"backend", "optimize_lir", "simulator", "consensus", "vm", "network", "module_tree_hash", "instance_tree_hash", "transaction_attempts", "attempts"},
                "Malformed password ledger comparison")
        mode = row["backend"], row["optimize_lir"]
        module, instance = modules[mode], instances[mode]
        require(row["simulator"] == "chia-sdk-test 0.36.0" and row["consensus"] == "chia-consensus 0.36.1"
                and row["vm"] == "clvmr 0.16.2" and row["network"] == "TESTNET11", "Changed ledger VM/protocol/network")
        require(row["module_tree_hash"] == module["tree_hash"] and row["instance_tree_hash"] == instance["tree_hash"], "Ledger is not bound to its own module/instance")
        attempts = row["attempts"]
        require(type(row["transaction_attempts"]) is int and row["transaction_attempts"] == 7
                and isinstance(attempts, list) and [a["label"] for a in attempts] == LABELS, "Missing/unknown ledger attempt coverage")
        for index, attempt in enumerate(attempts):
            require(set(attempt) == {"label", "argument_hex", "coin_id", "coin_puzzle_hash", "reveal_hex", "reveal_tree_hash", "solution", "solution_hex", "vm_021", "consensus", "before", "after", "full_state_unchanged"}, "Malformed ledger attempt")
            argument = other_hash if index == 6 else PUBLIC_HASH
            reveal = serialize(curry_tree(deserialize(bytes.fromhex(module["clvm_hex"])), argument))
            require(attempt["argument_hex"] == serialize(argument).hex() and attempt["reveal_hex"] == reveal.hex()
                    and attempt["reveal_tree_hash"] == CODEC["tree_hash"](deserialize(reveal)).hex(), "Ledger reveal is not exact C(sameM,selectedH)")
            require(attempt["coin_puzzle_hash"] == instance["tree_hash"], "Starting coin changed fixed commitment")
            hash_hex(attempt["coin_id"])
            solution = parse(attempt["solution"])
            require(solution == solutions[index] and attempt["solution_hex"] == serialize(solution).hex(), "Changed ledger solution or serialized input")
            expected = relation(native_environment(argument, solution))
            require(check_execution(attempt["vm_021"]) == expected, "Ledger primary VM differs from independent whole-environment relation")
            if expected["status"] == "accepted":
                require(attempt["vm_021"]["cost"] <= 1_000_000, "Ledger primary-VM execution exceeded its budget")
            if index == 2:
                require(attempt["vm_021"].get("failure") == {"kind": "program", "code": "raise"}, "Wrong password must have exact typed primary-VM Raise")
            else:
                require(expected["status"] == "accepted", "Malformed conditions/reveal must reach consensus after successful VM evaluation")
            observation = attempt["consensus"]
            require(isinstance(observation, dict) and set(observation) == {"status", "code", "error", "semantic_rejection_credit"}
                    and type(observation["semantic_rejection_credit"]) is bool, "Malformed/untypechecked consensus observation")
            accepted = index < 2
            status = "accepted" if accepted else "runtime_umbrella" if index == 2 else "consensus_rejected"
            require(observation == {"status": status, "code": CODES[index],
                "error": None if accepted else f"Validation({CODES[index]})",
                "semantic_rejection_credit": index >= 3}, "Wrong consensus reason or misleading semantic credit")
            destination = b"B" * 32 if index == 0 else b"C" * 32
            states(attempt["before"], attempt["after"], attempt["full_state_unchanged"], accepted, destination, attempt["coin_id"])
        require(attempts[0]["coin_id"] == attempts[1]["coin_id"], "Retargeting used another starting coin")
        require(attempts[6]["reveal_tree_hash"] != row["instance_tree_hash"], "Wrong-reveal control did not change the committed hash")
    return {"language_groups": 3, "transaction_attempts": 21, "semantic_rejections": 12,
            "runtime_umbrellas_without_semantic_credit": 3, "accepted_spends": 6}
