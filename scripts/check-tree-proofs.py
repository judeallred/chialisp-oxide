#!/usr/bin/env python3
"""Prove source-derived p2_conditions structural IR and replay real witnesses.

Default executes and publishes deterministic proof artifacts/report. --check
checks current declared inputs and recorded claims without running Rust/Z3;
--self-test rejects stale evidence, false results and unrecognized mutations.
No copied Rust policy or generated source wrapper is compiled.
"""

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import tomllib
import unittest

from cargo_authoring import contained_file, load_json, snapshot
from vm_failures import PROTOCOL, check_native_outcome, check_replay, execution_sources


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "corpus/tree-proofs.json"
DIRECTORY = "proofs/p2-conditions"
SPECIFICATION = f"{DIRECTORY}/specification.json"
SOURCE = "puzzles/p2-conditions/puzzle.rs"
ENTRY = "chialisp_oxide_puzzles::p2_conditions::puzzle"
UPSTREAM = "cfe5ced68af8b5e9d5e2cc66071bbe6497f9eda8"
CANONICAL = f"corpus/upstream/chia-puzzles/{UPSTREAM}"
REFERENCE = f"{CANONICAL}/puzzles/p2_conditions.clsp"
PUBLISHED = f"{REFERENCE}.hex"
REFERENCE_SHA = "ef451b050debe66fb0be3d79afdcf7c7a15bea45db8ffd04c0d4e7f2b62f396f"
CLVM_HEX = "ff04ffff0101ff0280"
TREE_HASH = "1c77d7d5efde60a7a1d2d27db6d746bc8e568aea1ef8586ca967a0d60b83cc36"
NORMALIZATION = "Only absolute repository prefixes inside Reject.message diagnostics become <repo>; all other IR fields are unchanged."
SCOPE = {
    "kind": "structural_ir_model",
    "domain": "All finite atom/pair trees with arbitrary finite byte-sequence atoms; no numeric canonicalization or tail restriction.",
    "excludes": ["rustc extraction preservation", "SMT-translator correctness proof", "CLVM lowering/serialization preservation", "Rue optimization preservation", "VM cost/resource equivalence", "module identity inferred from the model proof", "consensus/authorization/security proof"],
}
BINARIES = {name: ROOT / f"target/debug/{name}" for name in [
    "cargo-chialisp-oxide", "chialisp-oxide-driver", "chialisp-oxide", "chialisp-oxide-verifier", "chialisp-oxide-tree-reference",
]}
ARTIFACTS = {"program.ir.json", "accept-atom.ir.json", "wrong-quote.ir.json", "atom-domain.smt2",
             "pair-domain.smt2", "atom-case.smt2", "pair-case.smt2", "accept-atom.smt2", "wrong-quote.smt2",
             "optimized.ir.json", "optimized-atom-domain.smt2", "optimized-pair-domain.smt2",
             "optimized-atom-case.smt2", "optimized-pair-case.smt2", "equivalence-atom-domain.smt2",
             "equivalence-pair-domain.smt2", "equivalence-atom-case.smt2", "equivalence-pair-case.smt2",
             "unsafe-guard-before.ir.json", "unsafe-guard-after.ir.json", "unsafe-guard.smt2"}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def run(args, expected=0, timeout=180):
    result = subprocess.run([str(arg) for arg in args], cwd=ROOT, capture_output=True,
                            text=True, timeout=timeout, check=False,
                            env={**os.environ, "CHIALISP_OXIDE_DRIVER": str(BINARIES["chialisp-oxide-driver"])})
    require(result.returncode == expected,
            f"Command failed ({result.returncode}, expected {expected}): {args}\n{result.stdout}\n{result.stderr}")
    return result.stdout


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def authoring():
    value = snapshot(ROOT)
    require(value["exports"].get("p2-conditions") == {"entry": ENTRY, "source": SOURCE}, "Canonical raw-tree Cargo export identity changed")
    return {**value, "entry": ENTRY}


def implementation_sources():
    paths = {"scripts/check-tree-proofs.py", "scripts/cargo_authoring.py", SPECIFICATION,
             "Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml",
             "puzzles/p2-conditions/puzzle.json", "puzzles/p2-conditions/reference.clsp", REFERENCE,
             PUBLISHED, f"{CANONICAL}/LICENSE"}
    paths.update(row["path"] for row in execution_sources())
    for package in ["crates/chialisp-oxide-driver", "crates/chialisp-oxide-ir", "crates/chialisp-oxide-verifier",
                    "crates/chialisp-oxide-reference", "crates/chialisp-oxide-corpus", "crates/chialisp-oxide-cli",
                    "crates/cargo-chialisp-oxide", "experiments/backend", "vendor/rue-lir"]:
        paths.add(f"{package}/Cargo.toml")
        if (ROOT / package / "build.rs").is_file():
            paths.add(f"{package}/build.rs")
        paths.update(str(path.relative_to(ROOT)) for path in (ROOT / package / "src").rglob("*.rs"))
    return [{"path": path, "sha256": sha(contained_file(ROOT, path))} for path in sorted(paths)]


def specification():
    value = load_json(ROOT / SPECIFICATION)
    require(value == {"acceptance": "pair", "output": {"term": "pair", "first": {"term": "atom", "hex": "01"},
                    "rest": {"term": "first", "value": {"term": "input"}}}}, "Independently authored canonical structural specification changed")
    return value


