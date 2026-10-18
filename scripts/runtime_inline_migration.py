"""Closed helper-inlining -> signed405 projection, before historical cost gates.

Current evidence is validated before any slots are restored. The old405 slots
come from the immutable predecessor's reviewed_after, not a new transcription.
No compiler, solver, VM or consensus execution occurs in this module.
"""
import copy
import hashlib
import json
from pathlib import Path
import re

from cargo_authoring import unique_object
import runtime_cost_migration as cost
from vm_failures import check_execution

ROOT = Path(__file__).resolve().parents[1]
PATH = ROOT / "corpus/helper-inlining-baseline.json"
PIN = "0b7326f83065ab1bc186a1e9b0d3b5c581fa7cc3110845cc4b6db70a39b5da52"
COMMIT = "405792113242aed8d94d3dc4805ed68fc667b3d1"
REPORT_SHA = "edaeebaebe368b72bb471c37b0fd608d1000f5a83b1f8defc9c8ec6eb04b468d"
REPORT_BLOB = "41283c25de4beb2ce31aa33a2bfd0aa8262302d8"
REPORT_CANONICAL = "7a4631f085bf9701ebede299c457aa115fde286dcf39505fadce43a03edd215e"
OLD_PIPELINE = cost.NEW_PIPELINE
NEW_PIPELINE = cost.READABLE["INLINE_PIPELINE"]
BEFORE_PATCHES = [f"https://github.com/xch-dev/rue/issues/{n}" for n in (108, 109, 110, 111, 112, 113)]
ADDED_PATCHES = ["https://github.com/xch-dev/rue/issues/114"]
require, sha, canonical, same, rust_on = cost.require, cost.sha, cost.canonical, cost.same, cost.rust_on


def is_inline(report):
    return report.get("optimization_pipeline") == NEW_PIPELINE


def predecessor():
    value = cost.load()
    require(value["reviewed_after"]["status"] == "reviewed_final"
            and value["reviewed_after"]["report_sha256"] == REPORT_SHA
            and value["reviewed_after"]["optimization_pipeline"] == OLD_PIPELINE,
            "Predecessor is not the final signed405 measurement")
    return value


def load(payload=None):
    payload = PATH.read_bytes() if payload is None else payload
    require(sha(payload) == PIN, "Reviewed helper-inlining snapshot/profile bytes changed")
    value = json.loads(payload, object_pairs_hook=unique_object)
    require(set(value) == {"schema_version", "protocol", "scope", "baseline", "predecessor",
                           "optimization_pipeline", "rue_local_patches", "reviewed_after"}
            and type(value["schema_version"]) is int and value["schema_version"] == 1
            and value["protocol"] == "chialisp-oxide-helper-inlining/v1", "Unsupported helper-inlining profile")
    require(value["baseline"] == {"repository": "https://github.com/judeallred/chialisp-oxide", "commit": COMMIT,
            "report_path": "corpus/validation.json", "report_sha256": REPORT_SHA,
            "report_git_blob_sha1": REPORT_BLOB, "canonical_report_sha256": REPORT_CANONICAL},
            "Changed signed405 baseline identity")
    require(value["predecessor"] == {"path": "corpus/cost-optimization-baseline.json", "sha256": cost.PIN}
            and value["optimization_pipeline"] == {"before": OLD_PIPELINE, "after": NEW_PIPELINE},
            "Changed immutable predecessor or helper-inlining pipeline")
    require(set(value["rue_local_patches"]) == {"before", "added", "status"}
            and value["rue_local_patches"]["before"] == BEFORE_PATCHES,
            "Changed pre-existing Rue patch history")
    predecessor()  # Enforce the predecessor's own immutable byte pin as well.
    return value


