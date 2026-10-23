#!/usr/bin/env python3
"""Bounded independent recorded-reader checks for canonical CAT lifecycle data.

This reader runs no compiler, native policy, VM, SDK or network operation. Its
checks establish retained consistency, not authenticated execution or state.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import re
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[1] / "scripts"))
from registry_codegen_evidence import decode, encode, identity, proper, require
from vectors import check_provenance

CAT = "37bef360ee858133b69d595a906dc45d01af50379dad515eb9518abb7c1d2a7a"
TAIL = "493afb89eed93ab86741b2aa61b8f5de495d33ff9b781dfc8919e602b2afa150"
STANDARD = "e9aaa49f45bad5c889b86ee3341550c155cfdd10c3a6757de618d20612fffd52"
EMPTY_SIGNATURE = "c0" + "00" * 95
MAIN = [
    ("fund_genesis_to_cat_eve", "accepted", None),
    ("eve_empty_owner_signature", "consensus_rejected", "BadAggregateSignature"),
    ("eve_nonempty_wrong_key_signature", "consensus_rejected", "BadAggregateSignature"),
    ("eve_nonempty_wrong_message_signature", "consensus_rejected", "BadAggregateSignature"),
    ("eve_wrong_ring_successor", "consensus_rejected", "AssertCoinAnnouncementFailed"),
    ("eve_nonzero_extra_delta", "runtime_umbrella", "raise"),
    ("eve_wrong_tail_reveal_hash", "runtime_umbrella", "raise"),
    ("eve_malformed_solution", "runtime_umbrella", "path_into_atom"),
    ("zero_delta_tail_issuance", "accepted", None),
    ("transfer_wrong_parent_lineage", "runtime_umbrella", "raise"),
    ("transfer_cat_ring_imbalance", "consensus_rejected", "AssertCoinAnnouncementFailed"),
    ("owner_transfers_issued_cat", "accepted", None),
    ("recipient_rejects_previous_owner_signature", "consensus_rejected", "BadAggregateSignature"),
    ("recipient_spends_with_valid_lineage", "accepted", None),
    ("consumed_genesis_input_replay", "consensus_rejected", "DoubleSpend"),
]
CONTROL_NAMES = ["original_wrong_genesis", "drop_genesis_wrong_genesis", "original_nonzero_delta", "drop_delta_nonzero_delta"]


def check_lifecycle_provenance():
    check_provenance()
    record = json.loads((HERE / "lifecycle-provenance.json").read_text())
    require(record["revision"] == "d122456feeef55708d354a065623037bfa010714"
            and record["root_tree_sha1"] == "89e5f55c5afdde2c436b9a2c08b8e30ecf11eb62"
            and record["license"] == "Apache-2.0", "pinned CAT2 source provenance")
    pins = {"cat_v2.clsp":"aeebd641205c09b97243aebf3306218bd739a1fa",
            "cat_v2.clsp.hex":"5cbde1ad697c5084f9b3ba8832afb4584dbc101a",
            "condition_codes.clib":"a6a5d22b5883487b87b51f5cf3f66d63440d68cf",
            "curry-and-treehash.clib":"0b7632aad3682cfbf9038d37033a006ce3d2ed8f",
            "utility_macros.clib":"271a4ba5136a260f1eca0df3ab8b247dc8dfb7c2"}
    require(len(record["files"]) == len(pins) and {row["retained_path"] for row in record["files"]} == {"upstream/" + name for name in pins}, "closed selected CAT2 source/include inventory")
    for row in record["files"]:
        name = Path(row["retained_path"]).name
        payload = (HERE / "upstream" / name).read_bytes()
        require(len(payload) == row["bytes"] <= 65536 and hashlib.sha256(payload).hexdigest() == row["sha256"]
                and hashlib.sha1(f"blob {len(payload)}\0".encode() + payload).hexdigest() == row["git_blob_sha1"] == pins[name], "actual pinned CAT2/include bytes")


def raw(value, maximum=65536):
    require(type(value) is str and len(value) <= maximum * 2 and re.fullmatch(r"(?:[0-9a-f]{2})+", value), "bounded exact hexadecimal data")
    return bytes.fromhex(value)


def tree_hash(blob):
    return identity(blob)["module_tree_hash"]


def fields(tree):
    result = []
    while type(tree) is tuple:
        require(len(result) < 8192, "bounded proper-list fields")
        result.append(tree[0])
        tree = tree[1]
    require(tree == b"", "exact proper list")
    return result


def integer(value):
    require(type(value) is int and 0 <= value < 1 << 64, "bounded fixture unsigned amount")
    return b"" if value == 0 else value.to_bytes((value.bit_length() + 8) // 8, "big", signed=True)


def curry(module, arguments):
    environment = b"\x01"
    for argument in reversed(arguments):
        environment = proper([b"\x04", (b"\x01", argument), environment])
    return encode(proper([b"\x02", (b"\x01", decode(module)), environment]))


def check_coin(coin):
    require(set(coin) == {"coin_id", "parent_coin_id", "puzzle_hash", "amount"}, "coin fields")
    parent, puzzle = raw(coin["parent_coin_id"], 32), raw(coin["puzzle_hash"], 32)
    require(len(parent) == len(puzzle) == 32, "coin field widths")
    expected = hashlib.sha256(parent + puzzle + integer(coin["amount"])).hexdigest()
    require(coin["coin_id"] == expected, "independent coin ID")


def check_child(child, parent, puzzle, amount):
    check_coin(child)
    require(child["parent_coin_id"] == parent["coin_id"] and child["puzzle_hash"] == puzzle
            and child["amount"] == amount, "exact consumed parent/created child identity")


def check_attempt(attempt, name, status, code, *, max_cost=50_000_000):
    require(attempt["name"] == name, "exact named lifecycle phase")
    outcome = attempt["outcome"]
    require(outcome["status"] == status, "intended actual SDK outcome")
    if status == "consensus_rejected":
        require(outcome.get("full_simulator_equality") is True, "exact Boolean full in-memory equality marker")
        require(outcome == {"status": status, "code": code, "full_simulator_equality": True}, "typed consensus/state marker")
    elif status == "runtime_umbrella":
        require(outcome.get("full_simulator_equality") is True
                and outcome.get("semantic_rejection_credit") is False, "exact Boolean runtime scope markers")
        require(outcome == {"status": status, "code": "GeneratorRuntimeError", "semantic_rejection_credit": False,
                           "primary_program_code": code, "full_simulator_equality": True}, "runtime umbrella has no semantic consensus credit")
    else:
        require(outcome == {"status": "accepted"}, "accepted result fields")
    transaction = attempt["transaction"]
    require(len(raw(transaction["signature_hex"], 96)) == 96, "signature transport width")
    spends, runs = transaction["spends"], attempt["primary_vm"]
    require(1 <= len(spends) <= 2 and len(spends) == len(runs), "one separately recorded primary VM run per actual spend")
    failed = []
    for spend, run in zip(spends, runs):
        check_coin(spend["coin"])
        require(tree_hash(raw(spend["reveal_hex"])) == spend["coin"]["puzzle_hash"], "fund own exact reveal")
        decode(raw(spend["solution_hex"]))
        if run["outcome"]["status"] == "accepted":
            require(type(run["cost"]) is int and 0 < run["cost"] <= max_cost and run["failure"] is None, "conclusive primary success")
            fields(decode(raw(run["outcome"]["output_hex"])))
        else:
            require(run["outcome"] == {"status": "rejected"} and run["cost"] is None
                    and run["failure"]["kind"] == "program", "resource/infrastructure cannot replace intended program failure")
            failed.append(run["failure"]["code"])
    require(failed == ([code] if status == "runtime_umbrella" else []), "exact separate primary failure identity")
    if status == "accepted" or status == "consensus_rejected" and code != "BadAggregateSignature":
        require(transaction["signature_hex"] != EMPTY_SIGNATURE, "actual signed submitted bundle")


def created(attempt):
    output = attempt["primary_vm"][0]["outcome"]
    require(output["status"] == "accepted", "created outputs require executed conditions")
    result = []
    for condition in fields(decode(raw(output["output_hex"]))):
        values = fields(condition)
        if values[0] == b"\x33":
            require(len(values) == 3 and type(values[1]) is bytes and len(values[1]) == 32, "exact CREATE_COIN wire fields")
            result.append((values[1].hex(), values[2]))
    return result


def check_cat_solution(attempt, input_coin, inner_hash, parent, *, lineage, delta=0, wrong_ring=False, wrong_lineage=False, parent_inner_hash=None):
    spend = attempt["transaction"]["spends"][0]
    require(spend["coin"] == input_coin, "phase spends the actual preceding created coin")
    values = fields(decode(raw(spend["solution_hex"])))
    require(len(values) == 7, "exact pinned CatSolution schema")
    expected_lineage = proper([raw(parent["parent_coin_id"]), raw(parent_inner_hash or inner_hash), integer(parent["amount"])]) if lineage else b""
    if wrong_lineage:
        expected_lineage = proper([b"\x92" * 32, raw(inner_hash), integer(parent["amount"])])
    require(values[1] == expected_lineage and values[2] == raw(input_coin["coin_id"]), "exact real lineage and previous self-ring ID")
    require(values[3] == proper([raw(input_coin["parent_coin_id"]), raw(input_coin["puzzle_hash"]), integer(input_coin["amount"])]), "exact this_coin_info")
    require(values[4] == proper([b"\x91" * 32 if wrong_ring else raw(input_coin["parent_coin_id"]), raw(inner_hash), integer(input_coin["amount"])]), "exact self-ring next proof")
    require(values[5] == b"" and values[6] == integer(delta), "zero prev_subtotal and explicit extra_delta")


def standard_solution(destination, amount=1000, run_tail=None):
    conditions = []
    if destination is not None:
        conditions.append(proper([b"\x33", raw(destination), integer(amount)]))
    if run_tail is not None:
        conditions.append(proper([b"\x33", b"", b"\x8f", decode(run_tail), b""]))
    return proper([b"", (b"\x01", proper(conditions)), b""])


def check_inner_solution(attempt, destination, amount=1000, run_tail=None):
    outer = fields(decode(raw(attempt["transaction"]["spends"][0]["solution_hex"])))
    require(outer[0] == standard_solution(destination, amount, run_tail), "exact delegated standard/CREATE_COIN/TAIL wire contract")


def _validate_main_lifecycle(report, *, admitted_tail=None, max_cost=50_000_000):
    check_lifecycle_provenance()
    require(report["protocol"] == "chialisp-oxide-canonical-cat-genesis-lifecycle/v1" and report["status"] == "executed", "canonical experiment protocol")
    for key, value in {"sdk": "chia-sdk-test 0.36.0", "consensus": "chia-consensus 0.36.1", "sdk_vm": "clvmr 0.16.2",
                       "network": "TESTNET11", "primary_vm": "clvmr 0.21.0", "primary_protocol": "clvmr-0.21.0/v1", "primary_max_cost": max_cost}.items():
        require(report[key] == value, "explicit pinned protocol/version: " + key)
    cat_module, tail_module = raw(report["cat_module_hex"]), raw(report["tail_module_hex"])
    require(len(cat_module) == 1672 and tree_hash(cat_module) == report["cat_module_tree_hash"] == CAT
            and cat_module == raw((HERE / "upstream/cat_v2.clsp.hex").read_text().strip()), "canonical CAT2 identity")
    if admitted_tail is None:
        require(len(tail_module) == 45 and tree_hash(tail_module) == TAIL
                and tail_module == raw((HERE / "upstream/genesis_by_coin_id.clsp.hex").read_text().strip()), "exact retained canonical TAIL")
    else:
        require(type(admitted_tail) is bytes and tail_module == admitted_tail,
                "main lifecycle uses the independently admitted selected module")
    standard_fixture = HERE.parents[1] / "crates/chialisp-oxide-package/tests/fixtures/mainnet-standard/expected.json"
    standard = raw(json.loads(standard_fixture.read_text())["module_hex"])
    require(tree_hash(standard) == STANDARD, "pinned canonical standard module")
    owner, recipient = raw(report["owner_public_key"], 48), raw(report["recipient_public_key"], 48)
    require(len(owner) == len(recipient) == 48 and owner != recipient, "distinct chosen disposable authorization roles")
    owner_inner, recipient_inner = curry(standard, [owner]), curry(standard, [recipient])
    owner_hash, recipient_hash = tree_hash(owner_inner), tree_hash(recipient_inner)
    lane = report["lifecycle"]
    check_coin(lane["genesis"])
    require(lane["genesis"]["puzzle_hash"] == owner_hash and lane["genesis"]["amount"] == 1000, "funded genesis input")
    tail_reveal = curry(tail_module, [raw(lane["genesis"]["coin_id"])])
    asset = tree_hash(tail_reveal)
    owner_cat = curry(cat_module, [raw(CAT), raw(asset), decode(owner_inner)])
    recipient_cat = curry(cat_module, [raw(CAT), raw(asset), decode(recipient_inner)])
    for key, blob in {"tail_reveal_hex": tail_reveal, "owner_inner_reveal_hex": owner_inner, "recipient_inner_reveal_hex": recipient_inner,
                      "owner_cat_reveal_hex": owner_cat, "recipient_cat_reveal_hex": recipient_cat}.items():
        require(lane[key] == blob.hex(), "independent exact curry bytes: " + key)
    require(lane["asset_id"] == asset, "curried TAIL is asset ID")
    for child, parent, puzzle in [("eve", "genesis", owner_cat), ("issued", "eve", owner_cat), ("received", "issued", recipient_cat), ("continuation", "received", recipient_cat)]:
        check_child(lane[child], lane[parent], tree_hash(puzzle), 1000)
    attempts = lane["attempts"]
    require(len(attempts) == len(MAIN), "one canonical lifecycle group with exactly fifteen SDK submissions")
    for attempt, spec in zip(attempts, MAIN):
        check_attempt(attempt, *spec, max_cost=report["primary_max_cost"])
    by_name = {row["name"]: row for row in attempts}
    funding = by_name["fund_genesis_to_cat_eve"]["transaction"]["spends"]
    require(len(funding) == 1 and funding[0]["coin"] == lane["genesis"]
            and decode(raw(funding[0]["solution_hex"])) == standard_solution(tree_hash(owner_cat)), "fund actual declared genesis coin into own Eve asset")
    for name in [spec[0] for spec in MAIN[1:9] if spec[0] != "eve_malformed_solution"]:
        check_cat_solution(by_name[name], lane["eve"], owner_hash, lane["genesis"], lineage=False,
                           delta=int(name == "eve_nonzero_extra_delta"), wrong_ring=name == "eve_wrong_ring_successor")
        check_inner_solution(by_name[name], owner_hash, run_tail=tail_module if name == "eve_wrong_tail_reveal_hash" else tail_reveal)
    malformed = by_name["eve_malformed_solution"]["transaction"]["spends"]
    require(len(malformed) == 1 and malformed[0]["coin"] == lane["eve"] and malformed[0]["solution_hex"] == "80", "actual malformed Eve solution witness")
    for name in ("transfer_wrong_parent_lineage", "transfer_cat_ring_imbalance", "owner_transfers_issued_cat"):
        check_cat_solution(by_name[name], lane["issued"], owner_hash, lane["eve"], lineage=True, wrong_lineage=name == "transfer_wrong_parent_lineage")
        check_inner_solution(by_name[name], recipient_hash, amount=999 if name == "transfer_cat_ring_imbalance" else 1000)
    for name in ("recipient_rejects_previous_owner_signature", "recipient_spends_with_valid_lineage"):
        row = by_name[name]
        check_cat_solution(row, lane["received"], recipient_hash, lane["issued"], lineage=True, parent_inner_hash=owner_hash)
        check_inner_solution(row, recipient_hash)
    for label, target in [("fund_genesis_to_cat_eve", "eve"), ("zero_delta_tail_issuance", "issued"), ("owner_transfers_issued_cat", "received"), ("recipient_spends_with_valid_lineage", "continuation")]:
        require(created(by_name[label]) == [(lane[target]["puzzle_hash"], integer(1000))], "exact output condition identity")
    require(by_name["consumed_genesis_input_replay"]["transaction"] == by_name["fund_genesis_to_cat_eve"]["transaction"], "replay exact funded genesis bundle")
    for name in ("eve_empty_owner_signature", "eve_nonempty_wrong_key_signature", "eve_nonempty_wrong_message_signature"):
        require(by_name[name]["transaction"]["spends"] == by_name["zero_delta_tail_issuance"]["transaction"]["spends"], "signature negative isolates actual unchanged spend")
        require((by_name[name]["transaction"]["signature_hex"] == EMPTY_SIGNATURE) == (name == "eve_empty_owner_signature"), "actual empty/nonempty signature witness")
    require(by_name["recipient_rejects_previous_owner_signature"]["transaction"]["spends"] == by_name["recipient_spends_with_valid_lineage"]["transaction"]["spends"], "recipient signature negative isolates same spend")
    recipient_wrong = by_name["recipient_rejects_previous_owner_signature"]["transaction"]["signature_hex"]
    require(recipient_wrong != EMPTY_SIGNATURE
            and recipient_wrong != by_name["recipient_spends_with_valid_lineage"]["transaction"]["signature_hex"],
            "actual nonempty previous-owner signature differs from accepted recipient signature")
    return {"tail_module": tail_module, "cat_module": cat_module,
            "owner_inner": owner_inner, "owner_hash": owner_hash, "attempts": attempts}


def attempt_counts(attempts):
    return {"sdk_transaction_attempts": len(attempts),
            "accepted": sum(a["outcome"]["status"] == "accepted" for a in attempts),
            "typed_rejected": sum(a["outcome"]["status"] == "consensus_rejected" for a in attempts),
            "runtime_umbrellas": sum(a["outcome"]["status"] == "runtime_umbrella" for a in attempts),
            "component_primary_vm_runs": sum(len(a["primary_vm"]) for a in attempts)}


def validate_main_lifecycle(report, *, admitted_tail=None, max_cost=50_000_000):
    """Check only the fifteen actual main submissions; no source-control credit.

    An explicit admitted_tail is supplied by an independent artifact reader.
    This does not infer a typed ABI from that module or execute any program.
    """
    context = _validate_main_lifecycle(report, admitted_tail=admitted_tail, max_cost=max_cost)
    counts = attempt_counts(context["attempts"])
    require(counts == {"sdk_transaction_attempts":15,"accepted":4,"typed_rejected":7,
                      "runtime_umbrellas":4,"component_primary_vm_runs":15}, "exact main lifecycle scope")
    return counts


def validate_source_controls(report, context, *, rust_policy=None):
    """Original/mutant controls retain their own reveals, assets and scope."""
    tail_module, cat_module = context["tail_module"], context["cat_module"]
    owner_inner, owner_hash = context["owner_inner"], context["owner_hash"]
    controls = report["source_controls"]
    require([row["name"] for row in controls] == CONTROL_NAMES, "four separate original/mutant lifecycle-control groups")
    original = (HERE / "upstream/genesis_by_coin_id.clsp").read_text()
    recipe = json.loads((HERE / "lifecycle-controls.json").read_text())
    source_map = {CONTROL_NAMES[0]: original, CONTROL_NAMES[2]: original}
    for edit in recipe["recipes"]:
        require(original.count(edit["before"]) == 1, "unique exact source control")
        source_map[edit["mutant"]] = original.replace(edit["before"], edit["after"])
    for index, control in enumerate(controls):
        name = control["name"]
        if rust_policy is None:
            require(control["source"] == source_map[name] and control["source_sha256"] == hashlib.sha256(source_map[name].encode()).hexdigest(), "actual canonical source edit identity")
        else:
            selected_control = "original" if index % 2 == 0 else "drop_genesis_equality" if index == 1 else "drop_delta_guard"
            require(control["artifact"] == rust_policy[selected_control]
                    and control["module_hex"] == rust_policy[selected_control]["clvm_hex"], "exact actual Rust source-control artifact")
        module = raw(control["module_hex"])
        require(tree_hash(module) == control["module_tree_hash"], "source-control compiled module identity")
        if index % 2 == 0: require(module == tail_module, "baseline uses unchanged original")
        else: require(tree_hash(module) != TAIL, "mutant has own actual module identity")
        genesis = control["actual_genesis"]
        check_coin(genesis)
        require(genesis["puzzle_hash"] == owner_hash and genesis["amount"] == 1000, "control actual funded genesis")
        declared = b"\xa7" * 32 if index < 2 else raw(genesis["coin_id"])
        require(control["declared_genesis_id"] == declared.hex(), "isolated wrong/correct genesis declaration")
        control_tail = curry(module, [declared]); asset = tree_hash(control_tail)
        reveal = curry(cat_module, [raw(CAT), raw(asset), decode(owner_inner)])
        require(control["tail_reveal_hex"] == control_tail.hex() and control["asset_id"] == asset
                and control["cat_reveal_hex"] == reveal.hex(), "fund each source control's own exact asset/reveal")
        check_child(control["eve"], genesis, tree_hash(reveal), 1000)
        delta, output = int(index >= 2), 1000 + int(index >= 2)
        require(control["extra_delta"] == delta and control["output_amount"] == output, "source-derived subtotal witness")
        require(len(control["attempts"]) == 2, "one ordinary funding spend and one CAT submission per source-control group")
        funding, attempt = control["attempts"]
        check_attempt(funding, "fund_own_tail_asset_eve", "accepted", None, max_cost=report["primary_max_cost"])
        check_attempt(attempt, name, "accepted" if index % 2 else "runtime_umbrella", None if index % 2 else "raise", max_cost=report["primary_max_cost"])
        require(created(funding) == [(tree_hash(reveal), integer(1000))], "control funds its actual asset")
        funding_spends = funding["transaction"]["spends"]
        require(len(funding_spends) == 1 and funding_spends[0]["coin"] == genesis
                and decode(raw(funding_spends[0]["solution_hex"])) == standard_solution(tree_hash(reveal)), "source-control funding consumes actual declared genesis")
        check_cat_solution(attempt, control["eve"], owner_hash, genesis, lineage=False, delta=delta)
        check_inner_solution(attempt, owner_hash, output, control_tail)
        if delta:
            subsidy = control["subsidy"]; check_coin(subsidy)
            require(subsidy["amount"] == 1 and subsidy["puzzle_hash"] == owner_hash and len(attempt["transaction"]["spends"]) == 2
                    and attempt["transaction"]["spends"][1]["coin"] == subsidy, "actual one-mojo standard subsidy input")
            require(attempt["primary_vm"][1]["outcome"]["status"] == "accepted", "subsidy program returns executable conditions")
            require(decode(raw(attempt["transaction"]["spends"][1]["solution_hex"])) == standard_solution(None)
                    and created({"primary_vm":[attempt["primary_vm"][1]]}) == [], "actual standard subsidy has no outputs")
        else: require(control["subsidy"] is None and len(attempt["transaction"]["spends"]) == 1, "no hidden subsidy on wrong-genesis witness")
        if index % 2:
            check_child(control["child"], control["eve"], tree_hash(reveal), output)
            require(created(attempt) == [(tree_hash(reveal), integer(output))], "mutant's actual created CAT")
        else: require(control["child"] is None, "rejected original has no created-child claim")
    return [attempt for control in controls for attempt in control["attempts"]]


def validate(report, *, rust_policy=None):
    # Preserve the exact standalone canonical/two-Rust-mode report contract.
    admitted_tail = None
    if rust_policy is not None:
        require(set(rust_policy) == {"original", "drop_delta_guard", "drop_genesis_equality"},
                "complete independently admitted Rust source-control inventory")
        admitted_tail = raw(rust_policy["original"]["clvm_hex"])
        require(tree_hash(admitted_tail) != TAIL, "actual Rust module differs from canonical")
    context = _validate_main_lifecycle(report, admitted_tail=admitted_tail,
                                       max_cost=50_000_000 if rust_policy is None else 250_000_000)
    controls = validate_source_controls(report, context, rust_policy=rust_policy)
    counts = attempt_counts(context["attempts"] + controls)
    require(counts == {"sdk_transaction_attempts":23,"accepted":10,"typed_rejected":7,"runtime_umbrellas":6,"component_primary_vm_runs":25}, "separate fixed scenario scope")
    require(all(report[key] == value for key, value in counts.items()), "declared counts must match observations")
    return {"status":"recorded_consistency_checked", **counts, "canonical_lifecycle_groups":1,"source_control_groups":4,
            "accepted_standard_funding_transactions":5,"accepted_cat_transitions":5,
            "scope":"Independent retained identities/phase/framing/outcome consistency only; state-equality flags are not a state serialization, authenticated execution, new VM/SDK execution or compiler preservation."}


def load(filename):
    with filename.open("rb") as stream:
        import os
        import stat
        info = os.fstat(stream.fileno())
        require(stat.S_ISREG(info.st_mode) and 0 < info.st_size <= 2 * 1024 * 1024, "bounded regular lifecycle report")
        payload = stream.read(2 * 1024 * 1024 + 1)
    require(len(payload) <= 2 * 1024 * 1024, "report grew beyond byte bound")
    return json.loads(payload)


def self_test(report):
    validate(report)
    controls = 0
    def cat_field(packet, attempt, field, value):
        spend = packet["lifecycle"]["attempts"][attempt]["transaction"]["spends"][0]
        values = fields(decode(raw(spend["solution_hex"]))); values[field] = value
        spend["solution_hex"] = encode(proper(values)).hex()
    def coherent_funding_coin(packet):
        coin = packet["lifecycle"]["attempts"][0]["transaction"]["spends"][0]["coin"]
        coin["parent_coin_id"] = "55" * 32
        coin["coin_id"] = hashlib.sha256(raw(coin["parent_coin_id"]) + raw(coin["puzzle_hash"]) + integer(coin["amount"])).hexdigest()
    def changed_this_coin(packet):
        actual = packet["lifecycle"]["received"]
        cat_field(packet, 13, 3, proper([raw(actual["parent_coin_id"]),raw(actual["puzzle_hash"]),integer(1001)]))
    def subsidy_outputs(packet):
        subsidy = packet["source_controls"][3]["attempts"][1]["transaction"]["spends"][1]
        subsidy["solution_hex"] = encode(standard_solution(subsidy["coin"]["puzzle_hash"],1)).hex()
    for mutate in [
        lambda p: p.update(sdk_vm="clvmr 0.21.0"),
        lambda p: p.update(sdk_transaction_attempts=25),
        lambda p: p["lifecycle"]["eve"].update(amount=1001),
        lambda p: p["lifecycle"].update(asset_id=TAIL),
        lambda p: p["lifecycle"]["attempts"][1]["outcome"].update(code="DoubleSpend"),
        lambda p: p["lifecycle"]["attempts"][1]["outcome"].update(full_simulator_equality=False),
        lambda p: p["lifecycle"]["attempts"][1]["outcome"].update(full_simulator_equality=1),
        lambda p: p["lifecycle"]["attempts"][5]["outcome"].update(full_simulator_equality=1),
        lambda p: p["lifecycle"]["attempts"][5]["outcome"].update(semantic_rejection_credit=0),
        lambda p: p["lifecycle"]["attempts"][5]["outcome"].update(semantic_rejection_credit=True),
        lambda p: p["lifecycle"]["attempts"][5]["primary_vm"][0].update(failure={"kind":"resource","code":"cost_exceeded"}),
        lambda p: p["lifecycle"]["attempts"][2]["transaction"].update(signature_hex=EMPTY_SIGNATURE),
        lambda p: p["lifecycle"]["attempts"][12]["transaction"].update(signature_hex=EMPTY_SIGNATURE),
        lambda p: p["lifecycle"]["attempts"][8].update(name="constructed_not_submitted"),
        lambda p: p["source_controls"][1].update(source=p["source_controls"][0]["source"]),
        lambda p: p["source_controls"][1].update(asset_id=p["source_controls"][0]["asset_id"]),
        lambda p: p["source_controls"][3].update(extra_delta=0),
        lambda p: p["source_controls"][3]["subsidy"].update(amount=0),
        lambda p: p["source_controls"][3]["attempts"][1]["transaction"]["spends"].pop(),
        coherent_funding_coin,
        changed_this_coin,
        lambda p: cat_field(p,13,5,b"\x01"),
        lambda p: cat_field(p,11,0,standard_solution("44" * 32)),
        subsidy_outputs,
        lambda p: p["lifecycle"]["attempts"][5]["primary_vm"][0].update(failure={"kind":"program","code":"invalid_op_arg"}),
    ]:
        bad = copy.deepcopy(report); mutate(bad)
        try: validate(bad)
        except (ValueError, KeyError, TypeError, IndexError): controls += 1
        else: raise ValueError("escaped constructed lifecycle reader corruption")
    print(f"PASS {controls} constructed recorded-reader corruptions; no execution credit")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--self-test", action="store_true")
    options = parser.parse_args()
    report = load(options.report)
    if options.self_test: self_test(report)
    print(json.dumps(validate(report), indent=2))
