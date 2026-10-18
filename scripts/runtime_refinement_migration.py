"""Closed runtime-refinement -> signed c879 projection, before older gates.

The immutable predecessor stores the reviewed old optimized slots. Restoring
only those slots must reproduce the entire signed report's canonical digest.
No compiler, solver, VM, native Rust or consensus runs in this module.
"""
import copy
import json
import re
from pathlib import Path

from cargo_authoring import unique_object
import runtime_cost_migration as cost
import runtime_inline_migration as inline

ROOT = Path(__file__).resolve().parents[1]
PATH = ROOT / "corpus/runtime-refinement-baseline.json"
PIN = "11f14c3af4b400bf1569a8a7e3991c381e584a1415057610aedb0d0a26a88d41"
COMMIT = "c87957e2d28c832ec76551aa0c5e16e31f973b2f"
REPORT_SHA = "c3448703f6912d94900b2d012aabb610f1f2efc174f19699094fd43229f4e1ac"
REPORT_BLOB = "38c14f911eb6462044fd8bf0d0b11d6404457614"
REPORT_CANONICAL = "aa2ae87876efdcc1e3b5cdfd828548fc0411d69c316ddb12ac1726d1f04fe29a"
OLD_PIPELINE = inline.NEW_PIPELINE
NEW_PIPELINE = cost.READABLE["REFINEMENT_PIPELINE"]
PATCHES = inline.BEFORE_PATCHES + inline.ADDED_PATCHES
require, sha, canonical, same, rust_on = cost.require, cost.sha, cost.canonical, cost.same, cost.rust_on


def is_refinement(report):
    return report.get("optimization_pipeline") == NEW_PIPELINE


def predecessor():
    value = inline.load()
    require(value["reviewed_after"]["status"] == "reviewed_final"
            and value["reviewed_after"]["report_sha256"] == REPORT_SHA
            and value["reviewed_after"]["optimization_pipeline"] == OLD_PIPELINE,
            "Predecessor is not the final signed c879 measurement")
    return value


def load(payload=None):
    payload = PATH.read_bytes() if payload is None else payload
    require(sha(payload) == PIN, "Reviewed runtime-refinement snapshot/profile bytes changed")
    value = json.loads(payload, object_pairs_hook=unique_object)
    require(set(value) == {"schema_version", "protocol", "scope", "baseline", "predecessor",
                           "optimization_pipeline", "reviewed_after"}
            and type(value["schema_version"]) is int and value["schema_version"] == 1
            and value["protocol"] == "chialisp-oxide-runtime-refinement/v1", "Unsupported runtime-refinement profile")
    require(value["baseline"] == {"repository": "https://github.com/judeallred/chialisp-oxide", "commit": COMMIT,
            "report_path": "corpus/validation.json", "report_sha256": REPORT_SHA,
            "report_git_blob_sha1": REPORT_BLOB, "canonical_report_sha256": REPORT_CANONICAL},
            "Changed signed c879 baseline identity")
    require(value["predecessor"] == {"path": "corpus/helper-inlining-baseline.json", "sha256": inline.PIN}
            and value["optimization_pipeline"] == {"before": OLD_PIPELINE, "after": NEW_PIPELINE},
            "Changed immutable predecessor or runtime-refinement pipeline")
    predecessor()  # Enforce the predecessor's independent immutable byte pin.
    return value


def reviewed_new(report):
    after = load()["reviewed_after"]
    require(after.get("status") == "reviewed_final",
            "Runtime-refinement profile is pending; no new evidence promotion")
    require(after["report_path"] == "corpus/validation.json"
            and isinstance(after["report_sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", after["report_sha256"])
            and after["optimization_pipeline"] == NEW_PIPELINE == report["optimization_pipeline"],
            "Wrong final runtime-refinement report/pipeline")
    old = predecessor()["reviewed_after"]["puzzles"]
    require([p["slug"] for p in after["puzzles"]] == [p["slug"] for p in old],
            "Missing/extra/reordered reviewed runtime-refinement fixture")
    require(report.get("rue_local_patches") == PATCHES, "Changed Rue patch history during runtime refinement")
    return after


def expected_patches(report):
    """Closed current profile selection only; never historical projection."""
    reviewed_new(report)
    return PATCHES


def hash_diagnostic_profile(report):
    """Small current hash slots; never recursively invokes full preservation."""
    after = {p["slug"]: p for p in reviewed_new(report)["puzzles"]}
    old = {p["slug"]: p for p in predecessor()["reviewed_after"]["puzzles"]}
    for slug in ("sha256tree", "singleton-launcher"):
        matches = [p for p in report["puzzles"] if p["slug"] == slug]
        require(len(matches) == 1, "Missing/duplicate refined hash fixture")
        puzzle = matches[0]
        require(same(rust_on(puzzle["artifacts"]), after[slug]["artifact"]), "Unreviewed refined hash artifact")
        if slug == "sha256tree":
            inline.check_rows(puzzle["resource_controls"]["cases"], old[slug]["resource_cases"],
                              after[slug]["resource_cases"], diagnostic=True)
    return True


def project_validated(report):
    """Restore reviewed slots only; source/current validation precedes this call."""
    old_rows = predecessor()["reviewed_after"]["puzzles"]
    require([p["slug"] for p in report["puzzles"]] == [p["slug"] for p in old_rows]
            and same(report["authoring"], cost.load()["authoring"]), "Changed exact21 Cargo/source identities")
    if not is_refinement(report):
        require(report.get("optimization_pipeline") == OLD_PIPELINE
                and sha(canonical(report)) == REPORT_CANONICAL,
                "Historical runtime-refinement baseline is not exactly signed c879")
        return copy.deepcopy(report)
    result = inline.project_slots(report, old_rows, reviewed_new(report)["puzzles"])
    result["optimization_pipeline"] = copy.deepcopy(OLD_PIPELINE)
    require(sha(canonical(result)) == REPORT_CANONICAL,
            "Runtime refinement changed retained c879 OFF/reference/Rue/source/ABI/native/order/coverage/outcomes/provenance/ledger or unknown metadata")
    return result


def preserve(report):
    # Current source/typed/ledger checks execute exactly once, never on history.
    cost.validate_current(report)
    return project_validated(cost.project_append(report))


def capture_after(report):
    """Pure candidate extraction; neither writes nor authorizes a profile."""
    return inline.capture_after(report)