def reviewed_new(report):
    profile = load()
    after, patches = profile["reviewed_after"], profile["rue_local_patches"]
    require(after.get("status") == "reviewed_final" and patches["status"] == "reviewed",
            "Helper-inlining profile/patch metadata is pending; no new evidence promotion")
    require(patches["added"] == ADDED_PATCHES, "Expected exactly the reviewed Rue issue114 metadata")
    require(after["report_path"] == "corpus/validation.json"
            and isinstance(after["report_sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", after["report_sha256"])
            and after["optimization_pipeline"] == NEW_PIPELINE == report["optimization_pipeline"],
            "Wrong final helper-inlining report/pipeline")
    require(report.get("rue_local_patches") == BEFORE_PATCHES + patches["added"],
            "Unknown, removed or reordered Rue patch metadata")
    old = predecessor()["reviewed_after"]["puzzles"]
    require([p["slug"] for p in after["puzzles"]] == [p["slug"] for p in old],
            "Missing/extra/reordered reviewed helper-inlining fixture")
    return after


def expected_patches(report):
    """Side-effect-free profile selection for source checking, never projection."""
    reviewed_new(report)
    return BEFORE_PATCHES + load()["rue_local_patches"]["added"]


def check_rows(actual, old, new, *, diagnostic=False):
    require(len(actual) == len(old) == len(new), "Changed helper-inlining measured coverage")
    for row, before, after in zip(actual, old, new):
        require(row["label"] == before["label"] == after["label"]
                and sha(row["solution"].encode()) == before["solution_sha256"] == after["solution_sha256"],
                "Changed helper-inlining input/order")
        execution = rust_on(row["runs"])["execution"]
        check_execution(before["execution"], require_conclusive=not diagnostic)
        observed = check_execution(execution, require_conclusive=not diagnostic)
        require(same(execution, after["execution"]), "Unreviewed helper-inlining VM envelope")
        if diagnostic:
            if observed["status"] == "inconclusive":
                require(execution.get("failure") == {"kind": "resource", "code": "cost_exceeded"},
                        "Only exact cost exhaustion is a declared low-budget resource control")
            else:
                require(observed == row["native"], "Diagnostic conclusive output differs from actual native policy")
        else:
            require(observed == before["execution"]["outcome"] == row["native"],
                    "Changed finite native/output/acceptance domain")


def hash_diagnostic_profile(report):
    """Closed current hash slots; avoids current-validation -> hash recursion."""
    after = {p["slug"]: p for p in reviewed_new(report)["puzzles"]}
    old = {p["slug"]: p for p in predecessor()["reviewed_after"]["puzzles"]}
    for slug in ("sha256tree", "singleton-launcher"):
        matches = [p for p in report["puzzles"] if p["slug"] == slug]
        require(len(matches) == 1, "Missing/duplicate inlined hash fixture")
        puzzle = matches[0]
        require(same(rust_on(puzzle["artifacts"]), after[slug]["artifact"]), "Unreviewed inlined hash artifact")
        if slug == "sha256tree":
            check_rows(puzzle["resource_controls"]["cases"], old[slug]["resource_cases"],
                       after[slug]["resource_cases"], diagnostic=True)
    return True


def project_validated(report):
    """Historical projection only; public preserve checks current sources first."""
    baseline = predecessor()
    old_rows = baseline["reviewed_after"]["puzzles"]
    require([p["slug"] for p in report["puzzles"]] == [p["slug"] for p in old_rows]
            and same(report["authoring"], baseline["authoring"]), "Changed exact21 Cargo/source identities")
    if not is_inline(report):
        require(report["optimization_pipeline"] == OLD_PIPELINE
                and sha(canonical(report)) == REPORT_CANONICAL,
                "Historical helper baseline is not exactly signed405")
        return copy.deepcopy(report)
    after_rows = reviewed_new(report)["puzzles"]
    result = copy.deepcopy(report)
    for current, target, old, measured in zip(report["puzzles"], result["puzzles"], old_rows, after_rows):
        cost.artifact_bytes(rust_on(current["artifacts"]))
        require(same(rust_on(current["artifacts"]), measured["artifact"]), "Unreviewed inlined Rust-on artifact")
        check_rows(current["cases"], old["cases"], measured["cases"])
        target["artifacts"][target["artifacts"].index(rust_on(target["artifacts"]))] = copy.deepcopy(old["artifact"])
        cost.restore_rows(target["cases"], old["cases"])
        if "resource_cases" in old:
            check_rows(current["resource_controls"]["cases"], old["resource_cases"], measured["resource_cases"], diagnostic=True)
            cost.restore_rows(target["resource_controls"]["cases"], old["resource_cases"])
        if "curried_instances" not in old:
            continue
        c, t, previous = current["curried_instances"], target["curried_instances"], old["curried_instances"]
        candidate = measured["curried_instances"]
        records = [r for r in c["artifacts"] if r["artifact"]["backend"] == "rust_mir" and r["artifact"]["optimize_lir"] is True]
        require(len(records) == 1 and same(records[0], candidate["artifact"]), "Unreviewed inlined fixed-instance artifact")
        check_rows(c["cases"], previous["cases"], candidate["cases"])
        t["artifacts"][c["artifacts"].index(records[0])] = copy.deepcopy(previous["artifact"])
        cost.restore_rows(t["cases"], previous["cases"])
        require(len(c["varying_arguments"]) == len(previous["varying_runs"]) == len(candidate["varying_runs"]) == 3,
                "Changed varying-H coverage")
        for index, (row, restored) in enumerate(zip(c["varying_arguments"], t["varying_arguments"])):
            runs = [r for r in row["runs"] if r["artifact"]["backend"] == "rust_mir" and r["artifact"]["optimize_lir"] is True]
            require(len(runs) == 1 and same(runs[0], candidate["varying_runs"][index]), "Unreviewed varying-H artifact/envelope")
            require(check_execution(runs[0]["execution"]) == previous["varying_runs"][index]["execution"]["outcome"] == row["native"],
                    "Changed varying-H result")
            restored["runs"][row["runs"].index(runs[0])] = copy.deepcopy(previous["varying_runs"][index])
        ledger = rust_on(c["ledger"])
        require(same(ledger, candidate["ledger"]), "Unreviewed inlined Rust-on ledger evidence")
        cost.check_ledger_transition(ledger, previous["ledger"])
        t["ledger"][c["ledger"].index(ledger)] = copy.deepcopy(previous["ledger"])
    result["optimization_pipeline"] = copy.deepcopy(OLD_PIPELINE)
    result["rue_local_patches"] = copy.deepcopy(BEFORE_PATCHES)
    require(sha(canonical(result)) == REPORT_CANONICAL,
            "Helper-inlining changed retained405 OFF/reference/Rue/raw sources/ABI/order/coverage/outcomes/ledger or unknown metadata")
    return result


def preserve(report):
    # No source/freshness validator runs on the projected historical object.
    cost.validate_current(report)
    return project_validated(report)


def capture_after(report):
    """Return candidate measured slots for explicit review; does not authorize.

    The caller must have actually built/validated this report, review these
    bytes, update the NEW profile and its pin, then run every real gate/replay.
    This helper never writes reports, changes a profile or bypasses its pin.
    """
    def cases(rows):
        return [{"label": row["label"], "solution_sha256": sha(row["solution"].encode()),
                 "execution": copy.deepcopy(rust_on(row["runs"])["execution"])} for row in rows]
    values = []
    for puzzle in report["puzzles"]:
        value = {"slug": puzzle["slug"], "artifact": copy.deepcopy(rust_on(puzzle["artifacts"])), "cases": cases(puzzle["cases"])}
        if "resource_controls" in puzzle:
            value["resource_cases"] = cases(puzzle["resource_controls"]["cases"])
        if "curried_instances" in puzzle:
            c = puzzle["curried_instances"]
            value["curried_instances"] = {
                "artifact": copy.deepcopy(next(r for r in c["artifacts"] if r["artifact"]["backend"] == "rust_mir" and r["artifact"]["optimize_lir"] is True)),
                "cases": cases(c["cases"]), "varying_runs": [copy.deepcopy(next(r for r in row["runs"] if r["artifact"]["backend"] == "rust_mir" and r["artifact"]["optimize_lir"] is True)) for row in c["varying_arguments"]],
                "ledger": copy.deepcopy(rust_on(c["ledger"]))}
        values.append(value)
    return values
