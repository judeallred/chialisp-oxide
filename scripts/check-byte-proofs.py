#!/usr/bin/env python3
"""Check actual Cargo byte-policy IR against an independent whole-tree contract.

Default builds, proves and replays before publishing portable evidence. --check
checks recorded source/artifact/claim freshness without invoking Rust or Z3.
Neither mode proves extraction, the symbolic translator or CLVM preservation.
"""
import argparse
import copy
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib
import unittest

from byte_proof_evidence import (
    ROOT, DIRECTORY, SPECIFICATION, SOURCE, ENTRY, CONTROL_IDS, POSITIVE, SCOPE,
    arguments, authoring, boolean_outcome, check_artifact, checked_corpus, check_program, control_authoring,
    control_expected, expected, implementation_sources, lineage, require, sha,
    solution, specification, tree,
)
from cargo_authoring import contained_file, load_json
from cargo_bundle import load_declared_export
from vm_failures import PROTOCOL, check_execution, check_native_outcome, check_replay

OUTPUT = ROOT / "corpus/byte-proofs.json"
VERIFIER = ROOT / "target/debug/chialisp-oxide-verifier"
CLI = ROOT / "target/debug/chialisp-oxide"
DRIVER = ROOT / "target/debug/chialisp-oxide-driver"
CARGO = ROOT / "target/debug/cargo-chialisp-oxide"
NATIVE = ROOT / "target/debug/chialisp-oxide-byte-proof-native"
QUERIES = ("abi_nonvacuity", "acceptance_nonvacuity", "true_nonvacuity", "false_nonvacuity", "counterexample")
SYNTHETIC = ("loose_environment", "fixed_length_narrowing")
MODELS = ("original", *CONTROL_IDS, *SYNTHETIC)
ARTIFACTS = ({f"{name}.ir.json" for name in MODELS}
             | {f"{name}-{query}.smt2" for name in MODELS for query in QUERIES}
             | {f"{name}-{mode}.clvm.hex" for name in MODELS for mode in ("off", "on")})
EXTRACTION = {"protocol": "cargo-chialisp-oxide-artifact/v1", "compiler_driver_identity_checked": True,
    "standard_library": "Trusts the installed pinned compiler/standard library and primitive bridge; no preservation theorem.",
    "portable_record": "Machine-specific driver digest/full compiler identity were checked against fresh local Cargo manifests; recorded inputs below identify source, not a binary attestation."}


def run(args, expected_code=0, timeout=180):
    result = subprocess.run([str(arg) for arg in args], cwd=ROOT, text=True,
                            capture_output=True, timeout=timeout,
                            env={**os.environ, "CHIALISP_OXIDE_DRIVER": str(DRIVER)})
    require(result.returncode == expected_code,
            f"Command failed ({result.returncode}, expected {expected_code}): {args}\n{result.stdout}\n{result.stderr}")
    return result.stdout


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def builds(program):
    rows = json.loads(run([VERIFIER, program, "--artifacts"]))
    require(len(rows) == 2 and [row["optimize"] for row in rows] == [False, True], "Missing actual IR build mode")
    return rows


def check_replays(rows, desired):
    require(len(rows) == 2 and all(set(row) == {"optimize", "outcome", "artifact", "retained_execution"}
                                 and type(row["optimize"]) is bool for row in rows)
            and [row["optimize"] for row in rows] == [False, True], "Missing Boolean replay modes")
    for row in rows:
        require(check_replay(row["outcome"], "byte-model VM replay") == desired,
                "Modeled VM output differs from independent data relation")
        retained = row["retained_execution"]
        require(check_execution(retained, "retained byte-model artifact") == desired,
                "Retained artifact execution differs from independent data relation")
        if desired["status"] == "accepted":
            require(retained["cost"] == row["outcome"]["cost"], "Retained artifact replay cost differs from internal IR replay")
        else:
            require(retained["failure"] == row["outcome"]["failure"], "Retained artifact replay failure differs from internal IR replay")


def source_native(control, witness):
    values = arguments(witness)
    require(values is not None, "Native source-control replay requires well-typed atom arguments")
    value = json.loads(run([NATIVE, control, values[0].hex(), values[1].hex()]))
    return boolean_outcome(value)


