"""Shared bounded finite replay/comparison; no stage selection or promotion.

Callers supply their independently closed historical projection and immutable
baseline identities. The default record check does not execute native Rust,
consensus, Cargo or a compiler.
"""
import json
from pathlib import Path
import subprocess
import tempfile

from cargo_authoring import unique_object
from runtime_cost_migration import require, rust_on, same, sha
from vm_failures import check_execution


def measurements(report, *, project):
    previous = project(report)
    rows = []
    for old, current in zip(previous["puzzles"], report["puzzles"]):
        deltas = [rust_on(c["runs"])["execution"]["cost"] - rust_on(a["runs"])["execution"]["cost"]
                  for a, c in zip(old["cases"], current["cases"]) if a["native"]["status"] == "accepted"]
        rows.append({"slug": current["slug"], "old_optimized_bytes": rust_on(old["artifacts"])["bytes"],
            "new_optimized_bytes": rust_on(current["artifacts"])["bytes"],
            "accepted_costs_lower": sum(x < 0 for x in deltas), "accepted_costs_equal": sum(x == 0 for x in deltas),
            "accepted_costs_higher": sum(x > 0 for x in deltas)})
    return {"puzzles": rows, "named_cases": sum(len(p["cases"]) for p in report["puzzles"]),
        "scope": "Finite recorded named measurements; no universal cost dominance or preservation proof",
        "diagnostic_resource_improvements": sum(
            rust_on(a["runs"])["execution"]["outcome"]["status"] == "inconclusive"
            and rust_on(b["runs"])["execution"]["outcome"]["status"] == "accepted"
            for old, new in zip(previous["puzzles"], report["puzzles"])
            for a, b in zip(old.get("resource_controls", {}).get("cases", []), new.get("resource_controls", {}).get("cases", [])))}


def batch(cli, artifact, rows, budget, *, conclusive):
    require(type(budget) is int and 1 <= budget <= 10_000_000, "Invalid retained VM budget")
    observations = []
    with tempfile.TemporaryDirectory(prefix="chialisp-oxide-cost-replay-") as temporary:
        code, solutions = Path(temporary) / "retained.hex", Path(temporary) / "solutions.json"
        code.write_text(artifact["clvm_hex"] + "\n")
        for start in range(0, len(rows), 32):
            values = [r["solution"] for r in rows[start:start + 32]]
            payload = json.dumps(values).encode()
            require(len(payload) <= 8 * 1024 * 1024, "Replay input exceeds bounded CLI payload")
            solutions.write_bytes(payload)
            result = subprocess.run([str(cli), "run-batch", str(code), "--solutions", str(solutions), "--max-cost", str(budget)],
                capture_output=True, text=True, timeout=120)
            require(result.returncode == 0, "Retained cost replay failed: " + result.stderr[:2000])
            values = json.loads(result.stdout, object_pairs_hook=unique_object)
            require(isinstance(values, list) and len(values) == len(rows[start:start + 32]), "Missing/extra retained observations")
            for value in values:
                check_execution(value, require_conclusive=conclusive)
            observations.extend(values)
    return observations


