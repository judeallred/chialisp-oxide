"""Validate recorded password-module and standard-curry observations.

This checks finite evidence and current inputs. Only an explicit retained-byte
replay reruns the VM; source freshness does not authenticate a claimed run.
"""
import hashlib
import re

from password_ledger import validate as check_ledger
from password_provenance import (DEPENDENCIES, ROOT, dependencies, digest,
                                 read_source, source_contract)
from password_vectors import (CODEC, MAX_COST, MODES, PUBLIC_HASH, curry_tree,
                              deserialize, expected, generated_for, instance_cases,
                              named, native_environment, parse, proper, relation,
                              require, serialize)
from vm_failures import check_execution, check_native_outcome

CURRY_REVISION = "56f6b676a96de8659fe05d7e4f705df553f37fa5"
CURRY_SOURCE = f"https://github.com/Chia-Network/chia_rs/blob/{CURRY_REVISION}/crates/clvm-utils/src/curried_program.rs"
REFERENCE_HEX = "ff02ffff03ffff09ffff0bff0580ff0280ffff010bffff01ff088080ff0180"
REFERENCE_HASH = "caf9f00763b55cfc71db469957fc5c21a4da350c4d5d36ab0a929de987b8eade"


def modes(rows, label):
    require(isinstance(rows, list) and all(isinstance(row, dict) and type(row.get("optimize_lir")) is bool for row in rows)
            and [(row.get("backend"), row.get("optimize_lir")) for row in rows] == MODES,
            f"{label}: missing, unknown, duplicate or reordered five compiler modes")
    return {(row["backend"], row["optimize_lir"]): row for row in rows}


def artifact_bytes(artifact):
    require(isinstance(artifact, dict) and set(artifact) == {"backend", "optimize_lir", "bytes", "tree_hash", "clvm_hex", "disassembly"}
            and isinstance(artifact["backend"], str) and type(artifact["optimize_lir"]) is bool
            and type(artifact["bytes"]) is int and 1 <= artifact["bytes"] <= 2_000_000,
            "Malformed retained password artifact")
    text = artifact["clvm_hex"]
    require(isinstance(text, str) and re.fullmatch(r"(?:[0-9a-f]{2})+", text), "Malformed serialized artifact hex")
    binary = bytes.fromhex(text)
    require(len(binary) == artifact["bytes"], "Artifact size differs from retained bytes")
    tree = deserialize(binary)
    require(serialize(tree) == binary and CODEC["tree_hash"](tree).hex() == artifact["tree_hash"],
            "Artifact serialization/tree hash mismatch")
    require(isinstance(artifact["disassembly"], str) and artifact["disassembly"], "Missing artifact disassembly")
    return binary


def execution(value, desired, *, budget=MAX_COST, conclusive=True):
    require(type(budget) is int and 1 <= budget <= MAX_COST, "Invalid password VM budget")
    observed = check_execution(value, require_conclusive=conclusive)
    if observed["status"] == "inconclusive":
        require(not conclusive and value.get("failure") == {"kind": "resource", "code": "cost_exceeded"},
                "Only exact cost exhaustion is a declared low-budget diagnostic")
    else:
        require(observed == desired, "Password VM differs from independent exact-byte relation")
        if observed["status"] == "accepted":
            require(value["cost"] <= budget, "Accepted password execution exceeded its declared budget")
    return observed


def cases(actual, required, outcome, *, budget=MAX_COST):
    require(isinstance(actual, list) and len(actual) == len(required), "Missing/extra password cases")
    for case, recipe in zip(actual, required):
        require(set(case) == {"label", "solution", "native", "runs"} and case["label"] == recipe["label"]
                and parse(case["solution"]) == parse(recipe["solution"]), "Changed case label/order/input")
        desired = outcome(recipe["solution"])
        require(check_native_outcome(case["native"]) == desired, "Actual native Cargo result differs from independent relation")
        modes(case["runs"], case["label"])
        for run in case["runs"]:
            require(set(run) == {"backend", "optimize_lir", "execution"}, "Malformed case execution")
            execution(run["execution"], desired, budget=budget)


def resource_cases():
    selected = {row["label"]: row for row in named()}
    return [selected[label] for label in ("large_missing_conditions_match", "large_pair_hash", "large_password_success")]


def resources(puzzle):
    controls = puzzle.get("resource_controls")
    require(isinstance(controls, dict) and set(controls) == {"max_cost", "cases"}
            and type(controls["max_cost"]) is int and controls["max_cost"] == 100_000,
            "Missing/changed password low-budget diagnostic contract")
    actual, recipes = controls["cases"], resource_cases()
    require(isinstance(actual, list) and len(actual) == 3, "Missing/extra diagnostic cases")
    for case, recipe in zip(actual, recipes):
        require(set(case) == {"label", "solution", "native", "runs"}
                and case["label"] == recipe["label"] and case["solution"] == recipe["solution"], "Changed resource input coverage")
        desired = expected(recipe["solution"])
        require(check_native_outcome(case["native"]) == desired, "Resource diagnostic changed native policy output")
        modes(case["runs"], "resource diagnostic")
        for run in case["runs"]:
            require(set(run) == {"backend", "optimize_lir", "execution"}, "Malformed diagnostic execution")
            execution(run["execution"], desired, budget=100_000, conclusive=False)
    return 15


def check_curried_artifact(record, module, argument):
    require(set(record) == {"module_tree_hash", "artifact"}, "Malformed standard-curried artifact binding")
    artifact = record["artifact"]
    require((artifact["backend"], artifact["optimize_lir"]) == (module["backend"], module["optimize_lir"])
            and record["module_tree_hash"] == module["tree_hash"], "Instance uses a different module/mode")
    binary = artifact_bytes(artifact)
    desired = serialize(curry_tree(deserialize(artifact_bytes(module)), argument))
    require(binary == desired, "Instance is not exactly (a (q . sameM) (c (q . H) 1))")
    return artifact