def original_replay(witness, corpus, temp):
    desired = expected(witness)
    expression = solution(tree(witness["environment"]))
    native = json.loads(run([CLI, "native", "atom-not-equal", "--solution", expression]))
    require(check_native_outcome(native) == desired, "Actual native Cargo policy disagrees with independent byte specification")
    rows = []
    for index, artifact in enumerate(corpus["artifacts"]):
        path = temp / f"original-{index}.hex"
        path.write_text(artifact["clvm_hex"] + "\n")
        execution = json.loads(run([CLI, "run", path, "--solution", expression, "--max-cost", "10000000"],
                                   expected_code=0 if desired["status"] == "accepted" else 2))
        require(check_execution(execution) == desired, "Original language artifact disagrees with independent witness relation")
        rows.append({"backend": artifact["backend"], "optimize_lir": artifact["optimize_lir"], "execution": execution})
    return {"native": native, "runs": rows}


def synthetic_program(original, name):
    result = copy.deepcopy(original)
    result["name"] = f"synthetic::{name}"
    if name == "fixed_length_narrowing":
        for parameter in result["parameters"]:
            parameter["ty"] = {"kind": "bytes", "len": 2}
        result["body"]["ty"] = {"kind": "bytes", "len": 2}
        return result
    require(name == "loose_environment", "Unknown synthetic ABI control")
    def atom_argument(index):
        value = {"op": "arg", "index": 0}
        if index:
            value = {"op": "tree_rest", "value": value}
        return {"op": "tree_atom_value", "value": {"op": "tree_first", "value": value}}
    result["parameters"] = [{"name": "environment", "ty": {"kind": "tree"}}]
    result["body"]["left"] = atom_argument(0)
    result["body"]["right"] = atom_argument(1)
    return {"ir_version": 3, "environment": "raw", "program": result}


def proof_model(name, ir_path, stage, corpus, temp):
    negative = name in (*SYNTHETIC, *(control for control in CONTROL_IDS if control != POSITIVE))
    result = json.loads(run([VERIFIER, ir_path, "--structural-boolean", ROOT / SPECIFICATION], expected_code=int(negative)))
    require(result["status"] == ("refuted" if negative else "proven"), "Unexpected model proof/control status")
    queries = json.loads(run([VERIFIER, ir_path, "--structural-boolean", ROOT / SPECIFICATION, "--emit-json"]))
    require(set(queries) == set(QUERIES), "Unexpected structural Boolean query protocol")
    compiled = builds(ir_path)
    for artifact in compiled:
        mode = "on" if artifact["optimize"] else "off"
        (stage / f"{name}-{mode}.clvm.hex").write_text(artifact["clvm_hex"] + "\n")
    files = {}
    for key in QUERIES:
        filename = f"{name}-{key}.smt2"
        (stage / filename).write_text(queries[key])
        files[key] = f"{DIRECTORY}/{filename}"
    witnesses = {"counterexample": result["counterexample"]} if negative else {
        key: result[key] for key in ("abi_nonvacuity", "acceptance_nonvacuity", "true_witness", "false_witness")}
    grouped = {}
    for role, witness in witnesses.items():
        require(witness is not None, "Actual byte helper requires both expected true/false witness domains")
        identity = json.dumps(witness, sort_keys=True)
        group = grouped.setdefault(identity, {"witness": witness, "roles": []})
        group["roles"].append(role)
    replays = []
    for index, group in enumerate(grouped.values()):
        witness = group["witness"]
        path = temp / f"{name}-{index}.witness.json"
        write_json(path, witness)
        modeled = json.loads(run([VERIFIER, ir_path, "--structural-boolean-replay", path]))
        for replay in modeled:
            mode = "on" if replay["optimize"] else "off"
            replay["artifact"] = f"{DIRECTORY}/{name}-{mode}.clvm.hex"
            replay["retained_execution"] = json.loads(run([
                CLI, "run", stage / f"{name}-{mode}.clvm.hex", "--solution", solution(tree(witness["environment"])),
                "--max-cost", "10000000"], expected_code=0 if replay["outcome"]["status"] == "accepted" else 2))
        original = original_replay(witness, corpus, temp)
        desired = expected(witness)
        if name in CONTROL_IDS:
            wanted = control_expected(name, witness)
            native_control = source_native(name, witness)
            require(native_control == wanted, "Actual mutated Rust source differs from its explicit relation")
        else:
            native_control = None
            wanted = desired if name == "original" else boolean_outcome(False)
            if name == "loose_environment":
                value = tree(witness["environment"])
                require(desired == {"status": "rejected"} and isinstance(value, tuple) and isinstance(value[0], bytes)
                        and isinstance(value[1], tuple) and isinstance(value[1][0], bytes), "Loose ABI needs two atom heads outside the strict contract")
                wanted = boolean_outcome(value[0] != value[1][0])
            elif name == "fixed_length_narrowing":
                require(arguments(witness) is not None and any(len(value) != 2 for value in arguments(witness)), "Narrow ABI needs accepted original atom widths outside2")
                wanted = {"status": "rejected"}
        check_replays(modeled, wanted)
        require((wanted != desired) == negative, "Mutation witness must actually distinguish outcomes; positive control must agree")
        replays.append({**group, "expected_original": desired, "expected_modeled": wanted,
                        "original": original, "modeled": modeled, "native_control": native_control})
    return {"id": name, "origin": "actual_cargo_source" if name in ("original", *CONTROL_IDS) else "synthetic_abi_ir_control",
            "ir": f"{DIRECTORY}/{name}.ir.json", "queries": files, "result": result,
            "artifacts": compiled, "replays": replays}