def replay(report, cli, *, project):
    old = project(report)
    counts = {"named_vm_executions": 0, "diagnostic_vm_executions": 0, "fixed_instance_vm_executions": 0,
              "varying_instance_vm_executions": 0, "dedicated_primary_vm_executions": 0}
    for previous, current in zip(old["puzzles"], report["puzzles"]):
        for puzzle in (previous, current):
            rows = puzzle["cases"]
            results = batch(cli, rust_on(puzzle["artifacts"]), rows, puzzle.get("validation_max_cost", 1_000_000), conclusive=True)
            require(same(results, [rust_on(r["runs"])["execution"] for r in rows]), "Retained named costs/failures/bytes differ")
            counts["named_vm_executions"] += len(results)
            if "resource_controls" in puzzle:
                controls = puzzle["resource_controls"]
                results = batch(cli, rust_on(puzzle["artifacts"]), controls["cases"], controls["max_cost"], conclusive=False)
                require(same(results, [rust_on(r["runs"])["execution"] for r in controls["cases"]]), "Retained diagnostics differ")
                counts["diagnostic_vm_executions"] += len(results)
            if "curried_instances" not in puzzle:
                continue
            c = puzzle["curried_instances"]
            artifact = next(r["artifact"] for r in c["artifacts"] if r["artifact"]["backend"] == "rust_mir" and r["artifact"]["optimize_lir"] is True)
            results = batch(cli, artifact, c["cases"], 1_000_000, conclusive=True)
            require(same(results, [rust_on(r["runs"])["execution"] for r in c["cases"]]), "Retained fixed-instance envelope differs")
            counts["fixed_instance_vm_executions"] += len(results)
            for row in c["varying_arguments"]:
                run = next(r for r in row["runs"] if r["artifact"]["backend"] == "rust_mir" and r["artifact"]["optimize_lir"] is True)
                result = batch(cli, run["artifact"], [row], 1_000_000, conclusive=True)[0]
                require(same(result, run["execution"]), "Retained varying-instance envelope differs")
                counts["varying_instance_vm_executions"] += 1
            for attempt in rust_on(c["ledger"])["attempts"]:
                result = batch(cli, {"clvm_hex": attempt["reveal_hex"]}, [attempt], 1_000_000, conclusive=True)[0]
                require(same(result, attempt["vm_021"]), "Retained dedicated primary-VM envelope differs")
                counts["dedicated_primary_vm_executions"] += 1
    require(counts["named_vm_executions"] == 778, "Changed frozen named replay coverage")
    return {**counts, "total": sum(counts.values()), "scope": "Finite retained primary VM only; no consensus resubmission, native execution or compilation"}


def benchmark_check(path, report, *, project, baseline_report_sha, raw_ir):
    measured = json.loads(path.read_bytes(), object_pairs_hook=unique_object)
    old = project(report)
    require(measured["status"] == "passed" and measured["baseline_report_sha256"] == baseline_report_sha
            and [r["slug"] for r in measured["puzzles"]] == [r["slug"] for r in report["puzzles"]], "Wrong/incomplete external benchmark baseline")
    hashes = {r["entry"]: r["sha256"] for r in raw_ir}
    for observed, previous, current in zip(measured["puzzles"], old["puzzles"], report["puzzles"]):
        require(observed["source_ir_sha256"] == hashes[f"chialisp_oxide_puzzles::{current['slug'].replace('-', '_')}::puzzle"]
                and observed["off_byte_exact"] is True, "External benchmark rawIR/OFF identity differs")
        for field, puzzle in (("old_off", previous), ("new_off", current)):
            records = [r for r in puzzle["artifacts"] if r["backend"] == "rust_mir" and r["optimize_lir"] is False]
            require(len(records) == 1 and same({key: observed[field][key] for key in ("bytes", "clvm_hex", "tree_hash")},
                    {key: records[0][key] for key in ("bytes", "clvm_hex", "tree_hash")}),
                    "External benchmark OFF bytes/size/hash differ from retained actual artifact")
        for mode, puzzle in (("old_on", previous), ("new_on", current)):
            artifact = rust_on(puzzle["artifacts"])
            require(same({key: observed[mode][key] for key in ("bytes", "clvm_hex", "tree_hash")},
                         {key: artifact[key] for key in ("bytes", "clvm_hex", "tree_hash")}), "Benchmark artifact differs from measured profile")
        require(len(observed["named"]) == len(current["cases"]), "Missing external named rows")
        for row, before, after in zip(observed["named"], previous["cases"], current["cases"]):
            require((row["label"], row["solution"]) == (after["label"], after["solution"])
                    and same(row["old_on"], rust_on(before["runs"])["execution"])
                    and same(row["new_on"], rust_on(after["runs"])["execution"]), "Benchmark named envelope differs from frozen/profile data")
    return {"sha256": sha(path.read_bytes()), "named_cases": 389,
            "scope": "Cross-check of retained external benchmark data, not authentication that its processes ran"}
