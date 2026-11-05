"""Current compiler evidence first; exact immutable history afterwards.

This recorded comparator does not execute policy, authenticate captures, verify
BLS signatures or prove compiler preservation. Runtime source checks belong to
the caller and are never applied to the returned historical checkpoint.
"""
import copy
import json
import os
from pathlib import Path
import re
import stat

from cargo_authoring import contained_file, unique_object
import p2_append_migration as append
import p2_singleton_evidence as ledger
import p2_source_cost_migration as source_cost
import runtime_cost_migration as cost
from vm_failures import check_execution

ROOT = Path(__file__).resolve().parents[1]
PATH = "corpus/guarded-projection-baseline.json"
CHECKPOINT = "corpus/checkpoints/p2-source-cost-validation.json"
METRICS = "corpus/guarded-projection-costs.json"
CONTROLS = "corpus/guarded-projection-controls.json"
PIN = "66dc316cf447bd123b11bb33c3a4568a7be28f64d31b7947ebedf9ed3e614b54"
PENDING = "pending_signed_checkpoint_and_capture_review"
FINAL = "reviewed_final_guarded_projection"
BEFORE_RAW = "1a19cca4b2f1d840bf49c3119305f8872ff03e4ea91c5f74e93ad57ef4ad8b03"
BEFORE_CANONICAL = "1f8819d8c2ddab219b1277e2550381378c3e7b2a9e5933830b1bbdfe96370943"
SIGNED_BASELINE = {"commit": None, "tree_sha1": None, "report_git_blob_sha1": None}
OLD_PIPELINE = cost.READABLE["REFINEMENT_PIPELINE"]
NEW_PIPELINE = copy.deepcopy(OLD_PIPELINE)
NEW_PIPELINE["rust_mir"]["optimized"].insert(7, "runtime_guarded_projection_forwarding")
IMMUTABLE = {**source_cost.IMMUTABLE, "corpus/p2-source-cost-baseline.json": source_cost.PIN}
require, sha, canonical, same = cost.require, cost.sha, cost.canonical, cost.same


def read(path, *, limit=4 * 1024 * 1024):
    with contained_file(ROOT, path).open("rb") as stream:
        payload = stream.read(limit + 1)
    require(0 < len(payload) <= limit, "Guarded evidence byte bound: " + path)
    return payload


def report_payload(filename):
    """Bound report IO; concurrent input mutation remains a caller assumption."""
    filename, limit = Path(filename), 16 * 1024 * 1024
    descriptor = os.open(filename, os.O_RDONLY | os.O_NONBLOCK)
    try:
        info = os.fstat(descriptor)
        require(stat.S_ISREG(info.st_mode) and 0 < info.st_size <= limit, "Current report regular-file/byte bound")
        with os.fdopen(descriptor, "rb", closefd=False) as stream:
            payload = stream.read(limit + 1)
    finally:
        os.close(descriptor)
    require(0 < len(payload) <= limit, "Current report byte bound")
    return payload