def generate(output):
    before, controls, inputs = authoring(), control_authoring(), implementation_sources()
    specification()
    run(["cargo", "build", "--workspace", "--locked"])
    compiler, driver_hash = run(["rustc", "-vV"]), sha(DRIVER)
    toolchain = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    corpus_report, corpus = checked_corpus()
    with tempfile.TemporaryDirectory(prefix="chialisp-oxide-byte-proof-") as directory:
        temp = Path(directory)
        stage = temp / "proofs"
        stage.mkdir()
        exports = {}
        for key, declared, callback in (("original", before, authoring), ("controls", controls, control_authoring)):
            bundle = temp / key
            options = ["--manifest-path", declared["manifest"], "--locked"]
            run([CARGO, "build", *options, "--output-dir", bundle])
            selected = {"original": ENTRY} if key == "original" else {name: row["entry"] for name, row in declared["exports"].items()}
            for name, entry in selected.items():
                export = load_declared_export(bundle, root=ROOT, authoring=declared, selected_entry=entry,
                    compiler=compiler, driver_sha256=driver_hash, toolchain=toolchain, current_authoring=callback,
                    rue_revision=corpus_report["rue_revision"], cargo_options=options, optimize=True)
                check_program(export.program, None if name == "original" else name)
                shutil.copyfile(export.path, stage / f"{name}.ir.json")
                require(sha(stage / f"{name}.ir.json") == export.ir_sha256, "Retained IR must be the exact raw Cargo export bytes")
                exports[name] = export.program
        for name in SYNTHETIC:
            write_json(stage / f"{name}.ir.json", synthetic_program(exports["original"], name))
        models = [proof_model(name, stage / f"{name}.ir.json", stage, corpus, temp) for name in MODELS]
        require(before == authoring() and controls == control_authoring() and inputs == implementation_sources()
                and driver_hash == sha(DRIVER), "Source/compiler inputs changed during proof extraction or replay")
        destination = ROOT / DIRECTORY
        for name in ARTIFACTS:
            shutil.copyfile(stage / name, destination / name)
        report = {"schema_version": 1, "scope": SCOPE, "vm_failure_protocol": PROTOCOL,
                  "toolchain": compiler.splitlines()[0], "solver": run(["z3", "--version"]).strip(),
                  "extraction": EXTRACTION,
                  "authoring": before, "control_authoring": controls, "source_lineage": lineage(),
                  "implementation_sources": inputs, "specification": {"path": SPECIFICATION, "sha256": sha(ROOT / SPECIFICATION)},
                  "artifacts": [{"path": f"{DIRECTORY}/{name}", "sha256": sha(destination / name)} for name in sorted(ARTIFACTS)],
                  "models": models}
        validate(report)
        write_json(output, report)
    print(f"PASS: actual Cargo byte IR +5 real source controls +2 synthetic ABI controls; solver/native/five-language-mode witness replays -> {output}")