def canonical_source():
    payload = (ROOT / REFERENCE).read_bytes()
    require(sha(ROOT / REFERENCE) == REFERENCE_SHA and payload == (ROOT / "puzzles/p2-conditions/reference.clsp").read_bytes(),
            "Canonical source changed or a wrapper/adaptation was introduced")
    require((ROOT / PUBLISHED).read_text().strip() == CLVM_HEX, "Published canonical module bytes changed")
    manifest = load_json(ROOT / "puzzles/p2-conditions/puzzle.json")
    require(manifest["slug"] == "p2-conditions" and manifest["environment"] == "raw" and manifest["kind"] == "program_factory",
            "Canonical factory/raw-environment contract changed")
    provenance = manifest["provenance"]
    require(provenance["classification"] == "canonical_puzzle_port"
            and provenance["upstream"]["revision"] == UPSTREAM
            and provenance["upstream"]["source_sha256"] == REFERENCE_SHA
            and provenance["source_adaptation"]["kind"] == "none"
            and provenance["upstream_artifact"]["clvm_hex"] == CLVM_HEX
            and provenance["upstream_artifact"]["tree_hash"] == TREE_HASH,
            "Canonical module/source provenance changed")


def normalized_ir(original):
    normalized = copy.deepcopy(original)
    def walk(value):
        if isinstance(value, dict):
            if value.get("op") == "reject":
                require(isinstance(value.get("message"), str), "Missing Reject diagnostic")
                value["message"] = value["message"].replace(str(ROOT), "<repo>")
            for nested in value.values():
                walk(nested)
        elif isinstance(value, list):
            for nested in value:
                walk(nested)
    walk(normalized)
    def compare(before, after):
        require(type(before) is type(after), "IR normalization changed a field type")
        if isinstance(before, dict):
            require(before.keys() == after.keys(), "IR normalization changed object keys")
            for key in before:
                if before.get("op") == "reject" and key == "message":
                    require(after[key] == before[key].replace(str(ROOT), "<repo>"), "Unrecognized diagnostic normalization")
                else:
                    compare(before[key], after[key])
        elif isinstance(before, list):
            require(len(before) == len(after), "IR normalization changed item count")
            for a, b in zip(before, after):
                compare(a, b)
        else:
            require(before == after, "IR normalization changed a non-diagnostic value")
    compare(original, normalized)
    return normalized


def mutants(program):
    require(set(program) == {"ir_version", "environment", "program"}
            and type(program["ir_version"]) is int and program["ir_version"] == 3
            and program["environment"] == "raw", "Expected the version3 explicit raw-environment IR envelope")
    inner = program["program"]
    require(inner["name"] == ENTRY and len(inner["parameters"]) == 1
            and inner["parameters"][0]["ty"] == {"kind": "tree"} and inner["result"] == {"kind": "tree"}, "Raw-tree IR signature/entry changed")
    body = inner["body"]
    shape = {"op": "tree_is_pair", "value": {"op": "arg", "index": 0}}
    require(body.get("op") == "if" and body.get("condition") == {"op": "not", "value": shape}
            and body.get("then_value", {}).get("op") == "reject", "Unrecognized source-derived atom rejection; refusing to guess a mutation")
    pair_case = body["else_value"]
    expected = {"op": "tree_pair", "first": {"op": "tree_atom", "value": {"op": "atom", "hex": "01"}},
                "rest": {"op": "tree_first", "value": {"op": "arg", "index": 0}}}
    require(pair_case.get("op") == "if" and pair_case.get("condition") == shape
            and pair_case.get("then_value") == expected and pair_case.get("else_value", {}).get("op") == "reject",
            "Unrecognized source-derived quote construction; refusing to mutate unrelated IR")
    guard = copy.deepcopy(program)
    guard["program"]["body"]["then_value"] = {"op": "tree_atom", "value": {"op": "atom", "hex": ""}}
    construction = copy.deepcopy(program)
    construction["program"]["body"]["else_value"]["then_value"]["first"]["value"]["hex"] = "02"
    return {"accept-atom": guard, "wrong-quote": construction}


def load_cargo(output, current):
    pointer = load_json(contained_file(output, "latest.json"))
    bundle = pointer["bundle"]
    require(len(Path(bundle).parts) == 1 and pointer["manifest"] == f"{bundle}/manifest.json", "Invalid atomic Cargo bundle pointer")
    path = contained_file(output, pointer["manifest"])
    manifest = load_json(path)
    require(manifest["schema_version"] == 1 and manifest["package"] == current["package"]
            and Path(manifest["manifest_path"]).resolve() == (ROOT / current["manifest"]).resolve(), "Wrong Cargo extraction manifest")
    names = [entry["entry"] for entry in manifest["entries"]]
    require(len(names) == len(set(names)) and set(names) == {entry["entry"] for entry in current["exports"].values()}, "Cargo export set differs from declared library")
    entry = next(entry for entry in manifest["entries"] if entry["entry"] == ENTRY)
    return normalized_ir(load_json(contained_file(path.parent, entry["ir"])))


