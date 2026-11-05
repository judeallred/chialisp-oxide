"""Explicit signed anchor and unsigned reviewed checkpoint composition.

Recorded consistency is not execution authentication, a signature verifier,
compiler preservation or a universal security/cost claim. Current validators
belong to the caller; returned historical objects are pure comparison values.
The preceding guarded v1 profile stays unchanged and pending.
"""
import copy
import json
from pathlib import Path

from cargo_authoring import contained_file, snapshot, unique_object
import p2_append_migration as append
import p2_singleton_evidence as ledger
import p2_source_cost_migration as source
import runtime_cost_migration as cost
import runtime_guarded_projection_migration as guarded

ROOT = Path(__file__).resolve().parents[1]
PATH = "corpus/runtime-composition-baseline.json"
PIN = "8024f7430fb8378031f0864336267b9584a458d25e89c54f2405385e37f3c01b"
PENDING = "pending_actual_composition_capture_review"
FINAL = "reviewed_final_runtime_composition"
PROTOCOL = "chialisp-oxide-runtime-composition/v1"
METRICS = "corpus/runtime-composition-costs.json"
CONTROLS = "corpus/runtime-composition-controls.json"
IMMUTABLE = {**guarded.IMMUTABLE, guarded.PATH: guarded.PIN}
ANCHOR = {"kind": "signed_git_checkpoint", "repository": "https://github.com/judeallred/chialisp-oxide",
    **source.BASELINE}
INTERMEDIATE = {"kind": "unsigned_reviewed_content_addressed_checkpoint",
    "checkpoint_path": guarded.CHECKPOINT, "report_sha256": guarded.BEFORE_RAW,
    "canonical_report_sha256": guarded.BEFORE_CANONICAL,
    "authorizing_transition_path": "corpus/p2-source-cost-baseline.json",
    "authorizing_transition_sha256": source.PIN}
SDK = "crates/chialisp-oxide/src/lib.rs"
CONS = "crates/chialisp-oxide/src/cons.rs"
AUTHORING = "puzzles/authoring.json"
OLD_SDK = "91ef20602b10762149ed4af95b516a77aaf99bf13f959d27830ecfbac769f314"
NEW_SDK = "b8971aebdb39c998846b7b82019a3d7ca3cfe3d9999c8a8bd92efa609aeccc64"
NEW_CONS = "8e624e3d9f888b2de8d713bbe4ddfaa19bc9d08747ddd3e6c8cc2fda05cc99a2"
OLD_AUTHORING = "c451f3e0362a019b1cd5362073ed9f531b8a02375c4dbcf7d562a3c2c9732f51"
NEW_AUTHORING = "03aeab62d690eba4b99af2c108a7312675fd6606b9afe2ea17c049b9478c57f0"
SDK_SLUGS = sorted({"fixed-payment", "capped-payment", "split-payment", "timelocked-payment",
    "signature-payment", "two-signature-payment", "recovery-payment", "boolean-gate", "p2-conditions",
    "rightmost-tree", "last-list-element", "prefix-list", "insertion-sort-by-keys", "sha256tree",
    "singleton-launcher", "password-coin", "p2-singleton"})
METADATA = {"added": {CONS: NEW_CONS}, "changed": {
    SDK: {"before": OLD_SDK, "current": NEW_SDK},
    AUTHORING: {"before": OLD_AUTHORING, "current": NEW_AUTHORING}},
    "embedded_sdk_copy_slugs": SDK_SLUGS}
require, sha, canonical, same = cost.require, cost.sha, cost.canonical, cost.same
report_payload = guarded.report_payload


def read(path, *, limit=4 * 1024 * 1024):
    return report_payload(contained_file(ROOT, path)) if limit == 16 * 1024 * 1024 else _small(path, limit)


def _small(path, limit):
    payload = report_payload(contained_file(ROOT, path))
    require(len(payload) <= limit, "Composition evidence byte bound: " + path)
    return payload