def instances(value, module_rows):
    require(isinstance(value, dict) and set(value) == {"protocol", "constructor", "source_role", "scheme", "source", "revision", "fixed_argument", "artifacts", "cases", "varying_arguments", "ledger"},
            "Missing/unknown dedicated curried-instance evidence")
    require(value["protocol"] == "chialisp-oxide-password-instances/v1"
            and value["constructor"] == "private standard-tree constructor using clvmr 0.21.0; no clvm-utils call"
            and value["source_role"] == "algorithm-reference provenance, not an executed library dependency"
            and value["scheme"] == "(a (q . M) (c (q . H) 1))" and value["source"] == CURRY_SOURCE
            and value["revision"] == CURRY_REVISION and value["fixed_argument"] == serialize(PUBLIC_HASH).hex(),
            "Changed standard curry reference or public fixed commitment")
    rows = value["artifacts"]
    require(isinstance(rows, list) and len(rows) == 5, "Missing/extra fixed-instance artifacts")
    fixed = [check_curried_artifact(row, module, PUBLIC_HASH) for row, module in zip(rows, module_rows)]
    modules = modes(module_rows, "source modules")
    instance_map = modes(fixed, "fixed instances")
    cases(value["cases"], instance_cases(), lambda solution: relation(native_environment(PUBLIC_HASH, parse(solution))))
    controls = value["varying_arguments"]
    require(isinstance(controls, list) and len(controls) == 3, "Missing/extra varying-hash controls")
    for control, password in zip(controls, [b"", b"abc", b"\0\0"]):
        require(set(control) == {"argument_hex", "password_hex", "solution", "environment", "native", "runs"}, "Malformed varying argument record")
        argument = hashlib.sha256(password).digest()
        solution = password, ((b"\x08", b"\0\0"), b"\0")
        environment = native_environment(argument, solution)
        require(control["argument_hex"] == serialize(argument).hex() and control["password_hex"] == password.hex()
                and parse(control["solution"]) == solution and parse(control["environment"]) == environment,
                "Varying-H control changed password, selected tree or H.S binding")
        desired = relation(environment)
        require(check_native_outcome(control["native"]) == desired and desired["status"] == "accepted", "Varying-H control lacks native passthrough")
        runs = control["runs"]
        require(isinstance(runs, list) and len(runs) == 5, "Missing varying-H modes")
        for run, module in zip(runs, module_rows):
            require(set(run) == {"module_tree_hash", "artifact", "execution"}, "Malformed varying-H run")
            check_curried_artifact({key: run[key] for key in ("module_tree_hash", "artifact")}, module, argument)
            execution(run["execution"], desired)
    return {"fixed_instances": 5, "fixed_case_vm_runs": 240, "varying_instances": 15,
            "varying_vm_runs": 15, "ledger": check_ledger(value["ledger"], modules, instance_map)}


def validate(report, reader=read_source):
    manifest = source_contract(reader)
    puzzle_rows = [row for row in report["puzzles"] if row["slug"] == "password-coin"]
    require(len(puzzle_rows) == 1, "Missing/duplicate password fixture")
    puzzle = puzzle_rows[0]
    require(set(puzzle) == {"artifacts", "cases", "environment", "generated_cases", "kind", "native_sources",
        "provenance", "reference", "rue", "security_scope", "simulator", "simulator_comparisons",
        "slug", "source", "source_dependencies", "title", "validated", "curried_instances", "resource_controls"},
        "Unknown or missing password template/instance evidence fields")
    for field in ("title", "kind", "environment", "security_scope", "provenance"):
        require(puzzle[field] == manifest[field], "Stale password claim/source identity: " + field)
    require(puzzle["validated"] is True and puzzle["kind"] == "pure_function"
            and puzzle["simulator"] == [] and puzzle["simulator_comparisons"] == []
            and puzzle.get("reference_includes", {}) == {} and puzzle.get("reference_mode", "module") == "module"
            and "module_identity" not in puzzle and "factory_checks" not in puzzle and "validation_max_cost" not in puzzle,
            "Raw template cannot claim a locked coin, factory or published module artifact")
    require(puzzle["source"] == reader("puzzles/password-coin/puzzle.rs").decode()
            and puzzle["reference"] == reader("puzzles/password-coin/reference.clsp").decode(), "Stale Rust/reference source")
    rue_data = reader("puzzles/password-coin/puzzle.rue")
    require(puzzle["rue"] == {"path": "puzzles/password-coin/puzzle.rue", "source": rue_data.decode(), "sha256": digest(rue_data), "dependencies": []}, "Stale/unknown Rue source")
    dependencies(puzzle, report["authoring"], reader)
    modes(puzzle["artifacts"], "raw artifacts")
    for artifact in puzzle["artifacts"]:
        artifact_bytes(artifact)
    reference = puzzle["artifacts"][2]
    require(reference["clvm_hex"] == REFERENCE_HEX and reference["tree_hash"] == REFERENCE_HASH and reference["bytes"] == 31,
            "Independent exact tutorial source compilation changed from its reviewed31-byte regression")
    cases(puzzle["cases"], named(), expected)
    require(type(puzzle["generated_cases"]) is int and puzzle["generated_cases"] == len(generated_for()), "Changed generated coverage count")
    return {"slug": "password-coin", "raw_named": 48, "raw_generated": 2315,
            "password_specific_generated": 256, "generated_matched_successes": 128,
            "raw_named_vm_runs": 240, "resource_observations": resources(puzzle),
            **instances(puzzle.get("curried_instances"), puzzle["artifacts"])}