def tree(value, depth=0, budget=None):
    budget = [20_000, 1_048_576] if budget is None else budget
    require(isinstance(value, dict) and depth <= 128 and budget[0] > 0, "Invalid/oversized witness tree")
    budget[0] -= 1
    if value.get("kind") == "atom":
        require(set(value) == {"kind", "hex"} and isinstance(value["hex"], str)
                and re.fullmatch(r"(?:[0-9a-f]{2})*", value["hex"]), "Invalid byte atom witness")
        payload = bytes.fromhex(value["hex"])
        budget[1] -= len(payload)
        require(budget[1] >= 0, "Witness total byte budget exceeded")
        return payload
    require(set(value) == {"kind", "first", "rest"} and value["kind"] == "pair", "Invalid pair witness")
    return (tree(value["first"], depth + 1, budget), tree(value["rest"], depth + 1, budget))


def wire(value):
    """Independent serialization for expected raw-tree outputs, not compiler code."""
    if isinstance(value, tuple):
        return b"\xff" + wire(value[0]) + wire(value[1])
    size = len(value)
    if size == 0:
        return b"\x80"
    if size == 1 and value[0] < 128:
        return value
    if size < 64:
        return bytes([128 + size]) + value
    if size < 8192:
        return bytes([192 + (size >> 8), size & 255]) + value
    require(size < 1_048_576, "Expected-result atom exceeds proof replay bound")
    return bytes([224 + (size >> 16), (size >> 8) & 255, size & 255]) + value


def solution(value):
    if isinstance(value, bytes):
        return f"0x{value.hex()}" if value else "()"
    return f"({solution(value[0])} . {solution(value[1])})"


def expected(witness, prefix=b"\x01"):
    value = tree(witness["environment"])
    return {"status": "rejected"} if isinstance(value, bytes) else {"status": "accepted", "output_hex": wire((prefix, value[0])).hex()}


def outcome(value):
    return check_replay(value, "structural VM witness")


def check_replays(rows, desired):
    require(len(rows) == 2 and all(type(row["optimize"]) is bool for row in rows)
            and {row["optimize"] for row in rows} == {False, True}, "Missing/invalid tree replay optimization mode")
    require(all(outcome(row["outcome"]) == desired for row in rows), "Tree VM witness outcome differs from independently specified result")


def replay(program_path, witness, temp, name):
    path = temp / f"{name}.witness.json"
    write_json(path, witness)
    native = json.loads(run([BINARIES["chialisp-oxide"], "native", "p2-conditions", "--solution", solution(tree(witness["environment"]))]))
    rust = json.loads(run([BINARIES["chialisp-oxide-verifier"], program_path, "--tree-replay", path]))
    reference = json.loads(run([BINARIES["chialisp-oxide-tree-reference"], ROOT / REFERENCE, path]))
    require(reference["compiler"] == "chialisp 0.4.6" and reference["vm"] == "clvmr 0.21.0 MEMPOOL_MODE"
            and reference["bytes"] == 9 and reference["clvm_hex"] == CLVM_HEX and reference["tree_hash"] == TREE_HASH,
            "Independent canonical compilation differs from published module; refusing to change its identity")
    require(len(reference["runs"]) == 1 and reference["runs"][0]["witness"] == witness, "Wrong canonical reference witness")
    desired = expected(witness)
    require(check_native_outcome(native) == desired and outcome(reference["runs"][0]["outcome"]) == desired,
            "Native/independently compiled original differs from the structural specification")
    check_replays(rust, desired)
    return {"native_original": native, "rust_original": rust, "canonical_reference": reference["runs"][0]["outcome"]}, reference


def optimized_shape(program):
    """Independent expected p2 rewrite; this does not implement the optimizer."""
    mutants(program)  # Check the complete source-derived guard/construction shape.
    result = copy.deepcopy(program)
    result["program"]["body"] = copy.deepcopy(program["program"]["body"]["else_value"]["then_value"])
    return result


def unsafe_guard_control(program):
    """Synthetic negative control, explicitly separate from the Cargo puzzle."""
    before = copy.deepcopy(program)
    before["program"]["name"] = "synthetic_unsafe_guard_drop"
    kept = {"op": "tree_atom", "value": {"op": "atom", "hex": "aa"}}
    before["program"]["body"] = {
        "op": "if", "condition": {"op": "tree_is_pair", "value": {"op": "arg", "index": 0}},
        "then_value": kept,
        "else_value": {"op": "reject", "message": "Synthetic guard rejects atom inputs"},
    }
    after = copy.deepcopy(before)
    after["program"]["body"] = copy.deepcopy(kept)
    return before, after


def query_files(stage, queries, prefix):
    result = {}
    for shape in ["atom", "pair"]:
        names = {"domain": f"{prefix}{shape}-domain.smt2", "counterexample": f"{prefix}{shape}-case.smt2"}
        for kind, name in names.items():
            key = f"{shape}_nonvacuity" if kind == "domain" else f"{shape}_counterexample"
            (stage / name).write_text(queries[key])
        result[shape] = {key: f"{DIRECTORY}/{name}" for key, name in names.items()}
    return result


