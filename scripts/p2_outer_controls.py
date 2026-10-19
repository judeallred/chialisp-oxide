"""Nonvacuous whole-current controls before exact historical21 normalization."""
import copy

import p2_append_migration as append
import runtime_cost_migration as cost


def self_test(report):
    if not append.is_append(report):
        return 0
    cost.validate_current(report)
    selected = [index for index, puzzle in enumerate(report["puzzles"])
                if puzzle["slug"] == append.ADDED]
    cost.require(len(selected) == 1, "Unique current pay-to-singleton component required")
    p2_index = selected[0]
    cost.project_append(report)
    # Keep the positively selected current transition while corrupting its input.
    # The generic pure dispatcher also accepts historical21 as an intentional
    # no-op; it is not a standalone current-source completeness validator.
    current_projector = cost.current22_migration(report).project_validated
    current_projector(report)
    controls = 0
    for name in ("source", "native", "resource", "preflight", "rollback", "receipt", "debug_rejection", "debug_lifecycle", "artifact", "generated", "missing", "duplicate", "order", "pipeline", "unknown"):
        bad = copy.deepcopy(report)
        p2 = bad["puzzles"][p2_index]
        first = p2["protocol_validation"]["observations"][0]
        if name == "source": p2["source"] += "\n"
        elif name == "native": p2["native_sources"][0]["sha256"] = "0"*64
        elif name == "resource": first["negatives"][0]["observed"] = "CostExceeded"
        elif name == "preflight": first["negatives"][0]["stage"] = "signer_preflight"
        elif name == "rollback": first["negatives"][0]["full_simulator_equality"] = False
        elif name == "receipt": first["lifecycle"]["receipts"].pop()
        elif name == "debug_rejection": first["negatives"][0]["before_debug"] = "private SDK Debug"
        elif name == "debug_lifecycle": first["lifecycle"]["joint_after_debug"] = "private SDK Debug"
        elif name == "artifact": p2["artifacts"][0]["tree_hash"] = "0"*64
        elif name == "generated": p2["generated_cases"] += 1
        elif name == "missing": bad["puzzles"].pop(p2_index)
        elif name == "duplicate": bad["puzzles"].append(copy.deepcopy(p2))
        elif name == "order": bad["puzzles"].reverse()
        elif name == "pipeline": bad["optimization_pipeline"]["rust_mir"]["optimized"].reverse()
        else: bad["unknown_append_allowance"] = True
        cost.require(cost.canonical(bad) != cost.canonical(report), "No-op whole-current corruption: "+name)
        for action in (cost.validate_current, current_projector):
            try:
                action(bad)
            except (ValueError,KeyError,TypeError,IndexError):
                pass
            else:
                raise ValueError("Whole-current corruption passed: "+name)
        controls += 1
    return controls
