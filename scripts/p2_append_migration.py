"""Closed P2 append projection prototype; current22 validation is a prerequisite.

This module NEVER validates typed outcomes or submits ledger transactions.
Its pure projection must follow current22 source/native/typed/dedicated-ledger
validation. Explicit fresh ON/OFF bundle checking also precedes projection
when requested by a CLI. Existing cost profiles
remain immutable; current validators must never run on projected history.
"""
import copy
import json
import re
from pathlib import Path

from cargo_authoring import check_recorded, contained_file, unique_object
from cargo_bundle import load_declared_export
import runtime_cost_migration as cost
import runtime_refinement_migration as refinement

ROOT = Path(__file__).resolve().parents[1]
PATH = ROOT / "corpus/p2-singleton-append-baseline.json"
PIN = "39665eb448e5b7ed9cb2eb0e987b597a4b811a68fb3f37f0e4e151e6ab9374a8"
COMMIT = "bb2c851dc9cd73c69acd4da905bae93874b99d5d"
REPORT_SHA = "acbf197e3e20ed96ab9d3c80e5394b5301f1d9e790bc36865a5bdc61867fc114"
REPORT_CANONICAL = "1921433a1575fb622b7fabd8298bb0593efe8a02b65aef896e7ed442dc58fb06"
ADDED = "p2-singleton"
CHANGED = ["puzzles/lib.rs", "puzzles/authoring.json",
           "crates/chialisp-oxide-corpus/src/lib.rs", "crates/chialisp-oxide-corpus/src/native.rs"]
AFFECTED = ["sha256tree", "singleton-launcher", "password-coin"]
require, sha, canonical, same = cost.require, cost.sha, cost.canonical, cost.same


def load(payload=None):
    payload = PATH.read_bytes() if payload is None else payload
    require(sha(payload) == PIN, "P2 append reviewed profile bytes changed")
    value = json.loads(payload, object_pairs_hook=unique_object)
    require(set(value) == {"schema_version", "protocol", "scope", "baseline", "predecessor",
                          "preserved_slugs", "added_slug", "authoring", "native_source_before",
                          "raw_ir_before", "reviewed_after"}
            and type(value["schema_version"]) is int and value["schema_version"] == 1
            and value["protocol"] == "chialisp-oxide-p2-singleton-append/v1", "Unsupported P2 append profile")
    require(value["baseline"] == {"repository": "https://github.com/judeallred/chialisp-oxide",
            "commit": COMMIT, "tree_sha1": "6909d61b916144bd7051f824dd88561e2dbf7358",
            "report_path": "corpus/validation.json", "report_sha256": REPORT_SHA,
            "report_git_blob_sha1": "5f6b99bc6f003b3f4a9f2af257acdf926fc7c5eb",
            "canonical_report_sha256": REPORT_CANONICAL}, "Changed signed cost21 baseline identity")
    require(value["predecessor"] == {"path": "corpus/runtime-refinement-baseline.json", "sha256": refinement.PIN}
            and value["added_slug"] == ADDED and value["authoring"]["changed_existing_inputs"] == CHANGED,
            "Changed immutable predecessor/append registration scope")
    refinement.load()  # Independent immutable predecessor byte pin.
    old = cost.load()
    require(value["preserved_slugs"] == [p["slug"] for p in old["puzzles"]]
            and same(value["authoring"]["before"], old["authoring"])
            and value["raw_ir_before"] == old["raw_ir"]
            and list(value["native_source_before"]) == sorted(AFFECTED),
            "Changed reviewed21 source/IR/native identities")
    return value


def is_append(report):
    """Dispatch by exact append identity, before unchanged pipeline selection."""
    return any(p.get("slug") == ADDED for p in report.get("puzzles", []))