def validate(report):
    require(type(report["schema_version"]) is int and report["schema_version"] == 1
            and report["scope"] == SCOPE and report["vm_failure_protocol"] == PROTOCOL,
            "Unsupported byte proof scope/protocol")
    require(report["authoring"] == authoring() and report["control_authoring"] == control_authoring()
            and report["source_lineage"] == lineage() and report["implementation_sources"] == implementation_sources(), "Stale declared source/compiler/model inputs")
    require(report["specification"] == {"path": SPECIFICATION, "sha256": sha(ROOT / SPECIFICATION)}, "Stale independent byte specification")
    specification()
    require([Path(row["path"]).name for row in report["artifacts"]] == sorted(ARTIFACTS), "Missing/duplicate/unexpected proof artifact")
    for artifact in report["artifacts"]:
        require(artifact["path"] == f"{DIRECTORY}/{Path(artifact['path']).name}"
                and sha(contained_file(ROOT, artifact["path"])) == artifact["sha256"], "Changed proof artifact bytes")
    require([row["id"] for row in report["models"]] == list(MODELS), "Missing/reordered/duplicate actual models or mutation controls")
    original = load_json(ROOT / DIRECTORY / "original.ir.json")
    check_program(original)
    corpus_report, corpus = checked_corpus()
    require(report["extraction"] == EXTRACTION and report["extraction"]["compiler_driver_identity_checked"] is True
            and report["toolchain"] == corpus_report["toolchain"]
            and isinstance(report["solver"], str) and report["solver"].startswith("Z3 version "), "Wrong extraction/solver/toolchain provenance")
    for row in report["models"]:
        validate_model(row, original, corpus)


def validate_model(row, original, corpus):
    name = row["id"]
    program = load_json(contained_file(ROOT, row["ir"]))
    require(row["ir"] == f"{DIRECTORY}/{name}.ir.json", "Wrong modeled IR identity")
    if name in SYNTHETIC:
        require(program == synthetic_program(original, name) and row["origin"] == "synthetic_abi_ir_control", "Synthetic ABI control misrepresented")
    else:
        check_program(program, None if name == "original" else name)
        require(row["origin"] == "actual_cargo_source", "Actual source control mislabeled")
    negative = name not in ("original", POSITIVE)
    result = row["result"]
    fields = {"status", "abi_nonvacuity", "acceptance_nonvacuity", "true_witness", "false_witness"} | ({"counterexample"} if negative else set())
    require(set(result) == fields and result["status"] == ("refuted" if negative else "proven"), "False/vacuous model result")
    for key in fields - {"status"}:
        require(result[key] is not None, "Missing required satisfiable model witness")
        tree(result[key]["environment"])
    abi_values = arguments(result["abi_nonvacuity"])
    if name != "loose_environment":
        require(abi_values is not None and (name != "fixed_length_narrowing" or all(len(x) == 2 for x in abi_values)), "Invalid actual ABI nonvacuity witness")
    require(expected(result["acceptance_nonvacuity"])["status"] == "accepted"
            and expected(result["true_witness"]) == boolean_outcome(True)
            and expected(result["false_witness"]) == boolean_outcome(False), "Nonvacuity witness disagrees with independent byte relation")
    require(row["queries"] == {query: f"{DIRECTORY}/{name}-{query}.smt2" for query in QUERIES}, "Query coverage/path changed")
    require(len(row["artifacts"]) == 2 and [a["optimize"] for a in row["artifacts"]] == [False, True], "Missing modeled artifacts")
    for artifact in row["artifacts"]:
        check_artifact(artifact)
        mode = "on" if artifact["optimize"] else "off"
        require((ROOT / DIRECTORY / f"{name}-{mode}.clvm.hex").read_text() == artifact["clvm_hex"] + "\n",
                "Modeled artifact metadata differs from retained bytes")
    roles = {"counterexample"} if negative else {"abi_nonvacuity", "acceptance_nonvacuity", "true_witness", "false_witness"}
    seen = []
    for replay in row["replays"]:
        witness = replay["witness"]
        require(replay["roles"] and all(role in roles and result[role] == witness for role in replay["roles"]), "Replay disconnected from actual solver witnesses")
        seen.extend(replay["roles"])
        desired = expected(witness)
        require(replay["expected_original"] == desired and check_native_outcome(replay["original"]["native"]) == desired, "Wrong native/original witness outcome")
        runs = replay["original"]["runs"]
        require([(r["backend"], r["optimize_lir"]) for r in runs] == [(a["backend"], a["optimize_lir"]) for a in corpus["artifacts"]]
                and all(check_execution(r["execution"]) == desired for r in runs), "Incomplete or false five-mode witness replay")
        wanted = replay["expected_modeled"]
        if name in CONTROL_IDS:
            require(wanted == control_expected(name, witness) and replay["native_control"] == wanted, "Wrong actual mutant native result")
        else:
            require(replay["native_control"] is None, "Synthetic IR cannot claim actual mutant native execution")
            if name == "original": require(wanted == desired, "Original modeled result changed")
            elif name == "fixed_length_narrowing": require(arguments(witness) is not None and any(len(x) != 2 for x in arguments(witness)) and wanted == {"status": "rejected"}, "Narrow-ABI witness changed")
            else:
                value = tree(witness["environment"])
                require(desired == {"status": "rejected"} and isinstance(value, tuple) and isinstance(value[0], bytes)
                        and isinstance(value[1], tuple) and isinstance(value[1][0], bytes)
                        and wanted == boolean_outcome(value[0] != value[1][0]), "Loose-ABI witness changed")
        check_replays(replay["modeled"], wanted)
        for execution in replay["modeled"]:
            mode = "on" if execution["optimize"] else "off"
            require(execution["artifact"] == f"{DIRECTORY}/{name}-{mode}.clvm.hex", "Modeled replay is disconnected from its retained artifact")
        require((wanted != desired) == negative, "Mutation control does not demonstrate the claimed outcome difference")
    require(set(seen) == roles and len(seen) == len(roles), "Solver witness roles missing/duplicated")
    if name == "original":
        for artifact, measured in zip(row["artifacts"], [a for a in corpus["artifacts"] if a["backend"] == "rust_mir"]):
            require(all(artifact[key] == measured[key] for key in ("bytes", "clvm_hex", "tree_hash"))
                    and artifact["optimize"] == measured["optimize_lir"], "Actual source IR artifacts differ from validated corpus")


