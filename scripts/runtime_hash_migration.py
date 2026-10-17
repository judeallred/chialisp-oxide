"""Closed preservation boundary for the two hashing corpus additions.

Call only after checking the complete current report's typed observations,
sources and Cargo authoring metadata. This projects retained observations; it
does not execute programs or prove compiler preservation.
"""
import copy
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PATH = ROOT / "corpus/hashing-migration.json"
PIN = "dcfa5312b8043819e0aab2170d011e71baf28ce0e32436ef6ddd832d4f9b6ec7"
BEFORE_REPORT = "244ccf48fe0ad5e286d43f18f9b88da60567869562e3ea2e7efc9f969ebd1377"
BEFORE_NORMALIZED = "fb93d0b76e492281b14d2bbe07fc0dfc5bb361d45c36abe76e861b9f193f75c4"
ADDED = ["sha256tree", "singleton-launcher"]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def digest(value):
    return hashlib.sha256(value).hexdigest()


def load(payload=None):
    if payload is None:
        payload = PATH.read_bytes()
    require(digest(payload) == PIN, "Reviewed hashing migration snapshot changed")
    migration = json.loads(payload)
    require(migration["schema_version"] == 1 and migration["added_slugs"] == ADDED,
            "Unsupported hashing fixture transition")
    require(migration["reviewed_before"] == {
        "status": "reviewed_uncommitted_snapshot",
        "report_sha256": BEFORE_REPORT,
        "normalized_report_sha256": BEFORE_NORMALIZED,
    }, "Hashing transition reviewed report identity changed")
    transition = migration["sdk_transition"]
    require(transition["path"] == "crates/chialisp-oxide/src/lib.rs",
            "Unexpected hashing SDK transition path")
    for phase in ("before", "after"):
        row = transition[phase]
        require(row["path"] == transition["path"]
                and digest(row["source"].encode()) == row["sha256"],
                "Hashing SDK snapshot hash or path mismatch")
    return migration


def preserve(report):
    migration = load()
    require([p["slug"] for p in report["puzzles"]]
            == migration["preserved_slugs"] + ADDED,
            "Hashing transition permits only the two appended fixtures in reviewed order")
    result = copy.deepcopy(report)
    result["puzzles"] = result["puzzles"][:-len(ADDED)]
    transition = migration["sdk_transition"]
    affected = []
    for puzzle in result["puzzles"]:
        for index, dependency in enumerate(puzzle["source_dependencies"]):
            if dependency["path"] == transition["path"]:
                require(dependency == transition["after"],
                        f"{puzzle['slug']}: unreviewed facade source transition")
                puzzle["source_dependencies"][index] = copy.deepcopy(transition["before"])
                affected.append(puzzle["slug"])
    require(affected == transition["affected_slugs"],
            "Missing, duplicate or unexpected transitioned facade record")
    normalized = {key: value for key, value in result.items() if key != "authoring"}
    require(digest(canonical(normalized)) == BEFORE_NORMALIZED,
            "Hashing changed a retained18 source, contract, artifact, cost, failure, order, coverage or ledger observation")
    return result


def controls(report):
    """Exercise this boundary directly, without relying on freshness failures."""
    preserve(report)
    count = 0
    for mutation in (
        "missing_old", "missing_new", "unknown_new", "duplicate_new", "new_order", "old_order",
        "source", "reference", "rue", "dependency", "facade_source", "facade_hash",
        "missing_facade", "duplicate_facade", "artifact", "cost", "failure", "output",
        "coverage", "case_order", "ledger", "patches", "metadata",
    ):
        bad = copy.deepcopy(report)
        puzzle = next(p for p in bad["puzzles"] if p["slug"] == "last-list-element")
        if mutation == "missing_old": bad["puzzles"].pop(0)
        elif mutation == "missing_new": bad["puzzles"].pop()
        elif mutation == "unknown_new": bad["puzzles"][-1]["slug"] = "unknown-hash-puzzle"
        elif mutation == "duplicate_new": bad["puzzles"].append(copy.deepcopy(bad["puzzles"][-1]))
        elif mutation == "new_order": bad["puzzles"][-2:] = reversed(bad["puzzles"][-2:])
        elif mutation == "old_order": bad["puzzles"][0:2] = reversed(bad["puzzles"][0:2])
        elif mutation == "source": puzzle["source"] += "\n"
        elif mutation == "reference": puzzle["reference"] += "\n"
        elif mutation == "rue": puzzle["rue"]["source"] += "\n"
        elif mutation == "dependency": puzzle["source_dependencies"][-1]["sha256"] = "0" * 64
        elif mutation in ("facade_source", "facade_hash", "missing_facade", "duplicate_facade"):
            dependencies = puzzle["source_dependencies"]
            index = next(i for i, row in enumerate(dependencies)
                         if row["path"] == "crates/chialisp-oxide/src/lib.rs")
            if mutation == "facade_source":
                dependencies[index]["source"] += "\n"
                dependencies[index]["sha256"] = digest(dependencies[index]["source"].encode())
            elif mutation == "facade_hash": dependencies[index]["sha256"] = "0" * 64
            elif mutation == "missing_facade": dependencies.pop(index)
            else: dependencies.append(copy.deepcopy(dependencies[index]))
        elif mutation == "artifact": puzzle["artifacts"][0]["tree_hash"] = "0" * 64
        elif mutation in ("cost", "failure", "output"):
            case = next(c for c in puzzle["cases"]
                        if c["native"]["status"] == ("rejected" if mutation == "failure" else "accepted"))
            execution = case["runs"][0]["execution"]
            if mutation == "cost": execution["cost"] += 1
            elif mutation == "failure":
                execution["failure"]["code"] = "path_into_atom" if execution["failure"]["code"] == "raise" else "raise"
            else:
                changed = "80" if case["native"]["output_hex"] != "80" else "01"
                case["native"]["output_hex"] = changed
                for run in case["runs"]: run["execution"]["outcome"]["output_hex"] = changed
        elif mutation == "coverage": puzzle["generated_cases"] -= 1
        elif mutation == "case_order": puzzle["cases"].reverse()
        elif mutation == "ledger": bad["puzzles"][0]["simulator"][0]["name"] += " changed"
        elif mutation == "patches": bad["rue_local_patches"].reverse()
        else: bad["unreviewed_metadata"] = "not permitted"
        require(bad != report, f"No-op hashing preservation control: {mutation}")
        try:
            preserve(bad)
        except (ValueError, KeyError):
            count += 1
            continue
        raise ValueError(f"Hashing preservation corruption passed: {mutation}")
    migration = load()
    for key in ("scope", "reviewed_before", "preserved_slugs", "added_slugs", "sdk_transition"):
        bad = copy.deepcopy(migration)
        if key == "scope": bad[key] += " changed"
        elif key == "reviewed_before": bad[key]["normalized_report_sha256"] = "0" * 64
        elif key == "sdk_transition": bad[key]["after"]["source"] += "\n"
        else: bad[key].reverse()
        require(bad != migration, f"No-op hashing snapshot control: {key}")
        try:
            load(json.dumps(bad, indent=2).encode() + b"\n")
        except ValueError:
            count += 1
            continue
        raise ValueError(f"Altered hashing snapshot passed: {key}")
    return count