def generate_optimization(stage, temp, original):
    verifier = BINARIES["chialisp-oxide-verifier"]
    before_path, after_path = stage / "program.ir.json", stage / "optimized.ir.json"
    optimized = json.loads(run([verifier, before_path, "--optimize-ir"]))
    require(optimized == optimized_shape(original), "Actual optimizer did not produce the independently expected p2 rewrite")
    write_json(after_path, optimized)
    proof = json.loads(run([verifier, after_path, "--tree", ROOT / SPECIFICATION]))
    require(proof["status"] == "proven", "Rewritten IR does not satisfy the independent canonical specification")
    queries = query_files(stage, json.loads(run([verifier, after_path, "--tree", ROOT / SPECIFICATION, "--emit-json"])), "optimized-")
    cases = []
    for case in proof["cases"]:
        observed, _ = replay(after_path, case["domain_nonvacuity"], temp, f"optimized-{case['input_shape']}")
        cases.append({**case, "queries": queries[case["input_shape"]], "replay": observed})
    equivalence = json.loads(run([verifier, before_path, "--tree-equivalent", after_path]))
    require(equivalence["status"] == "proven", "Actual before/after structural IR differ")
    queries = query_files(stage, json.loads(run([verifier, before_path, "--tree-equivalent", after_path, "--emit-json"])), "equivalence-")
    equivalence["cases"] = [{**case, "queries": queries[case["input_shape"]]} for case in equivalence["cases"]]
    control_before, control_after = unsafe_guard_control(original)
    control_before_path, control_after_path = stage / "unsafe-guard-before.ir.json", stage / "unsafe-guard-after.ir.json"
    write_json(control_before_path, control_before)
    write_json(control_after_path, control_after)
    # The real optimizer must keep this guard: its successful branch does not
    # evaluate a pair projection. The deliberately unsafe edit must be refuted.
    require(json.loads(run([verifier, control_before_path, "--optimize-ir"])) == control_before,
            "Actual optimizer removed a guard without a required pair projection")
    result = json.loads(run([verifier, control_before_path, "--tree-equivalent", control_after_path], expected=1))
    require(result["status"] == "refuted", "Unsafe guard-removal control was not refuted")
    witness = result["cases"][0]["counterexample"]
    require(witness is not None and isinstance(tree(witness["environment"]), bytes), "Unsafe guard removal needs an actual atom witness")
    witness_path = temp / "unsafe-guard.witness.json"
    write_json(witness_path, witness)
    guarded = json.loads(run([verifier, control_before_path, "--tree-replay", witness_path]))
    unguarded = json.loads(run([verifier, control_after_path, "--tree-replay", witness_path]))
    check_replays(guarded, {"status": "rejected"})
    check_replays(unguarded, {"status": "accepted", "output_hex": wire(b"\xaa").hex()})
    query = json.loads(run([verifier, control_before_path, "--tree-equivalent", control_after_path, "--emit-json"]))["atom_counterexample"]
    (stage / "unsafe-guard.smt2").write_text(query)
    return {
        "pass": "required_pair_projection_guard_elision",
        "ir": {"path": f"{DIRECTORY}/optimized.ir.json", "sha256": sha(after_path)},
        "proof": {"status": "proven", "cases": cases}, "equivalence": equivalence,
        "artifacts": {"source": json.loads(run([verifier, before_path, "--artifacts"])),
                      "rewritten": json.loads(run([verifier, after_path, "--artifacts"]))},
        "control": {"id": "unsafe-guard-drop", "origin": "synthetic structural IR; not a source-derived puzzle",
                    "before_ir": f"{DIRECTORY}/unsafe-guard-before.ir.json", "after_ir": f"{DIRECTORY}/unsafe-guard-after.ir.json",
                    "query": f"{DIRECTORY}/unsafe-guard.smt2", "result": result, "counterexample": witness,
                    "replay": {"guarded": guarded, "unguarded": unguarded}, "detected": True},
    }


