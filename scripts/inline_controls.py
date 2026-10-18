"""Nonvacuous corruption controls for the closed signed405 helper-inlining transition."""
import copy

from runtime_inline_migration import (canonical, load, preserve, project_validated, require, rust_on, predecessor)
from runtime_cost_migration import check_ledger_transition, coin_id


def rejected(action, label):
    try:
        action()
    except (ValueError, KeyError, TypeError, IndexError):
        return
    raise ValueError("Cost-profile corruption passed: " + label)


def self_test(report, *, projection=project_validated, current_validation=preserve,
              normalize_refinement=True, baseline_ledger=None):
    current_validation(report)
    from runtime_cost_migration import project_append
    report = project_append(report)
    from runtime_refinement_migration import is_refinement, project_validated as refinement_projection
    if normalize_refinement and is_refinement(report):
        report = refinement_projection(report)
    # Establish this historical stage's positive object before any mutation.
    projection(report)
    controls = 0
    for mutation in ("source", "reference", "rue", "dependency", "native", "authoring", "abi", "input", "native_output",
                     "off_artifact", "off_cost", "reference_artifact", "rue_cost", "on_artifact", "on_cost", "on_failure",
                     "resource_failure_kind", "resource_policy_credit", "ledger", "case_order", "fixture_order", "missing_fixture",
                     "unknown_fixture", "coverage", "pipeline_order", "unknown_metadata", "curried_argument", "curried_artifact",
                     "curried_native", "curried_cost", "varying_artifact", "ledger_coin", "ledger_credit", "ledger_state", "patch_history", "patch_omitted",
                     "numeric_on_mode", "numeric_off_mode", "float_on_mode", "missing_resource", "coherent_output"):
        bad = copy.deepcopy(report)
        first = bad["puzzles"][0]
        password = bad["puzzles"][-1]
        accepted = next(c for c in first["cases"] if c["native"]["status"] == "accepted")
        rejected_case = next(c for p in bad["puzzles"] for c in p["cases"] if c["native"]["status"] == "rejected")
        if mutation == "source": first["source"] += "\n"
        elif mutation == "reference": first["reference"] += "\n"
        elif mutation == "rue": first["rue"]["source"] += "\n"
        elif mutation == "dependency": first["source_dependencies"][0]["sha256"] = "0" * 64
        elif mutation == "native": password["native_sources"][0]["sha256"] = "0" * 64
        elif mutation == "authoring": bad["authoring"]["sources"][0]["sha256"] = "0" * 64
        elif mutation == "abi": first["environment"] = "ignored"
        elif mutation == "input": accepted["solution"] += " "
        elif mutation == "native_output": accepted["native"]["output_hex"] = "01"
        elif mutation == "off_artifact": first["artifacts"][0]["tree_hash"] = "0" * 64
        elif mutation == "off_cost": accepted["runs"][0]["execution"]["cost"] += 1
        elif mutation == "reference_artifact": first["artifacts"][2]["tree_hash"] = "0" * 64
        elif mutation == "rue_cost": accepted["runs"][3]["execution"]["cost"] += 1
        elif mutation == "on_artifact": rust_on(first["artifacts"])["tree_hash"] = "0" * 64
        elif mutation == "on_cost": rust_on(accepted["runs"])["execution"]["cost"] += 1
        elif mutation == "on_failure":
            execution = rust_on(rejected_case["runs"])["execution"]
            execution["failure"]["code"] = "path_into_atom" if execution["failure"]["code"] != "path_into_atom" else "raise"
        elif mutation.startswith("resource_"):
            execution = rust_on(password["resource_controls"]["cases"][0]["runs"])["execution"]
            if mutation == "resource_failure_kind": execution["failure"]["kind"] = "program"
            else: execution["outcome"]["status"] = "rejected"
        elif mutation == "ledger": first["simulator"][0]["scope"] += " changed"
        elif mutation == "case_order": password["cases"].reverse()
        elif mutation == "fixture_order": bad["puzzles"].reverse()
        elif mutation == "missing_fixture": bad["puzzles"].pop()
        elif mutation == "unknown_fixture": password["slug"] = "unreviewed-extra-port"
        elif mutation == "coverage": first["generated_cases"] += 1
        elif mutation == "pipeline_order": bad["optimization_pipeline"]["rust_mir"]["optimized"].reverse()
        elif mutation == "unknown_metadata": bad["unreviewed"] = True
        elif mutation == "curried_argument": password["curried_instances"]["fixed_argument"] = "a0" + "0" * 64
        elif mutation == "curried_artifact": password["curried_instances"]["artifacts"][1]["module_tree_hash"] = "0" * 64
        elif mutation == "curried_native": password["curried_instances"]["cases"][0]["native"] = {"status": "rejected"}
        elif mutation == "curried_cost": rust_on(password["curried_instances"]["cases"][0]["runs"])["execution"]["cost"] += 1
        elif mutation == "varying_artifact": password["curried_instances"]["varying_arguments"][0]["runs"][1]["module_tree_hash"] = "0" * 64
        elif mutation == "ledger_coin": rust_on(password["curried_instances"]["ledger"])["attempts"][0]["coin_id"] = "0" * 64
        elif mutation == "ledger_credit": rust_on(password["curried_instances"]["ledger"])["attempts"][2]["consensus"]["semantic_rejection_credit"] = True
        elif mutation == "ledger_state": rust_on(password["curried_instances"]["ledger"])["attempts"][2]["full_state_unchanged"] = False
        elif mutation == "patch_history": bad["rue_local_patches"][0] += "-unreviewed"
        elif mutation == "patch_omitted": bad["rue_local_patches"].pop()
        elif mutation == "numeric_on_mode": rust_on(first["artifacts"])["optimize_lir"] = 1
        elif mutation == "numeric_off_mode": first["artifacts"][0]["optimize_lir"] = 0
        elif mutation == "float_on_mode": rust_on(first["artifacts"])["optimize_lir"] = 1.0
        elif mutation == "missing_resource": password["resource_controls"]["cases"].pop()
        elif mutation == "coherent_output":
            accepted["native"]["output_hex"] = "01"
            for run in accepted["runs"]: run["execution"]["outcome"]["output_hex"] = "01"
        require(canonical(bad) != canonical(report), "No-op cost-profile corruption: " + mutation)
        # Exercise the projection itself as well as source checks. This prevents
        # whole-current freshness from masking a permissive normalization rule.
        rejected(lambda: projection(bad), mutation)
        controls += 1
    old = (predecessor()["reviewed_after"]["puzzles"][-1]["curried_instances"]["ledger"]
           if baseline_ledger is None else baseline_ledger)
    actual = copy.deepcopy(rust_on(report["puzzles"][-1]["curried_instances"]["ledger"]))
    for attempt in actual["attempts"]:
        attempt["coin_id"] = "11" * 32
        for child in attempt["after"]["children"]:
            child["parent_coin_id"] = attempt["coin_id"]
            child["coin_id"] = coin_id(attempt["coin_id"], child["puzzle_hash"])
    rejected(lambda: check_ledger_transition(actual, old), "coherent arbitrary starting coin IDs")
    controls += 1
    return controls
