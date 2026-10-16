#!/usr/bin/env python3
"""Prove real Rust-derived IR properties and replay mutation counterexamples.

Requires cargo build --workspace --locked first. This deliberately fails rather
than skipping if rustc, oxide-driver, oxide-verifier, or Z3 is unavailable.
"""

import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
PROOFS = ROOT / "proofs/bounded-counter"
SOURCE = Path("puzzles/bounded-counter/puzzle.rs")
DRIVER = ROOT / "target/debug/oxide-driver"
VERIFIER = ROOT / "target/debug/oxide-verifier"


def run(args, expected=0):
    result = subprocess.run(
        [str(arg) for arg in args], cwd=ROOT,
        text=True, capture_output=True, timeout=30, check=False,
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "corpus/proofs.json")
    args = parser.parse_args()
    for binary in [DRIVER, VERIFIER]:
        if not binary.is_file():
            raise SystemExit(f"Missing {binary}; run cargo build --workspace --locked")

    with tempfile.TemporaryDirectory(prefix="oxide-ir-proofs-") as temporary:
        temp = Path(temporary)
        program_path = temp / "program.ir.json"
        run([DRIVER, SOURCE, "--output", program_path])
        program = json.loads(program_path.read_text())
        expected_u64 = {"kind": "int", "bits": 64, "signed": False}
        if program["result"] != expected_u64 or [p["ty"] for p in program["parameters"]] != [expected_u64]:
            raise SystemExit("bounded-counter ABI changed; update independently authored specifications")

        checks = []
        for name in ["range", "acceptance"]:
            specification = PROOFS / f"{name}.spec.json"
            result = json.loads(run([VERIFIER, program_path, specification]))
            if result["status"] != "proven":
                raise SystemExit(f"{name} was not proven: {result}")
            queries = run([VERIFIER, program_path, specification, "--emit"])
            (PROOFS / f"{name}.smt2").write_text(queries.rstrip()+"\n")
            witness = temp / f"{name}.witness.json"
            write_json(witness, result["nonvacuity"])
            replay = json.loads(run([VERIFIER, program_path, "--replay", witness]))
            check_replays(replay, "accepted")
            checks.append({"name": name, "specification": str(specification.relative_to(ROOT)), "result": result, "clvm_nonvacuity_replay": replay})

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
        mutation = json.loads(run([VERIFIER, mutant_path, PROOFS / "range.spec.json"], expected=1))
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

        harness_source = temp / "native.rs"
        harness_source.write_text((ROOT / SOURCE).read_text() + r'''
fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let value = std::env::args().nth(1).unwrap().parse::<u64>().unwrap();
    match std::panic::catch_unwind(|| puzzle(value)) {
        Ok(output) => println!("{{\"status\":\"accepted\",\"value\":{output}}}"),
        Err(_) => println!("{{\"status\":\"rejected\"}}"),
    }
}
''')
        native = temp / "native"
        run(["rustc", "--edition=2024", "-C", "overflow-checks=yes", harness_source, "-o", native])
        native_counterexample = json.loads(run([native, value]))
        if native_counterexample["status"] != "rejected":
            raise SystemExit(f"Native source accepted the attack witness: {native_counterexample}")
        for check in checks:
            value = int(check["result"]["nonvacuity"]["arguments"][0]["value"])
            native_result = json.loads(run([native, value]))
            if native_result != {"status": "accepted", "value": value + 1}:
                raise SystemExit(f"Native nonvacuity mismatch: {native_result}")
            for row in check["clvm_nonvacuity_replay"]:
                if row["outcome"]["output_hex"] != encoded_positive_integer(native_result["value"]):
                    raise SystemExit(f"Native/CLVM nonvacuity output mismatch: {native_result}, {row}")
            check["native_nonvacuity_replay"] = native_result

        (PROOFS / "program.ir.json").write_text(program_path.read_text())
        (PROOFS / "mutant.ir.json").write_text(mutant_path.read_text())
        (PROOFS / "mutation.smt2").write_text(run([VERIFIER, mutant_path, PROOFS / "range.spec.json", "--emit"]).rstrip()+"\n")
        report = {
            "schema_version": 1,
            "scope": "IR-model proof over typed scalar values; source-derived IR, no compiler-preservation or ABI-decoder proof",
            "puzzle": "bounded-counter",
            "source": str(SOURCE),
            "source_sha256": sha256(ROOT / SOURCE),
            "ir": "proofs/bounded-counter/program.ir.json",
            "ir_sha256": sha256(program_path),
            "solver": run(["z3", "--version"]).strip(),
            "checks": checks,
            "mutation": {
                "description": "Remove only the outer counter < 1000 assertion; retain checked increment",
                "result": mutation,
                "original_clvm_replay": original_replay,
                "mutant_clvm_replay": mutant_replay,
                "original_native_replay": native_counterexample,
            },
            "limitations": [
                "Assumes successful ABI decoding of canonical typed values; byte semantics unsupported",
                "Does not prove rustc MIR extraction, SMT translation, Rue optimization, or CLVM lowering",
                "Witness execution is tested evidence, not compiler preservation",
                "Not a claim of complete puzzle security or spend-bundle validity",
            ],
        }
        output = args.output if args.output.is_absolute() else ROOT / args.output
        output.parent.mkdir(parents=True, exist_ok=True)
        write_json(output, report)
        print(f"PASS: source-derived range + acceptance; nonvacuity; guard mutation; native/CLVM witness replay -> {output}")


if __name__ == "__main__":
    main()