def generate(output):
    specification()
    canonical_source()
    current = authoring()
    before = implementation_sources()
    pin = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    run(["cargo", f"+{pin}", "build", "--locked", "--offline", "--target-dir", ROOT / "target",
         "-p", "cargo-chialisp-oxide", "-p", "chialisp-oxide-driver",
         "-p", "chialisp-oxide-cli", "-p", "chialisp-oxide-verifier", "--bins"])
    for path in BINARIES.values():
        require(path.is_file(), f"Missing required compiler/replay binary: {path}")
    with tempfile.TemporaryDirectory(prefix="chialisp-oxide-tree-proofs-") as directory:
        temp = Path(directory)
        cargo_output = temp / "cargo"
        run([BINARIES["cargo-chialisp-oxide"], "build", "--manifest-path", ROOT / current["manifest"],
             "--package", current["package"], "--locked", "--output-dir", cargo_output])
        program = load_cargo(cargo_output, current)
        variants = mutants(program)
        stage = temp / "artifacts"
        stage.mkdir()
        write_json(stage / "program.ir.json", program)
        proof = json.loads(run([BINARIES["chialisp-oxide-verifier"], stage / "program.ir.json", "--tree", ROOT / SPECIFICATION]))
        require(proof["status"] == "proven", "Source-derived structural obligations were not proven")
        queries = json.loads(run([BINARIES["chialisp-oxide-verifier"], stage / "program.ir.json", "--tree", ROOT / SPECIFICATION, "--emit-json"]))
        query_names = {"atom_nonvacuity": "atom-domain.smt2", "atom_counterexample": "atom-case.smt2",
                       "pair_nonvacuity": "pair-domain.smt2", "pair_counterexample": "pair-case.smt2"}
        for key, filename in query_names.items():
            (stage / filename).write_text(queries[key])
        cases, reference = [], None
        for case in proof["cases"]:
            shape = case["input_shape"]
            observed, reference = replay(stage / "program.ir.json", case["domain_nonvacuity"], temp, f"{shape}-domain")
            cases.append({**case, "queries": {"domain": f"{DIRECTORY}/{shape}-domain.smt2", "counterexample": f"{DIRECTORY}/{shape}-case.smt2"}, "replay": observed})
        mutation_reports = []
        for identity, mutant in variants.items():
            path = stage / f"{identity}.ir.json"
            write_json(path, mutant)
            result = json.loads(run([BINARIES["chialisp-oxide-verifier"], path, "--tree", ROOT / SPECIFICATION], expected=1))
            require(result["status"] == "refuted", f"{identity}: mutation did not produce a model counterexample")
            shape = "atom" if identity == "accept-atom" else "pair"
            case = next(case for case in result["cases"] if case["input_shape"] == shape)
            witness = case["counterexample"]
            require(witness is not None, f"{identity}: missing actual solver witness")
            observed, _ = replay(stage / "program.ir.json", witness, temp, identity)
            witness_path = temp / f"{identity}.witness.json"
            mutated = json.loads(run([BINARIES["chialisp-oxide-verifier"], path, "--tree-replay", witness_path]))
            desired = {"status": "accepted", "output_hex": "80"} if identity == "accept-atom" else expected(witness, b"\x02")
            check_replays(mutated, desired)
            require(desired != expected(witness), "Mutation witness does not distinguish the independently specified policy")
            mutation_queries = json.loads(run([BINARIES["chialisp-oxide-verifier"], path, "--tree", ROOT / SPECIFICATION, "--emit-json"]))
            (stage / f"{identity}.smt2").write_text(mutation_queries[f"{shape}_counterexample"])
            mutation_reports.append({"id": identity, "description": "Replace the atom-case rejection with a successful empty atom" if identity == "accept-atom" else "Change only the constructed quote opcode atom from01 to02",
                                     "ir": f"{DIRECTORY}/{identity}.ir.json", "query": f"{DIRECTORY}/{identity}.smt2", "result": result,
                                     "counterexample": witness, "replay": {**observed, "mutant_ir": mutated}, "detected": True})
        optimization = generate_optimization(stage, temp, program)
        require(authoring() == current and implementation_sources() == before, "Declared policy/model/compiler inputs changed during proof/replay")
        artifact_records = [{"path": f"{DIRECTORY}/{name}", "sha256": sha(stage / name)} for name in sorted(ARTIFACTS)]
        reference.pop("runs")
        report = {"schema_version": 1, "authoring": current, "source": SOURCE, "source_sha256": sha(ROOT / SOURCE),
                  "specification": {"path": SPECIFICATION, "sha256": sha(ROOT / SPECIFICATION)},
                  "ir": {"path": f"{DIRECTORY}/program.ir.json", "sha256": sha(stage / "program.ir.json"), "diagnostic_normalization": NORMALIZATION},
                  "implementation_sources": before, "vm_failure_protocol": PROTOCOL, "scope": SCOPE,
                  "reference": {"source": REFERENCE, "source_sha256": REFERENCE_SHA, "published_artifact": PUBLISHED, **reference},
                  "proof": {"status": "proven", "cases": cases}, "mutations": mutation_reports,
                  "optimization": optimization, "artifacts": artifact_records}
        # Publish the report last; interrupted artifact writes cannot masquerade
        # as current evidence because the read-only gate checks every hash.
        for name in sorted(ARTIFACTS):
            (ROOT / DIRECTORY / name).write_bytes((stage / name).read_bytes())
        validate(report)
        write_json(output, report)


def artifact_hash(encoded):
    """Independently hash ordinary serialized atom/pair artifacts, not source IR."""
    require(isinstance(encoded, str) and re.fullmatch(r"(?:[0-9a-f]{2})+", encoded), "Invalid compiled artifact hex")
    data = bytes.fromhex(encoded)
    require(len(data) <= 1_048_576, "Compiled artifact exceeds checker bound")
    position, nodes = 0, 0
    def read(depth):
        nonlocal position, nodes
        nodes += 1
        require(depth <= 256 and nodes <= 20_000 and position < len(data), "Malformed compiled artifact tree")
        prefix = data[position]
        position += 1
        if prefix == 255:
            return hashlib.sha256(b"\x02" + read(depth + 1) + read(depth + 1)).digest()
        if prefix <= 127:
            payload = bytes([prefix])
        else:
            mask, count = 128, 0
            while prefix & mask:
                count += 1
                mask >>= 1
            require(count <= 6 and position + count - 1 <= len(data), "Malformed compiled atom header")
            size = prefix & (mask - 1)
            for _ in range(count - 1):
                size = (size << 8) | data[position]
                position += 1
            require(position + size <= len(data), "Truncated compiled atom")
            payload = data[position:position + size]
            position += size
        return hashlib.sha256(b"\x01" + payload).digest()
    digest = read(0)
    require(position == len(data), "Trailing compiled artifact bytes")
    return digest.hex()


