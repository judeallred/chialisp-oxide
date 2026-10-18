"""Nonvacuous corruption controls for the closed21 optimization transition."""
import copy

from runtime_cost_migration import (canonical, check_ledger_transition, coin_id,
                                    load, preserve, project_validated, require,
                                    rust_on)


def rejected(action, label):
    try:
        action()
    except (ValueError, KeyError, TypeError, IndexError):
        return
    raise ValueError("Cost-profile corruption passed: " + label)


def self_test(report):
    preserve(report)
    from runtime_inline_migration import is_inline, project_validated as inline_projection
    from runtime_refinement_migration import is_refinement
    if is_inline(report) or is_refinement(report):
        # Public preserve above validates current evidence before this pure
        # historical boundary. The outer layer has its own direct mutants.
        report = inline_projection(report)
    # The historical projector must accept this positive object before any
    # mutant is challenged, so a new pipeline cannot make all mutants vacuous.
    project_validated(report)
    controls = 0
    for mutation in ("source", "reference", "rue", "dependency", "native", "authoring", "abi", "input", "native_output",
                     "off_artifact", "off_cost", "reference_artifact", "rue_cost", "on_artifact", "on_cost", "on_failure",
                     "resource_failure_kind", "resource_policy_credit", "ledger", "case_order", "fixture_order", "missing_fixture",
                     "unknown_fixture", "coverage", "pipeline_order", "unknown_metadata", "curried_argument", "curried_artifact",
                     "curried_native", "curried_cost", "varying_artifact", "ledger_coin", "ledger_credit", "ledger_state"):
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
        require(canonical(bad) != canonical(report), "No-op cost-profile corruption: " + mutation)
        # Exercise the projection itself as well as source checks. This prevents
        # whole-current freshness from masking a permissive normalization rule.
        rejected(lambda: project_validated(bad), mutation)
        controls += 1
    old = load()["puzzles"][-1]["curried_instances"]["ledger"]
    actual = copy.deepcopy(rust_on(report["puzzles"][-1]["curried_instances"]["ledger"]))
    for attempt in actual["attempts"]:
        attempt["coin_id"] = "11" * 32
        for child in attempt["after"]["children"]:
            child["parent_coin_id"] = attempt["coin_id"]
            child["coin_id"] = coin_id(attempt["coin_id"], child["puzzle_hash"])
    rejected(lambda: check_ledger_transition(actual, old), "coherent arbitrary starting coin IDs")
    controls += 1
    return controls
