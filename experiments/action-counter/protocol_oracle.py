"""Independent finite host protocol oracle; reading adds no execution credit."""
from pathlib import Path
import hashlib
import importlib.util
import json
import stat
import sys
sys.dont_write_bytecode = True

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("counter_compiled_codec", ROOT / "experiments/cat-genesis-tail/compiled_codec.py")
codec = importlib.util.module_from_spec(spec)
spec.loader.exec_module(codec)
NIL = b""
MAX = 2**64 - 1
INTENT = b"chialisp-oxide/counter/reset:v1!"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def same_json(actual, expected):
    return json.dumps(actual,sort_keys=True,separators=(",",":"),allow_nan=False) == json.dumps(expected,sort_keys=True,separators=(",",":"),allow_nan=False)


def sha(raw):
    return hashlib.sha256(raw).digest()


def bounded_read(path, maximum):
    import os
    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
    with os.fdopen(fd, "rb") as stream:
        info = os.fstat(stream.fileno())
        require(stat.S_ISREG(info.st_mode) and info.st_size <= maximum, "bounded regular input before read")
        raw = stream.read(maximum + 1)
    require(len(raw) <= maximum, "bounded input growth")
    return raw


def uint(n):
    require(type(n) is int and 0 <= n <= MAX, "finite unsigned u64")
    if n == 0:
        return NIL
    raw = n.to_bytes((n.bit_length() + 7) // 8, "big")
    return b"\x00" + raw if raw[0] & 128 else raw


def proper(*values):
    tail = NIL
    for value in reversed(values):
        tail = (value, tail)
    return tail


def treehash(tree):
    # Codec preflight bounds the complete shape before this separate hash walk.
    codec.encode(tree)
    pending = [(tree, False)]
    results = []
    while pending:
        value, visited = pending.pop()
        if type(value) is bytes:
            results.append(sha(b"\x01" + value))
        elif visited:
            right, left = results.pop(), results.pop()
            results.append(sha(b"\x02" + left + right))
        else:
            pending.extend(((value, True), (value[1], False), (value[0], False)))
    return results[0]


def curry(module, *arguments):
    env = uint(1)
    for arg in reversed(arguments):
        env = proper(uint(4), (uint(1), arg), env)
    result = proper(uint(2), (uint(1), module), env)
    codec.encode(result)
    return result


def load_hex(path, size=None, digest=None, th=None):
    text = "".join(bounded_read(path, 3 * codec.MAX_BYTES).decode("ascii").split())
    require(len(text) <= 2 * codec.MAX_BYTES and text == text.lower(), "bounded canonical hex")
    raw = bytes.fromhex(text)
    require(raw.hex() == text, "plain canonical hex")
    identity = codec.identity(raw)
    require(size is None or len(raw) == size, "pinned module length")
    require(digest is None or sha(raw).hex() == digest, "pinned module blob")
    require(th is None or identity["module_tree_hash"] == th, "pinned module tree")
    return codec.decode(raw)


def modules():
    slot = ROOT / "corpus/upstream/slot-machine/6edc94856306d0f77e065c6c99aacf1055e51e2f/rue-puzzles/singleton"
    upstream = ROOT / "corpus/upstream/chia-puzzles/cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8/puzzles"
    action = load_hex(slot / "action.rue.hex", 630,
        "849e048c276a39f05b68f78ba2d3a3b5924f5e8b8944a7c8ca923119eb442a7a",
        "afa03f2903f9e4a293523237799074b13ab2361f250df10de5df884b0b09a22b")
    finalizer = load_hex(slot / "finalizer.rue.hex", 494,
        "7561d066e49565e123e800c03898fe3739576d204060371c27ab3aed5b35ef19",
        "9d274de59aeaa128d1b0a802b52911838420ef9720d605c78942e8dadfc0810b")
    launcher = load_hex(upstream / "singleton_launcher.clsp.hex", 175, th="eff07522495060c066f66f32acc2a77e3a3e737aca8baea4d1a64ea4cdc13da9")
    top = load_hex(upstream / "singleton_top_layer_v1_1.clsp.hex", 967, th="7faa3253bfddd1e0decb0906b2dc6247bbc4cf608f58345d173adb63e8b47c9f")
    standard_record = json.loads(bounded_read(ROOT / "crates/chialisp-oxide-package/tests/fixtures/mainnet-standard/expected.json", codec.MAX_BYTES))
    standard = codec.decode(bytes.fromhex(standard_record["module_hex"]))
    require(treehash(standard).hex() == "e9aaa49f45bad5c889b86ee3341550c155cfdd10c3a6757de618d20612fffd52", "standard module pin")
    return action, finalizer, launcher, top, standard


def admit_packet(path):
    raw = bounded_read(path, 16 * 1024 * 1024)
    packet = json.loads(raw)
    require(packet["protocol"] in ("chialisp-oxide-action-counter-artifacts/v1", "chialisp-oxide-action-counter-artifacts/v2"), "packet protocol")
    expected = [(c, m, e) for c in ("original", "increment_no_change", "reset_drop_signature") for m in ("off", "on") for e in ("increment", "reset")]
    expected += [("original", "chialisp", e) for e in ("increment", "reset")]
    if packet["protocol"].endswith("/v2"):
        expected += [("original", m, e) for m in ("rue-off", "rue-on") for e in ("increment", "reset")]
    rows = packet["artifacts"]
    require(len(rows) == len(expected), "complete inventory")
    total = 0
    for row, (control, mode, export) in zip(rows, expected):
        require((row["control"], row["mode"], row["export"], row["id"]) == (control, mode, export, f"{control}/{mode}/{export}"), "closed ordered inventory")
        text = row["clvm_hex"]
        require(type(text) is str and 0 < len(text) <= 2 * codec.MAX_BYTES and len(text) % 2 == 0, "blob precharge")
        total += len(text) // 2
        require(total <= 16 * codec.MAX_BYTES, "aggregate blob precharge")
    admitted = {}
    decoded_allocation_charge = 0
    for row in rows:
        text = row["clvm_hex"]
        raw_blob = bytes.fromhex(text)
        require(raw_blob.hex() == text, "canonical hex")
        inspection = codec.inspect(raw_blob)
        # Each bounded streaming inspection has its own codec cap. This separate
        # aggregate charges decoded allocation before any decoded tree is built;
        # it is not a claim about total scan work or exact Python memory.
        decoded_allocation_charge += inspection["decoded_work_profile"]["charged_bytes"]
        require(decoded_allocation_charge <= 128 * codec.MAX_BYTES, "aggregate decoded allocation precharge")
        require(same_json(inspection["identity"],row["inspection"]["identity"]), "all blob identities")
        admitted[row["id"]] = codec.decode(raw_blob)
    return sha(raw).hex(), admitted


class Policy:
    def __init__(self, increment, reset, owner, launcher_id, pinned=None):
        require(type(owner) is bytes and len(owner) == 48, "owner atom48")
        require(type(launcher_id) is bytes and len(launcher_id) == 32, "launcher atom32")
        self.action, finalizer, self.launcher_module, self.top, self.standard = pinned or modules()
        self.owner, self.launcher = owner, launcher_id
        self.actions = (increment, curry(reset, owner))
        self.leaves = tuple(sha(b"\x01" + treehash(p)) for p in self.actions)
        self.root = sha(b"\x02" + self.leaves[0] + self.leaves[1])
        first = curry(finalizer, treehash(self.action), launcher_id)
        self.finalizer = curry(first, treehash(first))

    def inner(self, state):
        return curry(self.action, self.finalizer, self.root, uint(state))

    def full(self, state):
        struct = (treehash(self.top), (self.launcher, treehash(self.launcher_module)))
        return curry(self.top, struct, self.inner(state))

    def solution(self, actions):
        require(type(actions) in (tuple, list) and 0 < len(actions) <= 8 and all(type(a) is int and a in (0, 1) for a in actions), "bounded action batch")
        unique = list(dict.fromkeys(actions))
        proven, entries = set(), []
        for a in reversed(actions):
            proof = NIL if a in proven else proper(uint(a), self.leaves[1-a])
            proven.add(a)
            selector = (2, 5)[unique.index(a)]
            entries.append((uint(selector), proof))
        return proper(proper(*(self.actions[a] for a in unique)), proper(*entries), proper(*(NIL for _ in actions)))

    def transition(self, state, actions, control="original"):
        uint(state)
        require(control in ("original", "increment_no_change", "reset_drop_signature"), "declared source control")
        require(type(actions) in (tuple, list) and 0 < len(actions) <= 8, "bounded transition batch")
        groups = []
        for action in actions:
            require(type(action) is int and action in (0, 1), "known action")
            if action == 0:
                if control != "increment_no_change":
                    require(state < MAX, "checked increment overflow")
                    state += 1
                groups.append([])
            else:
                state = 0
                groups.append([] if control == "reset_drop_signature" else [proper(uint(50), self.owner, INTENT)])
        return state, [c for g in reversed(groups) for c in g]

    def conditions(self, state, actions, full=False, parent=None, control="original"):
        next_state, other = self.transition(state, actions, control)
        destination = self.full(next_state) if full else self.inner(next_state)
        output = [proper(uint(51), treehash(destination), uint(1), proper(self.launcher)), *other]
        if full:
            require(type(parent) is bytes and len(parent) == 32, "actual asserted parent")
            output = [proper(uint(73), uint(1)), proper(uint(71), parent), *output]
        return proper(*output)


def coin_id(parent, puzzle, amount):
    require(type(parent) is bytes and len(parent) == 32 and type(puzzle) is bytes and len(puzzle) == 32, "coin byte domains")
    return sha(parent + puzzle + uint(amount))


def check_coin(row, parent=None, puzzle=None, amount=1):
    p, h = bytes.fromhex(row["parent_coin_id"]), bytes.fromhex(row["puzzle_hash"])
    require(row["amount"] == amount and type(row["amount"]) is int, "exact coin amount")
    require(coin_id(p, h, amount).hex() == row["coin_id"], "independent coin name")
    require(parent is None or p == parent, "actual coin parent")
    require(puzzle is None or h == puzzle, "actual coin puzzle")
    return bytes.fromhex(row["coin_id"])


def exact_hex(text, tree, message):
    require(text == codec.encode(tree).hex(), message)


def check_execution(execution, expected):
    require(execution["outcome"]["status"] == "accepted" and execution["failure"] is None, "accepted primary execution")
    require(type(execution["cost"]) is int and 0 < execution["cost"] <= 250_000_000, "bounded measured cost")
    exact_hex(execution["outcome"]["output_hex"], expected, "independent exact primary output")


def check_minimal(report, packet_path):
    packet_sha, artifacts = admit_packet(packet_path)
    require(report["protocol"] in ("chialisp-oxide-action14-prototype/v1", "chialisp-oxide-action14-prototype/v2") and report["artifact_packet_sha256"] == packet_sha, "exact prototype input")
    modes = ["off", "on", "chialisp"]
    if report["protocol"].endswith("/v2"):
        require(len(artifacts) == 18, "complete five-mode source packet")
        modes += ["rue-off", "rue-on"]
    else:
        require(len(artifacts) == 14, "original three-mode source packet")
    require(same_json(report["primary_vm"],{"version":"clvmr 0.21.0", "flags":535, "max_cost":250_000_000}), "primary protocol")
    require(same_json(report["sdk"],{"simulator":"chia-sdk-test 0.36.0", "consensus":"chia-consensus 0.36.1", "vm":"clvmr 0.16.2", "network":"TESTNET11", "max_transaction_cost":5_500_000_000}), "SDK protocol")
    require([r["mode"] for r in report["runs"]] == modes, "all declared actual action modes")
    pinned = modules()
    for run in report["runs"]:
        owner = bytes.fromhex(run["owner_hex"])
        wallet = curry(pinned[4], owner)
        genesis = check_coin(run["genesis"], puzzle=treehash(wallet))
        launcher = check_coin(run["launcher"], genesis, treehash(pinned[2]))
        mode = run["mode"]
        policy = Policy(artifacts[f"original/{mode}/increment"], artifacts[f"original/{mode}/reset"], owner, launcher, pinned)
        require(run["merkle_root"] == policy.root.hex(), "independent Merkle commitment")
        eve = check_coin(run["eve"], launcher, treehash(policy.full(0)))
        check_coin(run["increment_child"], eve, treehash(policy.full(1)))
        components = run["outer_components"]
        require(len(components) == 1 and components[0]["name"] == "increment_finalizer", "exact component observation")
        exact_hex(components[0]["reveal_hex"], policy.inner(0), "outer state commitment")
        exact_hex(components[0]["solution_hex"], policy.solution([0]), "outer solution")
        check_execution(components[0]["execution"], policy.conditions(0, [0]))
        launch_solution = proper(treehash(policy.full(0)), uint(1), NIL)
        announcement = sha(launcher + treehash(launch_solution))
        fund_conditions = proper(proper(uint(51), treehash(pinned[2]), uint(1)), proper(uint(61), announcement))
        delegated = (uint(1), fund_conditions)
        fund_solution = proper(NIL, delegated, NIL)
        eve_solution = proper(proper(genesis, uint(1)), uint(1), policy.solution([0]))
        expected = [
            ("fund_and_launch_singleton", [
                (run["genesis"], wallet, fund_solution, (proper(uint(50), owner, treehash(delegated)), fund_conditions)),
                (run["launcher"], pinned[2], launch_solution, proper(proper(uint(51), treehash(policy.full(0)), uint(1)), proper(uint(60), treehash(launch_solution))))]),
            ("eve_increment", [(run["eve"], policy.full(0), eve_solution, policy.conditions(0, [0], True, launcher))])]
        require(len(run["attempts"]) == 2, "exact SDK submission count")
        for attempt, (name, spends) in zip(run["attempts"], expected):
            require(attempt["name"] == name and attempt["outcome"] == {"status":"accepted"}, "actual accepted SDK observation")
            tx = attempt["transaction"]
            require(len(bytes.fromhex(tx["signature_hex"])) == 96, "serialized aggregate signature")
            require(len(tx["spends"]) == len(spends) == len(attempt["primary_vm"]), "complete transaction/component inventory")
            for actual, execution, (coin, reveal, solution, output) in zip(tx["spends"], attempt["primary_vm"], spends):
                require(same_json(actual["coin"],coin), "spent exact declared coin")
                exact_hex(actual["reveal_hex"], reveal, "exact independently constructed reveal")
                exact_hex(actual["solution_hex"], solution, "exact independently constructed solution")
                check_execution(execution, output)
    return {"status":"independent-minimal-reader-passed", "sdk_observations":len(modes)*2, "primary_observations":len(modes)*4, "scope":"Reader only; no additional execution, BLS verification or complete issue14 credit."}


def check_followups(report, packet_path):
    minimal = check_minimal(report, packet_path)
    _, artifacts = admit_packet(packet_path)
    pinned = modules()
    sdk_count = 0
    for run in report["runs"]:
        mode = run["mode"]
        launcher = bytes.fromhex(run["launcher"]["coin_id"])
        owner = bytes.fromhex(run["owner_hex"])
        policy = Policy(artifacts[f"original/{mode}/increment"], artifacts[f"original/{mode}/reset"], owner, launcher, pinned)
        initial = run["increment_child"]
        initial_id = bytes.fromhex(initial["coin_id"])
        eve_parent = bytes.fromhex(run["eve"]["parent_coin_id"])
        initial_parent = bytes.fromhex(initial["parent_coin_id"])
        lineage = proper(eve_parent, treehash(policy.inner(0)), uint(1))
        cases = run["lifecycle_cases"]
        expected_cases = [("reset", [1]), ("increment_then_reset", [0, 1]), ("reset_then_increment", [1, 0]), ("reset_twice", [1, 1])]
        require(len(cases) == len(expected_cases), "complete alternative branches")
        single_reset_signature = None
        for case, (name, actions) in zip(cases, expected_cases):
            state, _ = policy.transition(1, actions)
            require(case["name"] == name and case["actions"] == actions and all(type(a) is int for a in case["actions"]), "named ordered actions")
            require(case["ledger_relation"] == "alternative clone of the same unspent increment_child", "alternative ledger scope")
            require(type(case["initial_state"]) is int and case["initial_state"] == 1 and type(case["next_state"]) is int and case["next_state"] == state, "independently calculated next state")
            child = check_coin(case["child"], initial_id, treehash(policy.full(state)))
            require(type(case["continued_state"]) is int and case["continued_state"] == state+1, "continued state")
            check_coin(case["continued_child"], child, treehash(policy.full(state+1)))
            solution = proper(lineage, uint(1), policy.solution(actions))
            continued_lineage = proper(initial_parent, treehash(policy.inner(1)), uint(1))
            continued_solution = proper(continued_lineage, uint(1), policy.solution([0]))
            attempts = case["attempts"]
            expected_names = ["missing_reset_signature", name, "consumed_bundle_replay", "continue_actual_child"]
            if name == "reset_twice":
                expected_names.insert(1, "single_signature_for_two_resets")
            require([a["name"] for a in attempts] == expected_names, "exact attempt sequence")
            signed_tx = None
            for attempt in attempts:
                sdk_count += 1
                aname = attempt["name"]
                continuation = aname == "continue_actual_child"
                expected_coin = case["child"] if continuation else initial
                expected_reveal = policy.full(state if continuation else 1)
                expected_solution = continued_solution if continuation else solution
                expected_conditions = policy.conditions(state if continuation else 1, [0] if continuation else actions, True, initial_id if continuation else initial_parent)
                tx = attempt["transaction"]
                require(len(tx["spends"]) == len(attempt["primary_vm"]) == 1, "single singleton component")
                spend = tx["spends"][0]
                require(same_json(spend["coin"],expected_coin), "actual branch coin")
                exact_hex(spend["reveal_hex"], expected_reveal, "committed branch reveal")
                exact_hex(spend["solution_hex"], expected_solution, "actual lineage/action solution")
                check_execution(attempt["primary_vm"][0], expected_conditions)
                signature = bytes.fromhex(tx["signature_hex"])
                require(len(signature) == 96, "aggregate signature encoding")
                if aname in ("missing_reset_signature", "single_signature_for_two_resets", "consumed_bundle_replay"):
                    code = "DoubleSpend" if aname == "consumed_bundle_replay" else "BadAggregateSignature"
                    require(attempt["outcome"] == {"status":"rejected", "stage":"consensus", "code":code, "complete_state_unchanged":True}, "typed rejected submission/state equality observation")
                    require(attempt["outcome"]["complete_state_unchanged"] is True, "boolean state marker")
                    if aname == "consumed_bundle_replay":
                        require(same_json(tx,signed_tx), "exact consumed transaction replay")
                else:
                    require(attempt["outcome"] == {"status":"accepted"}, "actual accepted branch")
                    if not continuation:
                        signed_tx = tx
                        if name == "reset":
                            single_reset_signature = signature
                        elif name == "reset_twice":
                            require(signature != single_reset_signature, "double reset multiplicity signature differs")
                if aname in ("missing_reset_signature", "continue_actual_child"):
                    require(signature == b"\xc0" + bytes(95), "actual empty signature control")
                elif aname == "single_signature_for_two_resets":
                    require(signature == single_reset_signature and signature != b"\xc0" + bytes(95), "exact single-reset signature with insufficient multiplicity")
    return {"status":"independent-followup-reader-passed", "minimal":minimal, "followup_sdk_observations":sdk_count, "scope":"Recorded reader; no additional SDK/VM execution or independent BLS verification."}


def replace_path(tree, path, replacement):
    require(type(path) is list and len(path) <= 64 and all(type(p) is bool for p in path), "bounded physical path")
    parents = []
    cursor = tree
    for right in path:
        require(type(cursor) is tuple, "physical pair path")
        parents.append((cursor, right))
        cursor = cursor[int(right)]
    cursor = replacement
    for pair, right in reversed(parents):
        cursor = (pair[0], cursor) if right else (cursor, pair[1])
    o = codec.encode(cursor)
    require(len(o) <= codec.MAX_BYTES, "rewritten transport bound")
    return cursor


def check_adversarial(report, packet_path):
    followups = check_followups(report, packet_path)
    _, artifacts = admit_packet(packet_path)
    pinned = modules()
    program_codes = {f["code"] for f in json.loads(bounded_read(ROOT / "crates/chialisp-oxide-vm/failure-protocol.json", 65536))["failures"] if f["kind"] == "program"}
    domain = bytes.fromhex("37a90eb5185a9c4439a91ddc98bbadce7b4feba060d50116a067de66bf236615")
    signatures = ["wrong_key_signature", "wrong_message_signature", "wrong_coin_signature", "wrong_domain_signature"]
    prefix = [True, True, False]
    variants = [
        ("wrong_membership_sibling", prefix+[True,False,False,True,True,False], bytes([0x45])*32, "GeneratorRuntimeError"),
        ("missing_membership_sibling", prefix+[True,False,False,True,True], NIL, "GeneratorRuntimeError"),
        ("first_scanned_nil_proof", prefix+[True,False,False,True], NIL, "GeneratorRuntimeError"),
        ("missing_action_solution", prefix+[True,True,False], NIL, "GeneratorRuntimeError"),
        ("wrong_three_field_lineage", [False,False], bytes([0x46])*32, "AssertMyParentIdFailed"),
        ("wrong_two_field_eve_proof", [False], proper(bytes([0x47])*32,uint(1)), "GeneratorRuntimeError"),
        ("claimed_odd_amount_three", [True,False], uint(3), "AssertMyAmountFailed"),
        ("claimed_even_amount_two", [True,False], uint(2), "GeneratorRuntimeError"),
        ("proof_path_high_bit_alias", prefix+[True,False,False,True,False], uint(2), None),
        ("unused_extra_puzzle", prefix+[False,True], proper(uint(1)), None),
        ("unused_improper_solution_tail", prefix+[True,True,False,True], uint(1), None),
        ("ignored_finalizer_pair", prefix+[True,True,True], (bytes([0xaa]),bytes([0xbb])), None),
    ]
    names = signatures + [v[0] for v in variants] + ["repeat_increment", "repeat_increment_three"]
    total = 0
    for run in report["runs"]:
        mode = run["mode"]
        owner = bytes.fromhex(run["owner_hex"])
        policy = Policy(artifacts[f"original/{mode}/increment"], artifacts[f"original/{mode}/reset"], owner, bytes.fromhex(run["launcher"]["coin_id"]), pinned)
        initial = run["increment_child"]
        initial_id = bytes.fromhex(initial["coin_id"])
        parent = bytes.fromhex(initial["parent_coin_id"])
        lineage = proper(bytes.fromhex(run["eve"]["parent_coin_id"]), treehash(policy.inner(0)), uint(1))
        cases = run["adversarial_cases"]
        require([c["name"] for c in cases] == names, "complete named adversarial inventory")
        for case in cases:
            total += 1
            name = case["name"]
            actions = [1] if name in signatures else [0,0,0] if name == "repeat_increment_three" else [0,0] if name == "repeat_increment" else [0]
            require(type(case["initial_state"]) is int and case["initial_state"] == 1 and case["actions"] == actions and all(type(a) is int for a in case["actions"]), "adversarial action/state domain")
            solution = proper(lineage, uint(1), policy.solution(actions))
            conditions = policy.conditions(1, actions, True, parent)
            code = "BadAggregateSignature" if name in signatures else None
            mutation = case["mutation"]
            if name in signatures:
                require(mutation["kind"] == "signature", "signature mutation")
                message = bytes([INTENT[0]^1])+INTENT[1:] if name == "wrong_message_signature" else INTENT
                bound_coin = bytes([0x44])*32 if name == "wrong_coin_signature" else initial_id
                selected_domain = bytes(32) if name == "wrong_domain_signature" else domain
                require(mutation["condition_message_hex"] == message.hex() and mutation["bound_coin_id"] == bound_coin.hex() and mutation["additional_data_hex"] == selected_domain.hex(), "exact public signature mutation recipe")
                require(mutation["signed_message_hex"] == (message+bound_coin+selected_domain).hex(), "independent AGG_SIG_ME preimage")
                key = bytes.fromhex(mutation["signing_public_key_hex"])
                require(len(key) == 48 and ((key != owner) if name == "wrong_key_signature" else (key == owner)), "intended signing-key relation")
            elif name.startswith("repeat_increment"):
                require(mutation == {"kind":"none"}, "actual repeated action recipe")
            else:
                _, path, replacement, code = next(v for v in variants if v[0] == name)
                require(mutation == {"kind":"replace_physical_path", "path":path, "replacement_hex":codec.encode(replacement).hex()}, "exact independently specified physical mutation")
                require(all(type(p) is bool for p in mutation["path"]), "actual bool path")
                solution = replace_path(solution, path, replacement)
                if name == "wrong_three_field_lineage":
                    bad_parent = coin_id(bytes([0x46])*32, treehash(policy.full(0)), 1)
                    conditions = policy.conditions(1, [0], True, bad_parent)
                elif name == "claimed_odd_amount_three":
                    # Only ASSERT_MY_AMOUNT changes; the finalizer still returns
                    # amount-one recreation and the lineage parent is unchanged.
                    conditions = (proper(uint(73),uint(3)), conditions[1])
            require(len(case["attempts"]) == 1, "one actual adversarial submission")
            attempt = case["attempts"][0]
            require(attempt["name"] == name, "exact submitted adversarial case")
            tx = attempt["transaction"]
            require(len(tx["spends"]) == len(attempt["primary_vm"]) == 1, "one adversarial spend/execution")
            spend = tx["spends"][0]
            require(same_json(spend["coin"],initial), "same unspent branch coin")
            exact_hex(spend["reveal_hex"], policy.full(1), "unchanged committed reveal")
            exact_hex(spend["solution_hex"], solution, "independently mutated input")
            sig = bytes.fromhex(tx["signature_hex"])
            require(len(sig) == 96 and ((sig != b"\xc0"+bytes(95)) if name in signatures else (sig == b"\xc0"+bytes(95))), "actual nonempty invalid signature or permissionless empty signature")
            execution = attempt["primary_vm"][0]
            if code == "GeneratorRuntimeError":
                require(execution["outcome"] == {"status":"rejected"} and execution["cost"] is None and execution["failure"]["kind"] == "program" and execution["failure"]["code"] in program_codes, "runtime umbrella separately resolved to typed primary program failure")
            else:
                check_execution(execution, conditions)
            if code is None:
                state, _ = policy.transition(1, actions)
                require(type(case["expected_state"]) is int and case["expected_state"] == state, "accepted adversarial state")
                check_coin(case["child"], initial_id, treehash(policy.full(state)))
                require(attempt["outcome"] == {"status":"accepted"}, "measured canonical accepted behavior")
            else:
                require(case["expected_state"] is None and case["child"] is None, "no accepted child on rejection")
                require(attempt["outcome"] == {"status":"rejected", "stage":"runtime_umbrella" if code == "GeneratorRuntimeError" else "consensus", "code":code, "complete_state_unchanged":True} and attempt["outcome"]["complete_state_unchanged"] is True, "exact typed SDK rejection/state relation")
    return {"status":"independent-adversarial-reader-passed", "followups":followups, "adversarial_sdk_observations":total, "scope":"Reader only; no additional execution/BLS/compiler-preservation credit."}


def check_source_controls(report, packet_path):
    original = check_adversarial(report, packet_path)
    _, artifacts = admit_packet(packet_path)
    pinned = modules()
    rows = report["source_control_runs"]
    expected = [(c,m) for c in ("increment_no_change", "reset_drop_signature") for m in ("off","on")]
    require([(r["control"],r["mode"]) for r in rows] == expected, "all four actual source controls")
    for run, (control,mode) in zip(rows,expected):
        identity = control == "increment_no_change"
        action = 0 if identity else 1
        inc_id = f"{control}/{mode}/increment" if identity else "original/chialisp/increment"
        reset_id = "original/chialisp/reset" if identity else f"{control}/{mode}/reset"
        require(run["companion_mode"] == "chialisp" and run["policy_increment_artifact_id"] == inc_id and run["policy_reset_artifact_id"] == reset_id, "independent reference companion identity")
        require(type(run["initial_state"]) is int and run["initial_state"] == 5 and run["actions"] == [action] and type(run["actions"][0]) is int, "mutant source state/action")
        require(run["original_policy_requires_signature"] is (not identity) and run["observed_requires_signature"] is False, "original/observed authorization distinction")
        require(type(run["original_policy_expected_state"]) is int and run["original_policy_expected_state"] == (6 if identity else 0), "original independent policy expectation")
        state = 5 if identity else 0
        require(type(run["observed_state"]) is int and run["observed_state"] == state, "mutated state expectation")
        owner = bytes.fromhex(run["owner_hex"])
        wallet = curry(pinned[4], owner)
        genesis = check_coin(run["genesis"], puzzle=treehash(wallet))
        launcher = check_coin(run["launcher"],genesis,treehash(pinned[2]))
        policy = Policy(artifacts[inc_id],artifacts[reset_id],owner,launcher,pinned)
        require(run["merkle_root"] == policy.root.hex(), "each mutant's own committed root")
        eve = check_coin(run["eve"],launcher,treehash(policy.full(5)))
        check_coin(run["child"],eve,treehash(policy.full(state)))
        launch_solution = proper(treehash(policy.full(5)),uint(1),NIL)
        announcement = sha(launcher+treehash(launch_solution))
        fund_conditions = proper(proper(uint(51),treehash(pinned[2]),uint(1)),proper(uint(61),announcement))
        delegated = (uint(1),fund_conditions)
        expected_spends = [
            [(run["genesis"],wallet,proper(NIL,delegated,NIL),(proper(uint(50),owner,treehash(delegated)),fund_conditions)),
             (run["launcher"],pinned[2],launch_solution,proper(proper(uint(51),treehash(policy.full(5)),uint(1)),proper(uint(60),treehash(launch_solution))))],
            [(run["eve"],policy.full(5),proper(proper(genesis,uint(1)),uint(1),policy.solution([action])),policy.conditions(5,[action],True,launcher,control))]]
        require(len(run["attempts"]) == 2, "fund and mutant transition inventory")
        for attempt, name, spends in zip(run["attempts"], ("fund_and_launch_singleton","mutant_unsigned_transition"), expected_spends):
            require(attempt["name"] == name and attempt["outcome"] == {"status":"accepted"}, "actual accepted source-control submission")
            tx = attempt["transaction"]
            require(len(tx["spends"]) == len(spends) == len(attempt["primary_vm"]), "complete mutant components")
            signature = bytes.fromhex(tx["signature_hex"])
            require(len(signature) == 96 and ((signature == b"\xc0"+bytes(95)) if name == "mutant_unsigned_transition" else (signature != b"\xc0"+bytes(95))), "actual unsigned mutant / signed funding")
            for spend, execution, (coin,reveal,solution,conditions) in zip(tx["spends"],attempt["primary_vm"],spends):
                require(same_json(spend["coin"],coin), "actual own funded mutant coin")
                exact_hex(spend["reveal_hex"],reveal,"mutant own reveal")
                exact_hex(spend["solution_hex"],solution,"mutant exact solution")
                check_execution(execution,conditions)
    return {"status":"independent-source-control-reader-passed", "original":original, "source_control_sdk_observations":8, "scope":"Reader of actual-source controls with each mutant's own funded root; no extra execution or compiler-preservation credit."}


if __name__ == "__main__":
    require(len(sys.argv) == 3, "report and artifact packet paths")
    report = json.loads(bounded_read(Path(sys.argv[1]), 16 * codec.MAX_BYTES))
    checker = check_source_controls if "source_control_runs" in report else check_adversarial if any("adversarial_cases" in run for run in report["runs"]) else check_followups if any("lifecycle_cases" in run for run in report["runs"]) else check_minimal
    print(json.dumps(checker(report, Path(sys.argv[2])), sort_keys=True))
