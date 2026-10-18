"""Closed measured cost transition, before all historical corpus projections.

Current source/typed evidence is checked before restoring reviewed OLD-on slots.
This is finite recorded-evidence validation, not authenticated execution or a
compiler-preservation/cost-dominance proof. Retained replay is a separate action.
"""
import copy
import hashlib
import json
from pathlib import Path
import runpy

from cargo_authoring import check_recorded, contained_file, unique_object
from password_evidence import artifact_bytes, validate as password_evidence
from password_vectors import CODEC
from runtime_password_migration import current_native_sources
from vm_failures import check_execution

ROOT = Path(__file__).resolve().parents[1]
PATH = ROOT / "corpus/cost-optimization-baseline.json"
PIN = "484168ef0aad4c53808ff37f5300288bb022dde58748e34db03e61b4f5834308"
COMMIT = "d5948428efa719b5d7eb73d125139a6c7e5b400c"
BEFORE_REPORT = "52b2ef139c93f3d2235fd946d70096d4311bf455a2d244b68d541eda3f137cab"
PARENT = "ae2915f8495b82ab18f7f06a86c7498e34d193781a0906f753a08d4b9afd3fc8"
READABLE = runpy.run_path(str(ROOT / "scripts/check-readable-puzzles.py"))
OLD_PIPELINE, NEW_PIPELINE = READABLE["PIPELINE"], READABLE["COST_PIPELINE"]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()


def sha(value):
    return hashlib.sha256(value).hexdigest()


def same(left, right):
    return canonical(left) == canonical(right)


def load(payload=None):
    payload = PATH.read_bytes() if payload is None else payload
    require(sha(payload) == PIN, "Reviewed cost baseline/profile bytes changed")
    value = json.loads(payload, object_pairs_hook=unique_object)
    require(set(value) == {"schema_version", "protocol", "scope", "baseline", "optimization_pipeline", "authoring", "raw_ir", "puzzles", "reviewed_after", "simulator_parent_coin_info"}
            and type(value["schema_version"]) is int and value["schema_version"] == 1
            and value["protocol"] == "chialisp-oxide-cost-migration/v1", "Unsupported cost profile")
    require(value["baseline"]["commit"] == COMMIT and value["baseline"]["report_sha256"] == BEFORE_REPORT
            and value["optimization_pipeline"] == OLD_PIPELINE and value["simulator_parent_coin_info"] == PARENT,
            "Changed signed21 baseline or independent simulator parent")
    slugs = [p["slug"] for p in value["puzzles"]]
    require(len(slugs) == len(set(slugs)) == 21 and [p["slug"] for p in value["reviewed_after"]["puzzles"]] == slugs,
            "Missing/reordered/unknown cost-profile fixture")
    require(len(value["raw_ir"]) == 21 and len({r["entry"] for r in value["raw_ir"]}) == 21
            and {r["entry"] for r in value["raw_ir"]} == {f"chialisp_oxide_puzzles::{s.replace('-', '_')}::puzzle" for s in slugs},
            "Missing/duplicate/unreviewed rawIR identities")
    require(value["reviewed_after"]["optimization_pipeline"] == NEW_PIPELINE, "Unknown new optimization pipeline")
    return value


def rust_on(rows):
    selected = [r for r in rows if r.get("backend") == "rust_mir" and r.get("optimize_lir") is True]
    require(len(selected) == 1, "Missing/duplicate typed Rust-on mode")
    return selected[0]


def is_new(report):
    pipeline = report.get("optimization_pipeline")
    if pipeline == READABLE["INLINE_PIPELINE"]:
        return True  # New relative to the original pre-cost pipeline as well.
    require(pipeline in (OLD_PIPELINE, NEW_PIPELINE), "Unknown optimization pipeline")
    return pipeline == NEW_PIPELINE


def reviewed_new(report):
    profile = load()["reviewed_after"]
    require(profile["status"] == "reviewed_final", "New cost profile is pending final stage3 evidence; no promotion yet")
    require(profile["optimization_pipeline"] == report["optimization_pipeline"], "Wrong reviewed cost pipeline")
    return profile


