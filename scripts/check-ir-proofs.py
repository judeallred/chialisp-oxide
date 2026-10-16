#!/usr/bin/env python3
"""Prove real Rust-derived IR properties and replay mutation counterexamples.

Requires cargo build --workspace --locked first. Uses the actual Cargo puzzle
library and its annotated export; no generated policy wrapper or copied Rust
policy is compiled. Fails rather than skipping when required tools are absent.
"""

import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

from cargo_authoring import contained_file, load_json, snapshot


ROOT = Path(__file__).resolve().parents[1]
PROOFS = ROOT / "proofs/bounded-counter"
SOURCE = Path("puzzles/bounded-counter/puzzle.rs")
DRIVER = ROOT / "target/debug/chialisp-oxide-driver"
CARGO_OXIDE = ROOT / "target/debug/cargo-chialisp-oxide"
NATIVE = ROOT / "target/debug/chialisp-oxide"
VERIFIER = ROOT / "target/debug/chialisp-oxide-verifier"


def run(args, expected=0, timeout=30):
    result = subprocess.run(
        [str(arg) for arg in args], cwd=ROOT,
        text=True, capture_output=True, timeout=timeout, check=False,
    )
    if result.returncode != expected:
        raise SystemExit(
            f"Command failed ({result.returncode}, expected {expected}): {args}\n"
            f"{result.stdout}\n{result.stderr}"
        )
    return result.stdout


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def check_replays(replays, expected):
    if len(replays) != 2 or {row["optimize"] for row in replays} != {False, True}:
        raise SystemExit(f"Expected both optimization settings: {replays}")
    for row in replays:
        if row["outcome"]["status"] != expected:
            raise SystemExit(f"Unexpected CLVM replay: {row}, expected {expected}")
        if expected == "rejected" and not row["outcome"].get("error", "").startswith("Raise("):
            raise SystemExit(f"Typed original witness must hit its assertion, not another VM failure: {row}")


