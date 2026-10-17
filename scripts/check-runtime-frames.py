#!/usr/bin/env python3
"""Check a pinned finite runtime-frame performance baseline and replay old CLVM.

This compares recorded artifacts/observations, not arbitrary program equivalence,
termination or universal cost dominance. Replay executes retained old bytes only;
it does not rebuild the historical compiler or run native recursive policies.
The owned-helper extension keeps every original15 measurement byte-exact against
a separately reviewed report snapshot, while admitting only two pinned SDK
source transitions and freshness-checked Cargo authoring metadata.
The byte-equality extension additionally pins every observation from the
reviewed16-fixture owned-tree snapshot before admitting one new helper.
The byte-ordering extension first pins the full reviewed17-fixture report,
excluding only independently freshness-checked Cargo authoring metadata.
Rue issues112 and113 first pin the complete reviewed18-fixture report while
allowing only their two exact patch-provenance URLs and checked authoring data.
The hashing extension preserves those18 observations while appending only the
two reviewed fixtures and normalizing one exact facade source transition.
The password extension preserves all20 observations while appending only its
raw template and separately labeled instances. Only two exact reviewed native
source metadata transitions are normalized, after full-current closure checks.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import runpy
import subprocess
import tempfile

from vm_failures import check_execution
from runtime_hash_migration import controls as hashing_controls, preserve as hashing_preservation
from runtime_password_migration import (controls as password_controls, current_native_sources,
                                        preserve as password_preservation)
from password_evidence import validate as password_evidence

ROOT = Path(__file__).resolve().parents[1]
COMMIT = "6f1e197ef6647683f4c3d7dd171c5e7e4317b01c"
REPORT_PATH = "corpus/validation.json"
REPORT_SHA256 = "96b0f8f5bf18eb132f787fb35f12299b9bdcdad35afbd19eddc9bbfca2e702eb"
REPORT_BLOB = "67d2e63db7b47545dd3efcf83d60e49b202ae3fa"
BASELINE_PATH = ROOT / "corpus/runtime-frame-baseline.json"
BASELINE_SHA256 = "99c5492bd34be6ae6da5ccdc67315b7c2f0dc831fdca8e99edd869cec5610f4a"
MIGRATION_PATH = ROOT / "corpus/runtime-frame-owned-migration.json"
MIGRATION_SHA256 = "93e90c59ca3e0734b863b5612c929ec7897a29f41ede65a2f3be28b3e28ed252"
REVIEWED_REPORT_SHA256 = "14b37c5dcca3d402a9e2264ae956ca4f76aa4aa0b046d1e0c3b02f5cf2b2c61c"
EQUALITY_MIGRATION_PATH = ROOT / "corpus/atom-equality-migration.json"
EQUALITY_MIGRATION_SHA256 = "8b1e569f89e062b28ea289204d1c4092157163bc2767e766243e639a7f0f380d"
OWNED_REPORT_SHA256 = "7fc4f55de1b65cf0f004d5e51bd2081d19a6af5be118cd6264c028318577738d"
ORDERING_MIGRATION_PATH = ROOT / "corpus/byte-ordering-migration.json"
ORDERING_MIGRATION_SHA256 = "9f25db49a076e77928467d5eb0095cf2282dfcd1e33dbd4335472f36f7397c7d"
EQUALITY_REPORT_SHA256 = "d83e31b89e3a9f02623897618c7d6887b6739476e4052475ea0a90e9b7db9a7a"
BITWISE_MIGRATION_PATH = ROOT / "corpus/bitwise-patch-migration.json"
BITWISE_MIGRATION_SHA256 = "003011ff9cd47c9163c52e606e304b529bac81146777b15e96eca564084e5522"
ORDERING_REPORT_SHA256 = "946453ad036eed0cc75810ae38c1b20c488f02c3f5b8f62f844bb904ed936148"
SDK_TRANSITIONS = ("crates/chialisp-oxide/src/lib.rs", "crates/chialisp-oxide/src/tree.rs")
SLUGS = ("rightmost-tree", "last-list-element")
MAX_COST = 1_000_000
readable = runpy.run_path(str(ROOT / "scripts/check-readable-puzzles.py"))
codec = runpy.run_path(str(ROOT / "scripts/check-canonical-factories.py"))


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha256(value):
    return hashlib.sha256(value).hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def corpus_counts(report):
    puzzles = report["puzzles"]
    scenarios = [p["simulator"] for p in puzzles]
    runs = scenarios + [r["scenarios"] for p in puzzles for r in p["simulator_comparisons"]]
    return {"slugs": [p["slug"] for p in puzzles],
            "named_cases": sum(len(p["cases"]) for p in puzzles),
            "generated_cases": sum(p["generated_cases"] for p in puzzles),
            "ledger_groups": sum(map(len, scenarios)), "ledger_scenario_runs": sum(map(len, runs)),
            "consensus_rejections": sum(len(s["rejections"]) for rows in runs for s in rows)}


def from_report(payload):
    require(sha256(payload) == REPORT_SHA256, "Historical report SHA-256 differs from the reviewed commit")
    require(hashlib.sha1(f"blob {len(payload)}\0".encode() + payload).hexdigest() == REPORT_BLOB,
            "Historical report Git blob differs from the reviewed commit")
    report = json.loads(payload, object_pairs_hook=readable["object_without_duplicates"])
    require(report["optimization_pipeline"] == readable["PREVIOUS_PIPELINE"], "Wrong baseline optimization pipeline")
    readable["check_report"]({**report, "optimization_pipeline": readable["PIPELINE"]})
    puzzles = []
    for slug in SLUGS:
        puzzle = next(p for p in report["puzzles"] if p["slug"] == slug)
        puzzles.append({
            "slug": slug, "source_path": f"puzzles/{slug}/puzzle.rs", "source_sha256": sha256(puzzle["source"].encode()),
            "artifacts": [a for a in puzzle["artifacts"] if a["backend"] == "rust_mir"],
            "cases": [{"label": c["label"], "solution": c["solution"], "expected": c["native"],
                       "runs": [r for r in c["runs"] if r["backend"] == "rust_mir"]} for c in puzzle["cases"]],
        })
    return {
        "schema_version": 1,
        "baseline": {"repository": "https://github.com/judeallred/chialisp-oxide", "commit": COMMIT,
                     "report": {"path": REPORT_PATH, "sha256": REPORT_SHA256, "git_blob_sha1": REPORT_BLOB}},
        "scope": "Two recursive pure helpers; finite named-case measurements. No termination, universal cost-dominance or compiler-preservation proof.",
        **{key: report[key] for key in ("toolchain", "execution_vm", "vm_failure_protocol", "rue_revision", "rue_local_patches", "optimization_pipeline")},
        "max_cost": MAX_COST, "corpus": corpus_counts(report),
        # A canonical digest retains every source/contract/reference/off/other
        # observation without copying the entire historical report into Git.
        "preserved_report_sha256": sha256(canonical(readable["runtime_frames_behavior"](report))),
        "puzzles": puzzles,
    }


def artifact_valid(artifact):
    data = bytes.fromhex(artifact["clvm_hex"])
    require(artifact["bytes"] == len(data), "Baseline/current artifact size mismatch")
    require(codec["tree_hash"](codec["deserialize"](data)).hex() == artifact["tree_hash"], "Artifact tree hash mismatch")


def load_baseline(payload):
    require(sha256(payload) == BASELINE_SHA256, "Compact baseline bytes differ from the reviewed snapshot")
    return json.loads(payload, object_pairs_hook=readable["object_without_duplicates"])


def load_migration(payload):
    require(sha256(payload) == MIGRATION_SHA256, "Reviewed owned-helper migration snapshot changed")
    migration = json.loads(payload, object_pairs_hook=readable["object_without_duplicates"])
    require(migration["schema_version"] == 1 and migration["added_slug"] == "prefix-list",
            "Unsupported runtime-frame migration")
    require(migration["historical"]["commit"] == COMMIT
            and migration["historical"]["report_sha256"] == REPORT_SHA256,
            "Migration historical provenance changed")
    require(migration["reviewed_before"]["status"] == "reviewed_uncommitted_snapshot"
            and migration["reviewed_before"]["report_sha256"] == REVIEWED_REPORT_SHA256,
            "Migration reviewed report identity changed")
    require(tuple(row["path"] for row in migration["sdk_dependencies"]) == SDK_TRANSITIONS,
            "Migration must identify only the two reviewed SDK snapshots")
    for dependency in migration["sdk_dependencies"]:
        for version in ("before", "after"):
            snapshot = dependency[version]
            require(set(snapshot) == {"sha256", "source"}
                    and sha256(snapshot["source"].encode()) == snapshot["sha256"],
                    "Migration SDK snapshot hash/source mismatch")
    return migration


def without_authoring(report):
    result = copy.deepcopy(report)
    result.pop("authoring")
    return result


def load_equality_migration(payload):
    require(sha256(payload) == EQUALITY_MIGRATION_SHA256,
            "Reviewed byte-equality migration snapshot changed")
    migration = json.loads(payload, object_pairs_hook=readable["object_without_duplicates"])
    require(migration["schema_version"] == 1 and migration["added_slug"] == "atom-not-equal",
            "Unsupported byte-equality migration")
    require(migration["reviewed_before"]["status"] == "reviewed_uncommitted_snapshot"
            and migration["reviewed_before"]["report_sha256"] == OWNED_REPORT_SHA256,
            "Byte-equality migration reviewed report identity changed")
    return migration


def byte_equality_preservation(report, old_slugs, migration=None):
    if migration is None:
        migration = load_equality_migration(EQUALITY_MIGRATION_PATH.read_bytes())
    require(migration["preserved_slugs"] == old_slugs + ["prefix-list"],
            "Byte-equality migration original16 identities changed")
    require([p["slug"] for p in report["puzzles"]] == migration["preserved_slugs"] + ["atom-not-equal"],
            "Byte-equality migration permits only atom-not-equal appended to the original16 fixtures")
    retained = copy.deepcopy(report)
    retained["puzzles"].pop()
    require(sha256(canonical(without_authoring(retained))) == migration["reviewed_before"]["normalized_report_sha256"],
            "An original16 source, contract, artifact, cost, failure, order or ledger record changed")
    return retained


def load_ordering_migration(payload):
    require(sha256(payload) == ORDERING_MIGRATION_SHA256,
            "Reviewed byte-ordering migration snapshot changed")
    migration = json.loads(payload, object_pairs_hook=readable["object_without_duplicates"])
    require(migration["schema_version"] == 1 and migration["added_slug"] == "insertion-sort-by-keys",
            "Unsupported byte-ordering migration")
    require(migration["reviewed_before"]["status"] == "reviewed_uncommitted_snapshot"
            and migration["reviewed_before"]["report_sha256"] == EQUALITY_REPORT_SHA256,
            "Byte-ordering migration reviewed report identity changed")
    return migration


def byte_ordering_preservation(report, old_slugs, migration=None):
    """Check the entire original17 report before entering the historical chain.

    Only the exact appended fixture and separately freshness-checked authoring
    metadata are projected out. No source, artifact, cost or outcome fields are
    normalized, and unknown, duplicate, missing or reordered fixtures fail.
    """
    if migration is None:
        migration = load_ordering_migration(ORDERING_MIGRATION_PATH.read_bytes())
    require(migration["preserved_slugs"] == old_slugs + ["prefix-list", "atom-not-equal"],
            "Byte-ordering migration original17 identities changed")
    require([p["slug"] for p in report["puzzles"]] == migration["preserved_slugs"] + ["insertion-sort-by-keys"],
            "Byte-ordering migration permits only insertion-sort-by-keys appended to the original17 fixtures")
    retained = copy.deepcopy(report)
    retained["puzzles"].pop()
    require(sha256(canonical(without_authoring(retained))) == migration["reviewed_before"]["normalized_report_sha256"],
            "An original17 source, contract, artifact, cost, failure, order or ledger record changed")
    return retained


def owned_preservation(report, baseline, migration):
    """Retain all old data; normalize only exact reviewed SDK dependency bytes.

    The caller validates the full report and current source/Cargo freshness
    before removing its separately verified authoring record or its new row.
    Unlike runtime_frames_behavior, the reviewed report digest also keeps both
    optimized helpers' actual artifacts, costs, diagnostics and failure codes.
    """
    old_slugs = migration["preserved_slugs"]
    require(old_slugs == baseline["corpus"]["slugs"], "Migration original fixture identity changed")
    report = byte_equality_preservation(report, old_slugs)
    require([p["slug"] for p in report["puzzles"]] == old_slugs + ["prefix-list"],
            "Owned migration permits only prefix-list appended to the original15 fixtures")
    retained = without_authoring(report)
    retained["puzzles"].pop()
    transitions = {row["path"]: row for row in migration["sdk_dependencies"]}
    for puzzle in retained["puzzles"]:
        for dependency in puzzle["source_dependencies"]:
            transition = transitions.get(dependency["path"])
            if transition is None:
                continue
            require(dependency == {"path": transition["path"], **transition["after"]},
                    f"{puzzle['slug']}: SDK dependency differs from its exact reviewed snapshot")
            dependency.update(transition["before"])
    require(sha256(canonical(retained)) == migration["reviewed_before"]["normalized_report_sha256"],
            "An original15 source, contract, artifact, cost, failure, order or ledger record changed")
    compatible = readable["runtime_frames_behavior"](retained)
    require(sha256(canonical(compatible)) == migration["historical"]["normalized_runtime_frame_sha256"],
            "Original evidence differs from the normalized pinned historical frame baseline")
    return retained


def load_bitwise_migration(payload):
    require(sha256(payload) == BITWISE_MIGRATION_SHA256,
            "Reviewed bitwise-patch migration snapshot changed")
    migration = json.loads(payload, object_pairs_hook=readable["object_without_duplicates"])
    require(migration["schema_version"] == 1
            and migration["reviewed_before"]["status"] == "reviewed_uncommitted_snapshot"
            and migration["reviewed_before"]["report_sha256"] == ORDERING_REPORT_SHA256,
            "Unsupported bitwise-patch reviewed report")
    require(migration["before_patches"] == [f"https://github.com/xch-dev/rue/issues/{n}" for n in (108, 109, 110, 111)]
            and migration["added_patches"] == [f"https://github.com/xch-dev/rue/issues/{n}" for n in (112, 113)],
            "Unsupported bitwise optimizer patch transition")
    return migration


def bitwise_patch_preservation(report, migration=None):
    """Normalize only two reviewed provenance additions, retaining all18 rows.

    This is called after current typed/source/Cargo checks by the full gate.
    Authoring metadata is separately freshness checked; no puzzle source,
    contract, program byte, cost, failure or coverage is normalized here.
    """
    if migration is None:
        migration = load_bitwise_migration(BITWISE_MIGRATION_PATH.read_bytes())
    require(report["rue_local_patches"] == migration["before_patches"] + migration["added_patches"],
            "Missing, reordered or unknown bitwise optimizer patch provenance")
    require([p["slug"] for p in report["puzzles"]] == migration["preserved_slugs"],
            "Bitwise patch must preserve the exact18 fixture identities and order")
    retained = copy.deepcopy(report)
    retained["rue_local_patches"] = copy.deepcopy(migration["before_patches"])
    require(sha256(canonical(without_authoring(retained))) == migration["reviewed_before"]["normalized_report_sha256"],
            "An original18 source, contract, artifact, cost, failure, order or ledger record changed")
    return retained


def check(report, baseline, migration=None):
    readable["check_report"](report)
    # Never let projection of new rows or authoring metadata exempt them from
    # typed failure validation or actual current-source/Cargo-input freshness.
    readable["check_sources"](report, ROOT, require_rue=True)
    for puzzle in report["puzzles"]:
        reference = readable["contained_file"](ROOT, f"puzzles/{puzzle['slug']}/reference.clsp")
        require(reference.read_bytes().decode() == puzzle["reference"],
                f"{puzzle['slug']}: report contains stale reference source; rerun validation")
    readable["check_recorded"](report.get("authoring"), ROOT, [p["slug"] for p in report["puzzles"]])
    # Full-current native bytes and dedicated typed instance evidence must be
    # checked before any new row/metadata is projected into historical layers.
    current_native_sources(report)
    password_evidence(report)
    if migration is None:
        migration = load_migration(MIGRATION_PATH.read_bytes())
    require(baseline["baseline"] == {"repository": "https://github.com/judeallred/chialisp-oxide", "commit": COMMIT,
            "report": {"path": REPORT_PATH, "sha256": REPORT_SHA256, "git_blob_sha1": REPORT_BLOB}}, "Changed historical provenance")
    require(baseline["schema_version"] == 1 and baseline["max_cost"] == MAX_COST
            and baseline["optimization_pipeline"] == readable["PREVIOUS_PIPELINE"], "Changed baseline contract")
    compatible = bitwise_patch_preservation(hashing_preservation(password_preservation(report)))
    previous = byte_ordering_preservation(compatible, baseline["corpus"]["slugs"])
    retained = owned_preservation(previous, baseline, migration)
    require(corpus_counts(retained) == baseline["corpus"], "Original corpus order, coverage or ledger counts changed")
    for key in ("toolchain", "execution_vm", "vm_failure_protocol", "rue_revision", "rue_local_patches"):
        require(compatible[key] == baseline[key], f"Changed execution/toolchain metadata: {key}")
    # The original compact digest includes the historical Cargo input record.
    # Its bytes/hash remain untouched; the separate migration digest is derived
    # from the same pinned report with only that verified metadata omitted.
    require(tuple(p["slug"] for p in baseline["puzzles"]) == SLUGS, "Missing or reordered compact baseline fixture")
    measurements = []
    for old in baseline["puzzles"]:
        puzzle = next(p for p in report["puzzles"] if p["slug"] == old["slug"])
        require(sha256(puzzle["source"].encode()) == old["source_sha256"], "Original Rust source changed")
        require(len(puzzle["cases"]) == len(old["cases"]), "Named vector coverage changed")
        artifacts = readable["modes"](puzzle["artifacts"], puzzle["slug"])
        retained = readable["modes"](old["artifacts"], puzzle["slug"])
        require(set(retained) == {("rust_mir", False), ("rust_mir", True)}, "Incomplete retained Rust builds")
        for artifact in [*old["artifacts"], artifacts[("rust_mir", False)], artifacts[("rust_mir", True)]]:
            artifact_valid(artifact)
        require(artifacts[("rust_mir", False)] == retained[("rust_mir", False)], "Unoptimized artifact changed")
        before, after = retained[("rust_mir", True)]["bytes"], artifacts[("rust_mir", True)]["bytes"]
        require(after < before, f"{puzzle['slug']}: no measured optimized size reduction")
        lower = equal = higher = 0
        for previous, current in zip(old["cases"], puzzle["cases"]):
            require((current["label"], current["solution"], current["native"]) == (previous["label"], previous["solution"], previous["expected"]), "Named input/expected result changed")
            old_runs = readable["modes"](previous["runs"], puzzle["slug"])
            new_runs = readable["modes"](current["runs"], puzzle["slug"])
            require(set(old_runs) == {("rust_mir", False), ("rust_mir", True)}, "Incomplete retained case builds")
            for run in old_runs.values():
                require(check_execution(run["execution"]) == previous["expected"], "Retained baseline execution conflicts with expected result")
            require(new_runs[("rust_mir", False)] == old_runs[("rust_mir", False)], "Unoptimized execution changed")
            if previous["expected"]["status"] == "accepted":
                a, b = old_runs[("rust_mir", True)]["execution"]["cost"], new_runs[("rust_mir", True)]["execution"]["cost"]
                lower += b < a
                equal += b == a
                higher += b > a
        require(lower > 0 and equal == 0 and higher == 0,
                f"{puzzle['slug']}: every accepted named case must have a measured cost reduction")
        measurements.append({"slug": puzzle["slug"], "bytes_before": before, "bytes_after": after,
                             "named_costs_lower": lower, "named_costs_equal": equal, "named_costs_higher": higher})
    return measurements


def replay(baseline, cli):
    count = 0
    with tempfile.TemporaryDirectory(prefix="chialisp-oxide-runtime-baseline-") as temporary:
        for puzzle in baseline["puzzles"]:
            for artifact in puzzle["artifacts"]:
                artifact_valid(artifact)
                path = Path(temporary) / f"{puzzle['slug']}-{artifact['optimize_lir']}.hex"
                path.write_text(artifact["clvm_hex"] + "\n")
                for case in puzzle["cases"]:
                    result = subprocess.run([str(cli), "run", str(path), "--solution", case["solution"], "--max-cost", str(MAX_COST)],
                                            capture_output=True, text=True, timeout=30)
                    require(result.returncode in (0, 2), f"Historical VM invocation failed: {result.stderr}")
                    execution = json.loads(result.stdout, object_pairs_hook=readable["object_without_duplicates"])
                    expected = readable["modes"](case["runs"], puzzle["slug"])[("rust_mir", artifact["optimize_lir"])]["execution"]
                    check_execution(execution)
                    require(result.returncode == (0 if execution["outcome"]["status"] == "accepted" else 2), "VM exit status contradicts its typed outcome")
                    require(execution == expected, f"Historical artifact replay differs: {puzzle['slug']}/{case['label']}/{artifact['optimize_lir']}")
                    count += 1
    return count


def verify_git(baseline, migration):
    def git_object(name):
        result = subprocess.run(["git", "show", f"{COMMIT}:{name}"], cwd=ROOT, capture_output=True, timeout=30)
        require(result.returncode == 0, f"Historical Git object unavailable; fetch {COMMIT} for --verify-git")
        return result.stdout
    historical_raw = git_object(REPORT_PATH)
    require(from_report(historical_raw) == baseline, "Compact baseline differs from the pinned historical report")
    historical = json.loads(historical_raw, object_pairs_hook=readable["object_without_duplicates"])
    compatible = without_authoring(readable["runtime_frames_behavior"](historical))
    require(sha256(canonical(compatible)) == migration["historical"]["normalized_runtime_frame_sha256"],
            "Compatible frame digest differs from the pinned historical Git report")
    for transition in migration["sdk_dependencies"]:
        source = git_object(transition["path"])
        require({"sha256": sha256(source), "source": source.decode()} == transition["before"],
                "Migration before-SDK snapshot differs from the pinned Git source")
    for puzzle in baseline["puzzles"]:
        require(sha256(git_object(puzzle["source_path"])) == puzzle["source_sha256"], "Historical Rust source Git identity mismatch")


def ordering_controls(report, baseline):
    old_slugs = baseline["corpus"]["slugs"]
    current = report
    report = bitwise_patch_preservation(current)
    byte_ordering_preservation(report, old_slugs)
    controls = 0
    # Exercise the exact snapshot boundary independently of source freshness;
    # coherent edits must fail even if the corresponding files were also edited.
    for mutation in ("source", "dependency", "artifact", "cost", "output", "failure", "coverage",
                     "case_order", "ledger", "puzzle_order", "missing_old", "duplicate_old",
                     "unknown_added", "missing_added", "duplicate_added", "added_order", "metadata"):
        bad = copy.deepcopy(report)
        puzzle = next(p for p in bad["puzzles"] if p["slug"] == "atom-not-equal")
        if mutation == "source": puzzle["source"] += "\n"
        elif mutation == "dependency":
            dependency = puzzle["source_dependencies"][0]
            dependency["source"] += "\n"
            dependency["sha256"] = sha256(dependency["source"].encode())
        elif mutation == "artifact": puzzle["artifacts"][0]["tree_hash"] = "0" * 64
        elif mutation in ("cost", "output", "failure"):
            status = "rejected" if mutation == "failure" else "accepted"
            case = next(c for c in puzzle["cases"] if c["native"]["status"] == status)
            execution = readable["modes"](case["runs"], "test")[("rust_mir", True)]["execution"]
            if mutation == "cost": execution["cost"] += 1
            elif mutation == "failure":
                execution["failure"]["code"] = "path_into_atom" if execution["failure"]["code"] == "raise" else "raise"
            else:
                output = "01" if case["native"]["output_hex"] == "80" else "80"
                case["native"]["output_hex"] = output
                for run in case["runs"]: run["execution"]["outcome"]["output_hex"] = output
        elif mutation == "coverage": puzzle["generated_cases"] -= 1
        elif mutation == "case_order": puzzle["cases"].reverse()
        elif mutation == "ledger": bad["puzzles"][0]["simulator"][0]["name"] += " changed"
        elif mutation == "puzzle_order": bad["puzzles"][0:2] = reversed(bad["puzzles"][0:2])
        elif mutation == "missing_old": bad["puzzles"].pop(0)
        elif mutation == "duplicate_old": bad["puzzles"].insert(1, copy.deepcopy(bad["puzzles"][0]))
        elif mutation == "unknown_added": bad["puzzles"][-1]["slug"] = "unknown-byte-helper"
        elif mutation == "missing_added": bad["puzzles"].pop()
        elif mutation == "duplicate_added": bad["puzzles"].append(copy.deepcopy(bad["puzzles"][-1]))
        elif mutation == "added_order": bad["puzzles"][-2:] = reversed(bad["puzzles"][-2:])
        elif mutation == "metadata": bad["toolchain"] += " changed"
        require(bad != report, f"No-op byte-ordering preservation control: {mutation}")
        try: byte_ordering_preservation(bad, old_slugs)
        except (ValueError, KeyError): controls += 1; continue
        raise ValueError(f"Byte-ordering preserved17 mutation passed: {mutation}")

    # The appended record and authoring metadata are removed only AFTER the
    # complete current report passes the same source and Cargo freshness gate.
    for mutation in ("source", "reference", "rue", "dependency", "export", "authoring_hash",
                     "authoring_duplicate", "authoring_missing", "authoring_unknown_export"):
        bad = copy.deepcopy(current)
        puzzle = bad["puzzles"][-1]
        source_path = "puzzles/insertion-sort-by-keys/puzzle.rs"
        if mutation == "source": puzzle["source"] += "\n"
        elif mutation == "reference": puzzle["reference"] += "\n"
        elif mutation == "rue": puzzle["rue"]["source"] += "\n"
        elif mutation == "dependency":
            next(d for d in puzzle["source_dependencies"] if d["path"].endswith("generated-cases.json"))["sha256"] = "0" * 64
        elif mutation == "export":
            bad["authoring"]["exports"]["insertion-sort-by-keys"]["entry"] = "chialisp_oxide_puzzles::unknown::puzzle"
        elif mutation == "authoring_hash":
            next(s for s in bad["authoring"]["sources"] if s["path"] == source_path)["sha256"] = "0" * 64
        elif mutation == "authoring_duplicate": bad["authoring"]["sources"].append(copy.deepcopy(bad["authoring"]["sources"][-1]))
        elif mutation == "authoring_missing": del bad["authoring"]
        elif mutation == "authoring_unknown_export":
            bad["authoring"]["exports"]["unknown-byte-helper"] = copy.deepcopy(bad["authoring"]["exports"]["insertion-sort-by-keys"])
        require(bad != current, f"No-op byte-ordering freshness control: {mutation}")
        try: check(bad, baseline)
        except (ValueError, KeyError): controls += 1; continue
        raise ValueError(f"Byte-ordering projected-record freshness mutation passed: {mutation}")

    migration = load_ordering_migration(ORDERING_MIGRATION_PATH.read_bytes())
    for mutation in ("digest", "status", "report", "slugs", "added_slug"):
        bad = copy.deepcopy(migration)
        if mutation == "digest": bad["reviewed_before"]["normalized_report_sha256"] = "0" * 64
        elif mutation == "status": bad["reviewed_before"]["status"] = "unverified"
        elif mutation == "report": bad["reviewed_before"]["report_sha256"] = "0" * 64
        elif mutation == "slugs": bad["preserved_slugs"].reverse()
        else: bad["added_slug"] = "arbitrary-new-puzzle"
        require(bad != migration, f"No-op byte-ordering snapshot control: {mutation}")
        try: load_ordering_migration(json.dumps(bad, indent=2).encode() + b"\n")
        except ValueError: controls += 1; continue
        raise ValueError(f"Altered byte-ordering migration passed: {mutation}")
    return controls


def bitwise_patch_controls(report):
    bitwise_patch_preservation(report)
    controls = 0
    for mutation in ("missing_first_added_patch", "missing_second_added_patch", "duplicate_patch", "reordered_patches",
                     "unknown_first_added_patch", "unknown_second_added_patch",
                     "source", "reference", "rue", "artifact", "cost", "failure", "output",
                     "coverage", "case_order", "puzzle_order", "missing_fixture", "extra_fixture",
                     "ledger", "unknown_metadata"):
        bad = copy.deepcopy(report)
        puzzle = bad["puzzles"][-1]
        if mutation == "missing_first_added_patch": bad["rue_local_patches"].pop(-2)
        elif mutation == "missing_second_added_patch": bad["rue_local_patches"].pop()
        elif mutation == "duplicate_patch": bad["rue_local_patches"].append(bad["rue_local_patches"][-1])
        elif mutation == "reordered_patches": bad["rue_local_patches"][-2:] = reversed(bad["rue_local_patches"][-2:])
        elif mutation == "unknown_first_added_patch": bad["rue_local_patches"][-2] = "https://github.com/xch-dev/rue/issues/999999"
        elif mutation == "unknown_second_added_patch": bad["rue_local_patches"][-1] = "https://github.com/xch-dev/rue/issues/999999"
        elif mutation == "source": puzzle["source"] += "\n"
        elif mutation == "reference": puzzle["reference"] += "\n"
        elif mutation == "rue": puzzle["rue"]["source"] += "\n"
        elif mutation == "artifact": puzzle["artifacts"][0]["tree_hash"] = "0" * 64
        elif mutation in ("cost", "failure", "output"):
            case = next(c for c in puzzle["cases"] if c["native"]["status"] == ("rejected" if mutation == "failure" else "accepted"))
            run = case["runs"][0]["execution"]
            if mutation == "cost": run["cost"] += 1
            elif mutation == "failure": run["failure"]["code"] = "path_into_atom" if run["failure"]["code"] == "raise" else "raise"
            else:
                changed = "80" if case["native"]["output_hex"] != "80" else "01"
                case["native"]["output_hex"] = changed
                for row in case["runs"]: row["execution"]["outcome"]["output_hex"] = changed
        elif mutation == "coverage": puzzle["generated_cases"] -= 1
        elif mutation == "case_order": puzzle["cases"].reverse()
        elif mutation == "puzzle_order": bad["puzzles"].reverse()
        elif mutation == "missing_fixture": bad["puzzles"].pop()
        elif mutation == "extra_fixture": bad["puzzles"].append(copy.deepcopy(puzzle))
        elif mutation == "ledger": bad["puzzles"][0]["simulator"][0]["name"] += " changed"
        else: bad["unreviewed_metadata"] = "not permitted"
        require(bad != report, f"No-op bitwise-patch control: {mutation}")
        try: bitwise_patch_preservation(bad)
        except (ValueError, KeyError): controls += 1; continue
        raise ValueError(f"Altered bitwise-patch report passed: {mutation}")
    migration = load_bitwise_migration(BITWISE_MIGRATION_PATH.read_bytes())
    for key in ("scope", "reviewed_before", "before_patches", "added_patches", "preserved_slugs"):
        bad = copy.deepcopy(migration)
        if key == "reviewed_before": bad[key]["normalized_report_sha256"] = "0" * 64
        elif key in ("before_patches", "added_patches", "preserved_slugs"): bad[key].reverse()
        else: bad[key] += " changed"
        require(bad != migration, f"No-op bitwise-patch snapshot control: {key}")
        try: load_bitwise_migration(json.dumps(bad, indent=2).encode() + b"\n")
        except ValueError: controls += 1; continue
        raise ValueError(f"Altered bitwise-patch migration passed: {key}")
    return controls


def self_test(report, baseline):
    check(report, baseline)
    controls = 0
    for mutation in ("old_pipeline", "unknown_pipeline", "pipeline_order", "remove_puzzle", "puzzle_order",
                     "remove_case", "case_order", "run_order", "source", "source_dependency", "reference", "rue",
                     "off_artifact", "off_cost", "reference_artifact", "reference_cost", "rue_artifact", "rue_cost",
                     "other_on_artifact", "other_on_cost", "coherent_output", "on_output", "abi", "coverage",
                     "ledger_group", "unknown_failure", "missing_failure", "resource_rejection", "inconclusive",
                     "no_size_gain", "no_cost_gain", "one_cost_higher", "one_cost_equal"):
        bad = copy.deepcopy(report)
        puzzle = next(p for p in bad["puzzles"] if p["slug"] == "last-list-element")
        accepted = next(c for c in puzzle["cases"] if c["native"]["status"] == "accepted")
        rejected = next(c for c in puzzle["cases"] if c["native"]["status"] == "rejected")
        modes = readable["modes"]
        if mutation == "old_pipeline": bad["optimization_pipeline"] = copy.deepcopy(readable["PREVIOUS_PIPELINE"])
        elif mutation == "unknown_pipeline": bad["optimization_pipeline"]["rust_mir"]["optimized"].append("unknown")
        elif mutation == "pipeline_order": bad["optimization_pipeline"]["rust_mir"]["optimized"].reverse()
        elif mutation == "remove_puzzle": bad["puzzles"].pop(0)
        elif mutation == "puzzle_order": bad["puzzles"].reverse()
        elif mutation == "remove_case": puzzle["cases"].pop()
        elif mutation == "case_order": puzzle["cases"].reverse()
        elif mutation == "run_order": accepted["runs"].reverse()
        elif mutation == "source": puzzle["source"] += "\n"
        elif mutation == "source_dependency": puzzle["source_dependencies"][0]["sha256"] = "0" * 64
        elif mutation == "reference": puzzle["reference"] += "\n"
        elif mutation == "rue": puzzle["rue"]["source"] += "\n"
        elif mutation in ("off_artifact", "reference_artifact", "rue_artifact", "other_on_artifact"):
            row = bad["puzzles"][0] if mutation == "other_on_artifact" else puzzle
            mode = {"off_artifact": ("rust_mir", False), "reference_artifact": ("chialisp", True),
                    "rue_artifact": ("rue_source", True), "other_on_artifact": ("rust_mir", True)}[mutation]
            modes(row["artifacts"], "test")[mode]["tree_hash"] = "0" * 64
        elif mutation in ("off_cost", "reference_cost", "rue_cost", "other_on_cost"):
            row = bad["puzzles"][0]["cases"][0] if mutation == "other_on_cost" else accepted
            mode = {"off_cost": ("rust_mir", False), "reference_cost": ("chialisp", True),
                    "rue_cost": ("rue_source", True), "other_on_cost": ("rust_mir", True)}[mutation]
            modes(row["runs"], "test")[mode]["execution"]["cost"] += 1
        elif mutation == "coherent_output":
            accepted["native"]["output_hex"] = "ff0180"
            for run in accepted["runs"]: run["execution"]["outcome"]["output_hex"] = "ff0180"
        elif mutation == "on_output": modes(accepted["runs"], "test")[("rust_mir", True)]["execution"]["outcome"]["output_hex"] = "ff0180"
        elif mutation == "abi": puzzle["environment"] = "ignored"
        elif mutation == "coverage": puzzle["generated_cases"] -= 1
        elif mutation == "ledger_group":
            coin = next(p for p in bad["puzzles"] if p["kind"] == "coin_puzzle")
            coin["simulator"].pop()
            for row in coin["simulator_comparisons"]: row["scenarios"].pop()
        elif mutation in ("unknown_failure", "missing_failure", "resource_rejection", "inconclusive"):
            execution = modes(rejected["runs"], "test")[("rust_mir", True)]["execution"]
            if mutation == "unknown_failure": execution["failure"]["code"] = "unknown"
            elif mutation == "missing_failure": del execution["failure"]
            else:
                execution["failure"] = {"kind": "resource", "code": "cost_exceeded"}
                if mutation == "inconclusive": execution["outcome"]["status"] = "inconclusive"
        elif mutation == "no_size_gain":
            old = next(p for p in baseline["puzzles"] if p["slug"] == puzzle["slug"])
            for index, artifact in enumerate(puzzle["artifacts"]):
                if artifact["backend"] == "rust_mir" and artifact["optimize_lir"] is True:
                    puzzle["artifacts"][index] = copy.deepcopy(modes(old["artifacts"], "test")[("rust_mir", True)])
        elif mutation in ("one_cost_higher", "one_cost_equal"):
            old = next(p for p in baseline["puzzles"] if p["slug"] == puzzle["slug"])
            previous = next(c for c in old["cases"] if c["label"] == accepted["label"])
            cost = modes(previous["runs"], "test")[("rust_mir", True)]["execution"]["cost"]
            modes(accepted["runs"], "test")[("rust_mir", True)]["execution"]["cost"] = cost + (mutation == "one_cost_higher")
        else:
            old = next(p for p in baseline["puzzles"] if p["slug"] == puzzle["slug"])
            for before, after in zip(old["cases"], puzzle["cases"]):
                if before["expected"]["status"] == "accepted":
                    modes(after["runs"], "test")[("rust_mir", True)]["execution"]["cost"] = modes(before["runs"], "test")[("rust_mir", True)]["execution"]["cost"]
        try:
            check(bad, baseline)
        except (ValueError, KeyError):
            controls += 1
            continue
        raise ValueError(f"Runtime-frame corrupted evidence passed: {mutation}")
    for mutation in ("provenance", "source_hash", "artifact", "old_cost", "expected", "coverage_digest"):
        bad = copy.deepcopy(baseline)
        if mutation == "provenance": bad["baseline"]["commit"] = "0" * 40
        elif mutation == "source_hash": bad["puzzles"][0]["source_sha256"] = "0" * 64
        elif mutation == "artifact": bad["puzzles"][0]["artifacts"][0]["clvm_hex"] = "80"
        elif mutation == "old_cost": bad["puzzles"][0]["cases"][0]["runs"][1]["execution"]["cost"] += 1
        elif mutation == "expected": bad["puzzles"][0]["cases"][0]["expected"]["output_hex"] = "01"
        else: bad["preserved_report_sha256"] = "0" * 64
        try: load_baseline(json.dumps(bad, indent=2).encode() + b"\n")
        except ValueError: controls += 1; continue
        raise ValueError(f"Altered retained baseline passed: {mutation}")
    migration = load_migration(MIGRATION_PATH.read_bytes())
    # Validate this projection once, then exercise the older boundaries using
    # their actual accepted17-fixture input, not an invalid18-fixture report.
    password_previous = password_preservation(report)
    hashing_previous = hashing_preservation(password_previous)
    previous = byte_ordering_preservation(bitwise_patch_preservation(hashing_previous), migration["preserved_slugs"])
    owned_preservation(previous, baseline, migration)
    # Exercise the preservation boundary directly as well as the full source
    # freshness gate. Coherently edited source+hash pairs must still be rejected
    # if someone changes both the working source and its recorded snapshot.
    for mutation in ("old_source", "unreviewed_dependency", "sdk_transition", "unexpected_added_slug",
                     "old_optimized_cost", "old_optimized_failure", "missing_prefix"):
        bad = copy.deepcopy(previous)
        puzzle = next(p for p in bad["puzzles"] if p["slug"] == "last-list-element")
        if mutation == "old_source":
            puzzle["source"] += "\n"
        elif mutation in ("unreviewed_dependency", "sdk_transition"):
            path = "crates/chialisp-oxide/src/lib.rs" if mutation == "sdk_transition" else "crates/chialisp-oxide/src/domain.rs"
            dependency = next(d for p in bad["puzzles"] for d in p["source_dependencies"] if d["path"] == path)
            dependency["source"] += "\n"
            dependency["sha256"] = sha256(dependency["source"].encode())
        elif mutation == "unexpected_added_slug":
            bad["puzzles"][-1]["slug"] = "unexpected-owned-helper"
        elif mutation == "missing_prefix":
            bad["puzzles"] = [p for p in bad["puzzles"] if p["slug"] != "prefix-list"]
        else:
            status = "accepted" if mutation == "old_optimized_cost" else "rejected"
            case = next(c for c in puzzle["cases"] if c["native"]["status"] == status)
            execution = readable["modes"](case["runs"], "test")[("rust_mir", True)]["execution"]
            if mutation == "old_optimized_cost": execution["cost"] += 1
            else: execution["failure"]["code"] = "path_into_atom" if execution["failure"]["code"] == "raise" else "raise"
        try: owned_preservation(bad, baseline, migration)
        except (ValueError, KeyError): controls += 1; continue
        raise ValueError(f"Owned migration preservation mutation passed: {mutation}")
    for mutation in ("historical_digest", "reviewed_digest", "sdk_source", "sdk_hash", "added_slug"):
        bad = copy.deepcopy(migration)
        if mutation == "historical_digest": bad["historical"]["normalized_runtime_frame_sha256"] = "0" * 64
        elif mutation == "reviewed_digest": bad["reviewed_before"]["normalized_report_sha256"] = "0" * 64
        elif mutation == "sdk_source": bad["sdk_dependencies"][0]["after"]["source"] += "\n"
        elif mutation == "sdk_hash": bad["sdk_dependencies"][0]["after"]["sha256"] = "0" * 64
        else: bad["added_slug"] = "arbitrary-new-puzzle"
        try: load_migration(json.dumps(bad, indent=2).encode() + b"\n")
        except ValueError: controls += 1; continue
        raise ValueError(f"Altered reviewed migration snapshot passed: {mutation}")
    equality_migration = load_equality_migration(EQUALITY_MIGRATION_PATH.read_bytes())
    byte_equality_preservation(previous, migration["preserved_slugs"], equality_migration)
    for mutation in ("prefix_source", "prefix_reference", "prefix_provenance", "prefix_artifact",
                     "prefix_cost", "prefix_failure", "prefix_output", "prefix_coverage", "prefix_order"):
        bad = copy.deepcopy(previous)
        prefix = next(p for p in bad["puzzles"] if p["slug"] == "prefix-list")
        if mutation == "prefix_source": prefix["source"] += "\n"
        elif mutation == "prefix_reference": prefix["reference"] += "\n"
        elif mutation == "prefix_provenance": prefix["provenance"]["limitations"].append("unreviewed")
        elif mutation == "prefix_artifact": prefix["artifacts"][0]["tree_hash"] = "0" * 64
        elif mutation == "prefix_coverage": prefix["generated_cases"] -= 1
        elif mutation == "prefix_order": prefix["cases"].reverse()
        else:
            status = "rejected" if mutation == "prefix_failure" else "accepted"
            case = next(c for c in prefix["cases"] if c["native"]["status"] == status)
            execution = readable["modes"](case["runs"], "test")[("rust_mir", True)]["execution"]
            if mutation == "prefix_cost": execution["cost"] += 1
            elif mutation == "prefix_failure": execution["failure"]["code"] = "path_into_atom" if execution["failure"]["code"] == "raise" else "raise"
            else:
                case["native"]["output_hex"] = "01"
                for run in case["runs"]: run["execution"]["outcome"]["output_hex"] = "01"
        try: byte_equality_preservation(bad, migration["preserved_slugs"], equality_migration)
        except (ValueError, KeyError): controls += 1; continue
        raise ValueError(f"Byte-equality preserved16 mutation passed: {mutation}")
    for mutation in ("digest", "status", "report", "slugs", "added_slug"):
        bad = copy.deepcopy(equality_migration)
        if mutation == "digest": bad["reviewed_before"]["normalized_report_sha256"] = "0" * 64
        elif mutation == "status": bad["reviewed_before"]["status"] = "unverified"
        elif mutation == "report": bad["reviewed_before"]["report_sha256"] = "0" * 64
        elif mutation == "slugs": bad["preserved_slugs"].reverse()
        else: bad["added_slug"] = "arbitrary-new-puzzle"
        try: load_equality_migration(json.dumps(bad, indent=2).encode() + b"\n")
        except ValueError: controls += 1; continue
        raise ValueError(f"Altered byte-equality migration passed: {mutation}")
    return (controls + ordering_controls(hashing_previous, baseline)
            + bitwise_patch_controls(hashing_previous) + hashing_controls(password_previous)
            + password_controls(report))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--current", type=Path, default=ROOT / REPORT_PATH)
    parser.add_argument("--verify-git", action="store_true", help="also extract and verify the pinned report/source Git objects; requires history")
    parser.add_argument("--replay", action="store_true", help="execute retained old serialized programs with the pinned current VM")
    parser.add_argument("--cli", type=Path, default=ROOT / "target/debug/chialisp-oxide")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        baseline = load_baseline(BASELINE_PATH.read_bytes())
        report = readable["load_report"](args.current)
        migration = load_migration(MIGRATION_PATH.read_bytes())
        measurements = check(report, baseline, migration)
        if args.verify_git: verify_git(baseline, migration)
        if args.replay: print(f"PASS: {replay(baseline, args.cli.resolve())} exact historical serialized-program VM replays")
        if args.self_test: print(f"PASS: {self_test(report, baseline)} runtime-frame corruption controls")
        for row in measurements:
            print(f"PASS {row['slug']}: optimized bytes {row['bytes_before']} -> {row['bytes_after']}; named accepted costs lower/equal/higher {row['named_costs_lower']}/{row['named_costs_equal']}/{row['named_costs_higher']}")
        print("Scope: finite measurements, exact retained observations and two reviewed SDK snapshots; no universal cost-dominance or compiler-preservation proof")
    except (ValueError, KeyError, TypeError, OSError, subprocess.SubprocessError) as error:
        parser.exit(1, f"Runtime-frame evidence check failed: {error}\n")


if __name__ == "__main__":
    main()