def portable(report):
    result = copy.deepcopy(report)
    del result["solver"]
    return result


class ByteProofControls(unittest.TestCase):
    report_path = OUTPUT

    def test_current_record_and_report_corruptions(self):
        original = load_json(self.report_path)
        validate(original)
        mutations = ("scope", "source", "authoring", "lineage", "missing_model", "model_order", "missing_artifact",
                     "artifact_hash", "specification", "false_proof", "empty_acceptance", "false_nonvacuity",
                     "missing_replay", "roles", "query", "native", "source_native", "modeled_output",
                     "resource_rejection", "missing_mode", "artifact_substitution", "artifact_binding", "origin", "extraction",
                     "boolean_schema", "integer_mode", "retained_output", "retained_cost", "retained_failure")
        for mutation in mutations:
            bad = copy.deepcopy(original)
            model = bad["models"][1]
            replay = model["replays"][0]
            if mutation == "scope": bad["scope"]["kind"] = "compiler_preservation_proof"
            elif mutation == "source": bad["implementation_sources"][0]["sha256"] = "0" * 64
            elif mutation == "authoring": bad["authoring"]["sources"][0]["sha256"] = "0" * 64
            elif mutation == "lineage": bad["source_lineage"]["controls"][0]["patch"]["text"] += "\n"
            elif mutation == "missing_model": bad["models"].pop()
            elif mutation == "model_order": bad["models"].reverse()
            elif mutation == "missing_artifact": bad["artifacts"].pop()
            elif mutation == "artifact_hash": bad["artifacts"][0]["sha256"] = "0" * 64
            elif mutation == "specification": bad["specification"]["sha256"] = "0" * 64
            elif mutation == "false_proof": model["result"]["status"] = "proven"
            elif mutation == "empty_acceptance": model["result"]["acceptance_nonvacuity"] = None
            elif mutation == "false_nonvacuity": model["result"]["true_witness"] = model["result"]["false_witness"]
            elif mutation == "missing_replay": model["replays"] = []
            elif mutation == "roles": replay["roles"].append("counterexample")
            elif mutation == "query": model["queries"]["counterexample"] = "../unreviewed.smt2"
            elif mutation == "native":
                native = check_native_outcome(replay["original"]["native"])
                require(native["status"] == "accepted" and native["output_hex"] in ("80", "01"), "Native corruption needs a canonical Boolean")
                replay["original"]["native"] = boolean_outcome(native["output_hex"] == "80")
            elif mutation == "source_native": replay["native_control"] = None
            elif mutation == "modeled_output": replay["modeled"][0]["outcome"]["output_hex"] = "ff0180"
            elif mutation == "resource_rejection":
                replay["modeled"][0]["outcome"] = {"status": "rejected", "error": "cost exceeded", "failure": {"kind": "resource", "code": "cost_exceeded"}}
            elif mutation == "missing_mode": replay["original"]["runs"].pop()
            elif mutation == "artifact_substitution": model["artifacts"] = copy.deepcopy(bad["models"][2]["artifacts"])
            elif mutation == "artifact_binding": replay["modeled"][0]["artifact"] = bad["models"][2]["ir"]
            elif mutation == "origin": model["origin"] = "synthetic_abi_ir_control"
            elif mutation == "extraction": bad["extraction"]["compiler_driver_identity_checked"] = False
            elif mutation == "boolean_schema": bad["schema_version"] = True
            elif mutation == "integer_mode": replay["modeled"][0]["optimize"] = 0
            elif mutation == "retained_output": replay["modeled"][0]["retained_execution"]["outcome"]["output_hex"] = "ff0180"
            elif mutation == "retained_cost": replay["modeled"][0]["retained_execution"]["cost"] += 1
            else:
                row = bad["models"][-1]["replays"][0]["modeled"][0]
                previous = row["retained_execution"]["failure"]["code"]
                row["retained_execution"]["failure"] = {"kind": "program", "code": "raise" if previous != "raise" else "path_into_atom"}
            self.assertTrue(json.dumps(bad, sort_keys=True) != json.dumps(original, sort_keys=True),
                            f"Corruption control {mutation} must actually change the report, including JSON types")
            with self.subTest(mutation=mutation), self.assertRaises((ValueError, KeyError)):
                validate(bad)

    def test_corpus_source_and_five_mode_boundary(self):
        original = load_json(ROOT / "corpus/validation.json")
        checked_corpus(original)
        for mutation in ("empty_modes", "missing_mode", "duplicate_mode", "rust_source", "reference", "rue", "authoring", "coherent_wrong_output"):
            bad = copy.deepcopy(original)
            row = next(row for row in bad["puzzles"] if row["slug"] == "atom-not-equal")
            if mutation == "empty_modes": row["artifacts"] = []
            elif mutation == "missing_mode": row["artifacts"].pop()
            elif mutation == "duplicate_mode": row["artifacts"][-1] = copy.deepcopy(row["artifacts"][0])
            elif mutation == "rust_source": row["source"] += "\n"
            elif mutation == "reference": row["reference"] += "\n"
            elif mutation == "rue": row["rue"]["source"] += "\n"
            elif mutation == "authoring": bad["authoring"]["sources"][0]["sha256"] = "0" * 64
            else:
                observed = row["cases"][0]["native"]
                require(observed["status"] == "accepted" and observed["output_hex"] in ("80", "01"), "Corpus corruption needs a canonical Boolean")
                changed = boolean_outcome(observed["output_hex"] == "80")
                row["cases"][0]["native"] = changed
                for run in row["cases"][0]["runs"]: run["execution"]["outcome"] = changed
            self.assertTrue(json.dumps(bad, sort_keys=True) != json.dumps(original, sort_keys=True),
                            f"Corpus control {mutation} must actually change the report, including JSON types")
            with self.subTest(mutation=mutation), self.assertRaises((ValueError, KeyError)):
                checked_corpus(bad)

    def test_portable_comparison_omits_only_solver_version(self):
        original = load_json(self.report_path)
        other = copy.deepcopy(original)
        other["solver"] = "Z3 version another checked build"
        self.assertEqual(portable(original), portable(other))
        other["models"][0]["artifacts"][0]["bytes"] += 1
        self.assertNotEqual(portable(original), portable(other))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=OUTPUT)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--compare", type=Path, help="compare another freshly validated record; only the solver version may differ")
    args = parser.parse_args()
    try:
        if args.self_test:
            ByteProofControls.report_path = args.output
            result = unittest.TextTestRunner().run(unittest.defaultTestLoader.loadTestsFromTestCase(ByteProofControls))
            if not result.wasSuccessful():
                raise ValueError("Byte proof corruption controls failed")
            print("PASS:37 evidence/source/mode corruption controls; solver-version-only comparison control")
        elif args.compare:
            left, right = load_json(args.output), load_json(args.compare)
            validate(left)
            validate(right)
            require(portable(left) == portable(right), "Portable proof record changed beyond the solver version")
            print("PASS: exact portable model/query/artifact/witness/native/VM record; only solver version omitted")
        elif args.check:
            validate(load_json(args.output))
            print("PASS: current source/IR/specification/query/replay evidence (freshness only; Rust/Z3 not rerun)")
        else:
            generate(args.output)
    except (ValueError, KeyError, TypeError, OSError, subprocess.SubprocessError) as error:
        parser.exit(1, f"Byte proof check failed: {error}\n")


if __name__ == "__main__":
    main()