def load(payload=None):
    payload = read(PATH) if payload is None else payload
    require(sha(payload) == PIN, "Composition reviewed profile bytes changed")
    value = json.loads(payload, object_pairs_hook=unique_object)
    ledger.closed(value, ["schema_version", "protocol", "status", "scope", "signed_anchor",
        "reviewed_intermediate", "immutable_profiles", "implementation_transition",
        "optimization_pipeline", "reviewed_after"], "composition profile")
    require(type(value["schema_version"]) is int and value["schema_version"] == 1
        and value["protocol"] == PROTOCOL and value["status"] in (PENDING, FINAL)
        and type(value["scope"]) is str and value["scope"], "Unsupported composition profile")
    require(same(value["signed_anchor"], ANCHOR) and same(value["reviewed_intermediate"], INTERMEDIATE)
        and same(value["immutable_profiles"], IMMUTABLE), "Changed signed anchor/unsigned checkpoint/profile identity")
    for path, pin in IMMUTABLE.items():
        require(sha(read(path, limit=16 * 1024 * 1024)) == pin, "Changed immutable composition predecessor: " + path)
    implementation = value["implementation_transition"]
    ledger.closed(implementation, ["source_metadata", "compiler_sources", "pipeline_scope"], "composition implementation")
    require(same(implementation["source_metadata"], METADATA)
        and implementation["pipeline_scope"] == "Recorded eleven-stage metadata overlaps preceding guarded/Atom/unary variants; exact reviewed source and capture identities, not that label, select this composition.",
        "Changed composition metadata/experimental pipeline scope")
    rows = implementation["compiler_sources"]
    require(type(rows) is list and len(rows) == 4 and [r["path"] for r in rows] == [
        "crates/chialisp-oxide-ir/src/lower.rs", "crates/chialisp-oxide-ir/src/lower/runtime.rs",
        "crates/chialisp-oxide-ir/tests/runtime_atom_facts.rs", "crates/chialisp-oxide-ir/tests/runtime_unary_frames.rs"],
        "Changed composition compiler source inventory")
    for row in rows:
        ledger.closed(row, ["path", "sha256"], "composition compiler input")
        require(source.digest(row["sha256"]), "Invalid composition compiler source hash")
    require(value["optimization_pipeline"] == {"before": guarded.OLD_PIPELINE, "after": guarded.NEW_PIPELINE},
        "Changed composition pipeline metadata")
    old = source.load()
    require(old["baseline"] == source.BASELINE and old["status"] == "reviewed_final_source_cost"
        and old["reviewed_after"]["report_sha256"] == INTERMEDIATE["report_sha256"]
        and old["reviewed_after"]["canonical_report_sha256"] == INTERMEDIATE["canonical_report_sha256"],
        "Unsigned checkpoint not bound to immutable signed-anchor transition")
    require(sha(read(source.METRICS_PATH)) == old["reviewed_after"]["metrics_sha256"]
        and sha(read(source.CONTROLS_PATH)) == old["reviewed_after"]["controls_sha256"],
        "Changed immutable unsigned M1 metric/control bytes")
    require(value["reviewed_after"] is None if value["status"] == PENDING
        else type(value["reviewed_after"]) is dict, "Mixed pending/final composition metadata")
    if value["status"] == FINAL:
        capture_fields(value["reviewed_after"])
    return value


def predecessor(profile=None):
    """Read exact raw unsigned M1 bytes only; do not project history here."""
    profile = load() if profile is None else profile
    payload = read(INTERMEDIATE["checkpoint_path"], limit=16 * 1024 * 1024)
    require(sha(payload) == INTERMEDIATE["report_sha256"], "Changed unsigned complete M1 checkpoint bytes")
    value = json.loads(payload, object_pairs_hook=unique_object)
    require(sha(canonical(value)) == INTERMEDIATE["canonical_report_sha256"]
        and value["optimization_pipeline"] == guarded.OLD_PIPELINE, "Changed unsigned M1 checkpoint object")
    return value


def anchor_checkpoint(intermediate):
    """Explicit pure M1→E20 link, after current/fresh checks or for history only."""
    require(sha(canonical(intermediate)) == INTERMEDIATE["canonical_report_sha256"],
        "Anchor requires the exact unsigned M1 checkpoint")
    result = source.project_validated(intermediate)
    require(sha(canonical(result)) == ANCHOR["canonical_report_sha256"],
        "Unsigned M1 does not restore the actual signed E20 anchor")
    return result