def load(payload=None):
    payload = read(PATH) if payload is None else payload
    require(sha(payload) == PIN, "Guarded reviewed profile bytes changed")
    value = json.loads(payload, object_pairs_hook=unique_object)
    ledger.closed(value, ["schema_version", "protocol", "status", "scope", "baseline",
        "immutable_profiles", "optimization_pipeline", "reviewed_after"], "guarded profile")
    require(type(value["schema_version"]) is int and value["schema_version"] == 1
        and value["protocol"] == "chialisp-oxide-runtime-guarded-projection/v1"
        and value["status"] in (PENDING, FINAL) and type(value["scope"]) is str and value["scope"],
        "Unsupported guarded profile")
    expected = {"repository": "https://github.com/judeallred/chialisp-oxide", **SIGNED_BASELINE,
        "report_path": "corpus/validation.json", "checkpoint_path": CHECKPOINT,
        "report_sha256": BEFORE_RAW, "canonical_report_sha256": BEFORE_CANONICAL}
    require(same(value["baseline"], expected) and value["immutable_profiles"] == IMMUTABLE
        and value["optimization_pipeline"] == {"before": OLD_PIPELINE, "after": NEW_PIPELINE},
        "Changed guarded signed predecessor, profiles or pipeline")
    for path, pin in IMMUTABLE.items():
        require(sha(read(path, limit=16 * 1024 * 1024)) == pin, "Changed immutable profile: " + path)
    old_profile = source_cost.load()  # Its three retained raw files are independently pinned.
    old_capture = old_profile["reviewed_after"]
    require(type(old_capture) is dict and sha(read(source_cost.METRICS_PATH)) == old_capture["metrics_sha256"]
        and sha(read(source_cost.CONTROLS_PATH)) == old_capture["controls_sha256"],
        "Changed immutable source-only metric/control bytes")
    signed = all(type(v) is str and re.fullmatch(r"[0-9a-f]{40}", v) for v in SIGNED_BASELINE.values())
    require(signed or all(v is None for v in SIGNED_BASELINE.values()), "Incomplete signed predecessor metadata")
    if value["status"] == PENDING:
        require(value["reviewed_after"] is None, "Pending guarded profile cannot carry final authorization")
    else:
        require(type(value["reviewed_after"]) is dict and signed, "Missing actual signed guarded predecessor")
    return value


def predecessor(profile=None):
    profile = load() if profile is None else profile
    payload = read(CHECKPOINT, limit=16 * 1024 * 1024)
    require(sha(payload) == profile["baseline"]["report_sha256"] == BEFORE_RAW,
        "Changed complete source-only checkpoint bytes")
    value = json.loads(payload, object_pairs_hook=unique_object)
    require(sha(canonical(value)) == BEFORE_CANONICAL and value["optimization_pipeline"] == OLD_PIPELINE,
        "Changed complete source-only checkpoint object")
    return value


def is_transition(report):
    return report.get("optimization_pipeline") == NEW_PIPELINE


def reviewed_after(report):
    profile = load()
    require(profile["status"] == FINAL, "Guarded profile pending signed checkpoint/capture review")
    after = profile["reviewed_after"]
    ledger.closed(after, ["report_sha256", "canonical_report_sha256", "p2_sha256", "protocol_sha256",
        "raw_ir_sha256", "metrics_sha256", "controls_sha256", "bundles", "delta"], "guarded capture")
    for key in set(after) - {"bundles", "delta"}:
        require(source_cost.digest(after[key]), "Missing guarded capture hash: " + key)
    ledger.closed(after["bundles"], ["on", "off"], "guarded paired capture")
    for mode, name in ((True, "on"), (False, "off")):
        pin = after["bundles"][name]
        ledger.closed(pin, ["manifest_sha256", "optimize_lir"], "guarded mode")
        require(source_cost.digest(pin["manifest_sha256"]) and pin["optimize_lir"] is mode,
            "Malformed guarded capture mode/hash")
    ledger.closed(after["delta"], ["leaf_count", "inventory_sha256"], "guarded delta")
    require(type(after["delta"]["leaf_count"]) is int and 0 < after["delta"]["leaf_count"] <= 100_000
        and source_cost.digest(after["delta"]["inventory_sha256"]) and is_transition(report)
        and sha(canonical(report)) == after["canonical_report_sha256"], "Unreviewed guarded current report")
    p2 = report["puzzles"][-1]
    require(p2["slug"] == append.ADDED and sha(canonical(p2)) == after["p2_sha256"]
        and sha(canonical(p2["protocol_validation"])) == after["protocol_sha256"], "Changed guarded P2 capture")
    return profile, after


def expected_patches(report):
    require(is_transition(report), "Changed guarded optimizer pipeline")
    return source_cost.append.refinement.PATCHES


def _equal(before, current, label):
    require(same(before, current), "Guarded changed " + label)


def _execution(before, current):
    check_execution(before, require_conclusive=False)
    check_execution(current, require_conclusive=False)
    _equal({k: v for k, v in before.items() if k != "cost"},
        {k: v for k, v in current.items() if k != "cost"}, "typed result/failure/diagnostic")
    if before["cost"] is None:
        require(current["cost"] is None, "Changed failed execution cost contract")
    else:
        require(type(current["cost"]) is int and 0 <= current["cost"] <= before["cost"],
            "Measured guarded VM cost regression")