def reviewed_after(report):
    profile = load()
    after = profile["reviewed_after"]
    require(after.get("status") == "reviewed_final_current22",
            "P2 append pending actual current22 integration/validation; no promotion")
    require(set(after) == {"status", "report_path", "report_sha256", "canonical_report_sha256",
                         "authoring", "native_source_after", "added_puzzle_sha256", "raw_ir_added_sha256",
                         "bundles", "dedicated_evidence"}, "Unknown P2 append reviewed fields")
    for key in ("report_sha256", "canonical_report_sha256", "added_puzzle_sha256", "raw_ir_added_sha256"):
        require(isinstance(after[key], str) and re.fullmatch(r"[0-9a-f]{64}", after[key]),
                "Missing P2 final report/IR identity")
    require(after["report_path"] == "corpus/validation.json"
            and sha(canonical(report)) == after["canonical_report_sha256"], "Unreviewed current22 report bytes")
    rows = [p for p in report["puzzles"] if p.get("slug") == ADDED]
    require(len(rows) == 1, "Missing/duplicate current22 protocol component")
    dedicated = after["dedicated_evidence"]
    require(type(dedicated) is dict and set(dedicated) == {"protocol_validation_sha256", "read_protocol",
            "independent_named_cases", "ordinary_generated_cases", "scope"}
            and dedicated["read_protocol"] == "chialisp-oxide-p2-singleton/v1"
            and type(dedicated["independent_named_cases"]) is int and dedicated["independent_named_cases"] == 50
            and type(dedicated["ordinary_generated_cases"]) is int and dedicated["ordinary_generated_cases"] == 2187
            and isinstance(dedicated["scope"], str) and dedicated["scope"], "Malformed dedicated current22 capture scope")
    require(dedicated["protocol_validation_sha256"] == sha(canonical(rows[0]["protocol_validation"]))
            and len(rows[0]["cases"]) == dedicated["independent_named_cases"]
            and rows[0]["generated_cases"] == dedicated["ordinary_generated_cases"],
            "Dedicated current22 recorded protocol/named/generated bindings differ")
    return profile, after


def authoring_transition(profile, after):
    before, current = profile["authoring"]["before"], after["authoring"]
    require(set(before) == set(current) and same({k:v for k,v in before.items() if k not in ("exports", "sources")},
            {k:v for k,v in current.items() if k not in ("exports", "sources")}), "Changed Cargo authoring contract")
    exports = copy.deepcopy(before["exports"])
    exports[ADDED] = {"entry": "chialisp_oxide_puzzles::p2_singleton::puzzle", "source": "puzzles/p2-singleton/puzzle.rs"}
    require(same(current["exports"], exports), "Append permits exactly one new Cargo export")
    a = {r["path"]:r for r in before["sources"]}
    b = {r["path"]:r for r in current["sources"]}
    require(len(a) == len(before["sources"]) and len(b) == len(current["sources"])
            and [r["path"] for r in current["sources"]] == sorted(b), "Duplicate/reordered Cargo inputs")
    added = profile["authoring"]["prototype_added_inputs"] + profile["authoring"]["reviewed_integration_added_inputs"]
    require(len(added) == len(set(added)) and set(a) < set(b) and set(b) - set(a) == set(added),
            "Missing/extra/unreviewed added Cargo inputs")
    require([path for path in CHANGED if path in a and a[path] != b[path]] == CHANGED
            and {p for p in a if a[p] != b[p]} == set(CHANGED), "Changed unreviewed existing Cargo input")
    for row in b.values():
        require(set(row) == {"path", "sha256"} and re.fullmatch(r"[0-9a-f]{64}", row["sha256"]),
                "Malformed current22 source fingerprint")


def expected_patches(report):
    """Current append selection before unchanged optimizer pipeline dispatch."""
    reviewed_after(report)
    return refinement.PATCHES


def current_native_sources(report):
    """Check actual current22 native declarations/bytes, never projected history."""
    profile, _ = reviewed_after(report)
    check_recorded(report["authoring"], ROOT, profile["preserved_slugs"] + [ADDED])
    inputs = {row["path"]:row["sha256"] for row in report["authoring"]["sources"]}
    for puzzle in report["puzzles"]:
        manifest = json.loads(contained_file(ROOT, f"puzzles/{puzzle['slug']}/puzzle.json").read_bytes(),
                              object_pairs_hook=unique_object)
        paths = manifest.get("native_sources", [])
        rows = puzzle.get("native_sources", [])
        require(type(paths) is list and len(paths) == len(set(paths))
                and type(rows) is list and [row["path"] for row in rows] == paths,
                "Current22 native closure missing/duplicate/unknown paths")
        for row, path in zip(rows, paths, strict=True):
            require(set(row) == {"path", "sha256"} and path in inputs
                    and row["sha256"] == inputs[path] == sha(contained_file(ROOT, path).read_bytes()),
                    "Current22 actual native source/declared Cargo hash mismatch: " + path)
    return True


def hash_diagnostic_profile(report):
    """Current appended hash slots; no current validation runs on history."""
    reviewed_after(report)
    return refinement.hash_diagnostic_profile(report)


