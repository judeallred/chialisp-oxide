"""Additional finite benchmark/profile comparison for runtime refinements.

This compares recorded observations. It does not authenticate that an external
process ran, and does not turn retained VM replay into a new consensus spend.
"""
from runtime_cost_migration import require, rust_on, same
from vm_failures import check_execution


def mode(rows, backend, optimize):
    selected = [r for r in rows if r.get("backend") == backend and r.get("optimize_lir") is optimize]
    require(len(selected) == 1, "Missing/duplicate typed benchmark execution mode")
    return selected[0]


def benchmark_extra(measured, old, current):
    resources = [(a, b) for a, b in zip(old["puzzles"], current["puzzles"]) if "resource_controls" in a]
    require([r["slug"] for r in measured["resources"]] == [a["slug"] for a, _ in resources],
            "Changed benchmark diagnostic coverage/order")
    for observed, (before, after) in zip(measured["resources"], resources):
        previous, now = before["resource_controls"], after["resource_controls"]
        require(observed["max_cost"] == previous["max_cost"] == now["max_cost"]
                and same(observed["cases"], previous["cases"]), "Benchmark diagnostic baseline/input differs")
        require(len(observed["retained_baseline_runs"]) == len(before["artifacts"]), "Changed retained diagnostic modes")
        for artifact in before["artifacts"]:
            backend, optimize = artifact["backend"], artifact["optimize_lir"]
            runs = mode(observed["retained_baseline_runs"], backend, optimize)
            require(same(runs["executions"], [mode(row["runs"], backend, optimize)["execution"] for row in previous["cases"]]),
                    "Benchmark retained diagnostic envelope differs")
        require(same(observed["new_rust_on_runs"], [rust_on(row["runs"])["execution"] for row in now["cases"]]),
                "Benchmark candidate diagnostic envelope differs")
        for execution in observed["new_rust_on_runs"]:
            check_execution(execution, require_conclusive=False)

    before, after = old["puzzles"][-1]["curried_instances"], current["puzzles"][-1]["curried_instances"]
    fixed = measured["password_instances"]
    require(fixed["fixed_argument"] == before["fixed_argument"] == after["fixed_argument"]
            and fixed["scheme"] == before["scheme"] == after["scheme"], "Changed benchmark fixed-instance binding")
    for prefix, instances in (("old", before), ("new", after)):
        artifact = next(row["artifact"] for row in instances["artifacts"]
                        if row["artifact"]["backend"] == "rust_mir" and row["artifact"]["optimize_lir"] is True)
        require(fixed[prefix + "_reveal_hex"] == artifact["clvm_hex"]
                and fixed[prefix + "_tree_hash"] == artifact["tree_hash"], "Benchmark fixed-instance artifact differs")
    require(len(fixed["cases"]) == len(before["cases"]) == len(after["cases"]) == 48, "Changed benchmark fixed coverage")
    for row, a, b in zip(fixed["cases"], before["cases"], after["cases"]):
        require((row["label"], row["solution"], row["native"], row["max_cost"])
                == (a["label"], a["solution"], a["native"], 1_000_000)
                and same(row["old_on"], rust_on(a["runs"])["execution"])
                and same(row["new_on"], rust_on(b["runs"])["execution"]), "Benchmark fixed-instance envelope/input differs")

    require(len(measured["password_varying"]) == len(before["varying_arguments"]) == len(after["varying_arguments"]) == 3,
            "Changed benchmark varying-instance coverage")
    for row, a, b in zip(measured["password_varying"], before["varying_arguments"], after["varying_arguments"]):
        require(all(same(row[key], a[key]) for key in ("argument_hex", "password_hex", "solution", "native")),
                "Benchmark varying-instance input differs")
        for prefix, value in (("old", a), ("new", b)):
            run = next(v for v in value["runs"] if v["artifact"]["backend"] == "rust_mir" and v["artifact"]["optimize_lir"] is True)
            require(row[prefix + "_reveal_hex"] == run["artifact"]["clvm_hex"]
                    and same(row[prefix + "_on"], run["execution"]), "Benchmark varying-instance artifact/envelope differs")

    def attempts(instances, candidate):
        result = []
        records = [rust_on(instances["ledger"])] if candidate else instances["ledger"]
        for ledger in records:
            for attempt in ledger["attempts"]:
                row = {"backend": ledger["backend"], "optimize_lir": ledger["optimize_lir"],
                       "label": attempt["label"], "solution": attempt["solution"],
                       "reveal_hex": attempt["reveal_hex"], "reveal_tree_hash": attempt["reveal_tree_hash"],
                       "expected": attempt["vm_021"]["outcome"], "execution": attempt["vm_021"]}
                if candidate:
                    row["argument_hex"] = attempt["argument_hex"]
                result.append(row)
        return result
    primary = measured["dedicated_primary_vm"]
    require(same(primary["baseline_attempts"], attempts(before, False))
            and same(primary["candidate_rust_attempts"], attempts(after, True)),
            "Benchmark dedicated primary-VM record differs; consensus resubmission is not inferred")
    synthetic = [row for puzzle in measured["puzzles"] for row in puzzle["synthetic"]]
    require(len(synthetic) == 12, "Changed external synthetic finite scope")
    for row in synthetic:
        require(check_execution(row["old_on"]) == row["native"] == check_execution(row["new_on"]),
                "External synthetic observations differ from their declared finite oracle")
    counts = {"named_cases": 389, "synthetic_cases": 12, "ordinary_vm_executions": 802,
              "retained_diagnostic_vm_executions": 40, "new_diagnostic_vm_executions": 8,
              "password_instance_vm_executions": 96, "password_varying_vm_executions": 6,
              "dedicated_primary_vm_executions": 28, "total_vm_executions": 980}
    require(all(type(measured.get(key)) is int and measured[key] == value for key, value in counts.items()),
            "Changed external finite benchmark execution counts")
    return {"recorded_vm_observations_compared": 956, "additional_declared_synthetic_observations": 24,
            "scope": "Recorded finite VM/profile comparison; no execution authentication, native execution or consensus resubmission"}