def capture_fields(after):
    """Closed metadata also gates final historical-only reads, without a report."""
    ledger.closed(after, ["report_sha256", "canonical_report_sha256", "p2_sha256", "protocol_sha256",
        "raw_ir_sha256", "metrics_sha256", "controls_sha256", "bundles", "delta"], "composition capture")
    require(all(source.digest(after[k]) for k in after if k not in ("bundles", "delta")),
        "Missing actual composition capture identity")
    ledger.closed(after["bundles"], ["on", "off"], "composition paired capture")
    for name, mode in (("on", True), ("off", False)):
        row = after["bundles"][name]
        ledger.closed(row, ["manifest_sha256", "optimize_lir"], "composition mode")
        require(source.digest(row["manifest_sha256"]) and row["optimize_lir"] is mode, "Invalid composition mode")
    ledger.closed(after["delta"], ["leaf_count", "inventory_sha256"], "composition delta")
    require(type(after["delta"]["leaf_count"]) is int and 0 < after["delta"]["leaf_count"] <= 100_000
        and source.digest(after["delta"]["inventory_sha256"]), "Invalid complete composition delta")
    require(after["raw_ir_sha256"] == source.load()["reviewed_after"]["raw_ir_sha256"],
        "Composition changed retained source IR identity")


def reviewed_after(report):
    profile = load()
    require(profile["status"] == FINAL, "Runtime composition pending actual capture/review")
    after = profile["reviewed_after"]
    capture_fields(after)
    require(report["optimization_pipeline"] == guarded.NEW_PIPELINE
        and sha(canonical(report)) == after["canonical_report_sha256"], "Unreviewed actual composition report")
    p2 = report["puzzles"][-1]
    require(p2["slug"] == append.ADDED and sha(canonical(p2)) == after["p2_sha256"]
        and sha(canonical(p2["protocol_validation"])) == after["protocol_sha256"], "Changed composition P2 capture")
    return profile, after


def source_map(authoring):
    rows = authoring["sources"]
    require(type(rows) is list, "Invalid authoring sources")
    for row in rows:
        ledger.closed(row, ["path", "sha256"], "authoring source")
        require(type(row["path"]) is str and source.digest(row["sha256"]), "Invalid authoring source identity")
    require([r["path"] for r in rows] == sorted(set(r["path"] for r in rows)), "Duplicate/reordered authoring sources")
    return {r["path"]: r["sha256"] for r in rows}


def normalize_metadata(before, current, *, current_snapshot):
    """Pure closed Cons record relation; current source checks are separate.

    Exact outer profile/report hashes authorize historical use after a future
    caller validates its newer current corpus. No files or source snapshot are
    read here, and this function never grants current-source admission.
    """
    require(same(current["authoring"], current_snapshot), "Current source snapshot differs from report")
    old, new = source_map(before["authoring"]), source_map(current_snapshot)
    require(old.get(SDK) == OLD_SDK and old.get(AUTHORING) == OLD_AUTHORING and CONS not in old,
        "Changed pre-Cons authoring identity")
    expected = {**old, SDK: NEW_SDK, AUTHORING: NEW_AUTHORING, CONS: NEW_CONS}
    require(new == expected, "Unreviewed Cons source additions/changes")
    require(same({k:v for k,v in before["authoring"].items() if k != "sources"},
        {k:v for k,v in current_snapshot.items() if k != "sources"}), "Changed non-source authoring metadata")
    require(len(current["puzzles"]) == len(before["puzzles"]) == 22
        and [p["slug"] for p in current["puzzles"]] == [p["slug"] for p in before["puzzles"]], "Changed complete22 order")
    result, copies = copy.deepcopy(current), []
    result["authoring"] = copy.deepcopy(before["authoring"])
    for old_puzzle, new_puzzle, output in zip(before["puzzles"], current["puzzles"], result["puzzles"], strict=True):
        a, b = old_puzzle["source_dependencies"], new_puzzle["source_dependencies"]
        require([r["path"] for r in a] == [r["path"] for r in b]
            and len({r["path"] for r in a}) == len(a), "Changed dependency identity/order")
        for index, (old_row, new_row) in enumerate(zip(a, b, strict=True)):
            if old_row["path"] != SDK:
                require(same(old_row, new_row), "Changed other source dependency")
                continue
            require(old_puzzle["slug"] in SDK_SLUGS and old_row == {"path": SDK, "sha256": OLD_SDK, "source": old_row["source"]}
                and sha(old_row["source"].encode()) == OLD_SDK
                and new_row == {"path": SDK, "sha256": NEW_SDK, "source": new_row["source"]}
                and type(new_row["source"]) is str and sha(new_row["source"].encode()) == NEW_SDK,
                "Changed embedded SDK source/hash")
            output["source_dependencies"][index] = copy.deepcopy(old_row)
            copies.append(old_puzzle["slug"])
    require(sorted(copies) == SDK_SLUGS, "Changed exact seventeen SDK copies")
    return result