def _artifact(before, current):
    keys = ["backend", "optimize_lir", "bytes", "clvm_hex", "disassembly", "tree_hash"]
    ledger.closed(before, keys, "old artifact")
    ledger.closed(current, keys, "current artifact")
    _equal([before["backend"], before["optimize_lir"]], [current["backend"], current["optimize_lir"]], "artifact mode")
    if current["backend"] != "rust_mir" or current["optimize_lir"] is not True:
        return _equal(before, current, "OFF/reference/Rue artifact")
    cost.artifact_bytes(before)
    cost.artifact_bytes(current)
    require(current["bytes"] <= before["bytes"], "Measured guarded module size regression")


def _runs(before, current):
    require(type(current) is list and len(before) == len(current), "Changed compiler run coverage")
    for old, new in zip(before, current, strict=True):
        ledger.closed(new, old, "compiler run")
        artifact = old.get("artifact")
        mode = (artifact["backend"], artifact["optimize_lir"]) if artifact else (old["backend"], old["optimize_lir"])
        selected = mode == ("rust_mir", True)
        for key in old:
            if key == "artifact": _artifact(old[key], new[key])
            elif key == "execution" and selected: _execution(old[key], new[key])
            elif key != "module_tree_hash": _equal(old[key], new[key], "run metadata/mode")


def _cases(before, current):
    require(type(current) is list and len(before) == len(current), "Changed named/resource coverage")
    for old, new in zip(before, current, strict=True):
        ledger.closed(new, old, "comparison case")
        _equal({k: v for k, v in old.items() if k != "runs"},
            {k: v for k, v in new.items() if k != "runs"}, "input/native/coverage/metadata")
        _runs(old["runs"], new["runs"])


def _instance_ledger_bindings(value, instances, module_hash):
    artifact = cost.rust_on([row["artifact"] for row in instances["artifacts"]])
    require(value["module_tree_hash"] == module_hash
        and value["instance_tree_hash"] == artifact["tree_hash"], "Changed instance ledger module/artifact binding")
    module, _ = ledger.uncurry_one(ledger.tree(artifact["clvm_hex"]))
    require(ledger.CODEC["tree_hash"](module).hex() == module_hash, "Changed curried embedded module")
    for attempt in value["attempts"]:
        # Each actual argument determines its reveal, including the intentional
        # wrong-commitment negative; every input coin still commits the fixed instance.
        reveal = ledger.curry(module, [ledger.tree(attempt["argument_hex"])])
        require(attempt["coin_puzzle_hash"] == artifact["tree_hash"]
            and attempt["reveal_hex"] == ledger.CODEC["serialize"](reveal).hex()
            and attempt["reveal_tree_hash"] == ledger.CODEC["tree_hash"](reveal).hex(),
            "Changed instance ledger coin/reveal derivation")


def _curried(before, current, old_module, new_module):
    ledger.closed(current, before, "curried instances")
    for key in before:
        old, new = before[key], current[key]
        if key == "cases": _cases(old, new)
        elif key == "artifacts":
            require(len(old) == len(new), "Changed curried artifact modes")
            for a, b in zip(old, new, strict=True):
                ledger.closed(b, a, "curried artifact")
                _artifact(a["artifact"], b["artifact"])
                if a["artifact"]["backend"] == "rust_mir" and a["artifact"]["optimize_lir"] is True:
                    require(a["module_tree_hash"] == old_module and b["module_tree_hash"] == new_module,
                        "Changed curried module binding")
                else: _equal(a, b, "nonoptimized curried artifact")
        elif key == "varying_arguments":
            require(len(old) == len(new), "Changed varying-argument coverage")
            for a, b in zip(old, new, strict=True):
                _cases([a], [b])
                for x, y in zip(a["runs"], b["runs"], strict=True):
                    require(x["module_tree_hash"] == old_module and y["module_tree_hash"] == new_module
                        if x["artifact"]["backend"] == "rust_mir" and x["artifact"]["optimize_lir"] is True
                        else x["module_tree_hash"] == y["module_tree_hash"], "Changed varying module binding")
        elif key == "ledger":
            require(len(old) == len(new), "Changed instance ledger coverage")
            for a, b in zip(old, new, strict=True):
                if a["backend"] == "rust_mir" and a["optimize_lir"] is True:
                    _instance_ledger_bindings(a, before, old_module)
                    _instance_ledger_bindings(b, current, new_module)
                    cost.check_ledger_transition(b, a)
                    for x, y in zip(a["attempts"], b["attempts"], strict=True): _execution(x["vm_021"], y["vm_021"])
                else: _equal(a, b, "nonoptimized instance ledger")
        else: _equal(old, new, "curried metadata")


