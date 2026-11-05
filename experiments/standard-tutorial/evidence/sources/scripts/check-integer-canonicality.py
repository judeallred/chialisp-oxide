#!/usr/bin/env python3
"""Execute the fixed byte-sequence canonicality law and retain a deterministic report.

This is a mathematical model over arbitrary finite Seq8 values. It does not
prove CLVM opcode semantics, exact ABI decoding, lowering/compiler preservation,
integer-width admission, execution cost, or the native/VM test bridge. Decoder
source hashes are metadata-only freshness bindings. Installed Z3 is assumed.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
MODEL = "proofs/integer-canonicality.smt2"
CHECKER = "scripts/check-integer-canonicality.py"
DECODER_SOURCES = ("crates/chialisp-oxide-ir/src/lower.rs", "crates/chialisp-oxide-ir/tests/integer_decoder.rs")
MODEL_SHA = "68cca1a7f136b1e6ff6bf3e2da89e2cfc1e12f5c2086fc8671e240fc6cf65ef1"
PROTOCOL = "chialisp-oxide-integer-canonicality/v1"
Z3_VERSION = "Z3 version 4.15.4 - 64 bit"
TIMEOUT = 20
SCOPE = {
    "domain": "all arbitrary finite sequences of unsigned 8-bit bytes",
    "law": "minimal signed sign extension iff nil or unsigned lexicographic bytes in [0080, ff80)",
    "method": "Z3 Seq8 model: negated equivalence UNSAT, explicit positive/negative witnesses and bound mutations",
    "not_proven": ["actual CLVM opcode semantics", "exact environment ABI", "integer-width admission",
                   "compiler or optimizer preservation", "VM execution cost", "native/VM execution bridge"],
    "decoder_source_binding": "metadata freshness only; source is not translated into this model",
    "solver_assumption": "the installed version-matching Z3 executable is trusted; no binary authentication claim",
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(payload):
    return hashlib.sha256(payload).hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "Duplicate proof report JSON key: " + key)
        result[key] = value
    return result


def source_rows():
    return [{"path": name, "sha256": sha((ROOT / name).read_bytes())}
            for name in (MODEL, CHECKER, *DECODER_SOURCES)]


def model_text():
    payload = (ROOT / MODEL).read_bytes()
    require(sha(payload) == MODEL_SHA, "Integer-canonicality model changed; review the fixed law and update its pin")
    return payload.decode("ascii")


def atom(hex_value):
    payload = bytes.fromhex(hex_value)
    if not payload:
        return "(as seq.empty Bytes)"
    units = [f"(seq.unit #x{value:02x})" for value in payload]
    return units[0] if len(units) == 1 else "(seq.++ " + " ".join(units) + ")"


def fixed_queries():
    queries = [("equivalence", "unsat", ["(not (= minimal interval))"])]
    for label, value in (("nil", ""), ("positive-128", "0080"), ("negative-129", "ff7f")):
        queries.append(("valid-" + label, "sat", [f"(= value {atom(value)})", "minimal", "interval"]))
    for value in ("00", "0001", "ffff", "ff80", "007f00"):
        binding = f"(= value {atom(value)})"
        queries.append(("reject-" + value, "unsat", [binding, "interval"]))
        queries.append(("invalid-domain-" + value, "sat", [binding, "(not minimal)", "(not interval)"]))
    return queries


def query_payload(model, queries):
    pieces = [model]
    for label, _, assertions in queries:
        require(label.replace("-", "").isalnum(), "Unsafe solver query label")
        pieces.extend([f'(echo "{label}")\n', "(push)\n"])
        pieces.extend(f"(assert {assertion})\n" for assertion in assertions)
        pieces.extend(["(check-sat)\n", "(pop)\n"])
    return "".join(pieces)


def check_response(result, queries):
    require(type(result.returncode) is int and result.returncode == 0, "Z3 returned a nonzero/invalid exit status")
    require(isinstance(result.stdout, str) and isinstance(result.stderr, str)
            and len(result.stdout.encode()) <= 8192 and not result.stderr,
            "Z3 output is malformed, oversized, or has stderr")
    expected = [value for label, status, _ in queries for value in (label, status)]
    require(result.stdout.splitlines() == expected,
            "Z3 query protocol/status mismatch (unknown, error, missing/extra/reordered output are inconclusive)")
    return [{"name": label, "expected_status": status, "observed_status": status} for label, status, _ in queries]


def call_z3(args, *, payload=None, runner=None):
    runner = subprocess.run if runner is None else runner
    try:
        return runner(args, input=payload, text=True, capture_output=True, timeout=TIMEOUT, check=False)
    except (subprocess.TimeoutExpired, OSError) as error:
        raise ValueError("Z3 execution is inconclusive: " + type(error).__name__) from error


def check_version(z3):
    result = call_z3([str(z3), "--version"])
    require(type(result.returncode) is int and result.returncode == 0 and not result.stderr
            and result.stdout.splitlines() == [Z3_VERSION], "Installed solver is not the pinned Z3 4.15.4 protocol")
    return Z3_VERSION


def execute(z3, model, queries):
    payload = query_payload(model, queries)
    result = call_z3([str(z3), "-in", "-smt2"], payload=payload)
    rows = check_response(result, queries)
    return {"smt2": payload, "smt2_sha256": sha(payload.encode()), "queries": rows}


def mutate_model(model, name):
    if name == "lower-007f":
        before, after = "(not (less_than_two value #x00 #x80))", "(not (less_than_two value #x00 #x7f))"
    elif name == "upper-ff81":
        before, after = "(less_than_two value #xff #x80)", "(less_than_two value #xff #x81)"
    elif name == "drop-nil":
        before = "(define-fun interval () Bool\n  (or (= (seq.len value) 0)\n      (and (not (less_than_two value #x00 #x80))\n           (less_than_two value #xff #x80))))"
        after = "(define-fun interval () Bool\n  (and (not (less_than_two value #x00 #x80))\n       (less_than_two value #xff #x80)))"
    else:
        raise ValueError("Unknown canonicality model mutation")
    require(model.count(before) == 1, "Mutation target is missing or ambiguous: " + name)
    mutated = model.replace(before, after)
    require(mutated != model, "No-op canonicality model mutation")
    return mutated


def mutation_checks(z3, model):
    results = []
    for name, value, assertions in (
        ("lower-007f", "007f00", ["(not minimal)", "interval"]),
        ("upper-ff81", "ff80", ["(not minimal)", "interval"]),
        ("drop-nil", "", ["minimal", "(not interval)"]),
    ):
        witness = [f"(= value {atom(value)})", *assertions]
        # Run the same distinguishing assertion on the original model first.
        # A changed source/pipeline cannot make every corruption vacuous.
        before = execute(z3, model, [(name + "-before", "unsat", witness)])
        mutated = mutate_model(model, name)
        after = execute(z3, mutated, [(name + "-after", "sat", witness),
                                      (name + "-inequivalent", "sat", ["(not (= minimal interval))"])])
        results.append({"name": name, "witness_hex": value, "original_query": before,
                        "mutated_model_sha256": sha(mutated.encode()), "mutated_query": after,
                        "scope": "actual SMT model-source mutation; not a Rust/CLVM compiler mutation"})
    return results


def protocol_controls():
    query = [("positive-control", "unsat", [])]
    positive = subprocess.CompletedProcess([], 0, "positive-control\nunsat\n", "")
    check_response(positive, query)
    variants = [
        subprocess.CompletedProcess([], code, positive.stdout, "") for code in (1, -9, True, 0.0)
    ]
    variants += [subprocess.CompletedProcess([], 0, stdout, "") for stdout in (
        "positive-control\nunknown\n", "positive-control\nsat\n", "unsat\n", "", "positive-control\n",
        "positive-control\nunsat\nunsat\n", "positive-control\nunsat\n(error fake)\n",
        "positive-control\nunsat\npositive-control\nunsat\n", "unsat\npositive-control\n",
        "positive-control\nunsat\nnoise\n", "x" * 8193)]
    variants += [subprocess.CompletedProcess([], 0, positive.stdout, stderr) for stderr in ("error", "\n")]
    for variant in variants:
        try:
            check_response(variant, query)
        except ValueError:
            continue
        raise ValueError("A nonvacuous solver protocol corruption passed")
    for error in (subprocess.TimeoutExpired("z3", TIMEOUT), OSError("missing solver")):
        def fail(*args, **kwargs):
            raise error
        try:
            call_z3(["z3"], runner=fail)
        except ValueError:
            continue
        raise ValueError("A solver execution failure was credited as proof")
    return len(variants) + 2


def produce(z3="z3"):
    before = source_rows()
    model = model_text()
    version = check_version(z3)
    baseline = execute(z3, model, fixed_queries())
    mutations = mutation_checks(z3, model)
    controls = protocol_controls()
    require(source_rows() == before, "Declared canonicality model/checker/decoder inputs changed during solver checks")
    return {"schema_version": 1, "protocol": PROTOCOL, "scope": copy.deepcopy(SCOPE),
            "implementation_sources": before[:2], "decoder_sources": before[2:],
            "solver": {"name": "Z3", "version": version}, "baseline": baseline,
            "model_source_mutations": mutations, "solver_protocol_controls": controls}


def check_recorded_sources(path):
    """Offline transcript/source validation; no new solver execution credit."""
    value = json.loads(path.read_bytes(), object_pairs_hook=unique_object)
    require(set(value) == {"schema_version", "protocol", "scope", "implementation_sources", "decoder_sources",
                           "solver", "baseline", "model_source_mutations", "solver_protocol_controls"},
            "Unexpected canonicality report fields")
    require(type(value["schema_version"]) is int and value["schema_version"] == 1
            and value["protocol"] == PROTOCOL and value["scope"] == SCOPE,
            "Changed canonicality report protocol/scope")
    sources = source_rows()
    require(value["implementation_sources"] == sources[:2] and value["decoder_sources"] == sources[2:],
            "Stale canonicality model/checker/decoder source bindings")
    require(value["solver"] == {"name": "Z3", "version": Z3_VERSION}, "Changed recorded canonicality solver")
    model = model_text()

    def transcript(record, source, queries):
        payload = query_payload(source, queries)
        expected = {"smt2": payload, "smt2_sha256": sha(payload.encode()),
                    "queries": [{"name": label, "expected_status": status, "observed_status": status}
                                for label, status, _ in queries]}
        require(canonical(record) == canonical(expected), "Changed recorded canonicality query/transcript/status")

    transcript(value["baseline"], model, fixed_queries())
    mutations = value["model_source_mutations"]
    require(isinstance(mutations, list) and len(mutations) == 3, "Missing/extra canonicality model mutations")
    for record, (name, atom_hex, assertions) in zip(mutations, (
        ("lower-007f", "007f00", ["(not minimal)", "interval"]),
        ("upper-ff81", "ff80", ["(not minimal)", "interval"]),
        ("drop-nil", "", ["minimal", "(not interval)"]),
    )):
        require(set(record) == {"name", "witness_hex", "original_query", "mutated_model_sha256", "mutated_query", "scope"}
                and record["name"] == name and record["witness_hex"] == atom_hex
                and record["scope"] == "actual SMT model-source mutation; not a Rust/CLVM compiler mutation",
                "Changed recorded canonicality mutation identity/scope")
        witness = [f"(= value {atom(atom_hex)})", *assertions]
        transcript(record["original_query"], model, [(name + "-before", "unsat", witness)])
        mutated = mutate_model(model, name)
        require(record["mutated_model_sha256"] == sha(mutated.encode()), "Changed recorded mutant source hash")
        transcript(record["mutated_query"], mutated,
                   [(name + "-after", "sat", witness),
                    (name + "-inequivalent", "sat", ["(not (= minimal interval))"])])
    require(type(value["solver_protocol_controls"]) is int
            and value["solver_protocol_controls"] == protocol_controls(), "Changed recorded solver control count")
    return value


def check_recorded(path, expected):
    value = json.loads(path.read_bytes(), object_pairs_hook=unique_object)
    require(canonical(value) == canonical(expected), "Stale or corrupted integer-canonicality proof report")
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "corpus/integer-canonicality-proof.json")
    parser.add_argument("--check", action="store_true", help="execute the law again and require exact report/source freshness")
    parser.add_argument("--self-test", action="store_true", help="announce the always-executed protocol and model mutation controls")
    parser.add_argument("--check-recorded", action="store_true", help="offline source/transcript freshness only; does not execute Z3")
    parser.add_argument("--z3", default="z3", help="installed trusted Z3 executable (version checked, not authenticated)")
    args = parser.parse_args()
    require(not (args.check and args.check_recorded), "Choose actual solver recheck or offline recorded check")
    if args.check_recorded:
        check_recorded_sources(args.output)
        print("PASS: recorded canonicality source/query/status freshness; no solver execution or new proof credit")
        return
    report = produce(args.z3)
    if args.check:
        check_recorded(args.output, report)
    else:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2, sort_keys=True, ensure_ascii=False) + "\n")
    print("PASS: Seq8 canonicality equivalence; 3 valid witnesses; 5 rejected-padding witnesses; 3 nonvacuous model-source mutations")
    if args.self_test:
        print("PASS:", report["solver_protocol_controls"], "fail-closed solver protocol controls; no compiler/opcode/ABI/cost theorem")


if __name__ == "__main__":
    main()