def project_validated(report):
    """Pure historical projection ONLY. Current22 validation must precede it."""
    profile, after = reviewed_after(report)
    require([p["slug"] for p in report["puzzles"]] == profile["preserved_slugs"] + [ADDED],
            "Missing/duplicate/extra/reordered append fixture")
    require(same(report["authoring"], after["authoring"]), "Changed reviewed current22 authoring")
    authoring_transition(profile, after)
    require(sha(canonical(report["puzzles"][-1])) == after["added_puzzle_sha256"],
            "Unreviewed P2 puzzle source/artifact/outcome/protocol evidence")
    result = copy.deepcopy(report)
    result["puzzles"].pop()
    result["authoring"] = copy.deepcopy(profile["authoring"]["before"])
    old_native, new_native = profile["native_source_before"], after["native_source_after"]
    require(set(new_native) == set(old_native) == set(AFFECTED), "Changed native transition scope")
    observed = []
    for puzzle in result["puzzles"]:
        slug = puzzle["slug"]
        if slug not in old_native:
            continue
        require(same(puzzle["native_sources"], new_native[slug]), "Unreviewed current native closure")
        before = old_native[slug]
        current = puzzle["native_sources"]
        require([r["path"] for r in before] == [r["path"] for r in current], "Changed retained native closure paths")
        for a, b in zip(before, current, strict=True):
            if a["path"] in CHANGED[2:]:
                require(a != b and set(b) == {"path", "sha256"}, "Missing native implementation transition")
            else:
                require(a == b, "Changed unreviewed native implementation fingerprint")
        puzzle["native_sources"] = copy.deepcopy(before)
        observed.append(slug)
    require(observed == AFFECTED, "Missing/extra/reordered native transition fixture")
    require(sha(canonical(result)) == REPORT_CANONICAL,
            "P2 append changed retained21 IR/OFF/ON/reference/Rue/source/ABI/coverage/native/outcome/ledger or unknown metadata")
    return result


def verify_fresh_bundles(report, on, off, *, compiler, driver_sha256, toolchain, cargo_options):
    """Check actual current22 before projection; never extract or authenticate.

    Compiler identity/driver hash must be captured independently by the caller;
    manifest self-description cannot supply these expected values.
    """
    profile, after = reviewed_after(report)
    check_recorded(report["authoring"], ROOT, profile["preserved_slugs"] + [ADDED])
    expected_ir = {r["entry"]:r["sha256"] for r in profile["raw_ir_before"]}
    expected_ir["chialisp_oxide_puzzles::p2_singleton::puzzle"] = after["raw_ir_added_sha256"]
    require(type(after["bundles"]) is dict and set(after["bundles"]) == {"on", "off"}, "Missing fresh22 modes")
    puzzles = {e["entry"]:p for slug,e in report["authoring"]["exports"].items()
               for p in report["puzzles"] if p["slug"] == slug}
    results = []
    for optimize, output in ((True, on), (False, off)):
        loaded = load_declared_export(output, root=ROOT, authoring=report["authoring"],
            selected_entry="chialisp_oxide_puzzles::p2_singleton::puzzle", compiler=compiler,
            driver_sha256=driver_sha256, toolchain=toolchain,
            current_authoring=lambda: __import__("cargo_authoring").snapshot(ROOT), optimize=optimize,
            rue_revision=report["rue_revision"], cargo_options=cargo_options)
        pin = after["bundles"]["on" if optimize else "off"]
        require(type(pin) is dict and set(pin) == {"manifest_sha256", "optimize_lir"}
                and isinstance(pin["manifest_sha256"], str) and re.fullmatch(r"[0-9a-f]{64}", pin["manifest_sha256"])
                and type(pin["optimize_lir"]) is bool and pin["optimize_lir"] is optimize,
                "Malformed recorded fresh22 manifest/mode metadata")
        entries = loaded.manifest["entries"]
        require(len(entries) == 22 and {r["entry"] for r in entries} == set(expected_ir), "Not exact22 fresh exports")
        for row in entries:
            require(sha((loaded.manifest_path.parent / row["ir"]).read_bytes()) == expected_ir[row["entry"]],
                    "Changed retained21/new22 actual rawIR")
            matching = [a for a in puzzles[row["entry"]]["artifacts"] if a["backend"] == "rust_mir"
                        and a["optimize_lir"] is optimize]
            require(len(matching) == 1 and same(matching[0], row["artifact"]), "Fresh current22 artifact/report mismatch")
        results.append({"mode": optimize, "exports":len(entries), "retained_old_raw_IR":21,
            "fresh_manifest_sha256":sha(loaded.manifest_path.read_bytes()),
            "recorded_manifest_sha256":pin["manifest_sha256"],
            "scope":"Current independently queried compiler/driver/source/rawIR/artifact/mode consistency; recorded manifest hash is capture metadata, not a cross-host admission pin"})
    return results