def check_rows(actual, old, new=None, *, diagnostic=False, slug=None):
    require(len(actual) == len(old) and (new is None or len(new) == len(old)), "Changed cost-profile case count")
    for index, (row, previous) in enumerate(zip(actual, old)):
        require(row["label"] == previous["label"] and sha(row["solution"].encode()) == previous["solution_sha256"],
                "Changed cost-profile input/order")
        execution = rust_on(row["runs"])["execution"]
        before = previous["execution"]
        check_execution(before, require_conclusive=not diagnostic)
        check_execution(execution, require_conclusive=not diagnostic)
        if new is not None:
            require(new[index]["label"] == previous["label"] and new[index]["solution_sha256"] == previous["solution_sha256"]
                    and same(execution, new[index]["execution"]), "Current Rust-on execution differs from reviewed measured profile")
        else:
            require(same(execution, before), "OLD pipeline carries an unreviewed Rust-on execution")
        if diagnostic:
            old_outcome, outcome = before["outcome"], execution["outcome"]
            improved = slug == "sha256tree" and index in (2, 3, 4) and new is not None
            if improved:
                require(before.get("failure") == {"kind": "resource", "code": "cost_exceeded"}
                        and outcome == row["native"] and outcome["status"] == "accepted", "Changed diagnostic improvement contract")
            else:
                require(outcome == old_outcome, "Unreviewed low-budget outcome transition")
                if outcome["status"] == "inconclusive":
                    require(execution.get("failure") == {"kind": "resource", "code": "cost_exceeded"}, "Diagnostic is not exact cost exhaustion")
            require(outcome["status"] != "rejected", "Low-budget diagnostics cannot add policy rejection credit")
        else:
            require(execution["outcome"] == before["outcome"] == row["native"], "Changed finite output/acceptance domain")


def hash_diagnostic_profile(report):
    """Small closed profile for the direct hash gate; no historical projection."""
    if report.get("optimization_pipeline") == READABLE["INLINE_PIPELINE"]:
        from runtime_inline_migration import hash_diagnostic_profile as selected
        return selected(report)
    if not is_new(report):
        return False
    profile = reviewed_new(report)
    old = {p["slug"]: p for p in load()["puzzles"]}
    after = {p["slug"]: p for p in profile["puzzles"]}
    for slug in ("sha256tree", "singleton-launcher"):
        selected = [p for p in report["puzzles"] if p["slug"] == slug]
        require(len(selected) == 1, "Missing/duplicate cost hash fixture")
        puzzle = selected[0]
        require(same(rust_on(puzzle["artifacts"]), after[slug]["artifact"]), "Unreviewed optimized hash artifact")
        if slug == "sha256tree":
            check_rows(puzzle["resource_controls"]["cases"], old[slug]["resource_cases"],
                       after[slug]["resource_cases"], diagnostic=True, slug=slug)
    return True


def validate_current(report):
    READABLE["check_report"](report)
    READABLE["check_sources"](report, ROOT, require_rue=True)
    check_recorded(report.get("authoring"), ROOT, [p["slug"] for p in report["puzzles"]])
    current_native_sources(report)
    password_evidence(report)
    # This source/current-domain gate is independent of the historical digest.
    gate = runpy.run_path(str(ROOT / "scripts/check-hashing-puzzles.py"))
    gate["validate"](report)


def restore_rows(actual, previous):
    for row, old in zip(actual, previous):
        rust_on(row["runs"])["execution"] = copy.deepcopy(old["execution"])


def coin_id(parent, puzzle_hash):
    return sha(bytes.fromhex(parent) + bytes.fromhex(puzzle_hash) + b"\x03\xe8")


def check_ledger_transition(actual, previous):
    """Only derive identity changes from reviewed M/H; keep policy/state exact."""
    projected = copy.deepcopy(actual)
    projected["module_tree_hash"] = previous["module_tree_hash"]
    projected["instance_tree_hash"] = previous["instance_tree_hash"]
    require(len(actual["attempts"]) == len(previous["attempts"]) == 7, "Changed dedicated ledger coverage")
    for new, old, restored in zip(actual["attempts"], previous["attempts"], projected["attempts"]):
        require(new["coin_id"] == coin_id(PARENT, actual["instance_tree_hash"])
                and old["coin_id"] == coin_id(PARENT, previous["instance_tree_hash"]), "Starting coin id is not independently derived")
        check_execution(new["vm_021"])
        require(new["vm_021"]["outcome"] == old["vm_021"]["outcome"], "Changed dedicated primary-VM policy result")
        for key in ("coin_id", "coin_puzzle_hash", "reveal_hex", "reveal_tree_hash", "vm_021"):
            restored[key] = copy.deepcopy(old[key])
        require(len(new["after"]["children"]) == len(old["after"]["children"]), "Changed ledger child count")
        for child, old_child, target in zip(new["after"]["children"], old["after"]["children"], restored["after"]["children"]):
            require(child["parent_coin_id"] == new["coin_id"] and child["coin_id"] == coin_id(new["coin_id"], child["puzzle_hash"]),
                    "Child identity is not independently derived")
            target["coin_id"], target["parent_coin_id"] = old_child["coin_id"], old_child["parent_coin_id"]
    require(same(projected, previous), "Changed non-derived ledger input/state/consensus/semantic-credit field")


