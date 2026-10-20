#!/usr/bin/env python3
"""Read recorded costs, or explicitly execute all three pinned measurement APIs.

Current corpus admission runs first. Recorded checks independently reconstruct
uncompressed generators, decode backreferences and check byte charges. Actual VM,
BLS and resource controls receive execution credit only with --execute.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from cargo_authoring import unique_object
from raw_tree import deserialize, serialize, require
import runtime_cost_migration as current
from p2_delegated_conditions_evidence import coin, amount
from codec import decode, same_tree

MODES = [("rust_mir", False), ("rust_mir", True), ("chialisp", True),
         ("rue_source", False), ("rue_source", True)]
SCOPE = ("Pinned TESTNET11 conditions-helper and signature-verified block-generator measurements "
         "for exact separately admitted bundles. Backreference generators are executed, not merely sized. "
         "No coin-store admission, fee quote, mainnet/fork coverage, execution authentication, "
         "compiler preservation or security theorem is claimed. Unsafe uncurried key-selection "
         "successes remain intentional policy counterexamples.")


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()


def sha(value):
    return hashlib.sha256(value).hexdigest()


def load(path):
    return json.loads(path.read_bytes(), object_pairs_hook=unique_object)


def fields(value, names):
    require(type(value) is dict and set(value) == set(names), "unexpected/missing fields")


def integer(value):
    require(type(value) is int and value >= 0, "expected nonnegative integer")
    return value


def hex_bytes(value):
    require(type(value) is str, "expected hex string")
    result = bytes.fromhex(value)
    require(result.hex() == value and result, "noncanonical/empty hex")
    return result


def pointer(value, path):
    require(type(path) is str and path.startswith("/"), "expected JSON pointer")
    for part in path[1:].split("/"):
        part = part.replace("~1", "/").replace("~0", "~")
        value = value[int(part)] if type(value) is list else value[part]
    return value


def jobs(report):
    spec = load(HERE / "cases.json")
    fields(spec, ["schema_version", "ports"])
    require(type(spec["schema_version"]) is int and spec["schema_version"] == 1, "case schema")
    cases = []
    for port in spec["ports"]:
        fields(port, ["slug", "builds_pointer", "cases"])
        puzzles = [p for p in report["puzzles"] if p["slug"] == port["slug"]]
        require(len(puzzles) == 1, "ambiguous/missing measured port")
        builds = pointer(puzzles[0], port["builds_pointer"])
        require([(b["backend"], b["optimize_lir"]) for b in builds] == MODES, "five build order")
        for build in builds:
            require(type(build["optimize_lir"]) is bool, "optimization must be Boolean")
            for case in port["cases"]:
                fields(case, ["label", "transaction_pointer", "receipt_pointer", "receipt_status",
                              "previous_transaction_height"])
                require(case["receipt_status"] == "accepted"
                    and pointer(build, case["receipt_pointer"]) == "accepted", "bundle not admitted")
                transaction = pointer(build, case["transaction_pointer"])
                # Label/receipt identity is also checked by current admission.
                parent = case["receipt_pointer"].rsplit("/", 1)[0]
                receipt = pointer(build, parent)
                require(receipt.get("label", receipt.get("name")) == case["label"], "case label mismatch")
                cases.append({"slug": port["slug"], "backend": build["backend"],
                    "optimize_lir": build["optimize_lir"], "label": case["label"],
                    "previous_transaction_height": integer(case["previous_transaction_height"]),
                    "transaction": transaction})
    identities = [(c["slug"], c["backend"], c["optimize_lir"], c["label"]) for c in cases]
    require(len(cases) == len(set(identities)) and 0 < len(cases) <= 256, "duplicate/excessive jobs")
    return {"schema_version": 1, "cases": cases}


def source_window():
    paths = [HERE / name for name in ("Cargo.toml", "Cargo.lock", "cases.json")]
    paths += sorted(HERE.glob("*.py"))
    paths += sorted((HERE / "src").rglob("*.rs"))
    # The admission reader imports the repository's shared evidence modules.
    # Bind that complete bounded Python source directory, not just this entry point.
    paths += sorted((ROOT / "scripts").glob("*.py"))
    paths += [ROOT / name for name in ("rust-toolchain.toml", ".cargo/config.toml", "crates/chialisp-oxide-simulator/Cargo.toml")]
    paths += sorted((ROOT / "crates/chialisp-oxide-simulator/src").rglob("*.rs"))
    return [{"path": p.relative_to(ROOT).as_posix(), "sha256": sha(p.read_bytes())} for p in sorted(paths)]


def proper(*items):
    result = b""
    for item in reversed(items):
        result = item, result
    return result


def generator(transaction):
    items = []
    for spend in transaction["spends"]:
        record = coin(spend["coin"])
        items.append(proper(hex_bytes(record["parent_id"]), deserialize(hex_bytes(spend["reveal_hex"])),
                            amount(record["amount"]), deserialize(hex_bytes(spend["solution_hex"]))))
    return serialize((b"\x01", proper(proper(*reversed(items)))))


def metrics(value, charged, signature_verified, transaction, overhead):
    fields(value, ["total_cost", "execution_cost", "condition_cost", "byte_cost", "charged_bytes",
                   "signature_verified", "per_spend"])
    for name in ("total_cost", "execution_cost", "condition_cost", "byte_cost", "charged_bytes"):
        integer(value[name])
    require(value["signature_verified"] is signature_verified and value["charged_bytes"] == charged,
            "signature/byte-charge scope changed")
    spends = value["per_spend"]
    require(type(spends) is list and len(spends) == len(transaction["spends"]), "missing spend costs")
    for spend in spends:
        fields(spend, ["coin_id", "execution_cost", "condition_cost"])
        integer(spend["execution_cost"]); integer(spend["condition_cost"])
    require([s["coin_id"] for s in spends] == sorted(s["coin"]["id"] for s in transaction["spends"]),
            "spend coin identity mismatch")
    require(value["execution_cost"] == sum(s["execution_cost"] for s in spends) + overhead
        and value["condition_cost"] == sum(s["condition_cost"] for s in spends)
        and value["byte_cost"] == 12_000 * charged
        and value["total_cost"] == value["execution_cost"] + value["condition_cost"] + value["byte_cost"]
        and value["total_cost"] <= 5_500_000_000, "inconsistent cost arithmetic")


def validate_measurement(result, inputs):
    fields(result, ["schema_version", "cases"])
    require(type(result["schema_version"]) is int and result["schema_version"] == 1, "result schema")
    require(type(result["cases"]) is list and len(result["cases"]) == len(inputs["cases"]), "case count")
    controls = 0
    for row, job in zip(result["cases"], inputs["cases"], strict=True):
        fields(row, ["slug", "backend", "optimize_lir", "label", "costs"])
        require(type(row["optimize_lir"]) is bool
            and all(row[k] == job[k] for k in ("slug", "backend", "optimize_lir", "label")), "case identity")
        costs, transaction = row["costs"], job["transaction"]
        fields(costs, ["previous_transaction_height", "flags", "max_cost", "cost_per_byte", "helper",
                       "generators", "total_reveal_bytes", "total_solution_bytes"])
        for key in ("previous_transaction_height", "flags", "max_cost", "cost_per_byte", "total_reveal_bytes", "total_solution_bytes"):
            integer(costs[key])
        require(costs["previous_transaction_height"] == job["previous_transaction_height"]
            and costs["previous_transaction_height"] < 0xffff_ffff
            and costs["flags"] == 0xa_0006  # MEMPOOL_MODE; pre-HF2 TESTNET11.
            and costs["max_cost"] == 5_500_000_000 and costs["cost_per_byte"] == 12_000,
            "height/budget/byte-cost contract")
        require(costs["total_reveal_bytes"] == sum(len(hex_bytes(s["reveal_hex"])) for s in transaction["spends"])
            and costs["total_solution_bytes"] == sum(len(hex_bytes(s["solution_hex"])) for s in transaction["spends"]),
            "reveal/solution sizes")
        plain = generator(transaction)
        metrics(costs["helper"], len(plain)-2, False, transaction, 0)
        require(type(costs["generators"]) is list and len(costs["generators"]) == 2, "two encodings required")
        for value, encoding in zip(costs["generators"], ("uncompressed", "backrefs"), strict=True):
            fields(value, ["encoding", "generator_hex", "generator_bytes", "metrics", "signature_controls", "budget_control"])
            raw = hex_bytes(value["generator_hex"])
            require(value["encoding"] == encoding and integer(value["generator_bytes"]) == len(raw), "generator length")
            if encoding == "uncompressed": require(raw == plain, "generator differs from exact transaction")
            else: same_tree(decode(raw), deserialize(plain))
            metrics(value["metrics"], len(raw), True, transaction, 20)
            require(value["metrics"]["per_spend"] == costs["helper"]["per_spend"], "per-spend mismatch")
            identity = "c0" + "00"*95
            names = ["unrelated_nonempty"] + (["identity"] if transaction["signature_hex"] != identity else [])
            require(value["signature_controls"] == [{"signature": name, "code": "BadAggregateSignature"}
                for name in names], "missing/wrong signature control")
            require(value["budget_control"] == {"exact_cost_succeeded": True, "one_less_code": "CostExceeded"}
                and value["budget_control"]["exact_cost_succeeded"] is True, "missing typed resource control")
            controls += len(names)
    return {"bundles": len(inputs["cases"]), "generator_executions": 2*len(inputs["cases"]),
            "signature_rejection_controls": controls, "resource_budget_pairs": 2*len(inputs["cases"])}


def validate(record, report_bytes, inputs):
    fields(record, ["schema_version", "scope", "validation_report_sha256", "input_sha256", "source_window", "measurement"])
    require(type(record["schema_version"]) is int and record["schema_version"] == 1 and record["scope"] == SCOPE,
            "changed evidence scope")
    require(record["validation_report_sha256"] == sha(report_bytes)
        and record["input_sha256"] == sha(canonical(inputs))
        and record["source_window"] == source_window(), "stale input/source window")
    return validate_measurement(record["measurement"], inputs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, default=ROOT / "corpus/validation.json")
    parser.add_argument("--recorded", type=Path, default=HERE / "results.json")
    parser.add_argument("--execute", type=Path, help="built measurement binary; explicitly rerun all costs and BLS controls")
    parser.add_argument("--output", type=Path, help="new execution directory; prior runs are never overwritten")
    parser.add_argument("--record", action="store_true", help="install newly executed results after validation")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    report_bytes = args.report.read_bytes()
    report = json.loads(report_bytes, object_pairs_hook=unique_object)
    current.validate_current(report)  # Actual current admission BEFORE any selected view.
    inputs = jobs(report)
    if args.execute:
        require(args.output is not None, "execution requires new --output directory")
        args.output.mkdir(parents=True, exist_ok=False)
        runner = args.execute.resolve(strict=True)
        before, window = sha(runner.read_bytes()), source_window()
        input_path, output_path = args.output.resolve() / "input.json", args.output.resolve() / "measurement.json"
        input_path.write_bytes(canonical(inputs))
        result = subprocess.run([str(runner), str(input_path), str(output_path)], cwd=ROOT,
                                text=True, capture_output=True, timeout=180)
        (args.output / "stdout.log").write_text(result.stdout)
        (args.output / "stderr.log").write_text(result.stderr)
        require(result.returncode == 0 and not result.stderr, "measurement runner failed; inspect retained logs")
        require(before == sha(runner.read_bytes()) and window == source_window()
            and args.report.read_bytes() == report_bytes and input_path.read_bytes() == canonical(inputs),
            "measurement source/tool/input window changed")
        record = {"schema_version": 1, "scope": SCOPE, "validation_report_sha256": sha(report_bytes),
                  "input_sha256": sha(canonical(inputs)), "source_window": window, "measurement": load(output_path)}
        summary = validate(record, report_bytes, inputs)
        (args.output / "results.json").write_text(json.dumps(record, indent=2)+"\n")
        (args.output / "execution.json").write_text(json.dumps({"runner_sha256": before, "summary": summary}, indent=2)+"\n")
        if args.record: args.recorded.write_text(json.dumps(record, indent=2)+"\n")
        else: require(canonical(record) == canonical(load(args.recorded)), "execution differs from recorded results")
        print("PASS actual pinned API execution:", summary)
    else:
        require(args.output is None and not args.record, "--output/--record require --execute")
        summary = validate(load(args.recorded), report_bytes, inputs)
        print("PASS recorded consistency only:", summary)
    if args.self_test:
        import unittest
        suite = unittest.defaultTestLoader.discover(str(HERE), pattern="test_*.py")
        require(suite.countTestCases() > 0, "missing corruption controls")
        require(unittest.TextTestRunner().run(suite).wasSuccessful(), "corruption controls failed")


if __name__ == "__main__":
    main()