def _pairs(before, current):
    stack, work = [((), before, current)], 0
    while stack:
        path, old, new = stack.pop()
        work += 1
        require(work <= 500_000 and len(path) <= 128 and type(old) is type(new), "Changed/bounded evidence shape")
        if type(old) is dict:
            require(set(old) == set(new), "Changed evidence keys")
            yield path, old, new
            stack.extend((path + (k,), old[k], new[k]) for k in old)
        elif type(old) is list:
            require(len(old) == len(new), "Changed evidence coverage/order")
            stack.extend((path + (str(i),), a, b) for i, (a, b) in enumerate(zip(old, new, strict=True)))
        else: yield path, old, new


def _atom_map(tree, mapping):
    stack, output, work = [(tree, False)], [], 0
    while stack:
        value, done = stack.pop()
        work += 1
        require(work <= 200_000, "Protocol tree substitution work bound")
        if type(value) is bytes: output.append(mapping.get(value, value))
        elif done:
            right, left = output.pop(), output.pop()
            output.append((left, right))
        else: stack.extend([(value, True), (value[1], False), (value[0], False)])
    require(len(output) == 1, "Malformed protocol substitution")
    return output[0]


def _protocol(before, current, old_artifact, new_artifact):
    _equal({k: v for k, v in before.items() if k != "observations"},
        {k: v for k, v in current.items() if k != "observations"}, "protocol header")
    old_rows, rows = before["observations"], current["observations"]
    require(len(old_rows) == len(rows) == 5, "Changed dedicated protocol modes")
    old, new = old_rows[1], rows[1]
    for group, artifact in ((old, old_artifact), (new, new_artifact)):
        require(group["module_tree_hash"] == artifact["tree_hash"]
            and group["curried_p2_hash"] == ledger.CODEC["tree_hash"](ledger.tree(group["curried_p2_reveal_hex"])).hex(),
            "Changed protocol module/curried reveal binding")
    mapping = {}
    def add(a, b):
        a, b = bytes.fromhex(a), bytes.fromhex(b)
        require(len(a) == len(b) == 32 and mapping.get(a, b) == b, "Conflicting protocol identity substitution")
        mapping[a] = b
    add(old["module_tree_hash"], new["module_tree_hash"])
    add(old["curried_p2_hash"], new["curried_p2_hash"])
    add(old["lifecycle"]["p2"]["id"], new["lifecycle"]["p2"]["id"])
    # The auxiliary coin is deliberately not submitted. Bind its two uses in
    # the named wrong-ID request; no absent parent/coin derivation is claimed.
    for group in (old, new):
        require(group["negatives"][4]["name"] == "another p2 coin ID cannot identify this input", "Changed auxiliary-issuer recipe")
    auxiliary = [ledger.tree(g["negatives"][4]["transaction"]["spends"][1]["solution_hex"])[1][0] for g in (old, new)]
    require(all(type(v) is bytes and len(v) == 32 for v in auxiliary)
        and all(v.hex() != g["lifecycle"]["p2"]["id"] for v, g in zip(auxiliary, (old, new), strict=True)), "Malformed auxiliary coin identity")
    add(*(v.hex() for v in auxiliary))
    for a, b in list(mapping.items()):
        for message in (b"$", b"x"): add(sha(a + message), sha(b + message))
    old_code, new_code = bytes.fromhex(old_artifact["clvm_hex"]), bytes.fromhex(new_artifact["clvm_hex"])
    def reveal(value):
        raw = ledger.hex_bytes(value)
        if old_code in raw:
            require(raw.count(old_code) == 1, "Protocol reveal module must occur once")
            raw = raw.replace(old_code, new_code, 1)
        return ledger.CODEC["serialize"](_atom_map(ledger.CODEC["deserialize"](raw), mapping)).hex()
    pairs = list(_pairs(old, new))
    for path, a, b in pairs:
        if path and path[-1].endswith("reveal_hex"):
            expected = reveal(a)
            require(expected == b, "Changed protocol reveal beyond module/identity substitution")
            add(ledger.CODEC["tree_hash"](ledger.tree(a)).hex(), ledger.CODEC["tree_hash"](ledger.tree(b)).hex())
    for _, a, b in pairs:
        if type(a) is dict and set(a) == {"amount", "id", "parent_id", "puzzle_hash"}:
            ledger.coin(a); ledger.coin(b)
            require(a["amount"] == b["amount"] and a["parent_id"] == b["parent_id"]
                and mapping.get(bytes.fromhex(a["puzzle_hash"]), bytes.fromhex(a["puzzle_hash"])).hex() == b["puzzle_hash"], "Changed protocol coin amount/parent/recipient")
            add(a["id"], b["id"])
    for a, b in list(mapping.items()):
        for message in (b"$", b"x"): add(sha(a + message), sha(b + message))
    for path, a, b in pairs:
        if type(a) in (dict, list) or a == b: continue
        key = path[-1]
        if key.endswith("reveal_hex"): require(reveal(a) == b, "Changed protocol reveal")
        elif key == "solution_hex": require(ledger.CODEC["serialize"](_atom_map(ledger.tree(a), mapping)).hex() == b, "Changed protocol solution beyond identity substitution")
        elif key == "signature_hex":
            ledger.hex_bytes(a, size=96); ledger.hex_bytes(b, size=96)
            require(a != "c0" + "00" * 95 and b != "c0" + "00" * 95, "Changed empty signature recipe")
        elif type(a) is str and len(a) == 64 and re.fullmatch(r"[0-9a-f]{64}", a):
            require(mapping.get(bytes.fromhex(a), bytes.fromhex(a)).hex() == b, "Changed unmapped protocol identity")
        else: _equal(a, b, "protocol policy/receipts/rollback/metadata")
    for index, (a, b) in enumerate(zip(old_rows, rows, strict=True)):
        if index == 1: continue
        if index == 2:
            a, b = copy.deepcopy(a), copy.deepcopy(b)
            left, right = a["negatives"][16], b["negatives"][16]
            require(left["name"] == right["name"] == "different-language reveal submitted to consensus"
                and left["transaction"]["spends"][1]["reveal_hex"] == old["curried_p2_reveal_hex"]
                and right["transaction"]["spends"][1]["reveal_hex"] == new["curried_p2_reveal_hex"], "Unbound cross-language reveal")
            right["transaction"]["spends"][1]["reveal_hex"] = left["transaction"]["spends"][1]["reveal_hex"]
        _equal(a, b, "OFF/nonRust protocol")


