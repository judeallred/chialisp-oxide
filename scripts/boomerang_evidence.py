"""Independent finite boomerang policy, instance and recorded-ledger checks.

Trees and canonical integers are checked independently of all three puzzle
sources. Reading a receipt does not authenticate execution, verify BLS or prove
full simulator equality, compiler preservation, liveness or production safety.
Only replay_retained executes the already retained primary-VM program bytes.
"""
import hashlib
import json
from pathlib import Path
import re
import tempfile

from cargo_authoring import contained_file, unique_object
import raw_tree
from vm_failures import check_execution, check_native_outcome

ROOT = Path(__file__).resolve().parents[1]
SLUG = "boomerang"
DIRECTORY = "puzzles/boomerang"
PROTOCOL = "chialisp-oxide-boomerang-instances/v1"
ROLE = "original_teaching_coin_instances"
SCHEME = "(a (q . M) (c (q . A) 1))"
CONSTRUCTOR = "independent clvmr standard-form construction cross-checked with chialisp_oxide::curry"
SCOPE = "Original teaching policy with one publicly known demonstration key and fixed payment terms. Each of five compiled templates has its own independently constructed, SDK-cross-checked curried instance. Primary VM observations, SDK consensus attempts and actual source mutants are separate finite evidence; no automatic refund service, mainnet/fork universality, exhaustive authorization, BLS proof, optional-output proof, compiler preservation, authenticated execution or production-readiness claim."
MODES = [("rust_mir", False), ("rust_mir", True), ("chialisp", True),
         ("rue_source", False), ("rue_source", True)]
MAX_COST = 1_000_000
OWNER = bytes.fromhex("8bc61e93380f3427ed2ace813fdff67504f55268a99a72cea57133c2b32a1d836fd52381ae9ca10957557b80064059e5")
OTHER_KEY = bytes.fromhex("b47fa1e15162a5b1f9444d1aae57d131a1d5c98bade8f8e93b8ebfe1649b95c55787b8731d2f1a0638b3e1d23ada48e8")
RECIPIENT, REFUND = b"B" * 32, b"C" * 32
INTENT = b"chialisp-oxide/boomerang/claim:1"
EMPTY_SIGNATURE = bytes.fromhex("c0" + "00" * 95)
ADDITIONAL_DATA = bytes.fromhex("37a90eb5185a9c4439a91ddc98bbadce7b4feba060d50116a067de66bf236615")
# Independent spellings of the existing pinned Simulator's first two parents.
PARENT = "ae2915f8495b82ab18f7f06a86c7498e34d193781a0906f753a08d4b9afd3fc8"
OTHER_PARENT = "1c9bdb76cf327fe9af5a533ffc71baa6c93ca0ebc7fe37eb86f4ae90502771a7"
TEMPLATE_VECTORS_SHA256 = "16e0036a7f65bd6c375dc180cee6897ece68a0686c7ba205928a4df8c1818c6e"
INSTANCE_VECTORS_SHA256 = "69c30c1467564c486deef182aa8d7637a32c69f2838318aeabce215628041def"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def closed(value, keys, label):
    require(type(value) is dict and set(value) == set(keys), label + ": missing/unknown fields")


def same(left, right):
    """JSON equality preserves Boolean/integer/float distinctions in receipts."""
    return json.dumps(left, sort_keys=True, separators=(",", ":"), allow_nan=False) == json.dumps(right, sort_keys=True, separators=(",", ":"), allow_nan=False)


def read_source(path):
    with contained_file(ROOT, path).open("rb") as stream:
        payload = stream.read(16 * 1024 * 1024 + 1)
    require(0 < len(payload) <= 16 * 1024 * 1024, "Boomerang source byte bound")
    return payload


def load(payload):
    try:
        return json.loads(payload, object_pairs_hook=unique_object)
    except (UnicodeError, json.JSONDecodeError, RecursionError) as error:
        raise ValueError("Malformed boomerang JSON") from error


def hex_bytes(value, *, size=None):
    require(type(value) is str and 0 < len(value) <= 4_000_000 and len(value) % 2 == 0
            and re.fullmatch(r"[0-9a-f]+", value)
            and (size is None or len(value) == 2 * size), "Malformed retained boomerang hex")
    return bytes.fromhex(value)


def tree(value):
    payload = hex_bytes(value)
    try:
        result = raw_tree.deserialize(payload)
        require(raw_tree.serialize(result) == payload, "Noncanonical retained CLVM serialization")
        return result
    except RecursionError as error:
        raise ValueError("Retained tree exceeds checker recursion bound") from error


def parse(text):
    require(type(text) is str and 0 < len(text) <= 2_000_000, "Boomerang fixture text bound")
    try:
        return raw_tree.parse(text)
    except RecursionError as error:
        raise ValueError("Fixture tree exceeds checker recursion bound") from error


def proper(*items):
    result = b""
    for item in reversed(items):
        result = item, result
    return result


def fields(value, count):
    result = []
    for _ in range(count):
        if type(value) is not tuple or len(value) != 2:
            return None
        result.append(value[0])
        value = value[1]
    return result if value == b"" else None


def integer(value):
    require(type(value) is int, "Expected an integer")
    if value == 0:
        return b""
    width = (value.bit_length() + 8) // 8
    encoded = value.to_bytes(width, "big", signed=True)
    while len(encoded) > 1 and ((encoded[0] == 0 and encoded[1] < 128)
                               or (encoded[0] == 255 and encoded[1] >= 128)):
        encoded = encoded[1:]
    return encoded


def uint(value):
    # The complete source u64 domain, including zero and u64::MAX. These are
    # semantic width/canonicality checks, not a positivity or timing policy.
    if type(value) is not bytes or len(value) > 9:
        return None
    number = int.from_bytes(value, "big", signed=True)
    return number if 0 <= number < 2**64 and integer(number) == value else None


def public_terms():
    return proper(OWNER, RECIPIENT, REFUND, integer(1000), integer(100))


def relation(environment):
    arguments = fields(environment, 2)
    if arguments is None:
        return {"status": "rejected"}
    terms, claim = arguments
    terms = fields(terms, 5)
    if terms is None or claim not in (b"", b"\1"):
        return {"status": "rejected"}
    key, recipient, refund, amount, deadline = terms
    if not (type(key) is bytes and len(key) == 48
            and type(recipient) is bytes and len(recipient) == 32
            and type(refund) is bytes and len(refund) == 32
            and uint(amount) is not None and uint(deadline) is not None):
        return {"status": "rejected"}
    conditions = [proper(integer(85 if claim else 81), deadline),
                  proper(integer(73), amount),
                  proper(integer(51), recipient if claim else refund, amount)]
    if claim:
        conditions.append(proper(integer(50), key, INTENT))
    return {"status": "accepted", "output_hex": raw_tree.serialize(proper(*conditions)).hex()}


def expected(solution):
    return relation(parse(solution))


def curry(module, argument):
    return proper(b"\2", (b"\1", module), proper(b"\4", (b"\1", argument), b"\1"))