def validate_optimization(value, original):
    require(value["pass"] == "required_pair_projection_guard_elision", "Changed optimization claim")
    path = f"{DIRECTORY}/optimized.ir.json"
    require(value["ir"] == {"path": path, "sha256": sha(ROOT / path)}
            and load_json(ROOT / path) == optimized_shape(original), "Rewritten IR differs from the reviewed canonical construction")
    for key, prefix in [("proof", "optimized-"), ("equivalence", "equivalence-")]:
        proof = value[key]
        require(proof["status"] == "proven" and [case["input_shape"] for case in proof["cases"]] == ["atom", "pair"],
                "Missing optimized-specification or before/after equivalence obligation")
        for case in proof["cases"]:
            shape, witness = case["input_shape"], case["domain_nonvacuity"]
            require(case["counterexample"] is None and (isinstance(tree(witness["environment"]), bytes)) == (shape == "atom"),
                    "Vacuous or failed optimized/equivalence proof")
            require(case["queries"] == {"domain": f"{DIRECTORY}/{prefix}{shape}-domain.smt2",
                    "counterexample": f"{DIRECTORY}/{prefix}{shape}-case.smt2"}, "Wrong optimized/equivalence query binding")
            if key == "proof":
                observed, desired = case["replay"], expected(witness)
                require(check_native_outcome(observed["native_original"]) == desired
                        and outcome(observed["canonical_reference"]) == desired, "Rewritten witness differs from native/canonical reference")
                check_replays(observed["rust_original"], desired)
    artifacts = value["artifacts"]
    require(set(artifacts) == {"source", "rewritten"}, "Missing before/after compiled artifact evidence")
    for kind in ["source", "rewritten"]:
        require(len(artifacts[kind]) == 2 and [row["optimize"] for row in artifacts[kind]] == [False, True],
                "Missing before/after compilation mode")
        for row in artifacts[kind]:
            require(type(row["optimize"]) is bool and type(row["bytes"]) is int
                    and row["bytes"] == len(row["clvm_hex"]) // 2 and row["tree_hash"] == artifact_hash(row["clvm_hex"]),
                    "Compiled artifact byte count or independently computed tree hash differs")
            if row["optimize"]:
                require(row["clvm_hex"] == CLVM_HEX and row["tree_hash"] == TREE_HASH,
                        "Optimized canonical module identity has not been achieved")
    require(artifacts["source"][0]["clvm_hex"] != CLVM_HEX,
            "Expected a genuine unoptimized source baseline for the measured hash-gap experiment")
    require(artifacts["rewritten"][0]["bytes"] < artifacts["source"][0]["bytes"],
            "Explicit IR rewrite did not reduce the genuinely unoptimized artifact")
    control = value["control"]
    require(control["id"] == "unsafe-guard-drop" and control["origin"] == "synthetic structural IR; not a source-derived puzzle"
            and control["detected"] is True and control["query"] == f"{DIRECTORY}/unsafe-guard.smt2", "False or mislabeled unsafe-rule control")
    before, after = unsafe_guard_control(original)
    for key, expected_ir in [("before_ir", before), ("after_ir", after)]:
        name = "before" if key == "before_ir" else "after"
        require(control[key] == f"{DIRECTORY}/unsafe-guard-{name}.ir.json" and load_json(ROOT / control[key]) == expected_ir,
                "Unsafe-rule control changed unrelated IR")
    result, witness = control["result"], control["counterexample"]
    require(result["status"] == "refuted" and [case["input_shape"] for case in result["cases"]] == ["atom", "pair"]
            and result["cases"][0]["counterexample"] == witness and isinstance(tree(witness["environment"]), bytes),
            "Unsafe rule lacks an actual atom counterexample")
    for case in result["cases"]:
        require((isinstance(tree(case["domain_nonvacuity"]["environment"]), bytes)) == (case["input_shape"] == "atom"),
                "Missing unsafe-rule domain witness")
    check_replays(control["replay"]["guarded"], {"status": "rejected"})
    check_replays(control["replay"]["unguarded"], {"status": "accepted", "output_hex": wire(b"\xaa").hex()})