def differences(before, current):
    stack, result = [("", before, current)], []
    while stack:
        path, old, new = stack.pop()
        require(len(stack) + len(result) <= 500_000 and path.count("/") <= 128, "Delta work/depth bound")
        if type(old) is not type(new): result.append({"path": path, "kind": "type"})
        elif type(old) is dict:
            require(set(old) == set(new), "Changed report metadata keys")
            stack.extend((path + "/" + k, old[k], new[k]) for k in old)
        elif type(old) is list:
            if len(old) != len(new): result.append({"path": path, "kind": "length", "before": len(old), "current": len(new)})
            else: stack.extend((path + "/" + str(i), a, b) for i, (a, b) in enumerate(zip(old, new, strict=True)))
        elif old != new:
            row = {"path": path, "kind": "value"}
            if type(old) is str and max(len(old), len(new)) > 160:
                row.update(before_sha256=sha(old.encode()), current_sha256=sha(new.encode()), before_chars=len(old), current_chars=len(new))
            else: row.update(before=old, current=new)
            result.append(row)
    return sorted(result, key=lambda row: row["path"])


def compare_compiler_delta(before, current):
    require(before["optimization_pipeline"] == OLD_PIPELINE and is_transition(current), "Changed guarded stage placement")
    _equal({k: v for k, v in before.items() if k not in ("puzzles", "optimization_pipeline")},
        {k: v for k, v in current.items() if k not in ("puzzles", "optimization_pipeline")}, "report header/authoring")
    require(len(before["puzzles"]) == len(current["puzzles"]) == 22, "Changed complete22 coverage")
    for old, new in zip(before["puzzles"], current["puzzles"], strict=True):
        ledger.closed(new, old, "puzzle")
        _equal({k: v for k, v in old.items() if k not in ("artifacts", "cases", "resource_controls", "curried_instances", "protocol_validation")},
            {k: v for k, v in new.items() if k not in ("artifacts", "cases", "resource_controls", "curried_instances", "protocol_validation")}, "puzzle source/ABI/native/provenance/coverage/metadata")
        require(len(old["artifacts"]) == len(new["artifacts"]) == 5, "Changed five artifacts")
        for a, b in zip(old["artifacts"], new["artifacts"], strict=True): _artifact(a, b)
        _cases(old["cases"], new["cases"])
        if "resource_controls" in old:
            ledger.closed(new["resource_controls"], old["resource_controls"], "resource controls")
            _equal(old["resource_controls"]["max_cost"], new["resource_controls"]["max_cost"], "diagnostic budget")
            _cases(old["resource_controls"]["cases"], new["resource_controls"]["cases"])
        if "curried_instances" in old: _curried(old["curried_instances"], new["curried_instances"], old["artifacts"][1]["tree_hash"], new["artifacts"][1]["tree_hash"])
        if "protocol_validation" in old: _protocol(old["protocol_validation"], new["protocol_validation"], old["artifacts"][1], new["artifacts"][1])
    delta = differences(before, current)
    return {"leaf_count": len(delta), "inventory_sha256": sha((json.dumps(delta, indent=2) + "\n").encode())}