def project_validated(report):
    """Historical-only projection. Public preserve() checks current evidence first."""
    baseline = load()
    require([p["slug"] for p in report["puzzles"]] == [p["slug"] for p in baseline["puzzles"]]
            and same(report["authoring"], baseline["authoring"]), "Changed exact21 identities or Cargo/native/source inputs")
    new = is_new(report)
    after = reviewed_new(report)["puzzles"] if new else [None] * 21
    result = copy.deepcopy(report)
    for current, target, old, measured in zip(report["puzzles"], result["puzzles"], baseline["puzzles"], after):
        expected = measured["artifact"] if new else old["artifact"]
        artifact_bytes(rust_on(current["artifacts"]))
        require(same(rust_on(current["artifacts"]), expected), "Unreviewed current Rust-on artifact")
        check_rows(current["cases"], old["cases"], measured["cases"] if new else None)
        target["artifacts"][target["artifacts"].index(rust_on(target["artifacts"]))] = copy.deepcopy(old["artifact"])
        restore_rows(target["cases"], old["cases"])
        if "resource_cases" in old:
            check_rows(current["resource_controls"]["cases"], old["resource_cases"], measured["resource_cases"] if new else None,
                       diagnostic=True, slug=current["slug"])
            restore_rows(target["resource_controls"]["cases"], old["resource_cases"])
        if "curried_instances" in old:
            c, t, previous = current["curried_instances"], target["curried_instances"], old["curried_instances"]
            candidate = measured["curried_instances"] if new else previous
            current_artifact = [r for r in c["artifacts"] if r["artifact"]["backend"] == "rust_mir" and r["artifact"]["optimize_lir"] is True]
            require(len(current_artifact) == 1 and same(current_artifact[0], candidate["artifact"]), "Unreviewed fixed-instance artifact")
            check_rows(c["cases"], previous["cases"], candidate["cases"] if new else None)
            t["artifacts"][c["artifacts"].index(current_artifact[0])] = copy.deepcopy(previous["artifact"])
            restore_rows(t["cases"], previous["cases"])
            require(len(c["varying_arguments"]) == len(previous["varying_runs"]) == 3, "Changed varying-H coverage")
            for index, (row, restored) in enumerate(zip(c["varying_arguments"], t["varying_arguments"])):
                found = [run for run in row["runs"] if run["artifact"]["backend"] == "rust_mir" and run["artifact"]["optimize_lir"] is True]
                require(len(found) == 1 and same(found[0], candidate["varying_runs"][index]), "Unreviewed varying-H artifact/VM envelope")
                require(check_execution(found[0]["execution"]) == previous["varying_runs"][index]["execution"]["outcome"] == row["native"], "Changed varying-H finite result")
                restored["runs"][row["runs"].index(found[0])] = copy.deepcopy(previous["varying_runs"][index])
            ledger = rust_on(c["ledger"])
            require(same(ledger, candidate["ledger"]), "Unreviewed dedicated Rust-on ledger record")
            check_ledger_transition(ledger, previous["ledger"])
            t["ledger"][c["ledger"].index(ledger)] = copy.deepcopy(previous["ledger"])
    result["optimization_pipeline"] = copy.deepcopy(OLD_PIPELINE)
    require(sha(canonical(result)) == baseline["baseline"]["canonical_report_sha256"],
            "Changed retained21 source/ABI/inputs/raw outcomes/OFF/reference/Rue/ledger or unknown metadata")
    return result


def preserve(report):
    if report.get("optimization_pipeline") == READABLE["INLINE_PIPELINE"]:
        from runtime_inline_migration import preserve as outer_preservation
        # The outer layer checks actual current evidence first. Never run
        # current-source checks again on its historical signed405 projection.
        return project_validated(outer_preservation(report))
    validate_current(report)
    return project_validated(report)


def verify_ir_bundle(path, report):
    """Explicit supplied-bundle retention check; does not extract/authenticate."""
    from runtime_password_migration import verify_ir_bundle as password_bundle
    path = path.resolve()
    result = password_bundle(path, report)
    manifest = json.loads(path.read_bytes(), object_pairs_hook=unique_object)
    expected = {row["entry"]: row["sha256"] for row in load()["raw_ir"]}
    require(len(manifest["entries"]) == 21, "Expected exact21-export source-derived bundle")
    for row in manifest["entries"]:
        require(row["entry"] in expected and sha((path.parent / row["ir"]).read_bytes()) == expected[row["entry"]],
                "A frozen21 rawIR byte string changed")
    return {**result, "compared_old_ir": 21, "scope": "Explicit caller-supplied fresh bundle consistency/byte retention, not authenticated provenance"}