def encoded_positive_integer(value):
    """Independent canonical CLVM integer atom serialization for this u64 ABI."""
    if value == 0:
        return "80"
    payload = value.to_bytes((value.bit_length() + 8) // 8, "big")
    prefix = bytes([0x80 + len(payload)]) if len(payload) != 1 or payload[0] > 0x7f else b""
    return (prefix + payload).hex()


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def authoring_snapshot():
    authoring = snapshot(ROOT)
    selected = authoring["exports"]["bounded-counter"]
    if selected["source"] != str(SOURCE):
        raise SystemExit("Proof entry source changed; review the independently authored specification")
    return {**authoring, "entry": selected["entry"]}


def cargo_program(output, authoring):
    run([CARGO_OXIDE, "build", "--manifest-path", ROOT / authoring["manifest"],
         "--package", authoring["package"], "--locked", "--output-dir", output], timeout=180)
    return load_cargo_program(output, authoring)


def load_cargo_program(output, authoring):
    pointer = load_json(contained_file(output, "latest.json"))
    bundle = pointer["bundle"]
    if len(Path(bundle).parts) != 1 or pointer["manifest"] != f"{bundle}/manifest.json":
        raise SystemExit("Malformed atomic Cargo bundle pointer")
    manifest_path = contained_file(output, pointer["manifest"])
    manifest = load_json(manifest_path)
    if not (
        type(manifest.get("schema_version")) is int and manifest["schema_version"] == 1
        and manifest["package"] == authoring["package"]
        and Path(manifest["manifest_path"]).resolve() == (ROOT / authoring["manifest"]).resolve()
    ):
        raise SystemExit("Cargo bundle does not describe the requested puzzle library")
    entries = manifest["entries"]
    names = [entry["entry"] for entry in entries]
    expected = {export["entry"] for export in authoring["exports"].values()}
    if len(names) != len(set(names)) or set(names) != expected:
        raise SystemExit("Cargo bundle exports differ from declared authoring entries")
    entry = next(entry for entry in entries if entry["entry"] == authoring["entry"])
    path = contained_file(manifest_path.parent, entry["ir"])
    program = load_json(path)
    if set(program) != {"name", "parameters", "result", "body"}:
        raise SystemExit("bounded-counter proof requires the legacy strict-environment IR; refusing an envelope or changed ABI")
    if program["name"] != authoring["entry"]:
        raise SystemExit("Cargo-exported IR name does not match selected proof entry")
    return path, manifest


def native_replay(value):
    outcome = json.loads(run([NATIVE, "native", "bounded-counter", "--solution", f"({value})"]))
    if outcome == {"status": "rejected"}:
        return outcome
    expected = {"status": "accepted", "output_hex": encoded_positive_integer(value + 1)}
    if outcome != expected:
        raise SystemExit(f"Native Cargo puzzle returned unexpected canonical output: {outcome}")
    return {"status": "accepted", "value": value + 1}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "corpus/proofs.json")
    args = parser.parse_args()
    for binary in [CARGO_OXIDE, DRIVER, VERIFIER, NATIVE]:
        if not binary.is_file():
            raise SystemExit(f"Missing {binary}; run cargo build --workspace --locked")

    with tempfile.TemporaryDirectory(prefix="chialisp-oxide-proofs-") as temporary:
        temp = Path(temporary)
        authoring = authoring_snapshot()
        # Refresh the native CLI's Cargo-library dependency before witness replay.
        run(["cargo", "build", "--locked", "--target-dir", ROOT / "target", "-p", "chialisp-oxide-cli"], timeout=180)
        cargo_path, cargo_manifest = cargo_program(temp / "cargo-artifacts", authoring)
        program_path = temp / "program.ir.json"
        program_path.write_bytes(cargo_path.read_bytes())
        program = json.loads(program_path.read_text())
        expected_u64 = {"kind": "int", "bits": 64, "signed": False}
        if program["result"] != expected_u64 or [p["ty"] for p in program["parameters"]] != [expected_u64]:
            raise SystemExit("bounded-counter ABI changed; update independently authored specifications")

        specifications = {}
        for name in ["range", "acceptance"]:
            path = temp / f"{name}.spec.json"
            path.write_bytes((PROOFS / f"{name}.spec.json").read_bytes())
            specifications[name] = path
        checks = []
        for name in ["range", "acceptance"]:
            specification = PROOFS / f"{name}.spec.json"
            result = json.loads(run([VERIFIER, program_path, specifications[name]]))
            if result["status"] != "proven":
                raise SystemExit(f"{name} was not proven: {result}")
            queries = run([VERIFIER, program_path, specifications[name], "--emit"])
            query_path = temp / f"{name}.smt2"
            query_path.write_text(queries.rstrip()+"\n")
            witness = temp / f"{name}.witness.json"
            write_json(witness, result["nonvacuity"])
            replay = json.loads(run([VERIFIER, program_path, "--replay", witness]))
            check_replays(replay, "accepted")
            checks.append({"name": name, "specification": str(specification.relative_to(ROOT)), "specification_sha256": sha256(specifications[name]), "query": str((PROOFS / f"{name}.smt2").relative_to(ROOT)), "query_sha256": sha256(query_path), "result": result, "clvm_nonvacuity_replay": replay})

        # Fail if rustc's assertion structure changes. Remove precisely the cap
        # guard while retaining the entire checked-arithmetic continuation.
        body = program["body"]
        condition = body.get("condition", {})
        cap = condition.get("left", {})
        if not (
            body.get("op") == "if"
            and body.get("then_value", {}).get("op") == "reject"
            and condition.get("op") == "binary" and condition.get("kind") == "eq"
            and condition.get("right") == {"op": "bool", "value": False}
            and cap.get("op") == "binary" and cap.get("kind") == "lt"
            and cap.get("left") == {"op": "arg", "index": 0}
            and cap.get("right") == {"op": "int", "value": "1000", "ty": expected_u64}
            and cap.get("ty") == expected_u64
        ):
            raise SystemExit("Assertion IR shape changed; refusing to mutate an unrecognized guard")
        mutant = copy.deepcopy(program)
        mutant["body"] = copy.deepcopy(body["else_value"])
        mutant_path = temp / "mutant.ir.json"
        write_json(mutant_path, mutant)
        mutation = json.loads(run([VERIFIER, mutant_path, specifications["range"]], expected=1))
        if mutation["status"] != "refuted":
            raise SystemExit(f"Guard removal was not detected: {mutation}")
        value = int(mutation["counterexample"]["arguments"][0]["value"])
        if not (1000 <= value < 2**64 - 1):
            raise SystemExit(f"Mutation witness does not violate cap with defined increment: {value}")
        witness_path = temp / "mutation.witness.json"
        write_json(witness_path, mutation["counterexample"])
        original_replay = json.loads(run([VERIFIER, program_path, "--replay", witness_path]))
        mutant_replay = json.loads(run([VERIFIER, mutant_path, "--replay", witness_path]))
        check_replays(original_replay, "rejected")
        check_replays(mutant_replay, "accepted")
        # Full serialization is checked by the VM tests; compare the actual
        # atom payload too so a mutant acceptance with the wrong value fails.
        expected_hex = encoded_positive_integer(value + 1)
        for row in mutant_replay:
            if row["outcome"]["output_hex"] != expected_hex:
                raise SystemExit(f"Counterexample output mismatch: {row}")

        native_counterexample = native_replay(value)
        if native_counterexample["status"] != "rejected":
            raise SystemExit(f"Native source accepted the attack witness: {native_counterexample}")
        for check in checks:
            value = int(check["result"]["nonvacuity"]["arguments"][0]["value"])
            native_result = native_replay(value)
            if native_result != {"status": "accepted", "value": value + 1}:
                raise SystemExit(f"Native nonvacuity mismatch: {native_result}")
            for row in check["clvm_nonvacuity_replay"]:
                if row["outcome"]["output_hex"] != encoded_positive_integer(native_result["value"]):
                    raise SystemExit(f"Native/CLVM nonvacuity output mismatch: {native_result}, {row}")
            check["native_nonvacuity_replay"] = native_result

        mutation_query = temp / "mutation.smt2"
        mutation_query.write_text(run([VERIFIER, mutant_path, specifications["range"], "--emit"]).rstrip()+"\n")
        if authoring_snapshot() != authoring:
            raise SystemExit("Declared Cargo authoring inputs changed during proof extraction/replay")
        for name, snapshot in specifications.items():
            if snapshot.read_bytes() != (PROOFS / f"{name}.spec.json").read_bytes():
                raise SystemExit("An independently authored specification changed during verification")
        for filename in ["program.ir.json", "mutant.ir.json", "range.smt2", "acceptance.smt2", "mutation.smt2"]:
            (PROOFS / filename).write_bytes((temp / filename).read_bytes())
        report = {
            "schema_version": 1,
            "scope": "IR-model proof over typed scalar values; source-derived IR, no compiler-preservation or ABI-decoder proof",
            "puzzle": "bounded-counter",
            "authoring": authoring,
            "toolchain": cargo_manifest["compiler"].splitlines()[0],
            "rue_revision": cargo_manifest["rue_revision"],
            "source": str(SOURCE),
            "source_sha256": sha256(ROOT / SOURCE),
            "ir": "proofs/bounded-counter/program.ir.json",
            "ir_sha256": sha256(program_path),
            "mutant_ir": "proofs/bounded-counter/mutant.ir.json",
            "mutant_ir_sha256": sha256(mutant_path),
            "solver": run(["z3", "--version"]).strip(),
            "checks": checks,
            "mutation": {
                "description": "Remove only the outer counter < 1000 assertion; retain checked increment",
                "specification": "proofs/bounded-counter/range.spec.json",
                "specification_sha256": sha256(specifications["range"]),
                "query": "proofs/bounded-counter/mutation.smt2",
                "query_sha256": sha256(PROOFS / "mutation.smt2"),
                "result": mutation,
                "original_clvm_replay": original_replay,
                "mutant_clvm_replay": mutant_replay,
                "original_native_replay": native_counterexample,
            },
            "limitations": [
                "Assumes successful ABI decoding of canonical typed values; byte semantics unsupported",
                "Does not prove rustc MIR extraction, SMT translation, Rue optimization, or CLVM lowering",
                "Witness execution is tested evidence, not compiler preservation",
                "Authoring hashes cover declared Cargo inputs, not the complete effective build environment",
                "Not a claim of complete puzzle security or spend-bundle validity",
            ],
        }
        output = args.output if args.output.is_absolute() else ROOT / args.output
        output.parent.mkdir(parents=True, exist_ok=True)
        write_json(output, report)
        print(f"PASS: Cargo-derived range + acceptance; nonvacuity; guard mutation; native/CLVM witness replay -> {output}")


if __name__ == "__main__":
    main()