def compare_delta(before, current, *, current_snapshot):
    normalized = normalize_metadata(before, current, current_snapshot=current_snapshot)
    guarded.compare_compiler_delta(before, normalized)  # Pure, does not load/admit pending v1.
    delta = guarded.differences(before, current)
    return {"leaf_count": len(delta), "inventory_sha256": sha((json.dumps(delta, indent=2) + "\n").encode())}


def project_validated(report):
    """Pure whole final22 to exact UNSIGNED M1; current validation is prerequisite."""
    profile, after = reviewed_after(report)
    previous = predecessor(profile)
    require(compare_delta(previous, report, current_snapshot=report["authoring"]) == after["delta"],
        "Unreviewed complete composition delta")
    anchor_checkpoint(previous)
    return previous


def check_report_bytes(payload, report):
    require(sha(payload) == reviewed_after(report)[1]["report_sha256"], "Changed actual composition report bytes")


def hash_diagnostic_profile(report):
    reviewed_after(report)
    for puzzle in report["puzzles"]:
        if puzzle["slug"] in ("sha256tree", "singleton-launcher"):
            cost.artifact_bytes(puzzle["artifacts"][1])
    return True


def verify_fresh_bundles(report, on, off, **identities):
    _, after = reviewed_after(report)
    expected = {row["entry"]: row["sha256"] for row in append.load()["raw_ir_before"]}
    expected[source.ENTRY] = source.load()["reviewed_after"]["raw_ir_sha256"]
    require(after["raw_ir_sha256"] == expected[source.ENTRY], "Composition changed retained source IR")
    return append.check_fresh_bundles(report, on, off, expected_ir=expected,
        bundle_pins=after["bundles"], **identities)


def check_sidecars(report):
    profile, after = reviewed_after(report)
    require(same(snapshot(ROOT), report["authoring"]), "Stale actual composition authoring snapshot")
    for row in profile["implementation_transition"]["compiler_sources"]:
        require(sha(read(row["path"])) == row["sha256"], "Stale current composition compiler source")
    for path, pin in ((SDK, NEW_SDK), (CONS, NEW_CONS), (AUTHORING, NEW_AUTHORING)):
        require(sha(read(path)) == pin, "Stale current Cons implementation")
    normalize_metadata(predecessor(profile), report, current_snapshot=report["authoring"])
    metrics, controls = read(METRICS), read(CONTROLS)
    require(sha(metrics) == after["metrics_sha256"] and sha(controls) == after["controls_sha256"], "Changed composition sidecars")
    counts = source.validate_controls(report, json.loads(controls, object_pairs_hook=unique_object))
    values = json.loads(metrics, object_pairs_hook=unique_object)
    measured = source.validate_metrics(report, values)
    previous = predecessor()
    old_metrics = json.loads(read(source.METRICS_PATH), object_pairs_hook=unique_object)
    source.validate_metrics(previous, old_metrics)  # Pure recorded arithmetic, no current-source check.
    guarded.compare_metrics(previous, report, old_metrics, values)
    return {**counts, **measured}


def checkpoint_recorded():
    """Pinned unsigned intermediate + signed-anchor link, no current-source check."""
    profile = load()
    require(profile["status"] == FINAL, "Runtime composition pending actual capture/review")
    previous = predecessor(profile)
    anchor_checkpoint(previous)
    return previous