def project_validated(report):
    """PURE exact source-only checkpoint; validate_current is prerequisite."""
    profile, after = reviewed_after(report)
    previous = predecessor(profile)
    require(compare_compiler_delta(previous, report) == after["delta"], "Unreviewed typed compiler delta")
    return previous


def check_report_bytes(payload, report):
    require(sha(payload) == reviewed_after(report)[1]["report_sha256"], "Changed current guarded report bytes")


def hash_diagnostic_profile(report):
    reviewed_after(report)
    for puzzle in report["puzzles"]:
        if puzzle["slug"] in ("sha256tree", "singleton-launcher"): cost.artifact_bytes(puzzle["artifacts"][1])
    return True


def verify_fresh_bundles(report, on, off, **identities):
    _, after = reviewed_after(report)
    expected = {row["entry"]: row["sha256"] for row in append.load()["raw_ir_before"]}
    expected[source_cost.ENTRY] = source_cost.load()["reviewed_after"]["raw_ir_sha256"]
    require(after["raw_ir_sha256"] == expected[source_cost.ENTRY], "Guarded changed source IR")
    return append.check_fresh_bundles(report, on, off, expected_ir=expected, bundle_pins=after["bundles"], **identities)


def check_sidecars(report):
    _, after = reviewed_after(report)
    metrics, controls = read(METRICS), read(CONTROLS)
    require(sha(metrics) == after["metrics_sha256"] and sha(controls) == after["controls_sha256"], "Changed guarded current sidecars")
    counts = source_cost.validate_controls(report, json.loads(controls, object_pairs_hook=unique_object))
    values = json.loads(metrics, object_pairs_hook=unique_object)
    measured = source_cost.validate_metrics(report, values)
    previous = predecessor()
    old_metrics = json.loads(read(source_cost.METRICS_PATH), object_pairs_hook=unique_object)
    source_cost.validate_metrics(previous, old_metrics)  # Pure historical bundle arithmetic only.
    compare_metrics(previous, report, old_metrics, values)
    return {**counts, **measured}


