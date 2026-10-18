"""Nonvacuous current22 controls before exact historical21 normalization."""
import copy

import p2_append_migration as append
import runtime_cost_migration as cost


def self_test(report):
    if not append.is_append(report):
        return 0
    cost.validate_current(report)
    append.project_validated(report)
    controls = 0
    for name in ("source", "native", "resource", "preflight", "rollback", "receipt", "debug_rejection", "debug_lifecycle", "artifact", "generated", "missing", "duplicate", "order", "unknown"):
        bad = copy.deepcopy(report)
        p2 = bad["puzzles"][-1]
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
        elif name == "missing": bad["puzzles"].pop()
        elif name == "duplicate": bad["puzzles"].append(copy.deepcopy(p2))
        elif name == "order": bad["puzzles"].reverse()
        else: bad["unknown_append_allowance"] = True
        cost.require(cost.canonical(bad) != cost.canonical(report), "No-op current22 corruption: "+name)
        for action in (cost.validate_current, append.project_validated):
            try:
                action(bad)
            except (ValueError,KeyError,TypeError,IndexError):
                pass
            else:
                raise ValueError("Current22 corruption passed: "+name)
        controls += 1
    return controls
