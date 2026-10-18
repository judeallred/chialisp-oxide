"""Meaningful offline corruptions of independent password data and evidence.

Oracle mutants here are Python test controls, not compiled source mutations or
new theorem claims. Report mutations test the checker, not fabrication resistance.
"""
import copy
import json

from password_evidence import validate
from password_provenance import (LICENSE, PAGE, TREE_PATH, load, read_source,
                                 source_contract)
from password_vectors import expected, named, require
from runtime_password_migration import canonical, controls as migration_controls


def reject(action, label):
    try:
        action()
    except ValueError:
        return 1
    raise ValueError("Password corruption passed: " + label)


def self_test(report=None):
    source_contract()
    count = 0
    for option in ("normalize", "strict_tail", "require_conditions_list"):
        require(any(expected(row["solution"]) != expected(row["solution"], **{option: True}) for row in named()),
                "No distinguishing input for independent oracle mutant: " + option)
        count += 1
    # Independent golden outputs: nonempty00 is data; an improper condition
    # subtree is copied, and its atom/pair content is never executed.
    golden = {"arbitrary_condition_atom": "00", "conditions_nil": "80",
              "conditions_program_data": "ff0880", "conditions_improper": "ff0102",
              "ignored_tail_padded": "01", "password_bytes_3": "820001"}
    rows = {row["label"]: row for row in named()}
    for label, output in golden.items():
        require(expected(rows[label]["solution"]) == {"status": "accepted", "output_hex": output}, "Independent golden relation failed")
        count += 1
    for label in ("page", "license", "tree", "reference", "generated", "span", "adaptation", "dependency", "native_path", "classification", "published_artifact"):
        changes = {}
        manifest_path = "puzzles/password-coin/puzzle.json"
        manifest = load(read_source(manifest_path))
        if label in ("page", "license", "tree", "reference", "generated"):
            path = {"page": PAGE, "license": LICENSE, "tree": TREE_PATH,
                    "reference": "puzzles/password-coin/reference.clsp", "generated": "puzzles/password-coin/generated-cases.json"}[label]
            changes[path] = read_source(path) + b"\n"
        else:
            if label == "span": manifest["provenance"]["source_span"]["start_line"] += 1
            elif label == "adaptation": manifest["provenance"]["source_adaptation"]["kind"] = "wrapper"
            elif label == "dependency": manifest["source_dependencies"].append("unknown/extra.rs")
            elif label == "native_path": manifest["native_sources"].append("../outside.rs")
            elif label == "classification": manifest["provenance"]["classification"] = "complete_tutorial"
            else: manifest["provenance"]["upstream_artifact"] = {"tree_hash": "0" * 64}
            changes[manifest_path] = json.dumps(manifest).encode()
        reader = lambda path: changes[path] if path in changes else read_source(path)
        count += reject(lambda: source_contract(reader), "source/" + label)
    if report is None:
        return count
    validate(report)
    mutations = ("raw_output", "raw_cost", "raw_mode", "artifact_hex", "artifact_hash", "unknown_field",
                 "fixed_argument", "scheme", "source_role", "protocol", "instance_module", "instance_hex",
                 "instance_tail", "instance_output", "varying_password", "varying_argument", "varying_environment",
                 "varying_module", "missing_varying", "resource_budget", "resource_omitted", "resource_as_program",
                 "resource_as_infrastructure", "native_row", "missing_ledger", "ledger_mode", "umbrella_credit",
                 "wrong_reveal", "rollback", "child", "ledger_solution", "consensus_code")
    for mutation in mutations:
        bad = copy.deepcopy(report)
        puzzle = next(p for p in bad["puzzles"] if p["slug"] == "password-coin")
        instance = puzzle["curried_instances"]
        if mutation == "raw_output": puzzle["cases"][0]["native"]["output_hex"] = "80"
        elif mutation == "raw_cost": puzzle["cases"][0]["runs"][0]["execution"]["cost"] = 1_000_001
        elif mutation == "raw_mode": puzzle["cases"][0]["runs"].reverse()
        elif mutation == "artifact_hex": puzzle["artifacts"][0]["clvm_hex"] += "80"
        elif mutation == "artifact_hash": puzzle["artifacts"][0]["tree_hash"] = "0" * 64
        elif mutation == "unknown_field": puzzle["security_verified"] = True
        elif mutation == "fixed_argument": instance["fixed_argument"] = "80"
        elif mutation == "scheme": instance["scheme"] = "(a (q . M) (q . H))"
        elif mutation == "source_role": instance["source_role"] = "executed clvm-utils library"
        elif mutation == "protocol": instance["protocol"] = "unreviewed/v2"
        elif mutation == "instance_module": instance["artifacts"][0]["module_tree_hash"] = "0" * 64
        elif mutation == "instance_hex": instance["artifacts"][0]["artifact"]["clvm_hex"] += "80"
        elif mutation == "instance_tail": instance["cases"][0]["solution"] = "(0x616263)"
        elif mutation == "instance_output": instance["cases"][0]["native"]["output_hex"] = "80"
        elif mutation == "varying_password": instance["varying_arguments"][0]["password_hex"] = "00"
        elif mutation == "varying_argument": instance["varying_arguments"][0]["argument_hex"] = "80"
        elif mutation == "varying_environment": instance["varying_arguments"][0]["environment"] = "()"
        elif mutation == "varying_module": instance["varying_arguments"][0]["runs"][0]["module_tree_hash"] = "0" * 64
        elif mutation == "missing_varying": instance["varying_arguments"].pop()
        elif mutation == "resource_budget": puzzle["resource_controls"]["max_cost"] += 1
        elif mutation == "resource_omitted": puzzle["resource_controls"]["cases"].pop()
        elif mutation.startswith("resource_as_"):
            execution = puzzle["resource_controls"]["cases"][2]["runs"][0]["execution"]
            execution.update(outcome={"status": "rejected"}, cost=None, error="spoofed", failure={"kind": "program", "code": "raise"})
            if mutation == "resource_as_infrastructure":
                execution.update(outcome={"status": "inconclusive"}, failure={"kind": "infrastructure", "code": "internal_error"})
        elif mutation == "native_row": puzzle["native_sources"][0]["sha256"] = "0" * 64
        elif mutation == "missing_ledger": instance["ledger"].pop()
        elif mutation == "ledger_mode": instance["ledger"].reverse()
        elif mutation == "umbrella_credit": instance["ledger"][0]["attempts"][2]["consensus"]["semantic_rejection_credit"] = True
        elif mutation == "wrong_reveal": instance["ledger"][0]["attempts"][6]["reveal_hex"] = instance["ledger"][0]["attempts"][0]["reveal_hex"]
        elif mutation == "rollback": instance["ledger"][0]["attempts"][2]["after"]["spent_height"] = 1
        elif mutation == "child": instance["ledger"][0]["attempts"][0]["after"]["children"][0]["amount"] += 1
        elif mutation == "ledger_solution": instance["ledger"][0]["attempts"][6]["solution"] = "(0x616263 ())"
        else: instance["ledger"][0]["attempts"][3]["consensus"]["code"] = "GeneratorRuntimeError"
        require(canonical(bad) != canonical(report), "No-op password evidence mutation: " + mutation)
        count += reject(lambda: validate(bad), "evidence/" + mutation)
    # Python considers False==0 and True==1. Each must fail on the JSON wire,
    # rather than relying on ordinary dictionary inequality as a mutation guard.
    for numeric in (int, float):
        for field in ("artifact_mode", "case_mode", "fixed_mode", "varying_mode", "ledger_mode", "credit_false", "credit_true", "rollback", "generated", "budget", "attempts"):
            bad = copy.deepcopy(report)
            puzzle = next(p for p in bad["puzzles"] if p["slug"] == "password-coin")
            instance = puzzle["curried_instances"]
            if field == "artifact_mode": target, key = puzzle["artifacts"][0], "optimize_lir"
            elif field == "case_mode": target, key = puzzle["cases"][0]["runs"][0], "optimize_lir"
            elif field == "fixed_mode": target, key = instance["artifacts"][0]["artifact"], "optimize_lir"
            elif field == "varying_mode": target, key = instance["varying_arguments"][0]["runs"][0]["artifact"], "optimize_lir"
            elif field == "ledger_mode": target, key = instance["ledger"][0], "optimize_lir"
            elif field in ("credit_false", "credit_true"): target, key = instance["ledger"][0]["attempts"][2 if field == "credit_false" else 3]["consensus"], "semantic_rejection_credit"
            elif field == "rollback": target, key = instance["ledger"][0]["attempts"][2], "full_state_unchanged"
            elif field == "generated": target, key = puzzle, "generated_cases"
            elif field == "budget": target, key = puzzle["resource_controls"], "max_cost"
            else: target, key = instance["ledger"][0], "transaction_attempts"
            if type(target[key]) is int and numeric is int:
                continue  # int→int would be a genuine no-op, not a control.
            target[key] = numeric(target[key])
            require(canonical(bad) != canonical(report), "No-op wire-type mutation")
            count += reject(lambda: validate(bad), "wire/" + field)
    # Source/evidence corruptions above always use the actual current report.
    # Only the historical migration controls receive the normalized old21 view.
    from runtime_cost_migration import preserve as cost_preservation
    historical = cost_preservation(report)
    return count + migration_controls(historical, current_report=report)