def validate(report):
    require(report.get("vm_failure_protocol") == PROTOCOL, "Missing/unsupported VM failure protocol")
    specification()
    canonical_source()
    require(type(report["schema_version"]) is int and report["schema_version"] == 1 and report["scope"] == SCOPE, "Unsupported structural proof schema/scope")
    require(report["authoring"] == authoring() and report["implementation_sources"] == implementation_sources(), "Stale declared Cargo/model/compiler implementation inputs")
    require(report["source"] == SOURCE and report["source_sha256"] == sha(ROOT / SOURCE), "Stale source-derived policy")
    require(report["specification"] == {"path": SPECIFICATION, "sha256": sha(ROOT / SPECIFICATION)}, "Changed structural specification")
    artifacts = report["artifacts"]
    require([item["path"] for item in artifacts] == [f"{DIRECTORY}/{name}" for name in sorted(ARTIFACTS)], "Missing/duplicate structural proof artifacts")
    for artifact in artifacts:
        require(artifact["sha256"] == sha(contained_file(ROOT, artifact["path"])), "Stale IR/query artifact hash")
    ir_path = ROOT / DIRECTORY / "program.ir.json"
    require(report["ir"] == {"path": f"{DIRECTORY}/program.ir.json", "sha256": sha(ir_path), "diagnostic_normalization": NORMALIZATION}, "Changed source-IR binding or metadata normalization")
    variants = mutants(load_json(ir_path))
    for identity, variant in variants.items():
        require(load_json(ROOT / DIRECTORY / f"{identity}.ir.json") == variant, "Mutation changed fields beyond the exact reviewed IR edit")
    reference = report["reference"]
    require(reference == {"source": REFERENCE, "source_sha256": REFERENCE_SHA, "published_artifact": PUBLISHED,
                          "compiler": "chialisp 0.4.6", "vm": "clvmr 0.21.0 MEMPOOL_MODE", "max_cost": 10_000_000,
                          "bytes": 9, "clvm_hex": CLVM_HEX, "tree_hash": TREE_HASH}, "Canonical independent compilation/module identity claim changed")
    proof = report["proof"]
    require(proof["status"] == "proven" and len(proof["cases"]) == 2
            and [case["input_shape"] for case in proof["cases"]] == ["atom", "pair"], "Missing either structural domain obligation")
    for case in proof["cases"]:
        shape, witness = case["input_shape"], case["domain_nonvacuity"]
        value = tree(witness["environment"])
        require((isinstance(value, bytes)) == (shape == "atom") and case["counterexample"] is None, "Vacuous/misclassified/failed structural proof")
        require(case["queries"] == {"domain": f"{DIRECTORY}/{shape}-domain.smt2", "counterexample": f"{DIRECTORY}/{shape}-case.smt2"}, "Wrong structural case/query binding")
        observed = case["replay"]
        desired = expected(witness)
        require(check_native_outcome(observed["native_original"]) == desired and outcome(observed["canonical_reference"]) == desired, "Model domain witness disagrees with actual native/reference replay")
        check_replays(observed["rust_original"], desired)
    require([row["id"] for row in report["mutations"]] == ["accept-atom", "wrong-quote"], "Both guard/construction mutation controls are required")
    for row in report["mutations"]:
        identity, witness = row["id"], row["counterexample"]
        shape = "atom" if identity == "accept-atom" else "pair"
        require(row["detected"] is True and row["result"]["status"] == "refuted"
                and row["ir"] == f"{DIRECTORY}/{identity}.ir.json" and row["query"] == f"{DIRECTORY}/{identity}.smt2", "False mutation-detection claim")
        cases = row["result"]["cases"]
        require([case["input_shape"] for case in cases] == ["atom", "pair"] and all(case["domain_nonvacuity"] for case in cases), "Missing mutation domain nonvacuity")
        require(next(case for case in cases if case["input_shape"] == shape)["counterexample"] == witness,
                "Mutation replay is not the model's counterexample")
        require((isinstance(tree(witness["environment"]), bytes)) == (shape == "atom"), "Wrong mutation witness domain")
        observed = row["replay"]
        require(check_native_outcome(observed["native_original"]) == expected(witness)
                and outcome(observed["canonical_reference"]) == expected(witness), "Mutation witness does not preserve original native/reference behavior")
        check_replays(observed["rust_original"], expected(witness))
        mutated = {"status": "accepted", "output_hex": "80"} if identity == "accept-atom" else expected(witness, b"\x02")
        require(mutated != expected(witness), "Mutation witness has no observable policy difference")
        check_replays(observed["mutant_ir"], mutated)
    validate_optimization(report["optimization"], load_json(ir_path))


