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
    "excludes": ["rustc extraction preservation", "SMT-translator correctness proof", "CLVM lowering/serialization preservation", "Rue optimization preservation", "VM cost/resource equivalence", "Rust canonical module identity", "consensus/authorization/security proof"],
}
BINARIES = {name: ROOT / f"target/debug/{name}" for name in [
    "cargo-chialisp-oxide", "chialisp-oxide-driver", "chialisp-oxide", "chialisp-oxide-verifier", "chialisp-oxide-tree-reference",
]}
ARTIFACTS = {"program.ir.json", "accept-atom.ir.json", "wrong-quote.ir.json", "atom-domain.smt2",
             "pair-domain.smt2", "atom-case.smt2", "pair-case.smt2", "accept-atom.smt2", "wrong-quote.smt2"}


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
    for package in ["crates/chialisp-oxide-driver", "crates/chialisp-oxide-ir", "crates/chialisp-oxide-verifier",
                    "crates/chialisp-oxide-reference", "crates/chialisp-oxide-corpus", "crates/chialisp-oxide-cli",
                    "crates/cargo-chialisp-oxide", "experiments/backend", "vendor/rue-lir"]:
        paths.add(f"{package}/Cargo.toml")
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
    require(value["status"] in ("accepted", "rejected"), "Invalid replay status")
    if value["status"] == "rejected":
        require("CostExceeded" not in value.get("error", ""), "Cost exhaustion cannot stand for a policy rejection")
        return {"status": "rejected"}
    require(isinstance(value["output_hex"], str) and re.fullmatch(r"(?:[0-9a-f]{2})+", value["output_hex"]), "Invalid replay output bytes")
    if "cost" in value:
        require(type(value["cost"]) is int and value["cost"] > 0, "Missing successful replay cost")
    return {"status": "accepted", "output_hex": value["output_hex"]}


def check_replays(rows, desired):
    require(len(rows) == 2 and {row["optimize"] for row in rows} == {False, True}, "Missing tree replay optimization mode")
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
    require(outcome(native) == desired and outcome(reference["runs"][0]["outcome"]) == desired,
            "Native/independently compiled original differs from the structural specification")
    check_replays(rust, desired)
    return {"native_original": native, "rust_original": rust, "canonical_reference": reference["runs"][0]["outcome"]}, reference


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
        require(authoring() == current and implementation_sources() == before, "Declared policy/model/compiler inputs changed during proof/replay")
        artifact_records = [{"path": f"{DIRECTORY}/{name}", "sha256": sha(stage / name)} for name in sorted(ARTIFACTS)]
        reference.pop("runs")
        report = {"schema_version": 1, "authoring": current, "source": SOURCE, "source_sha256": sha(ROOT / SOURCE),
                  "specification": {"path": SPECIFICATION, "sha256": sha(ROOT / SPECIFICATION)},
                  "ir": {"path": f"{DIRECTORY}/program.ir.json", "sha256": sha(stage / "program.ir.json"), "diagnostic_normalization": NORMALIZATION},
                  "implementation_sources": before, "scope": SCOPE,
                  "reference": {"source": REFERENCE, "source_sha256": REFERENCE_SHA, "published_artifact": PUBLISHED, **reference},
                  "proof": {"status": "proven", "cases": cases}, "mutations": mutation_reports, "artifacts": artifact_records}
        # Publish the report last; interrupted artifact writes cannot masquerade
        # as current evidence because the read-only gate checks every hash.
        for name in sorted(ARTIFACTS):
            (ROOT / DIRECTORY / name).write_bytes((stage / name).read_bytes())
        validate(report)
        write_json(output, report)


def validate(report):
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
        require(outcome(observed["native_original"]) == desired and outcome(observed["canonical_reference"]) == desired, "Model domain witness disagrees with actual native/reference replay")
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
        require(outcome(observed["native_original"]) == expected(witness)
                and outcome(observed["canonical_reference"]) == expected(witness), "Mutation witness does not preserve original native/reference behavior")
        check_replays(observed["rust_original"], expected(witness))
        mutated = {"status": "accepted", "output_hex": "80"} if identity == "accept-atom" else expected(witness, b"\x02")
        require(mutated != expected(witness), "Mutation witness has no observable policy difference")
        check_replays(observed["mutant_ir"], mutated)


class ProofControls(unittest.TestCase):
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