def modes(rows, label):
    require(type(rows) is list and all(type(row) is dict and type(row.get("optimize_lir")) is bool for row in rows)
            and [(row.get("backend"), row.get("optimize_lir")) for row in rows] == MODES,
            label + ": missing, unknown, duplicate or reordered five compiler modes")


def artifact(value):
    closed(value, ["backend", "optimize_lir", "bytes", "tree_hash", "clvm_hex", "disassembly"], "boomerang artifact")
    require(type(value["backend"]) is str and type(value["optimize_lir"]) is bool
            and type(value["bytes"]) is int and 0 < value["bytes"] <= 2_000_000
            and type(value["disassembly"]) is str and value["disassembly"], "Malformed boomerang artifact")
    program = tree(value["clvm_hex"])
    require(len(bytes.fromhex(value["clvm_hex"])) == value["bytes"]
            and raw_tree.tree_hash(program).hex() == value["tree_hash"], "Retained artifact size/tree hash mismatch")
    return program


def vector_digest(rows):
    require(type(rows) is list, "Missing boomerang vectors")
    recipes = [{"label": row["label"], "solution_hex": raw_tree.serialize(parse(row["solution"])).hex()}
               for row in rows]
    require(all(type(row["label"]) is str and row["label"] for row in rows)
            and len({row["label"] for row in rows}) == len(rows), "Missing/duplicate boomerang vector labels")
    return hashlib.sha256(json.dumps(recipes, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def expected_failure(mode, label):
    # Exact finite extra-tail witnesses, not a theorem about every decoder.
    # Rust/off tests a remaining pair with atom equality against nil, which
    # faults InvalidOpArg. The reviewed optimized path and independent source
    # guards instead Raise. Keep this mode distinction visible and closed.
    extra_pair_tails = {"extra outer argument", "extra terms field", "extra path argument", "malformed_extra_argument"}
    return "invalid_op_arg" if mode == ("rust_mir", False) and label in extra_pair_tails else "raise"


def execution(value, desired, label, mode=None):
    require(check_execution(value, label) == desired, label + ": output/rejection differs from independent policy")
    if desired["status"] == "accepted":
        require(value["cost"] <= MAX_COST, label + ": primary VM exceeded declared budget")
    else:
        code = expected_failure(mode, label)
        require(value.get("failure") == {"kind": "program", "code": code}, label + ": wrong exact typed ABI failure code")


def cases(actual, required, argument=None):
    require(type(actual) is list and len(actual) == len(required), "Missing/extra boomerang cases")
    accepted = 0
    for case, recipe in zip(actual, required, strict=True):
        closed(case, ["label", "solution", "native", "runs"], "boomerang case")
        require(case["label"] == recipe["label"] and parse(case["solution"]) == parse(recipe["solution"]),
                "Changed boomerang case identity/input/order")
        solution = parse(recipe["solution"])
        desired = relation(solution if argument is None else (argument, solution))
        if "expected" in recipe:
            require(check_native_outcome(recipe["expected"]) == desired, "Manifest expectation differs from independent boomerang policy")
        require(check_native_outcome(case["native"]) == desired, "Actual native boomerang differs from independent policy")
        accepted += desired["status"] == "accepted"
        modes(case["runs"], "boomerang case")
        for run in case["runs"]:
            closed(run, ["backend", "optimize_lir", "execution"], "boomerang run")
            execution(run["execution"], desired, case["label"], (run["backend"], run["optimize_lir"]))
    return accepted


def source_identity(puzzle, manifest, reader):
    for key in ["slug", "title", "kind", "security_scope", "provenance"]:
        require(puzzle.get(key) == manifest[key], "Changed boomerang source claim: " + key)
    require(puzzle.get("validated") is True and puzzle.get("environment", "strict") == "strict"
            and puzzle.get("reference_mode", "module") == "module", "Missing strict validated boomerang contract")
    require(puzzle["source"] == reader(DIRECTORY + "/puzzle.rs").decode()
            and puzzle["reference"] == reader(DIRECTORY + "/reference.clsp").decode(), "Stale boomerang source snapshot")
    rue = reader(DIRECTORY + "/puzzle.rue")
    require(puzzle["rue"] == {"path": DIRECTORY + "/puzzle.rue", "source": rue.decode(),
                              "sha256": hashlib.sha256(rue).hexdigest(), "dependencies": []}, "Stale boomerang Rue source")
    require("module_identity" not in puzzle and "protocol_validation" not in puzzle
            and "curried_instances" not in puzzle and "curried_examples" not in puzzle,
            "Original boomerang must not claim canonical/module/component evidence")
    for key in ["source_dependencies", "native_sources"]:
        rows = puzzle.get(key, [])
        require(type(rows) is list and [row.get("path") for row in rows] == manifest.get(key, []),
                "Changed boomerang source closure: " + key)
        for row in rows:
            keys = ["path", "sha256", "source"] if key == "source_dependencies" else ["path", "sha256"]
            closed(row, keys, "boomerang source identity")
            payload = reader(row["path"])
            require(hashlib.sha256(payload).hexdigest() == row["sha256"]
                    and (key != "source_dependencies" or row["source"] == payload.decode()), "Stale boomerang source dependency")


def coin(value):
    closed(value, ["coin_id", "parent_coin_id", "puzzle_hash", "amount"], "boomerang coin")
    require(type(value["amount"]) is int and 0 <= value["amount"] < 2**64, "Invalid ledger coin amount")
    parent = hex_bytes(value["parent_coin_id"], size=32)
    puzzle_hash = hex_bytes(value["puzzle_hash"], size=32)
    require(value["coin_id"] == hashlib.sha256(parent + puzzle_hash + integer(value["amount"])).hexdigest(),
            "Ledger coin ID does not bind parent/puzzle/amount")
    return value


# (label, timestamp, path, signing, expected consensus code). None is acceptance;
# malformed paths have their own primary Raise/runtime-umbrella lane below.
ATTEMPTS = [
    ("claim_before_deadline", 99, True, "recipient", None),
    ("refund_at_deadline", 100, False, "empty", None),
    ("refund_after_deadline", 101, False, "empty", None),
    ("claim_at_deadline", 100, True, "recipient", "AssertBeforeSecondsAbsoluteFailed"),
    ("claim_after_deadline", 101, True, "recipient", "AssertBeforeSecondsAbsoluteFailed"),
    ("refund_before_deadline", 99, False, "empty", "AssertSecondsAbsoluteFailed"),
    ("claim_missing_signature", 99, True, "empty", "BadAggregateSignature"),
    ("claim_wrong_key", 99, True, "wrong_key", "BadAggregateSignature"),
    ("claim_wrong_intent", 99, True, "wrong_intent", "BadAggregateSignature"),
    ("claim_other_coin_signature", 99, True, "other_coin", "BadAggregateSignature"),
    ("refund_amount_too_small", 100, False, "empty", "AssertMyAmountFailed"),
    ("refund_amount_too_large", 100, False, "empty", "AssertMyAmountFailed"),
    *[("substituted_" + field, 101, False, "empty", "WrongPuzzleHash")
      for field in ["recipient_key", "recipient", "refund", "amount", "deadline"]],
    ("refund_replay_setup", 100, False, "empty", None),
    ("refund_replay", 101, False, "empty", "DoubleSpend"),
    ("claim_then_refund_setup", 99, True, "recipient", None),
    ("claim_then_refund", 100, False, "empty", "DoubleSpend"),
]
MALFORMED = [
    ("malformed_empty_solution", "()"), ("malformed_atom_solution", "1"),
    ("malformed_bool_two", "(2)"), ("malformed_zero_byte", "(0x00)"),
    ("malformed_padded_true", "(0x0001)"), ("malformed_pair_bool", "((1))"),
    ("malformed_extra_argument", "(1 0)"), ("malformed_improper_tail", "(1 . 2)"),
]


def selected_terms(label):
    values = [OWNER, RECIPIENT, REFUND, integer(1000), integer(100)]
    substitutions = {
        "substituted_recipient_key": (0, OTHER_KEY),
        "substituted_recipient": (1, b"D" * 32),
        "substituted_refund": (2, b"E" * 32),
        "substituted_amount": (3, integer(1001)),
        "substituted_deadline": (4, integer(101)),
    }
    if label in substitutions:
        index, replacement = substitutions[label]
        values[index] = replacement
    return proper(*values)


def signature(attempt, original_reveal):
    kind, primary = attempt["signature_kind"], coin(attempt["coin"])
    retained = hex_bytes(attempt["signature_hex"], size=96)
    context, origin = attempt["signature_context"], attempt["signature_origin"]
    if kind == "empty":
        require(retained == EMPTY_SIGNATURE and context is None and origin is None,
                "Empty signature is not actual BLS identity/null signing context")
        return
    require(kind in ["recipient", "wrong_key", "wrong_intent", "other_coin"] and retained != EMPTY_SIGNATURE,
            "Missing/nonempty signature distinction")
    intent = b"\xcd" * 32 if kind == "wrong_intent" else INTENT
    key = OTHER_KEY if kind == "wrong_key" else OWNER
    signing_coin = primary
    if kind in ["wrong_intent", "other_coin"]:
        closed(origin, ["coin", "reveal_hex", "reveal_tree_hash", "solution", "solution_hex"], "signature origin")
        signing_coin = coin(origin["coin"])
        require(parse(origin["solution"]) == proper(b"\1") and origin["solution_hex"] == "ff0180",
                "Signature origin does not use exact claim solution")
        if kind == "wrong_intent":
            require(signing_coin == primary, "Wrong-intent signature changed coin")
            conditions = fields(tree(relation(proper(public_terms(), b"\1"))["output_hex"]), 4)
            conditions[3] = proper(integer(50), OWNER, intent)
            origin_reveal = b"\1", proper(*conditions)
        else:
            require(signing_coin["coin_id"] != primary["coin_id"] and signing_coin["amount"] == 1000
                    and signing_coin["puzzle_hash"] == primary["puzzle_hash"]
                    and signing_coin["parent_coin_id"] == OTHER_PARENT,
                    "Other-coin signature did not change only actual coin identity")
            origin_reveal = original_reveal
        require(tree(origin["reveal_hex"]) == origin_reveal
                and origin["reveal_tree_hash"] == raw_tree.tree_hash(origin_reveal).hex(), "Wrong signer-only reveal/origin binding")
    else:
        require(origin is None, "Unexpected signature origin for direct signing")
    expected_context = {"public_key": key.hex(), "intent_hex": intent.hex(),
                        "coin_id": signing_coin["coin_id"], "additional_data": ADDITIONAL_DATA.hex(),
                        "message_hex": (intent + bytes.fromhex(signing_coin["coin_id"]) + ADDITIONAL_DATA).hex()}
    require(context == expected_context, "Signature key/intent/coin/network/augmented-message context changed")


def state(value):
    closed(value, ["height", "next_timestamp", "created_height", "spent_height", "children"], "boomerang ledger state")
    require(type(value["height"]) is int and 0 <= value["height"] < 2**32
            and type(value["next_timestamp"]) is int and 0 <= value["next_timestamp"] < 2**64
            and type(value["created_height"]) is int and value["created_height"] == 0
            and (value["spent_height"] is None or type(value["spent_height"]) is int and value["spent_height"] == 0)
            and type(value["children"]) is list and len(value["children"]) <= 1, "Malformed/unreviewed visible ledger state")
    for child in value["children"]:
        coin(child)


def states(attempt, accepted, claim, previous=None):
    before, after = attempt["before"], attempt["after"]
    state(before)
    state(after)
    require(type(attempt["full_state_unchanged"]) is bool and attempt["full_state_unchanged"] is not accepted,
            "Missing/false full simulator equality flag")
    require(before["next_timestamp"] == attempt["timestamp"], "Recorded clock differs from consensus submission time")
    if previous is None:
        require(before == {"height": 0, "next_timestamp": attempt["timestamp"], "created_height": 0,
                           "spent_height": None, "children": []}, "Fresh fixture was not unspent/childless")
    else:
        require(before == previous["after"] and attempt["coin"] == previous["coin"], "Replay is not the exact prior spent coin/state")
    if not accepted:
        require(after == before, "Rejected submission changed recorded state")
        return
    destination = RECIPIENT if claim else REFUND
    require(after["height"] == 1 and after["next_timestamp"] == attempt["timestamp"] + 1
            and after["spent_height"] == 0 and len(after["children"]) == 1, "Accepted spend missing exact block/input transition")
    child = after["children"][0]
    require(child["parent_coin_id"] == attempt["coin"]["coin_id"] and child["puzzle_hash"] == destination.hex()
            and type(child["amount"]) is int and child["amount"] == 1000, "Accepted payout changed full amount/destination/parent")


def validate_ledger(rows, modules, instances):
    modes(rows, "boomerang ledger")
    recipes = ATTEMPTS + [(label, 100, None, "empty", "GeneratorRuntimeError") for label, _ in MALFORMED]
    malformed = dict(MALFORMED)
    for row, module_artifact, instance, mode in zip(rows, modules, instances, MODES, strict=True):
        closed(row, ["backend", "optimize_lir", "instance_label", "module_tree_hash", "instance_tree_hash", "simulator", "consensus", "vm", "network", "transaction_attempts", "attempts"], "boomerang ledger comparison")
        require(row["instance_label"] == "public_demo_payment" and row["module_tree_hash"] == module_artifact["tree_hash"]
                and row["instance_tree_hash"] == instance["tree_hash"] and row["simulator"] == "chia-sdk-test 0.36.0"
                and row["consensus"] == "chia-consensus 0.36.1" and row["vm"] == "clvmr 0.16.2"
                and row["network"] == "TESTNET11", "Ledger compiler/instance/SDK/VM/network binding changed")
        attempts = row["attempts"]
        require(type(row["transaction_attempts"]) is int and row["transaction_attempts"] == 29
                and type(attempts) is list and len(attempts) == 29, "Missing/extra actual29 transaction attempts")
        previous = {}
        module = tree(module_artifact["clvm_hex"])
        original = tree(instance["clvm_hex"])
        for attempt, (label, timestamp, claim, signing, code) in zip(attempts, recipes, strict=True):
            closed(attempt, ["label", "timestamp", "argument_hex", "coin", "reveal_hex", "reveal_tree_hash", "solution", "solution_hex", "signature_kind", "signature_hex", "signature_context", "signature_origin", "vm_021", "consensus", "before", "after", "full_state_unchanged"], "boomerang transaction attempt")
            require(attempt["label"] == label and type(attempt["timestamp"]) is int and attempt["timestamp"] == timestamp
                    and attempt["signature_kind"] == signing, "Changed ledger label/order/time/signing context")
            argument = selected_terms(label)
            reveal = curry(module, argument)
            require(attempt["argument_hex"] == raw_tree.serialize(argument).hex()
                    and tree(attempt["reveal_hex"]) == reveal and attempt["reveal_tree_hash"] == raw_tree.tree_hash(reveal).hex(),
                    "Ledger reveal does not bind own module and selected exact terms")
            input_coin = coin(attempt["coin"])
            amount = 999 if label == "refund_amount_too_small" else 1001 if label == "refund_amount_too_large" else 1000
            require(input_coin["puzzle_hash"] == instance["tree_hash"] and input_coin["amount"] == amount
                    and input_coin["parent_coin_id"] == PARENT, "Starting coin changed original commitment/amount/fixture parent")
            require((attempt["reveal_tree_hash"] != row["instance_tree_hash"]) is label.startswith("substituted_"),
                    "Wrong-reveal control did not change exactly its committed term")
            solution = parse(malformed[label]) if claim is None else proper(b"\1" if claim else b"")
            require(parse(attempt["solution"]) == solution and attempt["solution_hex"] == raw_tree.serialize(solution).hex(),
                    "Changed ledger bool/malformed solution")
            execution(attempt["vm_021"], relation((argument, solution)), label, mode)
            signature(attempt, original)
            runtime = claim is None
            accepted = code is None
            desired = {"status": "accepted" if accepted else "runtime_umbrella" if runtime else "consensus_rejected",
                       "code": code, "error": None if accepted else f"Validation({code})",
                       "semantic_rejection_credit": not accepted and not runtime}
            require(attempt["consensus"] == desired and type(attempt["consensus"]["semantic_rejection_credit"]) is bool,
                    "Wrong typed consensus code or misleading runtime/preflight rejection credit")
            replay_setup = {"refund_replay": "refund_replay_setup", "claim_then_refund": "claim_then_refund_setup"}.get(label)
            states(attempt, accepted, claim, previous.get(replay_setup))
            previous[label] = attempt
        require(attempts[0]["signature_hex"] == attempts[3]["signature_hex"] == attempts[4]["signature_hex"]
                == attempts[19]["signature_hex"], "Timestamp controls did not carry the same actual timely claim signature")
    return {"ledger_groups": 5, "transaction_attempts": 145, "accepted_spends": 25,
            "semantic_rejections": 80, "runtime_umbrellas_without_semantic_credit": 40}


def source_contract(reader=read_source):
    """Check declared input recipes/oracle only; no compiled execution credit."""
    manifest = load(reader(DIRECTORY + "/puzzle.json"))
    require(manifest.get("slug") == SLUG and manifest.get("kind") == "coin_puzzle"
            and manifest.get("environment", "strict") == "strict" and manifest.get("reference_mode", "module") == "module"
            and type(manifest.get("validation_max_cost", MAX_COST)) is int
            and manifest.get("validation_max_cost", MAX_COST) == MAX_COST,
            "Changed boomerang source/budget contract")
    require(len(manifest["cases"]) == 101 and vector_digest(manifest["cases"]) == TEMPLATE_VECTORS_SHA256,
            "Changed reviewed101 template vector recipes")
    accepted = 0
    for row in manifest["cases"]:
        closed(row, ["label", "solution", "expected"], "declared boomerang vector")
        desired = expected(row["solution"])
        require(check_native_outcome(row["expected"]) == desired, "Manifest expectation differs from independent boomerang policy")
        accepted += desired["status"] == "accepted"
    require(accepted == 28, "Changed28 accepted/73 rejected template coverage")
    specs = manifest.get("coin_instances")
    require(type(specs) is list and len(specs) == 1, "Exactly one reviewed boomerang fixture instance required")
    spec = specs[0]
    closed(spec, ["label", "argument", "solutions"], "boomerang fixture spec")
    require(spec["label"] == "public_demo_payment" and parse(spec["argument"]) == public_terms()
            and len(spec["solutions"]) == 14 and vector_digest(spec["solutions"]) == INSTANCE_VECTORS_SHA256,
            "Changed fixed five-term commitment or14 instance vector recipes")
    for row in spec["solutions"]:
        closed(row, ["label", "solution"], "declared boomerang instance vector")
    return manifest


def validate_coin(puzzle, *, reader=read_source):
    """Check the dedicated lane; never accept an empty generic ledger as credit."""
    require(puzzle.get("slug") == SLUG and puzzle.get("kind") == "coin_puzzle"
            and puzzle.get("simulator") == [] and puzzle.get("simulator_comparisons") == [],
            "Boomerang requires its explicit five-build coin-instance ledger lane")
    manifest = source_contract(reader)
    source_identity(puzzle, manifest, reader)
    require(manifest.get("environment", "strict") == "strict" and manifest.get("reference_mode", "module") == "module"
            and manifest.get("validation_max_cost", MAX_COST) == MAX_COST
            and "validation_max_cost" not in puzzle and puzzle.get("generated_cases") == 0
            and type(puzzle.get("generated_cases")) is int, "Changed boomerang budget/generated/source contract")
    template_accepted = cases(puzzle["cases"], manifest["cases"])
    require(template_accepted == 28, "Changed28 accepted/73 rejected template coverage")
    modes(puzzle["artifacts"], "boomerang modules")
    modules = [artifact(row) for row in puzzle["artifacts"]]
    spec = manifest["coin_instances"][0]
    argument = public_terms()
    value = puzzle["coin_instances"]
    closed(value, ["protocol", "role", "scheme", "examples", "ledger", "scope"], "boomerang coin instances")
    require(value["protocol"] == PROTOCOL and value["role"] == ROLE and value["scheme"] == SCHEME
            and value["scope"] == SCOPE, "Changed boomerang coin-instance protocol/scope")
    require(type(value["examples"]) is list and len(value["examples"]) == 1, "Missing/extra fixed-term instance")
    example = value["examples"][0]
    closed(example, ["label", "argument", "argument_hex", "argument_tree_hash", "scheme", "constructor", "instances", "cases"], "boomerang example")
    require(example["label"] == spec["label"] and parse(example["argument"]) == argument
            and example["argument_hex"] == raw_tree.serialize(argument).hex()
            and example["argument_tree_hash"] == raw_tree.tree_hash(argument).hex()
            and example["scheme"] == SCHEME and example["constructor"] == CONSTRUCTOR, "Changed boomerang argument/constructor binding")
    modes(example["instances"], "boomerang instances")
    instances = []
    for row, module, module_artifact, mode in zip(example["instances"], modules, puzzle["artifacts"], MODES, strict=True):
        closed(row, ["backend", "optimize_lir", "module_tree_hash", "artifact", "sdk_owned_curry_bytes_equal", "sdk_hash_equal"], "boomerang curried artifact")
        require(row["sdk_owned_curry_bytes_equal"] is True and row["sdk_hash_equal"] is True
                and row["module_tree_hash"] == module_artifact["tree_hash"]
                and (row["artifact"]["backend"], row["artifact"]["optimize_lir"]) == mode
                and type(row["artifact"]["optimize_lir"]) is bool, "Changed SDK check or compiler module/mode binding")
        instance = artifact(row["artifact"])
        require(instance == curry(module, argument), "Instance is not exact C(own module, complete committed terms)")
        instances.append(row["artifact"])
    require(cases(example["cases"], spec["solutions"], argument) == 2, "Changed two accepted/12 malformed instance cases")
    ledger = validate_ledger(value["ledger"], puzzle["artifacts"], instances)
    return {"slug": SLUG, "status": "finite_recorded_evidence_checked", "template_cases": 101,
            "instance_cases": 14, "compiler_modes": 5, **ledger,
            "scope": "Recorded consistency and independent finite policy/commitment checks; no execution authentication, BLS, full-state, compiler-preservation or production-safety proof."}


def replay_retained(puzzle, cli, *, reader=read_source):
    """Execute exact retained template/instance bytes, never Cargo or a compiler."""
    validate_coin(puzzle, reader=reader)
    from port_evidence import run_batch
    total = 0
    example = puzzle["coin_instances"]["examples"][0]
    groups = [(puzzle["artifacts"], puzzle["cases"], None),
              ([row["artifact"] for row in example["instances"]], example["cases"], public_terms())]
    with tempfile.TemporaryDirectory(prefix="boomerang-retained-vm-") as directory:
        for group, (artifacts, rows, argument) in enumerate(groups):
            solutions = Path(directory) / f"{group}-solutions.json"
            solutions.write_text(json.dumps([row["solution"] for row in rows]))
            for mode_index, retained in enumerate(artifacts):
                program = Path(directory) / f"{group}-{mode_index}.hex"
                program.write_text(retained["clvm_hex"] + "\n")
                executions = run_batch(Path(cli), program, solutions, len(rows), max_cost=MAX_COST)
                for observed, row in zip(executions, rows, strict=True):
                    solution = parse(row["solution"])
                    desired = relation(solution if argument is None else (argument, solution))
                    execution(observed, desired, row["label"], (retained["backend"], retained["optimize_lir"]))
                    require(observed == row["runs"][mode_index]["execution"], "Retained VM output/cost/typed failure differs from capture")
                    total += 1
    return {"retained_primary_vm_replays": total,
            "replay_scope": "Retained template and instance bytes in clvmr0.21 only; no fresh compiler, SDK/BLS ledger or source-mutation execution."}


CONTROL_NAMES = ("original", "equivalent", "drop_claim_signature", "late_claim", "early_refund")
CONTROL_RECIPES_SHA256 = "e637871296ed685802b3bd8cd0df25425eab66c9478e2e52b4af8550182479fb"


def control_output(name, claim):
    """Independent explicit distinguishing relation for the three policy edits."""
    require(name in CONTROL_NAMES and type(claim) is bool, "Unknown source-control policy/path")
    result = relation(proper(public_terms(), b"\1" if claim else b""))
    conditions = fields(tree(result["output_hex"]), 4 if claim else 3)
    if name == "drop_claim_signature" and claim:
        conditions.pop()
    if (name == "late_claim" and claim) or (name == "early_refund" and not claim):
        conditions[0] = proper(integer(81), b"")
    return {"status": "accepted", "output_hex": raw_tree.serialize(proper(*conditions)).hex()}


def control_witnesses(common):
    scenarios = [("timely_unsigned_claim", True, 99, "none", "BadAggregateSignature"),
                 ("signed_late_claim", True, 101, "recipient", "AssertBeforeSecondsAbsoluteFailed"),
                 ("early_unsigned_refund", False, 99, "none", "AssertSecondsAbsoluteFailed")]
    rows = common.witnesses()
    recipes = [(name, scenario) for index, name in enumerate(CONTROL_NAMES)
               for scenario in (scenarios if index < 2 else [scenarios[index - 2]])]
    require(len(rows) == 9, "Exactly nine actual-source witness recipes required")
    for row, (name, (label, claim, timestamp, signing, code)) in zip(rows, recipes, strict=True):
        closed(row, ["name", "witness", "claim", "timestamp", "signing", "solution", "instance_solution", "expected", "ledger_expected"], "source-control witness")
        require((row["name"], row["witness"], row["claim"], row["timestamp"], row["signing"])
                == (name, label, claim, timestamp, signing) and type(row["claim"]) is bool
                and type(row["timestamp"]) is int and parse(row["solution"]) == proper(public_terms(), b"\1" if claim else b"")
                and parse(row["instance_solution"]) == proper(b"\1" if claim else b"")
                and row["expected"] == control_output(name, claim)
                and row["ledger_expected"] == ({"status": "consensus_rejected", "code": code} if name in CONTROL_NAMES[:2] else {"status": "accepted"}),
                "Source-control recipe differs from independent policy-error witness")
    return rows


def _control_context(report, base, root, common):
    import tomllib
    from p2dc_source_controls import compiler_host
    context = report["context"]
    closed(context, ["source_root", "package_root", "capture_root", "compiler", "toolchain", "source_window", "tools", "last_host_window"], "source-control context")
    source, package, capture = (common.absolute(context[key]) for key in ["source_root", "package_root", "capture_root"])
    host = compiler_host(context["compiler"])
    require(context["toolchain"] == tomllib.loads(common.read(Path(root) / "rust-toolchain.toml").decode())["toolchain"]["channel"]
            == "nightly-2026-07-14" and context["source_window"] == common.source_window(root), "Stale source-control compiler/native/SDK/reader input window")
    closed(context["tools"], common.TOOLS, "source-control tools")
    parents = set()
    for name, row in context["tools"].items():
        closed(row, ["path", "sha256"], "captured tool identity")
        path = common.absolute(row["path"])
        hex_bytes(row["sha256"], size=32)
        require(path.name == name and path.parent.name == "debug", "Captured tool path/name differs")
        parents.add(path.parent)
    require(len(parents) == 1, "Captured tools do not share one host target")
    closed(context["last_host_window"], ["path", "sha256"], "LAST-host receipt binding")
    common.absolute(context["last_host_window"]["path"])
    quiet_path = common.relative(base, "last-host-window.json")
    quiet = common.load(quiet_path)
    closed(quiet, ["schema_version", "protocol", "status", "source_root", "package_root", "compiler", "toolchain", "source_window", "tools", "native_receipt_sha256", "scope"], "LAST-host quiet receipt")
    require(type(quiet["schema_version"]) is int and quiet["schema_version"] == 1
            and quiet["protocol"] == "chialisp-oxide-boomerang-controls-quiet-window/v1"
            and quiet["status"] == "LAST_host_build_terminal_tools_quiet"
            and all(quiet[key] == context[key] for key in ["source_root", "package_root", "compiler", "toolchain", "source_window", "tools"])
            and type(quiet["scope"]) is str and quiet["scope"]
            and common.sha(quiet_path) == context["last_host_window"]["sha256"]
            and common.sha(common.relative(base, "native-receipt.json")) == quiet["native_receipt_sha256"], "Broken LAST-host/native/source/tool identity relation")
    return context, source, package, capture, host, next(iter(parents))


def _control_native(report, base, root, common, context, source, package):
    import tomllib
    prepared, native = report["prepared"], report["native"]
    closed(prepared, ["recipe", "source_files", "witnesses"], "prepared source controls")
    expected_files, recipe = common.package_files(source, root)
    witnesses = control_witnesses(common)
    require(same(prepared["recipe"], recipe) and same(prepared["witnesses"], witnesses),
            "Retained base/checked mutation recipes or witnesses changed")
    require(type(prepared["source_files"]) is list and [row["path"] for row in prepared["source_files"]] == sorted(common.FILES + ["Cargo.lock"]),
            "Closed generated source/package file set required")
    for row in prepared["source_files"]:
        closed(row, ["path", "sha256"], "prepared source identity")
        payload = common.read(common.relative(base, "package/" + row["path"]))
        require(hashlib.sha256(payload).hexdigest() == row["sha256"], "Retained prepared source digest changed")
        if row["path"] == "Cargo.lock":
            tomllib.loads(payload.decode())
        else:
            require(payload == expected_files[row["path"]], "Retained source is not exact base plus the checked recipe")
    closed(native, ["argv", "exit_code", "native_test_groups", "native_policy_observations", "log", "log_sha256", "ledger_binary"], "native source-control receipt")
    receipt = common.load(common.relative(base, "native-receipt.json"))
    closed(receipt, ["schema_version", "protocol", "status", "source_root", "package_root", "toolchain", "source_window", "prepared", "native", "commands"], "native phase receipt")
    require(type(receipt["schema_version"]) is int and receipt["schema_version"] == 1
            and receipt["protocol"] == "chialisp-oxide-boomerang-controls-native/v1"
            and receipt["status"] == "native_phase_passed_before_LAST_host_build"
            and all(receipt[key] == context[key] for key in ["source_root", "package_root", "toolchain", "source_window"])
            and same(receipt["prepared"], prepared) and same(receipt["native"], native), "Native source/package/log relation changed")
    phases = [(["generate-lockfile", "--offline"], []), (["fmt"], ["--all", "--check"]),
              (["check"], ["--all-targets", "--locked", "--offline"]),
              (["test"], ["--locked", "--offline", "--", "--nocapture"]),
              (["clippy"], ["--all-targets", "--locked", "--offline", "--", "-D", "warnings"]),
              (["build"], ["--example", "ledger", "--locked", "--offline"])]
    require(type(receipt["commands"]) is list and len(receipt["commands"]) == 6, "Six ordinary native/check/fmt/Clippy/SDK-build commands required")
    retained_files = set()
    for index, (row, (flags, tail)) in enumerate(zip(receipt["commands"], phases, strict=True)):
        closed(row, ["argv", "exit_code", "log", "log_sha256"], "native command receipt")
        expected = ["rustup", "run", context["toolchain"], "cargo", *flags, "--manifest-path", str(package / "Cargo.toml"), *tail]
        require(row["argv"] == expected and type(row["exit_code"]) is int and row["exit_code"] == 0
                and row["log"] == f"native-logs/{index}.log"
                and common.sha(common.relative(base, row["log"])) == row["log_sha256"], "Native command/log/normal-exit relation changed")
        retained_files.add(row["log"])
    require(native["argv"] == receipt["commands"][3]["argv"] and type(native["exit_code"]) is int and native["exit_code"] == 0
            and type(native["native_test_groups"]) is int and native["native_test_groups"] == 1
            and type(native["native_policy_observations"]) is int and native["native_policy_observations"] == 9
            and native["log"] == "native.log" and native["log_sha256"] == receipt["commands"][3]["log_sha256"]
            and common.sha(common.relative(base, native["log"])) == native["log_sha256"], "One native group/nine exact policy outputs required")
    common.native_rows(common.read(common.relative(base, native["log"])).decode())
    closed(native["ledger_binary"], ["path", "sha256"], "captured SDK runner")
    binary = common.absolute(native["ledger_binary"]["path"])
    hex_bytes(native["ledger_binary"]["sha256"], size=32)
    require(binary.name == "ledger" and binary.parent.name == "examples" and binary.parent.parent.name == "debug",
            "Unreviewed SDK example binary path")
    return witnesses, binary, retained_files


def _control_bundles(report, base, common, context, package, capture, host):
    import cargo_bundle
    closed(report["bundles"], ["off", "on"], "two source-control compiler modes")
    closed(report["instances"], ["off", "on"], "two source-control instance modes")
    artifacts, ir_hashes, compile_receipts, commands, retained_files = {}, {}, [], [], set()
    reader = cargo_bundle._Reader()
    for mode in ["off", "on"]:
        value = report["bundles"][mode]
        closed(value, ["manifest", "manifest_sha256", "recompiled"], "retained source-control bundle")
        require(value["manifest"] == f"bundles/{mode}/manifest.json", "Unexpected retained bundle path")
        manifest_path = common.relative(base, value["manifest"])
        require(common.sha(manifest_path) == value["manifest_sha256"], "Retained bundle manifest bytes changed")
        manifest = common.load(manifest_path)
        closed(manifest, ["analysis_target", "cargo_options", "compiler", "driver_sha256", "entries", "manifest_path", "package", "package_id", "rue_revision", "schema_version", "status", "toolchain", "workspace_root"], "actual Cargo manifest")
        require(type(manifest["schema_version"]) is int and manifest["schema_version"] == 1 and manifest["status"] == "wip_prototype_not_ready_for_use"
                and manifest["package"] == common.PACKAGE and common.absolute(manifest["workspace_root"]) == package
                and common.absolute(manifest["manifest_path"]) == package / "Cargo.toml"
                and manifest["package_id"] == f"path+{package.as_uri()}#{common.PACKAGE}@0.1.0"
                and manifest["analysis_target"] == host and manifest["compiler"] == context["compiler"]
                and manifest["toolchain"] == context["toolchain"] and manifest["driver_sha256"] == context["tools"]["chialisp-oxide-driver"]["sha256"]
                and manifest["cargo_options"] == common.options(package) and manifest["rue_revision"] == "245c107a8e795229ea08097142f005a4c0d91075",
                "Changed actual Cargo package/compiler/driver/options/analysis identity")
        entries = manifest["entries"]
        require(type(entries) is list and len(entries) == 5
                and {entry["entry"] for entry in entries} == {f"chialisp_oxide_boomerang_controls::{name}::puzzle" for name in CONTROL_NAMES},
                "Complete five actual source-control exports required")
        require(type(value["recompiled"]) is list and [row["entry"] for row in value["recompiled"]] == [entry["entry"] for entry in entries], "Missing/reordered five IR recompilation receipts")
        commands.append(("export-" + mode, [context["tools"]["cargo-chialisp-oxide"]["path"], "build", *common.options(package), "--output-dir", str(capture / ("fresh-" + mode))] + (["--no-optimize"] if mode == "off" else []), None))
        closed(report["instances"][mode], CONTROL_NAMES, "five funded source-control instances")
        paths = set()
        retained_files.add(value["manifest"])
        for entry, record in zip(entries, value["recompiled"], strict=True):
            name = entry["entry"].split("::")[-2]
            program, _, ir_hash = cargo_bundle._artifact(entry, manifest_path.parent, reader, paths, mode == "on")
            common.strict_abi(program, entry["entry"])
            closed(record, ["entry", "ir_sha256", "artifacts"], "validated IR recompilation")
            compiled = record["artifacts"]
            require(record["ir_sha256"] == ir_hash and type(compiled) is list and len(compiled) == 2
                    and [row["optimize"] for row in compiled] == [False, True], "Wrong IR/recompilation mode identity")
            for result in compiled:
                closed(result, ["optimize", "bytes", "clvm_hex", "tree_hash"], "IR compile artifact")
                require(type(result["optimize"]) is bool, "IR optimization flag must be Boolean")
                artifact(dict(backend="rust_mir", optimize_lir=result["optimize"], disassembly="retained recompile", **{key: result[key] for key in ["bytes", "clvm_hex", "tree_hash"]}))
            selected = compiled[mode == "on"]
            require(all(selected[key] == entry["artifact"][key] for key in ["bytes", "clvm_hex", "tree_hash"]), "Cargo and validated IR recompilation disagree")
            module = artifact(entry["artifact"])
            artifacts[(mode, name)] = entry["artifact"]
            ir_hashes[(mode, name)] = ir_hash
            compile_receipts.append((name, compiled))
            commands.append((f"recompile-{mode}-{name}", [context["tools"]["chialisp-oxide-verifier"]["path"], str(capture / f"bundles/{mode}" / entry["ir"]), "--artifacts"], compiled))
            instance = report["instances"][mode][name]
            closed(instance, ["path", "bytes", "clvm_hex", "tree_hash", "module_tree_hash"], "funded mutant instance")
            expected = raw_tree.serialize(curry(module, public_terms()))
            require(instance["path"] == f"instances/{mode}/{name}.clvm.hex" and type(instance["bytes"]) is int
                    and instance["bytes"] == len(expected) and instance["clvm_hex"] == expected.hex()
                    and instance["tree_hash"] == raw_tree.tree_hash(tree(expected.hex())).hex()
                    and instance["module_tree_hash"] == entry["artifact"]["tree_hash"]
                    and common.read(common.relative(base, instance["path"])) == (expected.hex() + "\n").encode(),
                    "Mutant is not funded under its own exact compiled module/terms/curry")
            retained_files.update([instance["path"], f"bundles/{mode}/" + entry["ir"], f"bundles/{mode}/" + entry["clvm"]])
        for witness in control_witnesses(common):
            name, label = witness["name"], witness["witness"]
            instance = report["instances"][mode][name]
            solution = f"inputs/{mode}-{name}-{label}.json"
            require(common.load(common.relative(base, solution)) == [witness["instance_solution"]], "Actual VM input file/witness differs")
            retained_files.add(solution)
            commands.append((f"vm-{mode}-{name}-{label}", [context["tools"]["chialisp-oxide"]["path"], "run-batch", str(capture / instance["path"]), "--solutions", str(capture / solution), "--max-cost", "1000000"], (mode, name, label)))
    for name in CONTROL_NAMES:
        require(ir_hashes[("off", name)] == ir_hashes[("on", name)], "Unoptimized source IR differs across compiler modes")
    for name, compiled in compile_receipts:
        for result in compiled:
            require(all(result[key] == artifacts[("on" if result["optimize"] else "off", name)][key] for key in ["bytes", "clvm_hex", "tree_hash"]),
                    "Each validated recompile must reproduce both actual retained modes")
    return artifacts, commands, retained_files


def _control_runs(report, witnesses):
    runs = report["runs"]
    require(type(runs) is list and len(runs) == 18, "Exactly eighteen retained primary-VM source witnesses required")
    for row, (mode, witness) in zip(runs, [(mode, witness) for mode in ["off", "on"] for witness in witnesses], strict=True):
        closed(row, ["mode", "name", "witness", "solution", "instance_tree_hash", "instance_sha256", "execution"], "actual-source VM witness")
        instance = report["instances"][mode][witness["name"]]
        require((row["mode"], row["name"], row["witness"], row["solution"])
                == (mode, witness["name"], witness["witness"], witness["instance_solution"])
                and row["instance_tree_hash"] == instance["tree_hash"]
                and row["instance_sha256"] == hashlib.sha256(bytes.fromhex(instance["clvm_hex"])).hexdigest(),
                "Source witness uses another instance/path/solution")
        execution(row["execution"], control_output(witness["name"], witness["claim"]), "actual-source VM witness")


def _control_ledger(report, base, common, witnesses, artifacts):
    value = report["ledger"]
    closed(value, ["request", "request_sha256", "output", "output_sha256", "result"], "source-control SDK capture")
    require(value["request"] == "ledger-request.json" and value["output"] == "ledger.json"
            and common.sha(common.relative(base, value["request"])) == value["request_sha256"]
            and common.sha(common.relative(base, value["output"])) == value["output_sha256"], "SDK request/output retained byte binding changed")
    selected = [(mode, witness) for mode in ["off", "on"] for witness in witnesses]
    request = {"protocol": common.LEDGER_PROTOCOL, "runs": [
        {"mode": mode, "name": witness["name"], "witness": witness["witness"],
         "module_hex": artifacts[(mode, witness["name"])]["clvm_hex"],
         "instance_hex": report["instances"][mode][witness["name"]]["clvm_hex"]} for mode, witness in selected]}
    require(common.load(common.relative(base, value["request"])) == request, "Actual SDK request does not use all own funded mutant instances")
    ledger = value["result"]
    require(same(common.load(common.relative(base, value["output"])), ledger), "SDK parsed result differs from raw output")
    closed(ledger, ["protocol", "simulator", "consensus", "vm", "network", "scenario_runs", "transaction_attempts", "attempts"], "SDK source-control result")
    require(ledger["protocol"] == common.LEDGER_PROTOCOL and ledger["simulator"] == "chia-sdk-test 0.36.0"
            and ledger["consensus"] == "chia-consensus 0.36.1" and ledger["vm"] == "clvmr 0.16.2" and ledger["network"] == "TESTNET11"
            and type(ledger["scenario_runs"]) is int and ledger["scenario_runs"] == 18
            and type(ledger["transaction_attempts"]) is int and ledger["transaction_attempts"] == 18
            and type(ledger["attempts"]) is list and len(ledger["attempts"]) == 18, "SDK scope/version/counts changed")
    for attempt, (mode, witness) in zip(ledger["attempts"], selected, strict=True):
        closed(attempt, ["mode", "name", "witness", "claim", "timestamp", "signing", "module_tree_hash", "instance_tree_hash", "instance_hex", "solution_hex", "signature_hex", "input", "outcome"], "source-control SDK attempt")
        name, claim = witness["name"], witness["claim"]
        instance = report["instances"][mode][name]
        require((attempt["mode"], attempt["name"], attempt["witness"], attempt["claim"], attempt["timestamp"], attempt["signing"])
                == (mode, name, witness["witness"], claim, witness["timestamp"], witness["signing"])
                and type(attempt["claim"]) is bool and type(attempt["timestamp"]) is int
                and attempt["module_tree_hash"] == artifacts[(mode, name)]["tree_hash"]
                and attempt["instance_tree_hash"] == instance["tree_hash"] and attempt["instance_hex"] == instance["clvm_hex"]
                and attempt["solution_hex"] == raw_tree.serialize(parse(witness["instance_solution"])).hex(),
                "SDK source witness clock/path/compiler/module/instance binding changed")
        primary = coin(attempt["input"])
        require(primary["parent_coin_id"] == PARENT and primary["puzzle_hash"] == instance["tree_hash"] and primary["amount"] == 1000,
                "Source mutant was not funded under its own full-amount curried reveal")
        signature = hex_bytes(attempt["signature_hex"], size=96)
        require((signature == EMPTY_SIGNATURE) is (witness["signing"] == "none"), "Actual omitted/nonempty late-claim signature changed")
        outcome = attempt["outcome"]
        if name in CONTROL_NAMES[:2]:
            require(outcome == {"status": "consensus_rejected", "code": witness["ledger_expected"]["code"],
                                "semantic_rejection_credit": True, "full_state_unchanged": True}
                    and outcome["semantic_rejection_credit"] is True and outcome["full_state_unchanged"] is True,
                    "Original/equivalent requires exact typed consensus failure and full rollback")
        else:
            closed(outcome, ["status", "input_spent", "child_count", "child"], "accepted policy-error counterexample")
            require(outcome["status"] == "accepted" and outcome["input_spent"] is True
                    and type(outcome["child_count"]) is int and outcome["child_count"] == 1,
                    "Mutant did not admit the forbidden complete spend")
            child = coin(outcome["child"])
            require(child["parent_coin_id"] == primary["coin_id"] and child["amount"] == 1000
                    and child["puzzle_hash"] == (RECIPIENT if claim else REFUND).hex(), "Policy mutant counterexample changed payout/parent/amount")
    return ledger


def _control_commands(report, base, common, expected, capture, binary, ledger):
    expected = [*expected, ("sdk-ledger", [str(binary), str(capture / "ledger-request.json")], ledger)]
    rows = report["commands"]
    require(type(rows) is list and len(rows) == len(expected) == 31, "Exactly two exports/ten recompile/eighteen VM/one SDK commands required")
    files = set()
    for index, (row, (role, argv, result)) in enumerate(zip(rows, expected, strict=True)):
        closed(row, ["role", "argv", "exit_code", "stdout", "stdout_sha256", "stderr", "stderr_sha256"], "source-control actual command")
        require(row["role"] == role and row["argv"] == argv and type(row["exit_code"]) is int and row["exit_code"] == 0
                and row["stdout"] == f"logs/{index:02}.stdout" and row["stderr"] == f"logs/{index:02}.stderr", "Actual command role/argv/normal status changed")
        stdout = common.read(common.relative(base, row["stdout"]), allow_empty=True)
        stderr = common.read(common.relative(base, row["stderr"]), allow_empty=True)
        require(hashlib.sha256(stdout).hexdigest() == row["stdout_sha256"] and hashlib.sha256(stderr).hexdigest() == row["stderr_sha256"],
                "Captured actual command stream digest changed")
        files.update([row["stdout"], row["stderr"]])
        if role.startswith("export-"):
            continue
        require(not stderr, "JSON compile/VM/SDK command has unreviewed stderr")
        actual = load(stdout)
        if type(result) is tuple:
            mode, name, witness = result
            observed = next(run for run in report["runs"] if (run["mode"], run["name"], run["witness"]) == (mode, name, witness))
            require(same(actual, [observed["execution"]]), "Actual VM raw stdout differs from parsed witness")
        else:
            require(same(actual, result), "Actual recompile/SDK raw stdout differs from retained result")
    return files


def validate_source_controls(report, base, *, root=ROOT):
    """Portable recorded reader only; never executes tools or trusts live paths."""
    import boomerang_source_controls as common
    base, root = Path(base).resolve(), Path(root).resolve()
    closed(report, ["schema_version", "protocol", "status", "scope", "context", "prepared", "native", "bundles", "instances", "runs", "ledger", "commands", "files"], "source-control packet")
    require(type(report["schema_version"]) is int and report["schema_version"] == 1 and report["protocol"] == common.PROTOCOL
            and report["status"] == "actual_source_controls_captured" and report["scope"] == common.SCOPE,
            "Source-control protocol/status/scope changed")
    require(tuple(common.NAMES) == CONTROL_NAMES
            and hashlib.sha256(common.read(root / common.RECIPES)).hexdigest() == CONTROL_RECIPES_SHA256,
            "Unreviewed exact source mutation recipes")
    context, source, package, capture, host, _ = _control_context(report, base, root, common)
    witnesses, binary, native_files = _control_native(report, base, root, common, context, source, package)
    artifacts, commands, bundle_files = _control_bundles(report, base, common, context, package, capture, host)
    _control_runs(report, witnesses)
    ledger = _control_ledger(report, base, common, witnesses, artifacts)
    command_files = _control_commands(report, base, common, commands, capture, binary, ledger)
    expected = {"package/" + path for path in common.FILES + ["Cargo.lock"]}
    expected.update(["native.log", "native-receipt.json", "last-host-window.json", "ledger-request.json", "ledger.json"])
    expected.update(native_files | bundle_files | command_files)
    inventory = report["files"]
    require(type(inventory) is list and [row["path"] for row in inventory] == sorted(expected), "Missing/unknown/duplicate retained source-control data inventory")
    for row in inventory:
        closed(row, ["path", "sha256"], "source-control retained file")
        require(common.sha(common.relative(base, row["path"])) == row["sha256"], "Retained source-control file bytes changed")
    return {"status": "recorded_actual_source_controls_checked", "native_test_groups": 1, "native_policy_observations": 9,
            "exports": 5, "compiler_export_modes": 10, "retained_primary_VM_observations": 18,
            "SDK_scenario_runs": 18, "SDK_transaction_attempts": 18, "accepted_policy_counterexamples": 6,
            "exact_typed_rejections": 12,
            "scope": "Recorded current-source/checked-recipe/retained-byte/native/typed-consensus consistency only; no new execution, authenticated build, BLS/Optional/ledger theorem, exhaustive security or compiler-preservation credit."}