class ProofControls(unittest.TestCase):
    def test_replay_modes_require_actual_booleans(self):
        original = [{"optimize": optimize, "outcome": {"status": "accepted", "output_hex": "01", "cost": 1}}
                    for optimize in (False, True)]
        desired = {"status": "accepted", "output_hex": "01"}
        check_replays(original, desired)
        for value in (0, 1, 0.0, 1.0):
            rows = copy.deepcopy(original)
            next(row for row in rows if row["optimize"] is bool(value))["optimize"] = value
            with self.subTest(numeric=value), self.assertRaises(ValueError):
                check_replays(rows, desired)

    def test_stale_false_proof_and_missing_mutation_claims_fail_closed(self):
        original = load_json(OUTPUT)
        validate(original)
        for mutation in ("source", "model", "scope", "ir", "query", "canonical_hash", "case", "nonvacuity", "native", "rust_vm", "reference_vm", "mutation", "witness", "mutant_vm"):
            changed = copy.deepcopy(original)
            if mutation == "source": changed["source_sha256"] = "0" * 64
            elif mutation == "model": changed["implementation_sources"][0]["sha256"] = "0" * 64
            elif mutation == "scope": changed["scope"]["kind"] = "compiler_preservation_proof"
            elif mutation == "ir": changed["ir"]["diagnostic_normalization"] = "Rewrite source policy"
            elif mutation == "query": changed["artifacts"][0]["sha256"] = "0" * 64
            elif mutation == "canonical_hash": changed["reference"]["tree_hash"] = "0" * 64
            elif mutation == "case": changed["proof"]["cases"].pop()
            elif mutation == "nonvacuity": changed["proof"]["cases"][0]["domain_nonvacuity"] = changed["proof"]["cases"][1]["domain_nonvacuity"]
            elif mutation == "native": changed["proof"]["cases"][1]["replay"]["native_original"]["output_hex"] = "80"
            elif mutation == "rust_vm": changed["proof"]["cases"][1]["replay"]["rust_original"].pop()
            elif mutation == "reference_vm": changed["proof"]["cases"][1]["replay"]["canonical_reference"]["output_hex"] = "80"
            elif mutation == "mutation": changed["mutations"][0]["detected"] = False
            elif mutation == "witness": changed["mutations"][0]["counterexample"] = changed["mutations"][1]["counterexample"]
            else: changed["mutations"][0]["replay"]["mutant_ir"][0]["outcome"] = {"status": "rejected", "error": "Raise"}
            with self.subTest(mutation=mutation), self.assertRaises((ValueError, KeyError, TypeError)):
                validate(changed)

    def test_optimization_evidence_cannot_hide_a_failed_proof_or_hash_gap(self):
        original = load_json(OUTPUT)
        validate(original)
        for mutation in ("ir", "equivalence", "specification", "domain", "replay", "artifact", "hash", "baseline", "control", "witness", "control_replay", "origin"):
            changed = copy.deepcopy(original)
            value = changed["optimization"]
            if mutation == "ir": value["ir"]["sha256"] = "0" * 64
            elif mutation == "equivalence": value["equivalence"]["status"] = "refuted"
            elif mutation == "specification": value["proof"]["cases"][1]["counterexample"] = value["proof"]["cases"][1]["domain_nonvacuity"]
            elif mutation == "domain": value["equivalence"]["cases"][0]["domain_nonvacuity"] = value["equivalence"]["cases"][1]["domain_nonvacuity"]
            elif mutation == "replay": value["proof"]["cases"][1]["replay"]["rust_original"][0]["outcome"]["output_hex"] = "80"
            elif mutation == "artifact":
                value["artifacts"]["rewritten"][1].update(bytes=1, clvm_hex="01", tree_hash=artifact_hash("01"))
            elif mutation == "hash": value["artifacts"]["rewritten"][0]["tree_hash"] = "0" * 64
            elif mutation == "baseline":
                value["artifacts"]["source"][0] = copy.deepcopy(value["artifacts"]["rewritten"][0])
            elif mutation == "control": value["control"]["result"]["status"] = "proven"
            elif mutation == "witness": value["control"]["counterexample"] = value["equivalence"]["cases"][1]["domain_nonvacuity"]
            elif mutation == "control_replay": value["control"]["replay"]["guarded"][0]["outcome"] = {"status": "accepted", "output_hex": "81aa", "cost": 1}
            else: value["control"]["origin"] = "source-derived canonical puzzle"
            with self.subTest(mutation=mutation), self.assertRaises((ValueError, KeyError, TypeError)):
                validate(changed)

    def test_nested_vm_failures_require_exact_typed_program_classification(self):
        original = load_json(OUTPUT)
        validate(original)
        # Cover reference compilation, source IR, rewritten IR, mutation and
        # synthetic unsafe-optimization controls, without altering their status.
        def rejected_records(value):
            if isinstance(value, dict):
                if value.get("status") == "rejected" and "error" in value:
                    yield value
                for nested in value.values(): yield from rejected_records(nested)
            elif isinstance(value, list):
                for nested in value: yield from rejected_records(nested)
        count = len(list(rejected_records(original)))
        self.assertGreaterEqual(count, 5)
        for index in range(count):
            for mutation in ("missing", "unknown", "resource", "infrastructure", "forged"):
                changed = copy.deepcopy(original)
                record = list(rejected_records(changed))[index]
                record["error"] = "Raise(spoofed diagnostic)"
                if mutation == "missing": del record["failure"]
                elif mutation == "unknown": record["failure"]["code"] = "future_code"
                elif mutation == "resource": record["failure"] = {"kind": "resource", "code": "cost_exceeded"}
                elif mutation == "infrastructure": record["failure"] = {"kind": "infrastructure", "code": "internal_error"}
                else: record["failure"] = {"kind": "program", "code": "cost_exceeded"}
                with self.subTest(index=index, mutation=mutation), self.assertRaises(ValueError):
                    validate(changed)

    def test_guard_matcher_and_metadata_normalization_cannot_hide_policy_edits(self):
        original = load_json(ROOT / DIRECTORY / "program.ir.json")
        variants = mutants(original)
        self.assertNotEqual(variants["accept-atom"], original)
        self.assertNotEqual(variants["wrong-quote"], original)
        altered = copy.deepcopy(original)
        altered["program"]["body"]["else_value"]["then_value"]["first"]["value"]["hex"] = "02"
        with self.assertRaises(ValueError): mutants(altered)
        normalized = normalized_ir(original)
        self.assertEqual(mutants(normalized)["wrong-quote"]["program"]["body"]["else_value"]["then_value"]["first"]["value"]["hex"], "02")
        for byte in [b"", b"\x00", b"\x01", b"\x80", b"\x00\x01"]:
            self.assertEqual(tree({"kind": "atom", "hex": byte.hex()}), byte)
        self.assertEqual(wire((b"\x01", b"")), bytes.fromhex("ff0180"))
        self.assertEqual(wire((b"\x00\x01", b"\x00")), bytes.fromhex("ff82000100"))
        self.assertEqual(artifact_hash(CLVM_HEX), TREE_HASH)
        for malformed in ["", "ff", "81", "01ff", "fe"]:
            with self.subTest(serialization=malformed), self.assertRaises(ValueError): artifact_hash(malformed)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=OUTPUT)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        result = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(ProofControls))
        return 0 if result.wasSuccessful() else 1
    try:
        if args.check:
            validate(load_json(args.output))
        else:
            generate(args.output)
    except (ValueError, KeyError, TypeError, OSError, subprocess.TimeoutExpired) as error:
        parser.exit(1, f"Structural tree proof failed: {error}\n")
    print("PASS: source-derived p2_conditions structural model; atom/pair domains, two obligations, guard/construction mutations and native/VM replays" + (" (recorded evidence freshness only)" if args.check else ""))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