def compare_metrics(before, current, old_metrics, metrics):
    """Only the measured joint improvement and two derived genesis encodings."""
    _equal({k: v for k, v in old_metrics.items() if k != "observations"},
        {k: v for k, v in metrics.items() if k != "observations"}, "conditions API/header/scope")
    old_rows = before["puzzles"][-1]["protocol_validation"]["observations"]
    rows = current["puzzles"][-1]["protocol_validation"]["observations"]
    require(len(old_metrics["observations"]) == len(metrics["observations"]) == 5, "Changed fifteen metric coverage")
    for index, (old, new) in enumerate(zip(old_metrics["observations"], metrics["observations"], strict=True)):
        ledger.closed(new, old, "metric build")
        if index != 1:
            _equal(old, new, "OFF/nonRust metrics")
            continue
        _equal([old["backend"], old["optimize_lir"]], [new["backend"], new["optimize_lir"]], "metric mode")
        for phase in ("genesis", "joint", "lineage"):
            a, b = old[phase], new[phase]
            ledger.closed(b, a, "metric phase")
            if phase == "lineage":
                _equal(a, b, "lineage metrics")
                continue
            if phase == "genesis":
                hashes = [bytes.fromhex(group[index]["curried_p2_hash"]) for group in (old_rows, rows)]
                for field in a:
                    if field not in ("generator_uncompressed_hex", "generator_backrefs_hex"):
                        _equal(a[field], b[field], "genesis numeric/other metric")
                        continue
                    left, right = ledger.hex_bytes(a[field]), ledger.hex_bytes(b[field])
                    require(hashes[0] != hashes[1] and left.count(hashes[0]) == right.count(hashes[1]) == 1
                        and right.count(hashes[0]) == left.count(hashes[1]) == 0
                        and left.replace(hashes[0], hashes[1], 1) == right,
                        "Genesis generator must change exactly one protocol-derived recipient")
                    if field == "generator_uncompressed_hex":
                        for binary in (left, right):
                            require(ledger.CODEC["serialize"](ledger.CODEC["deserialize"](binary)) == binary,
                                "Noncanonical uncompressed genesis generator")
                continue
            decreasing = {"execution_cost", "total_conditions_cost", "charged_generator_bytes_pinned_source_rule",
                "generator_bytes_calculated", "generator_uncompressed_bytes", "generator_backrefs_bytes",
                "observed_byte_cost_component", "total_reveal_bytes"}
            for field in a:
                if field in decreasing:
                    require(type(b[field]) is int and 0 <= b[field] <= a[field], "Measured joint cost/size regression")
                elif field not in ("generator_uncompressed_hex", "generator_backrefs_hex", "per_spend"):
                    _equal(a[field], b[field], "joint condition/height/rule/scope metric")
            inputs = dict(zip((s["coin"]["id"] for s in old_rows[index]["lifecycle"][phase]["spends"]),
                (s["coin"]["id"] for s in rows[index]["lifecycle"][phase]["spends"]), strict=True))
            current_spends = {s["coin_id"]: s for s in b["per_spend"]}
            require(len(a["per_spend"]) == len(current_spends) == len(inputs), "Changed per-spend metric coverage")
            for spend in a["per_spend"]:
                new_spend = current_spends[inputs[spend["coin_id"]]]
                _equal(spend["condition_cost"], new_spend["condition_cost"], "per-spend condition charge")
                require(type(new_spend["execution_cost"]) is int and 0 <= new_spend["execution_cost"] <= spend["execution_cost"],
                    "Measured per-spend execution regression")


def checkpoint_recorded():
    """Pinned historical bytes only; never current compiler/source validation."""
    profile = load()
    require(profile["status"] == FINAL, "Guarded/source-only signed checkpoint pending review")
    previous = predecessor(profile)
    _, after = source_cost.reviewed_after(previous)
    require(sha(read(source_cost.METRICS_PATH)) == after["metrics_sha256"]
        and sha(read(source_cost.CONTROLS_PATH)) == after["controls_sha256"], "Changed historical source-cost sidecars")
    source_cost.project_validated(previous)
    return previous
